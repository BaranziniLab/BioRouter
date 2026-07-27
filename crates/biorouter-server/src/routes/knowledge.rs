use axum::{
    body::Body,
    extract::{FromRequest, Multipart, Path, Query, Request, State},
    http::{header, HeaderMap, StatusCode},
    response::Response,
    routing::{get, post, put},
    Json, Router,
};
use biorouter::knowledge::ProviderCompleter;
use biorouter::model::ModelConfig;
use biorouter_mcp::knowledge::{
    convert,
    macros::{ingest as ingest_macro, lint as lint_macro, query as query_macro},
    paths,
    service::{KnowledgeService, ReadPageError},
    source_paths, store,
    subagent::{events::SubAgentEvent, loop_::SubAgentBounds},
    types::{Credibility, Graph, HistoryEntry, Manifest, ModelRef},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use utoipa::ToSchema;

/// Build the knowledge router.  The router owns an `Arc<KnowledgeService>` directly so
/// it can be tested without constructing a full `AppState`.
pub fn router(svc: Arc<KnowledgeService>) -> Router {
    Router::new()
        .route("/bases", get(list_bases).post(create_base))
        .route("/bases/import", post(import_brkb))
        .route(
            "/bases/{id}",
            get(get_base).put(update_base).delete(delete_base),
        )
        .route("/bases/{id}/default-model", put(set_default_model))
        .route("/bases/{id}/graph", get(get_graph))
        .route("/bases/{id}/location", get(get_location))
        .route("/bases/{id}/page", get(get_page_body))
        .route("/bases/{id}/pages", get(list_pages))
        .route(
            "/bases/{id}/pages/{*page_path}",
            get(read_page).put(write_page),
        )
        .route("/bases/{id}/history", get(list_history))
        .route("/bases/{id}/preview", post(preview_state))
        .route("/bases/{id}/restore", post(restore_state))
        .route("/expand-path", post(expand_path))
        .route("/bases/{id}/raw", post(add_raw_source))
        .route("/bases/{id}/ingest", post(ingest))
        .route("/bases/{id}/ingest-conversation", post(ingest_conversation))
        .route("/bases/{id}/query", post(query_kb))
        .route("/bases/{id}/lint", post(lint))
        .route("/bases/{id}/export", get(export_brkb))
        .route("/bases/{id}/sources/{sid}/reclassify", post(reclassify))
        .route(
            "/bases/{id}/sources/{sid}/credibility",
            put(override_credibility),
        )
        .route("/active", get(get_active).post(set_active))
        .route("/check-model", post(check_model))
        .with_state(svc)
}

const MAX_INGEST_FILE_BYTES: usize = 25 * 1024 * 1024;
const MAX_INGEST_CSV_BYTES: usize = 8 * 1024 * 1024;

fn ingest_upload_limit(filename: &str) -> usize {
    let lower = filename.to_lowercase();
    // Tabular formats share the tighter cap: their markdown-table expansion is
    // much larger than the file itself.
    if [".csv", ".xlsx", ".xlsm", ".xls", ".ods"]
        .iter()
        .any(|ext| lower.ends_with(ext))
    {
        MAX_INGEST_CSV_BYTES
    } else {
        MAX_INGEST_FILE_BYTES
    }
}

fn validate_ingest_upload(filename: &str, size: usize) -> Result<(), (StatusCode, String)> {
    let lower = filename.to_lowercase();

    if lower.ends_with(".brkb") {
        return Err((
            StatusCode::BAD_REQUEST,
            "'.brkb' archives must be imported via the knowledge-base import action, not the digest dropzone"
                .to_string(),
        ));
    }

    if lower.ends_with(".ppt") {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "{filename} is a legacy PowerPoint binary. Re-save it as .pptx and ingest that instead."
            ),
        ));
    }

    if [
        ".exe", ".app", ".pkg", ".dmg", ".msi", ".dll", ".dylib", ".so", ".bin", ".zip",
    ]
    .iter()
    .any(|ext| lower.ends_with(ext))
    {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "{filename} looks like an executable, binary, or packaged archive. Digest readable source material instead."
            ),
        ));
    }

    let limit = ingest_upload_limit(filename);
    if size > limit {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("{filename} is too large for ingestion ({size} bytes > {limit} byte limit)"),
        ));
    }

    Ok(())
}

#[derive(Debug)]
struct MultipartUpload {
    bytes: Vec<u8>,
    filename: String,
    mime: Option<String>,
}

fn sanitize_part_mime(mime: Option<&str>) -> Option<String> {
    mime.map(str::trim)
        .filter(|mime| !mime.is_empty())
        .map(ToOwned::to_owned)
}

async fn finish_macro_stream(
    sse_tx: mpsc::Sender<String>,
    mut result_rx: mpsc::Receiver<Result<serde_json::Value, String>>,
    macro_handle: JoinHandle<()>,
) {
    match result_rx.recv().await {
        Some(Ok(result_json)) => {
            let data = serde_json::to_string(&result_json).unwrap_or_default();
            let _ = sse_tx.send(format!("event: done\ndata: {data}\n\n")).await;
        }
        Some(Err(msg)) => {
            let _ = sse_tx.send(sse_error_frame(&msg)).await;
        }
        None => {
            let msg = match macro_handle.await {
                Ok(()) => "macro task ended without result".to_string(),
                Err(join_err) if join_err.is_cancelled() => {
                    "macro task was cancelled before producing a result".to_string()
                }
                Err(join_err) => format!("macro task failed: {join_err}"),
            };
            let _ = sse_tx.send(sse_error_frame(&msg)).await;
        }
    }
}

