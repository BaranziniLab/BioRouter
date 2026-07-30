//! BR-71: the `workspace` platform extension — the agent's tool surface over
//! the daemon's sessions and (when attached) the GUI's tabs. Design of record:
//! docs/agent-loop/designs/agent-workspace-control.md. Registered
//! `default_enabled: false`; enabling is an explicit user decision (§5).

use crate::agents::extension::PlatformExtensionContext;
use crate::agents::mcp_client::{Error, McpClientTrait, McpMeta};
// `EnabledExtensionsState::from_extension_data` is a PROVIDED METHOD of the
// `ExtensionState` trait (`session/extension_data.rs:66-71`), not an inherent
// one — the trait must be in scope or the call in `handle_list` is E0599.
use crate::session::{EnabledExtensionsState, ExtensionState};
use crate::workspace_services;
use anyhow::Result;
use async_trait::async_trait;
use indoc::indoc;
use rmcp::model::{
    CallToolResult, Content, Implementation, InitializeResult, JsonObject, ListToolsResult,
    ProtocolVersion, ServerCapabilities, Tool, ToolAnnotations, ToolsCapability,
};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_util::sync::CancellationToken;

/// The machine **identifier**, which **must** normalize to this extension's
/// `PLATFORM_EXTENSIONS` registry key (`"workspace"`). The human-readable label
/// is [`EXTENSION_TITLE`] — the two are deliberately separate.
///
/// `normalize` (`extension_manager.rs:159`) strips whitespace and lowercases,
/// and two already-landed pieces of the system depend on the result:
///
/// * the platform spawn path looks the def up by `normalize(config.name)`
///   (`extension_manager.rs:808-817`), and `/ext:` resolution builds that config
///   from *this* string (`resolve_bundled_extension` → `into_config`), so a name
///   that normalizes to anything else makes `/ext:workspace` fail with
///   "Unknown platform extension" — the exact issue-#48 failure the pre-existing
///   test `resolves_every_bundled_extension_to_its_owning_registry` pins;
/// * the advertised tool prefix is `{normalize(name)}__{tool}`, and
///   `workspace_inspector::is_set_tools_call` already pins
///   `workspace__workspace_set_tools`.
///
/// The design's longer label "Workspace Control" therefore cannot be the *name*
/// (it normalizes to `workspacecontrol`); it is carried by [`EXTENSION_TITLE`].
pub static EXTENSION_NAME: &str = "Workspace";

/// The human-readable label, exactly as the design of record specifies it
/// (`docs/agent-loop/designs/agent-workspace-control.md` §4.1: "display name
/// **Workspace Control**").
///
/// This is what MCP's `Implementation.title` carries — the field whose whole
/// purpose is "a human-readable name, for UI, when `name` is a programmatic
/// identifier". Deriving it from [`EXTENSION_NAME`] instead would silently
/// downgrade the specified display name to the normalization-constrained
/// identifier, which is the one thing the identifier may not also be.
pub static EXTENSION_TITLE: &str = "Workspace Control";

