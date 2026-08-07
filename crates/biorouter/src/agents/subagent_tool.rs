use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::LazyLock;

use anyhow::{anyhow, Result};
use futures::FutureExt;
use rmcp::model::{CallToolResult, Content, ErrorCode, ErrorData, Tool};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::agents::subagent_handle::{self, BackgroundSubagent};
use crate::agents::subagent_handler::run_complete_subagent_task;
use crate::agents::subagent_result::SubagentResult;
use crate::agents::subagent_task_config::TaskConfig;
use crate::agents::tool_execution::ToolCallResult;
use crate::agents::AgentConfig;
use crate::providers;
use crate::workflow::build_workflow::build_workflow_from_template;
use crate::workflow::local_workflows::load_local_workflow_file;
use crate::workflow::{SubWorkflow, Workflow};

pub const SUBAGENT_TOOL_NAME: &str = "subagent";
/// The name dispatch actually sees once the workspace extension advertises the
/// tool: extension-advertised tools are prefixed `{extension}__{tool}`
/// (`ExtensionManager::get_prefixed_tools`).
pub const SUBAGENT_TOOL_PREFIXED: &str = "workspace__subagent";

// --- Fork-bomb guard -------------------------------------------------------
// The model is told it can spawn many subagents in parallel, and a subagent can
// itself spawn subagents, so spawning was previously unbounded. Three caps bound
// it: the semaphore throttles *concurrent* subagents; the in-flight ceiling
// refuses outright once too many are queued+running so a recursive spawn storm
// can't accumulate unbounded tasks; and the pending ceiling bounds the QUEUE
// itself (see `max_pending_subagents`). All three are overridable.
fn max_concurrent_subagents() -> usize {
    std::env::var("BIOROUTER_SUBAGENT_MAX_CONCURRENT")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or(DEFAULT_MAX_CONCURRENT_SUBAGENTS)
}
fn max_inflight_subagents() -> usize {
    std::env::var("BIOROUTER_SUBAGENT_MAX_INFLIGHT")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or(DEFAULT_MAX_INFLIGHT_SUBAGENTS)
}
pub const DEFAULT_MAX_CONCURRENT_SUBAGENTS: usize = 8;
pub const DEFAULT_MAX_INFLIGHT_SUBAGENTS: usize = 64;

// --- The pending queue -----------------------------------------------------
//
// **What happened before this bound existed.** A spawn that could not get one
// of the `max_concurrent_subagents()` permits parked on
// `SUBAGENT_SEMAPHORE.acquire()` while still holding its `InflightGuard`, on
// both the blocking and the detached path. So the queue *was* bounded — but
// only transitively, and only by `max_inflight_subagents()`: at shipped
// defaults the deepest reachable queue is 64 in flight minus the 8 that hold a
// permit and are running, i.e. **56**. Nothing measured that depth, nothing
// refused on it, and nothing reported it.
//
// That transitive bound is not one to rest on, for one specific reason: the
// in-flight refusal's own text ends "…or raise BIOROUTER_SUBAGENT_MAX_INFLIGHT."
// An operator (or a model reading its own tool error) who follows that advice
// raises the *only* thing bounding the queue, and the pending side becomes
// genuinely unbounded — `BIOROUTER_SUBAGENT_MAX_INFLIGHT=100000` buys a queue
// of 99_992 parked spawns, each one an unreturned tool call with no session
// row, no handle, no History entry and no timeout.
//
/// **Why 56, and why that is not a behaviour change.** 56 is exactly the
/// deepest queue reachable today at shipped defaults
/// (`DEFAULT_MAX_INFLIGHT_SUBAGENTS` − `DEFAULT_MAX_CONCURRENT_SUBAGENTS`), so
/// a spawn that queues today still queues: at defaults this refuses nothing
/// that was previously accepted. What changes is that the queue now has a bound
/// *of its own* that survives someone raising the in-flight ceiling. It is a
/// deliberate constant rather than a derived `inflight - concurrent`, because
/// deriving it would re-couple it to the knob it exists to be independent of.
///
/// An operator who genuinely wants a deeper queue raises
/// `BIOROUTER_SUBAGENT_MAX_PENDING`, which the refusal names.
pub const DEFAULT_MAX_PENDING_SUBAGENTS: usize = 56;
pub const MAX_PENDING_SUBAGENTS_ENV: &str = "BIOROUTER_SUBAGENT_MAX_PENDING";

/// Read through [`crate::config::Config::get_param`], not `std::env::var`, so
/// the cap in force can be scoped to one task (`with_config_overrides`) as well
/// as set from the environment or `config.yaml`. That matters beyond tests: the
/// value is read in the REQUESTING task and passed down, so the detached
/// background path sees the same cap the caller saw (a `tokio::spawn` does not
/// inherit task-locals).
fn max_pending_subagents() -> usize {
    crate::config::Config::global()
        .get_param::<usize>(MAX_PENDING_SUBAGENTS_ENV)
        .ok()
        .filter(|&n| n > 0)
        .unwrap_or(DEFAULT_MAX_PENDING_SUBAGENTS)
}

static SUBAGENT_SEMAPHORE: LazyLock<Semaphore> =
    LazyLock::new(|| Semaphore::new(max_concurrent_subagents()));
static SUBAGENT_INFLIGHT: AtomicUsize = AtomicUsize::new(0);
static SUBAGENT_PENDING: AtomicUsize = AtomicUsize::new(0);

/// RAII counter for total in-flight subagents (queued + running).
struct InflightGuard;
impl InflightGuard {
    /// Increment and return the new in-flight count.
    fn enter() -> (Self, usize) {
        let prev = SUBAGENT_INFLIGHT.fetch_add(1, Ordering::SeqCst);
        (Self, prev + 1)
    }
}
impl Drop for InflightGuard {
    fn drop(&mut self) {
        SUBAGENT_INFLIGHT.fetch_sub(1, Ordering::SeqCst);
    }
}

/// RAII counter for subagents parked on the concurrency semaphore — the
/// *pending* queue, a strict subset of the in-flight set. Held only across the
/// wait: a spawn that gets a permit immediately is never counted, and one that
/// gets it after waiting stops being counted the instant it does.
struct PendingGuard;
impl PendingGuard {
    /// Increment and return the new queue depth, including this spawn.
    fn enter() -> (Self, usize) {
        let prev = SUBAGENT_PENDING.fetch_add(1, Ordering::SeqCst);
        (Self, prev + 1)
    }
}
impl Drop for PendingGuard {
    fn drop(&mut self) {
        SUBAGENT_PENDING.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Current number of in-flight subagents (test/introspection helper).
pub fn inflight_subagent_count() -> usize {
    SUBAGENT_INFLIGHT.load(Ordering::SeqCst)
}

/// Current number of subagents queued for a concurrency slot
/// (test/introspection helper).
pub fn pending_subagent_count() -> usize {
    SUBAGENT_PENDING.load(Ordering::SeqCst)
}

/// The pure half of the pending gate, so the rule is testable without a
/// semaphore, a runtime or the process environment.
///
/// `depth` is the queue depth *including the spawn being judged*, which is what
/// [`PendingGuard::enter`] returns — so `depth == max_pending` is the last
/// accepted spawn and the refusal starts at `max_pending + 1`, matching the
/// in-flight ceiling's `>` exactly.
fn pending_refusal(depth: usize, max_pending: usize) -> Option<String> {
    if depth <= max_pending {
        return None;
    }
    Some(format!(
        "Subagent queue full: {depth} spawns are already waiting for a free concurrency slot \
         (max {max_pending}, {MAX_PENDING_SUBAGENTS_ENV}). Nothing was started for this one. \
         Wait for running subagents to finish before spawning more, or raise \
         {MAX_PENDING_SUBAGENTS_ENV}."
    ))
}

/// **The one door onto the concurrency semaphore.** Both spawn paths — the
/// blocking one in `execute_subagent` and the detached one inside
/// `spawn_background_subagent`'s `tokio::spawn` — go through here; there is no
/// other `SUBAGENT_SEMAPHORE.acquire()` in the tree. Adding a third path means
/// calling this, not the semaphore.
///
/// `max_pending` is passed in rather than read here so that both doors use the
/// value read in the requesting task (see [`max_pending_subagents`]).
async fn acquire_subagent_permit(
    max_pending: usize,
) -> Result<tokio::sync::SemaphorePermit<'static>, ErrorData> {
    acquire_permit_bounded(&SUBAGENT_SEMAPHORE, max_pending).await
}

/// The gate's body, over an explicit semaphore so a test can drive it with a
/// permit count it controls (the global one is a `LazyLock` fixed at first use).
async fn acquire_permit_bounded(
    semaphore: &'static Semaphore,
    max_pending: usize,
) -> Result<tokio::sync::SemaphorePermit<'static>, ErrorData> {
    // Fast path: a free permit means this spawn never joins the queue, so it is
    // never counted against the queue bound and can never be refused by it.
    if let Ok(permit) = semaphore.try_acquire() {
        return Ok(permit);
    }
    let (_pending, depth) = PendingGuard::enter();
    if let Some(message) = pending_refusal(depth, max_pending) {
        // INVALID_PARAMS, like the in-flight refusal next to it: this is a
        // condition the model can act on (wait, then retry), not an internal
        // fault. The guard drops on this return, so a refused spawn does not
        // itself occupy a queue slot.
        return Err(ErrorData {
            code: ErrorCode::INVALID_PARAMS,
            message: Cow::from(message),
            data: None,
        });
    }
    semaphore.acquire().await.map_err(|e| ErrorData {
        code: ErrorCode::INTERNAL_ERROR,
        message: Cow::from(format!("Subagent semaphore closed: {e}")),
        data: None,
    })
}

// --- BR-71 decisions 24 + 26: glass-box children, bounded ------------------

/// BR-71 decision 26: how many children of ONE parent may hold a visible tab at
/// once. Matches the injected-turn cap for the same reason — a fan-out must not
/// become a tab storm. Beyond it, children run in the background and are
/// reachable from History and from the parent's summary; a spawn is never
/// refused for this.
///
/// Overridable, like the cap it is matched to: decision 26 says "**default** 4",
/// and the sentence that justifies the number points at
/// `BIOROUTER_WORKSPACE_MAX_INJECTED_TURNS`, which is an env var. A hard
/// constant would be a limit, not a default — and a user on a 49" display has a
/// legitimate reason to want six.
pub const DEFAULT_MAX_VISIBLE_CHILD_TABS: usize = 4;
pub const MAX_VISIBLE_CHILD_TABS_ENV: &str = "BIOROUTER_WORKSPACE_MAX_VISIBLE_CHILD_TABS";

/// Pure half, so the parsing rules are testable without touching the process
/// environment (which unit tests share).
fn parse_visible_child_tabs(raw: Option<&str>) -> usize {
    raw.and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_MAX_VISIBLE_CHILD_TABS)
}

pub fn max_visible_child_tabs() -> usize {
    parse_visible_child_tabs(std::env::var(MAX_VISIBLE_CHILD_TABS_ENV).ok().as_deref())
}

/// The resolved visibility of one child, with the reason, so the parent can be
/// told why a tab did not appear instead of silently believing one did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChildVisibility {
    /// A tab will be announced for this child.
    Visible,
    /// The caller passed `visible: false`.
    OptedOut,
    /// No GUI is attached (headless CLI, server-only) — today's behaviour.
    Headless,
    /// A GUI is attached, but the user turned on "never open tabs
    /// automatically" (decision 7 / Task 29). No tab is opened; a notification
    /// names the child instead.
    AnnounceOnly,
    /// The parent already holds `max_visible_child_tabs()` visible slots, so
    /// `VisibleChildGuard::try_claim` refused one. `cap` is the value in force
    /// at the time, which the env override can change.
    BackgroundCapped { cap: usize },
}

impl ChildVisibility {
    pub fn is_visible(&self) -> bool {
        matches!(self, ChildVisibility::Visible)
    }

    /// One sentence for the parent's tool result. Only the capped and
    /// announce-only cases need explaining; the others are what the caller
    /// asked for or already knows.
    pub fn parent_note(&self, child_session_id: &str) -> String {
        match self {
            ChildVisibility::BackgroundCapped { cap } => format!(
                "Subagent {child_session_id} is running in the background: you already have \
                 {cap} subagent tabs open, which is the limit. It is listed in History under \
                 this conversation and you can read it with workspace_read_conversation."
            ),
            ChildVisibility::AnnounceOnly => format!(
                "Subagent {child_session_id} is running, but no tab was opened: the user \
                 turned on \"never open tabs automatically\". Do not tell them you opened a \
                 tab. They can open it from History; you can read it with \
                 workspace_read_conversation."
            ),
            _ => String::new(),
        }
    }
}

/// Decision 24: visible by default when there is a GUI to show it in.
///
/// **The cap is deliberately NOT decided here.** An earlier draft took a
/// `visible_children: usize` argument, which made the sequence
/// `resolve_visibility(…, visible_children_of(parent))` then
/// `VisibleChildGuard::claim(parent)` — a check-then-act with no atomicity, in
/// the one code path that is *specifically* concurrent. Subagent dispatch is
/// excluded from the tool-dispatch semaphore on purpose (the `let bound_dispatch
/// = !is_spawn_tool_call(…)` line in `agent.rs`) and concurrent tool calls in
/// one assistant message are driven by `select_all`, so a fan-out of ten spawns
/// can have all ten read `0` and all ten claim. The cap lives inside
/// `VisibleChildGuard::try_claim`, under one lock: you either hold a slot or you
/// do not.
///
/// `announce_only` is decision 7's user setting, and it is resolved HERE rather
/// than left to the frame transform. `apply_focus_etiquette` (Task 29) rewrites
/// an `open_tab` frame into a notification *after* a slot has been claimed —
/// so with the setting on, every child would consume one of the four cap slots
/// while no tab ever opens, and the fifth child would be told "you already have
/// 4 subagent tabs open, which is the limit" when the true count is zero. That
/// is the same class of lie Task 29 exists to prevent on the `workspace_open`
/// path. Announce-only therefore claims no slot, like `Headless`.
pub fn resolve_visibility(
    requested: Option<bool>,
    gui_attached: bool,
    announce_only: bool,
) -> ChildVisibility {
    if requested == Some(false) {
        return ChildVisibility::OptedOut;
    }
    if !gui_attached {
        return ChildVisibility::Headless;
    }
    if announce_only {
        return ChildVisibility::AnnounceOnly;
    }
    ChildVisibility::Visible
}

/// Live count of visible children per parent session. RAII, like the in-flight
/// subagent counter above: the slot is released when the child's run ends, so a
/// parent that spawns four, waits, and spawns four more shows tabs every time.
static VISIBLE_CHILDREN: LazyLock<std::sync::Mutex<HashMap<String, usize>>> =
    LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

pub struct VisibleChildGuard {
    parent: String,
}

impl VisibleChildGuard {
    /// Claim one visible-tab slot for `parent_session_id`, or `None` if the
    /// parent is already at the cap. Check and increment happen under the SAME
    /// lock acquisition — that single property is what makes the cap hold for a
    /// parallel fan-out, which is the only case it exists for.
    pub fn try_claim(parent_session_id: &str) -> Option<Self> {
        let cap = max_visible_child_tabs();
        let mut map = VISIBLE_CHILDREN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let count = map.entry(parent_session_id.to_string()).or_insert(0);
        if *count >= cap {
            // Leave the entry at its current value; `Drop` only decrements
            // slots that were actually granted.
            return None;
        }
        *count += 1;
        Some(Self {
            parent: parent_session_id.to_string(),
        })
    }
}

impl Drop for VisibleChildGuard {
    fn drop(&mut self) {
        let mut map = VISIBLE_CHILDREN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(count) = map.get_mut(&self.parent) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                map.remove(&self.parent);
            }
        }
    }
}

pub fn visible_children_of(parent_session_id: &str) -> usize {
    VISIBLE_CHILDREN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(parent_session_id)
        .copied()
        .unwrap_or(0)
}

/// The CLOSED placement vocabulary for a spawned child's tab.
///
/// It is *checked* rather than forwarded for the same reason `workspace_open`
/// checks it (`handle_open` in `workspace_extension.rs`, which carries the
/// original note): `announce_subagent_tab` below branches on
/// `placement == "window"` and hands everything else to `open_tab` verbatim,
/// and the GUI planner (`workspaceCommandPlanner.ts`, `case 'open_tab'`) only
/// special-cases `"split"`. So an unvalidated typo — `"windows"`, `"Window"` —
/// is not an error the renderer reports; it is a tab, silently, which is the one
/// outcome the caller did not ask for. The tool schema's `enum` is advice to the
/// model, not a constraint on the JSON that arrives: `serde` does not enforce it.
///
/// Returns `&'static str` so the accepted spellings are the only values that can
/// ever reach a frame.
fn validate_placement(requested: Option<&str>) -> Result<&'static str, String> {
    match requested.unwrap_or("tab") {
        "tab" => Ok("tab"),
        "split" => Ok("split"),
        "window" => Ok("window"),
        other => Err(format!(
            "unknown placement {other:?} — use \"tab\" (default), \"split\" or \"window\""
        )),
    }
}

/// BR-71 §4.5 step 3: announce the child over the WorkspaceBridge. Background
/// open (never steals the composer) + a subagent badge carrying the parent link.
/// Returns the resolved visibility so the caller can fold
/// `ChildVisibility::parent_note` into the tool result.
///
/// Fire-and-forget on the wire: a refused split or a disconnecting window must
/// never break a spawn.
fn announce_subagent_tab(
    child_session_id: &str,
    parent_session_id: &str,
    params: &SubagentParams,
) -> (ChildVisibility, Option<VisibleChildGuard>) {
    let services = crate::workspace_services::get();
    let gui_attached = services.as_ref().is_some_and(|s| s.gui_attached());
    let announce_only = crate::agents::workspace_extension::announce_only_enabled();
    let visibility = resolve_visibility(params.visible, gui_attached, announce_only);

    // Nothing reaches the GUI for these two.
    if matches!(
        visibility,
        ChildVisibility::OptedOut | ChildVisibility::Headless
    ) {
        return (visibility, None);
    }

    // A SLOT IS CLAIMED ONLY FOR A REAL TAB. `AnnounceOnly` still tells the user
    // about the child (the frame below is downgraded to a notification by
    // `apply_focus_etiquette`), but it opens nothing, so claiming would have the
    // fifth child of a fan-out told "you already have 4 subagent tabs open,
    // which is the limit" while zero tabs exist.
    let mut visibility = visibility;
    let guard = if visibility.is_visible() {
        // The cap is the claim: no separate read of the counter, so a parallel
        // fan-out cannot slip past it. Failing to claim is not a refusal — the
        // child runs, it just runs in the background, and `parent_note` tells
        // the model why (decision 26).
        match VisibleChildGuard::try_claim(parent_session_id) {
            Some(guard) => Some(guard),
            None => {
                visibility = ChildVisibility::BackgroundCapped {
                    cap: max_visible_child_tabs(),
                };
                None
            }
        }
    } else {
        None
    };

    let Some(services) = services else {
        return (visibility, guard);
    };

    // A capped child opens NOTHING — that is the whole of decision 26 — but it
    // still gets its badge below. It does not fall out here with the opted-out
    // and headless children, because a capped child is precisely the one the
    // user opens later from History, and `ChatGroupsContext` stores
    // `annotate_tab` in `tabAnnotations` keyed by session id whether or not a
    // tab exists yet. Sending it now is what makes that later tab show as a
    // subagent of its parent.
    let open_a_tab = !matches!(visibility, ChildVisibility::BackgroundCapped { .. });

    // `handle_subagent_tool` already rejected anything outside the vocabulary,
    // before a session was created. Defaulting here as well means no frame can
    // carry an unknown placement even if a future caller reaches this function
    // without going through that check — the failure mode is a silent tab, so
    // the belt is cheaper than the diagnosis.
    let placement = validate_placement(params.placement.as_deref()).unwrap_or("tab");
    let child = child_session_id.to_string();
    let parent = parent_session_id.to_string();
    tokio::spawn(async move {
        if open_a_tab {
            announce_open_frame(services.as_ref(), &child, placement, announce_only).await;
        }
        // The badge is NOT focus-stealing, so it is sent for every child this
        // function announced at all — including a capped one, which has no tab
        // yet and is exactly the child the user opens later from History. The
        // renderer stores it by session id (`ChatGroupsContext`'s
        // `tabAnnotations`), so it is already waiting when that tab appears.
        let _ = services
            .gui_command(
                serde_json::json!({
                    "type": "workspace", "cmd": "annotate_tab",
                    "session_id": child, "badge": "subagent", "parent_session_id": parent,
                }),
                false,
            )
            .await;
    });
    (visibility, guard)
}

