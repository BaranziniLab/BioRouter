//! Issue #56 Task 31 — the workflow load-time provider check.
//!
//! A workflow YAML is a **shared artefact**. It is written once, mailed around a
//! lab, and pins `settings.biorouter_provider` to whatever the author happened
//! to be using — very often `anthropic`. Before this feature that pin was inert
//! policy; with Gate A live it is a wall, and the wall lands in the worst
//! possible place: the workflow starts, the session binds, and the first turn
//! comes back a refusal that names the *model* and says nothing about the
//! workflow that chose it.
//!
//! So the pin is checked when the workflow is **loaded**, before a provider is
//! constructed and before anything is sent, and the refusal names the workflow's
//! own pin. The check is two functions rather than one so that the part with the
//! sentence in it is pure and table-testable, and only the registry lookup is
//! async.
//!
//! ⚠ **This is the DECLARED tier, not an instance's.** [`declared_provider_tier`]
//! reads `ProviderMetadata::tier` — the tier the provider ships at — because at
//! load time there is no instance to ask. The two can disagree (`ollama`
//! re-pointed off the machine by `OLLAMA_HOST` ships Private and resolves
//! Public), and the disagreement is in the *permissive* direction here: a
//! workflow pinning a re-pointed `ollama` passes this check and is then refused
//! by Gate A on the bind, which reads the instance. That is the correct order —
//! this is an early, well-explained refusal, never the authority — and it is why
//! nothing downstream was removed.

use anyhow::{anyhow, Result};

use crate::privacy::{bind_allowed, privacy_tiers_enabled, ProviderTier, SessionClassification};
use crate::workflow::Settings;

/// The tier the provider **registry** publishes for `name`: the same value the
/// settings grid and every other UI surface reads.
///
/// An unknown name gets [`ProviderTier::default`], which is Public — the
/// fail-safe side. A typo'd pin therefore earns the explanation below rather
/// than a silent pass, and then fails again on `providers::create` with "Unknown
/// provider", which is the message that actually diagnoses a typo.
///
/// It lives here rather than beside `ProviderTier` because this is its only
/// caller-shaped question: *given a name on a page, what does this install think
/// it is?* Every enforcement path in the feature holds an instance and asks
/// `Provider::tier` instead.
pub async fn declared_provider_tier(name: &str) -> ProviderTier {
    crate::providers::providers()
        .await
        .into_iter()
        .find(|(metadata, _)| metadata.name == name)
        .map(|(metadata, _)| metadata.tier)
        .unwrap_or_default()
}

/// The sentence a workflow pinning `provider` earns in a session classified
/// `session`. `None` when the pin is fine.
///
/// Pure: no config, no registry, no session store — so the whole of its
/// behaviour is pinned by a table-driven test, and so a caller that has already
/// resolved a tier (the CLI, which resolves one for the *effective* provider
/// whatever its source) can reuse the wording without a second lookup.
///
/// §14.4: it names the pin and the tier and nothing else. A workflow's title,
/// its prompt and its parameters are all content.
pub fn workflow_provider_refusal(
    provider: &str,
    tier: ProviderTier,
    session: SessionClassification,
) -> Option<String> {
    if bind_allowed(tier, session) {
        return None;
    }
    Some(format!(
        "This workflow pins `{provider}`, which is a public model, and this is a private session: \
         a chat classified private may only run on a model hosted inside the institution. The \
         workflow was not started, nothing has been sent, and the chat is unchanged. Run it in a \
         new chat, or re-run this one with `--provider <a private model>` to override the pin."
    ))
}

/// The load-time check itself: refuse before anything is built.
///
/// DR-15's master opt-out is read here, directly, for the reason
/// `privacy::mod`'s doc gives — this is not a tool-call path, so there is no
/// admitted [`crate::privacy::CallCapability`] whose sample it could inherit.
///
/// A workflow that pins nothing is not a workflow that pins a public model:
/// `None` is `Ok`, and the provider then comes from the CLI flag, the saved
/// session or the global default, each of which the caller checks in its own
/// right.
pub async fn assert_workflow_provider_allowed(
    settings: Option<&Settings>,
    session: SessionClassification,
) -> Result<()> {
    if !privacy_tiers_enabled() {
        return Ok(());
    }
    let Some(provider) = settings.and_then(|s| s.biorouter_provider.as_deref()) else {
        return Ok(());
    };
    match workflow_provider_refusal(provider, declared_provider_tier(provider).await, session) {
        Some(text) => Err(anyhow!(text)),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privacy::{ProviderTier, SessionClassification};
    use crate::workflow::Settings;

    fn pinning(provider: &str) -> Settings {
        Settings {
            biorouter_provider: Some(provider.to_string()),
            biorouter_model: None,
            temperature: None,
        }
    }

    #[tokio::test]
    async fn a_workflow_pinning_a_public_provider_fails_at_load_not_mid_turn() {
        let err = assert_workflow_provider_allowed(
            Some(&pinning("anthropic")),
            SessionClassification::Private,
        )
        .await
        .expect_err("a shared workflow may not drag a private chat onto a public model");
        let err = err.to_string();
        assert!(err.contains("pins `anthropic`"), "{err}");
        assert!(err.contains("private session"), "{err}");
    }

    #[tokio::test]
    async fn the_three_permitted_combinations_are_untouched() {
        for (provider, session) in [
            ("versa_azure", SessionClassification::Private),
            ("versa_azure", SessionClassification::Public),
            ("anthropic", SessionClassification::Public),
        ] {
            assert!(
                assert_workflow_provider_allowed(Some(&pinning(provider)), session)
                    .await
                    .is_ok(),
                "{provider} in a {session:?} chat must run"
            );
        }
        assert!(
            assert_workflow_provider_allowed(None, SessionClassification::Private)
                .await
                .is_ok(),
            "a workflow that pins nothing is not a workflow that pins a public model"
        );
    }

    #[tokio::test]
    async fn the_registry_is_what_decides_the_tier() {
        assert_eq!(
            declared_provider_tier("versa_azure").await,
            ProviderTier::Private
        );
        assert_eq!(
            declared_provider_tier("anthropic").await,
            ProviderTier::Public
        );
        assert_eq!(
            declared_provider_tier("no-such-provider").await,
            ProviderTier::Public,
            "an unknown name must fail SAFE"
        );
    }

    #[test]
    fn the_refusal_is_pure_and_carries_no_workflow_content() {
        assert!(workflow_provider_refusal(
            "versa_azure",
            ProviderTier::Private,
            SessionClassification::Private
        )
        .is_none());
        let text = workflow_provider_refusal(
            "anthropic",
            ProviderTier::Public,
            SessionClassification::Private,
        )
        .expect("the one refused combination");
        assert!(text.contains("pins `anthropic`"), "{text}");
        assert!(text.contains("private session"), "{text}");
    }
}
