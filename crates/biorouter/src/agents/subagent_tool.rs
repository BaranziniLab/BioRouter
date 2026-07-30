use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::LazyLock;
use std::time::Duration;

use anyhow::{anyhow, Result};
use futures::FutureExt;
use rmcp::model::{CallToolResult, Content, ErrorCode, ErrorData, Tool};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::agents::subagent_handle::{self, BackgroundSubagent, DEFAULT_WAIT_SECS, MAX_WAIT_SECS};
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
pub const SUBAGENT_STATUS_TOOL_NAME: &str = "subagent_status";

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
            "description": "If true, start the subagent and return a handle immediately \
                            instead of waiting for it. Poll it with the `subagent_status` \
                            tool. Use for long tasks you want to run while you keep working."
        });
    }

    Tool::new(
        SUBAGENT_TOOL_NAME,
        description,
        schema.as_object().unwrap().clone(),
    )
}

/// The poll/await half of the spawn→poll model (BR-40). Only listed when
/// `BIOROUTER_SUBAGENT_BACKGROUND` is on.
pub fn create_subagent_status_tool() -> Tool {
    let schema = json!({
        "type": "object",
        "properties": {
            "handle": {
                "type": "string",
                "description": "Handle returned by a background `subagent` call (e.g. \"sub_1\"). Omit to list all background subagents of this session."
            },
            "wait": {
                "type": "boolean",
                "default": false,
                "description": "If true, block until the subagent finishes (or `timeout_seconds` elapses) instead of returning its current state."
            },
            "timeout_seconds": {
                "type": "number",
                "description": "How long to block when `wait` is true. Default 60, max 600. A timeout is not an error — the subagent keeps running and can be polled again."
            },
            "cancel": {
                "type": "boolean",
                "default": false,
                "description": "If true, ask the subagent to stop. It finishes with whatever it produced so far."
            }
        }
    });

    Tool::new(
        SUBAGENT_STATUS_TOOL_NAME,
        "Check on subagents started with `background: true`: list them, poll one, block until one \
         finishes, or cancel one. A finished subagent returns the same structured result envelope \
         a blocking `subagent` call would have returned.",
        schema.as_object().unwrap().clone(),
    )
}

#[derive(Debug, Default, Deserialize)]
pub struct SubagentStatusParams {
    pub handle: Option<String>,
    #[serde(default)]
    pub wait: bool,
    pub timeout_seconds: Option<u64>,
    #[serde(default)]
    pub cancel: bool,
}

/// Resolve the requested block, clamped to something a turn can survive.
fn wait_duration(timeout_seconds: Option<u64>) -> Duration {
    let secs = timeout_seconds
        .unwrap_or(DEFAULT_WAIT_SECS)
        .clamp(1, MAX_WAIT_SECS);
    Duration::from_secs(secs)
}

pub fn handle_subagent_status_tool(params: Value, parent_session_id: String) -> ToolCallResult {
    let parsed: SubagentStatusParams = match serde_json::from_value(params) {
        Ok(p) => p,
        Err(e) => {
            return ToolCallResult::from(Err(ErrorData {
                code: ErrorCode::INVALID_PARAMS,
                message: Cow::from(format!("Invalid parameters: {e}")),
                data: None,
            }));
        }
    };

    ToolCallResult {
        notification_stream: None,
        result: Box::new(subagent_status(parsed, parent_session_id).boxed()),
    }
}

async fn subagent_status(
    params: SubagentStatusParams,
    parent_session_id: String,
) -> Result<CallToolResult, ErrorData> {
    let Some(id) = params.handle.clone() else {
        return Ok(list_handles(&parent_session_id));
    };

    let handle = subagent_handle::get_for_session(&parent_session_id, &id).ok_or_else(|| {
        let known: Vec<String> = subagent_handle::list_for_session(&parent_session_id)
            .iter()
            .map(|h| h.id.clone())
            .collect();
        ErrorData {
            code: ErrorCode::INVALID_PARAMS,
            message: Cow::from(if known.is_empty() {
                format!("Unknown subagent handle '{id}'. This session has no background subagents.")
            } else {
                format!(
                    "Unknown subagent handle '{id}'. Known handles: {}",
                    known.join(", ")
                )
            }),
            data: None,
        }
    })?;

    if params.cancel {
        handle.cancel();
    }

    let finished = if params.wait {
        handle.wait(wait_duration(params.timeout_seconds)).await
    } else {
        handle.result()
    };

    let snapshot = handle.snapshot();
    let text = match finished {
        Some(result) => format!(
            "Subagent {} finished.\n\n{}",
            handle.id,
            result.to_agent_text()
        ),
        None if params.cancel => format!(
            "Cancellation requested for subagent {} ({}). Poll it again to collect its result.",
            handle.id, handle.title
        ),
        None => format!(
            "Subagent {} is still running ({}s elapsed): {}. \
             Poll again later, or call again with wait=true to block until it finishes.",
            handle.id, snapshot.elapsed_seconds, handle.title
        ),
    };

    Ok(CallToolResult {
        content: vec![Content::text(text)],
        structured_content: serde_json::to_value(&snapshot).ok(),
        is_error: Some(false),
        meta: None,
    })
}

