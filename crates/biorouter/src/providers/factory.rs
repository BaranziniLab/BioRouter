use std::sync::{Arc, RwLock};

use super::{
    anthropic::AnthropicProvider,
    azure::AzureProvider,
    base::{Provider, ProviderMetadata},
    databricks::DatabricksProvider,
    gcpvertexai::GcpVertexAIProvider,
    githubcopilot::GithubCopilotProvider,
    google::GoogleProvider,
    lead_worker::LeadWorkerProvider,
    litellm::LiteLLMProvider,
    llamacpp::LlamaCppProvider,
    ollama::OllamaProvider,
    openai::OpenAiProvider,
    openrouter::OpenRouterProvider,
    provider_registry::ProviderRegistry,
    snowflake::SnowflakeProvider,
    tetrate::TetrateProvider,
    venice::VeniceProvider,
    versa_azure::VersaAzureProvider,
    xai::XaiProvider,
    xiaomi_mimo::XiaomiMimoProvider,
    zai::ZaiProvider,
};
#[cfg(feature = "aws-providers")]
use super::{
    bedrock::BedrockProvider, sagemaker_tgi::SageMakerTgiProvider,
    versa_bedrock::VersaBedrockProvider,
};
use crate::model::ModelConfig;
use crate::providers::base::ProviderType;
use crate::{
    config::declarative_providers::register_declarative_providers,
    providers::provider_registry::ProviderEntry,
};
use anyhow::Result;
use tokio::sync::OnceCell;

const DEFAULT_LEAD_TURNS: usize = 3;
const DEFAULT_FAILURE_THRESHOLD: usize = 2;
const DEFAULT_FALLBACK_TURNS: usize = 2;

static REGISTRY: OnceCell<RwLock<ProviderRegistry>> = OnceCell::const_new();

async fn init_registry() -> RwLock<ProviderRegistry> {
    let mut registry = ProviderRegistry::new().with_providers(register_builtin_providers);
    if let Err(e) = load_custom_providers_into_registry(&mut registry) {
        tracing::warn!("Failed to load custom providers: {}", e);
    }
    RwLock::new(registry)
}

/// Every provider compiled into this binary, and the single place they are
/// declared.
///
/// ⚠ Extracted from `init_registry` so a test can enumerate exactly this set
/// without also picking up the bundled and user-written declarative providers
/// that `load_custom_providers_into_registry` adds — see
/// `tests::every_registered_provider_is_classified_for_affiliation`, which fails
/// until a provider added here is classified against DR-26's third axis.
fn register_builtin_providers(registry: &mut ProviderRegistry) {
    registry.register::<AnthropicProvider, _>(|m| Box::pin(AnthropicProvider::from_env(m)), true);
    registry.register::<AzureProvider, _>(|m| Box::pin(AzureProvider::from_env(m)), false);
    #[cfg(feature = "aws-providers")]
    registry.register::<BedrockProvider, _>(|m| Box::pin(BedrockProvider::from_env(m)), false);
    registry
        .register::<VersaAzureProvider, _>(|m| Box::pin(VersaAzureProvider::from_env(m)), false);
    #[cfg(feature = "aws-providers")]
    registry.register::<VersaBedrockProvider, _>(
        |m| Box::pin(VersaBedrockProvider::from_env(m)),
        false,
    );
    registry.register::<DatabricksProvider, _>(|m| Box::pin(DatabricksProvider::from_env(m)), true);
    registry
        .register::<GcpVertexAIProvider, _>(|m| Box::pin(GcpVertexAIProvider::from_env(m)), false);
    registry.register::<GithubCopilotProvider, _>(
        |m| Box::pin(GithubCopilotProvider::from_env(m)),
        false,
    );
    registry.register::<GoogleProvider, _>(|m| Box::pin(GoogleProvider::from_env(m)), true);
    registry.register::<LiteLLMProvider, _>(|m| Box::pin(LiteLLMProvider::from_env(m)), false);
    registry.register::<LlamaCppProvider, _>(|m| Box::pin(LlamaCppProvider::from_env(m)), true);
    registry.register::<OllamaProvider, _>(|m| Box::pin(OllamaProvider::from_env(m)), true);
    registry.register::<OpenAiProvider, _>(|m| Box::pin(OpenAiProvider::from_env(m)), true);
    registry.register::<OpenRouterProvider, _>(|m| Box::pin(OpenRouterProvider::from_env(m)), true);
    #[cfg(feature = "aws-providers")]
    registry.register::<SageMakerTgiProvider, _>(
        |m| Box::pin(SageMakerTgiProvider::from_env(m)),
        false,
    );
    registry.register::<SnowflakeProvider, _>(|m| Box::pin(SnowflakeProvider::from_env(m)), false);
    registry.register::<TetrateProvider, _>(|m| Box::pin(TetrateProvider::from_env(m)), true);
    registry.register::<VeniceProvider, _>(|m| Box::pin(VeniceProvider::from_env(m)), false);
    registry.register::<XaiProvider, _>(|m| Box::pin(XaiProvider::from_env(m)), false);
    registry
        .register::<XiaomiMimoProvider, _>(|m| Box::pin(XiaomiMimoProvider::from_env(m)), false);
    registry.register::<ZaiProvider, _>(|m| Box::pin(ZaiProvider::from_env(m)), false);
}

