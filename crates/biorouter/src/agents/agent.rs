use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use futures::stream::BoxStream;
use futures::{stream, Stream, StreamExt, TryStreamExt};
use uuid::Uuid;

use super::final_output_tool::FinalOutputTool;
use super::platform_tools;
use super::tool_execution::{ToolCallResult, CHAT_MODE_TOOL_SKIPPED_RESPONSE, DECLINED_RESPONSE};
use super::turn_abort::TurnAbortCode;
use crate::action_required_manager::ActionRequiredManager;
use crate::agents::budget::{BudgetAction, BudgetTracker, ReplyBudget};
use crate::agents::effort::ReasoningEffort;
use crate::agents::extension::{ExtensionConfig, ExtensionResult, ToolInfo};
use crate::agents::extension_manager::{get_parameter_names, normalize, ExtensionManager};
use crate::agents::extension_manager_extension::MANAGE_EXTENSIONS_TOOL_NAME_COMPLETE;
use crate::agents::final_output_tool::{FINAL_OUTPUT_CONTINUATION_MESSAGE, FINAL_OUTPUT_TOOL_NAME};
use crate::agents::platform_tools::{
    PLATFORM_INGEST_CONVERSATION_TOOL_NAME, PLATFORM_MANAGE_SCHEDULE_TOOL_NAME,
    PLATFORM_READ_SESSION_BLOB_TOOL_NAME,
};
use crate::agents::prompt_manager::PromptManager;
use crate::agents::resource_refs::{
    canonical_builtin_extension_name, extract_resource_refs, ResourceRefs,
};
use crate::agents::retry::{RetryManager, RetryResult};
use crate::agents::stall::{StallAction, StallCheckConfig, StallWatch};
use crate::agents::subagent_handle;
use crate::agents::subagent_task_config::TaskConfig;
use crate::agents::subagent_tool::{
    create_subagent_status_tool, create_subagent_tool, handle_subagent_status_tool,
    handle_subagent_tool, SUBAGENT_STATUS_TOOL_NAME, SUBAGENT_TOOL_NAME,
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
    TokenState, ToolRequest,
};
use crate::conversation::tool_result_serde::call_tool_result;
use crate::conversation::{debug_conversation_fix, fix_conversation, Conversation};
use crate::managed::ManagedPolicy;
use crate::mcp_utils::ToolResult;
use crate::observability::loop_safety::{self, LoopSafetyEvent, LoopSafetyKind};
use crate::permission::managed_inspector::ManagedPolicyInspector;
use crate::permission::permission_inspector::PermissionInspector;
use crate::permission::permission_judge::PermissionCheckResult;
use crate::permission::tool_risk::ToolRiskRegistry;
use crate::permission::PermissionConfirmation;
use crate::providers::base::Provider;
use crate::providers::errors::ProviderError;
use crate::scheduler_trait::SchedulerTrait;
use crate::security::security_inspector::SecurityInspector;
use crate::session::extension_data::{EnabledExtensionsState, ExtensionState};
use crate::session::message_blobs;
use crate::session::{Session, SessionManager, SessionType};
use crate::tool_inspection::{InspectionAction, InspectionResult, ToolInspectionManager};
use crate::tool_monitor::{FailureLoopConfig, RepetitionInspector, SemanticLoopConfig};
use crate::utils::is_token_cancelled;
use crate::workflow::{Author, Response, Settings, SubWorkflow, Workflow};
use regex::Regex;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, ErrorCode, ErrorData, GetPromptResult, Prompt,
    ServerNotification, Tool,
};
use rmcp::object;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot, Mutex};
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
/// BR-29 staged repetition guard, soft stage: the Nth consecutive byte-identical
/// tool call earns a non-blocking warning injected into the model's context (the
/// call still runs). Overridable with `BIOROUTER_REPETITION_SOFT_WARN`.
const DEFAULT_REPETITION_SOFT_WARN: u32 = 3;
/// BR-29 staged repetition guard, hard stage: the Nth consecutive byte-identical
/// tool call is denied outright, with an honest "repetition guard" reason (never
/// the misleading "the user declined"). Overridable with
/// `BIOROUTER_REPETITION_HARD_STOP`. Set below the soft threshold to disable the
/// soft stage entirely.
const DEFAULT_REPETITION_HARD_STOP: u32 = 5;
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
    /// The transcript as it stood before the turn started — the snapshot the retry
    /// path restores from. A `Conversation` (not a `Vec<Message>`) so taking it is
    /// a refcount bump rather than a deep copy of the history (BR-56).
    pub initial_messages: Conversation,
}

/// BR-56: normalize the transcript before every provider call, not just once per
/// reply. Kill switch: `BIOROUTER_NORMALIZE_EACH_TURN=false`.
fn normalize_each_turn() -> bool {
    Config::global()
        .get_param::<bool>("BIOROUTER_NORMALIZE_EACH_TURN")
        .unwrap_or(true)
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

/// What happened to a tool-permission decision handed to [`Agent::handle_confirmation`]
/// (BR-62).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmationOutcome {
    /// A prompt with that request id was waiting; the decision reached the loop.
    Delivered,
    /// Nothing was waiting on that request id — a duplicate click, a decision for
    /// a prompt that already expired or was cancelled, or a stale client. The
    /// decision was dropped rather than applied to some other pending call.
    Unknown,
}

/// The main biorouter Agent
pub struct Agent {
    pub(super) provider: SharedProvider,
    pub config: AgentConfig,

    pub extension_manager: Arc<ExtensionManager>,
    pub(super) sub_workflows: Mutex<HashMap<String, SubWorkflow>>,
    /// Whether the generic `subagent` tool is offered at all.
    ///
    /// Default `true` (every existing caller). An Agent-Drafter app that declares
    /// worker profiles sets this `false`, because otherwise TWO delegation
    /// mechanisms are armed at once and the generic one is easier to reach: the
    /// `subagent` tool's description auto-lists the very worker names the author
    /// registered, and it takes a free-form `instructions` string. The model
    /// picked it every time — spec-006 declared the same four workers *twice*
    /// (`sub_agents` AND `agents`), and the declared profiles were dead config.
    /// A tool that is absent from the tool list cannot be called; prose competing
    /// with an available tool loses.
    pub(super) subagent_tool_enabled: AtomicBool,
    pub(super) final_output_tool: Arc<Mutex<Option<FinalOutputTool>>>,
    pub(super) frontend_tools: Mutex<HashMap<String, FrontendTool>>,
    pub(super) frontend_instructions: Mutex<Option<String>>,
    pub(super) prompt_manager: Mutex<PromptManager>,
    /// BR-62: tool-permission prompts still awaiting a decision, keyed by tool
    /// **request id**. One `oneshot` per prompt, registered *before* the
    /// confirmation message is yielded (so a fast client cannot answer into a
    /// void), replaces the single per-agent mpsc: a stale or duplicate
    /// `/action-required` POST can no longer resolve a *different* pending
    /// request, and [`Agent::handle_confirmation`] can tell its caller whether
    /// the id was still live — which is what makes the route idempotent.
    pub(super) pending_confirmations:
        Arc<std::sync::Mutex<HashMap<String, oneshot::Sender<PermissionConfirmation>>>>,
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
    /// BR-31 no-progress detector. The same config the `RepetitionInspector`
    /// carries: the inspector owns the hard stop (it can only block a *future*
    /// call), while the reply loop owns the escalating nudges, which it emits at
    /// the result-collection seam as soon as the failing result lands.
    pub(super) failure_loop: FailureLoopConfig,
    /// BR-18: tool-name → risk grade, derived from each tool's MCP annotations
    /// and refreshed in `prepare_tools_and_prompt` from the exact tool list the
    /// model is handed this turn (so platform/frontend tools and freshly-enabled
    /// extensions are graded too). Shared with the `PermissionInspector`, which
    /// reads it to auto-approve read-only calls in `SmartApprove`. Before this,
    /// its predecessor sets were constructed empty and never populated, so the
    /// read-only short-circuit was unreachable.
    pub(super) tool_risks: Arc<ToolRiskRegistry>,
    /// BR-63: per-session sticky reasoning effort, set by `/effort`. A per-turn
    /// effort on the `SessionConfig` (the GUI composer toggle) wins over it.
    pub(super) efforts: crate::agents::effort::EffortRegistry,
    /// BR-56: caches the normalized prefix of the transcript so `fix_conversation`
    /// only re-runs over the messages appended since the last call — the agent
    /// normalizes at least once per reply and once per provider call, and a full
    /// pass is O(history). A prefix is only reused while every message in it
    /// fingerprints the same, so a rewritten history (compaction, `HistoryReplaced`,
    /// a session reload, a different session on a shared agent) simply misses and
    /// falls back to a full normalization.
    pub(super) normalizer: crate::conversation::SharedNormalizer,
}

