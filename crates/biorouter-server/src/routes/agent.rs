use crate::routes::errors::ErrorResponse;
use crate::routes::workflow_utils::{
    apply_workflow_to_agent, build_workflow_with_parameter_values, load_workflow_by_id,
    validate_workflow,
};
use crate::state::AppState;
use axum::response::IntoResponse;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use biorouter::agents::ExtensionLoadResult;
use biorouter::biorouter_apps::{fetch_mcp_apps, BioRouterApp, McpAppCache};

use base64::Engine;
use biorouter::agents::ExtensionConfig;
use biorouter::config::resolve_extensions_for_new_session;
use biorouter::config::{BioRouterMode, Config};
use biorouter::model::ModelConfig;
use biorouter::permission::{grade_tool, SmartApproveConfig};
use biorouter::privacy::refusal::PrivacyRefusal;
use biorouter::privacy::{raise_needs_user_action, ProviderTier, SessionClassification};
use biorouter::prompt_template::render_global_file;
use biorouter::providers::create;
use biorouter::session::extension_data::ExtensionState;
use biorouter::session::session_manager::SessionType;
use biorouter::session::{EnabledExtensionsState, Session, SessionManager, WorkingDirUpdate};
use biorouter::workflow::{Workflow, WorkflowKnowledgeBases};
use biorouter::workflow_deeplink;
use biorouter::{
    agents::{
        extension::ToolInfo,
        extension_manager::{get_parameter_names, normalize},
    },
    config::permission::PermissionLevel,
};
use biorouter_mcp::knowledge::service::{KnowledgeService, PrimaryUpdate};
// Issue #56 DR-16. Named through the LIB path, not `crate::auth`: `src/routes/`
// is compiled into the `biorouterd` binary as well as the lib (see
// `routes::secret_matches`), and the digest is a process-global that must have
// exactly ONE instance — the lib's. `crate::auth` does not exist in the binary
// compilation, and a copy under `routes` would give the binary a second, empty
// static that `commands::agent` never installs into.
// Task 49 takes the THREE-way form beside the boolean one: a daemon that holds no
// user-action key must be reported differently from a caller that presented no
// proof (Task 18A's open question 23), and only `user_action_proof` can tell them
// apart. Both are defined in terms of the same one header, one key, one
// comparison — a second mechanism is what DR-18 refused.
use biorouter_server::auth::{is_user_action, user_action_proof, UserActionProof};
use rmcp::model::{CallToolRequestParams, Content};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{error, warn};

const PERMISSION_SETTINGS_SESSION_ID: &str = "__permission_settings__";
const PERMISSION_SETTINGS_LOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateFromSessionRequest {
    session_id: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateProviderRequest {
    provider: String,
    model: Option<String>,
    session_id: String,
    context_limit: Option<usize>,
    request_params: Option<std::collections::HashMap<String, serde_json::Value>>,
}

/// The body of the 409 `/agent/update_provider` returns when a privacy boundary
/// refused the bind (issue #56, Gate A).
///
/// Typed rather than a bare string because the GUI does not merely report the
/// refusal — it renders §14.4's repair card from it, and the card names both
/// colliding tiers and offers the private models that would work. A plain-text
/// 409 would leave the renderer parsing prose.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct PrivacyBarrierBody {
    /// Discriminator the client switches on. Always `privacy_barrier`.
    pub code: String,
    /// The classification of the chat that refused.
    pub session_classification: SessionClassification,
    /// The tier of the model that was offered.
    pub provider_tier: ProviderTier,
    /// The names of the providers a private chat *can* be switched to, so the
    /// card can offer the way forward rather than only the wall.
    pub available_private_providers: Vec<String>,
}

impl PrivacyBarrierBody {
    pub const CODE: &'static str = "privacy_barrier";
}

/// How `/agent/update_provider` failed, at the granularity the client needs.
///
/// Split out of the handler so the mapping can be tested without an
/// `AppState`: `AppState::new()` opens the REAL user session database (see the
/// note in `routes/session.rs`), so a route test must never go through it.
#[derive(Debug)]
pub(crate) enum ProviderBindFailure {
    /// A privacy boundary refused the bind — 409, with a body the GUI renders.
    Privacy(Box<PrivacyBarrierBody>),
    /// Anything else — 500, as before.
    Internal(String),
}

