use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use futures::stream::BoxStream;
use futures::{stream, FutureExt, Stream, StreamExt, TryStreamExt};
use uuid::Uuid;

use super::final_output_tool::FinalOutputTool;
use super::platform_tools;
use super::tool_execution::{ToolCallResult, CHAT_MODE_TOOL_SKIPPED_RESPONSE, DECLINED_RESPONSE};
use crate::action_required_manager::ActionRequiredManager;
use crate::agents::extension::{ExtensionConfig, ExtensionResult, ToolInfo};
use crate::agents::extension_manager::{get_parameter_names, normalize, ExtensionManager};
use crate::agents::extension_manager_extension::MANAGE_EXTENSIONS_TOOL_NAME_COMPLETE;
use crate::agents::final_output_tool::{FINAL_OUTPUT_CONTINUATION_MESSAGE, FINAL_OUTPUT_TOOL_NAME};
use crate::agents::platform_tools::{
    PLATFORM_INGEST_CONVERSATION_TOOL_NAME, PLATFORM_MANAGE_SCHEDULE_TOOL_NAME,
};
use crate::agents::prompt_manager::PromptManager;
use crate::agents::resource_refs::{
    canonical_builtin_extension_name, extract_resource_refs, ResourceRefs,
};
use crate::agents::retry::{RetryManager, RetryResult};
use crate::agents::subagent_task_config::TaskConfig;
use crate::agents::subagent_tool::{
    create_subagent_tool, handle_subagent_tool, SUBAGENT_TOOL_NAME,
};
use crate::agents::types::SessionConfig;
use crate::agents::types::{FrontendTool, SharedProvider, ToolResultReceiver};
use crate::checkpoint::{CheckpointConfig, CheckpointKind, CheckpointManager};
use crate::config::permission::PermissionManager;
use crate::config::{BioRouterMode, Config};
use crate::context_mgmt::{
    check_if_compaction_needed, compact_messages, compact_messages_with_recovery,
    overflow_recovery_for_attempt, DEFAULT_COMPACTION_THRESHOLD,
};
use crate::conversation::message::{
    ActionRequiredData, Message, MessageContent, ProviderMetadata, SystemNotificationType,
    ToolRequest,
};
use crate::conversation::tool_result_serde::call_tool_result;
use crate::conversation::{debug_conversation_fix, fix_conversation, Conversation};
use crate::managed::ManagedPolicy;
use crate::mcp_utils::ToolResult;
use crate::permission::managed_inspector::ManagedPolicyInspector;
use crate::permission::permission_inspector::PermissionInspector;
use crate::permission::permission_judge::PermissionCheckResult;
use crate::permission::PermissionConfirmation;
use crate::providers::base::Provider;
use crate::providers::errors::ProviderError;
use crate::scheduler_trait::SchedulerTrait;
use crate::security::security_inspector::SecurityInspector;
use crate::session::extension_data::{EnabledExtensionsState, ExtensionState};
use crate::session::{Session, SessionManager, SessionType};
use crate::tool_inspection::{InspectionAction, InspectionResult, ToolInspectionManager};
use crate::tool_monitor::RepetitionInspector;
use crate::utils::is_token_cancelled;
use crate::workflow::{Author, Response, Settings, SubWorkflow, Workflow};
use regex::Regex;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, ErrorCode, ErrorData, GetPromptResult, Prompt,
    ServerNotification, Tool,
};
use rmcp::object;
use serde_json::Value;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn};

const DEFAULT_MAX_TURNS: u32 = 100;
/// Absolute cap on the number of tool calls in a single reply, summed across all
/// iterations. `max_turns` counts provider round-trips, but one round-trip can
/// fan out many parallel tool calls, so a few iterations can run an unbounded
/// number of tools with ever-changing args (which the exact-duplicate guard
/// misses). This is the backstop for that. Generous by default so it never bites
/// normal work; overridable per session (`max_tool_calls`) or globally
/// (`BIOROUTER_MAX_TOOL_CALLS`).
const DEFAULT_MAX_TOOL_CALLS: u32 = 200;
const DEFAULT_MAX_REPETITIONS: u32 = 3;
const COMPACTION_THINKING_TEXT: &str = "biorouter is compacting the conversation...";
/// Max consecutive auto-continues for a turn the provider cut off by the output
/// length limit (`finish_reason == "length"`) with no tool call. Bounded so a
/// pathological "always truncates, never progresses" stream can't loop forever;
/// any tool call resets the streak. Also globally bounded by `max_turns`.
const MAX_TRUNCATION_CONTINUATIONS: u32 = 12;
/// Injected when auto-continuing a length-truncated turn, so the model resumes
/// instead of the agent ending the turn on a half-finished response.
const TRUNCATION_CONTINUATION_MESSAGE: &str = "Your previous response was cut off because it reached the output length limit (finish_reason=\"length\"). Continue exactly where you left off — do not repeat what you already wrote.";

/// Injected in place of a selected skill's full body on any turn after the first
/// it was loaded (BR-8), so a skill-heavy session doesn't re-inline the whole
/// body every turn.
fn skill_already_loaded_pointer() -> &'static str {
    "This skill's full instructions were already loaded earlier in this session, so they are not \
     repeated here to save context. They remain in effect; call the `skills__loadSkill` tool to \
     re-read the full text if you need it again."
}
// NOTE: the "continue when the agent stops with unchecked todos" behavior used to
// live here as a hard-coded agent-loop completion gate that fabricated a *visible*
// `user` message every turn. That over-reached: when the agent was genuinely stuck
// (e.g. an unrecoverable provider error), it re-injected the same message forever
// and never resolved the root cause — and it polluted the conversation with fake
// user input. "Don't stop while work is unfinished" is now left to the proper,
// bounded, user-configurable mechanisms: the Stop-hook system (`StopHookVerdict`,
// capped by `STOP_HOOK_BLOCK_CAP`, delivered as hidden-visibility feedback + a
// user-facing system notification) and the `/goal` loop (whose stall budget does
// NOT reset when tools run, so it gives up when progress stalls). A user who wants
// "keep going until the todos are done" sets a `/goal` or a Stop hook — both go
// through that bounded, stall-aware path instead of an unbounded loop injection.

/// Context needed for the reply function
pub struct ReplyContext {
    pub conversation: Conversation,
    pub tools: Vec<Tool>,
    pub toolshim_tools: Vec<Tool>,
    pub system_prompt: String,
    pub biorouter_mode: BioRouterMode,
    pub initial_messages: Vec<Message>,
}

pub struct ToolCategorizeResult {
    pub frontend_requests: Vec<ToolRequest>,
    pub remaining_requests: Vec<ToolRequest>,
    pub filtered_response: Message,
}

#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct ExtensionLoadResult {
    pub name: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct AgentConfig {
    pub session_manager: Arc<SessionManager>,
    pub permission_manager: Arc<PermissionManager>,
    pub scheduler_service: Option<Arc<dyn SchedulerTrait>>,
    pub biorouter_mode: BioRouterMode,
}

impl AgentConfig {
    pub fn new(
        session_manager: Arc<SessionManager>,
        permission_manager: Arc<PermissionManager>,
        scheduler_service: Option<Arc<dyn SchedulerTrait>>,
        biorouter_mode: BioRouterMode,
    ) -> Self {
        Self {
            session_manager,
            permission_manager,
            scheduler_service,
            biorouter_mode,
        }
    }
}

/// The main biorouter Agent
pub struct Agent {
    pub(super) provider: SharedProvider,
    pub config: AgentConfig,

    pub extension_manager: Arc<ExtensionManager>,
    pub(super) sub_workflows: Mutex<HashMap<String, SubWorkflow>>,
    pub(super) final_output_tool: Arc<Mutex<Option<FinalOutputTool>>>,
    pub(super) frontend_tools: Mutex<HashMap<String, FrontendTool>>,
    pub(super) frontend_instructions: Mutex<Option<String>>,
    pub(super) prompt_manager: Mutex<PromptManager>,
    pub(super) confirmation_tx: mpsc::Sender<(String, PermissionConfirmation)>,
    pub(super) confirmation_rx: Mutex<mpsc::Receiver<(String, PermissionConfirmation)>>,
    pub(super) tool_result_tx: mpsc::Sender<(String, ToolResult<CallToolResult>)>,
    pub(super) tool_result_rx: ToolResultReceiver,

    pub(super) retry_manager: RetryManager,
    pub(super) tool_inspection_manager: ToolInspectionManager,
    pub(super) hooks_manager: Arc<crate::hooks::HooksManager>,
    /// Active `/goal` conditions per session (see [`crate::agents::goal`]).
    pub(super) goals: crate::agents::goal::GoalRegistry,
    /// Lazily-created scheduler for `/loop`/`/schedule` when no
    /// `scheduler_service` was injected (plain CLI/TUI sessions).
    pub(super) fallback_scheduler: tokio::sync::OnceCell<Arc<dyn SchedulerTrait>>,
    /// BRSDK encryption: per-app decrypted secrets, substituted into tool-call
    /// arguments at dispatch (`{{vault:NAME}}`). `None` for normal sessions.
    pub(super) vault: Mutex<Option<Arc<crate::agents::vault_refs::VaultRefs>>>,
    /// Soft-interrupt queue: user messages submitted mid-turn. Drained and
    /// injected at the next safe loop boundary in `reply_internal` instead of
    /// cancelling the turn (no lost work, no full context re-send). A plain
    /// `std::Mutex` so callers can push without awaiting the agent's async locks.
    pub(super) soft_interrupts: Arc<std::sync::Mutex<Vec<String>>>,
    /// BR-43 shadow-git checkpoints: captures the work-tree at turn boundaries so
    /// `/rewind` can restore files/conversation. `None` when disabled (the
    /// default) or on the subagent/test paths. Gated by `BIOROUTER_CHECKPOINTS`.
    pub(super) checkpoints: Option<Arc<CheckpointManager>>,
    /// BR-12: sessions with a background eager-compaction task in flight. Keeps
    /// at most one summarizer running per session so two tasks never both try to
    /// swap in a compacted history. A plain `std::Mutex` — only held for the
    /// insert/remove, never across an await.
    pub(super) eager_compactions: Arc<std::sync::Mutex<HashSet<String>>>,
    /// Per-session set of skill names whose full body has already been inlined
    /// into an earlier turn's `<explicit-resource-context>` (BR-8). A skill's
    /// body is inlined in full the first turn it is selected, then replaced by a
    /// short pointer on later turns so a skill-heavy session doesn't re-pay the
    /// multi-KB body cost every turn. Keyed by session id.
    pub(super) injected_skills: Mutex<HashMap<String, std::collections::HashSet<String>>>,
}

#[derive(Clone, Debug)]
pub enum AgentEvent {
    Message(Message),
    McpNotification((String, ServerNotification)),
    ModelChange { model: String, mode: String },
    HistoryReplaced(Conversation),
}

impl Default for Agent {
    fn default() -> Self {
        Self::new()
    }
}

pub enum ToolStreamItem<T> {
    Message(ServerNotification),
    Result(T),
}

pub type ToolStream =
    Pin<Box<dyn Stream<Item = ToolStreamItem<ToolResult<CallToolResult>>> + Send>>;

// tool_stream combines a stream of ServerNotifications with a future representing the
// final result of the tool call. MCP notifications are not request-scoped, but
// this lets us capture all notifications emitted during the tool call for
// simpler consumption
pub fn tool_stream<S, F>(rx: S, done: F) -> ToolStream
where
    S: Stream<Item = ServerNotification> + Send + Unpin + 'static,
    F: Future<Output = ToolResult<CallToolResult>> + Send + 'static,
{
    Box::pin(async_stream::stream! {
        tokio::pin!(done);
        let mut rx = rx;

        loop {
            tokio::select! {
                Some(msg) = rx.next() => {
                    yield ToolStreamItem::Message(msg);
                }
                r = &mut done => {
                    yield ToolStreamItem::Result(r);
                    break;
                }
            }
        }
    })
}

/// BR-12: RAII marker that a session has a background eager-compaction task in
/// flight. Removed from the agent's `eager_compactions` set on drop (task ends,
/// panics, or the runtime shuts down), so a later turn can spawn again.
struct EagerCompactionGuard {
    session_id: String,
    in_flight: Arc<std::sync::Mutex<HashSet<String>>>,
}

impl Drop for EagerCompactionGuard {
    fn drop(&mut self) {
        self.in_flight
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.session_id);
    }
}

/// Fire a Pre/PostCompact hook without an `Agent` receiver. Split out of
/// [`Agent::fire_compaction_hook`] so the BR-12 background eager-compaction task
/// (which holds only a cloned `Arc<HooksManager>`, not `&self`) can fire the
/// same hooks.
pub(super) fn fire_compaction_hook_on(
    hooks_manager: &Arc<crate::hooks::HooksManager>,
    event: crate::hooks::HookEvent,
    session_id: &str,
    working_dir: &std::path::Path,
    trigger: &str,
    reason: Option<&str>,
) {
    let mut payload =
        crate::hooks::HookPayload::new(event, session_id, working_dir.to_string_lossy());
    payload.trigger = Some(trigger.to_string());
    payload.reason = reason.map(str::to_string);
    hooks_manager.fire(
        event,
        Some(trigger.to_string()),
        payload,
        working_dir.to_path_buf(),
    );
}

impl Agent {
    pub fn new() -> Self {
        Self::with_config(AgentConfig::new(
            Arc::new(SessionManager::instance()),
            PermissionManager::instance(),
            None,
            Config::global()
                .get_biorouter_mode()
                .unwrap_or(BioRouterMode::Auto),
        ))
    }

