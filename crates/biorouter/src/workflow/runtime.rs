use std::collections::HashMap;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::agents::extension::ExtensionConfig;
use crate::agents::Agent;
use crate::prompt_template::render_global_file;
use crate::workflow::{Workflow, WorkflowKnowledgeBases};

use biorouter_mcp::knowledge::service::{KnowledgeService, PrimaryUpdate};

/// Workflows that declare a knowledge selection require the Knowledge
/// capability even when an older persisted workflow omitted it. This keeps
/// managed Meditation copies from losing their ingest path after capability
/// disablement became authoritative.
pub fn ensure_required_extensions(workflow: &Workflow, extensions: &mut Vec<ExtensionConfig>) {
    if workflow.knowledge_bases.is_none()
        || extensions
            .iter()
            .any(|config| config.name().eq_ignore_ascii_case("knowledge"))
    {
        return;
    }
    if let Some(target) = crate::agents::extension_manager::resolve_bundled_extension("knowledge") {
        extensions.push(target.into_config(
            "Read, search, validate, and update Biorouter knowledge bases".to_string(),
        ));
    }
}

/// Turn a workflow's declared knowledge selection into the session state the
/// knowledge service stores. The primary is never inferred from `visible`:
/// workflow authors must name a write target explicitly with `default`.
pub fn plan_knowledge_selection(
    selection: &WorkflowKnowledgeBases,
) -> (Vec<String>, Option<String>) {
    let mut visible = selection.visible.clone();
    if let Some(default) = selection.default.as_deref() {
        if !visible.iter().any(|id| id == default) {
            visible.push(default.to_string());
        }
    }
    visible.sort();
    visible.dedup();
    (visible, selection.default.clone())
}

/// Apply a workflow's knowledge selection before its first model call.
pub fn apply_knowledge_selection(
    service: &KnowledgeService,
    session_id: &str,
    workflow: &Workflow,
) -> Result<()> {
    let Some(selection) = workflow.knowledge_bases.as_ref() else {
        return Ok(());
    };

    let (visible, primary) = plan_knowledge_selection(selection);
    let primary = match primary.as_deref() {
        Some(id) => PrimaryUpdate::Set(id),
        None => PrimaryUpdate::Clear,
    };
    service
        .set_visible_kbs(Some(session_id), &visible, primary)
        .context("applying workflow knowledge-base selection")?;
    Ok(())
}

/// Install a workflow's components and return its rendered system-prompt
/// addition. Declared skills are resolved to their actual, session-enabled
/// bodies; naming a missing or disabled skill is an error rather than a hint
/// the model may silently ignore.
pub async fn apply_to_agent(
    agent: &Agent,
    session_id: &str,
    workflow: &Workflow,
    include_final_output_tool: bool,
) -> Result<Option<String>> {
    let prompt =
        prepare_prompt(agent.config.session_manager.as_ref(), session_id, workflow).await?;
    apply_prepared_to_agent(agent, workflow, include_final_output_tool, prompt.clone()).await;
    Ok(prompt)
}

/// Resolve every fallible prompt dependency without mutating the live agent.
pub async fn prepare_prompt(
    session_manager: &crate::session::SessionManager,
    session_id: &str,
    workflow: &Workflow,
) -> Result<Option<String>> {
    let skill_instructions = match workflow.skills.as_deref() {
        Some(skills) if !skills.is_empty() => {
            crate::agents::skills_extension::workflow_skill_instructions(
                session_manager,
                session_id,
                skills,
            )
            .await?
        }
        _ => String::new(),
    };

    let prompt = if workflow.instructions.is_none() && skill_instructions.is_empty() {
        None
    } else {
        let mut instructions = workflow.instructions.clone().unwrap_or_default();
        if !skill_instructions.is_empty() {
            if !instructions.is_empty() {
                instructions.push_str("\n\n");
            }
            instructions.push_str(&skill_instructions);
        }

        let mut context: HashMap<&str, Value> = HashMap::new();
        context.insert("workflow_instructions", Value::String(instructions));
        Some(
            render_global_file("desktop_workflow_instruction.md", &context)
                .context("rendering workflow instructions")?,
        )
    };

    Ok(prompt)
}

/// Commit a prompt that was already prepared successfully, then install the
/// workflow's in-memory tools. This stage has no fallible filesystem lookup.
pub async fn apply_prepared_to_agent(
    agent: &Agent,
    workflow: &Workflow,
    include_final_output_tool: bool,
    prompt: Option<String>,
) {
    agent
        .apply_workflow_components(
            workflow.sub_workflows.clone(),
            workflow.response.clone(),
            include_final_output_tool,
        )
        .await;
    agent.set_session_context_prompt(prompt).await;
}

/// Install a workflow into the session about to run it: its knowledge
/// selection, its components, and the prompt [`prepare_prompt`] already
/// resolved.
///
/// ⚠ Takes an ALREADY-PREPARED prompt rather than calling `prepare_prompt`
/// itself, and that split is load-bearing. `prepare_prompt` is the fallible
/// half — it resolves declared skills and errors when one is missing or
/// disabled — so a caller that persists the workflow to its session row runs it
/// *before* the persist, and a workflow naming a skill that is not installed
/// fails without leaving a half-armed session behind. Collapsing the two would
/// persist first and fail second.
///
/// Every headless surface that starts a workflow comes here. The CLI did not,
/// which is why `biorouter run --workflow` silently ignored `skills:` and
/// `knowledge_bases:` while the desktop and the scheduler honoured both.
pub async fn install_prepared(
    agent: &Agent,
    session_id: &str,
    workflow: &Workflow,
    include_final_output_tool: bool,
    prompt: Option<String>,
) -> Result<()> {
    let knowledge = biorouter_mcp::knowledge::service::KnowledgeService::new_default()?;
    apply_knowledge_selection(&knowledge, session_id, workflow)?;
    apply_prepared_to_agent(agent, workflow, include_final_output_tool, prompt).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knowledge_default_is_visible_but_never_inferred() {
        let declared = WorkflowKnowledgeBases {
            default: Some("soul".into()),
            visible: vec!["research".into(), "research".into()],
        };
        assert_eq!(
            plan_knowledge_selection(&declared),
            (
                vec!["research".to_string(), "soul".to_string()],
                Some("soul".into())
            )
        );

        let visible_only = WorkflowKnowledgeBases {
            default: None,
            visible: vec!["soul".into()],
        };
        assert_eq!(
            plan_knowledge_selection(&visible_only),
            (vec!["soul".to_string()], None)
        );
    }

    #[test]
    fn knowledge_workflows_recover_their_required_capability_without_duplicates() {
        let mut workflow = Workflow::builder()
            .title("Meditation")
            .description("test")
            .instructions("test")
            .build()
            .unwrap();
        workflow.knowledge_bases = Some(WorkflowKnowledgeBases {
            default: Some("soul".into()),
            visible: vec!["soul".into()],
        });
        let mut extensions = Vec::new();
        ensure_required_extensions(&workflow, &mut extensions);
        ensure_required_extensions(&workflow, &mut extensions);
        assert_eq!(
            extensions
                .iter()
                .filter(|config| config.name() == "knowledge")
                .count(),
            1
        );
        assert!(matches!(
            extensions.first(),
            Some(ExtensionConfig::Builtin { .. })
        ));
    }
}