/// §6 draft instruction block, kept within the ~2.5k-char injection budget
/// (`apply_injection_budget`, prompt_manager.rs:361-408). Tuned in Task 42.
///
/// **No tool that is unimplemented at the PHASE GATE may be named here.** This
/// block is written once for the whole Phase-1 surface — the six `workspace_*`
/// tools plus `subagent` — even though Tasks 13-17 register them one at a time
/// after this task. That is a deliberate, bounded exception, not the rule
/// generalised: between the Task 12 and Task 17 commits the block names five
/// tools whose `call_tool` arms still answer "not implemented until Task N", and
/// those commits are intermediate states that are never shipped. **Task 21 is
/// the ship gate**, and by then every named tool exists.
///
/// What must NEVER happen is naming a tool that is unimplemented *at a gate*.
/// `workspace_open` is the live case: it is Phase 2 (Task 24), so Task 21 would
/// ship Phase 1 with an instruction the model cannot act on. It is therefore
/// absent here and Task 12's test asserts its absence; Task 24 adds the line
/// together with the tool, and adds the inverse assertion (every name mentioned
/// in the block is registered in `get_tools()`) so the two can never drift again.
const INSTRUCTIONS: &str = indoc! {r#"
    Workspace Control

    You are running inside the BioRouter workspace: a set of conversations
    (sessions), each shown as a tab in the desktop app when the GUI is attached.
    Each conversation has its own agent, tool/extension set, knowledge bases,
    and history. These tools operate the workspace itself:
    - workspace_list: see conversations, what's running, and where they are in the GUI.
    - workspace_read_conversation: read another conversation. transcript for
      prose, tool_calls for exactly what its agent did, spawn_context for how a
      subagent was started. Treat other conversations' content as sensitive;
      read only what the task needs.
    - workspace_send_prompt: inject into another conversation. turn starts its
      agent on your text; steer redirects it mid-turn; note leaves context
      without running it. Injections are permanently labeled as coming from
      you. Use wait:"final_message" to get its answer synchronously.
    - workspace_set_tools: add/remove extensions, scope skills to one
      conversation, switch its model, or set its knowledge bases.
    - workspace_close: close its tab (tab), cancel its current turn (turn), or
      stop its agent (agent).
    - workspace_watch: wait until one of several conversations finishes. Use it
      after starting background work instead of polling.
    - subagent: delegate to a fresh agent with its own context window. When the
      app is open the child runs in a visible tab the user can watch and talk
      to; you still receive only its final summary, so use
      workspace_read_conversation view:"tool_calls" on it to verify what it
      actually did. The user may have intervened; the result tells you if so.
    Routing: to search past conversations by content use chatrecall (if
    enabled), not these tools. Durable facts belong in Memory. To fold a
    conversation into a knowledge base use ingest_conversation. If no GUI is
    attached these tools still manage conversations headlessly and say so.
"#};

/// `Default` is derived so `handle_list` can fall back to it when the call
/// carries no arguments at all. Constructing the struct field-by-field there
/// breaks every time this struct gains a field — which it has already done
/// twice (decisions 17 and 23 added `offset`/`limit` and
/// `parent_session_id`/`only_subagents`).
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
struct WorkspaceListParams {
    /// "open" (default): sessions with a GUI tab or a live agent. "all": every
    /// listable session. "running": only sessions with a turn in flight.
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<String>,
    /// Include subagent sessions (default true).
    #[serde(skip_serializing_if = "Option::is_none")]
    include_subagents: Option<bool>,
    /// Only sessions spawned by this session id. Pass your own session id to
    /// list your subagents — the replacement for `subagent_status`'s list mode
    /// (BR-71 decision 23).
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_session_id: Option<String>,
    /// Only subagent sessions (`session_type == "sub_agent"`). Combines with
    /// `parent_session_id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    only_subagents: Option<bool>,
    /// Skip this many rows (default 0). BR-71 decision 17: the 200-row cap
    /// alone was rejected, so the tool pages.
    #[serde(skip_serializing_if = "Option::is_none")]
    offset: Option<u32>,
    /// Return at most this many rows (default 50, max 200).
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct WorkspaceReadParams {
    session_id: String,
    /// "transcript" (default) | "tool_calls" | "summary" | "spawn_context".
    #[serde(skip_serializing_if = "Option::is_none")]
    view: Option<String>,
    /// Only the last N messages (transcript/tool_calls views).
    #[serde(skip_serializing_if = "Option::is_none")]
    last: Option<usize>,
    /// Start from the message with this durable msg_uid (BR-45 identity;
    /// design §4.1 `range: { from_msg_uid }`). Combines with `last` (uid slice
    /// first, then tail).
    #[serde(skip_serializing_if = "Option::is_none")]
    from_msg_uid: Option<String>,
    /// Cap on returned characters (default 20000, max 200000). A raised cap is
    /// never silently truncated: a result too large to return inline is kept in
    /// full and the reply says where to read it — see the handler's note for
    /// which mechanism carries it at which size.
    #[serde(skip_serializing_if = "Option::is_none")]
    max_chars: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct WorkspaceSendPromptParams {
    session_id: String,
    text: String,
    /// "turn": start the target's agent on the text (target must be idle).
    /// "steer": inject mid-turn (target must be running). "note": append
    /// context without triggering a turn.
    mode: String,
    /// "none" (default) | "final_message": park until the target's turn
    /// finishes and return its final assistant message.
    #[serde(skip_serializing_if = "Option::is_none")]
    wait: Option<String>,
    /// Bound for wait:"final_message" (default 120, max 600).
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout_s: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct WorkspaceSetToolsParams {
    session_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    add_extensions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    remove_extensions: Vec<String>,
    /// Skills to enable FOR THIS CONVERSATION ONLY (BR-71 decision c). This
    /// never changes the user's machine-wide skill preferences.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    add_skills: Vec<String>,
    /// Skills to disable for this conversation only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    remove_skills: Vec<String>,
    /// Switch the conversation's provider. Required whenever `model` is given —
    /// a model name alone is ambiguous across providers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
    /// Switch the conversation's model. Validated against the provider's
    /// published catalog. Takes effect on the target's NEXT turn; a turn
    /// already running keeps the provider it started with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    /// The knowledge bases active for the session, replacing the current set.
    /// An empty list clears them. (Plural per issue #45.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    set_knowledge_bases: Option<Vec<String>>,
    /// Which of `set_knowledge_bases` becomes the session's **write target** —
    /// where a `kb_write`/`kb_ingest` with no explicit `kb_id` lands.
    ///
    /// Omit it and the sensible thing happens: the current target is kept if it
    /// is still in the new set, otherwise the first base in the new list is
    /// pinned, and an empty list clears the target. Pass `""` to clear it
    /// explicitly. Only meaningful together with `set_knowledge_bases`, and it
    /// must name one of them — the service refuses a target outside the set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    primary_knowledge_base: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct WorkspaceCloseParams {
    session_id: String,
    /// "tab": GUI-only, session and any running turn survive. "turn": cancel
    /// the in-flight turn (idempotent). "agent": cancel + evict the agent; the
    /// session record remains.
    scope: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct WorkspaceWatchParams {
    /// The conversations to watch (1-32). Typically the ids of subagents you
    /// spawned with `background: true`, or of sessions you started a turn in.
    session_ids: Vec<String>,
    /// "any" (default): return as soon as ONE finishes. "all": wait for all of
    /// them (or the timeout).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mode: Option<String>,
    /// How long to wait, in seconds. Default 120, max 600. A timeout is NOT an
    /// error — the sessions keep running and you can watch again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timeout_s: Option<u64>,
    /// Skip the "is it already idle?" pre-check and park unconditionally.
    /// Used when you know a turn is starting but the lock may not be claimed
    /// yet, and by the tests. Default false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    assume_running: Option<bool>,
}

/// Max sessions one watch call may subscribe to. Each id costs one broadcast
/// receiver for the duration of the park.
const WATCH_MAX_SESSIONS: usize = 32;

/// Whether a session is running, idle, or not knowable from here.
///
/// The third variant is the load-bearing one. Collapsing it into `Idle` — which
/// is what `services.is_some_and(|s| s.is_turn_active(id))` does — makes
/// `workspace_watch` report "already idle" for every session in every headless
/// process, because `workspace_services::get()` is `None` there. That is the
/// one configuration decision 21 exists to keep working (reconciliation #12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionLiveness {
    Running,
    Idle,
    Unknown,
}

/// Resolve liveness from the best source available.
///
/// The handle registry is checked **FIRST and is a veto**, not a fallback the
/// daemon pre-empts:
///
/// 1. the background-subagent handle registry, scoped to the CALLING session
///    (`subagent_handle::list_for_session`, deliberately parent-scoped so one
///    chat can never inspect another's children). It is the same registry
///    `subagent_status { wait: true }` blocked on, read through the child's
///    session id instead of a handle id (`BackgroundSubagent.child_session_id`
///    is public). A handle that `is_running()` means the run exists and has not
///    completed — full stop;
/// 2. otherwise the daemon, when installed — authoritative for every session it
///    knows about;
/// 3. otherwise Unknown.
///
/// **Why the registry outranks the daemon, and not the other way round.**
/// `spawn_background_subagent` registers its handle SYNCHRONOUSLY
/// (`BackgroundSubagent::register`) and only then `tokio::spawn`s the run, whose
/// FIRST await is `SUBAGENT_SEMAPHORE.acquire()` (cap 8 by default,
/// `max_concurrent_subagents()`). Task 33 takes the server turn lease *inside*
/// `run_complete_subagent_task`, i.e. after that permit. So a queued child — one
/// the parent has definitely started and is waiting on — has no `ActiveTurn`,
/// and `AppState::is_turn_active` answers `false` for it. With the daemon
/// consulted first, a parent that fans out 10 background children gets 8 leased
/// and 2 queued, and `workspace_watch` in the default `mode: "any"` returns
/// IMMEDIATELY reporting two children as "already idle" that have not begun.
/// That is F1 relocated from headless into the daemon configuration, which is
/// the normal desktop and `biorouterd` case.
fn session_liveness(
    services: Option<&std::sync::Arc<dyn crate::workspace_services::WorkspaceServices>>,
    caller_session_id: &str,
    session_id: &str,
) -> SessionLiveness {
    for handle in crate::agents::subagent_handle::list_for_session(caller_session_id) {
        if handle.child_session_id == session_id {
            if handle.is_running() {
                // VETO: registered and not yet complete. The daemon may not have
                // a lease for it yet (semaphore queue) — that is not idleness.
                return SessionLiveness::Running;
            }
            return SessionLiveness::Idle;
        }
    }
    if let Some(services) = services {
        return if services.is_turn_active(session_id) {
            SessionLiveness::Running
        } else {
            SessionLiveness::Idle
        };
    }
    SessionLiveness::Unknown
}

/// The tools [`INSTRUCTIONS`] names whose handler is still a placeholder, each
/// with the task that lands it.
///
/// A data table rather than one match arm per tool so the invariant is
/// *checkable*: `get_tools()` must advertise none of these (a tool the model can
/// see but not use is worse than one it cannot see), and every `workspace_*` name
/// in the instruction block must be either advertised or listed here. Both are
/// asserted in `advertises_no_tool_whose_handler_is_still_a_placeholder`.
///
/// The task that implements a tool deletes its row here — it must, or its own
/// dispatch arm is shadowed by nothing and the surface test fails — and adds it
/// to `get_tools()` in the same commit.
const PENDING_TOOLS: &[(&str, &str)] = &[("workspace_open", "Task 24")];

pub struct WorkspaceClient {
    info: InitializeResult,
    pub(crate) context: PlatformExtensionContext,
}

impl WorkspaceClient {
    pub fn new(context: PlatformExtensionContext) -> Result<Self> {
        let info = InitializeResult {
            protocol_version: ProtocolVersion::V_2025_03_26,
            capabilities: ServerCapabilities {
                tasks: None,
                tools: Some(ToolsCapability {
                    list_changed: Some(false),
                }),
                resources: None,
                prompts: None,
                completions: None,
                experimental: None,
                logging: None,
            },
            server_info: Implementation {
                name: EXTENSION_NAME.to_string(),
                title: Some(EXTENSION_TITLE.to_string()),
                version: "1.0.0".to_string(),
                icons: None,
                website_url: None,
            },
            instructions: Some(INSTRUCTIONS.to_string()),
        };
        Ok(Self { info, context })
    }

    fn tool(name: &str, description: &str, schema: serde_json::Value, read_only: bool) -> Tool {
        let input_schema = schema.as_object().expect("schema is an object").clone();
        Tool::new(name.to_string(), description.to_string(), input_schema).annotate(
            ToolAnnotations {
                title: Some(name.replace('_', " ")),
                read_only_hint: Some(read_only),
                destructive_hint: Some(!read_only),
                idempotent_hint: Some(read_only),
                open_world_hint: Some(false),
            },
        )
    }

    fn get_tools() -> Vec<Tool> {
        vec![
            Self::tool(
                "workspace_list",
                "List conversations in the workspace: id, name, type, running \
                 state, parent, enabled extensions, active knowledge base, and \
                 GUI tab placement when a GUI is attached.",
                serde_json::to_value(schema_for!(WorkspaceListParams)).unwrap(),
                true,
            ),
            Self::tool(
                "workspace_read_conversation",
                "Structured read of any conversation. view: transcript (prose), \
                 tool_calls (exactly what its agent did), summary (head/tail), \
                 spawn_context (how a subagent was started). Refuses hidden sessions.",
                serde_json::to_value(schema_for!(WorkspaceReadParams)).unwrap(),
                true,
            ),
            Self::tool(
                "workspace_send_prompt",
                "Inject a prompt into another conversation. mode turn: start its \
                 agent (target idle); steer: redirect mid-turn (target running); \
                 note: append context without a turn. Injections are permanently \
                 provenance-labeled. wait:\"final_message\" returns its answer.",
                serde_json::to_value(schema_for!(WorkspaceSendPromptParams)).unwrap(),
                false,
            ),
            Self::tool(
                "workspace_set_tools",
                "Change what a conversation may use: add/remove extensions, add/remove \
                 skills for that conversation only, switch its provider+model (applies \
                 to its next turn), or set its knowledge bases. Security-relevant \
                 changes always ask the user first, in every permission mode.",
                serde_json::to_value(schema_for!(WorkspaceSetToolsParams)).unwrap(),
                false,
            ),
            Self::tool(
                "workspace_close",
                "Close down a conversation at one of three scopes. tab: close its \
                 GUI tab only — the session and any running turn survive. turn: \
                 cancel the turn it is running (idempotent; not an error when idle). \
                 agent: cancel and evict its agent; the session record is kept.",
                serde_json::to_value(schema_for!(WorkspaceCloseParams)).unwrap(),
                false,
            ),
            Self::tool(
                "workspace_watch",
                "Wait until one (or all) of the named conversations finishes its \
                 current turn, and report why it ended. Use after spawning \
                 background subagents or injecting turns instead of polling. A \
                 timeout is not an error.",
                serde_json::to_value(schema_for!(WorkspaceWatchParams)).unwrap(),
                true,
            ),
            // Tasks 19/24 append:
            // workspace_open and `subagent`
            // (advertised only; the spawn dispatch lives in agent.rs — see
            // Task 19).
        ]
    }

    async fn handle_list(
        &self,
        _caller_session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let args: WorkspaceListParams = match arguments {
            Some(a) => serde_json::from_value(serde_json::Value::Object(a))
                .map_err(|e| format!("invalid arguments: {e}"))?,
            // Derived, not written out: a field-by-field literal here is a
            // compile error every time the params struct grows.
            None => WorkspaceListParams::default(),
        };
        let scope = args.scope.as_deref().unwrap_or("open");
        let include_subagents = args.include_subagents.unwrap_or(true);
        // Decision 17: real paging, not a silent 200-row truncation. The page is
        // applied AFTER scope filtering, so `offset` walks the rows the model
        // actually sees.
        let offset = args.offset.unwrap_or(0) as usize;
        let limit = (args.limit.unwrap_or(50) as usize).clamp(1, 200);

        let services = workspace_services::get();
        let gui_attached = services.as_ref().is_some_and(|s| s.gui_attached());
        let layout = services.as_ref().and_then(|s| s.layout_snapshot());

        // SCAN the store in chunks rather than reading one fixed window.
        //
        // Decision 17 rejected a silent cap, and a single
        // `list_session_summaries(1000, 0, …)` reintroduces one a decimal place
        // higher: `total_matching` and `has_more` would be computed over at most
        // 1000 rows, so `offset >= 1000` returns nothing and the paging metadata
        // — the whole point of the decision — lies on any workspace with more
        // sessions than that. Paging the STORAGE query directly is not
        // equivalent either: scope filtering happens here, so a storage page
        // yields short, ragged tool pages.
        //
        // So: walk the store `SCAN_CHUNK` rows at a time, filter, and stop at
        // `MAX_SCAN_ROWS`. If the ceiling is ever hit the payload says so
        // explicitly (`scan_truncated`) instead of quietly under-reporting.
        const SCAN_CHUNK: u32 = 500;
        const MAX_SCAN_ROWS: usize = 20_000;

        // NOTE for the unit tests: `AgentManager::instance()` resolves
        // `Paths::data_dir()` and the process-global `SessionManager::instance()`,
        // and its first initialization runs `run_first_run_init` (seeds built-in
        // skills, installs the Soul KB + a 3 AM schedule). Run every test that
        // reaches this handler under a sandboxed `BIOROUTER_PATH_ROOT` (or
        // `XDG_CONFIG_HOME`) so it cannot touch the developer's real
        // `~/.config/biorouter`. The same caveat applies to Tasks 14, 15 and 17.
        let agent_manager = crate::execution::manager::AgentManager::instance()
            .await
            .map_err(|e| format!("agent manager unavailable: {e}"))?;

        let mut rows = Vec::new();
        let mut matched = 0usize;
        let mut scanned = 0usize;
        let mut scan_truncated = false;
        let mut db_offset: u32 = 0;
        'scan: loop {
            let summaries = self
                .context
                .session_manager
                // `include_empty: true` (Task 4): the sidebar's INNER JOIN on
                // `messages` hides a session that has none, and `workspace_open`
                // (Task 24) creates exactly that — a session with a working dir
                // and no message yet. `workspace_list` must be able to see it.
                .list_session_summaries(SCAN_CHUNK, db_offset, include_subagents, true)
                .await
                .map_err(|e| format!("failed to list sessions: {e}"))?;
            if summaries.is_empty() {
                break;
            }
            db_offset += summaries.len() as u32;
            for s in summaries {
                scanned += 1;
                if scanned > MAX_SCAN_ROWS {
                    scan_truncated = true;
                    break 'scan;
                }
                let running = services
                    .as_ref()
                    .is_some_and(|svc| svc.is_turn_active(&s.id));
                let live = agent_manager.has_session(&s.id).await;
                let gui_placement = gui_tab_for(layout.as_ref(), &s.id);
                let in_scope = match scope {
                    "running" => running,
                    "all" => true,
                    // "open": a conversation is open if it has a live agent, a
                    // turn in flight, or a GUI tab. `running` is load-bearing
                    // here, not redundant: a glass-box subagent is registered in
                    // `AgentManager`'s PINNED sidecar (Task 33), never in the
                    // `sessions` LRU that `has_session` reads, and a background
                    // child holds no GUI tab — so without it every running
                    // subagent is invisible in the DEFAULT scope, which is the
                    // scope decision 23's migration note tells prompts to use.
                    _ /* "open" */ => live || running || gui_placement.is_some(),
                };
                // Decision 23: these two filters are what `subagent_status`'s
                // list mode becomes. `parent_session_id` answers "my
                // subagents"; `only_subagents` answers "every delegation in the
                // workspace".
                let parent_matches = args
                    .parent_session_id
                    .as_deref()
                    .is_none_or(|want| s.parent_session_id.as_deref() == Some(want));
                let type_matches = !args.only_subagents.unwrap_or(false)
                    || s.session_type.as_deref() == Some("sub_agent");
                if !in_scope || !parent_matches || !type_matches {
                    continue;
                }
                matched += 1;
                if matched <= offset || rows.len() >= limit {
                    // Still counted in `matched` — that is what makes
                    // `total_matching` / `has_more` honest rather than
                    // page-local.
                    continue;
                }
                // §4.1 required row fields: enabled extension names + active
                // KBs. Read per INCLUDED row only (the summary row has no
                // extension_data), exactly the GET /sessions/{id}/extensions
                // fallback logic — `get_session_extensions`, whose two lines are
                // `EnabledExtensionsState::from_extension_data(&session.extension_data)`
                // `.unwrap_or_else(biorouter::config::get_enabled_extensions)`.
                // Best-effort: a read failure yields an empty list, never fails
                // the listing.
                let extensions: Vec<String> =
                    match self.context.session_manager.get_session(&s.id, false).await {
                        Ok(full) => {
                            EnabledExtensionsState::from_extension_data(&full.extension_data)
                                .map(|st| st.extensions.iter().map(|e| e.name()).collect())
                                .unwrap_or_else(|| {
                                    // No session-specific state → global config, the
                                    // exact fallback GET /sessions/{id}/extensions
                                    // performs (`from_extension_data` returns Option).
                                    crate::config::get_enabled_extensions()
                                        .iter()
                                        .map(|e| e.name())
                                        .collect()
                                })
                        }
                        Err(_) => Vec::new(),
                    };
                // Post-#45: ONE call returning set + write target together
                // (Task 9). `primary_kb` is on the row because a model that can
                // SET a write target and cannot READ it back will thrash — it
                // has no way to tell "already correct" from "not applied".
                let kbs = services
                    .as_ref()
                    .map(|svc| svc.knowledge_selection(&s.id))
                    .unwrap_or_default();
                rows.push(json!({
                    "session_id": s.id,
                    "name": s.name,
                    "session_type": s.session_type,
                    "working_dir": s.working_dir,
                    "running": running,
                    "parent_session_id": s.parent_session_id,
                    "extensions": extensions,
                    "knowledge_bases": kbs.kb_ids,
                    // `null` means "no write target chosen", which is a real and
                    // distinct state from "no knowledge bases" — a session can
                    // have several bases and no primary, and a KB-less write then
                    // fails. Do not collapse it to the first id here; the service
                    // owns promotion (`repair_primary_unlocked`).
                    "primary_kb": kbs.primary_kb,
                    "gui": gui_placement,
                }));
            }
        }

        let mut payload = json!({
            "gui_attached": gui_attached,
            "scope": scope,
            // Paging metadata, so the model can walk the list instead of
            // guessing whether it saw everything (decision 17).
            "offset": offset,
            "limit": limit,
            "returned": rows.len(),
            "total_matching": matched,
            "has_more": matched > offset + rows.len(),
            "sessions": rows,
        });
        if scan_truncated {
            // The one case where `total_matching` is a lower bound. Say so in
            // the payload rather than letting the model believe a floor is a
            // total — the failure decision 17 exists to prevent.
            payload["scan_truncated"] = json!(true);
            payload["scanned"] = json!(MAX_SCAN_ROWS);
            payload["note"] = json!(format!(
                "Stopped after scanning {MAX_SCAN_ROWS} conversations; \
                 total_matching is a lower bound. Narrow the query with \
                 scope, parent_session_id or only_subagents."
            ));
        }
        Ok(vec![Content::text(
            serde_json::to_string_pretty(&payload).unwrap(),
        )])
    }

    async fn handle_read_conversation(
        &self,
        caller_session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let args: WorkspaceReadParams = parse_args(arguments)?;
        let view = args.view.as_deref().unwrap_or("transcript");
        let max_chars = args.max_chars.unwrap_or(20_000).min(200_000);

        let session = self
            .context
            .session_manager
            .get_session(&args.session_id, true)
            .await
            .map_err(|e| format!("failed to load session: {e}"))?;

        // §5 "no covert reads": Hidden sessions honor the same visibility rules
        // as the session list. The read itself is auditable — it IS a tool call
        // in the caller's transcript.
        if session.session_type == crate::session::session_manager::SessionType::Hidden {
            return Err("this session is hidden and cannot be read".to_string());
        }
        tracing::info!(
            caller = caller_session_id,
            target = %args.session_id,
            view,
            "workspace cross-session read"
        );

        let messages: Vec<_> = session
            .conversation
            .as_ref()
            .map(|c| c.messages().to_vec())
            .unwrap_or_default();
        // BR-45 range: slice from the named msg_uid (message ids ARE the durable
        // uids — #41 add_message_adopting_uid), then apply `last` as a tail.
        let from_start = match &args.from_msg_uid {
            Some(uid) => messages
                .iter()
                .position(|m| m.id.as_deref() == Some(uid.as_str()))
                .ok_or_else(|| format!("no message with msg_uid '{uid}' in this session"))?,
            None => 0,
        };
        let ranged = &messages[from_start..];
        let tail = |n: Option<usize>| -> &[crate::conversation::message::Message] {
            match n {
                Some(n) if n < ranged.len() => &ranged[ranged.len() - n..],
                _ => ranged,
            }
        };

        let body = match view {
            "tool_calls" => project_tool_calls(tail(args.last)),
            "summary" => project_summary(&session, &messages),
            "spawn_context" => project_spawn_context(&messages)
                .ok_or("this session has no recorded spawn context")?,
            _ => project_transcript(tail(args.last)),
        };

        // Oversized-result handling. The design of record says "oversized results
        // go through the existing session-blob mechanism rather than truncating
        // silently" (§4.1). The binding half of that is **never truncating
        // silently**; the mechanism that actually carries a big result is NOT
        // BR-7's session blob, and saying so here was wrong. The real production
        // path, traced end to end:
        //
        //   1. This is an ordinary extension tool, so `dispatch_tool_call`
        //      returns into `Agent::dispatch_tool_call`, which hands every result
        //      to BR-6 `large_response_handler::process_tool_response`.
        //   2. BR-6 measures the AGGREGATE token count and, above
        //      `DEFAULT_LARGE_RESPONSE_TOKENS` (~25k tokens — reachable here,
        //      since the 200k-char `max_chars` ceiling is roughly 50k tokens of
        //      prose), writes the FULL body to a handle under
        //      `<working_dir>/.biorouter/tool-output/` and replaces the result
        //      with a head/tail preview naming that path. So above that budget
        //      the payload never reaches persistence whole, and BR-7 never sees
        //      it — the session-blob claim was false exactly where it mattered.
        //   3. Below the BR-6 budget the result is persisted intact, and only
        //      there does BR-7 apply: `message_blobs::externalize` moves a tool
        //      response text item over `DEFAULT_BLOB_THRESHOLD_BYTES` (64 KB) to
        //      the blob table, hydrated back byte-for-byte on read (or left as a
        //      stub, readable with `platform__read_session_blob`, under
        //      `BIOROUTER_SESSION_BLOB_LAZY_LOAD`). BR-7's own module doc states
        //      this ordering: its threshold sits "comfortably above anything the
        //      BR-6 handler lets through".
        //
        // Both bands retain the whole payload and both announce the indirection,
        // so §4.1's requirement holds — via a filesystem handle above ~25k tokens
        // and a session blob below it. `read_conversation_oversized_result_is_
        // retained_in_full_on_the_production_path` pins band 2 against the real
        // BR-6 entry point. The tool-level cap below is model-facing pagination
        // layered on top; when it clips it names the narrowing controls rather
        // than dropping data silently, and it must not promise a mechanism.
        let clipped = if body.chars().count() > max_chars {
            let cut: String = body.chars().take(max_chars).collect();
            format!(
                "{cut}\n… [clipped at {max_chars} chars — narrow with `last` or \
                 `from_msg_uid`, or raise `max_chars` (up to 200000). A raised cap \
                 is not silently truncated: a result too large to return inline is \
                 kept in full and the reply says where to read it.]"
            )
        } else {
            body
        };
        Ok(vec![Content::text(format!(
            "Session {} ({}, {:?})\n\n{}",
            session.id, session.name, session.session_type, clipped
        ))])
    }

    /// PER-CALLER-SESSION cap on concurrently injected detached turns (§5
    /// bounded fan-out: "a per-session cap on concurrently injected detached
    /// turns (default 4)"). The counter map below is keyed by the CALLING
    /// session id, so one conversation cannot saturate the daemon's turn locks
    /// while independent conversations keep their own budgets.
    fn injected_turn_cap() -> usize {
        std::env::var("BIOROUTER_WORKSPACE_MAX_INJECTED_TURNS")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|&n: &usize| n > 0)
            .unwrap_or(4)
    }

    /// Keep a fan-out slot reserved for as long as the turn that took it is
    /// actually running, then release it — the half of the cap that cannot be
    /// done on the calling stack.
    ///
    /// The guard is RAII, so "hold it" means "own it somewhere that lives as
    /// long as the turn". That place is this task: it owns the guard and the
    /// turn's own [`TurnFollower`], and returns — dropping the guard — on the
    /// turn's terminal event.
    ///
    /// The `is_turn_active` poll is the *safety valve*, not the mechanism. A
    /// terminal event is guaranteed (`workspace/turn.rs` publishes exactly one
    /// per turn, and `supervise_turn` publishes one even for a panicking task),
    /// but an observer that fell 1024 events behind can be `Lagged` past it, and
    /// a permanently-held slot would refuse this caller's injections for the
    /// life of the process. The server drops the turn lock *after* publishing
    /// the terminal, so a false release is not reachable the other way round.
    fn hold_slot_until_turn_ends(
        guard: InjectedTurnGuard,
        mut follower: TurnFollower,
        session_id: String,
        services: std::sync::Arc<dyn workspace_services::WorkspaceServices>,
    ) {
        tokio::spawn(async move {
            // Named, not `_`: `let _ = guard` would drop it immediately.
            let _guard = guard;
            loop {
                tokio::select! {
                    // `broadcast::Receiver::recv` is cancel-safe, and the
                    // follower's `started` flag lives in the struct rather than
                    // the future, so losing this branch to the timer costs
                    // neither an event nor the correlation.
                    outcome = follower.run() => {
                        if let Err(e) = outcome {
                            tracing::debug!(session_id, error = %e, "workspace: injected-turn watcher lost its stream");
                        }
                        return;
                    }
                    () = tokio::time::sleep(SLOT_RELEASE_POLL) => {
                        if !services.is_turn_active(&session_id) {
                            tracing::debug!(
                                session_id,
                                "workspace: releasing an injected-turn slot whose turn is no longer active"
                            );
                            return;
                        }
                    }
                }
            }
        });
    }

    async fn caller_provenance(
        &self,
        caller_session_id: &str,
    ) -> crate::conversation::message::MessageProvenance {
        use crate::conversation::message::{MessageProvenance, ProvenanceKind};
        let from_session_name = self
            .context
            .session_manager
            .get_session(caller_session_id, false)
            .await
            .ok()
            .map(|s| s.name);
        MessageProvenance {
            kind: ProvenanceKind::AgentInjection,
            from_session_id: Some(caller_session_id.to_string()),
            from_session_name,
        }
    }

    /// §5 autonomous-mode visibility, and decision 2's "toasts": a cross-session
    /// action must never be silent in the GUI. Best-effort — a toast that cannot
    /// be delivered never fails the tool.
    ///
    /// Defined HERE rather than in Task 16, because `workspace_send_prompt` is
    /// the highest-blast-radius consumer (`mode:"steer"` redirects a turn the
    /// user is actively watching). Task 16's `workspace_close` reuses it as-is.
    async fn notify_target(&self, session_id: &str, message: String) {
        if let Some(services) = workspace_services::get() {
            if services.gui_attached() {
                let _ = services
                    .gui_command(
                        json!({
                            "type": "workspace", "cmd": "notify",
                            "session_id": session_id,
                            "level": "info",
                            "message": message,
                        }),
                        false,
                    )
                    .await;
            }
        }
    }

    /// Decision 4, read from the RIGHT place — and read WITHOUT creating an
    /// agent.
    ///
    /// The mode that decides whether the *target's* turn raises confirmations is
    /// the target agent's own `AgentConfig.biorouter_mode`, fixed when that
    /// agent was created (`execution/manager.rs`'s `get_or_create_agent` reads
    /// the global config **once**, at creation). Reading
    /// `Config::global().get_biorouter_mode()` here instead judges the target by
    /// whatever the machine's mode happens to be *now*, and is wrong in both
    /// directions.
    ///
    /// **`get_or_create_agent` cannot be used to ask this question.** It is
    /// create-on-miss, and its miss path is precisely the
    /// `Config::global().get_biorouter_mode()` read this method exists to
    /// avoid — so for any target with no live agent (the normal case for
    /// `workspace_send_prompt` on a conversation the user has not opened this
    /// run) the check would MINT the agent and then read today's global config
    /// off it. Worse, it would leave a bare agent cached under that session id:
    /// no extensions, and no provider at all (`AgentManager::default_provider`
    /// has no production setter, so `Agent::provider()` returns
    /// `Err("Provider not set")`). The turn runner would then pick that agent up.
    async fn target_mode_requires_approval(&self, target_session_id: &str) -> bool {
        let Ok(manager) = crate::execution::manager::AgentManager::instance().await else {
            return true;
        };
        match manager.peek_agent(target_session_id).await {
            Some(agent) => mode_requires_approval(agent.config.biorouter_mode),
            // No live agent: its mode is not yet fixed, so there is nothing to
            // read. Take the conservative branch rather than minting one.
            None => true,
        }
    }

    async fn handle_send_prompt(
        &self,
        caller_session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        use crate::session_events;

        let args: WorkspaceSendPromptParams = parse_args(arguments)?;
        if args.session_id == caller_session_id {
            return Err(
                "refusing to inject into your own session — just continue the conversation".into(),
            );
        }
        if args.text.trim().is_empty() {
            return Err("text must not be empty".into());
        }
        let provenance = self.caller_provenance(caller_session_id).await;
        let services = workspace_services::get();

        match args.mode.as_str() {
            "note" => {
                // NO mid-turn refusal. Reconciliation #16 used to put one here,
                // on the premise that the in-turn compaction sites rewrote the
                // whole history with no freshness check. #51 closed that: both
                // sites now go through
                // `SessionManager::replace_conversation_preserving_tail`, which
                // classifies a message appended above the rewrite's watermark
                // and absent from the turn's `known` conversation as FOREIGN and
                // carries it over rather than deleting it. Refusing now would
                // deny the tool's own recommended headless fallback in the exact
                // case a parent most wants it.
                //
                // What the store CANNOT do for us is the second half. A note that
                // survives the write-back is still summarized away by the next
                // compaction once it falls out of the `keep_last_turns` window —
                // so it is PINNED below. `MessageMetadata.pinned` (#51) exists
                // for this call and says so by name; this is its first consumer.
                //
                // Append without a turn: user_visible + agent_visible (picked up
                // as context on the target's next turn, §4.1), provenance-stamped
                // AND wrapped in an untrusted-data envelope — see
                // `frame_workspace_injection`.
                let body = crate::conversation::message::frame_workspace_injection(
                    provenance.from_session_name.as_deref(),
                    &args.text,
                );
                let mut message = crate::conversation::message::Message::user()
                    .with_text(body)
                    .with_provenance(provenance)
                    // ⚠ Do not drop this. Without it the tool reports success and
                    // the note evaporates a few turns later — the same broken
                    // promise, arriving more slowly. The pin is honoured only on
                    // a message with no tool request/response content and only
                    // while it is agent-visible
                    // (`context_mgmt::pins::pin_is_eligible`); a framed text note
                    // satisfies both, and the tests prove it rather than assuming
                    // it.
                    .pinned();
                self.context
                    .session_manager
                    .add_message_adopting_uid(&args.session_id, &mut message)
                    .await
                    .map_err(|e| format!("failed to append note: {e}"))?;
                Ok(vec![Content::text(format!(
                    "Note appended to session {} (no turn started; preserved across \
                     compaction).",
                    args.session_id
                ))])
            }
            "steer" => {
                let services = services.ok_or(
                    "steer requires the BioRouter daemon (no workspace services installed)",
                )?;
                if !services.is_turn_active(&args.session_id) {
                    return Err(
                        "target session has no turn in flight — use mode:\"turn\" instead".into(),
                    );
                }
                let agent_manager = crate::execution::manager::AgentManager::instance()
                    .await
                    .map_err(|e| e.to_string())?;
                // Returns the LIVE agent whose loop drains the queue: /reply-
                // driven sessions are registered by the server's get_agent, and
                // glass-box subagent runs register themselves (Task 33) — the
                // steer lands on the running instance in both cases.
                let agent = agent_manager
                    .get_or_create_agent(args.session_id.clone())
                    .await
                    .map_err(|e| e.to_string())?;
                // The drain loop frames agent-provenance steers (Task 3); the
                // raw text is queued so the human's own soft interrupt, which
                // carries no provenance, stays unframed.
                //
                // GUARDED (#69): the unconditional `queue_soft_interrupt_with_
                // provenance` returns `()`, so it would report "queued" for a
                // turn that has already closed its queue and will never consume
                // it. `is_turn_active` above is the server's lock, which is
                // released *after* the loop stops accepting — so the two can
                // disagree, and only `try_queue_soft_interrupt` observes the
                // queue's own state atomically.
                let turn = agent
                    .try_queue_soft_interrupt(args.text, Some(provenance.clone()))
                    .map_err(|refused| {
                        format!(
                            "steer refused for session {}: {refused} — use mode:\"turn\" instead",
                            args.session_id
                        )
                    })?;
                // §5 / decision 2: a cross-session mutation is never silent in
                // the GUI. Redirecting a turn the user is watching is the most
                // intrusive thing this tool does, so it gets the same toast
                // `workspace_close` and `workspace_set_tools` post.
                self.notify_target(
                    &args.session_id,
                    format!(
                        "Another agent ({}) steered this turn.",
                        provenance
                            .from_session_name
                            .as_deref()
                            .unwrap_or(caller_session_id)
                    ),
                )
                .await;
                Ok(vec![Content::text(format!(
                    "Steer queued for session {}'s running turn ({turn}).",
                    args.session_id
                ))])
            }
            "turn" => {
                let services = services.ok_or(
                    "mode:\"turn\" requires the BioRouter daemon (no workspace services installed); \
                     use mode:\"note\" to leave context headlessly",
                )?;
                // Decision 4: never park an approval prompt where nobody can see
                // it. In manual/smart-approval modes a detached turn's tool
                // confirmations arrive as ToolConfirmationRequest messages that
                // only a GUI (or an observer) can answer; with no GUI attached
                // the turn would sit until its timeout with no one watching.
                // Refuse clearly instead — the caller can use mode:"note", or
                // the user can open the app.
                if !services.gui_attached()
                    && self.target_mode_requires_approval(&args.session_id).await
                {
                    return Err(format!(
                        "refusing to start a turn in session {}: this machine is in an \
                         approval permission mode and no desktop window is attached, so any \
                         tool confirmation the turn raises would wait unseen until it timed \
                         out. Use mode:\"note\" to leave the text as context, or ask the user \
                         to open the Biorouter app.",
                        args.session_id
                    ));
                }
                // Bounded fan-out, PER CALLING SESSION (§5): subscribe before
                // starting so completion is never missed, and count this
                // caller's own in-flight injections.
                //
                // ⚠ The guard is NOT `_`-bound. A slot is occupied by a turn that
                // is *in flight*, and a `wait:"none"` call returns while its turn
                // runs on — so binding the guard to the call's stack frame would
                // release every fire-and-forget injection's slot the instant the
                // tool answered, and the cap would bound nothing at all (five,
                // fifty, five hundred detached turns all accepted under a cap of
                // four). It is moved into the reservation task below instead, and
                // released on the turn's own terminal event.
                let (inflight, cap_guard) = InjectedTurnGuard::enter(caller_session_id);
                if inflight > Self::injected_turn_cap() {
                    return Err(format!(
                        "this session already has {} injected turns in flight (cap {}); \
                         wait for one to finish",
                        inflight - 1,
                        Self::injected_turn_cap()
                    ));
                }

                let rx = session_events::subscribe(&args.session_id);
                let body = crate::conversation::message::frame_workspace_injection(
                    provenance.from_session_name.as_deref(),
                    &args.text,
                );
                let message = crate::conversation::message::Message::user()
                    .with_text(body)
                    .with_provenance(provenance.clone());
                let turn_id = services
                    .start_detached_turn(&args.session_id, message)
                    .await
                    .map_err(|e| format!("could not start turn: {e}"))?;
                // §5 / decision 2: GUI-visible, always.
                self.notify_target(
                    &args.session_id,
                    format!(
                        "Another agent ({}) started a turn here.",
                        provenance
                            .from_session_name
                            .as_deref()
                            .unwrap_or(caller_session_id)
                    ),
                )
                .await;

                // ⚠ Everything the follower reads is scoped to THIS turn id. The
                // subscription opened above is older than the turn: the real
                // service hydrates the target's provider and extensions between
                // the subscribe and `start_turn`
                // (`biorouter-server/src/workspace/services.rs`), and a turn that
                // was already running when the caller asked can finish inside
                // that window — publishing its assistant text and its
                // `TurnFinished` onto the very stream we are about to read. A
                // loop that accepts the first terminal it sees therefore reports
                // the PREVIOUS turn's answer as this one's.
                let mut follower = TurnFollower::new(rx, turn_id.clone());

                if args.wait.as_deref() != Some("final_message") {
                    // Fire-and-forget, but not accounting-free: the reservation
                    // outlives this call (see `InjectedTurnGuard::enter` above).
                    Self::hold_slot_until_turn_ends(
                        cap_guard,
                        follower,
                        args.session_id.clone(),
                        std::sync::Arc::clone(&services),
                    );
                    return Ok(vec![Content::text(format!(
                        "Detached turn {turn_id} started on session {}.",
                        args.session_id
                    ))]);
                }

                // ui_ask-style bounded park (§4.1): watch the bus for the final
                // assistant message, bounded by timeout_s.
                let timeout =
                    std::time::Duration::from_secs(args.timeout_s.unwrap_or(120).min(600));
                let waited = tokio::time::timeout(timeout, follower.run()).await;

                match waited {
                    Ok(Ok(TurnOutcome::Finished {
                        reason,
                        last_assistant,
                    })) => Ok(vec![Content::text(format!(
                        "Turn {turn_id} finished ({reason}). Final message:\n\n{}",
                        last_assistant.unwrap_or_else(|| "<no assistant text>".into())
                    ))]),
                    Ok(Ok(TurnOutcome::Failed(e))) => {
                        Err(format!("turn {turn_id} ended in error: {e}"))
                    }
                    Ok(Err(e)) => Err(format!("event stream error while waiting: {e}")),
                    Err(_) => {
                        // The park gave up; the TURN did not. It is still in
                        // flight and still counts against the cap, so the
                        // reservation is handed to the same background follower
                        // the no-wait path uses — with its `started` state
                        // intact, so it is still watching for THIS turn's
                        // terminal and not the next one's.
                        Self::hold_slot_until_turn_ends(
                            cap_guard,
                            follower,
                            args.session_id.clone(),
                            std::sync::Arc::clone(&services),
                        );
                        Ok(vec![Content::text(format!(
                            "Turn {turn_id} is still running after {}s; it continues in the background. \
                             Read it later with workspace_read_conversation.",
                            timeout.as_secs()
                        ))])
                    }
                }
            }
            other => Err(format!("unknown mode '{other}' (turn | steer | note)")),
        }
    }

    /// BR-71 `workspace_set_tools`: the one place an agent changes *what another
    /// conversation can use* — extensions, session-scoped skills,
    /// provider+model, and knowledge bases.
    async fn handle_set_tools(
        &self,
        caller_session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let args: WorkspaceSetToolsParams = parse_args(arguments)?;

        // ---- Resolve EVERYTHING before mutating anything, so a bad name is a
        // clean no-op rather than a half-applied change. ------------------
        //
        // Resolve through `get_extension_entry_by_name`, NOT
        // `get_extension_by_name`. The latter is `…entry_by_name(name).map(|e|
        // e.config)` (`config/extensions.rs:138-140`) — it DISCARDS the
        // operator's `enabled` flag. Issue #42's gate lives one layer up, in
        // `manage_extensions`' enable path (`check_enable_allowed`,
        // `extension_manager_extension.rs:97-125`), and `Agent::add_extension`
        // does not re-check it. So resolving with the flag-less helper would
        // make `workspace_set_tools` a SECOND, ungated way to enable an
        // extension an operator deliberately wrote `enabled: false` for —
        // including on the caller's own session. That is the pinned
        // tool-environment case (benchmarking, safety) the #42 doc comment
        // names, and defeating it is a straight privilege escalation.
        let mut add_configs = Vec::new();
        for name in &args.add_extensions {
            match crate::config::get_extension_entry_by_name(name) {
                None => return Err(format!("unknown extension '{name}'")),
                Some(entry)
                    if !entry.enabled
                        && crate::config::extension_entry_is_persisted(&entry.config.name()) =>
                {
                    // Same refusal text `manage_extensions` gives, so the model
                    // gets the same guidance whichever door it tried.
                    return Err(format!(
                        "Extension '{name}' is disabled in the Biorouter configuration \
                         (enabled: false). The operator turned it off deliberately, so do not \
                         enable it yourself — not here and not on another conversation. If it \
                         is needed for this task, ask the user to re-enable it."
                    ));
                }
                Some(entry) => add_configs.push(entry.config),
            }
        }

        // §5: workspace control must not fan out through delegation trees.
        //
        // Matched on the RESOLVED config name, normalized — the registry key an
        // extension is actually loaded under (`extension_manager::normalize`).
        // A literal `n == "workspace"` on the raw request would sail past
        // `"Workspace"`, which is this extension's own configured name
        // ([`EXTENSION_NAME`]) and therefore the spelling a model is most likely
        // to send.
        let grants_workspace = add_configs.iter().any(|config| {
            crate::agents::extension_manager::normalize(&config.name())
                == crate::agents::extension_manager::normalize(EXTENSION_NAME)
        });
        if grants_workspace {
            let target = self
                .context
                .session_manager
                .get_session(&args.session_id, false)
                .await
                .map_err(|e| e.to_string())?;
            if target.session_type == crate::session::session_manager::SessionType::SubAgent {
                return Err(
                    "subagent sessions can never be granted the workspace extension".into(),
                );
            }
        }

        // Model/provider (decision b): resolve and validate here; apply below.
        let new_provider = match (&args.provider, &args.model) {
            (None, None) => None,
            (None, Some(_)) => {
                return Err(
                    "`model` requires `provider` — a model name is ambiguous across providers; \
                     pass both (e.g. provider:\"anthropic\", model:\"claude-opus-5\")"
                        .into(),
                );
            }
            (Some(provider_name), model) => {
                // The provider registry is the same one /agent/update_provider
                // resolves through. NOTE the real signature: `pub async fn
                // providers() -> Vec<(ProviderMetadata, ProviderType)>`
                // (`providers/factory.rs:109`, re-exported at
                // `providers/mod.rs:47`). It must be AWAITED, and its items are
                // 2-tuples — `.find(|m| m.name == …)` on the raw item does not
                // compile. One `await`, destructured once and reused for the
                // error message, so the registry is not read twice.
                let registry = crate::providers::providers().await;
                let metadata = registry
                    .iter()
                    .map(|(metadata, _kind)| metadata)
                    .find(|m| m.name == *provider_name)
                    .ok_or_else(|| {
                        format!(
                            "unknown provider '{provider_name}' (known: {})",
                            registry
                                .iter()
                                .map(|(m, _)| m.name.clone())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    })?
                    .clone();
                let model_name = model
                    .clone()
                    .unwrap_or_else(|| metadata.default_model.clone());
                let known: Vec<String> = metadata
                    .known_models
                    .iter()
                    .map(|m| m.name.clone())
                    .collect();
                if !model_is_known(&model_name, &known, metadata.allows_unlisted_models) {
                    return Err(format!(
                        "'{model_name}' is not a known model for provider '{provider_name}' \
                         (known: {})",
                        known.join(", ")
                    ));
                }
                let model_config = crate::model::ModelConfig::new(&model_name)
                    .map_err(|e| format!("invalid model config: {e}"))?;
                Some((
                    provider_name.clone(),
                    model_name,
                    crate::providers::create(provider_name, model_config)
                        .await
                        .map_err(|e| format!("failed to create {provider_name} provider: {e}"))?,
                ))
            }
        };

        // ---- Apply. --------------------------------------------------------
        let mut applied = Vec::new();

        // The live agent is fetched ONLY for the dimensions that need one.
        // `get_or_create_agent` is create-on-miss, and its miss path mints a
        // bare, provider-less agent and caches it under the target's id, where
        // the turn runner will pick it up — the hazard
        // `target_mode_requires_approval` documents at length. A skills-only or
        // KB-only call must not pay that price for a target the user has not
        // opened.
        let needs_agent =
            !add_configs.is_empty() || !args.remove_extensions.is_empty() || new_provider.is_some();
        let agent = if needs_agent {
            let agent_manager = crate::execution::manager::AgentManager::instance()
                .await
                .map_err(|e| e.to_string())?;
            Some(
                agent_manager
                    .get_or_create_agent(args.session_id.clone())
                    .await
                    .map_err(|e| e.to_string())?,
            )
        } else {
            None
        };

        // The exact /agent/add_extension handler path (routes/agent.rs:744-767):
        // add on the live agent, persist only after a successful load.
        if let Some(agent) = &agent {
            let mut extensions_changed = false;
            for config in add_configs {
                let name = config.name().to_string();
                agent
                    .add_extension(config)
                    .await
                    .map_err(|e| format!("failed to add '{name}': {e}"))?;
                applied.push(format!("+{name}"));
                extensions_changed = true;
            }
            for name in &args.remove_extensions {
                agent
                    .remove_extension(name)
                    .await
                    .map_err(|e| format!("failed to remove '{name}': {e}"))?;
                applied.push(format!("-{name}"));
                extensions_changed = true;
            }
            if extensions_changed {
                agent
                    .persist_extension_state(&args.session_id)
                    .await
                    .map_err(|e| format!("failed to persist extension state: {e}"))?;
            }
        }

        // Skills — SESSION-SCOPED (Task 11). Never the machine-wide file.
        if !args.add_skills.is_empty() || !args.remove_skills.is_empty() {
            crate::agents::session_skills::apply(
                &self.context.session_manager,
                &args.session_id,
                &args.add_skills,
                &args.remove_skills,
            )
            .await
            .map_err(|e| format!("failed to scope skills: {e}"))?;
            for name in &args.add_skills {
                applied.push(format!("+skill:{name}"));
            }
            for name in &args.remove_skills {
                applied.push(format!("-skill:{name}"));
            }
        }

        // Model/provider — mirrors /agent/update_provider, which also persists
        // provider_name + model_config onto the session row.
        if let (Some(agent), Some((provider_name, model_name, provider))) = (&agent, new_provider) {
            agent
                .update_provider(provider, &args.session_id)
                .await
                .map_err(|e| format!("failed to switch provider: {e}"))?;
            applied.push(format!("model={provider_name}/{model_name}"));
        }

        // Knowledge bases (plural — issue #45), with their write target.
        if let Some(kbs) = args.set_knowledge_bases {
            use crate::workspace_services::KbPrimaryChoice;
            let services = workspace_services::get()
                .ok_or("knowledge-base scoping requires the BioRouter daemon")?;
            // Three-valued, because the underlying model is: absent → Auto
            // (keep-if-member, else first, else clear); `""` → an explicit
            // "no write target here"; a name → pin it. Membership is validated
            // by the service against the RESULTING set, so a name outside `kbs`
            // comes back as a clear error rather than a half-applied write.
            let primary = match args.primary_knowledge_base.as_deref() {
                None => KbPrimaryChoice::Auto,
                Some("") => KbPrimaryChoice::Clear,
                Some(id) => KbPrimaryChoice::Set(id.to_string()),
            };
            let selection = services.set_knowledge_bases(&args.session_id, &kbs, primary)?;
            applied.push(if selection.kb_ids.is_empty() {
                "kb=<cleared>".to_string()
            } else {
                // Report the RESULT, not the request: the service may have moved
                // the write target itself, and a tool result that echoes the
                // request teaches the model a state the store does not hold.
                format!(
                    "kb={} (primary={})",
                    selection.kb_ids.join("+"),
                    selection.primary_kb.as_deref().unwrap_or("<none>")
                )
            });
        }

        if applied.is_empty() {
            return Ok(vec![Content::text(format!(
                "No changes requested for session {}.",
                args.session_id
            ))]);
        }

        // §5 autonomous-mode visibility: every change surfaces on the target tab.
        // (The always-confirm inspector, Task 10, has already run for the
        // security-relevant subset — this toast is what covers the rest.)
        self.notify_target(
            &args.session_id,
            format!(
                "Tools changed by another agent ({caller_session_id}): {}",
                applied.join(", ")
            ),
        )
        .await;

        let next_turn_note = if applied.iter().any(|a| a.starts_with("model=")) {
            " The model change applies to this conversation's NEXT turn."
        } else {
            ""
        };
        Ok(vec![Content::text(format!(
            "Applied to session {}: {}.{next_turn_note}",
            args.session_id,
            applied.join(", ")
        ))])
    }

    /// `workspace_close` — the three scopes of "stop", smallest blast radius
    /// first (§4.1).
    ///
    /// The §5 autonomous-mode toasts go through [`Self::notify_target`], which
    /// Task 14 already defined for `workspace_send_prompt`'s `turn`/`steer`
    /// announcements — there is deliberately only one copy.
    async fn handle_close(
        &self,
        caller_session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let args: WorkspaceCloseParams = parse_args(arguments)?;
        let services = workspace_services::get();

        match args.scope.as_str() {
            "tab" => match services {
                Some(s) if s.gui_attached() => {
                    s.gui_command(
                        json!({ "type": "workspace", "cmd": "close_tab", "session_id": args.session_id }),
                        false,
                    )
                    .await?;
                    Ok(vec![Content::text(format!(
                        "Tab for session {} closed (session survives).",
                        args.session_id
                    ))])
                }
                _ => Ok(vec![Content::text(
                    "No GUI attached — nothing to close at tab scope (gui_attached: false)."
                        .to_string(),
                )]),
            },
            "turn" => {
                let services = services.ok_or("scope:\"turn\" requires the BioRouter daemon")?;
                match services.cancel_turn(&args.session_id) {
                    Some(turn_id) => {
                        self.notify_target(
                            &args.session_id,
                            format!("Turn cancelled by another agent ({caller_session_id})."),
                        )
                        .await;
                        Ok(vec![Content::text(format!(
                            "Cancelled turn {turn_id} on session {}.",
                            args.session_id
                        ))])
                    }
                    None => Ok(vec![Content::text(format!(
                        "Session {} had no turn in flight (nothing to cancel).",
                        args.session_id
                    ))]),
                }
            }
            "agent" => {
                let services = services.ok_or("scope:\"agent\" requires the BioRouter daemon")?;
                services.stop_agent(&args.session_id).await?;
                self.notify_target(
                    &args.session_id,
                    format!("Agent stopped by another agent ({caller_session_id})."),
                )
                .await;
                Ok(vec![Content::text(format!(
                    "Agent for session {} stopped and evicted (session record kept).",
                    args.session_id
                ))])
            }
            other => Err(format!("unknown scope '{other}' (tab | turn | agent)")),
        }
    }

    async fn handle_watch(
        &self,
        caller_session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        use crate::session_events::{self, SessionBusEvent};

        let args: WorkspaceWatchParams = parse_args(arguments)?;
        if args.session_ids.is_empty() {
            return Err("session_ids must name at least one conversation".into());
        }
        if args.session_ids.len() > WATCH_MAX_SESSIONS {
            return Err(format!(
                "watching {} conversations at once exceeds the cap of {WATCH_MAX_SESSIONS}",
                args.session_ids.len()
            ));
        }
        let wait_all = match args.mode.as_deref() {
            None | Some("any") => false,
            Some("all") => true,
            Some(other) => return Err(format!("unknown mode '{other}' (any | all)")),
        };
        let timeout = std::time::Duration::from_secs(args.timeout_s.unwrap_or(120).clamp(1, 600));
        let assume_running = args.assume_running.unwrap_or(false);

        // Subscribe FIRST, then pre-check. Reversing this loses a completion
        // that lands between the check and the subscribe.
        //
        // `session_events::subscribe` hands back a `Subscription`, not a bare
        // `broadcast::Receiver`: the receiver is deliberately not exposed,
        // because only the wrapper's `Drop` reclaims the session's 1024-slot
        // ring. Holding `Subscription` here is what keeps a watch on an idle or
        // made-up id from pinning that ring for the life of the process.
        let mut receivers: Vec<(String, session_events::Subscription)> = args
            .session_ids
            .iter()
            .map(|id| (id.clone(), session_events::subscribe(id)))
            .collect();

        let services = workspace_services::get();
        let mut completed: Vec<(String, String)> = Vec::new();
        // How many watched ids we could not resolve at all — reported at the end
        // so a headless timeout does not read as "they are all still working".
        let mut unknown_liveness = 0usize;
        if !assume_running {
            for (id, _) in &receivers {
                match session_liveness(services.as_ref(), caller_session_id, id) {
                    // Only a POSITIVE idle answer short-circuits. `Unknown`
                    // parks — see `SessionLiveness`.
                    SessionLiveness::Idle => {
                        completed.push((id.clone(), "already idle".to_string()));
                    }
                    SessionLiveness::Running => {}
                    SessionLiveness::Unknown => unknown_liveness += 1,
                }
            }
            receivers.retain(|(id, _)| !completed.iter().any(|(done, _)| done == id));
        }

        let done_now = if wait_all {
            receivers.is_empty()
        } else {
            !completed.is_empty()
        };
        if !done_now && !receivers.is_empty() {
            let deadline = tokio::time::Instant::now() + timeout;
            // One task per watched session, all feeding one channel: simpler
            // and more obviously correct than a hand-rolled select over a Vec,
            // and 32 short-lived tasks is nothing.
            let (tx, mut rx) = tokio::sync::mpsc::channel::<(String, String)>(WATCH_MAX_SESSIONS);
            for (id, mut receiver) in receivers.drain(..) {
                let tx = tx.clone();
                tokio::spawn(async move {
                    loop {
                        match receiver.recv().await {
                            Ok(SessionBusEvent::TurnFinished { reason, .. }) => {
                                let _ = tx.send((id, reason)).await;
                                return;
                            }
                            Ok(SessionBusEvent::TurnError { message, .. }) => {
                                let _ = tx.send((id, format!("error: {message}"))).await;
                                return;
                            }
                            Ok(_) => {}
                            // A lagged watcher has certainly not missed the
                            // *last* event yet; keep listening.
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                        }
                    }
                });
            }
            drop(tx); // so `rx.recv()` ends if every watcher exits

            // `want` counts entries in `completed`, which already holds the
            // sessions the pre-check found idle — so "all" is the full id list
            // and "any" is one more than we already have.
            let want = if wait_all {
                args.session_ids.len()
            } else {
                completed.len() + 1
            };
            let _ = tokio::time::timeout_at(deadline, async {
                while completed.len() < want {
                    match rx.recv().await {
                        Some(entry) => completed.push(entry),
                        None => break,
                    }
                }
            })
            .await;
        }

        let still_running: Vec<&String> = args
            .session_ids
            .iter()
            .filter(|id| !completed.iter().any(|(done, _)| done == *id))
            .collect();

        let mut report = String::new();
        if completed.is_empty() {
            report.push_str(&format!(
                "No conversation finished within {}s. Still running: {}. \
                 They keep running — watch again or read them later.\n",
                timeout.as_secs(),
                still_running
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            if unknown_liveness > 0 {
                // Honest about the headless case rather than implying we
                // observed them working.
                report.push_str(
                    "(No BioRouter daemon is attached, so whether they had started \
                     could not be checked — some of these may never have been \
                     running.)\n",
                );
            }
        } else {
            report.push_str("Completed:\n");
            for (id, reason) in &completed {
                report.push_str(&format!("- {id} ({reason})\n"));
            }
            if !still_running.is_empty() {
                report.push_str(&format!(
                    "Still running: {}\n",
                    still_running
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            report.push_str(
                "\nRead a completed conversation with workspace_read_conversation \
                 (view:\"summary\" for its outcome, view:\"tool_calls\" for what it did).",
            );
        }
        Ok(vec![Content::text(report)])
    }
}

/// True in every permission mode that can ACTUALLY raise a tool confirmation.
/// Free and pure, so the decision is testable without writing global config.
///
/// Exhaustive `match`, not `matches!(…, Auto)`: a fifth mode must be classified
/// deliberately rather than inherit a default.
fn mode_requires_approval(mode: crate::config::BioRouterMode) -> bool {
    use crate::config::BioRouterMode;
    match mode {
        // Fully Automatic never prompts.
        BioRouterMode::Auto => false,
        // Chat CANNOT prompt. `PermissionInspector::inspect` returns
        // `Ok(vec![])` before inspecting anything in Chat mode, the agent loop
        // skips every remaining tool call and splices
        // `CHAT_MODE_TOOL_SKIPPED_RESPONSE`, and the tool list is stripped from
        // the prompt entirely. There is no confirmation that could park unseen,
        // so decision 4's refusal — whose message claims one would — must not
        // fire here. Decision 4's trigger is "manual mode"; Chat is not one.
        // Classifying it as an approval mode would refuse every headless
        // `mode:"turn"` on a Chat-mode machine, which is the SAFEST
        // configuration, with a message that is false for it.
        BioRouterMode::Chat => false,
        BioRouterMode::Approve | BioRouterMode::SmartApprove => true,
    }
}

/// How often a background slot-holder re-checks whether its turn is still
/// running, for the case where the terminal event was missed. See
/// [`WorkspaceClient::hold_slot_until_turn_ends`].
const SLOT_RELEASE_POLL: std::time::Duration = std::time::Duration::from_secs(2);

/// How one injected turn ended, as [`TurnFollower`] observed it.
#[derive(Debug)]
enum TurnOutcome {
    /// `TurnFinished`, plus the last non-empty assistant text of THAT turn.
    Finished {
        reason: String,
        last_assistant: Option<String>,
    },
    /// `TurnError` — terminal too. A turn publishes exactly one of the two.
    Failed(String),
}

/// Follows ONE detached turn on the session bus, from its own
/// `TurnStarted { turn_id }` to its terminal event.
///
/// **The `turn_id` gate is the whole point.** The subscription is deliberately
/// opened before the turn is started (a ring is only created by `subscribe`, so
/// subscribing after would drop `TurnStarted` and, on a fast turn, the terminal
/// too). That makes the stream necessarily *older* than the turn: the daemon
/// hydrates the target's provider and extensions before it acquires the turn
/// lock, and a turn already in flight when the caller asked can publish its
/// answer and its `TurnFinished` inside that window. Accepting the first
/// terminal seen therefore attributes the previous turn's final message to this
/// one, with no error and nothing in the text to give it away. Everything before
/// this turn's own start belongs to somebody else and is discarded.
///
/// The state lives in the struct, not in the future returned by [`Self::run`],
/// so a park that times out can hand the follower to a background task mid-turn
/// without forgetting that the start was already seen.
struct TurnFollower {
    events: crate::session_events::Subscription,
    turn_id: String,
    started: bool,
    last_assistant: Option<String>,
}

impl TurnFollower {
    fn new(events: crate::session_events::Subscription, turn_id: String) -> Self {
        Self {
            events,
            turn_id,
            started: false,
            last_assistant: None,
        }
    }

    /// Fold one bus event in. `Some` when the turn reached its terminal.
    fn step(&mut self, event: crate::session_events::SessionBusEvent) -> Option<TurnOutcome> {
        use crate::session_events::SessionBusEvent;
        if !self.started {
            if let SessionBusEvent::TurnStarted { turn_id } = &event {
                if *turn_id == self.turn_id {
                    self.started = true;
                }
            }
            return None;
        }
        match event {
            SessionBusEvent::Agent(crate::agents::AgentEvent::Message(m))
                if m.role == rmcp::model::Role::Assistant =>
            {
                let text: String = m
                    .content
                    .iter()
                    .filter_map(|c| c.as_text())
                    .collect::<Vec<_>>()
                    .join("\n");
                if !text.trim().is_empty() {
                    self.last_assistant = Some(text);
                }
                None
            }
            // `..` because `TurnFinished` also carries `token_state` (Task 5) —
            // a two-field pattern here is a missing-field compile error.
            SessionBusEvent::TurnFinished { reason, .. } => Some(TurnOutcome::Finished {
                reason,
                last_assistant: self.last_assistant.take(),
            }),
            // A turn publishes "exactly one `TurnError` or one `TurnFinished`,
            // never both" (`workspace/turn.rs`), so an error is TERMINAL:
            // without this arm a park would sit out its whole timeout after the
            // turn had already died, and then report "still running".
            SessionBusEvent::TurnError { message, .. } => Some(TurnOutcome::Failed(message)),
            _ => None,
        }
    }

    /// Read until the turn ends. `Err` only when the stream itself is gone.
    ///
    /// Cancel-safe: the only await is `recv`, which is, and no state is held in
    /// the future.
    async fn run(&mut self) -> Result<TurnOutcome, String> {
        loop {
            match self.events.recv().await {
                Ok(event) => {
                    if let Some(outcome) = self.step(event) {
                        return Ok(outcome);
                    }
                }
                // Falling behind loses events, not the stream. The one event
                // whose loss matters is this turn's `TurnStarted`, and the
                // caller bounds that: a park times out, and a background
                // slot-holder polls `is_turn_active`.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(e) => return Err(e.to_string()),
            }
        }
    }
}

/// §5 bounded fan-out: PER-SESSION counts of injected detached turns, keyed by
/// the CALLING session id (the design's "per-session cap", default 4). RAII:
/// the guard decrements its own key on drop and removes empty entries so the
/// map never grows unboundedly.
static INJECTED_TURNS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, usize>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

struct InjectedTurnGuard {
    caller: String,
}

impl InjectedTurnGuard {
    /// Increment the caller's count; returns (new count, guard).
    fn enter(caller_session_id: &str) -> (usize, Self) {
        let mut map = INJECTED_TURNS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let count = map.entry(caller_session_id.to_string()).or_insert(0);
        *count += 1;
        (
            *count,
            Self {
                caller: caller_session_id.to_string(),
            },
        )
    }
}

impl Drop for InjectedTurnGuard {
    fn drop(&mut self) {
        let mut map = INJECTED_TURNS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(count) = map.get_mut(&self.caller) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                map.remove(&self.caller);
            }
        }
    }
}

/// Is this model acceptable for this provider?
///
/// Three ways to be yes, in decreasing order of certainty:
/// 1. it is in the provider's published `known_models` catalog;
/// 2. the provider publishes no catalog at all (nothing to check against — let
///    the provider reject it at request time with a better message than we
///    could synthesize);
/// 3. the provider **declares** that it takes unlisted model names
///    (`ProviderMetadata.allows_unlisted_models`, builder
///    `ProviderMetadata::with_unlisted_models()`). ollama, llamacpp,
///    gcpvertexai and every custom/declarative provider set it, and the field
///    exists for exactly this question — the GUI's model picker reads it to
///    decide whether to offer a free-text box. A locally pulled
///    `ollama`/`qwen3.6:latest` is not in any published catalog and must not be
///    refused here when the app's own picker accepts it.
fn model_is_known(model: &str, known_models: &[String], allows_unlisted: bool) -> bool {
    allows_unlisted || known_models.is_empty() || known_models.iter().any(|m| m == model)
}

fn parse_args<T: serde::de::DeserializeOwned>(arguments: Option<JsonObject>) -> Result<T, String> {
    let args = arguments.ok_or("Missing arguments")?;
    serde_json::from_value(serde_json::Value::Object(args))
        .map_err(|e| format!("invalid arguments: {e}"))
}

/// The `tool_calls` projection (§4.1): ToolRequest/ToolResponse pairs only,
/// correlated by their shared id — "what did that agent actually do".
fn project_tool_calls(messages: &[crate::conversation::message::Message]) -> String {
    use crate::conversation::message::MessageContent;
    let mut out = String::new();
    for message in messages {
        for content in &message.content {
            match content {
                MessageContent::ToolRequest(req) => {
                    out.push_str(&format!("→ [{}] {}\n", req.id, req.to_readable_string()));
                }
                MessageContent::ToolResponse(resp) => {
                    let digest = match &resp.tool_result {
                        Ok(result) => {
                            let text: String = result
                                .content
                                .iter()
                                .filter_map(|c| c.as_text().map(|t| t.text.clone()))
                                .collect::<Vec<_>>()
                                .join(" ");
                            let short: String = text.chars().take(400).collect();
                            format!("ok: {short}")
                        }
                        Err(e) => format!("error: {e}"),
                    };
                    out.push_str(&format!("← [{}] {digest}\n", resp.id));
                }
                _ => {}
            }
        }
    }
    if out.is_empty() {
        "No tool calls in range.".to_string()
    } else {
        out
    }
}

fn project_transcript(messages: &[crate::conversation::message::Message]) -> String {
    use crate::conversation::message::MessageContent;
    let mut out = String::new();
    for message in messages {
        // Tab-invisible bookkeeping rows (agent_only) are skipped; tool
        // payloads collapse to one-line stubs (§4.1).
        if !message.metadata.user_visible {
            continue;
        }
        out.push_str(&format!("[{:?}] ", message.role));
        if let Some(p) = &message.metadata.provenance {
            out.push_str(&format!("(injected: {:?}) ", p.kind));
        }
        for content in &message.content {
            match content {
                MessageContent::ToolRequest(req) => {
                    out.push_str(&format!("<tool call: {}>", req.to_readable_string()));
                }
                MessageContent::ToolResponse(resp) => {
                    out.push_str(&format!("<tool result: {}>", resp.id));
                }
                other => {
                    if let Some(text) = other.as_text() {
                        out.push_str(text);
                    }
                }
            }
        }
        out.push('\n');
    }
    out
}

fn project_summary(
    session: &crate::session::Session,
    messages: &[crate::conversation::message::Message],
) -> String {
    // chatrecall-load parity (§3.2): head 3 + tail 3, via the same data.
    let head = project_transcript(&messages[..messages.len().min(3)]);
    let tail_start = messages.len().saturating_sub(3).max(messages.len().min(3));
    let tail = project_transcript(&messages[tail_start..]);
    format!(
        "Working dir: {}\nMessages: {}\n\n--- First ---\n{head}\n--- Last ---\n{tail}",
        session.working_dir.display(),
        messages.len()
    )
}

fn project_spawn_context(messages: &[crate::conversation::message::Message]) -> Option<String> {
    use crate::conversation::message::ProvenanceKind;
    messages.iter().find_map(|m| {
        let p = m.metadata.provenance.as_ref()?;
        (p.kind == ProvenanceKind::SpawnContext).then(|| {
            m.content
                .iter()
                .filter_map(|c| c.as_text())
                .collect::<Vec<_>>()
                .join("\n")
        })
    })
}

/// Find `session_id` inside a layout echo (§4.3 `workspace_echo.layout`).
fn gui_tab_for(layout: Option<&serde_json::Value>, session_id: &str) -> Option<serde_json::Value> {
    let windows = layout?.as_array()?;
    for window in windows {
        let window_id = window.get("window_id")?.as_str().unwrap_or_default();
        for group in window.get("layout")?.as_array()? {
            for tab in group.get("tabs")?.as_array()? {
                if tab.get("session_id")?.as_str() == Some(session_id) {
                    return Some(json!({
                        "window_id": window_id,
                        "group_id": group.get("group_id"),
                        "tab_id": tab.get("tab_id"),
                        "focused": group.get("active_tab") == tab.get("tab_id"),
                    }));
                }
            }
        }
    }
    None
}

#[async_trait]
impl McpClientTrait for WorkspaceClient {
    async fn list_tools(
        &self,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        Ok(ListToolsResult {
            tools: Self::get_tools(),
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        name: &str,
        arguments: Option<JsonObject>,
        meta: McpMeta,
        _cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        let caller = &meta.session_id;
        let content = match name {
            "workspace_list" => self.handle_list(caller, arguments).await,
            "workspace_read_conversation" => self.handle_read_conversation(caller, arguments).await,
            "workspace_send_prompt" => self.handle_send_prompt(caller, arguments).await,
            "workspace_set_tools" => self.handle_set_tools(caller, arguments).await,
            "workspace_close" => self.handle_close(caller, arguments).await,
            "workspace_watch" => self.handle_watch(caller, arguments).await,
            // BR-71 decision 22: the spawn tool is advertised here but
            // dispatched by the agent loop (it needs the parent's TaskConfig).
            // Reachable only if that interception is ever removed.
            crate::agents::subagent_tool::SUBAGENT_TOOL_NAME => {
                Err("`subagent` is dispatched by the agent loop, not by this extension".to_string())
            }
            _ => match PENDING_TOOLS.iter().find(|(tool, _)| *tool == name) {
                Some((_, task)) => Err(format!("not implemented until {task}")),
                None => Err(format!("Unknown tool: {name}")),
            },
        };
        match content {
            Ok(content) => Ok(CallToolResult::success(content)),
            Err(error) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Error: {error}"
            ))])),
        }
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::extension::PlatformExtensionContext;
    use crate::agents::mcp_client::McpClientTrait;
    use tokio_util::sync::CancellationToken;

    fn client() -> WorkspaceClient {
        let temp = tempfile::TempDir::new().unwrap();
        let session_manager = std::sync::Arc::new(crate::session::SessionManager::new(
            temp.path().to_path_buf(),
        ));
        // Leak the tempdir for the test's lifetime so the DB stays alive.
        std::mem::forget(temp);
        WorkspaceClient::new(PlatformExtensionContext {
            extension_manager: None,
            session_manager,
        })
        .unwrap()
    }

    fn test_meta() -> crate::agents::mcp_client::McpMeta {
        crate::agents::mcp_client::McpMeta::new("caller")
    }

    /// Call `workspace_list` and return the parsed payload.
    ///
    /// Every assertion below reads this rather than substring-matching the
    /// pretty-printed text: `text.contains("\"extensions\"")` is satisfied by an
    /// empty array, by the field appearing on the wrong object, and by the word
    /// turning up anywhere at all in a session name — so it pins nothing.
    async fn list(c: &WorkspaceClient, args: serde_json::Value) -> serde_json::Value {
        let args: rmcp::model::JsonObject = serde_json::from_value(args).unwrap();
        let result = c
            .call_tool(
                "workspace_list",
                Some(args),
                test_meta(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let text = result.content[0].as_text().unwrap().text.clone();
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("not JSON ({e}): {text}"))
    }

    /// The returned session ids, sorted so the assertion does not depend on the
    /// store's row order.
    fn sorted_ids(payload: &serde_json::Value) -> Vec<String> {
        let mut ids: Vec<String> = payload["sessions"]
            .as_array()
            .expect("sessions is an array")
            .iter()
            .map(|r| {
                r["session_id"]
                    .as_str()
                    .expect("row has a string id")
                    .to_string()
            })
            .collect();
        ids.sort();
        ids
    }

    fn sorted<S: AsRef<str>>(ids: Vec<S>) -> Vec<String> {
        let mut ids: Vec<String> = ids.into_iter().map(|s| s.as_ref().to_string()).collect();
        ids.sort();
        ids
    }

    /// The machine identifier and the human label are two different strings and
    /// must stay that way.
    ///
    /// `EXTENSION_NAME` is an IDENTIFIER: it is normalized into the
    /// `PLATFORM_EXTENSIONS` key by the platform spawn path
    /// (`extension_manager.rs`, `normalize(config.name)`) and into the advertised
    /// tool prefix `workspace__…`. `EXTENSION_TITLE` is the DISPLAY name the
    /// design of record specifies (agent-workspace-control.md §4.1, "display
    /// name **Workspace Control**") and is what the MCP `Implementation.title`
    /// carries. Collapsing the two — either way round — breaks one of them:
    /// "Workspace Control" normalizes to `workspacecontrol`, and "Workspace"
    /// is not the design's label.
    // `client()` builds a `SessionManager`, whose sqlx pool requires a Tokio
    // context even though nothing here awaits.
    #[tokio::test]
    async fn machine_name_normalizes_to_the_key_and_title_is_the_design_label() {
        assert_eq!(crate::agents::normalize(EXTENSION_NAME), "workspace");
        assert_eq!(EXTENSION_TITLE, "Workspace Control");
        let c = client();
        let info = c.get_info().unwrap();
        assert_eq!(info.server_info.name, EXTENSION_NAME);
        assert_eq!(info.server_info.title.as_deref(), Some(EXTENSION_TITLE));
    }

    /// The advertised surface and the placeholder surface must be disjoint, and
    /// together they must cover every tool [`INSTRUCTIONS`] names.
    ///
    /// This is the discriminating half of the surface check that
    /// `advertises_workspace_list_with_instructions` deliberately cannot be: that
    /// test asserts MEMBERSHIP (`contains`) because Tasks 13-17 each append one
    /// tool and re-run it expecting PASS, so a whole-vector equality there would
    /// be a fail-again-six-times gate (the plan puts its one exact-surface
    /// assertion in Task 24, the last task that changes `get_tools()`). But
    /// membership alone lets a premature tool — one whose handler still answers
    /// "not implemented until Task N" — be advertised without any test noticing,
    /// which is exactly the failure the phase-gate rule exists to prevent.
    ///
    /// The invariant below closes that hole without ever going stale: a task that
    /// implements a tool deletes its [`PENDING_TOOLS`] row (it must, or its own
    /// dispatch is unreachable) and adds it to `get_tools()`, and the two halves
    /// stay disjoint and complete by construction.
    #[tokio::test]
    async fn advertises_no_tool_whose_handler_is_still_a_placeholder() {
        let c = client();
        let advertised: Vec<String> = c
            .list_tools(None, CancellationToken::new())
            .await
            .unwrap()
            .tools
            .iter()
            .map(|t| t.name.to_string())
            .collect();

        for (pending, task) in PENDING_TOOLS {
            assert!(
                !advertised.contains(&pending.to_string()),
                "{pending} is advertised but its handler answers \
                 'not implemented until {task}'"
            );
        }

        // Every `workspace_*` tool the instruction block tells the model about is
        // either live or explicitly pending — never a name nothing handles.
        let mentioned: std::collections::BTreeSet<String> = INSTRUCTIONS
            .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .filter(|tok| tok.starts_with("workspace_"))
            .map(|tok| tok.to_string())
            .collect();
        assert!(
            mentioned.contains("workspace_list"),
            "the token scan found nothing; got: {mentioned:?}"
        );
        for name in &mentioned {
            assert!(
                advertised.contains(name)
                    || PENDING_TOOLS.iter().any(|(pending, _)| pending == name),
                "the instructions name {name}, which is neither advertised nor \
                 listed in PENDING_TOOLS"
            );
        }
    }

    /// This task registers exactly ONE tool; Tasks 13-17 append the rest.
    ///
    /// **This assertion is deliberately ADDITIVE (`contains`), not exact
    /// (`assert_eq!` on the whole vector).** Six later tasks each append one
    /// entry to `get_tools()` — 13, 14, 15, 16, 17 and 18 — and every one of
    /// them re-runs this test under the filter
    /// `--lib agents::workspace_extension` with "Expected: PASS". A
    /// whole-vector equality here would therefore be a fail-again-six-times
    /// gate. Exactly ONE exact-surface assertion exists in the plan, and it
    /// lives in the LAST task that changes the surface (Task 24) so it can
    /// never go stale mid-phase.
    #[tokio::test]
    async fn advertises_workspace_list_with_instructions() {
        let c = client();
        let tools = c
            .list_tools(None, CancellationToken::new())
            .await
            .unwrap()
            .tools;
        let names: Vec<_> = tools.iter().map(|t| t.name.to_string()).collect();
        assert!(
            names.contains(&"workspace_list".to_string()),
            "got: {names:?}"
        );

        let info = c.get_info().unwrap();
        let instructions = info.instructions.as_deref().unwrap();
        assert!(instructions.contains("chatrecall"));
        assert!(instructions.len() <= 2500, "injection budget (§6)");
        // No tool that is unimplemented AT A PHASE GATE may be named. The block
        // is written once for the whole Phase-1 surface (see its doc comment),
        // but `workspace_open` is Phase 2 — Task 21 would otherwise ship Phase 1
        // telling the model to call a tool that answers "not implemented".
        assert!(
            !instructions.contains("workspace_open"),
            "workspace_open is not advertised until Task 24"
        );
    }

    /// `parallel`, not `serial`: this test READS the process-global services
    /// slot (it asserts the headless answers — no GUI, nothing running, no KB
    /// selection), so it must never overlap a test that has overridden it.
    /// `serial_test`'s `parallel(key)` says exactly that — run freely alongside
    /// anything else, but never alongside `serial(workspace_services)`. Until a
    /// stand-in with `gui_attached() == true` existed, every override in this
    /// binary happened to answer `false` here and the overlap was invisible.
    #[tokio::test]
    #[serial_test::parallel(workspace_services)]
    async fn workspace_list_reports_headless_and_sessions() {
        let c = client();
        let parent = c
            .context
            .session_manager
            .create_session(
                std::env::temp_dir(),
                "listed".to_string(),
                crate::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();

        let v = list(&c, serde_json::json!({ "scope": "all" })).await;

        // Envelope: no GUI is attached in a unit test, and the paging metadata
        // (decision 17) is always present and honest, not merely present.
        assert_eq!(v["gui_attached"], serde_json::json!(false));
        assert_eq!(v["scope"], serde_json::json!("all"));
        assert_eq!(v["offset"], serde_json::json!(0));
        assert_eq!(v["returned"], serde_json::json!(1));
        assert_eq!(v["total_matching"], serde_json::json!(1));
        assert_eq!(v["has_more"], serde_json::json!(false));
        assert_eq!(sorted_ids(&v), vec![parent.id.clone()]);

        let row = &v["sessions"][0];
        assert_eq!(row["session_id"], serde_json::json!(parent.id));
        assert_eq!(row["name"], serde_json::json!("listed"));
        assert_eq!(row["session_type"], serde_json::json!("user"));
        assert_eq!(row["running"], serde_json::json!(false));
        assert_eq!(row["parent_session_id"], serde_json::Value::Null);
        assert!(
            !row["working_dir"].as_str().unwrap().is_empty(),
            "row: {row}"
        );
        // No GUI, so no tab placement — `null`, not a fabricated object.
        assert_eq!(row["gui"], serde_json::Value::Null);

        // §4.1: per-session enabled extensions. This session has no
        // session-specific extension state, so the row must carry the global
        // config — the same fallback `GET /sessions/{id}/extensions` performs.
        // Asserting the VALUE (not just the key) is what distinguishes the real
        // read from an empty placeholder array.
        //
        // Compared as a sorted set: the exact NAMES are the contract, their
        // order is an artifact of the config file's key order.
        let expected_extensions = sorted(
            crate::config::get_enabled_extensions()
                .iter()
                .map(|e| e.name())
                .collect::<Vec<String>>(),
        );
        assert!(
            !expected_extensions.is_empty(),
            "the fallback is empty, so this assertion would pin nothing"
        );
        let got_extensions = sorted(
            row["extensions"]
                .as_array()
                .expect("extensions is an array")
                .iter()
                .map(|n| n.as_str().expect("extension names are strings").to_string())
                .collect::<Vec<String>>(),
        );
        assert_eq!(got_extensions, expected_extensions);

        // Headless: no workspace services, so no knowledge selection.
        assert_eq!(row["knowledge_bases"], serde_json::json!([]));
        // Post-#45: the write target is reported alongside the set. Without it a
        // model can set a primary and never read it back. `null` is the correct
        // value here and is a DIFFERENT state from the key being absent, so the
        // presence check is separate from the value check.
        assert!(
            row.get("primary_kb").is_some(),
            "primary_kb is a required row field; row: {row}"
        );
        assert_eq!(row["primary_kb"], serde_json::Value::Null);
    }

    /// Decision 17: the page window is honoured and reported.
    #[tokio::test]
    async fn workspace_list_pages_instead_of_truncating() {
        let c = client();
        for i in 0..5 {
            c.context
                .session_manager
                .create_session(
                    std::env::temp_dir(),
                    format!("paged-{i}"),
                    crate::session::session_manager::SessionType::User,
                )
                .await
                .unwrap();
        }
        let page = |offset: u32, limit: u32| serde_json::json!({ "scope": "all", "offset": offset, "limit": limit });
        let first = list(&c, page(0, 2)).await;
        assert_eq!(first["returned"], serde_json::json!(2));
        assert_eq!(first["offset"], serde_json::json!(0));
        // `total_matching` counts every matching row, not just the page — that is
        // the whole point of decision 17.
        assert_eq!(first["total_matching"], serde_json::json!(5));
        assert_eq!(first["has_more"], serde_json::json!(true));

        let second = list(&c, page(2, 2)).await;
        assert_eq!(second["offset"], serde_json::json!(2));
        assert_eq!(second["returned"], serde_json::json!(2));
        assert_eq!(second["has_more"], serde_json::json!(true));

        let last = list(&c, page(4, 2)).await;
        assert_eq!(last["returned"], serde_json::json!(1));
        assert_eq!(last["has_more"], serde_json::json!(false));

        // The three pages must partition the five sessions: no overlap, no gap.
        let mut walked: Vec<String> = sorted_ids(&first);
        walked.extend(sorted_ids(&second));
        walked.extend(sorted_ids(&last));
        let unique: std::collections::BTreeSet<&String> = walked.iter().collect();
        assert_eq!(unique.len(), 5, "pages overlap or drop rows: {walked:?}");
        assert_eq!(sorted_ids(&list(&c, page(0, 200)).await), sorted(walked));
    }

    /// Decision 23: the `subagent_status` list mode, re-expressed.
    ///
    /// The fixture carries a DISTRACTOR for each filter, so the two are
    /// independently pinned. With only one parent and its one subagent, ignoring
    /// `parent_session_id` (→ every subagent) and ignoring `only_subagents` (→
    /// every child of that parent) both still yield the one expected row, and the
    /// test passes on an implementation that applies neither filter. Here:
    ///
    /// * `stray` is a subagent of a DIFFERENT parent — only `parent_session_id`
    ///   excludes it;
    /// * `masquerade` is a non-subagent child of the SAME parent — only
    ///   `only_subagents` excludes it.
    #[tokio::test]
    async fn workspace_list_filters_by_parent_and_by_subagent_type() {
        let c = client();
        let sm = c.context.session_manager.clone();
        let new = |name: &'static str, kind| {
            let sm = sm.clone();
            async move {
                sm.create_session(std::env::temp_dir(), name.into(), kind)
                    .await
                    .unwrap()
            }
        };
        use crate::session::session_manager::SessionType;
        let parent = new("p", SessionType::User).await;
        let other_parent = new("other-p", SessionType::User).await;
        let child = new("c", SessionType::SubAgent).await;
        let stray = new("stray", SessionType::SubAgent).await;
        let masquerade = new("masquerade", SessionType::User).await;
        for (id, parent_id) in [
            (&child.id, &parent.id),
            (&stray.id, &other_parent.id),
            (&masquerade.id, &parent.id),
        ] {
            sm.update(id)
                .parent_session_id(Some(parent_id.clone()))
                .apply()
                .await
                .unwrap();
        }

        // Assert on the ROW SET, not on substrings. Every child row carries
        // `"parent_session_id": "<parent id>"`, so a naive
        // `assert!(!text.contains(&parent.id))` is false by construction — the
        // parent's id is present as a FIELD of the matched child.
        let both = list(
            &c,
            serde_json::json!({
                "scope": "all", "parent_session_id": parent.id, "only_subagents": true
            }),
        )
        .await;
        assert_eq!(
            sorted_ids(&both),
            sorted(vec![&child.id]),
            "the parent is not its own subagent, and neither are the distractors"
        );
        assert_eq!(both["total_matching"], serde_json::json!(1));

        // `parent_session_id` alone: both children of `parent`, whatever their
        // type — so dropping `only_subagents` is observable.
        let by_parent = list(
            &c,
            serde_json::json!({ "scope": "all", "parent_session_id": parent.id }),
        )
        .await;
        assert_eq!(
            sorted_ids(&by_parent),
            sorted(vec![&child.id, &masquerade.id])
        );

        // `only_subagents` alone: every subagent in the workspace, whoever spawned
        // it — so dropping `parent_session_id` is observable.
        let by_type = list(
            &c,
            serde_json::json!({ "scope": "all", "only_subagents": true }),
        )
        .await;
        assert_eq!(sorted_ids(&by_type), sorted(vec![&child.id, &stray.id]));

        // And unfiltered: all five, so neither filter is silently always-on.
        let all = list(&c, serde_json::json!({ "scope": "all" })).await;
        assert_eq!(
            sorted_ids(&all),
            sorted(vec![
                &parent.id,
                &other_parent.id,
                &child.id,
                &stray.id,
                &masquerade.id
            ])
        );
    }

    /// The seeded workspace every `workspace_read_conversation` test reads.
    ///
    /// The fixture exists because the original single gate test could not tell a
    /// correct handler from several plausible wrong ones: it seeded a lone tool
    /// *request* (so correlation, status and digests were untested), asserted
    /// only the ABSENCE of earlier prose for `from_msg_uid` (so an off-by-one
    /// that dropped the named message too would pass), never exercised `last`,
    /// `summary` or `max_chars`, and refused an EMPTY hidden session through a
    /// single view (so "refuses hidden" was indistinguishable from "renders
    /// nothing" or "refuses transcript only").
    ///
    /// So the fixture carries, in `open`, seven messages chosen so that every
    /// projection has both something it MUST contain and something it MUST NOT:
    ///
    /// | # | message                                        | discriminates |
    /// |---|------------------------------------------------|---------------|
    /// | 1 | user "please compute"                          | prose vs. tool views; the head of `summary` |
    /// | 2 | assistant tool request `call-1` → `shell`      | inclusive `from_msg_uid` slicing |
    /// | 3 | user tool response `call-1` → ok               | request/response correlation + ok digest |
    /// | 4 | assistant tool request `call-2` → `grep`       | a second pair, so correlation is per-id |
    /// | 5 | user tool response `call-2` → error            | the error status branch |
    /// | 6 | assistant "here is the answer"                 | `last` tail ordering |
    /// | 7 | user "SPAWN CONTEXT …" (SpawnContext, agent-invisible) | provenance-specific projection |
    ///
    /// plus a POPULATED `hidden` session (so a refusal cannot be an empty render)
    /// and a `plain` session with no spawn record (so the missing-spawn error has
    /// a subject).
    struct ReadFixture {
        client: WorkspaceClient,
        open_id: String,
        hidden_id: String,
        plain_id: String,
        /// The durable msg_uid of message 2 (the `shell` request), adopted by
        /// `add_message_adopting_uid` (#41).
        shell_request_uid: String,
    }

    impl ReadFixture {
        /// Call `workspace_read_conversation` and return the whole result.
        async fn read(&self, args: serde_json::Value) -> CallToolResult {
            let args: rmcp::model::JsonObject = serde_json::from_value(args).unwrap();
            self.client
                .call_tool(
                    "workspace_read_conversation",
                    Some(args),
                    test_meta(),
                    CancellationToken::new(),
                )
                .await
                .unwrap()
        }

        /// Call it and return the text of a SUCCESSFUL result, failing loudly on
        /// an error result — otherwise "the projection omitted X" and "the call
        /// errored" are the same passing assertion.
        async fn read_ok(&self, args: serde_json::Value) -> String {
            let result = self.read(args).await;
            let text = result.content[0].as_text().unwrap().text.clone();
            assert_ne!(result.is_error, Some(true), "unexpected error: {text}");
            text
        }
    }

    async fn read_fixture() -> ReadFixture {
        use crate::conversation::message::{Message, MessageProvenance, ProvenanceKind};
        use crate::session::session_manager::SessionType;
        let c = client();
        let sm = c.context.session_manager.clone();

        let hidden = sm
            .create_session(std::env::temp_dir(), "h".into(), SessionType::Hidden)
            .await
            .unwrap();
        let open = sm
            .create_session(std::env::temp_dir(), "o".into(), SessionType::User)
            .await
            .unwrap();
        let plain = sm
            .create_session(std::env::temp_dir(), "p".into(), SessionType::User)
            .await
            .unwrap();

        // The hidden session is POPULATED. An empty one would let "refuses
        // hidden" pass on a handler that simply had nothing to say.
        let mut secret = Message::user().with_text("hidden secret");
        sm.add_message_adopting_uid(&hidden.id, &mut secret)
            .await
            .unwrap();
        let mut ordinary = Message::user().with_text("nothing special");
        sm.add_message_adopting_uid(&plain.id, &mut ordinary)
            .await
            .unwrap();

        // CallToolRequestParams derives NO Default — spell all four fields.
        // The citation is the DEPENDENCY's file, not this repo's:
        // ~/.cargo/registry/.../rmcp-0.14.0/src/model.rs:1887-1902 (derive
        // :1887, `pub struct CallToolRequestParams` :1890, fields
        // meta/name/arguments/task :1892-1901, `}` :1902). Re-verified
        // 2026-07-28 against the pinned `rmcp = "0.14.0"` (Cargo.toml:18):
        // the derive list is `Debug, Serialize, Deserialize, Clone, PartialEq`
        // — still no `Default`. Do NOT resolve this against
        // `crates/biorouter/src/model.rs`, which is 908 lines and unrelated.
        let request = |name: &str, args: serde_json::Value| {
            Ok(rmcp::model::CallToolRequestParams {
                meta: None,
                name: name.to_string().into(),
                arguments: Some(args.as_object().unwrap().clone()),
                task: None,
            })
        };

        let mut m1 = Message::user().with_text("please compute");
        let mut m2 = Message::assistant().with_tool_request(
            "call-1",
            request("shell", serde_json::json!({"command": "ls"})),
        );
        let mut m3 = Message::user().with_tool_response(
            "call-1",
            Ok(rmcp::model::CallToolResult::success(vec![Content::text(
                "total 0 alpha.txt",
            )])),
        );
        let mut m4 = Message::assistant().with_tool_request(
            "call-2",
            request("grep", serde_json::json!({"pattern": "beta"})),
        );
        let mut m5 = Message::user().with_tool_response(
            "call-2",
            Err(rmcp::model::ErrorData::new(
                rmcp::model::ErrorCode::INTERNAL_ERROR,
                "grep exploded",
                None,
            )),
        );
        let mut m6 = Message::assistant().with_text("here is the answer");
        let mut spawn = Message::user()
            .with_text("SPAWN CONTEXT …")
            .with_provenance(MessageProvenance {
                kind: ProvenanceKind::SpawnContext,
                from_session_id: None,
                from_session_name: None,
            });
        // Agent-invisible, so the spawn_context projection is proven to key off
        // PROVENANCE rather than off whatever the agent happens to still see.
        spawn.metadata.agent_visible = false;

        for message in [
            &mut m1, &mut m2, &mut m3, &mut m4, &mut m5, &mut m6, &mut spawn,
        ] {
            sm.add_message_adopting_uid(&open.id, message)
                .await
                .unwrap();
        }

        ReadFixture {
            client: c,
            open_id: open.id,
            hidden_id: hidden.id,
            plain_id: plain.id,
            shell_request_uid: m2.id.clone().expect("adopted uid"),
        }
    }

    /// §5 "no covert reads": a hidden session is refused whatever view is asked
    /// for, and its content never leaks into the refusal.
    ///
    /// Every view is exercised because refusing only the default (`transcript`)
    /// would leave `tool_calls` — the projection that shows exactly what that
    /// agent DID — as an unguarded back door.
    #[tokio::test]
    async fn read_conversation_refuses_a_populated_hidden_session_in_every_view() {
        let f = read_fixture().await;
        for view in ["transcript", "tool_calls", "summary", "spawn_context"] {
            let result = f
                .read(serde_json::json!({ "session_id": f.hidden_id, "view": view }))
                .await;
            let text = result.content[0].as_text().unwrap().text.clone();
            assert_eq!(result.is_error, Some(true), "view {view} was not refused");
            assert!(text.contains("hidden"), "view {view}: {text}");
            assert!(
                !text.contains("hidden secret"),
                "view {view} leaked the session's content: {text}"
            );
        }
        // The same read against a visible session succeeds — so the refusal is
        // the session TYPE, not a handler that fails on everything.
        let ok = f
            .read_ok(serde_json::json!({ "session_id": f.open_id, "view": "transcript" }))
            .await;
        assert!(ok.contains("please compute"), "{ok}");
    }

    /// §4.1 `tool_calls`: request/response pairs only, correlated by their
    /// shared id, each carrying its status and a result digest.
    #[tokio::test]
    async fn read_conversation_tool_calls_correlates_pairs_with_status_and_digest() {
        let f = read_fixture().await;
        let text = f
            .read_ok(serde_json::json!({ "session_id": f.open_id, "view": "tool_calls" }))
            .await;

        // Both directions of both pairs, each stamped with its own id — a
        // projection that dropped responses, or that lost the correlation, fails
        // here rather than passing on the tool name alone.
        assert!(text.contains("→ [call-1]"), "{text}");
        assert!(text.contains("← [call-1]"), "{text}");
        assert!(text.contains("→ [call-2]"), "{text}");
        assert!(text.contains("← [call-2]"), "{text}");
        // Arguments, not just the name (§4.1 "tool name, arguments, status,
        // result digest").
        assert!(text.contains("Tool: shell"), "{text}");
        assert!(text.contains("\"command\""), "{text}");
        assert!(text.contains("Tool: grep"), "{text}");
        // Status + digest, both branches.
        assert!(text.contains("ok: total 0 alpha.txt"), "{text}");
        assert!(text.contains("error:"), "{text}");
        assert!(text.contains("grep exploded"), "{text}");

        // …and none of the prose. This is what "without transcript noise" means.
        assert!(!text.contains("please compute"), "{text}");
        assert!(!text.contains("here is the answer"), "{text}");
        assert!(!text.contains("SPAWN CONTEXT"), "{text}");

        // The empty branch is reachable and says so rather than returning "".
        let none = f
            .read_ok(serde_json::json!({
                "session_id": f.open_id, "view": "tool_calls", "last": 1
            }))
            .await;
        assert!(none.contains("No tool calls in range."), "{none}");
    }

    /// `transcript`: prose plus one-line stubs for tool payloads, with the
    /// injection stamp on a provenance-marked message.
    #[tokio::test]
    async fn read_conversation_transcript_stubs_tool_payloads_and_stamps_provenance() {
        let f = read_fixture().await;
        let text = f
            .read_ok(serde_json::json!({ "session_id": f.open_id, "view": "transcript" }))
            .await;

        // The header identifies the session being read — the model is looking at
        // someone else's conversation and must be able to tell which.
        assert!(
            text.starts_with(&format!("Session {} (o, User)", f.open_id)),
            "{text}"
        );
        assert!(text.contains("please compute"), "{text}");
        assert!(text.contains("here is the answer"), "{text}");
        // Tool payloads collapse to stubs — the tool call is named, the result is
        // referenced by id rather than inlined.
        assert!(text.contains("<tool call: Tool: shell"), "{text}");
        assert!(text.contains("<tool result: call-1>"), "{text}");
        assert!(
            !text.contains("total 0 alpha.txt"),
            "the result payload must not be inlined in a transcript: {text}"
        );
        // BR-71 provenance: an injected record says so.
        assert!(text.contains("(injected: SpawnContext)"), "{text}");
    }

    /// BR-45 range: `from_msg_uid` is INCLUSIVE of the named message, and `last`
    /// is a tail applied after the slice.
    ///
    /// The inclusive half is the assertion the original gate lacked: it checked
    /// only that earlier prose was gone, which an off-by-one that also dropped
    /// the named message would satisfy.
    #[tokio::test]
    async fn read_conversation_range_is_inclusive_and_last_is_a_tail() {
        let f = read_fixture().await;
        let from_shell = serde_json::json!({
            "session_id": f.open_id, "view": "transcript",
            "from_msg_uid": f.shell_request_uid,
        });

        let ranged = f.read_ok(from_shell.clone()).await;
        assert!(
            !ranged.contains("please compute"),
            "messages before the uid are excluded: {ranged}"
        );
        assert!(
            ranged.contains("<tool call: Tool: shell"),
            "the NAMED message is included, not skipped: {ranged}"
        );
        assert!(ranged.contains("SPAWN CONTEXT"), "{ranged}");

        // `last` is a TAIL, not a head: the newest messages survive.
        let tail1 = f
            .read_ok(serde_json::json!({
                "session_id": f.open_id, "view": "transcript", "last": 1
            }))
            .await;
        assert!(tail1.contains("SPAWN CONTEXT"), "{tail1}");
        assert!(!tail1.contains("please compute"), "{tail1}");
        assert!(!tail1.contains("here is the answer"), "{tail1}");

        let tail2 = f
            .read_ok(serde_json::json!({
                "session_id": f.open_id, "view": "transcript", "last": 2
            }))
            .await;
        assert!(tail2.contains("here is the answer"), "{tail2}");
        assert!(tail2.contains("SPAWN CONTEXT"), "{tail2}");
        assert!(!tail2.contains("<tool call: Tool: grep"), "{tail2}");

        // Combined, in the documented order: uid slice FIRST, then tail.
        let mut both = from_shell.clone();
        both["last"] = serde_json::json!(1);
        let both = f.read_ok(both).await;
        assert!(both.contains("SPAWN CONTEXT"), "{both}");
        assert!(!both.contains("<tool call: Tool: shell"), "{both}");

        // A `last` wider than the slice does not reach back past the uid — the
        // range wins, so the two controls compose instead of fighting.
        let mut wide = from_shell;
        wide["last"] = serde_json::json!(500);
        let wide = f.read_ok(wide).await;
        assert!(wide.contains("<tool call: Tool: shell"), "{wide}");
        assert!(!wide.contains("please compute"), "{wide}");

        // An unknown uid is an error naming it, not a silent whole-transcript
        // read — the failure mode that would quietly hand back everything.
        let unknown = f
            .read(serde_json::json!({
                "session_id": f.open_id, "view": "transcript", "from_msg_uid": "no-such-uid"
            }))
            .await;
        let text = unknown.content[0].as_text().unwrap().text.clone();
        assert_eq!(unknown.is_error, Some(true), "{text}");
        assert!(text.contains("no-such-uid"), "{text}");
        assert!(!text.contains("please compute"), "{text}");
    }

    /// `summary`: the chatrecall-load head/tail digest, over the WHOLE session.
    #[tokio::test]
    async fn read_conversation_summary_reports_head_tail_and_size() {
        let f = read_fixture().await;
        let text = f
            .read_ok(serde_json::json!({ "session_id": f.open_id, "view": "summary" }))
            .await;

        assert!(text.contains("Working dir:"), "{text}");
        // The count is over every message, which is what makes this a summary
        // rather than a differently-formatted transcript.
        assert!(text.contains("Messages: 7"), "{text}");
        let (head, tail) = text
            .split_once("--- Last ---")
            .expect("summary has a head and a tail section");
        assert!(head.contains("--- First ---"), "{text}");
        assert!(head.contains("please compute"), "head: {head}");
        assert!(tail.contains("SPAWN CONTEXT"), "tail: {tail}");
        assert!(
            !tail.contains("please compute"),
            "head and tail must not overlap: {tail}"
        );

        // The digest is of the whole session, so the transcript-only `last`
        // narrowing does not silently shrink it.
        let narrowed = f
            .read_ok(serde_json::json!({
                "session_id": f.open_id, "view": "summary", "last": 1
            }))
            .await;
        assert!(narrowed.contains("Messages: 7"), "{narrowed}");
        assert!(narrowed.contains("please compute"), "{narrowed}");
    }

    /// `spawn_context`: the provenance-marked record only — §4.4's "how was this
    /// subagent started", not the conversation that followed.
    #[tokio::test]
    async fn read_conversation_spawn_context_returns_only_the_provenance_record() {
        let f = read_fixture().await;
        let text = f
            .read_ok(serde_json::json!({ "session_id": f.open_id, "view": "spawn_context" }))
            .await;

        assert!(text.contains("SPAWN CONTEXT"), "{text}");
        // A handler that fell through to the ordinary transcript would also
        // contain the record — these three are what tell the two apart.
        assert!(!text.contains("please compute"), "{text}");
        assert!(!text.contains("here is the answer"), "{text}");
        assert!(!text.contains("Tool: shell"), "{text}");

        // A session with no spawn record says so rather than returning the
        // transcript or an empty success.
        let missing = f
            .read(serde_json::json!({ "session_id": f.plain_id, "view": "spawn_context" }))
            .await;
        let text = missing.content[0].as_text().unwrap().text.clone();
        assert_eq!(missing.is_error, Some(true), "{text}");
        assert!(text.contains("no recorded spawn context"), "{text}");
        assert!(!text.contains("nothing special"), "{text}");
    }

    /// `max_chars` clips the BODY and says so, naming the controls that narrow
    /// the read — §4.1's "never truncating silently".
    #[tokio::test]
    async fn read_conversation_max_chars_clips_visibly_and_names_the_controls() {
        let f = read_fixture().await;
        let full = f
            .read_ok(serde_json::json!({ "session_id": f.open_id, "view": "transcript" }))
            .await;
        assert!(!full.contains("clipped at"), "the full read is not clipped");

        let clipped = f
            .read_ok(serde_json::json!({
                "session_id": f.open_id, "view": "transcript", "max_chars": 40
            }))
            .await;
        assert!(clipped.contains("[clipped at 40 chars"), "{clipped}");
        // The marker names the narrowing controls instead of leaving the model
        // to re-run the same call and get the same clip.
        assert!(clipped.contains("`last`"), "{clipped}");
        assert!(clipped.contains("`from_msg_uid`"), "{clipped}");
        assert!(clipped.contains("max_chars"), "{clipped}");
        // The tail of the transcript really is gone, and the header — which is
        // outside the cap — really is still there.
        assert!(!clipped.contains("SPAWN CONTEXT"), "{clipped}");
        assert!(clipped.starts_with("Session "), "{clipped}");

        // A cap above the body length is a no-op, so the clip is driven by the
        // parameter rather than being always-on.
        let roomy = f
            .read_ok(serde_json::json!({
                "session_id": f.open_id, "view": "transcript", "max_chars": 200_000
            }))
            .await;
        assert_eq!(roomy, full);
    }

    /// §4.1's "never truncating silently", proven on the REAL production path.
    ///
    /// A `max_chars` raised to its 200k ceiling produces a body of roughly 50k
    /// tokens — well over BR-6's ~25k-token inline budget. In production this
    /// tool is an ordinary extension tool, so its result leaves
    /// `dispatch_tool_call` and goes straight into
    /// [`large_response_handler::process_tool_response`], which offloads the
    /// whole payload to a handle and replaces the reply with a preview. The
    /// handler's doc comment used to promise a BR-7 *session blob* instead;
    /// that mechanism only applies below BR-6's budget, so the promise was
    /// false in exactly the band a raised cap reaches.
    ///
    /// This test pins the behaviour the promise is really made of: nothing is
    /// lost, and the reply says where the rest is. It drives the same entry
    /// point the agent loop drives, so it fails if BR-6 is ever reordered out
    /// of this tool's path, if its budget is raised past the `max_chars`
    /// ceiling, or if offloading stops writing the full body.
    #[tokio::test]
    async fn read_conversation_oversized_result_is_retained_in_full_on_the_production_path() {
        use crate::agents::large_response_handler::{
            process_tool_response, LargeResponseContext, DEFAULT_LARGE_RESPONSE_TOKENS,
        };
        use crate::conversation::message::Message;
        use crate::session::session_manager::SessionType;

        let c = client();
        let sm = c.context.session_manager.clone();
        let big = sm
            .create_session(std::env::temp_dir(), "big".into(), SessionType::User)
            .await
            .unwrap();
        // Token-dense filler (distinct short numbers), so "over 200k chars" is
        // reliably also "over 25k tokens" rather than depending on how well the
        // BPE merges repeated prose.
        let chunk: String = (0..900).map(|n| format!("{n} ")).collect();
        for i in 0..80 {
            let mut m = Message::user().with_text(format!("line {i}: {chunk}"));
            sm.add_message_adopting_uid(&big.id, &mut m).await.unwrap();
        }

        // 1. What this extension hands back at the documented ceiling.
        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
            "session_id": big.id, "view": "transcript", "max_chars": 200_000
        }))
        .unwrap();
        let raw = c
            .call_tool(
                "workspace_read_conversation",
                Some(args),
                test_meta(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let full = raw.content[0].as_text().unwrap().text.clone();
        assert!(
            full.chars().count() > 200_000,
            "the fixture must reach the cap; got {} chars",
            full.chars().count()
        );
        assert!(full.contains("[clipped at 200000 chars"), "cap not applied");

        // 2. The BR-6 stage the agent loop applies to every dispatched result.
        // Its handle lands under the session working dir, so point that at a
        // directory this test owns.
        let workdir = tempfile::TempDir::new().unwrap();
        let processed = process_tool_response(
            Ok(raw.clone()),
            &LargeResponseContext {
                session_id: big.id.clone(),
                working_dir: workdir.path().to_path_buf(),
                tool_name: "workspace__workspace_read_conversation".to_string(),
            },
        )
        .await
        .unwrap();
        let summary = processed.content[0].as_text().unwrap().text.clone();
        assert_ne!(
            summary, full,
            "a {}k-char result must not be handed to the model inline \
             (BR-6 budget is {DEFAULT_LARGE_RESPONSE_TOKENS} tokens)",
            200
        );
        assert!(
            summary.contains("The complete output is saved at:"),
            "the reply must say where the rest is: {summary}"
        );
        assert!(summary.contains("preview"), "{summary}");

        // 3. …and the FULL payload really is there, byte for byte. This is the
        // whole claim: a raised `max_chars` round-trips instead of being
        // silently truncated.
        let handle_dir = workdir.path().join(".biorouter/tool-output");
        let handle = std::fs::read_dir(&handle_dir)
            .unwrap_or_else(|e| panic!("no handle dir at {}: {e}", handle_dir.display()))
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .find(|p| p.is_file())
            .expect("BR-6 wrote no handle file");
        assert_eq!(
            std::fs::read_to_string(&handle).unwrap(),
            full,
            "the offloaded handle must hold the complete result"
        );

        // The boundary is real: an ordinary-sized read of the same session is
        // passed through untouched, so the offload is size-driven rather than
        // always-on for this tool.
        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
            "session_id": big.id, "view": "summary"
        }))
        .unwrap();
        let small = c
            .call_tool(
                "workspace_read_conversation",
                Some(args),
                test_meta(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let small_text = small.content[0].as_text().unwrap().text.clone();
        let passed_through = process_tool_response(
            Ok(small),
            &LargeResponseContext {
                session_id: big.id,
                working_dir: workdir.path().to_path_buf(),
                tool_name: "workspace__workspace_read_conversation".to_string(),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            passed_through.content[0].as_text().unwrap().text,
            small_text
        );
    }

    #[tokio::test]
    async fn send_prompt_note_appends_with_provenance_without_running_a_turn() {
        use crate::conversation::message::ProvenanceKind;
        let c = client();
        let sm = c.context.session_manager.clone();
        let caller = sm
            .create_session(
                std::env::temp_dir(),
                "caller-name".into(),
                crate::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();
        let target = sm
            .create_session(
                std::env::temp_dir(),
                "target".into(),
                crate::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();

        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
            "session_id": target.id, "text": "context for later", "mode": "note"
        }))
        .unwrap();
        let meta = crate::agents::mcp_client::McpMeta::new(caller.id.clone());
        let result = c
            .call_tool(
                "workspace_send_prompt",
                Some(args),
                meta,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_ne!(result.is_error, Some(true));

        let reread = sm.get_session(&target.id, true).await.unwrap();
        let msgs = reread.conversation.unwrap().messages().to_vec();
        let injected = msgs.last().expect("note appended");
        let p = injected
            .metadata
            .provenance
            .as_ref()
            .expect("provenance stamped");
        assert_eq!(p.kind, ProvenanceKind::AgentInjection);
        assert_eq!(p.from_session_id.as_deref(), Some(caller.id.as_str()));
        assert_eq!(p.from_session_name.as_deref(), Some("caller-name"));

        // …and the TEXT carries the untrusted-data envelope. The provenance
        // stamp lives in `MessageMetadata`, which never reaches the provider —
        // only the framing tells the target's MODEL that this came from another
        // agent rather than from its user.
        let body = injected
            .content
            .iter()
            .filter_map(|c| c.as_text())
            .collect::<Vec<_>>()
            .join("");
        assert!(body.contains("untrusted=\"true\""), "got: {body}");
        assert!(body.contains("caller-name"), "the frame names the source");
        assert!(body.contains("context for later"), "the payload survives");

        // …and it is PINNED, and the pin is actually honourable.
        //
        // Two assertions, not one, because `is_pinned()` alone is a marker that
        // every compaction site is free to ignore. `pin_is_eligible` is the
        // predicate they all consult: the message must carry the marker, be
        // agent-visible, AND have every content block pin-eligible.
        // `frame_workspace_injection` wraps the text, and a note built
        // agent-invisible or carrying a tool block would keep `is_pinned()`
        // true while being silently unpreservable.
        assert!(
            injected.is_pinned(),
            "a note must survive compaction (#51 pin)"
        );
        assert!(
            crate::context_mgmt::pins::pin_is_eligible(injected),
            "the pin must be HONOURABLE, not merely present"
        );

        // The control that makes the two assertions above mean "the pin worked"
        // rather than "this is true of any message": the same message without
        // `.pinned()` is not eligible for preservation at all.
        let unpinned = {
            let mut m = injected.clone();
            m.metadata.pinned = false;
            m
        };
        assert!(
            !crate::context_mgmt::pins::pin_is_eligible(&unpinned),
            "without the marker there is nothing to honour — so the assertion \
             above is testing the pin"
        );
    }

    #[test]
    fn approval_modes_are_the_two_that_can_actually_prompt() {
        use crate::config::BioRouterMode;
        assert!(!mode_requires_approval(BioRouterMode::Auto));
        assert!(mode_requires_approval(BioRouterMode::Approve));
        assert!(mode_requires_approval(BioRouterMode::SmartApprove));
        // NOT an oversight: Chat mode skips tools entirely and can never raise
        // a confirmation. Classifying it as an approval mode refuses the safest
        // configuration there is, with a refusal message that is factually false
        // for it.
        assert!(!mode_requires_approval(BioRouterMode::Chat));
    }

    /// Decision c / the shared drain loop: the HUMAN's own soft interrupt must
    /// NOT be framed. `queue_soft_interrupt` enqueues with `provenance: None`,
    /// and wrapping the user's own words in "treat this as lower-trust" is worse
    /// than not framing at all.
    #[tokio::test]
    async fn a_human_soft_interrupt_is_never_framed_as_untrusted() {
        use crate::conversation::message::frame_workspace_injection;
        // The framer is only reached through the `Some(AgentInjection)` arm of
        // the drain loop (Task 3); this pins the discrimination it depends on.
        let framed = frame_workspace_injection(None, "stop and use Python");
        assert!(framed.contains("untrusted=\"true\""));
        assert!(!"stop and use Python".contains("untrusted"));
    }

    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn send_prompt_turn_and_steer_error_clearly_without_a_daemon() {
        // NO daemon — declared, not hoped for. Task 9's `set_for_tests(None)`
        // is what makes this deterministic; before it existed, whether another
        // test in this binary had installed services decided the outcome.
        crate::workspace_services::set_for_tests(None);
        let c = client();
        let sm = c.context.session_manager.clone();
        let target = sm
            .create_session(
                std::env::temp_dir(),
                "t".into(),
                crate::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();
        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
            "session_id": target.id, "text": "go", "mode": "steer"
        }))
        .unwrap();
        let result = c
            .call_tool(
                "workspace_send_prompt",
                Some(args),
                test_meta(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        crate::workspace_services::clear_test_override();
        // steer with no running turn is always an error (mirrors /interrupt 409).
        assert_eq!(result.is_error, Some(true));
    }

    // ---------------------------------------------------------------------
    // Handler-level tests, against a daemon stand-in that publishes the same
    // bus lifecycle the real turn runner does.
    //
    // The three tests above are real but not DISCRIMINATING: `note` never
    // reaches the daemon at all, and the no-daemon test only pins that a steer
    // without services fails somehow. Everything the tool actually promises —
    // that a steer lands in the running turn's queue and is refused when that
    // queue has closed, that a turn is framed/stamped/announced, that
    // `wait:"final_message"` returns THIS turn's answer, that the fan-out cap
    // bounds turns rather than tool calls — lives below, because each of those
    // needs a controllable `WorkspaceServices` and a controllable event stream.
    // Without them a stubbed-out `mode:"turn"` and the unguarded
    // `queue_soft_interrupt_with_provenance` both pass the suite.
    // ---------------------------------------------------------------------

    use crate::session_events::SessionBusEvent;
    use crate::workspace_services::{
        KbPrimaryChoice, KbSelectionView, WorkspaceServices, WorkspaceTurnLease,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    /// A daemon stand-in that can be told what to answer AND publishes the bus
    /// lifecycle the real runner publishes (`workspace/turn.rs`): the events a
    /// previous turn emits inside the hydration window, then this turn's
    /// `TurnStarted { turn_id }`, then whatever this turn emits.
    #[derive(Default)]
    struct FakeServices {
        gui: bool,
        /// Sessions `is_turn_active` reports busy.
        active: Mutex<std::collections::HashSet<String>>,
        /// Published to the target BEFORE the new turn's start — i.e. attributed
        /// to whatever was running when the caller asked.
        preamble: Mutex<Vec<SessionBusEvent>>,
        /// Published after it. Empty leaves the turn running.
        epilogue: Mutex<Vec<SessionBusEvent>>,
        /// Every accepted turn: (session id, turn id, the injected message).
        started: Mutex<Vec<(String, String, crate::conversation::message::Message)>>,
        /// Every `gui_command` frame.
        frames: Mutex<Vec<serde_json::Value>>,
        /// Every `cancel_turn(session_id)`, in order. Task 16 needs the CALL
        /// recorded, not just its answer: a `workspace_close` that returned the
        /// right sentence without ever tripping the token is exactly the wrong
        /// implementation this records to exclude.
        cancels: Mutex<Vec<String>>,
        /// Every `stop_agent(session_id)`, in order — same reason.
        stops: Mutex<Vec<String>>,
        /// When set, `stop_agent` fails with it (and records the call anyway).
        stop_error: Mutex<Option<String>>,
        turn_seq: AtomicUsize,
    }

    impl FakeServices {
        fn with_gui(gui: bool) -> Self {
            Self {
                gui,
                ..Default::default()
            }
        }
        fn busy(self, session_id: &str) -> Self {
            self.active.lock().unwrap().insert(session_id.to_string());
            self
        }
        fn preamble(self, events: Vec<SessionBusEvent>) -> Self {
            *self.preamble.lock().unwrap() = events;
            self
        }
        fn epilogue(self, events: Vec<SessionBusEvent>) -> Self {
            *self.epilogue.lock().unwrap() = events;
            self
        }
        /// Make `stop_agent` fail, so a test can pin that the failure surfaces
        /// as a tool error instead of a cheerful "stopped and evicted".
        fn stop_fails(self, message: &str) -> Self {
            *self.stop_error.lock().unwrap() = Some(message.to_string());
            self
        }
        fn install(self) -> std::sync::Arc<Self> {
            let me = std::sync::Arc::new(self);
            crate::workspace_services::set_for_tests(Some(me.clone()));
            me
        }
        /// End a turn the way the runner does: publish the terminal, then drop
        /// the lock (never the other way round).
        fn finish(&self, session_id: &str) {
            crate::session_events::publish(
                session_id,
                SessionBusEvent::TurnFinished {
                    reason: "stop".into(),
                    token_state: None,
                },
            );
            self.active.lock().unwrap().remove(session_id);
        }
        fn notify_frames(&self) -> Vec<serde_json::Value> {
            self.frames
                .lock()
                .unwrap()
                .iter()
                .filter(|f| f["cmd"] == "notify")
                .cloned()
                .collect()
        }
        fn all_frames(&self) -> Vec<serde_json::Value> {
            self.frames.lock().unwrap().clone()
        }
        fn cancels(&self) -> Vec<String> {
            self.cancels.lock().unwrap().clone()
        }
        fn stops(&self) -> Vec<String> {
            self.stops.lock().unwrap().clone()
        }
    }

    struct FakeLease;
    impl WorkspaceTurnLease for FakeLease {
        fn turn_id(&self) -> &str {
            "turn-lease"
        }
    }

    #[async_trait::async_trait]
    impl WorkspaceServices for FakeServices {
        fn gui_attached(&self) -> bool {
            self.gui
        }
        fn layout_snapshot(&self) -> Option<serde_json::Value> {
            None
        }
        fn is_turn_active(&self, session_id: &str) -> bool {
            self.active.lock().unwrap().contains(session_id)
        }
        /// Records the call and answers the way the daemon does: there is a
        /// token to trip only while a turn is in flight, and tripping it ends
        /// that turn — so an idle session yields `None` and a busy one yields
        /// its id exactly once.
        fn cancel_turn(&self, session_id: &str) -> Option<String> {
            self.cancels.lock().unwrap().push(session_id.to_string());
            self.active
                .lock()
                .unwrap()
                .remove(session_id)
                .then(|| "turn-live".to_string())
        }
        fn begin_turn(
            &self,
            _session_id: &str,
            _cancel: tokio_util::sync::CancellationToken,
        ) -> Result<Box<dyn WorkspaceTurnLease>, String> {
            Ok(Box::new(FakeLease))
        }
        async fn stop_agent(&self, session_id: &str) -> Result<(), String> {
            self.stops.lock().unwrap().push(session_id.to_string());
            // Bind out of the guard before the early return: a `MutexGuard`
            // alive across the tail of an `async fn` makes the future `!Send`.
            let failure = self.stop_error.lock().unwrap().clone();
            if let Some(message) = failure {
                return Err(message);
            }
            self.active.lock().unwrap().remove(session_id);
            Ok(())
        }
        async fn start_detached_turn(
            &self,
            session_id: &str,
            message: crate::conversation::message::Message,
        ) -> Result<String, String> {
            let turn_id = format!("turn-{}", self.turn_seq.fetch_add(1, Ordering::SeqCst) + 1);
            // The hydration window: the real service awaits the target's
            // provider and extension restore here, BEFORE it takes the turn
            // lock, so a turn that was already in flight can end in the middle
            // of this call.
            for event in self.preamble.lock().unwrap().iter().cloned() {
                crate::session_events::publish(session_id, event);
            }
            self.active.lock().unwrap().insert(session_id.to_string());
            crate::session_events::publish(
                session_id,
                SessionBusEvent::TurnStarted {
                    turn_id: turn_id.clone(),
                },
            );
            for event in self.epilogue.lock().unwrap().iter().cloned() {
                crate::session_events::publish(session_id, event);
            }
            self.started
                .lock()
                .unwrap()
                .push((session_id.to_string(), turn_id.clone(), message));
            Ok(turn_id)
        }
        async fn start_session(
            &self,
            _working_dir: std::path::PathBuf,
            _extensions: Option<Vec<String>>,
            _knowledge_bases: Vec<String>,
            _primary: KbPrimaryChoice,
        ) -> Result<String, String> {
            Ok("s-new".into())
        }
        fn set_knowledge_bases(
            &self,
            _session_id: &str,
            _kbs: &[String],
            _primary: KbPrimaryChoice,
        ) -> Result<KbSelectionView, String> {
            Ok(KbSelectionView::default())
        }
        fn knowledge_selection(&self, _session_id: &str) -> KbSelectionView {
            KbSelectionView::default()
        }
        async fn gui_command(
            &self,
            frame: serde_json::Value,
            _wait_result: bool,
        ) -> Result<serde_json::Value, String> {
            self.frames.lock().unwrap().push(frame);
            Ok(serde_json::json!({ "ok": true }))
        }
    }

    /// A caller id no other test in this binary shares. The fan-out counters and
    /// the `AgentManager` LRU are both process-global.
    fn unique_id(prefix: &str) -> String {
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        format!("{prefix}-{}", SEQ.fetch_add(1, Ordering::SeqCst))
    }

    /// Call `workspace_send_prompt` as `caller`.
    async fn send_prompt(
        c: &WorkspaceClient,
        caller: &str,
        args: serde_json::Value,
    ) -> CallToolResult {
        let args: rmcp::model::JsonObject = serde_json::from_value(args).unwrap();
        c.call_tool(
            "workspace_send_prompt",
            Some(args),
            crate::agents::mcp_client::McpMeta::new(caller.to_string()),
            CancellationToken::new(),
        )
        .await
        .unwrap()
    }

    fn text_of(result: &CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn assistant_says(text: &str) -> SessionBusEvent {
        SessionBusEvent::Agent(crate::agents::AgentEvent::Message(
            crate::conversation::message::Message::assistant().with_text(text),
        ))
    }

    fn turn_finished(reason: &str) -> SessionBusEvent {
        SessionBusEvent::TurnFinished {
            reason: reason.into(),
            token_state: None,
        }
    }

    /// `wait:"final_message"` must return the answer of the turn IT started.
    ///
    /// The subscription is necessarily older than the turn (it is opened before
    /// `start_detached_turn`, because a ring only exists once someone has
    /// subscribed), and the daemon hydrates the target before it takes the turn
    /// lock. A turn already in flight can therefore publish its own final
    /// message and its `TurnFinished` into our stream during that window — and a
    /// wait loop that accepts the first terminal it sees hands the caller the
    /// PREVIOUS conversation's answer, labelled as this one's, with nothing in
    /// the text to reveal the substitution.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn wait_returns_this_turns_answer_not_one_that_landed_before_it_started() {
        let services = FakeServices::with_gui(true)
            .preamble(vec![
                assistant_says("PREVIOUS TURN ANSWER"),
                turn_finished("previous"),
            ])
            .epilogue(vec![
                assistant_says("THIS TURN ANSWER"),
                turn_finished("stop"),
            ])
            .install();
        let c = client();
        let caller = unique_id("caller");
        let target = unique_id("target");

        let result = send_prompt(
            &c,
            &caller,
            serde_json::json!({
                "session_id": target, "text": "go", "mode": "turn",
                "wait": "final_message", "timeout_s": 5
            }),
        )
        .await;
        crate::workspace_services::clear_test_override();

        assert_ne!(result.is_error, Some(true), "got: {}", text_of(&result));
        let text = text_of(&result);
        assert!(
            text.contains("THIS TURN ANSWER"),
            "the wait must return the started turn's answer; got: {text}"
        );
        assert!(
            !text.contains("PREVIOUS TURN ANSWER"),
            "an answer that landed before this turn started is somebody else's; got: {text}"
        );
        // …and the terminal it reported is this turn's, not the previous one's.
        assert!(text.contains("(stop)"), "got: {text}");
        assert!(!text.contains("(previous)"), "got: {text}");
        // The started turn id is named, so the caller can correlate.
        assert!(
            text.contains(&services.started.lock().unwrap()[0].1),
            "got: {text}"
        );
    }

    /// §5's cap counts TURNS IN FLIGHT, not tool calls in progress.
    ///
    /// A `wait:"none"` injection returns while its turn runs on, so a cap whose
    /// guard is bound to the calling stack frame releases the slot the instant
    /// the tool answers and bounds nothing: the fifth, fiftieth and five
    /// hundredth detached turn are all accepted under a cap of four. The
    /// release half is asserted too — a cap that never releases would pass the
    /// refusal assertion alone while permanently wedging the caller.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[serial_test::serial(workspace_services)]
    async fn a_fire_and_forget_injection_holds_its_slot_until_the_turn_ends() {
        let services = FakeServices::with_gui(true).install();
        let c = client();
        let caller = unique_id("fanout-caller");
        let cap = WorkspaceClient::injected_turn_cap();

        let mut targets = Vec::new();
        for i in 0..cap {
            let target = unique_id("fanout-target");
            let result = send_prompt(
                &c,
                &caller,
                serde_json::json!({ "session_id": target, "text": "go", "mode": "turn" }),
            )
            .await;
            assert_ne!(
                result.is_error,
                Some(true),
                "injection {i} of {cap} must fit under the cap; got: {}",
                text_of(&result)
            );
            targets.push(target);
        }

        // Every one of those turns is STILL RUNNING (no terminal published), so
        // the next injection is over budget — even though every one of the calls
        // that started them has long since returned.
        let over = send_prompt(
            &c,
            &caller,
            serde_json::json!({
                "session_id": unique_id("fanout-target"), "text": "go", "mode": "turn"
            }),
        )
        .await;
        assert_eq!(
            over.is_error,
            Some(true),
            "the {}th detached turn must be refused; got: {}",
            cap + 1,
            text_of(&over)
        );
        assert!(
            text_of(&over).contains("in flight"),
            "got: {}",
            text_of(&over)
        );

        // Ending one turn frees exactly one slot. (Held forever would be a
        // different bug with the same green assertion above.)
        services.finish(&targets[0]);
        let mut accepted = None;
        for _ in 0..100 {
            let result = send_prompt(
                &c,
                &caller,
                serde_json::json!({
                    "session_id": unique_id("fanout-target"), "text": "go", "mode": "turn"
                }),
            )
            .await;
            if result.is_error != Some(true) {
                accepted = Some(result);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        crate::workspace_services::clear_test_override();
        assert!(
            accepted.is_some(),
            "the slot of a finished turn must be released"
        );
    }

    /// `mode:"turn"` delivers a FRAMED, provenance-stamped message and says so
    /// in the GUI.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn a_turn_injects_a_framed_stamped_message_and_announces_itself() {
        use crate::conversation::message::ProvenanceKind;
        let services = FakeServices::with_gui(true).install();
        let c = client();
        let caller = c
            .context
            .session_manager
            .create_session(
                std::env::temp_dir(),
                "planner".into(),
                crate::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();
        let target = unique_id("turn-target");

        let result = send_prompt(
            &c,
            &caller.id,
            serde_json::json!({ "session_id": target, "text": "use the log scale", "mode": "turn" }),
        )
        .await;
        crate::workspace_services::clear_test_override();
        assert_ne!(result.is_error, Some(true), "got: {}", text_of(&result));

        let started = services.started.lock().unwrap();
        assert_eq!(started.len(), 1, "exactly one turn was started");
        let (session_id, turn_id, message) = &started[0];
        assert_eq!(session_id, &target);
        assert!(
            text_of(&result).contains(turn_id),
            "the caller is told the turn id"
        );

        let body: String = message
            .content
            .iter()
            .filter_map(|c| c.as_text())
            .collect::<Vec<_>>()
            .join("");
        assert!(body.contains("untrusted=\"true\""), "got: {body}");
        assert!(
            body.contains("planner"),
            "the frame names the source: {body}"
        );
        assert!(body.contains("use the log scale"), "got: {body}");
        let p = message
            .metadata
            .provenance
            .as_ref()
            .expect("provenance stamped");
        assert_eq!(p.kind, ProvenanceKind::AgentInjection);
        assert_eq!(p.from_session_id.as_deref(), Some(caller.id.as_str()));

        // Decision 2: never silent in the GUI.
        let frames = services.notify_frames();
        assert_eq!(frames.len(), 1, "one toast; got {frames:?}");
        assert_eq!(frames[0]["session_id"], serde_json::json!(target));
        let msg = frames[0]["message"].as_str().unwrap();
        assert!(
            msg.contains("planner") && msg.contains("started a turn"),
            "got: {msg}"
        );
    }

    /// Decision 4, both branches: with no GUI attached, a target whose mode
    /// cannot be read is refused, and one whose live agent is in a
    /// non-prompting mode is not.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn without_a_gui_a_turn_is_refused_unless_the_target_agent_cannot_prompt() {
        let services = FakeServices::with_gui(false).install();
        let c = client();
        let caller = unique_id("caller");
        let unknown = unique_id("no-agent-target");

        let refused = send_prompt(
            &c,
            &caller,
            serde_json::json!({ "session_id": unknown, "text": "go", "mode": "turn" }),
        )
        .await;
        assert_eq!(refused.is_error, Some(true), "got: {}", text_of(&refused));
        assert!(
            text_of(&refused).contains("refusing to start a turn"),
            "got: {}",
            text_of(&refused)
        );
        assert!(
            services.started.lock().unwrap().is_empty(),
            "a refused turn must not have been started"
        );

        // The mirror: a LIVE agent whose own mode cannot raise a confirmation.
        let live = unique_id("live-agent-target");
        let manager = crate::execution::manager::AgentManager::instance()
            .await
            .expect("agent manager");
        let agent = manager
            .get_or_create_agent(live.clone())
            .await
            .expect("agent");
        assert!(
            !mode_requires_approval(agent.config.biorouter_mode),
            "this test needs a non-prompting default mode; got {:?}",
            agent.config.biorouter_mode
        );
        let allowed = send_prompt(
            &c,
            &caller,
            serde_json::json!({ "session_id": live, "text": "go", "mode": "turn" }),
        )
        .await;
        crate::workspace_services::clear_test_override();
        assert_ne!(allowed.is_error, Some(true), "got: {}", text_of(&allowed));
        assert_eq!(services.started.lock().unwrap().len(), 1);
    }

    /// A steer reaches the RUNNING turn's queue, unframed and stamped, and is
    /// announced.
    ///
    /// Unframed is decision c: the drain loop frames the
    /// `Some(AgentInjection)` arm itself, so framing here would double-wrap.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn a_steer_lands_in_the_running_turns_queue_stamped_and_unframed() {
        use crate::agents::agent::TurnId;
        use crate::conversation::message::ProvenanceKind;
        let target = unique_id("steer-target");
        let services = FakeServices::with_gui(true).busy(&target).install();
        let c = client();
        let caller = c
            .context
            .session_manager
            .create_session(
                std::env::temp_dir(),
                "planner".into(),
                crate::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();

        let manager = crate::execution::manager::AgentManager::instance()
            .await
            .expect("agent manager");
        let agent = manager
            .get_or_create_agent(target.clone())
            .await
            .expect("agent");
        agent.open_for_turn(TurnId::new("agent-turn-live"));

        let result = send_prompt(
            &c,
            &caller.id,
            serde_json::json!({ "session_id": target, "text": "use Python instead", "mode": "steer" }),
        )
        .await;
        crate::workspace_services::clear_test_override();
        assert_ne!(result.is_error, Some(true), "got: {}", text_of(&result));
        assert!(
            text_of(&result).contains("agent-turn-live"),
            "the caller is told which turn took it; got: {}",
            text_of(&result)
        );

        let queued = agent.drain_soft_interrupts();
        assert_eq!(
            queued.len(),
            1,
            "the steer must be in the live turn's queue"
        );
        assert_eq!(
            queued[0].text, "use Python instead",
            "the RAW text is queued — the drain loop frames it, so framing here \
             would wrap it twice"
        );
        let p = queued[0].provenance.as_ref().expect("stamped");
        assert_eq!(p.kind, ProvenanceKind::AgentInjection);
        assert_eq!(p.from_session_name.as_deref(), Some("planner"));

        let frames = services.notify_frames();
        assert_eq!(frames.len(), 1, "one toast; got {frames:?}");
        assert!(
            frames[0]["message"].as_str().unwrap().contains("steered"),
            "got {frames:?}"
        );
    }

    /// #69: the server's turn lock and the agent's interrupt queue can disagree,
    /// and only the queue is authoritative.
    ///
    /// `is_turn_active` is still true here — the lock is released *after* the
    /// loop stops accepting — but the loop has already closed. The unguarded
    /// `queue_soft_interrupt_with_provenance` returns `()`, so it would push
    /// into the closed queue and report "queued" for a steer that this turn will
    /// never consume and the *next* turn would be ambushed by.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn a_steer_into_a_closed_queue_is_refused_not_deferred() {
        use crate::agents::agent::{Drained, TurnId};
        let target = unique_id("closed-steer-target");
        let _services = FakeServices::with_gui(true).busy(&target).install();
        let c = client();
        let caller = unique_id("caller");

        let manager = crate::execution::manager::AgentManager::instance()
            .await
            .expect("agent manager");
        let agent = manager
            .get_or_create_agent(target.clone())
            .await
            .expect("agent");
        agent.open_for_turn(TurnId::new("agent-turn-closing"));
        // The loop reaches its exit with nothing queued and closes atomically.
        assert!(matches!(agent.close_and_drain(), Drained::Empty));

        let result = send_prompt(
            &c,
            &caller,
            serde_json::json!({ "session_id": target, "text": "too late", "mode": "steer" }),
        )
        .await;
        crate::workspace_services::clear_test_override();

        assert_eq!(result.is_error, Some(true), "got: {}", text_of(&result));
        assert!(
            text_of(&result).contains("steer refused"),
            "got: {}",
            text_of(&result)
        );
        assert!(
            !agent.has_soft_interrupts(),
            "a refused steer must not be sitting in the queue waiting to ambush \
             the next turn"
        );
    }

    // ---- Task 15: workspace_set_tools ----------------------------------
    //
    // Every test below is `serial(workspace_services)` even though none of them
    // installs a stand-in. `handle_set_tools` ends by pushing a §5 visibility
    // toast through `notify_target`, which reads the PROCESS-GLOBAL services
    // override — so an un-serialized set_tools test that lands while
    // `a_turn_injects_a_framed_stamped_message_and_announces_itself` holds its
    // `FakeServices` deposits a "Tools changed by another agent" frame in that
    // test's `notify_frames()` and fails its "one toast" assertion. Observed,
    // not theorized: it is what the first green run of this task did.

    async fn set_tools(c: &WorkspaceClient, args: serde_json::Value) -> CallToolResult {
        let args: rmcp::model::JsonObject = serde_json::from_value(args).unwrap();
        c.call_tool(
            "workspace_set_tools",
            Some(args),
            test_meta(),
            CancellationToken::new(),
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn set_tools_rejects_unknown_names_before_mutating_anything() {
        let c = client();
        let sm = c.context.session_manager.clone();
        let target = sm
            .create_session(
                std::env::temp_dir(),
                "t".into(),
                crate::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();

        let result = set_tools(
            &c,
            serde_json::json!({
                "session_id": target.id, "add_extensions": ["definitely-not-an-extension"]
            }),
        )
        .await;
        assert_eq!(result.is_error, Some(true), "got: {}", text_of(&result));
        assert!(
            text_of(&result).contains("definitely-not-an-extension"),
            "got: {}",
            text_of(&result)
        );
    }

    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn set_tools_applies_session_scoped_skills_without_touching_the_machine_config() {
        let c = client();
        let sm = c.context.session_manager.clone();
        let target = sm
            .create_session(
                std::env::temp_dir(),
                "skills-target".into(),
                crate::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();

        let result = set_tools(
            &c,
            serde_json::json!({
                "session_id": target.id,
                "add_skills": ["single-cell"],
                "remove_skills": ["ralph"]
            }),
        )
        .await;
        assert_ne!(result.is_error, Some(true), "got: {}", text_of(&result));

        // Session-scoped, and persisted where Task 11 says it lives.
        let over = crate::agents::session_skills::for_session(&sm, &target.id)
            .await
            .unwrap();
        assert!(over.add.contains(&"single-cell".to_string()));
        assert!(over.remove.contains(&"ralph".to_string()));

        // Decision (c): the machine-wide preference is untouched.
        let machine = crate::config::paths::Paths::config_dir().join("skills-config.json");
        let before = std::fs::read_to_string(&machine).ok();
        let _ = set_tools(
            &c,
            serde_json::json!({ "session_id": target.id, "add_skills": ["proteomics"] }),
        )
        .await;
        assert_eq!(
            std::fs::read_to_string(&machine).ok(),
            before,
            "workspace_set_tools must never write skills-config.json"
        );
    }

    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn set_tools_validates_the_model_against_the_providers_catalog() {
        let c = client();
        let sm = c.context.session_manager.clone();
        let target = sm
            .create_session(
                std::env::temp_dir(),
                "model-target".into(),
                crate::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();

        // Unknown PROVIDER: refused by name, before any agent is touched.
        let result = set_tools(
            &c,
            serde_json::json!({
                "session_id": target.id, "provider": "not-a-provider", "model": "whatever"
            }),
        )
        .await;
        assert_eq!(result.is_error, Some(true), "got: {}", text_of(&result));
        assert!(
            text_of(&result).contains("not-a-provider"),
            "got: {}",
            text_of(&result)
        );

        // `model` without `provider` is refused with a message that says so —
        // a model name alone is ambiguous across providers.
        let result = set_tools(
            &c,
            serde_json::json!({ "session_id": target.id, "model": "gpt-5.5" }),
        )
        .await;
        assert_eq!(result.is_error, Some(true), "got: {}", text_of(&result));
        assert!(
            text_of(&result).contains("provider"),
            "got: {}",
            text_of(&result)
        );
    }

    #[test]
    fn known_model_check_accepts_catalog_entries_and_rejects_typos() {
        // The pure half, testable without a configured provider: a provider's
        // metadata carries `known_models` and `allows_unlisted_models`.
        let known = vec!["claude-sonnet-9".to_string(), "claude-opus-5".to_string()];
        assert!(model_is_known("claude-opus-5", &known, false));
        assert!(!model_is_known("claude-opus-V", &known, false));
        // An empty catalog means "this provider does not publish one" — accept,
        // and let the provider itself reject at request time.
        assert!(model_is_known("anything", &[], false));
        // Decision b, honestly implemented: a provider that DECLARES it accepts
        // unlisted models must accept them here too. ollama, llamacpp,
        // gcpvertexai and every custom/declarative provider set this flag
        // (`ProviderMetadata::with_unlisted_models()`), and the GUI's own model
        // picker honours it. Refusing `ollama` + `qwen3.6:latest` — a locally
        // pulled model that is by definition not in any published catalog —
        // would make the tool stricter than the UI it mirrors.
        assert!(model_is_known("qwen3.6:latest", &known, true));
    }

    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn set_tools_reports_every_applied_change_in_one_line() {
        let c = client();
        let sm = c.context.session_manager.clone();
        let target = sm
            .create_session(
                std::env::temp_dir(),
                "report".into(),
                crate::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();
        let result = set_tools(
            &c,
            serde_json::json!({ "session_id": target.id, "add_skills": ["single-cell"] }),
        )
        .await;
        let text = text_of(&result);
        assert!(text.contains("+skill:single-cell"), "got: {text}");
    }

    // ---- Task 16: workspace_close --------------------------------------

    /// Runs WITH a daemon stand-in installed, and says so explicitly. This is
    /// not decoration: `scope:"turn"` starts with
    /// `services.ok_or("scope:\"turn\" requires the BioRouter daemon")?`, so
    /// without an override the first assertion below sees `is_error == Some(true)`
    /// and fails. The override is process-global, hence `#[serial]`; the
    /// `workspace_services` key is shared with every other test in the crate
    /// that pins the daemon state.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn close_turn_is_idempotent_and_close_tab_reports_headless() {
        crate::workspace_services::set_for_tests(Some(std::sync::Arc::new(
            crate::workspace_services::NullServices,
        )));

        let c = client();
        let sm = c.context.session_manager.clone();
        let target = sm
            .create_session(
                std::env::temp_dir(),
                "t".into(),
                crate::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();

        // scope:"turn" with nothing running: success with cancelled=false
        // semantics (never an error — mirrors POST /agent/cancel).
        // `NullServices::cancel_turn` returns None, which is that path.
        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
            "session_id": target.id, "scope": "turn"
        }))
        .unwrap();
        let result = c
            .call_tool(
                "workspace_close",
                Some(args),
                test_meta(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_ne!(result.is_error, Some(true));
        assert!(result.content[0]
            .as_text()
            .unwrap()
            .text
            .contains("no turn"));

        // scope:"tab" with no GUI attached (`NullServices::gui_attached()` is
        // false): not an error — session-level no-op, says so.
        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
            "session_id": target.id, "scope": "tab"
        }))
        .unwrap();
        let result = c
            .call_tool(
                "workspace_close",
                Some(args),
                test_meta(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_ne!(result.is_error, Some(true));
        assert!(result.content[0]
            .as_text()
            .unwrap()
            .text
            .to_lowercase()
            .contains("no gui"));

        crate::workspace_services::clear_test_override();
    }

    /// The other world: NO daemon at all. `scope:"turn"` must fail loudly rather
    /// than pretend it cancelled something.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn close_turn_without_a_daemon_says_so() {
        crate::workspace_services::set_for_tests(None);

        let c = client();
        let sm = c.context.session_manager.clone();
        let target = sm
            .create_session(
                std::env::temp_dir(),
                "t2".into(),
                crate::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();
        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
            "session_id": target.id, "scope": "turn"
        }))
        .unwrap();
        let result = c
            .call_tool(
                "workspace_close",
                Some(args),
                test_meta(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].as_text().unwrap().text.contains("daemon"));

        crate::workspace_services::clear_test_override();
    }

    // The two tests above exercise only the paths where `workspace_close`
    // *declines to act*: an idle `turn`, a headless `tab`, and a `turn` with no
    // daemon at all. Every one of them is satisfied by a handler that matches on
    // `scope` and returns the expected sentence without ever calling
    // `gui_command`, `cancel_turn` or `stop_agent` — which is why the reviewer
    // could not tell the real implementation from that stub. The tests below
    // pin the CALLS, not the prose: `FakeServices` records each one, so a
    // handler that only talks fails them.

    /// Call `workspace_close` as `caller`.
    async fn close(c: &WorkspaceClient, caller: &str, args: serde_json::Value) -> CallToolResult {
        let args: rmcp::model::JsonObject = serde_json::from_value(args).unwrap();
        c.call_tool(
            "workspace_close",
            Some(args),
            crate::agents::mcp_client::McpMeta::new(caller.to_string()),
            CancellationToken::new(),
        )
        .await
        .unwrap()
    }

    /// `scope:"tab"` WITH a GUI: the tab close must actually reach the renderer,
    /// addressed to the target, and nothing else may happen — the session and
    /// its turn survive, so no cancel, no stop, and no toast (closing a tab is
    /// the user's own window management, not a cross-session intervention).
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn close_tab_with_a_gui_sends_the_close_tab_frame_and_nothing_else() {
        let services = FakeServices::with_gui(true).install();
        let c = client();
        let target = unique_id("tab-target");

        let result = close(
            &c,
            "closer",
            serde_json::json!({ "session_id": target, "scope": "tab" }),
        )
        .await;

        assert_ne!(result.is_error, Some(true), "got: {}", text_of(&result));
        let frames = services.all_frames();
        assert_eq!(
            frames.len(),
            1,
            "expected exactly one frame, got: {frames:?}"
        );
        assert_eq!(frames[0]["type"], "workspace");
        assert_eq!(frames[0]["cmd"], "close_tab");
        assert_eq!(frames[0]["session_id"], target);
        assert!(services.cancels().is_empty(), "tab scope must not cancel");
        assert!(services.stops().is_empty(), "tab scope must not stop");
        assert!(text_of(&result).contains(&target));

        crate::workspace_services::clear_test_override();
    }

    /// `scope:"turn"` on a session that IS running: the token must be tripped
    /// for that session, the answer must name the turn the daemon reported, and
    /// §5 says the target's GUI is told who did it.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn close_turn_cancels_the_running_turn_and_tells_the_target() {
        let target = unique_id("turn-target");
        let services = FakeServices::with_gui(true).busy(&target).install();
        let c = client();
        let caller = unique_id("closer");

        let result = close(
            &c,
            &caller,
            serde_json::json!({ "session_id": target, "scope": "turn" }),
        )
        .await;

        assert_ne!(result.is_error, Some(true), "got: {}", text_of(&result));
        assert_eq!(
            services.cancels(),
            vec![target.clone()],
            "the running turn's token was never tripped"
        );
        let text = text_of(&result);
        assert!(
            text.contains("turn-live") && text.contains(&target),
            "the answer must name the cancelled turn and its session; got: {text}"
        );

        let notifies = services.notify_frames();
        assert_eq!(notifies.len(), 1, "expected one toast, got: {notifies:?}");
        assert_eq!(
            notifies[0]["session_id"], target,
            "toast went to the wrong session"
        );
        let message = notifies[0]["message"].as_str().unwrap();
        assert!(
            message.contains(&caller) && message.to_lowercase().contains("cancel"),
            "the toast must say what happened and who did it; got: {message}"
        );
        assert!(
            services.stops().is_empty(),
            "turn scope must not evict the agent"
        );

        crate::workspace_services::clear_test_override();
    }

    /// `scope:"agent"`: cancel + evict, and tell the target. The session record
    /// surviving is what the answer promises, so it says so.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn close_agent_stops_the_agent_and_tells_the_target() {
        let target = unique_id("agent-target");
        let services = FakeServices::with_gui(true).busy(&target).install();
        let c = client();
        let caller = unique_id("closer");

        let result = close(
            &c,
            &caller,
            serde_json::json!({ "session_id": target, "scope": "agent" }),
        )
        .await;

        assert_ne!(result.is_error, Some(true), "got: {}", text_of(&result));
        assert_eq!(
            services.stops(),
            vec![target.clone()],
            "stop_agent was never called"
        );
        let text = text_of(&result);
        assert!(
            text.contains(&target) && text.contains("session record"),
            "got: {text}"
        );

        let notifies = services.notify_frames();
        assert_eq!(notifies.len(), 1, "expected one toast, got: {notifies:?}");
        assert_eq!(notifies[0]["session_id"], target);
        let message = notifies[0]["message"].as_str().unwrap();
        assert!(
            message.contains(&caller) && message.to_lowercase().contains("stopped"),
            "got: {message}"
        );

        crate::workspace_services::clear_test_override();
    }

    /// A failing eviction must surface, not be papered over by the success
    /// sentence — and it must NOT toast the target that its agent was stopped
    /// when it was not.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn close_agent_surfaces_a_failed_stop() {
        let target = unique_id("stop-fail");
        let services = FakeServices::with_gui(true)
            .stop_fails("registry is wedged")
            .install();
        let c = client();

        let result = close(
            &c,
            "closer",
            serde_json::json!({ "session_id": target, "scope": "agent" }),
        )
        .await;

        assert_eq!(result.is_error, Some(true), "got: {}", text_of(&result));
        assert!(
            text_of(&result).contains("registry is wedged"),
            "got: {}",
            text_of(&result)
        );
        assert!(
            services.notify_frames().is_empty(),
            "a failed stop must not announce that the agent was stopped"
        );

        crate::workspace_services::clear_test_override();
    }

    /// An unrecognised scope is a refusal that names the three legal ones — and
    /// above all it must not silently fall through to one of them.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn close_rejects_an_unknown_scope_without_touching_anything() {
        let target = unique_id("bad-scope");
        let services = FakeServices::with_gui(true).busy(&target).install();
        let c = client();

        let result = close(
            &c,
            "closer",
            serde_json::json!({ "session_id": target, "scope": "everything" }),
        )
        .await;

        assert_eq!(result.is_error, Some(true));
        let text = text_of(&result);
        assert!(
            text.contains("everything")
                && text.contains("tab")
                && text.contains("turn")
                && text.contains("agent"),
            "the refusal must name the offending scope and the legal ones; got: {text}"
        );
        assert!(services.cancels().is_empty());
        assert!(services.stops().is_empty());
        assert!(services.all_frames().is_empty());

        crate::workspace_services::clear_test_override();
    }

    /// The annotation the task specifies. `workspace_close` cancels turns and
    /// evicts agents, so a client that trusts `read_only_hint` to decide what it
    /// may call unattended must be told the truth.
    #[tokio::test]
    async fn close_is_annotated_as_a_mutating_tool() {
        let c = client();
        let tools = c
            .list_tools(None, CancellationToken::new())
            .await
            .unwrap()
            .tools;
        let tool = tools
            .iter()
            .find(|t| t.name == "workspace_close")
            .expect("workspace_close is advertised");
        let annotations = tool.annotations.as_ref().expect("annotated");
        assert_eq!(annotations.read_only_hint, Some(false));
        assert_eq!(annotations.destructive_hint, Some(true));
        assert_eq!(annotations.idempotent_hint, Some(false));
    }

    // ---- Task 17: workspace_watch ------------------------------------------

    /// The resolver itself, over all three sources. Pure enough to test without
    /// a daemon, which is the point — the daemon is the source that is ABSENT in
    /// the configuration this whole helper exists for.
    #[tokio::test]
    async fn liveness_prefers_the_handle_registry_then_the_daemon_then_unknown() {
        use crate::agents::subagent_handle::BackgroundSubagent;
        use crate::agents::subagent_result::SubagentResult;

        // No daemon and no handle: UNKNOWN, never Idle. This is the assertion
        // that stops `workspace_watch` from silently no-opping headless.
        assert_eq!(
            session_liveness(None, "caller-1", "s-unrelated"),
            SessionLiveness::Unknown
        );

        // A running background child of THIS caller: Running, with no daemon.
        let running = BackgroundSubagent::register(
            "caller-1",
            "child-running",
            "count files",
            CancellationToken::new(),
        );
        assert_eq!(
            session_liveness(None, "caller-1", "child-running"),
            SessionLiveness::Running
        );

        // …and once it completes, Idle — so a watch on a finished child still
        // returns immediately headless, which is the deadlock the table forbids.
        running.complete(SubagentResult::from_error("done"));
        assert_eq!(
            session_liveness(None, "caller-1", "child-running"),
            SessionLiveness::Idle
        );

        // Handles are scoped to their parent (`list_for_session`), so another
        // session's child is Unknown to me, not Idle.
        assert_eq!(
            session_liveness(None, "caller-2", "child-running"),
            SessionLiveness::Unknown
        );
    }

    /// The headless regression, end to end through the tool: a genuinely
    /// running background child must NOT be reported "already idle".
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn watch_parks_on_a_running_headless_child_instead_of_claiming_it_is_idle() {
        use crate::agents::subagent_handle::BackgroundSubagent;
        // NO daemon. Declared, not assumed: another test in this binary may
        // have pinned one, and `set_for_tests(None)` is the only way to say
        // "there is no daemon" once anything has.
        crate::workspace_services::set_for_tests(None);

        let c = client();
        let _running = BackgroundSubagent::register(
            "caller",
            "child-live",
            "long job",
            CancellationToken::new(),
        );

        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
            "session_ids": ["child-live"], "timeout_s": 1
        }))
        .unwrap();
        let result = c
            .call_tool(
                "workspace_watch",
                Some(args),
                test_meta(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let text = result.content[0].as_text().unwrap().text.clone();
        crate::workspace_services::clear_test_override();
        assert!(
            !text.contains("already idle"),
            "a running child must never be reported idle: {text}"
        );
        assert!(text.contains("Still running"), "got: {text}");
    }

    /// The SAME regression in the DAEMON configuration, which is the normal
    /// desktop and `biorouterd` case and which the headless test above cannot
    /// reach.
    ///
    /// `spawn_background_subagent` registers its handle synchronously and only
    /// then spawns a task whose first await is `SUBAGENT_SEMAPHORE.acquire()`
    /// (cap 8). Task 33 takes the server turn lease INSIDE the run, i.e. after
    /// that permit. So a queued child is registered-and-running from the
    /// parent's point of view while `is_turn_active` is still false for it —
    /// exactly what `NullServices` models. If `session_liveness` asks the daemon
    /// first, a 10-way fan-out reports the two queued children "already idle"
    /// and `mode:"any"` returns immediately with work that has not started.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn watch_does_not_trust_the_daemon_over_a_running_handle() {
        use crate::agents::subagent_handle::BackgroundSubagent;
        crate::workspace_services::set_for_tests(Some(std::sync::Arc::new(
            crate::workspace_services::NullServices, // is_turn_active -> false
        )));

        let c = client();
        let _queued = BackgroundSubagent::register(
            "caller",
            "child-queued",
            "waiting on the semaphore",
            CancellationToken::new(),
        );

        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
            "session_ids": ["child-queued"], "timeout_s": 1
        }))
        .unwrap();
        let result = c
            .call_tool(
                "workspace_watch",
                Some(args),
                test_meta(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let text = result.content[0].as_text().unwrap().text.clone();
        crate::workspace_services::clear_test_override();
        assert!(
            !text.contains("already idle"),
            "a registered, not-yet-complete child must never be reported idle \
             just because the daemon has no lease for it yet: {text}"
        );
    }

    /// The other half: a FINISHED background child returns immediately, with no
    /// daemon and with no 120-second park.
    #[tokio::test]
    async fn watch_returns_immediately_for_a_finished_background_child() {
        use crate::agents::subagent_handle::BackgroundSubagent;
        use crate::agents::subagent_result::SubagentResult;
        let c = client();
        let handle = BackgroundSubagent::register(
            "caller",
            "child-done",
            "short job",
            CancellationToken::new(),
        );
        handle.complete(SubagentResult::from_error("finished"));

        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
            "session_ids": ["child-done"], "timeout_s": 30
        }))
        .unwrap();
        let started = std::time::Instant::now();
        let result = c
            .call_tool(
                "workspace_watch",
                Some(args),
                test_meta(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "must not block on a finished child"
        );
        assert_ne!(result.is_error, Some(true));
        let text = result.content[0].as_text().unwrap().text.clone();
        assert!(text.contains("child-done"));
        assert!(text.contains("already idle"));
    }

    /// `Unknown` must PARK, and say it could not check.
    ///
    /// Added beyond the task's own test list, which leaves this branch
    /// unpinned: the resolver test asserts `session_liveness(None, …) ==
    /// Unknown` in isolation, and both end-to-end "must not say already idle"
    /// tests register a handle first — so they resolve `Running`, and nothing
    /// exercised `Unknown` THROUGH the tool. Rewriting `handle_watch`'s
    /// `Unknown` arm to push "already idle" (exactly the `is_some_and` collapse
    /// this whole helper exists to prevent, just moved one level out of the
    /// resolver) kept all of the task's tests green. The elapsed-time assertion
    /// pins the park positively, not merely by the absence of a phrase.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn watch_parks_on_an_unknown_session_and_admits_it_could_not_check() {
        // No daemon, and an id no handle in this process knows: the headless
        // "watching something that is not one of my background children" row.
        crate::workspace_services::set_for_tests(None);
        let c = client();
        let target_id = unique_id("never-seen");

        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
            "session_ids": [target_id], "timeout_s": 1
        }))
        .unwrap();
        let started = std::time::Instant::now();
        let result = c
            .call_tool(
                "workspace_watch",
                Some(args),
                test_meta(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let elapsed = started.elapsed();
        let text = result.content[0].as_text().unwrap().text.clone();
        crate::workspace_services::clear_test_override();

        assert_ne!(result.is_error, Some(true), "a timeout is not a tool error");
        assert!(
            !text.contains("already idle"),
            "an UNKNOWN session must never be reported idle: {text}"
        );
        assert!(text.contains("Still running"), "got: {text}");
        assert!(
            text.contains("No BioRouter daemon is attached"),
            "the report must admit liveness was unverifiable: {text}"
        );
        assert!(
            elapsed >= std::time::Duration::from_millis(900),
            "it must actually park for the bound, not short-circuit: {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn watch_wakes_on_a_terminal_bus_event() {
        use crate::session_events::{self, SessionBusEvent};
        let c = client();
        // `unique_id`, NOT `session_manager::create_session`. Session ids are
        // `YYYYMMDD_N` counted within one SQLite file, and every `client()` here
        // gets a fresh temp DB — so the first session of EVERY test in this
        // binary is `<today>_1`, while `session_events` is a process-global bus
        // keyed by that id. Two such tests then publish onto each other's bus:
        // this test's `TurnFinished{reason:"stop"}` was arriving inside
        // `watch_timeout_is_not_an_error_…`, turning its timeout into a
        // completion. Production mints ids from one manager per process, so the
        // collision is a fixture artifact — and `handle_watch` never consults
        // the session manager at all, it only subscribes by id, so a plain
        // unique id exercises exactly the same path.
        let target_id = unique_id("watched");

        // Make the session look busy to the watcher, then finish it.
        session_events::publish(
            &target_id,
            SessionBusEvent::TurnStarted {
                turn_id: "turn-w".into(),
            },
        );
        let sid = target_id.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            session_events::publish(
                &sid,
                SessionBusEvent::TurnFinished {
                    reason: "stop".into(),
                    token_state: None,
                },
            );
        });

        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
            "session_ids": [target_id], "timeout_s": 20, "assume_running": true
        }))
        .unwrap();
        let result = c
            .call_tool(
                "workspace_watch",
                Some(args),
                test_meta(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let text = result.content[0].as_text().unwrap().text.clone();
        assert!(
            text.contains("stop"),
            "the completion reason is reported: {text}"
        );
    }

    #[tokio::test]
    async fn watch_timeout_is_not_an_error_and_names_what_is_still_running() {
        use crate::session_events::{self, SessionBusEvent};
        let c = client();
        // Unique id rather than a minted session id — see
        // `watch_wakes_on_a_terminal_bus_event` for why `<today>_1` is shared by
        // every test in this binary and what that cost this test specifically.
        let target_id = unique_id("slow");
        session_events::publish(
            &target_id,
            SessionBusEvent::TurnStarted {
                turn_id: "turn-slow".into(),
            },
        );

        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
            "session_ids": [target_id], "timeout_s": 1, "assume_running": true
        }))
        .unwrap();
        let result = c
            .call_tool(
                "workspace_watch",
                Some(args),
                test_meta(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_ne!(result.is_error, Some(true), "a timeout is not a tool error");
        let text = result.content[0].as_text().unwrap().text.clone();
        // Capital S: `str::contains` is case-sensitive and the report says
        // "Still running:" in both of its branches.
        assert!(text.contains("Still running"), "got: {text}");
        assert!(text.contains(&target_id));
    }

    #[tokio::test]
    async fn watch_rejects_an_empty_or_oversized_id_list() {
        let c = client();
        for ids in [serde_json::json!([]), serde_json::json!(vec!["s"; 33])] {
            let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
                "session_ids": ids
            }))
            .unwrap();
            let result = c
                .call_tool(
                    "workspace_watch",
                    Some(args),
                    test_meta(),
                    CancellationToken::new(),
                )
                .await
                .unwrap();
            assert_eq!(result.is_error, Some(true));
        }
    }

    /// Tasks 12-17 together register the six headless tools.
    ///
    /// **Membership, not equality.** `get_tools()` keeps growing after this
    /// task: Task 18 appends `subagent` and Task 24 appends `workspace_open`,
    /// and BOTH re-run this test under `--lib agents::workspace_extension` with
    /// "Expected: PASS". An `assert_eq!` on the sorted vector here would go red
    /// at Task 18 Step 6 and stay red. The plan holds exactly ONE exact-surface
    /// assertion, in Task 24 — the last task that touches `get_tools()`.
    #[tokio::test]
    async fn advertises_every_slice1_tool() {
        let c = client();
        let tools = c
            .list_tools(None, CancellationToken::new())
            .await
            .unwrap()
            .tools;
        let names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
        for expected in [
            "workspace_close",
            "workspace_list",
            "workspace_read_conversation",
            "workspace_send_prompt",
            "workspace_set_tools",
            "workspace_watch",
        ] {
            assert!(
                names.iter().any(|n| n == expected),
                "the Slice-1 surface must include {expected}: {names:?}"
            );
        }
        assert!(
            !names.iter().any(|n| n == "workspace_open"),
            "workspace_open is Phase 2 (Task 24): {names:?}"
        );
        // And every one of the six is named in the instruction block (§6).
        let info = c.get_info().unwrap();
        let instructions = info.instructions.as_deref().unwrap();
        for name in &names {
            assert!(
                instructions.contains(name.as_str()),
                "instructions omit {name}"
            );
        }
        assert!(
            !instructions.contains("workspace_open"),
            "not advertised until Task 24"
        );
        assert!(instructions.len() <= 2500, "injection budget (§6)");
    }
}
