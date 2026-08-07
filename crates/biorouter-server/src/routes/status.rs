use axum::body::Body;
use axum::extract::State;
use axum::http::HeaderValue;
use axum::response::{IntoResponse, Response};
use axum::{extract::Path, http::StatusCode, routing::get, Json, Router};
use biorouter::session::{generate_diagnostics, get_system_info, SystemInfo};
use std::sync::Arc;

use crate::state::AppState;

#[utoipa::path(get, path = "/status",
    responses(
        (status = 200, description = "ok", body = String),
    )
)]
async fn status() -> String {
    "ok".to_string()
}

#[utoipa::path(get, path = "/system_info",
    responses(
        (status = 200, description = "System information", body = SystemInfo),
    )
)]
async fn system_info() -> Json<SystemInfo> {
    Json(get_system_info())
}

// `GET /diagnostics/{session_id}` — the support bundle for one chat.
//
// ⚠ **This route returns the transcript.** `generate_diagnostics` writes
// `session.json` into the zip from `SessionManager::export_session`, which is
// `get_session(id, true)` — byte for byte the payload `GET /sessions/{id}` and
// `GET /sessions/{id}/export` return, both of which have been gated since
// Task 58. It ships this session's log files beside it, which carry its
// prompts. So it is a third spelling of the same read, and
// `routes::session_reach`'s own header names that shape: an unguarded sibling
// of a guarded read is the defect this campaign keeps shipping.
//
// ⚠ Deliberately `//` and not `///`: utoipa publishes a handler's doc comment
// as the operation's `description` in `ui/desktop/openapi.json`, and this is
// internal reasoning about a defect, not something to ship to every consumer of
// the spec. The sibling `session::export_session` is written the same way.
#[utoipa::path(get, path = "/diagnostics/{session_id}",
    responses(
        (status = 200, description = "Diagnostics zip file", content_type = "application/zip", body = Vec<u8>),
        (status = 403, description = "Out of reach - a private or unreadable session named without the user-action proof"),
        (status = 500, description = "Failed to generate diagnostics"),
    )
)]
async fn diagnostics(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    headers: axum::http::HeaderMap,
) -> Response {
    // Issue #56 Task 58 / #47. `session_id` is a request parameter, not a
    // credential; see `routes::session_reach`.
    if let Err(refusal) =
        crate::routes::session_reach::session_reach(state.session_manager(), &session_id, &headers)
            .await
    {
        return refusal.into_response();
    }
    match generate_diagnostics(state.session_manager(), &session_id).await {
        Ok(zip_data) => {
            let filename = format!("attachment; filename=\"diagnostics_{}.zip\"", session_id);
            let Ok(disposition) = HeaderValue::from_str(&filename) else {
                return StatusCode::BAD_REQUEST.into_response();
            };
            let response_headers = [
                (
                    http::header::CONTENT_TYPE,
                    HeaderValue::from_static("application/zip"),
                ),
                (http::header::CONTENT_DISPOSITION, disposition),
            ];

            (response_headers, Body::from(zip_data)).into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/status", get(status))
        .route("/system_info", get(system_info))
        .route("/diagnostics/{session_id}", get(diagnostics))
        .with_state(state)
}
