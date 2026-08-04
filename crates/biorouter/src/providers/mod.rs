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

use std::sync::LazyLock;

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
        ProviderTier::Private => Some(ModelAffiliation::Institution(*UCSF_ID)),
        ProviderTier::Public => None,
    }
}

/// The two ids this module can produce, interned once each.
///
/// [`InstitutionId::new`] normalises and then takes the **process-wide** interner
/// mutex (`privacy::affiliation::INTERNER`). That is fine for a cold constructor
/// and wrong for a decider: Task 48 samples affiliation once per call, so under a
/// parallel tool batch every provider in the process would queue on one lock to
/// re-derive a constant. The value is identical either way — `InstitutionId`
/// compares by string contents, not by pointer, which
/// `ids_with_equal_contents_but_distinct_pointers_are_equal` pins — so this is a
/// lock removed, not a semantic change, and the existing assertions against
/// `InstitutionId::new("ucsf")` are what would catch a wrong one.
static UCSF_ID: LazyLock<InstitutionId> = LazyLock::new(|| InstitutionId::new(UCSF_INSTITUTION));

/// See [`UCSF_ID`]. Interned on the same terms, for the same reason.
static SPANS_INSTITUTIONS_ID: LazyLock<InstitutionId> =
    LazyLock::new(|| InstitutionId::new(SPANS_INSTITUTIONS));

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

/// A placeholder institution, and **not** a real one: what a composite spanning
/// two *different* institutions resolves to.
///
/// ⚠ **It is unreachable in this build**, and that is pinned by
/// `factory::this_build_knows_exactly_one_institution` rather than assumed —
/// `UCSF_INSTITUTION` is the tree's only institution, so no lead/worker pair can
/// span two. It exists because [`composite_affiliation`] must be total, and
/// every *representable* alternative for that arm fails open: `None` is
/// specified to mean "a public model, affiliation never applies";
/// [`ModelAffiliation::Local`] reaches every private extension; and naming
/// either half's institution silently clears the flow to the other one. This
/// value clears only
/// [`ExtensionAffiliation::Any`](crate::privacy::affiliation::ExtensionAffiliation::Any)
/// — an extension with no institutional claim, which is genuinely unaffected —
/// and warns for every named institution, which is the safe direction.
///
/// ⚠ **It is not the answer, only the safe direction.** DR-26's model side is
/// `Local | Institution(id) | none`, with no value meaning "covered by two
/// institutions at once"; the correct encoding is a *set* with subset-of-the-
/// allowlist semantics, which `ModelAffiliation` cannot hold while it is `Copy`
/// (Task 45 rests `CallCapability`'s `Copy` derive on that). Settling that is an
/// operator ruling on DR-26, and the pin above forces it before a second
/// institution can reach this arm.
pub(crate) const SPANS_INSTITUTIONS: &str = "<spans-institutions>";

/// The affiliation of a **composite** provider — DR-26's third axis for the
/// tree's one lead/worker pair, and the sibling of [`ProviderTier::least`].
///
/// A composite discloses the whole transcript to *both* endpoints, so it may
/// reach an extension only where both halves may: the answer is the **meet** of
/// the two, never the lead's. Anything keyed on `get_name()` returns the lead's
/// alone (`LeadWorkerProvider::get_name`), which is the same mistake `tier()`
/// already had to override for.
///
/// ⚠ [`ModelAffiliation::Local`] is the **identity** of this fold, not an
/// absorbing element. That is DR-26's inversion arriving on a second axis: a
/// local half discloses nothing, so it neither dilutes the institution the other
/// half is covered by (which would forge reach it never had) nor overrides it
/// with `Local`'s blanket permission (which would grant the pair everything
/// private). Both wrong answers pass a fold written as equality.
///
/// ⚠ `None` from a half means that half is **Public** — affiliation is `Some`
/// exactly while a provider's tier is Private, which every affiliated provider
/// decides with the one predicate that decides its tier
/// (`ucsf_gateway_affiliation`, `self_hosted_affiliation`) and
/// `a_versa_endpoint_is_ucsf_exactly_when_it_is_private` pins. So a `None` half
/// has already made [`ProviderTier::least`] answer Public for the pair, and a
/// public model's affiliation is the absence of one. Returning the other half's
/// institution here would put a private-looking badge on a composite whose
/// transcript is going to a public endpoint.
pub(crate) fn composite_affiliation(
    lead: Option<ModelAffiliation>,
    worker: Option<ModelAffiliation>,
) -> Option<ModelAffiliation> {
    match (lead, worker) {
        // A public half. `least` has already demoted the pair to Public.
        (None, _) | (_, None) => None,
        // `Local` is the identity: a half that discloses nothing contributes
        // nothing to whose agreements cover the pair.
        (Some(ModelAffiliation::Local), other) => other,
        (other, Some(ModelAffiliation::Local)) => other,
        (Some(ModelAffiliation::Institution(a)), Some(ModelAffiliation::Institution(b))) => {
            if a == b {
                Some(ModelAffiliation::Institution(a))
            } else {
                // Unreachable in this build — see `SPANS_INSTITUTIONS`.
                Some(ModelAffiliation::Institution(*SPANS_INSTITUTIONS_ID))
            }
        }
    }
}