/// The `open_tab` / `open_window` half of [`announce_subagent_tab`], split out so
/// the capped path can skip it without duplicating the badge send.
async fn announce_open_frame(
    services: &dyn crate::workspace_services::WorkspaceServices,
    child_session_id: &str,
    placement: &'static str,
    announce_only: bool,
) {
    // Frame vocabulary parity with workspace_open (Task 24): "window" is its
    // own cmd; tab/split ride open_tab. Focus etiquette (Task 29) downgrades
    // either to a notification when announce-only is on — which is exactly the
    // `ChildVisibility::AnnounceOnly` path.
    let open_frame = if placement == "window" {
        serde_json::json!({
            "type": "workspace", "cmd": "open_window", "session_id": child_session_id,
        })
    } else {
        serde_json::json!({
            "type": "workspace", "cmd": "open_tab",
            "session_id": child_session_id, "placement": placement, "focus": false,
        })
    };
    // ⚠ KNOWN TRADE-OFF, stated because it is otherwise invisible.
    // `wait_result: false` means a GUI *refusal* — `refuse("split refused:
    // already at 4 groups")`, or `open_tab` failing for any other reason — is
    // discarded here, while the caller has already been handed
    // `ChildVisibility::Visible` (whose `parent_note` is empty). So in that
    // narrow case the model believes a tab opened when none did, and the cap
    // slot stays claimed for the child's whole run. `workspace_open` does
    // better on its own path because `place_in_gui` can afford to park on the
    // round-trip and thread the answer into `open_result_text`.
    //
    // Not fixed here, deliberately: turning the note honest would mean awaiting
    // this frame before `announce_subagent_tab` returns, which couples every
    // spawn to the renderer — `emit_and_wait` gives up only after 10 s, so one
    // wedged window would stall every fan-out. The rule for this path is
    // fire-and-forget: a refused split or a disconnecting window must never
    // break a spawn, and a spawn is far more expensive to lose than a misplaced
    // tab is to notice. The lie is also bounded — the child exists, runs, and is
    // reachable from History and `workspace_read_conversation` wherever its tab
    // did or did not land.
    let _ = services
        .gui_command(
            crate::agents::workspace_extension::apply_focus_etiquette(open_frame, announce_only),
            false,
        )
        .await;
}

const SUMMARY_INSTRUCTIONS: &str = r#"
Important: Your parent agent will only receive your final message as a summary of your work.
Make sure your last message provides a comprehensive summary of:
- What you were asked to do
- What actions you took
- The results or outcomes
- Any important findings or recommendations

Be concise but complete.
"#;

#[derive(Debug, Deserialize)]
pub struct SubagentParams {
    pub instructions: Option<String>,
    pub subworkflow: Option<String>,
    pub parameters: Option<HashMap<String, Value>>,
    pub extensions: Option<Vec<String>>,
    pub settings: Option<SubagentSettings>,
    #[serde(default = "default_summary")]
    pub summary: bool,
    /// BR-40: run detached and return a handle immediately instead of blocking
    /// the parent's turn for the child's whole run. Ignored (and not advertised)
    /// unless `BIOROUTER_SUBAGENT_BACKGROUND` is on, so the default is the
    /// historical blocking call.
    #[serde(default)]
    pub background: bool,
    /// BR-71 §4.5: open the child as a visible tab. Defaults to true when a GUI
    /// is attached and false headless (Task 36 resolves it); `false` forces
    /// today's invisible run even with the app open.
    #[serde(default)]
    pub visible: Option<bool>,
    /// "tab" (default) | "split" | "window" — where the child's tab opens.
    #[serde(default)]
    pub placement: Option<String>,
}

fn default_summary() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct SubagentSettings {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub temperature: Option<f32>,
}

pub fn create_subagent_tool(sub_workflows: &[SubWorkflow]) -> Tool {
    let description = build_tool_description(sub_workflows);

    let mut schema = json!({
        "type": "object",
        "properties": {
            "instructions": {
                "type": "string",
                "description": "Instructions for the subagent. Required for ad-hoc tasks. For predefined tasks, adds additional context."
            },
            "subworkflow": {
                "type": "string",
                "description": "Name of a predefined subworkflow to run."
            },
            "parameters": {
                "type": "object",
                "additionalProperties": true,
                "description": "Parameters for the subworkflow. Only valid when 'subworkflow' is specified."
            },
            "extensions": {
                "type": "array",
                "items": {"type": "string"},
                "description": "Extensions to enable. Omit to inherit all, empty array for none."
            },
            "settings": {
                "type": "object",
                "properties": {
                    "provider": {"type": "string", "description": "Override LLM provider"},
                    "model": {"type": "string", "description": "Override model"},
                    "temperature": {"type": "number", "description": "Override temperature"}
                },
                "description": "Override model/provider settings."
            },
            "summary": {
                "type": "boolean",
                "default": true,
                "description": "If true (default), return only the subagent's final summary."
            },
            "visible": {
                "type": "boolean",
                "description": "Show this subagent in its own tab that the user can watch and talk to. Defaults to true when the desktop app is open. Pass false to run it silently."
            },
            "placement": {
                "type": "string",
                "enum": ["tab", "split", "window"],
                "description": "Where the subagent's tab opens. Default \"tab\" (background, never steals focus)."
            }
        }
    });

    // BR-40: the background parameter only exists when the async-handle path is
    // enabled — an advertised parameter the tool would then ignore is worse than
    // no parameter at all.
    if subagent_handle::background_enabled() {
        schema["properties"]["background"] = json!({
            "type": "boolean",
            "default": false,
            "description": "If true, start the subagent and return its session id immediately \
                            instead of waiting for it. Wait for it later with `workspace_watch`, \
                            read it with `workspace_read_conversation`, stop it with \
                            `workspace_close`. Use for long tasks you want to run while you \
                            keep working."
        });
    }

    Tool::new(
        SUBAGENT_TOOL_NAME,
        description,
        schema.as_object().unwrap().clone(),
    )
}

/// `pub(crate)` so `Agent::list_tools` can restore the sub-workflow-enriched
/// description onto the tool the workspace extension advertises with `&[]` —
/// only the agent holds the `sub_workflows` map.
pub(crate) fn build_tool_description(sub_workflows: &[SubWorkflow]) -> String {
    let mut desc = String::from(
        "Delegate a task to a subagent that runs independently with its own context.\n\n\
         Modes:\n\
         1. Ad-hoc: Provide `instructions` for a custom task\n\
         2. Predefined: Provide `subworkflow` name to run a predefined task\n\
         3. Augmented: Provide both `subworkflow` and `instructions` to add context\n\n\
         The subagent has access to the same tools as you by default. \
         Use `extensions` to limit which extensions the subagent can use.\n\n\
         For parallel execution, make multiple `subagent` tool calls in the same message.",
    );

    if subagent_handle::background_enabled() {
        desc.push_str(
            "\n\nBy default the call blocks until the subagent finishes. For a long task, \
             pass `background: true` to get the child's session id back immediately and \
             keep working; wait for it later with `workspace_watch`, read it with \
             `workspace_read_conversation`, stop it with `workspace_close`.",
        );
    }

    if !sub_workflows.is_empty() {
        desc.push_str("\n\nAvailable subworkflows:");
        for sr in sub_workflows {
            let params_info = get_subworkflow_params_description(sr);
            let sequential_hint = if sr.sequential_when_repeated {
                " [run sequentially, not in parallel]"
            } else {
                ""
            };
            desc.push_str(&format!(
                "\n• {}{} - {}{}",
                sr.name,
                sequential_hint,
                sr.description.as_deref().unwrap_or("No description"),
                if params_info.is_empty() {
                    String::new()
                } else {
                    format!(" (params: {})", params_info)
                }
            ));
        }
    }

    desc
}