#[derive(Clone, Debug)]
pub enum AgentEvent {
    Message(Message),
    McpNotification((String, ServerNotification)),
    ModelChange {
        model: String,
        mode: String,
    },
    HistoryReplaced(Conversation),
    /// BR-52: the session's token counters as of the last turn/compaction
    /// boundary, emitted by the agent right after it wrote them.
    ///
    /// Token accounting only changes at those boundaries — never mid-stream —
    /// so a consumer can cache this and attach it to every event it forwards.
    /// The server used to re-read the counters from SQLite on *every* streamed
    /// chunk, which was pure redundant disk work on the hottest path.
    TokenUsage(TokenState),
    /// The turn ended **without doing its work**.
    ///
    /// A provider failure used to be yielded only as an assistant `Message`
    /// ("Ran into this error: …") after which the stream ended normally — so a
    /// caller could not distinguish a 403 from a completed turn without
    /// regex-matching English prose. `biorouter run` exited 0 on an auth failure
    /// and telemetry recorded it as a success.
    ///
    /// The human-readable `Message` is still emitted first (the desktop UX shows
    /// it); this event is the machine-checkable companion, and it is always
    /// terminal. See [`crate::agents::turn_abort`].
    TurnAborted {
        code: TurnAbortCode,
        message: String,
    },
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
        // BR-18: one risk table, shared by the agent (which refreshes it from the
        // per-turn tool list) and the permission inspector (which reads it).
        let tool_risks = Arc::new(ToolRiskRegistry::new());
        Self {
            provider: provider.clone(),
            config,
            extension_manager: Arc::new(ExtensionManager::new(provider.clone(), session_manager)),
            sub_workflows: Mutex::new(HashMap::new()),
            subagent_tool_enabled: AtomicBool::new(true),
            final_output_tool: Arc::new(Mutex::new(None)),
            frontend_tools: Mutex::new(HashMap::new()),
            frontend_instructions: Mutex::new(None),
            prompt_manager: Mutex::new(PromptManager::new()),
            pending_confirmations: Arc::new(std::sync::Mutex::new(HashMap::new())),
            tool_result_tx: tool_tx,
            tool_result_rx: Arc::new(Mutex::new(tool_rx)),
            retry_manager: RetryManager::new(),
            tool_inspection_manager: Self::create_tool_inspection_manager(
                permission_manager,
                Arc::clone(&hooks_manager),
                Arc::clone(&managed),
                Arc::clone(&tool_risks),
                provider.clone(),
            ),
            hooks_manager,
            goals: Default::default(),
            fallback_scheduler: tokio::sync::OnceCell::new(),
            vault: Mutex::new(None),
            soft_interrupts: Arc::new(std::sync::Mutex::new(Vec::new())),
            checkpoints,
            eager_compactions: Arc::new(std::sync::Mutex::new(HashSet::new())),
            injected_skills: Mutex::new(HashMap::new()),
            failure_loop: Self::failure_loop_config(Config::global()),
            tool_risks,
            efforts: Default::default(),
            normalizer: Default::default(),
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

    /// Whether any soft interrupt is still waiting to be injected. Checked at the
    /// turn's exit so a steer that landed while the *final* provider response was
    /// streaming keeps the loop alive for one more step (BR-61) instead of being
    /// stranded on the queue until some later turn.
    pub fn has_soft_interrupts(&self) -> bool {
        self.soft_interrupts
            .lock()
            .map(|q| !q.is_empty())
            .unwrap_or(false)
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
        tool_risks: Arc<ToolRiskRegistry>,
        provider: SharedProvider,
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

        // Add permission inspector (medium-high priority). BR-18: it reads the
        // shared risk registry the agent refreshes each turn from the model's
        // tool list, so `SmartApprove` auto-approves read-only-annotated tools
        // and only prompts on grades at/above the configured threshold.
        tool_inspection_manager.add_inspector(Box::new(PermissionInspector::new(
            tool_risks,
            permission_manager,
            managed,
            provider,
        )));

        // Add repetition inspector (lower priority - basic repetition checking).
        // BR-29: staged — a soft, non-blocking warning first, a hard stop only if
        // the model keeps repeating itself through it.
        // BR-30: plus the semantic heuristics (near-duplicate arg tweaks, A/B/A/B
        // oscillation), which are warn-only unless a hard stop is configured.
        let config = Config::global();
        let soft_warn_at = config
            .get_param::<u32>("BIOROUTER_REPETITION_SOFT_WARN")
            .unwrap_or(DEFAULT_REPETITION_SOFT_WARN);
        let hard_stop_at = config
            .get_param::<u32>("BIOROUTER_REPETITION_HARD_STOP")
            .unwrap_or(DEFAULT_REPETITION_HARD_STOP);
        tool_inspection_manager.add_inspector(Box::new(
            RepetitionInspector::staged(soft_warn_at, hard_stop_at)
                .with_semantic(Self::semantic_loop_config(config))
                .with_failure_loop(Self::failure_loop_config(config)),
        ));

        // Add user-configured PreToolUse hooks (runs last)
        tool_inspection_manager
            .add_inspector(Box::new(crate::hooks::HookInspector::new(hooks_manager)));

        tool_inspection_manager
    }

    /// BR-30: resolve the semantic loop-detection config.
    ///
    /// Defaults are deliberately warn-only — a heuristic that denies a call it
    /// misread is worse than one that nudges. Operators who want enforcement set
    /// `BIOROUTER_LOOP_NEAR_DUP_HARD_STOP` / `BIOROUTER_LOOP_OSCILLATION_HARD_STOP`
    /// (a value of 0 keeps the stage off).
    fn semantic_loop_config(config: &Config) -> SemanticLoopConfig {
        let defaults = SemanticLoopConfig::default();
        let positive = |value: u32| (value > 0).then_some(value);
        SemanticLoopConfig {
            enabled: config
                .get_param::<bool>("BIOROUTER_LOOP_SEMANTIC_DETECTION")
                .unwrap_or(defaults.enabled),
            similarity_threshold: config
                .get_param::<f32>("BIOROUTER_LOOP_ARG_SIMILARITY")
                .ok()
                .filter(|threshold| (0.0..=1.0).contains(threshold))
                .unwrap_or(defaults.similarity_threshold),
            near_dup_soft_warn: config
                .get_param::<u32>("BIOROUTER_LOOP_NEAR_DUP_SOFT_WARN")
                .ok()
                .map_or(defaults.near_dup_soft_warn, positive),
            near_dup_hard_stop: config
                .get_param::<u32>("BIOROUTER_LOOP_NEAR_DUP_HARD_STOP")
                .ok()
                .map_or(defaults.near_dup_hard_stop, positive),
            oscillation_soft_warn: config
                .get_param::<u32>("BIOROUTER_LOOP_OSCILLATION_SOFT_WARN")
                .ok()
                .map_or(defaults.oscillation_soft_warn, positive),
            oscillation_hard_stop: config
                .get_param::<u32>("BIOROUTER_LOOP_OSCILLATION_HARD_STOP")
                .ok()
                .map_or(defaults.oscillation_hard_stop, positive),
        }
    }

    /// BR-31: resolve the repeated-failing-result ("no progress") config.
    ///
    /// Unlike BR-30's heuristics this ships with its hard stage on: a run of
    /// identical *failures* is observed evidence, not a similarity guess. Each
    /// stage is individually disabled by setting it to 0
    /// (`BIOROUTER_FAILURE_LOOP_HARD_STOP=0` keeps the nudges but never blocks);
    /// `BIOROUTER_FAILURE_LOOP_DETECTION=false` turns the whole detector off.
    fn failure_loop_config(config: &Config) -> FailureLoopConfig {
        let defaults = FailureLoopConfig::default();
        let positive = |value: u32| (value > 0).then_some(value);
        FailureLoopConfig {
            enabled: config
                .get_param::<bool>("BIOROUTER_FAILURE_LOOP_DETECTION")
                .unwrap_or(defaults.enabled),
            similarity_threshold: config
                .get_param::<f32>("BIOROUTER_FAILURE_ERROR_SIMILARITY")
                .ok()
                .filter(|threshold| (0.0..=1.0).contains(threshold))
                .unwrap_or(defaults.similarity_threshold),
            soft_warn_at: config
                .get_param::<u32>("BIOROUTER_FAILURE_LOOP_SOFT_WARN")
                .ok()
                .map_or(defaults.soft_warn_at, positive),
            escalate_at: config
                .get_param::<u32>("BIOROUTER_FAILURE_LOOP_ESCALATE")
                .ok()
                .map_or(defaults.escalate_at, positive),
            hard_stop_at: config
                .get_param::<u32>("BIOROUTER_FAILURE_LOOP_HARD_STOP")
                .ok()
                .map_or(defaults.hard_stop_at, positive),
            // BR-51: opt in to hard-stopping a streak of *retryable* failures
            // (timeouts, transient dependency errors), which is off by default —
            // blocking the retry that would have worked is worse than one more.
            deny_retryable: config
                .get_param::<bool>("BIOROUTER_FAILURE_LOOP_DENY_RETRYABLE")
                .unwrap_or(defaults.deny_retryable),
        }
    }

    /// BR-31 result-collection seam: the escalating no-progress nudges owed to
    /// this batch's tool results.
    ///
    /// Called once the batch's results have been written into the response slots
    /// by [`Self::integrate_tool_result`], so the model sees "you have failed the
    /// same way 3 times" attached to the *third* failure — not one provider
    /// round-trip (and one more wasted call) later.
    ///
    /// `history` is the conversation as of the previous iteration; the outcomes of
    /// the batch that just ran are appended from the response slots, matching the
    /// exact same request→response pairing the inspector does on the transcript,
    /// so a streak spans iterations.
    async fn failure_loop_nudges(
        &self,
        history: &[Message],
        requests: &[ToolRequest],
        request_to_response_map: &HashMap<String, Arc<Mutex<Message>>>,
    ) -> Vec<String> {
        if !self.failure_loop.enabled {
            return Vec::new();
        }

        let mut outcomes = crate::tool_monitor::tool_outcomes_since_last_user_turn(history);
        let mut failed_tools: Vec<String> = Vec::new();

        for request in requests {
            let Ok(tool_call) = &request.tool_call else {
                continue;
            };
            let Some(slot) = request_to_response_map.get(&request.id) else {
                continue;
            };
            let response = slot.lock().await.clone();
            let Some(outcome) = crate::tool_monitor::outcome_from_response_message(
                &tool_call.name,
                &request.id,
                &response,
            ) else {
                continue;
            };
            if outcome.failure.is_some() && !failed_tools.contains(&outcome.tool_name) {
                failed_tools.push(outcome.tool_name.clone());
            }
            outcomes.push(outcome);
        }

        failed_tools
            .iter()
            .filter_map(|tool_name| {
                let nudge = crate::tool_monitor::failure_loop_nudge(
                    &self.failure_loop,
                    &outcomes,
                    tool_name,
                )?;
                // BR-67: the nudge is a loop-safety decision; put which tool has
                // been failing, and how long its streak is, on the record.
                loop_safety::emit(
                    LoopSafetyEvent::new(LoopSafetyKind::FailureLoopNudge)
                        .tool(tool_name)
                        .count(crate::tool_monitor::failing_streak(
                            &outcomes,
                            tool_name,
                            self.failure_loop.similarity_threshold,
                        )),
                );
                Some(nudge)
            })
            .collect()
    }

    /// BR-66: this batch's outcomes as the general mistake-streak counter sees
    /// them — one entry per tool call the *model* is answerable for, in request
    /// order.
    ///
    /// Two deliberate differences from BR-31's view of the same batch:
    ///
    /// * A **malformed** tool call (one the provider emitted that never parsed)
    ///   counts as a mistake. BR-31 skips it — it has no tool name to key a
    ///   per-tool failure streak on — but "the model keeps emitting garbage
    ///   calls" is exactly the streak BR-66 exists to catch.
    /// * Calls that never ran because a **guard denied** them, or because the
    ///   **user declined** them, are dropped. Those are policy verdicts, not the
    ///   model's failures; counting them would nudge the model for a decision it
    ///   did not make, on top of the warning BR-29/30/31 already sent.
    async fn mistake_outcomes(
        &self,
        requests: &[ToolRequest],
        permission_check_result: &PermissionCheckResult,
        request_to_response_map: &HashMap<String, Arc<Mutex<Message>>>,
    ) -> Vec<crate::tool_monitor::ToolOutcome> {
        let denied: HashSet<&str> = permission_check_result
            .denied
            .iter()
            .map(|request| request.id.as_str())
            .collect();

        let mut outcomes = Vec::new();
        for request in requests {
            if denied.contains(request.id.as_str()) {
                continue;
            }
            match &request.tool_call {
                Ok(tool_call) => {
                    let Some(slot) = request_to_response_map.get(&request.id) else {
                        continue;
                    };
                    let response = slot.lock().await.clone();
                    let Some(outcome) = crate::tool_monitor::outcome_from_response_message(
                        &tool_call.name,
                        &request.id,
                        &response,
                    ) else {
                        continue;
                    };
                    if crate::agents::mistakes::is_user_decline(&outcome) {
                        continue;
                    }
                    outcomes.push(outcome);
                }
                Err(error) => outcomes.push(crate::tool_monitor::ToolOutcome {
                    tool_name: crate::agents::mistakes::MALFORMED_TOOL_NAME.to_string(),
                    failure: Some(error.message.to_string()),
                    // BR-51: a call the model emitted malformed never reached a
                    // tool — the arguments themselves were the failure.
                    kind: Some(crate::agents::tool_errors::ToolErrorKind::InvalidArgs),
                }),
            }
        }
        outcomes
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
        // BR-56: the pre-fix copy exists only to render the debug diff, so only pay
        // for it when that log line will actually be emitted. The `Conversation`
        // clone itself is now a refcount bump.
        let unfixed_messages =
            tracing::enabled!(tracing::Level::DEBUG).then(|| unfixed_conversation.clone());
        let (conversation, issues) = self.normalizer.normalize(unfixed_conversation);
        if !issues.is_empty() {
            if let Some(unfixed) = &unfixed_messages {
                debug!(
                    "Conversation issue fixed: {}",
                    debug_conversation_fix(unfixed.messages(), conversation.messages(), &issues)
                );
            }
        }
        // Cheap now that the transcript is Arc-shared: this is the pre-turn
        // snapshot the retry path restores from.
        let initial_messages = conversation.clone();

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
            .filter_map(|content| content.as_text().map(|text| text.text.as_str()))
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
    ///
    /// BR-56: the transcript is normalized here — i.e. before *every* provider
    /// call, not once per reply. Inside a multi-tool turn the loop appends
    /// assistant/tool messages between provider calls, so a reply-time-only fix
    /// let the next call receive an un-normalized suffix (e.g. two consecutive
    /// assistant messages). MOIM injection already re-normalized on the way
    /// through; this closes the same hole for sessions with no MOIM provider.
    /// `BIOROUTER_NORMALIZE_EACH_TURN=false` restores the old behaviour.
    async fn assemble_turn_context(
        &self,
        session_id: &str,
        conversation: &Conversation,
        working_dir: &std::path::Path,
    ) -> Conversation {
        let (conversation, moim_injected) = super::moim::inject_moim(
            session_id,
            conversation.clone(),
            &self.extension_manager,
            working_dir,
            &self.normalizer,
        )
        .await;

        if moim_injected || !normalize_each_turn() {
            // MOIM injection normalizes on its way through.
            return conversation;
        }
        self.normalizer.normalize(conversation).0
    }

    /// Run the per-tool inspection gauntlet (inspectors → permission judge →
    /// extension-enable tracking) and eagerly dispatch approved/denied tools,
    /// returning the inspection results, permission verdict, enable-extension
    /// request ids, and the pending tool futures.
    ///
    /// BR-19: `remaining_requests` is `&mut` because a PreToolUse hook may
    /// *rewrite* a tool's input (sandbox a path, redact a payload, normalize a
    /// command). The rewrite is applied here — after the hooks ran, before
    /// anything is dispatched — and the requests the caller goes on to persist
    /// are the rewritten ones, so the transcript matches what actually executed.
    async fn inspect_and_gate_tool_requests(
        &self,
        remaining_requests: &mut Vec<ToolRequest>,
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
        let mut inspection_results = self
            .tool_inspection_manager
            .inspect_tools(
                remaining_requests,
                conversation.messages(),
                biorouter_mode,
                session,
            )
            .await?;

        // BR-19: apply any tool-input rewrite the PreToolUse hooks staged. The
        // rewritten input has NOT been seen by the security/permission
        // inspectors (they ran above, on the model's original arguments), so a
        // rewrite must not be a hole around them: re-run every inspector except
        // the hook one (re-running that would execute the user's hook commands
        // twice and let a rewrite trigger another rewrite).
        let rewrites = self.hooks_manager.take_tool_input_rewrites(&session.id);
        if !rewrites.is_empty()
            && crate::hooks::apply_tool_input_rewrites(remaining_requests, &rewrites) > 0
        {
            let mut revalidated = self
                .tool_inspection_manager
                .inspect_tools_excluding(
                    &[crate::hooks::inspector::HOOK_INSPECTOR_NAME],
                    remaining_requests,
                    conversation.messages(),
                    biorouter_mode,
                    session,
                )
                .await?;
            inspection_results.retain(|result| {
                result.inspector_name == crate::hooks::inspector::HOOK_INSPECTOR_NAME
            });
            inspection_results.append(&mut revalidated);
        }

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

    /// Integrate one completed tool result: validate it before persistence,
    /// classify a failure (BR-51), note extension-install failures, record it for
    /// PostToolUse hooks, and write it into the request's response slot.
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
        tool_error_taxonomy: crate::agents::tool_errors::ToolErrorTaxonomyConfig,
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

        // BR-51: a failure is classified once, here — the single funnel every
        // completed tool result passes through. The envelope rides on the result
        // (so the GUI and a reloaded session get it) and the typed header rides
        // in the text (so the model, and the BR-31/66 detectors reading the
        // transcript back, can tell a retryable blip from a hard failure).
        let (output, tool_error) =
            crate::agents::tool_errors::annotate_tool_result(output, tool_error_taxonomy);
        if let Some(error) = &tool_error {
            debug!(
                request_id = %request_id,
                kind = error.kind.as_str(),
                retryable = error.retryable,
                "tool call failed"
            );
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

    /// BR-32 loop seam: the periodic "are you looping?" progress check, and the
    /// staged response to its verdict.
    ///
    /// The `/goal` loop has had real stall detection for a while (fuzzy
    /// similarity of the judge's feedback across attempts, a counter that does
    /// NOT reset when tools run, a graceful give-up) — but only for sessions with
    /// a goal set, which is not where most stuck loops happen. This runs the same
    /// idea for *every* session, on a schedule: nothing until a single turn has
    /// already burned [`stall::StallCheckConfig::first_check_at`] actions without
    /// returning to the user, then one small fast-model call every
    /// `interval` actions. See [`crate::agents::stall`].
    ///
    /// Skipped when a `/goal` is active: that session already pays for an LLM
    /// judge on every stop attempt and owns a non-resetting stall budget, so a
    /// second detector would double the cost and could fight the goal's own
    /// give-up. Fail-open everywhere — no tail, no provider, a provider error, or
    /// an unreadable verdict all mean [`StallAction::Proceed`].
    async fn stall_check(
        &self,
        session_id: &str,
        conversation: &Conversation,
        actions_taken: u32,
        config: &StallCheckConfig,
        watch: &mut StallWatch,
    ) -> StallAction {
        if !config.due(actions_taken) || watch.has_given_up() {
            return StallAction::Proceed;
        }
        if self.active_goal(session_id).await.is_some() {
            return StallAction::Proceed;
        }
        let Some(tail) = crate::agents::stall::progress_tail(conversation) else {
            return StallAction::Proceed;
        };
        let Ok(provider) = self.provider().await else {
            return StallAction::Proceed;
        };
        let verdict = crate::agents::stall::check_progress(
            provider,
            &tail,
            actions_taken,
            watch.last_reason(),
        )
        .await;
        watch.record(verdict.as_deref(), config)
    }

    /// BR-19: honor a PostToolUse / PostToolUseFailure `block` decision.
    ///
    /// PostToolUse hooks were observe-only: the decision was computed and thrown
    /// away, so a hook could not reject e.g. a write that fails lint. It is now
    /// applied to the already-integrated tool response, in place — the tool has
    /// *run*, so its output is kept (its side effects stand and the model may
    /// need to see what happened) and the hook's reason is appended as corrective
    /// feedback, with the result marked as an error so the model treats it as a
    /// failure to address rather than a success to build on.
    async fn apply_post_tool_block(
        &self,
        request_id: &str,
        tool_name: &str,
        reason: &str,
        request_to_response_map: &HashMap<String, Arc<Mutex<Message>>>,
    ) {
        let Some(response_msg) = request_to_response_map.get(request_id) else {
            return;
        };
        let mut response = response_msg.lock().await;
        for content in response.content.iter_mut() {
            let MessageContent::ToolResponse(tool_response) = content else {
                continue;
            };
            if tool_response.id != request_id {
                continue;
            }
            let mut items = match &tool_response.tool_result {
                Ok(result) => result.content.clone(),
                Err(e) => vec![Content::text(e.to_string())],
            };
            items.push(Content::text(format!(
                "A PostToolUse hook blocked this result for `{tool_name}`.\n\n\
                 Hook feedback: {reason}\n\n\
                 The tool already ran, so its side effects stand. Address the feedback \
                 before continuing; do not simply retry the identical call."
            )));
            tool_response.tool_result = Ok(CallToolResult {
                content: items,
                structured_content: None,
                is_error: Some(true),
                meta: None,
            });
        }
    }

    /// Record this turn's provider usage exactly once for token accounting
    /// (no-op when the turn reported none, e.g. an error before the first usage chunk).
    ///
    /// BR-35: the same usage also feeds the per-reply budget, which is the only
    /// thing that sees the *whole reply's* spend — the session gauge tracks the
    /// live context, not what this reply has burned. Pricing is looked up per
    /// turn against the model that actually ran (a lead/worker swap mid-reply is
    /// therefore priced correctly), and only when a dollar limit is set.
    /// Returns `true` when it actually wrote the session's counters, so the
    /// caller knows a fresh [`AgentEvent::TokenUsage`] is worth emitting (BR-52).
    async fn record_turn_usage(
        &self,
        session_config: &SessionConfig,
        turn_usage: Option<crate::providers::base::ProviderUsage>,
        budget: &mut BudgetTracker,
        event_key: &str,
    ) -> Result<bool> {
        if let Some(usage) = turn_usage {
            self.record_budget_usage(budget, &usage).await;
            self.update_session_metrics(session_config, &usage, false, event_key)
                .await?;
            return Ok(true);
        }
        Ok(false)
    }

    /// BR-52: the session's token counters as the agent last wrote them.
    ///
    /// Read exactly once per turn/compaction boundary (where the counters can
    /// actually change) and carried in the event stream, instead of the server
    /// re-reading SQLite for every streamed chunk. Best-effort: a failed read
    /// yields `None` and the consumer simply keeps the state it already has.
    pub(super) async fn current_token_state(&self, session_id: &str) -> Option<TokenState> {
        match self
            .config
            .session_manager
            .get_token_counts(session_id)
            .await
        {
            Ok(counts) => Some(TokenState::from(counts)),
            Err(e) => {
                warn!("Failed to read token counts for session {session_id}: {e}");
                None
            }
        }
    }

    /// Fold one provider round-trip (a turn, or an in-reply compaction) into the
    /// BR-35 budget. Free when no budget is set.
    async fn record_budget_usage(
        &self,
        budget: &mut BudgetTracker,
        usage: &crate::providers::base::ProviderUsage,
    ) {
        if !budget.is_active() {
            return;
        }
        let provider_name = match self.provider().await {
            Ok(provider) => provider.get_name().to_string(),
            // No provider is a pricing miss, not a stop: the token and clock
            // axes still hold.
            Err(_) => String::new(),
        };
        budget.record_usage(&provider_name, usage);
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
                    // BR-29/BR-31: a loop guard tripped — the call repeated
                    // itself, or the tool has been failing the same way over and
                    // over. The user did not decline anything; telling the model
                    // they did (the old DECLINED_RESPONSE) is actively misleading
                    // and leaves it unable to diagnose the stop. Return the real
                    // reason.
                    Some(result)
                        if result.inspector_name
                            == crate::tool_monitor::REPETITION_INSPECTOR_NAME =>
                    {
                        result.reason.clone()
                    }
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

    /// BR-63: set the session's sticky reasoning effort (`/effort <level>`).
    pub async fn set_reasoning_effort(&self, session_id: &str, effort: ReasoningEffort) {
        self.efforts.set(session_id, effort).await;
    }

    /// The session's sticky reasoning effort, if `/effort` set one.
    pub async fn reasoning_effort(&self, session_id: &str) -> ReasoningEffort {
        self.efforts.get(session_id).await.unwrap_or_default()
    }

    /// Resolve the effort for one turn: an explicit per-turn effort on the
    /// request (the GUI composer toggle) wins over the session's sticky
    /// `/effort`, which in turn wins over the default (`Normal`, a no-op).
    async fn resolve_effort(&self, session_config: &SessionConfig) -> ReasoningEffort {
        match session_config.reasoning_effort {
            Some(effort) => effort,
            None => self.reasoning_effort(&session_config.id).await,
        }
    }

    /// The effort-stamped provider to run this turn's completions through, or
    /// `None` when the turn should just use the session's provider as it always
    /// has (the default effort, or a provider the effort can't be applied to).
    ///
    /// `quick`/`deep` re-stamp the model config with the effort and rebuild the
    /// provider around it, once per reply — the streaming path reads its model
    /// config off the provider, so there is nowhere else to inject a per-turn
    /// config.
    ///
    /// Failure is not fatal: an unreconstructible provider (a lead/worker
    /// composite, a provider whose registry entry is gone) falls back to the
    /// session's provider, which still gets the effort's exploration caps. That
    /// is the "degrade gracefully" the proposal asks for.
    async fn provider_with_effort(
        &self,
        effort: ReasoningEffort,
    ) -> Result<Option<Arc<dyn Provider>>> {
        if effort.is_default() {
            return Ok(None);
        }
        let provider = self.provider().await?;
        if provider.as_lead_worker().is_some() {
            return Ok(None);
        }

        let model_config = effort.apply_to_model(provider.get_model_config());
        match crate::providers::create(provider.get_name(), model_config).await {
            Ok(rebuilt) => Ok(Some(rebuilt)),
            Err(e) => {
                warn!(
                    "Reasoning effort '{}' not applied to provider '{}' ({}); \
                     falling back to the session provider (exploration caps still apply)",
                    effort.as_str(),
                    provider.get_name(),
                    e
                );
                Ok(None)
            }
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

        // BR-7: read back a tool result that was externalized out of the
        // conversation. Reads the session store, so it never touches the
        // extension manager's dispatch path.
        if tool_call.name == PLATFORM_READ_SESSION_BLOB_TOOL_NAME {
            let arguments = tool_call
                .arguments
                .map(Value::Object)
                .unwrap_or(Value::Object(serde_json::Map::new()));
            let result = self.handle_read_session_blob(arguments, session).await;
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
        } else if tool_call.name == SUBAGENT_STATUS_TOOL_NAME {
            // BR-40: poll / await / cancel a background subagent. Scoped to this
            // session's own handles, so one chat can never reach into another's.
            let arguments = tool_call
                .arguments
                .clone()
                .map(Value::Object)
                .unwrap_or(Value::Object(serde_json::Map::new()));
            handle_subagent_status_tool(arguments, session.id.clone())
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

        // BR-6: the large-response handler needs the session working dir so an
        // oversized result is offloaded to a handle the model's file/shell
        // tools can actually reach (not a bare temp path outside the sandbox).
        let large_response_ctx = super::large_response_handler::LargeResponseContext {
            session_id: session.id.clone(),
            working_dir: session.working_dir.clone(),
            tool_name: tool_call.name.to_string(),
        };
        let inner = result.result;

        // BR-58: the returned future is what `select_all` drives concurrently, so
        // this is the choke point that must bound total tool parallelism and
        // serialize overlapping write paths. The guard is acquired *inside* the
        // future (not eagerly) so parking on it does not stall the dispatch loop,
        // and is held across the whole execution + post-processing.
        //
        // The subagent tool is deliberately excluded: it recursively runs its own
        // agent loop whose leaf tools contend for this *same* global semaphore, so
        // a subagent wrapper holding a permit while its inner tools wait for one
        // would deadlock. Subagents already have their own `SUBAGENT_SEMAPHORE`;
        // their leaf tools are still bounded here (permit acquired before any
        // path lock, so a lock holder always makes progress — no deadlock).
        let dispatch_args = tool_call.arguments.clone();
        let bound_dispatch = tool_call.name != SUBAGENT_TOOL_NAME;

        (
            request_id,
            Ok(ToolCallResult {
                notification_stream: result.notification_stream,
                result: Box::new(Box::pin(async move {
                    let _dispatch_guard = if bound_dispatch {
                        Some(
                            super::tool_dispatch_limits::acquire(
                                &large_response_ctx.tool_name,
                                dispatch_args.as_ref(),
                                &large_response_ctx.working_dir,
                            )
                            .await,
                        )
                    } else {
                        None
                    };
                    super::large_response_handler::process_tool_response(
                        inner.await,
                        &large_response_ctx,
                    )
                    .await
                })),
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

    /// Offer (or withhold) the generic `subagent` tool.
    ///
    /// Agent-Drafter apps with declared worker profiles call this with `false`, so
    /// `consult` is the ONE delegation mechanism. Two armed mechanisms is what let
    /// the main agent bypass the profiles the author declared.
    pub fn set_subagent_tool_enabled(&self, enabled: bool) {
        self.subagent_tool_enabled.store(enabled, Ordering::Relaxed);
    }

    pub async fn subagents_enabled(&self, session_id: &str) -> bool {
        // An app that delegates through `consult(agent: …)` must not ALSO be
        // offered the generic `subagent` tool — see `subagent_tool_enabled`.
        if !self.subagent_tool_enabled.load(Ordering::Relaxed) {
            return false;
        }
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

        // BR-7: the retrieval half of externalized tool results. Only offered
        // when lazy blob loading is on — with the default hydrating read the
        // payloads are spliced back into the conversation at load time, so the
        // model never sees a stub and would have nothing to read back.
        if (extension_name.is_none() || extension_name.as_deref() == Some("platform"))
            && message_blobs::lazy_load_enabled()
        {
            prefixed_tools.push(platform_tools::read_session_blob_tool());
        }

        if extension_name.is_none() {
            if let Some(final_output_tool) = self.final_output_tool.lock().await.as_ref() {
                prefixed_tools.push(final_output_tool.tool());
            }

            if subagents_enabled {
                let sub_workflows = self.sub_workflows.lock().await;
                let sub_workflows_vec: Vec<_> = sub_workflows.values().cloned().collect();
                prefixed_tools.push(create_subagent_tool(&sub_workflows_vec));

                // BR-40: the poll half of the spawn→poll model. Offered only
                // when background subagents are enabled — without them there is
                // never a handle to poll.
                if subagent_handle::background_enabled() {
                    prefixed_tools.push(create_subagent_status_tool());
                }
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

    /// Register a pending tool-permission prompt and get the receiver the loop
    /// parks on. Called *before* the confirmation message is yielded so a client
    /// that answers instantly still finds a live sender (BR-62).
    pub(super) fn register_confirmation(
        &self,
        request_id: &str,
    ) -> oneshot::Receiver<PermissionConfirmation> {
        let (tx, rx) = oneshot::channel();
        if let Ok(mut pending) = self.pending_confirmations.lock() {
            pending.insert(request_id.to_string(), tx);
        }
        rx
    }

    /// Drop a pending prompt without a decision (it expired, the turn was
    /// cancelled, or it was already answered). Idempotent.
    pub(super) fn forget_confirmation(&self, request_id: &str) {
        if let Ok(mut pending) = self.pending_confirmations.lock() {
            pending.remove(request_id);
        }
    }

    /// Whether a tool-permission prompt with this request id is still awaiting a
    /// decision. Lets a route answer a duplicate/late POST idempotently instead
    /// of pretending it resolved something.
    pub fn has_pending_confirmation(&self, request_id: &str) -> bool {
        self.pending_confirmations
            .lock()
            .map(|pending| pending.contains_key(request_id))
            .unwrap_or(false)
    }

    /// Handle a confirmation response for a tool request.
    ///
    /// BR-62: routed by request id to that prompt's own channel. A decision for
    /// an id nobody is waiting on (double-click, a prompt that already expired or
    /// was cancelled, a stale client replaying an old card) is **dropped** and
    /// reported as [`ConfirmationOutcome::Unknown`] — it must never be applied to
    /// whatever other tool call happens to be pending now.
    pub async fn handle_confirmation(
        &self,
        request_id: String,
        confirmation: PermissionConfirmation,
    ) -> ConfirmationOutcome {
        let sender = self
            .pending_confirmations
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(&request_id));

        match sender {
            Some(tx) => {
                if tx.send(confirmation).is_ok() {
                    ConfirmationOutcome::Delivered
                } else {
                    // The waiter went away between our lookup and the send (turn
                    // ended/cancelled). Nothing to do, and nothing to blame.
                    debug!(
                        "Confirmation for request {} arrived after the waiter went away",
                        request_id
                    );
                    ConfirmationOutcome::Unknown
                }
            }
            None => {
                debug!(
                    "Ignoring confirmation for request {}: no prompt is awaiting a decision",
                    request_id
                );
                ConfirmationOutcome::Unknown
            }
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
                // BR-19: a turn cancelled between the PreToolUse inspector and
                // the tool-path injection point can leave staged hook context
                // behind. Drop it on a fresh prompt rather than injecting a note
                // about a tool call the user aborted; the consecutive
                // PostToolUse-block count is per-turn for the same reason.
                hooks.clear_staged_tool_hooks(&session_config.id);
                hooks.reset_post_tool_blocks(&session_config.id).await;

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
                let usage_event_key = uuid::Uuid::new_v4().to_string();
                match compact_messages(self.provider().await?.as_ref(), &conversation_to_compact, false).await {
                    Ok((compacted_conversation, summarization_usage)) => {
                        session_manager.replace_conversation(&session_config.id, &compacted_conversation).await?;
                        self.update_session_metrics(
                            &session_config,
                            &summarization_usage,
                            true,
                            &usage_event_key,
                        ).await?;
                        self.fire_compaction_hook(
                            crate::hooks::HookEvent::PostCompact,
                            &session_config.id,
                            &session.working_dir,
                            "auto",
                            None,
                        );

                        // BR-52: compaction rewrote the live gauge (the summary
                        // becomes the new input context) and billed its own turn.
                        if let Some(token_state) = self.current_token_state(&session_config.id).await {
                            yield AgentEvent::TokenUsage(token_state);
                        }

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

        // BR-63: the turn's reasoning effort. `Normal` (the default) changes
        // nothing: same provider object, same caps.
        let effort = self.resolve_effort(&session_config).await;
        let turn_provider = self.provider_with_effort(effort).await?;

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
            // Enforcement state is scoped to this user turn. The repetition
            // inspector owns policy; this guard only applies its exact-signature
            // deny decision.
            let mut turn_guard = super::turn_guard::TurnToolGuard::new();
            let mut turns_taken = 0u32;
            // BR-63: the effort scales the exploration budget — `quick` halves it
            // (never below a usable floor, never above what the user configured),
            // `deep` doubles it, `normal` leaves it exactly as configured.
            let max_turns = effort.scale_turns(
                session_config
                    .max_turns
                    .or_else(|| Config::global().get_param("BIOROUTER_MAX_TURNS").ok())
                    .unwrap_or(DEFAULT_MAX_TURNS),
            );
            // Cumulative tool calls dispatched this reply, across all iterations,
            // bounded by `max_tool_calls` so parallel fan-out can't run unbounded
            // even while `turns_taken` stays under `max_turns`.
            let mut tool_calls_taken = 0u32;
            let max_tool_calls = effort.scale_tool_calls(
                session_config
                    .max_tool_calls
                    .or_else(|| Config::global().get_param("BIOROUTER_MAX_TOOL_CALLS").ok())
                    .unwrap_or(DEFAULT_MAX_TOOL_CALLS),
            );
            let mut compaction_attempts = 0;
            // Consecutive auto-continues of a length-truncated turn; reset on any
            // tool call (real progress). Bounds the continue-on-truncation guard.
            let mut truncation_continuations = 0u32;
            // Resolve the tool-output guardrail policy once per reply (config
            // reads touch the filesystem, so we avoid doing it per tool result).
            let tool_output_guardrail =
                crate::guardrails::tool_output::ToolOutputGuardrailMode::from_config();
            // BR-51: the tool-error taxonomy policy, resolved once for the same
            // reason (a config read touches the filesystem).
            let tool_error_taxonomy =
                crate::agents::tool_errors::ToolErrorTaxonomyConfig::from_config();
            // BR-47: the auto post-edit diagnostics policy, resolved once per
            // reply (config read touches the filesystem). The tree-sitter analyzer
            // that runs the actual check is built lazily, only if a `text_editor`
            // write actually lands while the feature is active.
            let post_edit_diag_config =
                crate::agents::post_edit_diagnostics::PostEditDiagnosticsConfig::from_config();
            let mut post_edit_analyzer: Option<
                biorouter_mcp::developer::analyze::CodeAnalyzer,
            > = None;
            // BR-47: consecutive post-edit reflections this reply. Bounded by
            // `post_edit_diag_config.max_reflections` so a file that never parses
            // clean cannot wedge the turn — mirrors the PostToolUse block cap. Reset
            // to 0 whenever an edited file comes back clean, so a genuine fix
            // restores the budget.
            let mut post_edit_reflections: u32 = 0;
            // BR-50: the optional self-critique / reflection policy, resolved once
            // per reply (config read touches the filesystem). Default OFF; when a
            // user opts in it re-reads an ordinary answer for correctness before
            // it is returned, using the goal-judge LLM primitive.
            let self_critique_config =
                crate::agents::self_critique::SelfCritiqueConfig::from_config();
            // BR-50: corrective passes the critique has requested this reply,
            // bounded by `self_critique_config.max_passes` so a stubborn answer
            // cannot spin. Reply-scoped, like `post_edit_reflections`.
            let mut self_critique_passes: u32 = 0;
            // BR-48: the optional deterministic done-ness gate, resolved once per
            // reply (config read touches the filesystem). Default OFF; when a user
            // opts in it re-runs their `SuccessCheck`s before the turn may finish
            // and keeps the agent working on the failures. `done_gate_iterations`
            // is the per-reply corrective-attempt counter — it does NOT reset on
            // tool calls (mirroring the /goal iteration budget), so a check that
            // never goes green cannot loop past the cap.
            let done_gate_config =
                crate::agents::done_gate::DoneGateConfig::from_config();
            let mut done_gate_iterations: u32 = 0;
            // BR-32: the /goal stall detector, generalized to ordinary chat. Both
            // resolved once per reply (config reads hit the filesystem).
            let stall_config = crate::agents::stall::StallCheckConfig::from_config(Config::global());
            let mut stall_watch = crate::agents::stall::StallWatch::default();
            // BR-66: the general mistake streak — consecutive failed tool calls of
            // *any* kind (BR-31 only sees one tool failing one way), plus the
            // recoverable-provider-error counter that decides whether a failed
            // model call ends the turn or earns one more attempt with a hint.
            // Reply-scoped, like the stall tracker, so a streak can never leak
            // across turns.
            let mistake_config = crate::agents::mistakes::MistakeConfig::from_config(Config::global());
            let mut mistakes = crate::agents::mistakes::MistakeTracker::default();
            // Set once the stall check has told the model to wrap up: the action
            // count by which this turn must be over, so a model that ignores the
            // give-up instruction and keeps calling tools cannot spin all the way
            // to `max_turns`.
            let mut stall_deadline: Option<u32> = None;
            // BR-35: the per-reply wall-clock / token / dollar ceiling. `max_turns`
            // and `max_tool_calls` bound how many *steps* a reply takes, which is
            // not a bound on time or money — 429 backoff (~2 min/call) compounds
            // inside a single step, and one step can re-bill a 200k-token context.
            // Inert (and free) unless a limit is configured; a limit set on the
            // session wins per-axis over the global config.
            let reply_started = std::time::Instant::now();
            let mut budget = BudgetTracker::new(
                ReplyBudget::resolve(session_config.budget, Config::global()),
            );
            // Set once the budget is spent and the model has been told to wrap up:
            // the action count by which this reply must be over, mirroring
            // `stall_deadline`, so a model that keeps calling tools anyway cannot
            // spend the budget twice over.
            let mut budget_deadline: Option<u32> = None;

            loop {
                if is_token_cancelled(&cancel_token) {
                    // BR-67: a cancelled turn and a completed turn look identical
                    // in the logs otherwise.
                    loop_safety::emit(
                        LoopSafetyEvent::new(LoopSafetyKind::Cancelled)
                            .session(&session_config.id)
                            .count(turns_taken),
                    );
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
                    loop_safety::emit(
                        LoopSafetyEvent::new(LoopSafetyKind::TurnLimitStop)
                            .session(&session_config.id)
                            .count(turns_taken)
                            .limit(max_turns),
                    );
                    yield AgentEvent::Message(
                        Message::assistant().with_text(format!(
                            "I've reached my action limit for this turn ({max_turns} actions without user input), so I'm stopping here rather than because the task is necessarily complete. Would you like me to continue? (raise the cap with `max_turns` / `BIOROUTER_MAX_TURNS`.)"
                        ))
                    );
                    break;
                }
                if tool_calls_taken > max_tool_calls {
                    loop_safety::emit(
                        LoopSafetyEvent::new(LoopSafetyKind::ToolCallLimitStop)
                            .session(&session_config.id)
                            .count(tool_calls_taken)
                            .limit(max_tool_calls),
                    );
                    yield AgentEvent::Message(
                        Message::assistant().with_text(format!(
                            "I've made {tool_calls_taken} tool calls this turn, past my per-turn limit of {max_tool_calls}, so I'm stopping here rather than because the task is necessarily complete. Would you like me to continue? (raise the cap with `max_tool_calls` / `BIOROUTER_MAX_TOOL_CALLS`.)"
                        ))
                    );
                    break;
                }
                // BR-32: the stall check already told the model to wrap up and it
                // kept going. End the turn rather than let a confirmed loop run to
                // the `max_turns` cap.
                if stall_deadline.is_some_and(|deadline| turns_taken > deadline) {
                    let reason = stall_watch
                        .last_reason()
                        .unwrap_or("repeating the same actions without progress")
                        .to_string();
                    warn!("stall give-up ignored; ending the turn at action {turns_taken}");
                    // BR-67: the judge's `reason` is model prose about the user's
                    // work — the event carries the action count only.
                    loop_safety::emit(
                        LoopSafetyEvent::new(LoopSafetyKind::StallStop)
                            .session(&session_config.id)
                            .count(turns_taken),
                    );
                    yield AgentEvent::Message(
                        Message::assistant().with_text(crate::agents::stall::stopped_message(&reason))
                    );
                    break;
                }
                // BR-35: the budget was spent, the model was asked to wrap up, and
                // it kept working past its grace window. End the reply rather than
                // let it spend the budget over again.
                if budget_deadline.is_some_and(|deadline| turns_taken > deadline) {
                    let snapshot = budget.snapshot_at(reply_started.elapsed());
                    warn!(
                        elapsed_seconds = snapshot.elapsed_seconds,
                        tokens = snapshot.tokens,
                        "reply budget wrap-up ignored; ending the turn at action {turns_taken}"
                    );
                    loop_safety::emit(
                        LoopSafetyEvent::new(LoopSafetyKind::BudgetStop)
                            .session(&session_config.id)
                            .count(turns_taken)
                            .maybe_axis(snapshot.axis),
                    );
                    yield AgentEvent::Message(
                        Message::assistant()
                            .with_text(crate::agents::budget::stopped_message(&snapshot))
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

                // BR-32: periodic "are you looping?" check for a turn that has been
                // running a long time without returning to the user. Off in normal
                // chat (nothing gets 30 actions deep); the LLM call is fail-open, so
                // an error here can never break a turn.
                match self.stall_check(
                    &session_config.id,
                    &conversation,
                    turns_taken,
                    &stall_config,
                    &mut stall_watch,
                ).await {
                    StallAction::Proceed => {}
                    StallAction::Nudge { reason } => {
                        info!(actions = turns_taken, "stall check flagged a loop; nudging the model");
                        loop_safety::emit(
                            LoopSafetyEvent::new(LoopSafetyKind::StallNudge)
                                .session(&session_config.id)
                                .count(turns_taken),
                        );
                        let nudge = Message::user()
                            .with_text(crate::agents::stall::nudge_instruction(&reason, turns_taken))
                            .with_visibility(false, true);
                        session_manager.add_message(&session_config.id, &nudge).await?;
                        conversation.push(nudge);
                        yield AgentEvent::Message(
                            Message::assistant()
                                .with_system_notification(
                                    SystemNotificationType::InlineMessage,
                                    format!(
                                        "⏳ Progress check: {}",
                                        crate::agents::stall::ellipsize(&reason, 200)
                                    ),
                                )
                                .user_only(),
                        );
                    }
                    StallAction::GiveUp { reason, flags, stalled } => {
                        warn!(
                            actions = turns_taken,
                            flags,
                            stalled,
                            "stall check gave up; asking for a best-effort answer"
                        );
                        // `flags` (how many progress checks flagged this turn) is
                        // the count that tripped the give-up; the reason prose
                        // stays out of the trace.
                        loop_safety::emit(
                            LoopSafetyEvent::new(LoopSafetyKind::StallGiveUp)
                                .session(&session_config.id)
                                .count(flags)
                                .limit(stall_config.max_flags),
                        );
                        // The model gets a short grace window to write its wrap-up;
                        // after that the turn ends whether or not it complied.
                        stall_deadline =
                            Some(turns_taken + crate::agents::stall::STALL_WRAPUP_GRACE);
                        let wrapup = Message::user()
                            .with_text(crate::agents::stall::giveup_instruction(&reason))
                            .with_visibility(false, true);
                        session_manager.add_message(&session_config.id, &wrapup).await?;
                        conversation.push(wrapup);
                        let why = if stalled {
                            "the same loop kept repeating"
                        } else {
                            "no progress across several checks"
                        };
                        yield AgentEvent::Message(
                            Message::assistant()
                                .with_system_notification(
                                    SystemNotificationType::InlineMessage,
                                    format!(
                                        "⏳ Stopped looping after {flags} progress check(s) — {why}. \
                                         Wrapping up with a best-effort answer."
                                    ),
                                )
                                .user_only(),
                        );
                    }
                }

                // BR-35: the per-reply budget meter. Cheap (no LLM call, no I/O):
                // a comparison against the running totals `record_turn_usage` has
                // already folded in, skipped entirely when no limit is set.
                match budget.check_at(reply_started.elapsed()) {
                    BudgetAction::Proceed => {}
                    BudgetAction::Warn(snapshot) => {
                        // One heads-up as the reply nears its ceiling, so a long
                        // agentic turn is never a silent spend.
                        info!(
                            elapsed_seconds = snapshot.elapsed_seconds,
                            tokens = snapshot.tokens,
                            axis = snapshot.axis,
                            "reply budget is running low"
                        );
                        loop_safety::emit(
                            LoopSafetyEvent::new(LoopSafetyKind::BudgetWarn)
                                .session(&session_config.id)
                                .count(turns_taken)
                                .maybe_axis(snapshot.axis),
                        );
                        yield AgentEvent::Message(
                            Message::assistant()
                                .with_system_notification(
                                    SystemNotificationType::InlineMessage,
                                    crate::agents::budget::progress_note(&snapshot),
                                )
                                .user_only(),
                        );
                    }
                    BudgetAction::Exceeded(snapshot) => {
                        warn!(
                            elapsed_seconds = snapshot.elapsed_seconds,
                            tokens = snapshot.tokens,
                            axis = snapshot.axis,
                            "reply budget spent; asking for a wrap-up"
                        );
                        loop_safety::emit(
                            LoopSafetyEvent::new(LoopSafetyKind::BudgetExceeded)
                                .session(&session_config.id)
                                .count(turns_taken)
                                .maybe_axis(snapshot.axis),
                        );
                        // Graceful: the model is told the budget is spent (and how
                        // many tokens it has left) and gets a short grace window to
                        // summarize where it got to. The hard stop above fires only
                        // if it ignores that.
                        budget_deadline = Some(
                            turns_taken + crate::agents::budget::BUDGET_WRAPUP_GRACE,
                        );
                        let wrapup = Message::user()
                            .with_text(crate::agents::budget::wrapup_instruction(&snapshot))
                            .with_visibility(false, true);
                        session_manager.add_message(&session_config.id, &wrapup).await?;
                        conversation.push(wrapup);
                        yield AgentEvent::Message(
                            Message::assistant()
                                .with_system_notification(
                                    SystemNotificationType::InlineMessage,
                                    format!(
                                        "⏳ Budget reached ({}). Wrapping up with what I have.",
                                        snapshot.describe()
                                    ),
                                )
                                .user_only(),
                        );
                    }
                }

                let conversation_with_moim = self
                    .assemble_turn_context(&session_config.id, &conversation, &working_dir)
                    .await;

                // BR-63: the effort-stamped provider for this turn, or the
                // session's provider when the effort is the default (unchanged
                // behaviour, and it picks up a mid-session model switch).
                let iteration_provider = match &turn_provider {
                    Some(provider) => Arc::clone(provider),
                    None => self.provider().await?,
                };
                let usage_event_key = uuid::Uuid::new_v4().to_string();
                let mut stream = Self::stream_response_from_provider(
                    iteration_provider,
                    &system_prompt,
                    conversation_with_moim.messages(),
                    &tools,
                    &toolshim_tools,
                ).await?;

                let mut no_tools_called = true;
                let mut messages_to_add = Conversation::default();
                let mut tools_updated = false;
                let mut did_recovery_compact_this_iteration = false;
                // BR-66: set when a recoverable provider error was absorbed and a
                // hint pushed into `messages_to_add`; the turn continues instead of
                // ending on the error.
                let mut did_recover_provider_error_this_iteration = false;
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
                // Set by an enforcing loop denial or a non-recoverable provider
                // failure. The terminal event is emitted only after usage and
                // conversation state are durable.
                let mut pending_turn_abort: Option<(TurnAbortCode, String)> = None;

                while let Some(next) = stream.next().await {
                    if is_token_cancelled(&cancel_token) {
                        break;
                    }

                    match next {
                        Ok((response, usage)) => {
                            compaction_attempts = 0;
                            // BR-66: the provider is answering again; whatever blip
                            // was retried before is over.
                            mistakes.observe_provider_success();

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
                                    // BR-19: `mut` — a PreToolUse hook may rewrite a
                                    // tool's input inside inspect_and_gate_tool_requests,
                                    // and the rewritten request is the one dispatched,
                                    // persisted, and handed to the PostToolUse hooks.
                                    mut remaining_requests,
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
                                // Soft-stage advisories injected after this batch's tool
                                // results so the model can break the loop itself before the
                                // hard stop: BR-29/BR-30's call-shape warnings, gathered from
                                // inspection, plus BR-31's no-progress nudges, gathered from
                                // the results themselves.
                                let mut loop_warnings: Vec<String> = Vec::new();
                                // BR-47: the framed post-edit syntax diagnostics for this
                                // batch, if any. Computed at the result seam but injected
                                // *after* the tool request/response pair below, so the
                                // transcript reads "you edited X (tool response), then the
                                // syntax check on X found ...".
                                let mut pending_post_edit_diagnostics: Option<String> = None;

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
                                        &mut remaining_requests,
                                        &conversation,
                                        biorouter_mode,
                                        &session,
                                        &request_to_response_map,
                                        cancel_token.clone(),
                                    ).await?;
                                    loop_warnings = crate::tool_inspection::collect_warning_reasons(&inspection_results);

                                    // RepetitionInspector is the sole policy authority.
                                    // TurnToolGuard only converts its exact-request Deny
                                    // into a terminal event; it has no independent counter
                                    // or threshold.
                                    if let Some((result, request)) = inspection_results
                                        .iter()
                                        .filter(|result| {
                                            result.inspector_name
                                                == crate::tool_monitor::REPETITION_INSPECTOR_NAME
                                                && result.action == InspectionAction::Deny
                                        })
                                        .find_map(|result| {
                                            permission_check_result
                                                .denied
                                                .iter()
                                                .find(|request| request.id == result.tool_request_id)
                                                .map(|request| (result, request))
                                        })
                                    {
                                        if let Some(code) = turn_guard.enforce_denial(request) {
                                            warn!(
                                                tool_request_id = %request.id,
                                                "repetition policy denied a tool signature; terminating this user turn"
                                            );
                                            pending_turn_abort =
                                                Some((code, result.reason.clone()));
                                        }
                                    }

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

                                    // BR-19: PreToolUse (inspector) and PermissionRequest
                                    // (permission gate) hooks stage their additionalContext /
                                    // systemMessage there because neither return channel can
                                    // carry them — both sites used to read only the decision
                                    // and drop the rest. Drain them here, once both have run:
                                    // messages surface as inline notices, context reaches the
                                    // model with the same untrusted framing (BR-26) as the
                                    // SessionStart / UserPromptSubmit path.
                                    {
                                        let staged = self.hooks_manager.drain_tool_hook_context(&session.id);
                                        let mut hook_contexts: Vec<String> = Vec::new();
                                        for entry in staged {
                                            for msg in entry.system_messages {
                                                yield AgentEvent::Message(
                                                    Message::assistant()
                                                        .with_system_notification(
                                                            SystemNotificationType::InlineMessage,
                                                            msg,
                                                        )
                                                        .user_only(),
                                                );
                                            }
                                            hook_contexts.extend(entry.additional_context);
                                        }
                                        if !hook_contexts.is_empty() {
                                            messages_to_add.push(
                                                Message::user()
                                                    .with_text(crate::hooks::outcome::frame_hook_context(
                                                        &hook_contexts.join("\n\n"),
                                                    ))
                                                    .with_visibility(false, true),
                                            );
                                        }
                                    }

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
                                                    tool_error_taxonomy,
                                                ).await;
                                            }
                                            ToolStreamItem::Message(msg) => {
                                                yield AgentEvent::McpNotification((request_id, msg));
                                            }
                                        }
                                    }

                                    // BR-47: auto post-edit diagnostics. A successful
                                    // `text_editor` write is re-parsed with the developer
                                    // analyzer's tree-sitter grammars; any ERROR / MISSING
                                    // nodes become agent-visible corrective context, so the
                                    // model fixes broken syntax in the same turn instead of
                                    // only discovering it if it happens to run tests. Bounded
                                    // by a per-reply reflection counter so a file that never
                                    // parses clean cannot wedge the turn — the built-in twin
                                    // of the BR-19 PostToolUse block cap below. Runs off the
                                    // still-owned `post_tool_results`, before the PostToolUse
                                    // hooks consume it.
                                    if post_edit_diag_config.is_active() {
                                        use crate::agents::post_edit_diagnostics as ped;
                                        // (display path, resolved path) for each successful write.
                                        let mut edited: Vec<(String, std::path::PathBuf)> = Vec::new();
                                        for (request_id, _response_value, error_text) in &post_tool_results {
                                            if error_text.is_some() {
                                                // The write itself failed; there is nothing valid
                                                // on disk to parse.
                                                continue;
                                            }
                                            let Some(request) = remaining_requests.iter().find(|r| &r.id == request_id) else { continue };
                                            let Some(resolved) = ped::edited_path_from_request(request, &session.working_dir) else { continue };
                                            // Show the model the path it actually sent, when readable.
                                            let display = request
                                                .tool_call
                                                .as_ref()
                                                .ok()
                                                .and_then(|tc| tc.arguments.as_ref())
                                                .and_then(|a| a.get("path").or_else(|| a.get("file_path")))
                                                .and_then(|v| v.as_str())
                                                .map(str::to_string)
                                                .unwrap_or_else(|| resolved.display().to_string());
                                            edited.push((display, resolved));
                                        }
                                        if !edited.is_empty() {
                                            let analyzer = post_edit_analyzer.get_or_insert_with(
                                                biorouter_mcp::developer::analyze::CodeAnalyzer::new,
                                            );
                                            let mut files: Vec<ped::FileDiagnostics> = Vec::new();
                                            // Dedup by resolved path: a file written twice in one
                                            // batch is reported once, on its final on-disk state.
                                            let mut seen = std::collections::HashSet::new();
                                            for (display, resolved) in edited {
                                                if !seen.insert(resolved.clone()) {
                                                    continue;
                                                }
                                                let diags = analyzer.diagnose_file(&resolved);
                                                if diags.is_empty() {
                                                    continue;
                                                }
                                                files.push(ped::FileDiagnostics {
                                                    path: display,
                                                    lines: diags.iter().map(|d| d.render()).collect(),
                                                });
                                            }
                                            match ped::next_reflection(
                                                !files.is_empty(),
                                                post_edit_reflections,
                                                post_edit_diag_config.max_reflections,
                                            ) {
                                                ped::ReflectionOutcome::Reset => {
                                                    // Every edited file parsed clean: a genuine
                                                    // fix (or a clean edit) restores the budget.
                                                    post_edit_reflections = 0;
                                                }
                                                ped::ReflectionOutcome::Inject { next } => {
                                                    post_edit_reflections = next;
                                                    let total: usize = files.iter().map(|f| f.lines.len()).sum();
                                                    tracing::info!(
                                                        files = files.len(),
                                                        diagnostics = total,
                                                        reflection = post_edit_reflections,
                                                        "BR-47: injecting post-edit syntax diagnostics"
                                                    );
                                                    loop_safety::emit(
                                                        LoopSafetyEvent::new(LoopSafetyKind::PostEditDiagnostics)
                                                            .session(&session_config.id)
                                                            .count(post_edit_reflections),
                                                    );
                                                    // Held, not pushed: it must land after the
                                                    // tool response for the edit it describes.
                                                    pending_post_edit_diagnostics =
                                                        Some(ped::frame_post_edit_diagnostics(&files));
                                                }
                                                ped::ReflectionOutcome::Capped => {
                                                    // Deliver the result as-is so the turn is not
                                                    // wedged on a file that never parses clean.
                                                    tracing::info!(
                                                        cap = post_edit_diag_config.max_reflections,
                                                        "BR-47: post-edit diagnostics reflection cap reached; not injecting again this reply"
                                                    );
                                                }
                                            }
                                        }
                                    }

                                    // BR-31: the results are in. If a tool has now failed the
                                    // same way N times in a row, nudge the model here — with
                                    // the failing result still in front of it — rather than
                                    // waiting for it to burn another call. The hard stop for
                                    // a streak that survives the nudges is enforced by the
                                    // repetition inspector on the next call.
                                    loop_warnings.extend(
                                        self.failure_loop_nudges(
                                            conversation.messages(),
                                            &remaining_requests,
                                            &request_to_response_map,
                                        ).await,
                                    );

                                    // BR-66: the general streak. BR-31 above only speaks when
                                    // *one* tool has failed *the same way* N times; a run of
                                    // different tools failing in different ways — the ordinary
                                    // shape of an agent that has lost the thread — is invisible
                                    // to it. Count every failed call of any kind (malformed
                                    // calls included) and, at the cap, make the model stop and
                                    // re-plan. Warn-only: a mixed run of failures is not proof
                                    // the next call is doomed, so nothing is blocked.
                                    if let Some(nudge) = mistakes.observe_tool_outcomes(
                                        &mistake_config,
                                        &self.mistake_outcomes(
                                            &remaining_requests,
                                            &permission_check_result,
                                            &request_to_response_map,
                                        ).await,
                                    ) {
                                        tracing::info!(
                                            streak = mistakes.streak(),
                                            "Injecting mistake-streak reflect-and-replan nudge"
                                        );
                                        loop_safety::emit(
                                            LoopSafetyEvent::new(LoopSafetyKind::MistakeStreakNudge)
                                                .session(&session_config.id)
                                                .count(mistakes.streak()),
                                        );
                                        loop_warnings.push(nudge);
                                    }

                                    // PostToolUse / PostToolUseFailure hooks: awaited so their
                                    // injected context lands before the next provider call, and
                                    // (BR-19) their decision is now honored — a `block` turns the
                                    // result into corrective feedback instead of being computed
                                    // and thrown away. Bounded by POST_TOOL_HOOK_BLOCK_CAP so a
                                    // hook that always blocks cannot wedge the turn.
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
                                                let aggregate = hooks
                                                    .dispatch(event, Some(&tool_name), &payload, &working_dir)
                                                    .await;
                                                (request_id, tool_name, aggregate)
                                            });
                                        }
                                        if !post_futures.is_empty() {
                                            let mut hook_contexts: Vec<String> = Vec::new();
                                            let mut blocked_any = false;
                                            for (request_id, tool_name, aggregate) in futures::future::join_all(post_futures).await {
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
                                                if let Some(reason) = aggregate.deny_reason() {
                                                    if self.hooks_manager.note_post_tool_block(&session.id).await {
                                                        blocked_any = true;
                                                        self.apply_post_tool_block(
                                                            &request_id,
                                                            &tool_name,
                                                            reason,
                                                            &request_to_response_map,
                                                        ).await;
                                                        yield AgentEvent::Message(
                                                            Message::assistant()
                                                                .with_system_notification(
                                                                    SystemNotificationType::InlineMessage,
                                                                    format!("Hook blocked the result of {tool_name}: {reason}"),
                                                                )
                                                                .user_only(),
                                                        );
                                                    } else {
                                                        yield AgentEvent::Message(
                                                            Message::assistant()
                                                                .with_system_notification(
                                                                    SystemNotificationType::InlineMessage,
                                                                    format!(
                                                                        "A PostToolUse hook has blocked {tool_name} {} times; delivering the result anyway.",
                                                                        crate::hooks::POST_TOOL_HOOK_BLOCK_CAP
                                                                    ),
                                                                )
                                                                .user_only(),
                                                        );
                                                    }
                                                }
                                                if let Some(ctx) = aggregate.joined_context() {
                                                    hook_contexts.push(ctx);
                                                }
                                            }
                                            if !blocked_any {
                                                self.hooks_manager.reset_post_tool_blocks(&session.id).await;
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

                                // BR-47: the post-edit syntax diagnostics for this batch,
                                // injected here so they sit right after the tool responses
                                // for the edits they describe. Model-visible only — like the
                                // loop-guard nudges, this is corrective plumbing the user
                                // does not need in the transcript.
                                if let Some(diagnostics_text) = pending_post_edit_diagnostics.take() {
                                    messages_to_add.push(
                                        Message::user()
                                            .with_id(format!("msg_{}", Uuid::new_v4()))
                                            .with_text(diagnostics_text)
                                            .with_visibility(false, true),
                                    );
                                }

                                // Soft stage (BR-29/30/31): the repeated — or repeatedly
                                // failing — call *ran*; nudge the model right after its
                                // result so it changes approach before the hard stop fires.
                                // Model-visible only — this is loop-safety plumbing, not
                                // something the user needs in the transcript.
                                if !loop_warnings.is_empty() {
                                    tracing::info!(
                                        warnings = loop_warnings.len(),
                                        "Injecting loop-guard soft warning"
                                    );
                                    messages_to_add.push(
                                        Message::user()
                                            .with_id(format!("msg_{}", Uuid::new_v4()))
                                            .with_text(crate::tool_inspection::frame_loop_warnings(&loop_warnings))
                                            .with_visibility(false, true),
                                    );
                                }

                                no_tools_called = false;
                                if pending_turn_abort.is_some() {
                                    break;
                                }
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
                            let compaction_usage_event_key = uuid::Uuid::new_v4().to_string();
                            match compact_messages_with_recovery(self.provider().await?.as_ref(), &conversation, recovery).await {
                                Ok((compacted_conversation, usage)) => {
                                    session_manager.replace_conversation(&session_config.id, &compacted_conversation).await?;
                                    // BR-35: a summarization round-trip inside the
                                    // reply is spend like any other — bill it to the
                                    // budget too, not just the session gauge.
                                    self.record_budget_usage(&mut budget, &usage).await;
                                    self.update_session_metrics(
                                        &session_config,
                                        &usage,
                                        true,
                                        &compaction_usage_event_key,
                                    ).await?;
                                    conversation = compacted_conversation;
                                    did_recovery_compact_this_iteration = true;
                                    self.fire_compaction_hook(
                                        crate::hooks::HookEvent::PostCompact,
                                        &session_config.id,
                                        &session.working_dir,
                                        "auto",
                                        Some("context_overflow"),
                                    );
                                    // BR-52: recovery compaction moved the counters too.
                                    if let Some(token_state) = self.current_token_state(&session_config.id).await {
                                        yield AgentEvent::TokenUsage(token_state);
                                    }
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
                            // BR-66: a non-context provider error used to end the turn
                            // outright, handing the user a "please retry" string for a
                            // blip the agent could have absorbed itself. Give a
                            // *recoverable* error one more attempt with a hint in
                            // context; a fatal one (auth, rate limit, unsupported) or a
                            // spent retry budget still stops, with the conversation
                            // preserved so the user can just say "continue".
                            match mistakes.observe_provider_error(&mistake_config, provider_err) {
                                crate::agents::mistakes::ProviderErrorAction::Recover { notice, attempt, limit } => {
                                    warn!(
                                        "Provider call failed ({provider_err}); retrying with a hint ({attempt}/{limit})"
                                    );
                                    // BR-67: retries are a loop-safety decision too —
                                    // the error text itself never enters the trace.
                                    loop_safety::emit(
                                        LoopSafetyEvent::new(LoopSafetyKind::ProviderErrorRecover)
                                            .session(&session_config.id)
                                            .count(attempt)
                                            .limit(limit),
                                    );
                                    yield AgentEvent::Message(
                                        Message::assistant().with_system_notification(
                                            SystemNotificationType::InlineMessage,
                                            format!("Model call failed: {provider_err}. Retrying ({attempt}/{limit})…"),
                                        )
                                    );
                                    // Model-visible only: the hint is loop plumbing, and
                                    // the user already has the notification above.
                                    messages_to_add.push(
                                        Message::user()
                                            .with_id(format!("msg_{}", Uuid::new_v4()))
                                            .with_text(crate::tool_inspection::frame_loop_warnings(
                                                std::slice::from_ref(&notice),
                                            ))
                                            .with_visibility(false, true),
                                    );
                                    did_recover_provider_error_this_iteration = true;
                                    break;
                                }
                                crate::agents::mistakes::ProviderErrorAction::Stop { notice } => {
                                    loop_safety::emit(
                                        LoopSafetyEvent::new(LoopSafetyKind::ProviderErrorStop)
                                            .session(&session_config.id)
                                            .count(mistakes.provider_errors()),
                                    );
                                    let message = Message::assistant().with_text(notice);
                                    yield AgentEvent::Message(message.clone());
                                    messages_to_add.push(message);
                                    pending_turn_abort = Some((
                                        TurnAbortCode::ProviderFailure {
                                            kind: provider_err.kind(),
                                        },
                                        provider_err.to_string(),
                                    ));
                                    break;
                                }
                            }
                        }
                    }
                }

                // Record the turn exactly once, whether the stream finished, was
                // cancelled, or errored out. The provider still processed (and
                // billed) whatever it reported.
                let usage_recorded = self
                    .record_turn_usage(
                        &session_config,
                        turn_usage.take(),
                        &mut budget,
                        &usage_event_key,
                    )
                    .await?;

                // BR-52: the counters just moved — publish them so downstream
                // consumers (the SSE route) can attach a fresh `TokenState` to
                // every event they forward without touching the DB per token.
                if usage_recorded {
                    if let Some(token_state) = self.current_token_state(&session_config.id).await {
                        yield AgentEvent::TokenUsage(token_state);
                    }
                }

                if tools_updated {
                    (tools, toolshim_tools, system_prompt) =
                        self.prepare_tools_and_prompt(&session_config.id, &session.working_dir).await?;
                }
                let mut exit_chat = false;
                if pending_turn_abort.is_some() {
                    // The typed failure is emitted after this iteration's messages
                    // and usage have been persisted below.
                } else if no_tools_called {
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
                    } else if did_recover_provider_error_this_iteration {
                        // BR-66: the provider call failed recoverably and the hint is
                        // already in `messages_to_add`. No tool ran and the model said
                        // nothing, so this is not a finished turn — take the retry
                        // rather than ending the turn (or handing it to the retry
                        // manager, whose job is a *completed* response that failed
                        // validation). Bounded by `provider_error_retries`, and by
                        // `max_turns` like every other iteration.
                    } else {
                        match self.handle_retry_logic(&mut conversation, &session_config, initial_messages.messages()).await {
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

                if let Some((code, message)) = pending_turn_abort.take() {
                    yield AgentEvent::TurnAborted { code, message };
                    break;
                }

                // BR-61: a soft interrupt queued while the final provider response
                // was streaming arrives too late for this iteration's drain, and a
                // turn that is about to exit would leave it parked until some later
                // turn injected it out of context. Keep the loop alive for one more
                // step so the steer is drained, answered, and seen now. Still bounded
                // by max_turns / max_tool_calls, which are re-checked at the top.
                if exit_chat && self.has_soft_interrupts() {
                    info!("soft interrupt pending at turn exit; continuing the loop to consume it");
                    exit_chat = false;
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

                    // BR-48: deterministic done-ness gate. When enabled, re-run
                    // the configured `SuccessCheck`s before the turn is allowed to
                    // finish; on failure inject *what failed* and keep working
                    // (iterating on the current diff, never resetting the way the
                    // workflow retry does). Skipped when the turn is already being
                    // wound down under a stall/budget deadline or after a cancel —
                    // those wrap-ups must be allowed to end. Default OFF, so this
                    // is inert unless a user opted in. Runs before the (optional,
                    // LLM) self-critique so a broken build is caught deterministically
                    // and cheaply, without spending a judge call.
                    if done_gate_config.is_active()
                        && stall_deadline.is_none()
                        && budget_deadline.is_none()
                        && !is_token_cancelled(&cancel_token)
                    {
                        let failures = crate::agents::retry::collect_check_failures(
                            &done_gate_config.checks,
                            done_gate_config.timeout,
                            Some(working_dir.as_path()),
                        )
                        .await;
                        if !failures.is_empty() {
                            if done_gate_iterations < done_gate_config.max_iterations {
                                done_gate_iterations += 1;
                                loop_safety::emit(
                                    LoopSafetyEvent::new(LoopSafetyKind::DoneGateBlock)
                                        .session(&session_config.id)
                                        .count(done_gate_iterations)
                                        .limit(done_gate_config.max_iterations),
                                );
                                let feedback = Message::user()
                                    .with_text(crate::agents::done_gate::gate_instruction(
                                        &failures,
                                    ))
                                    .with_visibility(false, true);
                                session_manager
                                    .add_message(&session_config.id, &feedback)
                                    .await?;
                                conversation.push(feedback);
                                // Keep looping so the model fixes the failures;
                                // skip this iteration's Stop hook. The counter does
                                // not reset on the tool calls the fix requires, so
                                // the loop is bounded by `max_iterations`.
                                tokio::task::yield_now().await;
                                continue;
                            } else {
                                // Budget spent with checks still red: let the turn
                                // finish rather than wedge, but tell the user it is
                                // on unmet conditions.
                                loop_safety::emit(
                                    LoopSafetyEvent::new(LoopSafetyKind::DoneGateGiveUp)
                                        .session(&session_config.id)
                                        .count(done_gate_iterations)
                                        .limit(done_gate_config.max_iterations),
                                );
                                yield AgentEvent::Message(
                                    Message::assistant()
                                        .with_system_notification(
                                            SystemNotificationType::InlineMessage,
                                            crate::agents::done_gate::giveup_notice(
                                                done_gate_iterations,
                                                &failures,
                                            ),
                                        )
                                        .user_only(),
                                );
                            }
                        }
                    }

                    // BR-50: optional self-critique pass on an *ordinary* answer.
                    // Skipped when a /goal is active (its Stop-hook judge already
                    // re-reads the work), when the turn is already being wrapped up
                    // under a stall/budget deadline (a critique would re-expand a
                    // turn we are deliberately ending), when cancelled, or once the
                    // per-reply pass budget is spent. Default OFF, so this is inert
                    // unless a user opted in.
                    if self_critique_config.is_active()
                        && active_goal.is_none()
                        && stall_deadline.is_none()
                        && budget_deadline.is_none()
                        && self_critique_passes < self_critique_config.max_passes
                        && !is_token_cancelled(&cancel_token)
                    {
                        if let Some(reason) = self.run_self_critique(&conversation).await {
                            self_critique_passes += 1;
                            loop_safety::emit(
                                LoopSafetyEvent::new(LoopSafetyKind::SelfCritiqueRevise)
                                    .session(&session_config.id)
                                    .count(self_critique_passes),
                            );
                            let feedback = Message::user()
                                .with_text(
                                    crate::agents::self_critique::revise_instruction(&reason),
                                )
                                .with_visibility(false, true);
                            session_manager.add_message(&session_config.id, &feedback).await?;
                            conversation.push(feedback);
                            // Keep looping so the model revises; skip this
                            // iteration's Stop hook. The next finish attempt runs
                            // Stop hooks normally, and the critique won't fire again
                            // once the pass budget is spent.
                            tokio::task::yield_now().await;
                            continue;
                        }
                    }

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
    use crate::permission::{Permission, PermissionConfirmation};
    use crate::workflow::Response;

    fn confirmation(permission: Permission) -> PermissionConfirmation {
        PermissionConfirmation {
            principal_type: crate::permission::permission_confirmation::PrincipalType::Tool,
            permission,
        }
    }

    /// BR-62's core safety property. Confirmations used to land on a single
    /// per-agent mpsc, so a decision for one request could be picked up by
    /// whatever tool call happened to be waiting — a late "allow" for a prompt
    /// the user had long since dismissed could approve an unrelated later call.
    /// Now each prompt owns its own channel, keyed by request id.
    #[tokio::test]
    async fn confirmation_reaches_only_its_own_request() {
        let agent = Agent::new();

        let rx_a = agent.register_confirmation("req-a");
        let rx_b = agent.register_confirmation("req-b");

        let outcome = agent
            .handle_confirmation("req-b".to_string(), confirmation(Permission::AllowOnce))
            .await;
        assert_eq!(outcome, ConfirmationOutcome::Delivered);

        // B got exactly the decision meant for it...
        let decided = rx_b.await.expect("b's prompt received its decision");
        assert_eq!(decided.permission, Permission::AllowOnce);

        // ...and A is untouched, still awaiting its own.
        assert!(agent.has_pending_confirmation("req-a"));
        assert!(!agent.has_pending_confirmation("req-b"));
        drop(rx_a);
    }

    /// A duplicate click, or a decision for a prompt that already expired or was
    /// cancelled, must be dropped — not applied to some other pending call. This
    /// is what makes `/action-required` safe to retry.
    #[tokio::test]
    async fn duplicate_and_stale_confirmations_are_dropped() {
        let agent = Agent::new();

        let rx = agent.register_confirmation("req-a");
        assert_eq!(
            agent
                .handle_confirmation("req-a".to_string(), confirmation(Permission::AllowOnce))
                .await,
            ConfirmationOutcome::Delivered
        );
        let _ = rx.await;

        // Second click on the same card: nothing is waiting on that id any more.
        assert_eq!(
            agent
                .handle_confirmation("req-a".to_string(), confirmation(Permission::AlwaysAllow))
                .await,
            ConfirmationOutcome::Unknown
        );

        // A decision for an id that was never registered at all.
        assert_eq!(
            agent
                .handle_confirmation(
                    "never-existed".to_string(),
                    confirmation(Permission::DenyOnce)
                )
                .await,
            ConfirmationOutcome::Unknown
        );
    }

    /// After a prompt is forgotten (it expired, or the turn was cancelled), a
    /// decision arriving late is reported as unknown rather than silently
    /// resolving anything.
    #[tokio::test]
    async fn a_forgotten_prompt_no_longer_accepts_a_decision() {
        let agent = Agent::new();

        let _rx = agent.register_confirmation("req-a");
        assert!(agent.has_pending_confirmation("req-a"));

        agent.forget_confirmation("req-a");
        assert!(!agent.has_pending_confirmation("req-a"));

        assert_eq!(
            agent
                .handle_confirmation("req-a".to_string(), confirmation(Permission::AllowOnce))
                .await,
            ConfirmationOutcome::Unknown
        );

        // forget is idempotent.
        agent.forget_confirmation("req-a");
    }

    /// If the waiting side goes away (turn ended/cancelled) between the lookup and
    /// the send, the decision is dropped, not blamed on a live prompt.
    #[tokio::test]
    async fn a_decision_for_an_abandoned_prompt_is_unknown() {
        let agent = Agent::new();

        let rx = agent.register_confirmation("req-a");
        drop(rx);

        assert_eq!(
            agent
                .handle_confirmation("req-a".to_string(), confirmation(Permission::AllowOnce))
                .await,
            ConfirmationOutcome::Unknown
        );
    }

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

/// BR-32: the reply loop's stall-check seam — when it runs, when it stays silent,
/// and who owns stall detection when a `/goal` is set.
#[cfg(test)]
mod stall_seam_tests {
    use super::*;
    use crate::agents::AgentConfig;
    use crate::config::permission::PermissionManager;
    use crate::config::BioRouterMode;
    use crate::model::ModelConfig;
    use crate::providers::base::{ProviderMetadata, ProviderUsage, Usage};
    use crate::providers::errors::ProviderError;
    use crate::session::session_manager::SessionType;
    use crate::session::SessionManager;
    use async_trait::async_trait;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::TempDir;

    /// A judge that always reports a loop, and counts how often it was consulted —
    /// so a test can assert the check did NOT cost a provider round-trip.
    struct LoopyJudge {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Provider for LoopyJudge {
        fn metadata() -> ProviderMetadata {
            ProviderMetadata::new(
                "loopy",
                "Loopy",
                "",
                "loopy-model",
                vec!["loopy-model"],
                "",
                vec![],
            )
        }

        fn get_name(&self) -> &str {
            "loopy"
        }

        async fn complete_with_model(
            &self,
            _model_config: &ModelConfig,
            _system: &str,
            _messages: &[Message],
            _tools: &[Tool],
        ) -> Result<(Message, ProviderUsage), ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok((
                Message::assistant().with_text(
                    r#"{"looping": true, "reason": "the same failing shell command, six times"}"#,
                ),
                ProviderUsage::new("loopy-model".to_string(), Usage::default()),
            ))
        }

        fn get_model_config(&self) -> ModelConfig {
            ModelConfig::new_or_fail("loopy-model")
        }
    }

    /// An agent over an isolated session store, wired to the counting judge.
    async fn agent_with_judge(dir: &std::path::Path) -> (Agent, String, Arc<AtomicUsize>) {
        let session_manager = Arc::new(SessionManager::new(dir.to_path_buf()));
        let permission_manager = Arc::new(PermissionManager::new(dir.to_path_buf()));
        let agent = Agent::with_config(AgentConfig::new(
            session_manager,
            permission_manager,
            None,
            BioRouterMode::Auto,
        ));
        let session_id = agent
            .config
            .session_manager
            .create_session(PathBuf::from("."), "stall".to_string(), SessionType::User)
            .await
            .unwrap()
            .id;
        let calls = Arc::new(AtomicUsize::new(0));
        agent
            .update_provider(
                Arc::new(LoopyJudge {
                    calls: Arc::clone(&calls),
                }),
                &session_id,
            )
            .await
            .unwrap();
        (agent, session_id, calls)
    }

    fn busy_conversation() -> Conversation {
        let mut conversation = Conversation::default();
        conversation.push(Message::user().with_text("fix the failing build"));
        conversation.push(Message::assistant().with_text("Running the build again."));
        conversation
    }

    #[tokio::test]
    async fn a_normal_turn_never_pays_for_the_check() {
        let dir = TempDir::new().unwrap();
        let (agent, session_id, calls) = agent_with_judge(dir.path()).await;
        let config = StallCheckConfig::default();
        let mut watch = StallWatch::default();

        for actions in [1u32, 12, 29] {
            let action = agent
                .stall_check(
                    &session_id,
                    &busy_conversation(),
                    actions,
                    &config,
                    &mut watch,
                )
                .await;
            assert_eq!(action, StallAction::Proceed);
        }
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "no provider round-trip before the threshold"
        );
    }

    #[tokio::test]
    async fn a_long_turn_is_checked_and_nudged() {
        let dir = TempDir::new().unwrap();
        let (agent, session_id, calls) = agent_with_judge(dir.path()).await;
        let config = StallCheckConfig::default();
        let mut watch = StallWatch::default();

        let action = agent
            .stall_check(&session_id, &busy_conversation(), 30, &config, &mut watch)
            .await;
        match action {
            StallAction::Nudge { reason } => assert!(reason.contains("same failing shell command")),
            other => panic!("expected a nudge, got {other:?}"),
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1, "exactly one check");
    }

    #[tokio::test]
    async fn a_goal_session_keeps_its_own_stall_detector() {
        let dir = TempDir::new().unwrap();
        let (agent, session_id, calls) = agent_with_judge(dir.path()).await;
        agent
            .set_goal(&session_id, "the build passes".to_string())
            .await;
        let config = StallCheckConfig::default();
        let mut watch = StallWatch::default();

        let action = agent
            .stall_check(&session_id, &busy_conversation(), 30, &config, &mut watch)
            .await;
        assert_eq!(
            action,
            StallAction::Proceed,
            "the goal loop already judges every stop and owns its own stall budget"
        );
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "a goal session must not pay for a second loop judge"
        );
    }

    #[tokio::test]
    async fn a_turn_that_gave_up_is_not_re_checked() {
        let dir = TempDir::new().unwrap();
        let (agent, session_id, calls) = agent_with_judge(dir.path()).await;
        let config = StallCheckConfig::default();
        let mut watch = StallWatch::default();

        // Three near-identical looping verdicts (checks at 30/40/50) → give up.
        for actions in [30u32, 40, 50] {
            agent
                .stall_check(
                    &session_id,
                    &busy_conversation(),
                    actions,
                    &config,
                    &mut watch,
                )
                .await;
        }
        assert!(watch.has_given_up());
        let checks_at_giveup = calls.load(Ordering::SeqCst);
        assert_eq!(checks_at_giveup, 3);

        // The wrap-up window must not re-run the judge.
        let action = agent
            .stall_check(&session_id, &busy_conversation(), 60, &config, &mut watch)
            .await;
        assert_eq!(action, StallAction::Proceed);
        assert_eq!(calls.load(Ordering::SeqCst), checks_at_giveup);
    }

    #[tokio::test]
    async fn a_provider_less_agent_fails_open() {
        let dir = TempDir::new().unwrap();
        let session_manager = Arc::new(SessionManager::new(dir.path().to_path_buf()));
        let permission_manager = Arc::new(PermissionManager::new(dir.path().to_path_buf()));
        let agent = Agent::with_config(AgentConfig::new(
            session_manager,
            permission_manager,
            None,
            BioRouterMode::Auto,
        ));
        let mut watch = StallWatch::default();

        let action = agent
            .stall_check(
                "no-such-session",
                &busy_conversation(),
                30,
                &StallCheckConfig::default(),
                &mut watch,
            )
            .await;
        assert_eq!(action, StallAction::Proceed, "no provider → no verdict");
    }
}
