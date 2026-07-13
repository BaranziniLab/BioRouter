#[derive(Debug, Clone, PartialEq)]
pub struct ProviderModelPricing {
    pub input_token_cost: f64,
    pub output_token_cost: f64,
    pub currency: String,
    pub context_length: Option<u32>,
}

impl ProviderModelPricing {
    fn usd_per_million(input: f64, output: f64, context_length: u32) -> Self {
        Self {
            input_token_cost: input / 1_000_000.0,
            output_token_cost: output / 1_000_000.0,
            currency: "$".to_string(),
            context_length: Some(context_length),
        }
    }
}

pub fn provider_model_pricing(provider: &str, model: &str) -> Option<ProviderModelPricing> {
    let provider = provider.to_ascii_lowercase();
    let model = model.to_ascii_lowercase();

    match provider.as_str() {
        "openrouter" => openrouter_pricing(&model),
        "groq" => groq_pricing(&model),
        "zai" | "z.ai" | "z-ai" => zai_pricing(&model),
        "xiaomi_mimo" | "xiaomi-mimo" | "xiaomi" => xiaomi_mimo_pricing(&model),
        "custom_deepseek" | "deepseek" => deepseek_pricing(&model),
        "inception" => inception_pricing(&model),
        "mistral" | "mistralai" => mistral_pricing(&model),
        "xai" | "x-ai" => xai_pricing(&model),
        _ => None,
    }
}

fn openrouter_pricing(model: &str) -> Option<ProviderModelPricing> {
    match model {
        "deepseek/deepseek-v4-flash" => {
            Some(ProviderModelPricing::usd_per_million(0.09, 0.18, 1_048_576))
        }
        "inception/mercury-2" => Some(ProviderModelPricing::usd_per_million(0.25, 0.75, 128_000)),
        "minimax/minimax-m3" => Some(ProviderModelPricing::usd_per_million(0.30, 1.20, 1_048_576)),
        "moonshotai/kimi-k2.6" => Some(ProviderModelPricing::usd_per_million(0.66, 3.41, 262_144)),
        "moonshotai/kimi-k2.7-code" => {
            Some(ProviderModelPricing::usd_per_million(0.74, 3.50, 262_144))
        }
        "x-ai/grok-build-0.1" => Some(ProviderModelPricing::usd_per_million(1.00, 2.00, 256_000)),
        "xiaomi/mimo-v2.5" => Some(ProviderModelPricing::usd_per_million(
            0.105, 0.28, 1_048_576,
        )),
        "xiaomi/mimo-v2.5-pro" => Some(ProviderModelPricing::usd_per_million(
            0.435, 0.87, 1_048_576,
        )),
        "z-ai/glm-5.1" => Some(ProviderModelPricing::usd_per_million(0.966, 3.036, 202_752)),
        "z-ai/glm-5.2" => Some(ProviderModelPricing::usd_per_million(
            0.9086, 2.8556, 1_048_576,
        )),
        _ => None,
    }
}

fn groq_pricing(model: &str) -> Option<ProviderModelPricing> {
    match model {
        "openai/gpt-oss-120b" => Some(ProviderModelPricing::usd_per_million(0.15, 0.60, 131_072)),
        "openai/gpt-oss-20b" | "openai/gpt-oss-safeguard-20b" => {
            Some(ProviderModelPricing::usd_per_million(0.075, 0.30, 131_072))
        }
        "llama-3.1-8b-instant" => Some(ProviderModelPricing::usd_per_million(0.05, 0.08, 131_072)),
        "llama-3.3-70b-versatile" => {
            Some(ProviderModelPricing::usd_per_million(0.59, 0.79, 131_072))
        }
        _ => None,
    }
}

fn zai_pricing(model: &str) -> Option<ProviderModelPricing> {
    match model {
        "glm-5.2" => Some(ProviderModelPricing::usd_per_million(1.40, 4.40, 1_048_576)),
        "glm-5.1" => Some(ProviderModelPricing::usd_per_million(1.40, 4.40, 202_752)),
        "glm-5" => Some(ProviderModelPricing::usd_per_million(1.00, 3.20, 200_000)),
        "glm-5-turbo" => Some(ProviderModelPricing::usd_per_million(1.20, 4.00, 200_000)),
        "glm-4.7" | "glm-4.6" | "glm-4.5" => {
            Some(ProviderModelPricing::usd_per_million(0.60, 2.20, 200_000))
        }
        "glm-4.5-air" => Some(ProviderModelPricing::usd_per_million(0.20, 1.10, 131_072)),
        _ => None,
    }
}

fn xiaomi_mimo_pricing(model: &str) -> Option<ProviderModelPricing> {
    match model {
        "mimo-v2.5" => Some(ProviderModelPricing::usd_per_million(0.14, 0.28, 1_000_000)),
        "mimo-v2.5-pro" => Some(ProviderModelPricing::usd_per_million(
            0.435, 0.87, 1_000_000,
        )),
        _ => None,
    }
}

