use crate::model::ModelConfig;
use crate::providers::retry::{retry_operation, RetryConfig};

pub async fn detect_provider_from_api_key(api_key: &str) -> Option<(String, Vec<String>)> {
    let provider_tests = vec![
        ("anthropic", "ANTHROPIC_API_KEY"),
        ("openai", "OPENAI_API_KEY"),
        ("google", "GOOGLE_API_KEY"),
        ("groq", "GROQ_API_KEY"),
        ("xai", "XAI_API_KEY"),
        ("zai", "ZAI_API_KEY"),
        ("xiaomi_mimo", "XIAOMI_MIMO_API_KEY"),
        // Ollama and OpenRouter don't validate keys, so they would match any input
    ];

    let tasks: Vec<_> = provider_tests
        .into_iter()
        .map(|(provider_name, env_key)| {
            let api_key = api_key.to_string();
            tokio::spawn(async move {
                let original_value = std::env::var(env_key).ok();
                std::env::set_var(env_key, &api_key);

                let result = match crate::providers::create(
                    provider_name,
                    ModelConfig::new_or_fail("default"),
                )
                .await
                {
                    Ok(provider) => {
                        match retry_operation(&RetryConfig::default(), || async {
                            provider.fetch_supported_models().await
                        })
                        .await
                        {
                            Ok(Some(models)) => Some((provider_name.to_string(), models)),
                            _ => None,
                        }
                    }
                    Err(_) => None,
                };

                match original_value {
                    Some(val) => std::env::set_var(env_key, val),
                    None => std::env::remove_var(env_key),
                }

                result
            })
        })
        .collect();

    for task in tasks {
        if let Ok(Some(result)) = task.await {
            return Some(result);
        }
    }

    None
}

/// Display-purpose list of provider registry names that auto-detection supports.
///
/// This is the single source of truth surfaced to the UI via
/// `GET /config/detectable-providers`. Keep it in sync with the provider list
/// probed in [`detect_provider_from_api_key`].
pub fn detectable_providers() -> Vec<&'static str> {
    vec![
        "anthropic",
        "openai",
        "google",
        "groq",
        "xai",
        "zai",
        "xiaomi_mimo",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
