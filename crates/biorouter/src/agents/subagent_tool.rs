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
// itself spawn subagents, so spawning was previously unbounded. Two caps bound
// it: the semaphore throttles *concurrent* subagents; the in-flight ceiling
// refuses outright once too many are queued+running so a recursive spawn storm
// can't accumulate unbounded tasks. Both env-overridable.
fn max_concurrent_subagents() -> usize {
    std::env::var("BIOROUTER_SUBAGENT_MAX_CONCURRENT")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or(8)
}
fn max_inflight_subagents() -> usize {
    std::env::var("BIOROUTER_SUBAGENT_MAX_INFLIGHT")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or(64)
}
static SUBAGENT_SEMAPHORE: LazyLock<Semaphore> =
    LazyLock::new(|| Semaphore::new(max_concurrent_subagents()));
static SUBAGENT_INFLIGHT: AtomicUsize = AtomicUsize::new(0);

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

/// Current number of in-flight subagents (test/introspection helper).
pub fn inflight_subagent_count() -> usize {
    SUBAGENT_INFLIGHT.load(Ordering::SeqCst)
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
    let guard = if visibility.is_visible() {
        // The cap is the claim: no separate read of the counter, so a parallel
        // fan-out cannot slip past it. Failing to claim is not a refusal — the
        // child runs, it just runs in the background, and `parent_note` tells
        // the model why (decision 26).
        match VisibleChildGuard::try_claim(parent_session_id) {
            Some(guard) => Some(guard),
            None => {
                return (
                    ChildVisibility::BackgroundCapped {
                        cap: max_visible_child_tabs(),
                    },
                    None,
                );
            }
        }
    } else {
        None
    };

    let Some(services) = services else {
        return (visibility, guard);
    };

    let placement = params
        .placement
        .clone()
        .unwrap_or_else(|| "tab".to_string());
    let child = child_session_id.to_string();
    let parent = parent_session_id.to_string();
    tokio::spawn(async move {
        // Frame vocabulary parity with workspace_open (Task 24): "window" is
        // its own cmd; tab/split ride open_tab. Focus etiquette (Task 29)
        // downgrades either to a notification when announce-only is on — which
        // is exactly the `ChildVisibility::AnnounceOnly` path.
        let open_frame = if placement == "window" {
            serde_json::json!({
                "type": "workspace", "cmd": "open_window", "session_id": child,
            })
        } else {
            serde_json::json!({
                "type": "workspace", "cmd": "open_tab",
                "session_id": child, "placement": placement, "focus": false,
            })
        };
        let _ = services
            .gui_command(
                crate::agents::workspace_extension::apply_focus_etiquette(
                    open_frame,
                    announce_only,
                ),
                false,
            )
            .await;
        // The badge is NOT focus-stealing, so it is sent regardless: a child the
        // user opens later from History still shows as a subagent of its parent.
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
    // flight, then throttle concurrency. The guard + permit are held until the
    // subagent finishes — on the blocking path that is when this function
    // returns; on the background path the guard moves into the detached task, so
    // a storm of background spawns is bounded exactly like a storm of blocking
    // ones.
    let (inflight, inflight_count) = InflightGuard::enter();
    let max_inflight = max_inflight_subagents();
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
        let session =
            create_subagent_session(&config, working_dir, &task_config.parent_session_id).await?;
        let task_config = overridden_task_config(task_config, &params).await?;
        return Ok(spawn_background_subagent(
            config,
            workflow,
            task_config,
            &params,
            session.id,
            inflight,
        ));
    }

    let _permit = SUBAGENT_SEMAPHORE.acquire().await.map_err(|e| ErrorData {
        code: ErrorCode::INTERNAL_ERROR,
        message: Cow::from(format!("Subagent semaphore closed: {e}")),
        data: None,
    })?;
    let _inflight = inflight;

    let session =
        create_subagent_session(&config, working_dir, &task_config.parent_session_id).await?;

    // BR-71 decision 24: glass-box by default. The guard lives for the child's
    // whole run, so the slot is released exactly when the child finishes.
    let (visibility, _visible_guard) =
        announce_subagent_tab(&session.id, &task_config.parent_session_id, &params);
    let visibility_note = visibility.parent_note(&session.id);

    let task_config = overridden_task_config(task_config, &params).await?;

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
    Ok(call_result)
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
) -> Result<crate::session::Session, ErrorData> {
    let internal = |e: &dyn std::fmt::Display| ErrorData {
        code: ErrorCode::INTERNAL_ERROR,
        message: Cow::from(format!("Failed to create session: {e}")),
        data: None,
    };

    let mut session = config
        .session_manager
        .create_session(
            working_dir,
            "Subagent task".to_string(),
            crate::session::session_manager::SessionType::SubAgent,
        )
        .await
        .map_err(|e| internal(&e))?;

    config
        .session_manager
        .update(&session.id)
        .parent_session_id(Some(parent_session_id.to_string()))
        .apply()
        .await
        .map_err(|e| internal(&e))?;
    // Keep the in-memory copy honest with the row we just wrote.
    session.parent_session_id = Some(parent_session_id.to_string());

    Ok(session)
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

    let task_handle = handle.clone();
    tokio::spawn(async move {
        // Held for the child's whole life, exactly as on the blocking path.
        let _inflight = inflight;
        let _visible = visible_guard;
        let _permit = match SUBAGENT_SEMAPHORE.acquire().await {
            Ok(permit) => permit,
            Err(e) => {
                task_handle.complete(SubagentResult::from_error(format!(
                    "Subagent semaphore closed: {e}"
                )));
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

    let text = background_started_message(
        &handle.id,
        &handle.child_session_id,
        &visibility.parent_note(&handle.child_session_id),
    );

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

async fn apply_settings_overrides(
    mut task_config: TaskConfig,
    params: &SubagentParams,
) -> Result<TaskConfig> {
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

    if let Some(extension_names) = &params.extensions {
        if extension_names.is_empty() {
            task_config.extensions = Vec::new();
        } else {
            task_config
                .extensions
                .retain(|ext| extension_names.contains(&ext.name()));
        }
    }

    Ok(task_config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        assert_eq!(SUBAGENT_TOOL_NAME, "subagent");
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

        let session = create_subagent_session(&config, temp.path().to_path_buf(), "parent-99")
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
}