    pub fn with_config(config: AgentConfig) -> Self {
        // Create channels with buffer size 32 (adjust if needed)
        let (confirm_tx, confirm_rx) = mpsc::channel(32);
        let (tool_tx, tool_rx) = mpsc::channel(32);
        let provider = Arc::new(Mutex::new(None));

        let session_manager = Arc::clone(&config.session_manager);
        let permission_manager = Arc::clone(&config.permission_manager);
        // Load the managed/enterprise policy once at startup and share it across
        // the hooks manager and the tool inspectors (BR-65).
        let managed = ManagedPolicy::load();
        let hooks_manager = Arc::new(crate::hooks::HooksManager::new_with_managed(
            provider.clone(),
            Arc::clone(&managed),
        ));
        // BR-43: build the checkpoint manager only when enabled, so the disabled
        // default path never touches disk. Reads `BIOROUTER_CHECKPOINTS` / caps.
        let checkpoint_cfg = CheckpointConfig::from_env();
        let checkpoints = checkpoint_cfg.enabled.then(|| {
            Arc::new(CheckpointManager::new(
                crate::config::paths::Paths::data_dir(),
                Arc::clone(&config.session_manager),
                checkpoint_cfg,
            ))
        });
        Self {
            provider: provider.clone(),
            config,
            extension_manager: Arc::new(ExtensionManager::new(provider.clone(), session_manager)),
            sub_workflows: Mutex::new(HashMap::new()),
            final_output_tool: Arc::new(Mutex::new(None)),
            frontend_tools: Mutex::new(HashMap::new()),
            frontend_instructions: Mutex::new(None),
            prompt_manager: Mutex::new(PromptManager::new()),
            confirmation_tx: confirm_tx,
            confirmation_rx: Mutex::new(confirm_rx),
            tool_result_tx: tool_tx,
            tool_result_rx: Arc::new(Mutex::new(tool_rx)),
            retry_manager: RetryManager::new(),
            tool_inspection_manager: Self::create_tool_inspection_manager(
                permission_manager,
                Arc::clone(&hooks_manager),
                Arc::clone(&managed),
            ),
            hooks_manager,
            goals: Default::default(),
            fallback_scheduler: tokio::sync::OnceCell::new(),
            vault: Mutex::new(None),
            soft_interrupts: Arc::new(std::sync::Mutex::new(Vec::new())),
            checkpoints,
            eager_compactions: Arc::new(std::sync::Mutex::new(HashSet::new())),
            injected_skills: Mutex::new(HashMap::new()),
        }
    }

    /// Install the per-app secret vault (BRSDK encryption). Decrypted secrets are
    /// substituted into tool-call arguments at dispatch — after the model has
    /// produced the call — so plaintext never enters the model's context.
    pub async fn set_vault(&self, refs: Arc<crate::agents::vault_refs::VaultRefs>) {
        *self.vault.lock().await = Some(refs);
    }

    /// Resolve `{{vault:NAME}}` placeholders in a tool call's arguments using the
    /// installed vault (no-op when none is set). Called ONLY on the leaf
    /// MCP-dispatch path in [`Self::dispatch_tool_call`] — never for the subagent,
    /// frontend, final_output, or schedule branches, whose arguments would carry
    /// the plaintext back to an LLM/browser/store. (Residual: a tool that echoes
    /// its arguments in its *result* can still surface the secret on the next turn
    /// — that's outside the request-side substitution's control.)
    pub(super) async fn apply_vault(&self, tool_call: &mut CallToolRequestParams) {
        let vault = { self.vault.lock().await.clone() };
        if let Some(vault) = vault {
            if let Some(args) = tool_call.arguments.as_mut() {
                vault.resolve_args(args);
            }
        }
    }

    /// Queue a user message to be injected into the running turn at the next safe
    /// loop boundary (soft interrupt). Cheap + lock-light: callable from a server
    /// route or the CLI while a turn is streaming, without cancelling it.
    pub fn queue_soft_interrupt(&self, text: String) {
        if let Ok(mut q) = self.soft_interrupts.lock() {
            q.push(text);
        }
    }

    /// Drain queued soft-interrupt messages (FIFO). Returns empty when none.
    pub(super) fn drain_soft_interrupts(&self) -> Vec<String> {
        self.soft_interrupts
            .lock()
            .map(|mut q| std::mem::take(&mut *q))
            .unwrap_or_default()
    }

    /// The hooks manager driving user-configured lifecycle hooks.
    pub fn hooks_manager(&self) -> Arc<crate::hooks::HooksManager> {
        Arc::clone(&self.hooks_manager)
    }

    /// The BR-43 checkpoint manager, when checkpoints are enabled.
    pub fn checkpoints(&self) -> Option<Arc<CheckpointManager>> {
        self.checkpoints.clone()
    }

    /// BR-43: snapshot the work-tree at a turn boundary (no-op when disabled).
    /// Best-effort — a checkpoint failure must never break the reply. `anchor_ts`
    /// is the `created` timestamp of the user message that opened this turn.
    pub(super) async fn maybe_checkpoint(
        &self,
        session_id: &str,
        working_dir: &std::path::Path,
        anchor_ts: i64,
        kind: CheckpointKind,
    ) {
        let Some(cp) = self.checkpoints.as_ref() else {
            return;
        };
        if let Err(e) = cp.snapshot(session_id, working_dir, anchor_ts, kind).await {
            warn!("BR-43 checkpoint snapshot failed (non-fatal): {e}");
        }
    }

    /// Fire a PreCompact/PostCompact hook (observe-only, fire-and-forget).
    pub(super) fn fire_compaction_hook(
        &self,
        event: crate::hooks::HookEvent,
        session_id: &str,
        working_dir: &std::path::Path,
        trigger: &str,
        reason: Option<&str>,
    ) {
        fire_compaction_hook_on(
            &self.hooks_manager,
            event,
            session_id,
            working_dir,
            trigger,
            reason,
        );
    }

    /// Create a tool inspection manager with default inspectors
    fn create_tool_inspection_manager(
        permission_manager: Arc<PermissionManager>,
        hooks_manager: Arc<crate::hooks::HooksManager>,
        managed: Arc<ManagedPolicy>,
    ) -> ToolInspectionManager {
        let mut tool_inspection_manager = ToolInspectionManager::new();

        // Managed/enterprise policy inspector (highest priority - runs first).
        // Its Deny/Ask verdicts ride the escalation-only merge and win over
        // every later inspector, including Auto mode's blanket Allow. Inert
        // (skipped) when no trusted managed file is present (BR-65).
        tool_inspection_manager
            .add_inspector(Box::new(ManagedPolicyInspector::new(Arc::clone(&managed))));

        // Add security inspector (runs after managed)
        tool_inspection_manager.add_inspector(Box::new(SecurityInspector::new()));

        // Add permission inspector (medium-high priority)
        tool_inspection_manager.add_inspector(Box::new(PermissionInspector::new(
            std::collections::HashSet::new(), // readonly tools - will be populated from extension manager
            std::collections::HashSet::new(), // regular tools - will be populated from extension manager
            permission_manager,
            managed,
        )));

        // Add repetition inspector (lower priority - basic repetition checking)
        tool_inspection_manager.add_inspector(Box::new(RepetitionInspector::new(Some(
            DEFAULT_MAX_REPETITIONS,
        ))));

        // Add user-configured PreToolUse hooks (runs last)
        tool_inspection_manager
            .add_inspector(Box::new(crate::hooks::HookInspector::new(hooks_manager)));

        tool_inspection_manager
    }

    /// Reset the retry attempts counter to 0
    pub async fn reset_retry_attempts(&self) {
        self.retry_manager.reset_attempts().await;
    }

    /// Increment the retry attempts counter and return the new value
    pub async fn increment_retry_attempts(&self) -> u32 {
        self.retry_manager.increment_attempts().await
    }

    /// Get the current retry attempts count
    pub async fn get_retry_attempts(&self) -> u32 {
        self.retry_manager.get_attempts().await
    }

    async fn handle_retry_logic(
        &self,
        messages: &mut Conversation,
        session_config: &SessionConfig,
        initial_messages: &[Message],
    ) -> Result<bool> {
        let result = self
            .retry_manager
            .handle_retry_logic(
                messages,
                session_config,
                initial_messages,
                &self.final_output_tool,
            )
            .await?;

        match result {
            RetryResult::Retried => Ok(true),
            RetryResult::Skipped
            | RetryResult::MaxAttemptsReached
            | RetryResult::SuccessChecksPassed => Ok(false),
        }
    }
    async fn drain_elicitation_messages(&self, session_id: &str) -> Vec<Message> {
        let mut messages = Vec::new();
        let manager = self.config.session_manager.clone();
        let mut elicitation_rx = ActionRequiredManager::global().request_rx.lock().await;
        while let Ok(elicitation_message) = elicitation_rx.try_recv() {
            if let Err(e) = manager.add_message(session_id, &elicitation_message).await {
                warn!("Failed to save elicitation message to session: {}", e);
            }
            messages.push(elicitation_message);
        }
        messages
    }

    async fn prepare_reply_context(
        &self,
        session_id: &str,
        unfixed_conversation: Conversation,
        working_dir: &std::path::Path,
    ) -> Result<ReplyContext> {
        let unfixed_messages = unfixed_conversation.messages().clone();
        let (conversation, issues) = fix_conversation(unfixed_conversation.clone());
        if !issues.is_empty() {
            debug!(
                "Conversation issue fixed: {}",
                debug_conversation_fix(
                    unfixed_messages.as_slice(),
                    conversation.messages(),
                    &issues
                )
            );
        }
        let initial_messages = conversation.messages().clone();

        let mut conversation = conversation;
        if let Some(context) = self
            .explicit_resource_context(session_id, conversation.messages())
            .await
        {
            conversation.push(
                Message::user()
                    .with_text(format!(
                        "<explicit-resource-context>\n{context}\n</explicit-resource-context>"
                    ))
                    .with_visibility(false, true),
            );
        }

        let (tools, toolshim_tools, system_prompt) = self
            .prepare_tools_and_prompt(session_id, working_dir)
            .await?;

        Ok(ReplyContext {
            conversation,
            tools,
            toolshim_tools,
            system_prompt,
            biorouter_mode: self.config.biorouter_mode,
            initial_messages,
        })
    }

    async fn explicit_resource_context(
        &self,
        session_id: &str,
        messages: &[Message],
    ) -> Option<String> {
        let latest_user_text = messages
            .iter()
            .rev()
            .find(|message| {
                message.role == rmcp::model::Role::User && message.metadata.user_visible
            })
            .map(Message::as_concat_text)?;

        let refs = extract_resource_refs(&latest_user_text);
        if refs.is_empty() {
            return None;
        }

        let mut sections = Vec::new();

        if !refs.skills.is_empty() {
            sections.push(self.skill_resource_context(session_id, &refs).await);
        }

        if !refs.extensions.is_empty() {
            sections.push(
                self.extension_resource_context(session_id, &refs.extensions)
                    .await,
            );
        }

        if !refs.knowledge_bases.is_empty() {
            sections.push(
                self.knowledge_resource_context(session_id, &latest_user_text, &refs)
                    .await,
            );
        }

        Some(sections.join("\n\n"))
    }

    async fn skill_resource_context(&self, session_id: &str, refs: &ResourceRefs) -> String {
        let mut output = format!(
            "The user explicitly selected these skills for this request: {}.\n\
             Treat these selected skills as mandatory. Use the loaded skill instructions below before answering or taking action.",
            refs.skills
                .iter()
                .map(|name| format!("`{name}`"))
                .collect::<Vec<_>>()
                .join(", ")
        );

        for skill in &refs.skills {
            output.push_str(&format!("\n\n## Loaded skill: {skill}\n"));

            // BR-8: a skill body is inlined in full only the first turn it is
            // selected. On later turns the full text (megabytes across a
            // skill-heavy session) is replaced by a short pointer — the skill
            // stays mandatory and the model can re-read it on demand.
            if self.skill_already_injected(session_id, skill).await {
                output.push_str(skill_already_loaded_pointer());
                continue;
            }

            match self
                .call_prefetch_tool(
                    session_id,
                    "skills__loadSkill",
                    object!({ "name": skill.clone() }),
                )
                .await
            {
                Ok(text) => {
                    // Cap a single body against the BR-2 injection budget so a
                    // pathological SKILL.md can't blow the window on its own.
                    output.push_str(&crate::context_budget::truncate_to_tokens(
                        &text,
                        crate::context_budget::max_skill_body_tokens(),
                        &format!("selected skill `{skill}`"),
                    ));
                    self.mark_skill_injected(session_id, skill).await;
                }
                Err(error) => output.push_str(&format!(
                    "Could not load this selected skill: {error}. Tell the user instead of silently substituting another skill."
                )),
            }
        }

        output
    }

    /// Whether `skill`'s full body was already inlined earlier in this session
    /// (BR-8 cache).
    async fn skill_already_injected(&self, session_id: &str, skill: &str) -> bool {
        self.injected_skills
            .lock()
            .await
            .get(session_id)
            .is_some_and(|injected| injected.contains(skill))
    }

    /// Record that `skill`'s full body has now been inlined for this session, so
    /// later turns inject only a pointer (BR-8).
    async fn mark_skill_injected(&self, session_id: &str, skill: &str) {
        self.injected_skills
            .lock()
            .await
            .entry(session_id.to_string())
            .or_default()
            .insert(skill.to_string());
    }

    async fn extension_resource_context(&self, session_id: &str, extensions: &[String]) -> String {
        let mut selected = Vec::new();
        let mut notes = Vec::new();

        for requested in extensions {
            let requested = requested.trim();
            if requested.is_empty() {
                continue;
            }

            let canonical = canonical_builtin_extension_name(requested)
                .unwrap_or_else(|| requested.to_string());
            let normalized = normalize(&canonical);

            if !self
                .extension_manager
                .is_extension_enabled(&normalized)
                .await
            {
                let is_builtin = biorouter_mcp::BUILTIN_EXTENSIONS.contains_key(canonical.as_str())
                    || crate::agents::extension::PLATFORM_EXTENSIONS
                        .contains_key(normalized.as_str());
                if is_builtin {
                    let config = ExtensionConfig::Builtin {
                        name: canonical.clone(),
                        description: format!(
                            "Selected via explicit resource marker /ext:{canonical}"
                        ),
                        display_name: None,
                        timeout: Some(300),
                        bundled: Some(true),
                        available_tools: Vec::new(),
                    };
                    match self.add_extension(config).await {
                        Ok(()) => {
                            if let Err(error) = self.persist_extension_state(session_id).await {
                                notes.push(format!(
                                    "`{canonical}` was enabled for this turn but its session state could not be persisted: {error}"
                                ));
                            } else {
                                notes.push(format!(
                                    "`{canonical}` was enabled because the user selected it explicitly."
                                ));
                            }
                        }
                        Err(error) => notes.push(format!(
                            "`{canonical}` could not be enabled: {error}. Tell the user instead of silently substituting another extension."
                        )),
                    }
                } else {
                    notes.push(format!(
                        "`{canonical}` is not currently enabled and is not a known built-in extension. Tell the user instead of silently substituting another extension."
                    ));
                }
            }

            selected.push(canonical);
        }

        let mut output = format!(
            "The user explicitly selected these extensions for this request: {}.\n\
             Treat these selected extensions as mandatory. Use tools from these extensions when the request needs tool use. Tool names are prefixed with the extension name and `__`; if a selected extension is unavailable, say so plainly.",
            selected
                .iter()
                .map(|name| format!("`{name}`"))
                .collect::<Vec<_>>()
                .join(", ")
        );

        if !notes.is_empty() {
            output.push_str("\n\n");
            output.push_str(&notes.join("\n"));
        }

        output
    }

