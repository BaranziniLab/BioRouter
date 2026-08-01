//! Task 5 (issue #56): the private set, and the two demotion rules behind
//! [`Provider::tier`].
//!
//! Declared in `providers/mod.rs` as `#[cfg(test)] mod tier_tests;` — **not**
//! `include!`, or the filter `providers::tier_tests` resolves to nothing and
//! prints `0 passed`.

use std::sync::Arc;

use super::base::Provider;
use super::lead_worker::LeadWorkerProvider;
use super::ollama::{OllamaProvider, OLLAMA_HOST};
use super::versa_azure::VERSA_AZURE_ENDPOINT;
use crate::config::declarative_providers::{DeclarativeProviderConfig, ProviderEngine};
use crate::model::ModelConfig;
use crate::privacy::ProviderTier;

/// The tier a freshly-installed Biorouter publishes for `name`: the real
/// registry entry's metadata, which is exactly what the settings grid and every
/// other UI surface reads. A name with no entry gets the fail-safe default,
/// which is the whole point of the default being Public.
async fn tier_for_name_at_default_config(name: &str) -> ProviderTier {
    super::providers()
        .await
        .into_iter()
        .find(|(metadata, _)| metadata.name == name)
        .map(|(metadata, _)| metadata.tier)
        .unwrap_or_default()
}

/// Every registered provider whose shipped metadata claims Private.
async fn private_names_in_the_registry() -> Vec<String> {
    let mut names: Vec<String> = super::providers()
        .await
        .into_iter()
        .filter(|(metadata, _)| metadata.tier == ProviderTier::Private)
        .map(|(metadata, _)| metadata.name)
        .collect();
    names.sort();
    names
}