fn load_custom_providers_into_registry(registry: &mut ProviderRegistry) -> Result<()> {
    register_declarative_providers(registry)
}

async fn get_registry() -> &'static RwLock<ProviderRegistry> {
    REGISTRY.get_or_init(init_registry).await
}

pub async fn providers() -> Vec<(ProviderMetadata, ProviderType)> {
    get_registry()
        .await
        .read()
        .unwrap()
        .all_metadata_with_types()
}

pub async fn refresh_custom_providers() -> Result<()> {
    let registry = get_registry().await;
    registry.write().unwrap().remove_custom_providers();

    if let Err(e) = load_custom_providers_into_registry(&mut registry.write().unwrap()) {
        tracing::warn!("Failed to refresh custom providers: {}", e);
        return Err(e);
    }

    tracing::info!("Custom providers refreshed");
    Ok(())
}

async fn get_from_registry(name: &str) -> Result<ProviderEntry> {
    let guard = get_registry().await.read().unwrap();
    guard
        .entries
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("Unknown provider: {}", name))
        .cloned()
}

pub async fn create(name: &str, model: ModelConfig) -> Result<Arc<dyn Provider>> {
    let config = crate::config::Config::global();

    if let Ok(lead_model_name) = config.get_param::<String>("BIOROUTER_LEAD_MODEL") {
        tracing::info!("Creating lead/worker provider from environment variables");
        return create_lead_worker_from_env(name, &model, &lead_model_name).await;
    }

    let constructor = get_from_registry(name).await?.constructor.clone();
    constructor(model).await
}

pub async fn create_with_default_model(name: impl AsRef<str>) -> Result<Arc<dyn Provider>> {
    get_from_registry(name.as_ref())
        .await?
        .create_with_default_model()
        .await
}

pub async fn create_with_named_model(
    provider_name: &str,
    model_name: &str,
) -> Result<Arc<dyn Provider>> {
    let config = ModelConfig::new(model_name)?;
    create(provider_name, config).await
}