fn ingest_bounds() -> SubAgentBounds {
    SubAgentBounds {
        max_steps: 60,
        max_wall: Duration::from_secs(900),
        max_tokens: 200_000,
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Request / response DTOs
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize, ToSchema)]
pub struct CreateBaseBody {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateBaseBody {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct SetDefaultModelBody {
    #[serde(default)]
    pub model: Option<ModelRef>,
}

#[derive(Deserialize, ToSchema)]
pub struct ListPagesQuery {
    #[serde(default)]
    pub path_prefix: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct WritePageBody {
    pub content: String,
    pub commit_message: String,
}

#[derive(Serialize, ToSchema)]
pub struct CommitResponse {
    pub commit_sha: String,
}

#[derive(Deserialize, ToSchema)]
pub struct HistoryQuery {
    #[serde(default = "default_limit")]
    pub limit: usize,
}
fn default_limit() -> usize {
    50
}

#[derive(Deserialize, ToSchema)]
pub struct PreviewBody {
    pub commit_sha: String,
    pub path: String,
}

#[derive(Serialize, ToSchema)]
pub struct PreviewResponse {
    pub content: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct RestoreBody {
    pub commit_sha: String,
}

#[derive(Serialize, ToSchema)]
pub struct RestoreResponse {
    pub new_commit_sha: String,
}

// Task 8 DTOs
#[derive(Serialize, ToSchema)]
pub struct RawSourceResponse {
    pub source_id: String,
    pub source_md_path: String,
}

// Task 11 DTOs
#[derive(Serialize, ToSchema)]
pub struct CredibilityResponse {
    pub credibility: Credibility,
}

// check-model DTOs
#[derive(Deserialize, ToSchema)]
pub struct CheckModelBody {
    pub model: ModelRef,
}

#[derive(Serialize, ToSchema)]
pub struct CheckModelResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct ExpandPathBody {
    pub path: String,
}

#[derive(Serialize, ToSchema)]
pub struct ExpandPathFile {
    pub path: String,
    pub name: String,
    pub relative_path: String,
}

#[derive(Serialize, ToSchema)]
pub struct ExpandPathWarning {
    pub level: String,
    pub title: String,
    pub message: String,
}

#[derive(Serialize, ToSchema)]
pub struct ExpandPathResponse {
    pub files: Vec<ExpandPathFile>,
    pub warnings: Vec<ExpandPathWarning>,
}

// Task 9 DTOs
#[derive(Deserialize, ToSchema)]
pub struct IngestBody {
    pub source: serde_json::Value,
    pub model: ModelRef,
    #[serde(default)]
    pub focus: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct QueryBody {
    pub question: String,
    pub model: ModelRef,
    #[serde(default)]
    pub file_as_page: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
pub struct IngestConversationBody {
    /// Session ids to digest. At least one is required.
    pub session_ids: Vec<String>,
    pub model: ModelRef,
    #[serde(default)]
    pub focus: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct LintBody {
    pub model: ModelRef,
    #[serde(default)]
    pub autofix: Option<bool>,
}

// ──────────────────────────────────────────────────────────────────────────────
// Task 5: read-only routes (list / create / get / delete / graph)
// ──────────────────────────────────────────────────────────────────────────────

#[utoipa::path(
    get, path = "/knowledge/bases",
    responses((status = 200, description = "List of knowledge bases", body = Vec<Manifest>))
)]
pub async fn list_bases(
    State(svc): State<Arc<KnowledgeService>>,
) -> Result<Json<Vec<Manifest>>, (StatusCode, String)> {
    let bases = svc
        .list_bases()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(bases))
}

#[utoipa::path(
    post, path = "/knowledge/bases",
    request_body = CreateBaseBody,
    responses((status = 200, description = "Created knowledge base", body = Manifest))
)]
pub async fn create_base(
    State(svc): State<Arc<KnowledgeService>>,
    Json(body): Json<CreateBaseBody>,
) -> Result<Json<Manifest>, (StatusCode, String)> {
    let m = svc
        .create_base(&body.id, &body.name, body.color.as_deref())
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(m))
}

#[utoipa::path(
    get, path = "/knowledge/bases/{id}",
    params(("id" = String, Path, description = "Knowledge base ID")),
    responses(
        (status = 200, description = "Knowledge base manifest", body = Manifest),
        (status = 404, description = "Not found"),
    )
)]
pub async fn get_base(
    State(svc): State<Arc<KnowledgeService>>,
    Path(id): Path<String>,
) -> Result<Json<Manifest>, (StatusCode, String)> {
    svc.get_base(&id)
        .map(Json)
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))
}

#[utoipa::path(
    put, path = "/knowledge/bases/{id}",
    request_body = UpdateBaseBody,
    params(("id" = String, Path, description = "Knowledge base ID")),
    responses(
        (status = 200, description = "Updated knowledge base manifest", body = Manifest),
        (status = 400, description = "Bad request"),
        (status = 404, description = "Not found"),
    )
)]
pub async fn update_base(
    State(svc): State<Arc<KnowledgeService>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateBaseBody>,
) -> Result<Json<Manifest>, (StatusCode, String)> {
    let manifest = svc
        .update_base(&id, body.name.as_deref(), body.color.as_deref())
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("not found") {
                (StatusCode::NOT_FOUND, msg)
            } else {
                (StatusCode::BAD_REQUEST, msg)
            }
        })?;
    Ok(Json(manifest))
}

#[utoipa::path(
    put, path = "/knowledge/bases/{id}/default-model",
    request_body = SetDefaultModelBody,
    params(("id" = String, Path, description = "Knowledge base ID")),
    responses(
        (status = 200, description = "Updated knowledge base default model", body = Manifest),
        (status = 400, description = "Bad request"),
        (status = 404, description = "Not found"),
    )
)]
pub async fn set_default_model(
    State(svc): State<Arc<KnowledgeService>>,
    Path(id): Path<String>,
    Json(body): Json<SetDefaultModelBody>,
) -> Result<Json<Manifest>, (StatusCode, String)> {
    let manifest = svc.set_default_model(&id, body.model).map_err(|e| {
        let msg = e.to_string();
        if msg.contains("not found") {
            (StatusCode::NOT_FOUND, msg)
        } else {
            (StatusCode::BAD_REQUEST, msg)
        }
    })?;
    Ok(Json(manifest))
}

