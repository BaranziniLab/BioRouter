//! Task 46 (issue #56, DR-26): which institution's agreements cover the model a
//! session is bound to, and the two predicates that decide it.
//!
//! Affiliation is a **sibling of [`Provider::tier`]**, not a member of
//! `privacy/`: tier is a trait method on `Provider` whose deciding logic lives
//! in three free functions in `providers/mod.rs`, and affiliation answers the
//! second question — *under whose agreements?* — from the same inputs. The
//! vocabulary it answers in lives in `privacy::affiliation`; the *decision*
//! lives here, beside the tier decision it must never contradict.
//!
//! Declared in `providers/mod.rs` as `#[cfg(test)] mod affiliation_tests;` —
//! **not** `include!`, or the filter `providers::affiliation_tests` resolves to
//! nothing and prints `0 passed`.
//!
//! The **completeness** half of this task is deliberately not here. It
//! enumerates what `factory::register_builtin_providers` registers, so it sits
//! directly beneath that function in `providers/factory.rs`, where the person
//! adding a provider is already looking.

use async_trait::async_trait;
use rmcp::model::Tool;

use super::base::{Provider, ProviderMetadata, ProviderUsage};
use super::errors::ProviderError;
use super::ollama::{OllamaProvider, OLLAMA_HOST};
use super::versa_azure::VERSA_AZURE_ENDPOINT;
use super::{self_hosted_affiliation, ucsf_gateway_affiliation};
use crate::config::declarative_providers::{DeclarativeProviderConfig, ProviderEngine};
use crate::conversation::message::Message;
use crate::model::ModelConfig;
use crate::privacy::affiliation::{InstitutionId, ModelAffiliation};
use crate::privacy::ProviderTier;

fn ucsf() -> ModelAffiliation {
    ModelAffiliation::Institution(InstitutionId::new("ucsf"))
}

/// A **real** self-hosted-engine provider built the way a declarative JSON file
/// builds one, pointed wherever the caller says. The same construction
/// `tier_tests.rs` uses, and for the same reason: `from_env` would read the
/// developer's own `OLLAMA_HOST`, so a test through it asserts one thing on this
/// machine and another on a colleague's.
fn self_hosted_at(base_url: &str) -> OllamaProvider {
    let config = DeclarativeProviderConfig {
        name: "ollama".to_string(),
        engine: ProviderEngine::Ollama,
        display_name: "Not Versa At All".to_string(),
        description: None,
        api_key_env: "NOT_USED".to_string(),
        base_url: base_url.to_string(),
        models: vec![],
        headers: None,
        timeout_seconds: None,
        supports_streaming: None,
    };
    OllamaProvider::from_custom_config(ModelConfig::new_or_fail("qwen3"), config)
        .expect("a declarative ollama provider must construct")
}

// ------------------------------------------------------------------ Versa/UCSF

/// The assignment DR-26 names: both Versa providers are `Institution("ucsf")`.
///
/// Asserted against the **shipped constants**, not their text, so moving a
/// default endpoint off the gateway fails here rather than silently keeping the
/// badge.
#[test]
fn the_ucsf_gateway_is_what_makes_a_versa_provider_ucsf() {
    assert_eq!(ucsf_gateway_affiliation(VERSA_AZURE_ENDPOINT), Some(ucsf()));
    #[cfg(feature = "aws-providers")]
    assert_eq!(
        ucsf_gateway_affiliation(super::versa_bedrock::VERSA_BEDROCK_DEFAULT_ENDPOINT),
        Some(ucsf())
    );
    assert_eq!(
        ucsf_gateway_affiliation("https://unified-api.ucsf.edu/general/awsai"),
        Some(ucsf())
    );
    // Hosts are case-insensitive, exactly as the tier predicate treats them.
    assert_eq!(
        ucsf_gateway_affiliation("https://UNIFIED-API.UCSF.EDU/general"),
        Some(ucsf())
    );
}