async fn create_lead_worker_from_env(
    default_provider_name: &str,
    default_model: &ModelConfig,
    lead_model_name: &str,
) -> Result<Arc<dyn Provider>> {
    let config = crate::config::Config::global();

    let lead_provider_name = config
        .get_param::<String>("BIOROUTER_LEAD_PROVIDER")
        .unwrap_or_else(|_| default_provider_name.to_string());

    let lead_turns = config
        .get_param::<usize>("BIOROUTER_LEAD_TURNS")
        .unwrap_or(DEFAULT_LEAD_TURNS);
    let failure_threshold = config
        .get_param::<usize>("BIOROUTER_LEAD_FAILURE_THRESHOLD")
        .unwrap_or(DEFAULT_FAILURE_THRESHOLD);
    let fallback_turns = config
        .get_param::<usize>("BIOROUTER_LEAD_FALLBACK_TURNS")
        .unwrap_or(DEFAULT_FALLBACK_TURNS);

    let lead_model_config = ModelConfig::new_with_context_env(
        lead_model_name.to_string(),
        Some("BIOROUTER_LEAD_CONTEXT_LIMIT"),
    )?;

    let worker_model_config = create_worker_model_config(default_model)?;

    let registry = get_registry().await;

    let lead_constructor = {
        let guard = registry.read().unwrap();
        guard
            .entries
            .get(&lead_provider_name)
            .ok_or_else(|| anyhow::anyhow!("Unknown provider: {}", lead_provider_name))?
            .constructor
            .clone()
    };

    let worker_constructor = {
        let guard = registry.read().unwrap();
        guard
            .entries
            .get(default_provider_name)
            .ok_or_else(|| anyhow::anyhow!("Unknown provider: {}", default_provider_name))?
            .constructor
            .clone()
    };

    let lead_provider = lead_constructor(lead_model_config).await?;
    let worker_provider = worker_constructor(worker_model_config).await?;

    Ok(Arc::new(LeadWorkerProvider::new_with_settings(
        lead_provider,
        worker_provider,
        lead_turns,
        failure_threshold,
        fallback_turns,
    )))
}

