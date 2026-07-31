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

/// Why a child's turn ended early: `(wire code, human-readable message)`.
type TurnAbort = (String, String);

type AgentMessagesFuture =
    Pin<Box<dyn Future<Output = Result<(Conversation, Option<String>, Option<TurnAbort>)>> + Send>>;

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

    let (messages, final_output, aborted) = match get_agent_messages(
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

    // An aborted turn is a failure even though the loop left a perfectly
    // well-formed assistant message behind explaining it. Deciding this here,
    // rather than letting `from_conversation` read that message as a summary, is
    // what keeps a subagent that never ran from reporting `completed`.
    let mut result = match aborted {
        Some((code, message)) => SubagentResult::from_aborted_turn(&messages, &code, message),
        None => SubagentResult::from_conversation(&messages, final_output, return_last_only),
    };
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

/// BR-71 §4.4: persist the child's rendered spawn context as its first message
/// — user_visible (the tab header shows it), agent_visible: false (the child's
/// model context already receives it as the system override; storing it
/// visibly must not double-inject it). Also stamps parent_session_id. The
/// record carries ALL grants the issue names — extensions, skills, and the
/// knowledge bases — so `workspace_read_conversation view:"spawn_context"` and
/// the tab header can show them without a second source of truth.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn persist_spawn_context(
    session_manager: &SessionManager,
    child_session_id: &str,
    parent_session_id: &str,
    rendered_system_prompt: &str,
    task_instructions: &str,
    extension_names: &[String],
    skill_names: &[String],
    knowledge_bases: &[String],
) -> Result<()> {
    use crate::conversation::message::{MessageProvenance, ProvenanceKind};

    session_manager
        .update(child_session_id)
        .parent_session_id(Some(parent_session_id.to_string()))
        .apply()
        .await?;

    let body = format!(
        "## Subagent spawn context\n\nSpawned by session: {parent_session_id}\n\n\
         ### Task instructions\n{task_instructions}\n\n\
         ### Granted extensions\n{}\n\n\
         ### Granted skills\n{}\n\n\
         ### Knowledge bases\n{}\n\n\
         ### Rendered system prompt\n{rendered_system_prompt}",
        if extension_names.is_empty() {
            "(parent defaults)".to_string()
        } else {
            extension_names.join(", ")
        },
        if skill_names.is_empty() {
            "(none)".to_string()
        } else {
            skill_names.join(", ")
        },
        if knowledge_bases.is_empty() {
            "(none)".to_string()
        } else {
            knowledge_bases.join(", ")
        },
    );
    let mut record = Message::user().with_text(body);
    record.metadata.user_visible = true;
    record.metadata.agent_visible = false;
    // DELIBERATELY NOT `.pinned()`, and this is the product decision the
    // 2026-07-28 amendment owes the reader (Task 14 pins its `note`; this record
    // does not, and the difference is not an oversight):
    //
    // `pin_is_eligible` (`context_mgmt::pins`) requires the message to be
    // AGENT-VISIBLE, and this one is `agent_visible: false` by design — it is a
    // transcript header for the human and the tab, not context for the child's
    // model, which already received all of it as its rendered system prompt. A
    // pin here would be inert: silently unhonoured, and misleading to the next
    // reader who assumes it does something.
    //
    // The child's own copy of this content therefore cannot be lost to
    // compaction, because it is not in the child's context to begin with.
    //
    // What keeps the stored ROW alive across a whole-history rewrite is NOT
    // #51's foreign-tail carry-over, and the earlier draft of this comment said
    // it was. That guard only covers rows ABOVE `basis.max_rowid` that `known`
    // does not name (see `RewriteBasis`, agent.rs); this record is written
    // before the child's first turn, so it is inside `known` and below the
    // watermark — the DELETE half of `replace_conversation_preserving_tail`
    // covers it, and it survives only because it is in the `replacement`.
    //
    // It is in the replacement because every compaction path RE-EMITS every
    // original message and only flips `agent_visible`: both branches of
    // `compact_messages_with_window` push `msg.clone()` for each input, and the
    // bottom rung of the recovery ladder, `drop_oldest_agent_visible_turns`,
    // `map`s rather than filters. No path deletes a row. Anyone changing
    // compaction to PRUNE instead of hide must give this record an explicit
    // carve-out — the store will not save it.
    //
    // If this record is ever made agent-visible, revisit the pin decision too —
    // at that point it becomes exactly the "one message a child must never
    // lose" case and should be pinned.
    record.metadata.provenance = Some(MessageProvenance {
        kind: ProvenanceKind::SpawnContext,
        from_session_id: Some(parent_session_id.to_string()),
        from_session_name: None,
    });
    session_manager
        .add_message_adopting_uid(child_session_id, &mut record)
        .await?;
    Ok(())
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

        // Prep binding 4: `config` is moved into `Agent::with_config` on the
        // next line, so the spawn-context record's session handle is taken now.
        let session_manager = config.session_manager.clone();
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

        // Prep binding 1: the loop below consumes `task_config.extensions` by
        // value, so the grant list for the spawn-context record is taken first.
        let extension_names: Vec<String> = task_config
            .extensions
            .iter()
            .map(|e| e.name().to_string())
            .collect();

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

        // Prep binding 2: the prompt context below moves `system_instructions`
        // into `task_instructions`.
        let task_instructions_for_record = system_instructions.clone();

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
        // Prep binding 3: `override_system_prompt` takes the template BY VALUE.
        let rendered_prompt = subagent_prompt.clone();
        agent.override_system_prompt(subagent_prompt).await;

        // BR-71 §4.4: record the child's spawn context as its first message,
        // before the reply stream starts. Grants for the record: extensions from
        // the task config; skills from the workflow; the child's knowledge bases
        // via the daemon services when installed (empty headless, where `get()`
        // returns `None`).
        //
        // This is NOT usually empty when the daemon is installed, contrary to an
        // earlier draft of this comment. `knowledge_selection` resolves to
        // `KnowledgeService::selection`, whose visible set is *every installed
        // base minus the hidden ones* (`selection_unlocked`), and a brand-new
        // child session has no `.hidden-kb-sessions/<digest>` file, so
        // `get_hidden_for_session_or_persisted` falls back to the machine-wide
        // hidden list. A child therefore inherits the machine's whole visible
        // set on its first read. That is the truth the record should carry — but
        // do not restate the old "a subagent inherits no KB" claim; it is wrong.
        //
        // The record names only the KB *set* — the primary is per-session mutable
        // state, not a grant, and recording a value that can change five minutes
        // later as part of an immutable spawn record is how a "source of truth"
        // starts lying.
        //
        // The read is dispatched to a blocking thread: the daemon implementation
        // takes `KnowledgeService`'s root `flock` and scans directories, so a
        // concurrent KB ingest macro holding that lock would otherwise park a
        // tokio worker for the length of the ingest, on every subagent spawn.
        let skill_names: Vec<String> = workflow.skills.clone().unwrap_or_default();
        let knowledge_bases = match crate::workspace_services::get() {
            Some(services) => {
                let kb_session_id = session_id.clone();
                tokio::task::spawn_blocking(move || {
                    services.knowledge_selection(&kb_session_id).kb_ids
                })
                .await
                .unwrap_or_default()
            }
            None => Vec::new(),
        };
        if let Err(e) = persist_spawn_context(
            &session_manager,
            &session_id,
            &task_config.parent_session_id,
            &rendered_prompt,
            &task_instructions_for_record,
            &extension_names,
            &skill_names,
            &knowledge_bases,
        )
        .await
        {
            // Best-effort, and the asymmetry with `create_subagent_session`'s
            // `?` on the SAME stamp is deliberate, not an accident of which
            // call site got a `?`:
            //
            // At birth nothing has been spent — no provider configured, no
            // extension loaded, no billed call — and `create_session` on that
            // same store failing already aborts the spawn, so a targeted UPDATE
            // failing one statement later means the store is gone and there is
            // nothing to salvage. Failing there costs an error message and
            // saves a permanently unparented row that no later path retries.
            //
            // Here the calculus is inverted: the provider and every extension
            // are already configured, the reply stream is one line away, and
            // the parent stamp is ALREADY durable from birth — so all that is
            // at risk is the transcript header. Killing a configured run to
            // save a header would be the worse trade.
            tracing::warn!("failed to persist subagent spawn context: {e}");
        }

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
            // Subagents run at the model's default depth; a parent turn's effort
            // is not inherited (its exploration caps are the parent's, not this
            // task's). BR-63.
            reasoning_effort: None,
        };

        let mut aborted: Option<TurnAbort> = None;
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
                Ok(AgentEvent::McpNotification(_))
                | Ok(AgentEvent::ModelChange { .. })
                | Ok(AgentEvent::ToolCallPending(_))
                // #59: the subagent's own rows are already carried by the
                // `Message` events above (which now name themselves); the
                // parent has no `expectedMessageIds` to satisfy.
                | Ok(AgentEvent::MessagesPersisted(_))
                | Ok(AgentEvent::TokenUsage(_)) => {}
                Ok(AgentEvent::HistoryReplaced(updated_conversation)) => {
                    conversation = updated_conversation;
                }
                Ok(AgentEvent::TurnAborted { code, message }) => {
                    // The subagent's turn failed. Its assistant Message (the
                    // human-readable "Ran into this error: …") is already in the
                    // conversation, so the parent still sees *what* happened —
                    // but as prose indistinguishable from a real summary. Carry
                    // the abort out so the envelope can say `error`.
                    tracing::error!(abort = code.wire_code(), "Subagent turn aborted: {message}");
                    aborted = Some((code.wire_code().to_string(), message));
                    break;
                }
                Err(e) => {
                    tracing::error!("Error receiving message from subagent: {}", e);
                    aborted = Some(("stream_error".to_string(), e.to_string()));
                    break;
                }
            }
        }

        // BR-28: the subagent is done — join its SubagentStart hook rather than
        // leaving the detached task to outlive the subagent and race shutdown.
        // The aggregate is keyed by the *parent* session (that is the payload's
        // session_id), which the child's own turn boundaries never drain, so
        // this is its only settle point. A subagent's stream is not user-visible,
        // so a `systemMessage` surfaces in the log; errors are already warned by
        // `dispatch`.
        for outcome in agent
            .hooks_manager()
            .settle_fired(
                &task_config.parent_session_id,
                crate::hooks::FIRE_JOIN_BUDGET_SHUTDOWN,
            )
            .await
        {
            for message in &outcome.aggregate.system_messages {
                info!("hooks: {} systemMessage: {}", outcome.event, message);
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

        Ok((conversation, final_output, aborted))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::ProvenanceKind;
    use crate::session::session_manager::SessionType;

    /// The body of one `### `-delimited section of the spawn record, so a grant
    /// can be asserted to be in the RIGHT section. Six bare `contains` checks
    /// pass just as happily on a record that renders the skills under
    /// "Granted extensions" and the extensions under "Granted skills".
    fn section<'a>(body: &'a str, heading: &str) -> &'a str {
        let start = body
            .find(heading)
            .unwrap_or_else(|| panic!("spawn record has no {heading} section:\n{body}"))
            + heading.len();
        let rest = &body[start..];
        match rest.find("\n### ") {
            Some(end) => &rest[..end],
            None => rest,
        }
    }

    #[tokio::test]
    async fn spawn_context_is_persisted_visible_to_user_not_agent() {
        let temp = tempfile::TempDir::new().unwrap();
        let sm = std::sync::Arc::new(SessionManager::new(temp.path().to_path_buf()));
        let child = sm
            .create_session(
                temp.path().to_path_buf(),
                "Subagent task".into(),
                SessionType::SubAgent,
            )
            .await
            .unwrap();

        persist_spawn_context(
            &sm,
            &child.id,
            "parent-1",
            "SYSTEM PROMPT RENDERED HERE",
            "task: count the files",
            &["developer".to_string()],
            &["single-cell".to_string()],
            &["kb-papers".to_string(), "kb-methods".to_string()],
        )
        .await
        .unwrap();

        let reread = sm.get_session(&child.id, true).await.unwrap();
        assert_eq!(reread.parent_session_id.as_deref(), Some("parent-1"));
        let msgs = reread.conversation.unwrap().messages().to_vec();
        // Exactly one row: the record is written once per spawn. Without this a
        // double-write would still leave a correct-looking first message.
        assert_eq!(
            msgs.len(),
            1,
            "one spawn call must write exactly one record, got {msgs:#?}"
        );
        let record = msgs.first().expect("spawn context is the first message");
        // `MessageMetadata::default()` is already `user_visible: true`, so this
        // assertion documents the requirement rather than discriminating; the
        // discriminating half of the pair is the `agent_visible` one below,
        // whose default is `true`.
        assert!(record.metadata.user_visible);
        assert!(
            !record.metadata.agent_visible,
            "must not enter the child's model context"
        );
        assert_eq!(
            record.metadata.provenance.as_ref().unwrap().kind,
            ProvenanceKind::SpawnContext
        );
        let text: String = record.content.iter().filter_map(|c| c.as_text()).collect();
        assert!(text.contains("SYSTEM PROMPT RENDERED HERE"));
        assert!(text.contains("count the files"));
        assert!(text.contains("developer"));
        // §4.5/issue: the record carries ALL grants — extensions, skills, KB.
        assert!(text.contains("single-cell"));
        assert!(text.contains("kb-papers"));
        // Issue #45: the record shows EVERY active base, not just the first.
        assert!(text.contains("kb-methods"));

        // …and each grant is under its OWN heading. The `contains` checks above
        // are satisfied by a record that files every grant in the wrong section.
        assert_eq!(
            section(&text, "### Task instructions").trim(),
            "task: count the files"
        );
        assert_eq!(section(&text, "### Granted extensions").trim(), "developer");
        assert_eq!(section(&text, "### Granted skills").trim(), "single-cell");
        assert_eq!(
            section(&text, "### Knowledge bases").trim(),
            "kb-papers, kb-methods"
        );
        assert_eq!(
            section(&text, "### Rendered system prompt").trim(),
            "SYSTEM PROMPT RENDERED HERE"
        );
        // The parent is named in the record body too, not only in `provenance`.
        assert!(text.contains("Spawned by session: parent-1"));
    }

    /// The empty-grant rendering is its own case: a spawn with no extensions
    /// must say "(parent defaults)", not silently render an empty section that
    /// reads as "no extensions were granted".
    #[tokio::test]
    async fn spawn_context_names_the_empty_grants_explicitly() {
        let temp = tempfile::TempDir::new().unwrap();
        let sm = std::sync::Arc::new(SessionManager::new(temp.path().to_path_buf()));
        let child = sm
            .create_session(
                temp.path().to_path_buf(),
                "Subagent task".into(),
                SessionType::SubAgent,
            )
            .await
            .unwrap();

        persist_spawn_context(
            &sm,
            &child.id,
            "parent-2",
            "PROMPT",
            "do a thing",
            &[],
            &[],
            &[],
        )
        .await
        .unwrap();

        let reread = sm.get_session(&child.id, true).await.unwrap();
        let msgs = reread.conversation.unwrap().messages().to_vec();
        let text: String = msgs[0].content.iter().filter_map(|c| c.as_text()).collect();
        assert_eq!(
            section(&text, "### Granted extensions").trim(),
            "(parent defaults)"
        );
        assert_eq!(section(&text, "### Granted skills").trim(), "(none)");
        assert_eq!(section(&text, "### Knowledge bases").trim(), "(none)");
    }
}
