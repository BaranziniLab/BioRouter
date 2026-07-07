use anyhow::{anyhow, Result};

use crate::agents::types::SessionConfig;
use crate::context_mgmt::compact_messages;
use crate::conversation::message::{Message, SystemNotificationType};
use crate::workflow::build_workflow::build_workflow_from_template_with_positional_params;

use super::Agent;

pub const COMPACT_TRIGGERS: &[&str] =
    &["/compact", "Please compact this conversation", "/summarize"];

pub struct CommandDef {
    pub name: &'static str,
    pub description: &'static str,
}

static COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "compact",
        description: "Compact the conversation history",
    },
    CommandDef {
        name: "clear",
        description: "Clear the conversation history",
    },
    CommandDef {
        name: "goal",
        description: "Keep working until a condition is met (/goal <condition>, /goal clear)",
    },
    CommandDef {
        name: "loop",
        description: "Run a prompt on an interval (/loop 5m <prompt>, /loop stop <id>)",
    },
    CommandDef {
        name: "schedule",
        description: "Schedule a recurring prompt (/schedule @daily <prompt>, /schedule list)",
    },
];

pub fn list_commands() -> &'static [CommandDef] {
    COMMANDS
}

impl Agent {
    pub async fn execute_command(
        &self,
        message_text: &str,
        session_config: &SessionConfig,
    ) -> Result<Option<Message>> {
        let session_id = session_config.id.as_str();
        let mut trimmed = message_text.trim().to_string();

        if COMPACT_TRIGGERS.contains(&trimmed.as_str()) {
            trimmed = COMPACT_TRIGGERS[0].to_string();
        }

        if !trimmed.starts_with('/') {
            return Ok(None);
        }

        let command_str = trimmed.strip_prefix('/').unwrap_or(&trimmed);
        let (command, params_str) = command_str
            .split_once(' ')
            .map(|(cmd, p)| (cmd, p.trim()))
            .unwrap_or((command_str, ""));

        if command.starts_with("skill:")
            || command.starts_with("ext:")
            || command.starts_with("kb:")
        {
            return Ok(None);
        }

        match command {
            "compact" => self.handle_compact_command(session_config).await,
            "clear" => self.handle_clear_command(session_id).await,
            "goal" => self.handle_goal_command(params_str, session_id).await,
            "loop" => self.handle_loop_command(params_str, session_id).await,
            "schedule" => self.handle_schedule_command(params_str, session_id).await,
            _ => {
                self.handle_workflow_command(command, params_str, session_id)
                    .await
            }
        }
    }

    async fn handle_compact_command(
        &self,
        session_config: &SessionConfig,
    ) -> Result<Option<Message>> {
        let session_id = session_config.id.as_str();
        let manager = self.config.session_manager.clone();
        let session = manager.get_session(session_id, true).await?;
        let conversation = session
            .conversation
            .ok_or_else(|| anyhow!("Session has no conversation"))?;

        self.fire_compaction_hook(
            crate::hooks::HookEvent::PreCompact,
            session_id,
            &session.working_dir,
            "manual",
            None,
        );

        let (compacted_conversation, summarization_usage) = compact_messages(
            self.provider().await?.as_ref(),
            &conversation,
            true, // is_manual_compact
        )
        .await?;

        manager
            .replace_conversation(session_id, &compacted_conversation)
            .await?;

        self.fire_compaction_hook(
            crate::hooks::HookEvent::PostCompact,
            session_id,
            &session.working_dir,
            "manual",
            None,
        );

        // Without this, session.total_tokens stays at the pre-compact value:
        // the UI gauge keeps reading the old number, and the next reply's
        // check_if_compaction_needed re-triggers a full LLM summarization.
        self.update_session_metrics(session_config, &summarization_usage, true)
            .await?;

        Ok(Some(Message::assistant().with_system_notification(
            SystemNotificationType::InlineMessage,
            "Compaction complete",
        )))
    }