/// A **real** self-hosted-engine provider built the way a declarative JSON file
/// builds one, named and pointed wherever the caller says. Registering by
/// `config.name` after the built-ins is what lets one writable file shadow the
/// real registry entry, so the name is a parameter here on purpose.
fn self_hosted_named(name: &str, base_url: &str) -> OllamaProvider {
    let config = DeclarativeProviderConfig {
        name: name.to_string(),
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

fn tier_for_self_hosted_base(base_url: &str) -> ProviderTier {
    self_hosted_named("versa_azure", base_url).tier()
}

/// One half of the composite below, at the requested tier — a **real**
/// provider, not a mock.
///
/// A mock would have to override `tier()`, and this file would then hold a
/// seventh implementation of it — which is what Step 5's enumerating gate
/// lists. That gate names the six production implementations and would need a
/// human to read past a test one every run, so it is deliberately not spelled
/// out here either. A real Ollama-engine provider's tier is a
/// function of the base URL it resolved, so loopback yields Private and a
/// remote host yields Public — pinned independently by
/// `a_self_hosted_provider_pointed_off_the_machine_is_not_private` below, and
/// asserted here too so a break in that mapping is named rather than
/// mis-attributed to the composite rule.
fn half(name: &str, tier: ProviderTier) -> Arc<dyn Provider> {
    let base_url = match tier {
        ProviderTier::Private => "http://localhost:11434",
        ProviderTier::Public => "https://api.example-saas.com",
    };
    let provider = self_hosted_named(name, base_url);
    assert_eq!(
        provider.tier(),
        tier,
        "the {name} half must actually resolve {tier:?}"
    );
    Arc::new(provider)
}

/// The real production composite, with a lead named `versa_azure`.
fn lead_worker_with_tiers(lead: ProviderTier, worker: ProviderTier) -> LeadWorkerProvider {
    LeadWorkerProvider::new(half("versa_azure", lead), half("anthropic", worker), None)
}

/// The production predicate both versa providers hand their resolved endpoint.
fn versa_tier_for_endpoint(endpoint: &str) -> ProviderTier {
    super::ucsf_gateway_tier(endpoint)
}

#[tokio::test]
async fn the_private_set_is_the_four_the_operator_named() {
    use crate::privacy::ProviderTier::{Private, Public};

    let mut expected: Vec<&str> = vec!["llamacpp", "ollama", "versa_azure"];
    // Without the feature the provider is not compiled in, so it is not in the
    // registry and there is nothing to assert about it.
    #[cfg(feature = "aws-providers")]
    expected.push("versa_bedrock");
    expected.sort();

    for name in &expected {
        assert_eq!(
            tier_for_name_at_default_config(name).await,
            Private,
            "{name}"
        );
    }
    // Everything hosted by an AI company or a large cloud is public — including
    // the ones whose names look institutional. azure.rs ships the UCSF gateway
    // as AZURE_OPENAI_ENDPOINT's default, so a name-keyed rule would call
    // azure_openai Private; it must not.
    for name in [
        "anthropic",
        "openai",
        "azure_openai",
        "bedrock",
        "aws_bedrock",
        "databricks",
        "vertex",
        "google",
        "groq",
        "unknown_provider",
    ] {
        assert_eq!(
            tier_for_name_at_default_config(name).await,
            Public,
            "{name}"
        );
    }
    // ...and the set is closed: naming a *fifth* provider Private anywhere in
    // the tree fails here, which the loop above cannot do on its own.
    assert_eq!(private_names_in_the_registry().await, expected);

    // The registry above is the type-level claim every UI surface reads. Cross
    // -check it against a real instance at the shipped default host, where the
    // two must agree.
    assert_eq!(tier_for_self_hosted_base(OLLAMA_HOST), Private);
    // llamacpp's equivalent lives in `llamacpp.rs`, not here: `from_env` reads
    // `LLAMACPP_EXTERNAL_HOST` from the developer's real config, so calling it
    // would assert Private on a default machine and fail on one that
    // legitimately points at a lab box. That test covers all three arms from a
    // struct literal instead of the one the current environment happens to
    // produce.
}

#[tokio::test]
async fn a_composite_takes_the_least_privileged_of_its_two_halves() {
    use crate::privacy::ProviderTier::{Private, Public};
    // get_name() on a composite returns the LEAD's name, so a name-keyed tier
    // would badge private-lead/public-worker Private — the exact inverse of R2.
    let lw = lead_worker_with_tiers(Private, Public);
    assert_eq!(lw.get_name(), "versa_azure"); // the lead's name
    assert_eq!(lw.tier(), Public); // least(), not the name
    assert_eq!(lead_worker_with_tiers(Private, Private).tier(), Private);
    assert_eq!(lead_worker_with_tiers(Public, Public).tier(), Public);
}

#[test]
fn a_self_hosted_provider_pointed_off_the_machine_is_not_private() {
    use crate::privacy::ProviderTier::{Private, Public};
    // Open question 5 rates this ergonomics. It is a live bypass: config.yaml
    // is agent-writable (§9.3 C1 concedes SecretGuard cannot stop `shell`
    // writing it), and a declarative provider file whose engine is Ollama
    // yields an OllamaProvider with an arbitrary base_url. See the two
    // `declarative_providers` rows in this task's Files table for the anchors —
    // they are NOT repeated here, because a line number inside a code comment
    // is a citation no gate can check and no reviewer re-verifies.
    // Anyone who can write one JSON file would otherwise mint a Private-tier
    // provider pointing anywhere.
    assert_eq!(tier_for_self_hosted_base("http://localhost:11434"), Private);
    assert_eq!(tier_for_self_hosted_base("http://127.0.0.1:11434"), Private);
    assert_eq!(tier_for_self_hosted_base("http://[::1]:11434"), Private);
    assert_eq!(
        tier_for_self_hosted_base("http://gpu.lab.ucsf.edu:11434"),
        Public
    );
    assert_eq!(
        tier_for_self_hosted_base("https://api.example-saas.com"),
        Public
    );
}

#[test]
fn only_a_loopback_host_reads_as_this_machine() {
    use super::is_loopback_host;
    // The predicate behind both self-hosted providers, at its edges. Every case
    // below is one config field away from the one above it, and the direction
    // each one errs in is the whole argument for the rule.
    assert!(is_loopback_host("http://localhost:11434"));
    assert!(is_loopback_host("http://127.0.0.1:11434"));
    assert!(is_loopback_host("http://[::1]:11434"));
    // Hosts are case-insensitive; `host_of` lower-cases before comparing.
    assert!(is_loopback_host("http://LOCALHOST:11434"));

    // RFC 6761 §6.3 reserves the `.localhost` TLD for loopback, and both macOS
    // and systemd-resolved honour it. Accepting it is a deliberate ergonomic
    // concession with a residual assumption attached: unlike `localhost`, which
    // is in every /etc/hosts, a `*.localhost` name is only loopback because a
    // resolver says so, and an attacker who controls DNS but not /etc/hosts
    // could answer for it. That is a strictly harder position than the one this
    // rule defends against (a single writable JSON file), so the arm stays —
    // but it is the first thing to remove if that ever stops being true.
    assert!(is_loopback_host("http://sidecar.localhost:11434"));
    // ...and only as a TLD. These are what an attacker actually writes:
    assert!(!is_loopback_host("http://localhost.evil.example/"));
    assert!(!is_loopback_host("http://notlocalhost/"));
    assert!(!is_loopback_host("http://127.0.0.1.evil.example/"));
    assert!(!is_loopback_host("http://gpu.lab.ucsf.edu:11434"));

    // Fail-SAFE, not fail-open. These three genuinely are this machine and
    // still read Public, which costs a user with an unusual setup a private
    // badge and costs nobody a transcript. Do not "fix" them by broadening the
    // predicate without re-reading the paragraph above: every spelling added
    // here is a spelling an attacker may also write.
    assert!(!is_loopback_host("http://127.0.0.2:11434"));
    assert!(!is_loopback_host("http://[::ffff:127.0.0.1]:11434"));
    // A bare host:port parses as a scheme with an opaque path and has no host
    // at all. `OllamaProvider` normalises one to `http://…` before storing it,
    // so this shape cannot reach `tier()` from that path — but the predicate
    // is the thing under test, and it answers Public for anything hostless.
    assert!(!is_loopback_host("localhost:11434"));
    assert!(!is_loopback_host(""));
}

#[test]
fn versa_demotes_when_its_endpoint_is_not_the_ucsf_gateway() {
    use crate::privacy::ProviderTier::{Private, Public};
    // versa_azure reads AZURE_OPENAI_ENDPOINT, the same key the public
    // azure_openai provider reads, and versa_bedrock falls back to
    // AWS_ENDPOINT_URL_BEDROCK_RUNTIME, which bedrock.rs sets PROCESS-GLOBALLY
    // with std::env::set_var. The shipped constants are asserted rather than
    // their text, so moving a default off the gateway fails here too.
    assert_eq!(versa_tier_for_endpoint(VERSA_AZURE_ENDPOINT), Private);
    assert_eq!(
        versa_tier_for_endpoint("https://unified-api.ucsf.edu/general"),
        Private
    );
    #[cfg(feature = "aws-providers")]
    assert_eq!(
        versa_tier_for_endpoint(super::versa_bedrock::VERSA_BEDROCK_DEFAULT_ENDPOINT),
        Private
    );
    assert_eq!(
        versa_tier_for_endpoint("https://unified-api.ucsf.edu/general/awsai"),
        Private
    );
    assert_eq!(
        versa_tier_for_endpoint("https://evil.example.com/general"),
        Public
    );
    // The comparison is on the HOST, so a name that merely contains the
    // gateway's does not pass. Both of these are one typo away in a config
    // field a user can edit.
    assert_eq!(
        versa_tier_for_endpoint("https://unified-api.ucsf.edu.evil.example/general"),
        Public
    );
    assert_eq!(
        versa_tier_for_endpoint("https://evil.example/unified-api.ucsf.edu"),
        Public
    );
    // ...and hosts are case-insensitive, so the shipped host in a different
    // case is still the gateway rather than a silent demotion.
    assert_eq!(
        versa_tier_for_endpoint("https://UNIFIED-API.UCSF.EDU/general"),
        Private
    );
    // Anything `url::Url` cannot parse has no host to vouch for. Public.
    assert_eq!(versa_tier_for_endpoint("unified-api.ucsf.edu"), Public);
    assert_eq!(versa_tier_for_endpoint(""), Public);
}
