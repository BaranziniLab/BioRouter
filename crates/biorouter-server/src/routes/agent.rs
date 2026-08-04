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
use biorouter_server::auth::is_user_action;
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
        (status = 200, description = "Provider updated successfully"),
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
) -> Result<(), axum::response::Response> {
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

    Ok(())
}

#[utoipa::path(
    post,
    path = "/agent/add_extension",
    request_body = AddExtensionRequest,
    responses(
        (status = 200, description = "Extension added", body = String),
        (status = 401, description = "Unauthorized - invalid secret key"),
        (status = 409, description = "Refused by a privacy boundary (issue #56, DR-16): a \
                                      private extension cannot be attached to a chat running on \
                                      a public model"),
        (status = 424, description = "Agent not initialized"),
        (status = 500, description = "Internal server error")
    )
)]
async fn agent_add_extension(
    State(state): State<Arc<AppState>>,
    Json(request): Json<AddExtensionRequest>,
) -> Result<StatusCode, ErrorResponse> {
    let agent = state.get_agent(request.session_id.clone()).await?;

    // Issue #56 DR-16. `/agent/add_extension` hands `request.config` straight to
    // the agent and persists it, which is how a private extension's TOOLS arrive
    // in a session Gate F1 already refuses to let the model enable through
    // `extensionmanager__manage_extensions`.
    //
    // Refused OUTRIGHT — no user-proof branch, deliberately. Attaching a private
    // extension to a public session is not a raise the user can authorize
    // either; their route is to switch the model first and then attach.
    let capability = agent
        .provider()
        .await
        .map(|p| p.tier())
        .unwrap_or(ProviderTier::Public);
    let extension_name = request.config.name();
    // DR-15's master opt-out, read INSIDE the gate. A direct read, not a
    // `CallCapability`: `/agent/add_extension` is not a tool dispatch and has no
    // admitted capability to inherit.
    if biorouter::privacy::privacy_tiers_enabled()
        // Task 43 (DR-23): the config, not just its name — a renamed entry is
        // resolved through the install directory in its arguments.
        && biorouter::privacy::classify_extension_entry(&extension_name, Some(&request.config))
            .is_private()
        && capability == ProviderTier::Public
    {
        return Err(ErrorResponse {
            status: StatusCode::CONFLICT,
            message: PrivacyRefusal::PrivateExtensionOverHttp {
                name: extension_name,
            }
            .to_string(),
        });
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

    Ok(StatusCode::OK)
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
    Json(payload): Json<UpdateWorkingDirRequest>,
) -> Result<StatusCode, ErrorResponse> {
    let session_id = payload.session_id.clone();

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
        .route("/agent/remove_extension", post(agent_remove_extension))
        .route("/agent/stop", post(stop_agent))
        .with_state(state)
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
    #[test]
    fn the_add_extension_gate_resolves_from_the_config_not_from_the_name() {
        let needle = format!(
            "{}(&extension_name,Some(&request.config))",
            "classify_extension_entry"
        );
        assert!(
            squeezed().contains(&needle),
            "`/agent/add_extension` no longer hands the resolver the config it was given. \
             A renamed private extension resolves Public from its name alone, so this route \
             would attach a clinical connector to a public session — see DR-23."
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
