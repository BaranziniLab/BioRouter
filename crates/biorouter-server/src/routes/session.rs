use crate::routes::errors::ErrorResponse;
use crate::routes::workflow_utils::{
    apply_workflow_to_agent, build_workflow_with_parameter_values,
};
use crate::state::AppState;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{
    extract::Path,
    http::StatusCode,
    routing::{delete, get, put},
    Json, Router,
};
use biorouter::agents::ExtensionConfig;
use biorouter::conversation::message::Message;
use biorouter::session::extension_data::ExtensionState;
use biorouter::session::session_manager::{
    ActivityWindow, ModelUsageRow, SessionInsights, TruncateOutcome,
};
use biorouter::session::{EnabledExtensionsState, Session, SessionSummary, SessionType};
use biorouter::workflow::Workflow;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionListResponse {
    /// List of available session information objects
    sessions: Vec<Session>,
}

const DEFAULT_SIDEBAR_SESSION_LIMIT: u32 = 10;
const MAX_SIDEBAR_SESSION_LIMIT: u32 = 50;

#[derive(Debug, Deserialize, ToSchema)]
pub struct SidebarSessionsQuery {
    #[serde(default = "default_sidebar_session_limit")]
    limit: u32,
    #[serde(default)]
    offset: u32,
    /// BR-71: include `sub_agent` sessions (grouped under `parent_session_id`).
    #[serde(default)]
    include_subagents: bool,
}

/// Query parameters for `GET /sessions`.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ListSessionsQuery {
    /// BR-71: include `sub_agent` sessions (grouped under `parent_session_id`).
    #[serde(default)]
    pub include_subagents: bool,
}