fn create_worker_model_config(default_model: &ModelConfig) -> Result<ModelConfig> {
    let mut worker_config = ModelConfig::new_or_fail(&default_model.model_name)
        .with_context_limit(default_model.context_limit)
        .with_temperature(default_model.temperature)
        .with_max_tokens(default_model.max_tokens)
        .with_toolshim(default_model.toolshim)
        .with_toolshim_model(default_model.toolshim_model.clone());

    let global_config = crate::config::Config::global();

    if let Ok(limit) = global_config.get_param::<usize>("BIOROUTER_WORKER_CONTEXT_LIMIT") {
        worker_config = worker_config.with_context_limit(Some(limit));
    } else if let Ok(limit) = global_config.get_param::<usize>("BIOROUTER_CONTEXT_LIMIT") {
        worker_config = worker_config.with_context_limit(Some(limit));
    }

    Ok(worker_config)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Every built-in provider whose **instances** can carry an affiliation
    /// (DR-26, Task 46), with the predicate that decides it. Nothing here is a
    /// name-keyed assignment: the name selects which predicate runs, the
    /// predicate reads what the instance actually resolved.
    #[allow(unused_mut)]
    fn affiliated_providers() -> Vec<(&'static str, &'static str)> {
        let mut rows = vec![
            (
                "llamacpp",
                "Local: the managed sidecar, or `self_hosted_affiliation` on LLAMACPP_EXTERNAL_HOST",
            ),
            (
                "ollama",
                "Local: `self_hosted_affiliation` on the resolved OLLAMA_HOST",
            ),
            (
                "versa_azure",
                "Institution(ucsf): `ucsf_gateway_affiliation` on the resolved AZURE_OPENAI_ENDPOINT",
            ),
        ];
        #[cfg(feature = "aws-providers")]
        rows.push((
            "versa_bedrock",
            "Institution(ucsf): `ucsf_gateway_affiliation` on the resolved bedrock endpoint",
        ));
        rows
    }

    /// Every other built-in provider, each with the one-line reason affiliation
    /// does not apply to it. These are not `None` by omission — the trait
    /// default *is* `None`, asserted in `affiliation_tests`, so a provider that
    /// says nothing gets less reach rather than more. This table records that the
    /// silence was a decision.
    #[allow(unused_mut)]
    fn unaffiliated_providers() -> Vec<(&'static str, &'static str)> {
        let mut rows = vec![
            ("anthropic", "public: hosted by an AI company"),
            (
                "azure_openai",
                "public: a large cloud, whatever endpoint it is pointed at",
            ),
            ("databricks", "public: a large cloud"),
            ("gcp_vertex_ai", "public: a large cloud"),
            ("github_copilot", "public: hosted by a software company"),
            ("google", "public: hosted by an AI company"),
            (
                "litellm",
                "public: an arbitrary proxy, and it makes no loopback claim",
            ),
            ("openai", "public: hosted by an AI company"),
            ("openrouter", "public: a model marketplace"),
            ("snowflake", "public: a large cloud"),
            ("tetrate", "public: a hosted gateway"),
            ("venice", "public: a hosted inference service"),
            ("xai", "public: hosted by an AI company"),
            ("xiaomi_mimo", "public: hosted by an AI company"),
            ("zai", "public: hosted by an AI company"),
        ];
        #[cfg(feature = "aws-providers")]
        {
            rows.push(("aws_bedrock", "public: a large cloud"));
            rows.push((
                "sagemaker_tgi",
                "public: an AWS-hosted endpoint, not this machine",
            ));
        }
        rows
    }

    /// Every built-in provider that ships `ProviderTier::Private` (Task 5), with
    /// the one-line reason. Like the affiliation table above, nothing here is a
    /// name-keyed assignment — the reason names the predicate or the endpoint the
    /// provider's own module states its tier from.
    ///
    /// ⚠ **`pub(crate)` so `tier_tests` reads THIS table rather than keeping a
    /// second one** (Task 56 Step 4). Two lists of which providers are Private is
    /// two things to update and one to forget, and the forgotten one is the guard
    /// — it would go on passing while naming a provider that no longer exists, or
    /// silently stop covering one that does.
    #[allow(unused_mut)]
    pub(crate) fn private_tier_providers() -> Vec<(&'static str, &'static str)> {
        let mut rows = vec![
            (
                "llamacpp",
                "private: the bundled sidecar runs on this machine; a LLAMACPP_EXTERNAL_HOST off it demotes the instance",
            ),
            (
                "ollama",
                "private: a self-hosted server on this machine; an OLLAMA_HOST off it demotes the instance",
            ),
            (
                "versa_azure",
                "private: the UCSF Versa gateway, under a signed agreement; an endpoint off the gateway demotes the instance",
            ),
        ];
        #[cfg(feature = "aws-providers")]
        rows.push((
            "versa_bedrock",
            "private: the UCSF Versa gateway's bedrock route; an endpoint off the gateway demotes the instance",
        ));
        rows
    }

    /// Every other built-in provider, each with the one-line reason it is Public.
    ///
    /// These are not Public by omission — the default *is* Public, pinned at all
    /// four levels by `tier_tests::a_provider_that_declares_no_tier_is_public`,
    /// so a provider that says nothing gets less reach rather than more. ⚠ This
    /// table therefore **records** a decision; it does not make one. A row here
    /// changes nothing about how the provider behaves, which is exactly why it is
    /// worth writing: without it, "nobody ever decided" and "we decided Public"
    /// are indistinguishable.
    #[allow(unused_mut)]
    fn public_tier_providers() -> Vec<(&'static str, &'static str)> {
        let mut rows = vec![
            ("anthropic", "public: general commercial endpoint"),
            (
                "azure_openai",
                "public: a large cloud. ⚠ azure.rs ships the UCSF gateway as \
                 AZURE_OPENAI_ENDPOINT's default, so this one *looks* institutional \
                 and is not; only versa_azure carries the agreement",
            ),
            ("databricks", "public: general commercial endpoint"),
            ("gcp_vertex_ai", "public: general commercial endpoint"),
            ("github_copilot", "public: general commercial endpoint"),
            ("google", "public: general commercial endpoint"),
            (
                "litellm",
                "public: an arbitrary proxy, and it makes no loopback claim",
            ),
            ("openai", "public: general commercial endpoint"),
            (
                "openrouter",
                "public: a model marketplace, upstream unknown",
            ),
            ("snowflake", "public: general commercial endpoint"),
            ("tetrate", "public: a hosted gateway"),
            ("venice", "public: a hosted inference service"),
            ("xai", "public: general commercial endpoint"),
            ("xiaomi_mimo", "public: general commercial endpoint"),
            ("zai", "public: general commercial endpoint"),
        ];
        #[cfg(feature = "aws-providers")]
        {
            rows.push(("aws_bedrock", "public: a large cloud"));
            rows.push((
                "sagemaker_tgi",
                "public: an AWS-hosted endpoint, not this machine",
            ));
        }
        rows
    }

    /// Task 53 Step 2: a new provider's tier cannot be left undecided.
    ///
    /// Affiliation has had a completeness census since Task 46; tier had none,
    /// so the fail-safe default silently absorbed every provider nobody thought
    /// about. That is a **bookkeeping** gap rather than a security hole — the
    /// default is Public, which is least-permission, and
    /// `tier_tests::the_private_set_is_a_table_of_reviewed_decisions` is what stops
    /// a new provider from claiming Private. This test closes the other half:
    /// adding a `registry.register::<…>` line above now fails until someone
    /// writes down which tier that provider ships at and why.
    ///
    /// ⚠ Same `aws-providers` caveat as the affiliation census: the feature is
    /// default-on, and `--no-default-features` legitimately registers three
    /// fewer providers, which is why both tables are `cfg`-gated in step with the
    /// registrations.
    #[test]
    fn every_registered_provider_is_classified_for_tier() {
        use crate::privacy::ProviderTier;

        let registered = registered_builtin_tiers();
        let private = private_tier_providers();
        let public = public_tier_providers();

        for (name, shipped) in &registered {
            let in_private = private.iter().any(|(n, _why)| n == name);
            let in_public = public.iter().any(|(n, _why)| n == name);
            assert!(
                in_private ^ in_public,
                "{name} is in neither tier table, or in both; decide it \
                 (may a private session be bound to this provider's models?). \
                 A row reading `public: general commercial endpoint` is a \
                 complete and correct answer."
            );

            // The table records the decision; `P::metadata()` *is* the decision.
            // Without this, a row could claim Public for a provider shipping
            // Private and the census would still be "complete".
            let recorded = if in_private {
                ProviderTier::Private
            } else {
                ProviderTier::Public
            };
            assert_eq!(
                *shipped, recorded,
                "{name} ships {shipped:?} but is filed under {recorded:?}: \
                 fix the table, not the provider (changing a tier is Task 5's \
                 `the_private_set_is_a_table_of_reviewed_decisions`, which needs \
                 an operator ruling)"
            );
        }

        // ...and the tables name nothing that is not registered, so a provider
        // deleted from the factory does not leave a stale classification behind
        // to make the count look right.
        for (name, _why) in private.iter().chain(public.iter()) {
            assert!(
                registered.iter().any(|(r, _)| r == name),
                "{name} is classified but not registered"
            );
        }

        // The count closes the loop: without it, adding a provider to BOTH the
        // registry and one table while deleting another table's row still passes
        // the loops above.
        let names: Vec<&String> = registered.iter().map(|(n, _)| n).collect();
        assert_eq!(
            registered.len(),
            private.len() + public.len(),
            "registered: {names:?}"
        );

        // Every reason is a real reason. An empty string would satisfy the
        // tables while recording nothing.
        for (name, why) in private.iter().chain(public.iter()) {
            assert!(!why.is_empty(), "{name} has no stated reason");
        }
    }

    /// The names `register_builtin_providers` actually registers, in this build's
    /// feature configuration.
    ///
    /// ⚠ Built from a **fresh** registry rather than the process-wide one:
    /// `init_registry` also loads the bundled and user-written declarative
    /// providers, so the live registry contains whatever `custom_providers/`
    /// happens to hold on the machine running the test.
    fn registered_builtin_names() -> Vec<String> {
        let mut registry = ProviderRegistry::new();
        register_builtin_providers(&mut registry);
        let mut names: Vec<String> = registry.entries.keys().cloned().collect();
        names.sort();
        names
    }

    /// The same fresh-registry set, carrying the tier each provider's own
    /// `metadata()` shipped — the value `GET /config/providers` serves and the
    /// tier census checks its table against.
    fn registered_builtin_tiers() -> Vec<(String, crate::privacy::ProviderTier)> {
        let mut registry = ProviderRegistry::new();
        register_builtin_providers(&mut registry);
        let mut rows: Vec<(String, crate::privacy::ProviderTier)> = registry
            .all_metadata_with_types()
            .into_iter()
            .map(|(metadata, _)| (metadata.name, metadata.tier))
            .collect();
        // By name only. `ProviderTier` is deliberately not `Ord` — ordering it
        // would make `max` spellable, and `max` over a capability is always a
        // bug (see `privacy::ProviderTier`) — so the tuple is not `Ord` either.
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        rows
    }

    /// Task 46 Step 3: a new provider cannot be forgotten.
    ///
    /// Not a rule someone must remember — the same mechanism Task 18A built for
    /// `CAPABILITY_CONFIG_KEYS`. Adding a `registry.register::<…>` line above
    /// fails this test until someone decides whether that provider's instances
    /// carry an affiliation, and writes down why. ⚠ It lives here, directly
    /// beneath the registration list, rather than beside the rest of Task 46's
    /// tests in `affiliation_tests.rs`, because here is where the person adding
    /// a provider is already looking.
    ///
    /// ⚠ The `aws-providers` feature is **default-on**. Running the suite with
    /// `--no-default-features` compiles neither `versa_bedrock` nor the two AWS
    /// public providers, and this test then legitimately sees a shorter list —
    /// which is why both tables are `cfg`-gated in step with the registrations
    /// rather than being flat constants.
    #[test]
    fn every_registered_provider_is_classified_for_affiliation() {
        let registered = registered_builtin_names();
        let affiliated = affiliated_providers();
        let unaffiliated = unaffiliated_providers();

        for name in &registered {
            let is_affiliated = affiliated.iter().any(|(n, _why)| n == name);
            let is_not = unaffiliated.iter().any(|(n, _why)| n == name);
            assert!(
                is_affiliated ^ is_not,
                "{name} is in neither affiliation table, or in both; classify it \
                 (DR-26: does an instance of it carry Local, an Institution, or nothing?)"
            );
        }

        // ...and the tables name nothing that is not registered, so a provider
        // deleted from the factory does not leave a stale classification behind
        // to make the count look right.
        for (name, _why) in affiliated.iter().chain(unaffiliated.iter()) {
            assert!(
                registered.iter().any(|r| r == name),
                "{name} is classified but not registered"
            );
        }

        // The count closes the loop: without it, adding a provider to BOTH the
        // registry and one table while deleting another table's row still
        // passes the two loops above.
        assert_eq!(
            registered.len(),
            affiliated.len() + unaffiliated.len(),
            "registered: {registered:?}"
        );

        // Every reason is a real reason. An empty string would satisfy the
        // tables while recording nothing.
        for (name, why) in affiliated.iter().chain(unaffiliated.iter()) {
            assert!(!why.is_empty(), "{name} has no stated reason");
        }
    }

    /// Task 56 Step 3 — **referential integrity, which is what replaced a
    /// count.**
    ///
    /// This slot used to hold `this_build_knows_exactly_one_institution`: every
    /// affiliated provider had to be `Local` or `Institution(ucsf)`, because
    /// `composite_affiliation` had no representable answer for a lead/worker pair
    /// spanning two institutions and fell back to a sentinel id. The set-valued
    /// model affiliation removed that constraint, so the pin has nothing left to
    /// force.
    ///
    /// ⚠ **A count was the wrong shape of guard anyway, and it is worth saying
    /// why rather than just deleting it.** "There should be one institution" says
    /// nothing about whether the one is the *right* one, and its only possible
    /// repair is deletion — the day a second institution is genuinely added the
    /// assertion is simply wrong, so the person adding it removes the gate and
    /// learns nothing. What replaces it scales to any number of institutions and
    /// catches the error people actually make: a typo'd or invented institution
    /// id, which produces a real, silent constraint — an id in no allowlist
    /// mismatches every connector, and one that collides with a published id
    /// clears flows nobody approved.
    ///
    /// The other half of the integrity — an institution in the map that nothing
    /// references — is checked where "referenced" is defined, in
    /// `landing/scripts/build-registry.mjs`'s
    /// `assertInstitutionsAreNamedByACard`, which can see the catalog's cards.
    /// Here we can see the providers.
    ///
    /// ⚠ **What this does NOT check, said plainly.** The rows are prose written
    /// beside each provider, so this holds the TABLE to the registry, not the
    /// provider's `affiliation()` to its row: a provider whose row says
    /// `Institution(ucsf)` while its implementation returns something else
    /// passes here. That tie is per provider and lives with the provider —
    /// `versa_azure::tests::affiliation_follows_the_endpoint_this_instance_resolved`
    /// and its `versa_bedrock` twin assert the real function against the real
    /// resolved endpoint — while the sibling census above keeps the table and
    /// the built-in provider list covering each other.
    #[test]
    fn every_institution_a_provider_claims_is_published_by_the_registry() {
        // Both floors, because this test's whole shape is "for each row" and a
        // table with no institutional row satisfies it while checking nothing.
        // A cardinality gate is what it replaced; a vacuous one would be worse.
        let rows = affiliated_providers();
        assert!(
            !rows.is_empty(),
            "the affiliation table is empty, so this test resolves no institution at all"
        );
        let mut resolved = 0usize;

        for (name, why) in rows {
            let Some(rest) = why.strip_prefix("Institution(") else {
                assert!(
                    why.starts_with("Local:"),
                    "{name}'s affiliation row must begin `Local:` or `Institution(<id>):` so the \
                     institution it claims can be checked against the registry; got {why:?}"
                );
                continue;
            };
            let (id, _) = rest.split_once("):").unwrap_or_else(|| {
                panic!("{name}'s affiliation row opens `Institution(` and never closes it: {why:?}")
            });
            assert!(
                crate::privacy::affiliation::institution_display_name(
                    crate::privacy::affiliation::InstitutionId::new(id)
                )
                .is_some(),
                "{name} claims institution {id:?}, which the registry snapshot does not publish \
                 (privacy::registry_private::INSTITUTIONS). Either it is a typo, in which case \
                 it silently mismatches every connector, because an unpublished id is in no \
                 allowlist, or the institution is real and belongs in INSTITUTIONS in \
                 landing/scripts/build-registry.mjs, which is the one place an institution is \
                 declared."
            );
            resolved += 1;
        }

        assert!(
            resolved > 0,
            "no row named an institution, so every assertion above was skipped and this test \
             proved nothing about the registry. If the build genuinely ships no institutional \
             provider, that is a change to what DR-26's third axis governs and belongs in the \
             table's own doc, not in a silently green test."
        );
    }

    #[test_case::test_case(None, None, None, DEFAULT_LEAD_TURNS, DEFAULT_FAILURE_THRESHOLD, DEFAULT_FALLBACK_TURNS ; "defaults")]
    #[test_case::test_case(Some("7"), Some("4"), Some("3"), 7, 4, 3 ; "custom")]
    #[tokio::test]
    async fn test_create_lead_worker_provider(
        lead_turns: Option<&str>,
        failure_threshold: Option<&str>,
        fallback_turns: Option<&str>,
        expected_turns: usize,
        expected_failure: usize,
        expected_fallback: usize,
    ) {
        let _guard = env_lock::lock_env([
            ("BIOROUTER_LEAD_MODEL", Some("gpt-4o")),
            ("BIOROUTER_LEAD_PROVIDER", None),
            ("BIOROUTER_LEAD_TURNS", lead_turns),
            ("BIOROUTER_LEAD_FAILURE_THRESHOLD", failure_threshold),
            ("BIOROUTER_LEAD_FALLBACK_TURNS", fallback_turns),
            ("OPENAI_API_KEY", Some("fake-openai-no-keyring")),
        ]);

        let provider = create("openai", ModelConfig::new_or_fail("gpt-4o-mini"))
            .await
            .unwrap();
        let lw = provider.as_lead_worker().unwrap();
        let (lead, worker) = lw.get_model_info();
        assert_eq!(lead, "gpt-4o");
        assert_eq!(worker, "gpt-4o-mini");
        assert_eq!(
            lw.get_settings(),
            (expected_turns, expected_failure, expected_fallback)
        );
    }

    #[tokio::test]
    async fn test_create_regular_provider_without_lead_config() {
        let _guard = env_lock::lock_env([
            ("BIOROUTER_LEAD_MODEL", None),
            ("BIOROUTER_LEAD_PROVIDER", None),
            ("BIOROUTER_LEAD_TURNS", None),
            ("BIOROUTER_LEAD_FAILURE_THRESHOLD", None),
            ("BIOROUTER_LEAD_FALLBACK_TURNS", None),
            ("OPENAI_API_KEY", Some("fake-openai-no-keyring")),
        ]);

        let provider = create("openai", ModelConfig::new_or_fail("gpt-4o-mini"))
            .await
            .unwrap();
        assert!(provider.as_lead_worker().is_none());
        assert_eq!(provider.get_model_config().model_name, "gpt-4o-mini");
    }

    #[test_case::test_case(None, None, 16_000 ; "no overrides uses default")]
    #[test_case::test_case(Some("32000"), None, 32_000 ; "worker limit overrides default")]
    #[test_case::test_case(Some("32000"), Some("64000"), 32_000 ; "worker limit takes priority over global")]
    fn test_worker_model_context_limit(
        worker_limit: Option<&str>,
        global_limit: Option<&str>,
        expected_limit: usize,
    ) {
        let _guard = env_lock::lock_env([
            ("BIOROUTER_WORKER_CONTEXT_LIMIT", worker_limit),
            ("BIOROUTER_CONTEXT_LIMIT", global_limit),
        ]);

        let default_model =
            ModelConfig::new_or_fail("gpt-3.5-turbo").with_context_limit(Some(16_000));

        let result = create_worker_model_config(&default_model).unwrap();
        assert_eq!(result.context_limit, Some(expected_limit));
    }

    #[tokio::test]
    async fn test_openai_compatible_providers_config_keys() {
        let providers_list = providers().await;
        let cases = vec![
            ("openai", "OPENAI_API_KEY"),
            ("groq", "GROQ_API_KEY"),
            ("mistral", "MISTRAL_API_KEY"),
            ("custom_deepseek", "DEEPSEEK_API_KEY"),
            ("xiaomi_mimo", "XIAOMI_MIMO_API_KEY"),
            ("zai", "ZAI_API_KEY"),
        ];
        for (name, expected_key) in cases {
            if let Some((meta, _)) = providers_list.iter().find(|(m, _)| m.name == name) {
                assert!(
                    !meta.config_keys.is_empty(),
                    "{name} provider should have config keys"
                );
                assert_eq!(
                    meta.config_keys[0].name, expected_key,
                    "First config key for {name} should be {expected_key}, got {}",
                    meta.config_keys[0].name
                );
                assert!(
                    meta.config_keys[0].required,
                    "{expected_key} should be required"
                );
                assert!(
                    meta.config_keys[0].secret,
                    "{expected_key} should be secret"
                );
            } else {
                // Provider not registered; skip test for this provider
                continue;
            }
        }
    }
}