#[utoipa::path(
    delete, path = "/knowledge/bases/{id}",
    params(("id" = String, Path, description = "Knowledge base ID")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "Not found"),
    )
)]
pub async fn delete_base(
    State(svc): State<Arc<KnowledgeService>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    svc.delete_base(&id).map_err(|e| {
        let msg = e.to_string();
        if msg.contains("not found") {
            (StatusCode::NOT_FOUND, msg)
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    })?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get, path = "/knowledge/bases/{id}/graph",
    params(("id" = String, Path, description = "Knowledge base ID")),
    responses(
        (status = 200, description = "Knowledge graph", body = Graph),
        (status = 404, description = "Not found"),
    )
)]
pub async fn get_graph(
    State(svc): State<Arc<KnowledgeService>>,
    Path(id): Path<String>,
) -> Result<Json<Graph>, (StatusCode, String)> {
    let g = svc
        .get_graph(&id)
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
    Ok(Json(g))
}

#[derive(Serialize, ToSchema)]
pub struct LocationResponse {
    /// Absolute on-disk path to the knowledge base directory (the folder that
    /// holds `knowledge/`, `raw/`, `index.md`, …). Clients use this to open the
    /// folder in the OS file explorer so users can inspect raw markdown.
    pub path: String,
}

#[utoipa::path(
    get, path = "/knowledge/bases/{id}/location",
    params(("id" = String, Path, description = "Knowledge base ID")),
    responses(
        (status = 200, description = "Knowledge base on-disk location", body = LocationResponse),
        (status = 404, description = "Not found"),
    )
)]
pub async fn get_location(
    State(svc): State<Arc<KnowledgeService>>,
    Path(id): Path<String>,
) -> Result<Json<LocationResponse>, (StatusCode, String)> {
    let kb_root = paths::kb_root(svc.root(), &id);
    if !kb_root.exists() {
        return Err((StatusCode::NOT_FOUND, format!("kb '{id}' not found")));
    }
    Ok(Json(LocationResponse {
        path: kb_root.to_string_lossy().into_owned(),
    }))
}

// ──────────────────────────────────────────────────────────────────────────────
// Task 6: page CRUD routes
// ──────────────────────────────────────────────────────────────────────────────

#[utoipa::path(
    get, path = "/knowledge/bases/{id}/pages",
    params(
        ("id" = String, Path, description = "Knowledge base ID"),
        ("path_prefix" = Option<String>, Query, description = "Optional path prefix filter"),
    ),
    responses((status = 200, description = "Page list"))
)]
pub async fn list_pages(
    State(svc): State<Arc<KnowledgeService>>,
    Path(id): Path<String>,
    Query(q): Query<ListPagesQuery>,
) -> Result<Json<Vec<store::PageRef>>, (StatusCode, String)> {
    let kb_root = paths::kb_root(svc.root(), &id);
    let pages = store::list_pages(&kb_root, q.path_prefix.as_deref())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(pages))
}

#[utoipa::path(
    get, path = "/knowledge/bases/{id}/pages/{page_path}",
    params(
        ("id" = String, Path, description = "Knowledge base ID"),
        ("page_path" = String, Path, description = "Page path within KB"),
    ),
    responses(
        (status = 200, description = "Page content"),
        (status = 404, description = "Page not found"),
    )
)]
pub async fn read_page(
    State(svc): State<Arc<KnowledgeService>>,
    Path((id, page_path)): Path<(String, String)>,
) -> Result<Json<store::PageContent>, (StatusCode, String)> {
    let kb_root = paths::kb_root(svc.root(), &id);
    let page = store::read_page(&kb_root, &page_path)
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
    Ok(Json(page))
}

#[utoipa::path(
    put, path = "/knowledge/bases/{id}/pages/{page_path}",
    request_body = WritePageBody,
    params(
        ("id" = String, Path, description = "Knowledge base ID"),
        ("page_path" = String, Path, description = "Page path within KB"),
    ),
    responses(
        (status = 200, description = "Written", body = CommitResponse),
        (status = 400, description = "Bad request"),
    )
)]
pub async fn write_page(
    State(svc): State<Arc<KnowledgeService>>,
    Path((id, page_path)): Path<(String, String)>,
    Json(body): Json<WritePageBody>,
) -> Result<Json<CommitResponse>, (StatusCode, String)> {
    let _lock = svc
        .lock_kb(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let kb_root = paths::kb_root(svc.root(), &id);
    let sha_opt = store::write_page(
        &kb_root,
        &page_path,
        &body.content,
        &body.commit_message,
        None,
    )
    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let commit_sha = sha_opt.unwrap_or_default();
    Ok(Json(CommitResponse { commit_sha }))
}

// ──────────────────────────────────────────────────────────────────────────────
// Plan 5 Task 1: GET /bases/:id/page?path=... — raw markdown body for the
// frontend NodePreview card. Distinct from `GET /bases/:id/pages/{*path}`
// (which returns parsed PageContent with frontmatter split out); this route
// returns the file verbatim so the renderer can show the original markdown.
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize, ToSchema)]
pub struct ReadPageQuery {
    pub path: String,
}

#[derive(Serialize, ToSchema)]
pub struct ReadPageResponse {
    pub content: String,
}

// ──────────────────────────────────────────────────────────────────────────────
// Plan 6 Task 2: GET + POST /knowledge/active — cross-session active-KB sync.
// Thin pass-throughs to `KnowledgeService::{get,set}_active_persisted` so the
// frontend can render the chat chip on startup and persist user picks.
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize, ToSchema)]
pub struct SetActiveBody {
    /// `None` clears the active KB.
    #[serde(default)]
    pub kb_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub hidden_kbs: Option<Vec<String>>,
}