impl ProviderBindFailure {
    pub(crate) fn status(&self) -> StatusCode {
        match self {
            Self::Privacy(_) => StatusCode::CONFLICT,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for ProviderBindFailure {
    fn into_response(self) -> axum::response::Response {
        let status = self.status();
        match self {
            Self::Privacy(body) => (status, Json(body)).into_response(),
            Self::Internal(message) => (status, message).into_response(),
        }
    }
}

/// Classify an `Agent::update_provider` failure.
///
/// The refusal is a typed error carried inside `anyhow::Error`, so this asks
/// with `downcast_ref` rather than matching on a message. Every other failure
/// keeps the pre-#56 behaviour — a 500 with the error's text.
///
/// The card needs the *pair* of colliding tiers, so only a refusal that carries
/// one is rendered as a barrier. `Agent::update_provider` produces exactly one
/// such refusal today; Task 18A's three HTTP-channel variants are about a
/// channel rather than a session collision, carry no pair, and are rendered at
/// their own handlers — they never travel through here.
pub(crate) fn classify_provider_bind_failure(
    error: &anyhow::Error,
    available_private_providers: Vec<String>,
) -> ProviderBindFailure {
    match error
        .downcast_ref::<PrivacyRefusal>()
        .and_then(|refusal| Some((refusal.session_classification()?, refusal.provider_tier()?)))
    {
        Some((session_classification, provider_tier)) => {
            ProviderBindFailure::Privacy(Box::new(PrivacyBarrierBody {
                code: PrivacyBarrierBody::CODE.to_string(),
                session_classification,
                provider_tier,
                available_private_providers,
            }))
        }
        None => ProviderBindFailure::Internal(format!("Failed to update provider: {error}")),
    }
}

/// Every registered provider whose shipped metadata claims Private — the models
/// a private chat may be switched to.
///
/// Read from the same registry the settings grid reads, so the card can never
/// offer a model the grid does not show.
async fn available_private_providers() -> Vec<String> {
    let mut names: Vec<String> = biorouter::providers::providers()
        .await
        .into_iter()
        .filter(|(metadata, _)| metadata.tier.is_private())
        .map(|(metadata, _)| metadata.name)
        .collect();
    names.sort();
    names
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct GetToolsQuery {
    extension_name: Option<String>,
    session_id: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct StartAgentRequest {
    working_dir: String,
    #[serde(default)]
    workflow: Option<Workflow>,
    #[serde(default)]
    workflow_id: Option<String>,
    #[serde(default)]
    workflow_deeplink: Option<String>,
    #[serde(default)]
    extension_overrides: Option<Vec<ExtensionConfig>>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct StopAgentRequest {
    session_id: String,
}

/// Turn a workflow's `{ default, visible }` into "which bases the session
/// should hold" and "what its primary should be".
///
/// `WorkflowKnowledgeBases` already expresses a set plus one primary, which is
/// exactly the session model — so this is a translation, not a schema change.
/// It deliberately does **not** consult the installed bases: inverting the set
/// against an inventory is the service's job, inside the lock (see
/// [`apply_workflow_knowledge_selection`]).
///
/// **The primary comes only from `default`.** A workflow that lists bases
/// without naming one is a workflow with no primary, whatever the set size.
/// Promoting a sole visible member looked harmless — "there is only one
/// candidate, so it cannot be the wrong one" — but the merged model forbids
/// *inventing* the pointer at all: it is the target of KB-less writes, so a
/// promoted primary silently turns "I did not say where to write" into a
/// commit into someone's base. With no primary, the write fails and names the
/// candidates; the author who wanted the write target says so in `default`.
///
/// One rule still earns its keep: a `default` that was not listed in `visible`
/// is unioned into the set, because the invariant requires the primary to be a
/// member and the author plainly meant it.
pub(crate) fn plan_workflow_knowledge_selection(
    selection: &WorkflowKnowledgeBases,
) -> (Vec<String>, Option<String>) {
    let mut visible: Vec<String> = selection.visible.clone();
    if let Some(default) = selection.default.as_deref() {
        if !visible.iter().any(|id| id == default) {
            visible.push(default.to_string());
        }
    }
    visible.sort();
    visible.dedup();
    (visible, selection.default.clone())
}

/// Install a workflow's declared selection into the session it just created.
///
/// The whole gesture is one root-locked service call. It used to list the
/// installed bases here, invert them against `visible` to get a hidden list,
/// and only then take the lock to write it — so a base created by any other
/// surface in that window was absent from the hidden list it wrote, and joined
/// a workflow session the workflow had never declared it in. `set_visible_kbs`
/// takes the visible set and does the inversion *inside* the lock, against the
/// inventory as it is at the moment of the write.
fn apply_workflow_knowledge_selection(
    svc: &KnowledgeService,
    session_id: &str,
    workflow: &Workflow,
) -> Result<(), ErrorResponse> {
    let Some(selection) = workflow.knowledge_bases.as_ref() else {
        return Ok(());
    };

    let (visible, primary) = plan_workflow_knowledge_selection(selection);
    let primary = match primary.as_deref() {
        Some(id) => PrimaryUpdate::Set(id),
        None => PrimaryUpdate::Clear,
    };

    svc.set_visible_kbs(Some(session_id), &visible, primary)
        .map_err(|err| {
            error!("Failed to apply workflow knowledge bases: {}", err);
            ErrorResponse {
                message: format!("Failed to apply workflow knowledge bases: {}", err),
                status: StatusCode::BAD_REQUEST,
            }
        })?;

    Ok(())
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct RestartAgentRequest {
    session_id: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateWorkingDirRequest {
    session_id: String,
    working_dir: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ResumeAgentRequest {
    session_id: String,
    load_model_and_extensions: bool,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct AddExtensionRequest {
    session_id: String,
    config: ExtensionConfig,
}

/// Issue #56 Task 49 (DR-26): the user accepting ONE cross-institutional data
/// flow.
///
/// ⚠ **There is no `affiliation` field, and there must not be one.** The grant is
/// keyed on the affiliation of the model bound *right now*, read by the daemon
/// from the same sample that produced the warning. A client-supplied institution
/// would let a caller record an acceptance for a triple the user was never shown
/// — which is the one thing this control exists to prevent.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct CrossAffiliationGrantRequest {
    session_id: String,
    /// The extension whose cross-institutional flow the user accepted. Any
    /// spelling the UI holds; it is `name_to_key`-normalised on both sides.
    extension: String,
}

/// What was accepted, echoed back so the caller can record the exact sentence the
/// user was shown rather than a paraphrase of it.
#[derive(Serialize, utoipa::ToSchema)]
pub struct CrossAffiliationGrantResponse {
    /// The full statement, including the scope of the approval
    /// (`privacy::grant::GRANT_SCOPE_COPY`).
    accepted: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct RemoveExtensionRequest {
    name: String,
    session_id: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ReadResourceRequest {
    session_id: String,
    extension_name: String,
    uri: String,
}

#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReadResourceResponse {
    uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    mime_type: Option<String>,
    text: String,
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    meta: Option<serde_json::Map<String, Value>>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CallToolRequest {
    session_id: String,
    name: String,
    arguments: Value,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct CallToolResponse {
    content: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    structured_content: Option<Value>,
    is_error: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    _meta: Option<Value>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ResumeAgentResponse {
    pub session: Session,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension_results: Option<Vec<ExtensionLoadResult>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initialization_error: Option<AgentInitializationError>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct AgentInitializationError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct RestartAgentResponse {
    pub extension_results: Vec<ExtensionLoadResult>,
}

#[utoipa::path(
    post,
    path = "/agent/start",
    request_body = StartAgentRequest,
    responses(
        (status = 200, description = "Agent started successfully", body = Session),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Unauthorized - invalid secret key"),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
#[allow(clippy::too_many_lines)]
async fn start_agent(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<StartAgentRequest>,
) -> Result<Json<Session>, ErrorResponse> {
    let StartAgentRequest {
        working_dir,
        workflow,
        workflow_id,
        workflow_deeplink,
        extension_overrides,
    } = payload;

    let original_workflow = if let Some(deeplink) = workflow_deeplink {
        match workflow_deeplink::decode(&deeplink) {
            Ok(workflow) => Some(workflow),
            Err(err) => {
                error!("Failed to decode workflow deeplink: {}", err);
                return Err(ErrorResponse {
                    message: err.to_string(),
                    status: StatusCode::BAD_REQUEST,
                });
            }
        }
    } else if let Some(id) = workflow_id {
        match load_workflow_by_id(state.as_ref(), &id).await {
            Ok(workflow) => Some(workflow),
            Err(err) => return Err(err),
        }
    } else {
        workflow
    };

    if let Some(ref workflow) = original_workflow {
        if let Err(err) = validate_workflow(workflow) {
            return Err(ErrorResponse {
                message: err.message,
                status: err.status,
            });
        }
    }

    // Always create new sessions with the same human-friendly placeholder.
    // The numbered variant ("New session 154") leaked process-internal counter
    // state into the UI and made it hard for the frontend to recognize a name
    // as still being the default. Whether this session has been named is now
    // tracked exclusively via `user_set_name` plus a name-vs-default check.
    let name = "New Session".to_string();

    let manager = state.session_manager();

    let mut session = manager
        .create_session(PathBuf::from(&working_dir), name, SessionType::User)
        .await
        .map_err(|err| {
            error!("Failed to create session: {}", err);
            ErrorResponse {
                message: format!("Failed to create session: {}", err),
                status: StatusCode::BAD_REQUEST,
            }
        })?;

    if let Some(workflow) = original_workflow.as_ref() {
        apply_workflow_knowledge_selection(&state.knowledge_service, &session.id, workflow)?;
    }

    let workflow_extensions = original_workflow
        .as_ref()
        .and_then(|r| r.extensions.as_deref());
    let extensions_to_use =
        resolve_extensions_for_new_session(workflow_extensions, extension_overrides);
    let mut extension_data = session.extension_data.clone();
    let extensions_state = EnabledExtensionsState::new(extensions_to_use);
    if let Err(e) = extensions_state.to_extension_data(&mut extension_data) {
        tracing::warn!("Failed to initialize session with extensions: {}", e);
    } else {
        manager
            .update(&session.id)
            .extension_data(extension_data.clone())
            .apply()
            .await
            .map_err(|err| {
                error!("Failed to save initial extension state: {}", err);
                ErrorResponse {
                    message: format!("Failed to save initial extension state: {}", err),
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                }
            })?;
    }

    if let Some(workflow) = original_workflow {
        manager
            .update(&session.id)
            .workflow(Some(workflow))
            .apply()
            .await
            .map_err(|err| {
                error!("Failed to update session with workflow: {}", err);
                ErrorResponse {
                    message: format!("Failed to update session with workflow: {}", err),
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                }
            })?;
    }

    // Refetch session to get all updates
    session = manager
        .get_session(&session.id, false)
        .await
        .map_err(|err| {
            error!("Failed to get updated session: {}", err);
            ErrorResponse {
                message: format!("Failed to get updated session: {}", err),
                status: StatusCode::INTERNAL_SERVER_ERROR,
            }
        })?;

    // Eagerly start loading extensions in the background
    let session_for_spawn = session.clone();
    let state_for_spawn = state.clone();
    let session_id_for_task = session.id.clone();
    let task = tokio::spawn(async move {
        match state_for_spawn
            .get_agent(session_for_spawn.id.clone())
            .await
        {
            Ok(agent) => {
                let results = agent.load_extensions_from_session(&session_for_spawn).await;
                tracing::debug!(
                    "Background extension loading completed for session {}",
                    session_for_spawn.id
                );
                results
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to create agent for background extension loading: {}",
                    e
                );
                vec![]
            }
        }
    });

    state
        .set_extension_loading_task(session_id_for_task, task)
        .await;

    Ok(Json(session))
}

#[utoipa::path(
    post,
    path = "/agent/resume",
    request_body = ResumeAgentRequest,
    responses(
        (status = 200, description = "Agent started successfully", body = ResumeAgentResponse),
        (status = 400, description = "Bad request - invalid working directory"),
        (status = 401, description = "Unauthorized - invalid secret key"),
        (status = 500, description = "Internal server error")
    )
)]
async fn resume_agent(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ResumeAgentRequest>,
) -> Result<Json<ResumeAgentResponse>, ErrorResponse> {
    let session = state
        .session_manager()
        .get_session(&payload.session_id, true)
        .await
        .map_err(|err| {
            error!("Failed to resume session {}: {}", payload.session_id, err);
            ErrorResponse {
                message: format!("Failed to resume session: {}", err),
                status: StatusCode::NOT_FOUND,
            }
        })?;

    let mut initialization_error = None;
    let extension_results = if payload.load_model_and_extensions {
        match state.get_agent_for_route(payload.session_id.clone()).await {
            Ok(agent) => match agent.restore_provider_from_session(&session).await {
                Ok(()) => {
                    let extension_results = if let Some(results) =
                        state.take_extension_loading_task(&payload.session_id).await
                    {
                        tracing::debug!(
                            "Using background extension loading results for session {}",
                            payload.session_id
                        );
                        state
                            .remove_extension_loading_task(&payload.session_id)
                            .await;
                        results
                    } else {
                        tracing::debug!(
                            "No background task found, loading extensions for session {}",
                            payload.session_id
                        );
                        agent.load_extensions_from_session(&session).await
                    };
                    Some(extension_results)
                }
                Err(error) => {
                    tracing::error!(
                        "Failed to restore provider for session {}: {}",
                        payload.session_id,
                        error
                    );
                    initialization_error = Some(AgentInitializationError {
                        code: "provider_restore_failed".into(),
                        message: error.to_string(),
                        retryable: false,
                    });
                    None
                }
            },
            Err(status) => {
                tracing::error!(
                    "Failed to prepare agent for session {}: {}",
                    payload.session_id,
                    status
                );
                initialization_error = Some(AgentInitializationError {
                    code: "agent_unavailable".into(),
                    message: "Biorouter could not prepare the model agent for this session.".into(),
                    retryable: status.is_server_error(),
                });
                None
            }
        }
    } else {
        None
    };

    Ok(Json(ResumeAgentResponse {
        session,
        extension_results,
        initialization_error,
    }))
}

#[utoipa::path(
    post,
    path = "/agent/update_from_session",
    request_body = UpdateFromSessionRequest,
    responses(
        (status = 200, description = "Update agent from session data successfully"),
        (status = 401, description = "Unauthorized - invalid secret key"),
        (status = 424, description = "Agent not initialized"),
    ),
)]
async fn update_from_session(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UpdateFromSessionRequest>,
) -> Result<StatusCode, ErrorResponse> {
    let agent = state
        .get_agent_for_route(payload.session_id.clone())
        .await
        .map_err(|status| ErrorResponse {
            message: format!("Failed to get agent: {}", status),
            status,
        })?;
    let session = state
        .session_manager()
        .get_session(&payload.session_id, false)
        .await
        .map_err(|err| ErrorResponse {
            message: format!("Failed to get session: {}", err),
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;
    let context: HashMap<&str, Value> = HashMap::new();
    let desktop_prompt =
        render_global_file("desktop_prompt.md", &context).expect("Prompt should render");
    let mut update_prompt = desktop_prompt;
    if let Some(workflow) = session.workflow {
        match build_workflow_with_parameter_values(
            &workflow,
            session.user_workflow_values.unwrap_or_default(),
        )
        .await
        {
            Ok(Some(workflow)) => {
                if let Some(prompt) = apply_workflow_to_agent(&agent, &workflow, true).await {
                    update_prompt = prompt;
                }
            }
            Ok(None) => {
                // Workflow has missing parameters - use default prompt
            }
            Err(e) => {
                return Err(ErrorResponse {
                    message: e.to_string(),
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                });
            }
        }
    }
    agent.extend_system_prompt(update_prompt).await;

    Ok(StatusCode::OK)
}

#[utoipa::path(
    get,
    path = "/agent/tools",
    params(
        ("extension_name" = Option<String>, Query, description = "Optional extension name to filter tools"),
        ("session_id" = String, Query, description = "Session ID used to inspect active tools; pass an empty string to inspect one globally enabled extension from settings")
    ),
    responses(
        (status = 200, description = "Tools retrieved successfully", body = Vec<ToolInfo>),
        (status = 401, description = "Unauthorized - invalid secret key"),
        (status = 408, description = "Extension timed out while loading for settings"),
        (status = 424, description = "Agent not initialized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn get_tools(
    State(state): State<Arc<AppState>>,
    Query(query): Query<GetToolsQuery>,
) -> Result<Json<Vec<ToolInfo>>, StatusCode> {
    let config = Config::global();
    let biorouter_mode = config.get_biorouter_mode().unwrap_or(BioRouterMode::Auto);
    let session_id = query.session_id;
    let extension_name = query.extension_name.map(|name| normalize(&name));
    let agent_session_id = if session_id.is_empty() {
        PERMISSION_SETTINGS_SESSION_ID
    } else {
        &session_id
    };
    let agent = state
        .get_agent_for_route(agent_session_id.to_string())
        .await?;

    if session_id.is_empty() {
        let Some(extension_name) = extension_name.as_deref() else {
            return Ok(Json(Vec::new()));
        };
        let Some(extension) = biorouter::config::get_all_extensions()
            .into_iter()
            .find(|entry| entry.enabled && entry.config.key() == extension_name)
        else {
            return Ok(Json(Vec::new()));
        };

        agent
            .remove_extension(extension_name)
            .await
            .map_err(|error| {
                warn!(extension = extension_name, %error, "Failed to refresh permission settings extension");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        tokio::time::timeout(
            PERMISSION_SETTINGS_LOAD_TIMEOUT,
            agent.add_extension(extension.config),
        )
        .await
        .map_err(|_| {
            warn!(extension = extension_name, "Timed out loading extension for permission settings");
            StatusCode::REQUEST_TIMEOUT
        })?
        .map_err(|error| {
            warn!(extension = extension_name, %error, "Failed to load extension for permission settings");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }

    let permission_manager = agent.config.permission_manager.clone();
    // BR-18: SmartApprove now auto-approves read-only-annotated tools, so the
    // level shown here has to reflect the risk grade the inspector will actually
    // apply — otherwise the settings UI keeps claiming a plain file read will
    // prompt. Graded from the same annotations, through the same predicate the
    // inspector uses, so the two cannot disagree.
    let smart = SmartApproveConfig::from_config();

    // Issue #56 Task 16: the permission editor's list, NOT the model's. This
    // route is what Settings → Extensions → tool permissions renders, and a
    // private extension has to stay visible and badged there whatever model
    // happens to be bound — including on the empty-`session_id` branch above,
    // which loads one globally enabled extension purely so its tools can be
    // listed here. Gate E is about the model's context; this is the user's own
    // administration surface. Do not swap this for `list_tools`.
    let mut tools: Vec<ToolInfo> = agent
        .list_tools_for_permission_settings(&session_id, extension_name)
        .await
        .into_iter()
        .map(|tool| {
            let permission = permission_manager
                .get_user_permission(&tool.name)
                .or_else(|| {
                    if biorouter_mode == BioRouterMode::SmartApprove {
                        permission_manager
                            .get_smart_approve_permission(&tool.name)
                            .or_else(|| {
                                smart.enabled.then(|| {
                                    if smart.requires_confirmation(grade_tool(&tool)) {
                                        PermissionLevel::AskBefore
                                    } else {
                                        PermissionLevel::AlwaysAllow
                                    }
                                })
                            })
                    } else if biorouter_mode == BioRouterMode::Approve {
                        Some(PermissionLevel::AskBefore)
                    } else {
                        None
                    }
                });

            ToolInfo::new(
                &tool.name,
                tool.description
                    .as_ref()
                    .map(|d| d.as_ref())
                    .unwrap_or_default(),
                get_parameter_names(&tool),
                permission,
            )
        })
        .collect::<Vec<ToolInfo>>();
    tools.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(Json(tools))
}

#[utoipa::path(
    post,
    path = "/agent/update_provider",
    request_body = UpdateProviderRequest,
    responses(
        (status = 200, description = "Provider updated. The body is DR-26's cross-institutional \
                                      warning for this chat on the model just bound — the \
                                      statement the user is shown before proceeding, warnings \
                                      separated by a blank line — and is EMPTY when the bind \
                                      crosses no institutional boundary, which is the normal \
                                      case.",
                       body = String),
        (status = 400, description = "Bad request - missing or invalid parameters"),
        (status = 401, description = "Unauthorized - invalid secret key"),
        (status = 409, description = "Refused by a privacy boundary (issue #56). Gate A: \
                                      a public model cannot be bound to a private chat \
                                      (body = PrivacyBarrierBody). DR-16: the bind raises this \
                                      chat's capability to Private and the request carried no \
                                      proof it came from the user (body = plain text)",
                       body = PrivacyBarrierBody),
        (status = 424, description = "Agent not initialized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn update_agent_provider(
    State(state): State<Arc<AppState>>,
    // Before `Json`, which consumes the body and must be last.
    headers: axum::http::HeaderMap,
    Json(payload): Json<UpdateProviderRequest>,
) -> Result<String, axum::response::Response> {
    let agent = state
        .get_agent_for_route(payload.session_id.clone())
        .await
        .map_err(|e| (e, "No agent for session id".to_owned()).into_response())?;

    let config = Config::global();
    let model = match payload.model.or_else(|| config.get_biorouter_model().ok()) {
        Some(m) => m,
        None => {
            return Err((StatusCode::BAD_REQUEST, "No model specified".to_owned()).into_response());
        }
    };

    let model_config = ModelConfig::new(&model)
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("Invalid model config: {}", e),
            )
                .into_response()
        })?
        .with_context_limit(payload.context_limit)
        .with_request_params(payload.request_params);

    let new_provider = create(&payload.provider, model_config).await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Failed to create {} provider: {}", &payload.provider, e),
        )
            .into_response()
    })?;

    // Issue #56 DR-26, taken from the provider this route CREATED rather than
    // re-read off the agent afterwards. It is the key half of the triple the
    // grant lookup below is done on, and `update_provider` reassigns the provider
    // mutex with no turn lock — so a second read could key the lookup on a model
    // some other caller bound in between and suppress a warning on an acceptance
    // that was never given for this model.
    let model_affiliation = new_provider.affiliation();

    // Issue #56 DR-16. Raising this chat's capability to Private is the user's
    // act alone, and this route has no principal — `check_token` compares one
    // machine-wide bearer and every authenticated request looks the same,
    // whoever sent it (AR-11/AR-15). `llamacpp` needs no credential at all, so
    // `{"provider":"llamacpp"}` would otherwise be a tier raise any caller can
    // perform with nothing but the daemon secret.
    //
    // A CONDITION, not a blanket refusal: refusing every raise would take the
    // user's own model picker away along with the model's, which is the posture
    // DR-16 rejected. Sideways and downward binds are untouched for every
    // caller, which is what keeps Gate A's path, the CLI,
    // `restore_provider_from_session` and every apps-runtime bind working.
    //
    // An unbound session reads as Public — `Agent::provider` errors when nothing
    // is bound (and when Gate B' refuses what is), and the conservative reading
    // is that a session with no private capability has none to preserve, so a
    // first bind to a private provider is a raise.
    let current = agent
        .provider()
        .await
        .map(|p| p.tier())
        .unwrap_or(ProviderTier::Public);
    // DR-15's master opt-out, read INSIDE the gate. A direct read, not a
    // `CallCapability`: a provider raise over HTTP is not a tool call and has no
    // admitted capability to inherit.
    if biorouter::privacy::privacy_tiers_enabled()
        && raise_needs_user_action(current, new_provider.tier())
        && !is_user_action(&headers)
    {
        return Err((
            StatusCode::CONFLICT,
            PrivacyRefusal::TierRaiseNeedsUser {
                requested: payload.provider.clone(),
            }
            .to_string(),
        )
            .into_response());
    }

    // Issue #56 Gate A (P3). A privacy refusal is a 409 with a body the GUI
    // renders a repair card from, not a 500 with a sentence: collapsing it to
    // the generic error is what let the renderer report a refused switch as a
    // green success toast.
    if let Err(e) = agent
        .update_provider(new_provider, &payload.session_id)
        .await
    {
        return Err(
            classify_provider_bind_failure(&e, available_private_providers().await).into_response(),
        );
    }

    // Issue #56 DR-26 at the BIND surface, and the whole of what this route was
    // missing. `Agent::update_provider` has detected this mismatch since Task 48
    // and has only ever written it to `tracing::warn!`, where the person who just
    // switched models cannot see it — so the ruling's "warn the user, naming both
    // institutions, before proceeding" was, on this surface, unimplemented.
    //
    // ⚠ **It warns; it does not refuse, and the ordering says so.** This is read
    // AFTER the bind has succeeded and is returned with a 200, not raised as a
    // 409 — both endpoints are Private, legitimate cross-institutional work under
    // a real DUA exists, and refusing here would strand a chat whose model is
    // already bound. Gate C still refuses the first DISPATCH, which is where the
    // user meets an accept control; this is the earlier, quieter statement that
    // stops the refusal being the first they hear of it.
    //
    // ⚠ **Empty is the normal answer** — every public model, every local model,
    // every model bound to the same institution as the chat's connectors, and
    // every machine with DR-27's `open` policy. A caller must treat an empty body
    // as "nothing to say" and never as "the daemon did not answer".
    Ok(agent
        .cross_affiliation_notice(&payload.session_id, model_affiliation)
        .await)
}

#[utoipa::path(
    post,
    path = "/agent/add_extension",
    request_body = AddExtensionRequest,
    responses(
        (status = 200, description = "Extension added. The body is DR-26's cross-institutional \
                                      warning for this chat once the extension is attached — the \
                                      statement the user is shown before proceeding, warnings \
                                      separated by a blank line — and is EMPTY when nothing in \
                                      the chat crosses an institutional boundary, which is the \
                                      normal case.",
                       body = String),
        (status = 401, description = "Unauthorized - invalid secret key"),
        (status = 403, description = "Refused by a privacy boundary (issue #56 Task 58 / #47): \
                                      the named chat is private (or absent — an unproven caller \
                                      is told the same thing for both) and the request carried no \
                                      proof it came from the user"),
        (status = 409, description = "Refused by a privacy boundary (issue #56, DR-16): a \
                                      private extension cannot be attached to a chat running on \
                                      a public model"),
        (status = 424, description = "Agent not initialized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn agent_add_extension(
    State(state): State<Arc<AppState>>,
    // Before `Json`, which consumes the body and must be last.
    headers: axum::http::HeaderMap,
    Json(request): Json<AddExtensionRequest>,
) -> Result<String, ErrorResponse> {
    // Issue #56 Task 58 / #47. FIRST, before the agent is fetched — `get_agent`
    // CREATES one for a session that has none, so a gate below it would let an
    // unproven caller materialise an agent for a chat it may not address, and
    // the refusals downstream (424 "not initialized", the tier 409) would tell
    // it what it had found. `session_id` is a request parameter, not a
    // credential; see `routes::session_reach`.
    //
    // ⚠ This is NOT a relaxation of the outright refusal below. That one is
    // about the EXTENSION's tier against the chat's model and stays
    // unconditional — attaching a private extension to a public chat is not a
    // raise the user can authorize either. This one is about whether the caller
    // may address the named chat at all, and it is a no-op for every public
    // chat, which is precisely the case that refusal governs.
    crate::routes::session_reach::session_reach(
        state.session_manager(),
        &request.session_id,
        &headers,
    )
    .await?;

    let agent = state.get_agent(request.session_id.clone()).await?;

    // Issue #56 DR-16. `/agent/add_extension` hands `request.config` straight to
    // the agent and persists it, which is how a private extension's TOOLS arrive
    // in a session Gate F1 already refuses to let the model enable through
    // `extensionmanager__manage_extensions`.
    //
    // Refused OUTRIGHT — no user-proof branch, deliberately. Attaching a private
    // extension to a public session is not a raise the user can authorize
    // either; their route is to switch the model first and then attach.
    // ONE read of the bound provider, from which BOTH privacy axes are taken.
    // Two `agent.provider().await` calls would be the read-then-read
    // `CallCapability` exists to collapse — the mutex can be reassigned between
    // them with no turn lock — so the `Arc` is taken once and asked twice.
    let bound = agent.provider().await.ok();
    let capability = bound
        .as_ref()
        .map(|p| p.tier())
        .unwrap_or(ProviderTier::Public);
    let extension_name = request.config.name();
    // Task 43 (DR-23): the config, not just its name — a renamed entry is
    // resolved through the install directory in its arguments. Task 48 (DR-26):
    // one resolution carries the tier AND the affiliation, so the two axes
    // cannot disagree about this entry.
    let classification =
        biorouter::privacy::resolve_extension(&extension_name, Some(&request.config));
    // DR-15's master opt-out, read INSIDE the gate. A direct read, not a
    // `CallCapability`: `/agent/add_extension` is not a tool dispatch and has no
    // admitted capability to inherit. Read ONCE and shared with the
    // cross-affiliation check below, so this route cannot refuse on the tier
    // axis with tiers on and then warn — or not warn — with tiers off.
    let enforced = biorouter::privacy::privacy_tiers_enabled();
    // The DR-26 half of that same single read, bound once here rather than taken
    // twice: the gate below states the mismatch against it and the notice at the
    // bottom suppresses flows the user has already accepted for it, and those two
    // must be the same model or the route can warn about one institution while
    // checking an acceptance recorded for another.
    let model_affiliation = bound.as_ref().and_then(|p| p.affiliation());
    // ⚠ **The predicate is asked, not re-typed.** This is the third door to the
    // same capability — the agent's two are `extensionmanager__manage_extensions`
    // and the workspace's enable pair, which share
    // `privacy::refusal::extension_enable_refusal` — and the rule it applies is
    // the identical one. It cannot call that gate (its refusal is the typed
    // `PrivateExtensionOverHttp` body the GUI renders a repair card from, and its
    // posture on the other two arms is deliberately different: a user proceeds
    // past the affiliation warning, and the operator pin is theirs to override).
    // So it asks the boolean underneath instead. Writing
    // `classification.tier.is_private() && capability == Public` out here is what
    // made this rule four hand-written copies; copies agree until the edit nobody
    // cross-checks.
    if enforced && biorouter::privacy::refusal::tier_refuses(classification.tier, capability) {
        return Err(ErrorResponse {
            status: StatusCode::CONFLICT,
            message: PrivacyRefusal::PrivateExtensionOverHttp {
                name: extension_name,
            }
            .to_string(),
        });
    }

    // Issue #56 Task 48, DR-26 at the USER's enable path. This is the same
    // mismatch the bind surface finds from the other end, and — unlike the tier
    // refusal above — it WARNS rather than refusing.
    //
    // The asymmetry with `extensionmanager__manage_extensions`, which refuses,
    // is DR-26's user/agent rule: a user who insists proceeds past a warning; an
    // agent never clears one automatically. This route is the user acting
    // through the GUI's Settings > Extensions, so the extension is attached and
    // the risk is stated.
    //
    // ⚠ **The log is the support transcript's copy, not the product.** DR-26
    // requires the user be shown the warning before proceeding, and for a long
    // time this `tracing::warn!` was the whole of it: a researcher enabling
    // another institution's connector from Settings > Extensions was told
    // nothing. The user's copy is the 200 body at the bottom of this handler.
    //
    // ⚠ **Both remain, and neither is redundant.** This one is read here, BEFORE
    // the attach, off the entry the caller actually sent — so a mismatch is on
    // the record even if the attach then fails, and even for the callers that
    // discard the body. The notice below is read AFTER the attach, off the
    // agent's live extension set, so it states the chat as it now is.
    if let Some(warning) = biorouter::privacy::affiliation::gate_cross_affiliation_warning(
        enforced,
        capability,
        model_affiliation,
        &extension_name,
        &classification,
    ) {
        tracing::warn!(
            session_id = request.session_id,
            extension = extension_name,
            "{warning}"
        );
    }

    agent
        .add_extension(request.config)
        .await
        .map_err(|e| ErrorResponse::internal(format!("Failed to add extension: {}", e)))?;

    // Persist here rather than in add_extension to ensure we only save state
    // after the extension successfully loads. This prevents failed extensions
    // from being persisted as enabled in the session.
    agent
        .persist_extension_state(&request.session_id)
        .await
        .map_err(|e| {
            error!("Failed to persist extension state: {}", e);
            ErrorResponse::internal(format!("Failed to persist extension state: {}", e))
        })?;

    // Issue #56 DR-26 at the USER's enable surface — the second half of the
    // ruling's "warn the user, naming both institutions, before proceeding", and
    // the half that had no implementation at all. Composed by
    // `Agent::cross_affiliation_notice`, the same method `/agent/update_provider`
    // returns, so the two surfaces cannot start describing one boundary in
    // different words.
    //
    // ⚠ **Read AFTER the attach, deliberately.** The user has already acted, and
    // DR-26 says a user who insists proceeds — so this states the chat as it now
    // is, including any OTHER connector the newly bound model does not reach.
    // Reading it before the attach would answer for a chat that no longer exists
    // by the time the caller sees it, and would silently omit the very extension
    // the request was about.
    //
    // ⚠ **This is not a refusal and must never become one.** The agent's own
    // enable path (`check_enable_allowed`) refuses; this one warns, because the
    // caller here is the person at the keyboard. Inverting that strands a
    // legitimate cross-institutional user with no way to attach their own
    // connector, which is the "researchers turn the feature off" outcome DR-26
    // exists to avoid.
    Ok(agent
        .cross_affiliation_notice(&request.session_id, model_affiliation)
        .await)
}

/// What the grant route says to a caller that presented no proof of a human.
///
/// It is the model-facing half of DR-26's ruling and it says the two things a
/// model needs: that this is not its decision, and what to do instead. It ends
/// on the user's own action rather than on
/// [`biorouter::privacy::refusal::ASK_THE_USER_TO_SWITCH`] — this chat is already
/// on a private model, so "switch to a private model" is advice it has taken.
///
/// ⚠ **It carries NEITHER of the two markers the renderer keys on**, which is the
/// same choice `DECLASSIFY_NEEDS_USER` makes and for the same reason.
/// `USER_ACTION_REFUSAL_MARKER`'s toast says *switch this chat's model* and
/// `COPY_OF_PRIVATE_REFUSAL_MARKER`'s says *branch it from the chat window*; both
/// would send the user somewhere that cannot help, because the way out of this
/// one is to approve the flow or to re-bind to the owning institution's model.
/// The wording steps around both.
const CROSS_AFFILIATION_GRANT_NEEDS_USER: &str =
    "Accepting a cross-institutional data flow is a decision only the person at the keyboard can \
     make, and this request carried no proof it came from them. Nothing was recorded. Do not \
     retry — the same call will be refused again, and no setting, hook or permission mode \
     changes it. Tell the user which extension you need and what for, and let them approve it.";

/// …and when the daemon holds no user-action key at all.
///
/// A separate sentence, per Task 18A's open question 23: reporting "this daemon
/// cannot verify a human" as "you are not a human" sends the person at the
/// keyboard hunting for a permission they can never obtain. `just run-server`, a
/// hand-run `biorouterd agent` and every headless deployment land here.
const CROSS_AFFILIATION_GRANT_NO_KEY: &str =
    "This daemon was started without a user-action key, so it cannot verify that a request came \
     from the person at the keyboard — and accepting a cross-institutional data flow requires \
     that proof. Nothing was recorded. This control is unavailable on this daemon; use the \
     desktop app, or bind this chat to a model covered by the same institution's agreements.";

/// …and when there is no live mismatch to accept.
///
/// ⚠ **Refusing here is the control, not a validation nicety.** DR-26's whole
/// premise is that a user accepts a risk **that was stated to them**. A grant
/// recorded with no mismatch behind it is a pre-authorisation for a flow nobody
/// has described — it would sit in the store waiting for a future bind to make it
/// meaningful, and the sentence the user agreed to would never have existed.
const CROSS_AFFILIATION_GRANT_NOTHING_TO_ACCEPT: &str =
    "There is no cross-institutional mismatch to accept for that extension in this chat: it is \
     not enabled here, or the model bound right now is already covered by agreements that reach \
     it. Nothing was recorded.";

/// …and when the chat is not loaded in this daemon at all.
///
/// ⚠ **A distinct sentence, and this route must not fold it into the one above.**
/// The two states are indistinguishable to the caller and completely different to
/// the user: "nothing to accept" says the risk you saw is gone, "this chat is not
/// loaded" says ask again once it is. Reporting the second as the first is open
/// question 23's mistake with a different subject — it sends the person at the
/// keyboard hunting for a mismatch to fix when what they need is to open the
/// chat.
///
/// It is a refusal rather than a load, because the affiliation a grant is keyed
/// on can only be sampled off the model this chat is actually bound to. Loading
/// the chat here to answer would read the process default instead — a grant
/// recorded against the wrong institution, which no later call would match.
const CROSS_AFFILIATION_GRANT_CHAT_NOT_LOADED: &str =
    "That chat is not loaded in this daemon right now, so the model it is bound to — and \
     therefore whose agreements cover it — cannot be read. Nothing was recorded. Open the chat \
     and approve the flow there.";

/// A verdict from [`user_action_proof`] to the grant route's answer: `Ok(())`
/// only for `Proven`.
///
/// ⚠ **Extracted so the claim "only the user may grant" is asserted rather than
/// grepped for.** The handler it belongs to cannot be driven from a test —
/// `AppState::new()` opens the developer's REAL session database — so every
/// other fact about this route is a source scan, and a scan for
/// `user_action_proof(` keeps passing against a match whose `Unproven` arm was
/// refactored into `=> {}`. This mapping is pure, so
/// `only_a_proven_user_action_gets_past_the_grant_guard` drives all three arms
/// for real. The one thing it cannot see — that the guard runs before the chat
/// is touched — stays a scan, and says so.
fn refuse_grant_unless_user(proof: UserActionProof) -> Result<(), ErrorResponse> {
    match proof {
        UserActionProof::Proven => Ok(()),
        UserActionProof::Unproven => Err(ErrorResponse {
            status: StatusCode::FORBIDDEN,
            message: CROSS_AFFILIATION_GRANT_NEEDS_USER.to_string(),
        }),
        // A separate sentence, per Task 18A's open question 23 — see the
        // constant.
        UserActionProof::NoKeyInstalled => Err(ErrorResponse {
            status: StatusCode::FORBIDDEN,
            message: CROSS_AFFILIATION_GRANT_NO_KEY.to_string(),
        }),
    }
}

/// …and when the machine is in `strict` and the operating system did not
/// confirm (issue #56 Task 52, DR-27).
///
/// It says which of the three modes is in force, because that is the fact the
/// user needs and the one they can change: a person who does not know their
/// machine is in `strict` reads a bare "authentication failed" as a bug. It does
/// **not** tell a model how to leave `strict` — that is a user-only setting
/// behind its own system prompt, and advice to change it is advice to attack the
/// control.
const CROSS_AFFILIATION_GRANT_STRICT_NEEDS_SYSTEM: &str =
    "This machine's cross-institution mixing policy is set to 'strict', so accepting a \
     cross-institutional data flow needs your operating system to confirm it is you as well as \
     the in-app approval. That did not happen. Nothing was recorded.";

/// What the `strict` prompt names where a declassification would name its chats.
///
/// ⚠ **A CONSTANT IN THIS SOURCE, and that is a security property rather than a
/// style.** [`biorouter::privacy::system_auth::AuthRequest::about`] is infallible
/// precisely because *"there is exactly one subject, and it is a constant in the
/// caller's source"*, and every prompter renders that slot verbatim. This route
/// is the first caller whose natural subject — an extension name — arrives on an
/// HTTP body and is stored in the agent-writable `config.yaml` (DR-17), so
/// putting it there would let a planted extension choose what the user's system
/// password dialog says. The name is still shown: it goes in the *reason*,
/// through [`biorouter::privacy::system_auth::dialog_safe`].
const CROSS_AFFILIATION_GRANT_AUTH_SUBJECT: &str = "one cross-institution connector";

/// The `strict` layer on the grant, as a function so it can be driven.
///
/// ⚠ **Extracted for [`refuse_grant_unless_user`]'s reason, which is the same
/// reason.** This handler cannot be driven from a test — `AppState::new()` opens
/// the developer's REAL session database — so every other fact about the route
/// is a source scan, and a scan cannot tell "prompts in strict" from "prompts in
/// every mode" or from "prompts in none". Taking the policy and the prompter as
/// arguments makes all three modes assertions about the real decision path, with
/// no password typed. Production passes [`biorouter::privacy::mixing::policy`]
/// and [`biorouter::privacy::system_auth::prompter`], which is the one resolver
/// in the tree that can reach the test seam — pinned by
/// `the_strict_prompt_sits_between_the_resolution_and_the_write`, because a
/// literal in either argument would disable `strict` in production and pass
/// every test in this module.
///
/// ⚠ **`open` and `standard` never touch the prompter.** In `standard` this must
/// be exactly today's behaviour — one in-app confirmation — and a prompt raised
/// there would be the DR-19 prompt fatigue this feature keeps arguing against. In
/// `open` a grant is recordable but pointless (the gate no longer refuses), and
/// charging a password for a no-op is worse than pointless.
async fn strict_mode_authorization(
    policy: biorouter::privacy::mixing::MixingPolicy,
    prompter: &dyn biorouter::privacy::system_auth::SystemAuthenticator,
    extension: &str,
) -> Result<(), ErrorResponse> {
    if policy != biorouter::privacy::mixing::MixingPolicy::Strict {
        return Ok(());
    }
    let named = biorouter::privacy::system_auth::dialog_safe(extension);
    let request = biorouter::privacy::system_auth::AuthRequest::about(
        format!(
            "Allow this Biorouter chat to send data to `{named}`, across an institutional \
             boundary."
        ),
        CROSS_AFFILIATION_GRANT_AUTH_SUBJECT,
    );
    let outcome = prompter.authenticate(&request).await;
    match biorouter::privacy::system_auth::refusal_for(outcome, prompter) {
        None => Ok(()),
        // Nothing has been written at this point — the grant row is the next
        // statement — so the flow stays refused, which is the safe direction.
        Some(refusal) => Err(ErrorResponse {
            status: StatusCode::FORBIDDEN,
            message: format!("{CROSS_AFFILIATION_GRANT_STRICT_NEEDS_SYSTEM} {refusal}"),
        }),
    }
}

/// Record the user's acceptance of one cross-institutional data flow (issue #56,
/// DR-26 / Task 49).
///
/// ⚠ **This route is a reversal of `/agent/add_extension`'s posture and the
/// difference is the ruling, not an inconsistency.** That route refuses a private
/// extension on a public session *outright*, with no user-proof branch, because
/// attaching one is a raise the user cannot authorize either. A Private↔Private
/// cross-institution flow is the opposite case: both endpoints are already
/// private, the tier boundary is not being crossed, and DR-26 states explicitly
/// that legitimate cross-institutional work under a real DUA exists and that the
/// user may accept the stated risk. Blocking it outright is the design
/// researchers route around by turning the feature off. So this route exists, and
/// the tier refusal beside it is untouched.
///
/// The grant is keyed on the **triple** (session, extension, model affiliation),
/// where the affiliation is read by the daemon from the same sample that produced
/// the warning — never from the request. Re-binding to a different institution's
/// model changes the triple, so the acceptance does not carry over.
///
/// ⚠ **The desktop surface is the accept control on the refusal itself** (Task
/// 57): `CrossAffiliationAcceptCard` renders inside the failed tool call that
/// carries `privacy::refusal::CROSS_AFFILIATION_ACCEPT_MARKER`, and posts here
/// with the `X-User-Action` header this route requires. The generated client
/// attaches no header of its own, so every caller must supply one — which is the
/// point: a call without it is refused, and only a surface the user gestured at
/// has one to send.
#[utoipa::path(
    post,
    path = "/agent/cross_affiliation_grant",
    request_body = CrossAffiliationGrantRequest,
    responses(
        (status = 200, description = "The user's acceptance was recorded", body = CrossAffiliationGrantResponse),
        (status = 400, description = "There is no cross-institutional mismatch to accept"),
        (status = 401, description = "Unauthorized - invalid secret key"),
        (status = 403, description = "Refused (issue #56, DR-26): only the user may accept a \
                                      cross-institutional data flow — or (DR-27) this machine's \
                                      mixing policy is 'strict' and the operating system did not \
                                      confirm the user"),
        (status = 424, description = "That chat is not loaded in this daemon, so the model it is \
                                      bound to cannot be read"),
        (status = 500, description = "Internal server error")
    )
)]
async fn agent_cross_affiliation_grant(
    State(state): State<Arc<AppState>>,
    // Before `Json`, which consumes the body and must be last.
    headers: axum::http::HeaderMap,
    Json(request): Json<CrossAffiliationGrantRequest>,
) -> Result<Json<CrossAffiliationGrantResponse>, ErrorResponse> {
    // FIRST, before the agent is fetched or the extension named in the request is
    // resolved. An unproven caller learns nothing about which extensions this chat
    // has, and cannot use the refusals to probe — pinned by
    // `the_guard_runs_before_the_chat_is_touched`.
    //
    // The three-way form rather than `is_user_action`, so a daemon that was handed
    // no key is told something different from a caller that presented no proof —
    // Task 18A's open question 23. The mapping is one function down so it can be
    // driven by a test; see `refuse_grant_unless_user`.
    refuse_grant_unless_user(user_action_proof(&headers))?;

    // PEEK, never `get_agent`. This route inspects a chat; creating one to
    // inspect it reads the process default provider rather than the chat's
    // binding, answers "nothing to accept" for a chat that has plenty, and
    // leaves a bare agent cached under a real session id. See
    // `AppState::peek_agent`.
    let Some(agent) = state.peek_agent(&request.session_id).await else {
        return Err(ErrorResponse {
            status: StatusCode::FAILED_DEPENDENCY,
            message: CROSS_AFFILIATION_GRANT_CHAT_NOT_LOADED.to_string(),
        });
    };

    // ONE sample: the warning the user is accepting and the affiliation the grant
    // is keyed on come from the same read of the bound provider. Two reads would
    // let `update_provider` slip between them and record an acceptance of a
    // sentence the user never saw.
    let Some((affiliation, warning)) = agent
        .cross_affiliation_grant_subject(&request.extension)
        .await
    else {
        return Err(ErrorResponse {
            status: StatusCode::BAD_REQUEST,
            message: CROSS_AFFILIATION_GRANT_NOTHING_TO_ACCEPT.to_string(),
        });
    };

    // Issue #56 Task 52, DR-27. In `strict` the in-app confirmation is not enough
    // on its own: the operating system has to say it is you as well. Raised HERE,
    // immediately before the write, so every other refusal this handler can make
    // is already past and the user is not asked for a password to be told
    // afterwards that there was nothing to accept.
    //
    // ⚠ **The mode is read once, at this one site.** The gate's own three modes
    // are decided in `privacy::affiliation::refusing_mismatch`; this is the
    // second half of DR-27 Step 3, and the only thing on the grant route that
    // knows the policy exists.
    //
    // ⚠ **It widens the window between the ONE sample above and the write below
    // from microseconds to human-scale, and that is accepted rather than
    // overlooked.** `cross_affiliation_grant_subject` exists to take the warning
    // and the affiliation from one `CallCapability`, because `update_provider`
    // reassigns the provider mutex with no turn lock; parking here for a password
    // means the chat can rebind while the user is looking at the dialog. It fails
    // SAFE: the grant is keyed to the affiliation that was sampled, so a grant
    // recorded after a rebind simply never matches and the flow stays refused.
    // The alternative — asking for the password first — takes one and then
    // reports there was nothing to accept, which is the failure DR-20's own
    // header rules out.
    strict_mode_authorization(
        biorouter::privacy::mixing::policy(),
        biorouter::privacy::system_auth::prompter(),
        &request.extension,
    )
    .await?;

    // The single construction of the cross-affiliation proof-of-user in the tree,
    // pinned by
    // `privacy::grant::tests::the_proof_of_user_is_constructed_in_exactly_one_place`.
    // It is minted here, inside the handler that checked the guard above, and
    // nowhere a tool call can reach.
    //
    // ⚠ **Written through THIS AGENT's session manager, not `AppState`'s.** A
    // grant that is not visible to the gate that reads it is a control that
    // silently does nothing, so the write and the read must be the same store —
    // and `Agent::with_config` hands `Arc::clone(&config.session_manager)` to
    // `ExtensionManager::new`, which is the very handle Gate C's
    // `cross_affiliation_denial` queries. Taking it from the agent makes that
    // identity structural. `state.session_manager()` resolves to the same `Arc`
    // today, through `AgentManager`, but only because nothing has ever built an
    // agent on a different one.
    biorouter::privacy::grant::record(
        &agent.config.session_manager,
        &request.session_id,
        &request.extension,
        affiliation,
        &biorouter::privacy::grant::UserCrossAffiliationGrant::from_user_action(),
    )
    .await
    .map_err(|e| {
        error!("Failed to record cross-affiliation grant: {}", e);
        ErrorResponse::internal(format!("Failed to record cross-affiliation grant: {}", e))
    })?;

    Ok(Json(CrossAffiliationGrantResponse {
        // Composed by the module that owns the copy, never here: the dialog that
        // asked and the response that confirms must not differ by a word.
        accepted: biorouter::privacy::grant::accepted_statement(&warning),
    }))
}

#[utoipa::path(
    post,
    path = "/agent/remove_extension",
    request_body = RemoveExtensionRequest,
    responses(
        (status = 200, description = "Extension removed", body = String),
        (status = 401, description = "Unauthorized - invalid secret key"),
        (status = 424, description = "Agent not initialized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn agent_remove_extension(
    State(state): State<Arc<AppState>>,
    Json(request): Json<RemoveExtensionRequest>,
) -> Result<StatusCode, ErrorResponse> {
    let agent = state.get_agent(request.session_id.clone()).await?;
    agent.remove_extension(&request.name).await?;

    agent
        .persist_extension_state(&request.session_id)
        .await
        .map_err(|e| {
            error!("Failed to persist extension state: {}", e);
            ErrorResponse {
                message: format!("Failed to persist extension state: {}", e),
                status: StatusCode::INTERNAL_SERVER_ERROR,
            }
        })?;

    Ok(StatusCode::OK)
}

#[utoipa::path(
    post,
    path = "/agent/stop",
    request_body = StopAgentRequest,
    responses(
        (status = 200, description = "Agent stopped successfully", body = String),
        (status = 401, description = "Unauthorized - invalid secret key"),
        (status = 404, description = "Session not found"),
        (status = 500, description = "Internal server error")
    )
)]
async fn stop_agent(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<StopAgentRequest>,
) -> Result<StatusCode, ErrorResponse> {
    let session_id = payload.session_id;

    // BR-62: stop the *turn*, not just the agent entry. Evicting the session from
    // the LRU below drops the manager's handle, but an in-flight `/reply` task
    // holds its own `Arc<Agent>` and kept running — so "stop" left a turn burning
    // tokens and streaming into a socket nobody was reading. Trip the running
    // turn's cancellation token first; the reply task then unwinds and releases
    // the turn lock. No-op when nothing is running.
    if let Some(turn_id) = state.cancel_turn(&session_id) {
        tracing::info!(
            "Stop for session {} cancelled in-flight turn {}",
            session_id,
            turn_id
        );
    }

    state
        .agent_manager
        .remove_session(&session_id)
        .await
        .map_err(|e| ErrorResponse {
            message: format!("Failed to stop agent for session {}: {}", session_id, e),
            status: StatusCode::NOT_FOUND,
        })?;

    Ok(StatusCode::OK)
}

async fn restart_agent_internal(
    state: &Arc<AppState>,
    session_id: &str,
    session: &Session,
) -> Result<Vec<ExtensionLoadResult>, ErrorResponse> {
    // Remove existing agent (ignore error if not found)
    let _ = state.agent_manager.remove_session(session_id).await;

    let agent = state
        .get_agent_for_route(session_id.to_string())
        .await
        .map_err(|code| ErrorResponse {
            message: "Failed to create new agent during restart".into(),
            status: code,
        })?;

    let provider_future = agent.restore_provider_from_session(session);
    let extensions_future = agent.load_extensions_from_session(session);

    let (provider_result, extension_results) = tokio::join!(provider_future, extensions_future);
    provider_result.map_err(|e| ErrorResponse {
        message: e.to_string(),
        status: StatusCode::INTERNAL_SERVER_ERROR,
    })?;

    let context: HashMap<&str, Value> = HashMap::new();
    let desktop_prompt =
        render_global_file("desktop_prompt.md", &context).expect("Prompt should render");
    let mut update_prompt = desktop_prompt;

    if let Some(ref workflow) = session.workflow {
        match build_workflow_with_parameter_values(
            workflow,
            session.user_workflow_values.clone().unwrap_or_default(),
        )
        .await
        {
            Ok(Some(workflow)) => {
                if let Some(prompt) = apply_workflow_to_agent(&agent, &workflow, true).await {
                    update_prompt = prompt;
                }
            }
            Ok(None) => {
                // Workflow has missing parameters - use default prompt
            }
            Err(e) => {
                return Err(ErrorResponse {
                    message: e.to_string(),
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                });
            }
        }
    }
    agent.extend_system_prompt(update_prompt).await;

    Ok(extension_results)
}

#[utoipa::path(
    post,
    path = "/agent/restart",
    request_body = RestartAgentRequest,
    responses(
        (status = 200, description = "Agent restarted successfully", body = RestartAgentResponse),
        (status = 401, description = "Unauthorized - invalid secret key"),
        (status = 404, description = "Session not found"),
        (status = 500, description = "Internal server error")
    )
)]
async fn restart_agent(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RestartAgentRequest>,
) -> Result<Json<RestartAgentResponse>, ErrorResponse> {
    let session_id = payload.session_id.clone();

    let session = state
        .session_manager()
        .get_session(&session_id, false)
        .await
        .map_err(|err| {
            error!("Failed to get session during restart: {}", err);
            ErrorResponse {
                message: format!("Failed to get session: {}", err),
                status: StatusCode::NOT_FOUND,
            }
        })?;

    let extension_results = restart_agent_internal(&state, &session_id, &session).await?;

    Ok(Json(RestartAgentResponse { extension_results }))
}

/// Validate and apply a working-directory change for `session_id`, enforcing
/// the empty-chat-only rule (#44): the working directory is choosable only
/// while the chat is completely empty and immutable once it has any messages.
/// A mid-chat switch breaks the session's own history — file references
/// printed earlier re-resolve against the new root — so a started session
/// rejects the change with 409 CONFLICT, as defense in depth against stale or
/// alternative clients. Returns the updated session so the caller can restart
/// the agent from it.
///
/// Concurrency: the emptiness check and the write are one atomic conditional
/// `UPDATE` ([`SessionManager::try_update_working_dir_if_empty`]), so a first
/// message landing concurrently can never slip between a check and a write.
/// That closes the persisted-state race only; the HTTP handler additionally
/// holds the per-session turn guard across the update + agent restart so a
/// restart can't race an in-flight `/reply` either.
pub(crate) async fn apply_working_dir_update(
    session_manager: &SessionManager,
    session_id: &str,
    working_dir: &str,
) -> Result<Session, ErrorResponse> {
    let working_dir = working_dir.trim();

    if working_dir.is_empty() {
        return Err(ErrorResponse {
            message: "Working directory cannot be empty".into(),
            status: StatusCode::BAD_REQUEST,
        });
    }

    let path = PathBuf::from(working_dir);
    if !path.exists() || !path.is_dir() {
        return Err(ErrorResponse {
            message: "Invalid directory path".into(),
            status: StatusCode::BAD_REQUEST,
        });
    }

    match session_manager
        .try_update_working_dir_if_empty(session_id, path)
        .await
    {
        Ok(WorkingDirUpdate::Updated) => {}
        Ok(WorkingDirUpdate::RefusedNotEmpty) => {
            return Err(ErrorResponse {
                message: "the working directory is fixed once a chat has messages".into(),
                status: StatusCode::CONFLICT,
            });
        }
        Ok(WorkingDirUpdate::SessionNotFound) => {
            return Err(ErrorResponse {
                message: format!("Session not found: {}", session_id),
                status: StatusCode::NOT_FOUND,
            });
        }
        Err(e) => {
            error!("Failed to update session working directory: {}", e);
            return Err(ErrorResponse {
                message: format!("Failed to update working directory: {}", e),
                status: StatusCode::INTERNAL_SERVER_ERROR,
            });
        }
    }

    session_manager
        .get_session(session_id, false)
        .await
        .map_err(|err| {
            error!("Failed to get session after working dir update: {}", err);
            ErrorResponse {
                message: format!("Failed to get session: {}", err),
                status: StatusCode::NOT_FOUND,
            }
        })
}

#[utoipa::path(
    post,
    path = "/agent/update_working_dir",
    request_body = UpdateWorkingDirRequest,
    responses(
        (status = 200, description = "Working directory updated and agent restarted successfully"),
        (status = 400, description = "Bad request - invalid directory path"),
        (status = 401, description = "Unauthorized - invalid secret key"),
        (status = 403, description = "Refused by a privacy boundary (issue #56 Task 58 / #47): \
                                      the named chat is private (or absent — an unproven caller \
                                      is told the same thing for both) and the request carried no \
                                      proof it came from the user"),
        (status = 404, description = "Session not found"),
        (
            status = 409,
            description = "Conflict - the working directory is fixed once a chat has messages, or a turn is in flight"
        ),
        (status = 500, description = "Internal server error")
    )
)]
async fn update_working_dir(
    State(state): State<Arc<AppState>>,
    // Before `Json`, which consumes the body and must be last.
    headers: axum::http::HeaderMap,
    Json(payload): Json<UpdateWorkingDirRequest>,
) -> Result<StatusCode, ErrorResponse> {
    let session_id = payload.session_id.clone();

    // Issue #56 Task 58 / #47. Before the turn lock, whose 409 tells an
    // unproven caller whether the chat it named is busy — the disclosure the
    // refusal is worded to withhold. This route repoints a chat at a directory
    // of the caller's choosing and restarts its agent there; `session_id` is a
    // request parameter, not a credential. See `routes::session_reach`.
    crate::routes::session_reach::session_reach(state.session_manager(), &session_id, &headers)
        .await?;

    // Serialize with `/reply`'s per-session turn lock (BR-33) by claiming the
    // turn slot for the whole update + restart. Without it, a first message
    // accepted (but not yet persisted) by an in-flight reply could pass the
    // emptiness check here, and that reply would then keep streaming against
    // the old agent while the session already points at the new dir. Holding
    // the guard means a reply either finished (its message makes the update
    // 409) or has not begun (it will find the restarted agent); a turn in
    // flight refuses the switch outright.
    let _turn_guard = state
        .try_begin_turn_idempotent(&session_id, CancellationToken::new(), None)
        .map_err(|_conflict| ErrorResponse {
            message: "cannot change the working directory while a turn is in progress".into(),
            status: StatusCode::CONFLICT,
        })?;

    let session =
        apply_working_dir_update(state.session_manager(), &session_id, &payload.working_dir)
            .await?;

    restart_agent_internal(&state, &session_id, &session).await?;

    Ok(StatusCode::OK)
}

#[utoipa::path(
    post,
    path = "/agent/read_resource",
    request_body = ReadResourceRequest,
    responses(
        (status = 200, description = "Resource read successfully", body = ReadResourceResponse),
        (status = 401, description = "Unauthorized - invalid secret key"),
        (status = 424, description = "Agent not initialized"),
        (status = 404, description = "Resource not found"),
        (status = 500, description = "Internal server error")
    )
)]
async fn read_resource(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ReadResourceRequest>,
) -> Result<Json<ReadResourceResponse>, StatusCode> {
    use rmcp::model::ResourceContents;

    let agent = state
        .get_agent_for_route(payload.session_id.clone())
        .await?;

    let read_result = agent
        .extension_manager
        .read_resource(
            &payload.uri,
            &payload.extension_name,
            // Issue #56: a route, not a tool call — there is no admitted turn
            // whose capability this could inherit, so Gate C's sibling guard
            // takes its own reading of the session's bound model.
            None,
            CancellationToken::default(),
        )
        .await
        .map_err(|_e| StatusCode::INTERNAL_SERVER_ERROR)?;

    let content = read_result
        .contents
        .into_iter()
        .next()
        .ok_or(StatusCode::NOT_FOUND)?;

    let (uri, mime_type, text, meta) = match content {
        ResourceContents::TextResourceContents {
            uri,
            mime_type,
            text,
            meta,
        } => (uri, mime_type, text, meta),
        ResourceContents::BlobResourceContents {
            uri,
            mime_type,
            blob,
            meta,
        } => {
            let decoded = match base64::engine::general_purpose::STANDARD.decode(&blob) {
                Ok(bytes) => {
                    String::from_utf8(bytes).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                }
                Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
            };
            (uri, mime_type, decoded, meta)
        }
    };

    let meta_map = meta.map(|m| m.0);

    Ok(Json(ReadResourceResponse {
        uri,
        mime_type,
        text,
        meta: meta_map,
    }))
}

/// Classify a dispatch failure: a **tool** error the caller can act on, or a
/// server fault that keeps its 500.
///
/// Extracted so the mapping can be tested without `AppState`, which builds the
/// process-global `AgentManager` and opens the REAL user session database. It
/// mirrors the agent loop, which downcasts to `ErrorData` to avoid
/// double-wrapping and hands the model the message.
///
/// ⚠ **The downcast is the whole classification, and it is not a formality.**
/// Every refusal `dispatch_tool_call` itself produces — Gate C's, `Tool '…' not
/// found`, `not available for extension`, BR-23's secret denial — is an
/// `ErrorData`, so all of them reach the caller as a tool result, which is the
/// fix this task owes: `.map_err(|_| INTERNAL_SERVER_ERROR)` threw Gate C's
/// refusal away and told the caller Biorouter had crashed.
///
/// Anything that is *not* an `ErrorData` is a fault of this process rather than
/// an answer about the tool, and Gate C introduced the first one: the O5 ratchet
/// propagates a session-store failure with `?`. Rendering that as `200 +
/// is_error` would tell an HTTP client the tool ran and disagreed, retire its
/// retry, and hand it raw store text. It keeps the 500 it had before this task.
fn dispatch_failure_response(error: &anyhow::Error) -> Result<CallToolResponse, StatusCode> {
    let Some(data) = error.downcast_ref::<rmcp::model::ErrorData>() else {
        tracing::error!(%error, "call_tool dispatch failed for a non-tool reason");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    };
    Ok(CallToolResponse {
        content: vec![Content::text(data.message.to_string())],
        structured_content: None,
        is_error: true,
        _meta: None,
    })
}

#[utoipa::path(
    post,
    path = "/agent/call_tool",
    request_body = CallToolRequest,
    responses(
        (status = 200, description = "Resource read successfully", body = CallToolResponse),
        (status = 401, description = "Unauthorized - invalid secret key"),
        (status = 424, description = "Agent not initialized"),
        (status = 404, description = "Resource not found"),
        (status = 500, description = "Internal server error")
    )
)]
async fn call_tool(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CallToolRequest>,
) -> Result<Json<CallToolResponse>, StatusCode> {
    let arguments = match payload.arguments {
        Value::Object(map) => Some(map),
        _ => None,
    };

    // Issue #63 review, finding 3. This route hands a tool call straight to the
    // extension manager, bypassing the agent loop and therefore every
    // `ToolInspector` — including the machine-wide memory consent gate. Nothing
    // in an HTTP handler can put an operation to the user and wait, so a global
    // memory operation is refused here rather than performed unasked. It is
    // returned as a tool error, not a status code, because the caller is a tool
    // caller and the remedy is in the text.
    //
    // Ahead of resolving the agent on purpose: whether this call is allowed does
    // not depend on any session state, and a decision that cannot be reached
    // without one is a decision that can be skipped by arriving without one.
    if let Some(refusal) = biorouter::security::global_memory::uninspected_boundary_refusal(
        &payload.name,
        arguments.as_ref(),
        biorouter::security::global_memory::UninspectedBoundary::AgentCallToolRoute,
    ) {
        return Ok(Json(CallToolResponse {
            content: vec![Content::text(refusal)],
            structured_content: None,
            is_error: true,
            _meta: None,
        }));
    }

    let agent = state
        .get_agent_for_route(payload.session_id.clone())
        .await?;

    let tool_call = CallToolRequestParams {
        task: None,
        name: payload.name.into(),
        arguments,
        meta: None,
    };

    // Issue #56: this entry has NO caller identity. It arrives outside the agent
    // loop, so there is no admitted turn whose capability it could inherit, and
    // the session's currently-bound provider is not this caller's — reading it
    // would hand an HTTP client whatever reach the user's chat happens to have.
    // Public + enforced is the most restrictive pair, and it is a constant, so
    // there is nothing here to race with `update_provider`.
    let tool_result = match agent
        .extension_manager
        .dispatch_tool_call(
            &payload.session_id,
            tool_call,
            biorouter::privacy::CallCapability::public_enforced(),
            CancellationToken::default(),
        )
        .await
    {
        Ok(result) => result,
        // Issue #56 Gate C: this used to be
        // `.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)`, which threw the
        // refusal away and told the caller that Biorouter had crashed. A tool
        // that could not be dispatched is a tool error and the remedy is in the
        // text, exactly as the agent loop treats it — see
        // [`dispatch_failure_response`] for the one class this does NOT cover.
        Err(error) => return dispatch_failure_response(&error).map(Json),
    };

    let result = tool_result
        .result
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(CallToolResponse {
        content: result.content,
        structured_content: result.structured_content,
        is_error: result.is_error.unwrap_or(false),
        _meta: result.meta.and_then(|m| serde_json::to_value(m).ok()),
    }))
}

#[derive(Deserialize, utoipa::IntoParams, utoipa::ToSchema)]
pub struct ListAppsRequest {
    session_id: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListAppsResponse {
    pub apps: Vec<BioRouterApp>,
}

#[utoipa::path(
    get,
    path = "/agent/list_apps",
    params(
        ListAppsRequest
    ),
    responses(
        (status = 200, description = "List of apps retrieved successfully", body = ListAppsResponse),
        (status = 401, description = "Unauthorized - Invalid or missing API key", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    ),
    security(
        ("api_key" = [])
    ),
    tag = "Agent"
)]
async fn list_apps(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListAppsRequest>,
) -> Result<Json<ListAppsResponse>, ErrorResponse> {
    let cache = McpAppCache::new().ok();

    let Some(session_id) = params.session_id else {
        let apps = cache
            .as_ref()
            .and_then(|c| c.list_apps().ok())
            .unwrap_or_default();
        return Ok(Json(ListAppsResponse { apps }));
    };

    let agent = state
        .get_agent_for_route(session_id)
        .await
        .map_err(|status| ErrorResponse {
            message: "Failed to get agent".to_string(),
            status,
        })?;

    let apps = fetch_mcp_apps(&agent.extension_manager)
        .await
        .map_err(|e| ErrorResponse {
            message: format!("Failed to list apps: {}", e.message),
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;

    if let Some(cache) = cache.as_ref() {
        let active_extensions: std::collections::HashSet<String> = apps
            .iter()
            .filter_map(|app| app.mcp_server.clone())
            .collect();

        for extension_name in active_extensions {
            if let Err(e) = cache.delete_extension_apps(&extension_name) {
                warn!(
                    "Failed to clean cache for extension {}: {}",
                    extension_name, e
                );
            }
        }

        for app in &apps {
            if let Err(e) = cache.store_app(app) {
                warn!("Failed to cache app {}: {}", app.resource.name, e);
            }
        }
    }

    Ok(Json(ListAppsResponse { apps }))
}

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/agent/start", post(start_agent))
        .route("/agent/resume", post(resume_agent))
        .route("/agent/restart", post(restart_agent))
        .route("/agent/update_working_dir", post(update_working_dir))
        .route("/agent/tools", get(get_tools))
        .route("/agent/read_resource", post(read_resource))
        .route("/agent/call_tool", post(call_tool))
        .route("/agent/list_apps", get(list_apps))
        .route("/agent/update_provider", post(update_agent_provider))
        .route("/agent/update_from_session", post(update_from_session))
        .route("/agent/add_extension", post(agent_add_extension))
        .route(
            "/agent/cross_affiliation_grant",
            post(agent_cross_affiliation_grant),
        )
        .route("/agent/remove_extension", post(agent_remove_extension))
        .route("/agent/stop", post(stop_agent))
        .with_state(state)
}

#[cfg(test)]
mod cross_affiliation_grant_route_tests {
    //! Issue #56 DR-26 / Task 49: what the grant route's four refusals may and
    //! may not say, and the one shape of the handler that cannot regress
    //! silently.
    //!
    //! The behaviour these guard is not testable at the HTTP layer here —
    //! `AppState::new()` opens the developer's REAL session database (see
    //! `working_dir_lock_tests`) — so the copy is asserted directly, the way
    //! `routes::session`'s `the_refusals_say_different_things` asserts the
    //! declassification pair.

    use super::{
        CROSS_AFFILIATION_GRANT_CHAT_NOT_LOADED, CROSS_AFFILIATION_GRANT_NEEDS_USER,
        CROSS_AFFILIATION_GRANT_NOTHING_TO_ACCEPT, CROSS_AFFILIATION_GRANT_NO_KEY,
    };
    use axum::http::StatusCode;

    const SOURCE: &str = include_str!("agent.rs");

    /// ⚠ **The grant route INSPECTS a chat, and an inspection must never mint
    /// one.** `AppState::get_agent` is `AgentManager::get_or_create_agent`,
    /// whose own sibling `peek_agent` documents the hazard verbatim: it reads
    /// the process-wide mode at creation time and leaves a bare, provider-less,
    /// extension-less agent cached under that session id.
    ///
    /// Three things go wrong on that miss path, and all three are silent. The
    /// minted agent enumerates no extensions, so a user who just watched a
    /// refusal in that very chat is told there is nothing to accept — the
    /// control is unusable in exactly the case it is needed, after a daemon
    /// restart or an LRU eviction. Its provider is the process default rather
    /// than the chat's, so the "one sample" the whole route is built around
    /// would be a sample of the wrong model's affiliation. And the bare agent
    /// stays cached under a real session id for whoever asks next.
    ///
    /// Both directions of the negative control, because `body_of` reads forward
    /// from a signature to the next column-0 `}`: `agent_add_extension` sits
    /// BEFORE this handler in the file and `agent_remove_extension` AFTER, and
    /// each legitimately creates. An over-reading extractor reports one of them
    /// as peeking, or reports this handler as creating.
    #[test]
    fn the_grant_route_looks_up_the_live_agent_and_never_creates_one() {
        // Assembled, so this assertion is not itself a match for the scan.
        let peek = concat!("peek", "_agent(");
        let create = concat!("get", "_agent(");

        let handler = crate::routes::body_of(SOURCE, "async fn agent_cross_affiliation_grant");
        assert!(
            handler.contains(peek),
            "the grant route no longer looks the chat up without creating it"
        );
        assert!(
            !handler.contains(create),
            "the grant route mints an agent to inspect a chat. On a miss that agent has the \
             process default provider and no extensions, so the affiliation the grant is keyed \
             on is the wrong model's and the user is told there is nothing to accept."
        );

        for (name, body) in [
            (
                "agent_add_extension",
                crate::routes::body_of(SOURCE, "async fn agent_add_extension"),
            ),
            (
                "agent_remove_extension",
                crate::routes::body_of(SOURCE, "async fn agent_remove_extension"),
            ),
        ] {
            assert!(
                body.contains(create),
                "{name} is the control for this scan and must still create on miss"
            );
            assert!(
                !body.contains(peek),
                "the body scan is over-reading: {name} reported the grant route's lookup"
            );
        }
    }

    /// ⚠ **The one piece of this route's behaviour that IS reachable from a
    /// test, so it is asserted rather than grepped for.**
    ///
    /// Everything else here is a source scan, for the reason the module header
    /// gives. That leaves the most important claim of all — *only the user may
    /// grant* — resting on `handler.contains("user_action_proof(")`, which
    /// survives a refactor that turns the `Unproven` arm into `=> {}`. The
    /// mapping from a proof verdict to a refusal is pure, so it lives in
    /// [`super::refuse_grant_unless_user`] and this drives all three arms.
    #[test]
    fn only_a_proven_user_action_gets_past_the_grant_guard() {
        // Through the LIB path, as the handler above imports it: this module is
        // compiled into the `biorouterd` binary too, where `crate::auth` does
        // not exist.
        use biorouter_server::auth::UserActionProof;

        super::refuse_grant_unless_user(UserActionProof::Proven)
            .expect("a proven user action is the one verdict that proceeds");

        let unproven = super::refuse_grant_unless_user(UserActionProof::Unproven)
            .expect_err("a caller with no proof of a human may not accept a compliance risk");
        assert_eq!(unproven.status, StatusCode::FORBIDDEN);
        assert_eq!(unproven.message, CROSS_AFFILIATION_GRANT_NEEDS_USER);

        let keyless = super::refuse_grant_unless_user(UserActionProof::NoKeyInstalled)
            .expect_err("a daemon that cannot verify a human refuses everyone, including them");
        assert_eq!(keyless.status, StatusCode::FORBIDDEN);
        assert_eq!(keyless.message, CROSS_AFFILIATION_GRANT_NO_KEY);
    }

    /// Task 52 gate (1), the half that lives on this route: **`strict` rejects a
    /// grant carrying only `X-User-Action`**, and the other two modes accept
    /// one.
    ///
    /// The prompter counts, because the two ways to get this wrong are opposite
    /// and a test that only checked the `strict` outcome would miss the other:
    /// prompting in `standard` is the DR-19 prompt fatigue this feature keeps
    /// arguing against, and prompting in none is `strict` not existing.
    #[tokio::test]
    async fn strict_demands_the_operating_system_on_top_of_the_header() {
        use biorouter::privacy::mixing::MixingPolicy;
        use biorouter::privacy::system_auth::{AuthOutcome, AuthRequest, SystemAuthenticator};
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct Prompter {
            answer: AuthOutcome,
            prompts: AtomicUsize,
        }

        #[async_trait::async_trait]
        impl SystemAuthenticator for Prompter {
            async fn authenticate(&self, _req: &AuthRequest) -> AuthOutcome {
                self.prompts.fetch_add(1, Ordering::Relaxed);
                self.answer
            }

            fn platform(&self) -> &'static str {
                "counting test prompter"
            }
        }

        // `open` and `standard` are today's behaviour: the in-app confirmation
        // this handler already checked is the whole of it, and a prompter that
        // would refuse is never consulted.
        for mode in [MixingPolicy::Open, MixingPolicy::Standard] {
            let prompter = Prompter {
                answer: AuthOutcome::Denied,
                prompts: AtomicUsize::new(0),
            };
            super::strict_mode_authorization(mode, &prompter, "ucsfomopagent")
                .await
                .unwrap_or_else(|e| panic!("{mode} must not need a password: {}", e.message));
            assert_eq!(
                prompter.prompts.load(Ordering::Relaxed),
                0,
                "{mode} raised a system prompt. Only `strict` costs something real; making \
                 the common case expensive is how you teach people to stop using the control"
            );
        }

        // `strict`: the same request, with the same header, is refused.
        let denied = Prompter {
            answer: AuthOutcome::Denied,
            prompts: AtomicUsize::new(0),
        };
        let refusal =
            super::strict_mode_authorization(MixingPolicy::Strict, &denied, "ucsfomopagent")
                .await
                .expect_err("in `strict` the in-app confirmation is not enough on its own");
        assert_eq!(refusal.status, StatusCode::FORBIDDEN);
        assert_eq!(denied.prompts.load(Ordering::Relaxed), 1);
        assert!(
            refusal.message.contains("strict"),
            "the refusal must name the mode in force, or the user reads it as a bug: {}",
            refusal.message
        );
        assert!(
            refusal.message.contains("Nothing was recorded"),
            "{}",
            refusal.message
        );

        // …and clears once the operating system approves.
        let approved = Prompter {
            answer: AuthOutcome::Approved,
            prompts: AtomicUsize::new(0),
        };
        super::strict_mode_authorization(MixingPolicy::Strict, &approved, "ucsfomopagent")
            .await
            .expect("an approved system authentication is what `strict` costs");
        assert_eq!(approved.prompts.load(Ordering::Relaxed), 1);

        // A machine that cannot raise a prompt at all refuses rather than
        // approving — DR-24's asymmetry, unchanged.
        let unavailable = Prompter {
            answer: AuthOutcome::Unavailable,
            prompts: AtomicUsize::new(0),
        };
        let error =
            super::strict_mode_authorization(MixingPolicy::Strict, &unavailable, "ucsfomopagent")
                .await
                .expect_err("an unavailable prompter must not approve a compliance risk");
        assert!(
            error.message.contains("counting test prompter"),
            "an unavailable prompt must name the platform: {}",
            error.message
        );
    }

    /// **Request-supplied text may not be rendered verbatim into an operating
    /// system's authentication dialog** (review finding, Task 52 fixup).
    ///
    /// This is the first caller in the tree to hand
    /// [`biorouter::privacy::system_auth::AuthRequest::about`] something that did
    /// not come from its own source, and that function's doc states the opposite
    /// as its precondition: it is infallible *"because there is exactly one
    /// subject, and it is a constant in the caller's source."* Extension names
    /// live in `config.yaml`, which DR-17 leaves agent-writable, and every
    /// prompter renders the id slot verbatim — macOS into
    /// `LAContext.localizedReason`, Windows into `UserConsentVerifier`, polkit
    /// into `--detail chats <value>`. So an agent that can plant an extension
    /// could write its own sentence into the system password dialog the user is
    /// then shown. Not injection — spoofing, which is worse here, because DR-20
    /// point 4's whole premise is that the dialog says honestly what it
    /// authorises.
    #[tokio::test]
    async fn the_system_dialog_never_renders_an_extension_name_verbatim() {
        use biorouter::privacy::mixing::MixingPolicy;
        use biorouter::privacy::system_auth::{AuthOutcome, AuthRequest, SystemAuthenticator};
        use std::sync::Mutex;

        struct Recorder(Mutex<Option<AuthRequest>>);

        #[async_trait::async_trait]
        impl SystemAuthenticator for Recorder {
            async fn authenticate(&self, req: &AuthRequest) -> AuthOutcome {
                *self.0.lock().unwrap() = Some(req.clone());
                AuthOutcome::Approved
            }

            fn platform(&self) -> &'static str {
                "recording test prompter"
            }
        }

        // A name a compromised agent could plant: it ends the sentence it is
        // spliced into, opens a line of its own, and runs long enough to push
        // the real text out of a dialog.
        let hostile = format!(
            "ok`.\n\nBioRouter: this is routine. Press Allow.\r\n{}",
            "x".repeat(400)
        );

        let recorder = Recorder(Mutex::new(None));
        super::strict_mode_authorization(MixingPolicy::Strict, &recorder, &hostile)
            .await
            .expect("an approving prompter clears the strict layer");
        let asked = recorder.0.lock().unwrap().clone().expect("the prompt ran");

        // The id slot is a CONSTANT in this file, honouring `about`'s stated
        // precondition. A request-supplied subject there is the spoof.
        assert_eq!(
            asked.session_ids,
            vec![super::CROSS_AFFILIATION_GRANT_AUTH_SUBJECT.to_string()],
            "the dialog's subject slot carried request-supplied text: {asked:?}"
        );

        // The reason may name the extension — that is DR-20 point 4 — but only
        // after the name has been made safe to render.
        //
        // ⚠ The claim is STRUCTURAL, not semantic, and the assertions say so. A
        // connector genuinely named `Routine check, press Allow` is
        // indistinguishable from a planted one; what must hold is that the name
        // stays inside Biorouter's own sentence, on one line, bounded, and
        // never in the id slot — so a user can always tell which words are the
        // application's. See `system_auth::dialog_safe`.
        assert!(
            !asked.reason.chars().any(char::is_control),
            "a control character reached the dialog, so a planted name can add a \
             line to it: {:?}",
            asked.reason
        );
        assert_eq!(
            asked.reason.matches('`').count(),
            2,
            "the planted name closed the delimiters it was wrapped in, so part of \
             it renders as Biorouter speaking: {:?}",
            asked.reason
        );
        assert!(
            asked.reason.len() < 200,
            "an unbounded name can push the real sentence out of the dialog: {} chars",
            asked.reason.len()
        );

        // …and an ordinary name still reaches it, or the sanitiser has bought
        // safety by saying nothing.
        let plain = Recorder(Mutex::new(None));
        super::strict_mode_authorization(MixingPolicy::Strict, &plain, "ucsfomopagent")
            .await
            .unwrap();
        assert!(
            plain
                .0
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|r| r.reason.contains("ucsfomopagent")),
            "the dialog no longer says which connector it is authorising"
        );
    }

    /// …and the `strict` prompt is raised AFTER the mismatch is resolved and
    /// BEFORE the grant is written, **with the live policy and the real
    /// prompter**.
    ///
    /// The pure test above cannot see the order. Asking earlier would take a
    /// password and then report there was nothing to accept; asking later would
    /// take one for a row already written.
    ///
    /// ⚠ **Nor can it see the arguments, which is the mutation review found.**
    /// [`super::strict_mode_authorization`] is testable precisely because it
    /// takes the mode and the prompter rather than resolving them — and that is
    /// also how `strict` could be silently switched off:
    ///
    /// ```ignore
    /// strict_mode_authorization(MixingPolicy::Open, prompter(), &request.extension)
    /// ```
    ///
    /// …passes every behavioural test in this module, both gate commands and the
    /// ordering scan above, while no machine ever raises the prompt again. So the
    /// production call's own text is asserted: the mode comes from the resolver
    /// and the prompter from DR-24's, neither from a literal.
    #[test]
    fn the_strict_prompt_sits_between_the_resolution_and_the_write() {
        let handler = crate::routes::body_of(SOURCE, "async fn agent_cross_affiliation_grant");
        let resolved = handler
            .find("cross_affiliation_grant_subject(")
            .expect("the grant handler no longer resolves the mismatch it is accepting");
        let prompted = handler
            .find(concat!("strict_mode_", "authorization("))
            .expect("the grant handler no longer applies DR-27's strict layer");
        let written = handler
            .find(concat!("grant::", "record("))
            .expect("the grant handler no longer writes the grant");
        assert!(
            resolved < prompted && prompted < written,
            "the strict prompt is outside the window it has to sit in: resolved at \
             {resolved}, prompted at {prompted}, written at {written}"
        );

        // The call runs from `strict_mode_authorization(` to the `.await?` that
        // ends the statement. Line-wise rather than by byte slice, for the reason
        // `mixing.rs`'s writer audit records: `clippy::string_slice` is
        // warn-by-default here and `-D warnings` makes it an error.
        let mut call = String::new();
        let mut inside = false;
        for line in handler.lines() {
            inside |= line.contains(concat!("strict_mode_", "authorization("));
            if inside {
                call.push_str(line);
                call.push('\n');
                if line.contains(".await?") {
                    break;
                }
            }
        }
        assert!(
            call.contains(".await?"),
            "the strict layer's call no longer ends where this scan expects: {call}"
        );
        assert!(
            call.contains(concat!("mixing::", "policy()")),
            "the strict layer is no longer handed the LIVE mixing policy, so a \
             machine in `strict` may never be asked: {call}"
        );
        assert!(
            call.contains(concat!("system_auth::", "prompter()")),
            "the strict layer is no longer handed DR-24's real prompter: {call}"
        );
    }

    /// …and the guard runs BEFORE the chat is looked up, so an unproven caller
    /// cannot use the refusals to probe which extensions a chat has.
    ///
    /// The pure test above cannot see the order; only the source can.
    #[test]
    fn the_guard_runs_before_the_chat_is_touched() {
        let handler = crate::routes::body_of(SOURCE, "async fn agent_cross_affiliation_grant");
        let guarded = handler
            .find(concat!("refuse_grant_unless_user(", "user_action_proof("))
            .expect(
                "the grant handler no longer passes the real guard's verdict to the refusal \
                 mapping",
            );
        let looked_up = handler
            .find(concat!("peek", "_agent("))
            .expect("the grant handler no longer looks the chat up");
        assert!(
            guarded < looked_up,
            "the proof is checked AFTER the chat is inspected, so an unproven caller can \
             distinguish a chat with the extension from one without"
        );
    }

    /// …and the refusal that miss produces says what actually happened.
    ///
    /// Reporting "this chat is not loaded" as "there is nothing to accept" is
    /// the same class of error open question 23 rejected for the keyless daemon:
    /// it sends the person at the keyboard looking for a mismatch to fix when
    /// what they need is to open the chat.
    #[test]
    fn a_chat_that_is_not_loaded_is_told_so_and_not_that_there_is_nothing_to_accept() {
        assert_ne!(
            CROSS_AFFILIATION_GRANT_CHAT_NOT_LOADED,
            CROSS_AFFILIATION_GRANT_NOTHING_TO_ACCEPT
        );
        assert!(CROSS_AFFILIATION_GRANT_CHAT_NOT_LOADED.contains("Nothing was recorded"));
        // It names the act that clears it, which is neither a retry nor a
        // setting: open the chat again.
        assert!(CROSS_AFFILIATION_GRANT_CHAT_NOT_LOADED.contains("Open the chat"));
    }

    /// A refusal that carries a renderer marker is claiming to be a different
    /// refusal, and the toast it triggers sends the user somewhere that cannot
    /// help: `USER_ACTION_REFUSAL_MARKER`'s says *switch this chat's model* (this
    /// chat is already private), and `COPY_OF_PRIVATE_REFUSAL_MARKER`'s says
    /// *branch it from the chat window* (nothing is being branched).
    #[test]
    fn no_grant_refusal_claims_another_refusals_marker() {
        for message in [
            CROSS_AFFILIATION_GRANT_NEEDS_USER,
            CROSS_AFFILIATION_GRANT_NO_KEY,
            CROSS_AFFILIATION_GRANT_NOTHING_TO_ACCEPT,
            // Task 52's fifth refusal, enrolled here rather than left to be
            // remembered: it is the newest and therefore the likeliest to have
            // been written by copying one of the others.
            super::CROSS_AFFILIATION_GRANT_STRICT_NEEDS_SYSTEM,
        ] {
            assert!(
                !message.contains(biorouter::privacy::refusal::USER_ACTION_REFUSAL_MARKER),
                "this refusal is claiming to be the model picker's: {message}"
            );
            assert!(
                !message.contains(super::super::session::COPY_OF_PRIVATE_REFUSAL_MARKER),
                "this refusal is claiming to be a copy handler's: {message}"
            );
        }
    }

    /// The two proof refusals say different things, which is the whole reason
    /// Task 49 takes the three-way `user_action_proof` over the boolean:
    /// reporting "this daemon cannot verify a human" as "you are not a human"
    /// sends the person at the keyboard hunting for a permission they can never
    /// obtain (open question 23).
    #[test]
    fn a_keyless_daemon_is_told_something_a_caller_with_no_proof_is_not() {
        assert_ne!(
            CROSS_AFFILIATION_GRANT_NEEDS_USER,
            CROSS_AFFILIATION_GRANT_NO_KEY
        );
        assert!(CROSS_AFFILIATION_GRANT_NO_KEY.contains("without a user-action key"));
        // …and neither of them accuses the user of being the model.
        assert!(CROSS_AFFILIATION_GRANT_NEEDS_USER.contains("person at the keyboard"));
    }

    /// Every refusal in this feature forecloses the retry, because a model that
    /// reads one as transient loops on it — and this one also has to name the
    /// human act, since the human act is the entire product of DR-26.
    #[test]
    fn the_proof_refusal_forecloses_the_retry_and_names_the_human_act() {
        let m = CROSS_AFFILIATION_GRANT_NEEDS_USER;
        assert!(m.contains("Do not"), "{m}");
        assert!(m.contains("Nothing was recorded"), "{m}");
        assert!(m.contains("let them approve it"), "{m}");
    }
}