    async fn knowledge_resource_context(
        &self,
        session_id: &str,
        user_text: &str,
        refs: &ResourceRefs,
    ) -> String {
        let mut output = String::from(
            "The user explicitly selected the following knowledge base(s). Use these results as the primary knowledge context for this request. If more context is needed, call `knowledge__kb_search` with the same exact `kb_id`.",
        );

        for kb in &refs.knowledge_bases {
            output.push_str(&format!("\n\n## Knowledge base: `{}`\n", kb.id));
            match self
                .call_prefetch_tool(
                    session_id,
                    "knowledge__kb_search",
                    object!({
                        "kb_id": kb.id.clone(),
                        "query": user_text,
                        "limit": 5
                    }),
                )
                .await
            {
                Ok(text) => output.push_str(&text),
                Err(error) => output.push_str(&format!(
                    "Could not search this selected knowledge base: {error}. Tell the user instead of silently searching a different knowledge base."
                )),
            }
        }

        output
    }

    async fn call_prefetch_tool(
        &self,
        session_id: &str,
        tool_name: &str,
        arguments: serde_json::Map<String, Value>,
    ) -> Result<String> {
        let tool = self
            .extension_manager
            .dispatch_tool_call(
                session_id,
                CallToolRequestParams {
                    name: tool_name.to_string().into(),
                    arguments: Some(arguments),
                    meta: None,
                    task: None,
                },
                CancellationToken::default(),
            )
            .await
            .map_err(|e| anyhow!(e.to_string()))?;

        let result = tool.result.await.map_err(|e| anyhow!(e.message))?;
        let text = result
            .content
            .iter()
            .filter_map(|content| content.as_text().map(|text| text.text.as_ref()))
            .collect::<Vec<_>>()
            .join("\n\n");

        if text.is_empty() {
            Ok("(The selected resource returned no text.)".to_string())
        } else {
            Ok(text)
        }
    }

    async fn categorize_tools(
        &self,
        response: &Message,
        tools: &[rmcp::model::Tool],
    ) -> ToolCategorizeResult {
        // Categorize tool requests
        let (frontend_requests, remaining_requests, filtered_response) =
            self.categorize_tool_requests(response, tools).await;

        ToolCategorizeResult {
            frontend_requests,
            remaining_requests,
            filtered_response,
        }
    }

    /// Assemble the per-turn model context by injecting MOIM ("message of the
    /// moment") into a clone of the live conversation, leaving persisted history untouched.
    async fn assemble_turn_context(
        &self,
        session_id: &str,
        conversation: &Conversation,
        working_dir: &std::path::Path,
    ) -> Conversation {
        super::moim::inject_moim(
            session_id,
            conversation.clone(),
            &self.extension_manager,
            working_dir,
        )
        .await
    }

    /// Run the per-tool inspection gauntlet (inspectors → permission judge →
    /// extension-enable tracking) and eagerly dispatch approved/denied tools,
    /// returning the inspection results, permission verdict, enable-extension
    /// request ids, and the pending tool futures.
    async fn inspect_and_gate_tool_requests(
        &self,
        remaining_requests: &[ToolRequest],
        conversation: &Conversation,
        biorouter_mode: BioRouterMode,
        session: &Session,
        request_to_response_map: &HashMap<String, Arc<Mutex<Message>>>,
        cancel_token: Option<CancellationToken>,
    ) -> Result<(
        Vec<InspectionResult>,
        PermissionCheckResult,
        Vec<String>,
        Vec<(String, ToolStream)>,
    )> {
        // Run all tool inspectors
        let inspection_results = self
            .tool_inspection_manager
            .inspect_tools(
                remaining_requests,
                conversation.messages(),
                biorouter_mode,
                session,
            )
            .await?;

        let permission_check_result = self
            .tool_inspection_manager
            .process_inspection_results_with_permission_inspector(
                remaining_requests,
                &inspection_results,
            )
            .unwrap_or_else(|| {
                let mut result = PermissionCheckResult {
                    approved: vec![],
                    needs_approval: vec![],
                    denied: vec![],
                };
                result
                    .needs_approval
                    .extend(remaining_requests.iter().cloned());
                result
            });

        // Track extension requests
        let mut enable_extension_request_ids = vec![];
        for request in remaining_requests {
            if let Ok(tool_call) = &request.tool_call {
                if tool_call.name == MANAGE_EXTENSIONS_TOOL_NAME_COMPLETE {
                    enable_extension_request_ids.push(request.id.clone());
                }
            }
        }

        let tool_futures = self
            .handle_approved_and_denied_tools(
                &permission_check_result,
                request_to_response_map,
                cancel_token,
                session,
                &inspection_results,
            )
            .await?;

        Ok((
            inspection_results,
            permission_check_result,
            enable_extension_request_ids,
            tool_futures,
        ))
    }

    /// Integrate one completed tool result: validate it before persistence, note
    /// extension-install failures, record it for PostToolUse hooks, and write it
    /// into the request's response slot.
    #[allow(clippy::too_many_arguments)]
    async fn integrate_tool_result(
        &self,
        request_id: String,
        output: ToolResult<CallToolResult>,
        enable_extension_request_ids: &[String],
        request_to_response_map: &HashMap<String, Arc<Mutex<Message>>>,
        request_metadata: &HashMap<String, Option<ProviderMetadata>>,
        all_install_successful: &mut bool,
        post_tool_results: &mut Vec<(String, Option<Value>, Option<String>)>,
        tool_output_guardrail: crate::guardrails::tool_output::ToolOutputGuardrailMode,
    ) {
        let output = call_tool_result::validate(output);

        // Scan tool output for injection markers + PII/PHI before it re-enters
        // the model context. Default policy is annotate-only (never blocks or
        // drops content); masking is opt-in. Off is a zero-cost pass-through.
        let (output, guardrail_summary) =
            crate::guardrails::tool_output::guard_tool_result(output, tool_output_guardrail);
        if let Some(summary) = &guardrail_summary {
            debug!(request_id = %request_id, "tool-output guardrail flagged: {summary}");
        }

        if enable_extension_request_ids.contains(&request_id) && output.is_err() {
            *all_install_successful = false;
        }
        {
            let (response_value, error_text) = match &output {
                Ok(res) => {
                    let value = serde_json::to_value(res).ok();
                    if res.is_error == Some(true) {
                        let text = value
                            .as_ref()
                            .map(|v| v.to_string())
                            .unwrap_or_else(|| "tool returned an error".to_string());
                        (value, Some(text))
                    } else {
                        (value, None)
                    }
                }
                Err(e) => (None, Some(e.to_string())),
            };
            post_tool_results.push((request_id.clone(), response_value, error_text));
        }
        if let Some(response_msg) = request_to_response_map.get(&request_id) {
            let metadata = request_metadata.get(&request_id).and_then(|m| m.as_ref());
            let mut response = response_msg.lock().await;
            *response = response
                .clone()
                .with_tool_response_with_metadata(request_id, output, metadata);
        }
    }

    /// Record this turn's provider usage exactly once for token accounting
    /// (no-op when the turn reported none, e.g. an error before the first usage chunk).
    async fn record_turn_usage(
        &self,
        session_config: &SessionConfig,
        turn_usage: Option<crate::providers::base::ProviderUsage>,
    ) -> Result<()> {
        if let Some(usage) = turn_usage {
            self.update_session_metrics(session_config, &usage, false)
                .await?;
        }
        Ok(())
    }

    /// BR-28: the turn-boundary settle for observe-only (`fire`d) hook events —
    /// Notification, SubagentStart/Stop, Pre/PostCompact.
    ///
    /// Those hooks used to be spawned detached with their whole `HookAggregate`
    /// dropped, so a `systemMessage` was invisible, a failing hook untraceable,
    /// and the task could outlive the turn. Now the boundary joins whatever has
    /// finished (bounded by [`crate::hooks::FIRE_JOIN_BUDGET`], so a slow hook
    /// delays only its own observability, never the loop) and turns each captured
    /// aggregate into user-visible inline notices. Errors are already logged by
    /// `dispatch`; hooks fired here stay observe-only, so any `decision` they
    /// return is deliberately not honored.
    async fn settle_fired_hooks(&self, session_id: &str) -> Vec<Message> {
        self.hooks_manager
            .settle_fired(session_id, crate::hooks::FIRE_JOIN_BUDGET)
            .await
            .into_iter()
            .flat_map(|outcome| {
                let event = outcome.event;
                outcome
                    .aggregate
                    .system_messages
                    .into_iter()
                    .map(move |msg| {
                        debug!("hooks: surfacing {event} systemMessage");
                        Message::assistant()
                            .with_system_notification(SystemNotificationType::InlineMessage, msg)
                            .user_only()
                    })
            })
            .collect()
    }

    /// BR-12: move auto-compaction off the user-visible critical path.
    ///
    /// Called at the turn boundary — after [`Self::record_turn_usage`] has
    /// written this turn's provider-reported token count and the reply loop has
    /// finished — so that if the session is now over the compaction threshold the
    /// (multi-second) summarization LLM round-trip runs in a detached
    /// `tokio::spawn` *between* turns instead of stalling the start of the next
    /// turn. The next turn then starts from an already-compacted history.
    ///
    /// The synchronous compaction at the top of `reply()` stays as the fallback:
    /// it fires when this background swap hasn't landed yet (a huge single turn, a
    /// fast follow-up message, or a failed task), so a session can never overflow
    /// even if eager compaction lags. That is the "keep a synchronous fallback"
    /// phasing BR-12 calls for; a later phase can lower the synchronous path to a
    /// 95%-budget no-LLM hard-drop floor.
    ///
    /// Idempotent per session: a second call while a compaction is in flight for
    /// the same session is a no-op, so the loop can call this freely.
    pub(super) fn maybe_spawn_eager_compaction(
        &self,
        session_config: &SessionConfig,
        working_dir: &std::path::Path,
    ) {
        if !crate::context_mgmt::eager_compaction_enabled() {
            return;
        }

        // At most one background compaction per session. `insert` returns false
        // when the id was already present (a task is running) — bail then.
        {
            let mut in_flight = self
                .eager_compactions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !in_flight.insert(session_config.id.clone()) {
                return;
            }
        }

        let provider = match self.provider.try_lock() {
            Ok(guard) => guard.clone(),
            // Provider is momentarily locked; skip — the synchronous fallback
            // still covers this session next turn.
            Err(_) => None,
        };
        let Some(provider) = provider else {
            self.clear_eager_compaction(&session_config.id);
            return;
        };

        let session_manager = self.config.session_manager.clone();
        let hooks_manager = Arc::clone(&self.hooks_manager);
        let in_flight = Arc::clone(&self.eager_compactions);
        let session_config = session_config.clone();
        let working_dir = working_dir.to_path_buf();
        let threshold = Config::global()
            .get_param::<f64>("BIOROUTER_AUTO_COMPACT_THRESHOLD")
            .unwrap_or(DEFAULT_COMPACTION_THRESHOLD);
        let session_id = session_config.id.clone();

        tokio::spawn(async move {
            // Remove the in-flight marker no matter how the task exits.
            let _guard = EagerCompactionGuard {
                session_id: session_id.clone(),
                in_flight,
            };

            // Fire PreCompact only when compaction actually proceeds (the routine
            // calls this back after its threshold check passes) — never on a turn
            // that ended under budget.
            let precompact_hooks = Arc::clone(&hooks_manager);
            let precompact_id = session_id.clone();
            let precompact_dir = working_dir.clone();
            let on_before_compact = move || {
                fire_compaction_hook_on(
                    &precompact_hooks,
                    crate::hooks::HookEvent::PreCompact,
                    &precompact_id,
                    &precompact_dir,
                    "auto",
                    Some("eager"),
                );
            };

            match crate::context_mgmt::run_eager_compaction(
                provider,
                session_manager,
                session_config,
                threshold,
                on_before_compact,
            )
            .await
            {
                Ok(crate::context_mgmt::EagerCompactionOutcome::Swapped) => {
                    info!("BR-12: eager compaction swapped in for session {session_id}");
                    fire_compaction_hook_on(
                        &hooks_manager,
                        crate::hooks::HookEvent::PostCompact,
                        &session_id,
                        &working_dir,
                        "auto",
                        Some("eager"),
                    );
                }
                Ok(_) => {}
                Err(e) => {
                    warn!("BR-12: eager compaction failed for session {session_id}: {e}");
                }
            }
        });
    }

    /// Clear the in-flight marker for a session's eager compaction. Used on the
    /// early-return path in [`Self::maybe_spawn_eager_compaction`] before any task
    /// was spawned (the spawned task clears it via [`EagerCompactionGuard`]).
    fn clear_eager_compaction(&self, session_id: &str) {
        self.eager_compactions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(session_id);
    }