#[derive(Serialize, ToSchema)]
pub struct ActiveKbResponse {
    pub active_kb: Option<String>,
    pub hidden_kbs: Vec<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct GetActiveQuery {
    #[serde(default)]
    pub session_id: Option<String>,
}

#[utoipa::path(
    get, path = "/knowledge/active",
    params(
        ("session_id" = Option<String>, Query, description = "Optional chat session id for session-scoped active KB selection"),
    ),
    responses((status = 200, description = "Current active KB id", body = ActiveKbResponse))
)]
pub async fn get_active(
    State(svc): State<Arc<KnowledgeService>>,
    Query(q): Query<GetActiveQuery>,
) -> Result<Json<ActiveKbResponse>, (StatusCode, String)> {
    let (active_kb, hidden_kbs) = if let Some(session_id) = q.session_id.as_deref() {
        (
            svc.get_primary_for_session(session_id)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
                .or_else(|| svc.get_primary_persisted().ok().flatten()),
            svc.get_hidden_for_session_or_persisted(session_id)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
        )
    } else {
        (
            svc.get_primary_persisted()
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
            svc.get_hidden_persisted()
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
        )
    };
    Ok(Json(ActiveKbResponse {
        active_kb,
        hidden_kbs,
    }))
}

#[utoipa::path(
    post, path = "/knowledge/active",
    request_body = SetActiveBody,
    responses(
        (status = 200, description = "Set successfully", body = ActiveKbResponse),
        (status = 400, description = "Invalid kb id"),
    )
)]
pub async fn set_active(
    State(svc): State<Arc<KnowledgeService>>,
    Json(body): Json<SetActiveBody>,
) -> Result<Json<ActiveKbResponse>, (StatusCode, String)> {
    let session_id = body.session_id.clone();
    if let Some(session_id) = body.session_id.as_deref() {
        svc.set_primary_for_session(session_id, body.kb_id.as_deref())
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
        if let Some(hidden_kbs) = body.hidden_kbs.as_ref() {
            svc.set_hidden_for_session(session_id, hidden_kbs)
                .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
        }
    } else {
        svc.set_primary_persisted(body.kb_id.as_deref())
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
        if let Some(hidden_kbs) = body.hidden_kbs.as_ref() {
            svc.set_hidden_persisted(hidden_kbs)
                .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
        }
    }
    let active_kb = if let Some(session_id) = session_id.as_deref() {
        svc.get_primary_for_session(session_id)
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
            .or_else(|| svc.get_primary_persisted().ok().flatten())
    } else {
        svc.get_primary_persisted()
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
    };
    Ok(Json(ActiveKbResponse {
        active_kb,
        hidden_kbs: if let Some(session_id) = session_id.as_deref() {
            svc.get_hidden_for_session_or_persisted(session_id)
                .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
        } else {
            svc.get_hidden_persisted()
                .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
        },
    }))
}

#[utoipa::path(
    get, path = "/knowledge/bases/{id}/page",
    params(
        ("id" = String, Path, description = "Knowledge base ID"),
        ("path" = String, Query, description = "Page path under the KB root \
         (knowledge/*.md, raw/*/source.md, or index.md/schema.md/log.md)"),
    ),
    responses(
        (status = 200, description = "Page content", body = ReadPageResponse),
        (status = 400, description = "Invalid kb id or path"),
        (status = 404, description = "Page not found"),
    )
)]
pub async fn get_page_body(
    State(svc): State<Arc<KnowledgeService>>,
    Path(id): Path<String>,
    Query(q): Query<ReadPageQuery>,
) -> Result<Json<ReadPageResponse>, (StatusCode, String)> {
    match svc.read_page(&id, &q.path) {
        Ok(content) => Ok(Json(ReadPageResponse { content })),
        Err(ReadPageError::InvalidKbId(m)) => Err((StatusCode::BAD_REQUEST, m)),
        Err(ReadPageError::InvalidPath(m)) => Err((StatusCode::BAD_REQUEST, m)),
        Err(ReadPageError::NotFound(m)) => Err((StatusCode::NOT_FOUND, m)),
        Err(e @ ReadPageError::Io(_)) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Task 7: history + preview + restore routes
// ──────────────────────────────────────────────────────────────────────────────

#[utoipa::path(
    get, path = "/knowledge/bases/{id}/history",
    params(
        ("id" = String, Path, description = "Knowledge base ID"),
        ("limit" = Option<usize>, Query, description = "Maximum entries (default 50)"),
    ),
    responses((status = 200, description = "Commit history"))
)]
pub async fn list_history(
    State(svc): State<Arc<KnowledgeService>>,
    Path(id): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<Vec<HistoryEntry>>, (StatusCode, String)> {
    svc.list_history(&id, q.limit)
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[utoipa::path(
    post, path = "/knowledge/bases/{id}/preview",
    request_body = PreviewBody,
    params(("id" = String, Path, description = "Knowledge base ID")),
    responses(
        (status = 200, description = "File content at commit", body = PreviewResponse),
        (status = 500, description = "Error"),
    )
)]
pub async fn preview_state(
    State(svc): State<Arc<KnowledgeService>>,
    Path(id): Path<String>,
    Json(body): Json<PreviewBody>,
) -> Result<Json<PreviewResponse>, (StatusCode, String)> {
    let content = svc
        .preview_state(&id, &body.commit_sha, &body.path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(PreviewResponse { content }))
}

#[utoipa::path(
    post, path = "/knowledge/bases/{id}/restore",
    request_body = RestoreBody,
    params(("id" = String, Path, description = "Knowledge base ID")),
    responses(
        (status = 200, description = "Restored; returns new commit SHA", body = RestoreResponse),
        (status = 500, description = "Error"),
    )
)]
pub async fn restore_state(
    State(svc): State<Arc<KnowledgeService>>,
    Path(id): Path<String>,
    Json(body): Json<RestoreBody>,
) -> Result<Json<RestoreResponse>, (StatusCode, String)> {
    let new_commit_sha = svc
        .restore_state(&id, &body.commit_sha)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(RestoreResponse { new_commit_sha }))
}