    async fn handle_clear_command(&self, session_id: &str) -> Result<Option<Message>> {
        use crate::conversation::Conversation;

        let manager = self.config.session_manager.clone();
        manager
            .replace_conversation(session_id, &Conversation::default())
            .await?;

        manager
            .update(session_id)
            .total_tokens(Some(0))
            .input_tokens(Some(0))
            .output_tokens(Some(0))
            .apply()
            .await?;

        Ok(Some(Message::assistant().with_system_notification(
            SystemNotificationType::InlineMessage,
            "Conversation cleared",
        )))
    }

    async fn handle_workflow_command(
        &self,
        command: &str,
        params_str: &str,
        _session_id: &str,
    ) -> Result<Option<Message>> {
        let full_command = format!("/{}", command);
        let workflow_path = match crate::slash_commands::get_workflow_for_command(&full_command) {
            Some(path) => path,
            None => return Ok(None),
        };

        if !workflow_path.exists() {
            return Ok(None);
        }

        let workflow_content = tokio::fs::read_to_string(&workflow_path)
            .await
            .map_err(|e| anyhow!("Failed to read workflow file: {}", e))?;

        let workflow_dir = workflow_path
            .parent()
            .ok_or_else(|| anyhow!("Workflow path has no parent directory"))?;

        let workflow_dir_str = workflow_dir.display().to_string();
        let validation_result =
            crate::workflow::validate_workflow::validate_workflow_template_from_content(
                &workflow_content,
                Some(workflow_dir_str),
            )
            .map_err(|e| anyhow!("Failed to parse workflow: {}", e))?;

        let param_values: Vec<String> = if params_str.is_empty() {
            vec![]
        } else {
            let params_without_default = validation_result
                .parameters
                .as_ref()
                .map(|params| params.iter().filter(|p| p.default.is_none()).count())
                .unwrap_or(0);

            if params_without_default <= 1 {
                vec![params_str.to_string()]
            } else {
                let param_names: Vec<String> = validation_result
                    .parameters
                    .as_ref()
                    .map(|params| {
                        params
                            .iter()
                            .filter(|p| p.default.is_none())
                            .map(|p| p.key.clone())
                            .collect()
                    })
                    .unwrap_or_default();

                let error_message = format!(
                    "The /{} workflow requires {} parameters: {}.\n\n\
                    Slash command workflows only support 1 parameter.\n\n\
                    **To use this workflow:**\n\
                    • **CLI:** `biorouter run --workflow {} {}`\n\
                    • **Desktop:** Launch from the workflows sidebar to fill in parameters",
                    command,
                    params_without_default,
                    param_names
                        .iter()
                        .map(|name| format!("**{}**", name))
                        .collect::<Vec<_>>()
                        .join(", "),
                    command,
                    param_names
                        .iter()
                        .map(|name| format!("--params {}=\"...\"", name))
                        .collect::<Vec<_>>()
                        .join(" ")
                );

                return Err(anyhow!(error_message));
            }
        };

        let param_values_len = param_values.len();

        let workflow = match build_workflow_from_template_with_positional_params(
            workflow_content,
            workflow_dir,
            param_values,
            None::<fn(&str, &str) -> Result<String>>,
        ) {
            Ok(workflow) => workflow,
            Err(crate::workflow::build_workflow::WorkflowError::MissingParams { parameters }) => {
                return Ok(Some(Message::assistant().with_text(format!(
                    "Workflow requires {} parameter(s): {}. Provided: {}",
                    parameters.len(),
                    parameters.join(", "),
                    param_values_len
                ))));
            }
            Err(e) => return Err(anyhow!("Failed to build workflow: {}", e)),
        };

        self.apply_workflow_components(
            workflow.sub_workflows.clone(),
            workflow.response.clone(),
            true,
        )
        .await;

        let prompt = [workflow.instructions.as_deref(), workflow.prompt.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("\n\n");

        Ok(Some(Message::user().with_text(prompt)))
    }
}
