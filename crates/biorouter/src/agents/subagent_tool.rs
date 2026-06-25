use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::LazyLock;

use anyhow::{anyhow, Result};
use futures::FutureExt;
use rmcp::model::{Content, ErrorCode, ErrorData, Tool};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::agents::subagent_handler::run_complete_subagent_task;
use crate::agents::subagent_task_config::TaskConfig;
use crate::agents::tool_execution::ToolCallResult;
use crate::agents::AgentConfig;
use crate::providers;
use crate::workflow::build_workflow::build_workflow_from_template;
use crate::workflow::local_workflows::load_local_workflow_file;
use crate::workflow::{SubWorkflow, Workflow};

pub const SUBAGENT_TOOL_NAME: &str = "subagent";

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

    let schema = json!({
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
            }
        }
    });

    Tool::new(
        SUBAGENT_TOOL_NAME,
        description,
        schema.as_object().unwrap().clone(),
    )
}

fn build_tool_description(sub_workflows: &[SubWorkflow]) -> String {
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
    // flight, then throttle concurrency. The guard + permit are held until this
    // function returns (i.e. the subagent finishes).
    let (_inflight, inflight_count) = InflightGuard::enter();
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
    let _permit = SUBAGENT_SEMAPHORE.acquire().await.map_err(|e| ErrorData {
        code: ErrorCode::INTERNAL_ERROR,
        message: Cow::from(format!("Subagent semaphore closed: {e}")),
        data: None,
    })?;

    let session = config
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
        })?;

    let task_config = apply_settings_overrides(task_config, &params)
        .await
        .map_err(|e| ErrorData {
            code: ErrorCode::INVALID_PARAMS,
            message: Cow::from(e.to_string()),
            data: None,
        })?;

    let result = run_complete_subagent_task(
        config,
        workflow,
        task_config,
        params.summary,
        session.id,
        cancellation_token,
    )
    .await;

    match result {
        Ok(text) => Ok(rmcp::model::CallToolResult {
            content: vec![Content::text(text)],
            structured_content: None,
            is_error: Some(false),
            meta: None,
        }),
        Err(e) => Err(ErrorData {
            code: ErrorCode::INTERNAL_ERROR,
            message: Cow::from(e.to_string()),
            data: None,
        }),
    }
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
}