fn deepseek_pricing(model: &str) -> Option<ProviderModelPricing> {
    match model {
        "deepseek-v4-flash" | "deepseek-chat" | "deepseek-reasoner" => {
            Some(ProviderModelPricing::usd_per_million(0.14, 0.28, 1_000_000))
        }
        "deepseek-v4-pro" => Some(ProviderModelPricing::usd_per_million(
            0.435, 0.87, 1_000_000,
        )),
        _ => None,
    }
}

fn inception_pricing(model: &str) -> Option<ProviderModelPricing> {
    match model {
        "mercury-2" => Some(ProviderModelPricing::usd_per_million(0.25, 0.75, 128_000)),
        "mercury-coder" | "mercury-edit-2" => {
            Some(ProviderModelPricing::usd_per_million(0.25, 0.75, 32_000))
        }
        _ => None,
    }
}

fn mistral_pricing(model: &str) -> Option<ProviderModelPricing> {
    match model {
        "mistral-medium-3-5" | "mistral-medium-latest" => {
            Some(ProviderModelPricing::usd_per_million(1.50, 7.50, 262_144))
        }
        "mistral-large-2512" => Some(ProviderModelPricing::usd_per_million(0.50, 1.50, 262_144)),
        "mistral-small-2603" => Some(ProviderModelPricing::usd_per_million(0.15, 0.60, 262_144)),
        "devstral-2512" => Some(ProviderModelPricing::usd_per_million(0.40, 2.00, 262_144)),
        "ministral-8b-2512" => Some(ProviderModelPricing::usd_per_million(0.15, 0.15, 262_144)),
        "mistral-medium-2508" => Some(ProviderModelPricing::usd_per_million(0.40, 2.00, 128_000)),
        "magistral-medium-2509" => Some(ProviderModelPricing::usd_per_million(2.00, 5.00, 128_000)),
        "codestral-2508" => Some(ProviderModelPricing::usd_per_million(0.30, 0.90, 128_000)),
        _ => None,
    }
}

fn xai_pricing(model: &str) -> Option<ProviderModelPricing> {
    match model {
        "grok-4.3" | "grok-4.3-latest" | "grok-latest" => {
            Some(ProviderModelPricing::usd_per_million(1.25, 2.50, 1_000_000))
        }
        "grok-4.20-0309-reasoning"
        | "grok-4.20-0309-non-reasoning"
        | "grok-4.20-multi-agent-0309" => {
            Some(ProviderModelPricing::usd_per_million(1.25, 2.50, 1_000_000))
        }
        "grok-build-0.1" => Some(ProviderModelPricing::usd_per_million(1.00, 2.00, 256_000)),
        _ => None,
    }
}

/// Estimated dollar cost of one completion, or `None` when the model's price is
/// unknown (local models, subscription providers, an unrecognised model id).
///
/// Same precedence the `/config/pricing` route and the CLI's cost line use:
/// provider-specific overrides first, then the canonical model catalog. Lives
/// here so the per-reply dollar budget (BR-35) and the display paths can never
/// disagree about what a turn cost.
pub fn estimate_cost_usd(
    provider: &str,
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
) -> Option<f64> {
    let (input_cost_per_token, output_cost_per_token) = if let Some(pricing) =
        provider_model_pricing(provider, model)
    {
        (pricing.input_token_cost, pricing.output_token_cost)
    } else {
        let canonical = crate::providers::canonical::maybe_get_canonical_model(provider, model)?;
        (canonical.pricing.prompt?, canonical.pricing.completion?)
    };

    Some(input_cost_per_token * input_tokens as f64 + output_cost_per_token * output_tokens as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_cost_prices_a_provider_override() {
        // groq/llama-3.1-8b-instant: $0.05/M in, $0.08/M out.
        let cost = estimate_cost_usd("groq", "llama-3.1-8b-instant", 1_000_000, 1_000_000).unwrap();
        assert!((cost - 0.13).abs() < 1e-9, "expected $0.13, got {cost}");
    }

    #[test]
    fn estimate_cost_is_none_for_an_unknown_model() {
        assert!(estimate_cost_usd("ollama", "llama3", 1_000, 1_000).is_none());
        assert!(estimate_cost_usd("nope", "not-a-model", 1_000, 1_000).is_none());
    }

    #[test]
    fn openrouter_uses_openrouter_specific_price() {
        let pricing = provider_model_pricing("openrouter", "deepseek/deepseek-v4-flash").unwrap();
        assert_eq!(pricing.input_token_cost, 0.09 / 1_000_000.0);
        assert_eq!(pricing.output_token_cost, 0.18 / 1_000_000.0);
        assert_eq!(pricing.context_length, Some(1_048_576));
    }

    #[test]
    fn direct_zai_uses_zai_price() {
        let pricing = provider_model_pricing("zai", "glm-5.2").unwrap();
        assert_eq!(pricing.input_token_cost, 1.40 / 1_000_000.0);
        assert_eq!(pricing.output_token_cost, 4.40 / 1_000_000.0);
    }

    #[test]
    fn local_or_subscription_models_are_unpriced() {
        assert!(provider_model_pricing("ollama", "llama3").is_none());
        assert!(provider_model_pricing("github_copilot", "gpt-5.5").is_none());
    }
}