#[cfg(test)]
mod privacy_barrier_tests {
    //! Issue #56 Gate A, route half: an `Agent::update_provider` refusal must
    //! reach the client as a typed 409, and everything else must keep its 500.
    //!
    //! Exercised through [`classify_provider_bind_failure`] — the exact mapping
    //! the handler applies — rather than through `AppState`, which opens the
    //! REAL user session database.

    use super::{classify_provider_bind_failure, PrivacyBarrierBody, ProviderBindFailure};
    use axum::http::StatusCode;
    use biorouter::privacy::refusal::PrivacyRefusal;
    use biorouter::privacy::{ProviderTier, SessionClassification};

    fn refusal() -> anyhow::Error {
        PrivacyRefusal::PublicModelOnPrivateSession {
            session_id: "20260801_3".into(),
            provider: "anthropic".into(),
        }
        .into()
    }

    #[test]
    fn a_privacy_refusal_becomes_a_typed_409() {
        let failure = classify_provider_bind_failure(
            &refusal(),
            vec!["llamacpp".to_string(), "versa_azure".to_string()],
        );
        assert_eq!(failure.status(), StatusCode::CONFLICT);
        let ProviderBindFailure::Privacy(body) = failure else {
            panic!("a privacy refusal must not be classified as an internal error");
        };
        assert_eq!(body.code, PrivacyBarrierBody::CODE);
        assert_eq!(body.session_classification, SessionClassification::Private);
        assert_eq!(body.provider_tier, ProviderTier::Public);
        assert_eq!(
            body.available_private_providers,
            ["llamacpp", "versa_azure"]
        );
    }