fn get_subworkflow_params_description(sub_workflow: &SubWorkflow) -> String {
    match load_local_workflow_file(&sub_workflow.path) {
        Ok(workflow_file) => match Workflow::from_content(&workflow_file.content) {
            Ok(workflow) => {
                if let Some(params) = workflow.parameters {
                    params
                        .iter()
                        .filter(|p| {
                            sub_workflow
                                .values
                                .as_ref()
                                .map(|v| !v.contains_key(&p.key))
                                .unwrap_or(true)
                        })
                        .map(|p| {
                            let req = match p.requirement {
                                crate::workflow::WorkflowParameterRequirement::Required => {
                                    "[required]"
                                }
                                _ => "[optional]",
                            };
                            format!("{} {}", p.key, req)
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                } else {
                    String::new()
                }
            }
            Err(_) => String::new(),
        },
        Err(_) => String::new(),
    }
}

/// Note: SubWorkflow.sequential_when_repeated is surfaced as a hint in the tool description
/// (e.g., "[run sequentially, not in parallel]") but not enforced. The LLM controls
/// sequencing by making sequential vs parallel tool calls.
pub fn handle_subagent_tool(
    config: &AgentConfig,
    params: Value,
    task_config: TaskConfig,
    sub_workflows: HashMap<String, SubWorkflow>,
    working_dir: PathBuf,
    cancellation_token: Option<CancellationToken>,
) -> ToolCallResult {
    let parsed_params: SubagentParams = match serde_json::from_value(params) {
        Ok(p) => p,
        Err(e) => {
            return ToolCallResult::from(Err(ErrorData {
                code: ErrorCode::INVALID_PARAMS,
                message: Cow::from(format!("Invalid parameters: {}", e)),
                data: None,
            }));
        }
    };

    if parsed_params.instructions.is_none() && parsed_params.subworkflow.is_none() {
        return ToolCallResult::from(Err(ErrorData {
            code: ErrorCode::INVALID_PARAMS,
            message: Cow::from("Must provide 'instructions' or 'subworkflow' (or both)"),
            data: None,
        }));
    }

    if parsed_params.parameters.is_some() && parsed_params.subworkflow.is_none() {
        return ToolCallResult::from(Err(ErrorData {
            code: ErrorCode::INVALID_PARAMS,
            message: Cow::from("'parameters' can only be used with 'subworkflow'"),
            data: None,
        }));
    }

    // BR-71 decision 24: checked HERE, before a session, an inflight slot or a
    // visible-tab slot exists — the same order `workspace_open` uses. A rejected
    // typo costs the caller one retry; a forwarded one costs it a tab where it
    // asked for a window, and it is never told.
    if let Err(e) = validate_placement(parsed_params.placement.as_deref()) {
        return ToolCallResult::from(Err(ErrorData {
            code: ErrorCode::INVALID_PARAMS,
            message: Cow::from(e),
            data: None,
        }));
    }

    let workflow = match build_workflow(&parsed_params, &sub_workflows) {
        Ok(r) => r,
        Err(e) => {
            return ToolCallResult::from(Err(ErrorData {
                code: ErrorCode::INVALID_PARAMS,
                message: Cow::from(e.to_string()),
                data: None,
            }));
        }
    };

    let config = config.clone();
    ToolCallResult {
        notification_stream: None,
        result: Box::new(
            execute_subagent(
                config,
                workflow,
                task_config,
                parsed_params,
                working_dir,
                cancellation_token,
            )
            .boxed(),
        ),
    }
}

async fn execute_subagent(
    config: AgentConfig,
    workflow: Workflow,
    task_config: TaskConfig,
    params: SubagentParams,
    working_dir: PathBuf,
    cancellation_token: Option<CancellationToken>,
) -> Result<rmcp::model::CallToolResult, ErrorData> {
    // Fork-bomb guard: count this spawn, refuse if too many are already in
    // flight, then throttle concurrency — and, if every concurrency slot is
    // taken, refuse rather than join an unbounded queue (`acquire_subagent_permit`).
    // The guard + permit are held until the subagent finishes — on the blocking
    // path that is when this function returns; on the background path the guard
    // moves into the detached task, so a storm of background spawns is bounded
    // exactly like a storm of blocking ones.
    let (inflight, inflight_count) = InflightGuard::enter();
    let max_inflight = max_inflight_subagents();
    // Read HERE, in the requesting task, and carried to whichever door claims
    // the permit. The detached path claims its permit inside a `tokio::spawn`,
    // which does not inherit this task's config scope, so reading it there would
    // silently ignore a per-task override the caller set.
    let max_pending = max_pending_subagents();
    if inflight_count > max_inflight {
        return Err(ErrorData {
            code: ErrorCode::INVALID_PARAMS,
            message: Cow::from(format!(
                "Subagent limit reached: {inflight_count} already in flight (max {max_inflight}). \
                 Wait for running subagents to finish, or raise BIOROUTER_SUBAGENT_MAX_INFLIGHT."
            )),
            data: None,
        });
    }

    // BR-40: detached run — create the child session (so the handle can name it),
    // register the handle, and hand it straight back to the parent.
    if params.background && subagent_handle::background_enabled() {
        // Issue #56: resolve the child's provider and tier BEFORE creating its
        // row. A refusal must not leave a durable `SubAgent` session behind, and
        // this path runs the whole stretch in a detached `tokio::spawn`.
        let task_config = overridden_task_config(task_config, &params).await?;
        let session = create_subagent_session(
            &config,
            working_dir,
            &task_config.parent_session_id,
            &params,
            task_config.privacy_tier,
        )
        .await?;
        return Ok(spawn_background_subagent(
            config,
            workflow,
            task_config,
            &params,
            session.id,
            inflight,
            max_pending,
        ));
    }

    // Door 1 of 2 onto the concurrency semaphore (door 2 is inside
    // `spawn_background_subagent`'s detached task). Both call the same gate.
    let _permit = acquire_subagent_permit(max_pending).await?;
    let _inflight = inflight;

    // Issue #56: resolve the child's provider and tier BEFORE creating its row,
    // exactly as on the background path above. `overridden_task_config` can
    // fail with `?` — an unknown provider, a model that will not construct, and
    // now R4's spawn refusal — and a row created first is a durable orphan
    // `SubAgent` session with no provider and no run.
    //
    // It also stays ahead of the announce, for the older reason: announcing
    // first would leave a tab open — and one of the four cap slots claimed until
    // the guard drops — for a child that never runs a turn. The override never
    // touches `parent_session_id`, so it reads the same either way.
    let task_config = overridden_task_config(task_config, &params).await?;

    let session = create_subagent_session(
        &config,
        working_dir,
        &task_config.parent_session_id,
        &params,
        task_config.privacy_tier,
    )
    .await?;

    // BR-71 decision 24: glass-box by default. The guard lives for the child's
    // whole run, so the slot is released exactly when the child finishes.
    let (visibility, _visible_guard) =
        announce_subagent_tab(&session.id, &task_config.parent_session_id, &params);
    let visibility_note = visibility.parent_note(&session.id);
    // Taken before `task_config` is moved into the run below.
    let privacy_note = dropped_extension_note(&task_config.dropped_private_extensions);
    let affiliation_note =
        cross_affiliation_drop_note(&task_config.dropped_cross_affiliation_extensions);

    // The result envelope encodes success, an incomplete (tool-call-ending)
    // run, or a failure — all as structured content — so this always returns a
    // CallToolResult (with `is_error` set) rather than a bare tool error.
    let result = run_complete_subagent_task(
        config,
        workflow,
        task_config,
        params.summary,
        session.id,
        cancellation_token,
    )
    .await;

    let mut call_result = result.into_call_tool_result();
    if !visibility_note.is_empty() {
        call_result.content.push(Content::text(visibility_note));
    }
    // Issue #56: a capability the child was denied is reported, never dropped
    // silently — the model delegated on the strength of a tool list it believes
    // the child holds.
    if let Some(note) = privacy_note {
        call_result.content.push(Content::text(note));
    }
    // Task 48 (DR-26): the affiliation drop is its own disclosure, for its own
    // reason. Both can fire on one spawn — a public child losing a private
    // extension AND a foreign-institution one losing another — and neither may
    // swallow the other.
    if let Some(note) = affiliation_note {
        call_result.content.push(Content::text(note));
    }
    Ok(call_result)
}

/// A one-line label for a spawned child, so two siblings of one fan-out are
/// distinguishable everywhere a session is listed — `biorouter session list`,
/// Task 38's grouped History, and `workspace_list` all read `Session.name`.
///
/// Prefers the subworkflow name (a run of a named workflow is best identified by
/// it) and otherwise the first non-empty line of the ad-hoc instructions.
/// Falls back to the historical literal so a paramless spawn is never nameless.
pub(crate) fn subagent_session_label(params: &SubagentParams) -> String {
    const MAX: usize = 60;
    // ⚠ This is MODEL-authored text that `biorouter session list` prints
    // straight to a terminal, so control characters are stripped here rather
    // than at each print site. `lines()` + `trim` already drop `\n` and `\r\n`;
    // they do NOT drop an embedded `\x1b[` (which would let a paraphrased file
    // excerpt repaint the listing), a bare `\r` (which rewrites the line just
    // printed), or `\x07`. Subagent instructions routinely paraphrase content
    // the parent agent has just read, so this needs no ill intent to trigger.
    let first_line = |s: &str| -> Option<String> {
        s.lines()
            .map(|line| {
                line.chars()
                    .filter(|c| !c.is_control())
                    .collect::<String>()
                    .trim()
                    .to_string()
            })
            .find(|line| !line.is_empty())
    };
    let source = params
        .subworkflow
        .as_deref()
        .and_then(first_line)
        .or_else(|| params.instructions.as_deref().and_then(first_line));
    let Some(source) = source else {
        return "Subagent task".to_string();
    };
    let mut label: String = source.chars().take(MAX).collect();
    if source.chars().count() > MAX {
        label.push('…');
    }
    format!("Subagent: {label}")
}

/// Create the child session and stamp its `parent_session_id` (BR-71) at birth.
///
/// `persist_spawn_context` stamps it too, but only once `get_agent_messages` has
/// reached the system-prompt override. Everything before that — the provider
/// update, extension loading — can fail with `?`, and the `background: true`
/// path hands the child's session id back to the parent *immediately*, before
/// the run starts at all. Stamping here means the row is never an orphan in that
/// window: History can group it, and the workspace tools can resolve its parent,
/// even for a child that dies before its first turn.
///
/// The stamp fails the spawn with `?` here, while the identical stamp inside
/// `persist_spawn_context` only warns. That split is a decision, not an
/// oversight: at this point nothing has been spent, and `create_session` on the
/// same store two statements up already aborts the spawn — so a targeted UPDATE
/// failing here means the store is unusable and continuing would only mint a
/// permanently unparented row that no later path retries. By the time
/// `persist_spawn_context` runs, the parent id is already durable from here and
/// a configured agent is one line from its first turn, so the same failure is
/// no longer worth the run. See the matching note at that call site.
async fn create_subagent_session(
    config: &AgentConfig,
    working_dir: PathBuf,
    parent_session_id: &str,
    params: &SubagentParams,
    privacy_tier: crate::privacy::SessionClassification,
) -> Result<crate::session::Session, ErrorData> {
    let internal = |e: &dyn std::fmt::Display| ErrorData {
        code: ErrorCode::INTERNAL_ERROR,
        message: Cow::from(format!("Failed to create session: {e}")),
        data: None,
    };

    // Issue #56 finding 11. The stamp is the child's own capability floor RAISED
    // to its parent's classification, and the raise is read here rather than
    // taken from `task_config`, because this is the statement that writes the
    // row. See [`parent_classification`] for why it is ungated.
    let privacy_tier = privacy_tier.max(parent_classification(config, parent_session_id).await);

    let mut session = config
        .session_manager
        .create_session(
            working_dir,
            subagent_session_label(params),
            crate::session::session_manager::SessionType::SubAgent,
        )
        .await
        .map_err(|e| internal(&e))?;

    config
        .session_manager
        .update(&session.id)
        .parent_session_id(Some(parent_session_id.to_string()))
        // Issue #56 §8.2: the child's classification is stamped in the SAME
        // statement as its parent link, and the capability half of
        // `privacy_tier` reaches here already decided —
        // `apply_settings_overrides` runs before this function on both spawn
        // paths, so a refused spawn never gets as far as a row.
        //
        // `raise_privacy` rather than a plain set, because the storage layer is
        // what refuses a downgrade; on a row created two statements up it is
        // always a raise from `public`, so the stamp is exact.
        .raise_privacy(privacy_tier, &format!("inherited:{parent_session_id}"))
        .apply()
        .await
        .map_err(|e| internal(&e))?;
    // Keep the in-memory copy honest with the row we just wrote.
    session.parent_session_id = Some(parent_session_id.to_string());
    session.privacy_tier = privacy_tier;

    Ok(session)
}

/// The spawning session's own classification — the floor its child's row is born
/// at, **ungated by DR-15's master switch** (issue #56, finding 11).
///
/// ⚠ **Enforcement and classification are different things, and the switch is
/// only allowed to disable the first.** Every *gate* in
/// [`apply_settings_overrides`] is behind `privacy_tiers_enabled()`, so with the
/// switch off a Private-classified chat may be bound to, and may run, a public
/// provider. The child of such a chat resolves `child_tier = Public`, and the
/// capability floor alone would therefore stamp its row `public` — permanently,
/// because re-enabling never revisits a row (AR-7). The documented opt-out would
/// then be a way to mint rows that turning protection back ON cannot correct,
/// which is a *worse* outcome than the enforcement it was asked to suspend.
///
/// So the parent's classification is carried across unconditionally. This is the
/// same rule, in the same polarity, that
/// `SessionStorage::create_derived_session` already applies to the copy/diverge
/// paths — carrying a parent's stamp is **column propagation**, not a
/// classification decision, and `privacy_toggle.rs`'s row 17 asserts that
/// identically in both toggle columns.
///
/// ⚠ **It cannot make the switch-off case worse than the switch-on case**, which
/// is what keeps this from becoming enforcement by the back door. `max` over
/// [`SessionClassification`] can only *raise*, and with the switch ON the raise
/// is provably a no-op: a Private-classified parent can only have bound a
/// private provider (Gate A) and can only have reached this spawn inside a turn
/// that same tier permitted (Gate B), so `floor(child_tier)` is already Private.
/// Only the switch-off state — a private row running a public model — is changed
/// by this, and only in the direction of keeping the row's true classification.
///
/// ⚠ **An unreadable parent keeps today's stamp rather than failing closed**, and
/// that is a deliberate, bounded trade rather than an oversight. The production
/// caller (`Agent::dispatch`) builds the `TaskConfig` from a session it has just
/// loaded, so the row is always there; a read that fails means the store is
/// gone, and `create_session` below would fail on it anyway. Failing closed to
/// Private instead would make every such spawn refuse its own provider bind at
/// `subagent_handler`'s `update_provider` (Gate A) and mint permanently
/// over-classified rows in the process — a raise is what §12.5's
/// declassification exists to undo, but only with the three proofs, and paying
/// that price for a store error is the worse of the two directions.
async fn parent_classification(
    config: &AgentConfig,
    parent_session_id: &str,
) -> crate::privacy::SessionClassification {
    match config
        .session_manager
        .get_session(parent_session_id, false)
        .await
    {
        Ok(parent) => parent.privacy_tier,
        Err(e) => {
            tracing::warn!(
                parent_session_id = %parent_session_id,
                error = %e,
                "could not read the spawning session's classification; the child's row is \
                 stamped from its own provider alone"
            );
            // The identity element of `max`, so the caller's own floor stands.
            crate::privacy::SessionClassification::Public
        }
    }
}

/// Issue #56 §8.2: what the parent is told when its child was denied one of the
/// parent's own private extensions.
///
/// A dropped capability MUST be reported. The model chose to delegate on the
/// strength of the tool list it believes the child holds; a silent removal
/// leaves it planning around a tool the child will answer "unknown tool" to, and
/// re-spawning to work around it. §14.4 bounds the text: it names the extensions
/// (which the parent already holds, so nothing is disclosed) and the boundary,
/// and no session content.
///
/// `None` when nothing was dropped, so the call sites read as
/// `if let Some(note) = …`.
fn dropped_extension_note(dropped: &[String]) -> Option<String> {
    if dropped.is_empty() {
        return None;
    }
    Some(format!(
        "Note: this subagent is running on a public model, so it was NOT given these private \
         extensions of yours: {}. They reach data held inside the institution, so only a private \
         model may call them. Do not ask the subagent to use them, and do not re-spawn it to try \
         again — the boundary is the same every time. If the task needs them, do it in this chat \
         instead, or {}",
        dropped.join(", "),
        crate::privacy::refusal::ASK_THE_USER_TO_SWITCH
    ))
}

/// Issue #56 Task 48 (DR-26): what the parent is told when its child was denied
/// one of the parent's extensions on the **affiliation** axis.
///
/// A separate sentence from [`dropped_extension_note`] rather than a shared one
/// with a substituted reason, because the two drops are not the same event.
/// That one says "this subagent is running on a public model", which is false
/// here — the child is Private, and what it may not cross is an institutional
/// boundary. DR-26 requires a statement specific enough to act on, so each
/// extension's own warning is quoted rather than a generic sentence composed
/// from its name.
///
/// ⚠ **It does not tell the model to escalate to the user, and that is not an
/// omission.** The parent is an agent; DR-26 says an agent never clears a
/// cross-institutional warning automatically. Telling it to "ask the user to
/// approve" here would invite exactly the escalation the ruling puts on the
/// USER's own surfaces (the bind prompt, `/agent/add_extension`), reached
/// through a model that has already been told the answer is no. What it is told
/// is what changed and why, so it can report accurately and stop planning
/// around a capability the child does not have.
///
/// `None` when nothing was dropped, so the call sites read as
/// `if let Some(note) = …`.
fn cross_affiliation_drop_note(dropped: &[(String, String)]) -> Option<String> {
    if dropped.is_empty() {
        return None;
    }
    let each = dropped
        .iter()
        .map(|(name, warning)| format!("- `{name}`: {warning}"))
        .collect::<Vec<_>>()
        .join("\n");
    Some(format!(
        "Note: this subagent was NOT given these extensions of yours, because they hold another \
         institution's data and the subagent's model is covered by a different institution's \
         agreements:\n{each}\nDo not ask the subagent to use them, and do not re-spawn it to try \
         again — the boundary is the same every time. Compliance does not transfer between \
         institutions. If the task needs them, do it in this chat instead, or tell the user what \
         you were trying to do and let them decide."
    ))
}

async fn overridden_task_config(
    task_config: TaskConfig,
    params: &SubagentParams,
) -> Result<TaskConfig, ErrorData> {
    apply_settings_overrides(task_config, params)
        .await
        .map_err(|e| ErrorData {
            code: ErrorCode::INVALID_PARAMS,
            message: Cow::from(e.to_string()),
            data: None,
        })
}

/// Run the subagent on a detached task and return its handle immediately.
///
/// The child gets a **fresh** cancellation token rather than the parent turn's:
/// the whole point of a background subagent is to outlive the turn that started
/// it, and inheriting the parent's token would kill it the moment that turn
/// ended. The token stays reachable — `workspace_close` (BR-71 decision 23's
/// replacement for the old `subagent_status { cancel: true }`) and the BR-42
/// active-work view (registered inside `run_complete_subagent_task`) both route
/// to it.
fn spawn_background_subagent(
    config: AgentConfig,
    workflow: Workflow,
    task_config: TaskConfig,
    params: &SubagentParams,
    child_session_id: String,
    inflight: InflightGuard,
    max_pending: usize,
) -> CallToolResult {
    let summary = params.summary;
    let title = background_title(&workflow);
    let cancel = CancellationToken::new();
    let handle = BackgroundSubagent::register(
        task_config.parent_session_id.clone(),
        child_session_id.clone(),
        // The title is no longer spliced into the assistant-facing text (it
        // reads off the handle's snapshot instead), so this is its last use.
        title,
        cancel.clone(),
    );

    // BR-71 decision 24 on the detached path. The guard moves into the task, so
    // the visible-tab slot is released when the child's run ends, not when this
    // function returns (which is immediately).
    let (visibility, visible_guard) =
        announce_subagent_tab(&child_session_id, &task_config.parent_session_id, params);
    // Taken before `task_config` moves into the detached task below.
    let privacy_note = dropped_extension_note(&task_config.dropped_private_extensions);
    let affiliation_note =
        cross_affiliation_drop_note(&task_config.dropped_cross_affiliation_extensions);

    let task_handle = handle.clone();
    tokio::spawn(async move {
        // Held for the child's whole life, exactly as on the blocking path.
        let _inflight = inflight;
        let _visible = visible_guard;
        // Door 2 of 2 onto the concurrency semaphore. A queue-full refusal here
        // cannot be returned to the parent — this function already returned the
        // handle — so it completes the handle with the refusal instead, which is
        // what `workspace_watch` / `workspace_read_conversation` will report. The
        // child session row already exists (it is created before the spawn so the
        // handle can name it) and is left as a zero-turn session, exactly as it
        // would be for any other run that never started.
        let _permit = match acquire_subagent_permit(max_pending).await {
            Ok(permit) => permit,
            Err(e) => {
                task_handle.complete(SubagentResult::from_error(e.message.to_string()));
                return;
            }
        };

        let result = run_complete_subagent_task(
            config,
            workflow,
            task_config,
            summary,
            child_session_id,
            Some(cancel),
        )
        .await;
        task_handle.complete(result);
    });

    let mut text = background_started_message(
        &handle.id,
        &handle.child_session_id,
        &visibility.parent_note(&handle.child_session_id),
    );
    // Issue #56: same disclosure as the blocking path. It has to ride this
    // message, because the background path returns before any `SubagentResult`
    // exists.
    if let Some(note) = privacy_note {
        text.push_str("\n\n");
        text.push_str(&note);
    }
    // Task 48 (DR-26), same reasoning: the background path returns before any
    // `SubagentResult` exists, so this message is the only carrier.
    if let Some(note) = affiliation_note {
        text.push_str("\n\n");
        text.push_str(&note);
    }

    CallToolResult {
        content: vec![Content::text(text)],
        structured_content: serde_json::to_value(handle.snapshot()).ok(),
        is_error: Some(false),
        meta: None,
    }
}

/// What a `background: true` spawn returns to the parent. BR-71 decision 23:
/// there is no dedicated poll tool any more, and the child's SESSION ID — not
/// the registry handle id — is what every workspace tool takes.
///
/// `visibility_note` carries `ChildVisibility::parent_note` (Task 36) when the
/// child ended up in the background for a reason the parent needs to know —
/// notably decision 26's 4-tab cap. The background path returns IMMEDIATELY,
/// before the `SubagentResult` exists, so the result's assistant-facing text
/// (which is where Task 36 otherwise appends the note) is not reachable here:
/// without this argument, the model is never told WHY a fan-out's fifth child
/// has no tab, which is precisely the case the cap exists for.
fn background_started_message(
    handle_id: &str,
    child_session_id: &str,
    visibility_note: &str,
) -> String {
    let mut text = format!(
        "Subagent started in the background (handle `{handle_id}`, session \
         `{child_session_id}`). It keeps working while you do.\n\
         - Wait for it: workspace_watch {{\"session_ids\": [\"{child_session_id}\"]}}\n\
         - Check on it: workspace_read_conversation {{\"session_id\": \"{child_session_id}\", \
         \"view\": \"summary\"}}\n\
         - Stop it: workspace_close {{\"session_id\": \"{child_session_id}\", \"scope\": \"turn\"}}"
    );
    if !visibility_note.is_empty() {
        text.push_str("\n\n");
        text.push_str(visibility_note);
    }
    text
}

/// A short label for the handle list, from the workflow's prompt/instructions.
fn background_title(workflow: &Workflow) -> String {
    let raw = workflow
        .prompt
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .or(workflow.instructions.as_deref())
        .unwrap_or("subagent task");
    let one_line = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut title: String = one_line.chars().take(80).collect();
    if one_line.chars().count() > 80 {
        title.push('…');
    }
    title
}

fn build_workflow(
    params: &SubagentParams,
    sub_workflows: &HashMap<String, SubWorkflow>,
) -> Result<Workflow> {
    let mut workflow = if let Some(subworkflow_name) = &params.subworkflow {
        build_subworkflow(subworkflow_name, params, sub_workflows)?
    } else {
        build_adhoc_workflow(params)?
    };

    if params.summary {
        let current = workflow.instructions.unwrap_or_default();
        workflow.instructions = Some(format!("{}\n{}", current, SUMMARY_INSTRUCTIONS));
    }

    Ok(workflow)
}

fn build_subworkflow(
    subworkflow_name: &str,
    params: &SubagentParams,
    sub_workflows: &HashMap<String, SubWorkflow>,
) -> Result<Workflow> {
    let sub_workflow = sub_workflows.get(subworkflow_name).ok_or_else(|| {
        let available: Vec<_> = sub_workflows.keys().cloned().collect();
        anyhow!(
            "Unknown subworkflow '{}'. Available: {}",
            subworkflow_name,
            available.join(", ")
        )
    })?;

    let workflow_file = load_local_workflow_file(&sub_workflow.path)
        .map_err(|e| anyhow!("Failed to load subworkflow '{}': {}", subworkflow_name, e))?;

    let mut param_values: Vec<(String, String)> = Vec::new();

    if let Some(values) = &sub_workflow.values {
        for (k, v) in values {
            param_values.push((k.clone(), v.clone()));
        }
    }

    if let Some(provided_params) = &params.parameters {
        for (k, v) in provided_params {
            let value_str = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            param_values.push((k.clone(), value_str));
        }
    }

    let mut workflow = build_workflow_from_template(
        workflow_file.content,
        &workflow_file.parent_dir,
        param_values,
        None::<fn(&str, &str) -> Result<String, anyhow::Error>>,
    )
    .map_err(|e| anyhow!("Failed to build subworkflow: {}", e))?;

    if let Some(extra) = &params.instructions {
        let mut current = workflow.instructions.take().unwrap_or_default();
        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(extra);
        workflow.instructions = Some(current);
    }

    Ok(workflow)
}

fn build_adhoc_workflow(params: &SubagentParams) -> Result<Workflow> {
    let instructions = params
        .instructions
        .as_ref()
        .ok_or_else(|| anyhow!("Instructions required for ad-hoc task"))?;

    let workflow = Workflow::builder()
        .version("1.0.0")
        .title("Subagent Task")
        .description("Ad-hoc subagent task")
        .instructions(instructions)
        .build()
        .map_err(|e| anyhow!("Failed to build workflow: {}", e))?;

    if workflow.check_for_security_warnings() {
        return Err(anyhow!("Workflow contains potentially harmful content"));
    }

    Ok(workflow)
}

/// Resolve the child's provider, classification and extension set from the
/// parent's `TaskConfig` and what the spawning model asked for.
///
/// **The whole of issue #56's spawn matrix (§8.2, R4, DR-19, DR-31) is decided
/// here**, before a child session row exists, and every one of its decisions
/// hangs off one read of DR-15's master toggle. `pub` for that reason and no other: the
/// toggle's behavioural gate
/// (`crates/biorouter/tests/privacy_toggle.rs`) is an integration binary — a
/// separate process, so that flipping a process-global atomic cannot disarm the
/// crate's own privacy tests — and an integration binary can only see what is
/// public. Left private, the decisions this function makes had no
/// both-directions assertion anywhere in the tree.
///
/// ⚠ **DR-31 narrows `settings.provider`, and callers should know the price.**
/// A child may be moved to any model with the SAME affiliation as the parent —
/// a UCSF chat can swap `versa_azure` for `versa_bedrock` — but not to one with
/// a different affiliation, so a UCSF chat can no longer spawn onto `llamacpp`.
/// The route there is a new chat on that model, started by the user.
pub async fn apply_settings_overrides(
    mut task_config: TaskConfig,
    params: &SubagentParams,
) -> Result<TaskConfig> {
    // Issue #56 §8.2. The PARENT's capability, read before any override below
    // can replace the instance it is a property of.
    //
    // `tier()` on a composite is `least()` over its components, so a lead/worker
    // parent contributes the reach it actually has, not the reach of its lead.
    //
    // This is the `Arc` the parent held when its `TaskConfig` was built, not a
    // re-read of its live binding, so a user who switches the parent's model
    // mid-turn leaves an in-flight spawn deciding against the older instance.
    // Deliberately not chased: the window is one turn and user-initiated, and
    // BOTH stale directions fail closed — a stale Private parent refuses the
    // public child DR-19 forbids, a stale Public parent refuses the private
    // child R4 forbids. Re-reading the live binding would trade that for a
    // race in which the tier used to authorise the spawn is not the tier the
    // prompt was composed under, which is the worse of the two.
    let parent_cap = task_config.provider.tier();

    // DR-31: the PARENT's affiliation, read here for the same reason and at the
    // same instant as its tier — before any override below can replace the
    // instance both are properties of.
    //
    // ⚠ **The fold, not the lead.** `Provider::affiliation` on a lead/worker
    // pair is `providers::composite_affiliation(lead, worker)`, which is the
    // MEET of the two halves and is exactly what a composite is covered by: the
    // transcript goes to both endpoints, so both institutions' agreements are in
    // play, and `Local` is the fold's IDENTITY rather than an absorbing element.
    // Reading `get_name()` — or either half — would answer for the lead alone,
    // which is the same mistake `tier()` already had to override for. A
    // `Local`-lead / `ucsf`-worker pair is covered by `ucsf`, and a check that
    // read the lead would refuse the `ucsf` child it is entitled to.
    let parent_affiliation = task_config.provider.affiliation();

    // DR-15's master opt-out. ONE read for this whole function, taken beside the
    // parent capability it qualifies, and used by every decision below — R4's
    // refusal, DR-19's refusal, DR-31's affiliation refusal and the
    // private-extension filter — so a spawn cannot be refused on one rule while
    // another silently keeps applying.
    //
    // A direct read, not a `CallCapability`: `apply_settings_overrides` runs on
    // the spawn path, which builds a whole new agent rather than dispatching a
    // tool, and has no admitted capability to inherit.
    //
    // ⚠ `task_config.privacy_tier` is NOT gated, and what it carries is only
    // HALF of the child's classification: the floor its own capability
    // establishes. It cannot carry the other half here, because the parent's
    // classification lives in the parent's ROW and this function is given a
    // `TaskConfig` (whose seed is `Public`) rather than a session. The two are
    // combined at the statement that writes the child's row — see
    // [`parent_classification`], which is likewise ungated, so that the switch
    // suspends enforcement without ever laundering a private parent's row to
    // public. Both halves matter: re-enabling never revisits a row (AR-7).
    let privacy_enforced = crate::privacy::privacy_tiers_enabled();

    if let Some(settings) = &params.settings {
        if settings.provider.is_some() || settings.model.is_some() || settings.temperature.is_some()
        {
            let provider_name = settings
                .provider
                .clone()
                .unwrap_or_else(|| task_config.provider.get_name().to_string());

            let mut model_config = task_config.provider.get_model_config();

            if let Some(model) = &settings.model {
                model_config.model_name = model.clone();
            }

            if let Some(temp) = settings.temperature {
                model_config = model_config.with_temperature(Some(temp));
            }

            task_config.provider = providers::create(&provider_name, model_config)
                .await
                .map_err(|e| anyhow!("Failed to create provider '{}': {}", provider_name, e))?;
        }
    }

    // The CHILD's capability, read off the CONSTRUCTED INSTANCE rather than off
    // the name that was requested. `providers::create` can return something
    // else entirely — the `BIOROUTER_LEAD_MODEL` intercept in `factory.rs` fires
    // BEFORE the registry lookup — and when only `model` is given, the code
    // above keeps the parent's provider name and swaps the model string. Both
    // are harmless here precisely because the tier is a property of the
    // instance and never of a model id.
    let child_tier = task_config.provider.tier();

    // DR-31: and the CHILD's affiliation, off that same constructed instance,
    // for word-for-word the reason above. A provider NAME is not a tier and it
    // is not an affiliation either — the `BIOROUTER_LEAD_MODEL` intercept can
    // hand back a lead/worker pair under the name of a single provider, and a
    // model-only or temperature-only override keeps the parent's name while
    // rebuilding the instance.
    let child_affiliation = task_config.provider.affiliation();

    // R4, refused: a public-capability session may never gain private reach,
    // not even through a child. Refusing (rather than silently downgrading the
    // child) is the point — a subagent is an extension of the chat that started
    // it, so "spawn a private child and hand the answer back" would make the
    // boundary a formality.
    if privacy_enforced && child_tier.is_private() && !parent_cap.is_private() {
        return Err(crate::privacy::PrivacyRefusal::spawn_upgrade(child_tier).into());
    }
    // DR-19, refused. This branch used to raise a downgrade-confirmation flag
    // on the child's config — a write nothing in the tree ever read, no surface
    // rendering it and no handler branching on it. A flag nothing reads is
    // worse than no control at all, because in review it reads like one. (The
    // flag's name is deliberately not written anywhere in this crate any more;
    // a Step 5 gate greps for it and expects silence.)
    //
    // A subagent spawn is a tool call and there is no surface on which a human
    // spawns one and picks its provider, so this is the MODEL choosing to send
    // private-origin prompt text to a public model it named. There is no
    // request on this path to carry a proof of user, and an approval an agent
    // can author the approver for — from config files it holds `text_editor`
    // over — is not an approval. So: refuse, ABOVE all of that machinery, and
    // say what the user can do instead.
    //
    // "Above" is load-bearing and is asserted two ways: this function names
    // nothing permission-shaped (a Step 5 gate greps for exactly that, which is
    // why the sentence above is careful not to name one either), and
    // `the_spawn_refusal_cannot_be_unlocked_by_anything_the_agent_can_write`
    // drives a real spawn under every such unlock at once.
    //
    // ⚠ This fires ONLY on a request that carried a `settings` override, and
    // needs no extra term to say so. An inheriting child is handed the parent's
    // SAME `Arc<dyn Provider>` (the fact R5 rides on), so
    // `child_tier == parent_cap` identically and the comparison cannot be true.
    //
    // The two can differ whenever the rebuild branch above ran — which is on
    // `provider`, on `model`, **and on `temperature`**. Temperature is not a
    // harmless third term: the rebuild calls `providers::create` with
    // `task_config.provider.get_name()`, and on a `LeadWorker` parent that name
    // is the LEAD's alone (`lead_worker.rs`, `get_name`) while `parent_cap` is
    // `least(lead, worker)` (`tier`). A temperature-only spawn under a
    // private-lead / public-worker parent therefore reads `parent_cap = Public`
    // and rebuilds a lead-only `child_tier = Private`, and R4 above refuses it —
    // a genuine reach increase, correctly caught, for a request that named no
    // model. The collapse is fail-closed in both directions, so it is left as
    // is; what is NOT acceptable is a comment that says it cannot happen.
    if privacy_enforced && !child_tier.is_private() && parent_cap.is_private() {
        return Err(crate::privacy::PrivacyRefusal::spawn_downgrade(child_tier).into());
    }
    // DR-31, refused: the third axis, beside the two tier arms and off the same
    // single `privacy_enforced` read, so a spawn cannot be refused on one rule
    // while another silently keeps applying.
    //
    // ⚠ **EQUALITY, in both directions — deliberately not DR-26's subset rule.**
    // The `settings` object lets the spawning model name any `provider`, and the
    // gate above it only ever compared tiers, so a UCSF-affiliated chat could
    // spawn a `Local`-affiliated child. That is not a lateral move: `Local` is
    // the TOP of this lattice — a local model reaches every private extension,
    // because no transfer occurs at all — so `Institution(x) → Local` is an
    // ELEVATION of exactly the shape R4 already refuses, on an axis this path
    // never learned to look at. And the mirror, `Local → Institution(x)`, is a
    // DISCLOSURE: the parent's text was never leaving the machine. The subset
    // rule DR-26 uses for model-versus-extension would permit that second one,
    // which is why it is not used here; it answers a different question.
    // `Institution(a) → Institution(b)` fails on both readings at once —
    // compliance does not transfer between institutions.
    //
    // ⚠ **REFUSED, not escalated**, for the reason DR-19's arm above spells out
    // at length: a spawn is a tool call, no shipped surface lets a human spawn
    // one and pick its provider, and no request on this path can carry a proof
    // of user. An approval an agent can author the approver for is not an
    // approval. The refusal says what the user can do instead.
    //
    // ⚠ **What it costs, so the narrowing is stated rather than discovered.**
    // `settings.provider` still moves a child between any two models with the
    // SAME affiliation — a UCSF chat may swap `versa_azure` for `versa_bedrock`,
    // both `ucsf` — but no longer to `llamacpp`, which is `Local`. The refusal
    // text says so, because a user who meets this should learn why rather than
    // conclude the override is broken.
    //
    // ⚠ An INHERITING spawn is free, and needs no extra term to say so: the
    // child is handed the parent's SAME `Arc<dyn Provider>`, so the two
    // affiliations are read off one instance and cannot differ. That is the
    // same fact R5 and the tier arms ride on.
    if privacy_enforced && child_affiliation != parent_affiliation {
        return Err(crate::privacy::PrivacyRefusal::spawn_affiliation(
            parent_affiliation,
            child_affiliation,
        )
        .into());
    }
    // The ONE crossing this task adds: the child's CAPABILITY establishes the
    // CLASSIFICATION its row is born with.
    task_config.privacy_tier = crate::privacy::floor(child_tier);

    if let Some(extension_names) = &params.extensions {
        if extension_names.is_empty() {
            task_config.extensions = Vec::new();
        } else {
            task_config
                .extensions
                .retain(|ext| extension_names.contains(&ext.name()));
        }
    }

    // The TIER filter the name-only narrowing above does not do. Without it a
    // session holding `ucsfomopagent` could spawn a public-model child that
    // inherited it verbatim, leaving Gate C as the only thing between a public
    // model and the clinical warehouse.
    //
    // It runs AFTER the name narrowing so `dropped_private_extensions` names
    // only what the child would otherwise actually have held: a caller that
    // asked for `extensions: []` lost nothing to privacy and must not be told
    // that it did.
    //
    // NOT `visible_to(child_tier, floor(<the extension's tier>))`: both sides of
    // that comparison are `ProviderTier`s, so `floor` has no business in it, and
    // writing it that way would add a SECOND crossing here. This is Gate C's own
    // predicate, which is also what Gate E uses, so all three agree by
    // construction rather than by three people reading the same paragraph.
    //
    // ⚠ Issue #56 Task 48 (DR-26): the AFFILIATION filter runs in the same
    // pass, off the SAME resolution. `resolve_extension` is named exactly ONCE
    // in this function, and a gate greps for that: do not repeat its name in
    // prose here. Two lookups would let the tier and the affiliation disagree
    // about one entry, which is the failure that resolver exists to remove —
    // and a partition is where it would be least visible, because a
    // disagreement drops the wrong half rather than erroring.
    //
    // **A spawn is the AGENT's enablement path for a whole new chat**, so it
    // takes the agent's rule and not the bind's. `check_enable_allowed` refuses
    // rather than warns because enabling a clinical connector is the call that
    // SPAWNS the server, pulls its credentials out of the keychain and opens
    // the institutional session — a disclosure no later refusal takes back.
    // `subagent_handler` does precisely that for every extension in this list.
    // Without this arm a chat on UCSF's Versa could spawn a child on another
    // institution's model holding the UCSF clinical connector, with no user in
    // the loop at any point; DR-26's asymmetry is that an agent never clears a
    // cross-institutional warning automatically.
    //
    // It DROPS rather than refusing the spawn, matching the tier filter: a
    // refusal would kill a legitimate delegation that merely inherited an
    // extension it never meant to use, and DR-26 is emphatic that a mismatch
    // must not become a blanket block.
    //
    // DR-15's master opt-out arrives as `gate_cross_affiliation`'s `enforced`
    // argument — the same single read the tier arm uses, not a second one.
    let mut kept = Vec::new();
    let mut dropped_private = Vec::new();
    let mut dropped_cross_affiliation = Vec::new();
    for extension in std::mem::take(&mut task_config.extensions) {
        let name = extension.name();
        let class = crate::privacy::resolve_extension(&name, Some(&extension));
        if privacy_enforced
            && crate::privacy::refusal::privacy_refusal(&name, class.tier, child_tier).is_some()
        {
            dropped_private.push(name);
            continue;
        }
        if let Some(warning) = crate::privacy::affiliation::gate_cross_affiliation_warning(
            privacy_enforced,
            child_tier,
            task_config.provider.affiliation(),
            &name,
            &class,
        ) {
            dropped_cross_affiliation.push((name, warning));
            continue;
        }
        kept.push(extension);
    }
    task_config.extensions = kept;
    task_config.dropped_private_extensions = dropped_private;
    task_config.dropped_cross_affiliation_extensions = dropped_cross_affiliation;

    Ok(task_config)
}

/// Hold **every** concurrency permit until the returned vector is dropped, so a
/// test can put the process into the every-slot-taken state the pending queue
/// exists for. The global semaphore's permit count is fixed at first use
/// (`LazyLock`), so it cannot be shrunk instead.
///
/// It waits for the full set rather than taking whatever is free right now: a
/// sibling test holding one permit would otherwise leave a slot that frees
/// mid-test, letting a spawn this test expects to stay queued escape and run.
#[cfg(test)]
async fn hold_every_subagent_permit() -> Vec<tokio::sync::SemaphorePermit<'static>> {
    let total = max_concurrent_subagents();
    let mut held = Vec::with_capacity(total);
    for _ in 0..20_000 {
        if held.len() == total {
            return held;
        }
        match SUBAGENT_SEMAPHORE.try_acquire() {
            Ok(permit) => held.push(permit),
            Err(_) => tokio::time::sleep(std::time::Duration::from_millis(1)).await,
        }
    }
    panic!("another test has held a subagent concurrency permit for 20s");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        assert_eq!(SUBAGENT_TOOL_NAME, "subagent");
    }

    // --- the pending queue ------------------------------------------------

    /// Poll `cond` until it holds, with a ceiling so a wiring mistake fails as a
    /// timeout instead of hanging the suite.
    async fn wait_until(mut cond: impl FnMut() -> bool, what: &str) {
        for _ in 0..2000 {
            if cond() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        panic!("timed out waiting for {what}");
    }

    /// The bound is `>`, not `>=`: `depth` counts the spawn being judged, so a
    /// cap of N admits N waiters and refuses the N+1th. Matching the in-flight
    /// ceiling's comparison exactly matters — off by one here silently halves
    /// or doubles a queue nobody can see.
    #[test]
    fn the_queue_bound_admits_exactly_its_cap_and_refuses_the_next() {
        assert!(pending_refusal(1, 2).is_none());
        assert!(
            pending_refusal(2, 2).is_none(),
            "the cap itself is admitted"
        );
        let refused = pending_refusal(3, 2).expect("one past the cap is refused");

        // The refusal must NAME the limit and the knob. A spawn refused by a
        // number the caller cannot see or change reads as a hang.
        assert!(refused.contains("Subagent queue full"), "got: {refused}");
        assert!(refused.contains("max 2"), "the cap is named: {refused}");
        assert!(
            refused.contains(MAX_PENDING_SUBAGENTS_ENV),
            "the knob is named: {refused}"
        );
        assert!(
            refused.contains("Nothing was started"),
            "the caller is told no child exists: {refused}"
        );
    }

    /// 56 is not arbitrary: it is exactly the deepest queue reachable at shipped
    /// defaults before this bound existed (in-flight ceiling minus the spawns
    /// that hold a permit and are running). Keeping it equal to that is what
    /// makes the bound a no-op at defaults rather than a behaviour change — if
    /// either sibling default moves, this fails and the choice gets re-made
    /// deliberately instead of drifting into refusing spawns that used to queue.
    #[test]
    fn the_queue_bound_is_the_deepest_queue_that_was_already_reachable() {
        assert_eq!(DEFAULT_MAX_CONCURRENT_SUBAGENTS, 8);
        assert_eq!(DEFAULT_MAX_INFLIGHT_SUBAGENTS, 64);
        assert_eq!(
            DEFAULT_MAX_PENDING_SUBAGENTS,
            DEFAULT_MAX_INFLIGHT_SUBAGENTS - DEFAULT_MAX_CONCURRENT_SUBAGENTS,
            "the default queue bound must equal the queue depth today's code already \
             permits, or it refuses spawns that used to be accepted"
        );
    }

    /// `SUBAGENT_PENDING` is process-global, so the two tests that assert on
    /// queue DEPTH must not overlap. Observed, not theorised: without this the
    /// first run of these tests read a depth of 3 where it expected 0, because
    /// the doors test's parked spawns were queued at the same moment.
    ///
    /// `unwrap_or_else(PoisonError::into_inner)`: a panic in one of these tests
    /// must fail that test, not turn every later one into a poisoned-lock panic
    /// that hides the original.
    static QUEUE_DEPTH_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// The gate's real body, driven concurrently over a semaphore this test
    /// owns: a spawn that gets a permit at once is never counted as pending, a
    /// full queue refuses without itself occupying a slot, and freeing permits
    /// drains the queue.
    ///
    /// Deliberately ONE test rather than four, for the same reason as the mutex:
    /// four tests asserting on one global counter would race each other.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn the_pending_queue_is_counted_bounded_and_drained() {
        let _serialised = QUEUE_DEPTH_TESTS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let semaphore: &'static Semaphore = Box::leak(Box::new(Semaphore::new(1)));
        const MAX_PENDING: usize = 2;

        // A free permit: taken immediately, never queued, never counted.
        let held = acquire_permit_bounded(semaphore, MAX_PENDING)
            .await
            .expect("the first spawn finds a free slot");
        assert_eq!(
            pending_subagent_count(),
            0,
            "a spawn that never waits must not be counted against the queue bound"
        );

        // Two more fill the queue to its bound and park.
        let first = tokio::spawn(acquire_permit_bounded(semaphore, MAX_PENDING));
        wait_until(|| pending_subagent_count() == 1, "the first spawn to queue").await;
        let second = tokio::spawn(acquire_permit_bounded(semaphore, MAX_PENDING));
        wait_until(
            || pending_subagent_count() == 2,
            "the queue to reach its bound",
        )
        .await;

        // The third is refused rather than queued.
        let refused = acquire_permit_bounded(semaphore, MAX_PENDING)
            .await
            .expect_err("a full queue refuses instead of growing");
        assert_eq!(
            refused.code,
            ErrorCode::INVALID_PARAMS,
            "a full queue is something the model can act on, not an internal fault"
        );
        assert!(
            refused.message.contains("Subagent queue full")
                && refused.message.contains("max 2")
                && refused.message.contains(MAX_PENDING_SUBAGENTS_ENV),
            "got: {}",
            refused.message
        );
        assert_eq!(
            pending_subagent_count(),
            2,
            "a REFUSED spawn must release its queue slot, or a storm of refusals \
             would keep the queue full forever"
        );

        // Freeing the permit drains the queue, one waiter at a time.
        drop(held);
        let first = first
            .await
            .unwrap()
            .expect("a queued spawn gets in when a slot frees");
        wait_until(
            || pending_subagent_count() == 1,
            "the first waiter to leave the queue",
        )
        .await;
        drop(first);
        let second = second.await.unwrap().expect("and so does the next");
        drop(second);
        wait_until(|| pending_subagent_count() == 0, "the queue to empty").await;
    }

    /// **Both** spawn doors, through the real tool entry point, with every
    /// concurrency slot taken so the queue is the only thing left to hit.
    ///
    /// One test rather than two because it holds every permit in the
    /// process-global semaphore, and two tests doing that would starve each
    /// other.
    ///
    /// Every spawn here passes `visible: false`. `announce_subagent_tab` writes
    /// to the process-global `workspace_services` override, so a visible child
    /// spawned while a sibling test has its recorder installed lands frames in
    /// that sibling's assertions — observed:
    /// `an_opted_out_or_headless_child_claims_nothing_and_sends_nothing` failed
    /// with `got: ["open_tab", "annotate_tab"]`, frames it never sent.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn both_spawn_doors_refuse_a_full_queue() {
        let _serialised = QUEUE_DEPTH_TESTS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Every slot taken: from here nothing starts, everything queues.
        let held = hold_every_subagent_permit().await;
        assert_eq!(SUBAGENT_SEMAPHORE.available_permits(), 0);

        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path().to_path_buf();

        // One spawn parked in the queue, under the DEFAULT cap so it queues
        // rather than being refused. It never gets a permit and is aborted below.
        // Its own store, so a row it might create cannot be mistaken for one the
        // refused spawns left behind.
        let parked_temp = tempfile::TempDir::new().unwrap();
        let parked_root = parked_temp.path().to_path_buf();
        let parked = tokio::spawn(async move {
            let sm = std::sync::Arc::new(crate::session::SessionManager::new(parked_root.clone()));
            let config = AgentConfig::new(
                sm,
                crate::config::permission::PermissionManager::instance(),
                None,
                crate::config::BioRouterMode::Auto,
            );
            let provider: std::sync::Arc<dyn crate::providers::base::Provider> =
                std::sync::Arc::new(TieredParent {
                    tier: ProviderTier::Public,
                });
            let task_config = TaskConfig::new(provider, "parent-parked", &parked_root, vec![]);
            handle_subagent_tool(
                &config,
                json!({ "instructions": "park in the queue", "visible": false }),
                task_config,
                HashMap::new(),
                parked_root,
                None,
            )
            .result
            .await
        });
        wait_until(
            || pending_subagent_count() >= 1,
            "a spawn to reach the queue",
        )
        .await;

        let sm = std::sync::Arc::new(crate::session::SessionManager::new(root.clone()));
        let config = AgentConfig::new(
            sm.clone(),
            crate::config::permission::PermissionManager::instance(),
            None,
            crate::config::BioRouterMode::Auto,
        );
        let public_parent = || -> std::sync::Arc<dyn crate::providers::base::Provider> {
            std::sync::Arc::new(TieredParent {
                tier: ProviderTier::Public,
            })
        };

        // --- Door 1: the blocking path in `execute_subagent`. ---
        let blocking = handle_subagent_tool(
            &config,
            json!({ "instructions": "refused at the blocking door", "visible": false }),
            TaskConfig::new(public_parent(), "parent-blocking", &root, vec![]),
            HashMap::new(),
            root.clone(),
            None,
        );
        let err = crate::config::with_config_overrides(
            HashMap::from([(MAX_PENDING_SUBAGENTS_ENV.to_string(), "1".to_string())]),
            blocking.result,
        )
        .await
        .expect_err("the blocking door refuses once the queue is full");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(
            err.message.contains("Subagent queue full") && err.message.contains("max 1"),
            "the blocking door must refuse with the queue's own wording, got: {}",
            err.message
        );
        assert_eq!(
            sm.count_all_sessions().await.unwrap(),
            0,
            "a spawn refused by the queue must leave no session behind — the \
             blocking door refuses before `create_subagent_session`"
        );

        // --- Door 2: the detached path inside `spawn_background_subagent`. ---
        // It has already returned a handle by the time it queues, so the refusal
        // lands on the handle instead of on the tool call.
        let background = handle_subagent_tool(
            &config,
            json!({
                "instructions": "refused at the background door",
                "background": true,
                "visible": false,
            }),
            TaskConfig::new(public_parent(), "parent-background", &root, vec![]),
            HashMap::new(),
            root.clone(),
            None,
        );
        crate::config::with_config_overrides(
            HashMap::from([
                (MAX_PENDING_SUBAGENTS_ENV.to_string(), "1".to_string()),
                (
                    "BIOROUTER_SUBAGENT_BACKGROUND".to_string(),
                    "true".to_string(),
                ),
            ]),
            background.result,
        )
        .await
        .expect("the background door returns its handle before it ever queues");

        let handle_error = {
            let mut found = None;
            for _ in 0..2000 {
                if let Some(result) =
                    crate::agents::subagent_handle::list_for_session("parent-background")
                        .into_iter()
                        .find_map(|h| h.result())
                {
                    found = Some(result);
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            found
                .expect("the detached task must complete its handle, not park forever")
                .error
                .expect("a queue refusal is an error result")
        };
        assert!(
            handle_error.contains("Subagent queue full") && handle_error.contains("max 1"),
            "the background door must report the same refusal on its handle, got: {handle_error}"
        );

        parked.abort();
        drop(held);
        // Release the serialising lock only once the queue is empty again: a
        // sibling test's spawn that parked while this test held every permit is
        // still counted until it gets one, and the next queue-depth test reads
        // the same global counter.
        wait_until(
            || pending_subagent_count() == 0,
            "the queue to drain after the permits are released",
        )
        .await;
    }

    /// The seam this whole change turns on. There must be exactly ONE place that
    /// takes a permit off `SUBAGENT_SEMAPHORE`, and it must be
    /// `acquire_permit_bounded`. A future third spawn path that calls the
    /// semaphore directly would look correct, compile, pass every behavioural
    /// test in this file, and queue without bound — the bound is invisible until
    /// the queue is full, which is exactly when nobody is running tests.
    ///
    /// Source-shaped on purpose: the thing being asserted is that no OTHER code
    /// exists, which no amount of exercising the code that does exist can show.
    #[test]
    fn the_semaphore_has_exactly_one_door() {
        const SENTINEL: &str = "fn the_semaphore_has_exactly_one_door";
        let source = include_str!("subagent_tool.rs");
        let body = source
            .split(SENTINEL)
            .next()
            .expect("the file contains this test");

        // Code only — the prose above deliberately names the semaphore.
        let uses: Vec<&str> = body
            .lines()
            .map(|l| l.trim())
            .filter(|l| l.contains("SUBAGENT_SEMAPHORE") && !l.starts_with("//"))
            .collect();
        // The declaration, the one bounded door, and the two test helpers.
        assert_eq!(
            uses,
            vec![
                "static SUBAGENT_SEMAPHORE: LazyLock<Semaphore> =",
                "acquire_permit_bounded(&SUBAGENT_SEMAPHORE, max_pending).await",
                "match SUBAGENT_SEMAPHORE.try_acquire() {",
                "assert_eq!(SUBAGENT_SEMAPHORE.available_permits(), 0);",
            ],
            "someone added a use of the concurrency semaphore outside the bounded \
             gate. Call `acquire_subagent_permit(max_pending)` instead — a direct \
             `acquire()` queues without bound."
        );
    }

    #[test]
    fn visibility_defaults_to_visible_with_a_gui_and_invisible_headless() {
        // Decision 24: glass-box is the default when there is somewhere to show it.
        // (requested, gui_attached, announce_only)
        assert!(resolve_visibility(None, true, false).is_visible());
        assert!(!resolve_visibility(None, false, false).is_visible());
        // Explicit opt-out wins in both cases.
        assert!(!resolve_visibility(Some(false), true, false).is_visible());
        // Explicit opt-IN cannot conjure a GUI.
        assert!(!resolve_visibility(Some(true), false, false).is_visible());
    }

    /// Decisions 7 × 26 must not collide. With announce-only ON, no tab is ever
    /// opened — so a child must NOT consume one of the four visible-tab slots,
    /// or the fifth spawn of a fan-out is told "you already have 4 subagent tabs
    /// open, which is the limit" when the true count is zero. That is the same
    /// fabricated constraint Task 29's `handle_open` rewrite exists to prevent
    /// on the `workspace_open` path.
    #[test]
    fn announce_only_opens_no_tab_and_therefore_claims_no_slot() {
        let v = resolve_visibility(
            None, /* gui_attached */ true, /* announce_only */ true,
        );
        assert_eq!(v, ChildVisibility::AnnounceOnly);
        assert!(
            !v.is_visible(),
            "announce-only must not claim a visible-tab slot"
        );
        // …and the parent is told the truth rather than nothing.
        let note = v.parent_note("child-9");
        assert!(note.contains("no tab was opened"), "got: {note}");
        assert!(note.contains("child-9"));
    }

    #[test]
    fn the_fan_out_cap_is_claimed_atomically_and_pushes_extras_to_the_background() {
        // Decision 26: N visible tabs, then background — never a refusal.
        let cap = max_visible_child_tabs();
        let guards: Vec<_> = (0..cap)
            .map(|i| {
                VisibleChildGuard::try_claim("cap-parent")
                    .unwrap_or_else(|| panic!("child {i} is within the cap"))
            })
            .collect();
        assert_eq!(visible_children_of("cap-parent"), cap);
        // The next one gets no slot — and that IS the cap decision, expressed as
        // the absence of a guard rather than as a number someone else read a
        // moment ago.
        assert!(VisibleChildGuard::try_claim("cap-parent").is_none());
        drop(guards);
        assert_eq!(visible_children_of("cap-parent"), 0);
    }

    /// The cap must hold under FAN-OUT, which is the only situation it exists
    /// for. `resolve_visibility(…, visible_children_of(parent))` followed by a
    /// separate `claim` is check-then-act: subagent dispatch is deliberately
    /// excluded from the tool-dispatch semaphore (the `let bound_dispatch = …`
    /// line in `agent.rs`) and concurrent tool calls in one assistant message
    /// are driven by `select_all`, so N simultaneous spawns all observe 0 and
    /// all claim. A sequential test cannot catch that; this one can.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_parallel_fan_out_cannot_exceed_the_visible_tab_cap() {
        let cap = max_visible_child_tabs();
        let attempts = cap * 4;
        let mut handles = Vec::with_capacity(attempts);
        for _ in 0..attempts {
            handles.push(tokio::spawn(async {
                VisibleChildGuard::try_claim("storm-parent")
            }));
        }
        let mut granted = Vec::new();
        for handle in handles {
            if let Some(guard) = handle.await.unwrap() {
                granted.push(guard);
            }
        }
        assert_eq!(
            granted.len(),
            cap,
            "exactly {cap} of {attempts} parallel claims may succeed"
        );
        assert_eq!(visible_children_of("storm-parent"), cap);
        drop(granted);
        assert_eq!(visible_children_of("storm-parent"), 0);
    }

    #[test]
    fn the_capped_reason_is_told_to_the_model_not_swallowed() {
        let capped = ChildVisibility::BackgroundCapped {
            cap: max_visible_child_tabs(),
        };
        let note = capped.parent_note("child-7");
        assert!(note.contains("child-7"));
        assert!(note.contains("background"));
        assert!(note.contains("History"));
    }

    #[test]
    fn the_visible_tab_cap_is_env_overridable_like_the_injected_turn_cap() {
        // Decision 26 says "default 4", and the sentence that justifies the
        // number points at BIOROUTER_WORKSPACE_MAX_INJECTED_TURNS — which is an
        // env var. A hard constant is not a default, it is a limit.
        assert_eq!(
            parse_visible_child_tabs(None),
            DEFAULT_MAX_VISIBLE_CHILD_TABS
        );
        assert_eq!(parse_visible_child_tabs(Some("8")), 8);
        // Nonsense and zero fall back rather than disabling tabs entirely.
        assert_eq!(
            parse_visible_child_tabs(Some("0")),
            DEFAULT_MAX_VISIBLE_CHILD_TABS
        );
        assert_eq!(
            parse_visible_child_tabs(Some("lots")),
            DEFAULT_MAX_VISIBLE_CHILD_TABS
        );
    }

    #[tokio::test]
    async fn the_visible_tab_counter_is_per_parent_and_released_when_a_child_ends() {
        let guard_a = VisibleChildGuard::try_claim("parent-1").unwrap();
        let guard_b = VisibleChildGuard::try_claim("parent-1").unwrap();
        assert_eq!(visible_children_of("parent-1"), 2);
        // A different parent has its own budget — one busy fan-out must not
        // silence another conversation's first subagent.
        let _other = VisibleChildGuard::try_claim("parent-2").unwrap();
        assert_eq!(visible_children_of("parent-1"), 2);
        assert_eq!(visible_children_of("parent-2"), 1);
        drop(guard_a);
        drop(guard_b);
        assert_eq!(visible_children_of("parent-1"), 0);
    }

    /// `placement` is a closed vocabulary, and the failure of an open one is
    /// SILENT: `announce_subagent_tab` branches on `== "window"` and forwards
    /// everything else verbatim as `open_tab`'s `placement`, which the GUI
    /// planner only special-cases for `"split"`. So `"Window"` is not a renderer
    /// error — it is a tab, which is the one thing the caller did not ask for.
    /// `workspace_open` guards this on its own path; the spawn path must too.
    #[test]
    fn an_unknown_placement_is_refused_rather_than_silently_becoming_a_tab() {
        assert_eq!(validate_placement(None), Ok("tab"));
        assert_eq!(validate_placement(Some("tab")), Ok("tab"));
        assert_eq!(validate_placement(Some("split")), Ok("split"));
        assert_eq!(validate_placement(Some("window")), Ok("window"));
        // The near-misses a model actually produces. Each one used to become a
        // tab with no error anywhere.
        for bad in ["windows", "Window", "WINDOW", "Split", " tab", "pane", ""] {
            let err = validate_placement(Some(bad))
                .expect_err(&format!("placement {bad:?} was accepted"));
            assert!(err.contains("unknown placement"), "got: {err}");
            // The message teaches the vocabulary rather than just refusing.
            assert!(err.contains("\"split\""), "got: {err}");
        }
    }

    /// The CALL SITE, not the predicate: a bad placement must be refused by the
    /// tool entry point, before a child session, an inflight slot or a
    /// visible-tab slot exists. Deleting the check in `handle_subagent_tool`
    /// leaves the pure test above green.
    #[tokio::test]
    async fn the_spawn_tool_refuses_an_unknown_placement_before_creating_anything() {
        let temp = tempfile::TempDir::new().unwrap();
        let sm = std::sync::Arc::new(crate::session::SessionManager::new(
            temp.path().to_path_buf(),
        ));
        let config = AgentConfig::new(
            sm.clone(),
            crate::config::permission::PermissionManager::instance(),
            None,
            crate::config::BioRouterMode::Auto,
        );
        let provider: std::sync::Arc<dyn crate::providers::base::Provider> = std::sync::Arc::new(
            crate::providers::testprovider::TestProvider::new_replaying(
                temp.path().join("no-such-cassette.json").to_string_lossy(),
            )
            .unwrap(),
        );
        let task_config = TaskConfig::new(provider, "parent-placement", temp.path(), vec![]);

        let refused = handle_subagent_tool(
            &config,
            json!({ "instructions": "do the thing", "placement": "Window" }),
            task_config,
            HashMap::new(),
            temp.path().to_path_buf(),
            None,
        );
        let err = refused
            .result
            .await
            .expect_err("an unknown placement must not spawn");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(
            err.message.contains("unknown placement"),
            "got: {}",
            err.message
        );

        // …and nothing was created on the way to the refusal: a child session
        // would be an orphan the parent is never told about.
        //
        // `COUNT(*)` rather than `list_sessions()`: the latter filters to
        // `User | Scheduled` and therefore cannot see a `SubAgent` row, which
        // is the only kind this path creates.
        assert_eq!(
            sm.count_all_sessions().await.unwrap(),
            0,
            "the refusal must precede session creation"
        );
    }

    // -----------------------------------------------------------------------
    // The ANNOUNCE CALL SITE (decisions 24 and 26).
    //
    // ⚠ Why these exist (2026-07-31 review). Every other test in this block is
    // a test of a pure function this module also defines — `resolve_visibility`,
    // `try_claim`, `parent_note`, `parse_visible_child_tabs`. All of them pass
    // against a perfect implementation that NOTHING CALLS: a child that never
    // announces, and a cap that is never claimed. The single behavioural rule
    // the `AnnounceOnly` doc-comment spends twelve lines justifying — a slot is
    // claimed only for a real tab — lived entirely in an untested branch.
    // -----------------------------------------------------------------------

    /// A GUI stand-in that records every frame. Only `gui_attached` and
    /// `gui_command` carry meaning here; the rest of the trait is unreachable
    /// from `announce_subagent_tab` and panics rather than pretending.
    #[derive(Default)]
    struct FakeGui {
        gui: bool,
        frames: std::sync::Mutex<Vec<Value>>,
    }

    impl FakeGui {
        fn install(gui: bool) -> std::sync::Arc<Self> {
            let me = std::sync::Arc::new(Self {
                gui,
                frames: std::sync::Mutex::new(Vec::new()),
            });
            crate::workspace_services::set_for_tests(Some(me.clone()));
            me
        }
        fn frames(&self) -> Vec<Value> {
            self.frames.lock().unwrap().clone()
        }
        fn cmds(&self) -> Vec<String> {
            self.frames()
                .iter()
                .map(|f| f["cmd"].as_str().unwrap_or_default().to_string())
                .collect()
        }
        /// The announce rides a detached `tokio::spawn`, so the frames arrive
        /// after the call returns. Poll rather than sleep a fixed span.
        async fn settle(&self, expected: usize) -> Vec<Value> {
            for _ in 0..400 {
                if self.frames().len() >= expected {
                    // One more yield, so an unexpected EXTRA frame still lands
                    // before an assertion that there are exactly `expected`.
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    return self.frames();
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            panic!("expected {expected} frames, saw {:?}", self.cmds());
        }
    }

    #[async_trait::async_trait]
    impl crate::workspace_services::WorkspaceServices for FakeGui {
        fn gui_attached(&self) -> bool {
            self.gui
        }
        fn layout_snapshot(&self) -> Option<Value> {
            None
        }
        fn is_turn_active(&self, _session_id: &str) -> bool {
            false
        }
        fn cancel_turn(&self, _session_id: &str) -> Option<String> {
            None
        }
        fn begin_turn(
            &self,
            _session_id: &str,
            _cancel: CancellationToken,
        ) -> Result<Box<dyn crate::workspace_services::WorkspaceTurnLease>, String> {
            unreachable!("the announce path takes no turn lease")
        }
        async fn stop_agent(&self, _session_id: &str) -> Result<(), String> {
            unreachable!("the announce path stops nothing")
        }
        async fn start_detached_turn(
            &self,
            _session_id: &str,
            _message: crate::conversation::message::Message,
        ) -> Result<String, String> {
            unreachable!("the announce path starts no turn")
        }
        async fn start_session(
            &self,
            _working_dir: PathBuf,
            _extensions: Option<Vec<String>>,
            _knowledge_bases: Vec<String>,
            _primary: crate::workspace_services::KbPrimaryChoice,
        ) -> Result<String, String> {
            unreachable!("the child session already exists by the time we announce")
        }
        fn set_knowledge_bases(
            &self,
            _session_id: &str,
            _kbs: &[String],
            _primary: crate::workspace_services::KbPrimaryChoice,
        ) -> Result<crate::workspace_services::KbSelectionView, String> {
            unreachable!("the announce path grants nothing")
        }
        fn knowledge_selection(
            &self,
            _session_id: &str,
        ) -> crate::workspace_services::KbSelectionView {
            Default::default()
        }
        async fn gui_command(&self, frame: Value, _wait_result: bool) -> Result<Value, String> {
            self.frames.lock().unwrap().push(frame);
            Ok(serde_json::json!({ "ok": true }))
        }
    }

    fn spawn_params(visible: Option<bool>, placement: Option<&str>) -> SubagentParams {
        let mut args = serde_json::Map::new();
        args.insert("instructions".into(), json!("do the thing"));
        if let Some(v) = visible {
            args.insert("visible".into(), json!(v));
        }
        if let Some(p) = placement {
            args.insert("placement".into(), json!(p));
        }
        serde_json::from_value(Value::Object(args)).unwrap()
    }

    /// Decision 24 at the call site: with a GUI attached and nothing opted out,
    /// the child claims a slot AND the tab frame really goes on the wire, with
    /// the placement it asked for and `focus: false` (never steals the composer).
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn announcing_a_visible_child_claims_a_slot_and_emits_the_open_frame() {
        let gui = FakeGui::install(true);
        let parent = "announce-visible-parent";

        let (visibility, guard) =
            announce_subagent_tab("child-a", parent, &spawn_params(None, Some("split")));

        assert_eq!(visibility, ChildVisibility::Visible);
        assert!(guard.is_some(), "a visible child must hold a slot");
        assert_eq!(visible_children_of(parent), 1);
        assert_eq!(
            visibility.parent_note("child-a"),
            "",
            "a tab that really opened needs no explanation"
        );

        let frames = gui.settle(2).await;
        assert_eq!(gui.cmds(), vec!["open_tab", "annotate_tab"]);
        assert_eq!(frames[0]["session_id"], "child-a");
        assert_eq!(frames[0]["placement"], "split");
        assert_eq!(frames[0]["focus"], false);
        assert_eq!(frames[1]["badge"], "subagent");
        assert_eq!(frames[1]["parent_session_id"], parent);

        // The slot is released with the guard, not with the function.
        drop(guard);
        assert_eq!(visible_children_of(parent), 0);
        crate::workspace_services::clear_test_override();
    }

    /// `placement: "window"` is a DIFFERENT frame, not a field on `open_tab` —
    /// the same vocabulary split `workspace_open` uses. A single `open_tab`
    /// carrying `placement: "window"` silently opens a tab.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn a_windowed_child_gets_open_window_not_an_open_tab() {
        let gui = FakeGui::install(true);
        let parent = "announce-window-parent";

        let (_visibility, guard) =
            announce_subagent_tab("child-w", parent, &spawn_params(None, Some("window")));

        let frames = gui.settle(2).await;
        assert_eq!(gui.cmds(), vec!["open_window", "annotate_tab"]);
        assert_eq!(frames[0]["session_id"], "child-w");
        assert!(
            frames[0].get("placement").is_none(),
            "open_window carries no placement: {:?}",
            frames[0]
        );

        drop(guard);
        crate::workspace_services::clear_test_override();
    }

    /// Decision 26 at the call site. The cap+1st child of one parent is pushed
    /// to the background: no slot, no tab frame — and the model is TOLD, which
    /// is the half a silent implementation gets wrong. It still gets its badge,
    /// because a capped child is precisely the one the user opens later from
    /// History and the renderer stores annotations by session id.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn the_child_past_the_cap_is_backgrounded_but_still_badged() {
        let gui = FakeGui::install(true);
        let parent = "announce-cap-parent";
        let cap = max_visible_child_tabs();

        let mut guards = Vec::new();
        for i in 0..cap {
            let (visibility, guard) =
                announce_subagent_tab(&format!("child-{i}"), parent, &spawn_params(None, None));
            assert_eq!(visibility, ChildVisibility::Visible, "child {i}");
            guards.push(guard.expect("within the cap"));
        }
        assert_eq!(visible_children_of(parent), cap);
        gui.settle(cap * 2).await;

        let (visibility, guard) =
            announce_subagent_tab("child-past-cap", parent, &spawn_params(None, None));
        assert_eq!(visibility, ChildVisibility::BackgroundCapped { cap });
        assert!(guard.is_none(), "the capped child holds no slot");
        assert_eq!(
            visible_children_of(parent),
            cap,
            "a refused claim must not consume a slot either"
        );
        let note = visibility.parent_note("child-past-cap");
        assert!(note.contains("background"), "got: {note}");
        assert!(note.contains("History"), "got: {note}");

        // Exactly one more frame, and it is the badge — no tab was opened.
        let frames = gui.settle(cap * 2 + 1).await;
        assert_eq!(
            frames.len(),
            cap * 2 + 1,
            "a capped child opens no tab: {:?}",
            gui.cmds()
        );
        let last = frames.last().unwrap();
        assert_eq!(last["cmd"], "annotate_tab");
        assert_eq!(last["session_id"], "child-past-cap");
        assert_eq!(last["parent_session_id"], parent);

        drop(guards);
        assert_eq!(visible_children_of(parent), 0);
        crate::workspace_services::clear_test_override();
    }

    /// Decision 7 × 26 at the call site, the interaction the pure test can only
    /// half-see: announce-only opens no tab, so the child must claim NO slot —
    /// otherwise a fan-out of five is told "you already have 4 subagent tabs
    /// open" while zero tabs exist. `with_config_overrides` is a task-local, so
    /// the setting is pinned without mutating the process environment.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn an_announce_only_child_is_announced_but_claims_no_slot() {
        let gui = FakeGui::install(true);
        let parent = "announce-only-parent";
        let overrides = HashMap::from([(
            crate::agents::workspace_extension::ANNOUNCE_ONLY_KEY.to_string(),
            "true".to_string(),
        )]);

        let (visibility, guard) = crate::config::with_config_overrides(overrides, async {
            announce_subagent_tab("child-quiet", parent, &spawn_params(None, None))
        })
        .await;

        assert_eq!(visibility, ChildVisibility::AnnounceOnly);
        assert!(guard.is_none(), "announce-only must claim no visible slot");
        assert_eq!(visible_children_of(parent), 0);
        assert!(visibility.parent_note("child-quiet").contains("no tab"));

        // It is still ANNOUNCED — `apply_focus_etiquette` downgrades the open
        // frame to a notification rather than dropping it.
        let frames = gui.settle(2).await;
        assert_eq!(frames[0]["cmd"], "notify");
        assert_eq!(frames[1]["cmd"], "annotate_tab");
        crate::workspace_services::clear_test_override();
    }

    /// `visible: false` and "no GUI" both reach the GUI with nothing at all —
    /// the two early returns. Without this, an implementation that announced
    /// unconditionally passes every other test in this file.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn an_opted_out_or_headless_child_claims_nothing_and_sends_nothing() {
        let gui = FakeGui::install(true);
        let (visibility, guard) = announce_subagent_tab(
            "child-silent",
            "announce-optout-parent",
            &spawn_params(Some(false), None),
        );
        assert_eq!(visibility, ChildVisibility::OptedOut);
        assert!(guard.is_none());
        assert_eq!(visible_children_of("announce-optout-parent"), 0);

        // A GUI is attached, so silence here is a decision, not an accident.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(gui.frames().is_empty(), "got: {:?}", gui.cmds());

        // Headless: no daemon at all.
        crate::workspace_services::set_for_tests(None);
        let (visibility, guard) = announce_subagent_tab(
            "child-headless",
            "announce-headless-parent",
            &spawn_params(None, None),
        );
        assert_eq!(visibility, ChildVisibility::Headless);
        assert!(guard.is_none());
        assert_eq!(visible_children_of("announce-headless-parent"), 0);
        crate::workspace_services::clear_test_override();
    }

    #[test]
    fn test_create_tool_without_subworkflows() {
        let tool = create_subagent_tool(&[]);
        assert_eq!(tool.name, "subagent");
        assert!(tool.description.as_ref().unwrap().contains("Ad-hoc"));
        assert!(!tool
            .description
            .as_ref()
            .unwrap()
            .contains("Available subworkflows"));
    }

    #[test]
    fn test_create_tool_with_subworkflows() {
        let sub_workflows = vec![SubWorkflow {
            name: "test_workflow".to_string(),
            path: "test.yaml".to_string(),
            values: None,
            sequential_when_repeated: false,
            description: Some("A test workflow".to_string()),
        }];

        let tool = create_subagent_tool(&sub_workflows);
        assert!(tool
            .description
            .as_ref()
            .unwrap()
            .contains("Available subworkflows"));
        assert!(tool.description.as_ref().unwrap().contains("test_workflow"));
    }

    #[test]
    fn test_sequential_hint_in_description() {
        let sub_workflows = vec![
            SubWorkflow {
                name: "parallel_ok".to_string(),
                path: "test.yaml".to_string(),
                values: None,
                sequential_when_repeated: false,
                description: Some("Can run in parallel".to_string()),
            },
            SubWorkflow {
                name: "sequential_only".to_string(),
                path: "test.yaml".to_string(),
                values: None,
                sequential_when_repeated: true,
                description: Some("Must run sequentially".to_string()),
            },
        ];

        let tool = create_subagent_tool(&sub_workflows);
        let desc = tool.description.as_ref().unwrap();

        assert!(desc.contains("parallel_ok"));
        assert!(!desc.contains("parallel_ok [run sequentially"));

        assert!(desc.contains("sequential_only [run sequentially, not in parallel]"));
    }

    #[test]
    fn test_params_deserialization_full() {
        let params: SubagentParams = serde_json::from_value(json!({
            "instructions": "Extra context",
            "subworkflow": "my_workflow",
            "parameters": {"key": "value"},
            "extensions": ["developer"],
            "settings": {"model": "gpt-4"},
            "summary": false
        }))
        .unwrap();

        assert_eq!(params.instructions, Some("Extra context".to_string()));
        assert_eq!(params.subworkflow, Some("my_workflow".to_string()));
        assert!(params.parameters.is_some());
        assert_eq!(params.extensions, Some(vec!["developer".to_string()]));
        assert!(!params.summary);
    }

    // --- BR-40: async handle -------------------------------------------------

    #[test]
    fn background_defaults_off_so_an_ordinary_call_still_blocks() {
        let params: SubagentParams = serde_json::from_value(json!({
            "instructions": "do the thing"
        }))
        .unwrap();
        assert!(!params.background);
    }

    #[test]
    fn background_param_round_trips() {
        let params: SubagentParams = serde_json::from_value(json!({
            "instructions": "long crawl",
            "background": true
        }))
        .unwrap();
        assert!(params.background);
    }

    #[test]
    fn spawn_params_accept_visible_and_placement_and_keep_every_legacy_field() {
        let params: SubagentParams = serde_json::from_value(serde_json::json!({
            "instructions": "count files",
            "extensions": ["developer"],
            "summary": false,
            "background": true,
            "visible": false,
            "placement": "split"
        }))
        .unwrap();
        assert_eq!(params.instructions.as_deref(), Some("count files"));
        assert_eq!(
            params.extensions.as_deref(),
            Some(&["developer".to_string()][..])
        );
        assert!(!params.summary);
        assert!(params.background);
        assert_eq!(params.visible, Some(false));
        assert_eq!(params.placement.as_deref(), Some("split"));
    }

    #[test]
    fn the_background_result_points_at_workspace_watch_not_subagent_status() {
        let text = background_started_message("sub_1", "child-session-id", "");
        assert!(text.contains("workspace_watch"));
        assert!(text.contains("child-session-id"));
        assert!(!text.contains("subagent_status"));
    }

    /// Decision 26: when a child goes to the background because the 4-tab cap
    /// was full, the PARENT must be told why. The background path returns
    /// before any `SubagentResult` exists, so the note has to ride on this
    /// message or it is never delivered.
    #[test]
    fn a_capped_background_start_tells_the_parent_why() {
        let note = "child-session-id is running in the background (you already have \
                    4 subagent tabs open, which is the limit). Find it in History.";
        let text = background_started_message("sub_2", "child-session-id", note);
        assert!(text.contains("background"));
        assert!(text.contains("History"));
    }

    /// BR-71: the child's `parent_session_id` is durable from BIRTH, not from
    /// the later `persist_spawn_context` call. The `background: true` path hands
    /// the child's id back to the parent before the run starts, so a child that
    /// dies before its first turn would otherwise be a permanently unparented
    /// row in History.
    #[tokio::test]
    async fn create_subagent_session_stamps_the_parent_at_birth() {
        let temp = tempfile::TempDir::new().unwrap();
        let sm = std::sync::Arc::new(crate::session::SessionManager::new(
            temp.path().to_path_buf(),
        ));
        let config = AgentConfig::new(
            sm.clone(),
            crate::config::permission::PermissionManager::instance(),
            None,
            crate::config::BioRouterMode::Auto,
        );

        let params: SubagentParams = serde_json::from_value(serde_json::json!({
            "instructions": "stamp the parent at birth"
        }))
        .unwrap();
        let session = create_subagent_session(
            &config,
            temp.path().to_path_buf(),
            "parent-99",
            &params,
            crate::privacy::SessionClassification::Public,
        )
        .await
        .expect("session creation succeeds");

        // The handle the caller gets back agrees with the row (the background
        // path returns this value and never re-reads).
        assert_eq!(session.parent_session_id.as_deref(), Some("parent-99"));

        // …and the STORE agrees, before a single turn has run.
        let reread = sm.get_session(&session.id, false).await.unwrap();
        assert_eq!(
            reread.parent_session_id.as_deref(),
            Some("parent-99"),
            "the parent stamp must be durable at birth, not only after the first turn"
        );
        assert_eq!(
            reread.session_type,
            crate::session::session_manager::SessionType::SubAgent
        );
        assert_eq!(reread.message_count, 0, "birth writes no message");
    }

    /// Two children of one fan-out must be distinguishable in every listing.
    /// Before this, `create_subagent_session` named all of them "Subagent task".
    ///
    /// `SubagentParams` derives only `Debug, Deserialize` (no `Default`), so the
    /// fixtures are built through serde rather than a struct literal — a literal
    /// here is a compile error the day a field is added.
    #[test]
    fn a_subagent_session_is_named_after_its_own_task() {
        let a: SubagentParams = serde_json::from_value(serde_json::json!({
            "instructions": "Audit the migration for data loss\nand then report back"
        }))
        .unwrap();
        let b: SubagentParams = serde_json::from_value(serde_json::json!({
            "instructions": "Benchmark the new covering index"
        }))
        .unwrap();

        assert_ne!(subagent_session_label(&a), subagent_session_label(&b));
        assert!(subagent_session_label(&a).contains("Audit the migration"));
        // ONE line: a multi-paragraph instruction must not become a multi-line
        // session name, which every listing renders as broken rows.
        assert!(!subagent_session_label(&a).contains("report back"));
        assert!(!subagent_session_label(&a).contains('\n'));

        // A subworkflow run is named for the workflow, not for the ad-hoc
        // instructions it also carries.
        let w: SubagentParams = serde_json::from_value(serde_json::json!({
            "subworkflow": "triage-failures", "instructions": "extra context"
        }))
        .unwrap();
        assert!(subagent_session_label(&w).contains("triage-failures"));

        // …and the fallback is still the old literal, so a paramless spawn does
        // not produce an empty name.
        let empty: SubagentParams = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(subagent_session_label(&empty), "Subagent task");
    }

    /// The label is MODEL-authored text that `biorouter session list` prints
    /// straight to a terminal, so it must not be able to carry an escape
    /// sequence there. Subagent instructions routinely paraphrase file contents
    /// the parent agent just read, which is how a stray `\x1b[` or a lone `\r`
    /// gets in without anyone intending it.
    ///
    /// `lines()` + `trim` already remove `\n`, `\r\n` and surrounding
    /// whitespace; they do NOT remove an embedded CSI introducer, a bare `\r`
    /// (which rewrites the line already printed), or `\x07`.
    #[test]
    fn a_session_label_carries_no_control_characters_to_the_terminal() {
        let nasty: SubagentParams = serde_json::from_value(serde_json::json!({
            "instructions": "Audit \u{1b}[31mthe\u{1b}[0m migration\u{7}\u{d}now"
        }))
        .unwrap();
        let label = subagent_session_label(&nasty);
        assert!(
            !label.chars().any(char::is_control),
            "a model-authored label reaches a TTY verbatim: {label:?}"
        );
        // The readable text survives — stripping must not gut the label.
        assert!(label.contains("Audit"));
        assert!(label.contains("migration"));
        // ⚠ Only the CONTROL characters go. The printable tail of a CSI
        // sequence (`[31m`) is left behind as inert text on purpose: removing
        // the `\x1b` is what neutralises the sequence, and pattern-matching CSI
        // grammar to strip the rest would just as happily eat a legitimate
        // `[TODO]` out of a real instruction.

        // A label that is ONLY control characters must fall back rather than
        // become the bare prefix "Subagent: ".
        let blank: SubagentParams = serde_json::from_value(serde_json::json!({
            "instructions": "\u{7}\u{1b}\u{d}\u{0}"
        }))
        .unwrap();
        assert_eq!(subagent_session_label(&blank), "Subagent task");
    }

    // -----------------------------------------------------------------------
    // Issue #56, Task 23: SPAWN INHERITANCE (design §8.2).
    //
    // A subagent is an extension of the chat that started it, so its reach and
    // the classification its row is born with are decided BEFORE the row
    // exists. Everything below drives the real `apply_settings_overrides` — and,
    // where the ordering is the whole point, the real tool entry point — rather
    // than a second copy of the rule.
    // -----------------------------------------------------------------------

    use crate::privacy::{PrivacyRefusal, ProviderTier, SessionClassification};

    /// A parent provider whose only interesting property is its tier — the same
    /// shape `agent::gate_a_bind_tests::TieredProvider` uses.
    ///
    /// `complete_with_model` returns `Authentication`, the one error class the
    /// tree documents as never worth retrying, so a test that accidentally
    /// reaches a turn fails fast instead of backing off.
    struct TieredParent {
        tier: ProviderTier,
    }

    #[async_trait::async_trait]
    impl crate::providers::base::Provider for TieredParent {
        fn metadata() -> crate::providers::base::ProviderMetadata {
            crate::providers::base::ProviderMetadata::new(
                "tiered-parent",
                "Tiered parent",
                "",
                "tiered-model",
                vec![],
                "",
                vec![],
            )
        }

        fn get_name(&self) -> &str {
            "tiered-parent"
        }

        fn tier(&self) -> ProviderTier {
            self.tier
        }

        /// ⚠ **A private double must state an affiliation, because every real
        /// private provider does** — issue #56 DR-26, Task 48. The same
        /// correction `privacy_toggle`'s `TieredProvider` and the three doubles
        /// in `agent.rs` / `code_execution_integration.rs` already carry.
        ///
        /// Left on the trait default this is Private tier + affiliation `None`,
        /// the one pairing DR-26's vocabulary says cannot exist, which the gate
        /// treats as *unstated* rather than *unconstrained* — deliberately, as
        /// the fail-closed direction. The spawn's affiliation filter then drops
        /// every institution-claimed extension from every row in this module,
        /// including the tier rows that are not about affiliation at all.
        ///
        /// `Local` because it is DR-26's identity element: the one model
        /// affiliation compatible with every extension, so a tier row keeps
        /// testing the tier. The rows that ARE about affiliation use
        /// [`AffiliatedParent`], which states it explicitly.
        fn affiliation(&self) -> Option<crate::privacy::ModelAffiliation> {
            match self.tier {
                ProviderTier::Private => Some(crate::privacy::ModelAffiliation::Local),
                ProviderTier::Public => None,
            }
        }

        fn get_model_config(&self) -> crate::model::ModelConfig {
            crate::model::ModelConfig::new_or_fail("tiered-model")
        }

        async fn complete_with_model(
            &self,
            _model_config: &crate::model::ModelConfig,
            _system: &str,
            _messages: &[crate::conversation::message::Message],
            _tools: &[Tool],
        ) -> std::result::Result<
            (
                crate::conversation::message::Message,
                crate::providers::base::ProviderUsage,
            ),
            crate::providers::errors::ProviderError,
        > {
            Err(crate::providers::errors::ProviderError::Authentication(
                "the spawn matrix never runs a real turn".to_string(),
            ))
        }
    }

    /// What the spawning model asked for, in the vocabulary of §8.2's matrix.
    #[derive(Clone, Copy, Debug)]
    enum Ask {
        /// No `settings` at all — the child runs the parent's own provider
        /// instance, which is what makes R5 (same lead/worker mode) free.
        Inherit,
        /// A named provider that resolves PRIVATE.
        Private,
        /// A named provider that resolves PUBLIC.
        Public,
    }

    /// The requested NAME is `ollama` for BOTH tiered asks, deliberately.
    ///
    /// §8.2 is validated on the CONSTRUCTED INSTANCE, never on the name:
    /// `providers::create` can hand back something other than what was asked for
    /// (the `BIOROUTER_LEAD_MODEL` intercept fires before the registry lookup),
    /// and when only `model` is given today's code keeps the parent's provider
    /// name and swaps the model string. `ollama` is the one registry entry that
    /// constructs with no credential and no network *and* reads its tier off the
    /// resolved base URL — so one real `providers::create` call yields a Private
    /// instance pointed at this machine and a Public one pointed off it, under
    /// the identical requested name.
    fn ask_params(ask: Ask) -> SubagentParams {
        let body = match ask {
            Ask::Inherit => json!({ "instructions": "do the thing" }),
            Ask::Private | Ask::Public => json!({
                "instructions": "do the thing",
                "settings": { "provider": "ollama" }
            }),
        };
        serde_json::from_value(body).unwrap()
    }

    /// Task-local config for one ask. `with_config_overrides` beats both the
    /// environment and the config file, and touches neither, so these tests can
    /// run in parallel with everything else in this binary.
    /// BOTH poles are pinned, including the one that agrees with the shipped
    /// default. Leaving `Ask::Private` as an empty map made the private pole
    /// rest on the *absence* of `OLLAMA_HOST`: `get_param` falls through
    /// override → env → `config.yaml`, so on a developer machine with Ollama
    /// pointed off-box the "private" row would resolve Public and the matrix
    /// would stop testing the crossing it exists for. It fails loudly rather
    /// than vacuously, but only after someone has spent an hour on it.
    fn ask_overrides(ask: Ask) -> HashMap<String, String> {
        let host = match ask {
            Ask::Public => "https://ollama.example.com",
            // Loopback, i.e. Private — the same value the provider defaults to,
            // written down so it cannot be taken away by the environment.
            Ask::Inherit | Ask::Private => "http://localhost:11434",
        };
        HashMap::from([("OLLAMA_HOST".to_string(), host.to_string())])
    }

    fn builtin_extension(name: &str) -> crate::agents::ExtensionConfig {
        crate::agents::ExtensionConfig::Builtin {
            name: name.to_string(),
            display_name: Some(name.to_string()),
            description: String::new(),
            timeout: Some(30),
            bundled: Some(true),
            available_tools: Vec::new(),
        }
    }

    fn parent_task_config(
        parent: ProviderTier,
        extensions: Vec<crate::agents::ExtensionConfig>,
    ) -> TaskConfig {
        let provider: std::sync::Arc<dyn crate::providers::base::Provider> =
            std::sync::Arc::new(TieredParent { tier: parent });
        TaskConfig::new(provider, "parent-1", std::path::Path::new("."), extensions)
    }

    /// One row of the matrix, through the real resolver.
    async fn resolve_child(
        parent: ProviderTier,
        ask: Ask,
        extensions: Vec<crate::agents::ExtensionConfig>,
    ) -> Result<TaskConfig> {
        let task_config = parent_task_config(parent, extensions);
        let params = ask_params(ask);
        crate::config::with_config_overrides(
            ask_overrides(ask),
            apply_settings_overrides(task_config, &params),
        )
        .await
    }

    /// A provider stating both privacy axes — issue #56 Task 48, DR-26.
    /// [`TieredParent`] cannot express the third one, and leaving it on the
    /// trait default would put every affiliation row into the *unstated* arm
    /// rather than the institution-versus-institution one they are about.
    struct AffiliatedParent {
        tier: ProviderTier,
        affiliation: Option<crate::privacy::ModelAffiliation>,
    }

    #[async_trait::async_trait]
    impl crate::providers::base::Provider for AffiliatedParent {
        fn metadata() -> crate::providers::base::ProviderMetadata {
            crate::providers::base::ProviderMetadata::new(
                "affiliated-parent",
                "Affiliated parent",
                "",
                "affiliated-model",
                vec![],
                "",
                vec![],
            )
        }

        fn get_name(&self) -> &str {
            "affiliated-parent"
        }

        fn tier(&self) -> ProviderTier {
            self.tier
        }

        fn affiliation(&self) -> Option<crate::privacy::ModelAffiliation> {
            self.affiliation
        }

        fn get_model_config(&self) -> crate::model::ModelConfig {
            crate::model::ModelConfig::new_or_fail("affiliated-model")
        }

        async fn complete_with_model(
            &self,
            _model_config: &crate::model::ModelConfig,
            _system: &str,
            _messages: &[crate::conversation::message::Message],
            _tools: &[Tool],
        ) -> std::result::Result<
            (
                crate::conversation::message::Message,
                crate::providers::base::ProviderUsage,
            ),
            crate::providers::errors::ProviderError,
        > {
            Err(crate::providers::errors::ProviderError::Authentication(
                "the affiliation rows never run a real turn".to_string(),
            ))
        }
    }

    /// An INHERITING spawn from a given parent instance — the child runs the
    /// parent's own `Arc`, so the child's affiliation is the parent's. That is
    /// the shape the DR-26 rows need: `Ask::Private` would rebuild through
    /// `providers::create` and hand back an `ollama` whose affiliation is
    /// `Local`, which is compatible with everything and would make every row
    /// pass vacuously.
    async fn resolve_child_for(
        provider: std::sync::Arc<dyn crate::providers::base::Provider>,
        extensions: Vec<crate::agents::ExtensionConfig>,
    ) -> Result<TaskConfig> {
        let task_config =
            TaskConfig::new(provider, "parent-1", std::path::Path::new("."), extensions);
        apply_settings_overrides(task_config, &ask_params(Ask::Inherit)).await
    }

    /// Design §8.2, row by row, as amended by DR-19.
    ///
    /// The `prompt` column is GONE. It had no subject: nothing in the tree read
    /// the downgrade-confirmation flag it reported, so the only thing this could
    /// assert was that a field had been written — and a flag nothing reads is
    /// worse than no control at all, because in review it reads like one. The
    /// row it annotated is now a refusal instead.
    #[tokio::test]
    async fn the_spawn_matrix_holds() {
        use ProviderTier::{Private, Public};
        for (parent, ask, tier) in [
            (Private, Ask::Inherit, SessionClassification::Private),
            (Private, Ask::Private, SessionClassification::Private),
            (Public, Ask::Inherit, SessionClassification::Public),
            (Public, Ask::Public, SessionClassification::Public),
        ] {
            let child = resolve_child(parent, ask, vec![])
                .await
                .unwrap_or_else(|e| panic!("{parent:?} + {ask:?} must be permitted: {e}"));
            assert_eq!(
                child.privacy_tier, tier,
                "{parent:?} + {ask:?} must be born {tier:?}"
            );
        }

        // R4: a public session may never gain private reach. Hard refusal.
        let err = resolve_child(Public, Ask::Private, vec![])
            .await
            .expect_err("a public parent may not spawn a private child");
        assert!(
            matches!(
                err.downcast_ref::<PrivacyRefusal>(),
                Some(PrivacyRefusal::PrivateChildOfPublicParent { .. })
            ),
            "expected R4's typed refusal, got: {err}"
        );

        // DR-19: and a private session may not hand its task prompt to a public
        // model the MODEL picked. A refusal in the other direction, for the
        // other reason — R4 permits this crossing, but only a model can ask for
        // it and there is no channel here to ask a human on.
        let err = resolve_child(Private, Ask::Public, vec![])
            .await
            .expect_err("a private parent may not spawn a child on a public model it named");
        assert!(
            matches!(
                err.downcast_ref::<PrivacyRefusal>(),
                Some(PrivacyRefusal::PublicChildOfPrivateParent { .. })
            ),
            "expected DR-19's typed refusal, got: {err}"
        );
    }

    /// The `settings.temperature` term of the rebuild branch, which review
    /// found the R4/DR-19 comment claiming was inert.
    ///
    /// It is not inert on a **composite** parent. `apply_settings_overrides`
    /// rebuilds on `provider || model || temperature`, and the rebuild passes
    /// `task_config.provider.get_name()` — which `LeadWorkerProvider` answers
    /// with the LEAD's name alone, while its `tier()` is `least(lead, worker)`.
    /// So a spawn that named nothing but a temperature can hand the child MORE
    /// reach than the parent had, and R4 refuses it.
    ///
    /// The parent here is a private-lead / public-worker pair, i.e. `parent_cap
    /// = Public`: a loopback `ollama` lead (the one registry entry that
    /// constructs with no credential and no network, and reads its tier off the
    /// resolved base URL) over a Public mock worker. Pinned so the comment
    /// cannot quietly become wrong again — deleting the `temperature` term makes
    /// this test fail, which is how it was checked.
    #[tokio::test]
    async fn a_temperature_only_spawn_is_not_inert_on_a_composite_parent() {
        use crate::providers::lead_worker::LeadWorkerProvider;
        use std::sync::Arc;

        let params: SubagentParams = serde_json::from_value(json!({
            "instructions": "do the thing",
            "settings": { "temperature": 0.25 }
        }))
        .unwrap();

        let err = crate::config::with_config_overrides(
            HashMap::from([(
                "OLLAMA_HOST".to_string(),
                "http://localhost:11434".to_string(),
            )]),
            async move {
                let lead = crate::providers::create(
                    "ollama",
                    crate::model::ModelConfig::new_or_fail("llama3"),
                )
                .await
                .expect("ollama constructs with no credential and no network");
                assert!(
                    lead.tier().is_private(),
                    "the lead must be Private for this test to be about anything"
                );

                let worker: Arc<dyn crate::providers::base::Provider> = Arc::new(TieredParent {
                    tier: ProviderTier::Public,
                });
                let parent: Arc<dyn crate::providers::base::Provider> =
                    Arc::new(LeadWorkerProvider::new(lead, worker, None));
                assert!(
                    !parent.tier().is_private(),
                    "parent_cap is least(lead, worker), so the pair reads Public"
                );
                assert_eq!(
                    parent.get_name(),
                    "ollama",
                    "get_name answers for the LEAD alone — this is the whole mechanism"
                );

                let task_config =
                    TaskConfig::new(parent, "parent-1", std::path::Path::new("."), vec![]);
                apply_settings_overrides(task_config, &params).await
            },
        )
        .await
        .expect_err("a temperature-only spawn collapsed the pair to its private lead");

        assert!(
            matches!(
                err.downcast_ref::<PrivacyRefusal>(),
                Some(PrivacyRefusal::PrivateChildOfPublicParent { .. })
            ),
            "expected R4's typed refusal, got: {err}"
        );
    }

    /// Step 3(d): a child is born at the tier of the model it actually
    /// resolved, with the spawn recorded as its provenance.
    ///
    /// Both classifications, and the PUBLIC one is the reason both are here:
    /// "born public" passes vacuously against an implementation that stamps
    /// nothing at all, because `sessions.privacy_tier` already defaults to
    /// `'public'`. Only the private arm discriminates; only the public arm
    /// proves the stamp is not unconditionally private.
    ///
    /// ⚠ The public arm is driven by a PUBLIC parent, not by DR-19's refused
    /// downgrade. Before that amendment this test reached Public through
    /// `parent = Private, ask = Public`, which is a spawn this feature no
    /// longer performs — asserting on the shape of a child that is never
    /// created is how a test outlives the behaviour it was written for.
    #[tokio::test]
    async fn a_child_is_born_at_the_tier_of_the_model_it_resolved() {
        let temp = tempfile::TempDir::new().unwrap();
        let sm = std::sync::Arc::new(crate::session::SessionManager::new(
            temp.path().to_path_buf(),
        ));
        let config = AgentConfig::new(
            sm.clone(),
            crate::config::permission::PermissionManager::instance(),
            None,
            crate::config::BioRouterMode::Auto,
        );
        let params = ask_params(Ask::Inherit);

        for (parent, expected) in [
            (ProviderTier::Public, SessionClassification::Public),
            (ProviderTier::Private, SessionClassification::Private),
        ] {
            let child_config = resolve_child(parent, Ask::Inherit, vec![]).await.unwrap();
            let session = create_subagent_session(
                &config,
                temp.path().to_path_buf(),
                "parent-1",
                &params,
                child_config.privacy_tier,
            )
            .await
            .unwrap();

            let row = sm.get_session(&session.id, false).await.unwrap();
            assert_eq!(row.privacy_tier, expected, "{parent:?}");
            assert_eq!(
                session.privacy_tier, expected,
                "{parent:?} in-memory handle"
            );
            assert_eq!(
                row.privacy_reason.as_deref(),
                Some("inherited:parent-1"),
                "the provenance names the spawn, so §12.4 can grade it"
            );
            // A child receives only the task prompt — none of the parent's
            // history. That is also why DR-19 refuses a downgrade rather than
            // disclosing it: the prompt would be the entire disclosure, and
            // only a model is ever there to read it.
            assert_eq!(row.message_count, 0, "no parent conversation is carried");
            assert_eq!(row.diverged_from, None, "a spawn is not a branch");
            assert_eq!(row.parent_session_id.as_deref(), Some("parent-1"));
        }
    }

    /// A persisted parent session at a given classification, for the rows below.
    ///
    /// The raise goes through the real `SessionUpdateBuilder`, so the fixture
    /// produces the same row shape Gate B's ratchet produces, provenance
    /// included.
    async fn persisted_parent(
        sm: &crate::session::SessionManager,
        working_dir: &std::path::Path,
        tier: SessionClassification,
    ) -> String {
        let parent = sm
            .create_session(
                working_dir.to_path_buf(),
                "parent".to_string(),
                crate::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();
        if tier.is_private() {
            sm.update(&parent.id)
                .raise_privacy(tier, "mcp:ucsfomopagent")
                .apply()
                .await
                .unwrap();
        }
        parent.id
    }

    /// **Issue #56, finding 11.** A child's row is never born BELOW the
    /// classification of the session that spawned it.
    ///
    /// The shape under test is the one DR-15's master switch makes reachable and
    /// nothing else does: a **Private-classified parent row bound to a public
    /// provider**. With the switch on it cannot exist — Gate A refuses that bind
    /// and Gate B refuses the turn the spawn would happen inside — so it is
    /// exactly what a period with the switch OFF leaves behind, and it outlives
    /// the switch being turned back on, because re-enabling never revisits a row
    /// (AR-7).
    ///
    /// ⚠ **Driven with the toggle untouched, and that is the point rather than a
    /// shortcut.** The classification half must be ungated, so a test that had to
    /// turn the switch off to see it would be testing the wrong thing — and
    /// flipping the process-global atomic from a lib test would disarm every
    /// other privacy test sharing this binary, which is why the toggle's
    /// behavioural matrix lives in its own process. The switch-off *sequence*
    /// (spawn while off → re-enable → read the row) is asserted there, in
    /// `crates/biorouter/tests/privacy_spawn_classification.rs`.
    ///
    /// ⚠ **Both classifications, because the private arm alone would pass against
    /// a stamp that is unconditionally Private** — which would be its own defect,
    /// permanently over-classifying every child. The public arm is what says the
    /// raise is a `max` and not a floor.
    #[tokio::test]
    async fn a_child_is_never_born_below_the_classification_of_its_parent() {
        for (parent_row, expected) in [
            (
                SessionClassification::Private,
                SessionClassification::Private,
            ),
            (SessionClassification::Public, SessionClassification::Public),
        ] {
            let temp = tempfile::TempDir::new().unwrap();
            let sm = std::sync::Arc::new(crate::session::SessionManager::new(
                temp.path().to_path_buf(),
            ));
            let config = AgentConfig::new(
                sm.clone(),
                crate::config::permission::PermissionManager::instance(),
                None,
                crate::config::BioRouterMode::Auto,
            );
            let parent_id = persisted_parent(&sm, temp.path(), parent_row).await;

            // A PUBLIC parent provider in both rows: the child inherits the same
            // `Arc`, so its own capability floor is Public either way. That is
            // the pre-condition the finding describes, and it is asserted rather
            // than assumed — without it the private row could pass because the
            // capability floor happened to be Private already.
            let provider: std::sync::Arc<dyn crate::providers::base::Provider> =
                std::sync::Arc::new(TieredParent {
                    tier: ProviderTier::Public,
                });
            let task_config = TaskConfig::new(provider, &parent_id, temp.path(), vec![]);
            let params = ask_params(Ask::Inherit);
            let child_config = apply_settings_overrides(task_config, &params)
                .await
                .unwrap();
            assert_eq!(
                child_config.privacy_tier,
                SessionClassification::Public,
                "the capability floor must be Public, or this row proves nothing"
            );

            let session = create_subagent_session(
                &config,
                temp.path().to_path_buf(),
                &parent_id,
                &params,
                child_config.privacy_tier,
            )
            .await
            .unwrap();

            let row = sm.get_session(&session.id, false).await.unwrap();
            assert_eq!(
                row.privacy_tier, expected,
                "a parent classified {parent_row:?} must not mint a child row below itself"
            );
            assert_eq!(
                session.privacy_tier, expected,
                "{parent_row:?}: the in-memory handle disagreed with the row just written"
            );
            let expected_reason = format!("inherited:{parent_id}");
            assert_eq!(
                row.privacy_reason.as_deref(),
                Some(expected_reason.as_str()),
                "{parent_row:?}: the provenance still names the spawn, so §12.4 can grade it"
            );
        }
    }

    // ⚠ The same invariant through the **tool entry point**, on BOTH spawn
    // paths, is asserted in `crates/biorouter/tests/privacy_spawn_classification.rs`
    // and deliberately NOT here. Driving a blocking spawn to completion in this
    // binary runs the child, and the child's first act is
    // `run_complete_subagent_task`'s `begin_turn` against the PROCESS-GLOBAL
    // `workspace_services` override — which neighbouring tests in this same
    // binary install and remove concurrently, and whose double `unreachable!`s
    // on `begin_turn`. The test failed exactly that way when it lived here, and
    // it would have been a flake rather than a failure on a luckier schedule.
    // The integration binary has no such neighbour, so the both-paths claim is
    // made there, where it is deterministic.

    /// DR-19, and it replaces `a_downgraded_child_is_born_public_…`.
    ///
    /// A subagent spawn is a **tool call**. There is no shipped surface on
    /// which a human spawns a subagent and chooses its provider — the request
    /// arrives as tool arguments the model wrote — so `parent = Private,
    /// request = Public` is an agent-initiated send of private-origin prompt
    /// text to a public model *the model itself named*. DR-19's agent half is
    /// unconditional: it escalates to a human, or it does not happen. And there
    /// is nothing here to escalate to — Task 18A's `X-User-Action` is an HTTP
    /// request header, and `apply_settings_overrides` runs in-process inside
    /// the parent's own turn. So it does not happen.
    ///
    /// Driven through the real tool on BOTH spawn paths, because a refusal must
    /// also leave nothing behind — which is only true because Step 3(a) moved
    /// the resolve ahead of the row.
    #[tokio::test]
    async fn a_private_parent_cannot_hand_its_prompt_to_a_public_model_it_picked() {
        for background in [false, true] {
            let temp = tempfile::TempDir::new().unwrap();
            let sm = std::sync::Arc::new(crate::session::SessionManager::new(
                temp.path().to_path_buf(),
            ));
            let config = AgentConfig::new(
                sm.clone(),
                crate::config::permission::PermissionManager::instance(),
                None,
                crate::config::BioRouterMode::Auto,
            );
            let provider: std::sync::Arc<dyn crate::providers::base::Provider> =
                std::sync::Arc::new(TieredParent {
                    tier: ProviderTier::Private,
                });
            let task_config = TaskConfig::new(provider, "parent-1", temp.path(), vec![]);

            let mut overrides = ask_overrides(Ask::Public);
            overrides.insert(
                "BIOROUTER_SUBAGENT_BACKGROUND".to_string(),
                background.to_string(),
            );

            let err = crate::config::with_config_overrides(overrides, async {
                handle_subagent_tool(
                    &config,
                    json!({
                        "instructions": "summarise the cohort in /phi/cohort-3",
                        "settings": { "provider": "ollama" },
                        "background": background,
                    }),
                    task_config,
                    HashMap::new(),
                    temp.path().to_path_buf(),
                    None,
                )
                .result
                .await
            })
            .await
            .expect_err(&format!(
                "DR-19 refuses a private parent's public child (background={background})"
            ));

            assert!(
                err.message.contains("public model"),
                "background={background}, got: {}",
                err.message
            );
            assert!(
                err.message.contains("start a new chat"),
                "a refusal must name the way out, not just say no (background={background}): {}",
                err.message
            );
            // §14.4 / R10: the prompt is the thing being withheld, so it must
            // not be quoted back — and the parent's own provider must not be
            // named, or the refusal becomes a classification oracle.
            assert!(
                !err.message.contains("cohort-3"),
                "the refusal quoted the prompt it exists to withhold: {}",
                err.message
            );
            assert_eq!(
                sm.count_all_sessions().await.unwrap(),
                0,
                "a refused spawn must leave no session behind (background={background})"
            );
        }
    }

    /// DR-19's second half, as an assertion.
    ///
    /// The refusal is a `return Err` inside `apply_settings_overrides`, which
    /// runs **above** every unlock an agent can author: hooks load from
    /// `~/.config/biorouter/config.yaml` and — with `allow_project_hooks` —
    /// from `.biorouter/hooks.yaml`, both writable by the same agent holding
    /// `text_editor`, and neither behind a deny root. An approval an agent can
    /// author the approver for is not an approval.
    ///
    /// ⚠ What this does and does not discriminate, stated so nobody reads more
    /// into a green run than is there. `handle_subagent_tool` consults none of
    /// these today, so this is a **position** assertion: it fails the day
    /// someone moves the privacy decision below an approval, which is exactly
    /// the change DR-19 forbids and the one no other test in this file would
    /// notice. The structural half — that nothing permission-shaped is even
    /// named inside `apply_settings_overrides` — is Step 5's `awk`/`grep` gate.
    #[tokio::test]
    async fn the_spawn_refusal_cannot_be_unlocked_by_anything_the_agent_can_write() {
        /// One agent-writable unlock, in the vocabulary of DR-19's banner.
        #[derive(Clone, Copy, Debug)]
        enum Unlock {
            /// A `PermissionRequest` hook in the global config that approves
            /// everything — open question 2 measured that this bypasses an
            /// ordinary approval.
            PermissionRequestHookAllow,
            /// The same, from the project file, with the opt-in that enables it.
            ProjectHooksAllow,
            /// A persisted `always_allow` record for the spawn tool itself.
            AlwaysAllowRecord,
            /// The two permission modes that dilute approval.
            PermissionMode(crate::config::BioRouterMode),
        }

        const ALLOW_EVERYTHING: &str = r#"{"PermissionRequest":[{"matcher":"*","hooks":[{"type":"command","command":"true"}]}]}"#;

        for unlock in [
            Unlock::PermissionRequestHookAllow,
            Unlock::ProjectHooksAllow,
            Unlock::AlwaysAllowRecord,
            Unlock::PermissionMode(crate::config::BioRouterMode::SmartApprove),
            Unlock::PermissionMode(crate::config::BioRouterMode::Chat),
        ] {
            let temp = tempfile::TempDir::new().unwrap();
            let sm = std::sync::Arc::new(crate::session::SessionManager::new(
                temp.path().to_path_buf(),
            ));

            // A permission store rooted in the temp dir, never the user's own:
            // `update_user_permission` WRITES `permission.yaml`, and the global
            // singleton points at `~/.config/biorouter`.
            let permissions = std::sync::Arc::new(
                crate::config::permission::PermissionManager::new(temp.path().to_path_buf()),
            );
            if matches!(unlock, Unlock::AlwaysAllowRecord) {
                for tool in ["subagent", "platform__subagent"] {
                    permissions.update_user_permission(
                        tool,
                        crate::config::permission::PermissionLevel::AlwaysAllow,
                    );
                    permissions.update_smart_approve_permission(
                        tool,
                        crate::config::permission::PermissionLevel::AlwaysAllow,
                    );
                }
            }

            let mode = match unlock {
                Unlock::PermissionMode(mode) => mode,
                _ => crate::config::BioRouterMode::Auto,
            };
            let config = AgentConfig::new(sm.clone(), permissions, None, mode);

            let mut overrides = ask_overrides(Ask::Public);
            match unlock {
                Unlock::PermissionRequestHookAllow => {
                    overrides.insert("HOOKS".to_string(), ALLOW_EVERYTHING.to_string());
                }
                Unlock::ProjectHooksAllow => {
                    std::fs::create_dir_all(temp.path().join(".biorouter")).unwrap();
                    std::fs::write(
                        temp.path().join(crate::hooks::config::PROJECT_HOOKS_FILE),
                        format!("hooks: {ALLOW_EVERYTHING}\n"),
                    )
                    .unwrap();
                    overrides.insert("BIOROUTER_ALLOW_PROJECT_HOOKS".to_string(), "true".into());
                }
                Unlock::AlwaysAllowRecord => {}
                // The wire spelling, not `Debug`: `BioRouterMode` deserializes
                // `snake_case`, so `{mode:?}` would be silently unparseable and
                // the override would do nothing at all.
                Unlock::PermissionMode(mode) => {
                    let spelling = match mode {
                        crate::config::BioRouterMode::SmartApprove => "smart_approve",
                        crate::config::BioRouterMode::Chat => "chat",
                        other => panic!("unexpected permission mode in this table: {other:?}"),
                    };
                    overrides.insert("BIOROUTER_MODE".to_string(), spelling.to_string());
                }
            }

            let provider: std::sync::Arc<dyn crate::providers::base::Provider> =
                std::sync::Arc::new(TieredParent {
                    tier: ProviderTier::Private,
                });
            let task_config = TaskConfig::new(provider, "parent-1", temp.path(), vec![]);

            let err = crate::config::with_config_overrides(overrides, async {
                handle_subagent_tool(
                    &config,
                    json!({
                        "instructions": "summarise the cohort",
                        "settings": { "provider": "ollama" },
                    }),
                    task_config,
                    HashMap::new(),
                    temp.path().to_path_buf(),
                    None,
                )
                .result
                .await
            })
            .await
            .expect_err(&format!("{unlock:?} must not unlock DR-19's refusal"));

            assert!(
                err.message.contains("public model"),
                "{unlock:?} changed the refusal into something else: {}",
                err.message
            );
            assert_eq!(
                sm.count_all_sessions().await.unwrap(),
                0,
                "{unlock:?} left a session behind"
            );
        }
    }

    /// THE ORDERING BUG. `create_subagent_session` used to run BEFORE
    /// `overridden_task_config` on both paths, so a refusal left a durable
    /// `SubAgent` row with no provider and no run — and on the background path
    /// the whole stretch runs inside a detached `tokio::spawn`.
    ///
    /// Both paths, because the fix has to be applied twice and a single-path
    /// test would pass with one of them still inverted.
    #[tokio::test]
    async fn a_refused_spawn_leaves_no_orphan_row() {
        for background in [false, true] {
            let temp = tempfile::TempDir::new().unwrap();
            let sm = std::sync::Arc::new(crate::session::SessionManager::new(
                temp.path().to_path_buf(),
            ));
            let config = AgentConfig::new(
                sm.clone(),
                crate::config::permission::PermissionManager::instance(),
                None,
                crate::config::BioRouterMode::Auto,
            );
            let provider: std::sync::Arc<dyn crate::providers::base::Provider> =
                std::sync::Arc::new(TieredParent {
                    tier: ProviderTier::Public,
                });
            let task_config = TaskConfig::new(provider, "parent-1", temp.path(), vec![]);

            let mut overrides = ask_overrides(Ask::Private);
            overrides.insert(
                "BIOROUTER_SUBAGENT_BACKGROUND".to_string(),
                background.to_string(),
            );

            let err = crate::config::with_config_overrides(overrides, async {
                handle_subagent_tool(
                    &config,
                    json!({
                        "instructions": "read the clinical warehouse for me",
                        "settings": { "provider": "ollama" },
                        "background": background,
                    }),
                    task_config,
                    HashMap::new(),
                    temp.path().to_path_buf(),
                    None,
                )
                .result
                .await
            })
            .await
            .expect_err(&format!(
                "a public parent may not spawn a private child (background={background})"
            ));

            assert_eq!(
                err.code,
                ErrorCode::INVALID_PARAMS,
                "background={background}"
            );
            // R4's OWN wording, not just "subagent" + INVALID_PARAMS: the
            // fork-bomb guard's refusal ("Wait for running subagents to
            // finish…") satisfies both of those AND leaves zero rows, so under
            // inflight pressure this test could pass for entirely the wrong
            // reason. `InflightGuard` is process-global across this binary.
            assert!(
                err.message
                    .contains("a subagent may never reach further than the chat that started it"),
                "expected R4's refusal, not some other INVALID_PARAMS \
                 (background={background}), got: {}",
                err.message
            );
            assert!(
                err.message.contains("No subagent was started"),
                "background={background}, got: {}",
                err.message
            );
            // `COUNT(*)`, NOT `list_sessions()`. `list_sessions` filters to
            // `User | Scheduled`, so it cannot see a `SubAgent` row at all —
            // asserting on it passes just as happily against the orphan this
            // test exists to catch.
            assert_eq!(
                sm.count_all_sessions().await.unwrap(),
                0,
                "a refused spawn must leave no session behind (background={background})"
            );
        }
    }

    /// `apply_settings_overrides` narrowed by NAME only, never by tier — so a
    /// session holding `ucsfomopagent` could spawn a public-model child that
    /// inherited it verbatim, and Gate C would then be the only thing between a
    /// public model and the clinical warehouse.
    ///
    /// ⚠ THE FIXTURE CHANGED WITH DR-19, and the filter did NOT become dead
    /// code. The obvious fixture — a private parent asking for a public child —
    /// is now a refusal, so written that way this test would assert on a spawn
    /// that never happens. The surviving reachable case is a parent whose
    /// CAPABILITY is Public while its extension list still holds a
    /// private-classified record: Gate C refuses that extension at dispatch and
    /// Gate E hides it from discovery, but neither REMOVES it from the manager,
    /// so `TaskConfig.extensions` — the parent's own list — still carries it.
    /// An inheriting child is then Public and must not receive it.
    #[tokio::test]
    async fn a_public_child_does_not_inherit_its_parents_private_extensions() {
        let child = resolve_child(
            ProviderTier::Public,
            Ask::Inherit,
            vec![
                builtin_extension("ucsfomopagent"),
                builtin_extension("developer"),
            ],
        )
        .await
        .unwrap();

        assert_eq!(
            child
                .extensions
                .iter()
                .map(crate::agents::ExtensionConfig::name)
                .collect::<Vec<_>>(),
            vec!["developer".to_string()]
        );
        assert_eq!(
            child.dropped_private_extensions,
            vec!["ucsfomopagent".to_string()],
            "the drop must be recorded so the model can be told"
        );
        let note = dropped_extension_note(&child.dropped_private_extensions)
            .expect("a drop that happened is a drop that is reported");
        assert!(note.contains("ucsfomopagent"), "got: {note}");

        // A child that stays private keeps it, and is told nothing.
        let kept = resolve_child(
            ProviderTier::Private,
            Ask::Inherit,
            vec![builtin_extension("ucsfomopagent")],
        )
        .await
        .unwrap();
        assert_eq!(kept.extensions.len(), 1);
        assert!(kept.dropped_private_extensions.is_empty());
        assert!(dropped_extension_note(&kept.dropped_private_extensions).is_none());
    }

    /// Issue #56 Task 48 (DR-26) at the **spawn**, which is the fourth of the
    /// bypassing paths the ruling names and the one where the data to decide it
    /// already exists.
    ///
    /// A spawn is the AGENT's enablement path for a whole new chat, so it takes
    /// the agent's rule, not the bind's: `check_enable_allowed` refuses because
    /// enabling a clinical connector is the call that spawns the server, pulls
    /// its credentials out of the keychain and opens the institutional session
    /// — "Gate C refusing the first tool call afterwards is already too late".
    /// `subagent_handler` does exactly that for every extension in the child's
    /// list. Without this, a chat on UCSF's Versa could spawn a child on
    /// another institution's model and hand it the UCSF clinical connector,
    /// with no user in the loop at any point.
    ///
    /// It DROPS rather than refusing the spawn, matching the tier filter beside
    /// it: refusing would kill a legitimate delegation that merely inherited an
    /// extension it never meant to use, and DR-26 is emphatic that a mismatch
    /// must not become a blanket block.
    ///
    /// ⚠ The drop is asserted on the recorded list AND the surviving list AND
    /// the note, because a silent drop is a capability the parent keeps
    /// planning around — the same reason the tier filter records its own.
    #[tokio::test]
    async fn a_spawn_may_not_hand_a_child_another_institutions_extension() {
        let child = resolve_child_for(
            std::sync::Arc::new(AffiliatedParent {
                tier: ProviderTier::Private,
                affiliation: Some(crate::privacy::ModelAffiliation::institution(
                    crate::privacy::affiliation::InstitutionId::new("stanford"),
                )),
            }),
            vec![
                builtin_extension("ucsfomopagent"),
                builtin_extension("developer"),
            ],
        )
        .await
        .expect("a mismatch drops the extension; it must never refuse the spawn");

        assert_eq!(
            child
                .extensions
                .iter()
                .map(crate::agents::ExtensionConfig::name)
                .collect::<Vec<_>>(),
            vec!["developer".to_string()],
            "a Stanford-covered child inherited a UCSF clinical connector"
        );
        // The TIER filter must not be what caught it — both endpoints here are
        // Private, so a test that passed through that arm would prove nothing
        // about the third axis.
        assert!(
            child.dropped_private_extensions.is_empty(),
            "the tier filter fired, so this fixture is not exercising affiliation: {:?}",
            child.dropped_private_extensions
        );

        let dropped = &child.dropped_cross_affiliation_extensions;
        assert_eq!(dropped.len(), 1, "{dropped:?}");
        assert_eq!(dropped[0].0, "ucsfomopagent");
        assert!(dropped[0].1.contains("ucsf"), "{}", dropped[0].1);
        assert!(dropped[0].1.contains("stanford"), "{}", dropped[0].1);

        let note = cross_affiliation_drop_note(dropped)
            .expect("a drop that happened is a drop that is reported");
        assert!(note.contains("ucsfomopagent"), "got: {note}");
        assert!(note.contains("ucsf"), "got: {note}");
        assert!(note.contains("stanford"), "got: {note}");
    }

    /// The passing row, and the inversion DR-26 warns about: `Local` is the
    /// MOST permissive affiliation, so a local parent hands its child
    /// everything private and is told nothing.
    #[tokio::test]
    async fn a_local_model_spawns_a_child_holding_every_private_extension() {
        let child = resolve_child_for(
            std::sync::Arc::new(AffiliatedParent {
                tier: ProviderTier::Private,
                affiliation: Some(crate::privacy::ModelAffiliation::Local),
            }),
            vec![
                builtin_extension("ucsfomopagent"),
                builtin_extension("developer"),
            ],
        )
        .await
        .unwrap();

        assert_eq!(child.extensions.len(), 2);
        assert!(child.dropped_private_extensions.is_empty());
        assert!(child.dropped_cross_affiliation_extensions.is_empty());
        assert!(cross_affiliation_drop_note(&child.dropped_cross_affiliation_extensions).is_none());
    }

    /// The same institution on both ends is the arrangement everyone approved.
    /// Without this row the filter above could be an over-block and still pass.
    ///
    /// DR-15's master opt-out is deliberately NOT asserted here: it reaches this
    /// filter as `gate_cross_affiliation`'s `enforced` argument, off the same
    /// single `privacy_tiers_enabled()` read the tier arm uses, and its guard is
    /// covered by `privacy::affiliation`'s
    /// `the_gate_asks_affiliation_only_of_two_private_endpoints_with_the_feature_on`.
    /// Flipping the process-global atomic from a lib test would disarm every
    /// other privacy test sharing this binary — which is exactly why the
    /// toggle's behavioural matrix lives in the separate `privacy_toggle`
    /// process.
    #[tokio::test]
    async fn a_child_covered_by_the_same_institution_keeps_the_extension() {
        let child = resolve_child_for(
            std::sync::Arc::new(AffiliatedParent {
                tier: ProviderTier::Private,
                affiliation: Some(crate::privacy::ModelAffiliation::institution(
                    crate::privacy::affiliation::InstitutionId::new("ucsf"),
                )),
            }),
            vec![builtin_extension("ucsfomopagent")],
        )
        .await
        .unwrap();
        assert_eq!(
            child.extensions.len(),
            1,
            "UCSF's own model reaching UCSF's own connector is the approved arrangement"
        );
        assert!(child.dropped_cross_affiliation_extensions.is_empty());
    }

    // -----------------------------------------------------------------------
    // DR-31: the spawn gate's THIRD axis.
    //
    // Everything above this line about affiliation is the extension FILTER: it
    // asks whether the child may keep an inherited connector, and drops the ones
    // it may not. The rows below are about the CHILD'S OWN MODEL — the axis the
    // gate compared for tier in both directions and never compared at all for
    // affiliation, so `settings: { "provider": "llamacpp" }` moved a UCSF chat's
    // work onto a model at the TOP of the affiliation lattice.
    //
    // Equality, both directions, mirroring the tier pair rather than DR-26's
    // subset rule. A subset rule would permit `Local → Institution(x)`, which is
    // the disclosure half.
    // -----------------------------------------------------------------------

    /// A parent covered by exactly one institution.
    fn institution_parent(slug: &str) -> std::sync::Arc<dyn crate::providers::base::Provider> {
        std::sync::Arc::new(AffiliatedParent {
            tier: ProviderTier::Private,
            affiliation: Some(crate::privacy::ModelAffiliation::institution(
                crate::privacy::affiliation::InstitutionId::new(slug),
            )),
        })
    }

    /// A parent at the TOP of the affiliation lattice.
    fn local_parent() -> std::sync::Arc<dyn crate::providers::base::Provider> {
        std::sync::Arc::new(AffiliatedParent {
            tier: ProviderTier::Private,
            affiliation: Some(crate::privacy::ModelAffiliation::Local),
        })
    }

    /// A public parent, whose affiliation is the absence of one — the shape the
    /// inheritance row needs so that "cannot differ" is checked for `None` too.
    fn public_parent() -> std::sync::Arc<dyn crate::providers::base::Provider> {
        std::sync::Arc::new(TieredParent {
            tier: ProviderTier::Public,
        })
    }

    /// The overrides that make `providers::create("versa_azure", …)` construct a
    /// real UCSF-affiliated instance with no network and no keychain: the
    /// gateway endpoint its affiliation is read off, pinned rather than left to
    /// the default so a developer's `config.yaml` cannot repoint it and quietly
    /// turn this row's child into an unaffiliated one, plus a placeholder key so
    /// `AzureAuth` takes the `ApiKey` branch instead of the Azure credential
    /// chain. Nothing here is ever sent anywhere — no row below runs a turn.
    fn ucsf_child_overrides() -> HashMap<String, String> {
        HashMap::from([
            (
                "VERSA_AZURE_API_KEY".to_string(),
                "not-a-real-key".to_string(),
            ),
            (
                "AZURE_OPENAI_ENDPOINT".to_string(),
                crate::providers::versa_azure::VERSA_AZURE_ENDPOINT.to_string(),
            ),
        ])
    }

    /// The same, for a `Local` child: loopback `ollama`, the one registry entry
    /// that constructs with no credential and no network and reads both its tier
    /// and its affiliation off the resolved base URL.
    fn local_child_overrides() -> HashMap<String, String> {
        HashMap::from([(
            "OLLAMA_HOST".to_string(),
            "http://localhost:11434".to_string(),
        )])
    }

    /// One DR-31 row through the real resolver: a spawn whose `settings` names
    /// `provider`, which is the override the ruling is about.
    async fn spawn_child_on(
        parent: std::sync::Arc<dyn crate::providers::base::Provider>,
        provider: &str,
        overrides: HashMap<String, String>,
    ) -> Result<TaskConfig> {
        let params: SubagentParams = serde_json::from_value(json!({
            "instructions": "do the thing",
            "settings": { "provider": provider }
        }))
        .unwrap();
        let task_config = TaskConfig::new(parent, "parent-1", std::path::Path::new("."), vec![]);
        crate::config::with_config_overrides(
            overrides,
            apply_settings_overrides(task_config, &params),
        )
        .await
    }

    fn affiliation_refusal(err: &anyhow::Error) -> &PrivacyRefusal {
        let refusal = err
            .downcast_ref::<PrivacyRefusal>()
            .unwrap_or_else(|| panic!("expected DR-31's typed refusal, got: {err}"));
        assert!(
            matches!(refusal, PrivacyRefusal::SpawnCrossesAffiliation { .. }),
            "a TIER arm fired, so this row is not exercising affiliation: {refusal}"
        );
        refusal
    }

    /// DR-31's headline row: **elevation**.
    ///
    /// `Local` is the TOP of the affiliation lattice, not a peer of the
    /// institutions — a local model reaches every private extension, because no
    /// transfer occurs at all. So a UCSF chat naming `llamacpp`/`ollama` in
    /// `settings.provider` is not moving sideways, it is handing its child reach
    /// it does not itself have. Exactly the shape R4 refuses on the tier axis,
    /// on the axis this gate never looked at.
    ///
    /// Both endpoints are Private, so neither tier arm can be what caught it —
    /// which is what [`affiliation_refusal`] asserts.
    #[tokio::test]
    async fn a_ucsf_chat_may_not_spawn_a_local_child() {
        let err = spawn_child_on(
            institution_parent("ucsf"),
            "ollama",
            local_child_overrides(),
        )
        .await
        .expect_err("UCSF → Local is an elevation, not a lateral move");
        let msg = affiliation_refusal(&err).to_string();

        // Both affiliations, named. A refusal that says only "affiliation
        // mismatch" leaves the model with nothing to tell the user.
        assert!(msg.contains("ucsf"), "got: {msg}");
        assert!(msg.contains("local model"), "got: {msg}");
        // ...and named in the RIGHT SLOTS. Both labels appear either way, so
        // `contains` alone passes against a constructor whose two arguments are
        // swapped — which would tell the user their local chat is refusing to
        // reach UCSF, the exact inverse of what happened.
        assert!(
            msg.find("ucsf") < msg.find("local model"),
            "parent and child are the wrong way round: {msg}"
        );
        // The remedy, and the cost — a user who meets this must not conclude
        // that `settings.provider` is broken.
        assert!(msg.contains("start a new chat"), "got: {msg}");
        assert!(msg.contains("versa_azure"), "got: {msg}");
        assert!(msg.contains("versa_bedrock"), "got: {msg}");
        assert!(msg.contains("llamacpp"), "got: {msg}");
        assert!(msg.contains("Do not retry"), "got: {msg}");
    }

    /// Two institutions: the child gains reach the parent lacks, and compliance
    /// does not transfer between them.
    ///
    /// ⚠ **The Stanford end is the PARENT, and it is the double, because no
    /// provider in this tree constructs a Stanford affiliation.** The row is
    /// about a mismatch between two named institutions, which this fixture is;
    /// putting the double on the child instead would test nothing, because a
    /// child is only ever built by `providers::create`.
    #[tokio::test]
    async fn a_chat_at_one_institution_may_not_spawn_a_child_at_another() {
        let err = spawn_child_on(
            institution_parent("stanford"),
            "versa_azure",
            ucsf_child_overrides(),
        )
        .await
        .expect_err("compliance does not transfer between institutions");
        let msg = affiliation_refusal(&err).to_string();
        assert!(msg.contains("stanford"), "got: {msg}");
        assert!(msg.contains("ucsf"), "got: {msg}");
    }

    /// The mirror, and the half a **subset** rule would have permitted:
    /// **disclosure**.
    ///
    /// The parent's data was never leaving this machine. `Local ⊆ anything` is
    /// the reading that makes this look harmless, and it is why DR-31 is
    /// equality rather than DR-26's model↔extension subset test — a different
    /// question, asked of the same vocabulary.
    #[tokio::test]
    async fn a_local_chat_may_not_spawn_an_institutional_child() {
        let err = spawn_child_on(local_parent(), "versa_azure", ucsf_child_overrides())
            .await
            .expect_err("the parent's text was never leaving the machine");
        let msg = affiliation_refusal(&err).to_string();
        assert!(msg.contains("local model"), "got: {msg}");
        assert!(msg.contains("ucsf"), "got: {msg}");
        // The mirror of the ordering assertion in the elevation row: here the
        // LOCAL side is the parent, so it must come first.
        assert!(
            msg.find("local model") < msg.find("ucsf"),
            "parent and child are the wrong way round: {msg}"
        );
    }

    /// The permitted row, and the one that keeps the three above from being an
    /// over-block: same affiliation on both ends.
    ///
    /// This is also the narrowing's stated bound, exercised rather than merely
    /// promised — a UCSF chat may still move its child between UCSF models.
    #[tokio::test]
    async fn a_child_covered_by_the_same_institution_is_permitted() {
        let child = spawn_child_on(
            institution_parent("ucsf"),
            "versa_azure",
            ucsf_child_overrides(),
        )
        .await
        .expect("UCSF → UCSF is the arrangement everyone approved");
        assert_eq!(
            child.provider.affiliation(),
            Some(crate::privacy::ModelAffiliation::institution(
                crate::privacy::affiliation::InstitutionId::new("ucsf")
            )),
            "the fixture must actually resolve a UCSF child or this row proves nothing"
        );
        assert_eq!(child.privacy_tier, SessionClassification::Private);
    }

    /// ⚠ **The default path — no `settings` at all — inherits and is
    /// PERMITTED.**
    ///
    /// Without this row an implementation that refuses *every* spawn passes all
    /// four rows of the matrix above, because all four of those are refusals or
    /// a same-affiliation override. This is the one that fails against a check
    /// written in the wrong direction, and it is the overwhelmingly common
    /// spawn: the child is handed the parent's SAME `Arc`, so the two
    /// affiliations are read off one instance and cannot differ.
    ///
    /// All three parent shapes, because "cannot differ" has to hold for the
    /// unaffiliated public parent too — where both sides are `None`, and an
    /// implementation that treated absence as a mismatch would refuse every
    /// public spawn in the product.
    #[tokio::test]
    async fn an_inheriting_spawn_carries_its_parents_affiliation_and_is_permitted() {
        for (label, parent) in [
            ("ucsf", institution_parent("ucsf")),
            ("local", local_parent()),
            ("public/unaffiliated", public_parent()),
        ] {
            let expected = parent.affiliation();
            let child = resolve_child_for(parent, vec![])
                .await
                .unwrap_or_else(|e| panic!("an inheriting spawn from a {label} parent: {e}"));
            assert_eq!(
                child.provider.affiliation(),
                expected,
                "an inheriting {label} child must run the parent's own instance"
            );
        }
    }

    /// ⚠ **A composite parent is compared as the FOLD of both halves, never as
    /// its lead.**
    ///
    /// The tier check needed its own reasoning for exactly this shape
    /// (`a_temperature_only_spawn_is_not_inert_on_a_composite_parent`), because
    /// `LeadWorkerProvider::get_name` answers for the lead alone while `tier()`
    /// is `least(lead, worker)`. Affiliation has the same split, with a twist
    /// that inverts the obvious guess: `providers::composite_affiliation` folds
    /// with `Local` as the **identity**, not as an absorbing element, so a
    /// `Local`-lead / `ucsf`-worker pair is covered by `ucsf`.
    ///
    /// So this row is the discriminating one, and it is a PERMIT: an
    /// implementation that read the lead's affiliation (`Local`) would refuse
    /// the UCSF child this pair is entitled to. Both other readings —
    /// absorbing-`Local`, or the worker alone — are also caught, the first by
    /// refusing and the second only by luck, which is why the fold is asserted
    /// directly beside the spawn.
    #[tokio::test]
    async fn a_composite_parent_is_compared_as_the_fold_of_both_halves() {
        use crate::providers::lead_worker::LeadWorkerProvider;

        let mut overrides = ucsf_child_overrides();
        overrides.extend(local_child_overrides());

        let child = crate::config::with_config_overrides(overrides.clone(), async move {
            let lead = crate::providers::create(
                "ollama",
                crate::model::ModelConfig::new_or_fail("llama3"),
            )
            .await
            .expect("ollama constructs with no credential and no network");
            assert_eq!(
                lead.affiliation(),
                Some(crate::privacy::ModelAffiliation::Local),
                "a loopback ollama is the Local half this row needs"
            );

            let parent: std::sync::Arc<dyn crate::providers::base::Provider> = std::sync::Arc::new(
                LeadWorkerProvider::new(lead, institution_parent("ucsf"), None),
            );
            assert_eq!(
                parent.get_name(),
                "ollama",
                "get_name answers for the LEAD alone — this is the whole mechanism"
            );
            assert_eq!(
                parent.affiliation(),
                Some(crate::privacy::ModelAffiliation::institution(
                    crate::privacy::affiliation::InstitutionId::new("ucsf")
                )),
                "Local is the fold's IDENTITY: the pair is covered by ucsf, not by Local"
            );

            spawn_child_on(parent, "versa_azure", overrides).await
        })
        .await
        .expect("the fold is ucsf and the child is ucsf, so this spawn is permitted");

        assert_eq!(
            child.provider.affiliation(),
            Some(crate::privacy::ModelAffiliation::institution(
                crate::privacy::affiliation::InstitutionId::new("ucsf")
            ))
        );
    }

    /// Task 43 (DR-23). The spawn partition is one of the three callers that
    /// never had a stamped tier to read and re-classified from a bare name
    /// instead, so a private extension renamed in `config.yaml` was inherited by
    /// a public child — the same enforcement failure the gates in the extension
    /// manager had, in a place no test looked.
    ///
    /// It was moved to the shared resolver with the rest of Step 1 and, review
    /// noticed, with no assertion of its own. This is that assertion. The
    /// fixture is a real rename: the entry's `name` is `mystuff`, nothing about
    /// it resembles `cdwagent`, and the only surviving link to the install is
    /// the `--directory` argument the marketplace wrote.
    ///
    /// `developer` rides along so a partition that dropped everything — or a
    /// fixture that inherited nothing — fails instead of passing vacuously, and
    /// the drop is asserted on `dropped_private_extensions` as well as on the
    /// surviving list, because a silent drop is a capability the model keeps
    /// planning around.
    #[tokio::test]
    async fn a_public_child_does_not_inherit_a_renamed_private_extension() {
        let install_dir = "/home/researcher/.config/biorouter/extensions/SubagentRenamed";
        crate::privacy::provenance::insert_test_record_at(
            "subagent-renamed-as-installed",
            "cdwagent",
            Some(install_dir),
        );
        let renamed = crate::agents::ExtensionConfig::Stdio {
            name: "mystuff".to_string(),
            description: "renamed by hand in config.yaml".to_string(),
            cmd: "uv".to_string(),
            args: vec![
                "run".to_string(),
                "--directory".to_string(),
                install_dir.to_string(),
                "server.py".to_string(),
            ],
            envs: crate::agents::extension::Envs::default(),
            env_keys: vec![],
            timeout: Some(300),
            bundled: None,
            available_tools: vec![],
        };
        assert_eq!(
            crate::privacy::classify_extension("mystuff"),
            ProviderTier::Public,
            "the fixture only discriminates if the NAME alone reads public"
        );

        let child = resolve_child(
            ProviderTier::Public,
            Ask::Inherit,
            vec![renamed, builtin_extension("developer")],
        )
        .await
        .unwrap();

        assert_eq!(
            child
                .extensions
                .iter()
                .map(crate::agents::ExtensionConfig::name)
                .collect::<Vec<_>>(),
            vec!["developer".to_string()],
            "a public child inherited a private extension because it had been renamed"
        );
        assert_eq!(
            child.dropped_private_extensions,
            vec!["mystuff".to_string()]
        );
    }

    /// The CALL SITE for the sentence above: the drop has to reach the parent's
    /// tool result, or the model silently loses a capability it will keep
    /// planning around. A pure test of the note passes against a note nothing
    /// ever appends.
    ///
    /// Driven through the real tool with a provider that fails on its first
    /// completion (the `empty.json` cassette pattern `subagent_handler`'s tests
    /// use), so the child's run ends immediately and the envelope this asserts
    /// on is the real one.
    #[tokio::test]
    #[serial_test::serial(workspace_services)]
    async fn the_parent_is_told_which_private_extensions_its_child_lost() {
        // This is the one test here that drives a REAL spawn, so it reaches
        // `announce_subagent_tab` and `begin_turn`. The `FakeGui` above is
        // installed process-wide by `set_for_tests`, and its `begin_turn` is an
        // `unreachable!` — so this must both take the `workspace_services`
        // serial token and pin itself headless rather than inherit whatever a
        // panicking neighbour left behind.
        crate::workspace_services::set_for_tests(None);
        let temp = tempfile::TempDir::new().unwrap();
        let sm = std::sync::Arc::new(crate::session::SessionManager::new(
            temp.path().to_path_buf(),
        ));
        let config = AgentConfig::new(
            sm.clone(),
            crate::config::permission::PermissionManager::instance(),
            None,
            crate::config::BioRouterMode::Auto,
        );
        let cassette = temp.path().join("empty.json");
        std::fs::write(&cassette, "{}").unwrap();
        // `TestProvider` takes the fail-safe default tier, Public — which is the
        // parent shape this covers: a public chat that still holds a private
        // extension in its list (Gate C refuses the CALL, it does not unload the
        // extension). The child inherits the list and must not inherit that one.
        let provider: std::sync::Arc<dyn crate::providers::base::Provider> = std::sync::Arc::new(
            crate::providers::testprovider::TestProvider::new_replaying(cassette.to_str().unwrap())
                .unwrap(),
        );
        let task_config = TaskConfig::new(
            provider,
            "parent-1",
            temp.path(),
            vec![builtin_extension("ucsfomopagent")],
        );

        let result = handle_subagent_tool(
            &config,
            json!({ "instructions": "summarise the cohort" }),
            task_config,
            HashMap::new(),
            temp.path().to_path_buf(),
            None,
        )
        .result
        .await
        .expect("the spawn itself is permitted — only the extension is dropped");

        let text = result
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            text.contains("ucsfomopagent"),
            "the parent must be told which capability its child lost: {text}"
        );
        crate::workspace_services::clear_test_override();
    }
}
