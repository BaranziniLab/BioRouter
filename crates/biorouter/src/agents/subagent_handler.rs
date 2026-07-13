use crate::{
    agents::{
        subagent_result::{SubagentResult, SubagentTokens},
        subagent_task_config::TaskConfig,
        Agent, AgentConfig, AgentEvent, SessionConfig,
    },
    conversation::{message::Message, Conversation},
    prompt_template::render_global_file,
    session::SessionManager,
    workflow::Workflow,
};
use anyhow::{anyhow, Result};
use futures::StreamExt;
use serde::Serialize;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

#[derive(Serialize)]
struct SubagentPromptContext {
    max_turns: usize,
    subagent_id: String,
    task_instructions: String,
    tool_count: usize,
    available_tools: String,
}

type AgentMessagesFuture =
    Pin<Box<dyn Future<Output = Result<(Conversation, Option<String>)>> + Send>>;

/// Standalone function to run a complete subagent task, returning a structured
/// result envelope. A run that fails, or one that ends on a tool call without a
/// final text message, still yields a meaningful `SubagentResult` (BR-40) —
/// never the old lossy "No text content in last message" string.
pub async fn run_complete_subagent_task(
    config: AgentConfig,
    workflow: Workflow,
    task_config: TaskConfig,
    return_last_only: bool,
    session_id: String,
    cancellation_token: Option<CancellationToken>,
) -> SubagentResult {
    let session_manager = config.session_manager.clone();

    // Surface this subagent in the process-wide "active work" view (BR-42) for
    // the run's whole lifetime. The guard deregisters on drop, so an early
    // return or panic never leaks a phantom "still running" entry. Cancel routes
    // to the run's cancellation token when one was supplied.
    let _active_work = {
        use biorouter_mcp::active_work::{ActiveWorkGuard, ActiveWorkKind};
        let title = subagent_work_title(&workflow);
        let cancel = cancellation_token.clone().map(|token| {
            let cancel: std::sync::Arc<dyn Fn() + Send + Sync> =
                std::sync::Arc::new(move || token.cancel());
            cancel
        });
        ActiveWorkGuard::register(
            ActiveWorkKind::Subagent,
            title,
            Some(format!("child session {session_id}")),
            Some(task_config.parent_session_id.clone()),
            cancel,
        )
    };

    let (messages, final_output) = match get_agent_messages(
        config,
        workflow,
        task_config,
        session_id.clone(),
        cancellation_token,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => return SubagentResult::from_error(format!("Failed to execute task: {e}")),
    };

    let mut result = SubagentResult::from_conversation(&messages, final_output, return_last_only);
    result.tokens = fetch_subagent_tokens(&session_manager, &session_id).await;
    result
}

/// A short, human-readable label for the active-work view: the subagent's task
/// prompt (or, failing that, its instructions), collapsed to one line and
/// truncated. Falls back to a generic label when the workflow carries neither.
fn subagent_work_title(workflow: &Workflow) -> String {
    let raw = workflow
        .prompt
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .or(workflow.instructions.as_deref())
        .unwrap_or("subagent task");
    let one_line = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut title: String = one_line.chars().take(120).collect();
    if one_line.chars().count() > 120 {
        title.push('…');
    }
    title
}

/// Read the child session's lifetime token totals for the result envelope.
/// Best-effort: a missing session or all-zero counts yields `None`.
async fn fetch_subagent_tokens(
    session_manager: &SessionManager,
    session_id: &str,
) -> Option<SubagentTokens> {
    let session = session_manager.get_session(session_id, false).await.ok()?;
    let total = session.accumulated_total_tokens.unwrap_or(0);
    let input = session.accumulated_input_tokens.unwrap_or(0);
    let output = session.accumulated_output_tokens.unwrap_or(0);
    if total == 0 && input == 0 && output == 0 {
        return None;
    }
    Some(SubagentTokens {
        total,
        input,
        output,
    })
}