    #[test]
    fn every_other_failure_keeps_its_500() {
        // The pre-#56 behaviour, pinned: a database or provider failure must not
        // start telling the user their chat is private.
        let failure = classify_provider_bind_failure(
            &anyhow::anyhow!("Failed to persist provider config to session"),
            vec![],
        );
        assert_eq!(failure.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(
            matches!(failure, ProviderBindFailure::Internal(_)),
            "a non-privacy failure must not be rendered as a privacy barrier"
        );
    }

    /// The refusal reaches the handler wrapped in the context
    /// `Agent::update_provider` adds, so the classifier must look through the
    /// whole chain rather than at the outermost error.
    #[test]
    fn a_refusal_is_still_found_under_an_added_context() {
        use anyhow::Context;
        let wrapped = Err::<(), _>(refusal())
            .context("while switching this chat's model")
            .unwrap_err();
        assert_eq!(
            classify_provider_bind_failure(&wrapped, vec![]).status(),
            StatusCode::CONFLICT
        );
    }

    /// §14.4: the body is what the user reads, and it must not carry
    /// conversation content — only the two tiers and the way forward.
    #[test]
    fn the_409_body_carries_no_session_identity() {
        let failure = classify_provider_bind_failure(&refusal(), vec!["ollama".to_string()]);
        let ProviderBindFailure::Privacy(body) = failure else {
            panic!("expected a privacy barrier");
        };
        let json = serde_json::to_string(&*body).unwrap();
        assert!(
            !json.contains("20260801_3"),
            "the 409 body leaked the session id: {json}"
        );
    }
}

#[cfg(test)]
mod add_extension_resolver_tests {
    //! Issue #56 Task 43 (DR-23), route half — and a **structural** test, said
    //! so plainly, because a behavioural one is not reachable from this crate.
    //!
    //! `/agent/add_extension` is one of the three callers that never had a
    //! stamped tier to read and re-classified from a bare name, so a private
    //! extension renamed in `config.yaml` could be attached to a public session
    //! over HTTP. Step 1 moved it to `classify_extension_entry`, passing the
    //! config as well as the name — the config is what carries the install
    //! directory, and the install directory is the only link a rename does not
    //! break.
    //!
    //! ⚠ **Why this is not a behavioural test.** Driving the real handler needs
    //! an `AppState`, which opens the real user session database (the module
    //! above says so for Gate A and takes the same way out), and the seam that
    //! states an extension's provenance — `provenance::insert_test_record` — is
    //! `#[cfg(test)] pub(crate)` in `biorouter`, so it does not exist for this
    //! crate at all. Making it exist would mean shipping a test-record injector
    //! in the release binary or adding a feature flag to a security module for
    //! the sake of one assertion; neither is worth it for a call that can only
    //! ever RAISE a tier.
    //!
    //! So this asserts the one thing that can silently regress: that the gate
    //! still hands the resolver the config. `classify_extension` compiles just
    //! as happily here and would take the refusal back to the name-only join
    //! without any test noticing — which is exactly how this bug lived in three
    //! places. The rename behaviour itself is covered where the seam exists, in
    //! `biorouter`'s `privacy::extensions` and `agents::extension_manager`
    //! tests.
    //!
    //! The `include_str!` pattern is `privacy::config_keys`'s, which scans
    //! provider sources the same way and for the same reason.

