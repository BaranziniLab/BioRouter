//! Best-effort detection of which commercial provider an API key belongs to.
//!
//! Onboarding lets a user paste a single API key and figures out the provider
//! by probing each candidate's `/models` endpoint with the key. Three
//! properties matter and are enforced here:
//!
//! * **No environment mutation.** The candidate key is supplied to provider
//!   constructors via a task-local override (see
//!   [`crate::config::with_config_overrides`]), never `std::env::set_var`.
//!   Mutating the process environment from a multi-threaded program is unsound
//!   and would leak the key into any subprocess spawned during the probe.
//! * **Bounded latency.** The whole probe is wrapped in a hard timeout and each
//!   candidate is tried exactly once (no retry budget), so a throttled or
//!   unreachable vendor can't hang the onboarding spinner.
//! * **Prefix narrowing.** Keys with a distinctive prefix (`sk-ant-`, `AIza`,
//!   `gsk_`, `xai-`) are only tried against the matching provider, avoiding
//!   wasted calls and cross-provider false positives. Ambiguous keys (generic
//!   `sk-…`) fan out across the OpenAI-compatible pool.

use std::collections::HashMap;
use std::time::Duration;

use tokio::task::JoinSet;

use crate::config::with_config_overrides;
use crate::model::ModelConfig;
use crate::providers::errors::ProviderError;

/// Upper bound on the whole detection probe. Provider retry budgets can reach
/// ~2 minutes on a rate limit; detection must never inherit that.
const DETECT_TIMEOUT: Duration = Duration::from_secs(12);

/// A provider that can be auto-detected from a single bearer API key.
struct Candidate {
    /// Registry name passed to [`crate::providers::create`].
    provider: &'static str,
    /// Secret key name the provider reads (and that we override during probing).
    env_key: &'static str,
    /// Distinctive key prefixes. If the pasted key starts with one of these,
    /// only this candidate is tried. Empty = part of the ambiguous fallback pool.
    exclusive_prefixes: &'static [&'static str],
    /// Extra non-secret config to persist alongside the key so the configured
    /// provider targets the same endpoint the probe validated against
    /// (e.g. MiMo's regional host).
    extra_config: &'static [(&'static str, &'static str)],
}

/// The detectable providers. Order is irrelevant — detection is first-success.
const CANDIDATES: &[Candidate] = &[
    Candidate {
        provider: "anthropic",
        env_key: "ANTHROPIC_API_KEY",
        exclusive_prefixes: &["sk-ant-"],
        extra_config: &[],
    },
    Candidate {
        provider: "google",
        env_key: "GOOGLE_API_KEY",
        exclusive_prefixes: &["AIza"],
        extra_config: &[],
    },
    Candidate {
        provider: "groq",
        env_key: "GROQ_API_KEY",
        exclusive_prefixes: &["gsk_"],
        extra_config: &[],
    },
    Candidate {
        provider: "xai",
        env_key: "XAI_API_KEY",
        exclusive_prefixes: &["xai-"],
        extra_config: &[],
    },
    // OpenAI-compatible providers share the generic `sk-…` shape, so they have
    // no exclusive prefix and form the ambiguous fallback pool.
    Candidate {
        provider: "openai",
        env_key: "OPENAI_API_KEY",
        exclusive_prefixes: &[],
        extra_config: &[],
    },
    Candidate {
        provider: "zai",
        env_key: "ZAI_API_KEY",
        exclusive_prefixes: &[],
        extra_config: &[],
    },
    Candidate {
        provider: "xiaomi_mimo",
        env_key: "XIAOMI_MIMO_API_KEY",
        exclusive_prefixes: &[],
        // Persist the host we validated against so the saved provider doesn't
        // silently fall back to a different default later.
        extra_config: &[(
            "XIAOMI_MIMO_HOST",
            crate::providers::xiaomi_mimo::XIAOMI_MIMO_API_HOST,
        )],
    },
];

/// A successful detection result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detected {
    /// Registry provider name (e.g. `"openai"`).
    pub provider: String,
    /// All model ids the provider reported for this key.
    pub models: Vec<String>,
    /// A sensible default/recommended chat model, when one can be determined.
    pub default_model: Option<String>,
    /// Non-secret config to persist alongside the key (may be empty).
    pub extra_config: HashMap<String, String>,
}

/// Why detection failed — surfaced to the UI so it can give actionable advice
/// instead of a single opaque "could not detect" message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectError {
    /// The probe exceeded [`DETECT_TIMEOUT`].
    Timeout,
    /// Every probe failed with a transient/connectivity error.
    Network,
    /// The key matched a provider by prefix, but that provider rejected it.
    InvalidKey,
    /// The key did not validate against any supported provider.
    NoMatch,
}

