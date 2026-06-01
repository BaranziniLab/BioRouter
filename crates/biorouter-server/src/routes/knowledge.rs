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
    paths, registry,
    service::KnowledgeService,
    store,
    subagent::{events::SubAgentEvent, loop_::SubAgentBounds},
    types::{Credibility, Graph, HistoryEntry, Manifest, ModelRef},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use utoipa::ToSchema;

/// Build the knowledge router.  The router owns an `Arc<KnowledgeService>` directly so
/// it can be tested without constructing a full `AppState`.
pub fn router(svc: Arc<KnowledgeService>) -> Router {
    Router::new()
        .route("/bases", get(list_bases).post(create_base))
        .route("/bases/import", post(import_brkb))
        .route("/bases/{id}", get(get_base).delete(delete_base))
        .route("/bases/{id}/graph", get(get_graph))
        .route("/bases/{id}/pages", get(list_pages))
        .route(
            "/bases/{id}/pages/{*page_path}",
            get(read_page).put(write_page),
        )
        .route("/bases/{id}/history", get(list_history))
        .route("/bases/{id}/preview", post(preview_state))
        .route("/bases/{id}/restore", post(restore_state))
        .route("/bases/{id}/raw", post(add_raw_source))
        .route("/bases/{id}/ingest", post(ingest))
        .route("/bases/{id}/query", post(query_kb))
        .route("/bases/{id}/lint", post(lint))
        .route("/bases/{id}/export", get(export_brkb))
        .route("/bases/{id}/sources/{sid}/reclassify", post(reclassify))
        .route(
            "/bases/{id}/sources/{sid}/credibility",
            put(override_credibility),
        )
        .with_state(svc)
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
    let bases = svc
        .list_bases()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    bases
        .into_iter()
        .find(|b| b.id == id)
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, format!("kb '{id}' not found")))
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
    let kb_root = paths::kb_root(svc.root(), &id);
    if !kb_root.exists() {
        return Err((StatusCode::NOT_FOUND, format!("kb '{id}' not found")));
    }
    std::fs::remove_dir_all(&kb_root)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    registry::unregister(svc.root(), &id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
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
    let model_config =
        ModelConfig::new(&model.model).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let provider = biorouter::providers::create(&model.provider, model_config)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Box::new(ProviderCompleter::new(provider)))
}

/// Spawn the SSE forwarder: reads `SubAgentEvent`s from `event_rx` and sends
/// serialized SSE frames to `sse_tx`.
fn spawn_event_forwarder(
    mut event_rx: tokio::sync::mpsc::UnboundedReceiver<SubAgentEvent>,
    sse_tx: mpsc::Sender<String>,
) {
    tokio::spawn(async move {
        while let Some(ev) = event_rx.recv().await {
            if let Ok(j) = serde_json::to_string(&ev) {
                let _ = sse_tx.send(format!("data: {j}\n\n")).await;
            }
        }
    });
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
    Json(body): Json<IngestBody>,
) -> Result<crate::routes::reply::SseResponse, (StatusCode, String)> {
    let source =
        parse_source_input(&body.source).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let completer = build_completer(&body.model).await?;

    let (sse_tx, sse_rx) = mpsc::channel::<String>(64);
    let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<SubAgentEvent>();

    spawn_event_forwarder(event_rx, sse_tx.clone());

    tokio::spawn(async move {
        let args = ingest_macro::IngestArgs {
            kb_id: id,
            source,
            completer,
            focus: body.focus,
            bounds: SubAgentBounds::default(),
            event_sink: Some(event_tx),
        };
        match ingest_macro::ingest(&svc, args).await {
            Ok(result) => {
                let json = serde_json::to_value(&result).unwrap_or_default();
                let _ = sse_tx.send(format!("event: done\ndata: {json}\n\n")).await;
            }
            Err(e) => {
                let msg = e.to_string().replace('"', "\\\"");
                let _ = sse_tx
                    .send(format!("event: error\ndata: {{\"message\":\"{msg}\"}}\n\n"))
                    .await;
            }
        }
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

    let (sse_tx, sse_rx) = mpsc::channel::<String>(64);
    let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<SubAgentEvent>();

    spawn_event_forwarder(event_rx, sse_tx.clone());

    tokio::spawn(async move {
        let args = query_macro::QueryArgs {
            kb_id: id,
            question: body.question,
            completer,
            file_as_page: body.file_as_page.unwrap_or(false),
            bounds: SubAgentBounds::default(),
            event_sink: Some(event_tx),
        };
        match query_macro::query(&svc, args).await {
            Ok(result) => {
                let json = serde_json::to_value(&result).unwrap_or_default();
                let _ = sse_tx.send(format!("event: done\ndata: {json}\n\n")).await;
            }
            Err(e) => {
                let msg = e.to_string().replace('"', "\\\"");
                let _ = sse_tx
                    .send(format!("event: error\ndata: {{\"message\":\"{msg}\"}}\n\n"))
                    .await;
            }
        }
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

    let (sse_tx, sse_rx) = mpsc::channel::<String>(64);
    let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<SubAgentEvent>();

    spawn_event_forwarder(event_rx, sse_tx.clone());

    tokio::spawn(async move {
        let args = lint_macro::LintArgs {
            kb_id: id,
            completer,
            autofix,
            bounds: SubAgentBounds::default(),
            event_sink: Some(event_tx),
        };
        match lint_macro::lint(&svc, args).await {
            Ok(result) => {
                let json = serde_json::to_value(&result).unwrap_or_default();
                let _ = sse_tx.send(format!("event: done\ndata: {json}\n\n")).await;
            }
            Err(e) => {
                let msg = e.to_string().replace('"', "\\\"");
                let _ = sse_tx
                    .send(format!("event: error\ndata: {{\"message\":\"{msg}\"}}\n\n"))
                    .await;
            }
        }
    });

    Ok(crate::routes::reply::SseResponse::from_rx(sse_rx))
}

/// Parse the JSON `source` field into a typed `SourceInput`.
fn parse_source_input(v: &serde_json::Value) -> anyhow::Result<convert::SourceInput> {
    if let Some(url) = v.get("url").and_then(|x| x.as_str()) {
        Ok(convert::SourceInput::Url(url.to_string()))
    } else if let Some(text) = v.get("text").and_then(|x| x.as_str()) {
        Ok(convert::SourceInput::Text {
            text: text.to_string(),
            title: v
                .get("title")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string()),
        })
    } else {
        anyhow::bail!("source must have 'url' or 'text'")
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
        let mut bytes_opt: Option<Vec<u8>> = None;
        let mut filename: Option<String> = None;
        while let Some(field) = mp
            .next_field()
            .await
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
        {
            if field.name() == Some("file") {
                filename = field.file_name().map(|s| s.to_string());
                bytes_opt = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
                        .to_vec(),
                );
            }
        }
        let bytes =
            bytes_opt.ok_or((StatusCode::BAD_REQUEST, "missing 'file' part".to_string()))?;
        let fname = filename.unwrap_or_else(|| "upload.bin".to_string());
        convert::SourceInput::File {
            bytes,
            filename: fname,
            mime: None,
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
    let credibility = svc
        .override_credibility(&id, &sid, cred)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(CredibilityResponse { credibility }))
}