    const SOURCE: &str = include_str!("agent.rs");

    /// Whitespace-insensitive, so rustfmt reflowing the call does not fail it.
    fn squeezed() -> String {
        SOURCE.chars().filter(|c| !c.is_whitespace()).collect()
    }

    /// ⚠ The needle is assembled at run time, and finding out why is what the
    /// mutation check was for: written as one literal it appears in this very
    /// file, so the scan found its own assertion and passed against a handler
    /// mutated back to the name-only form. Split across the `format!`, neither
    /// half is the needle and only the real call site can satisfy it.
    ///
    /// ⚠ **Task 48 (DR-26) tightened this from `classify_extension_entry` to
    /// `resolve_extension`, which is a strengthening rather than a rename.**
    /// The entry form is this one's `tier` field; the route now needs the
    /// AFFILIATION too, and taking the two from separate calls is what lets
    /// them disagree about the same entry — a connector coming back Private
    /// *and* unconstrained, which is the whole failure DR-26 exists to catch.
    /// One resolution, one record, both axes.
    #[test]
    fn the_add_extension_gate_resolves_from_the_config_not_from_the_name() {
        let needle = format!(
            "{}(&extension_name,Some(&request.config))",
            "resolve_extension"
        );
        assert!(
            squeezed().contains(&needle),
            "`/agent/add_extension` no longer hands the resolver the config it was given. \
             A renamed private extension resolves Public from its name alone, so this route \
             would attach a clinical connector to a public session — see DR-23. It must also \
             be the resolution that carries the AFFILIATION, or the two axes can disagree \
             about the same entry — see DR-26."
        );
    }

