//! Chat-side handler for the `platform__ingest_conversation` tool.
//!
//! Lets the user, mid-conversation, fold chat history into a knowledge base by
//! "just saying the word". It resolves the target KB (existing / new / active),
//! loads the requested sessions (defaulting to the current one), and runs the
//! shared [`conversation_ingest`] pipeline. Normal chat-side ingestion uses the
//! agent's own provider; scheduled knowledge jobs prefer the target KB's default
//! model when one is configured.

use rmcp::model::{Content, ErrorCode, ErrorData};
use serde_json::Value;

use super::Agent;
use crate::knowledge::conversation_ingest::{ingest_conversation, ConversationIngestArgs};
use crate::knowledge::ProviderCompleter;
use crate::mcp_utils::ToolResult;
use crate::model::ModelConfig;
use crate::session::session_manager::{Session, SessionType};
use biorouter_mcp::knowledge::service::KnowledgeService;
use biorouter_mcp::knowledge::subagent::loop_::{Completer, SubAgentBounds};
use biorouter_mcp::knowledge::types::ModelRef;

impl Agent {
    pub async fn handle_ingest_conversation(
        &self,
        arguments: Value,
        session: &Session,
    ) -> ToolResult<Vec<Content>> {
        let svc = KnowledgeService::new_default().map_err(internal)?;

        // Which sessions? Default to the current one.
        let session_ids: Vec<String> = arguments
            .get("session_ids")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| s.as_str().map(str::to_string))
                    .collect()
            })
            .filter(|v: &Vec<String>| !v.is_empty())
            .unwrap_or_else(|| vec![session.id.clone()]);

        // Resolve target KB: explicit id → new-by-name → active.
        let kb_id = self
            .resolve_target_kb(&svc, &arguments)
            .map_err(invalid_params)?;

        // Load the sessions (with messages).
        let mut sessions = Vec::new();
        for sid in &session_ids {
            match self.config.session_manager.get_session(sid, true).await {
                Ok(s) => sessions.push(s),
                Err(e) => {
                    return Err(invalid_params(format!("session '{sid}' not found: {e}")));
                }
            }
        }

        let completer = self
            .conversation_ingest_completer(&svc, &kb_id, session)
            .await?;

        let result = ingest_conversation(
            &svc,
            ConversationIngestArgs {
                kb_id: kb_id.clone(),
                sessions,
                completer,
                focus: arguments
                    .get("focus")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                bounds: SubAgentBounds::default(),
                event_sink: None,
                cancel: None,
            },
        )
        .await
        .map_err(internal)?;

        Ok(vec![Content::text(format!(
            "Ingested {} conversation(s) into knowledge base '{}'. \
             Source id: {}, commit: {}, sub-agent steps: {}.",
            session_ids.len(),
            kb_id,
            result.source_id,
            result.commit_sha.chars().take(8).collect::<String>(),
            result.steps
        ))])
    }

    /// Resolve which KB to ingest into. Creates a new KB when `new_kb_name` is
    /// given; otherwise uses `kb_id`, else the active KB.
    fn resolve_target_kb(
        &self,
        svc: &KnowledgeService,
        arguments: &Value,
    ) -> anyhow::Result<String> {
        if let Some(name) = arguments.get("new_kb_name").and_then(|v| v.as_str()) {
            let name = name.trim();
            if name.is_empty() {
                anyhow::bail!("new_kb_name cannot be empty");
            }
            let id = slugify_kb_name(name);
            if id.is_empty() {
                anyhow::bail!("new_kb_name must contain letters or numbers");
            }
            // Create only if it does not already exist.
            if !svc.list_bases()?.iter().any(|b| b.id == id) {
                svc.create_base(&id, name, None)?;
            }
            return Ok(id);
        }
        if let Some(id) = arguments.get("kb_id").and_then(|v| v.as_str()) {
            let id = id.trim();
            if !svc.list_bases()?.iter().any(|b| b.id == id) {
                anyhow::bail!("knowledge base '{id}' does not exist");
            }
            return Ok(id.to_string());
        }
        if let Some(active) = svc.get_active_persisted()? {
            return Ok(active);
        }
        anyhow::bail!(
            "no target knowledge base: pass kb_id, new_kb_name, or set an active knowledge base first"
        )
    }

    async fn conversation_ingest_completer(
        &self,
        svc: &KnowledgeService,
        kb_id: &str,
        session: &Session,
    ) -> Result<Box<dyn Completer>, ErrorData> {
        if should_use_knowledge_default_model(session) {
            let manifest = svc.get_base(kb_id).map_err(internal)?;
            if let Some(model) = manifest.default_model {
                return build_model_ref_completer(&model).await.map_err(|e| {
                    internal(format!(
                        "the default knowledge model for '{kb_id}' could not be used: {e}"
                    ))
                });
            }
        }

        let provider = self.provider().await.map_err(|e| {
            internal(format!(
                "a model provider is required to digest conversations: {e}"
            ))
        })?;
        Ok(Box::new(ProviderCompleter::new(provider)))
    }
}