// ──────────────────────────────────────────────────────────────────────────────
// Task 9: SSE-streamed macro routes (ingest / query / lint)
// ──────────────────────────────────────────────────────────────────────────────

/// Build a `Provider + ProviderCompleter` for the given `ModelRef`.
/// Returns a 400 error if the provider name is unknown or model config is invalid.
async fn build_completer(
    model: &ModelRef,
) -> Result<Box<dyn biorouter_mcp::knowledge::subagent::loop_::Completer>, (StatusCode, String)> {
    if biorouter_mcp::knowledge::test_mode::env_enabled() {
        return Ok(Box::new(
            biorouter_mcp::knowledge::test_mode::TestModeCompleter,
        ));
    }

    let model_config =
        ModelConfig::new(&model.model).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let provider = biorouter::providers::create(&model.provider, model_config)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Box::new(ProviderCompleter::new(provider)))
}

/// Build a well-formed SSE error frame. Uses `serde_json` for proper escaping so
/// that backslashes in Windows paths and newlines in multi-line `anyhow` chains do
/// not break JSON or SSE line framing.
fn sse_error_frame(message: &str) -> String {
    let payload = serde_json::json!({ "message": message });
    let json = serde_json::to_string(&payload)
        .unwrap_or_else(|_| String::from("{\"message\":\"<unserializable error>\"}"));
    format!("event: error\ndata: {json}\n\n")
}

async fn parse_ingest_request(
    headers: &HeaderMap,
    req: Request,
) -> Result<(convert::SourceInput, ModelRef, Option<String>), (StatusCode, String)> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if content_type.starts_with("multipart/form-data") {
        return parse_ingest_multipart(req).await;
    }

    let body_bytes = axum::body::to_bytes(req.into_body(), 16 * 1024 * 1024)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let body: IngestBody = serde_json::from_slice(&body_bytes)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let source =
        parse_source_input(&body.source).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok((source, body.model, body.focus))
}

async fn parse_ingest_multipart(
    req: Request,
) -> Result<(convert::SourceInput, ModelRef, Option<String>), (StatusCode, String)> {
    let mut mp = Multipart::from_request(req, &())
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let mut upload: Option<MultipartUpload> = None;
    let mut provider: Option<String> = None;
    let mut model_name: Option<String> = None;
    let mut focus: Option<String> = None;

    while let Some(field) = mp
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
    {
        match field.name() {
            Some("file") => {
                let filename = field
                    .file_name()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "upload.bin".to_string());
                let mime = sanitize_part_mime(field.content_type());
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
                    .to_vec();
                validate_ingest_upload(&filename, bytes.len())?;
                upload = Some(MultipartUpload {
                    bytes,
                    filename,
                    mime,
                });
            }
            Some("provider") => {
                provider = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?,
                );
            }
            Some("model") => {
                model_name = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?,
                );
            }
            Some("focus") => {
                let value = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
                if !value.trim().is_empty() {
                    focus = Some(value);
                }
            }
            _ => {}
        }
    }

    let upload = upload.ok_or((StatusCode::BAD_REQUEST, "missing 'file' part".to_string()))?;
    let provider = provider.ok_or((
        StatusCode::BAD_REQUEST,
        "missing 'provider' field".to_string(),
    ))?;
    let model_name =
        model_name.ok_or((StatusCode::BAD_REQUEST, "missing 'model' field".to_string()))?;

    Ok((
        convert::SourceInput::File {
            bytes: upload.bytes,
            filename: upload.filename,
            mime: upload.mime,
        },
        ModelRef {
            provider,
            model: model_name,
        },
        focus,
    ))
}

#[utoipa::path(
    post, path = "/knowledge/expand-path",
    request_body = ExpandPathBody,
    responses(
        (status = 200, description = "Expanded local path into stageable files", body = ExpandPathResponse),
        (status = 400, description = "Invalid local path"),
    )
)]
pub async fn expand_path(
    Json(body): Json<ExpandPathBody>,
) -> Result<Json<ExpandPathResponse>, (StatusCode, String)> {
    let expanded = source_paths::expand_ingest_path(std::path::Path::new(&body.path))
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    Ok(Json(ExpandPathResponse {
        files: expanded
            .files
            .into_iter()
            .map(|file| ExpandPathFile {
                path: file.path.to_string_lossy().into_owned(),
                name: file.name,
                relative_path: file.relative_path,
            })
            .collect(),
        warnings: expanded
            .warnings
            .into_iter()
            .map(|warning| ExpandPathWarning {
                level: match warning.level {
                    source_paths::WarningLevel::Warning => "warning".to_string(),
                    source_paths::WarningLevel::Error => "error".to_string(),
                },
                title: warning.title,
                message: warning.message,
            })
            .collect(),
    }))
}

#[utoipa::path(
    post, path = "/knowledge/check-model",
    request_body = CheckModelBody,
    responses(
        (status = 200, description = "Model responded OK", body = CheckModelResponse),
        (status = 502, description = "Model is unreachable / invalid", body = CheckModelResponse),
    )
)]
pub async fn check_model(
    State(svc): State<Arc<KnowledgeService>>,
    Json(body): Json<CheckModelBody>,
) -> Result<Json<CheckModelResponse>, (StatusCode, Json<CheckModelResponse>)> {
    let completer = match build_completer(&body.model).await {
        Ok(c) => c,
        Err((_status, msg)) => {
            return Err((
                StatusCode::BAD_GATEWAY,
                Json(CheckModelResponse {
                    ok: false,
                    error: Some(format!("provider build failed: {msg}")),
                }),
            ));
        }
    };
    match svc.check_model(completer).await {
        Ok(()) => Ok(Json(CheckModelResponse {
            ok: true,
            error: None,
        })),
        Err(e) => Err((
            StatusCode::BAD_GATEWAY,
            Json(CheckModelResponse {
                ok: false,
                error: Some(e.to_string()),
            }),
        )),
    }
}