    /// Issue #56 Task 48 (DR-26), the **user's** enable path — row 5 of Step
    /// 2's table, and the half nothing pinned.
    ///
    /// The agent's half (`check_enable_allowed`) has three behavioural tests in
    /// `biorouter`. This one had none: the gate above it only asserts that
    /// `resolve_extension` is called with the config, which the TIER arm
    /// already satisfies, so the entire DR-26 block could be deleted and every
    /// test in the tree would stay green.
    ///
    /// Structural for the reason the module header gives — `AppState` opens the
    /// real user session database and `provenance::insert_test_record` does not
    /// exist for this crate — so this pins the two things that can silently
    /// regress, in the order they would regress in:
    ///
    ///  1. the route stops asking the gate at all, and a cross-institutional
    ///     attach happens with nothing recorded anywhere;
    ///  2. someone "fixes the inconsistency" with the agent path by turning the
    ///     warning into a refusal. That is DR-26's user/agent asymmetry
    ///     inverted, not a tidy-up: a user who insists proceeds past a warning,
    ///     an agent never clears one automatically. Refusing here would also
    ///     leave a legitimate cross-institutional user under a real DUA with no
    ///     way to attach their own connector at all, which is the "researchers
    ///     turn the feature off" outcome the ruling exists to avoid.
    ///
    /// ⚠ Both needles are assembled at run time, for the reason the test above
    /// documents: written as literals they would appear in this very file and
    /// the scan would find its own assertion.
    #[test]
    fn the_add_extension_route_warns_on_a_mismatch_and_still_attaches() {
        let squeezed = squeezed();
        let gate = format!("gate_cross_affiliation{}(", "_warning");
        let attach = format!(".add_extension{}", "(request.config)");

        let asked = squeezed.find(&gate).expect(
            "`/agent/add_extension` no longer asks the cross-affiliation gate. A user attaching \
             another institution's connector to a chat bound to their own institution's model is \
             the mismatch DR-26 exists to state, and this route is the surface it is stated on.",
        );
        let attached = squeezed
            .find(&attach)
            .expect("`/agent/add_extension` no longer attaches the extension it was given");
        assert!(
            asked < attached,
            "the mismatch must be detected BEFORE the extension is attached, not after"
        );

        // Everything the route does between deciding and attaching. On a
        // mismatch it must state the risk and carry on.
        //
        // `get` rather than a slice: both indices come from `find`, so both are
        // already char boundaries, but the fallible form says that rather than
        // asserting it — and this file is not ASCII (the refusal copy carries
        // en-dashes), so a future needle landing mid-character would panic the
        // test with a UTF-8 error instead of the message it is here to give.
        let between = squeezed
            .get(asked..attached)
            .expect("both indices come from `find`, so both are UTF-8 boundaries");
        assert!(
            between.contains("tracing::warn!"),
            "the mismatch is detected and then dropped on the floor: {between}"
        );
        assert!(
            !between.contains("returnErr"),
            "DR-26 warns at the USER's enable path; it refuses only at the AGENT's \
             (`check_enable_allowed`). Turning this into a refusal inverts the asymmetry and \
             strands a legitimate cross-institutional user: {between}"
        );
    }