fn should_use_knowledge_default_model(session: &Session) -> bool {
    session.session_type == SessionType::Scheduled || session.schedule_id.is_some()
}

async fn build_model_ref_completer(model: &ModelRef) -> anyhow::Result<Box<dyn Completer>> {
    if biorouter_mcp::knowledge::test_mode::env_enabled() {
        return Ok(Box::new(
            biorouter_mcp::knowledge::test_mode::TestModeCompleter,
        ));
    }

    let model_config = ModelConfig::new(&model.model)?;
    let provider = crate::providers::create(&model.provider, model_config).await?;
    Ok(Box::new(ProviderCompleter::new(provider)))
}

/// Slugify a display name into a valid KB id (lowercase, a-z0-9-, no leading /
/// trailing / doubled dashes, ≤64 chars). Mirrors the service's own rule.
pub fn slugify_kb_name(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    trimmed.chars().take(64).collect::<String>()
}

fn internal(e: impl std::fmt::Display) -> ErrorData {
    ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
}

fn invalid_params(e: impl std::fmt::Display) -> ErrorData {
    ErrorData::new(ErrorCode::INVALID_PARAMS, e.to_string(), None)
}

#[cfg(test)]
mod tests {
    use super::{should_use_knowledge_default_model, slugify_kb_name};
    use crate::session::session_manager::{Session, SessionType};
    use std::path::PathBuf;

    #[test]
    fn slugify_produces_valid_ids() {
        assert_eq!(slugify_kb_name("My Research Notes!"), "my-research-notes");
        assert_eq!(slugify_kb_name("  Soul  "), "soul");
        assert_eq!(slugify_kb_name("a / b -- c"), "a-b-c");
        assert!(slugify_kb_name("***").is_empty());
    }

    #[test]
    fn knowledge_default_model_is_reserved_for_scheduled_contexts() {
        let user = test_session(SessionType::User, None);
        assert!(!should_use_knowledge_default_model(&user));

        let scheduled = test_session(SessionType::Scheduled, None);
        assert!(should_use_knowledge_default_model(&scheduled));

        let scheduled_by_id = test_session(SessionType::User, Some("daily-meditation"));
        assert!(should_use_knowledge_default_model(&scheduled_by_id));
    }

    fn test_session(session_type: SessionType, schedule_id: Option<&str>) -> Session {
        Session {
            id: "s".to_string(),
            working_dir: PathBuf::from("."),
            name: "Test".to_string(),
            user_set_name: false,
            session_type,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            extension_data: Default::default(),
            total_tokens: None,
            input_tokens: None,
            output_tokens: None,
            accumulated_total_tokens: None,
            accumulated_input_tokens: None,
            accumulated_output_tokens: None,
            schedule_id: schedule_id.map(ToOwned::to_owned),
            workflow: None,
            user_workflow_values: None,
            conversation: None,
            message_count: 0,
            provider_name: None,
            model_config: None,
            diverged_from: None,
            branch_point_msg_uid: None,
        }
    }
}
