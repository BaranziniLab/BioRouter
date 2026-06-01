use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post, put},
    Json, Router,
};
use biorouter_mcp::knowledge::{
    paths, registry,
    service::KnowledgeService,
    store,
    types::{Graph, HistoryEntry, Manifest},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
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
        .route("/bases/{id}/pages/{*page_path}", get(read_page).put(write_page))
        .route("/bases/{id}/history", get(list_history))
        .route("/bases/{id}/preview", post(preview_state))
        .route("/bases/{id}/restore", post(restore_state))
        .route("/bases/{id}/raw", post(add_raw_source))
        .route("/bases/{id}/ingest", post(ingest))
        .route("/bases/{id}/query", post(query_kb))
        .route("/bases/{id}/lint", post(lint))
        .route("/bases/{id}/export", get(export_brkb))
        .route(
            "/bases/{id}/sources/{sid}/reclassify",
            post(reclassify),
        )
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
    let sha_opt = store::write_page(&kb_root, &page_path, &body.content, &body.commit_message, None)
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
// Stubs for Tasks 8-11 (return 501 Not Implemented)
// ──────────────────────────────────────────────────────────────────────────────

pub async fn add_raw_source() -> StatusCode {
    StatusCode::NOT_IMPLEMENTED
}

pub async fn ingest() -> StatusCode {
    StatusCode::NOT_IMPLEMENTED
}

pub async fn query_kb() -> StatusCode {
    StatusCode::NOT_IMPLEMENTED
}

pub async fn lint() -> StatusCode {
    StatusCode::NOT_IMPLEMENTED
}

pub async fn export_brkb() -> StatusCode {
    StatusCode::NOT_IMPLEMENTED
}

pub async fn import_brkb() -> StatusCode {
    StatusCode::NOT_IMPLEMENTED
}

pub async fn reclassify() -> StatusCode {
    StatusCode::NOT_IMPLEMENTED
}

pub async fn override_credibility() -> StatusCode {
    StatusCode::NOT_IMPLEMENTED
}
