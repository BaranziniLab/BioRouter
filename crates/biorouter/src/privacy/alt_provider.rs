//! Gate H (issue #56): the alternate-provider barrier.
//!
//! Three production paths hand a **session's** content to a provider the session
//! row never records, and none of them passes `Agent::update_provider` or
//! `Agent::reply` — so Gates A–F are all blind to them:
//!
//! 1. **CLI plan mode** — `get_reasoner` (`biorouter-cli/src/session/mod.rs`)
//!    clones the whole message list and completes it on a provider named by
//!    `BIOROUTER_PLANNER_PROVIDER`, or, failing that, the global default.
//! 2. **Prompt hooks** — `resolve_prompt_provider` (`hooks/mod.rs`) builds a
//!    provider from a hook definition's `provider:`/`model:` pair, and the Stop
//!    hook's payload carries `transcript_tail(&conversation)`, i.e. real
//!    transcript text, at the end of every turn.
//! 3. **The knowledge default model** — `build_model_ref_completer`
//!    (`agents/knowledge_tool.rs`) prefers a target KB's `default_model` over
//!    the agent's own provider for scheduled jobs.
//!
//! A fourth site, the HTTP knowledge macros (`routes/knowledge.rs`'s
//! `build_completer`), is deliberately **not** here: it has a knowledge-base id
//! and no session at all, so the predicate it needs is the KB-keyed one Task 10C
//! installs, not this session-keyed one. Putting a `SessionClassification` check
//! there would be a type error, not merely a misfiling.

use anyhow::{anyhow, Result};

use super::{bind_allowed, SessionClassification};
use crate::providers::base::Provider;

/// Refuse to hand a session's content to a provider that is not the one bound to
/// it.
///
/// * `what` names the feature, in the sentence the user reads ("plan mode",
///   "this prompt hook").
/// * `session` is the classification of the session whose content is about to
///   travel — its **stored** tier, not the tier of whatever is bound right now:
///   the whole point of this gate is that nothing is bound to the provider being
///   built here.
/// * `env_key_to_name` is the knob that fixes it, named so the refusal is a
///   redirection rather than a dead end.
///
/// The predicate is [`bind_allowed`], unchanged and unduplicated: "may this
/// provider see this session's contents" is one question, and a second spelling
/// of it is a second answer waiting to drift.
pub fn assert_alt_provider_allowed(
    what: &str,
    provider: &dyn Provider,
    session: SessionClassification,
    env_key_to_name: &str,
) -> Result<()> {
    if bind_allowed(provider.tier(), session) {
        return Ok(());
    }
    Err(anyhow!(
        "This chat is private, so {what} cannot run on `{}`, which is a public model. \
         Set {env_key_to_name} to a private model, or start this work in a public chat.",
        provider.get_name()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::declarative_providers::{DeclarativeProviderConfig, ProviderEngine};
    use crate::model::ModelConfig;
    use crate::privacy::ProviderTier;
    use crate::providers::ollama::OllamaProvider;

    /// A **real** provider at the requested tier, built the way a declarative
    /// JSON file builds one. Not a mock: `tier()` has to be the production
    /// implementation, or this file asserts something about a stub.
    fn provider_at(tier: ProviderTier) -> OllamaProvider {
        let base_url = match tier {
            ProviderTier::Private => "http://localhost:11434",
            ProviderTier::Public => "https://api.example-saas.com",
        };
        let config = DeclarativeProviderConfig {
            name: "planner".to_string(),
            engine: ProviderEngine::Ollama,
            display_name: "Planner".to_string(),
            description: None,
            api_key_env: "NOT_USED".to_string(),
            base_url: base_url.to_string(),
            models: vec![],
            headers: None,
            timeout_seconds: None,
            supports_streaming: None,
        };
        let provider =
            OllamaProvider::from_custom_config(ModelConfig::new_or_fail("qwen3"), config)
                .expect("a declarative ollama provider must construct");
        assert_eq!(provider.tier(), tier, "the fixture must resolve {tier:?}");
        provider
    }

    #[test]
    fn only_a_private_session_on_a_public_provider_is_refused() {
        use SessionClassification::{Private, Public};

        // The one refused combination, and the message has to carry both the
        // reason and the fix — a refusal that names neither is indistinguishable
        // from a broken feature.
        let err = assert_alt_provider_allowed(
            "plan mode",
            &provider_at(ProviderTier::Public),
            Private,
            "BIOROUTER_PLANNER_PROVIDER",
        )
        .expect_err("a private chat on a public model must be refused")
        .to_string();
        assert!(err.contains("private"), "{err}");
        assert!(err.contains("plan mode"), "{err}");
        assert!(err.contains("planner"), "the provider must be named: {err}");
        assert!(err.contains("BIOROUTER_PLANNER_PROVIDER"), "{err}");

        // ...and the other three are allowed, or this is not a barrier but an
        // outage. A public session is unaffected in BOTH directions: upward is
        // Gate A's business, not this one's.
        for (session, tier) in [
            (Private, ProviderTier::Private),
            (Public, ProviderTier::Public),
            (Public, ProviderTier::Private),
        ] {
            assert!(
                assert_alt_provider_allowed("plan mode", &provider_at(tier), session, "KEY")
                    .is_ok(),
                "{session:?} + {tier:?} must be allowed"
            );
        }
    }
}
