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
use crate::privacy::ProviderTier;
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

        // Issue #56. The capability of the model *in this chat* — the audience
        // of every string this handler returns, including the candidate list
        // `resolve_target_kb` may put in its no-target error. Fail-closed to
        // Public when no provider is bound: less reach, never more.
        let chat_capability = self
            .provider()
            .await
            .map(|p| p.tier())
            .unwrap_or(ProviderTier::Public);

        // Resolve target KB: explicit id → new-by-name → this session's primary.
        let kb_id = resolve_target_kb(&svc, &arguments, &session.id, chat_capability)
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

        let (completer, caller_capability) = self
            .conversation_ingest_completer(&svc, &kb_id, session)
            .await?;

        let result = ingest_conversation(
            &svc,
            ConversationIngestArgs {
                kb_id: kb_id.clone(),
                // Issue #56. The tier of the provider this ingest will actually
                // run on — the KB's default model when a scheduled job names
                // one, otherwise this agent's own.
                caller_capability,
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

        Ok(vec![Content::text(ingest_summary(
            session_ids.len().saturating_sub(result.refused),
            result.refused,
            &kb_id,
            &result.ingested.source_id,
            &result.ingested.commit_sha,
            result.ingested.steps,
        ))])
    }

    /// The completer this ingest will run on, **and** the tier of the provider
    /// behind it (issue #56) — from `ProviderCompleter::paired`, so the two can
    /// never come from different providers.
    async fn conversation_ingest_completer(
        &self,
        svc: &KnowledgeService,
        kb_id: &str,
        session: &Session,
    ) -> Result<(Box<dyn Completer>, ProviderTier), ErrorData> {
        if should_use_knowledge_default_model(session) {
            let manifest = svc.get_base(kb_id).map_err(internal)?;
            if let Some(model) = manifest.default_model {
                return build_model_ref_completer(&model, session.privacy_tier)
                    .await
                    .map_err(|e| {
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
        let (completer, tier) = ProviderCompleter::paired(provider);
        Ok((Box::new(completer), tier))
    }
}

/// Resolve which KB a conversation ingest targets: `new_kb_name` creates one,
/// else an explicit `kb_id`, else **this session's primary**.
///
/// It must be the session's primary, not the machine-wide pointer: every other
/// surface writes session-scoped state, so reading the machine default here
/// sent a workflow/Meditation session's transcript into an unrelated base.
///
/// `caller` is the capability of the model that will read the error text
/// (issue #56). This function is `kb_id_or_primary`'s twin one crate over —
/// Task 10C's fix lives in `biorouter-mcp`'s `KnowledgeServer` and cannot reach
/// an `impl Agent` in `biorouter` — so it takes the same filter and the same
/// degrade.
pub(crate) fn resolve_target_kb(
    svc: &KnowledgeService,
    arguments: &Value,
    session_id: &str,
    caller: ProviderTier,
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
    if let Some(primary) = svc.primary_for_session(Some(session_id))? {
        return Ok(primary);
    }
    let ids: Vec<String> = svc
        .session_kb_ids(Some(session_id))?
        .into_iter()
        // Issue #56. Per id, and BEFORE the `is_empty` check below, so a chat
        // whose only base is private is told it has none rather than being
        // handed `(one of: )` — which is both useless and a tell. A barrier that
        // refuses a read and then hands over the identifier of the thing it
        // refused is not a barrier, and that identifier is the one argument that
        // makes the explicit-`kb_id` branch reachable.
        .filter(|id| caller.is_private() || !crate::knowledge::tier::is_private(svc.root(), id))
        .collect();
    if ids.is_empty() {
        anyhow::bail!(
            "no target knowledge base: this chat has none. Pass new_kb_name to create one, \
             or kb_id to name an existing base."
        );
    }
    anyhow::bail!(
        "no target knowledge base: pass kb_id (one of: {}) or new_kb_name, or call \
         kb_set_active to make one of them this chat's primary.",
        ids.join(", ")
    )
}

/// The success text for a conversation ingest. A KB-less write resolves its
/// target silently, so the result must name the base it landed in.
fn ingest_summary(
    session_count: usize,
    refused: usize,
    kb_id: &str,
    source_id: &str,
    commit_sha: &str,
    steps: usize,
) -> String {
    // Issue #56. A COUNT and nothing else — §11.4 classifies a session's id,
    // title and working directory as content, and this product's titles are
    // LLM-generated from the conversation itself.
    let refused_note = if refused == 0 {
        String::new()
    } else {
        format!(
            " {refused} private conversation(s) were skipped: this chat is running on a public \
             model. Ask the user to switch this chat to a private model to include them."
        )
    };
    format!(
        "Ingested {session_count} conversation(s) into knowledge base '{kb_id}'. \
         Source id: {source_id}, commit: {}, sub-agent steps: {steps}.{refused_note}",
        commit_sha.chars().take(8).collect::<String>()
    )
}

fn should_use_knowledge_default_model(session: &Session) -> bool {
    session.session_type == SessionType::Scheduled || session.schedule_id.is_some()
}

/// The completer behind a KB's `default_model`.
///
/// Issue #56 Gate H: `session` is the classification of the chat whose messages
/// this completer is about to digest. Required rather than optional — this
/// function's whole job is to build a provider the session row does not name, so
/// there is no bound provider for a later gate to consult.
async fn build_model_ref_completer(
    model: &ModelRef,
    session: crate::privacy::SessionClassification,
) -> anyhow::Result<(Box<dyn Completer>, ProviderTier)> {
    if biorouter_mcp::knowledge::test_mode::env_enabled() {
        // No provider exists on this path, so there is no instance to read a
        // tier from — the same fail-safe-for-a-ratchet reasoning as the two
        // `build_completer` test-mode branches. Nothing leaves the process on
        // this path either, so there is nothing for Gate H to refuse.
        return Ok((
            Box::new(biorouter_mcp::knowledge::test_mode::TestModeCompleter),
            ProviderTier::Public,
        ));
    }

    let model_config = ModelConfig::new(&model.model)?;
    let provider = crate::providers::create(&model.provider, model_config).await?;
    // AFTER `create`: the tier belongs to the instance that was resolved, not to
    // the name the manifest asked for. Constructing it discloses nothing.
    crate::privacy::assert_alt_provider_allowed(
        "digesting this conversation",
        provider.as_ref(),
        session,
        "the knowledge base's default model",
    )?;
    let (completer, tier) = ProviderCompleter::paired(provider);
    Ok((Box::new(completer), tier))
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
    use super::{
        ingest_summary, resolve_target_kb, should_use_knowledge_default_model, slugify_kb_name,
    };
    use crate::session::session_manager::{Session, SessionType};
    use biorouter_mcp::knowledge::service::KnowledgeService;
    use std::path::PathBuf;

    /// Pre-existing bug: the KB-less target came from the **machine-wide**
    /// `.active-kb`, while every other surface — the chat chip, kb_set_active,
    /// workflows, the apps platform — writes session-scoped state. A
    /// Meditation/workflow session whose KB was set per session therefore
    /// ingested into whatever the machine happened to point at.
    #[test]
    fn resolve_target_kb_uses_the_session_primary_not_the_machine_default() -> anyhow::Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let svc = KnowledgeService::new(tmp.path().to_path_buf());
        svc.create_base("machine-kb", "Machine", None)?;
        svc.create_base("session-kb", "Session", None)?;
        svc.set_primary_persisted(Some("machine-kb"))?;
        svc.set_primary_for_session("chat-1", Some("session-kb"))?;

        let args = serde_json::json!({});
        let public = crate::privacy::ProviderTier::Public;
        assert_eq!(
            resolve_target_kb(&svc, &args, "chat-1", public)?,
            "session-kb"
        );
        assert_eq!(
            resolve_target_kb(&svc, &args, "chat-2", public)?,
            "machine-kb",
            "a chat that never chose one still inherits the machine pointer"
        );

        svc.set_primary_persisted(None)?;
        let err = resolve_target_kb(&svc, &args, "chat-9", public)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("machine-kb, session-kb") && err.contains("kb_id"),
            "the error must list the candidates and the fix, got: {err}"
        );
        Ok(())
    }

    /// A KB-less write must name the base it wrote to, in the text the model
    /// and the user both read.
    #[test]
    fn ingest_summary_names_the_target_base() {
        let summary = ingest_summary(2, 0, "my-kb", "src-1", "abcdef1234567890", 7);
        assert!(summary.contains("'my-kb'"), "got: {summary}");
        assert!(summary.contains("abcdef12") && !summary.contains("abcdef123"));
        assert!(
            !summary.contains("skipped"),
            "a clean run must not mention a refusal: {summary}"
        );
    }

    /// Issue #56. When Gate G drops some of the requested chats, the model is
    /// told a COUNT and the fix — never an id, a title or a working directory
    /// (§11.4 classifies all three as content).
    #[test]
    fn ingest_summary_reports_refusals_as_a_count_and_names_nothing() {
        let summary = ingest_summary(1, 2, "my-kb", "src-1", "abcdef1234567890", 7);
        assert!(
            summary.contains("2 private conversation(s) were skipped"),
            "{summary}"
        );
        assert!(summary.contains("private model"), "{summary}");
    }

    #[test]
    fn slugify_produces_valid_ids() {
        assert_eq!(slugify_kb_name("My Research Notes!"), "my-research-notes");
        assert_eq!(slugify_kb_name("  Soul  "), "soul");
        assert_eq!(slugify_kb_name("a / b -- c"), "a-b-c");
        assert!(slugify_kb_name("***").is_empty());
    }

    /// Issue #56. `resolve_target_kb` is `kb_id_or_primary`'s twin one crate
    /// over, and Task 10C's fix cannot reach it: that one is in
    /// `biorouter-mcp`'s `KnowledgeServer`, this one is in `biorouter`'s
    /// `Agent`. Same rule — OMIT. A barrier that refuses a read and then hands
    /// the model the identifier of the thing it refused is not a barrier.
    #[test]
    fn the_no_target_error_names_only_the_bases_the_caller_may_reach() -> anyhow::Result<()> {
        use crate::privacy::ProviderTier;

        let tmp = tempfile::TempDir::new()?;
        let svc = KnowledgeService::new(tmp.path().to_path_buf());
        svc.create_base("default", "Default", None)?;
        svc.create_base("omop", "OMOP", None)?;
        crate::knowledge::tier::raise_unlocked(tmp.path(), "omop", true)?;

        let args = serde_json::json!({});
        let public = resolve_target_kb(&svc, &args, "chat-9", ProviderTier::Public)
            .unwrap_err()
            .to_string();
        assert!(
            public.contains("default"),
            "the public base must still be offered: {public}"
        );
        assert!(
            !public.contains("omop"),
            "the no-target error enumerated a private base: {public}"
        );

        // Both directions: a private model still sees both, or the filter is
        // just "refuse everyone" and the feature has quietly stopped working.
        let private = resolve_target_kb(&svc, &args, "chat-9", ProviderTier::Private)
            .unwrap_err()
            .to_string();
        assert!(
            private.contains("default") && private.contains("omop"),
            "a private model was denied its own bases: {private}"
        );
        Ok(())
    }

    /// The filter runs per id and BEFORE the emptiness check, so a chat whose
    /// only base is private is told it has none rather than handed
    /// `(one of: )` — which is both useless and a tell.
    #[test]
    fn a_chat_whose_only_base_is_private_is_told_it_has_none() -> anyhow::Result<()> {
        use crate::privacy::ProviderTier;

        let tmp = tempfile::TempDir::new()?;
        let svc = KnowledgeService::new(tmp.path().to_path_buf());
        svc.create_base("omop", "OMOP", None)?;
        crate::knowledge::tier::raise_unlocked(tmp.path(), "omop", true)?;

        let err = resolve_target_kb(&svc, &serde_json::json!({}), "chat-9", ProviderTier::Public)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("this chat has none"),
            "expected the no-bases branch, got: {err}"
        );
        assert!(!err.contains("omop"), "{err}");
        assert!(!err.contains("one of"), "an empty candidate list: {err}");
        Ok(())
    }

    /// Issue #56 Gate H. A scheduled knowledge job prefers the target KB's
    /// `default_model` over the agent's own provider, so the transcripts it
    /// digests go to a provider the session row never records — and
    /// `build_model_ref_completer` is reached from neither
    /// `Agent::update_provider` nor `Agent::reply`.
    #[tokio::test]
    async fn the_knowledge_default_model_obeys_the_barrier() {
        use super::build_model_ref_completer;
        use crate::privacy::SessionClassification;
        use biorouter_mcp::knowledge::types::ModelRef;
        use std::collections::HashMap;

        fn ollama_at(host: &str) -> HashMap<String, String> {
            HashMap::from([("OLLAMA_HOST".to_string(), host.to_string())])
        }
        let model = ModelRef {
            provider: "ollama".to_string(),
            model: "qwen3".to_string(),
        };

        let err = crate::config::with_config_overrides(
            // Not this machine ⇒ `tier()` says Public, from a real provider.
            ollama_at("https://api.example-saas.invalid"),
            build_model_ref_completer(&model, SessionClassification::Private),
        )
        .await
        // `Completer` is not `Debug`, so the Ok side cannot be unwrapped for a
        // message; match instead.
        .err()
        .expect("a private chat may not digest itself on a public model")
        .to_string();
        assert!(
            err.to_lowercase().contains("private"),
            "the refusal has to say why, got: {err}"
        );

        // Both directions, or the gate is just "refuse everyone".
        assert!(crate::config::with_config_overrides(
            ollama_at("http://localhost:11434"),
            build_model_ref_completer(&model, SessionClassification::Private),
        )
        .await
        .is_ok());
        assert!(crate::config::with_config_overrides(
            ollama_at("https://api.example-saas.invalid"),
            build_model_ref_completer(&model, SessionClassification::Public),
        )
        .await
        .is_ok());
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
            parent_session_id: None,
            privacy_tier: crate::privacy::SessionClassification::Public,
            privacy_reason: None,
        }
    }
}
