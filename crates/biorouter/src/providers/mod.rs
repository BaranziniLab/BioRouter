#[cfg(test)]
mod affiliation_tests;
pub mod anthropic;
pub mod api_client;
pub mod auto_detect;
pub mod azure;
pub mod azureauth;
pub mod base;
#[cfg(feature = "aws-providers")]
pub mod bedrock;
pub mod canonical;
pub mod databricks;
pub mod embedding;
pub mod errors;
mod factory;
pub mod formats;
mod gcpauth;
pub mod gcpvertexai;
pub mod githubcopilot;
pub mod google;
pub mod lead_worker;
pub mod litellm;
pub mod llamacpp;
pub mod llamacpp_sidecar;
pub mod oauth;
pub mod ollama;
pub mod openai;
pub mod openrouter;
pub mod pricing;
pub mod provider_registry;
pub mod provider_test;
pub(crate) mod retry;
#[cfg(feature = "aws-providers")]
pub mod sagemaker_tgi;
pub mod snowflake;
pub mod testprovider;
pub mod tetrate;
#[cfg(test)]
mod tier_tests;
pub mod toolshim;
pub mod usage_estimator;
pub mod utils;
pub mod venice;
pub mod versa_azure;
#[cfg(feature = "aws-providers")]
pub mod versa_bedrock;
pub mod xai;
pub mod xiaomi_mimo;
pub mod zai;

pub use factory::{
    create, create_with_default_model, create_with_named_model, providers, refresh_custom_providers,
};
pub use retry::{retry_operation, RetryConfig};

use crate::privacy::affiliation::{InstitutionId, ModelAffiliation};
use crate::privacy::ProviderTier;

/// The compiled-in UCSF gateway host. `versa_azure` and `versa_bedrock` are
/// Private only while their resolved endpoint is on it.
pub(crate) const UCSF_GATEWAY_HOST: &str = "unified-api.ucsf.edu";

/// The institution whose agreements cover [`UCSF_GATEWAY_HOST`] (DR-26).
///
/// It sits beside the host, not beside a provider name, because the host is
/// what the decision is made on — see [`ucsf_gateway_affiliation`].
pub(crate) const UCSF_INSTITUTION: &str = "ucsf";

pub(crate) fn host_of(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()?
        .host_str()
        .map(str::to_ascii_lowercase)
}

/// True only for a loopback host. R1 makes "self-hosted" private; a
/// non-loopback host is not evidence of self-hosting, and treating it as such
/// turns one writable config key into a forged private badge.
///
/// Purely lexical, and deliberately narrow: `127.0.0.2`, `::ffff:127.0.0.1` and
/// a scheme-less `localhost:11434` all answer **false** despite being this
/// machine. That direction is fail-safe — it costs an unusual setup a badge and
/// costs nobody a transcript — and every spelling added here is a spelling an
/// attacker may also write. The edges, and the residual assumption behind the
/// `.localhost` arm, are pinned in `tier_tests::only_a_loopback_host_reads_as_this_machine`.
pub(crate) fn is_loopback_host(url: &str) -> bool {
    match host_of(url).as_deref() {
        Some("localhost") | Some("127.0.0.1") | Some("::1") | Some("[::1]") => true,
        // RFC 6761 reserves `.localhost` for loopback. Note this does not match
        // `localhost.evil.example`, which has no leading dot before the label.
        Some(h) => h.ends_with(".localhost"),
        None => false,
    }
}

/// The tier of a provider that reaches the UCSF gateway and nothing else.
///
/// Demotion only, never promotion: `versa_azure` shares all three
/// `AZURE_OPENAI_*` keys with the public `azure_openai` provider, and
/// `bedrock.rs` sets `AWS_ENDPOINT_URL_BEDROCK_RUNTIME` process-globally, so an
/// endpoint that is not the gateway means the transcript is going somewhere
/// this build cannot vouch for.
pub(crate) fn ucsf_gateway_tier(endpoint: &str) -> ProviderTier {
    if host_of(endpoint).as_deref() == Some(UCSF_GATEWAY_HOST) {
        ProviderTier::Private
    } else {
        ProviderTier::Public
    }
}

/// The tier of a provider whose inference is supposed to run on this machine.
/// Private exactly while the resolved base URL is loopback.
pub(crate) fn self_hosted_tier(base_url: &str) -> ProviderTier {
    if is_loopback_host(base_url) {
        ProviderTier::Private
    } else {
        ProviderTier::Public
    }
}

/// The affiliation of a provider that reaches the UCSF gateway and nothing else
/// (DR-26). `Institution("ucsf")` exactly while [`ucsf_gateway_tier`] says
/// Private, and `None` otherwise.
///
/// ⚠ **It routes through the tier predicate rather than repeating its host
/// check**, so the two cannot disagree for any endpoint — not merely for the
/// ones a test thought to list. A name-keyed table (`versa_* => ucsf`) would
/// keep claiming the institution for a Versa module repointed at another host,
/// which `tier()` had already demoted to Public: a private-looking badge on a
/// public flow. The three `AZURE_OPENAI_*` keys are shared with the public
/// `azure_openai` provider and `bedrock.rs` sets
/// `AWS_ENDPOINT_URL_BEDROCK_RUNTIME` process-globally, so that repointing is a
/// config edit away.
pub(crate) fn ucsf_gateway_affiliation(endpoint: &str) -> Option<ModelAffiliation> {
    match ucsf_gateway_tier(endpoint) {
        ProviderTier::Private => Some(ModelAffiliation::Institution(InstitutionId::new(
            UCSF_INSTITUTION,
        ))),
        ProviderTier::Public => None,
    }
}

/// The affiliation of a provider whose inference is supposed to run on this
/// machine. [`ModelAffiliation::Local`] exactly while [`self_hosted_tier`] says
/// Private — i.e. while [`is_loopback_host`] holds.
///
/// ⚠ **A remote base URL yields `None`, not `Local`.** `OLLAMA_HOST` and
/// `LLAMACPP_EXTERNAL_HOST` are user-writable and need no auth, so a
/// non-loopback host is someone else's server: it is neither this machine nor an
/// institution this build can name. `Local` is the *most* permissive affiliation
/// in DR-26's model — it reaches every private extension, because no transfer
/// occurs at all — and inheriting it for a lab box in another building would
/// hand that box the blanket permission.
///
/// Like the tier above, it reuses [`is_loopback_host`] transitively rather than
/// re-deriving it. That predicate is deliberately narrow and lexical
/// (`127.0.0.2` and `::ffff:127.0.0.1` are **false**); a "more correct" second
/// copy would silently widen who counts as local.
pub(crate) fn self_hosted_affiliation(base_url: &str) -> Option<ModelAffiliation> {
    match self_hosted_tier(base_url) {
        ProviderTier::Private => Some(ModelAffiliation::Local),
        ProviderTier::Public => None,
    }
}