impl DetectError {
    /// Stable machine-readable code for the API/UI.
    pub fn code(&self) -> &'static str {
        match self {
            DetectError::Timeout => "timeout",
            DetectError::Network => "network",
            DetectError::InvalidKey => "invalid_key",
            DetectError::NoMatch => "no_match",
        }
    }
}

/// Per-candidate failure classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProbeError {
    /// The provider explicitly rejected the key (this isn't its key).
    Auth,
    /// Transient/connectivity failure — inconclusive.
    Network,
    /// Anything else (construction failed, unexpected error).
    Other,
}

/// Map a provider error encountered while probing to a coarse class.
fn classify(err: &ProviderError) -> ProbeError {
    match err {
        ProviderError::Authentication(_) => ProbeError::Auth,
        ProviderError::RateLimitExceeded { .. }
        | ProviderError::ServerError(_)
        | ProviderError::RequestFailed(_) => ProbeError::Network,
        _ => ProbeError::Other,
    }
}

/// Choose which candidates to probe for a given key, and whether the selection
/// was driven by an exclusive prefix match. Pure so it can be unit-tested.
fn candidates_for_key(key: &str) -> (Vec<&'static Candidate>, bool) {
    let matched: Vec<&'static Candidate> = CANDIDATES
        .iter()
        .filter(|c| {
            !c.exclusive_prefixes.is_empty()
                && c.exclusive_prefixes.iter().any(|p| key.starts_with(p))
        })
        .collect();
    if !matched.is_empty() {
        return (matched, true);
    }
    // No distinctive prefix: try the OpenAI-compatible ambiguous pool only.
    let pool: Vec<&'static Candidate> = CANDIDATES
        .iter()
        .filter(|c| c.exclusive_prefixes.is_empty())
        .collect();
    (pool, false)
}

/// Display-purpose list of provider registry names that auto-detection supports.
pub fn detectable_providers() -> Vec<&'static str> {
    CANDIDATES.iter().map(|c| c.provider).collect()
}

/// Probe a single candidate with the key, scoped via a task-local override so we
/// never touch the process environment.
async fn probe_candidate(c: &'static Candidate, key: String) -> Result<Detected, ProbeError> {
    let mut overrides = HashMap::new();
    overrides.insert(c.env_key.to_string(), key);
    for (k, v) in c.extra_config {
        overrides.insert((*k).to_string(), (*v).to_string());
    }

    with_config_overrides(overrides, async move {
        let provider =
            match crate::providers::create(c.provider, ModelConfig::new_or_fail("default")).await {
                Ok(p) => p,
                Err(_) => return Err(ProbeError::Other),
            };

        let models = match provider.fetch_supported_models().await {
            Ok(Some(models)) if !models.is_empty() => models,
            // No models returned means we can't confirm this is the right
            // provider for the key — treat as a non-match, not a hard error.
            Ok(_) => return Err(ProbeError::Auth),
            Err(e) => return Err(classify(&e)),
        };

        // Prefer a recommended (text-capable, canonical) model over an arbitrary
        // first entry, which may be an embedding/image model. Best-effort.
        let default_model = provider
            .fetch_recommended_models()
            .await
            .ok()
            .flatten()
            .and_then(|recommended| recommended.into_iter().next())
            .or_else(|| models.first().cloned());

        let extra_config = c
            .extra_config
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();

        Ok(Detected {
            provider: c.provider.to_string(),
            models,
            default_model,
            extra_config,
        })
    })
    .await
}