fn default_sidebar_session_limit() -> u32 {
    DEFAULT_SIDEBAR_SESSION_LIMIT
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SidebarSessionListResponse {
    sessions: Vec<SessionSummary>,
    has_more: bool,
    next_offset: Option<u32>,
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
    #[serde(alias = "fork")]
    Diverge,
    Edit,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EditMessageRequest {
    timestamp: i64,
    #[serde(default = "default_edit_type")]
    edit_type: EditType,
    /// Your view of this session: the durable ids (`Message.id`) of every
    /// message it holds, in any order.
    ///
    /// OPTIONAL, and enforced when you send it. For `edit`, which truncates the
    /// live session, supplying it is how the server checks that you are deleting
    /// the history you actually saw: a stored message your list does not name
    /// landed after you rendered the conversation, and the cut is refused with
    /// 409 `conversation_out_of_date` rather than destroying it. Omit it and the
    /// cut still runs under the turn lock and is still bounded to the rows the
    /// server itself just read — safe, but blind to that wider race. Ignored for
    /// `diverge`, which writes only to a new session and cannot destroy anything.
    ///
    /// An EMPTY list is not the same as omitting the field: it asserts your view
    /// holds nothing, and is checked as such (so a non-empty session refuses it).
    //
    // The same precondition `/reply`'s `conversation_so_far` carries (#51 W5),
    // reduced to what a truncation needs: ids alone prove the client has seen
    // everything stored, and unlike that field this one supplies no content.
    //
    // Optional, and still optional now that #59 has landed. The reply stream
    // DOES publish the ids a turn persisted (`MessagesPersisted`), so a client
    // can now be complete — but only one that consumes that frame, and no
    // shipped client does yet: the desktop claims completeness solely for a
    // session it has just re-read from the store. A REQUIRED field would
    // therefore still 409 every in-place edit in every live chat. See
    // `edit_in_place`.
    #[serde(default)]
    expected_message_ids: Option<Vec<String>>,
}

fn default_edit_type() -> EditType {
    EditType::Diverge
}

/// Ids of messages `stored` holds that `client_view` does not name, in stored
/// order.
///
/// Message ids are durable and server-assigned (BR-45/#41), so a client naming
/// one is proof it has seen it. Anything stored that the view does not name is a
/// message that landed after the client rendered the conversation — and an
/// open-above cut would delete it, after its writer was already told the append
/// succeeded.
///
/// The twin of `reply::unacknowledged_stored_ids`, which answers the same
/// question for `conversation_so_far`; that one compares against whole messages
/// because the client is also supplying content. Keep the two in step.
fn unacknowledged_stored_ids(stored: &[Message], client_view: &[String]) -> Vec<String> {
    let acknowledged: HashSet<&str> = client_view.iter().map(String::as_str).collect();
    stored
        .iter()
        .filter_map(|message| message.id.as_deref())
        .filter(|id| !acknowledged.contains(id))
        .map(str::to_string)
        .collect()
}

/// The 409 a refused truncation answers with, in the shape `/reply` already uses
/// for a refused write-back: `code` names the condition and `missing_message_ids`
/// names exactly what the cut would have destroyed, so the client can re-read the
/// session and retry rather than guess.
fn edit_conflict_response(missing: Vec<String>, stored_message_count: usize) -> Response {
    (
        StatusCode::CONFLICT,
        Json(serde_json::json!({
            "type": "Error",
            "error": "This session has messages your view does not contain; nothing was \
                      deleted. Re-read the session and retry.",
            "code": "conversation_out_of_date",
            "missing_message_ids": missing,
            "stored_message_count": stored_message_count,
        })),
    )
        .into_response()
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
    /// Optional anchor by durable message id (`Message.id`), the BR-45 divergence
    /// point. Preferred over `truncate_after`: it is unambiguous when two
    /// messages share a whole second and it records the branch's divergence point. It
    /// takes precedence when both are supplied; `truncate_after` stays for
    /// back-compatibility with older clients.
    #[serde(default)]
    truncate_after_id: Option<String>,
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
    params(
        ("include_subagents" = Option<bool>, Query, description = "Include sub_agent sessions (grouped under parent_session_id); default false")
    ),
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
    Query(query): Query<ListSessionsQuery>,
) -> Result<Json<SessionListResponse>, StatusCode> {
    let types: &[SessionType] = if query.include_subagents {
        &[
            SessionType::User,
            SessionType::Scheduled,
            SessionType::SubAgent,
        ]
    } else {
        &[SessionType::User, SessionType::Scheduled]
    };
    let sessions = state
        .session_manager()
        .list_sessions_by_types(types)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(SessionListResponse { sessions }))
}

#[utoipa::path(
    get,
    path = "/sessions/sidebar",
    params(
        ("limit" = Option<u32>, Query, description = "Session summaries per page (default 10, clamped to 1..=50)"),
        ("offset" = Option<u32>, Query, description = "Number of session summaries to skip"),
        ("include_subagents" = Option<bool>, Query, description = "Include sub_agent sessions (grouped under parent_session_id); default false")
    ),
    responses(
        (status = 200, description = "Paginated lightweight session summaries for the sidebar", body = SidebarSessionListResponse),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Session Management"
)]
async fn list_sidebar_sessions(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SidebarSessionsQuery>,
) -> Result<Json<SidebarSessionListResponse>, StatusCode> {
    let limit = query.limit.clamp(1, MAX_SIDEBAR_SESSION_LIMIT);
    let mut sessions = state
        .session_manager()
        .list_session_summaries(
            limit.saturating_add(1),
            query.offset,
            query.include_subagents,
            false,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let has_more = sessions.len() > limit as usize;
    sessions.truncate(limit as usize);
    let next_offset = has_more.then(|| query.offset.saturating_add(limit));

    Ok(Json(SidebarSessionListResponse {
        sessions,
        has_more,
        next_offset,
    }))
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
        (status = 404, description = "Session not found"),
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
        .map_err(|error| {
            if error.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;
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

/// Prepare a session for an edited message.
///
/// `diverge` (the default) copies the history into a NEW session, trimmed at the
/// edited message, and returns that session's id. The original is untouched.
///
/// `edit` truncates THIS session in place, dropping every message from
/// `timestamp` onwards. Because that destroys history a concurrent writer may
/// already have been told was saved, the cut is refused — with nothing deleted —
/// while a turn is in flight, and it never reaches past the rows the server read
/// when it took the request. You may additionally send `expectedMessageIds`, the
/// ids of every message your view of the session holds: the cut is then also
/// refused if the session holds a message your view does not name, and the 409
/// body's `missing_message_ids` says which, so you can re-read and retry.
#[utoipa::path(
    post,
    path = "/sessions/{session_id}/edit_message",
    request_body = EditMessageRequest,
    params(
        ("session_id" = String, Path, description = "Unique identifier for the session")
    ),
    responses(
        (status = 200, description = "Session prepared for editing - frontend should submit the edited message", body = EditMessageResponse),
        (status = 400, description = "Bad request - invalid session id"),
        (status = 401, description = "Unauthorized - Invalid or missing API key"),
        (status = 404, description = "Session or message not found"),
        (status = 409, description = "A turn is in flight for this session, or a supplied \
                                      `expectedMessageIds` is missing messages the server holds \
                                      (nothing was deleted; re-read the session and retry)"),
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
) -> Response {
    if !is_valid_session_id(&session_id) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let manager = state.session_manager();
    match request.edit_type {
        EditType::Diverge => {
            match manager
                .diverge_session_for_edit(&session_id, request.timestamp)
                .await
            {
                Ok(new_session) => Json(EditMessageResponse {
                    session_id: new_session.id,
                })
                .into_response(),
                Err(e) => {
                    tracing::error!("Failed to diverge session for message edit: {}", e);
                    StatusCode::INTERNAL_SERVER_ERROR.into_response()
                }
            }
        }
        EditType::Edit => edit_in_place(&state, &session_id, &request).await,
    }
}

/// The destructive half of [`edit_message`]: truncate the LIVE session.
///
/// #51 NF-D. This used to delete an arbitrary tail from a client-supplied
/// timestamp with no freshness check, no expected state and no turn lock —
/// strictly more dangerous than the `/reply` write-back hardened alongside it,
/// which at least carried the client's own copy for the server to compare
/// against. It now takes the same three guards, in the same order, answering
/// with the same codes:
///
/// 1. the BR-33 per-session turn lock, so a cut cannot land in a session an
///    agent is generating into — `/reply` refuses a second writer with 409 and
///    so does this;
/// 2. the client's view of the conversation (`expected_message_ids`) **when it
///    is supplied**, refused with 409 + `missing_message_ids` when the store
///    holds a message that view never saw — the WIDE race, between the client
///    rendering the conversation and the user clicking edit;
/// 3. [`biorouter::session::session_manager::SessionManager::truncate_conversation_bounded`]
///    rather than the open-above `truncate_conversation`, so a message landing
///    between the check above and the DELETE — the NARROW race, which no
///    client-side check can close — is kept rather than destroyed, and a basis
///    from a previous incarnation of a recycled session id is refused outright.
///
/// Guard 2 is the only one that needs the client's cooperation, and it is
/// therefore the only optional one. It landed REQUIRED, which was wrong: at the
/// time the reply stream never published the ids messages are persisted under,
/// so a client that had watched a live turn could not name them — a required
/// field would 409 every in-place edit in every live chat, which is a regression
/// against the unguarded endpoint this replaced.
///
/// #59 has since made the stream publish them (`MessagesPersisted`), so a
/// complete client view is now ACHIEVABLE. It is not yet ACHIEVED: a client only
/// has one if it consumes that frame, and none does — the desktop supplies the
/// field solely for a session it has just re-read from the store. Making this
/// field required again is the last step of #59, not a precondition of it, and
/// the log below is what will show when that step is safe to take.
/// Sent, it is enforced
/// exactly as strictly as before; omitted, the cut falls back to guards 1 and 3,
/// which need nothing from the client and are still strictly more than the
/// nothing that shipped before this. The gap is logged at `warn`, per cut, so it
/// shows up in an operator's log and not only in a design document.
///
/// An EMPTY `expected_message_ids` is *not* omission: it is a client asserting
/// its view holds nothing, so it goes down the checked path and a non-empty
/// session refuses it. Folding the two together would hand every client a
/// one-token opt-out of guard 2 and quietly delete the check.
async fn edit_in_place(
    state: &Arc<AppState>,
    session_id: &str,
    request: &EditMessageRequest,
) -> Response {
    // (1) Hold the BR-33 turn lock for the whole check-and-cut, so a `/reply`
    // cannot start against the rows being deleted. Named binding: `let _ =`
    // would drop the guard immediately and hold nothing.
    let _turn_guard = match state.try_begin_turn_idempotent(
        session_id,
        CancellationToken::new(),
        // No idempotency key: an edit is not a turn a client retries under a
        // name, so every conflict here is a genuine "someone else has the
        // session", never a `duplicate: true`.
        None,
    ) {
        Ok(guard) => guard,
        Err(conflict) => {
            tracing::warn!(
                "Refused an in-place edit of session {}: turn {} is in flight",
                session_id,
                conflict.running_turn_id
            );
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "type": "Error",
                    "error": "A turn is in progress for this session; nothing was deleted. \
                              Stop it and retry.",
                    "code": "turn_in_flight",
                    "running_turn_id": conflict.running_turn_id,
                })),
            )
                .into_response();
        }
    };

    // Revision first, then conversation (see `snapshot_for_rewrite`): a message
    // landing between the two reads is then inside the snapshot rather than
    // looking foreign. Taken before the client-view check because `basis` bounds
    // the cut in BOTH cases — it is the server's own view, and the fallback when
    // there is no client one.
    let (session, basis) = match state
        .session_manager()
        .snapshot_for_rewrite(session_id)
        .await
    {
        Ok(snapshot) => snapshot,
        Err(e) => {
            let status = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                tracing::error!("Failed to snapshot session {} for edit: {}", session_id, e);
                StatusCode::INTERNAL_SERVER_ERROR
            };
            return status.into_response();
        }
    };
    let stored = session.conversation.unwrap_or_default();

    // (2) The wide race: anything stored the client never saw would be deleted
    // by a cut it planned without knowing about it. Checkable only against a
    // view the client supplied; `Some(&[])` is such a view (it claims to hold
    // nothing) and is checked, `None` is the absence of one.
    match request.expected_message_ids.as_deref() {
        Some(client_view) => {
            let missing = unacknowledged_stored_ids(stored.messages(), client_view);
            if !missing.is_empty() {
                tracing::warn!(
                    "Refused an in-place edit of session {}: {} stored message(s) the client's \
                     view does not contain",
                    session_id,
                    missing.len()
                );
                return edit_conflict_response(missing, stored.messages().len());
            }
        }
        None => {
            tracing::warn!(
                "In-place edit of session {} carries no `expectedMessageIds`, so the wide race \
                 (a message appended between the client rendering the conversation and this \
                 request) is unchecked; the cut is still under the turn lock and bounded to the \
                 {} message(s) the server just read. The reply stream now publishes the ids a \
                 turn persisted (issue #59, `MessagesPersisted`), so a client that consumes that \
                 frame can supply a view — this one did not.",
                session_id,
                stored.messages().len()
            );
        }
    }

    // (3) The narrow race: `basis` bounds the delete to the rows this handler
    // actually read, inside the truncation's own transaction.
    match state
        .session_manager()
        .truncate_conversation_bounded(session_id, request.timestamp, basis)
        .await
    {
        Ok(TruncateOutcome::Truncated { .. }) => Json(EditMessageResponse {
            session_id: session_id.to_string(),
        })
        .into_response(),
        // The session was wiped and recreated under this id between the snapshot
        // and the cut, so the basis describes a conversation that no longer
        // exists. Same answer as a stale view — refuse, delete nothing.
        Ok(TruncateOutcome::Stale) => {
            tracing::warn!(
                "Refused an in-place edit of session {}: the conversation moved under the check",
                session_id
            );
            edit_conflict_response(Vec::new(), stored.messages().len())
        }
        Ok(TruncateOutcome::SessionNotFound) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("Failed to truncate conversation: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Diverge (branch) a session into a brand-new session that inherits the full
/// conversation history and metadata of the original, while leaving the
/// original completely untouched. Unlike `edit_message` with `Diverge`, no
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
    // records the lineage pointer + divergence point, and trims the branch to end at
    // the last complete assistant answer. Anchored by the durable message id
    // (`truncate_after_id`) when supplied — unambiguous even for two same-second
    // messages — else by the legacy `truncate_after` timestamp.
    let new_session = manager
        .diverge_session_at(
            &session_id,
            request.name,
            request.truncate_after,
            request.truncate_after_id,
        )
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
        .route("/sessions/sidebar", get(list_sidebar_sessions))
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

    #[test]
    fn edit_message_uses_diverge_as_the_canonical_action() {
        let request: EditMessageRequest =
            serde_json::from_value(serde_json::json!({ "timestamp": 1, "editType": "diverge" }))
                .unwrap();
        assert!(matches!(request.edit_type, EditType::Diverge));

        let legacy: EditMessageRequest =
            serde_json::from_value(serde_json::json!({ "timestamp": 1, "editType": "fork" }))
                .unwrap();
        assert!(matches!(legacy.edit_type, EditType::Diverge));
        assert!(matches!(default_edit_type(), EditType::Diverge));
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

    async fn get_sidebar_sessions(
        state: Arc<AppState>,
        query: &str,
    ) -> (axum::http::StatusCode, serde_json::Value) {
        let app = routes(state);
        let req = Request::builder()
            .method("GET")
            .uri(format!("/sessions/sidebar{query}"))
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

    #[tokio::test(flavor = "multi_thread")]
    #[serial]
    async fn sidebar_route_returns_paginated_lightweight_sessions() {
        let state = AppState::new().await.unwrap();
        let (status, body) = get_sidebar_sessions(state, "?limit=2&offset=0").await;
        assert_eq!(status, axum::http::StatusCode::OK);

        let sessions = body
            .get("sessions")
            .and_then(|sessions| sessions.as_array())
            .expect("sessions array");
        assert!(sessions.len() <= 2);
        assert!(body.get("has_more").is_some());
        assert!(body.get("next_offset").is_some());

        if let Some(session) = sessions.first().and_then(|session| session.as_object()) {
            for field in [
                "id",
                "working_dir",
                "name",
                "created_at",
                "updated_at",
                "message_count",
            ] {
                assert!(
                    session.contains_key(field),
                    "missing {field} in {session:?}"
                );
            }
            assert!(!session.contains_key("conversation"));
            assert!(!session.contains_key("extension_data"));
            assert!(!session.contains_key("workflow"));
        }
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
                    total_tokens: Some(370),
                    cache_read_tokens: Some(800),
                    cache_creation_tokens: Some(0),
                    turns: 2,
                },
                ModelUsageRow {
                    model_id: None,
                    provider: None,
                    input_tokens: 1,
                    output_tokens: 2,
                    total_tokens: Some(3),
                    cache_read_tokens: Some(0),
                    cache_creation_tokens: Some(0),
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
        assert_eq!(
            gpt.get("cacheReadTokens").and_then(|v| v.as_i64()),
            Some(800)
        );

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

    #[tokio::test(flavor = "multi_thread")]
    #[serial]
    async fn usage_route_returns_not_found_for_missing_session() {
        let state = AppState::new().await.unwrap();
        let (status, _) = get_usage(state, "29990101_99999").await;
        assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
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

/// #51 NF-D: `edit_message` with `EditType::Edit` deletes an arbitrary tail of a
/// LIVE session. These pin the preconditions that make that safe — the same
/// discipline `/reply` applies to `conversation_so_far`.
///
/// NOTE: `AppState::new()` opens the REAL user session database, so each test
/// creates its own session and deletes it again, exactly like `diverge_tests`.
#[cfg(test)]
mod edit_message_tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use biorouter::conversation::message::Message;
    use biorouter::session::SessionType;
    use serial_test::serial;
    use std::path::PathBuf;
    use tokio_util::sync::CancellationToken;
    use tower::ServiceExt;

    async fn post_edit(
        state: Arc<AppState>,
        session_id: &str,
        body: serde_json::Value,
    ) -> (axum::http::StatusCode, serde_json::Value) {
        let app = routes(state);
        let req = Request::builder()
            .method("POST")
            .uri(format!("/sessions/{session_id}/edit_message"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    fn user_at(ts: i64, text: &str) -> Message {
        let mut message = Message::user().with_text(text);
        message.created = ts;
        message
    }

    fn assistant_at(ts: i64, text: &str) -> Message {
        let mut message = Message::assistant().with_text(text);
        message.created = ts;
        message
    }

    /// A session holding `u1@1000, a1@1010, u2@1020`, returning its id plus the
    /// three server-assigned message uids in stored order.
    async fn seeded_session(state: &Arc<AppState>, dir: &str) -> (String, Vec<String>) {
        let manager = state.session_manager();
        let session = manager
            .create_session(
                PathBuf::from(dir),
                "placeholder".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();
        let mut ids = Vec::new();
        for message in [
            user_at(1_000, "first question"),
            assistant_at(1_010, "first answer"),
            user_at(1_020, "a message the client never saw"),
        ] {
            ids.push(manager.add_message(&session.id, &message).await.unwrap());
        }
        (session.id, ids)
    }

    async fn message_count(state: &Arc<AppState>, session_id: &str) -> usize {
        state
            .session_manager()
            .get_session(session_id, true)
            .await
            .unwrap()
            .conversation
            .map(|conversation| conversation.messages().len())
            .unwrap_or(0)
    }

    /// THE NF-D CASE. The client asks to cut from `1010`, naming only the two
    /// messages it has seen. A third message landed since — an append that was
    /// already acknowledged to whoever wrote it — and the open-above cut would
    /// take it too. The request must be refused with 409 and NOTHING deleted,
    /// exactly as a stale `conversation_so_far` is.
    #[tokio::test(flavor = "multi_thread")]
    #[serial]
    async fn edit_refuses_a_client_view_missing_a_stored_message() {
        let state = AppState::new().await.unwrap();
        let (session_id, ids) = seeded_session(&state, "/tmp/edit_route_stale").await;

        let (status, body) = post_edit(
            state.clone(),
            &session_id,
            serde_json::json!({
                "timestamp": 1_010,
                "editType": "edit",
                // Only the first two — the client never saw `ids[2]`.
                "expectedMessageIds": [ids[0], ids[1]],
            }),
        )
        .await;

        assert_eq!(
            status,
            axum::http::StatusCode::CONFLICT,
            "a stale client view must be refused; got {body}"
        );
        assert_eq!(body["code"], "conversation_out_of_date");
        assert_eq!(
            body["missing_message_ids"],
            serde_json::json!([ids[2]]),
            "the 409 must name what the cut would have destroyed"
        );
        assert_eq!(body["stored_message_count"], serde_json::json!(3));

        assert_eq!(
            message_count(&state, &session_id).await,
            3,
            "a refused edit must write nothing"
        );

        state
            .session_manager()
            .delete_session(&session_id)
            .await
            .unwrap();
    }

    /// The other half of the guard: a client that HAS seen everything still gets
    /// its edit. Without this, "return 409" degenerates into "reject everyone".
    #[tokio::test(flavor = "multi_thread")]
    #[serial]
    async fn edit_accepts_a_client_view_that_has_seen_every_stored_message() {
        let state = AppState::new().await.unwrap();
        let (session_id, ids) = seeded_session(&state, "/tmp/edit_route_fresh").await;

        let (status, body) = post_edit(
            state.clone(),
            &session_id,
            serde_json::json!({
                "timestamp": 1_010,
                "editType": "edit",
                "expectedMessageIds": ids,
            }),
        )
        .await;

        assert_eq!(status, axum::http::StatusCode::OK, "got {body}");
        assert_eq!(body["sessionId"], serde_json::json!(session_id));
        assert_eq!(
            message_count(&state, &session_id).await,
            1,
            "the tail from 1010 onwards is cut"
        );
        // The edit takes the BR-33 turn lock; leaking it would soft-lock the
        // session — every later `/reply` would 409 forever.
        assert!(
            !state.is_turn_active(&session_id),
            "the edit must release the turn lock it took"
        );

        state
            .session_manager()
            .delete_session(&session_id)
            .await
            .unwrap();
    }

    /// An `Edit` that supplies no view is NOT refused. The field landed
    /// required, and at the time no client could satisfy it — the reply stream
    /// did not publish the ids messages are persisted under — so requiring it
    /// broke "Edit in Place" in every live chat, which is a regression against
    /// the unguarded endpoint it replaced. #59 has since made the stream publish
    /// them, but a client only benefits if it consumes that frame, so the
    /// no-view path this covers is still the one every live chat takes and this
    /// test still guards a reachable case, not a historical one.
    /// Omitting it now falls back to the two
    /// guards that need nothing from the client: the turn lock (covered by
    /// `edit_without_an_expected_view_still_refuses_while_a_turn_is_in_flight`)
    /// and the bounded cut against the server's own snapshot.
    #[tokio::test(flavor = "multi_thread")]
    #[serial]
    async fn edit_without_an_expected_view_still_cuts() {
        let state = AppState::new().await.unwrap();
        let (session_id, _) = seeded_session(&state, "/tmp/edit_route_unchecked").await;

        let (status, body) = post_edit(
            state.clone(),
            &session_id,
            serde_json::json!({ "timestamp": 1_010, "editType": "edit" }),
        )
        .await;

        assert_eq!(
            status,
            axum::http::StatusCode::OK,
            "an edit with no client view must proceed under the remaining guards; got {body}"
        );
        assert_eq!(body["sessionId"], serde_json::json!(session_id));
        assert_eq!(
            message_count(&state, &session_id).await,
            1,
            "the tail from 1010 onwards is cut"
        );
        assert!(
            !state.is_turn_active(&session_id),
            "the edit must release the turn lock it took"
        );

        state
            .session_manager()
            .delete_session(&session_id)
            .await
            .unwrap();
    }

    /// `null` is how a JSON client spells "not sending this", so it must land on
    /// the same fallback as omitting the key entirely — not on the checked path
    /// with an empty view.
    #[tokio::test(flavor = "multi_thread")]
    #[serial]
    async fn edit_with_a_null_expected_view_is_the_same_as_omitting_it() {
        let state = AppState::new().await.unwrap();
        let (session_id, _) = seeded_session(&state, "/tmp/edit_route_null_view").await;

        let (status, body) = post_edit(
            state.clone(),
            &session_id,
            serde_json::json!({
                "timestamp": 1_010,
                "editType": "edit",
                "expectedMessageIds": serde_json::Value::Null,
            }),
        )
        .await;

        assert_eq!(status, axum::http::StatusCode::OK, "got {body}");
        assert_eq!(message_count(&state, &session_id).await, 1);

        state
            .session_manager()
            .delete_session(&session_id)
            .await
            .unwrap();
    }

    /// THE EMPTY-VS-ABSENT DECISION. `[]` is a client asserting its view holds
    /// NOTHING — a claim, not the absence of one — so it goes down the checked
    /// path and a session with messages refuses it, naming every one of them.
    /// Folding `[]` into "absent" would hand any caller a one-token opt-out of
    /// the client-view guard, which would delete the guard rather than make it
    /// optional.
    #[tokio::test(flavor = "multi_thread")]
    #[serial]
    async fn edit_with_an_empty_expected_view_is_checked_not_waived() {
        let state = AppState::new().await.unwrap();
        let (session_id, ids) = seeded_session(&state, "/tmp/edit_route_empty_view").await;

        let (status, body) = post_edit(
            state.clone(),
            &session_id,
            serde_json::json!({
                "timestamp": 1_010,
                "editType": "edit",
                "expectedMessageIds": [],
            }),
        )
        .await;

        assert_eq!(
            status,
            axum::http::StatusCode::CONFLICT,
            "an empty view of a non-empty session is a stale view; got {body}"
        );
        assert_eq!(body["code"], "conversation_out_of_date");
        assert_eq!(
            body["missing_message_ids"],
            serde_json::json!(ids),
            "every stored message is one the view does not name"
        );
        assert_eq!(
            message_count(&state, &session_id).await,
            3,
            "a refused edit must write nothing"
        );

        state
            .session_manager()
            .delete_session(&session_id)
            .await
            .unwrap();
    }

    /// The fallback is a fallback, not a bypass: dropping the client view does
    /// not drop the turn lock, which is the guard that stops a cut landing in a
    /// session an agent is generating into.
    #[tokio::test(flavor = "multi_thread")]
    #[serial]
    async fn edit_without_an_expected_view_still_refuses_while_a_turn_is_in_flight() {
        let state = AppState::new().await.unwrap();
        let (session_id, _) = seeded_session(&state, "/tmp/edit_route_unchecked_live").await;

        let guard = state
            .try_begin_turn_idempotent(&session_id, CancellationToken::new(), None)
            .expect("no turn should be running for a session we just created");

        let (status, body) = post_edit(
            state.clone(),
            &session_id,
            serde_json::json!({ "timestamp": 1_010, "editType": "edit" }),
        )
        .await;

        assert_eq!(
            status,
            axum::http::StatusCode::CONFLICT,
            "a live turn must block an unchecked cut too; got {body}"
        );
        assert_eq!(body["code"], "turn_in_flight");
        assert_eq!(
            message_count(&state, &session_id).await,
            3,
            "a refused edit must write nothing"
        );

        drop(guard);

        state
            .session_manager()
            .delete_session(&session_id)
            .await
            .unwrap();
    }

    /// Truncating the tail of a session an agent is actively generating into
    /// deletes rows out from under the running turn. `/reply` already refuses a
    /// second writer with 409; the destructive edit takes the SAME lock.
    #[tokio::test(flavor = "multi_thread")]
    #[serial]
    async fn edit_refuses_while_a_turn_is_in_flight() {
        let state = AppState::new().await.unwrap();
        let (session_id, ids) = seeded_session(&state, "/tmp/edit_route_live_turn").await;

        let guard = state
            .try_begin_turn_idempotent(&session_id, CancellationToken::new(), None)
            .expect("no turn should be running for a session we just created");

        let (status, body) = post_edit(
            state.clone(),
            &session_id,
            serde_json::json!({
                "timestamp": 1_010,
                "editType": "edit",
                "expectedMessageIds": ids,
            }),
        )
        .await;

        assert_eq!(
            status,
            axum::http::StatusCode::CONFLICT,
            "a live turn must block the cut; got {body}"
        );
        assert_eq!(body["code"], "turn_in_flight");
        assert!(
            body["running_turn_id"].is_string(),
            "name the turn that holds the session: {body}"
        );
        assert_eq!(
            message_count(&state, &session_id).await,
            3,
            "a refused edit must write nothing"
        );

        drop(guard);

        // Releasing the turn releases the edit: the 409 is about the live turn,
        // not a permanent refusal.
        let (status, body) = post_edit(
            state.clone(),
            &session_id,
            serde_json::json!({
                "timestamp": 1_010,
                "editType": "edit",
                "expectedMessageIds": ids,
            }),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK, "got {body}");
        assert_eq!(message_count(&state, &session_id).await, 1);

        state
            .session_manager()
            .delete_session(&session_id)
            .await
            .unwrap();
    }

    /// The hardening is scoped to the DESTRUCTIVE path. `Diverge` copies into a
    /// brand-new session and never writes to the live one, so it keeps working
    /// with no client view and with a turn in flight — which is what keeps the
    /// default, and the button the desktop app actually uses most, unbroken.
    #[tokio::test(flavor = "multi_thread")]
    #[serial]
    async fn diverge_is_not_subject_to_the_edit_preconditions() {
        let state = AppState::new().await.unwrap();
        let (session_id, _) = seeded_session(&state, "/tmp/edit_route_diverge").await;

        let _guard = state
            .try_begin_turn_idempotent(&session_id, CancellationToken::new(), None)
            .expect("no turn should be running for a session we just created");

        let (status, body) = post_edit(
            state.clone(),
            &session_id,
            serde_json::json!({ "timestamp": 1_010, "editType": "diverge" }),
        )
        .await;

        assert_eq!(status, axum::http::StatusCode::OK, "got {body}");
        let branch_id = body["sessionId"].as_str().unwrap().to_string();
        assert_ne!(branch_id, session_id);
        assert_eq!(
            message_count(&state, &session_id).await,
            3,
            "diverge leaves the original untouched"
        );

        let manager = state.session_manager();
        manager.delete_session(&branch_id).await.unwrap();
        manager.delete_session(&session_id).await.unwrap();
    }

    /// The field is optional on the wire (so `Diverge` bodies stay valid, and so
    /// an `Edit` from a client that cannot name its view still parses) and
    /// camelCase, matching every other request type in this module.
    ///
    /// The three states must stay distinct at the type level, because the
    /// handler treats `Some([])` (a view that holds nothing — checked) and
    /// `None` (no view — unchecked) differently.
    #[test]
    fn expected_message_ids_deserializes_from_camel_case() {
        let request: EditMessageRequest = serde_json::from_value(serde_json::json!({
            "timestamp": 1,
            "editType": "edit",
            "expectedMessageIds": ["a", "b"],
        }))
        .unwrap();
        assert_eq!(
            request.expected_message_ids.as_deref(),
            Some(["a".to_string(), "b".to_string()].as_slice())
        );

        let absent: EditMessageRequest =
            serde_json::from_value(serde_json::json!({ "timestamp": 1 })).unwrap();
        assert!(absent.expected_message_ids.is_none());

        // An empty list is a view, not the absence of one: it must survive
        // deserialization as `Some([])` so the handler can check it.
        let empty: EditMessageRequest = serde_json::from_value(serde_json::json!({
            "timestamp": 1,
            "editType": "edit",
            "expectedMessageIds": [],
        }))
        .unwrap();
        assert_eq!(empty.expected_message_ids.as_deref(), Some([].as_slice()));

        // ... and an explicit `null` is the absence of one, like omitting it.
        let null: EditMessageRequest = serde_json::from_value(serde_json::json!({
            "timestamp": 1,
            "editType": "edit",
            "expectedMessageIds": serde_json::Value::Null,
        }))
        .unwrap();
        assert!(null.expected_message_ids.is_none());
    }
}