    async fn handle_approved_and_denied_tools(
        &self,
        permission_check_result: &PermissionCheckResult,
        request_to_response_map: &HashMap<String, Arc<Mutex<Message>>>,
        cancel_token: Option<tokio_util::sync::CancellationToken>,
        session: &Session,
        inspection_results: &[InspectionResult],
    ) -> Result<Vec<(String, ToolStream)>> {
        let mut tool_futures: Vec<(String, ToolStream)> = Vec::new();

        // Handle pre-approved and read-only tools
        for request in &permission_check_result.approved {
            if let Ok(tool_call) = request.tool_call.clone() {
                let (req_id, tool_result) = self
                    .dispatch_tool_call(
                        tool_call,
                        request.id.clone(),
                        cancel_token.clone(),
                        session,
                    )
                    .await;

                tool_futures.push((
                    req_id,
                    match tool_result {
                        Ok(result) => tool_stream(
                            result
                                .notification_stream
                                .unwrap_or_else(|| Box::new(stream::empty())),
                            result.result,
                        ),
                        Err(e) => {
                            tool_stream(Box::new(stream::empty()), futures::future::ready(Err(e)))
                        }
                    },
                ));
            }
        }

        Self::handle_denied_tools(
            permission_check_result,
            request_to_response_map,
            inspection_results,
        )
        .await;
        Ok(tool_futures)
    }

    async fn handle_denied_tools(
        permission_check_result: &PermissionCheckResult,
        request_to_response_map: &HashMap<String, Arc<Mutex<Message>>>,
        inspection_results: &[InspectionResult],
    ) {
        for request in &permission_check_result.denied {
            if let Some(response_msg) = request_to_response_map.get(&request.id) {
                // When an inspector denied this call, tell the model why so it
                // can adjust instead of blindly retrying. The always-on
                // catastrophic-command block (security inspector) and hook denials
                // carry a reason; surface it verbatim / with context.
                let deny_reason = inspection_results.iter().find(|result| {
                    result.tool_request_id == request.id
                        && result.action == InspectionAction::Deny
                        && !result.reason.trim().is_empty()
                });
                let response_text = match deny_reason {
                    Some(result)
                        if result.inspector_name
                            == crate::hooks::inspector::HOOK_INSPECTOR_NAME =>
                    {
                        format!("{DECLINED_RESPONSE}\n\nHook feedback: {}", result.reason)
                    }
                    // Non-bypassable safety block: the user did not decline, the
                    // command is refused outright, so return the reason directly.
                    Some(result) if result.inspector_name == "security" => result.reason.clone(),
                    _ => DECLINED_RESPONSE.to_string(),
                };
                let mut response = response_msg.lock().await;
                *response = response.clone().with_tool_response_with_metadata(
                    request.id.clone(),
                    Ok(CallToolResult {
                        content: vec![rmcp::model::Content::text(response_text)],
                        structured_content: None,
                        is_error: Some(true),
                        meta: None,
                    }),
                    request.metadata.as_ref(),
                );
            }
        }
    }

    /// Get a reference count clone to the provider
    pub async fn provider(&self) -> Result<Arc<dyn Provider>, anyhow::Error> {
        match &*self.provider.lock().await {
            Some(provider) => Ok(Arc::clone(provider)),
            None => Err(anyhow!("Provider not set")),
        }
    }

    /// Check if a tool is a frontend tool
    pub async fn is_frontend_tool(&self, name: &str) -> bool {
        self.frontend_tools.lock().await.contains_key(name)
    }

    /// Get a reference to a frontend tool
    pub async fn get_frontend_tool(&self, name: &str) -> Option<FrontendTool> {
        self.frontend_tools.lock().await.get(name).cloned()
    }

    pub async fn add_final_output_tool(&self, response: Response) {
        let mut final_output_tool = self.final_output_tool.lock().await;
        let created_final_output_tool = FinalOutputTool::new(response);
        let final_output_system_prompt = created_final_output_tool.system_prompt();
        *final_output_tool = Some(created_final_output_tool);
        self.extend_system_prompt(final_output_system_prompt).await;
    }

    pub async fn add_sub_workflows(&self, sub_workflows_to_add: Vec<SubWorkflow>) {
        let mut sub_workflows = self.sub_workflows.lock().await;
        for sr in sub_workflows_to_add {
            sub_workflows.insert(sr.name.clone(), sr);
        }
    }

    pub async fn apply_workflow_components(
        &self,
        sub_workflows: Option<Vec<SubWorkflow>>,
        response: Option<Response>,
        include_final_output: bool,
    ) {
        if let Some(sub_workflows) = sub_workflows {
            self.add_sub_workflows(sub_workflows).await;
        }

        if include_final_output {
            if let Some(response) = response {
                self.add_final_output_tool(response).await;
            }
        }
    }

    /// Dispatch a single tool call to the appropriate client
    #[instrument(skip(self, tool_call, request_id), fields(input, output))]
    #[allow(clippy::too_many_lines)]
    pub async fn dispatch_tool_call(
        &self,
        mut tool_call: CallToolRequestParams,
        request_id: String,
        cancellation_token: Option<CancellationToken>,
        session: &Session,
    ) -> (String, Result<ToolCallResult, ErrorData>) {
        // Prevent subagents from creating other subagents
        if session.session_type == SessionType::SubAgent && tool_call.name == SUBAGENT_TOOL_NAME {
            return (
                request_id,
                Err(ErrorData::new(
                    ErrorCode::INVALID_REQUEST,
                    "Subagents cannot create other subagents".to_string(),
                    None,
                )),
            );
        }

        if tool_call.name == PLATFORM_MANAGE_SCHEDULE_TOOL_NAME {
            let arguments = tool_call
                .arguments
                .map(Value::Object)
                .unwrap_or(Value::Object(serde_json::Map::new()));
            let result = self
                .handle_schedule_management(arguments, request_id.clone())
                .await;
            let wrapped_result = result.map(|content| CallToolResult {
                content,
                structured_content: None,
                is_error: Some(false),
                meta: None,
            });
            return (request_id, Ok(ToolCallResult::from(wrapped_result)));
        }

        if tool_call.name == PLATFORM_INGEST_CONVERSATION_TOOL_NAME {
            let arguments = tool_call
                .arguments
                .map(Value::Object)
                .unwrap_or(Value::Object(serde_json::Map::new()));
            let result = self.handle_ingest_conversation(arguments, session).await;
            let wrapped_result = result.map(|content| CallToolResult {
                content,
                structured_content: None,
                is_error: Some(false),
                meta: None,
            });
            return (request_id, Ok(ToolCallResult::from(wrapped_result)));
        }

        if tool_call.name == FINAL_OUTPUT_TOOL_NAME {
            return if let Some(final_output_tool) = self.final_output_tool.lock().await.as_mut() {
                let result = final_output_tool.execute_tool_call(tool_call.clone()).await;
                (request_id, Ok(result))
            } else {
                (
                    request_id,
                    Err(ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        "Final output tool not defined".to_string(),
                        None,
                    )),
                )
            };
        }