    /// The name-only form must not reappear anywhere in this file. It is not
    /// wrong in itself — it is the right call for a caller that genuinely holds
    /// only a name — but no route here is such a caller, and its presence is
    /// what the regression would look like.
    /// ⚠ The two needles are assembled at run time rather than written as
    /// literals, because a literal `classify_extension` + `(` in this file would
    /// be found by the scan and fail it against itself.
    #[test]
    fn no_route_in_this_file_classifies_from_a_bare_name() {
        let bare = format!("classify_extension{}", '(');
        let entry = format!("classify_extension_entry{}", '(');
        for line in SOURCE.lines() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            assert!(
                !code.contains(&bare) || code.contains(&entry),
                "a route classified an extension from its name alone, which a rename defeats: \
                 {line}"
            );
        }
    }
}

#[cfg(test)]
mod cross_affiliation_notice_route_tests {
    //! Issue #56 DR-26 — the two surfaces where "warn the user, naming both
    //! institutions, before proceeding" was **log-only**, and the shape of the
    //! repair that can silently come undone.
    //!
    //! The daemon has detected this mismatch at the bind (`Agent::update_provider`)
    //! and at the user's own enable (`POST /agent/add_extension`) since Task 48,
    //! and both wrote it to `tracing::warn!` and nowhere else. The dispatch
    //! surface's accept card worked; these two told the person at the keyboard
    //! nothing, so a researcher attaching another institution's connector from
    //! the extension picker learned about the boundary when a tool call was
    //! refused, if at all.
    //!
    //! ⚠ **Structural, and the module above says why at length**: `AppState::new()`
    //! opens the developer's real session database, so neither handler can be
    //! driven here. What CAN regress silently is the wiring, and it is one line
    //! per handler appended to a `try` block that already looked finished. The
    //! behaviour behind those lines — that the notice names both institutions,
    //! and goes quiet for a flow the user has already accepted — is driven end to
    //! end where the seams exist, in `biorouter`'s
    //! `agents::agent::gate_c_dispatch_tests::
    //! the_bind_notice_names_both_institutions_and_goes_quiet_once_accepted`.

    const SOURCE: &str = include_str!("agent.rs");

    /// Assembled at run time for the reason the two modules above document: a
    /// literal would appear in this very file and every scan below would find
    /// its own assertion instead of the call site.
    fn notice() -> String {
        format!("cross_affiliation{}(", "_notice")
    }

    /// The handler's return type, read from its own signature.
    ///
    /// ⚠ A whole-file `contains` would not do: several handlers in this file
    /// share a return type, so the assertion could be satisfied by a NEIGHBOUR's
    /// signature while the one under test had been reverted to `()`.
    fn returns(signature: &str) -> String {
        let after = SOURCE
            .split_once(signature)
            .unwrap_or_else(|| panic!("{signature} is no longer in this file"))
            .1;
        after
            .split_once(" {\n")
            .expect("every handler signature ends at its opening brace")
            .0
            .to_string()
    }

