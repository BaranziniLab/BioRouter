#[derive(Debug, Clone, PartialEq)]
pub struct ProviderModelPricing {
    pub input_token_cost: f64,
    pub output_token_cost: f64,
    /// Cost **per token** of an input token served from the prompt cache, or
    /// `None` when the model has no separate cache-read rate. Same units as
    /// `input_token_cost`.
    pub cache_read_cost: Option<f64>,
    /// Cost **per token** of an input token written to the prompt cache, or
    /// `None`. Same units as `input_token_cost`.
    pub cache_write_cost: Option<f64>,
    pub currency: String,
    pub context_length: Option<u32>,
}

impl ProviderModelPricing {
    fn usd_per_million(input: f64, output: f64, context_length: u32) -> Self {
        Self {
            input_token_cost: input / 1_000_000.0,
            output_token_cost: output / 1_000_000.0,
            cache_read_cost: None,
            cache_write_cost: None,
            currency: "$".to_string(),
            context_length: Some(context_length),
        }
    }

    /// Anthropic-style pricing with prompt caching. Cache read is billed at
    /// **0.1x** the input rate and cache creation (write) at **1.25x**, the
    /// standard Claude multipliers (applied identically to Claude-on-Bedrock).
    fn usd_per_million_cached(input: f64, output: f64, context_length: u32) -> Self {
        Self {
            input_token_cost: input / 1_000_000.0,
            output_token_cost: output / 1_000_000.0,
            cache_read_cost: Some(input * 0.1 / 1_000_000.0),
            cache_write_cost: Some(input * 1.25 / 1_000_000.0),
            currency: "$".to_string(),
            context_length: Some(context_length),
        }
    }
}

/// Dollar cost of one turn including its cache buckets, plus a flag when the
/// model is priced but its cache tokens could not be (no cache rate on file).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurnCost {
    pub cost: f64,
    /// `true` when this turn had cache tokens but the model has no cache pricing,
    /// so those tokens are **excluded** from `cost` (a lower bound, not exact).
    pub cache_excluded: bool,
}

/// Dollar cost of a turn (or a rollup of turns) for one `(provider, model)`,
/// or `None` when the pair has no known pricing.
///
/// `input_token_cost` / `output_token_cost` are the currency's cost **per
/// token** (`usd_per_million` already divided the per-million rate by 1e6), so
/// the cost is simply `tokens * per-token-cost`. This is the single shared
/// definition of "what a turn costs" — the server usage report, the summary
/// gauge, and any future caller price identically through it.
pub fn model_cost(
    provider: &str,
    model: &str,
    input_tokens: i64,
    output_tokens: i64,
) -> Option<f64> {
    let pricing = provider_model_pricing(provider, model)?;
    Some(
        input_tokens as f64 * pricing.input_token_cost
            + output_tokens as f64 * pricing.output_token_cost,
    )
}

/// Dollar cost of a turn (or rollup) including its cache buckets. `None` when the
/// `(provider, model)` pair is unpriced. When the pair IS priced but has no cache
/// rate and the turn carried cache tokens, those tokens are left out of the cost
/// and `cache_excluded` is set so callers can flag the figure as a lower bound.
///
/// The buckets are the disjoint counts from `Usage`: `input_tokens` is fresh
/// (non-cached) input, and `cache_read` / `cache_creation` are additive.
pub fn model_cost_with_cache(
    provider: &str,
    model: &str,
    input_tokens: i64,
    output_tokens: i64,
    cache_read_tokens: i64,
    cache_creation_tokens: i64,
) -> Option<TurnCost> {
    let pricing = provider_model_pricing(provider, model)?;
    let mut cost = input_tokens as f64 * pricing.input_token_cost
        + output_tokens as f64 * pricing.output_token_cost;
    let mut cache_excluded = false;

    if cache_read_tokens > 0 {
        match pricing.cache_read_cost {
            Some(rate) => cost += cache_read_tokens as f64 * rate,
            None => cache_excluded = true,
        }
    }
    if cache_creation_tokens > 0 {
        match pricing.cache_write_cost {
            Some(rate) => cost += cache_creation_tokens as f64 * rate,
            None => cache_excluded = true,
        }
    }

    Some(TurnCost {
        cost,
        cache_excluded,
    })
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
        // Claude, natively and on Bedrock. Bedrock also serves non-Anthropic
        // models (Titan, Llama, …); `claude_family_pricing` returns None for
        // those, leaving them correctly unpriced.
        "anthropic" | "claude" | "bedrock" | "aws_bedrock" | "aws-bedrock" => {
            claude_family_pricing(&model)
        }
        _ => None,
    }
}

