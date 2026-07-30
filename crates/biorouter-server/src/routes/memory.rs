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

/// The store directory for a project — the same `<project>/.biorouter/memory`
/// the memory extension writes to for `is_global=false`.
fn local_store_for(working_dir: &str) -> PathBuf {
    PathBuf::from(working_dir).join(".biorouter").join("memory")
}

/// A management server over the machine-wide store and, when the caller named a
/// project, that project's store.
///
/// When no project is named the local store is pointed at an unnameable path
/// rather than at the daemon's working directory: every caller that could act
/// on it has already been refused by [`require_project`], and a directory that
/// cannot exist keeps a future caller from silently operating on the wrong
/// project's memories.
fn server_for(global: &PathBuf, working_dir: Option<&str>) -> MemoryServer {
    let local = working_dir
        .map(local_store_for)
        .unwrap_or_else(|| PathBuf::from("\0no-project"));
    MemoryServer::with_stores(global.clone(), local)
}

/// A local operation must say which project. Guessing would list — or delete —
/// memories belonging to whatever directory the daemon happens to be running in.
fn require_project(scope: MemoryScope, working_dir: Option<&str>) -> Result<(), ApiError> {
    if scope == MemoryScope::Local && working_dir.is_none_or(str::is_empty) {
        return Err((
            StatusCode::BAD_REQUEST,
            "A local memory operation needs the project it belongs to: send working_dir. \
             Local memories live in <project>/.biorouter/memory, and the daemon serves every \
             window, so there is no single directory it could mean."
                .to_string(),
        ));
    }
    Ok(())
}

/// Map a store error onto a status. A refused category is the caller's mistake
/// — `400`, the same reading `MemoryServer::tool_error` gives the model —
/// rather than a `500` that reads as "the daemon broke, retry".
fn store_error(e: &std::io::Error) -> ApiError {
    let status = if e.kind() == std::io::ErrorKind::InvalidInput {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    };
    (status, e.to_string())
}

/// Normalise an omitted-or-blank `working_dir` to `None`, so an empty string
/// from a form or a window with no project takes the "no project" path instead
/// of resolving to `/.biorouter/memory`.
fn project_dir(working_dir: Option<&String>) -> Option<&str> {
    working_dir.map(String::as_str).filter(|d| !d.is_empty())
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
    State(global): State<Arc<PathBuf>>,
    Query(query): Query<MemoryInventoryQuery>,
) -> Result<Json<MemoryInventoryResponse>, ApiError> {
    let working_dir = project_dir(query.working_dir.as_ref());
    let server = server_for(&global, working_dir);

    let global_inventory = server
        .inventory(MemoryScope::Global)
        .map_err(|e| store_error(&e))?;
    let local_inventory = match working_dir {
        Some(_) => Some(
            server
                .inventory(MemoryScope::Local)
                .map_err(|e| store_error(&e))?,
        ),
        None => None,
    };

    Ok(Json(MemoryInventoryResponse {
        global: global_inventory,
        local: local_inventory,
    }))
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
    State(global): State<Arc<PathBuf>>,
    Json(req): Json<MemoryDeleteEntryRequest>,
) -> Result<Json<MemoryDeleteEntryResponse>, ApiError> {
    let working_dir = project_dir(req.working_dir.as_ref());
    require_project(req.scope, working_dir)?;
    let server = server_for(&global, working_dir);

    match server
        .delete_entry(&req.category, req.scope, req.index, &req.content)
        .map_err(|e| store_error(&e))?
    {
        EntryDeletion::Deleted {
            remaining,
            category_removed,
        } => Ok(Json(MemoryDeleteEntryResponse {
            remaining,
            category_removed,
        })),
        EntryDeletion::OutOfRange => Err((
            StatusCode::NOT_FOUND,
            format!(
                "There is no memory at position {} of category \"{}\". It has already been \
                 deleted; reload to see what is there now.",
                req.index, req.category
            ),
        )),
        EntryDeletion::ContentMismatch => Err((
            StatusCode::CONFLICT,
            format!(
                "The memory at position {} of category \"{}\" is not the one that was listed — \
                 the category changed since then, most likely because a conversation wrote to \
                 it. Nothing was deleted; reload and try again.",
                req.index, req.category
            ),
        )),
    }
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
    State(global): State<Arc<PathBuf>>,
    Json(req): Json<MemoryDeleteCategoryRequest>,
) -> Result<Json<MemoryDeleteCategoryResponse>, ApiError> {
    let working_dir = project_dir(req.working_dir.as_ref());
    require_project(req.scope, working_dir)?;
    let server = server_for(&global, working_dir);

    match server
        .delete_category(&req.category, req.scope)
        .map_err(|e| store_error(&e))?
    {
        Some(removed_entries) => Ok(Json(MemoryDeleteCategoryResponse { removed_entries })),
        None => Err((
            StatusCode::NOT_FOUND,
            format!(
                "There is no memory category \"{}\" in the {} store.",
                req.category,
                match req.scope {
                    MemoryScope::Global => "global",
                    MemoryScope::Local => "local",
                }
            ),
        )),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The local store the routes manage has to be the one the memory
    /// extension writes to, or Settings would list a directory nothing uses.
    #[test]
    fn the_local_store_is_the_projects_biorouter_memory() {
        assert_eq!(
            local_store_for("/Users/someone/work/proj"),
            PathBuf::from("/Users/someone/work/proj/.biorouter/memory")
        );
    }

    /// A blank `working_dir` is "no project", not the filesystem root. Without
    /// this an empty string would resolve to `/.biorouter/memory`.
    #[test]
    fn a_blank_working_dir_is_no_project_at_all() {
        assert_eq!(project_dir(Some(&String::new())), None);
        assert_eq!(project_dir(None), None);
        assert_eq!(project_dir(Some(&"/proj".to_string())), Some("/proj"));
    }

    #[test]
    fn only_a_local_operation_needs_a_project() {
        assert!(require_project(MemoryScope::Global, None).is_ok());
        assert!(require_project(MemoryScope::Local, Some("/proj")).is_ok());
        let (status, message) = require_project(MemoryScope::Local, None).unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(message.contains("working_dir"));
    }
}