#[utoipa::path(
    post, path = "/knowledge/bases/{id}/ingest",
    request_body = IngestBody,
    params(("id" = String, Path, description = "Knowledge base ID")),
    responses(
        (status = 200, description = "SSE stream of sub-agent events (text/event-stream)"),
        (status = 400, description = "Invalid model or source"),
    )
)]
pub async fn ingest(
    State(svc): State<Arc<KnowledgeService>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    req: Request,
) -> Result<crate::routes::reply::SseResponse, (StatusCode, String)> {
    let (source, model, focus) = parse_ingest_request(&headers, req).await?;

    let completer = build_completer(&model).await?;

    let cancel = std::sync::Arc::new(tokio::sync::Notify::new());

    let (sse_tx, sse_rx) = mpsc::channel::<String>(64);
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<SubAgentEvent>();
    let (result_tx, result_rx) = mpsc::channel::<Result<serde_json::Value, String>>(1);

    // Macro task: run ingest, then report result through a dedicated channel.
    // Dropping `event_tx` signals the forwarder that no more events are coming.
    let cancel_for_macro = cancel.clone();
    let macro_handle = tokio::spawn(async move {
        let args = ingest_macro::IngestArgs {
            kb_id: id,
            source,
            completer,
            focus,
            bounds: ingest_bounds(),
            event_sink: Some(event_tx),
            cancel: Some(cancel_for_macro),
        };
        let outcome = match ingest_macro::ingest(&svc, args).await {
            Ok(r) => Ok(serde_json::to_value(&r).unwrap_or_default()),
            Err(e) => Err(e.to_string()),
        };
        let _ = result_tx.send(outcome).await;
    });

    // Forwarder task: owns `sse_tx`. Drains all SubAgent events first (guaranteed
    // ordering), then emits the terminal done/error frame from the macro result.
    // This eliminates the race where `done` could arrive before the last data events.
    // If the client disconnects (sse_tx.send returns Err), signal cancellation.
    let cancel_for_forwarder = cancel.clone();
    tokio::spawn(async move {
        while let Some(ev) = event_rx.recv().await {
            if let Ok(j) = serde_json::to_string(&ev) {
                if sse_tx.send(format!("data: {j}\n\n")).await.is_err() {
                    cancel_for_forwarder.notify_one();
                    return;
                }
            }
        }
        finish_macro_stream(sse_tx, result_rx, macro_handle).await;
    });

    Ok(crate::routes::reply::SseResponse::from_rx(sse_rx))
}

#[utoipa::path(
    post, path = "/knowledge/bases/{id}/ingest-conversation",
    request_body = IngestConversationBody,
    params(("id" = String, Path, description = "Knowledge base ID")),
    responses(
        (status = 200, description = "SSE stream of sub-agent events (text/event-stream)"),
        (status = 400, description = "Invalid model or no sessions"),
    )
)]
pub async fn ingest_conversation(
    State(svc): State<Arc<KnowledgeService>>,
    Path(id): Path<String>,
    Json(body): Json<IngestConversationBody>,
) -> Result<crate::routes::reply::SseResponse, (StatusCode, String)> {
    if body.session_ids.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "session_ids cannot be empty".into(),
        ));
    }

    // Load the requested sessions (with messages) from the global session store.
    let session_manager = biorouter::session::session_manager::SessionManager::instance();
    let mut sessions = Vec::new();
    for sid in &body.session_ids {
        match session_manager.get_session(sid, true).await {
            Ok(s) => sessions.push(s),
            Err(e) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!("session '{sid}' not found: {e}"),
                ));
            }
        }
    }

    let completer = build_completer(&body.model).await?;
    let cancel = std::sync::Arc::new(tokio::sync::Notify::new());

    let (sse_tx, sse_rx) = mpsc::channel::<String>(64);
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<SubAgentEvent>();
    let (result_tx, result_rx) = mpsc::channel::<Result<serde_json::Value, String>>(1);

    let focus = body.focus.clone();
    let cancel_for_macro = cancel.clone();
    let macro_handle = tokio::spawn(async move {
        let args = biorouter::knowledge::conversation_ingest::ConversationIngestArgs {
            kb_id: id,
            sessions,
            completer,
            focus,
            bounds: ingest_bounds(),
            event_sink: Some(event_tx),
            cancel: Some(cancel_for_macro),
        };
        let outcome = match biorouter::knowledge::conversation_ingest::ingest_conversation(
            &svc, args,
        )
        .await
        {
            Ok(r) => Ok(serde_json::to_value(&r).unwrap_or_default()),
            Err(e) => Err(e.to_string()),
        };
        let _ = result_tx.send(outcome).await;
    });

    let cancel_for_forwarder = cancel.clone();
    tokio::spawn(async move {
        while let Some(ev) = event_rx.recv().await {
            if let Ok(j) = serde_json::to_string(&ev) {
                if sse_tx.send(format!("data: {j}\n\n")).await.is_err() {
                    cancel_for_forwarder.notify_one();
                    return;
                }
            }
        }
        finish_macro_stream(sse_tx, result_rx, macro_handle).await;
    });

    Ok(crate::routes::reply::SseResponse::from_rx(sse_rx))
}