#[allow(clippy::too_many_lines)]
fn get_agent_messages(
    config: AgentConfig,
    workflow: Workflow,
    task_config: TaskConfig,
    session_id: String,
    cancellation_token: Option<CancellationToken>,
) -> AgentMessagesFuture {
    Box::pin(async move {
        let system_instructions = workflow.instructions.clone().unwrap_or_default();
        let user_task = workflow
            .prompt
            .clone()
            .unwrap_or_else(|| "Begin.".to_string());

        let agent = Arc::new(Agent::with_config(config));
        let parent_working_dir = task_config.parent_working_dir.clone();

        // SubagentStart hook (observe-only). The child agent fires its own
        // tool/stop hooks while it runs.
        {
            let hooks = agent.hooks_manager();
            let mut payload = crate::hooks::HookPayload::new(
                crate::hooks::HookEvent::SubagentStart,
                &task_config.parent_session_id,
                parent_working_dir.to_string_lossy(),
            );
            payload.subagent_id = Some(session_id.clone());
            payload.message = Some(system_instructions.chars().take(500).collect());
            hooks.fire(
                crate::hooks::HookEvent::SubagentStart,
                None,
                payload,
                parent_working_dir.clone(),
            );
        }

        agent
            .update_provider(task_config.provider, &session_id)
            .await
            .map_err(|e| anyhow!("Failed to set provider on sub agent: {}", e))?;

        for extension in task_config.extensions {
            if let Err(e) = agent.add_extension(extension.clone()).await {
                debug!(
                    "Failed to add extension '{}' to subagent: {}",
                    extension.name(),
                    e
                );
            }
        }

        let has_response_schema = workflow.response.is_some();
        agent
            .apply_workflow_components(
                workflow.sub_workflows.clone(),
                workflow.response.clone(),
                true,
            )
            .await;

        let tools = agent.list_tools(&session_id, None).await;
        let subagent_prompt = render_global_file(
            "subagent_system.md",
            &SubagentPromptContext {
                max_turns: task_config
                    .max_turns
                    .expect("TaskConfig always sets max_turns"),
                subagent_id: session_id.clone(),
                task_instructions: system_instructions,
                tool_count: tools.len(),
                available_tools: tools
                    .iter()
                    .map(|t| t.name.to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
            },
        )
        .map_err(|e| anyhow!("Failed to render subagent system prompt: {}", e))?;
        agent.override_system_prompt(subagent_prompt).await;

        let user_message = Message::user().with_text(user_task);
        let mut conversation = Conversation::new_unvalidated(vec![user_message.clone()]);

        if let Some(activities) = workflow.activities {
            for activity in activities {
                info!("Workflow activity: {}", activity);
            }
        }
        let session_config = SessionConfig {
            id: session_id.clone(),
            schedule_id: None,
            max_turns: task_config.max_turns.map(|v| v as u32),
            max_tool_calls: None,
            budget: None,
            retry_config: workflow.retry,
        };

        let mut stream = crate::session_context::with_session_id(Some(session_id.clone()), async {
            agent
                .reply(user_message, session_config, cancellation_token)
                .await
        })
        .await
        .map_err(|e| anyhow!("Failed to get reply from agent: {}", e))?;
        while let Some(message_result) = stream.next().await {
            match message_result {
                Ok(AgentEvent::Message(msg)) => conversation.push(msg),
                Ok(AgentEvent::McpNotification(_)) | Ok(AgentEvent::ModelChange { .. }) => {}
                Ok(AgentEvent::HistoryReplaced(updated_conversation)) => {
                    conversation = updated_conversation;
                }
                Err(e) => {
                    tracing::error!("Error receiving message from subagent: {}", e);
                    break;
                }
            }
        }

        let final_output = if has_response_schema {
            agent
                .final_output_tool
                .lock()
                .await
                .as_ref()
                .and_then(|tool| tool.final_output.clone())
        } else {
            None
        };

        Ok((conversation, final_output))
    })
}
