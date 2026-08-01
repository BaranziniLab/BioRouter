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

use crate::privacy::ProviderTier;

/// The compiled-in UCSF gateway host. `versa_azure` and `versa_bedrock` are
/// Private only while their resolved endpoint is on it.
pub(crate) const UCSF_GATEWAY_HOST: &str = "unified-api.ucsf.edu";

pub(crate) fn host_of(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()?
        .host_str()
        .map(str::to_ascii_lowercase)
}

/// True only for a loopback host. R1 makes "self-hosted" private; a
/// non-loopback host is not evidence of self-hosting, and treating it as such
/// turns one writable config key into a forged private badge.
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
