use crate::routes::errors::ErrorResponse;
use crate::routes::workflow_utils::{
    apply_workflow_to_agent, build_workflow_with_parameter_values,
};
use crate::state::AppState;
use axum::extract::{Query, State};
use axum::routing::post;
use axum::{
    extract::Path,
    http::StatusCode,
    routing::{delete, get, put},
    Json, Router,
};
use biorouter::agents::ExtensionConfig;
use biorouter::session::extension_data::ExtensionState;
use biorouter::session::session_manager::{ActivityWindow, ModelUsageRow, SessionInsights};
use biorouter::session::{EnabledExtensionsState, Session};
use biorouter::workflow::Workflow;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionListResponse {
    /// List of available session information objects
    sessions: Vec<Session>,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionNameRequest {
    /// Updated name for the session (max 200 characters)
    name: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionUserWorkflowValuesRequest {
    /// Workflow parameter values entered by the user
    user_workflow_values: HashMap<String, String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UpdateSessionUserWorkflowValuesResponse {
    workflow: Workflow,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportSessionRequest {
    json: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum EditType {
    Fork,
    Edit,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EditMessageRequest {
    timestamp: i64,
    #[serde(default = "default_edit_type")]
    edit_type: EditType,
}

fn default_edit_type() -> EditType {
    EditType::Fork
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EditMessageResponse {
    session_id: String,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DivergeSessionRequest {
    /// Optional name for the diverged session. When omitted or blank, the
    /// diverged session inherits the original session's name so it stays
    /// recognizable in the session list.
    #[serde(default)]
    name: Option<String>,
    /// Optional anchor: the `created` timestamp (ms) of the assistant message a
    /// per-message Diverge button was clicked on. The branch is trimmed to end
    /// at that answer. When omitted, the branch ends at the most recent
    /// complete assistant answer — an in-flight turn captured mid-generation is
    /// never carried over.
    #[serde(default)]
    truncate_after: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DivergeSessionResponse {
    /// The id of the newly-created diverged session.
    session_id: String,
    /// The working directory of the diverged session (identical to the
    /// original). Surfaced so the client can spawn the new window/backend in
    /// the correct directory.
    working_dir: String,
    /// The diverged session's resolved name (e.g. "Foo (branch 2)").
    name: String,
    /// The id of the session this one was diverged from (its lineage parent).
    diverged_from: Option<String>,
}

const MAX_NAME_LENGTH: usize = 200;

fn is_valid_session_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

#[utoipa::path(
    get,
    path = "/sessions",
    responses(
        (status = 200, description = "List of available sessions retrieved successfully", body = SessionListResponse),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn list_sessions(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SessionListResponse>, StatusCode> {
    let sessions = state
        .session_manager()
        .list_sessions()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(SessionListResponse { sessions }))
}

#[utoipa::path(
    get,
    path = "/sessions/{session_id}",
    params(
        ("session_id" = String, Path, description = "Unique identifier for the session")
    ),
    responses(
        (status = 200, description = "Session history retrieved successfully", body = Session),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 404, description = "Session not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<Session>, StatusCode> {
    if !is_valid_session_id(&session_id) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let session = state
        .session_manager()
        .get_session(&session_id, true)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok(Json(session))
}
#[utoipa::path(
    get,
    path = "/sessions/insights",
    responses(
        (status = 200, description = "Session insights retrieved successfully", body = SessionInsights),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn get_session_insights(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SessionInsights>, StatusCode> {
    let insights = state
        .session_manager()
        .get_insights()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(insights))
}

/// Query for `GET /sessions/activity`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ActivityQuery {
    /// How many calendar days back to report. Clamped to 1..=371 server-side.
    /// ~155 covers the five months the Home heatmap renders.
    #[serde(default = "default_activity_days")]
    pub days: i64,
}

fn default_activity_days() -> i64 {
    155
}

#[utoipa::path(
    get,
    path = "/sessions/activity",
    params(
        ("days" = Option<i64>, Query, description = "Calendar days to report (default 155, clamped to 1..=371)")
    ),
    responses(
        (status = 200, description = "Per-day usage for the Home heatmap", body = ActivityWindow),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn get_session_activity(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ActivityQuery>,
) -> Result<Json<ActivityWindow>, StatusCode> {
    let activity = state
        .session_manager()
        .get_activity(query.days)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(activity))
}

/// Per-model token breakdown for one session.
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionModelUsageResponse {
    /// One row per `(model, provider)` group; a `null` `modelId` is the
    /// "unknown" bucket (turns recorded before model attribution, or providers
    /// that reported no model).
    pub models: Vec<ModelUsageRow>,
}

#[utoipa::path(
    get,
    path = "/sessions/{session_id}/usage",
    params(
        ("session_id" = String, Path, description = "Unique identifier for the session")
    ),
    responses(
        (status = 200, description = "Per-model usage for the session", body = SessionModelUsageResponse),
        (status = 400, description = "Invalid session id"),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn get_session_usage(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionModelUsageResponse>, StatusCode> {
    if !is_valid_session_id(&session_id) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let models = state
        .session_manager()
        .get_session_model_usage(&session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(SessionModelUsageResponse { models }))
}

#[utoipa::path(
    put,
    path = "/sessions/{session_id}/name",
    request_body = UpdateSessionNameRequest,
    params(
        ("session_id" = String, Path, description = "Unique identifier for the session")
    ),
    responses(
        (status = 200, description = "Session name updated successfully"),
        (status = 400, description = "Bad request - Name too long (max 200 characters)"),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 404, description = "Session not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn update_session_name(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(request): Json<UpdateSessionNameRequest>,
) -> Result<StatusCode, StatusCode> {
    if !is_valid_session_id(&session_id) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let name = request.name.trim();
    if name.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if name.len() > MAX_NAME_LENGTH {
        return Err(StatusCode::BAD_REQUEST);
    }

    state
        .session_manager()
        .update(&session_id)
        .user_provided_name(name.to_string())
        .apply()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}

#[utoipa::path(
    put,
    path = "/sessions/{session_id}/user_workflow_values",
    request_body = UpdateSessionUserWorkflowValuesRequest,
    params(
        ("session_id" = String, Path, description = "Unique identifier for the session")
    ),
    responses(
        (status = 200, description = "Session user workflow values updated successfully", body = UpdateSessionUserWorkflowValuesResponse),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
// Update session user workflow parameter values
async fn update_session_user_workflow_values(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(request): Json<UpdateSessionUserWorkflowValuesRequest>,
) -> Result<Json<UpdateSessionUserWorkflowValuesResponse>, ErrorResponse> {
    if !is_valid_session_id(&session_id) {
        return Err(ErrorResponse {
            message: "Invalid session ID".to_string(),
            status: StatusCode::BAD_REQUEST,
        });
    }
    state
        .session_manager()
        .update(&session_id)
        .user_workflow_values(Some(request.user_workflow_values))
        .apply()
        .await
        .map_err(|err| ErrorResponse {
            message: err.to_string(),
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;

    let session = state
        .session_manager()
        .get_session(&session_id, false)
        .await
        .map_err(|err| ErrorResponse {
            message: err.to_string(),
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;
    let workflow = session.workflow.ok_or_else(|| ErrorResponse {
        message: "Workflow not found".to_string(),
        status: StatusCode::NOT_FOUND,
    })?;

    let user_workflow_values = session.user_workflow_values.unwrap_or_default();
    match build_workflow_with_parameter_values(&workflow, user_workflow_values).await {
        Ok(Some(workflow)) => {
            let agent = state
                .get_agent_for_route(session_id.clone())
                .await
                .map_err(|status| ErrorResponse {
                    message: format!("Failed to get agent: {}", status),
                    status,
                })?;
            if let Some(prompt) = apply_workflow_to_agent(&agent, &workflow, false).await {
                agent.extend_system_prompt(prompt).await;
            }
            Ok(Json(UpdateSessionUserWorkflowValuesResponse { workflow }))
        }
        Ok(None) => Err(ErrorResponse {
            message: "Missing required parameters".to_string(),
            status: StatusCode::BAD_REQUEST,
        }),
        Err(e) => Err(ErrorResponse {
            message: e.to_string(),
            status: StatusCode::INTERNAL_SERVER_ERROR,
        }),
    }
}

#[utoipa::path(
    delete,
    path = "/sessions/{session_id}",
    params(
        ("session_id" = String, Path, description = "Unique identifier for the session")
    ),
    responses(
        (status = 200, description = "Session deleted successfully"),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 404, description = "Session not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn delete_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    if !is_valid_session_id(&session_id) {
        return Err(StatusCode::BAD_REQUEST);
    }
    state
        .session_manager()
        .delete_session(&session_id)
        .await
        .map_err(|e| {
            if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;

    Ok(StatusCode::OK)
}

#[utoipa::path(
    get,
    path = "/sessions/{session_id}/export",
    params(
        ("session_id" = String, Path, description = "Unique identifier for the session")
    ),
    responses(
        (status = 200, description = "Session exported successfully", body = String),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 404, description = "Session not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn export_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<String>, StatusCode> {
    if !is_valid_session_id(&session_id) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let exported = state
        .session_manager()
        .export_session(&session_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok(Json(exported))
}

#[utoipa::path(
    post,
    path = "/sessions/import",
    request_body = ImportSessionRequest,
    responses(
        (status = 200, description = "Session imported successfully", body = Session),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 400, description = "Bad request - Invalid JSON"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn import_session(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ImportSessionRequest>,
) -> Result<Json<Session>, StatusCode> {
    let session = state
        .session_manager()
        .import_session(&request.json)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    Ok(Json(session))
}

#[utoipa::path(
    post,
    path = "/sessions/{session_id}/edit_message",
    request_body = EditMessageRequest,
    params(
        ("session_id" = String, Path, description = "Unique identifier for the session")
    ),
    responses(
        (status = 200, description = "Session prepared for editing - frontend should submit the edited message", body = EditMessageResponse),
        (status = 400, description = "Bad request - Invalid message timestamp"),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 404, description = "Session or message not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn edit_message(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(request): Json<EditMessageRequest>,
) -> Result<Json<EditMessageResponse>, StatusCode> {
    if !is_valid_session_id(&session_id) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let manager = state.session_manager();
    match request.edit_type {
        EditType::Fork => {
            let new_session = manager
                .copy_session(&session_id, "(edited)".to_string())
                .await
                .map_err(|e| {
                    tracing::error!("Failed to copy session: {}", e);
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;

            manager
                .truncate_conversation(&new_session.id, request.timestamp)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to truncate conversation: {}", e);
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;

            Ok(Json(EditMessageResponse {
                session_id: new_session.id,
            }))
        }
        EditType::Edit => {
            manager
                .truncate_conversation(&session_id, request.timestamp)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to truncate conversation: {}", e);
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;

            Ok(Json(EditMessageResponse {
                session_id: session_id.clone(),
            }))
        }
    }
}

/// Diverge (branch) a session into a brand-new session that inherits the full
/// conversation history and metadata of the original, while leaving the
/// original completely untouched. Unlike `edit_message` with `Fork`, no
/// truncation is performed — the entire history is carried over so the user can
/// continue a fresh conversation from exactly where they left off.
#[utoipa::path(
    post,
    path = "/sessions/{session_id}/diverge",
    request_body = DivergeSessionRequest,
    params(
        ("session_id" = String, Path, description = "Unique identifier for the session to diverge from")
    ),
    responses(
        (status = 200, description = "Session diverged successfully", body = DivergeSessionResponse),
        (status = 400, description = "Bad request - Invalid session id or name too long"),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 404, description = "Session not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn diverge_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(request): Json<DivergeSessionRequest>,
) -> Result<Json<DivergeSessionResponse>, StatusCode> {
    if !is_valid_session_id(&session_id) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let manager = state.session_manager();

    // Validate the optional custom name up front (blank → use the auto branch
    // name; too long → reject).
    if let Some(ref n) = request.name {
        if !n.trim().is_empty() && n.trim().chars().count() > MAX_NAME_LENGTH {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    // A missing source session is a clean 404 (rather than a 500 from the copy).
    manager
        .get_session(&session_id, false)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    // diverge_session does the placeholder-aware, sibling-numbered naming,
    // records the lineage pointer, and trims the branch to end at the last
    // complete assistant answer (anchored by `truncate_after` when the
    // per-message button supplies it).
    let new_session = manager
        .diverge_session(&session_id, request.name, request.truncate_after)
        .await
        .map_err(|e| {
            tracing::error!("Failed to diverge session {}: {}", session_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(DivergeSessionResponse {
        session_id: new_session.id,
        working_dir: new_session.working_dir.to_string_lossy().to_string(),
        name: new_session.name,
        diverged_from: new_session.diverged_from,
    }))
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionExtensionsResponse {
    extensions: Vec<ExtensionConfig>,
}

#[utoipa::path(
    get,
    path = "/sessions/{session_id}/extensions",
    params(
        ("session_id" = String, Path, description = "Unique identifier for the session")
    ),
    responses(
        (status = 200, description = "Session extensions retrieved successfully", body = SessionExtensionsResponse),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 404, description = "Session not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn get_session_extensions(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionExtensionsResponse>, StatusCode> {
    if !is_valid_session_id(&session_id) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let session = state
        .session_manager()
        .get_session(&session_id, false)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    // Try to get session-specific extensions, fall back to global config
    let extensions = EnabledExtensionsState::from_extension_data(&session.extension_data)
        .map(|state| state.extensions)
        .unwrap_or_else(biorouter::config::get_enabled_extensions);

    Ok(Json(SessionExtensionsResponse { extensions }))
}

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/sessions", get(list_sessions))
        .route("/sessions/{session_id}", get(get_session))
        .route("/sessions/{session_id}", delete(delete_session))
        .route("/sessions/{session_id}/export", get(export_session))
        .route("/sessions/import", post(import_session))
        .route("/sessions/insights", get(get_session_insights))
        .route("/sessions/activity", get(get_session_activity))
        // Static `/usage` suffix — distinct from the `/sessions/{session_id}`
        // wildcard, so axum routes it without a guard.
        .route("/sessions/{session_id}/usage", get(get_session_usage))
        .route("/sessions/{session_id}/name", put(update_session_name))
        .route(
            "/sessions/{session_id}/user_workflow_values",
            put(update_session_user_workflow_values),
        )
        .route("/sessions/{session_id}/edit_message", post(edit_message))
        .route("/sessions/{session_id}/diverge", post(diverge_session))
        .route(
            "/sessions/{session_id}/extensions",
            get(get_session_extensions),
        )
        .with_state(state)
}

#[cfg(test)]
mod diverge_tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use biorouter::conversation::message::Message;
    use biorouter::session::SessionType;
    use serial_test::serial;
    use std::path::PathBuf;
    use tower::ServiceExt;

    async fn post_diverge(
        state: Arc<AppState>,
        session_id: &str,
        body: serde_json::Value,
    ) -> (axum::http::StatusCode, serde_json::Value) {
        let app = routes(state);
        let req = Request::builder()
            .method("POST")
            .uri(format!("/sessions/{session_id}/diverge"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    fn user_msg(text: &str) -> Message {
        Message::user().with_text(text)
    }

    async fn get_activity(
        state: Arc<AppState>,
        query: &str,
    ) -> (axum::http::StatusCode, serde_json::Value) {
        let app = routes(state);
        let req = Request::builder()
            .method("GET")
            .uri(format!("/sessions/activity{query}"))
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    /// `/sessions/activity` must not be swallowed by the `/sessions/{session_id}`
    /// wildcard registered next to it, and the payload must be camelCase.
    ///
    /// NOTE: `AppState::new()` opens the REAL user session database, so a route
    /// test here must be READ-ONLY. The behaviour of the activity aggregation is
    /// covered by `session_manager`'s unit tests, which use a `TempDir`.
    #[tokio::test(flavor = "multi_thread")]
    #[serial]
    async fn activity_route_is_not_shadowed_by_the_session_id_wildcard() {
        let state = AppState::new().await.unwrap();
        let (status, body) = get_activity(state, "?days=30").await;
        assert_eq!(status, axum::http::StatusCode::OK);
        assert!(body.get("days").is_some(), "got {body}");
        assert!(body.get("currentStreak").is_some(), "camelCase payload");
        assert!(body.get("longestStreak").is_some());
        assert!(body.get("maxTokens").is_some());
    }

    async fn get_usage(
        state: Arc<AppState>,
        session_id: &str,
    ) -> (axum::http::StatusCode, serde_json::Value) {
        let app = routes(state);
        let req = Request::builder()
            .method("GET")
            .uri(format!("/sessions/{session_id}/usage"))
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    /// The `/usage` response serializes to the exact camelCase JSON the TS client
    /// expects: `models` array of `{modelId, provider, inputTokens, outputTokens,
    /// totalTokens, turns}`, with the unknown bucket carrying `null`.
    ///
    /// This is a pure serialization assertion rather than a live round-trip:
    /// `AppState::new()` opens the REAL shared session DB, whose token-ledger
    /// contents (and, across parallel worktrees, whose schema) are not stable
    /// enough for a write-then-read route test. The aggregation SQL itself is
    /// covered by `session_manager`'s `TempDir` unit tests.
    #[test]
    fn usage_response_serializes_to_camelcase_shape() {
        let response = SessionModelUsageResponse {
            models: vec![
                ModelUsageRow {
                    model_id: Some("gpt-5".to_string()),
                    provider: Some("openai".to_string()),
                    input_tokens: 300,
                    output_tokens: 70,
                    total_tokens: 370,
                    turns: 2,
                },
                ModelUsageRow {
                    model_id: None,
                    provider: None,
                    input_tokens: 1,
                    output_tokens: 2,
                    total_tokens: 3,
                    turns: 1,
                },
            ],
        };

        let json = serde_json::to_value(&response).unwrap();
        let models = json.get("models").and_then(|m| m.as_array()).unwrap();
        assert_eq!(models.len(), 2);

        let gpt = &models[0];
        assert_eq!(gpt.get("modelId").and_then(|v| v.as_str()), Some("gpt-5"));
        assert_eq!(gpt.get("provider").and_then(|v| v.as_str()), Some("openai"));
        assert_eq!(gpt.get("inputTokens").and_then(|v| v.as_i64()), Some(300));
        assert_eq!(gpt.get("outputTokens").and_then(|v| v.as_i64()), Some(70));
        assert_eq!(gpt.get("totalTokens").and_then(|v| v.as_i64()), Some(370));
        assert_eq!(gpt.get("turns").and_then(|v| v.as_i64()), Some(2));

        // The unknown bucket serializes model/provider as JSON null.
        let unknown = &models[1];
        assert!(unknown.get("modelId").unwrap().is_null());
        assert!(unknown.get("provider").unwrap().is_null());
        assert_eq!(unknown.get("totalTokens").and_then(|v| v.as_i64()), Some(3));
    }

    /// The `/usage` route rejects an invalid session id before touching the DB,
    /// and the `/usage` suffix is not swallowed by the `/sessions/{session_id}`
    /// wildcard registered next to it.
    #[tokio::test(flavor = "multi_thread")]
    #[serial]
    async fn usage_route_rejects_invalid_session_id() {
        let state = AppState::new().await.unwrap();
        let (status, _) = get_usage(state, "bad.id").await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    }

    /// `days` is attacker-controlled; the server clamps it rather than building a
    /// SQL modifier from an arbitrary integer.
    #[tokio::test(flavor = "multi_thread")]
    #[serial]
    async fn activity_clamps_an_absurd_window() {
        let state = AppState::new().await.unwrap();
        let (status, body) = get_activity(state, "?days=100000").await;
        assert_eq!(status, axum::http::StatusCode::OK);
        assert!(body.get("start").is_some());

        let state = AppState::new().await.unwrap();
        let (status, _) = get_activity(state, "?days=-5").await;
        assert_eq!(status, axum::http::StatusCode::OK);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[serial]
    async fn diverge_invalid_session_id_returns_400() {
        let state = AppState::new().await.unwrap();
        // Contains a '.', disallowed by is_valid_session_id but a valid URI path
        // segment — no DB touched.
        let (status, _) = post_diverge(state, "bad.id", serde_json::json!({})).await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[serial]
    async fn diverge_nonexistent_session_returns_404() {
        let state = AppState::new().await.unwrap();
        let (status, _) = post_diverge(state, "29990101_99999", serde_json::json!({})).await;
        assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[serial]
    async fn diverge_copies_full_history_and_keeps_original() {
        let state = AppState::new().await.unwrap();
        let manager = state.session_manager();

        let original = manager
            .create_session(
                PathBuf::from("/tmp/diverge_route_test"),
                "placeholder".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();
        // Give the original a name unique to this run (its own id) so the
        // sibling count is deterministic against the shared session store —
        // otherwise leftover "Route Original (branch N)" rows from prior runs
        // would bump the expected index.
        let base_name = format!("Route Original {}", original.id);
        manager
            .update(&original.id)
            .user_provided_name(base_name.clone())
            .apply()
            .await
            .unwrap();
        // A complete exchange (question + answer) so the branch — which is
        // trimmed to the last complete assistant answer — carries it over.
        manager
            .add_message(&original.id, &user_msg("hello"))
            .await
            .unwrap();
        manager
            .add_message(&original.id, &Message::assistant().with_text("hi there"))
            .await
            .unwrap();

        let expected_branch = format!("{base_name} (branch 1)");

        let (status, json) = post_diverge(state.clone(), &original.id, serde_json::json!({})).await;
        assert_eq!(status, axum::http::StatusCode::OK);

        let new_id = json["sessionId"].as_str().unwrap().to_string();
        assert_ne!(new_id, original.id);
        // Working dir surfaced and matches the original.
        assert_eq!(
            json["workingDir"].as_str().unwrap(),
            "/tmp/diverge_route_test"
        );
        // Response carries the branch name and lineage pointer.
        assert_eq!(json["name"].as_str().unwrap(), expected_branch);
        assert_eq!(json["divergedFrom"].as_str().unwrap(), original.id);

        // Branch has the full history; sibling-numbered branch name; lineage set.
        let branch = manager.get_session(&new_id, true).await.unwrap();
        assert_eq!(branch.message_count, 2);
        assert_eq!(branch.name, expected_branch);
        assert_eq!(branch.diverged_from.as_deref(), Some(original.id.as_str()));

        // Original is untouched (no lineage, history intact).
        let orig_after = manager.get_session(&original.id, true).await.unwrap();
        assert_eq!(orig_after.message_count, 2);
        assert_eq!(orig_after.diverged_from, None);

        // Cleanup so we don't pollute the shared session store.
        manager.delete_session(&new_id).await.unwrap();
        manager.delete_session(&original.id).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    #[serial]
    async fn diverge_with_custom_name_and_too_long_name() {
        let state = AppState::new().await.unwrap();
        let manager = state.session_manager();

        let original = manager
            .create_session(
                PathBuf::from("/tmp/diverge_route_name"),
                "Orig".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();

        // Custom name applied.
        let (status, json) = post_diverge(
            state.clone(),
            &original.id,
            serde_json::json!({ "name": "My Branch" }),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let new_id = json["sessionId"].as_str().unwrap().to_string();
        let branch = manager.get_session(&new_id, false).await.unwrap();
        assert_eq!(branch.name, "My Branch");

        // Name exceeding the 200-char cap is rejected.
        let long_name = "x".repeat(201);
        let (status, _) = post_diverge(
            state.clone(),
            &original.id,
            serde_json::json!({ "name": long_name }),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);

        manager.delete_session(&new_id).await.unwrap();
        manager.delete_session(&original.id).await.unwrap();
    }
}