#[utoipa::path(
    post, path = "/knowledge/bases/{id}/query",
    request_body = QueryBody,
    params(("id" = String, Path, description = "Knowledge base ID")),
    responses(
        (status = 200, description = "SSE stream of sub-agent events (text/event-stream)"),
        (status = 400, description = "Invalid model"),
    )
)]
pub async fn query_kb(
    State(svc): State<Arc<KnowledgeService>>,
    Path(id): Path<String>,
    Json(body): Json<QueryBody>,
) -> Result<crate::routes::reply::SseResponse, (StatusCode, String)> {
    let completer = build_completer(&body.model).await?;

    let cancel = std::sync::Arc::new(tokio::sync::Notify::new());

    let (sse_tx, sse_rx) = mpsc::channel::<String>(64);
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<SubAgentEvent>();
    let (result_tx, result_rx) = mpsc::channel::<Result<serde_json::Value, String>>(1);

    let cancel_for_macro = cancel.clone();
    let macro_handle = tokio::spawn(async move {
        let args = query_macro::QueryArgs {
            kb_id: id,
            question: body.question,
            completer,
            file_as_page: body.file_as_page.unwrap_or(false),
            bounds: SubAgentBounds::default(),
            event_sink: Some(event_tx),
            cancel: Some(cancel_for_macro),
        };
        let outcome = match query_macro::query(&svc, args).await {
            Ok(r) => Ok(serde_json::to_value(&r).unwrap_or_default()),
            Err(e) => Err(e.to_string()),
        };
        let _ = result_tx.send(outcome).await;
    });

    let cancel_for_forwarder = cancel.clone();
    tokio::spawn(async move {
        while let Some(ev) = event_rx.recv().await {
            if let Ok(j) = serde_json::to_string(&ev) {
                if sse_tx.send(format!("data: {j}\n\n")).await.is_err() {
                    cancel_for_forwarder.notify_one();
                    return;
                }
            }
        }
        finish_macro_stream(sse_tx, result_rx, macro_handle).await;
    });

    Ok(crate::routes::reply::SseResponse::from_rx(sse_rx))
}

#[utoipa::path(
    post, path = "/knowledge/bases/{id}/lint",
    request_body = LintBody,
    params(("id" = String, Path, description = "Knowledge base ID")),
    responses(
        (status = 200, description = "SSE stream of sub-agent events (text/event-stream)"),
        (status = 400, description = "Invalid model"),
    )
)]
pub async fn lint(
    State(svc): State<Arc<KnowledgeService>>,
    Path(id): Path<String>,
    Json(body): Json<LintBody>,
) -> Result<crate::routes::reply::SseResponse, (StatusCode, String)> {
    let autofix = body.autofix.unwrap_or(false);
    // Only build a completer when autofix is requested (it requires an LLM).
    let completer: Option<Box<dyn biorouter_mcp::knowledge::subagent::loop_::Completer>> =
        if autofix {
            Some(build_completer(&body.model).await?)
        } else {
            None
        };

    let cancel = std::sync::Arc::new(tokio::sync::Notify::new());

    let (sse_tx, sse_rx) = mpsc::channel::<String>(64);
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<SubAgentEvent>();
    let (result_tx, result_rx) = mpsc::channel::<Result<serde_json::Value, String>>(1);

    let cancel_for_macro = cancel.clone();
    let macro_handle = tokio::spawn(async move {
        let args = lint_macro::LintArgs {
            kb_id: id,
            completer,
            autofix,
            bounds: SubAgentBounds::default(),
            event_sink: Some(event_tx),
            cancel: Some(cancel_for_macro),
        };
        let outcome = match lint_macro::lint(&svc, args).await {
            Ok(r) => Ok(serde_json::to_value(&r).unwrap_or_default()),
            Err(e) => Err(e.to_string()),
        };
        let _ = result_tx.send(outcome).await;
    });

    let cancel_for_forwarder = cancel.clone();
    tokio::spawn(async move {
        while let Some(ev) = event_rx.recv().await {
            if let Ok(j) = serde_json::to_string(&ev) {
                if sse_tx.send(format!("data: {j}\n\n")).await.is_err() {
                    cancel_for_forwarder.notify_one();
                    return;
                }
            }
        }
        finish_macro_stream(sse_tx, result_rx, macro_handle).await;
    });

    Ok(crate::routes::reply::SseResponse::from_rx(sse_rx))
}

/// Parse the JSON `source` field into a typed `SourceInput`.
fn parse_source_input(v: &serde_json::Value) -> anyhow::Result<convert::SourceInput> {
    if let Some(url) = v.get("url").and_then(|x| x.as_str()) {
        Ok(convert::SourceInput::Url(url.to_string()))
    } else if let Some(path) = v.get("path").and_then(|x| x.as_str()) {
        Ok(convert::SourceInput::Path(std::path::PathBuf::from(path)))
    } else if let Some(text) = v.get("text").and_then(|x| x.as_str()) {
        Ok(convert::SourceInput::Text {
            text: text.to_string(),
            title: v
                .get("title")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string()),
        })
    } else {
        anyhow::bail!("source must have 'url', 'path', or 'text'")
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Task 8: POST /bases/:id/raw  (multipart file | JSON {url} | JSON {text,title})
// ──────────────────────────────────────────────────────────────────────────────

#[utoipa::path(
    post, path = "/knowledge/bases/{id}/raw",
    params(("id" = String, Path, description = "Knowledge base ID")),
    request_body(
        content = inline(serde_json::Value),
        description = "One of: multipart/form-data with 'file' field, \
                       JSON {url}, or JSON {text, title?}",
    ),
    responses(
        (status = 200, description = "Source ingested", body = RawSourceResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal error"),
    )
)]
pub async fn add_raw_source(
    State(svc): State<Arc<KnowledgeService>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    req: Request,
) -> Result<Json<RawSourceResponse>, (StatusCode, String)> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let input = if content_type.starts_with("multipart/form-data") {
        // Parse multipart — consume the whole request.
        let mut mp = Multipart::from_request(req, &())
            .await
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
        let mut upload: Option<MultipartUpload> = None;
        while let Some(field) = mp
            .next_field()
            .await
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
        {
            if field.name() == Some("file") {
                let filename = field
                    .file_name()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "upload.bin".to_string());
                let mime = sanitize_part_mime(field.content_type());
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
                    .to_vec();
                validate_ingest_upload(&filename, bytes.len())?;
                upload = Some(MultipartUpload {
                    bytes,
                    filename,
                    mime,
                });
            }
        }
        let upload = upload.ok_or((StatusCode::BAD_REQUEST, "missing 'file' part".to_string()))?;
        convert::SourceInput::File {
            bytes: upload.bytes,
            filename: upload.filename,
            mime: upload.mime,
        }
    } else {
        // JSON body — read raw bytes then parse.
        let body_bytes = axum::body::to_bytes(req.into_body(), 16 * 1024 * 1024)
            .await
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
        let json: serde_json::Value = serde_json::from_slice(&body_bytes)
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
        if let Some(url) = json.get("url").and_then(|v| v.as_str()) {
            convert::SourceInput::Url(url.to_string())
        } else if let Some(path) = json.get("path").and_then(|v| v.as_str()) {
            convert::SourceInput::Path(std::path::PathBuf::from(path))
        } else if let Some(text) = json.get("text").and_then(|v| v.as_str()) {
            let title = json
                .get("title")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            convert::SourceInput::Text {
                text: text.to_string(),
                title,
            }
        } else {
            return Err((
                StatusCode::BAD_REQUEST,
                "expected file (multipart), {url}, or {text}".to_string(),
            ));
        }
    };

    let _lock = svc
        .lock_kb(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let res = svc
        .add_raw_source(&id, input, None)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(RawSourceResponse {
        source_id: res.source_id,
        source_md_path: res.source_md_path,
    }))
}