fn list_handles(parent_session_id: &str) -> CallToolResult {
    let snapshots: Vec<_> = subagent_handle::list_for_session(parent_session_id)
        .iter()
        .map(|h| h.snapshot())
        .collect();

    let text = if snapshots.is_empty() {
        "No background subagents have been started in this session.".to_string()
    } else {
        let lines: Vec<String> = snapshots.iter().map(|s| s.to_line()).collect();
        format!("Background subagents:\n{}", lines.join("\n"))
    };

    CallToolResult {
        content: vec![Content::text(text)],
        structured_content: serde_json::to_value(json!({ "subagents": snapshots })).ok(),
        is_error: Some(false),
        meta: None,
    }
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
             pass `background: true` to get a handle back immediately and keep working; \
             collect the result later with the `subagent_status` tool.",
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
        let session = create_subagent_session(&config, working_dir).await?;
        let task_config = overridden_task_config(task_config, &params).await?;
        return Ok(spawn_background_subagent(
            config,
            workflow,
            task_config,
            params.summary,
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

    let session = create_subagent_session(&config, working_dir).await?;
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

    Ok(result.into_call_tool_result())
}

async fn create_subagent_session(
    config: &AgentConfig,
    working_dir: PathBuf,
) -> Result<crate::session::Session, ErrorData> {
    config
        .session_manager
        .create_session(
            working_dir,
            "Subagent task".to_string(),
            crate::session::session_manager::SessionType::SubAgent,
        )
        .await
        .map_err(|e| ErrorData {
            code: ErrorCode::INTERNAL_ERROR,
            message: Cow::from(format!("Failed to create session: {}", e)),
            data: None,
        })
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
/// ended. The token stays reachable — `subagent_status { cancel: true }` and the
/// BR-42 active-work view (registered inside `run_complete_subagent_task`) both
/// route to it.
fn spawn_background_subagent(
    config: AgentConfig,
    workflow: Workflow,
    task_config: TaskConfig,
    summary: bool,
    child_session_id: String,
    inflight: InflightGuard,
) -> CallToolResult {
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

    let task_handle = handle.clone();
    tokio::spawn(async move {
        // Held for the child's whole life, exactly as on the blocking path.
        let _inflight = inflight;
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

    // Task 36 replaces the empty note with `ChildVisibility::parent_note`.
    let text = background_started_message(&handle.id, &handle.child_session_id, "");

    CallToolResult {
        content: vec![Content::text(text)],
        structured_content: serde_json::to_value(handle.snapshot()).ok(),
        is_error: Some(false),
        meta: None,
    }
}

/// What a `background: true` spawn returns to the parent. BR-71 decision 23:
/// `subagent_status` no longer exists, and the child's SESSION ID is the handle
/// every workspace tool takes.
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

    #[test]
    fn status_tool_schema_exposes_poll_wait_and_cancel() {
        let tool = create_subagent_status_tool();
        assert_eq!(tool.name, SUBAGENT_STATUS_TOOL_NAME);
        let props = tool.input_schema["properties"]
            .as_object()
            .expect("object schema");
        for key in ["handle", "wait", "timeout_seconds", "cancel"] {
            assert!(props.contains_key(key), "missing '{key}' in status schema");
        }
    }

    #[test]
    fn status_params_default_to_a_plain_poll() {
        let params: SubagentStatusParams =
            serde_json::from_value(json!({"handle": "sub_1"})).unwrap();
        assert_eq!(params.handle.as_deref(), Some("sub_1"));
        assert!(!params.wait);
        assert!(!params.cancel);
        assert!(params.timeout_seconds.is_none());
    }

    #[test]
    fn wait_duration_is_clamped() {
        assert_eq!(wait_duration(None), Duration::from_secs(DEFAULT_WAIT_SECS));
        assert_eq!(wait_duration(Some(0)), Duration::from_secs(1));
        assert_eq!(wait_duration(Some(5)), Duration::from_secs(5));
        assert_eq!(
            wait_duration(Some(9_999)),
            Duration::from_secs(MAX_WAIT_SECS)
        );
    }

    fn finished_result(text: &str) -> SubagentResult {
        use crate::conversation::message::Message;
        use crate::conversation::Conversation;
        SubagentResult::from_conversation(
            &Conversation::new_unvalidated(vec![Message::assistant().with_text(text)]),
            None,
            true,
        )
    }

    async fn call_status(params: Value, session: &str) -> Result<CallToolResult, ErrorData> {
        let call = handle_subagent_status_tool(params, session.to_string());
        Box::into_pin(call.result).await
    }

    fn text_of(result: &CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|c| match &c.raw {
                rmcp::model::RawContent::Text(text) => Some(text.text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[tokio::test]
    async fn status_polls_a_running_handle_then_returns_its_envelope() {
        let session = "status-poll-session";
        let handle = BackgroundSubagent::register(
            session,
            "child-1",
            "crawl the corpus",
            CancellationToken::new(),
        );

        let running = call_status(json!({"handle": handle.id.clone()}), session)
            .await
            .expect("poll succeeds");
        assert_eq!(running.is_error, Some(false));
        assert!(text_of(&running).contains("still running"));
        assert_eq!(
            running.structured_content.as_ref().unwrap()["state"],
            "running"
        );

        handle.complete(finished_result("crawled 40 papers"));

        let done = call_status(json!({"handle": handle.id.clone()}), session)
            .await
            .expect("poll succeeds");
        assert!(text_of(&done).contains("crawled 40 papers"));
        let structured = done.structured_content.unwrap();
        assert_eq!(structured["state"], "finished");
        assert_eq!(structured["result"]["status"], "completed");
    }

    #[tokio::test]
    async fn status_wait_blocks_until_the_child_finishes() {
        let session = "status-wait-session";
        let handle =
            BackgroundSubagent::register(session, "child-2", "slow task", CancellationToken::new());

        let waiter = handle.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            waiter.complete(finished_result("slow task done"));
        });

        let result = call_status(
            json!({"handle": handle.id.clone(), "wait": true, "timeout_seconds": 5}),
            session,
        )
        .await
        .expect("wait succeeds");
        assert!(text_of(&result).contains("slow task done"));
    }

    #[tokio::test]
    async fn status_wait_timeout_reports_still_running_not_an_error() {
        let session = "status-timeout-session";
        let handle = BackgroundSubagent::register(
            session,
            "child-3",
            "very slow task",
            CancellationToken::new(),
        );

        let result = call_status(
            json!({"handle": handle.id.clone(), "wait": true, "timeout_seconds": 1}),
            session,
        )
        .await
        .expect("a timeout is not a tool error");
        assert_eq!(result.is_error, Some(false));
        assert!(text_of(&result).contains("still running"));
    }

    #[tokio::test]
    async fn status_cancel_requests_a_stop() {
        let session = "status-cancel-session";
        let token = CancellationToken::new();
        let handle = BackgroundSubagent::register(session, "child-4", "runaway", token.clone());

        let result = call_status(
            json!({"handle": handle.id.clone(), "cancel": true}),
            session,
        )
        .await
        .expect("cancel succeeds");
        assert!(token.is_cancelled());
        assert!(text_of(&result).contains("Cancellation requested"));
    }

    #[tokio::test]
    async fn status_without_a_handle_lists_this_sessions_subagents_only() {
        let mine = "status-list-mine";
        let theirs = "status-list-theirs";
        let a = BackgroundSubagent::register(mine, "c1", "task A", CancellationToken::new());
        let _b = BackgroundSubagent::register(theirs, "c2", "task B", CancellationToken::new());

        let listed = call_status(json!({}), mine).await.expect("list succeeds");
        let text = text_of(&listed);
        assert!(text.contains(&a.id));
        assert!(text.contains("task A"));
        assert!(!text.contains("task B"));

        let empty = call_status(json!({}), "status-list-nobody")
            .await
            .expect("list succeeds");
        assert!(text_of(&empty).contains("No background subagents"));
    }

    #[tokio::test]
    async fn status_rejects_a_handle_from_another_session() {
        let owner = "status-owner-session";
        let handle =
            BackgroundSubagent::register(owner, "c5", "private task", CancellationToken::new());

        let err = call_status(
            json!({"handle": handle.id.clone()}),
            "status-intruder-session",
        )
        .await
        .expect_err("another session cannot poll this handle");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(err.message.contains("Unknown subagent handle"));
    }
}