        debug!("WAITING_TOOL_START: {}", tool_call.name);
        let result: ToolCallResult = if tool_call.name == SUBAGENT_TOOL_NAME {
            let provider = match self.provider().await {
                Ok(p) => p,
                Err(_) => {
                    return (
                        request_id,
                        Err(ErrorData::new(
                            ErrorCode::INTERNAL_ERROR,
                            "Provider is required".to_string(),
                            None,
                        )),
                    );
                }
            };

            let extensions = self.get_extension_configs().await;
            let task_config =
                TaskConfig::new(provider, &session.id, &session.working_dir, extensions);
            let sub_workflows = self.sub_workflows.lock().await.clone();

            let arguments = tool_call
                .arguments
                .clone()
                .map(Value::Object)
                .unwrap_or(Value::Object(serde_json::Map::new()));

            handle_subagent_tool(
                &self.config,
                arguments,
                task_config,
                sub_workflows,
                session.working_dir.clone(),
                cancellation_token,
            )
        } else if self.is_frontend_tool(&tool_call.name).await {
            // For frontend tools, return an error indicating we need frontend execution
            ToolCallResult::from(Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                "Frontend tool execution required".to_string(),
                None,
            )))
        } else {
            // BRSDK encryption: resolve {{vault:NAME}} secrets ONLY here — on the
            // leaf MCP-dispatch path, after the model produced the call and right
            // before the tool runs. Deliberately NOT applied to the subagent /
            // frontend / final_output / schedule branches above, whose arguments
            // are re-consumed by an LLM, returned to the browser, or persisted — a
            // resolved secret there would leak. No-op unless a vault is installed.
            self.apply_vault(&mut tool_call).await;

            // Clone the result to ensure no references to extension_manager are returned
            let result = self
                .extension_manager
                .dispatch_tool_call(
                    &session.id,
                    tool_call.clone(),
                    cancellation_token.unwrap_or_default(),
                )
                .await;
            result.unwrap_or_else(|e| {
                // Try to downcast to ErrorData to avoid double wrapping
                let error_data = e.downcast::<ErrorData>().unwrap_or_else(|e| {
                    ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
                });
                ToolCallResult::from(Err(error_data))
            })
        };

        debug!("WAITING_TOOL_END: {}", tool_call.name);

        (
            request_id,
            Ok(ToolCallResult {
                notification_stream: result.notification_stream,
                result: Box::new(
                    result
                        .result
                        .map(super::large_response_handler::process_tool_response),
                ),
            }),
        )
    }

    /// Save current extension state to session metadata
    /// Should be called after any extension add/remove operation
    pub async fn save_extension_state(&self, session: &SessionConfig) -> Result<()> {
        let extension_configs = self.extension_manager.get_extension_configs().await;

        let extensions_state = EnabledExtensionsState::new(extension_configs);

        let session_manager = self.config.session_manager.clone();
        let mut session_data = session_manager.get_session(&session.id, false).await?;

        if let Err(e) = extensions_state.to_extension_data(&mut session_data.extension_data) {
            warn!("Failed to serialize extension state: {}", e);
            return Err(anyhow!("Extension state serialization failed: {}", e));
        }

        session_manager
            .update(&session.id)
            .extension_data(session_data.extension_data)
            .apply()
            .await?;

        Ok(())
    }

    /// Save current extension state to session by session_id
    pub async fn persist_extension_state(&self, session_id: &str) -> Result<()> {
        let extension_configs = self.extension_manager.get_extension_configs().await;
        let extensions_state = EnabledExtensionsState::new(extension_configs);

        let session_manager = self.config.session_manager.clone();
        let session = session_manager.get_session(session_id, false).await?;
        let mut extension_data = session.extension_data.clone();

        extensions_state
            .to_extension_data(&mut extension_data)
            .map_err(|e| anyhow!("Failed to serialize extension state: {}", e))?;

        session_manager
            .update(session_id)
            .extension_data(extension_data)
            .apply()
            .await?;

        Ok(())
    }

    /// Load extensions from session into the agent
    /// Skips extensions that are already loaded
    pub async fn load_extensions_from_session(
        self: &Arc<Self>,
        session: &Session,
    ) -> Vec<ExtensionLoadResult> {
        // Bind extensions to the session's working directory so the shell tool
        // (and child-process extensions) run where the user is working. The GUI
        // folder picker persists the new dir and restarts the agent, which
        // re-enters this path, so the change takes effect on the next load.
        self.extension_manager
            .set_working_dir(session.working_dir.clone())
            .await;

        let session_extensions =
            EnabledExtensionsState::from_extension_data(&session.extension_data);
        let enabled_configs = match session_extensions {
            Some(state) => state.extensions,
            None => {
                tracing::warn!(
                    "No extensions found in session {}. This is unexpected.",
                    session.id
                );
                return vec![];
            }
        };

        let extension_futures = enabled_configs
            .into_iter()
            .map(|config| {
                let config_clone = config.clone();
                let agent_ref = self.clone();

                async move {
                    let name = config_clone.name().to_string();
                    let normalized_name = normalize(&name);

                    if agent_ref
                        .extension_manager
                        .is_extension_enabled(&normalized_name)
                        .await
                    {
                        tracing::debug!("Extension {} already loaded, skipping", name);
                        return ExtensionLoadResult {
                            name,
                            success: true,
                            error: None,
                        };
                    }

                    match agent_ref.add_extension(config_clone).await {
                        Ok(_) => ExtensionLoadResult {
                            name,
                            success: true,
                            error: None,
                        },
                        Err(e) => {
                            let error_msg = e.to_string();
                            warn!("Failed to load extension {}: {}", name, error_msg);
                            ExtensionLoadResult {
                                name,
                                success: false,
                                error: Some(error_msg),
                            }
                        }
                    }
                }
            })
            .collect::<Vec<_>>();

        futures::future::join_all(extension_futures).await
    }

    pub async fn add_extension(&self, extension: ExtensionConfig) -> ExtensionResult<()> {
        match &extension {
            ExtensionConfig::Frontend {
                tools,
                instructions,
                ..
            } => {
                // For frontend tools, just store them in the frontend_tools map
                let mut frontend_tools = self.frontend_tools.lock().await;
                for tool in tools {
                    let frontend_tool = FrontendTool {
                        name: tool.name.to_string(),
                        tool: tool.clone(),
                    };
                    frontend_tools.insert(tool.name.to_string(), frontend_tool);
                }
                // Store instructions if provided, using "frontend" as the key
                let mut frontend_instructions = self.frontend_instructions.lock().await;
                if let Some(instructions) = instructions {
                    *frontend_instructions = Some(instructions.clone());
                } else {
                    // Default frontend instructions if none provided
                    *frontend_instructions = Some(
                        "The following tools are provided directly by the frontend and will be executed by the frontend when called.".to_string(),
                    );
                }
            }
            _ => {
                self.extension_manager
                    .add_extension(extension.clone())
                    .await?;
            }
        }

        Ok(())
    }

    pub async fn subagents_enabled(&self, session_id: &str) -> bool {
        if self.config.biorouter_mode != BioRouterMode::Auto {
            return false;
        }
        if self
            .provider()
            .await
            .map(|provider| provider.get_active_model_name().starts_with("gemini"))
            .unwrap_or(false)
        {
            return false;
        }
        let context = self.extension_manager.get_context();
        if matches!(
            context
                .session_manager
                .get_session(session_id, false)
                .await
                .ok()
                .map(|session| session.session_type),
            Some(SessionType::SubAgent)
        ) {
            return false;
        }
        !self
            .extension_manager
            .list_extensions()
            .await
            .map(|ext| ext.is_empty())
            .unwrap_or(true)
    }

    pub async fn list_tools(&self, session_id: &str, extension_name: Option<String>) -> Vec<Tool> {
        let mut prefixed_tools = self
            .extension_manager
            .get_prefixed_tools(extension_name.clone())
            .await
            .unwrap_or_default();

        let subagents_enabled = self.subagents_enabled(session_id).await;
        if (extension_name.is_none() || extension_name.as_deref() == Some("platform"))
            && self.config.scheduler_service.is_some()
        {
            prefixed_tools.push(platform_tools::manage_schedule_tool());
        }

        // The conversation-ingestion tool is always available on the platform
        // extension: it needs only the session store (always present) and the
        // agent's provider, which is checked at call time.
        if extension_name.is_none() || extension_name.as_deref() == Some("platform") {
            prefixed_tools.push(platform_tools::ingest_conversation_tool());
        }

        if extension_name.is_none() {
            if let Some(final_output_tool) = self.final_output_tool.lock().await.as_ref() {
                prefixed_tools.push(final_output_tool.tool());
            }

            if subagents_enabled {
                let sub_workflows = self.sub_workflows.lock().await;
                let sub_workflows_vec: Vec<_> = sub_workflows.values().cloned().collect();
                prefixed_tools.push(create_subagent_tool(&sub_workflows_vec));
            }
        }

        prefixed_tools
    }

    pub async fn remove_extension(&self, name: &str) -> Result<()> {
        self.extension_manager.remove_extension(name).await?;
        Ok(())
    }

    pub async fn list_extensions(&self) -> Vec<String> {
        self.extension_manager
            .list_extensions()
            .await
            .expect("Failed to list extensions")
    }

    pub async fn get_extension_configs(&self) -> Vec<ExtensionConfig> {
        self.extension_manager.get_extension_configs().await
    }

    /// Handle a confirmation response for a tool request
    pub async fn handle_confirmation(
        &self,
        request_id: String,
        confirmation: PermissionConfirmation,
    ) {
        if let Err(e) = self.confirmation_tx.send((request_id, confirmation)).await {
            error!("Failed to send confirmation: {}", e);
        }
    }

    #[instrument(skip(self, user_message, session_config), fields(user_message))]
    #[allow(clippy::too_many_lines)]
    pub async fn reply(
        &self,
        user_message: Message,
        session_config: SessionConfig,
        cancel_token: Option<CancellationToken>,
    ) -> Result<BoxStream<'_, Result<AgentEvent>>> {
        let session_manager = self.config.session_manager.clone();

        for content in &user_message.content {
            if let MessageContent::ActionRequired(action_required) = content {
                if let ActionRequiredData::ElicitationResponse { id, user_data } =
                    &action_required.data
                {
                    if let Err(e) = ActionRequiredManager::global()
                        .submit_response(id.clone(), user_data.clone())
                        .await
                    {
                        // No live request is waiting on this id. The usual cause
                        // is a daemon restart between the elicitation and the
                        // reply: the in-memory pending request — and the tool
                        // call parked on it — died with the old process, so the
                        // answer has nowhere to go (BR-41). Surface that the run
                        // was interrupted instead of a raw error, and still keep
                        // the reply in history.
                        tracing::warn!("Elicitation response for {id} could not be delivered: {e}");
                        session_manager
                            .add_message(&session_config.id, &user_message)
                            .await?;
                        let notice = Message::assistant()
                            .with_system_notification(
                                SystemNotificationType::InlineMessage,
                                "The request that was waiting for your input was interrupted \
                                 (most likely by a restart), so your answer couldn't be \
                                 delivered. Please re-send your request to continue.",
                            )
                            .user_only();
                        return Ok(Box::pin(stream::once(async move {
                            Ok(AgentEvent::Message(notice))
                        })));
                    }
                    session_manager
                        .add_message(&session_config.id, &user_message)
                        .await?;
                    return Ok(Box::pin(futures::stream::empty()));
                }
            }
        }

        // A daemon restart drops the in-memory goal registry and its Stop-hook
        // judge, while the goal itself persists in the session's extension_data
        // (like todos). Restore it before handling this turn so an active /goal
        // survives the restart. No-op when the goal is already live in this
        // process or none was stored (BR-41).
        self.restore_goal(&session_config.id).await;

        let message_text = user_message.as_concat_text();

        // User-configured hooks: SessionStart fires once per session, then
        // UserPromptSubmit may block the prompt or inject context. Slash
        // commands and elicitation responses don't count as prompts.
        let mut hook_context: Option<String> = None;
        if !message_text.trim().starts_with('/') {
            if let Ok(hook_session) = session_manager.get_session(&session_config.id, false).await {
                let hooks = self.hooks_manager();
                hooks.reset_stop_blocks(&session_config.id).await;

                let source = if hook_session.message_count > 0 {
                    "resume"
                } else {
                    "startup"
                };
                let session_start_context = hooks
                    .session_start_once(&hook_session.id, &hook_session.working_dir, source)
                    .await
                    .and_then(|aggregate| aggregate.joined_context());

                let prompt_aggregate = hooks
                    .user_prompt_submit(&hook_session.id, &hook_session.working_dir, &message_text)
                    .await;

                if prompt_aggregate.is_denied() {
                    let reason = prompt_aggregate
                        .deny_reason()
                        .unwrap_or("blocked")
                        .to_string();
                    session_manager
                        .add_message(
                            &session_config.id,
                            &user_message.clone().with_visibility(true, false),
                        )
                        .await?;
                    let notice = Message::assistant()
                        .with_system_notification(
                            SystemNotificationType::InlineMessage,
                            format!("Prompt blocked by hook: {reason}"),
                        )
                        .user_only();
                    return Ok(Box::pin(stream::iter(vec![
                        Ok(AgentEvent::Message(user_message)),
                        Ok(AgentEvent::Message(notice)),
                    ])));
                }

                let mut contexts: Vec<String> = Vec::new();
                if let Some(ctx) = session_start_context {
                    contexts.push(ctx);
                }
                if let Some(ctx) = prompt_aggregate.joined_context() {
                    contexts.push(ctx);
                }
                if !contexts.is_empty() {
                    hook_context = Some(contexts.join("\n\n"));
                }
            }
        }

        // Track custom slash command usage (don't track command name for privacy)
        if message_text.trim().starts_with('/') {
            let command = message_text.split_whitespace().next();
            if let Some(cmd) = command {
                if crate::slash_commands::get_workflow_for_command(cmd).is_some() {
                    // (telemetry for custom slash command usage removed)
                }
            }
        }

        let command_result = self.execute_command(&message_text, &session_config).await;

        match command_result {
            Err(e) => {
                let error_message = Message::assistant()
                    .with_text(e.to_string())
                    .with_visibility(true, false);
                return Ok(Box::pin(stream::once(async move {
                    Ok(AgentEvent::Message(error_message))
                })));
            }
            Ok(Some(response)) if response.role == rmcp::model::Role::Assistant => {
                session_manager
                    .add_message(
                        &session_config.id,
                        &user_message.clone().with_visibility(true, false),
                    )
                    .await?;
                session_manager
                    .add_message(
                        &session_config.id,
                        &response.clone().with_visibility(true, false),
                    )
                    .await?;

                // Check if this was a command that modifies conversation history
                let modifies_history = crate::agents::execute_commands::COMPACT_TRIGGERS
                    .contains(&message_text.trim())
                    || message_text.trim() == "/clear";

                return Ok(Box::pin(async_stream::try_stream! {
                    yield AgentEvent::Message(user_message);
                    yield AgentEvent::Message(response);

                    // After commands that modify history, notify UI that history was replaced
                    if modifies_history {
                        let updated_session = session_manager.get_session(&session_config.id, true)
                            .await
                            .map_err(|e| anyhow!("Failed to fetch updated session: {}", e))?;
                        let updated_conversation = updated_session
                            .conversation
                            .ok_or_else(|| anyhow!("Session has no conversation after history modification"))?;
                        yield AgentEvent::HistoryReplaced(updated_conversation);
                    }
                }));
            }
            Ok(Some(resolved_message)) => {
                session_manager
                    .add_message(
                        &session_config.id,
                        &user_message.clone().with_visibility(true, false),
                    )
                    .await?;
                session_manager
                    .add_message(
                        &session_config.id,
                        &resolved_message.clone().with_visibility(false, true),
                    )
                    .await?;
            }
            Ok(None) => {
                session_manager
                    .add_message(&session_config.id, &user_message)
                    .await?;
            }
        }

        // Context injected by SessionStart/UserPromptSubmit hooks: visible to
        // the model, hidden from the user.
        if let Some(context) = hook_context {
            session_manager
                .add_message(
                    &session_config.id,
                    &Message::user()
                        .with_text(crate::hooks::outcome::frame_hook_context(&context))
                        .with_visibility(false, true),
                )
                .await?;
        }

        let session = session_manager
            .get_session(&session_config.id, true)
            .await?;
        let conversation = session
            .conversation
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Session {} has no conversation", session_config.id))?;

        // BR-12: this synchronous check is the *fallback*. In the common case
        // the previous turn's `maybe_spawn_eager_compaction` already compacted in
        // the background between turns, so `needs_auto_compact` is false here and
        // the turn starts immediately. It still fires — and blocks the turn — when
        // eager compaction hasn't landed (a huge single turn, a fast follow-up
        // message before the background task finished, a disabled eager path, or a
        // failed task), so a session can never overflow.
        //
        // BR-15: on the cold path (a session's first turn, or a provider that
        // doesn't report usage) the token estimate needs the system prompt and
        // tool schemas or it undercounts badly. Assemble them only when needed —
        // the happy path reads session.total_tokens and shouldn't pay for
        // tool/prompt assembly here (the reply loop re-does it anyway).
        let cold_path_tools_and_prompt = if session.total_tokens.is_none() {
            Some(
                self.prepare_tools_and_prompt(&session_config.id, &session.working_dir)
                    .await?,
            )
        } else {
            None
        };

        let needs_auto_compact = check_if_compaction_needed(
            self.provider().await?.as_ref(),
            &conversation,
            None,
            &session,
            cold_path_tools_and_prompt
                .as_ref()
                .map(|(tools, _toolshim, system_prompt)| {
                    (system_prompt.as_str(), tools.as_slice())
                }),
        )
        .await?;

        let conversation_to_compact = conversation.clone();

        Ok(Box::pin(async_stream::try_stream! {
            let final_conversation = if !needs_auto_compact {
                conversation
            } else {
                let config = Config::global();
                let threshold = config
                    .get_param::<f64>("BIOROUTER_AUTO_COMPACT_THRESHOLD")
                    .unwrap_or(DEFAULT_COMPACTION_THRESHOLD);
                let threshold_percentage = (threshold * 100.0) as u32;

                let inline_msg = format!(
                    "Exceeded auto-compact threshold of {}%. Performing auto-compaction...",
                    threshold_percentage
                );

                yield AgentEvent::Message(
                    Message::assistant().with_system_notification(
                        SystemNotificationType::InlineMessage,
                        inline_msg,
                    )
                );

                yield AgentEvent::Message(
                    Message::assistant().with_system_notification(
                        SystemNotificationType::ThinkingMessage,
                        COMPACTION_THINKING_TEXT,
                    )
                );

                self.fire_compaction_hook(
                    crate::hooks::HookEvent::PreCompact,
                    &session_config.id,
                    &session.working_dir,
                    "auto",
                    None,
                );
                match compact_messages(self.provider().await?.as_ref(), &conversation_to_compact, false).await {
                    Ok((compacted_conversation, summarization_usage)) => {
                        session_manager.replace_conversation(&session_config.id, &compacted_conversation).await?;
                        self.update_session_metrics(&session_config, &summarization_usage, true).await?;
                        self.fire_compaction_hook(
                            crate::hooks::HookEvent::PostCompact,
                            &session_config.id,
                            &session.working_dir,
                            "auto",
                            None,
                        );

                        yield AgentEvent::HistoryReplaced(compacted_conversation.clone());

                        yield AgentEvent::Message(
                            Message::assistant().with_system_notification(
                                SystemNotificationType::InlineMessage,
                                "Compaction complete",
                            )
                        );

                        compacted_conversation
                    }
                    Err(e) => {
                        yield AgentEvent::Message(
                            Message::assistant().with_text(
                                format!("Ran into this error trying to compact: {e}.\n\nPlease try again or create a new session")
                            )
                        );
                        return;
                    }
                }
            };

            let mut reply_stream = self.reply_internal(final_conversation, session_config, session, cancel_token).await?;
            while let Some(event) = reply_stream.next().await {
                yield event?;
            }
        }))
    }

    #[allow(clippy::too_many_lines)]
    async fn reply_internal(
        &self,
        conversation: Conversation,
        session_config: SessionConfig,
        session: Session,
        cancel_token: Option<CancellationToken>,
    ) -> Result<BoxStream<'_, Result<AgentEvent>>> {
        let context = self
            .prepare_reply_context(&session_config.id, conversation, &session.working_dir)
            .await?;
        let ReplyContext {
            mut conversation,
            mut tools,
            mut toolshim_tools,
            mut system_prompt,
            biorouter_mode,
            initial_messages,
        } = context;
        let reply_span = tracing::Span::current();
        self.reset_retry_attempts().await;

        let session_manager = self.config.session_manager.clone();

        let working_dir = session.working_dir.clone();
        // BR-43: stable anchor for this turn's checkpoints — the `created`
        // timestamp of the last user message (the same key `truncate_conversation`
        // uses on restore). Computed once, before the loop mutates `conversation`.
        let checkpoint_anchor_ts = conversation
            .messages()
            .iter()
            .rev()
            .find(|m| m.role == rmcp::model::Role::User)
            .map(|m| m.created)
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
        Ok(Box::pin(async_stream::try_stream! {
            let _ = reply_span.enter();
            // Pre-turn snapshot: the clean work-tree state as this turn opens, so a
            // rewind to this turn can undo everything the agent does below.
            self.maybe_checkpoint(
                &session_config.id,
                &working_dir,
                checkpoint_anchor_ts,
                CheckpointKind::PreStep,
            ).await;
            let mut turns_taken = 0u32;
            let max_turns = session_config
                .max_turns
                .or_else(|| Config::global().get_param("BIOROUTER_MAX_TURNS").ok())
                .unwrap_or(DEFAULT_MAX_TURNS);
            // Cumulative tool calls dispatched this reply, across all iterations,
            // bounded by `max_tool_calls` so parallel fan-out can't run unbounded
            // even while `turns_taken` stays under `max_turns`.
            let mut tool_calls_taken = 0u32;
            let max_tool_calls = session_config
                .max_tool_calls
                .or_else(|| Config::global().get_param("BIOROUTER_MAX_TOOL_CALLS").ok())
                .unwrap_or(DEFAULT_MAX_TOOL_CALLS);
            let mut compaction_attempts = 0;
            // Consecutive auto-continues of a length-truncated turn; reset on any
            // tool call (real progress). Bounds the continue-on-truncation guard.
            let mut truncation_continuations = 0u32;
            // Resolve the tool-output guardrail policy once per reply (config
            // reads touch the filesystem, so we avoid doing it per tool result).
            let tool_output_guardrail =
                crate::guardrails::tool_output::ToolOutputGuardrailMode::from_config();

            loop {
                if is_token_cancelled(&cancel_token) {
                    break;
                }

                if let Some(final_output_tool) = self.final_output_tool.lock().await.as_ref() {
                    if final_output_tool.final_output.is_some() {
                        let final_event = AgentEvent::Message(
                            Message::assistant().with_text(final_output_tool.final_output.clone().unwrap())
                        );
                        yield final_event;
                        break;
                    }
                }

                turns_taken += 1;
                // Surface turn progress so an observer (CLI/GUI/logs) can tell how
                // much of the per-turn action budget has been used, and so a
                // budget-exhaustion stop is distinguishable from a normal completion.
                tracing::debug!("agent action {}/{} this turn", turns_taken, max_turns);
                if turns_taken > max_turns {
                    yield AgentEvent::Message(
                        Message::assistant().with_text(format!(
                            "I've reached my action limit for this turn ({max_turns} actions without user input), so I'm stopping here rather than because the task is necessarily complete. Would you like me to continue? (raise the cap with `max_turns` / `BIOROUTER_MAX_TURNS`.)"
                        ))
                    );
                    break;
                }
                if tool_calls_taken > max_tool_calls {
                    yield AgentEvent::Message(
                        Message::assistant().with_text(format!(
                            "I've made {tool_calls_taken} tool calls this turn, past my per-turn limit of {max_tool_calls}, so I'm stopping here rather than because the task is necessarily complete. Would you like me to continue? (raise the cap with `max_tool_calls` / `BIOROUTER_MAX_TOOL_CALLS`.)"
                        ))
                    );
                    break;
                }

                // Soft interrupt: inject any user messages queued mid-turn at this
                // safe boundary (after the previous turn's tools completed, before
                // the next provider call) so the model incorporates them without a
                // cancel-and-resend round trip that discards in-flight work.
                for text in self.drain_soft_interrupts() {
                    let m = Message::user().with_text(text);
                    session_manager.add_message(&session_config.id, &m).await?;
                    conversation.push(m.clone());
                    yield AgentEvent::Message(m);
                }

                let conversation_with_moim = self
                    .assemble_turn_context(&session_config.id, &conversation, &working_dir)
                    .await;

                let mut stream = Self::stream_response_from_provider(
                    self.provider().await?,
                    &system_prompt,
                    conversation_with_moim.messages(),
                    &tools,
                    &toolshim_tools,
                ).await?;

                let mut no_tools_called = true;
                let mut messages_to_add = Conversation::default();
                let mut tools_updated = false;
                let mut did_recovery_compact_this_iteration = false;
                // finish_reason of this turn's response (from the provider usage),
                // used below to auto-continue a length-truncated turn.
                let mut last_finish_reason: Option<String> = None;
                // The turn's usage, recorded ONCE when the stream ends.
                //
                // It used to be written on every usage-bearing chunk, which (a) lost
                // the whole turn when the user cancelled — the terminal chunk that
                // carries usage never arrives — and (b) would multiply the count
                // against any OpenAI-compatible host that emits `usage` on more than
                // one chunk. Last snapshot wins; a cancelled turn keeps whatever the
                // provider had reported so far.
                let mut turn_usage: Option<crate::providers::base::ProviderUsage> = None;

                while let Some(next) = stream.next().await {
                    if is_token_cancelled(&cancel_token) {
                        break;
                    }

                    match next {
                        Ok((response, usage)) => {
                            compaction_attempts = 0;

                            // Emit model change event if provider is lead-worker
                            let provider = self.provider().await?;
                            if let Some(lead_worker) = provider.as_lead_worker() {
                                if let Some(ref usage) = usage {
                                    let active_model = usage.model.clone();
                                    let (lead_model, worker_model) = lead_worker.get_model_info();
                                    let mode = if active_model == lead_model {
                                        "lead"
                                    } else if active_model == worker_model {
                                        "worker"
                                    } else {
                                        "unknown"
                                    };

                                    yield AgentEvent::ModelChange {
                                        model: active_model,
                                        mode: mode.to_string(),
                                    };
                                }
                            }

                            if let Some(ref usage) = usage {
                                if usage.finish_reason.is_some() {
                                    last_finish_reason = usage.finish_reason.clone();
                                }
                                turn_usage = Some(usage.clone());
                            }

                            if let Some(response) = response {
                                let ToolCategorizeResult {
                                    frontend_requests,
                                    remaining_requests,
                                    filtered_response,
                                } = self.categorize_tools(&response, &tools).await;

                                yield AgentEvent::Message(filtered_response.clone());
                                tokio::task::yield_now().await;

                                let num_tool_requests = frontend_requests.len() + remaining_requests.len();
                                if num_tool_requests == 0 {
                                    messages_to_add.push(response.clone());
                                    continue;
                                }
                                // Count every tool call this reply requests; the
                                // cumulative total is checked against `max_tool_calls`
                                // at the top of the next iteration.
                                tool_calls_taken = tool_calls_taken.saturating_add(num_tool_requests as u32);

                                let tool_response_messages: Vec<Arc<Mutex<Message>>> = (0..num_tool_requests)
                                    .map(|_| Arc::new(Mutex::new(Message::user().with_id(
                                        format!("msg_{}", Uuid::new_v4())
                                    ))))
                                    .collect();

                                let mut request_to_response_map = HashMap::new();
                                let mut request_metadata: HashMap<String, Option<ProviderMetadata>> = HashMap::new();
                                for (idx, request) in frontend_requests.iter().chain(remaining_requests.iter()).enumerate() {
                                    request_to_response_map.insert(request.id.clone(), tool_response_messages[idx].clone());
                                    request_metadata.insert(request.id.clone(), request.metadata.clone());
                                }

                                for (idx, request) in frontend_requests.iter().enumerate() {
                                    let mut frontend_tool_stream = self.handle_frontend_tool_request(
                                        request,
                                        tool_response_messages[idx].clone(),
                                    );

                                    while let Some(msg) = frontend_tool_stream.try_next().await? {
                                        yield AgentEvent::Message(msg);
                                    }
                                }
                                if biorouter_mode == BioRouterMode::Chat {
                                    // Skip all remaining tool calls in chat mode
                                    for request in remaining_requests.iter() {
                                        if let Some(response_msg) = request_to_response_map.get(&request.id) {
                                            let mut response = response_msg.lock().await;
                                            *response = response.clone().with_tool_response_with_metadata(
                                                request.id.clone(),
                                                Ok(CallToolResult {
                                                    content: vec![Content::text(CHAT_MODE_TOOL_SKIPPED_RESPONSE)],
                                                    structured_content: None,
                                                    is_error: Some(false),
                                                    meta: None,
                                                }),
                                                request.metadata.as_ref(),
                                            );
                                        }
                                    }
                                } else {
                                    let (
                                        inspection_results,
                                        permission_check_result,
                                        enable_extension_request_ids,
                                        mut tool_futures,
                                    ) = self.inspect_and_gate_tool_requests(
                                        &remaining_requests,
                                        &conversation,
                                        biorouter_mode,
                                        &session,
                                        &request_to_response_map,
                                        cancel_token.clone(),
                                    ).await?;

                                    let tool_futures_arc = Arc::new(Mutex::new(tool_futures));

                                    let mut tool_approval_stream = self.handle_approval_tool_requests(
                                        &permission_check_result.needs_approval,
                                        tool_futures_arc.clone(),
                                        &request_to_response_map,
                                        cancel_token.clone(),
                                        &session,
                                        &inspection_results,
                                    );

                                    while let Some(msg) = tool_approval_stream.try_next().await? {
                                        yield AgentEvent::Message(msg);
                                    }

                                    tool_futures = {
                                        let mut futures_lock = tool_futures_arc.lock().await;
                                        futures_lock.drain(..).collect::<Vec<_>>()
                                    };

                                    let with_id = tool_futures
                                        .into_iter()
                                        .map(|(request_id, stream)| {
                                            stream.map(move |item| (request_id.clone(), item))
                                        })
                                        .collect::<Vec<_>>();

                                    let mut combined = stream::select_all(with_id);
                                    let mut all_install_successful = true;
                                    // (request_id, tool_response, error) captured for PostToolUse hooks
                                    let mut post_tool_results: Vec<(String, Option<Value>, Option<String>)> = Vec::new();

                                    while let Some((request_id, item)) = combined.next().await {
                                        if is_token_cancelled(&cancel_token) {
                                            break;
                                        }

                                        for msg in self.drain_elicitation_messages(&session_config.id).await {
                                            yield AgentEvent::Message(msg);
                                        }

                                        match item {
                                            ToolStreamItem::Result(output) => {
                                                self.integrate_tool_result(
                                                    request_id,
                                                    output,
                                                    &enable_extension_request_ids,
                                                    &request_to_response_map,
                                                    &request_metadata,
                                                    &mut all_install_successful,
                                                    &mut post_tool_results,
                                                    tool_output_guardrail,
                                                ).await;
                                            }
                                            ToolStreamItem::Message(msg) => {
                                                yield AgentEvent::McpNotification((request_id, msg));
                                            }
                                        }
                                    }

                                    // PostToolUse / PostToolUseFailure hooks (observe-only):
                                    // awaited so injected context lands before the next
                                    // provider call, but decisions are ignored.
                                    {
                                        let hooks = self.hooks_manager();
                                        let mut post_futures = Vec::new();
                                        for (request_id, response_value, error_text) in post_tool_results {
                                            let Some(request) = remaining_requests.iter().find(|r| r.id == request_id) else { continue };
                                            let Ok(tool_call) = &request.tool_call else { continue };
                                            let tool_name = tool_call.name.to_string();
                                            let event = if error_text.is_some() {
                                                crate::hooks::HookEvent::PostToolUseFailure
                                            } else {
                                                crate::hooks::HookEvent::PostToolUse
                                            };
                                            if !hooks.has_hooks(event, Some(&tool_name), &session.working_dir).await {
                                                continue;
                                            }
                                            let mut payload = crate::hooks::HookPayload::new(
                                                event,
                                                &session_config.id,
                                                session.working_dir.to_string_lossy(),
                                            );
                                            payload.tool_name = Some(tool_name.clone());
                                            payload.tool_input = Some(
                                                tool_call
                                                    .arguments
                                                    .clone()
                                                    .map(Value::Object)
                                                    .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
                                            );
                                            payload.tool_response = response_value;
                                            payload.error = error_text;
                                            let hooks = Arc::clone(&hooks);
                                            let working_dir = session.working_dir.clone();
                                            post_futures.push(async move {
                                                hooks
                                                    .dispatch(event, Some(&tool_name), &payload, &working_dir)
                                                    .await
                                            });
                                        }
                                        if !post_futures.is_empty() {
                                            let mut hook_contexts: Vec<String> = Vec::new();
                                            for aggregate in futures::future::join_all(post_futures).await {
                                                for msg in &aggregate.system_messages {
                                                    yield AgentEvent::Message(
                                                        Message::assistant()
                                                            .with_system_notification(
                                                                SystemNotificationType::InlineMessage,
                                                                msg.clone(),
                                                            )
                                                            .user_only(),
                                                    );
                                                }
                                                if let Some(ctx) = aggregate.joined_context() {
                                                    hook_contexts.push(ctx);
                                                }
                                            }
                                            if !hook_contexts.is_empty() {
                                                let context_message = Message::user()
                                                    .with_text(crate::hooks::outcome::frame_hook_context(
                                                        &hook_contexts.join("\n\n"),
                                                    ))
                                                    .with_visibility(false, true);
                                                messages_to_add.push(context_message);
                                            }
                                        }
                                    }

                                    // check for remaining elicitation messages after all tools complete
                                    for msg in self.drain_elicitation_messages(&session_config.id).await {
                                        yield AgentEvent::Message(msg);
                                    }

                                    if all_install_successful && !enable_extension_request_ids.is_empty() {
                                        if let Err(e) = self.save_extension_state(&session_config).await {
                                            warn!("Failed to save extension state after runtime changes: {}", e);
                                        }
                                        tools_updated = true;
                                    }
                                }

                                // Preserve thinking content from the original response
                                // Gemini (and other thinking models) require thinking to be echoed back
                                let thinking_content: Vec<MessageContent> = response.content.iter()
                                    .filter(|c| matches!(c, MessageContent::Thinking(_)))
                                    .cloned()
                                    .collect();
                                if !thinking_content.is_empty() {
                                    let thinking_msg = Message::new(
                                        response.role.clone(),
                                        response.created,
                                        thinking_content,
                                    ).with_id(format!("msg_{}", Uuid::new_v4()));
                                    messages_to_add.push(thinking_msg);
                                }

                                for (idx, request) in frontend_requests.iter().chain(remaining_requests.iter()).enumerate() {
                                    if request.tool_call.is_ok() {
                                        let request_msg = Message::assistant()
                                            .with_id(format!("msg_{}", Uuid::new_v4()))
                                            .with_tool_request_with_metadata(
                                                request.id.clone(),
                                                request.tool_call.clone(),
                                                request.metadata.as_ref(),
                                                request.tool_meta.clone(),
                                            );
                                        messages_to_add.push(request_msg);
                                        let final_response = tool_response_messages[idx]
                                                                .lock().await.clone();
                                        yield AgentEvent::Message(final_response.clone());
                                        messages_to_add.push(final_response);
                                    }
                                }

                                no_tools_called = false;
                            }
                        }
                        Err(ProviderError::ContextLengthExceeded(_)) => {
                            compaction_attempts += 1;

                            // BR-13: progressive context-overflow fallback. Instead of a
                            // hard 2-attempt cliff, each successive overflow escalates to a
                            // more aggressive compaction strategy (keep-window ->
                            // shrink-window -> summarize-all -> drop-oldest); only once the
                            // ladder is exhausted do we surface the "still exceeded" error.
                            // `compaction_attempts` resets to 0 on the next successful
                            // provider response, so a later overflow restarts the ladder.
                            let Some(recovery) = overflow_recovery_for_attempt(compaction_attempts) else {
                                error!("Context limit exceeded after progressive compaction fallbacks - prompt too large");
                                yield AgentEvent::Message(
                                    Message::assistant().with_system_notification(
                                        SystemNotificationType::InlineMessage,
                                        "Unable to continue: Context limit still exceeded after compaction. Try using a shorter message, a model with a larger context window, or start a new session."
                                    )
                                );
                                break;
                            };

                            yield AgentEvent::Message(
                                Message::assistant().with_system_notification(
                                    SystemNotificationType::InlineMessage,
                                    "Context limit reached. Compacting to continue conversation...",
                                )
                            );
                            yield AgentEvent::Message(
                                Message::assistant().with_system_notification(
                                    SystemNotificationType::ThinkingMessage,
                                    COMPACTION_THINKING_TEXT,
                                )
                            );

                            self.fire_compaction_hook(
                                crate::hooks::HookEvent::PreCompact,
                                &session_config.id,
                                &session.working_dir,
                                "auto",
                                Some("context_overflow"),
                            );
                            match compact_messages_with_recovery(self.provider().await?.as_ref(), &conversation, recovery).await {
                                Ok((compacted_conversation, usage)) => {
                                    session_manager.replace_conversation(&session_config.id, &compacted_conversation).await?;
                                    self.update_session_metrics(&session_config, &usage, true).await?;
                                    conversation = compacted_conversation;
                                    did_recovery_compact_this_iteration = true;
                                    self.fire_compaction_hook(
                                        crate::hooks::HookEvent::PostCompact,
                                        &session_config.id,
                                        &session.working_dir,
                                        "auto",
                                        Some("context_overflow"),
                                    );
                                    yield AgentEvent::HistoryReplaced(conversation.clone());
                                    break;
                                }
                                Err(e) => {
                                    error!("Compaction failed: {}", e);
                                    break;
                                }
                            }
                        }
                        Err(ref provider_err) => {
                            error!("Error: {}", provider_err);
                            yield AgentEvent::Message(
                                Message::assistant().with_text(
                                    format!("Ran into this error: {provider_err}.\n\nPlease retry if you think this is a transient or recoverable error.")
                                )
                            );
                            break;
                        }
                    }
                }

                // Record the turn exactly once, whether the stream finished, was
                // cancelled, or errored out. The provider still processed (and
                // billed) whatever it reported.
                self.record_turn_usage(&session_config, turn_usage.take()).await?;

                if tools_updated {
                    (tools, toolshim_tools, system_prompt) =
                        self.prepare_tools_and_prompt(&session_config.id, &session.working_dir).await?;
                }
                let mut exit_chat = false;
                if no_tools_called {
                    // Observability: a turn that ends without a tool call is either a
                    // natural completion ("stop"), a length-truncation ("length"), or
                    // an unreported end (None). Logged so "done" vs "cut off" is
                    // distinguishable in the logs (and to scope continue-on-truncation).
                    info!(
                        "turn ended with no tool call; finish_reason={:?}",
                        last_finish_reason
                    );
                    if last_finish_reason.as_deref() == Some("length")
                        && truncation_continuations < MAX_TRUNCATION_CONTINUATIONS
                    {
                        // The provider cut the response off at the output-length
                        // limit (not a natural stop) and the model called no tool,
                        // so the turn is genuinely unfinished. Auto-continue it
                        // instead of ending on a half-written response. Bounded by
                        // the streak cap (reset on any tool call) and by max_turns.
                        // (Distinct from "the model chose to stop mid-task" — that
                        // is left to the Stop-hook / goal system, not a hard-coded
                        // loop injection; see the note near the top of this file.)
                        truncation_continuations += 1;
                        warn!(
                            "Response truncated by output-length limit (finish_reason=\"length\"); auto-continuing ({}/{})",
                            truncation_continuations, MAX_TRUNCATION_CONTINUATIONS
                        );
                        let message = Message::user().with_text(TRUNCATION_CONTINUATION_MESSAGE);
                        messages_to_add.push(message.clone());
                        yield AgentEvent::Message(message);
                    } else if let Some(final_output_tool) = self.final_output_tool.lock().await.as_ref() {
                        if final_output_tool.final_output.is_none() {
                            warn!("Final output tool has not been called yet. Continuing agent loop.");
                            let message = Message::user().with_text(FINAL_OUTPUT_CONTINUATION_MESSAGE);
                            messages_to_add.push(message.clone());
                            yield AgentEvent::Message(message);
                        } else {
                            let message = Message::assistant().with_text(final_output_tool.final_output.clone().unwrap());
                            messages_to_add.push(message.clone());
                            yield AgentEvent::Message(message);
                            exit_chat = true;
                        }
                    } else if did_recovery_compact_this_iteration {
                        // Avoid setting exit_chat; continue from last user message in the conversation
                    } else {
                        match self.handle_retry_logic(&mut conversation, &session_config, &initial_messages).await {
                            Ok(should_retry) => {
                                if should_retry {
                                    info!("Retry logic triggered, restarting agent loop");
                                } else {
                                    exit_chat = true;
                                }
                            }
                            Err(e) => {
                                error!("Retry logic failed: {}", e);
                                yield AgentEvent::Message(
                                    Message::assistant().with_text(
                                        format!("Retry logic encountered an error: {}", e)
                                    )
                                );
                                exit_chat = true;
                            }
                        }
                    }
                }

                for msg in &messages_to_add {
                    session_manager.add_message(&session_config.id, msg).await?;
                }
                conversation.extend(messages_to_add);

                // BR-28: turn boundary — join the observe-only hooks fired during
                // this iteration (Notification on a permission prompt, Pre/PostCompact
                // on an in-loop compaction) and surface what they returned instead of
                // dropping their aggregate. Placed before the `exit_chat` branch so
                // every exit path passes through it.
                for msg in self.settle_fired_hooks(&session_config.id).await {
                    yield AgentEvent::Message(msg);
                }

                if !no_tools_called {
                    // Tools ran this iteration: any Stop-hook block streak is over,
                    // and the turn made real progress, so reset the auto-continue streaks.
                    self.hooks_manager.reset_stop_blocks(&session_config.id).await;
                    truncation_continuations = 0;

                    // BR-43: post-step snapshot of the (possibly mutated) work-tree.
                    // Coarse: any tool-running iteration, relying on the shadow
                    // repo's tree-sha dedup to drop read-only steps (no row when the
                    // tree is unchanged).
                    self.maybe_checkpoint(
                        &session_config.id,
                        &working_dir,
                        checkpoint_anchor_ts,
                        CheckpointKind::PostStep,
                    ).await;
                }

                if exit_chat {
                    if session.session_type == SessionType::SubAgent {
                        // Subagents get an observe-only SubagentStop instead of a
                        // blockable Stop (avoids nested runaway loops).
                        let mut payload = crate::hooks::HookPayload::new(
                            crate::hooks::HookEvent::SubagentStop,
                            &session_config.id,
                            session.working_dir.to_string_lossy(),
                        );
                        payload.subagent_id = Some(session_config.id.clone());
                        self.hooks_manager.fire(
                            crate::hooks::HookEvent::SubagentStop,
                            None,
                            payload,
                            session.working_dir.clone(),
                        );
                        // BR-28: this break is the subagent's last boundary, so settle
                        // the SubagentStop hook here — nothing downstream would ever
                        // join it, and its aggregate would be lost with the task.
                        for msg in self.settle_fired_hooks(&session_config.id).await {
                            yield AgentEvent::Message(msg);
                        }
                        break;
                    }
                    let active_goal = self.active_goal(&session_config.id).await;
                    let transcript_tail = crate::agents::goal::transcript_tail(&conversation);
                    match self.hooks_manager.stop(&session_config.id, &session.working_dir, transcript_tail).await {
                        crate::hooks::StopHookVerdict::Proceed => {
                            // An active goal whose evaluator let the stop
                            // proceed is met: clear it and tell the user.
                            if let Some(goal) = active_goal {
                                self.clear_goal(&session_config.id).await;
                                yield AgentEvent::Message(
                                    Message::assistant()
                                        .with_system_notification(
                                            SystemNotificationType::InlineMessage,
                                            format!(
                                                "🎯 Goal met — cleared: {}",
                                                crate::agents::goal::ellipsize(&goal.condition, 200)
                                            ),
                                        )
                                        .user_only(),
                                );
                            }
                            break;
                        }
                        crate::hooks::StopHookVerdict::CapReached => {
                            let goal_hint = if active_goal.is_some() {
                                " The /goal stays active and will be re-evaluated next turn; run /goal clear to stop it."
                            } else {
                                ""
                            };
                            yield AgentEvent::Message(
                                Message::assistant()
                                    .with_system_notification(
                                        SystemNotificationType::InlineMessage,
                                        format!(
                                            "Stop hook block limit ({}) reached; finishing anyway.{}",
                                            crate::hooks::STOP_HOOK_BLOCK_CAP,
                                            goal_hint
                                        ),
                                    )
                                    .user_only(),
                            );
                            break;
                        }
                        crate::hooks::StopHookVerdict::Blocked { reason } => {
                            // For goal loops, account the block against the goal's
                            // own iteration/stall budget (which, unlike the generic
                            // Stop-hook cap, does not reset when tools run). On
                            // give-up, clear the goal and have the agent deliver a
                            // best-effort answer instead of looping forever.
                            let goal_outcome = if active_goal.is_some() {
                                self.record_goal_block(&session_config.id, &reason).await
                            } else {
                                None
                            };

                            let (feedback_text, notice) = match goal_outcome {
                                Some(crate::agents::goal::GoalOutcome::GiveUp { attempts, stalled }) => {
                                    self.clear_goal(&session_config.id).await;
                                    let why = if stalled {
                                        "it stopped making progress"
                                    } else {
                                        "it hit the attempt limit"
                                    };
                                    (
                                        crate::agents::goal::giveup_instruction(&reason),
                                        format!(
                                            "🎯 Goal stopped after {attempts} attempt(s) — {why}. \
                                             Wrapping up with a best-effort answer; refine with a \
                                             narrower /goal if needed."
                                        ),
                                    )
                                }
                                _ => (
                                    format!("Stop hook feedback: {reason}"),
                                    format!("Stop hook blocked completion: {reason}"),
                                ),
                            };

                            let feedback = Message::user()
                                .with_text(feedback_text)
                                .with_visibility(false, true);
                            session_manager.add_message(&session_config.id, &feedback).await?;
                            conversation.push(feedback);
                            yield AgentEvent::Message(
                                Message::assistant()
                                    .with_system_notification(
                                        SystemNotificationType::InlineMessage,
                                        notice,
                                    )
                                    .user_only(),
                            );
                            // Keep looping: the model sees the feedback next turn.
                            // After a give-up the goal is cleared, so the next stop
                            // proceeds once the agent delivers its wrap-up.
                        }
                    }
                }

                tokio::task::yield_now().await;
            }

            // BR-12: the turn is complete — the agent loop drained and control is
            // returning to the user. If the session ended over the compaction
            // threshold, kick off compaction in the background now, *between*
            // turns, so the next turn starts from an already-compacted history
            // instead of stalling on the summarization round-trip. Fire-and-forget
            // (spawns a task, doesn't await it); the synchronous check at the top
            // of reply() is the fallback if this hasn't landed by then. Like the
            // rename below, this tail runs only when the consumer drains the stream
            // to completion — an early cancel just defers compaction to that
            // synchronous fallback, which is harmless.
            self.maybe_spawn_eager_compaction(&session_config, &working_dir);

            // NOTE: LLM-driven session rename is intentionally NOT triggered here.
            // This code sits after the last `yield` of a lazy `async_stream`, so it
            // only runs if the consumer drains the stream all the way to `None`.
            // The SSE consumer can `break` early (e.g. on client disconnect /
            // cancellation) before that final poll, in which case the stream future
            // is dropped and this tail never executes — leaving the session stuck on
            // "New Session". The rename is now driven by the consumer instead, via
            // `maybe_rename_session`, which is guaranteed to run after the reply loop
            // ends regardless of how it ended. See routes/reply.rs and routes/apps.rs.
        }))
    }

    /// Best-effort LLM session rename, safe to call after a reply loop ends.
    ///
    /// Consumers of `reply()` call this once the stream loop exits (normal end,
    /// error, or cancellation). Unlike a tail appended to the lazy reply stream,
    /// this always runs, so a session with a real exchange is never left as the
    /// "New Session" placeholder. `maybe_update_name` is itself idempotent and
    /// guarded (it skips user-named sessions and stops after the first few
    /// exchanges), so calling it once per reply is cheap and correct.
    pub async fn maybe_rename_session(&self, session_id: &str) {
        let provider = match self.provider().await {
            Ok(provider) => provider,
            Err(e) => {
                warn!("Skipping session rename, no provider available: {}", e);
                return;
            }
        };
        if let Err(e) = self
            .config
            .session_manager
            .maybe_update_name(session_id, provider)
            .await
        {
            warn!("Failed to generate session description: {}", e);
        }
    }

    pub async fn extend_system_prompt(&self, instruction: String) {
        let mut prompt_manager = self.prompt_manager.lock().await;
        prompt_manager.add_system_prompt_extra(instruction);
    }

    pub async fn update_provider(
        &self,
        provider: Arc<dyn Provider>,
        session_id: &str,
    ) -> Result<()> {
        let provider_name = provider.get_name().to_string();
        let model_config = provider.get_model_config();

        let mut current_provider = self.provider.lock().await;
        *current_provider = Some(provider);

        self.config
            .session_manager
            .clone()
            .update(session_id)
            .provider_name(&provider_name)
            .model_config(model_config)
            .apply()
            .await
            .context("Failed to persist provider config to session")
    }

    /// Restore the provider from session data or fall back to global config
    /// This is used when resuming a session to restore the provider state
    pub async fn restore_provider_from_session(&self, session: &Session) -> Result<()> {
        let config = Config::global();

        let provider_name = session
            .provider_name
            .clone()
            .or_else(|| config.get_biorouter_provider().ok())
            .ok_or_else(|| anyhow!("Could not configure agent: missing provider"))?;

        let model_config = match session.model_config.clone() {
            Some(saved_config) => saved_config,
            None => {
                let model_name = config
                    .get_biorouter_model()
                    .map_err(|_| anyhow!("Could not configure agent: missing model"))?;
                crate::model::ModelConfig::new(&model_name)
                    .map_err(|e| anyhow!("Could not configure agent: invalid model {}", e))?
            }
        };

        let provider = crate::providers::create(&provider_name, model_config)
            .await
            .map_err(|e| anyhow!("Could not create provider: {}", e))?;

        self.update_provider(provider, &session.id).await
    }

    /// Override the system prompt with a custom template
    pub async fn override_system_prompt(&self, template: String) {
        let mut prompt_manager = self.prompt_manager.lock().await;
        prompt_manager.set_system_prompt_override(template);
    }

    pub async fn list_extension_prompts(&self) -> HashMap<String, Vec<Prompt>> {
        self.extension_manager
            .list_prompts(CancellationToken::default())
            .await
            .expect("Failed to list prompts")
    }

    pub async fn get_prompt(&self, name: &str, arguments: Value) -> Result<GetPromptResult> {
        // First find which extension has this prompt
        let prompts = self
            .extension_manager
            .list_prompts(CancellationToken::default())
            .await
            .map_err(|e| anyhow!("Failed to list prompts: {}", e))?;

        if let Some(extension) = prompts
            .iter()
            .find(|(_, prompt_list)| prompt_list.iter().any(|p| p.name == name))
            .map(|(extension, _)| extension)
        {
            return self
                .extension_manager
                .get_prompt(extension, name, arguments, CancellationToken::default())
                .await
                .map_err(|e| anyhow!("Failed to get prompt: {}", e));
        }

        Err(anyhow!("Prompt '{}' not found", name))
    }

    pub async fn get_plan_prompt(&self) -> Result<String> {
        let tools = self.extension_manager.get_prefixed_tools(None).await?;
        let tools_info = tools
            .into_iter()
            .map(|tool| {
                ToolInfo::new(
                    &tool.name,
                    tool.description
                        .as_ref()
                        .map(|d| d.as_ref())
                        .unwrap_or_default(),
                    get_parameter_names(&tool),
                    None,
                )
            })
            .collect();

        let plan_prompt = self.extension_manager.get_planning_prompt(tools_info).await;

        Ok(plan_prompt)
    }

    pub async fn handle_tool_result(&self, id: String, result: ToolResult<CallToolResult>) {
        if let Err(e) = self.tool_result_tx.send((id, result)).await {
            error!("Failed to send tool result: {}", e);
        }
    }

    #[allow(clippy::too_many_lines)]
    pub async fn create_workflow(&self, mut messages: Conversation) -> Result<Workflow> {
        tracing::info!(
            "Starting workflow creation with {} messages",
            messages.len()
        );

        let extensions_info = self.extension_manager.get_extensions_info().await;
        tracing::debug!("Retrieved {} extensions info", extensions_info.len());

        // Get model name from provider
        let provider = self.provider().await.map_err(|e| {
            tracing::error!("Failed to get provider for workflow creation: {}", e);
            e
        })?;
        let model_config = provider.get_model_config();
        let model_name = &model_config.model_name;
        tracing::debug!("Using model: {}", model_name);

        let prompt_manager = self.prompt_manager.lock().await;
        let system_prompt = prompt_manager
            .builder()
            .with_extensions(extensions_info.into_iter())
            .with_frontend_instructions(self.frontend_instructions.lock().await.clone())
            .build();

        let workflow_prompt = prompt_manager.get_workflow_prompt().await;
        let tools = self
            .extension_manager
            .get_prefixed_tools(None)
            .await
            .map_err(|e| {
                tracing::error!("Failed to get tools for workflow creation: {}", e);
                e
            })?;

        messages.push(Message::user().with_text(workflow_prompt));

        let (messages, issues) = fix_conversation(messages);
        if !issues.is_empty() {
            issues
                .iter()
                .for_each(|issue| tracing::warn!(workflow.conversation.issue = issue));
        }

        tracing::debug!(
            "Added workflow prompt to messages, total messages: {}",
            messages.len()
        );

        tracing::info!("Calling provider to generate workflow content");
        let (result, _usage) = self
            .provider
            .lock()
            .await
            .as_ref()
            .ok_or_else(|| {
                let error = anyhow!("Provider not available during workflow creation");
                tracing::error!("{}", error);
                error
            })?
            .complete(&system_prompt, messages.messages(), &tools)
            .await
            .map_err(|e| {
                tracing::error!("Provider completion failed during workflow creation: {}", e);
                e
            })?;

        let content = result.as_concat_text();
        tracing::debug!(
            "Provider returned content with {} characters",
            content.len()
        );

        // the response may be contained in ```json ```, strip that before parsing json
        let re = Regex::new(r"(?s)```[^\n]*\n(.*?)\n```").unwrap();
        let clean_content = re
            .captures(&content)
            .and_then(|caps| caps.get(1).map(|m| m.as_str()))
            .unwrap_or(&content)
            .trim()
            .to_string();

        let json_content = serde_json::from_str::<Value>(&clean_content).ok();

        let (instructions, activities) = if let Some(json_content) = json_content.as_ref() {
            let instructions = json_content
                .get("instructions")
                .ok_or_else(|| anyhow!("Missing 'instructions' in json response"))?
                .as_str()
                .ok_or_else(|| anyhow!("instructions' is not a string"))?
                .to_string();

            let activities = json_content
                .get("activities")
                .ok_or_else(|| anyhow!("Missing 'activities' in json response"))?
                .as_array()
                .ok_or_else(|| anyhow!("'activities' is not an array'"))?
                .iter()
                .map(|act| {
                    act.as_str()
                        .map(|s| s.to_string())
                        .ok_or(anyhow!("'activities' array element is not a string"))
                })
                .collect::<Result<_, _>>()?;

            (instructions, activities)
        } else {
            tracing::warn!("Failed to parse JSON, falling back to string parsing");
            // If we can't get valid JSON, try string parsing
            // Use split_once to get the content after "Instructions:".
            let after_instructions = content
                .split_once("instructions:")
                .map(|(_, rest)| rest)
                .unwrap_or(&content);

            // Split once more to separate instructions from activities.
            let (instructions_part, activities_text) = after_instructions
                .split_once("activities:")
                .unwrap_or((after_instructions, ""));

            let instructions = instructions_part
                .trim_end_matches(|c: char| c.is_whitespace() || c == '#')
                .trim()
                .to_string();
            let activities_text = activities_text.trim();

            // Regex to remove bullet markers or numbers with an optional dot.
            let bullet_re = Regex::new(r"^[•\-*\d]+\.?\s*").expect("Invalid regex");

            // Process each line in the activities section.
            let activities: Vec<String> = activities_text
                .lines()
                .map(|line| bullet_re.replace(line, "").to_string())
                .map(|s| s.trim().to_string())
                .filter(|line| !line.is_empty())
                .collect();

            (instructions, activities)
        };

        let extension_configs = self.get_extension_configs().await;

        let author = Author {
            contact: std::env::var("USER")
                .or_else(|_| std::env::var("USERNAME"))
                .ok(),
            metadata: None,
        };

        // Ideally we'd get the name of the provider we are using from the provider itself,
        // but it doesn't know and the plumbing looks complicated.
        let config = Config::global();
        let provider_name: String = config
            .get_biorouter_provider()
            .expect("No provider configured. Run 'biorouter configure' first");

        let settings = Settings {
            biorouter_provider: Some(provider_name.clone()),
            biorouter_model: Some(model_name.clone()),
            temperature: Some(model_config.temperature.unwrap_or(0.0)),
        };

        tracing::debug!(
            "Building workflow with {} activities and {} extensions",
            activities.len(),
            extension_configs.len()
        );

        let (title, description) = if let Some(json_content) = json_content.as_ref() {
            let title = json_content
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("Custom workflow from chat")
                .to_string();

            let description = json_content
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("a custom workflow instance from this chat session")
                .to_string();

            (title, description)
        } else {
            (
                "Custom workflow from chat".to_string(),
                "a custom workflow instance from this chat session".to_string(),
            )
        };

        let skills = json_content
            .as_ref()
            .and_then(|json| json.get("skills"))
            .and_then(|skills| skills.as_array())
            .map(|skills| {
                skills
                    .iter()
                    .filter_map(|skill| skill.as_str())
                    .map(str::trim)
                    .filter(|skill| !skill.is_empty())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let mut workflow_builder = Workflow::builder()
            .title(title)
            .description(description)
            .instructions(instructions)
            .activities(activities)
            .extensions(extension_configs)
            .settings(settings)
            .author(author);

        if !skills.is_empty() {
            workflow_builder = workflow_builder.skills(skills);
        }

        let workflow = workflow_builder.build().map_err(|e| {
            tracing::error!("Failed to build workflow: {}", e);
            anyhow!("Workflow build failed: {}", e)
        })?;

        tracing::info!("Workflow creation completed successfully");
        Ok(workflow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::Response;

    #[tokio::test]
    async fn test_add_final_output_tool() -> Result<()> {
        let agent = Agent::new();

        let response = Response {
            json_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "result": {"type": "string"}
                }
            })),
        };

        agent.add_final_output_tool(response).await;

        let tools = agent.list_tools("test-session-id", None).await;
        let final_output_tool = tools
            .iter()
            .find(|tool| tool.name == FINAL_OUTPUT_TOOL_NAME);

        assert!(
            final_output_tool.is_some(),
            "Final output tool should be present after adding"
        );

        let prompt_manager = agent.prompt_manager.lock().await;
        let system_prompt = prompt_manager.builder().build();

        let final_output_tool_ref = agent.final_output_tool.lock().await;
        let final_output_tool_system_prompt =
            final_output_tool_ref.as_ref().unwrap().system_prompt();
        assert!(system_prompt.contains(&final_output_tool_system_prompt));
        Ok(())
    }

    #[tokio::test]
    async fn apply_vault_resolves_secrets_in_tool_args() {
        use crate::agents::vault_refs::VaultRefs;
        use std::collections::HashMap;

        let agent = Agent::new();

        // No vault installed → arguments are untouched.
        let mut call = CallToolRequestParams {
            name: "files_read".into(),
            arguments: Some(
                serde_json::json!({ "header": "Bearer {{vault:API_KEY}}" })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            meta: None,
            task: None,
        };
        agent.apply_vault(&mut call).await;
        assert_eq!(
            call.arguments.as_ref().unwrap()["header"],
            serde_json::json!("Bearer {{vault:API_KEY}}"),
            "without a vault, placeholders are left intact"
        );

        // Install a vault → the placeholder resolves to the secret at dispatch.
        let mut secrets = HashMap::new();
        secrets.insert("API_KEY".to_string(), "sk-live-xyz".to_string());
        agent.set_vault(Arc::new(VaultRefs::new(secrets))).await;

        agent.apply_vault(&mut call).await;
        assert_eq!(
            call.arguments.as_ref().unwrap()["header"],
            serde_json::json!("Bearer sk-live-xyz"),
            "the installed vault resolves the secret into the args"
        );
    }

    #[tokio::test]
    async fn injected_skills_cache_is_per_session() {
        // BR-8: marking a skill injected is scoped to its session id, so a
        // different session (or a different skill) still gets the full body.
        let agent = Agent::new();

        assert!(!agent.skill_already_injected("s1", "demo").await);
        agent.mark_skill_injected("s1", "demo").await;

        assert!(agent.skill_already_injected("s1", "demo").await);
        // Same skill, different session → not yet injected.
        assert!(!agent.skill_already_injected("s2", "demo").await);
        // Different skill, same session → not yet injected.
        assert!(!agent.skill_already_injected("s1", "other").await);
    }

    #[tokio::test]
    async fn skill_resource_context_reinjects_on_load_failure() {
        // BR-8: a failed load must NOT be cached as "already injected" — the
        // next turn has to try again, and never silently emits the pointer in
        // place of a body that was never delivered.
        let agent = Agent::new();
        let refs = ResourceRefs {
            skills: vec!["nonexistent-skill".to_string()],
            ..Default::default()
        };

        let first = agent.skill_resource_context("sess", &refs).await;
        assert!(first.contains("Could not load this selected skill"));
        assert!(!first.contains("already loaded earlier in this session"));
        assert!(
            !agent
                .skill_already_injected("sess", "nonexistent-skill")
                .await
        );

        let second = agent.skill_resource_context("sess", &refs).await;
        assert!(second.contains("Could not load this selected skill"));
        assert!(!second.contains("already loaded earlier in this session"));
    }

    #[tokio::test]
    async fn skill_resource_context_pointer_after_injection() {
        // BR-8: once a skill is marked injected, later turns get the short
        // pointer instead of re-inlining the (potentially multi-KB) body.
        let agent = Agent::new();
        agent.mark_skill_injected("sess", "demo").await;

        let refs = ResourceRefs {
            skills: vec!["demo".to_string()],
            ..Default::default()
        };
        let out = agent.skill_resource_context("sess", &refs).await;
        assert!(out.contains("already loaded earlier in this session"));
        assert!(out.contains("skills__loadSkill"));
        assert!(!out.contains("Could not load this selected skill"));
    }

    #[tokio::test]
    async fn test_tool_inspection_manager_has_all_inspectors() -> Result<()> {
        let agent = Agent::new();

        // Verify that the tool inspection manager has all expected inspectors
        let inspector_names = agent.tool_inspection_manager.inspector_names();

        assert!(
            inspector_names.contains(&"repetition"),
            "Tool inspection manager should contain repetition inspector"
        );
        assert!(
            inspector_names.contains(&"permission"),
            "Tool inspection manager should contain permission inspector"
        );
        assert!(
            inspector_names.contains(&"security"),
            "Tool inspection manager should contain security inspector"
        );
        assert!(
            inspector_names.contains(&"managed"),
            "Tool inspection manager should contain managed policy inspector"
        );

        Ok(())
    }
}