// ──────────────────────────────────────────────────────────────────────────────
// Task 10: GET /bases/:id/export + POST /bases/import
// ──────────────────────────────────────────────────────────────────────────────

#[utoipa::path(
    get, path = "/knowledge/bases/{id}/export",
    params(("id" = String, Path, description = "Knowledge base ID")),
    responses(
        (status = 200, description = "Binary .brkb archive", content_type = "application/octet-stream"),
        (status = 404, description = "Not found"),
        (status = 500, description = "Internal error"),
    )
)]
pub async fn export_brkb(
    State(svc): State<Arc<KnowledgeService>>,
    Path(id): Path<String>,
) -> Result<Response, (StatusCode, String)> {
    let kb_root = paths::kb_root(svc.root(), &id);
    if !kb_root.exists() {
        return Err((StatusCode::NOT_FOUND, format!("kb '{id}' not found")));
    }
    let bytes = svc
        .export_brkb(&id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let disposition = format!("attachment; filename=\"{id}.brkb\"");
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_DISPOSITION, disposition)
        .body(Body::from(bytes))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(response)
}

#[utoipa::path(
    post, path = "/knowledge/bases/import",
    request_body(
        content = inline(serde_json::Value),
        description = "multipart/form-data with a 'file' field containing the .brkb archive",
    ),
    responses(
        (status = 200, description = "Imported knowledge base ID"),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal error"),
    )
)]
pub async fn import_brkb(
    State(svc): State<Arc<KnowledgeService>>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let mut file_bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
    {
        if field.name() == Some("file") {
            file_bytes = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
                    .to_vec(),
            );
        }
    }

    let bytes = file_bytes.ok_or((StatusCode::BAD_REQUEST, "missing 'file' part".to_string()))?;

    let new_id = svc
        .import_brkb(&bytes)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({ "id": new_id })))
}

// ──────────────────────────────────────────────────────────────────────────────
// Task 11: POST /bases/:id/sources/:sid/reclassify
//          PUT  /bases/:id/sources/:sid/credibility
// ──────────────────────────────────────────────────────────────────────────────

#[utoipa::path(
    post, path = "/knowledge/bases/{id}/sources/{sid}/reclassify",
    params(
        ("id" = String, Path, description = "Knowledge base ID"),
        ("sid" = String, Path, description = "Source ID"),
    ),
    responses(
        (status = 200, description = "Reclassified credibility", body = CredibilityResponse),
        (status = 404, description = "Source not found"),
        (status = 500, description = "Internal error"),
    )
)]
pub async fn reclassify(
    State(svc): State<Arc<KnowledgeService>>,
    Path((id, sid)): Path<(String, String)>,
) -> Result<Json<CredibilityResponse>, (StatusCode, String)> {
    let source_path = paths::kb_root(svc.root(), &id).join("raw").join(&sid);
    if !source_path.exists() {
        return Err((
            StatusCode::NOT_FOUND,
            format!("source '{sid}' not found in kb '{id}'"),
        ));
    }
    let credibility = svc
        .reclassify_source(&id, &sid)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(CredibilityResponse { credibility }))
}

#[utoipa::path(
    put, path = "/knowledge/bases/{id}/sources/{sid}/credibility",
    request_body = Credibility,
    params(
        ("id" = String, Path, description = "Knowledge base ID"),
        ("sid" = String, Path, description = "Source ID"),
    ),
    responses(
        (status = 200, description = "Credibility overridden", body = CredibilityResponse),
        (status = 404, description = "Source not found"),
        (status = 500, description = "Internal error"),
    )
)]
pub async fn override_credibility(
    State(svc): State<Arc<KnowledgeService>>,
    Path((id, sid)): Path<(String, String)>,
    Json(cred): Json<Credibility>,
) -> Result<Json<CredibilityResponse>, (StatusCode, String)> {
    let source_path = paths::kb_root(svc.root(), &id).join("raw").join(&sid);
    if !source_path.exists() {
        return Err((
            StatusCode::NOT_FOUND,
            format!("source '{sid}' not found in kb '{id}'"),
        ));
    }
    let credibility = svc
        .override_credibility(&id, &sid, cred)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(CredibilityResponse { credibility }))
}