/// Pricing for the Claude families, matched by substring so it handles both
/// native ids (`claude-sonnet-4-20250514`) and Bedrock ids
/// (`us.anthropic.claude-sonnet-4-20250514-v1:0`). Rates are per-million USD;
/// caching uses the standard 0.1x read / 1.25x write multipliers via
/// [`ProviderModelPricing::usd_per_million_cached`]. `model` is already
/// lowercased by the caller.
fn claude_family_pricing(model: &str) -> Option<ProviderModelPricing> {
    if !model.contains("claude") {
        return None;
    }
    // Order matters: check the more specific tiers before the sonnet default.
    let pricing = if model.contains("opus") {
        ProviderModelPricing::usd_per_million_cached(15.0, 75.0, 200_000)
    } else if model.contains("haiku") {
        ProviderModelPricing::usd_per_million_cached(0.80, 4.00, 200_000)
    } else {
        // Sonnet, and any unrecognised Claude tier, price as Sonnet-class.
        ProviderModelPricing::usd_per_million_cached(3.00, 15.00, 200_000)
    };
    Some(pricing)
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn model_cost_multiplies_tokens_by_per_token_rate() {
        // glm-5.2 on zai: $1.40 / 1M input, $4.40 / 1M output.
        // Hand-computed for 2,000,000 input + 500,000 output tokens:
        //   input:  2_000_000 * 1.40 / 1_000_000 = 2.80
        //   output:   500_000 * 4.40 / 1_000_000 = 2.20
        //   total = 5.00
        let cost = model_cost("zai", "glm-5.2", 2_000_000, 500_000).unwrap();
        assert!((cost - 5.00).abs() < 1e-9, "got {cost}");
    }

    #[test]
    fn model_cost_is_none_for_unpriced_pair() {
        assert!(model_cost("ollama", "llama3", 1_000, 1_000).is_none());
    }

    #[test]
    fn model_cost_of_zero_tokens_is_zero_not_none_for_priced_model() {
        assert_eq!(model_cost("zai", "glm-5.2", 0, 0), Some(0.0));
    }

    #[test]
    fn claude_sonnet_priced_natively_and_on_bedrock() {
        // Sonnet: $3/M input, $15/M output; cache read 0.1x = $0.30/M,
        // cache write 1.25x = $3.75/M.
        for (provider, model) in [
            ("anthropic", "claude-sonnet-4-20250514"),
            ("bedrock", "us.anthropic.claude-sonnet-4-20250514-v1:0"),
        ] {
            let p = provider_model_pricing(provider, model).unwrap();
            assert_eq!(p.input_token_cost, 3.0 / 1_000_000.0);
            assert_eq!(p.output_token_cost, 15.0 / 1_000_000.0);
            // Cache read = 0.1x input, write = 1.25x input (float-tolerant).
            assert!((p.cache_read_cost.unwrap() - 0.30 / 1_000_000.0).abs() < 1e-15);
            assert!((p.cache_write_cost.unwrap() - 3.75 / 1_000_000.0).abs() < 1e-15);
        }
    }

    #[test]
    fn claude_opus_and_haiku_tiers_are_distinct() {
        let opus = provider_model_pricing("anthropic", "claude-opus-4-1-20250805").unwrap();
        assert_eq!(opus.input_token_cost, 15.0 / 1_000_000.0);
        let haiku = provider_model_pricing("anthropic", "claude-haiku-4-5").unwrap();
        assert_eq!(haiku.input_token_cost, 0.80 / 1_000_000.0);
    }

    #[test]
    fn non_claude_bedrock_model_is_unpriced() {
        assert!(provider_model_pricing("bedrock", "amazon.titan-text-express-v1").is_none());
        assert!(provider_model_pricing("bedrock", "meta.llama3-70b-instruct-v1:0").is_none());
    }

    #[test]
    fn model_cost_with_cache_prices_every_bucket() {
        // Sonnet rates: input $3/M, output $15/M, cache_read $0.30/M,
        // cache_write $3.75/M. Hand-computed for:
        //   input 1,000,000  -> 1,000,000 * 3.00 / 1e6 = 3.00
        //   output 200,000   ->   200,000 * 15.0 / 1e6 = 3.00
        //   cache_read 500,000 -> 500,000 * 0.30 / 1e6 = 0.15
        //   cache_creation 100,000 -> 100,000 * 3.75 / 1e6 = 0.375
        //   total = 6.525
        let tc = model_cost_with_cache(
            "anthropic",
            "claude-sonnet-4-20250514",
            1_000_000,
            200_000,
            500_000,
            100_000,
        )
        .unwrap();
        assert!((tc.cost - 6.525).abs() < 1e-9, "got {}", tc.cost);
        assert!(!tc.cache_excluded);
    }

    #[test]
    fn model_cost_with_cache_flags_excluded_when_model_has_no_cache_rate() {
        // zai has input/output rates but no cache pricing. A turn with cache
        // tokens prices only input+output and flags the exclusion.
        // glm-5.2: input $1.40/M, output $4.40/M.
        //   input 1,000,000 -> 1.40 ; output 0 -> 0 ; cache omitted.
        let tc = model_cost_with_cache("zai", "glm-5.2", 1_000_000, 0, 900_000, 0).unwrap();
        assert!((tc.cost - 1.40).abs() < 1e-9, "got {}", tc.cost);
        assert!(tc.cache_excluded);
    }

    #[test]
    fn model_cost_with_cache_no_flag_when_no_cache_tokens() {
        let tc = model_cost_with_cache("zai", "glm-5.2", 1_000_000, 0, 0, 0).unwrap();
        assert!(!tc.cache_excluded);
    }

    #[test]
    fn model_cost_with_cache_is_none_for_unpriced_pair() {
        assert!(model_cost_with_cache("ollama", "llama3", 10, 10, 5, 5).is_none());
    }
}