/// The reason affiliation is derived from the gateway host rather than keyed on
/// the provider's name. A name-keyed table would keep claiming `ucsf` for a
/// `versa_azure` repointed at another host — while `tier()`, reading the
/// endpoint, had already demoted it to Public. The two would disagree, and the
/// disagreement is a private-looking badge on a public flow.
#[test]
fn a_versa_module_repointed_elsewhere_loses_ucsf_with_its_tier() {
    for elsewhere in [
        "https://evil.example.com/general",
        // One typo away in a user-editable config field, both directions.
        "https://unified-api.ucsf.edu.evil.example/general",
        "https://evil.example/unified-api.ucsf.edu",
        // Unparseable, so there is no host to vouch for.
        "unified-api.ucsf.edu",
        "",
    ] {
        assert_eq!(
            ucsf_gateway_affiliation(elsewhere),
            None,
            "{elsewhere} must not claim an institution"
        );
    }
}

/// Tier and affiliation are decided by **one** host check, so they cannot
/// disagree for any endpoint at all — not merely for the ones someone thought to
/// list above. Two independent implementations reading the same constant would
/// pass every test in this file and still drift the day one of them is edited.
#[test]
fn a_versa_endpoint_is_ucsf_exactly_when_it_is_private() {
    for endpoint in [
        VERSA_AZURE_ENDPOINT,
        "https://unified-api.ucsf.edu/general",
        "https://unified-api.ucsf.edu/general/awsai",
        "https://UNIFIED-API.UCSF.EDU/general",
        "https://unified-api.ucsf.edu.evil.example/general",
        "https://evil.example/unified-api.ucsf.edu",
        "https://bedrock-runtime.us-west-2.amazonaws.com",
        "http://localhost:11434",
        "unified-api.ucsf.edu",
        "",
    ] {
        let private = super::ucsf_gateway_tier(endpoint) == ProviderTier::Private;
        let affiliation = ucsf_gateway_affiliation(endpoint);
        assert_eq!(
            private,
            affiliation.is_some(),
            "{endpoint}: tier says private={private} but affiliation says {affiliation:?}"
        );
        if private {
            assert_eq!(affiliation, Some(ucsf()), "{endpoint}");
        }
    }
}

// ------------------------------------------------------------------- Self-hosted

/// `Local` is what a model running on this machine gets, and it is decided by
/// the same loopback predicate that decides the tier.
///
/// ⚠ These are **resolved base URLs**, not config values. The shipped
/// `OLLAMA_HOST` is the bare string `localhost`, which `is_loopback_host`
/// answers *false* for — it has no scheme, so `url::Url` parses it as an opaque
/// path with no host at all. Both `from_env` and `from_custom_config` normalise
/// it to `http://localhost:11434/` before storing, so the predicate never sees
/// the bare form; that the shipped default really does come out `Local` is
/// asserted through a real instance in
/// `a_real_ollama_instance_follows_the_base_url_it_resolved` below.
#[test]
fn inference_that_stays_on_this_machine_is_local() {
    for loopback in [
        "http://localhost:11434",
        "http://127.0.0.1:11434",
        "http://[::1]:11434",
        "http://LOCALHOST:11434",
        "http://sidecar.localhost:11543",
    ] {
        assert_eq!(
            self_hosted_affiliation(loopback),
            Some(ModelAffiliation::Local),
            "{loopback}"
        );
    }
}

/// A remote `OLLAMA_HOST` (or `LLAMACPP_EXTERNAL_HOST`) is **someone else's
/// server**. It is neither `Local` nor an `Institution`, and the first half of
/// that matters most: `Local` carries blanket permission over every private
/// extension, so inheriting it here would hand a lab box in another building the
/// most permissive affiliation in the model.
#[test]
fn a_self_hosted_provider_pointed_off_the_machine_is_not_local() {
    for remote in [
        "http://gpu.lab.ucsf.edu:11434",
        "https://api.example-saas.com",
        // The gateway itself: a Versa endpoint reached through the self-hosted
        // rule is still not this machine.
        "https://unified-api.ucsf.edu/general",
    ] {
        let affiliation = self_hosted_affiliation(remote);
        assert_ne!(
            affiliation,
            Some(ModelAffiliation::Local),
            "{remote} must not inherit Local's blanket permission"
        );
        assert_eq!(affiliation, None, "{remote}");
    }
}

