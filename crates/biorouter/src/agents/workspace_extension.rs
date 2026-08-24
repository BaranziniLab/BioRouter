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
/// `workspace_open` was the live case: it is Phase 2 (Task 24), so Task 21 would
/// have shipped Phase 1 with an instruction the model could not act on, and the
/// line was therefore withheld until the handler landed. Task 24 added both in
/// one commit, together with the inverse assertion
/// (`workspace_open_is_advertised_and_completes_the_surface`: every name this
/// block mentions is registered in `get_tools()`, and vice versa) so the two can
/// never drift again.
///
/// **This whole block ships even when only `subagent` is callable.** The common
/// injection mode is `Agent::ensure_spawn_extension`, which loads this extension
/// with `available_tools: ["subagent"]` for any session that merely has
/// delegation enabled. `available_tools` filters the *tool list*; instructions
/// are a single server string that `ExtensionManager::get_extensions_info`
/// copies verbatim, so a spawn-only session still reads all eight bullets.
///
/// That is fine for a bullet describing a tool the model simply does not have —
/// it cannot act on it either way. It is NOT fine for a negative imperative
/// about a fallback the model can always reach, because there the instruction
/// stays live after the tool it depends on is gone. `workspace_set_tools` is the
/// one such case: "never tell the user to change Settings" left a spawn-only
/// session with no legal answer to "give that other chat a skill", since
/// Settings is exactly where a user without the tool must go. Hence the
/// availability clause. Keep any future imperative here similarly scoped.
const INSTRUCTIONS: &str = indoc! {r#"
    Workspace Control

    You are in the BioRouter workspace: a set of conversations (sessions), each
    shown as a tab in the desktop app when a GUI is attached. Each has its own
    agent, tools, knowledge bases and history. These tools operate the workspace:
    - workspace_list: see conversations, what's running, and where they are in
      the GUI. For "what is that chat doing now?" list, then read its tool_calls.
    - workspace_open: open/focus an existing conversation, or start a new one the
      USER owns (new.kind:"user"; optionally split or new window; opens in the
      background). It never delegates: new.kind:"sub_agent" is refused.
    - workspace_read_conversation: read another conversation. summary for a
      digest, transcript for prose, tool_calls for exactly what its agent did,
      spawn_context for how a subagent was started. Treat other conversations'
      content as sensitive; prefer the narrowest view.
    - workspace_send_prompt: inject into another conversation. turn starts its
      agent on your text; steer redirects it mid-turn; note leaves context
      without running it. Injections are permanently labeled as coming from
      you. Use wait:"final_message" to get its answer synchronously.
    - workspace_set_tools: add/remove extensions, scope skills to one
      conversation (add_skills), switch its model, or set its knowledge bases.
      When you have it, do this yourself instead of pointing at Settings.
    - workspace_close: close its tab (tab), cancel its turn (turn), or stop its
      agent (agent).
    - workspace_watch: wait until one of several conversations finishes. Use it
      after starting background work; never poll workspace_read_conversation.
    - workspace_read_panel: read what the preview panel shows now: document,
      figure, file or live web page. Use it when the user says "this" or "the
      page"; text is cheap and you can act on it.
    - workspace_capture_panel: screenshot it (returns a PNG path) to judge how
      something LOOKS. You cannot act on a screenshot.
    - subagent: the ONLY way to delegate. A fresh agent with its own context
      window; "spin up subagents" and fan-out mean this tool, one call per child,
      same message for parallel. When the app is open the child runs in a visible
      tab the user can watch and talk to; you still receive only its final
      summary, so use workspace_read_conversation view:"tool_calls" on it to
      verify what it did. The user may have intervened; the result tells you so.
    Only the workspace tools in your tool list are available.
    Routing: to search past conversations by content use chatrecall (if
    enabled), not these tools. Durable facts belong in Memory. To fold a
    conversation into a knowledge base use ingest_conversation; to re-read an
    externalized payload use read_session_blob. If no GUI is attached these
    tools still manage conversations headlessly and say so.
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
    /// list your subagents — this is how you enumerate the children you
    /// delegated to, foreground or background alike.
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

/// The `new:` half of `workspace_open`'s arguments.
///
/// ⚠ **`kind` is declared first and is required, and that is the whole of issue
/// #111.** `workspace_open { new: { prompt } }` and `subagent` both read as
/// "start a conversation and give it a first instruction", so a model asked to
/// "spin up three sub-agents" reached for whichever it met first — and three
/// times over got an ordinary `SessionType::User` row with a null
/// `parent_session_id`, which History's nesting can never show and
/// `workspace_list { only_subagents }` can never find. The work ran; the
/// sessions were not subagents in the data model.
///
/// The fix is a declaration, not an inference. The caller has to say which of
/// the two things it means, in the same vocabulary `workspace_list` reports as
/// `session_type`, and one of the two answers is a refusal naming the right
/// tool. Nothing is guessed from the prompt: a conversation the *user* owns may
/// legitimately open with one, which is exactly why the prompt cannot be the
/// signal.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
// `kind` is optional in Rust so the handler owns the "you left it out" message
// (serde's own "missing field kind" teaches nothing), and required in the
// SCHEMA so a provider that validates tool arguments refuses the call before it
// costs a round trip. `new_declares_its_kind_in_the_schema` is the guard that
// the two halves stay in step.
#[schemars(extend("required" = ["kind"]))]
struct WorkspaceOpenNew {
    /// **Required**, and closed: `"user"` or `"sub_agent"`. This is the same
    /// vocabulary `workspace_list` reports as `session_type` — a conversation's
    /// kind has ONE set of names in this system, not one per tool.
    ///
    /// - `"user"` — a conversation the **user** owns, exactly like one they
    ///   started themselves: no parent, not your delegate, and it keeps its own
    ///   full tool surface. Use it to put a second piece of work in front of the
    ///   user. A first `prompt` is fine here.
    /// - `"sub_agent"` — delegation, which **this tool refuses**. Only the
    ///   `subagent` tool can create one: it stamps this conversation as the
    ///   child's parent before the child's first turn, applies the subagent
    ///   restrictions and lifecycle, and returns the child's summary to you.
    ///
    /// Anything else — including `scheduled`, `hidden` and `terminal`, which are
    /// not this door's to mint — is refused with the two names above.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    /// Where the new conversation works. **Defaults to your own working
    /// directory** (BR-71 decision 5); pass a different one only when the task
    /// really is somewhere else — the user is told when it differs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    working_dir: Option<String>,
    /// Extension names; same semantics as /agent/start extension_overrides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    extensions: Option<Vec<String>>,
    /// Knowledge bases to activate for the new conversation (issue #45).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    knowledge_bases: Vec<String>,
    /// Which of `knowledge_bases` is the new conversation's **write target** —
    /// where a `kb_write`/`kb_ingest` with no explicit `kb_id` lands. Omit it and
    /// the first base in the list is used, which is what you want unless you
    /// have a reason. Must name one of `knowledge_bases`.
    ///
    /// Post-#45 the primary is a real, validated pointer, not a derived
    /// convenience: a session with bases and no target has KB-less writes that
    /// fail. `workspace_open` therefore always chooses one rather than leaving
    /// the new session in that state by omission.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    primary_knowledge_base: Option<String>,
    /// Optional first user message, run as a detached turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    prompt: Option<String>,
}

/// Issue #111: the ONE crossing between `workspace_open` and delegation, and
/// the only place a `new.kind` is turned into a decision.
///
/// Three properties, each load-bearing:
///
/// * **It parses, it does not string-compare.** The value goes through
///   [`SessionType::from_str`], so this door speaks the vocabulary the store
///   persists and `workspace_list` reports rather than a private copy of it. A
///   rename of the persisted spelling cannot leave a second one stranded here.
/// * **It refuses everything it is not sure of.** `Scheduled`, `Hidden` and
///   `Terminal` parse fine and are still refused: creating those is not this
///   door's job, and a vocabulary that quietly accepts five values while
///   documenting two is the same ambiguity in a new place.
/// * **It never looks at the prompt.** Delegation is recognised because the
///   caller *said* so, not because the request looked like a task. A
///   conversation the user owns may legitimately open with a first prompt, so
///   the prompt carries no information about which of the two this is — and a
///   heuristic that read it would misclassify exactly the sessions the user
///   cares most about.
fn refuse_unless_creatable_kind(kind: Option<&str>) -> Result<(), String> {
    match kind.map(str::parse::<crate::session::session_manager::SessionType>) {
        Some(Ok(crate::session::session_manager::SessionType::User)) => Ok(()),
        Some(Ok(crate::session::session_manager::SessionType::SubAgent)) => {
            Err(DELEGATION_IS_NOT_THIS_TOOL.to_string())
        }
        _ => Err(format!(
            "new.kind is required, and closed: \"user\" (a conversation the USER owns — no \
             parent, not your delegate) or \"sub_agent\" (delegation, which this tool refuses \
             — call `subagent`). It is the same vocabulary workspace_list reports as \
             session_type. Got: {}. Nothing was created.",
            match kind {
                Some(k) => format!("{k:?}"),
                None => "no kind at all".to_string(),
            }
        )),
    }
}

/// The refusal a `kind: "sub_agent"` gets, as a constant because it is the
/// model-facing product of issue #111 and a test asserts the model is told all
/// four things it needs: that this tool cannot, why that is a data-model fact
/// rather than a naming quibble, which tool can, and what to pass if a peer
/// conversation is what was actually wanted.
const DELEGATION_IS_NOT_THIS_TOOL: &str = concat!(
    "workspace_open cannot create a subagent, and this is not a naming quibble: a conversation ",
    "created here belongs to the USER, so it is born with no parent, it is not your delegate, ",
    "and History can never nest it under you. Delegation has its own tool. Call `subagent` with ",
    "`instructions` instead — it creates the child as session_type \"sub_agent\", stamps this ",
    "conversation as its parent before the child's first turn, applies the subagent restrictions ",
    "and lifecycle, opens a tab the user can watch, and returns the child's summary to you. If ",
    "you did mean a conversation for the USER to own rather than a delegate of yours, pass ",
    "kind:\"user\". Nothing was created."
);

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct WorkspacePanelParams {
    /// The conversation whose preview panel to read. Defaults to the caller's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    /// `workspace_read_panel` only: how much text to return (default 20000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_chars: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct WorkspaceOpenParams {
    /// Open/focus an existing conversation. Mutually exclusive with `new`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    /// Start a fresh conversation. Mutually exclusive with `session_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    new: Option<WorkspaceOpenNew>,
    /// "tab" (default) | "split" | "window". Anything else is refused rather
    /// than treated as "tab".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    placement: Option<String>,
    /// Default false: open in the background, never steal the user's composer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    focus: Option<bool>,
}

/// A conversation `workspace_open` just created, as the rest of the call needs
/// it.
struct NewSession {
    session_id: String,
    /// Where it works. Carried out of [`WorkspaceClient::open_new_session`]
    /// because decision 5's "a different directory is never silent" has **two**
    /// channels and the GUI toast is only one of them: headless
    /// (`NullServices::gui_command` errors, and the notify path swallows it) the
    /// tool result is the ONLY channel, and even with a GUI a model that is
    /// never told where it put the session cannot report it to the user. The
    /// directory is model-chosen, unvalidated, and worked in immediately.
    working_dir: std::path::PathBuf,
    /// The user-facing notice, set only when `working_dir` is not the caller's.
    /// Held rather than sent because it is addressed to `session_id` and a
    /// renderer that routes a session's toasts to that session's tab would drop
    /// one that arrived before the tab did — so `handle_open` emits it after
    /// placement.
    notice: Option<String>,
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
///
/// **Empty as of Task 24**, which landed the last placeholder
/// (`workspace_open`). Keep the table (and the invariant test that reads it)
/// rather than deleting them: a later phase that stages another tool ahead of
/// its handler gets the same guard for free, and an empty table asserts the
/// stronger claim that nothing is currently staged.
const PENDING_TOOLS: &[(&str, &str)] = &[];

/// Tool names that once existed and must never reappear in [`INSTRUCTIONS`],
/// checked as plain substrings of the whole block.
///
/// The two structural scans this sits beside both have blind spots, and the
/// gap between them is exactly where a stale routing sentence lives:
///
/// - the `workspace_*` token scan in
///   `advertises_no_tool_whose_handler_is_still_a_placeholder` filters on the
///   `workspace_` prefix, so it catches `workspace_spawn_subagent` and is blind
///   to `subagent_status`;
/// - the `- name:` loop in
///   `workspace_open_is_advertised_and_completes_the_surface` reads bullet
///   *heads* only, so it is blind to every name mentioned in prose.
///
/// A retired name in a routing sentence — "poll subagent_status until it
/// finishes" — therefore passed both. That is not hypothetical: decision 23
/// folded `subagent_status` into `workspace_watch`, and Task 42's probe 6
/// exists to catch a model still trying to poll, by hand, against a real
/// provider. This table makes the instruction half of that a build failure
/// instead, which is the half a unit test can actually own.
///
/// Add a row whenever a tool is renamed or removed. Removing one is only
/// correct if the name is genuinely live again.
///
/// `#[cfg(test)]` because the only consumer is the instruction-scanning test
/// below; ungated, it is dead code in every shipped build and `-D warnings`
/// (which `scripts/clippy-lint.sh` uses) turns that into a hard error. It stays
/// declared *here*, beside the tool table, so the reader who renames a tool
/// meets the rule at the moment it applies.
#[cfg(test)]
const RETIRED_TOOL_NAMES: &[&str] = &["subagent_status", "workspace_spawn_subagent"];

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

    /// `pub` so `agent.rs` can cross-check `WORKSPACE_TOOL_NAMES` (the
    /// BR-71 §5 subagent refusal list) against what is actually advertised,
    /// and so Task 42b's CLI-parity gate in `biorouter-cli` can compare the
    /// advertised tool surface against the capability table. A hand-maintained
    /// mirror of this function is the one place either guard can rot silently.
    pub fn get_tools() -> Vec<Tool> {
        vec![
            Self::tool(
                "workspace_read_panel",
                "Read what the preview panel is currently showing: the rendered \
                 document, figure, file or live web page. **Prefer this over \
                 workspace_capture_panel** — text is cheaper and can be acted \
                 on, where a screenshot can only be looked at. Returns nothing \
                 readable for an image; capture it instead.",
                serde_json::to_value(schema_for!(WorkspacePanelParams)).unwrap(),
                true,
            ),
            Self::tool(
                "workspace_capture_panel",
                "Screenshot the preview panel, saved as a PNG whose path is \
                 returned. Use it to judge how something LOOKS — a figure, a \
                 rendered page, a layout. You cannot act on a screenshot: to \
                 find or change content, use workspace_read_panel.",
                serde_json::to_value(schema_for!(WorkspacePanelParams)).unwrap(),
                true,
            ),
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
                 GUI tab only; the session and any running turn survive. turn: \
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
                 timeout is NEVER an error: the reply lists whatever finished and \
                 whatever is still running, so call it again to keep waiting. The \
                 wait may be shortened to fit the transport carrying this turn — \
                 when it is, the reply says the effective wait and the one you \
                 asked for.",
                serde_json::to_value(schema_for!(WorkspaceWatchParams)).unwrap(),
                true,
            ),
            // BR-71 decisions 20/22: the ONE spawn tool, under its existing
            // name. Dispatch is intercepted by the agent loop (it needs the
            // parent's TaskConfig — provider, extensions, working dir — which
            // only `Agent::dispatch_tool_call` has); this advertisement is what
            // puts it in the model's tool list. `&[]` = the generic description;
            // Task 19 restores the sub-workflow-enriched one.
            crate::agents::subagent_tool::create_subagent_tool(&[]),
            Self::tool(
                "workspace_open",
                "Open or focus a conversation the USER owns. Pass session_id to \
                 bring an existing one up, or new to start a fresh one — then \
                 new.kind is REQUIRED and must be \"user\" (its working directory \
                 defaults to yours; extensions, knowledge bases and a first \
                 prompt are optional). This tool does NOT delegate: it cannot \
                 create a subagent, and new.kind:\"sub_agent\" is refused. To hand \
                 work to an agent of your own — however the request is phrased, \
                 including \"spin up subagents\" — call `subagent`, the only tool \
                 that stamps this conversation as the child's parent. placement: \
                 tab (default), split or window; focus defaults to false so the \
                 user's composer is never stolen. Headless, the session is still \
                 created and the result says no tab was opened.",
                serde_json::to_value(schema_for!(WorkspaceOpenParams)).unwrap(),
                false,
            ),
        ]
    }

    /// **Design §7 column C, as one call**: may a caller admitted on this
    /// capability reach the conversation it just named at all?
    ///
    /// Issue #56. Every `workspace_*` handler that reads another conversation's
    /// content asks this *first*. The predicate is
    /// [`privacy::visibility::may_read`] — not a hand-rolled
    /// comparison of the stored tier against the caller's, which is how one
    /// table becomes seven slightly-different tables — and which the execution
    /// plan's Task 21 Step 5 gate greps the tree for.
    ///
    /// Three properties, each load-bearing:
    ///
    /// * **The master opt-out is read off the same sample that carried the
    ///   tier** (`cap.enforced()`), never re-derived here. This runs inside the
    ///   driven future, past the dispatch semaphore, where the provider may
    ///   already be a different one — the whole reason [`CallCapability`]
    ///   exists. (The toggle's own function name is deliberately not spelled in
    ///   this file: a Step 5 gate counts that token tree-wide and must see
    ///   exactly one, inside `CallCapability`.)
    ///
    /// * **A caller that may read a private conversation never touches the
    ///   store.** That is not an optimisation. It is what leaves the handler's
    ///   own honest errors — "no such session" — intact for the caller entitled
    ///   to them, so that the one sentence below is ambiguous *only* for the
    ///   caller who must not tell the two apart.
    ///
    /// * **`Err` and "could not read the row" are the same answer.** §14.4 /
    ///   R10, and the same rule `routes::session_reach` states for HTTP: an
    ///   unauthorized caller must not learn from the refusal whether the
    ///   conversation exists. §7 already OMITS private rows from
    ///   `workspace_list` because a session's existence and its LLM-generated
    ///   title are content; a refusal that distinguished them would hand those
    ///   rows straight back, one id at a time.
    ///
    /// The row is read **metadata-only** (`with_messages: false`) for the reason
    /// [`session_reach::target_tier`] gives: resolving the tier must never
    /// itself be the way to load the transcript the gate is about to refuse.
    ///
    /// [`privacy::visibility::may_read`]: crate::privacy::visibility::may_read
    /// [`CallCapability`]: crate::privacy::CallCapability
    /// [`session_reach::target_tier`]: https://github.com/BaranziniLab/biorouter
    /// ⚠ **The body moved, the behaviour did not.** When
    /// `platform__manage_schedule`'s `session_content` action turned out to be a
    /// second handler reading any named transcript, the choice was one adapter
    /// with two callers or two adapters that agree until they do not. The
    /// decision itself — resolve metadata-only, ask [`may_read`], answer private
    /// and absent in one sentence — now lives at
    /// [`privacy::visibility::refuse_unless_readable`], which this delegates to.
    ///
    /// [`may_read`]: crate::privacy::visibility::may_read
    /// [`privacy::visibility::refuse_unless_readable`]: crate::privacy::visibility::refuse_unless_readable
    async fn refuse_unless_visible(
        &self,
        cap: crate::privacy::CallCapability,
        target_session_id: &str,
    ) -> Result<(), String> {
        crate::privacy::visibility::refuse_unless_readable(
            cap,
            &self.context.session_manager,
            target_session_id,
        )
        .await
    }

    async fn handle_list(
        &self,
        _caller_session_id: &str,
        cap: crate::privacy::CallCapability,
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
        // skills, installs the Soul KB + a 3 AM schedule) — so an unsandboxed run
        // writes into the developer's real `~/.config/biorouter`. This is no
        // longer left to the caller: `crate::test_sandbox`'s `ctor` pins
        // `BIOROUTER_PATH_ROOT` under a temp dir before any test in the lib
        // binary runs, and an outer `BIOROUTER_PATH_ROOT` (the Task 33 gate's)
        // still wins. The same hazard applies to Tasks 14, 15, 17 and 33, all of
        // which are covered by that one ctor.
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
                // Issue #56, design §7 row 1: a private conversation is **∅ —
                // omitted** from a public caller's list, not redacted. A row
                // carries the session's `name`, which in this product is
                // LLM-generated from the conversation (§11.4: "the one that
                // leaks most per byte"), and its `working_dir`, which routinely
                // names a cohort or a study; `extensions` re-exposes by name the
                // very private extensions Gate E hides from this model's own
                // tool list. Omission is also what removes the temptation to
                // then call `workspace_read_conversation` on the id.
                //
                // ⚠ **Before `matched += 1`, deliberately.** An omitted row must
                // not be counted in `total_matching` or flip `has_more`, or the
                // paging metadata becomes exactly the existence oracle the
                // omission exists to close — "page 2 of 3, showing 0 rows" says
                // there are private conversations and how many.
                //
                // No extra query: `SessionSummary.privacy_tier` was added by
                // this issue for the sidebar badge and is already in hand.
                if cap.enforced()
                    && !crate::privacy::visibility::appears_in_list(cap.tier(), s.privacy_tier)
                {
                    continue;
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
                rows.push(
                    self.list_session_row(&s, running, gui_placement, services.as_ref())
                        .await,
                );
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

    /// One row of the `workspace_list` payload.
    ///
    /// Split out of [`Self::handle_list`]'s scan loop: everything the summary
    /// row already carries is copied straight through, and the two fields §4.1
    /// requires that it does NOT carry are read here, per included row.
    async fn list_session_row(
        &self,
        s: &crate::session::session_manager::SessionSummary,
        running: bool,
        gui_placement: Option<serde_json::Value>,
        services: Option<&std::sync::Arc<dyn workspace_services::WorkspaceServices>>,
    ) -> serde_json::Value {
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
                Ok(full) => EnabledExtensionsState::from_extension_data(&full.extension_data)
                    .map(|st| st.extensions.iter().map(|e| e.name()).collect())
                    .unwrap_or_else(|| {
                        // No session-specific state → global config, the
                        // exact fallback GET /sessions/{id}/extensions
                        // performs (`from_extension_data` returns Option).
                        crate::config::get_enabled_extensions()
                            .iter()
                            .map(|e| e.name())
                            .collect()
                    }),
                Err(_) => Vec::new(),
            };
        // Post-#45: ONE call returning set + write target together
        // (Task 9). `primary_kb` is on the row because a model that can
        // SET a write target and cannot READ it back will thrash — it
        // has no way to tell "already correct" from "not applied".
        let kbs = services
            .map(|svc| svc.knowledge_selection(&s.id))
            .unwrap_or_default();
        json!({
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
        })
    }

    async fn handle_read_conversation(
        &self,
        caller_session_id: &str,
        cap: crate::privacy::CallCapability,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let args: WorkspaceReadParams = parse_args(arguments)?;
        let view = args.view.as_deref().unwrap_or("transcript");
        let max_chars = args.max_chars.unwrap_or(20_000).min(200_000);

        // ⚠ **Issue #56, design §7 row 2 — the release blocker.** This handler
        // used to check `session_type == Hidden` and nothing else, so a chat
        // running on a PUBLIC model read any named private conversation's whole
        // transcript through a tool call. Not through the filesystem: through
        // BioRouter's own API, which is precisely what the tier gates exist to
        // stop.
        //
        // FIRST, and before the `with_messages: true` load below, so resolving
        // the tier is not itself the way to load the transcript being refused —
        // the ordering rule `routes::session_reach` states for the five gated
        // HTTP routes, with a tool call as its subject.
        self.refuse_unless_visible(cap, &args.session_id).await?;

        let session = self
            .context
            .session_manager
            .get_session(&args.session_id, true)
            .await
            .map_err(|e| format!("failed to load session: {e}"))?;

        // §5 "no covert reads": Hidden sessions honor the same visibility rules
        // as the session list. The read itself is auditable — it IS a tool call
        // in the caller's transcript.
        //
        // ⚠ **A different rule, not a substitute for the one above, and it stays
        // below it.** Hidden is a session TYPE (a machine-internal conversation);
        // private is a CLASSIFICATION. A private-capability caller is still
        // refused a hidden session here, and a public caller naming a private
        // *hidden* one is refused above — so it never learns that a hidden
        // session with that id exists.
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
                "{cut}\n… [clipped at {max_chars} chars; narrow with `last` or \
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

    /// `workspace_open` (§4.1): open/focus an existing conversation, or start a
    /// fresh one and (when a GUI is attached) give it a tab.
    ///
    /// **It never retargets an existing session's working directory**, and that
    /// is load-bearing rather than incidental. The new-session path sets the
    /// directory at CREATION (`create_session(working_dir, …)`, exactly as
    /// `POST /agent/start` does), so it takes neither of the two sanctioned
    /// post-creation writers (`try_update_working_dir_if_empty` /
    /// `force_update_working_dir_unguarded`) and can never race the #44 turn
    /// guard that `PATCH /agent/update_working_dir` and the GUI's `DirSwitcher`
    /// share. A future revision that lets this tool move an *existing*
    /// conversation's directory inherits the whole #44 problem and must go
    /// through `try_update_working_dir_if_empty` plus the route's turn guard.
    /// `workspace_read_panel` and `workspace_capture_panel`.
    ///
    /// Two channels with an explicit division of labour, and the tool
    /// descriptions say so out loud: **text to act on, pixels to judge by**.
    /// A structured read is both cheaper and actionable, where a screenshot can
    /// only be looked at — so the read is the default and the capture is for
    /// "does this look right", which is the one question text cannot answer.
    ///
    /// Both are reads of a conversation's screen, so they take the same privacy
    /// gate as `workspace_read_conversation`: a public-capability caller must
    /// not read a private session's panel.
    async fn handle_panel(
        &self,
        tool: &str,
        caller_session_id: &str,
        cap: crate::privacy::CallCapability,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let args: WorkspacePanelParams = parse_args(arguments)?;
        let session_id = args
            .session_id
            .unwrap_or_else(|| caller_session_id.to_string());

        // Same gate, and deliberately BEFORE reaching the GUI: resolving what
        // is on a private session's screen must not itself be the way to see it.
        self.refuse_unless_visible(cap, &session_id).await?;

        let mut frame = serde_json::json!({
            "type": "workspace",
            "cmd": if tool == "workspace_read_panel" { "read_panel" } else { "capture_panel" },
            "session_id": session_id,
        });
        if let Some(max_chars) = args.max_chars {
            frame["max_chars"] = serde_json::json!(max_chars);
        }

        // Routed to the window holding that session rather than to whichever
        // window happens to be focused — the panel belongs to a tab, and
        // focus-based routing answers about the wrong screen (issue #78).
        let services = workspace_services::get();
        let services = services.as_ref().ok_or_else(|| {
            "no GUI is attached, so there is no preview panel to read".to_string()
        })?;
        let reply = services
            .gui_command_near(frame, true, &session_id)
            .await
            .map_err(|err| format!("could not reach the GUI: {err}"))?;

        Ok(vec![Content::text(
            serde_json::to_string_pretty(&reply).unwrap_or_else(|_| reply.to_string()),
        )])
    }

    async fn handle_open(
        &self,
        caller_session_id: &str,
        cap: crate::privacy::CallCapability,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let args: WorkspaceOpenParams = parse_args(arguments)?;
        // A CLOSED vocabulary, checked before anything is created. The GUI half
        // below branches on `placement == "window"` and forwards everything else
        // verbatim as `open_tab`'s `placement`, so an unvalidated typo
        // ("windows", "Window") is not an error the renderer reports — it is a
        // tab, silently, which is the one outcome the caller did not ask for.
        let placement = match args.placement.as_deref().unwrap_or("tab") {
            valid @ ("tab" | "split" | "window") => valid.to_string(),
            other => {
                return Err(format!(
                    "unknown placement {other:?}: use \"tab\" (default), \"split\" or \"window\""
                ));
            }
        };
        let focus = args.focus.unwrap_or(false);
        let services = workspace_services::get();

        let (session_id, created) = match (args.session_id, args.new) {
            (Some(_), Some(_)) => {
                return Err("pass either session_id OR new, not both".into());
            }
            (None, None) => {
                return Err("pass session_id (open existing) or new (start fresh)".into());
            }
            (Some(session_id), None) => {
                // Issue #56, design §7 row 4: `workspace_open` on an EXISTING
                // session is classed a **read** (✗ at C=Pub, T=Priv) — it puts
                // that conversation in front of the user, and its GUI answer
                // reports whether the id was real.
                //
                // ⚠ **Before the existence check below, not after.** "No such
                // session" and "that one is private" have to be one answer, and
                // this arm is the reason: for a caller the gate refuses, the
                // check below is never reached, so the only sentence it can ever
                // produce is the ambiguous one. (For a caller entitled to the
                // difference the gate is inert and the honest error survives.)
                self.refuse_unless_visible(cap, &session_id).await?;
                // Validate it exists so the GUI never gets a dangling frame.
                self.context
                    .session_manager
                    .get_session(&session_id, false)
                    .await
                    .map_err(|e| format!("no such session: {e}"))?;
                (session_id, None)
            }
            (None, Some(new)) => {
                let created = self.open_new_session(caller_session_id, cap, new).await?;
                (created.session_id.clone(), Some(created))
            }
        };

        let placed = self
            .place_in_gui(
                &session_id,
                &placement,
                focus,
                services.as_ref(),
                created.as_ref(),
            )
            .await;

        // Decision 5's user-facing half, AFTER the tab exists: the toast names
        // `session_id`, and a renderer that routes a session's toasts to that
        // session's tab has nothing to attach one to until `open_tab` has been
        // applied. Emitting it inside `open_new_session` — before placement —
        // baked that race into the frame ordering for Task 26 to inherit.
        if let Some(notice) = created.and_then(|c| c.notice) {
            self.notify_target(&session_id, notice).await;
        }
        placed
    }

    /// The `new:` half of [`Self::handle_open`]: create the session, work out
    /// whether its directory needs surfacing, and optionally seed it with a
    /// first detached turn. Split out of `handle_open` for the `too_many_lines`
    /// baseline, not because it has an independent contract.
    ///
    /// Returns the directory and the notice alongside the id because decision
    /// 5's disclosure has two channels, and neither belongs here: the tool
    /// result is written by `place_in_gui`, and the toast has to wait for the
    /// tab. See [`NewSession`].
    async fn open_new_session(
        &self,
        caller_session_id: &str,
        cap: crate::privacy::CallCapability,
        new: WorkspaceOpenNew,
    ) -> Result<NewSession, String> {
        // Issue #111, and FIRST — ahead of the extension gate, the daemon lookup
        // and `create_session` — because a refusal that has already minted a row
        // produces the exact outcome the refusal exists to prevent: an
        // unparented conversation nobody asked for. It sits inside the one
        // function that creates a session, rather than beside `placement`'s
        // check in the caller, so it cannot be bypassed by a future second
        // caller of this function.
        refuse_unless_creatable_kind(new.kind.as_deref())?;
        // Issue #56, finding 4: the second ungated extension-ENABLE door. Before
        // the daemon lookup and long before `create_session`, so a refusal
        // cannot leave a half-built conversation behind.
        Self::refuse_gated_new_session_extensions(cap, new.extensions.as_ref())?;
        let services = workspace_services::get()
            .ok_or("starting a new session requires the BioRouter daemon")?;
        // Decision 5: the working dir DEFAULTS to the caller's. A different
        // directory is allowed — an agent that has just been asked about another
        // project should be able to open it — but it is never silent: the tool
        // result names it and the GUI toast shows it.
        let caller_dir = self
            .context
            .session_manager
            .get_session(caller_session_id, false)
            .await
            .map(|s| s.working_dir)
            .ok();
        let working_dir = match new.working_dir.as_deref() {
            Some(dir) => std::path::PathBuf::from(dir),
            None => caller_dir.clone().ok_or(
                "no working_dir given and the calling session's directory could not be \
                 read; pass working_dir explicitly",
            )?,
        };
        let differs = caller_dir
            .as_ref()
            .is_some_and(|caller| caller != &working_dir);
        // Post-#45: the write target is chosen explicitly. `Auto` on a brand-new
        // session resolves to "pin the first id", because a fresh session has no
        // primary of its own to keep.
        let kb_primary = match new.primary_knowledge_base.as_deref() {
            None => workspace_services::KbPrimaryChoice::Auto,
            Some(id) => workspace_services::KbPrimaryChoice::Set(id.to_string()),
        };
        let session_id = services
            .start_session(
                working_dir.clone(),
                new.extensions,
                new.knowledge_bases,
                kb_primary,
            )
            .await?;
        let notice = differs.then(|| {
            format!(
                "An agent started a new conversation in {} (not this conversation's folder).",
                working_dir.display()
            )
        });
        if let Some(prompt) = new.prompt {
            let provenance = self.caller_provenance(caller_session_id).await;
            let message = crate::conversation::message::Message::user()
                .with_text(prompt)
                .with_provenance(provenance);
            services.start_detached_turn(&session_id, message).await?;
        }
        Ok(NewSession {
            session_id,
            working_dir,
            notice,
        })
    }

    /// The GUI half of [`Self::handle_open`] (§4.3): `open_tab` relies on the
    /// reducer's dedupe/adopt rules; "split" maps to `moveTabToGroup`; "window"
    /// is its OWN frame (`open_window`, per the §4.3 vocabulary) which the
    /// renderer relays to the create-chat-window IPC. The renderer answers via
    /// `workspace_result` so a refused split (MAX_GROUPS) comes back as a clear
    /// message, not silence. With no GUI attached the session still exists and
    /// the result says exactly that rather than claiming a tab.
    ///
    /// ⚠ **`open_tab` on a conversation that already has a tab opens nothing.**
    /// The reducer's `openTab` dedupes by session id, and the planner answers
    /// `ok:true, detail:"opened"` either way — so relaying that answer verbatim
    /// told the model a tab had been opened when the tab was already there and,
    /// with `focus:false`, nothing at all had happened. The daemon can tell the
    /// two apart: [`gui_tab_for`] over the window's own layout echo is the same
    /// evidence `workspace_list` reports a tab from. §4.3 already has the word
    /// for what should happen instead — `activate_tab`, which until now was a
    /// registered command with no emitter — so this is where it is sent.
    async fn place_in_gui(
        &self,
        session_id: &str,
        placement: &str,
        focus: bool,
        services: Option<&std::sync::Arc<dyn workspace_services::WorkspaceServices>>,
        created: Option<&NewSession>,
    ) -> Result<Vec<Content>, String> {
        // Decision 5, the model-facing half: when this call CREATED the session,
        // every result says where it works.
        let dir_note = created
            .map(|c| format!(" Working directory: {}.", c.working_dir.display()))
            .unwrap_or_default();
        match services {
            Some(s) if s.gui_attached() => {
                // Only a plain `placement:"tab"` on an EXISTING session can be a
                // no-op. A session this call just created cannot already have a
                // tab; `"window"` asks for a new surface whatever the tab state
                // is; and `"split"` on a tab that already exists still MOVES it
                // into a new pane (`moveTabToGroup`), so "nothing moved" would
                // be false — the split is a real layout change and the renderer
                // reports it in `detail` ("opened in split").
                let had_tab = created.is_none()
                    && placement == "tab"
                    && gui_tab_for(s.layout_snapshot().as_ref(), session_id).is_some();
                let outcome = match (had_tab, focus) {
                    (false, _) => TabOutcome::Opened,
                    (true, false) => TabOutcome::AlreadyOpen,
                    (true, true) => TabOutcome::Focused,
                };
                let open_frame = Self::placement_frame(session_id, placement, focus, outcome);
                // §8.1 / decision 7. Read ONCE, and used for both halves: the
                // frame the GUI gets and the sentence the model gets must agree,
                // or the model reports a tab the user cannot see.
                let announce_only = announce_only_enabled();
                let frame = apply_focus_etiquette(open_frame, announce_only);
                let result = match s.gui_command(frame, true).await {
                    Ok(result) => result,
                    // The session is already committed — extensions, knowledge
                    // grants and any seeded first turn — so failing the call
                    // here would leave the model holding an orphan whose id it
                    // was never told, and it would create another. Report both
                    // halves instead: it exists, and the GUI did not place it.
                    Err(e) if created.is_some() => {
                        return Ok(vec![Content::text(format!(
                            "Session {session_id} was created but the GUI did not place it \
                             ({e}).{dir_note} It exists and can be reached with \
                             workspace_send_prompt. Do NOT create another."
                        ))]);
                    }
                    // Nothing was created, so there is nothing to orphan.
                    Err(e) => return Err(e),
                };
                let (outcome, result) = Self::repair_stale_activation(
                    s,
                    // The frame the repair would send: the create frame this
                    // call would have sent had the echo not claimed a tab.
                    || Self::placement_frame(session_id, placement, focus, TabOutcome::Opened),
                    announce_only,
                    outcome,
                    result,
                )
                .await;
                // `open_result_text` is pure, so decision 5's directory note is
                // appended rather than interleaved. It therefore survives on
                // BOTH arms — the announce-only sentence still names where the
                // session works.
                Ok(vec![Content::text(format!(
                    "{}{dir_note}",
                    open_result_text(
                        session_id,
                        placement,
                        focus,
                        announce_only,
                        outcome,
                        &result
                    )
                ))])
            }
            _ => Ok(vec![Content::text(format!(
                "Session {session_id} ready (gui_attached: false, no tab opened; \
                 the session exists headlessly).{dir_note}"
            ))]),
        }
    }

    /// Which §4.3 frame says what is actually about to happen. Split out of
    /// [`Self::place_in_gui`] so the vocabulary choice is one expression a test
    /// can read, rather than three branches wrapped around a round trip.
    fn placement_frame(
        session_id: &str,
        placement: &str,
        focus: bool,
        outcome: TabOutcome,
    ) -> serde_json::Value {
        if placement == "window" {
            return json!({
                "type": "workspace", "cmd": "open_window",
                "session_id": session_id,
            });
        }
        if outcome == TabOutcome::Focused {
            // The tab exists and the caller asked for focus: the honest command
            // is the one that moves the view, not the one that allocates a tab.
            return json!({
                "type": "workspace", "cmd": "activate_tab",
                "session_id": session_id,
            });
        }
        // `AlreadyOpen` still sends `open_tab`: it is a dedupe no-op in the
        // renderer, and sending it is what repairs a layout echo that has gone
        // stale in the other direction (the tab closed since the last echo).
        // Only the SENTENCE changes — see [`open_result_text`].
        json!({
            "type": "workspace", "cmd": "open_tab",
            "session_id": session_id,
            "placement": placement,
            "focus": focus,
        })
    }

    /// The echo can be wrong about the tab in two ways, and the renderer reports
    /// both the same way — `planWorkspaceCommand`'s `activate_tab` arm refuses
    /// with `ok:false, detail:"session has no tab"`:
    ///
    ///  * it is debounced, so it can still name a tab the user closed a moment
    ///    ago;
    ///  * it is MERGED ACROSS WINDOWS, while `gui_command` addresses one
    ///    (`focused_or_recent`). A tab that exists in another window is not a
    ///    tab the frame's recipient can activate.
    ///
    /// Without this repair either case would turn a `workspace_open` that used
    /// to open a tab into a refusal — a regression caused by the fix, not by the
    /// user. With it, the second case does what the caller meant: the focused
    /// window gets the tab, and the result says "opened", because it was.
    ///
    /// Announce-only is exempt: there the frame was deliberately downgraded to a
    /// notification, so a `ok:false` means the *notification* was refused and
    /// re-sending an `open_tab` would defeat the setting.
    async fn repair_stale_activation(
        services: &std::sync::Arc<dyn workspace_services::WorkspaceServices>,
        create_frame: impl FnOnce() -> serde_json::Value,
        announce_only: bool,
        outcome: TabOutcome,
        result: serde_json::Value,
    ) -> (TabOutcome, serde_json::Value) {
        if announce_only || outcome != TabOutcome::Focused || announcement_delivered(&result) {
            return (outcome, result);
        }
        match services.gui_command(create_frame(), true).await {
            Ok(retry) => (TabOutcome::Opened, retry),
            // Keep the first answer: it is the one that names a real refusal,
            // and reporting the transport failure of the retry instead would
            // hide it.
            Err(_) => (outcome, result),
        }
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
        cap: crate::privacy::CallCapability,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let args: WorkspaceSendPromptParams = parse_args(arguments)?;
        if args.session_id == caller_session_id {
            return Err(
                "refusing to inject into your own session; just continue the conversation".into(),
            );
        }
        if args.text.trim().is_empty() {
            return Err("text must not be empty".into());
        }
        // Issue #56, design §7 row 5 (`workspace_send_prompt` = ✗ at C=Pub,
        // T=Priv, under every lineage). It is on this list as a **reader** as
        // well as a writer, and that is the half easy to miss: `mode:"turn"`
        // with `wait:"final_message"` parks on the target's turn and returns
        // its final assistant message verbatim — a private conversation's
        // content, arriving through a tool whose name says "send".
        //
        // Placed after the two pure argument checks above (neither touches the
        // store, and neither can say anything about the target) and before
        // `caller_provenance`, which is this handler's first store read.
        //
        // ⚠ This enforces VIS only. §7's write row is `may_write` — VIS **and**
        // lineage ∈ {self, child} — and the lineage half is not implemented
        // anywhere in this change, so a public caller may still steer a public
        // sibling it did not spawn (column B, `✗ R6`).
        self.refuse_unless_visible(cap, &args.session_id).await?;
        let provenance = self.caller_provenance(caller_session_id).await;
        let services = workspace_services::get();

        match args.mode.as_str() {
            "note" => self.send_prompt_note(args, provenance).await,
            "steer" => {
                self.send_prompt_steer(caller_session_id, args, provenance, services)
                    .await
            }
            "turn" => {
                self.send_prompt_turn(caller_session_id, args, provenance, services)
                    .await
            }
            other => Err(format!("unknown mode '{other}' (turn | steer | note)")),
        }
    }

    /// `mode:"note"` — leave context on the target without running it.
    async fn send_prompt_note(
        &self,
        args: WorkspaceSendPromptParams,
        provenance: crate::conversation::message::MessageProvenance,
    ) -> Result<Vec<Content>, String> {
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

    /// `mode:"steer"` — redirect the turn the target is already running.
    async fn send_prompt_steer(
        &self,
        caller_session_id: &str,
        args: WorkspaceSendPromptParams,
        provenance: crate::conversation::message::MessageProvenance,
        services: Option<std::sync::Arc<dyn workspace_services::WorkspaceServices>>,
    ) -> Result<Vec<Content>, String> {
        let services = services
            .ok_or("steer requires the BioRouter daemon (no workspace services installed)")?;
        if !services.is_turn_active(&args.session_id) {
            return Err("target session has no turn in flight; use mode:\"turn\" instead".into());
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
                    "steer refused for session {}: {refused}; use mode:\"turn\" instead",
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

    /// `mode:"turn"` — start the target's agent on the text, then either
    /// detach or park for its final message.
    async fn send_prompt_turn(
        &self,
        caller_session_id: &str,
        args: WorkspaceSendPromptParams,
        provenance: crate::conversation::message::MessageProvenance,
        services: Option<std::sync::Arc<dyn workspace_services::WorkspaceServices>>,
    ) -> Result<Vec<Content>, String> {
        use crate::session_events;

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
        if !services.gui_attached() && self.target_mode_requires_approval(&args.session_id).await {
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
        let timeout = std::time::Duration::from_secs(args.timeout_s.unwrap_or(120).min(600));
        let waited = tokio::time::timeout(timeout, follower.run()).await;

        match waited {
            Ok(Ok(TurnOutcome::Finished {
                reason,
                last_assistant,
            })) => Ok(vec![Content::text(format!(
                "Turn {turn_id} finished ({reason}). Final message:\n\n{}",
                last_assistant.unwrap_or_else(|| "<no assistant text>".into())
            ))]),
            Ok(Ok(TurnOutcome::Failed(e))) => Err(format!("turn {turn_id} ended in error: {e}")),
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

    /// BR-71 `workspace_set_tools`: the one place an agent changes *what another
    /// conversation can use* — extensions, session-scoped skills,
    /// provider+model, and knowledge bases.
    async fn handle_set_tools(
        &self,
        caller_session_id: &str,
        cap: crate::privacy::CallCapability,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let args: WorkspaceSetToolsParams = parse_args(arguments)?;

        // ⚠ **A conversation may not re-tool ITSELF through this door.**
        //
        // This is a cross-conversation tool: every other guard below asks what
        // the caller may do to ANOTHER chat. Self-targeting was never the point,
        // and once Workspace became a default-on capability it became an
        // escalation: `apply_tool_changes` adds extensions with
        // `agent.add_extension`, which stamps `ExtensionOrigin::Explicit`, and
        // `has_non_injected_extensions` counts Explicit entries. So an agent
        // could add any default-off public capability to its own session and
        // thereby satisfy condition 5 of the delegation gate on the next tool
        // listing.
        //
        // That is exactly the self-sustaining grant the gate exists to prevent.
        // Excluding `workspace` by name (issue #76) closed the door Workspace
        // came through; it did not close the door Workspace can OPEN. Found by
        // review, not by a test, and the regression test lives beside this.
        //
        // Refused rather than silently ignored: an agent that asked for this
        // should be told the boundary, not left believing it worked.
        if args.session_id == caller_session_id {
            return Err(
                "workspace_set_tools operates on ANOTHER conversation, not this one. \
                 To change the tools available here, ask the user: extensions are \
                 Settings > Extensions, skills are the composer's skill menu. Do not \
                 retry with this session's id."
                    .to_string(),
            );
        }

        // Issue #56, design §7 — the WRITE row, and the same predicate
        // `workspace_send_prompt` asks one screen up. This tool rewrites another
        // conversation's provider, its extension set, its skills and its
        // knowledge bases; a public caller that may not even read a private
        // conversation must certainly not re-tool one. FIRST, before any store
        // read that could answer a question about the target.
        //
        // ⚠ VIS only, exactly as `handle_send_prompt` documents: §7's write row
        // is `may_write` — VIS **and** lineage ∈ {self, child} — and the lineage
        // half is not implemented anywhere in this file.
        self.refuse_unless_visible(cap, &args.session_id).await?;

        // ---- Resolve EVERYTHING before mutating anything, so a bad name is a
        // clean no-op rather than a half-applied change. ------------------
        let add_configs = Self::resolve_added_extensions(cap, &args.add_extensions)?;
        self.refuse_workspace_grant_to_subagent(&args.session_id, &add_configs)
            .await?;
        // Model/provider (decision b): resolve and validate here; apply below.
        let new_provider = Self::resolve_provider_switch(&args.provider, &args.model).await?;

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

        if let Some(agent) = &agent {
            // **Gate F1's UNLOAD half, at this tool's own door (issue #56,
            // finding 14's SECOND door).** `manage_extensions {disable}` has
            // asked `assert_extension_manageable` since finding 14 landed; this
            // handler reached the very same executor — `Agent::remove_extension`,
            // a passthrough to `ExtensionManager::remove_extension` — with no
            // privacy decision anywhere on the path. So the capability was gated
            // at one entrance and open at the other, and the reachable caller is
            // the one finding 14 names: a chat classified Private but bound to a
            // public model passes `refuse_unless_visible` for its own row, and
            // then unloaded the private connector the public model may not see,
            // may not call into, and may not name.
            //
            // ⚠ **The same predicate as the other door, called by name — not a
            // second spelling of it.** `assert_extension_manageable` is
            // `assert_extension_reachable(&normalize(name), Some(admitted))`
            // verbatim, so all three of its consequences arrive here too, and
            // all three are wanted: an unknown name reads Private and is refused
            // (which is what stops this refusal being the existence oracle
            // `add_extensions` needed a comment to avoid), the name is
            // normalized to the key the executor removes under, and a model
            // bound to another institution may see a mismatched connector but
            // may not unload it. Writing the rule out here in this file's own
            // words is exactly how these two doors drifted apart in the first
            // place.
            //
            // ⚠ **Asked on the TARGET's manager**, because it is the target's
            // loaded set that is about to change and the tier is a property of
            // that entry — while `cap` is the CALLER's, because the caller is
            // the one being entitled. Both halves matter when the two
            // conversations differ.
            //
            // ⚠ **BEFORE `apply_extension_changes`, not inside its remove loop**,
            // so a refused removal cannot land after that function has already
            // applied the adds — the "resolve everything before mutating
            // anything" rule the add half states above, held across both halves.
            for name in &args.remove_extensions {
                agent
                    .extension_manager
                    .assert_extension_manageable(name, cap)
                    .await
                    .map_err(|e| e.message.to_string())?;
            }
            applied.extend(
                Self::apply_extension_changes(
                    agent,
                    &args.session_id,
                    add_configs,
                    &args.remove_extensions,
                )
                .await?,
            );
        }

        applied.extend(
            self.apply_session_skills(&args.session_id, &args.add_skills, &args.remove_skills)
                .await?,
        );

        // Model/provider — mirrors /agent/update_provider, which also persists
        // provider_name + model_config onto the session row.
        if let (Some(agent), Some((provider_name, model_name, provider))) = (&agent, new_provider) {
            agent
                .update_provider(provider, &args.session_id)
                .await
                .map_err(|e| format!("failed to switch provider: {e}"))?;
            applied.push(format!("model={provider_name}/{model_name}"));
        }

        if let Some(kbs) = &args.set_knowledge_bases {
            applied.push(Self::apply_knowledge_bases(
                &args.session_id,
                kbs,
                args.primary_knowledge_base.as_deref(),
            )?);
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

    /// **Gate F1, at the workspace's own two enable doors** (issue #56,
    /// finding 4).
    ///
    /// `extensionmanager__manage_extensions {action:"enable"}` is not the only
    /// way an agent turns an extension on. `workspace_set_tools
    /// {add_extensions}` attaches one to a live conversation, and
    /// `workspace_open {new:{extensions}}` picks the set a brand-new
    /// conversation starts with — and the instruction block above tells the
    /// model to prefer exactly these ("do this yourself instead of pointing at
    /// Settings"). Both ran with no tier check at all, so a chat on a public
    /// model could enable `ucsfomopagent` on itself, hand it to a sibling, or
    /// start a fresh conversation holding it. Enabling is not a tool call INTO
    /// a private server; it is the call that SPAWNS one — it pulls that
    /// server's secrets out of the keychain and opens the session — so Gate C
    /// refusing the first tool call afterwards is already too late.
    ///
    /// ⚠ **This decides nothing. It asks
    /// [`refusal::extension_enable_refusal`], which is the ONE enable gate, and
    /// renders its refusal as the `String` this file's handlers return.**
    ///
    /// It used to re-implement that gate arm for arm — the #42 pin, then
    /// [`privacy::resolve_extension`] + [`refusal::privacy_refusal`] for the
    /// tier, then [`CallCapability::cross_affiliation_warning`] for DR-26 — and
    /// this comment used to claim `check_enable_allowed`
    /// (`extension_manager_extension.rs`) was "built out of exactly these three
    /// pieces". **It was not:** that function hand-wrote its tier arm
    /// (`class.tier.is_private() && caller == Public`) with its own sentence,
    /// and put it FIRST, above the operator pin, because finding 13 showed the
    /// pin is an install-state oracle. The copy here asked the pin first — so at
    /// `workspace_open {new:{extensions}}`, which looks the entry up before
    /// asking, a public caller naming a private connector learned from the
    /// refusal whether this machine had it installed and pinned off. Two
    /// spellings of one rule, agreeing on the verdict and disagreeing on the
    /// order, with a false comment asserting they were one. Both doors now call
    /// the one function; the clause order lives there, once.
    ///
    /// `entry` is the on-disk config entry when the extension is installed.
    /// `None` means "not installed, or not looked up yet"; the tier still
    /// resolves, by name, from the compiled marketplace baseline — which is what
    /// makes the refusal identical in both worlds.
    ///
    /// [`privacy::resolve_extension`]: crate::privacy::resolve_extension
    /// [`refusal::privacy_refusal`]: crate::privacy::refusal::privacy_refusal
    /// [`refusal::extension_enable_refusal`]: crate::privacy::refusal::extension_enable_refusal
    /// [`CallCapability::cross_affiliation_warning`]: crate::privacy::CallCapability::cross_affiliation_warning
    fn refuse_gated_extension_enable(
        cap: crate::privacy::CallCapability,
        name: &str,
        entry: Option<&crate::config::ExtensionEntry>,
    ) -> Result<(), String> {
        // #42's provenance signal, asked here rather than inside the gate: it
        // reads the global config, and the gate is kept pure so it can be driven
        // at every tier in both toggle positions with no machine state. The gate
        // decides what to do with it — including that it is decided LAST, below
        // both privacy arms, because "the operator turned this off" is an answer
        // about this machine and no caller that may not reach the extension may
        // read it out of a refusal.
        let persisted =
            entry.is_some_and(|e| crate::config::extension_entry_is_persisted(&e.config.name()));
        match crate::privacy::refusal::extension_enable_refusal(cap, name, entry, persisted) {
            Some(err) => Err(err.message.to_string()),
            None => Ok(()),
        }
    }

    /// Resolve `add_extensions` to loadable configs, or fail the whole call.
    ///
    /// Resolve through `get_extension_entry_by_name`, NOT
    /// `get_extension_by_name`. The latter is `…entry_by_name(name).map(|e|
    /// e.config)` (`config/extensions.rs:138-140`) — it DISCARDS the
    /// operator's `enabled` flag. Issue #42's gate lives one layer up, in
    /// `manage_extensions`' enable path (`check_enable_allowed`,
    /// `extension_manager_extension.rs:97-125`), and `Agent::add_extension`
    /// does not re-check it. So resolving with the flag-less helper would
    /// make `workspace_set_tools` a SECOND, ungated way to enable an
    /// extension an operator deliberately wrote `enabled: false` for —
    /// including on the caller's own session. That is the pinned
    /// tool-environment case (benchmarking, safety) the #42 doc comment
    /// names, and defeating it is a straight privilege escalation.
    fn resolve_added_extensions(
        cap: crate::privacy::CallCapability,
        names: &[String],
    ) -> Result<Vec<crate::agents::ExtensionConfig>, String> {
        let mut add_configs = Vec::new();
        for name in names {
            // ⚠ **BEFORE the lookup, and that ordering is the point.** A
            // private extension the machine has not installed would otherwise
            // come back "unknown extension 'ucsfomopagent'" while an installed
            // one came back with the privacy refusal — an install oracle for a
            // model that may not reach the connector either way. Resolved by
            // NAME here, from the compiled baseline, so the sentence is the
            // same in both worlds.
            Self::refuse_gated_extension_enable(cap, name, None)?;
            match crate::config::get_extension_entry_by_name(name) {
                None => return Err(format!("unknown extension '{name}'")),
                // Asked a SECOND time with the entry, because it can only raise
                // the answer: `resolve_extension` also matches a renamed entry
                // through its install directory and the provenance store
                // (Task 43 / DR-23), which the name alone no longer carries.
                Some(entry) => {
                    Self::refuse_gated_extension_enable(cap, name, Some(&entry))?;
                    add_configs.push(entry.config);
                }
            }
        }
        Ok(add_configs)
    }

    /// The same gate at `workspace_open {new:{extensions}}` — the door that
    /// chooses what a conversation is BORN holding.
    ///
    /// It refuses rather than resolving, because the names are forwarded to
    /// `WorkspaceServices::start_session`, whose own resolution goes through
    /// `get_extension_by_name` — the flag-less helper that discards the
    /// operator's `enabled` flag (`config/extensions.rs`). So this path had
    /// neither of Gate F1's arms *nor* #42's pin, and was the one way to get an
    /// operator-disabled extension running again.
    ///
    /// ⚠ **An unknown name is NOT an error here**, unlike
    /// [`Self::resolve_added_extensions`]. `start_session` already answers
    /// `unknown extension '<name>'` for one, and duplicating that check would
    /// make this function's behaviour depend on whether the *caller's* process
    /// shares the daemon's installed set. The tier arm still fires, because it
    /// resolves from the compiled baseline and not from what is installed.
    ///
    /// Runs BEFORE `create_session`, so a refusal leaves no orphan conversation
    /// behind — the session is committed the moment `start_session` returns.
    fn refuse_gated_new_session_extensions(
        cap: crate::privacy::CallCapability,
        extensions: Option<&Vec<String>>,
    ) -> Result<(), String> {
        for name in extensions.into_iter().flatten() {
            let entry = crate::config::get_extension_entry_by_name(name);
            Self::refuse_gated_extension_enable(cap, name, entry.as_ref())?;
        }
        Ok(())
    }

    /// §5: workspace control must not fan out through delegation trees.
    async fn refuse_workspace_grant_to_subagent(
        &self,
        session_id: &str,
        add_configs: &[crate::agents::ExtensionConfig],
    ) -> Result<(), String> {
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
                .get_session(session_id, false)
                .await
                .map_err(|e| e.to_string())?;
            if target.session_type == crate::session::session_manager::SessionType::SubAgent {
                return Err(
                    "subagent sessions can never be granted the workspace extension".into(),
                );
            }
        }
        Ok(())
    }

    /// Decision b's resolve half: validate a provider+model switch and build the
    /// provider, without applying anything. `None` when no switch was asked for.
    async fn resolve_provider_switch(
        provider: &Option<String>,
        model: &Option<String>,
    ) -> Result<
        Option<(
            String,
            String,
            std::sync::Arc<dyn crate::providers::base::Provider>,
        )>,
        String,
    > {
        match (provider, model) {
            (None, None) => Ok(None),
            (None, Some(_)) => Err(
                "`model` requires `provider`: a model name is ambiguous across providers; \
                 pass both (e.g. provider:\"anthropic\", model:\"claude-opus-5\")"
                    .into(),
            ),
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
                Ok(Some((
                    provider_name.clone(),
                    model_name,
                    crate::providers::create(provider_name, model_config)
                        .await
                        .map_err(|e| format!("failed to create {provider_name} provider: {e}"))?,
                )))
            }
        }
    }

    /// The exact /agent/add_extension handler path (routes/agent.rs:744-767):
    /// add on the live agent, persist only after a successful load. Returns the
    /// `applied` labels for the extensions that changed.
    ///
    /// ⚠ **This function decides nothing about privacy, and it has exactly one
    /// caller for that reason.** Both of its halves are gated at
    /// [`Self::handle_set_tools`], where the capability lives: `add_configs`
    /// have already been through Gate F1's enable arm
    /// ([`Self::resolve_added_extensions`]), and every name in
    /// `remove_extensions` has already been through its unload arm
    /// (`ExtensionManager::assert_extension_manageable`, issue #56 finding 14's
    /// second door). A SECOND caller would silently be an ungated door to
    /// `Agent::add_extension` and `Agent::remove_extension` — which is precisely
    /// how this one came to be one. If you need this here, carry both gates with
    /// it or move them inside.
    async fn apply_extension_changes(
        agent: &crate::agents::Agent,
        session_id: &str,
        add_configs: Vec<crate::agents::ExtensionConfig>,
        remove_extensions: &[String],
    ) -> Result<Vec<String>, String> {
        let mut applied = Vec::new();
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
        for name in remove_extensions {
            agent
                .remove_extension(name)
                .await
                .map_err(|e| format!("failed to remove '{name}': {e}"))?;
            applied.push(format!("-{name}"));
            extensions_changed = true;
        }
        if extensions_changed {
            agent
                .persist_extension_state(session_id)
                .await
                .map_err(|e| format!("failed to persist extension state: {e}"))?;
        }
        Ok(applied)
    }

    /// Skills — SESSION-SCOPED (Task 11). Never the machine-wide file. Returns
    /// the `applied` labels for the skills that changed.
    async fn apply_session_skills(
        &self,
        session_id: &str,
        add_skills: &[String],
        remove_skills: &[String],
    ) -> Result<Vec<String>, String> {
        let mut applied = Vec::new();
        if !add_skills.is_empty() || !remove_skills.is_empty() {
            crate::agents::session_skills::apply(
                &self.context.session_manager,
                session_id,
                add_skills,
                remove_skills,
            )
            .await
            .map_err(|e| format!("failed to scope skills: {e}"))?;
            for name in add_skills {
                applied.push(format!("+skill:{name}"));
            }
            for name in remove_skills {
                applied.push(format!("-skill:{name}"));
            }
        }
        Ok(applied)
    }

    /// Knowledge bases (plural — issue #45), with their write target. Returns
    /// the one `applied` label describing what the service actually stored.
    fn apply_knowledge_bases(
        session_id: &str,
        kbs: &[String],
        primary_knowledge_base: Option<&str>,
    ) -> Result<String, String> {
        use crate::workspace_services::KbPrimaryChoice;
        let services = workspace_services::get()
            .ok_or("knowledge-base scoping requires the BioRouter daemon")?;
        // Three-valued, because the underlying model is: absent → Auto
        // (keep-if-member, else first, else clear); `""` → an explicit
        // "no write target here"; a name → pin it. Membership is validated
        // by the service against the RESULTING set, so a name outside `kbs`
        // comes back as a clear error rather than a half-applied write.
        let primary = match primary_knowledge_base {
            None => KbPrimaryChoice::Auto,
            Some("") => KbPrimaryChoice::Clear,
            Some(id) => KbPrimaryChoice::Set(id.to_string()),
        };
        let selection = services.set_knowledge_bases(session_id, kbs, primary)?;
        Ok(if selection.kb_ids.is_empty() {
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
        })
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
        cap: crate::privacy::CallCapability,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let args: WorkspaceCloseParams = parse_args(arguments)?;

        // Issue #56, finding 15. Every scope here acts ON another conversation:
        // `tab` closes the window the user is reading it in, `turn` cancels the
        // work it is doing, `agent` evicts it mid-flight. A caller that §7
        // refuses even a READ of that conversation must not be able to stop it —
        // and the answers are an oracle besides ("had no turn in flight" vs
        // "cancelled turn <id>" reports whether a private conversation is
        // working, one call at a time).
        //
        // FIRST, before the scope match: an out-of-reach target must produce the
        // one ambiguous sentence whatever scope was named, including an invalid
        // one, or the scope argument becomes the probe.
        self.refuse_unless_visible(cap, &args.session_id).await?;
        let services = workspace_services::get();

        match args.scope.as_str() {
            // ⚠ **The round trip is not optional here.** This used to emit with
            // `wait_result: false` — fire and forget — and then report the tab
            // closed. It could not have known: the renderer's `close_tab` arm
            // refuses outright when the session has no tab in this window
            // (`planWorkspaceCommand`: `refuse('session has no tab')`), and a
            // frame that arrives while no chat surface is mounted is QUEUED, not
            // applied (`applyWorkspaceCommand`). Both cases produced the cheerful
            // sentence below, so the model told the user a tab was gone while it
            // was still on screen. Same shape as `place_in_gui`'s: ask, wait,
            // and report the answer.
            "tab" => match services {
                Some(s) if s.gui_attached() => {
                    let result = s
                        .gui_command(
                            json!({ "type": "workspace", "cmd": "close_tab", "session_id": args.session_id }),
                            true,
                        )
                        .await?;
                    if !announcement_delivered(&result) {
                        return Ok(vec![Content::text(format!(
                            "The GUI did NOT close the tab for session {} ({}). The session \
                             is untouched; do not tell the user the tab is gone.",
                            args.session_id,
                            gui_detail(&result).unwrap_or("no reason given")
                        ))]);
                    }
                    Ok(vec![Content::text(format!(
                        "Tab for session {} closed (session survives).",
                        args.session_id
                    ))])
                }
                _ => Ok(vec![Content::text(
                    "No GUI attached, so nothing to close at tab scope (gui_attached: false)."
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
        cap: crate::privacy::CallCapability,
        arguments: Option<JsonObject>,
        cancel: &tokio_util::sync::CancellationToken,
    ) -> Result<Vec<Content>, String> {
        use crate::session_events;

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
        // Issue #56, finding 15: an ACTIVITY ORACLE over conversations §7 will
        // not let this caller read. "still running" / "already idle" /
        // "completed (stop)" is a live readout of whether a private
        // conversation is working and when it stopped — the same shape of
        // disclosure `workspace_list` omits rows to avoid, arriving through a
        // tool whose name says "wait".
        //
        // ⚠ **The WHOLE call is refused when any named id is out of reach**,
        // rather than the unreachable ids being dropped from the report. A
        // partial answer would say "these three are still running" and silently
        // omit the fourth — which is the existence disclosure with an extra
        // step, since the caller supplied the list and can diff it. The refusal
        // reveals no more than the same id asked for on its own would, and
        // private stays indistinguishable from absent because
        // `refuse_unless_visible` composes both into one sentence.
        //
        // Before `subscribe`, so a refused watch never claims a slot in another
        // conversation's event ring either.
        for id in &args.session_ids {
            self.refuse_unless_visible(cap, id).await?;
        }
        let wait_all = match args.mode.as_deref() {
            None | Some("any") => false,
            Some("all") => true,
            Some(other) => return Err(format!("unknown mode '{other}' (any | all)")),
        };
        let requested = std::time::Duration::from_secs(args.timeout_s.unwrap_or(120).clamp(1, 600));
        // #110: what the *transport* allows, which is not always what the schema
        // advertises. A bridged coding-agent call is held open inside the
        // child's own MCP client, which applies a hard per-call wall clock and
        // abandons the request when it elapses — so a wait that outruns it does
        // not trade latency for completeness, it loses the partial answer too:
        // the model is told "The operation timed out" instead of being handed
        // the completions this handler had already collected.
        let (timeout, clamped_from) = clamp_watch_to_transport(requested);
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

        let mut cancelled = false;
        let done_now = if wait_all {
            receivers.is_empty()
        } else {
            !completed.is_empty()
        };
        if !done_now && !receivers.is_empty() {
            // `want` counts entries in `completed`, which already holds the
            // sessions the pre-check found idle — so "all" is the full id list
            // and "any" is one more than we already have.
            let want = if wait_all {
                args.session_ids.len()
            } else {
                completed.len() + 1
            };
            cancelled =
                Self::park_for_completions(receivers, &mut completed, want, timeout, cancel).await;
        }

        let still_running: Vec<&String> = args
            .session_ids
            .iter()
            .filter(|id| !completed.iter().any(|(done, _)| done == *id))
            .collect();

        Ok(vec![Content::text(Self::watch_report(
            &completed,
            &still_running,
            timeout,
            clamped_from,
            unknown_liveness,
            cancelled,
        ))])
    }

    /// Park until `want` conversations have published a terminal event, or the
    /// deadline passes — whichever comes first. A timeout is not an error; the
    /// caller reports whatever arrived.
    async fn park_for_completions(
        receivers: Vec<(String, crate::session_events::Subscription)>,
        completed: &mut Vec<(String, String)>,
        want: usize,
        timeout: std::time::Duration,
        cancel: &tokio_util::sync::CancellationToken,
    ) -> bool {
        use crate::session_events::SessionBusEvent;

        let deadline = tokio::time::Instant::now() + timeout;
        // Cancelled when this function returns, **however** it returns — the
        // deadline, a cancel, or every watcher exiting.
        //
        // ⚠ The watcher tasks own the `Subscription`s, and a `Subscription` only
        // reclaims its session's 1024-slot event ring when it drops. Without
        // this the tasks looped on `recv()` for the life of the process after a
        // watch timed out, pinning a ring slot per watched session per watch —
        // a leak that got materially worse with #110, because a watch that used
        // to be killed by the child's 60-second deadline can now legitimately
        // park for ten minutes.
        let stop = tokio_util::sync::CancellationToken::new();
        let _reap_watchers = stop.clone().drop_guard();

        // One task per watched session, all feeding one channel: simpler
        // and more obviously correct than a hand-rolled select over a Vec,
        // and 32 short-lived tasks is nothing.
        let (tx, mut rx) = tokio::sync::mpsc::channel::<(String, String)>(WATCH_MAX_SESSIONS);
        for (id, mut receiver) in receivers {
            let tx = tx.clone();
            let stop = stop.clone();
            tokio::spawn(async move {
                loop {
                    // `recv` is cancel-safe, so losing this race never drops an
                    // event that had already resolved.
                    let event = tokio::select! {
                        biased;
                        () = stop.cancelled() => return,
                        event = receiver.recv() => event,
                    };
                    match event {
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

        // #110: the turn's own token ends the park. Every cancellation
        // mechanism Biorouter has — Stop, `AppState::cancel_turn`, the websocket
        // `TurnGuard`, and a bridge lease dropping — reaches a running tool
        // through it, and a watch that ignored it kept a cancelled turn alive
        // for the whole wait. That was survivable while the child's own deadline
        // capped it at a minute; it is not now that a watch may legitimately
        // park for ten.
        let mut cancelled = false;
        let _ = tokio::time::timeout_at(deadline, async {
            while completed.len() < want {
                tokio::select! {
                    biased;
                    () = cancel.cancelled() => {
                        cancelled = true;
                        break;
                    }
                    entry = rx.recv() => match entry {
                        Some(entry) => completed.push(entry),
                        None => break,
                    },
                }
            }
        })
        .await;
        cancelled
    }

    /// The `workspace_watch` reply: what finished, what is still running, and —
    /// when nothing finished — whether we were even able to tell.
    fn watch_report(
        completed: &[(String, String)],
        still_running: &[&String],
        timeout: std::time::Duration,
        clamped_from: Option<std::time::Duration>,
        unknown_liveness: usize,
        cancelled: bool,
    ) -> String {
        let mut report = String::new();
        if completed.is_empty() {
            report.push_str(&format!(
                "No conversation finished within {}s. Still running: {}. \
                 They keep running; watch again or read them later.\n",
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
                     could not be checked, so some of these may never have been \
                     running.)\n",
                );
            }
        } else {
            report.push_str("Completed:\n");
            for (id, reason) in completed {
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
        // #110: a cancelled watch is not a finished one, and saying nothing
        // would let the caller read "still running" as the answer to a wait that
        // never happened. The conversations themselves are untouched — what was
        // cancelled is the turn doing the watching.
        if cancelled {
            report.push_str(
                "\n(The watch was cancelled before its deadline. The conversations above \
                 were not affected.)",
            );
        }
        // #110: say when the wait was shorter than the one asked for, and why.
        // A caller that is not told will read "still running" after 60 s as the
        // answer to the 600-second question it asked, and either give up on a
        // subagent that is working fine or start polling transcripts. Naming the
        // effective wait is what makes "watch again" the obvious next move.
        if let Some(requested) = clamped_from {
            report.push_str(&format!(
                "\n(Waited {}s of the {}s requested: this turn's transport ends a single \
                 tool call at {}s, so the wait was shortened to return this status instead \
                 of failing. Watch again to keep waiting — the completions above are not \
                 repeated.)",
                timeout.as_secs(),
                requested.as_secs(),
                timeout.as_secs(),
            ));
        }
        report
    }
}

/// Shorten a requested wait to what the transport carrying this call allows.
///
/// Returns the effective wait and, when it was shortened, the one that was
/// asked for — so the report can say both rather than silently answering a
/// different question.
///
/// The margin is what the answer itself needs on the wire, and it is subtracted
/// rather than assumed: a wait equal to the budget finishes at the instant the
/// request is abandoned, which is the failure with the timing made tight rather
/// than the failure fixed.
///
/// Free and pure so the arithmetic is testable without a bridge; the only input
/// that varies is the task-local the bridge publishes.
fn clamp_watch_to_transport(
    requested: std::time::Duration,
) -> (std::time::Duration, Option<std::time::Duration>) {
    const ANSWER_MARGIN: std::time::Duration = std::time::Duration::from_secs(10);
    let Some(budget) = crate::providers::coding_agent::bridge::bridged_call_budget() else {
        // Not a bridged call: nothing is holding a socket open, so the schema's
        // ceiling is the only one that applies.
        return (requested, None);
    };
    let allowed = budget.saturating_sub(ANSWER_MARGIN).max(
        // A budget smaller than the margin would clamp every wait to zero and
        // turn `workspace_watch` into a liveness poll. One second is the schema's
        // own floor.
        std::time::Duration::from_secs(1),
    );
    if requested <= allowed {
        (requested, None)
    } else {
        (allowed, Some(requested))
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
        cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        let caller = &meta.session_id;
        // Issue #56 §7. The capability this call was ADMITTED on, carried from
        // `Agent::dispatch_tool_call` — never re-derived here. `Copy`, so it
        // threads into each handler without a clone; see `CallCapability`'s own
        // doc for why a second read at this program point would be a race rather
        // than a refresh.
        let cap = meta.capability;
        let content = match name {
            "workspace_list" => self.handle_list(caller, cap, arguments).await,
            "workspace_read_conversation" => {
                self.handle_read_conversation(caller, cap, arguments).await
            }
            "workspace_send_prompt" => self.handle_send_prompt(caller, cap, arguments).await,
            "workspace_set_tools" => self.handle_set_tools(caller, cap, arguments).await,
            "workspace_close" => self.handle_close(caller, cap, arguments).await,
            // #110: the only handler here that PARKS, so the only one the turn's
            // cancel token has anything to reach. Every other tool returns
            // promptly; a watch may legitimately wait ten minutes.
            "workspace_watch" => {
                self.handle_watch(caller, cap, arguments, &cancellation_token)
                    .await
            }
            "workspace_open" => self.handle_open(caller, cap, arguments).await,
            "workspace_read_panel" | "workspace_capture_panel" => {
                self.handle_panel(name, caller, cap, arguments).await
            }
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

/// BR-71 §8.1 / decision 7: the user's focus-etiquette preference. When on, the
/// workspace never opens a tab or a window on its own — it posts a notification
/// naming the conversation instead, and the tool result says so, so the model
/// does not claim to have opened something.
///
/// Config key, read through the same store every other daemon-visible
/// preference uses (`Config::global().get_param`, e.g. `SECURITY_COMMAND_POLICY`
/// in `security/mod.rs`). Default OFF — background-open stays the design's
/// default (§4.1 "opens in the background, never stealing the composer").
pub const ANNOUNCE_ONLY_KEY: &str = "WORKSPACE_ANNOUNCE_ONLY";

/// Pure, so the mapping is testable without a config file.
fn announce_only_enabled_for(configured: Option<bool>) -> bool {
    configured.unwrap_or(false)
}

pub(crate) fn announce_only_enabled() -> bool {
    announce_only_enabled_for(
        crate::config::Config::global()
            .get_param::<bool>(ANNOUNCE_ONLY_KEY)
            .ok(),
    )
}

/// The frames that put a conversation in front of the user, and are therefore
/// subject to the setting. `open_tab` and `open_window` create something new;
/// `activate_tab` yanks the view to an existing tab, which is the same
/// intrusion by a different route — the setting's promise is "don't take me
/// somewhere I didn't ask to go", not "don't allocate a tab". Everything else
/// (annotate, close, notify) is not a focus event and always reaches the GUI.
///
/// The `activate_tab` entry was forward protection when it landed (plan open
/// question 6: "no daemon-side emitter constructs one today"). It has one now —
/// [`WorkspaceClient::placement_frame`] sends it for a `workspace_open` on a
/// conversation that already has a tab — so this entry is load-bearing, and
/// `announce_only_downgrades_a_focus_of_an_existing_tab` is the test that walks
/// the whole path rather than the transform alone.
const FOCUS_STEALING_CMDS: [&str; 3] = ["open_tab", "open_window", "activate_tab"];

/// Downgrade focus-stealing frames to a notification when announce-only is on.
pub(crate) fn apply_focus_etiquette(
    frame: serde_json::Value,
    announce_only: bool,
) -> serde_json::Value {
    if !announce_only {
        return frame;
    }
    let cmd = frame
        .get("cmd")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if !FOCUS_STEALING_CMDS.contains(&cmd) {
        return frame;
    }
    let session_id = frame
        .get("session_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("a conversation")
        .to_string();
    json!({
        "type": "workspace",
        "cmd": "notify",
        "session_id": session_id,
        "level": "info",
        "message": format!(
            "An agent wants to show you conversation {session_id}. \
             Open it from History; automatic tab opening is turned off in Settings."
        ),
    })
}

/// What `workspace_open` actually did to the GUI, as opposed to what it asked
/// for. The distinction exists because `open_tab` is a dedupe/adopt command: on
/// a conversation that already has a tab it opens nothing, and the renderer
/// still answers `ok:true`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TabOutcome {
    /// No tab existed for this conversation (or one is being created, or a
    /// window was asked for): `open_tab` / `open_window` really places it.
    Opened,
    /// The layout echo already showed a tab and the caller did NOT ask for
    /// focus. The frame is still `open_tab` — a no-op that repairs a stale echo
    /// — but nothing on screen changed.
    AlreadyOpen,
    /// The layout echo already showed a tab and the caller asked for focus:
    /// `activate_tab` moves the view to it. No tab is opened.
    Focused,
}

/// What the MODEL is told. Pure, and separate from the frame, because the two
/// can disagree in exactly one direction that matters: the frame was downgraded
/// to a notification and the text still says "opened". A model that believes it
/// opened a tab will tell the user so, and the user is looking at a screen where
/// nothing happened.
///
/// Deviation from the plan's snippet: decision 5's `dir_note` is NOT a parameter
/// here, so [`WorkspaceClient::place_in_gui`] appends it to whatever this
/// returns. That keeps the note on BOTH arms — a newly created session announced
/// rather than opened still tells the model where it works.
///
/// ⚠ `outcome` was added after the five-argument form shipped, and it is the
/// whole point of the addition: the previous signature could not express "the
/// tab was already there", so every re-open was reported as an opening. A pinned
/// signature is worth less than a true sentence.
pub(crate) fn open_result_text(
    session_id: &str,
    placement: &str,
    focus: bool,
    announce_only: bool,
    outcome: TabOutcome,
    gui_result: &serde_json::Value,
) -> String {
    if announce_only {
        // Deny what was actually ASKED for. `placement: "window"` answered with
        // "no tab was opened" is true and useless: it reads as a statement about
        // tabs, leaving the model free to conclude a window opened instead —
        // the same false premise, one noun over. `apply_focus_etiquette`
        // suppresses `open_window` exactly as it suppresses `open_tab`, so the
        // two halves must say the same thing.
        let noun = if placement == "window" {
            "window"
        } else {
            "tab"
        };
        // The announcement is itself a round trip, and the GUI can refuse it.
        // Reporting "they were notified" when it answered `ok:false` is the
        // same class of falsehood as claiming a tab — the model hands the user
        // off to something they never saw — so the refusal is carried through
        // verbatim instead of being swallowed with the rest of `gui_result`.
        let handoff = if announcement_delivered(gui_result) {
            "they were notified and can open it themselves".to_string()
        } else {
            format!(
                "the GUI did NOT confirm the notification ({}), so say the conversation \
                 is waiting in History rather than that the user was told",
                gui_detail(gui_result).unwrap_or("no reason given")
            )
        };
        // A conversation that already HAD a tab was never going to get one, so
        // "no tab was opened" would be true for the wrong reason and would let
        // the model conclude the view moved. Deny the thing the setting actually
        // suppressed: the jump.
        if outcome == TabOutcome::Focused {
            return format!(
                "Session {session_id} already has a {noun} in the GUI, but the user has \
                 turned OFF automatic tab opening, so it was NOT brought to the front. \
                 {handoff}. Do not tell the user you opened or switched to a {noun}."
            );
        }
        return format!(
            "Session {session_id} is ready, but the user has turned OFF automatic tab \
             opening, so no {noun} was opened; {handoff}. Do not tell the user you \
             opened a {noun}."
        );
    }
    // The separator belongs to the detail, not to the sentence: `place_in_gui`
    // appends decision 5's directory note to whatever this returns, and a
    // trailing space on a GUI answer that carried no `detail` put a double
    // space in the middle of the model-facing text.
    let detail = gui_detail(gui_result)
        .map(|d| format!(" {d}"))
        .unwrap_or_default();
    let focus_note = if focus { ", focused" } else { ", background" };
    // The round trip is the only evidence anything happened. Whatever was asked
    // for, it did not happen — and the denial names what was asked for, so a
    // refused `activate_tab` is not read as a refused opening.
    if !announcement_delivered(gui_result) {
        let denied = if outcome == TabOutcome::Focused {
            "was NOT brought to the front"
        } else {
            "NOT opened"
        };
        return format!(
            "Session {session_id} {denied} in the GUI ({placement}{focus_note}).{detail}"
        );
    }
    match outcome {
        TabOutcome::Opened => {
            format!("Session {session_id} opened in the GUI ({placement}{focus_note}).{detail}")
        }
        // ⚠ Both arms below say "no new tab was opened" in words. The model is
        // about to paraphrase this sentence to the user, and "opened" is the
        // verb it will reach for unless it is told, explicitly, that nothing
        // was opened.
        TabOutcome::AlreadyOpen => format!(
            "Session {session_id} was ALREADY open in the GUI ({placement}{focus_note}); \
             no new tab was opened and nothing moved.{detail}"
        ),
        TabOutcome::Focused => format!(
            "Session {session_id} was already open in the GUI; its existing tab was \
             brought to the front ({placement}{focus_note}), and no new tab was \
             opened.{detail}"
        ),
    }
}

/// Did the renderer accept the frame? Absent or non-boolean `ok` counts as a
/// refusal: the round trip is the only evidence anything happened, and an
/// unparseable answer is not evidence.
fn announcement_delivered(gui_result: &serde_json::Value) -> bool {
    gui_result
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

/// The renderer's own reason, when it gave one.
fn gui_detail(gui_result: &serde_json::Value) -> Option<&str> {
    gui_result
        .get("detail")
        .and_then(serde_json::Value::as_str)
        .filter(|d| !d.is_empty())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::agents::extension::PlatformExtensionContext;
    use std::time::Duration;

    // ---------------------------------------------------------------------
    // #110: a wait that outruns the transport
    // ---------------------------------------------------------------------

    /// Off a bridged call there is nothing holding a socket open, so the
    /// schema's own ceiling is the only one that applies and the wait is
    /// answered as asked.
    #[test]
    fn an_unbridged_watch_waits_exactly_as_long_as_it_was_asked_to() {
        let (effective, clamped) = clamp_watch_to_transport(Duration::from_secs(600));
        assert_eq!(effective, Duration::from_secs(600));
        assert_eq!(
            clamped, None,
            "nothing to explain when nothing was shortened"
        );
    }

    /// The bug: a 600-second wait inside a transport that ends the call sooner.
    /// It must come back SHORTER — and say so — rather than run to 600 and be
    /// abandoned, which loses the partial answer along with the wait.
    #[tokio::test]
    async fn a_bridged_watch_is_shortened_to_fit_its_transport() {
        let (effective, clamped) =
            crate::providers::coding_agent::bridge::with_call_budget_for_test(
                Duration::from_secs(60),
                async { clamp_watch_to_transport(Duration::from_secs(600)) },
            )
            .await;
        assert_eq!(
            effective,
            Duration::from_secs(50),
            "the margin is what the answer itself needs on the wire"
        );
        assert_eq!(clamped, Some(Duration::from_secs(600)));
        assert!(
            effective < Duration::from_secs(60),
            "a wait equal to the budget finishes exactly when the request is \
             abandoned, which is the same failure with tighter timing"
        );
    }

    /// A wait that already fits is left alone, and reports nothing — an
    /// explanation for a shortening that did not happen is noise the model has
    /// to reason about.
    #[tokio::test]
    async fn a_bridged_watch_that_already_fits_is_untouched() {
        let (effective, clamped) =
            crate::providers::coding_agent::bridge::with_call_budget_for_test(
                Duration::from_secs(600),
                async { clamp_watch_to_transport(Duration::from_secs(120)) },
            )
            .await;
        assert_eq!(effective, Duration::from_secs(120));
        assert_eq!(clamped, None);
    }

    /// A pathologically small budget must not clamp the wait to zero and turn
    /// `workspace_watch` into a liveness poll that always says "still running".
    #[tokio::test]
    async fn a_tiny_budget_still_leaves_a_real_wait() {
        let (effective, _) = crate::providers::coding_agent::bridge::with_call_budget_for_test(
            Duration::from_secs(2),
            async { clamp_watch_to_transport(Duration::from_secs(600)) },
        )
        .await;
        assert!(
            effective >= Duration::from_secs(1),
            "the schema's own floor is one second"
        );
    }

    /// #110: a cancelled turn must end the park at the instant it lands, not at
    /// the deadline.
    ///
    /// The generous timeout is the assertion: if the token were ignored this
    /// would sit for ten minutes, so the test would fail by timing out rather
    /// than by asserting. That is exactly the shape of the bug — a watch that
    /// kept a cancelled turn alive for its whole wait, which was survivable
    /// while the child's own 60-second deadline capped it and is not now.
    #[tokio::test]
    async fn cancelling_the_turn_ends_a_parked_watch_at_once() {
        let cancel = tokio_util::sync::CancellationToken::new();
        let id = format!("watch-cancel-{:016x}", rand::random::<u64>());
        let receivers = vec![(id.clone(), crate::session_events::subscribe(&id))];
        let mut completed: Vec<(String, String)> = Vec::new();

        {
            let cancel = cancel.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                cancel.cancel();
            });
        }

        let started = std::time::Instant::now();
        let cancelled = tokio::time::timeout(
            Duration::from_secs(5),
            WorkspaceClient::park_for_completions(
                receivers,
                &mut completed,
                1,
                Duration::from_secs(600),
                &cancel,
            ),
        )
        .await
        .expect("a cancelled park must return, not run to its deadline");

        assert!(cancelled, "the caller has to be able to say why it stopped");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "it must end when the cancel lands, not later"
        );
        assert!(completed.is_empty(), "nothing finished");
    }

    /// And a cancelled watch says so, rather than letting the caller read "still
    /// running" as the answer to a wait that never happened.
    #[test]
    fn a_cancelled_watch_reports_that_it_was_cancelled() {
        let still = "sess-a".to_string();
        let report =
            WorkspaceClient::watch_report(&[], &[&still], Duration::from_secs(600), None, 0, true);
        assert!(report.contains("cancelled"), "{report}");
        assert!(
            report.contains("not affected"),
            "and that the conversations themselves are untouched: {report}"
        );
    }

    /// The watcher tasks own the `Subscription`s, and a `Subscription` only
    /// reclaims its session's event-ring slot when it drops. Before #110 they
    /// looped on `recv()` for the life of the process after a watch ended, so a
    /// timed-out watch leaked a slot per watched session — which a ten-minute
    /// park makes far easier to accumulate.
    #[tokio::test]
    async fn a_finished_park_reaps_its_watcher_tasks() {
        let id = format!("watch-reap-{:016x}", rand::random::<u64>());
        let receivers = vec![(id.clone(), crate::session_events::subscribe(&id))];
        let mut completed: Vec<(String, String)> = Vec::new();
        let cancel = tokio_util::sync::CancellationToken::new();

        // A short deadline, so the park ends the way a timed-out watch does.
        WorkspaceClient::park_for_completions(
            receivers,
            &mut completed,
            1,
            Duration::from_millis(50),
            &cancel,
        )
        .await;

        // Give the reaped tasks a moment to notice and drop their subscriptions,
        // then assert the ring is free: a session with no live subscriber
        // releases its bus entirely.
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(
            crate::session_events::observer_count(&id),
            0,
            "the watcher task must have dropped its Subscription when the park ended"
        );
    }

    /// The shortening has to be legible. A caller told only "still running"
    /// after 50 s will read that as the answer to the 600-second question it
    /// asked — and either abandon a subagent that is working fine or fall back
    /// to polling transcripts, which is the behaviour `workspace_watch` exists
    /// to replace.
    #[test]
    fn a_shortened_watch_reports_both_the_effective_and_the_requested_wait() {
        let still = "sess-a".to_string();
        let report = WorkspaceClient::watch_report(
            &[],
            &[&still],
            Duration::from_secs(50),
            Some(Duration::from_secs(600)),
            0,
            false,
        );
        assert!(report.contains("50s"), "the effective wait: {report}");
        assert!(report.contains("600s"), "and the one asked for: {report}");
        assert!(
            report.to_lowercase().contains("watch again"),
            "and the move that follows from it: {report}"
        );
        assert!(
            !report.to_lowercase().contains("timed out")
                && !report.to_lowercase().contains("error"),
            "a timeout is a status, never a failure: {report}"
        );
    }

    /// Completions observed before the deadline survive the shortening. Mode
    /// `all` holds finished targets until every target completes, so a clamp
    /// that dropped them would lose exactly the work the wait had already paid
    /// for.
    #[test]
    fn a_shortened_watch_keeps_the_completions_it_observed() {
        let still = "sess-c".to_string();
        let report = WorkspaceClient::watch_report(
            &[
                ("sess-a".to_string(), "finished".to_string()),
                ("sess-b".to_string(), "finished".to_string()),
            ],
            &[&still],
            Duration::from_secs(50),
            Some(Duration::from_secs(600)),
            0,
            false,
        );
        assert!(report.contains("sess-a") && report.contains("sess-b"));
        assert!(report.contains("sess-c"), "and what is still running");
        assert!(report.contains("600s"), "and that the wait was shortened");
    }

    /// ⚠ **The one list of idioms that must never come back, for every file
    /// that guards against them.**
    ///
    /// Both this file and `extension_manager_extension.rs` carried an
    /// anti-respelling scan, and each forbade only the idiom *its own* history
    /// had produced: this file barred `resolve_extension(` / `privacy_refusal(`
    /// / the affiliation helpers; the other barred `class.tier.is_private()` /
    /// `ASK_THE_USER_TO_SWITCH`. So a re-derivation written in the *other*
    /// file's vocabulary walked past both scans — the guard against duplicating
    /// a rule had itself been duplicated, into two versions that disagreed. One
    /// list, read by both, is the only shape that cannot drift.
    ///
    /// ⚠ **Do not put this in production text.** It contains the very literals
    /// the scans search for, so a copy above the `#[cfg(test)]` boundary would
    /// make both files fail on their own guard. It lives inside `mod tests`
    /// (hence `pub(crate) mod`) precisely so each scan's production/test cut
    /// removes it.
    ///
    /// New entries are cheap and belong here rather than in either caller: an
    /// arm of the enable decision re-spelled anywhere is the defect, and which
    /// file re-spelled it is an accident of who edited what.
    pub(crate) const ENABLE_GATE_RESPELLINGS: &[&str] = &[
        // This file's historic idiom (finding 13's oracle at `workspace_open`).
        "resolve_extension(",
        "privacy_refusal(",
        "cross_affiliation_warning(",
        "cross_affiliation_refusal(",
        // `extension_manager_extension.rs`'s historic idiom.
        "class.tier.is_private()",
        "ASK_THE_USER_TO_SWITCH",
    ];

    /// The production half of a source file: everything above its `#[cfg(test)]`
    /// boundary, as non-comment lines.
    ///
    /// Comments are dropped because both files DESCRIBE the forbidden idioms in
    /// doc comments — `workspace_extension.rs`'s `refuse_gated_extension_enable`
    /// spells `class.tier.is_private() && caller == Public` out in prose so the
    /// next reader knows what the shared gate decides. A scan that cannot tell a
    /// description from a re-derivation can only be satisfied by deleting the
    /// description, which is the wrong direction.
    pub(crate) fn asserts_no_respellings(production: &str, file: &str) {
        for respelling in ENABLE_GATE_RESPELLINGS {
            let hit = production
                .lines()
                .any(|l| !l.trim_start().starts_with("//") && l.contains(respelling));
            assert!(
                !hit,
                "`{respelling}` is back in {file}'s production text: an arm of the enable \
                 gate is being re-derived there instead of asked for. That is the \
                 two-spellings shape, and last time it cost an install-state oracle at \
                 `workspace_open {{new:{{extensions}}}}`. Ask \
                 `crate::privacy::refusal::extension_enable_refusal` instead."
            );
        }
    }
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
        crate::agents::mcp_client::McpMeta::new(
            "caller",
            crate::privacy::CallCapability::for_test_restricted(),
        )
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
    ///
    /// ⚠ **Since Task 24 the table is empty, so the first loop below iterates
    /// nothing.** Do not cite it as coverage: what still bites today is the
    /// SECOND half — every `workspace_*` name in the instruction block must be
    /// advertised — and an empty table makes that the stronger claim, not a
    /// weaker one. The first loop is a standing guard for the next phase that
    /// stages a tool ahead of its handler.
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

        // …and no retired name anywhere in the block, prefix or not, bullet head
        // or prose. Neither scan around this one can see those; see
        // [`RETIRED_TOOL_NAMES`].
        for retired in RETIRED_TOOL_NAMES {
            assert!(
                !INSTRUCTIONS.contains(retired),
                "the instructions name `{retired}`, which no longer exists"
            );
        }
        for name in &mentioned {
            assert!(
                advertised.contains(name)
                    || PENDING_TOOLS.iter().any(|(pending, _)| pending == name),
                "the instructions name {name}, which is neither advertised nor \
                 listed in PENDING_TOOLS"
            );
        }
    }

    /// Decision 22: the merged spawn tool keeps the name every prompt, skill,
    /// workflow and doc already uses — the whole reason the operator merged it
    /// into the workspace extension instead of adding a second spawn tool
    /// beside it. Every pre-merge parameter has to survive the move, or a
    /// config that passes `settings`/`extensions` silently starts failing
    /// schema validation.
    #[tokio::test]
    async fn the_workspace_extension_advertises_the_spawn_tool_under_its_existing_name() {
        let c = client();
        let tools = c
            .list_tools(None, CancellationToken::new())
            .await
            .unwrap()
            .tools;
        let spawn = tools
            .iter()
            .find(|t| t.name == "subagent")
            .expect("the merged spawn tool keeps its name (decision 22)");
        // Every pre-merge parameter survives …
        let props = spawn.input_schema.get("properties").unwrap();
        for field in [
            "instructions",
            "subworkflow",
            "parameters",
            "extensions",
            "settings",
            "summary",
        ] {
            assert!(props.get(field).is_some(), "lost parameter {field}");
        }
        // … plus the BR-71 additions.
        assert!(props.get("visible").is_some());
        assert!(props.get("placement").is_some());
    }

    #[tokio::test]
    async fn the_extension_arm_for_the_spawn_tool_directs_to_dispatch_rather_than_panicking() {
        // Unreachable in practice — Agent::dispatch_tool_call intercepts the
        // name first — but a reachable arm must not panic.
        let c = client();
        let args: rmcp::model::JsonObject =
            serde_json::from_value(serde_json::json!({ "instructions": "x" })).unwrap();
        let result = c
            .call_tool(
                "subagent",
                Some(args),
                test_meta(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0]
            .as_text()
            .unwrap()
            .text
            .contains("agent loop"));
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
        // §6 injection budget. Raised 2500 → 2800 when the panel pair landed:
        // this text is injected on EVERY turn, so the number exists to force a
        // decision rather than to be nudged. The decision here was that an
        // agent which cannot be told it may look at the user's screen cannot
        // use the feature at all, and that both entries earn their line —
        // they were cut to two lines each first, not after.
        assert!(instructions.len() <= 2800, "injection budget (§6)");
        // The "no tool that is unimplemented AT A PHASE GATE may be named"
        // assertion that used to live here named `workspace_open` specifically
        // and was true only up to Task 24, which registers it. The general form
        // of the rule is enforced without going stale by
        // `advertises_no_tool_whose_handler_is_still_a_placeholder` (nothing in
        // PENDING_TOOLS is advertised) and by
        // `workspace_open_is_advertised_and_completes_the_surface` (the block
        // names exactly what get_tools() registers).
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

    /// The DEFAULT scope must see a running child. A registered agent lives in
    /// `AgentManager`'s PINNED sidecar, never in the `sessions` LRU — so this
    /// passes only because Task 33 makes `has_session` consult the pin. Delete
    /// that one line and this is the test that goes red.
    ///
    /// ⚠ **Two keys, and both are load-bearing.**
    ///
    /// `serial(agent_manager_pin)`: the pin is a process-global map keyed by
    /// session **id**, and ids are minted per *store* as `<date>_<n>`
    /// (`session_manager.rs`'s `SELECT MAX(CAST(SUBSTR(id, 10) AS INTEGER))`),
    /// so every test that stands up its own `TempDir` `SessionManager` and
    /// registers its FIRST session is fighting over the single key `<today>_1`.
    /// `subagent_handler`'s two real-subagent tests are the other claimants.
    /// Unserialized, this test can pass on one of THEIR pins (vacuous) or their
    /// poll can expire against ours (a flake) — the same `<today>_1` collision
    /// that already had to be fixed one layer down on the session bus.
    ///
    /// `parallel(workspace_services)`: this test READS the process-global
    /// services slot and needs the headless answer — `running` false for every
    /// row and no layout — because `has_session` is then the ONLY branch of the
    /// `"open"` predicate that can be true, which is the whole point. A test
    /// that overrode the slot with a stand-in reporting the target busy would
    /// make this pass with the pin consult deleted.
    #[tokio::test]
    #[serial_test::parallel(workspace_services)]
    #[serial_test::serial(agent_manager_pin)]
    async fn the_default_scope_sees_a_registered_child_with_no_gui_tab() {
        let c = client();
        let sm = c.context.session_manager.clone();
        let child = sm
            .create_session(
                std::env::temp_dir(),
                "registered".into(),
                crate::session::session_manager::SessionType::SubAgent,
            )
            .await
            .unwrap();

        let manager = crate::execution::manager::AgentManager::instance()
            .await
            .unwrap();
        let agent = std::sync::Arc::new(crate::agents::Agent::with_config(
            crate::agents::AgentConfig::new(
                sm.clone(),
                crate::config::permission::PermissionManager::instance(),
                None,
                crate::config::BioRouterMode::Auto,
            ),
        ));
        manager
            .register_agent(child.id.clone(), agent.clone())
            .await;

        // No `scope` key at all -> the default "open".
        let args: rmcp::model::JsonObject =
            serde_json::from_value(serde_json::json!({ "include_subagents": true })).unwrap();
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
        manager.deregister_agent_if_same(&child.id, &agent).await;

        assert!(
            text.contains(&child.id),
            "a registered child with no GUI tab must be in the default scope: {text}"
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

    // ------------------------------------------------------------------
    // Issue #56, design §7 column C — the release blocker.
    //
    // A PUBLIC-capability caller reached a PRIVATE conversation through
    // these tools. `privacy::visibility` shipped the matrix that rules it
    // and no handler called it; the only check here was
    // `session_type == Hidden`, which is a different rule about a
    // different thing. The tests below drive each wired path with a real
    // private row in a real store.
    // ------------------------------------------------------------------

    /// A string that appears in the private conversation and nowhere else, so
    /// "the transcript came back" is an assertion rather than an impression.
    ///
    /// Unmistakably a fixture. These tests run against a throwaway temp
    /// database (`client()`), not the developer's own, but a marker shaped like
    /// a record would still be the wrong thing to teach.
    const PRIVATE_MARKER: &str = "workspace-tier-fixture-not-real-data";

    struct TierFixture {
        client: WorkspaceClient,
        /// Classified `private` through the store's own monotone ratchet.
        private_id: String,
        /// Classified `public`, with its own marker, so every refusal below can
        /// be shown to be about the TIER rather than about a handler that
        /// refuses whatever it is given.
        public_id: String,
        /// A syntactically ordinary id that names no row in this store.
        absent_id: String,
    }

    /// One private conversation, one public one, and an id that does not exist.
    ///
    /// The private row is raised the way a real one gets there — through
    /// `raise_privacy`, the monotone ratchet the storage layer owns — rather
    /// than by writing the column, so what these tests refuse is the same state
    /// a user's own chat reaches.
    async fn tier_fixture() -> TierFixture {
        use crate::conversation::message::Message;
        use crate::session::session_manager::SessionType;
        let c = client();
        let sm = c.context.session_manager.clone();

        let private = sm
            .create_session(std::env::temp_dir(), "priv".into(), SessionType::User)
            .await
            .unwrap();
        let public = sm
            .create_session(std::env::temp_dir(), "pub".into(), SessionType::User)
            .await
            .unwrap();
        for id in [&private.id, &public.id] {
            let mut m = Message::user().with_text(PRIVATE_MARKER);
            sm.add_message_adopting_uid(id, &mut m).await.unwrap();
        }
        sm.update(&private.id)
            .raise_privacy(
                crate::privacy::SessionClassification::Private,
                "test:workspace-tier-fixture",
            )
            .apply()
            .await
            .unwrap();
        // The ratchet really fired. Without this the whole file could pass
        // against a fixture that is merely public — the one way these tests
        // could lie in the dangerous direction.
        assert_eq!(
            sm.get_session(&private.id, false)
                .await
                .unwrap()
                .privacy_tier,
            crate::privacy::SessionClassification::Private,
            "the fixture is not private, so nothing below is testing the gate"
        );

        TierFixture {
            client: c,
            private_id: private.id,
            public_id: public.id,
            absent_id: unique_id("no-such-conversation"),
        }
    }

    fn meta_for(cap: crate::privacy::CallCapability) -> crate::agents::mcp_client::McpMeta {
        crate::agents::mcp_client::McpMeta::new("tier-caller", cap)
    }

    /// A chat running on a public model, with the feature on. The capability
    /// `test_meta()` already carries; named here so each assertion says which
    /// side of the matrix it is on.
    fn public_caller() -> crate::agents::mcp_client::McpMeta {
        meta_for(crate::privacy::CallCapability::for_test(
            crate::privacy::ProviderTier::Public,
            true,
        ))
    }

    /// A chat running on a model hosted inside the institution.
    fn private_caller() -> crate::agents::mcp_client::McpMeta {
        meta_for(crate::privacy::CallCapability::for_test(
            crate::privacy::ProviderTier::Private,
            true,
        ))
    }

    /// A public chat on a machine where the user turned the whole feature off
    /// (DR-15).
    fn opted_out_caller() -> crate::agents::mcp_client::McpMeta {
        meta_for(crate::privacy::CallCapability::for_test(
            crate::privacy::ProviderTier::Public,
            false,
        ))
    }

    async fn call_as(
        c: &WorkspaceClient,
        tool: &str,
        args: serde_json::Value,
        meta: crate::agents::mcp_client::McpMeta,
    ) -> CallToolResult {
        let args: rmcp::model::JsonObject = serde_json::from_value(args).unwrap();
        c.call_tool(tool, Some(args), meta, CancellationToken::new())
            .await
            .unwrap()
    }

    /// **The blocker itself, as a named regression test.** A chat on a public
    /// model must not read a private conversation's transcript through
    /// `workspace_read_conversation`.
    ///
    /// Every view, because refusing only the default would leave `tool_calls` —
    /// the projection that shows exactly what that agent DID — as an unguarded
    /// back door, and `summary` carries the working directory besides.
    ///
    /// Both directions in one test on purpose: a private caller reads the very
    /// same row, so the refusal is provably about the tier rather than about a
    /// handler that fails on everything, or about a fixture nobody could read.
    #[tokio::test]
    async fn a_public_caller_cannot_read_a_private_transcript_through_the_workspace_tool() {
        let f = tier_fixture().await;
        for view in ["transcript", "tool_calls", "summary", "spawn_context"] {
            let refused = call_as(
                &f.client,
                "workspace_read_conversation",
                serde_json::json!({ "session_id": f.private_id, "view": view }),
                public_caller(),
            )
            .await;
            let text = text_of(&refused);
            assert_eq!(refused.is_error, Some(true), "view {view} was not refused");
            assert!(
                !text.contains(PRIVATE_MARKER),
                "view {view} returned the private conversation: {text}"
            );
            assert!(
                text.contains(&crate::privacy::refusal::workspace_out_of_reach()),
                "view {view} was refused for some other reason: {text}"
            );
            // §14.4 / R10: the refusal names nothing about the conversation.
            assert!(
                !text.contains(&f.private_id),
                "view {view} leaked the id: {text}"
            );
        }

        // …and the same row, to a caller entitled to it.
        let allowed = call_as(
            &f.client,
            "workspace_read_conversation",
            serde_json::json!({ "session_id": f.private_id }),
            private_caller(),
        )
        .await;
        let text = text_of(&allowed);
        assert_ne!(allowed.is_error, Some(true), "{text}");
        assert!(
            text.contains(PRIVATE_MARKER),
            "a private caller could not read a private conversation: {text}"
        );

        // …and a PUBLIC conversation is untouched by the gate, which is the half
        // a barrier written only for its refusal loses.
        let public = call_as(
            &f.client,
            "workspace_read_conversation",
            serde_json::json!({ "session_id": f.public_id }),
            public_caller(),
        )
        .await;
        let text = text_of(&public);
        assert_ne!(public.is_error, Some(true), "{text}");
        assert!(text.contains(PRIVATE_MARKER), "{text}");
    }

    /// DR-15's master opt-out reaches this gate too: with tiers off, nothing is
    /// refused.
    ///
    /// Read off the capability's own sample rather than the process-global
    /// toggle, so this asserts the conjunct rather than a global some other test
    /// in this binary might have moved.
    #[tokio::test]
    async fn the_master_opt_out_turns_the_workspace_tier_gate_off() {
        let f = tier_fixture().await;
        let result = call_as(
            &f.client,
            "workspace_read_conversation",
            serde_json::json!({ "session_id": f.private_id }),
            opted_out_caller(),
        )
        .await;
        let text = text_of(&result);
        assert_ne!(result.is_error, Some(true), "{text}");
        assert!(
            text.contains(PRIVATE_MARKER),
            "the opt-out did not reach the workspace gate: {text}"
        );
    }

    /// §7 row 1: a private conversation is **omitted** from a public caller's
    /// `workspace_list`, not redacted — its `name` is LLM-generated from the
    /// conversation and its `working_dir` routinely names a cohort.
    ///
    /// The paging metadata is asserted as well as the rows. `total_matching` is
    /// what a model pages against, so a filter applied after the count would
    /// leave "3 matched, 2 returned, has_more" — an existence oracle with a
    /// number attached, which is exactly what omission is for.
    #[tokio::test]
    async fn a_private_conversation_is_omitted_from_a_public_callers_list() {
        let f = tier_fixture().await;
        let rows = |result: &CallToolResult| -> serde_json::Value {
            serde_json::from_str(&text_of(result)).expect("workspace_list returns JSON")
        };

        let public = rows(
            &call_as(
                &f.client,
                "workspace_list",
                serde_json::json!({ "scope": "all" }),
                public_caller(),
            )
            .await,
        );
        let ids = sorted_ids(&public);
        assert!(
            ids.contains(&f.public_id),
            "the public conversation vanished too, so this proves nothing: {ids:?}"
        );
        assert!(
            !ids.contains(&f.private_id),
            "a public caller listed a private conversation: {ids:?}"
        );
        assert_eq!(
            public["total_matching"].as_u64().unwrap(),
            ids.len() as u64,
            "the omitted row was still counted, which is an existence oracle: {public}"
        );
        // The title is content, and it is the reason omission was chosen over
        // redaction — so assert it never appears anywhere in the payload, not
        // merely that the id is absent from the row list.
        assert!(
            !public.to_string().contains("\"priv\""),
            "the private conversation's title reached a public caller: {public}"
        );

        // A private caller sees both, so the omission is the tier and not a
        // scope filter that happens to drop the row.
        let private = rows(
            &call_as(
                &f.client,
                "workspace_list",
                serde_json::json!({ "scope": "all" }),
                private_caller(),
            )
            .await,
        );
        let ids = sorted_ids(&private);
        assert!(ids.contains(&f.private_id), "{ids:?}");
        assert!(ids.contains(&f.public_id), "{ids:?}");
    }

    /// §7 row 5. `workspace_send_prompt` is on the gated list as a **reader** as
    /// well as a writer: `mode:"turn"` with `wait:"final_message"` returns the
    /// target's final assistant message verbatim.
    ///
    /// Driven through `mode:"note"`, which needs no daemon — and which makes the
    /// refusal checkable as an ABSENCE OF EFFECT rather than as a sentence: the
    /// note must not be in the conversation afterwards, read back by a caller
    /// that is allowed to look.
    #[tokio::test]
    async fn a_public_caller_cannot_inject_into_a_private_conversation() {
        let f = tier_fixture().await;
        const INJECTED: &str = "workspace-tier-injection-marker";

        let refused = call_as(
            &f.client,
            "workspace_send_prompt",
            serde_json::json!({
                "session_id": f.private_id, "text": INJECTED, "mode": "note"
            }),
            public_caller(),
        )
        .await;
        let text = text_of(&refused);
        assert_eq!(refused.is_error, Some(true), "{text}");
        assert!(
            text.contains(&crate::privacy::refusal::workspace_out_of_reach()),
            "refused for some other reason: {text}"
        );

        // Nothing was written. A refusal that reported failure after appending
        // would pass every assertion above.
        let after = text_of(
            &call_as(
                &f.client,
                "workspace_read_conversation",
                serde_json::json!({ "session_id": f.private_id }),
                private_caller(),
            )
            .await,
        );
        assert!(
            !after.contains(INJECTED),
            "the refused injection landed in the private conversation anyway: {after}"
        );

        // The same call into a PUBLIC conversation is accepted, so the refusal
        // is the tier rather than the mode.
        let ok = call_as(
            &f.client,
            "workspace_send_prompt",
            serde_json::json!({
                "session_id": f.public_id, "text": INJECTED, "mode": "note"
            }),
            public_caller(),
        )
        .await;
        assert_ne!(ok.is_error, Some(true), "{}", text_of(&ok));
    }

    /// §7 row 4: opening an existing conversation is a read.
    ///
    /// `workspace_services` is pinned to "no daemon" because the accepted arm
    /// below would otherwise talk to whatever fake another test in this binary
    /// installed.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn a_public_caller_cannot_open_a_private_conversation() {
        crate::workspace_services::set_for_tests(None);
        let f = tier_fixture().await;

        let refused = call_as(
            &f.client,
            "workspace_open",
            serde_json::json!({ "session_id": f.private_id }),
            public_caller(),
        )
        .await;
        let text = text_of(&refused);
        assert_eq!(refused.is_error, Some(true), "{text}");
        assert!(
            text.contains(&crate::privacy::refusal::workspace_out_of_reach()),
            "refused for some other reason: {text}"
        );

        let ok = call_as(
            &f.client,
            "workspace_open",
            serde_json::json!({ "session_id": f.public_id }),
            public_caller(),
        )
        .await;
        assert_ne!(ok.is_error, Some(true), "{}", text_of(&ok));

        crate::workspace_services::clear_test_override();
    }

    /// **The refusal is not a classification oracle.** §14.4 / R10, and the
    /// reason §7 omits private rows from the list rather than redacting them: a
    /// model that could tell "private" from "does not exist" would rebuild the
    /// omitted list one id at a time.
    ///
    /// Asserted on the whole result — `is_error` and every byte of the text —
    /// because either half alone is an oracle: two different statuses enumerate
    /// private conversations just as well as two different sentences do.
    ///
    /// The other direction is what keeps this from being satisfied by a handler
    /// that refuses everything: a caller entitled to the difference is told it.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn the_refusal_cannot_tell_a_private_conversation_from_one_that_does_not_exist() {
        crate::workspace_services::set_for_tests(None);
        let f = tier_fixture().await;

        for (tool, extra) in [
            ("workspace_read_conversation", serde_json::json!({})),
            (
                "workspace_send_prompt",
                serde_json::json!({ "text": "hello", "mode": "note" }),
            ),
            ("workspace_open", serde_json::json!({})),
        ] {
            let args = |id: &str| {
                let mut a = extra.clone();
                a["session_id"] = serde_json::json!(id);
                a
            };
            let private = call_as(&f.client, tool, args(&f.private_id), public_caller()).await;
            let absent = call_as(&f.client, tool, args(&f.absent_id), public_caller()).await;
            assert_eq!(
                (private.is_error, text_of(&private)),
                (absent.is_error, text_of(&absent)),
                "{tool} tells a public caller whether the conversation exists"
            );

            // …and a private caller IS told, which is what makes the equality
            // above a property of the refusal rather than of a tool that answers
            // the same thing to everyone.
            let private = call_as(&f.client, tool, args(&f.private_id), private_caller()).await;
            let absent = call_as(&f.client, tool, args(&f.absent_id), private_caller()).await;
            assert_ne!(
                (private.is_error, text_of(&private)),
                (absent.is_error, text_of(&absent)),
                "{tool} refuses a caller that is entitled to the difference"
            );
        }

        crate::workspace_services::clear_test_override();
    }

    /// **The Hidden rule survives, and it is a different rule.** §5's "no covert
    /// reads" is about a session TYPE — a machine-internal conversation — while
    /// the tier gate is about a CLASSIFICATION. Neither substitutes for the
    /// other, and the way that is asserted is that a hidden conversation is
    /// refused to a **private** caller, whom the tier gate lets straight through.
    ///
    /// The second half is the ordering: a hidden conversation that is also
    /// private must refuse a public caller with the *tier* sentence, so it never
    /// learns that a hidden session with that id exists.
    #[tokio::test]
    async fn a_hidden_conversation_is_refused_whatever_the_callers_tier() {
        use crate::conversation::message::Message;
        use crate::session::session_manager::SessionType;
        let c = client();
        let sm = c.context.session_manager.clone();
        let hidden = sm
            .create_session(std::env::temp_dir(), "h".into(), SessionType::Hidden)
            .await
            .unwrap();
        let mut m = Message::user().with_text(PRIVATE_MARKER);
        sm.add_message_adopting_uid(&hidden.id, &mut m)
            .await
            .unwrap();

        for meta in [public_caller(), private_caller(), opted_out_caller()] {
            let result = call_as(
                &c,
                "workspace_read_conversation",
                serde_json::json!({ "session_id": hidden.id }),
                meta,
            )
            .await;
            let text = text_of(&result);
            assert_eq!(
                result.is_error,
                Some(true),
                "a hidden session was read: {text}"
            );
            assert!(text.contains("hidden"), "{text}");
            assert!(!text.contains(PRIVATE_MARKER), "{text}");
        }

        // Hidden AND private: the public caller meets the tier gate first, so it
        // is not told that a hidden conversation with this id exists.
        sm.update(&hidden.id)
            .raise_privacy(
                crate::privacy::SessionClassification::Private,
                "test:workspace-tier-fixture",
            )
            .apply()
            .await
            .unwrap();
        let result = call_as(
            &c,
            "workspace_read_conversation",
            serde_json::json!({ "session_id": hidden.id }),
            public_caller(),
        )
        .await;
        let text = text_of(&result);
        assert_eq!(result.is_error, Some(true), "{text}");
        assert!(
            !text.contains("hidden"),
            "the tier refusal disclosed that the conversation is hidden: {text}"
        );
        assert!(
            text.contains(&crate::privacy::refusal::workspace_out_of_reach()),
            "{text}"
        );
        // …and a private caller still meets the Hidden rule, unchanged.
        let result = call_as(
            &c,
            "workspace_read_conversation",
            serde_json::json!({ "session_id": hidden.id }),
            private_caller(),
        )
        .await;
        assert_eq!(result.is_error, Some(true));
        assert!(text_of(&result).contains("hidden"));
    }

    // ------------------------------------------------------------------
    // Issue #56, findings 4 and 15 — the four doors this file still had
    // open after the round above wired the other four.
    //
    //   4  `workspace_set_tools {add_extensions}` and
    //      `workspace_open {new:{extensions}}` ENABLE an extension with no
    //      Gate F1 check, and the second also discarded the operator's
    //      `enabled: false`.
    //  15  `workspace_watch` and `workspace_close` act on — and report on —
    //      a conversation §7 refuses this caller even a read of.
    // ------------------------------------------------------------------

    /// A genuinely private extension, straight out of the compiled marketplace
    /// baseline (`privacy::registry_private::PRIVATE_EXTENSIONS`).
    ///
    /// Read off the real table rather than typed as a literal: a test that
    /// hardcodes a name keeps passing after the name leaves the private set,
    /// asserting a refusal the shipped build no longer produces.
    fn a_private_extension() -> &'static str {
        crate::privacy::private_extension_ids()
            .next()
            .expect("the marketplace baseline publishes at least one private extension")
    }

    /// Gate F1's refusal for `name`, composed by the shared function itself.
    ///
    /// Asserting against this rather than against a prose fragment is what pins
    /// the *reuse*: an equivalent-but-local re-spelling of the predicate would
    /// produce different words and fail here, which is the whole point of
    /// "do not write a second spelling".
    fn expected_private_extension_refusal(name: &str) -> String {
        crate::privacy::refusal::privacy_refusal(
            name,
            crate::privacy::ProviderTier::Private,
            crate::privacy::ProviderTier::Public,
        )
        .expect("a private extension refuses a public caller")
        .message
        .to_string()
    }

    /// A task-local `extensions:` map, so a test can install an extension —
    /// or pin one off — **without writing the developer's `config.yaml`**.
    ///
    /// `Config::get_param` consults the task-local override before the
    /// environment and before the file, and `get_extensions_map` /
    /// `persisted_extension_names` both go through it, so an entry supplied
    /// here is visible to the gate as an operator-authored one. That is the
    /// only machine-independent way to exercise the `enabled: false` arm:
    /// `extension_entry_is_persisted` answers from the real config file, so a
    /// synthetic entry that is not in *some* config can never trip it.
    fn extensions_override(
        key: &str,
        name: &str,
        enabled: bool,
    ) -> std::collections::HashMap<String, String> {
        let map = serde_json::json!({
            key: {
                "enabled": enabled,
                "type": "stdio",
                "name": name,
                "description": "br71 privacy fixture",
                "cmd": "/nonexistent/br71-fixture",
                "args": [],
                "timeout": 1,
            }
        });
        std::collections::HashMap::from([("EXTENSIONS".to_string(), map.to_string())])
    }

    /// **Finding 4, door 1.** `workspace_set_tools {add_extensions}` is an
    /// extension-ENABLE door, and it had no tier check at all — while the
    /// instruction block above points the model straight at it ("do this
    /// yourself instead of pointing at Settings").
    ///
    /// Enabling is not a call INTO a private server; it is the call that
    /// SPAWNS one, pulling its credentials out of the keychain. Gate C refusing
    /// the first tool call afterwards is already too late, which is why Gate F1
    /// exists at `manage_extensions` and why it has to exist here.
    ///
    /// Three properties, because two of them are how the gate could be wrong
    /// while looking right:
    ///
    ///  * the refusal is Gate F1's own words, not a local paraphrase;
    ///  * it does not depend on whether the extension is INSTALLED — an
    ///    installed-only gate answers "unknown extension 'x'" otherwise, which
    ///    tells a public model exactly which private connectors this machine
    ///    has;
    ///  * a caller entitled to it gets a different answer, so the refusal is
    ///    about the tier and not about a handler that refuses this name.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn set_tools_refuses_a_public_caller_a_private_extension() {
        crate::workspace_services::set_for_tests(None);
        let c = client();
        let target = seeded_target(&c, "set-tools-tier").await;
        let private_ext = a_private_extension();
        let args = serde_json::json!({
            "session_id": target, "add_extensions": [private_ext]
        });

        let refused = call_as(&c, "workspace_set_tools", args.clone(), public_caller()).await;
        let uninstalled = text_of(&refused);
        assert_eq!(refused.is_error, Some(true), "{uninstalled}");
        assert!(
            uninstalled.contains(&expected_private_extension_refusal(private_ext)),
            "not Gate F1's refusal: {uninstalled}"
        );

        // …and the identical sentence when the connector IS installed and
        // enabled, so the refusal is not an installation oracle.
        let installed = crate::config::with_config_overrides(
            extensions_override(private_ext, private_ext, true),
            call_as(&c, "workspace_set_tools", args.clone(), public_caller()),
        )
        .await;
        assert_eq!(
            (installed.is_error, text_of(&installed)),
            (refused.is_error, uninstalled.clone()),
            "the refusal tells a public caller whether the private connector is installed"
        );

        // The other direction. A private caller is NOT refused on tier grounds —
        // it gets the ordinary "you named something that is not installed",
        // which is also why this half never tries to spawn anything.
        let allowed = call_as(&c, "workspace_set_tools", args, private_caller()).await;
        let text = text_of(&allowed);
        assert!(
            text.contains("unknown extension") && !text.contains("private extension"),
            "a private caller met the tier gate: {text}"
        );

        crate::workspace_services::clear_test_override();
    }

    /// An MCP server with nothing in it, so a test can put a **loaded**
    /// extension under a chosen name into a real `ExtensionManager` without
    /// spawning `biorouter mcp <name>`.
    ///
    /// Every `ServerHandler` method has a default, so the empty impl is the
    /// whole server: the unload gate reads the extensions map and the entry's
    /// config, never the server's behaviour.
    #[derive(Clone)]
    struct NullServer;

    impl rmcp::ServerHandler for NullServer {}

    /// **Finding 14's SECOND door.** `manage_extensions {disable}` has asked
    /// `assert_extension_manageable` since finding 14 landed — and
    /// `workspace_set_tools {remove_extensions}` reached the same executor
    /// (`Agent::remove_extension`, a passthrough to
    /// `ExtensionManager::remove_extension`) with no privacy decision anywhere
    /// on the path. One capability, gated at one entrance and open at the other.
    ///
    /// Driven with a **loaded** connector and asserted as an ABSENCE OF EFFECT,
    /// because a handler that unloads the server and then reports a refusal
    /// passes every prose assertion: the private extension must still be in the
    /// target's extensions map afterwards.
    ///
    /// Three arms, because two of them are how the gate could be wrong while
    /// looking right:
    ///
    ///  * the private connector survives a public caller's unload, with Gate
    ///    F1's own sentence — not a local paraphrase;
    ///  * a **public** extension, loaded in the same manager, is still
    ///    unloadable by that same caller, so this is not a blanket refusal of
    ///    `remove_extensions`;
    ///  * the connector NOT being loaded produces the identical sentence, so
    ///    the refusal is not the existence oracle finding 13 closed next door.
    ///
    /// The target row is **public**, so `refuse_unless_visible` is provably not
    /// what refused: what is being tested is the EXTENSION's tier against the
    /// caller's capability, which is the axis the finding names.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn set_tools_refuses_a_public_caller_the_unload_of_a_private_extension() {
        crate::workspace_services::set_for_tests(None);
        let c = client();
        let target = seeded_target(&c, "set-tools-unload").await;
        let private_ext = a_private_extension();

        let manager = crate::execution::manager::AgentManager::instance()
            .await
            .expect("agent manager");
        let agent = manager
            .get_or_create_agent(target.clone())
            .await
            .expect("agent");
        // Loaded under the normalized key the gate and the executor both resolve.
        for name in [private_ext, "developer"] {
            agent
                .extension_manager
                .add_inprocess_server(name, NullServer)
                .await
                .expect("in-process server");
        }

        let unload = |name: &str| {
            serde_json::json!({
                "session_id": target, "remove_extensions": [name]
            })
        };

        let refused = call_as(
            &c,
            "workspace_set_tools",
            unload(private_ext),
            public_caller(),
        )
        .await;
        let loaded_refusal = text_of(&refused);
        // FIRST, and deliberately: the assertion the prose cannot make for
        // itself. An ungated handler unloads the connector and then reports a
        // failure of its own (the persist step, which cannot find the row) —
        // which satisfies `is_error` while the damage is already done.
        assert!(
            agent
                .extension_manager
                .is_extension_enabled(private_ext)
                .await,
            "the public model unloaded the private connector: {loaded_refusal}"
        );
        assert_eq!(refused.is_error, Some(true), "{loaded_refusal}");
        assert!(
            loaded_refusal.contains(&expected_private_extension_refusal(private_ext)),
            "not Gate F1's refusal: {loaded_refusal}"
        );

        // …and the same caller still unloads a PUBLIC extension, so the gate is
        // the tier and not a refusal of the whole argument. Asserted on the
        // manager rather than on the sentence: this test's session row lives in
        // the client's own store, so the persist step at the end of
        // `apply_extension_changes` may fail after the unload has happened.
        let public_unload = call_as(
            &c,
            "workspace_set_tools",
            unload("developer"),
            public_caller(),
        )
        .await;
        let public_text = text_of(&public_unload);
        assert!(
            !public_text.contains("private extension"),
            "a public extension met the tier gate: {public_text}"
        );
        assert!(
            !agent
                .extension_manager
                .is_extension_enabled("developer")
                .await,
            "the public extension was not unloaded: {public_text}"
        );

        // §14.4 / R10: "installed", "not installed" and "no such extension" are
        // one indistinguishable refusal for a caller that may not touch it.
        agent
            .extension_manager
            .remove_extension(private_ext)
            .await
            .expect("remove_extension is idempotent");
        let absent = call_as(
            &c,
            "workspace_set_tools",
            unload(private_ext),
            public_caller(),
        )
        .await;
        assert_eq!(
            (absent.is_error, text_of(&absent)),
            (refused.is_error, loaded_refusal),
            "the refusal tells a public caller whether the private connector is loaded"
        );

        crate::workspace_services::clear_test_override();
    }

    /// **Finding 4, door 2.** `workspace_open {new:{extensions}}` chooses what a
    /// brand-new conversation is BORN holding, and forwards the names to
    /// `start_session`, whose own resolution goes through the flag-less
    /// `get_extension_by_name`. So this door had neither Gate F1 nor #42's
    /// operator pin.
    ///
    /// The assertion that matters is not the sentence: it is that
    /// `sessions_started()` is EMPTY. A refusal issued after `start_session`
    /// would leave a real conversation on disk holding the private connector
    /// and merely report failure — every prose assertion would still pass.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn open_new_refuses_a_public_caller_a_conversation_born_private() {
        let recorder = FakeServices::with_gui(true).install();
        let c = client();
        let private_ext = a_private_extension();
        let args = serde_json::json!({ "new": {
            "kind": "user",
            "working_dir": std::env::temp_dir().to_string_lossy(),
            "extensions": [private_ext],
        }});

        let refused = call_as(&c, "workspace_open", args.clone(), public_caller()).await;
        let text = text_of(&refused);
        assert_eq!(refused.is_error, Some(true), "{text}");
        assert!(
            text.contains(&expected_private_extension_refusal(private_ext)),
            "not Gate F1's refusal: {text}"
        );
        assert!(
            recorder.sessions_started().is_empty(),
            "the refusal came AFTER the conversation was created: {:?}",
            recorder.sessions_started()
        );

        // A private caller starts exactly that conversation, so the refusal is
        // the caller's tier and not the argument.
        let ok = call_as(&c, "workspace_open", args, private_caller()).await;
        assert_ne!(ok.is_error, Some(true), "{}", text_of(&ok));
        let started = recorder.sessions_started();
        assert_eq!(started.len(), 1, "got {started:?}");
        assert_eq!(started[0].extensions, Some(vec![private_ext.to_string()]));

        crate::workspace_services::clear_test_override();
    }

    /// **Finding 4, door 2's second half: the operator's `enabled: false` was
    /// discarded.**
    ///
    /// `workspace_set_tools` has honoured #42's pin since Task 10
    /// (`resolve_added_extensions` deliberately resolves through
    /// `get_extension_entry_by_name`); `workspace_open {new:{extensions}}` did
    /// not, and forwarded the name to a daemon helper that drops the flag. So
    /// an agent could re-enable an extension the operator turned off simply by
    /// starting a fresh conversation with it — the pinned tool environment
    /// (benchmarking, safety) defeated in one call.
    ///
    /// Nothing about the TIER here: the fixture extension is public, so this
    /// arm is provably not the tier gate firing under another name.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn open_new_honours_an_extension_the_operator_pinned_off() {
        let recorder = FakeServices::with_gui(true).install();
        let c = client();
        const KEY: &str = "br71pinnedoff";
        const NAME: &str = "br71-pinned-off";
        let args = serde_json::json!({ "new": {
            "kind": "user",
            "working_dir": std::env::temp_dir().to_string_lossy(),
            "extensions": [NAME],
        }});

        let refused = crate::config::with_config_overrides(
            extensions_override(KEY, NAME, false),
            call_as(&c, "workspace_open", args.clone(), public_caller()),
        )
        .await;
        let text = text_of(&refused);
        assert_eq!(refused.is_error, Some(true), "{text}");
        assert!(
            text.contains("enabled: false") && text.contains(NAME),
            "not #42's refusal: {text}"
        );
        assert!(
            recorder.sessions_started().is_empty(),
            "a conversation was started holding the operator-disabled extension"
        );

        // The SAME name with the operator's flag on is accepted, so the refusal
        // is the flag and not the fixture.
        let ok = crate::config::with_config_overrides(
            extensions_override(KEY, NAME, true),
            call_as(&c, "workspace_open", args, public_caller()),
        )
        .await;
        assert_ne!(ok.is_error, Some(true), "{}", text_of(&ok));
        assert_eq!(recorder.sessions_started().len(), 1);

        crate::workspace_services::clear_test_override();
    }

    /// **The seam between finding 4's fix and finding 13's, at the door where it
    /// was open: `workspace_open {new:{extensions}}`.**
    ///
    /// Finding 13 established that #42's operator pin is an install-state
    /// oracle, and moved `manage_extensions`' tier arm above it. Finding 4's fix
    /// gated these two workspace doors the same afternoon, in different words
    /// and with the pin FIRST — which for `workspace_set_tools` was harmless
    /// (`resolve_added_extensions` asks with `entry: None` before the lookup, so
    /// the tier arm answers first anyway) but for this door was not: it looks
    /// the entry up and then asks, so a public caller naming a private connector
    /// this machine has and the operator pinned off was told exactly that.
    ///
    /// Three install states of one private connector, one caller who may not
    /// have it, one sentence. The `absent` arm is the reference because it is
    /// the state a caller can never learn anything from.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn open_new_tells_a_public_caller_nothing_about_a_private_extensions_install_state() {
        let recorder = FakeServices::with_gui(true).install();
        let c = client();
        let private_ext = a_private_extension();
        let args = serde_json::json!({ "new": {
            "kind": "user",
            "working_dir": std::env::temp_dir().to_string_lossy(),
            "extensions": [private_ext],
        }});

        let absent = text_of(&call_as(&c, "workspace_open", args.clone(), public_caller()).await);
        let installed = text_of(
            &crate::config::with_config_overrides(
                extensions_override(private_ext, private_ext, true),
                call_as(&c, "workspace_open", args.clone(), public_caller()),
            )
            .await,
        );
        let pinned_off = text_of(
            &crate::config::with_config_overrides(
                extensions_override(private_ext, private_ext, false),
                call_as(&c, "workspace_open", args.clone(), public_caller()),
            )
            .await,
        );

        assert!(
            absent.contains(&expected_private_extension_refusal(private_ext)),
            "not Gate F1's refusal: {absent}"
        );
        for (state, text) in [
            ("installed and enabled", &installed),
            ("installed and pinned off by the operator", &pinned_off),
        ] {
            assert_eq!(
                &absent, text,
                "the refusal tells a public caller that the private connector is {state}"
            );
        }
        assert!(
            !pinned_off.contains("enabled: false"),
            "#42's refusal, an answer about this machine, reached a caller who may not \
             have the connector at all: {pinned_off}"
        );
        assert!(
            recorder.sessions_started().is_empty(),
            "a conversation was started holding the private connector: {:?}",
            recorder.sessions_started()
        );

        // …and the pin is not swallowed: a caller ENTITLED to the connector still
        // meets #42 at this door, and still starts no conversation. Without this
        // the test above is satisfied by a gate that refuses everything.
        let entitled = crate::config::with_config_overrides(
            extensions_override(private_ext, private_ext, false),
            call_as(&c, "workspace_open", args, private_caller()),
        )
        .await;
        let text = text_of(&entitled);
        assert_eq!(entitled.is_error, Some(true), "{text}");
        assert!(
            text.contains("enabled: false"),
            "the reorder swallowed #42's pin for the caller it was written for: {text}"
        );
        assert!(recorder.sessions_started().is_empty(), "{text}");

        crate::workspace_services::clear_test_override();
    }

    /// The same property at the other workspace door. `workspace_set_tools`
    /// reached the right answer by a different route — it asks the gate once
    /// with no entry, before the lookup — so this pins the OUTCOME rather than
    /// that route: whatever order the two calls happen in, an operator pin must
    /// not become visible to a caller the tier arm refuses.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn set_tools_tells_a_public_caller_nothing_about_a_private_extensions_install_state() {
        crate::workspace_services::set_for_tests(None);
        let c = client();
        let target = seeded_target(&c, "set-tools-oracle").await;
        let private_ext = a_private_extension();
        let args = serde_json::json!({
            "session_id": target, "add_extensions": [private_ext]
        });

        let absent =
            text_of(&call_as(&c, "workspace_set_tools", args.clone(), public_caller()).await);
        let pinned_off = text_of(
            &crate::config::with_config_overrides(
                extensions_override(private_ext, private_ext, false),
                call_as(&c, "workspace_set_tools", args.clone(), public_caller()),
            )
            .await,
        );
        assert_eq!(
            absent, pinned_off,
            "the refusal tells a public caller that the private connector is installed \
             and pinned off"
        );
        assert!(
            absent.contains(&expected_private_extension_refusal(private_ext)),
            "not Gate F1's refusal: {absent}"
        );
        assert!(!pinned_off.contains("enabled: false"), "{pinned_off}");

        // The entitled caller still meets the pin here too.
        let entitled = text_of(
            &crate::config::with_config_overrides(
                extensions_override(private_ext, private_ext, false),
                call_as(&c, "workspace_set_tools", args, private_caller()),
            )
            .await,
        );
        assert!(
            entitled.contains("enabled: false"),
            "the reorder swallowed #42's pin at this door: {entitled}"
        );

        crate::workspace_services::clear_test_override();
    }

    /// **Finding 4's third door, found by enumerating what `workspace_set_tools`
    /// changes rather than what finding 4 named.** The tool rewrites another
    /// conversation's provider, extension set, session-scoped skills and
    /// knowledge bases, and had no §7 check on the TARGET at all — so a public
    /// caller could re-tool a private conversation it may not even read.
    ///
    /// Driven through `add_skills`, which needs no daemon and no agent, and
    /// asserted as an ABSENCE OF EFFECT: the skills override must not be on the
    /// private session afterwards, read back by a caller that is allowed to
    /// look.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn a_public_caller_cannot_retool_a_private_conversation() {
        crate::workspace_services::set_for_tests(None);
        let f = tier_fixture().await;
        let args =
            |id: &str| serde_json::json!({ "session_id": id, "add_skills": ["single-cell"] });

        let refused = call_as(
            &f.client,
            "workspace_set_tools",
            args(&f.private_id),
            public_caller(),
        )
        .await;
        let text = text_of(&refused);
        assert_eq!(refused.is_error, Some(true), "{text}");
        assert!(
            text.contains(&crate::privacy::refusal::workspace_out_of_reach()),
            "refused for some other reason: {text}"
        );

        let over = crate::agents::session_skills::for_session(
            &f.client.context.session_manager,
            &f.private_id,
        )
        .await
        .unwrap();
        assert!(
            !over.add.contains(&"single-cell".to_string()),
            "the refused re-tool landed on the private conversation anyway"
        );

        // §14.4 / R10: private and absent are the same answer.
        let absent = call_as(
            &f.client,
            "workspace_set_tools",
            args(&f.absent_id),
            public_caller(),
        )
        .await;
        assert_eq!(
            (absent.is_error, text_of(&absent)),
            (refused.is_error, text),
            "workspace_set_tools tells a public caller whether the conversation exists"
        );

        // …and the same call into a PUBLIC conversation is applied.
        let ok = call_as(
            &f.client,
            "workspace_set_tools",
            args(&f.public_id),
            public_caller(),
        )
        .await;
        assert_ne!(ok.is_error, Some(true), "{}", text_of(&ok));

        crate::workspace_services::clear_test_override();
    }

    /// **Finding 15, door 1: cross-conversation cancel.** All three
    /// `workspace_close` scopes act on another conversation — close the window
    /// the user is reading it in, cancel the work it is doing, evict its agent
    /// mid-flight — and none of them asked §7 anything.
    ///
    /// Asserted on the FakeServices call log, not on the sentence: a handler
    /// that cancels and then reports a refusal passes every prose assertion.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn a_public_caller_cannot_close_a_private_conversation() {
        let f = tier_fixture().await;
        let services = FakeServices::with_gui(true).busy(&f.private_id).install();

        for scope in ["tab", "turn", "agent"] {
            let refused = call_as(
                &f.client,
                "workspace_close",
                serde_json::json!({ "session_id": f.private_id, "scope": scope }),
                public_caller(),
            )
            .await;
            let text = text_of(&refused);
            assert_eq!(refused.is_error, Some(true), "scope {scope}: {text}");
            assert!(
                text.contains(&crate::privacy::refusal::workspace_out_of_reach()),
                "scope {scope} refused for some other reason: {text}"
            );
        }
        assert!(
            services.cancels().is_empty(),
            "a private turn was cancelled"
        );
        assert!(services.stops().is_empty(), "a private agent was evicted");
        assert!(
            services.all_frames().is_empty(),
            "a frame about a private conversation reached the GUI: {:?}",
            services.all_frames()
        );

        // The gate is the tier: the same scope on a PUBLIC conversation still
        // cancels, and a private caller may still close the private one.
        let ok = call_as(
            &f.client,
            "workspace_close",
            serde_json::json!({ "session_id": f.public_id, "scope": "turn" }),
            public_caller(),
        )
        .await;
        assert_ne!(ok.is_error, Some(true), "{}", text_of(&ok));
        let ok = call_as(
            &f.client,
            "workspace_close",
            serde_json::json!({ "session_id": f.private_id, "scope": "turn" }),
            private_caller(),
        )
        .await;
        assert_ne!(ok.is_error, Some(true), "{}", text_of(&ok));
        assert_eq!(
            services.cancels(),
            vec![f.public_id.clone(), f.private_id.clone()],
            "the cancels that DID happen are not the ones expected"
        );

        crate::workspace_services::clear_test_override();
    }

    /// **Finding 15, door 2: an activity oracle.** `workspace_watch` reports
    /// whether each named conversation is still working and, when it stops,
    /// why. Over a conversation §7 will not let this caller read, that is a
    /// live readout of a private chat's progress arriving through a tool whose
    /// name says "wait".
    ///
    /// The elapsed-time assertion is what distinguishes a refusal from a park:
    /// a gate placed after `subscribe`/`park_for_completions` would still
    /// return the refusal, one timeout later, having held a slot in the private
    /// conversation's event ring the whole time.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn a_public_caller_cannot_watch_a_private_conversation() {
        crate::workspace_services::set_for_tests(None);
        let f = tier_fixture().await;

        let started = std::time::Instant::now();
        let refused = call_as(
            &f.client,
            "workspace_watch",
            serde_json::json!({ "session_ids": [f.private_id], "timeout_s": 5 }),
            public_caller(),
        )
        .await;
        let elapsed = started.elapsed();
        let text = text_of(&refused);
        assert_eq!(refused.is_error, Some(true), "{text}");
        assert!(
            text.contains(&crate::privacy::refusal::workspace_out_of_reach()),
            "refused for some other reason: {text}"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(4),
            "the gate ran after the park rather than before it: {elapsed:?}"
        );

        // A batch is refused WHOLE. Reporting on the readable ids and silently
        // dropping the rest is the existence disclosure with one extra step,
        // since the caller supplied the list and can diff it.
        let mixed = call_as(
            &f.client,
            "workspace_watch",
            serde_json::json!({
                "session_ids": [f.public_id, f.private_id], "timeout_s": 1
            }),
            public_caller(),
        )
        .await;
        let text = text_of(&mixed);
        assert_eq!(mixed.is_error, Some(true), "{text}");
        assert!(
            !text.contains(&f.public_id),
            "the batch refusal reported on the readable conversation: {text}"
        );

        // …and the same watch, by a caller entitled to it, is the ordinary
        // timeout — not an error, and naming the conversation.
        let ok = call_as(
            &f.client,
            "workspace_watch",
            serde_json::json!({ "session_ids": [f.private_id], "timeout_s": 1 }),
            private_caller(),
        )
        .await;
        let text = text_of(&ok);
        assert_ne!(ok.is_error, Some(true), "{text}");
        assert!(text.contains(&f.private_id), "{text}");

        crate::workspace_services::clear_test_override();
    }

    /// DR-15's master opt-out reaches every newly gated door in this file, read
    /// off the capability's own sample rather than a process-global.
    ///
    /// Without this, "the feature is off" could mean five different things at
    /// five gates — and a gate that ignored the toggle would refuse a user who
    /// has switched the whole mechanism off, which is the one outcome DR-15
    /// forbids.
    ///
    /// The fifth arm is finding 14's second door (`remove_extensions`). It
    /// inherits the toggle rather than re-reading it — `assert_extension_reachable`
    /// asks `cap.enforced()` off the same sample that carried the tier — and
    /// this is what proves the inheritance rather than assuming it.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn the_master_opt_out_turns_every_new_workspace_gate_off() {
        let recorder = FakeServices::with_gui(true).install();
        let f = tier_fixture().await;
        let private_ext = a_private_extension();

        // Finding 15: close and watch reach the private conversation.
        for (tool, args) in [
            (
                "workspace_close",
                serde_json::json!({ "session_id": f.private_id, "scope": "turn" }),
            ),
            (
                "workspace_watch",
                serde_json::json!({ "session_ids": [f.private_id], "timeout_s": 1 }),
            ),
        ] {
            let result = call_as(&f.client, tool, args, opted_out_caller()).await;
            let text = text_of(&result);
            assert_ne!(result.is_error, Some(true), "{tool}: {text}");
            assert!(
                !text.contains("private"),
                "{tool} refused a caller with tiers switched off: {text}"
            );
        }

        // Finding 4: the private extension is enableable again. #42's operator
        // pin is NOT part of this — it is not a privacy tier and the master
        // switch does not silence it.
        let ok = call_as(
            &f.client,
            "workspace_open",
            serde_json::json!({ "new": {
                "kind": "user",
                "working_dir": std::env::temp_dir().to_string_lossy(),
                "extensions": [private_ext],
            }}),
            opted_out_caller(),
        )
        .await;
        assert_ne!(ok.is_error, Some(true), "{}", text_of(&ok));
        assert_eq!(recorder.sessions_started().len(), 1);

        let set_tools = call_as(
            &f.client,
            "workspace_set_tools",
            serde_json::json!({
                "session_id": f.public_id, "add_extensions": [private_ext]
            }),
            opted_out_caller(),
        )
        .await;
        let text = text_of(&set_tools);
        assert!(
            !text.contains("private extension"),
            "the opt-out did not reach the set_tools enable gate: {text}"
        );

        // Finding 14's second door: the UNLOAD half honours the same switch.
        let unload = call_as(
            &f.client,
            "workspace_set_tools",
            serde_json::json!({
                "session_id": f.public_id, "remove_extensions": [private_ext]
            }),
            opted_out_caller(),
        )
        .await;
        let text = text_of(&unload);
        assert!(
            !text.contains("private extension"),
            "the opt-out did not reach the set_tools unload gate: {text}"
        );

        crate::workspace_services::clear_test_override();
    }

    /// **The assertion the behavioural tests above cannot make: every gate this
    /// file owns is WIRED, and the dispatcher hands it the capability.**
    ///
    /// Nine unwired guards have shipped in this campaign — correct, tested, and
    /// called by nothing — and this file alone has produced two rounds of missed
    /// paths. A behavioural test cannot catch that class of defect for a door
    /// nobody thought to open, because the test for the missing door is the test
    /// nobody wrote. So this scans the production half of the source for the
    /// wiring itself, and it is written to fail the two ways a source scan
    /// usually lies:
    ///
    ///  * it cuts at `#[cfg(test)]` so the fixtures below cannot satisfy it, and
    ///    the `for_test_restricted` control proves the cut landed where it
    ///    claims (a `find` that returned the end of the file would otherwise
    ///    make every assertion here vacuous);
    ///  * it names the DISPATCH ARMS, not just the guards. A guard that exists,
    ///    is tested, and is never reached is the exact failure mode; a handler
    ///    that stopped taking `cap` would compile fine and silently return to
    ///    ungated, because a capability-less handler has nothing to ask with.
    #[test]
    fn every_gate_this_file_owns_is_wired_into_dispatch() {
        const SELF: &str = include_str!("workspace_extension.rs");
        // ⚠ Normalize line endings before scanning. `include_str!` embeds the
        // file's bytes verbatim, and a Windows checkout has CRLF ones (Git for
        // Windows defaults `core.autocrlf` to true and nothing in
        // `.gitattributes` pins `*.rs` to LF), so the `\n` in the needle below
        // matches nothing there and this whole guard dies on that platform
        // alone — which is exactly what it did. Every needle here is written
        // with `\n`, so normalizing once at the top is the fix that keeps them
        // all honest.
        let self_source = SELF.replace("\r\n", "\n");
        // Anchored on the WHOLE opening line, not on `#[cfg(test)]` alone: this
        // file has a `#[cfg(test)] const RETIRED_TOOL_NAMES` up in production,
        // and cutting there would drop most of the production text and make
        // every assertion below vacuously true.
        let cut = self_source
            .find("#[cfg(test)]\npub(crate) mod tests {")
            .expect("workspace_extension.rs no longer has a `#[cfg(test)] mod tests`");
        let (production, tests) = self_source.split_at(cut);
        assert!(
            !production.contains("for_test_restricted"),
            "the cut did not remove the test module, so the assertions below prove nothing"
        );
        assert!(
            tests.contains("for_test_restricted"),
            "the cut removed more than the test module"
        );

        // §7 column C, at every handler that names another conversation:
        // read_conversation, open (existing), send_prompt, set_tools, close,
        // watch, and the panel pair (which share one handler, hence one call
        // site for two tools). An exact count, so a further tool that names a
        // conversation cannot be added without either wiring the gate or
        // editing this number — which is the moment to think about it.
        assert_eq!(
            production
                .matches("self.refuse_unless_visible(cap,")
                .count(),
            7,
            "the number of §7-gated call sites changed. Seven handlers name \
             another conversation; if an eighth arrived it needs the gate, and if \
             one was removed this count needs to shrink deliberately."
        );

        // Gate F1, at the two doors that ENABLE an extension.
        for wiring in [
            "Self::refuse_gated_extension_enable(cap, name, None)",
            "Self::refuse_gated_extension_enable(cap, name, Some(&entry))",
            "Self::refuse_gated_extension_enable(cap, name, entry.as_ref())",
            "Self::resolve_added_extensions(cap, &args.add_extensions)",
            "Self::refuse_gated_new_session_extensions(cap, new.extensions.as_ref())",
        ] {
            assert!(
                production.contains(wiring),
                "`{wiring}` is gone: an extension-enable door lost its Gate F1 wiring"
            );
        }

        // …and that gate DECIDES nothing here. Its three arms and their order are
        // shared with `manage_extensions`' door through
        // `refusal::extension_enable_refusal`; this file's copy of them is what
        // reopened finding 13's oracle at `workspace_open`, because two spellings
        // of one rule agreed on every verdict and disagreed on the order. A
        // behavioural test cannot see a re-derivation that happens to agree
        // today, which is the only kind anyone ever writes.
        assert!(
            production.contains("crate::privacy::refusal::extension_enable_refusal("),
            "`refuse_gated_extension_enable` no longer asks the shared enable gate"
        );
        // ⚠ The list is [`ENABLE_GATE_RESPELLINGS`], shared with
        // `extension_manager_extension.rs`'s scan. It used to be a literal here
        // naming only THIS file's historic idiom, while the other file's scan
        // named only its own — so a re-derivation written in the other
        // vocabulary passed both. Adding an entry in one place now tightens
        // both doors, which is the only property that makes the guard a guard.
        asserts_no_respellings(production, "workspace_extension.rs");

        // …and Gate F1's UNLOAD half, at the one door in this file that takes an
        // extension AWAY (finding 14's second door). Spelled as the shared
        // predicate's own name rather than as a local rule, so a re-spelling of
        // the tier comparison here fails this assertion instead of passing it:
        // the whole defect was two doors to one capability answering with two
        // different pieces of code.
        assert!(
            production.contains(".assert_extension_manageable(name, cap)"),
            "`workspace_set_tools {{remove_extensions}}` lost Gate F1's unload wiring: it \
             reaches `Agent::remove_extension` again with no privacy decision on the path, \
             which is finding 14 with the other door open"
        );

        // …and the dispatcher hands each newly gated handler the capability the
        // call was ADMITTED on. Without this the guards above have nothing to
        // ask and the handlers go quietly back to ungated.
        //
        // ⚠ The needle stops at `cap,` on purpose. It used to pin the WHOLE
        // argument list — `(caller, cap, arguments)` — and #110 broke it by
        // giving `handle_watch` a fourth argument (the turn's cancel token, so a
        // parked watch is reachable by Stop), a change that threads the
        // capability exactly as before. A guard that fails on a legitimate
        // argument gets weakened by the next person in a hurry, and the way it
        // gets weakened is by deleting the row. Asserting the PROPERTY — this
        // handler is handed the admitted capability, second, right after the
        // caller — is what survives a growing signature while still failing the
        // thing it exists to catch: a handler that stops being given `cap` at
        // all, or that is given one it re-derived for itself.
        for arm in [
            "\"workspace_set_tools\" => self.handle_set_tools(caller, cap,",
            "\"workspace_close\" => self.handle_close(caller, cap,",
            "self.handle_watch(caller, cap,",
        ] {
            assert!(
                production.contains(arm),
                "dispatch no longer threads the capability: {arm}"
            );
        }
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
        let meta = crate::agents::mcp_client::McpMeta::new(
            caller.id.clone(),
            crate::privacy::CallCapability::for_test_restricted(),
        );
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
            "without the marker there is nothing to honour, so the assertion \
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

    /// One recorded `start_session` call, every argument of it.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct StartedSession {
        working_dir: std::path::PathBuf,
        extensions: Option<Vec<String>>,
        knowledge_bases: Vec<String>,
        primary: KbPrimaryChoice,
    }

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
        /// The `wait_result` each of those frames was sent with, in the same
        /// order. Recorded separately from `frames` so every existing frame
        /// assertion keeps its shape — and recorded AT ALL because "did the tool
        /// park for the renderer's answer?" is exactly the question `close_tab`
        /// got wrong: a fire-and-forget emit returns `{"sent": true}`, which no
        /// assertion about the frame itself can distinguish from a real reply.
        waits: Mutex<Vec<bool>>,
        /// Answers `gui_command` hands back, consumed in order; `{"ok": true}`
        /// once the queue is empty. A renderer that REFUSES (`ok:false`) is not
        /// an error — it is the normal way a split is declined, a tab is missing
        /// or a frame is queued — and without this the fake could only ever say
        /// yes.
        gui_answers: Mutex<std::collections::VecDeque<serde_json::Value>>,
        /// What `layout_snapshot` reports (§4.3 echo).
        layout: Mutex<Option<serde_json::Value>>,
        /// Every `start_session(…)` call, whole and in order. Task 24 needs the
        /// ARGUMENTS, not just the returned id: decision 5's deliverable is
        /// *which* directory a new conversation gets, and an implementation that
        /// hardcoded the process cwd (or `temp_dir()`, or `/`) would return the
        /// same id and satisfy every assertion that only looks at the answer.
        /// The same holds for the other three: `start_session` returns `s-new`
        /// whether or not the caller's extensions, knowledge bases and write
        /// target ever reached it.
        sessions_started: Mutex<Vec<StartedSession>>,
        /// Every `cancel_turn(session_id)`, in order. Task 16 needs the CALL
        /// recorded, not just its answer: a `workspace_close` that returned the
        /// right sentence without ever tripping the token is exactly the wrong
        /// implementation this records to exclude.
        cancels: Mutex<Vec<String>>,
        /// Every `stop_agent(session_id)`, in order — same reason.
        stops: Mutex<Vec<String>>,
        /// When set, `stop_agent` fails with it (and records the call anyway).
        stop_error: Mutex<Option<String>>,
        /// When set, `gui_command` fails with it (and records the frame anyway),
        /// the way a wedged renderer does: `emit_and_wait` gives up after 10s.
        gui_error: Mutex<Option<String>>,
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
        /// Make the GUI round-trip fail — a renderer that never answers, which
        /// `emit_and_wait` reports as an error after its 10 s timeout.
        fn gui_fails(self, message: &str) -> Self {
            *self.gui_error.lock().unwrap() = Some(message.to_string());
            self
        }
        /// Queue the renderer's answers, in order. A REFUSAL is not a transport
        /// failure: `planWorkspaceCommand` answers `ok:false` for a split it
        /// declined, a tab that is not there, and a frame it had to queue.
        fn gui_answers(self, answers: Vec<serde_json::Value>) -> Self {
            *self.gui_answers.lock().unwrap() = answers.into();
            self
        }
        /// A layout echo in which `session_id` already has a tab, in the shape
        /// `gui_tab_for` reads (`workspace_echo.layout`, §4.3).
        fn with_tab_for(self, session_id: &str) -> Self {
            *self.layout.lock().unwrap() = Some(serde_json::json!([{
                "window_id": "w-1",
                "layout": [{
                    "group_id": "g-1",
                    "active_tab": "t-other",
                    "tabs": [
                        { "tab_id": "t-1", "session_id": session_id },
                        { "tab_id": "t-other", "session_id": "s-someone-else" },
                    ],
                }],
            }]));
            self
        }
        /// The `wait_result` of every frame, in order.
        fn waits(&self) -> Vec<bool> {
            self.waits.lock().unwrap().clone()
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
        /// The first frame carrying this `cmd`, if any.
        fn frame_with_cmd(&self, cmd: &str) -> Option<serde_json::Value> {
            self.all_frames().into_iter().find(|f| f["cmd"] == cmd)
        }
        fn clear_frames(&self) {
            self.frames.lock().unwrap().clear();
        }
        fn sessions_started(&self) -> Vec<StartedSession> {
            self.sessions_started.lock().unwrap().clone()
        }
        /// Just the directories, for the decision-5 assertions.
        fn session_dirs(&self) -> Vec<std::path::PathBuf> {
            self.sessions_started()
                .into_iter()
                .map(|s| s.working_dir)
                .collect()
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
            self.layout.lock().unwrap().clone()
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
            working_dir: std::path::PathBuf,
            extensions: Option<Vec<String>>,
            knowledge_bases: Vec<String>,
            primary: KbPrimaryChoice,
        ) -> Result<String, String> {
            self.sessions_started.lock().unwrap().push(StartedSession {
                working_dir,
                extensions,
                knowledge_bases,
                primary,
            });
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
            wait_result: bool,
        ) -> Result<serde_json::Value, String> {
            self.frames.lock().unwrap().push(frame);
            self.waits.lock().unwrap().push(wait_result);
            // Bind out of the guard before the early return: a live `MutexGuard`
            // across the tail of an `async fn` makes the future `!Send`.
            let failure = self.gui_error.lock().unwrap().clone();
            let queued = self.gui_answers.lock().unwrap().pop_front();
            match failure {
                Some(message) => Err(message),
                // A fire-and-forget emit never carries an `ok` — the real
                // `ServerWorkspaceServices` answers `{"sent": true}` — so a
                // caller that did not wait cannot learn anything, and the fake
                // must not hand it a verdict it could not have had.
                None if !wait_result => Ok(serde_json::json!({ "sent": true })),
                None => Ok(queued.unwrap_or_else(|| serde_json::json!({ "ok": true }))),
            }
        }
    }

    /// A caller id no other test in this binary shares. The fan-out counters and
    /// the `AgentManager` LRU are both process-global.
    fn unique_id(prefix: &str) -> String {
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        format!("{prefix}-{}", SEQ.fetch_add(1, Ordering::SeqCst))
    }

    /// A **real** session row whose id is unique across this whole test binary.
    ///
    /// Two properties are needed at once, and until issue #56 only one of them
    /// was:
    ///
    /// * the row must EXIST, because the cross-session tools now resolve the
    ///   target's `privacy_tier` and refuse an id they cannot read — identically
    ///   to a private one, which is the anti-oracle rule and therefore not
    ///   negotiable. A made-up id is no longer a valid injection target;
    /// * the id must be unique in the PROCESS, because `session_events` and
    ///   `AgentManager` are keyed by session id process-wide while
    ///   `create_session` numbers ids `YYYYMMDD_N` **within one database file**
    ///   — and `client()` hands every test its own temp directory, so the first
    ///   session of every test would be `<today>_1`. That collision is exactly
    ///   why these tests reached for [`unique_id`] in the first place, and it is
    ///   a real hazard: one test's bus event would wake another's watcher.
    ///
    /// So reserve one number from a process-wide counter and burn the store's
    /// id sequence up to it. The n-th row created in a fresh store is
    /// `<today>_n`, so a distinct n per call yields a real row with an id no
    /// other test can mint. Overshooting is asserted rather than tolerated: it
    /// would silently reintroduce the collision this exists to avoid.
    async fn seeded_target(c: &WorkspaceClient, label: &str) -> String {
        // Starts above any test's own pre-created rows, so the assert below is
        // a tripwire rather than a routine failure.
        static BAND: AtomicUsize = AtomicUsize::new(16);
        let want = BAND.fetch_add(1, Ordering::SeqCst);
        let sm = c.context.session_manager.clone();
        loop {
            let id = sm
                .create_session(
                    std::env::temp_dir(),
                    format!("{label}-seed"),
                    crate::session::session_manager::SessionType::User,
                )
                .await
                .unwrap()
                .id;
            let n: usize = id
                .rsplit('_')
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or_else(|| panic!("session id numbering changed: {id}"));
            assert!(
                n <= want,
                "this test consumed its reserved band before asking for a target \
                 ({n} > {want}); raise BAND's start"
            );
            if n == want {
                return id;
            }
        }
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
            crate::agents::mcp_client::McpMeta::new(
                caller.to_string(),
                crate::privacy::CallCapability::for_test_restricted(),
            ),
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
        // A REAL row: issue #56's tier gate resolves the target's classification
        // before this handler runs, and refuses an id it cannot read.
        let target = seeded_target(&c, "target").await;

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
            let target = seeded_target(&c, "fanout-target").await;
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
        // One more real row, reused by the over-cap probe and by the retry loop
        // below. Both must reach the CAP check, which sits behind issue #56's
        // tier gate — a made-up id would be refused before it, and the "in
        // flight" assertion would then be testing the wrong refusal.
        let spare = seeded_target(&c, "fanout-spare").await;
        let over = send_prompt(
            &c,
            &caller,
            serde_json::json!({
                "session_id": spare, "text": "go", "mode": "turn"
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
                    "session_id": spare, "text": "go", "mode": "turn"
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
        let target = seeded_target(&c, "turn-target").await;

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
        // Real rows: issue #56's tier gate refuses an id it cannot resolve, so a
        // made-up target would now be refused for the wrong reason and this test
        // would pass without ever reaching the approval-mode check it is about.
        let unknown = seeded_target(&c, "no-agent-target").await;

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
        let live = seeded_target(&c, "live-agent-target").await;
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
        // The client comes FIRST now: the target has to be a real row (issue
        // #56's tier gate resolves it), and only a client owns a store to seed
        // it in. `install()` is process-global, so the order is free.
        let c = client();
        let target = seeded_target(&c, "steer-target").await;
        let services = FakeServices::with_gui(true).busy(&target).install();
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
            "the RAW text is queued: the drain loop frames it, so framing here \
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
        // Client first: the target must be a real row for issue #56's tier gate
        // to resolve. `install()` is process-global, so the order is free.
        let c = client();
        let target = seeded_target(&c, "closed-steer-target").await;
        let _services = FakeServices::with_gui(true).busy(&target).install();
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

    /// ⚠ **A conversation may not re-tool ITSELF, because that is an escalation.**
    ///
    /// Found by review, not by a test, which is why this one exists.
    ///
    /// `apply_tool_changes` adds extensions with `agent.add_extension`, stamping
    /// `ExtensionOrigin::Explicit`, and the delegation gate's condition 5
    /// (`has_non_injected_extensions`) counts Explicit entries. Once Workspace
    /// shipped as a default-on capability with its full surface, an agent could
    /// add any default-off public capability to its OWN session and thereby
    /// satisfy condition 5 on the next tool listing: delegation turned on by a
    /// grant the agent wrote for itself.
    ///
    /// Excluding `workspace` by name (issue #76) closed the door Workspace came
    /// through. It did not close the door Workspace can OPEN. This is that door.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn set_tools_refuses_to_retool_the_calling_conversation() {
        let c = client();

        // "caller" is the session id `test_meta()` presents, so this is the
        // self-targeting case exactly as an agent would issue it.
        let result = set_tools(
            &c,
            serde_json::json!({
                "session_id": "caller",
                "add_extensions": ["chatrecall"],
            }),
        )
        .await;

        let text = text_of(&result);
        assert!(
            result.is_error.unwrap_or(false),
            "self-targeting must be refused, got: {text}"
        );
        assert!(
            text.contains("ANOTHER conversation"),
            "the refusal must say why, got: {text}"
        );
        // The refusal has to be terminal. A model told only "no" retries; this
        // one is told the boundary and where the user changes tools instead.
        assert!(
            text.contains("Do not retry"),
            "the refusal must close the loop, got: {text}"
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
            crate::agents::mcp_client::McpMeta::new(
                caller.to_string(),
                crate::privacy::CallCapability::for_test_restricted(),
            ),
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
        // A REAL row, for the reason `seeded_target` documents: since issue #56
        // `workspace_close` resolves the target's tier and refuses an id it
        // cannot read — identically to a private one. A made-up id is no longer
        // a closeable target.
        let target = seeded_target(&c, "tab-target").await;

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
        // ⚠ And it PARKED for the answer. This shipped as `wait_result: false`,
        // which cannot fail any assertion about the frame — the frame is
        // identical either way — while making the sentence below unfounded.
        assert_eq!(
            services.waits(),
            vec![true],
            "close_tab must wait for the renderer, or it cannot know the tab closed"
        );

        crate::workspace_services::clear_test_override();
    }

    /// ⚠ The renderer can REFUSE a close, and it routinely does: `close_tab` on
    /// a session with no tab in this window is
    /// `refuse('session has no tab')`, and a frame arriving while no chat
    /// surface is mounted is queued rather than applied. Both used to come back
    /// as "Tab for session X closed" because nothing was waiting for an answer.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn close_tab_reports_a_refusal_instead_of_claiming_the_tab_is_gone() {
        let services = FakeServices::with_gui(true)
            .gui_answers(vec![
                serde_json::json!({ "ok": false, "detail": "session has no tab" }),
            ])
            .install();
        let c = client();
        let target = seeded_target(&c, "close-refused").await;

        let result = close(
            &c,
            "closer",
            serde_json::json!({ "session_id": target, "scope": "tab" }),
        )
        .await;

        assert_ne!(result.is_error, Some(true), "got: {}", text_of(&result));
        assert_eq!(services.waits(), vec![true]);
        let text = text_of(&result);
        assert!(
            text.contains("did NOT close the tab"),
            "a refused close must be reported as a refusal: {text}"
        );
        assert!(text.contains("session has no tab"), "got: {text}");
        assert!(
            !text.contains("closed (session survives)"),
            "the success sentence must not survive a refusal: {text}"
        );

        crate::workspace_services::clear_test_override();
    }

    /// `scope:"turn"` on a session that IS running: the token must be tripped
    /// for that session, the answer must name the turn the daemon reported, and
    /// §5 says the target's GUI is told who did it.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn close_turn_cancels_the_running_turn_and_tells_the_target() {
        let c = client();
        let target = seeded_target(&c, "turn-target").await;
        let services = FakeServices::with_gui(true).busy(&target).install();
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
        let c = client();
        let target = seeded_target(&c, "agent-target").await;
        let services = FakeServices::with_gui(true).busy(&target).install();
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
        let c = client();
        let target = seeded_target(&c, "stop-fail").await;
        let services = FakeServices::with_gui(true)
            .stop_fails("registry is wedged")
            .install();

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
        let c = client();
        let target = seeded_target(&c, "bad-scope").await;
        let services = FakeServices::with_gui(true).busy(&target).install();

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
        // A REAL row: since issue #56 `workspace_watch` resolves each watched
        // conversation's tier and refuses one it cannot read. `seeded_target`
        // gives a row whose id is also unique in the process, which is what the
        // handle registry and the event bus need.
        let child = seeded_target(&c, "child-live").await;
        let _running =
            BackgroundSubagent::register("caller", &child, "long job", CancellationToken::new());

        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
            "session_ids": [child], "timeout_s": 1
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
        let child = seeded_target(&c, "child-queued").await;
        let _queued = BackgroundSubagent::register(
            "caller",
            &child,
            "waiting on the semaphore",
            CancellationToken::new(),
        );

        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
            "session_ids": [child], "timeout_s": 1
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
        let child = seeded_target(&c, "child-done").await;
        let handle =
            BackgroundSubagent::register("caller", &child, "short job", CancellationToken::new());
        handle.complete(SubagentResult::from_error("finished"));

        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
            "session_ids": [child], "timeout_s": 30
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
        assert!(text.contains(&child));
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
        // A real row that no HANDLE in this process knows about — which is what
        // `Unknown` liveness means. (Before issue #56 this was a made-up id; the
        // tier gate now refuses one of those, and "not a background child of
        // mine" is the property this test is actually about.)
        let target_id = seeded_target(&c, "never-seen").await;

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
        // `seeded_target`, NOT a bare `create_session`. Session ids are
        // `YYYYMMDD_N` counted within one SQLite file, and every `client()` here
        // gets a fresh temp DB — so the first session of EVERY test in this
        // binary is `<today>_1`, while `session_events` is a process-global bus
        // keyed by that id. Two such tests then publish onto each other's bus:
        // this test's `TurnFinished{reason:"stop"}` was arriving inside
        // `watch_timeout_is_not_an_error_…`, turning its timeout into a
        // completion. A plain `unique_id` used to be the answer; since issue #56
        // `handle_watch` DOES consult the session manager — it resolves each
        // watched conversation's tier — so the id must now be a real row *and*
        // process-unique, which is exactly what `seeded_target` reserves.
        let target_id = seeded_target(&c, "watched").await;

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
        // A reserved, process-unique REAL row — see
        // `watch_wakes_on_a_terminal_bus_event` for why `<today>_1` is shared by
        // every test in this binary and what that cost this test specifically,
        // and why the id must nevertheless name a row the tier gate can read.
        let target_id = seeded_target(&c, "slow").await;
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
            "workspace_read_panel",
            "workspace_capture_panel",
        ] {
            assert!(
                names.iter().any(|n| n == expected),
                "the Slice-1 surface must include {expected}: {names:?}"
            );
        }
        // And every one of the six is named in the instruction block (§6).
        let info = c.get_info().unwrap();
        let instructions = info.instructions.as_deref().unwrap();
        for name in &names {
            assert!(
                instructions.contains(name.as_str()),
                "instructions omit {name}"
            );
        }
        // §6 injection budget. Raised 2500 → 2800 when the panel pair landed:
        // this text is injected on EVERY turn, so the number exists to force a
        // decision rather than to be nudged. The decision here was that an
        // agent which cannot be told it may look at the user's screen cannot
        // use the feature at all, and that both entries earn their line —
        // they were cut to two lines each first, not after.
        assert!(instructions.len() <= 2800, "injection budget (§6)");
    }

    #[tokio::test]
    async fn workspace_open_requires_exactly_one_of_session_id_or_new() {
        let c = client();
        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({})).unwrap();
        let result = c
            .call_tool(
                "workspace_open",
                Some(args),
                test_meta(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
        // The REASON, not just the flag. `CallToolResult::error` is also what
        // the `PENDING_TOOLS` stub arm returns ("not implemented until Task
        // 24"), so `is_error == Some(true)` alone cannot tell a validated
        // refusal apart from an unimplemented tool and would pass before the
        // tool existed.
        let text = result.content[0].as_text().unwrap().text.clone();
        assert!(text.contains("session_id"), "got: {text}");
        assert!(text.contains("new"), "got: {text}");

        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
            "session_id": "s-x", "new": { "working_dir": "/tmp" }
        }))
        .unwrap();
        let result = c
            .call_tool(
                "workspace_open",
                Some(args),
                test_meta(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
        let text = result.content[0].as_text().unwrap().text.clone();
        assert!(text.contains("not both"), "got: {text}");

        // `placement` is a CLOSED vocabulary, and the failure of an open one is
        // silent: anything that is not exactly "window" took the open_tab branch
        // and was forwarded verbatim as its `placement` field, so "windows" or
        // "Window" opened a tab — the one outcome the caller did not ask for,
        // reported as success. The refusal must also land BEFORE any session
        // work: nothing here exists, and a typo is not a reason to create.
        for bad in ["windows", "Window", "popup"] {
            let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
                "session_id": "s-x", "placement": bad
            }))
            .unwrap();
            let result = c
                .call_tool(
                    "workspace_open",
                    Some(args),
                    test_meta(),
                    CancellationToken::new(),
                )
                .await
                .unwrap();
            assert_eq!(
                result.is_error,
                Some(true),
                "placement {bad:?} was accepted"
            );
            let text = result.content[0].as_text().unwrap().text.clone();
            assert!(text.contains(bad), "the refusal names the value: {text}");
            assert!(text.contains("window"), "…and the vocabulary: {text}");
        }
    }

    /// **Issue #111.** `workspace_open` is not a delegation door, and the refusal
    /// that makes that true has to arrive *before* a session exists.
    ///
    /// The bug this replaces was not a missing check — it was a missing
    /// question. `workspace_open { new: { prompt } }` and `subagent` both read
    /// as "start a conversation and give it a first instruction", so a model
    /// asked to spin up three sub-agents used the first of the two it met and
    /// got three ordinary `User` rows with no parent. Nothing was wrong with
    /// each individual call; the tool simply never asked which of the two things
    /// was meant.
    ///
    /// So the assertions here are about the *shape* of the answer, not only its
    /// polarity:
    ///
    /// * nothing reached `start_session` — a refusal that has already minted a
    ///   row produces the exact outcome it exists to prevent;
    /// * the refusal names `subagent`, because a model told only "no" has no
    ///   move except to try the same call again (which is what happened in the
    ///   field with a differently-worded refusal);
    /// * it also names `kind:"user"`, so a caller that genuinely wanted a peer
    ///   conversation is not stranded by a message written for the other case.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn workspace_open_refuses_to_create_a_subagent_and_names_the_tool_that_can() {
        let recorder = FakeServices::with_gui(true).install();
        let c = client();

        let refused = open_as(
            &c,
            "caller",
            serde_json::json!({ "new": { "kind": "sub_agent", "prompt": "research Excel" } }),
        )
        .await;

        let text = text_of(&refused);
        assert_eq!(refused.is_error, Some(true), "{text}");
        assert!(
            recorder.sessions_started().is_empty(),
            "the refusal created a session anyway: {:?}",
            recorder.sessions_started()
        );
        assert!(
            text.contains("`subagent`"),
            "the refusal must name the tool that CAN delegate: {text}"
        );
        assert!(
            text.contains("kind:\"user\""),
            "…and what to pass for the other case: {text}"
        );
        assert!(
            text.contains("parent"),
            "…and why this is a data-model fact, not a naming quibble: {text}"
        );

        // ⚠ **Every test in this file that installs an override clears it, and
        // this one has to too.** `serial_test`'s `serial`/`parallel` pairing
        // stops an override from being *observed concurrently*; it does nothing
        // about one left behind. The reader is
        // `workspace_list_reports_headless_and_sessions`, whose whole subject is
        // the headless answers — so a leaked `with_gui(true)` makes it report
        // `gui_attached: true` in a run that never touched it, on whichever
        // platform happens to schedule the two in that order (this failed on
        // ubuntu while macOS and Windows both passed).
        crate::workspace_services::clear_test_override();
    }

    /// The other half of #111's closed vocabulary: absent, and every value this
    /// door does not create.
    ///
    /// `scheduled`, `hidden` and `terminal` are in the list deliberately. They
    /// are real `SessionType` variants, so a validator written as "reject
    /// sub_agent, accept the rest" would take all three — and `workspace_open`
    /// would quietly become the way to mint a hidden conversation. Refusing
    /// everything but the one kind it creates is the only shape where the
    /// vocabulary the docs state and the vocabulary the code accepts are the
    /// same two words.
    ///
    /// The empty-string and typo cases are here for the same reason `placement`
    /// has them: an open vocabulary fails silently, and "sub-agent" (a hyphen a
    /// model will absolutely produce) must not fall through to the accept arm.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn workspace_open_refuses_a_new_conversation_it_cannot_declare_the_kind_of() {
        let recorder = FakeServices::with_gui(true).install();
        let c = client();

        // No `kind` at all — the shape every pre-#111 caller sent.
        let refused = open_as(&c, "caller", serde_json::json!({ "new": {} })).await;
        let text = text_of(&refused);
        assert_eq!(refused.is_error, Some(true), "{text}");
        assert!(
            text.contains("\"user\"") && text.contains("\"sub_agent\""),
            "the refusal states the whole vocabulary: {text}"
        );
        assert!(
            text.contains("session_type"),
            "…and that it is the vocabulary workspace_list already reports: {text}"
        );

        for bad in [
            "scheduled",
            "hidden",
            "terminal",
            "sub-agent",
            "subagent",
            "peer",
            "User",
            "",
        ] {
            let refused =
                open_as(&c, "caller", serde_json::json!({ "new": { "kind": bad } })).await;
            let text = text_of(&refused);
            assert_eq!(refused.is_error, Some(true), "kind {bad:?} was accepted");
            assert!(
                text.contains("\"user\""),
                "kind {bad:?} was refused without stating the vocabulary: {text}"
            );
        }

        assert!(
            recorder.sessions_started().is_empty(),
            "one of the refused kinds created a session: {:?}",
            recorder.sessions_started()
        );

        crate::workspace_services::clear_test_override();
    }

    /// **`kind:"user"` plus a `prompt` is a legitimate call, and #111 must not
    /// have broken it.**
    ///
    /// This is the case the issue explicitly forbids inferring from: a
    /// conversation the user owns may open with a first prompt, so the prompt is
    /// not evidence of delegation. A fix that classified by prompt, or that
    /// refused `new.prompt` outright, would pass every assertion in the two
    /// tests above and fail here — which is the whole reason this test exists
    /// beside them rather than trusting the refusals to define the behaviour.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn a_user_kind_conversation_may_still_open_with_a_first_prompt() {
        let recorder = FakeServices::with_gui(true).install();
        let c = client();
        let dir = std::env::temp_dir().join("br111-peer");
        std::fs::create_dir_all(&dir).unwrap();

        let ok = open_as(
            &c,
            "caller",
            serde_json::json!({ "new": {
                "kind": "user",
                "working_dir": dir.to_str().unwrap(),
                "prompt": "research Excel automation",
            }}),
        )
        .await;

        let text = text_of(&ok);
        assert_ne!(ok.is_error, Some(true), "{text}");
        assert_eq!(
            recorder.session_dirs(),
            vec![dir],
            "the accepted call must reach start_session exactly once: {text}"
        );
        // The prompt still runs as a detached turn, provenance-stamped — the
        // pre-#111 behaviour for this kind, unchanged.
        assert_eq!(
            recorder.started.lock().unwrap().len(),
            1,
            "the first prompt no longer starts a turn"
        );

        crate::workspace_services::clear_test_override();
    }

    /// The `new.kind` vocabulary IS `SessionType`'s, not a private copy that
    /// happens to agree today.
    ///
    /// Derived from the enum on both sides rather than written out, so a rename
    /// of a persisted spelling fails here instead of leaving `workspace_open`
    /// accepting a word the store no longer uses. That is the same
    /// one-vocabulary rule the UI follows for `session_type`: a conversation's
    /// kind has one set of names in this system.
    #[test]
    fn the_new_kind_vocabulary_is_the_one_the_store_persists() {
        use crate::session::session_manager::SessionType;

        assert!(
            refuse_unless_creatable_kind(Some(&SessionType::User.to_string())).is_ok(),
            "the accepted kind must be spelled exactly as the store persists it"
        );
        let refusal = refuse_unless_creatable_kind(Some(&SessionType::SubAgent.to_string()))
            .expect_err("sub_agent must be refused here");
        assert_eq!(refusal, DELEGATION_IS_NOT_THIS_TOOL);
    }

    /// The schema half of #111: `new.kind` is advertised as **required**, so a
    /// provider that validates tool arguments refuses the malformed call before
    /// it costs a round trip.
    ///
    /// Asserted against the generated schema rather than the struct, because the
    /// two can disagree: the field is `Option<String>` in Rust (the handler owns
    /// the "you left it out" message) and required only by way of a
    /// `#[schemars(extend(...))]` attribute. Nothing else in the tree would
    /// notice if that attribute were dropped, or if a future schemars changed
    /// how `extend` merges with the derive's own `required` array.
    #[test]
    fn new_declares_its_kind_in_the_schema() {
        let schema = serde_json::to_value(schema_for!(WorkspaceOpenParams)).unwrap();
        // schemars renders an `Option<WorkspaceOpenNew>` as
        // `anyOf: [{$ref}, {type: null}]`, so the field's own subschema carries
        // nothing but the description — the `required` array lives on the
        // definition the `$ref` points at. A test that read
        // `/properties/new/required` would find no array at all and pass
        // vacuously if the assertion were the other polarity, which is why the
        // resolution below is written out rather than pointer-guessed.
        let definition = schema
            .pointer("/$defs/WorkspaceOpenNew")
            .unwrap_or_else(|| panic!("no WorkspaceOpenNew definition: {schema}"));
        let required: Vec<&str> = definition
            .get("required")
            .and_then(|r| r.as_array())
            .map(|r| r.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        assert!(
            required.contains(&"kind"),
            "new.kind is not required in the advertised schema: {definition}"
        );
        // The other fields stay optional — a `required` array that swept them in
        // would make every pre-existing caller invalid, which is a different
        // (and much larger) change than the one #111 asks for.
        assert_eq!(
            required,
            vec!["kind"],
            "only `kind` is required on `new`: {definition}"
        );
    }

    /// Call `workspace_open` as `caller`. Mirrors [`send_prompt`].
    ///
    /// The §8.1 focus-etiquette preference is pinned OFF here rather than
    /// inherited from the machine. `place_in_gui` resolves it through
    /// `Config::global().get_param`, which reads `WORKSPACE_ANNOUNCE_ONLY`
    /// from the environment and then from `~/.config/biorouter/config.yaml` —
    /// the very file the Settings toggle this feature ships writes. Without
    /// the pin, a maintainer who turns the setting on in the app gets a red
    /// suite (`open_new_inherits_…` panics with "an open_tab frame was sent")
    /// with no visible connection to what they changed, and CLAUDE.md's bare
    /// `cargo test` reads that real config file.
    async fn open_as(c: &WorkspaceClient, caller: &str, args: serde_json::Value) -> CallToolResult {
        open_as_with_announce_only(c, caller, args, false).await
    }

    /// [`open_as`] with the §8.1 preference forced to `announce_only`.
    ///
    /// The override is task-local ([`crate::config::with_config_overrides`]):
    /// it wins over both the environment and the config file, and — unlike
    /// `std::env::set_var`, which is unsound in a multi-threaded program —
    /// never mutates the process environment, so the parallel test threads
    /// outside this task cannot observe it.
    async fn open_as_with_announce_only(
        c: &WorkspaceClient,
        caller: &str,
        args: serde_json::Value,
        announce_only: bool,
    ) -> CallToolResult {
        let args: rmcp::model::JsonObject = serde_json::from_value(args).unwrap();
        let overrides = std::collections::HashMap::from([(
            ANNOUNCE_ONLY_KEY.to_string(),
            announce_only.to_string(),
        )]);
        crate::config::with_config_overrides(
            overrides,
            c.call_tool(
                "workspace_open",
                Some(args),
                crate::agents::mcp_client::McpMeta::new(
                    caller.to_string(),
                    crate::privacy::CallCapability::for_test_restricted(),
                ),
                CancellationToken::new(),
            ),
        )
        .await
        .unwrap()
    }

    /// Decision 5: a new conversation inherits the CALLER's directory. Two
    /// callers with different directories, because one caller cannot tell
    /// "inherits from the caller" apart from "hardcodes this particular path".
    ///
    /// The second half pins the GUI frame VOCABULARY, which nothing else on
    /// either side of the wire looks at: `open_tab` vs `open_window`,
    /// `placement`, `focus`. Task 26's planner consumes exactly these.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn open_new_inherits_the_callers_working_dir_and_emits_the_gui_frame() {
        let recorder = FakeServices::with_gui(true).install();

        let c = client();
        let sm = c.context.session_manager.clone();
        let dir_a = std::env::temp_dir().join("br71-caller-a");
        let dir_b = std::env::temp_dir().join("br71-caller-b");
        std::fs::create_dir_all(&dir_a).unwrap();
        std::fs::create_dir_all(&dir_b).unwrap();
        let caller_a = sm
            .create_session(
                dir_a.clone(),
                "caller-a".into(),
                crate::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();
        let caller_b = sm
            .create_session(
                dir_b.clone(),
                "caller-b".into(),
                crate::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();

        open_as(
            &c,
            &caller_a.id,
            serde_json::json!({ "new": { "kind": "user" } }),
        )
        .await;
        open_as(
            &c,
            &caller_b.id,
            serde_json::json!({ "new": { "kind": "user" } }),
        )
        .await;

        assert_eq!(
            recorder.session_dirs(),
            vec![dir_a.clone(), dir_b.clone()],
            "each new session takes ITS caller's directory; a hardcoded default, the \
             process cwd, or temp_dir() all produce two identical entries here"
        );

        // …and the directory is not the only thing the daemon half decides.
        let open = recorder
            .frame_with_cmd("open_tab")
            .expect("an open_tab frame was sent");
        assert_eq!(open["type"], "workspace");
        assert_eq!(open["session_id"], "s-new");
        assert_eq!(open["placement"], "tab", "the default placement");
        assert_eq!(
            open["focus"], false,
            "§4.1: background open, never steal the composer"
        );

        // `placement: "window"` is a DIFFERENT command, not a field on open_tab —
        // the renderer routes it to createChatWindow, and a planner that saw
        // `open_tab {placement: "window"}` would silently open a tab instead.
        recorder.clear_frames();
        open_as(
            &c,
            &caller_a.id,
            serde_json::json!({ "new": { "kind": "user" }, "placement": "window" }),
        )
        .await;
        assert!(
            recorder.frame_with_cmd("open_window").is_some(),
            "placement:\"window\" must emit open_window: {:?}",
            recorder.all_frames()
        );
        assert!(recorder.frame_with_cmd("open_tab").is_none());

        // An explicit, DIFFERENT working_dir is allowed but never silent
        // (decision 5) — the notify frame is how the user finds out.
        recorder.clear_frames();
        let elsewhere = std::env::temp_dir().join("br71-elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        let r = open_as(
            &c,
            &caller_a.id,
            serde_json::json!({ "new": { "kind": "user", "working_dir": elsewhere.to_str().unwrap() } }),
        )
        .await;
        assert_eq!(recorder.session_dirs().last().unwrap(), &elsewhere);
        // The toast is for the user; the RESULT is for the model, and a model
        // that is never told where it put the session cannot report it.
        assert!(
            text_of(&r).contains(&elsewhere.display().to_string()),
            "the result names the new conversation's directory: {}",
            text_of(&r)
        );
        let notify = recorder
            .frame_with_cmd("notify")
            .expect("a caller-dir mismatch must be surfaced, not swallowed");
        assert!(
            notify["message"]
                .as_str()
                .unwrap()
                .contains(&elsewhere.display().to_string()),
            "the notice names the directory: {notify}"
        );
        // …and it arrives AFTER the tab. The toast is addressed to
        // `session_id: s-new`, so a renderer that routes a session's toasts to
        // that session's tab drops one sent before the tab is created. The
        // ordering is the daemon's to get right; the renderer half (Task 26) has
        // no way to recover a frame it dropped.
        let frames = recorder.all_frames();
        let cmds: Vec<&str> = frames.iter().filter_map(|f| f["cmd"].as_str()).collect();
        assert_eq!(
            cmds,
            ["open_tab", "notify"],
            "the tab must exist before the toast addressed to it"
        );

        crate::workspace_services::clear_test_override();
    }

    /// §8.1 / decision 7, through the REAL emitter.
    ///
    /// The setting's other two tests are pure — they hand
    /// `apply_focus_etiquette` and `open_result_text` their arguments directly
    /// — so deleting the two lines in `place_in_gui` that wire them in leaves
    /// both green and ships a preference the daemon silently ignores, with the
    /// first symptom a user whose Settings toggle does nothing. This is the
    /// test that fails when the transform is not actually applied, and it pins
    /// the two halves TOGETHER: a frame that was downgraded and a result text
    /// that still says "opened" is the specific disagreement the whole feature
    /// exists to prevent, and neither pure test can see it.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn announce_only_downgrades_the_real_open_to_a_notification() {
        let recorder = FakeServices::with_gui(true).install();

        let c = client();
        let dir = std::env::temp_dir().join("br71-announce-only");
        std::fs::create_dir_all(&dir).unwrap();
        let caller = c
            .context
            .session_manager
            .create_session(
                dir.clone(),
                "caller".into(),
                crate::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();

        let r = open_as_with_announce_only(
            &c,
            &caller.id,
            serde_json::json!({ "new": { "kind": "user" } }),
            true,
        )
        .await;

        // The GUI never got a focus-stealing frame…
        assert!(
            recorder.frame_with_cmd("open_tab").is_none(),
            "announce-only must not open a tab: {:?}",
            recorder.all_frames()
        );
        assert!(recorder.frame_with_cmd("open_window").is_none());
        // …it got the notification instead, naming the conversation so the user
        // can find it in History.
        let notify = recorder
            .frame_with_cmd("notify")
            .expect("the downgraded frame must still reach the GUI");
        assert_eq!(notify["session_id"], "s-new");
        assert!(
            notify["message"].as_str().unwrap().contains("s-new"),
            "the announcement names the conversation: {notify}"
        );

        // …and in the SAME call the model was told the truth, not "opened".
        let text = text_of(&r);
        assert!(
            text.contains("no tab was opened") && text.contains("Do not tell the user"),
            "{text}"
        );
        assert!(
            !text.contains("opened in the GUI"),
            "the model must not be handed the phrase it will repeat: {text}"
        );
        // Decision 5's directory note survives the announce-only arm — a
        // session the model cannot see is exactly the one it most needs the
        // working directory for.
        assert!(text.contains(&dir.display().to_string()), "{text}");

        // The setting suppresses the TAB, never the work: the session was still
        // created, in the caller's directory.
        assert_eq!(recorder.session_dirs(), vec![dir]);

        // And with the setting off, the same call opens a tab — so the
        // assertions above are pinned to the preference, not to some unrelated
        // property of this fixture.
        recorder.clear_frames();
        let r = open_as(
            &c,
            &caller.id,
            serde_json::json!({ "new": { "kind": "user" } }),
        )
        .await;
        assert!(recorder.frame_with_cmd("open_tab").is_some());
        assert!(text_of(&r).contains("opened in the GUI"));

        crate::workspace_services::clear_test_override();
    }

    /// The rest of what `new:` promises, none of which the returned id or the
    /// GUI frame can show — and all of which the test above leaves green when
    /// deleted.
    ///
    /// Two failures in particular are silent in production. Sending
    /// `KbPrimaryChoice::Auto` where the caller named a target retargets every
    /// KB-less write in the new conversation to the wrong base (post-#45 the
    /// primary is a validated pointer, not a derived convenience). And dropping
    /// `prompt` — or running it unstamped — falsifies the instruction block's
    /// promise that "injections are permanently labeled as coming from you".
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn open_new_forwards_the_grants_and_runs_the_seeded_first_turn() {
        let recorder = FakeServices::with_gui(true).install();

        let c = client();
        let dir = std::env::temp_dir().join("br71-grants");
        std::fs::create_dir_all(&dir).unwrap();
        let caller = c
            .context
            .session_manager
            .create_session(
                dir.clone(),
                "planner".into(),
                crate::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();

        // ⚠ **A name no config on any machine carries**, and NOT `developer`.
        // Since issue #56 this path runs Gate F1 and #42's operator pin over the
        // requested set, and both read the machine's real `config.yaml`: on a
        // developer box that has `developer: enabled: false` — the common case,
        // because the GUI writes that — a `["developer"]` fixture would refuse
        // here and this test would fail for a reason that has nothing to do with
        // what it asserts. An unregistered name resolves Public (R11(ii)) and is
        // not persisted, so it passes the gate on every machine, and
        // `start_session` is faked here so nothing tries to load it.
        let r = open_as(
            &c,
            &caller.id,
            serde_json::json!({ "new": {
                "kind": "user",
                "extensions": ["br71-fixture-extension"],
                "knowledge_bases": ["kb-a", "kb-b"],
                "primary_knowledge_base": "kb-b",
                "prompt": "start on the migration",
            }}),
        )
        .await;
        assert_ne!(r.is_error, Some(true), "got: {}", text_of(&r));

        let started = recorder.sessions_started();
        assert_eq!(started.len(), 1, "one session; got {started:?}");
        assert_eq!(
            started[0].extensions,
            Some(vec!["br71-fixture-extension".to_string()]),
            "the granted extension set reaches the daemon"
        );
        assert_eq!(
            started[0].knowledge_bases,
            vec!["kb-a".to_string(), "kb-b".to_string()]
        );
        assert_eq!(
            started[0].primary,
            KbPrimaryChoice::Set("kb-b".into()),
            "an explicit write target must arrive as Set: Auto would pin kb-a and \
             every KB-less write in the new conversation would land in the wrong base"
        );

        // The seeded turn RAN, on the new session, carrying the prompt …
        let turns = recorder.started.lock().unwrap().clone();
        assert_eq!(
            turns.len(),
            1,
            "the prompt starts exactly one detached turn"
        );
        let (session_id, _turn_id, message) = &turns[0];
        assert_eq!(session_id, "s-new");
        let body: String = message
            .content
            .iter()
            .filter_map(|c| c.as_text())
            .collect::<Vec<_>>()
            .join("");
        assert!(body.contains("start on the migration"), "got: {body}");
        // … and it is LABELLED with the caller (§6).
        let p = message
            .metadata
            .provenance
            .as_ref()
            .expect("the seeded turn is provenance-stamped");
        assert_eq!(
            p.kind,
            crate::conversation::message::ProvenanceKind::AgentInjection
        );
        assert_eq!(p.from_session_id.as_deref(), Some(caller.id.as_str()));
        assert_eq!(p.from_session_name.as_deref(), Some(caller.name.as_str()));

        // No prompt and no named target: no turn at all, and `Auto` — which on a
        // FRESH session resolves to "pin the first id" (Task 9), the reason
        // `workspace_open` never leaves a KB-carrying session pointer-less.
        recorder.started.lock().unwrap().clear();
        recorder.sessions_started.lock().unwrap().clear();
        open_as(
            &c,
            &caller.id,
            serde_json::json!({ "new": { "kind": "user", "knowledge_bases": ["kb-a"] } }),
        )
        .await;
        assert_eq!(
            recorder.sessions_started()[0].primary,
            KbPrimaryChoice::Auto
        );
        assert!(
            recorder.started.lock().unwrap().is_empty(),
            "no prompt means no turn"
        );

        crate::workspace_services::clear_test_override();
    }

    /// A GUI round-trip that fails AFTER the session exists must not throw the
    /// id away.
    ///
    /// `gui_command(frame, true)` parks for the renderer's `workspace_result`
    /// with a 10 s timeout, and a wedged or merely slow renderer used to turn the
    /// whole call into `Error: …` — with the session, its extension set, its
    /// knowledge grants and its seeded first turn all already committed. The
    /// model is then holding an orphan it cannot name, and near-certainly makes a
    /// second one. Note the tell: fully headless behaved BETTER than a slow GUI.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn a_failed_gui_round_trip_still_reports_the_new_session_id() {
        let recorder = FakeServices::with_gui(true)
            .gui_fails("no workspace_result within 10s")
            .install();

        let c = client();
        let dir = std::env::temp_dir().join("br71-wedged-renderer");
        std::fs::create_dir_all(&dir).unwrap();
        let r = open_as(
            &c,
            "caller",
            serde_json::json!({ "new": { "kind": "user", "working_dir": dir.to_str().unwrap() } }),
        )
        .await;
        assert_eq!(recorder.session_dirs(), vec![dir.clone()], "it WAS created");
        assert_ne!(
            r.is_error,
            Some(true),
            "a created session must not be lost to a GUI failure: {}",
            text_of(&r)
        );
        let text = text_of(&r);
        assert!(text.contains("s-new"), "the id survives: {text}");
        assert!(
            text.contains("no workspace_result within 10s"),
            "…and the GUI failure is still reported, not hidden: {text}"
        );
        assert!(
            text.contains(&dir.display().to_string()),
            "…as is the directory: {text}"
        );

        // An EXISTING session the GUI will not place is a plain failure: nothing
        // was created, so there is nothing to orphan and nothing to soften.
        let existing = c
            .context
            .session_manager
            .create_session(
                dir.clone(),
                "existing".into(),
                crate::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();
        let r = open_as(
            &c,
            "caller",
            serde_json::json!({ "session_id": existing.id }),
        )
        .await;
        assert_eq!(r.is_error, Some(true), "got: {}", text_of(&r));

        crate::workspace_services::clear_test_override();
    }

    /// §2.1's headless requirement: with no GUI the tool still does the
    /// session-level half of its job and SAYS that it did only that — it does
    /// not fail, and it does not claim a tab it never opened. The existence
    /// check on `session_id` is the other half: the GUI must never be handed a
    /// frame naming a session that is not there.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn open_degrades_headlessly_and_refuses_a_session_that_does_not_exist() {
        crate::workspace_services::set_for_tests(Some(std::sync::Arc::new(
            crate::workspace_services::NullServices,
        )));

        let c = client();
        let existing = c
            .context
            .session_manager
            .create_session(
                std::env::temp_dir(),
                "existing".into(),
                crate::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();

        let r = open_as(
            &c,
            "caller",
            serde_json::json!({ "session_id": existing.id }),
        )
        .await;
        assert_ne!(r.is_error, Some(true), "got: {}", text_of(&r));
        let text = text_of(&r);
        assert!(text.contains(&existing.id), "got: {text}");
        assert!(text.contains("gui_attached: false"), "got: {text}");

        // An id that names no row is still refused — the GUI is never handed a
        // frame for a session that is not there.
        //
        // ⚠ **What it is refused WITH changed with issue #56**, and the change is
        // the point rather than a casualty. `open_as` carries a PUBLIC
        // capability, and §7's anti-oracle rule says a public caller must not be
        // able to tell "that conversation is private" from "there is no such
        // conversation" — so the sentence it gets is the one that says both. A
        // caller entitled to the difference is still told it, which is the arm
        // below; `the_refusal_cannot_tell_a_private_conversation_from_one_that_
        // does_not_exist` is what pins the equality itself.
        let r = open_as(&c, "caller", serde_json::json!({ "session_id": "s-nope" })).await;
        assert_eq!(r.is_error, Some(true));
        assert!(
            text_of(&r).contains(&crate::privacy::refusal::workspace_out_of_reach()),
            "got: {}",
            text_of(&r)
        );
        let r = call_as(
            &c,
            "workspace_open",
            serde_json::json!({ "session_id": "s-nope" }),
            private_caller(),
        )
        .await;
        assert_eq!(r.is_error, Some(true));
        assert!(
            text_of(&r).contains("no such session"),
            "the existence check is gone, not merely shadowed: {}",
            text_of(&r)
        );

        // A new session is still created headlessly.
        let dir = std::env::temp_dir().join("br71-headless-new");
        std::fs::create_dir_all(&dir).unwrap();
        let r = open_as(
            &c,
            "caller",
            serde_json::json!({ "new": { "kind": "user", "working_dir": dir.to_str().unwrap() } }),
        )
        .await;
        assert_ne!(r.is_error, Some(true), "got: {}", text_of(&r));
        assert!(text_of(&r).contains("headlessly"), "got: {}", text_of(&r));
        // Decision 5's disclosure has TWO channels and this is the only one left
        // here: `NullServices::gui_command` returns `Err`, which the notify path
        // swallows, so without the directory in the result text a model-chosen
        // divergent directory is disclosed nowhere at all — and `working_dir` is
        // unvalidated, model-chosen, and immediately worked in.
        assert!(
            text_of(&r).contains(&dir.display().to_string()),
            "the result names the new conversation's directory: {}",
            text_of(&r)
        );

        // …but "inherit the caller's directory" cannot be guessed when the
        // caller is unreadable, and the tool says exactly that instead of
        // inventing a directory.
        let r = open_as(
            &c,
            "caller",
            serde_json::json!({ "new": { "kind": "user" } }),
        )
        .await;
        assert_eq!(r.is_error, Some(true));
        assert!(
            text_of(&r).contains("pass working_dir explicitly"),
            "got: {}",
            text_of(&r)
        );

        crate::workspace_services::clear_test_override();
    }

    /// ⚠ **The pure-text tests above cannot see this.** They prove
    /// `open_result_text` is honest *given* an outcome; the defect was that
    /// `place_in_gui` never computed one — it sent `open_tab` unconditionally
    /// and passed the renderer's `ok:true` straight through, so a re-open of a
    /// conversation that already had a tab was reported as an opening. This
    /// walks the real dispatch path and asserts the FRAME, which is the only
    /// evidence that the daemon changed its mind about what to send.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn reopening_a_conversation_that_already_has_a_tab_activates_it() {
        let c = client();
        let target = seeded_target(&c, "already-tabbed").await;
        let services = FakeServices::with_gui(true).with_tab_for(&target).install();

        let r = open_as(
            &c,
            "caller",
            serde_json::json!({ "session_id": target, "focus": true }),
        )
        .await;
        assert_ne!(r.is_error, Some(true), "got: {}", text_of(&r));

        let frames = services.all_frames();
        assert_eq!(frames.len(), 1, "expected one frame, got: {frames:?}");
        assert_eq!(
            frames[0]["cmd"], "activate_tab",
            "a conversation that already has a tab is FOCUSED, not opened: {frames:?}"
        );
        assert_eq!(frames[0]["session_id"], target);
        assert_eq!(
            services.waits(),
            vec![true],
            "the answer is the only evidence the view moved, so the tool must park for it"
        );

        let text = text_of(&r);
        assert!(text.contains("brought to the front"), "got: {text}");
        assert!(
            text.contains("no new tab was opened"),
            "the model must be told, in words, that nothing was opened: {text}"
        );
        assert!(!text.contains(&format!("{target} opened")), "got: {text}");

        crate::workspace_services::clear_test_override();
    }

    /// The `focus:false` half. Here the frame stays `open_tab` — it is a dedupe
    /// no-op that also repairs a stale echo — and ONLY the sentence changes.
    /// Asserting the frame as well is what stops a future "simplification" from
    /// deciding the no-op can be skipped: skipping it is what makes a stale echo
    /// unrecoverable.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn a_background_reopen_of_an_existing_tab_says_nothing_was_opened() {
        let c = client();
        let target = seeded_target(&c, "already-tabbed-bg").await;
        let services = FakeServices::with_gui(true).with_tab_for(&target).install();

        let r = open_as(&c, "caller", serde_json::json!({ "session_id": target })).await;
        assert_ne!(r.is_error, Some(true), "got: {}", text_of(&r));

        let frames = services.all_frames();
        assert_eq!(frames.len(), 1, "got: {frames:?}");
        assert_eq!(frames[0]["cmd"], "open_tab", "got: {frames:?}");

        let text = text_of(&r);
        assert!(text.contains("ALREADY open"), "got: {text}");
        assert!(
            text.contains("no new tab was opened") && text.contains("nothing moved"),
            "got: {text}"
        );

        // …but `placement:"split"` on a tab that already exists is NOT a no-op:
        // the reducer MOVES it into a new pane. "nothing moved" would be false,
        // so a split keeps the opening vocabulary — the layout really changed.
        services.clear_frames();
        let r = open_as(
            &c,
            "caller",
            serde_json::json!({ "session_id": target, "placement": "split" }),
        )
        .await;
        let frames = services.all_frames();
        assert_eq!(frames[0]["cmd"], "open_tab", "got: {frames:?}");
        assert_eq!(frames[0]["placement"], "split", "got: {frames:?}");
        let text = text_of(&r);
        assert!(
            !text.contains("nothing moved") && !text.contains("ALREADY open"),
            "a split rearranges the layout even for a tab that exists: {text}"
        );

        crate::workspace_services::clear_test_override();
    }

    /// The echo is debounced and merged across windows, so it can name a tab the
    /// user closed a moment ago. The renderer says so (`ok:false, "session has
    /// no tab"`), and the tool must recover by opening — otherwise this fix
    /// turns a `workspace_open` that used to work into a refusal.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn a_stale_layout_echo_falls_back_to_opening_the_tab_it_could_not_focus() {
        let c = client();
        let target = seeded_target(&c, "stale-echo").await;
        let services = FakeServices::with_gui(true)
            .with_tab_for(&target)
            .gui_answers(vec![
                serde_json::json!({ "ok": false, "detail": "session has no tab" }),
                serde_json::json!({ "ok": true, "detail": "opened" }),
            ])
            .install();

        let r = open_as(
            &c,
            "caller",
            serde_json::json!({ "session_id": target, "focus": true }),
        )
        .await;
        assert_ne!(r.is_error, Some(true), "got: {}", text_of(&r));

        let cmds: Vec<String> = services
            .all_frames()
            .iter()
            .map(|f| f["cmd"].as_str().unwrap_or_default().to_string())
            .collect();
        assert_eq!(
            cmds,
            vec!["activate_tab", "open_tab"],
            "the refusal must be repaired by the create frame"
        );

        let text = text_of(&r);
        assert!(
            text.contains("opened in the GUI"),
            "the fallback's answer is what gets reported: {text}"
        );
        assert!(
            !text.contains("brought to the front"),
            "the focus that was refused must not be claimed: {text}"
        );

        crate::workspace_services::clear_test_override();
    }

    /// §8.1 through the whole path, not just the transform. `activate_tab` now
    /// has an emitter, so "is it in `FOCUS_STEALING_CMDS`?" is no longer the
    /// question — "does the emitter route around the transform?" is. It must
    /// also NOT fall back to `open_tab`: a refused notification is not a stale
    /// echo, and re-sending the create frame would defeat the setting outright.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn announce_only_downgrades_a_focus_of_an_existing_tab() {
        let c = client();
        let target = seeded_target(&c, "announce-existing").await;
        let services = FakeServices::with_gui(true)
            .with_tab_for(&target)
            .gui_answers(vec![
                serde_json::json!({ "ok": false, "detail": "renderer error" }),
            ])
            .install();

        let r = open_as_with_announce_only(
            &c,
            "caller",
            serde_json::json!({ "session_id": target, "focus": true }),
            true,
        )
        .await;
        assert_ne!(r.is_error, Some(true), "got: {}", text_of(&r));

        let frames = services.all_frames();
        assert_eq!(
            frames.len(),
            1,
            "a refused notification must not be retried as an open: {frames:?}"
        );
        assert_eq!(frames[0]["cmd"], "notify", "got: {frames:?}");

        let text = text_of(&r);
        assert!(text.contains("NOT brought to the front"), "got: {text}");
        assert!(
            !text.contains("they were notified"),
            "the GUI refused the notification, so no handoff may be claimed: {text}"
        );

        crate::workspace_services::clear_test_override();
    }

    /// The complete workspace surface. This is the ONE exact-set assertion in
    /// the plan: Tasks 12-18 each added a tool and each re-ran the extension's
    /// tests, so an exact assertion in any of them would have been a
    /// fail-again-every-task gate. `get_tools()` stops growing here.
    #[tokio::test]
    async fn workspace_open_is_advertised_and_completes_the_surface() {
        let c = client();
        let tools = c
            .list_tools(None, CancellationToken::new())
            .await
            .unwrap()
            .tools;
        let mut names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
        names.sort();
        assert_eq!(
            names,
            vec![
                // The merged spawn tool keeps its bare name (decision 22).
                "subagent".to_string(),
                "workspace_capture_panel".to_string(),
                "workspace_close".to_string(),
                "workspace_list".to_string(),
                "workspace_open".to_string(),
                "workspace_read_conversation".to_string(),
                "workspace_read_panel".to_string(),
                "workspace_send_prompt".to_string(),
                "workspace_set_tools".to_string(),
                "workspace_watch".to_string(),
            ],
            "the complete workspace surface"
        );

        let info = c.get_info().unwrap();
        let instructions = info.instructions.as_deref().unwrap();
        // Every registered tool is documented …
        for name in &names {
            assert!(
                instructions.contains(name.as_str()),
                "instructions omit {name}"
            );
        }
        // … and nothing is documented that is not registered. This is the
        // direction nothing tested before: the block is written once for a whole
        // phase, so only a check here can prove it never names a tool the model
        // cannot call.
        for line in instructions.lines() {
            let Some(rest) = line.trim().strip_prefix("- ") else {
                continue;
            };
            let Some((tool, _)) = rest.split_once(':') else {
                continue;
            };
            let tool = tool.trim();
            assert!(
                names.iter().any(|n| n == tool),
                "instructions name `{tool}`, which get_tools() does not register"
            );
        }
        // §6 injection budget. Raised 2500 → 2800 when the panel pair landed:
        // this text is injected on EVERY turn, so the number exists to force a
        // decision rather than to be nudged. The decision here was that an
        // agent which cannot be told it may look at the user's screen cannot
        // use the feature at all, and that both entries earn their line —
        // they were cut to two lines each first, not after.
        assert!(instructions.len() <= 2800, "injection budget (§6)");
    }

    #[test]
    fn announce_only_defaults_off_and_maps_open_tab_to_notify() {
        // Default: unset config → tabs open (today's behaviour).
        assert!(!announce_only_enabled_for(None));
        assert!(!announce_only_enabled_for(Some(false)));
        assert!(announce_only_enabled_for(Some(true)));

        // The frame transformation is the whole of the feature (§8.1).
        let open = json!({
            "type": "workspace", "cmd": "open_tab",
            "session_id": "s-child", "placement": "tab", "focus": false
        });
        let announced = apply_focus_etiquette(open.clone(), false);
        assert_eq!(announced["cmd"], "open_tab");

        let announced = apply_focus_etiquette(open, true);
        assert_eq!(announced["cmd"], "notify");
        assert_eq!(announced["session_id"], "s-child");
        let message = announced["message"].as_str().unwrap();
        assert!(message.contains("s-child"));
        assert!(message.to_lowercase().contains("open"));

        // A window request degrades the same way — it is the loudest of all.
        let window = json!({ "type": "workspace", "cmd": "open_window", "session_id": "s-w" });
        assert_eq!(apply_focus_etiquette(window, true)["cmd"], "notify");

        // The frame is only half the feature. The OTHER half is what the model
        // is told — see `open_result_text` below.

        // …and so does activate_tab. It does not OPEN anything, but it is the
        // frame that yanks the user's view to a different conversation, which is
        // the same intrusion the setting exists to prevent. This was forward
        // protection when it landed; `placement_frame` now sends the frame for a
        // `workspace_open` on a conversation that already has a tab, and
        // `announce_only_downgrades_a_focus_of_an_existing_tab` walks that whole
        // path — an assertion on the transform alone cannot see an emitter that
        // reads the setting and then routes around the transform.
        let activate = json!({ "type": "workspace", "cmd": "activate_tab", "session_id": "s-a" });
        assert_eq!(apply_focus_etiquette(activate, true)["cmd"], "notify");

        // Everything else is untouched: annotate/close/notify are not focus
        // events and must still reach the GUI — a child that runs without a tab
        // still gets its badge the moment the user opens it from History.
        for cmd in ["annotate_tab", "close_tab", "notify"] {
            let frame = json!({ "type": "workspace", "cmd": cmd, "session_id": "s" });
            assert_eq!(apply_focus_etiquette(frame, true)["cmd"], cmd);
        }
    }

    /// ⚠ The transform is the visible half; THIS is the half that decides what
    /// the model believes. A model told "opened" when nothing opened will answer
    /// the user from a false premise ("I've put it in a tab for you"), and no
    /// frame assertion above can catch that — `apply_focus_etiquette` is correct
    /// in both worlds. Before this test the whole truthful-result arm shipped
    /// untested, and its only net was a human reading the agent's reply during
    /// Task 31's live pass.
    #[test]
    fn the_result_text_never_claims_a_tab_that_was_not_opened() {
        let ok = json!({ "ok": true, "detail": "opened" });

        let announced = open_result_text("s-child", "tab", false, true, TabOutcome::Opened, &ok);
        assert!(announced.contains("s-child"));
        // The plan wrote this as `!announced.contains("opened")` alongside the
        // `contains("no tab was opened")` assertion below — which no string can
        // satisfy, since the required phrase contains the forbidden substring.
        // The INTENT ("must not use the word the model will repeat") is the
        // affirmative claim, so the negative is anchored to the exact phrasing
        // the truthful arm would fall through to. This is strictly the stronger
        // reading: a copy of the normal text still fails here, and so does any
        // other sentence that asserts a tab appeared.
        assert!(
            !announced.contains("Session s-child opened"),
            "announce-only must never claim the session opened: {announced}"
        );
        assert!(
            !announced.contains("opened in the GUI"),
            "announce-only must not reuse the phrase the model will repeat: {announced}"
        );
        assert!(
            announced.contains("no tab was opened") && announced.contains("Do not tell the user"),
            "the model must be told, in words, not to claim a tab: {announced}"
        );
        assert!(
            announced.contains("they were notified"),
            "the GUI confirmed the announcement, so the handoff is real: {announced}"
        );

        // The announcement is itself a round trip and it can come back refused.
        // "They were notified" when the GUI said otherwise is the same falsehood
        // one noun removed: the model reports a handoff to a user who saw
        // nothing, and then stops mentioning the session at all.
        let unheard = json!({ "ok": false, "detail": "renderer error: socket closed" });
        let silent = open_result_text("s-child", "tab", false, true, TabOutcome::Opened, &unheard);
        assert!(silent.contains("no tab was opened"), "{silent}");
        assert!(
            !silent.contains("they were notified"),
            "an unconfirmed announcement must not be reported as delivered: {silent}"
        );
        assert!(
            silent.contains("renderer error: socket closed"),
            "the GUI's reason survives here too: {silent}"
        );

        // `placement: "window"` degrades the same way, and must say so in the
        // CALLER'S vocabulary. Answering a window request with "no tab was
        // opened" is literally true and reads as a denial about tabs only — it
        // leaves the model free to conclude a window opened instead, which is
        // the exact false premise this function exists to prevent.
        let windowed = open_result_text("s-w", "window", false, true, TabOutcome::Opened, &ok);
        assert!(
            windowed.contains("no window was opened"),
            "a window request is denied in its own words: {windowed}"
        );
        assert!(
            !windowed.contains("opened in the GUI") && !windowed.contains("Session s-w opened"),
            "{windowed}"
        );

        let normal = open_result_text("s-child", "tab", false, false, TabOutcome::Opened, &ok);
        assert!(normal.contains("opened") && !normal.contains("NOT opened"));
        assert!(
            normal.contains("background"),
            "focus:false is background: {normal}"
        );
        assert!(normal.contains("tab"));

        let focused = open_result_text("s-child", "split", true, false, TabOutcome::Opened, &ok);
        assert!(focused.contains("focused") && focused.contains("split"));

        // A GUI that refused the command is reported as a refusal, not as
        // success — the round trip returns `ok:false`, and the previous inline
        // code path had no test that this branch was ever reachable.
        let refused = json!({ "ok": false, "detail": "no room for another split" });
        let text = open_result_text(
            "s-child",
            "split",
            false,
            false,
            TabOutcome::Opened,
            &refused,
        );
        assert!(text.contains("NOT opened"), "{text}");
        assert!(
            text.contains("no room for another split"),
            "the GUI's reason survives: {text}"
        );

        // A GUI answer with no `detail` must not leave a dangling separator:
        // `place_in_gui` appends decision 5's directory note to this string, so
        // a trailing space put a double space in the middle of the sentence the
        // model reads back to the user.
        let bare = open_result_text(
            "s-child",
            "tab",
            false,
            false,
            TabOutcome::Opened,
            &json!({ "ok": true }),
        );
        assert!(bare.ends_with("(tab, background)."), "{bare}");
        assert!(
            !format!("{bare} Working directory: /p.").contains("  "),
            "the appended directory note must not double the space: {bare:?}"
        );
    }

    /// ⚠ The renderer answers `ok:true, detail:"opened"` for an `open_tab` on a
    /// conversation that ALREADY has a tab — `openTab` dedupes by session id, so
    /// nothing is allocated and, with `focus:false`, nothing moves either. The
    /// old five-argument text could not express that and reported every re-open
    /// as an opening. These are the two sentences that could not be written
    /// before, and each is asserted to deny the opening **in words**, because
    /// the model paraphrases this string to the user.
    #[test]
    fn a_conversation_that_already_had_a_tab_is_never_reported_as_newly_opened() {
        let ok = json!({ "ok": true, "detail": "opened" });

        let already = open_result_text("s-x", "tab", false, false, TabOutcome::AlreadyOpen, &ok);
        assert!(
            already.contains("no new tab was opened"),
            "a dedupe no-op must say so: {already}"
        );
        assert!(
            !already.contains("s-x opened"),
            "…and must not use the verb the model will repeat: {already}"
        );

        let focused = open_result_text(
            "s-x",
            "tab",
            true,
            false,
            TabOutcome::Focused,
            &json!({ "ok": true }),
        );
        assert!(
            focused.contains("brought to the front") && focused.contains("no new tab was opened"),
            "an activate_tab reports a move, not an opening: {focused}"
        );
        assert!(!focused.contains("s-x opened"), "{focused}");

        // A refused `activate_tab` is denied in ITS OWN vocabulary. "NOT opened"
        // here would be true and misleading in the same way "no tab was opened"
        // was for a window request: it invites the model to conclude the view
        // moved anyway.
        let refused = json!({ "ok": false, "detail": "session has no tab" });
        let denied = open_result_text("s-x", "tab", true, false, TabOutcome::Focused, &refused);
        assert!(denied.contains("was NOT brought to the front"), "{denied}");
        assert!(denied.contains("session has no tab"), "{denied}");

        // Announce-only, on a tab that already exists: the setting suppressed a
        // JUMP, not an allocation, and the sentence has to deny the jump.
        let hushed = open_result_text(
            "s-x",
            "tab",
            true,
            true,
            TabOutcome::Focused,
            &json!({ "ok": true }),
        );
        assert!(
            hushed.contains("NOT brought to the front"),
            "announce-only must deny the move it actually suppressed: {hushed}"
        );
        assert!(
            hushed.contains("Do not tell the user you opened or switched"),
            "{hushed}"
        );
        assert!(!hushed.contains("s-x opened"), "{hushed}");
    }
}