    /// The BIND surface. `POST /agent/update_provider` returned a bare `()` and
    /// the model picker showed a green success toast over a mismatch nobody had
    /// mentioned.
    ///
    /// Two claims, and the second is the one a refactor breaks: the handler asks
    /// for the notice, and it **returns** it. A version that computed it and
    /// dropped it on the floor is the exact defect being repaired, one layer up.
    #[test]
    fn the_bind_route_hands_back_the_warning_it_used_to_only_log() {
        let handler = crate::routes::body_of(SOURCE, "async fn update_agent_provider");
        assert!(
            handler.contains(&notice()),
            "`/agent/update_provider` no longer asks for DR-26's bind statement. Binding a \
             model covered by another institution's agreements into a chat holding this \
             institution's connectors is the mismatch the ruling exists to state, and this \
             route is the one the model picker calls."
        );
        // The signature, not the body: a handler that returns `()` cannot carry
        // the statement however carefully it composed it.
        let signature = returns("async fn update_agent_provider(");
        assert!(
            signature.contains("-> Result<String,"),
            "`/agent/update_provider` no longer returns a body, so the warning it composes \
             cannot reach the user — which is the state this fix found it in: {signature}"
        );
    }

    /// The USER's enable surface, and the one the ruling names most directly.
    ///
    /// ⚠ The notice is read **after** the attach, and the ordering is asserted
    /// rather than assumed. Read before it, the answer describes a chat that no
    /// longer exists by the time the caller sees it and — worse — omits the very
    /// extension the request was about, so the surface would go quiet in exactly
    /// the case it exists for.
    #[test]
    fn the_enable_route_hands_back_the_warning_it_used_to_only_log() {
        let handler = crate::routes::body_of(SOURCE, "async fn agent_add_extension");
        let attach = format!(".add_extension{}", "(request.config)");

        let asked = handler.find(&notice()).expect(
            "`/agent/add_extension` no longer hands the user DR-26's statement. It logged this \
             mismatch and nothing else for its whole life, which is how a user enabling a \
             foreign connector from the extension picker came to be told nothing at all.",
        );
        let attached = handler
            .find(&attach)
            .expect("`/agent/add_extension` no longer attaches the extension it was given");
        assert!(
            attached < asked,
            "the notice is composed BEFORE the extension is attached, so it answers for a chat \
             that no longer exists — and omits the connector the request was about"
        );
        let signature = returns("async fn agent_add_extension(");
        assert!(
            signature.contains("-> Result<String,"),
            "`/agent/add_extension` no longer returns a body, so the warning it composes cannot \
             reach the user: {signature}"
        );
    }

    /// The half that is a correctness bug rather than a wiring one.
    ///
    /// The notice suppresses flows the user has already accepted, and a grant is
    /// keyed on (session, extension, **model affiliation**). Both handlers must
    /// pass the affiliation of the provider they themselves hold — the one the
    /// bind just created, the one the enable read once for both privacy axes.
    /// `Agent::update_provider` reassigns the provider mutex with no turn lock,
    /// so a second read here could key the lookup on a model some other caller
    /// bound in between and suppress a warning on an acceptance that was never
    /// given for the model in question. That failure is silent and it fails
    /// OPEN — the user is not warned.
    ///
    /// So: exactly one binding of the affiliation per handler, and the notice
    /// call reads that binding.
    #[test]
    fn each_route_keys_the_notice_on_the_provider_it_holds_itself() {
        let bind = format!("let model_{} =", "affiliation");
        for name in [
            "async fn update_agent_provider",
            "async fn agent_add_extension",
        ] {
            let handler = crate::routes::body_of(SOURCE, name);
            let bindings = handler.matches(&bind).count();
            assert_eq!(
                bindings, 1,
                "{name} binds the model affiliation {bindings} times. Two reads of the provider \
                 are the read-then-read `CallCapability` exists to collapse: the route can warn \
                 about one institution while checking an acceptance recorded for another, and \
                 the failure is a warning that is never shown."
            );
            let at_bind = handler.find(&bind).expect("just counted one");
            let at_notice = handler.find(&notice()).expect("pinned by the tests above");
            assert!(
                at_bind < at_notice,
                "{name} composes the notice before it has the affiliation to key it on"
            );
        }
    }
}

#[cfg(test)]
mod gate_c_call_tool_tests {
    //! Issue #56 Gate C, route half. `POST /agent/call_tool` is one of the four
    //! production paths into `ExtensionManager::dispatch_tool_call`, and the
    //! only one whose caller is an HTTP client rather than a model — so the
    //! refusal has to arrive as the tool's own result. It used to be thrown
    //! away by `.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)`, which told the
    //! caller nothing and told the model, when a client relayed it, that
    //! Biorouter had crashed.
    //!
    //! Exercised through [`dispatch_failure_response`] — the exact mapping the
    //! handler applies — rather than through `AppState`, which builds the
    //! process-global `AgentManager` and opens the REAL user session database.
    //! That is the same shape `privacy_barrier_tests` above uses for Gate A's
    //! route half, and the same reason.

    use super::dispatch_failure_response;
    use axum::http::StatusCode;
    use biorouter::privacy::refusal::privacy_refusal;
    use biorouter::privacy::ProviderTier;
    use rmcp::model::{ErrorCode, ErrorData};

    fn text_of(response: &super::CallToolResponse) -> String {
        response
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn a_gate_c_refusal_reaches_the_caller_rather_than_becoming_a_500() {
        let refusal = privacy_refusal("ucsfomopagent", ProviderTier::Private, ProviderTier::Public)
            .expect("a public caller on a private extension is refused");
        let response = dispatch_failure_response(&anyhow::Error::from(refusal))
            .expect("a refusal is an answer about the tool, not a server fault");
        assert!(response.is_error);
        let text = text_of(&response);
        assert!(text.contains("ucsfomopagent"), "{text}");
        assert!(text.contains("Settings"), "{text}");
    }

    /// Every OTHER refusal `dispatch_tool_call` raises is an `ErrorData` too, so
    /// the fix is not special-cased to Gate C: the pre-#56 500 was wrong for all
    /// of them, and a caller that asks for a tool that does not exist now learns
    /// which one.
    #[test]
    fn any_other_tool_level_failure_also_carries_its_reason() {
        let not_found = ErrorData::new(
            ErrorCode::RESOURCE_NOT_FOUND,
            "Tool 'nope__x' not found".to_string(),
            None,
        );
        let response = dispatch_failure_response(&anyhow::Error::from(not_found))
            .expect("a tool-level failure is a tool result");
        assert!(response.is_error);
        assert!(text_of(&response).contains("nope__x"));
    }

    /// …and the line the classification draws. Gate C's ratchet propagates a
    /// session-store failure with `?`, which is the first non-`ErrorData` error
    /// `dispatch_tool_call` can return. Rendering it as `200 + is_error` would
    /// tell an HTTP client the tool ran and disagreed — retiring its retry — and
    /// hand it raw store text. It keeps the 500 it had before this task.
    #[test]
    fn a_store_failure_is_not_laundered_into_a_tool_result() {
        // `CallToolResponse` is not `Debug`, so the outcome is matched rather
        // than `expect_err`'d.
        match dispatch_failure_response(&anyhow::anyhow!(
            "error returned from database: disk I/O error"
        )) {
            Ok(response) => panic!(
                "a session-store failure was rendered as a tool result: {}",
                text_of(&response)
            ),
            Err(status) => assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR),
        }
    }
}

#[cfg(test)]
mod working_dir_lock_tests {
    //! Route-level contract for `/agent/update_working_dir`'s empty-chat-only
    //! rule (#44), exercised through [`apply_working_dir_update`] — the exact
    //! validation + guard + update path the handler runs before restarting the
    //! agent. A hermetic tempdir-backed [`SessionManager`] stands in for the
    //! real one: `AppState::new()` opens the REAL user session database (see
    //! the note in `routes/session.rs`), so a mutating route test must never
    //! go through it.

    use super::apply_working_dir_update;
    use axum::http::StatusCode;
    use biorouter::conversation::message::Message;
    use biorouter::session::session_manager::SessionType;
    use biorouter::session::SessionManager;
    use tempfile::TempDir;

    /// A store + one session whose working dir is `start_dir`.
    async fn session_in(store: &TempDir, start_dir: &std::path::Path) -> (SessionManager, String) {
        let sm = SessionManager::new(store.path().to_path_buf());
        let session = sm
            .create_session(
                start_dir.to_path_buf(),
                "test session".into(),
                SessionType::User,
            )
            .await
            .unwrap();
        (sm, session.id)
    }

    #[tokio::test]
    async fn an_empty_session_accepts_a_working_dir_change() {
        let store = TempDir::new().unwrap();
        let old_dir = TempDir::new().unwrap();
        let new_dir = TempDir::new().unwrap();
        let (sm, id) = session_in(&store, old_dir.path()).await;

        let updated = apply_working_dir_update(&sm, &id, new_dir.path().to_str().unwrap())
            .await
            .expect("an empty session's working dir is still choosable");
        assert_eq!(updated.working_dir, new_dir.path());

        // And the change is persisted, not just echoed.
        let reloaded = sm.get_session(&id, false).await.unwrap();
        assert_eq!(reloaded.working_dir, new_dir.path());
    }

    #[tokio::test]
    async fn a_session_with_a_message_locks_its_working_dir_with_409() {
        let store = TempDir::new().unwrap();
        let old_dir = TempDir::new().unwrap();
        let new_dir = TempDir::new().unwrap();
        let (sm, id) = session_in(&store, old_dir.path()).await;

        sm.add_message(&id, &Message::user().with_text("hello"))
            .await
            .unwrap();

        let err = apply_working_dir_update(&sm, &id, new_dir.path().to_str().unwrap())
            .await
            .expect_err("one message is enough to lock the working dir");
        assert_eq!(err.status, StatusCode::CONFLICT);
        assert_eq!(
            err.message,
            "the working directory is fixed once a chat has messages"
        );

        // The rejected change must not have touched the session.
        let reloaded = sm.get_session(&id, false).await.unwrap();
        assert_eq!(reloaded.working_dir, old_dir.path());
    }

    #[tokio::test]
    async fn a_missing_session_is_a_404_not_a_silent_update() {
        let store = TempDir::new().unwrap();
        let new_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(store.path().to_path_buf());

        let err =
            apply_working_dir_update(&sm, "no_such_session", new_dir.path().to_str().unwrap())
                .await
                .expect_err("a missing session cannot accept a working dir");
        assert_eq!(err.status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn an_invalid_target_path_is_a_400() {
        let store = TempDir::new().unwrap();
        let old_dir = TempDir::new().unwrap();
        let (sm, id) = session_in(&store, old_dir.path()).await;

        for bad in ["", "   ", "/definitely/not/a/real/dir/for/br44"] {
            let err = apply_working_dir_update(&sm, &id, bad)
                .await
                .expect_err("invalid paths are rejected");
            assert_eq!(err.status, StatusCode::BAD_REQUEST, "for {bad:?}");
        }
    }
}

#[cfg(test)]
mod knowledge_selection_tests {
    use super::{apply_workflow_knowledge_selection, plan_workflow_knowledge_selection};
    use biorouter::workflow::{Workflow, WorkflowKnowledgeBases};
    use biorouter_mcp::knowledge::service::KnowledgeService;
    use std::sync::Arc;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    /// A workflow that lists five bases used to activate exactly one, silently
    /// (`selection.default.or(selection.visible.first())`). Under the merged
    /// model `{ default, visible }` already *is* "a set plus one primary", so
    /// the whole set applies and only `default` may set the pointer.
    #[test]
    fn workflow_applies_every_declared_base() {
        let selection = WorkflowKnowledgeBases {
            default: Some("c".to_string()),
            visible: ids(&["a", "b", "c", "d", "e"]),
        };
        let (visible, primary) = plan_workflow_knowledge_selection(&selection);
        assert_eq!(visible, ids(&["a", "b", "c", "d", "e"]));
        assert_eq!(primary.as_deref(), Some("c"));
    }

    /// No `default` means **no primary**, at every set size. A one-base
    /// workflow used to have its sole member promoted, which is the model's
    /// forbidden move — inventing a pointer the author never wrote. The
    /// author who wants that base to be the write target says so in `default`;
    /// the author who lists it without a `default` gets a session whose
    /// KB-less writes fail loudly naming `a` as the candidate.
    #[test]
    fn workflow_without_a_default_never_invents_a_primary() {
        let one = WorkflowKnowledgeBases {
            default: None,
            visible: ids(&["a"]),
        };
        assert_eq!(
            plan_workflow_knowledge_selection(&one).1,
            None,
            "a sole visible base is still not a primary the author chose"
        );

        let many = WorkflowKnowledgeBases {
            default: None,
            visible: ids(&["a", "b"]),
        };
        assert_eq!(plan_workflow_knowledge_selection(&many).1, None);
    }

    /// A `default` the author forgot to list is still the author's intent, and
    /// the invariant requires the primary to be a member — so union it in
    /// rather than dropping it.
    #[test]
    fn workflow_default_joins_the_set_when_it_was_not_listed() {
        let selection = WorkflowKnowledgeBases {
            default: Some("b".to_string()),
            visible: ids(&["a"]),
        };
        let (visible, primary) = plan_workflow_knowledge_selection(&selection);
        assert_eq!(
            visible,
            ids(&["a", "b"]),
            "b must be in the set — it is the primary"
        );
        assert_eq!(primary.as_deref(), Some("b"));
    }

    /// The workflow's set must be inverted against the installed bases *inside*
    /// the lock. Applying it used to list the bases first, unlocked, build a
    /// hidden list from that snapshot, and only then take the lock to write it —
    /// so a base created in that window was in nobody's hidden list and joined
    /// a workflow session the workflow had never mentioned.
    ///
    /// The race is staged deterministically: the creator takes the root lock
    /// first (`create_base` holds it for the whole git init), and the apply
    /// starts a few ms later. `list_bases` takes no lock, so the old code read
    /// its inventory straight through the creator's lock and missed `gamma`;
    /// the locked write blocks until the base is fully installed and then hides
    /// it like any other undeclared base.
    #[test]
    fn applying_a_workflow_hides_a_base_that_lands_mid_call() {
        let dir = tempfile::tempdir().unwrap();
        let svc = Arc::new(KnowledgeService::new(dir.path().to_path_buf()));
        for id in ["alpha", "beta"] {
            svc.create_base(id, id, None).unwrap();
        }

        let mut workflow = Workflow::builder()
            .title("t")
            .description("d")
            .instructions("i")
            .build()
            .unwrap();
        workflow.knowledge_bases = Some(WorkflowKnowledgeBases {
            default: Some("alpha".to_string()),
            visible: ids(&["alpha"]),
        });

        let creator = {
            let svc = Arc::clone(&svc);
            std::thread::spawn(move || svc.create_base("gamma", "gamma", None).unwrap())
        };
        // Long enough for `create_base` to be holding the root lock, short
        // enough that it is nowhere near registering `gamma`.
        std::thread::sleep(std::time::Duration::from_millis(10));
        apply_workflow_knowledge_selection(&svc, "s1", &workflow).unwrap();
        creator.join().unwrap();

        let selection = svc.selection(Some("s1")).unwrap();
        assert_eq!(
            selection.kb_ids,
            ids(&["alpha"]),
            "only the declared base belongs to a workflow session"
        );
        assert_eq!(selection.primary_kb.as_deref(), Some("alpha"));
    }
}