/// The loopback predicate is **reused, not reimplemented**. It is deliberately
/// narrow and lexical, and a "more correct" second copy would silently widen who
/// counts as local — which is the one direction that grants reach. Every case
/// below genuinely is (or looks like) this machine and still answers `None`;
/// they are here so a reimplementation that "fixes" them fails.
#[test]
fn the_narrow_loopback_predicate_decides_local() {
    for not_local in [
        "http://127.0.0.2:11434",
        "http://[::ffff:127.0.0.1]:11434",
        "localhost:11434",
        "",
        "http://localhost.evil.example/",
        "http://notlocalhost/",
        "http://127.0.0.1.evil.example/",
    ] {
        assert_eq!(
            self_hosted_affiliation(not_local),
            None,
            "{not_local}: `is_loopback_host` answers false, so affiliation must too"
        );
        // ...and it agrees with the tier the same predicate produced.
        assert_eq!(super::self_hosted_tier(not_local), ProviderTier::Public);
    }
}

/// Wired, on a **real** `OllamaProvider` — Step 4's explicit requirement.
///
/// The predicate tests above cannot see whether the provider calls it, or hands
/// it the right field. `from_custom_config` builds an Ollama-engine provider
/// from a user-writable JSON file whose `base_url` points anywhere and whose
/// `name` can shadow a built-in, so an affiliation keyed on the name would badge
/// this instance `Local` while it posts prompts to a SaaS endpoint.
#[test]
fn a_real_ollama_instance_follows_the_base_url_it_resolved() {
    let here = self_hosted_at("http://localhost:11434");
    assert_eq!(here.affiliation(), Some(ModelAffiliation::Local));
    assert_eq!(here.tier(), ProviderTier::Private);

    // The shipped default, through construction — `OLLAMA_HOST` is the bare
    // `localhost`, which only becomes a loopback *URL* once the provider
    // normalises it. Asserted on the constant, not on its text, so moving the
    // default off this machine fails here.
    let shipped = self_hosted_at(OLLAMA_HOST);
    assert_eq!(shipped.affiliation(), Some(ModelAffiliation::Local));
    assert_eq!(shipped.tier(), ProviderTier::Private);

    let elsewhere = self_hosted_at("https://api.example-saas.com");
    // Same name, same metadata, same everything a name-keyed rule can see.
    assert_eq!(elsewhere.get_name(), here.get_name());
    assert_ne!(elsewhere.affiliation(), Some(ModelAffiliation::Local));
    assert_eq!(elsewhere.affiliation(), None);
    assert_eq!(elsewhere.tier(), ProviderTier::Public);
}

// -------------------------------------------------------------- The trait default

/// A provider that overrides nothing. Not a mock of anything — it exists so the
/// **default** can be asserted directly, which is the claim every public
/// provider in the registry rests on: they are unaffiliated because they never
/// say otherwise, not because someone wrote `None` twenty times.
struct ProviderThatSaysNothing;

#[async_trait]
impl Provider for ProviderThatSaysNothing {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::empty()
    }

    fn get_name(&self) -> &str {
        "says_nothing"
    }

    fn get_model_config(&self) -> ModelConfig {
        ModelConfig::new_or_fail("nothing")
    }

    async fn complete_with_model(
        &self,
        _model_config: &ModelConfig,
        _system: &str,
        _messages: &[Message],
        _tools: &[Tool],
    ) -> Result<(Message, ProviderUsage), ProviderError> {
        unreachable!("this provider exists to be asked its affiliation and nothing else")
    }
}

/// DEFAULT = no affiliation, matching `tier`'s DEFAULT = Public. Fail-safe in
/// the same direction: a provider module that forgets to say gets *less* reach,
/// never more. A default of `Local` would be catastrophic — it is the most
/// permissive value in the model — and a default of `Institution(..)` would
/// claim an agreement nobody signed.
#[test]
fn a_provider_that_says_nothing_has_no_affiliation() {
    assert_eq!(ProviderThatSaysNothing.affiliation(), None);
    assert_eq!(ProviderThatSaysNothing.tier(), ProviderTier::Public);
}
