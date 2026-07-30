//! Seeing and pruning what has been remembered.
//!
//! Issue #63 put every machine-wide memory read and write behind the user's
//! approval. These routes are what makes that approval mean something: the
//! confirmation card names a category, and until now nothing anywhere in
//! Biorouter would show what that category held, or let the user throw it away.
//!
//! Two properties are load-bearing.
//!
//! **The global store is resolved here, never sent by the client.**
//! [`default_global_store`] goes through [`biorouter_mcp::global_memory_dir`],
//! the one resolver that honours `BIOROUTER_PATH_ROOT`; a request cannot name a
//! different one. The local store *is* client-supplied, because it has to be:
//! the daemon is a single process serving every window, and a window's local
//! memories live under the project it was opened in. Falling back to the
//! daemon's own working directory would show — and delete — some other
//! project's memories, so a local operation with no `working_dir` is refused
//! rather than guessed at.
//!
//! **A category is still a name.** Every path here goes through
//! `MemoryServer::get_memory_file`, so the containment checks issue #73 added
//! govern this door exactly as they govern the four MCP tools. A refused
//! category is a `400`, because it is the caller's mistake.

use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use biorouter_mcp::{EntryDeletion, MemoryScope, MemoryServer, MemoryStoreInventory};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::state::AppState;

/// Both stores, as far as the caller can see them.
#[derive(Debug, Serialize, ToSchema)]
pub struct MemoryInventoryResponse {
    /// The machine-wide store every Biorouter session on this computer shares.
    pub global: MemoryStoreInventory,
    /// This project's `.biorouter/memory`. `null` when the caller did not name
    /// a project — Settings can be opened from a window that has none.
    pub local: Option<MemoryStoreInventory>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct MemoryInventoryQuery {
    /// The project directory whose local store to list. Omit for global only.
    pub working_dir: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MemoryDeleteEntryRequest {
    pub scope: MemoryScope,
    pub category: String,
    /// The entry's position in the category, as listed.
    pub index: usize,
    /// The body that was listed at that position. The delete refuses unless it
    /// still matches, so a list that went stale while an agent appended to the
    /// store cannot delete the wrong memory.
    pub content: String,
    /// Required when `scope` is `local`.
    pub working_dir: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MemoryDeleteEntryResponse {
    /// Memories left in the category.
    pub remaining: usize,
    /// Whether the category itself was removed because it emptied.
    pub category_removed: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MemoryDeleteCategoryRequest {
    pub scope: MemoryScope,
    pub category: String,
    /// Required when `scope` is `local`.
    pub working_dir: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MemoryDeleteCategoryResponse {
    /// How many memories the category held.
    pub removed_entries: usize,
}

/// The machine-wide store, resolved the one way it may be resolved.
pub fn default_global_store() -> PathBuf {
    biorouter_mcp::global_memory_dir()
}

type ApiError = (StatusCode, String);

/// The store directory for a project, matching what the memory extension uses
/// for `is_global=false`.
fn local_store_for(_working_dir: &str) -> PathBuf {
    PathBuf::new() // STUB
}

fn server_for(_global: &PathBuf, _working_dir: Option<&str>) -> Result<MemoryServer, ApiError> {
    Err((StatusCode::NOT_IMPLEMENTED, "stub".into())) // STUB
}

#[utoipa::path(
    get,
    path = "/memory/inventory",
    params(MemoryInventoryQuery),
    responses(
        (status = 200, description = "Everything both memory stores hold", body = MemoryInventoryResponse),
        (status = 500, description = "A store could not be read"),
    ),
)]
async fn memory_inventory(
    State(_global): State<Arc<PathBuf>>,
    Query(_query): Query<MemoryInventoryQuery>,
) -> Result<Json<MemoryInventoryResponse>, ApiError> {
    // STUB
    Err((StatusCode::NOT_IMPLEMENTED, "stub".into()))
}

#[utoipa::path(
    post,
    path = "/memory/delete_entry",
    request_body = MemoryDeleteEntryRequest,
    responses(
        (status = 200, description = "The memory was deleted", body = MemoryDeleteEntryResponse),
        (status = 400, description = "Invalid category, or a local scope with no working_dir"),
        (status = 404, description = "No memory at that position"),
        (status = 409, description = "The category changed since it was listed"),
    ),
)]
async fn memory_delete_entry(
    State(_global): State<Arc<PathBuf>>,
    Json(_req): Json<MemoryDeleteEntryRequest>,
) -> Result<Json<MemoryDeleteEntryResponse>, ApiError> {
    // STUB
    Err((StatusCode::NOT_IMPLEMENTED, "stub".into()))
}

#[utoipa::path(
    post,
    path = "/memory/delete_category",
    request_body = MemoryDeleteCategoryRequest,
    responses(
        (status = 200, description = "The category was deleted", body = MemoryDeleteCategoryResponse),
        (status = 400, description = "Invalid category, or a local scope with no working_dir"),
        (status = 404, description = "No such category"),
    ),
)]
async fn memory_delete_category(
    State(_global): State<Arc<PathBuf>>,
    Json(_req): Json<MemoryDeleteCategoryRequest>,
) -> Result<Json<MemoryDeleteCategoryResponse>, ApiError> {
    // STUB
    Err((StatusCode::NOT_IMPLEMENTED, "stub".into()))
}

/// The routes over an explicit global store — the seam the tests use so they
/// never read or delete the machine's real memories.
pub fn router_with_global_store(global_memory_dir: PathBuf) -> Router {
    Router::new()
        .route("/memory/inventory", get(memory_inventory))
        .route("/memory/delete_entry", post(memory_delete_entry))
        .route("/memory/delete_category", post(memory_delete_category))
        .with_state(Arc::new(global_memory_dir))
}

pub fn router() -> Router {
    router_with_global_store(default_global_store())
}

pub fn routes(_state: Arc<AppState>) -> Router {
    router()
}

#[allow(dead_code)]
fn unused(e: EntryDeletion, m: MemoryScope) -> (EntryDeletion, MemoryScope) {
    (e, m)
}

#[allow(dead_code)]
fn unused_local(w: &str) -> PathBuf {
    local_store_for(w)
}

#[allow(dead_code)]
fn unused_server(g: &PathBuf, w: Option<&str>) -> Result<MemoryServer, ApiError> {
    server_for(g, w)
}