/// Detect which provider an API key belongs to by probing candidate `/models`
/// endpoints. Returns the first provider that validates the key, or a
/// classified [`DetectError`] on failure.
pub async fn detect_provider_from_api_key(api_key: &str) -> Result<Detected, DetectError> {
    let key = api_key.trim().to_string();
    if key.is_empty() {
        return Err(DetectError::NoMatch);
    }

    let (candidates, used_exclusive) = candidates_for_key(&key);

    let probe = async move {
        let mut set: JoinSet<Result<Detected, ProbeError>> = JoinSet::new();
        for c in candidates {
            let key = key.clone();
            set.spawn(probe_candidate(c, key));
        }

        let mut saw_network = false;
        let mut saw_auth = false;
        while let Some(joined) = set.join_next().await {
            match joined {
                Ok(Ok(detected)) => {
                    set.abort_all();
                    return Ok(detected);
                }
                Ok(Err(ProbeError::Network)) => saw_network = true,
                Ok(Err(ProbeError::Auth)) => saw_auth = true,
                Ok(Err(ProbeError::Other)) => {}
                // Task panicked or was aborted — ignore and keep going.
                Err(_) => {}
            }
        }

        // No candidate validated the key. Pick the most useful reason.
        if saw_network && !saw_auth {
            Err(DetectError::Network)
        } else if used_exclusive {
            // The key looked like a specific provider's key but was rejected.
            Err(DetectError::InvalidKey)
        } else {
            Err(DetectError::NoMatch)
        }
    };

    match tokio::time::timeout(DETECT_TIMEOUT, probe).await {
        Ok(result) => result,
        Err(_) => Err(DetectError::Timeout),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(cands: &[&'static Candidate]) -> Vec<&'static str> {
        cands.iter().map(|c| c.provider).collect()
    }

    #[test]
    fn exclusive_prefix_routes_to_single_provider() {
        let (cands, exclusive) = candidates_for_key("sk-ant-abc123");
        assert!(exclusive);
        assert_eq!(names(&cands), vec!["anthropic"]);

        let (cands, exclusive) = candidates_for_key("AIzaSyExample");
        assert!(exclusive);
        assert_eq!(names(&cands), vec!["google"]);

        let (cands, exclusive) = candidates_for_key("gsk_example");
        assert!(exclusive);
        assert_eq!(names(&cands), vec!["groq"]);

        let (cands, exclusive) = candidates_for_key("xai-example");
        assert!(exclusive);
        assert_eq!(names(&cands), vec!["xai"]);
    }

    #[test]
    fn generic_sk_key_uses_ambiguous_pool() {
        let (cands, exclusive) = candidates_for_key("sk-proj-generic-openai-key");
        assert!(!exclusive);
        let n = names(&cands);
        // Pool is exactly the no-prefix providers; anthropic/google/groq/xai
        // (which have distinctive prefixes) must be excluded.
        assert!(n.contains(&"openai"));
        assert!(n.contains(&"zai"));
        assert!(n.contains(&"xiaomi_mimo"));
        assert!(!n.contains(&"anthropic"));
        assert!(!n.contains(&"google"));
        assert!(!n.contains(&"groq"));
        assert!(!n.contains(&"xai"));
    }

    #[test]
    fn unknown_prefix_falls_back_to_pool() {
        // A key for an unsupported provider with an odd shape still gets probed
        // against the ambiguous pool (and will simply fail to validate).
        let (cands, exclusive) = candidates_for_key("weirdformatkey123");
        assert!(!exclusive);
        assert!(!cands.is_empty());
        assert!(cands.iter().all(|c| c.exclusive_prefixes.is_empty()));
    }

    #[test]
    fn classify_maps_errors_to_classes() {
        assert_eq!(
            classify(&ProviderError::Authentication("bad".into())),
            ProbeError::Auth
        );
        assert_eq!(
            classify(&ProviderError::ServerError("500".into())),
            ProbeError::Network
        );
        assert_eq!(
            classify(&ProviderError::RequestFailed("conn".into())),
            ProbeError::Network
        );
        assert_eq!(
            classify(&ProviderError::RateLimitExceeded {
                details: "429".into(),
                retry_delay: None
            }),
            ProbeError::Network
        );
        assert_eq!(
            classify(&ProviderError::UsageError("x".into())),
            ProbeError::Other
        );
    }

    #[test]
    fn detect_error_codes_are_stable() {
        assert_eq!(DetectError::Timeout.code(), "timeout");
        assert_eq!(DetectError::Network.code(), "network");
        assert_eq!(DetectError::InvalidKey.code(), "invalid_key");
        assert_eq!(DetectError::NoMatch.code(), "no_match");
    }

    #[test]
    fn detectable_providers_lists_all_candidates() {
        let providers = detectable_providers();
        for expected in [
            "anthropic",
            "google",
            "groq",
            "xai",
            "openai",
            "zai",
            "xiaomi_mimo",
        ] {
            assert!(providers.contains(&expected), "missing {expected}");
        }
    }

    #[tokio::test]
    async fn empty_key_is_no_match_without_probing() {
        assert_eq!(
            detect_provider_from_api_key("   ").await,
            Err(DetectError::NoMatch)
        );
    }

    #[test]
    fn mimo_persists_host_in_extra_config() {
        let mimo = CANDIDATES
            .iter()
            .find(|c| c.provider == "xiaomi_mimo")
            .unwrap();
        assert_eq!(mimo.extra_config.len(), 1);
        assert_eq!(mimo.extra_config[0].0, "XIAOMI_MIMO_HOST");
    }
}
