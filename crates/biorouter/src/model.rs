use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;
use utoipa::ToSchema;

use crate::agents::effort::ReasoningEffort;

const DEFAULT_CONTEXT_LIMIT: usize = 128_000;

#[derive(Debug, Clone, Deserialize)]
struct PredefinedModel {
    name: String,
    #[serde(default)]
    context_limit: Option<usize>,
    #[serde(default)]
    request_params: Option<HashMap<String, Value>>,
}

fn get_predefined_models() -> Vec<PredefinedModel> {
    static PREDEFINED_MODELS: Lazy<Vec<PredefinedModel>> =
        Lazy::new(|| match std::env::var("BIOROUTER_PREDEFINED_MODELS") {
            Ok(json_str) => serde_json::from_str(&json_str).unwrap_or_else(|e| {
                tracing::warn!("Failed to parse BIOROUTER_PREDEFINED_MODELS: {}", e);
                Vec::new()
            }),
            Err(_) => Vec::new(),
        });
    PREDEFINED_MODELS.clone()
}

fn find_predefined_model(model_name: &str) -> Option<PredefinedModel> {
    get_predefined_models()
        .into_iter()
        .find(|m| m.name == model_name)
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Environment variable '{0}' not found")]
    EnvVarMissing(String),
    #[error("Invalid value for '{0}': '{1}' - {2}")]
    InvalidValue(String, String, String),
    #[error("Value for '{0}' is out of valid range: {1}")]
    InvalidRange(String, String),
}

/// Exact, per-model context windows — the single source of truth.
///
/// Every model any provider advertises has its **own** entry. A model never
/// inherits another model's window, and a config that names two models (a main
/// model and a fast model) never reconciles them into one number: each is
/// resolved independently from this table. Lookup is exact-match on the full
/// model id; [`MODEL_SPECIFIC_LIMITS`] is only a fallback for ids that are not
/// known ahead of time (Ollama tags, arbitrary OpenRouter ids, raw HF specs).
///
/// Verified July 2026 against each vendor's docs. The per-provider catalogs are
/// authoritative for their own models; `tests/context_windows.rs` fails the
/// build if a provider declares a window that drifts from this table, or if a
/// provider advertises a model that has no entry here.
static MODEL_CONTEXT_WINDOWS: Lazy<HashMap<&'static str, usize>> = Lazy::new(|| {
    HashMap::from([
        // ── OpenAI ──
        ("gpt-4.1", 1_047_576),
        ("gpt-4.1-2025-04-14", 1_047_576),
        ("gpt-4.1-mini", 1_047_576),
        ("gpt-4.1-mini-2025-04-14", 1_047_576),
        ("gpt-4o-2024-11-20", 128_000),
        ("gpt-4o-mini", 128_000),
        ("gpt-5", 400_000),
        ("gpt-5-2025-08-07", 400_000),
        ("gpt-5-mini", 400_000),
        ("gpt-5-nano", 400_000),
        ("gpt-5.1", 400_000),
        ("gpt-5.1-2025-11-13", 400_000),
        ("gpt-5.2", 400_000),
        ("gpt-5.2-2025-12-11", 400_000),
        ("gpt-5.3-codex", 400_000),
        ("gpt-5.4", 1_050_000),
        ("gpt-5.4-2026-03-05", 1_050_000),
        ("gpt-5.4-mini", 400_000),
        ("gpt-5.4-mini-2026-03-17", 400_000),
        ("gpt-5.4-nano", 400_000),
        ("gpt-5.4-nano-2026-03-17", 400_000),
        ("gpt-5.4-pro", 1_050_000),
        ("gpt-5.5", 1_050_000),
        ("gpt-5.5-2026-04-24", 1_050_000),
        ("gpt-5.5-pro", 1_050_000),
        ("gpt-5.6", 1_050_000),
        ("gpt-5.6-luna", 1_050_000),
        ("gpt-5.6-sol", 1_050_000),
        ("gpt-5.6-terra", 1_050_000),
        ("o3", 200_000),
        ("o3-2025-04-16", 200_000),
        // ── Anthropic ──
        ("anthropic/claude-haiku-4.5", 200_000),
        ("anthropic/claude-opus-4.8", 1_000_000),
        ("anthropic/claude-sonnet-4.6", 1_000_000),
        ("anthropic/claude-sonnet-5", 1_000_000),
        ("claude-4-sonnet", 200_000),
        ("claude-fable-5", 1_000_000),
        ("claude-haiku-4-5", 200_000),
        ("claude-haiku-4-5-20251001", 200_000),
        ("claude-haiku-4-5@20251001", 200_000),
        ("claude-haiku-4.5", 200_000),
        ("claude-opus-4-1", 200_000),
        ("claude-opus-4-5", 200_000),
        ("claude-opus-4-5-20251101", 200_000),
        ("claude-opus-4-5@20251101", 200_000),
        ("claude-opus-4-6", 1_000_000),
        ("claude-opus-4-7", 1_000_000),
        ("claude-opus-4-8", 1_000_000),
        ("claude-opus-4.8", 1_000_000),
        ("claude-opus-5", 1_000_000),
        ("claude-sonnet-4-5", 200_000),
        ("claude-sonnet-4-5-20250929", 200_000),
        ("claude-sonnet-4-5@20250929", 200_000),
        ("claude-sonnet-4-6", 1_000_000),
        ("claude-sonnet-4.5", 200_000),
        ("claude-sonnet-4.6", 1_000_000),
        ("claude-sonnet-5", 1_000_000),
        ("us.anthropic.claude-haiku-4-5-20251001-v1:0", 200_000),
        ("us.anthropic.claude-opus-4-5-20251101-v1:0", 200_000),
        ("us.anthropic.claude-opus-4-6-v1", 1_000_000),
        ("us.anthropic.claude-opus-4-8", 1_000_000),
        ("us.anthropic.claude-sonnet-4-5-20250929-v1:0", 200_000),
        ("us.anthropic.claude-sonnet-4-6", 1_000_000),
        ("us.anthropic.claude-sonnet-5", 1_000_000),
        // ── Google ──
        ("gemini-2.5-flash", 1_048_576),
        ("gemini-2.5-flash-image", 1_048_576),
        ("gemini-2.5-flash-lite", 1_048_576),
        ("gemini-2.5-flash-preview-tts", 1_048_576),
        ("gemini-2.5-pro", 1_048_576),
        ("gemini-2.5-pro-preview-tts", 1_048_576),
        ("gemini-3-flash-preview", 1_048_576),
        ("gemini-3-pro", 1_048_576),
        ("gemini-3-pro-image", 1_048_576),
        ("gemini-3.1-flash-image", 1_048_576),
        ("gemini-3.1-flash-lite", 1_048_576),
        ("gemini-3.1-pro", 1_048_576),
        ("gemini-3.1-pro-preview", 1_048_576),
        ("gemini-3.5-flash", 1_048_576),
        ("gemma4", 131_072),
        ("gemma4-12b", 262_144),
        ("gemma4-26b", 262_144),
        ("gemma4-31b", 262_144),
        ("gemma4-e2b", 131_072),
        // ── Meta Llama ──
        ("llama-3.1-8b-instant", 131_072),
        ("llama-3.2-3b", 128_000),
        ("llama-3.3-70b", 128_000),
        ("llama-3.3-70b-versatile", 131_072),
        // ── Qwen ──
        ("qwen/qwen3-coder-next", 262_144),
        ("qwen3.6", 262_144),
        ("qwen3.6-27b", 262_144),
        ("qwen3.6-35b-a3b", 262_144),
        // ── DeepSeek ──
        ("deepseek-chat", 1_000_000),
        ("deepseek-reasoner", 1_000_000),
        ("deepseek-v4-flash", 1_000_000),
        ("deepseek-v4-pro", 1_000_000),
        ("deepseek/deepseek-v4-flash", 1_000_000),
        ("deepseek/deepseek-v4-pro", 1_000_000),
        // ── Z.ai GLM ──
        ("glm-4.5", 131_072),
        ("glm-4.5-air", 131_072),
        ("glm-4.6", 200_000),
        ("glm-4.7", 200_000),
        ("glm-5", 200_000),
        ("glm-5-turbo", 200_000),
        ("glm-5.1", 202_752),
        ("glm-5.2", 1_048_576),
        // ── xAI Grok ──
        ("grok-4.20-0309-non-reasoning", 1_000_000),
        ("grok-4.20-0309-reasoning", 1_000_000),
        ("grok-4.20-multi-agent-0309", 1_000_000),
        ("grok-4.3", 1_000_000),
        ("grok-4.3-latest", 1_000_000),
        ("grok-build-0.1", 256_000),
        ("grok-imagine-image", 131_072),
        ("grok-imagine-image-quality", 131_072),
        ("grok-latest", 131_072),
        ("x-ai/grok-4.20", 1_000_000),
        ("x-ai/grok-4.3", 1_000_000),
        ("x-ai/grok-build-0.1", 256_000),
        // ── Moonshot Kimi ──
        ("moonshotai/kimi-k2.6", 262_144),
        ("moonshotai/kimi-k2.7-code", 262_144),
        // ── Xiaomi MiMo ──
        ("mimo-v2-omni", 262_144),
        ("mimo-v2-pro", 262_144),
        ("mimo-v2.5", 1_000_000),
        ("mimo-v2.5-pro", 1_000_000),
        // ── Inception Mercury ──
        ("mercury-2", 128_000),
        ("mercury-coder", 128_000),
        // ── Mistral ──
        ("codestral-2508", 128_000),
        ("devstral-2512", 262_144),
        ("magistral-medium-2509", 128_000),
        ("ministral-8b-2512", 262_144),
        ("mistral-large-2512", 262_144),
        ("mistral-medium-2508", 128_000),
        ("mistral-medium-3-5", 262_144),
        ("mistral-medium-latest", 128_000),
        ("mistral-small-2603", 262_144),
        ("mistral-small-3-2-24b-instruct", 128_000),
        // ── MiniMax ──
        ("minimax/minimax-m3", 1_048_576),
        // ── Groq-hosted ──
        ("groq/compound", 131_072),
        ("groq/compound-mini", 131_072),
        ("openai/gpt-oss-120b", 131_072),
        ("openai/gpt-oss-20b", 131_072),
        ("openai/gpt-oss-safeguard-20b", 131_072),
        // ── Databricks ──
        ("databricks-claude-fable-5", 1_000_000),
        ("databricks-claude-haiku-4-5", 200_000),
        ("databricks-claude-opus-4-5", 200_000),
        ("databricks-claude-opus-4-6", 1_000_000),
        ("databricks-claude-opus-4-7", 1_000_000),
        ("databricks-claude-opus-4-8", 1_000_000),
        ("databricks-claude-sonnet-4-5", 200_000),
        ("databricks-claude-sonnet-4-6", 1_000_000),
        ("databricks-gemini-2-5-flash", 1_048_576),
        ("databricks-gemini-2-5-pro", 1_048_576),
        ("databricks-gemini-3-1-flash-lite", 1_048_576),
        ("databricks-gemini-3-1-pro", 1_048_576),
        ("databricks-gemini-3-5-flash", 1_048_576),
        ("databricks-gemini-3-flash", 1_048_576),
        ("databricks-gpt-5-4", 400_000),
        ("databricks-gpt-5-4-mini", 400_000),
        ("databricks-gpt-5-4-nano", 400_000),
        ("databricks-gpt-5-5", 400_000),
        ("databricks-gpt-5-5-pro", 400_000),
        ("databricks-llama-4-maverick", 128_000),
        ("databricks-meta-llama-3-3-70b-instruct", 128_000),
        // ── Other ──
        ("google/gemini-3.1-pro-preview", 1_048_576),
        ("google/gemini-3.5-flash", 1_048_576),
        ("o4-mini-2025-04-16", 200_000),
        ("sagemaker-tgi-endpoint", 128_000),
        ("z-ai/glm-5.1", 202_752),
        ("z-ai/glm-5.2", 1_048_576),
    ])
});

/// Heuristic **fallback only**, for model ids the exact registry above cannot
/// know ahead of time: Ollama tags (`qwen3.6:latest`), arbitrary OpenRouter ids,
/// raw `owner/repo:QUANT` HF specs. Patterns are matched with `str::contains`,
/// first match wins — keep more specific patterns before their prefixes.
///
/// Never add a model here that a provider advertises; give it an exact entry in
/// [`MODEL_CONTEXT_WINDOWS`] instead, or a broad pattern will silently shadow it.
static MODEL_SPECIFIC_LIMITS: Lazy<Vec<(&'static str, usize)>> = Lazy::new(|| {
    vec![
        // openai
        ("gpt-5.6", 1_050_000), // covers gpt-5.6 and its -sol/-terra/-luna variants
        ("gpt-5.5", 1_050_000), // covers gpt-5.5 and gpt-5.5-pro
        ("gpt-5.4-mini", 400_000),
        ("gpt-5.4-nano", 400_000),
        ("gpt-5.4", 1_050_000), // covers gpt-5.4 and gpt-5.4-pro
        ("gpt-5.3-codex", 400_000),
        ("gpt-5.2", 400_000),
        ("gpt-5.1", 400_000),
        ("gpt-5", 400_000), // gpt-5 / gpt-5-mini / gpt-5-nano / gpt-5-pro
        ("gpt-4-turbo", 128_000),
        ("gpt-4.1", 1_047_576),
        ("gpt-4-1", 1_047_576),
        ("gpt-4o", 128_000),
        ("o4-mini", 200_000),
        ("o3-mini", 200_000),
        ("o3", 200_000),
        ("o1", 200_000),
        // anthropic — Fable 5 / Opus 5 / Opus 4.6+ / Sonnet 4.6 are 1M; older
        // Claude 200k. Dotted variants cover OpenRouter/Copilot-style IDs.
        ("claude-fable-5", 1_000_000),
        ("claude-sonnet-5", 1_000_000),
        ("claude-opus-5", 1_000_000),
        ("claude-opus-4-8", 1_000_000),
        ("claude-opus-4.8", 1_000_000),
        ("claude-opus-4-7", 1_000_000),
        ("claude-opus-4.7", 1_000_000),
        ("claude-opus-4-6", 1_000_000),
        ("claude-opus-4.6", 1_000_000),
        ("claude-sonnet-4-6", 1_000_000),
        ("claude-sonnet-4.6", 1_000_000),
        ("claude", 200_000),
        // google — all Gemini 2.x/3.x text models are 1,048,576 in
        ("gemini-3", 1_048_576),
        ("gemini-2", 1_048_576),
        ("gemini-1.5-flash", 1_000_000),
        ("gemini-1", 128_000),
        ("gemma-4-e2b", 128_000),
        ("gemma-4-e4b", 128_000),
        ("gemma-4", 256_000),
        ("gemma4", 131_072),
        ("qwen3.6", 262_144),
        ("gemma-3-27b", 128_000),
        ("gemma-3-12b", 128_000),
        ("gemma-3-4b", 128_000),
        ("gemma-3-1b", 32_000),
        ("gemma3-27b", 128_000),
        ("gemma3-12b", 128_000),
        ("gemma3-4b", 128_000),
        ("gemma3-1b", 32_000),
        ("gemma-2-27b", 8_192),
        ("gemma-2-9b", 8_192),
        ("gemma-2-2b", 8_192),
        ("gemma2-", 8_192),
        ("gemma-7b", 8_192),
        ("gemma-2b", 8_192),
        ("gemma1", 8_192),
        ("gemma", 8_192),
        // facebook
        ("llama-2-1b", 32_000),
        ("llama", 128_000),
        // qwen
        ("qwen3-coder-next", 262_144),
        ("qwen3-coder", 1_048_576),
        ("qwen2-7b", 128_000),
        ("qwen2-14b", 128_000),
        ("qwen2-32b", 131_072),
        ("qwen2-70b", 262_144),
        ("qwen2", 128_000),
        ("qwen3-32b", 131_072),
        // xai — grok-4.3 / grok-4.20 are 1M; grok-build-0.1 is 256k
        ("grok-build", 256_000),
        ("grok-code-fast-1", 256_000),
        ("grok-4", 1_000_000),
        ("grok", 131_072),
        // zai (Zhipu GLM) — GLM-4.6/4.5 are 128k–200k; default to 128k
        ("glm-5.2", 1_048_576),
        ("glm-5.1", 202_752),
        ("glm-4.7", 200_000),
        ("glm-4.6", 200_000),
        ("glm-5", 200_000),
        ("glm", 131_072),
        // deepseek — V4 family is 1M
        ("deepseek-v4", 1_000_000),
        // moonshot — k2.5/k2.6 are 256k, original k2 is 128k
        ("kimi-k2.7", 262_144),
        ("kimi-k2.5", 262_144),
        ("kimi-k2.6", 262_144),
        ("kimi-k2", 131_072),
        // MiniMax and Inception coding models
        ("minimax-m3", 1_048_576),
        ("mercury-2", 128_000),
        ("mercury-edit-2", 32_000),
        // Inception raised Mercury Coder to 128k; the old 32k here was stale and
        // shadowed inception.json's (correct) 128k declaration.
        ("mercury-coder", 128_000),
        // xiaomi mimo — MiMo v2.5 family advertises ~1M; MiMo v2 family ~256k
        ("mimo-v2.5", 1_000_000), // covers mimo-v2.5 and mimo-v2.5-pro
        ("mimo-v2", 262_144),     // covers mimo-v2-pro and mimo-v2-omni
        ("mimo", 131_072),        // any other mimo variant
    ]
});

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ModelConfig {
    pub model_name: String,
    pub context_limit: Option<usize>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<i32>,
    pub toolshim: bool,
    pub toolshim_model: Option<String>,
    pub fast_model: Option<String>,
    /// Provider-specific request parameters (e.g., anthropic_beta headers)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_params: Option<HashMap<String, Value>>,
    /// BR-63: how hard to think on this call. Set per-turn from the session's
    /// reasoning-effort control; `None` (the default) means "whatever the
    /// provider/model already does", so nothing about the request changes.
    /// Each provider format takes what it understands — `reasoning_effort` for
    /// the OpenAI families, a `thinking` budget for Anthropic — and ignores it
    /// otherwise, so an unsupported provider degrades to no-op rather than
    /// erroring.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffort>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelLimitConfig {
    pub pattern: String,
    pub context_limit: usize,
}

impl ModelConfig {
    pub fn new(model_name: &str) -> Result<Self, ConfigError> {
        Self::new_with_context_env(model_name.to_string(), None)
    }

    pub fn new_with_context_env(
        model_name: String,
        context_env_var: Option<&str>,
    ) -> Result<Self, ConfigError> {
        let predefined = find_predefined_model(&model_name);

        let context_limit = if let Some(ref pm) = predefined {
            if let Some(env_var) = context_env_var {
                if let Ok(val) = std::env::var(env_var) {
                    Some(Self::validate_context_limit(&val, env_var)?)
                } else {
                    pm.context_limit
                }
            } else if let Ok(val) = std::env::var("BIOROUTER_CONTEXT_LIMIT") {
                Some(Self::validate_context_limit(
                    &val,
                    "BIOROUTER_CONTEXT_LIMIT",
                )?)
            } else {
                pm.context_limit
            }
        } else {
            Self::parse_context_limit(&model_name, context_env_var)?
        };

        let request_params = predefined.and_then(|pm| pm.request_params);

        let temperature = Self::parse_temperature()?;
        let max_tokens = Self::parse_max_tokens()?;
        let toolshim = Self::parse_toolshim()?;
        let toolshim_model = Self::parse_toolshim_model()?;

        Ok(Self {
            model_name,
            context_limit,
            temperature,
            max_tokens,
            toolshim,
            toolshim_model,
            fast_model: None,
            request_params,
            reasoning_effort: None,
        })
    }

    /// Resolve `model_name`'s own context window. Never consults any other
    /// model — a config's fast model has no bearing on its main model's window,
    /// and vice versa.
    fn parse_context_limit(
        model_name: &str,
        custom_env_var: Option<&str>,
    ) -> Result<Option<usize>, ConfigError> {
        // An explicit environment override wins over the model's own window.
        if let Some(env_var) = custom_env_var {
            if let Ok(val) = std::env::var(env_var) {
                return Self::validate_context_limit(&val, env_var).map(Some);
            }
        }
        if let Ok(val) = std::env::var("BIOROUTER_CONTEXT_LIMIT") {
            return Self::validate_context_limit(&val, "BIOROUTER_CONTEXT_LIMIT").map(Some);
        }

        Ok(Self::resolve_context_window(model_name))
    }

    fn validate_context_limit(val: &str, env_var: &str) -> Result<usize, ConfigError> {
        let limit = val.parse::<usize>().map_err(|_| {
            ConfigError::InvalidValue(
                env_var.to_string(),
                val.to_string(),
                "must be a positive integer".to_string(),
            )
        })?;

        if limit < 4 * 1024 {
            return Err(ConfigError::InvalidRange(
                env_var.to_string(),
                "must be greater than 4K".to_string(),
            ));
        }

        Ok(limit)
    }

    fn parse_temperature() -> Result<Option<f32>, ConfigError> {
        if let Ok(val) = std::env::var("BIOROUTER_TEMPERATURE") {
            let temp = val.parse::<f32>().map_err(|_| {
                ConfigError::InvalidValue(
                    "BIOROUTER_TEMPERATURE".to_string(),
                    val.clone(),
                    "must be a valid number".to_string(),
                )
            })?;
            if temp < 0.0 {
                return Err(ConfigError::InvalidRange(
                    "BIOROUTER_TEMPERATURE".to_string(),
                    val,
                ));
            }
            Ok(Some(temp))
        } else {
            Ok(None)
        }
    }

    fn parse_max_tokens() -> Result<Option<i32>, ConfigError> {
        match crate::config::Config::global().get_param::<i32>("BIOROUTER_MAX_TOKENS") {
            Ok(tokens) => {
                if tokens <= 0 {
                    return Err(ConfigError::InvalidRange(
                        "biorouter_max_tokens".to_string(),
                        "must be greater than 0".to_string(),
                    ));
                }
                Ok(Some(tokens))
            }
            Err(crate::config::ConfigError::NotFound(_)) => Ok(None),
            Err(e) => Err(ConfigError::InvalidValue(
                "biorouter_max_tokens".to_string(),
                String::new(),
                e.to_string(),
            )),
        }
    }

    fn parse_toolshim() -> Result<bool, ConfigError> {
        if let Ok(val) = std::env::var("BIOROUTER_TOOLSHIM") {
            match val.to_lowercase().as_str() {
                "1" | "true" | "yes" | "on" => Ok(true),
                "0" | "false" | "no" | "off" => Ok(false),
                _ => Err(ConfigError::InvalidValue(
                    "BIOROUTER_TOOLSHIM".to_string(),
                    val,
                    "must be one of: 1, true, yes, on, 0, false, no, off".to_string(),
                )),
            }
        } else {
            Ok(false)
        }
    }

    fn parse_toolshim_model() -> Result<Option<String>, ConfigError> {
        match std::env::var("BIOROUTER_TOOLSHIM_OLLAMA_MODEL") {
            Ok(val) if val.trim().is_empty() => Err(ConfigError::InvalidValue(
                "BIOROUTER_TOOLSHIM_OLLAMA_MODEL".to_string(),
                val,
                "cannot be empty if set".to_string(),
            )),
            Ok(val) => Ok(Some(val)),
            Err(_) => Ok(None),
        }
    }

    fn get_model_specific_limit(model_name: &str) -> Option<usize> {
        MODEL_SPECIFIC_LIMITS
            .iter()
            .find(|(pattern, _)| model_name.contains(pattern))
            .map(|(_, limit)| *limit)
    }

    /// This model's own context window: exact registry entry first, then the
    /// substring fallback for ids we can't enumerate ahead of time. Returns
    /// `None` when neither knows the model.
    fn resolve_context_window(model_name: &str) -> Option<usize> {
        MODEL_CONTEXT_WINDOWS
            .get(model_name)
            .copied()
            .or_else(|| Self::get_model_specific_limit(model_name))
    }

    /// The context window `model_name` has by virtue of being that model,
    /// ignoring any per-config or environment override. Use this when you need a
    /// model's intrinsic window (e.g. to advertise it in provider metadata)
    /// rather than the effective window of a particular session.
    pub fn context_window_for(model_name: &str) -> usize {
        Self::resolve_context_window(model_name).unwrap_or(DEFAULT_CONTEXT_LIMIT)
    }

    /// Whether this model has its **own** entry in [`MODEL_CONTEXT_WINDOWS`],
    /// rather than borrowing a window from a substring pattern or the default.
    /// Every model a provider advertises must satisfy this — see the
    /// `context_windows` integration test.
    pub fn has_declared_context_window(model_name: &str) -> bool {
        MODEL_CONTEXT_WINDOWS.contains_key(model_name)
    }

    /// The (pattern, window) list served to clients that resolve a window from a
    /// model name themselves — the desktop chat input does
    /// `modelName.includes(pattern)`, first match wins.
    ///
    /// Every exact model id comes first, **longest id first**, so a model always
    /// matches its own entry before a shorter generic pattern can claim it
    /// (`gpt-5.6-luna` before `gpt-5.6` before `gpt-5`). The heuristic patterns
    /// follow, for ids the registry doesn't know. Without the exact entries the
    /// UI would report a different window than the agent actually uses.
    pub fn get_all_model_limits() -> Vec<ModelLimitConfig> {
        let mut exact: Vec<(&str, usize)> = MODEL_CONTEXT_WINDOWS
            .iter()
            .map(|(name, limit)| (*name, *limit))
            .collect();
        // Longest first so an exact id wins; then by name to keep output stable
        // (HashMap iteration order is not).
        exact.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(b.0)));

        exact
            .into_iter()
            .chain(MODEL_SPECIFIC_LIMITS.iter().map(|(p, l)| (*p, *l)))
            .map(|(pattern, context_limit)| ModelLimitConfig {
                pattern: pattern.to_string(),
                context_limit,
            })
            .collect()
    }

    pub fn with_context_limit(mut self, limit: Option<usize>) -> Self {
        if limit.is_some() {
            self.context_limit = limit;
        }
        self
    }

    pub fn with_temperature(mut self, temp: Option<f32>) -> Self {
        self.temperature = temp;
        self
    }

    pub fn with_max_tokens(mut self, tokens: Option<i32>) -> Self {
        self.max_tokens = tokens;
        self
    }

    pub fn with_toolshim(mut self, toolshim: bool) -> Self {
        self.toolshim = toolshim;
        self
    }

    pub fn with_toolshim_model(mut self, model: Option<String>) -> Self {
        self.toolshim_model = model;
        self
    }

    pub fn with_fast(mut self, fast_model: String) -> Self {
        self.fast_model = Some(fast_model);
        self
    }

    pub fn with_request_params(mut self, params: Option<HashMap<String, Value>>) -> Self {
        self.request_params = params;
        self
    }

    /// BR-63: pin the reasoning effort for calls made with this config.
    pub fn with_reasoning_effort(mut self, effort: Option<ReasoningEffort>) -> Self {
        self.reasoning_effort = effort;
        self
    }

    /// Switch to the configured fast model, treating it as the separate model it
    /// is: it gets **its own** context window (and its own predefined request
    /// params), not the main model's. Sampling settings that describe how we call
    /// a model — temperature, max tokens, toolshim — carry over.
    ///
    /// Rebuilt via [`Self::new`] so an explicit `BIOROUTER_CONTEXT_LIMIT` still
    /// applies to the fast model too. If that fails (only possible when an env
    /// var holds an invalid value), fall back to renaming and re-resolving.
    pub fn use_fast_model(&self) -> Self {
        let Some(fast_model) = &self.fast_model else {
            return self.clone();
        };

        let mut config = Self::new(fast_model).unwrap_or_else(|_| {
            let mut fallback = self.clone();
            fallback.model_name = fast_model.clone();
            fallback.context_limit = Self::resolve_context_window(fast_model);
            fallback
        });

        config.temperature = self.temperature;
        config.max_tokens = self.max_tokens;
        config.toolshim = self.toolshim;
        config.toolshim_model = self.toolshim_model.clone();
        // Effort describes *how we call a model*, not which model it is, so the
        // turn's effort carries over to the fast model too (BR-63).
        config.reasoning_effort = self.reasoning_effort;
        // Keep `fast_model` set so callers can tell a fast config from the main
        // one (see `Provider::complete_fast`'s fallback check).
        config.fast_model = self.fast_model.clone();
        // Prefer the fast model's own predefined params; inherit only if it has none.
        if config.request_params.is_none() {
            config.request_params = self.request_params.clone();
        }
        config
    }

    /// The effective context window for **this config's** model.
    ///
    /// An explicit override (set via [`Self::with_context_limit`] or an env var)
    /// wins; otherwise the model's own registry window is used. The fast model,
    /// if any, is a different model and never constrains this one — call
    /// [`Self::use_fast_model`] to get a config for it.
    pub fn context_limit(&self) -> usize {
        if let Some(limit) = self.context_limit {
            return limit;
        }
        Self::context_window_for(&self.model_name)
    }

    pub fn new_or_fail(model_name: &str) -> ModelConfig {
        ModelConfig::new(model_name)
            .unwrap_or_else(|_| panic!("Failed to create model config for {}", model_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_max_tokens_valid() {
        let _guard = env_lock::lock_env([("BIOROUTER_MAX_TOKENS", Some("4096"))]);
        let result = ModelConfig::parse_max_tokens().unwrap();
        assert_eq!(result, Some(4096));
    }

    #[test]
    fn test_parse_max_tokens_not_set() {
        let _guard = env_lock::lock_env([("BIOROUTER_MAX_TOKENS", None::<&str>)]);
        let result = ModelConfig::parse_max_tokens().unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_max_tokens_invalid_string() {
        let _guard = env_lock::lock_env([("BIOROUTER_MAX_TOKENS", Some("not_a_number"))]);
        let result = ModelConfig::parse_max_tokens();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::InvalidValue(..)));
    }

    #[test]
    fn test_parse_max_tokens_zero() {
        let _guard = env_lock::lock_env([("BIOROUTER_MAX_TOKENS", Some("0"))]);
        let result = ModelConfig::parse_max_tokens();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::InvalidRange(..)));
    }

    #[test]
    fn test_parse_max_tokens_negative() {
        let _guard = env_lock::lock_env([("BIOROUTER_MAX_TOKENS", Some("-100"))]);
        let result = ModelConfig::parse_max_tokens();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::InvalidRange(..)));
    }

    #[test]
    fn test_model_config_with_max_tokens_env() {
        let _guard = env_lock::lock_env([
            ("BIOROUTER_MAX_TOKENS", Some("8192")),
            ("BIOROUTER_TEMPERATURE", None::<&str>),
            ("BIOROUTER_CONTEXT_LIMIT", None::<&str>),
            ("BIOROUTER_TOOLSHIM", None::<&str>),
            ("BIOROUTER_TOOLSHIM_OLLAMA_MODEL", None::<&str>),
        ]);
        let config = ModelConfig::new("test-model").unwrap();
        assert_eq!(config.max_tokens, Some(8192));
    }

    #[test]
    fn test_model_config_without_max_tokens_env() {
        let _guard = env_lock::lock_env([
            ("BIOROUTER_MAX_TOKENS", None::<&str>),
            ("BIOROUTER_TEMPERATURE", None::<&str>),
            ("BIOROUTER_CONTEXT_LIMIT", None::<&str>),
            ("BIOROUTER_TOOLSHIM", None::<&str>),
            ("BIOROUTER_TOOLSHIM_OLLAMA_MODEL", None::<&str>),
        ]);
        let config = ModelConfig::new("test-model").unwrap();
        assert_eq!(config.max_tokens, None);
    }

    // ── Per-model context windows: two models are two models ────────────────

    /// Pin the env so these exercise the registry, not an override. Other tests
    /// in this workspace set BIOROUTER_CONTEXT_LIMIT process-wide.
    fn clean_env() -> impl Drop {
        env_lock::lock_env([
            ("BIOROUTER_CONTEXT_LIMIT", None::<&str>),
            ("BIOROUTER_PREDEFINED_MODELS", None::<&str>),
        ])
    }

    #[test]
    fn a_fast_model_does_not_shrink_the_main_models_window() {
        let _guard = clean_env();
        // gpt-5.6 is 1.05M; the default fast model gpt-5.4-mini is only 400k.
        let cfg = ModelConfig::new("gpt-5.6")
            .unwrap()
            .with_fast("gpt-5.4-mini".to_string());
        assert_eq!(
            cfg.context_limit(),
            1_050_000,
            "the main model keeps its own window regardless of the fast model"
        );
    }

    #[test]
    fn the_fast_model_gets_its_own_window_not_the_main_models() {
        let _guard = clean_env();
        let main = ModelConfig::new("gpt-5.6")
            .unwrap()
            .with_fast("gpt-5.4-mini".to_string());
        let fast = main.use_fast_model();

        assert_eq!(fast.model_name, "gpt-5.4-mini");
        assert_eq!(
            fast.context_limit(),
            400_000,
            "the fast model must not inherit the main model's 1.05M budget"
        );
        assert_eq!(main.context_limit(), 1_050_000, "main config is untouched");
    }

    #[test]
    fn use_fast_model_is_a_noop_without_a_fast_model() {
        let _guard = clean_env();
        let cfg = ModelConfig::new("gpt-5.6").unwrap();
        let same = cfg.use_fast_model();
        assert_eq!(same.model_name, "gpt-5.6");
        assert_eq!(same.context_limit(), 1_050_000);
    }

    #[test]
    fn use_fast_model_carries_over_call_settings() {
        let _guard = clean_env();
        let main = ModelConfig::new("gpt-5.6")
            .unwrap()
            .with_fast("gpt-5.4-mini".to_string())
            .with_temperature(Some(0.25))
            .with_max_tokens(Some(4096))
            .with_toolshim(true)
            .with_toolshim_model(Some("shim-model".to_string()));
        let fast = main.use_fast_model();

        assert_eq!(fast.temperature, Some(0.25));
        assert_eq!(fast.max_tokens, Some(4096));
        assert!(fast.toolshim);
        assert_eq!(fast.toolshim_model.as_deref(), Some("shim-model"));
        // Still knows it is the fast variant, so `complete_fast` can detect fallback.
        assert_eq!(fast.fast_model.as_deref(), Some("gpt-5.4-mini"));
    }

    #[test]
    fn an_unknown_main_model_is_not_capped_by_the_fast_model() {
        let _guard = clean_env();
        // Previously the min() branch let a known fast model cap an unknown main
        // model. An unknown model gets the default window, full stop.
        let cfg = ModelConfig::new("some-unlisted-model-xyz")
            .unwrap()
            .with_fast("gemma-3-1b".to_string()); // 32k
        assert_eq!(cfg.context_limit(), DEFAULT_CONTEXT_LIMIT);
    }

    #[test]
    fn an_explicit_override_still_wins_for_the_model_it_was_set_on() {
        let _guard = clean_env();
        let cfg = ModelConfig::new("gpt-5.6")
            .unwrap()
            .with_context_limit(Some(50_000));
        assert_eq!(cfg.context_limit(), 50_000);
    }

    #[test]
    fn a_global_env_override_applies_to_the_fast_model_too() {
        let _guard = env_lock::lock_env([
            ("BIOROUTER_CONTEXT_LIMIT", Some("64000")),
            ("BIOROUTER_PREDEFINED_MODELS", None::<&str>),
        ]);
        let main = ModelConfig::new("gpt-5.6")
            .unwrap()
            .with_fast("gpt-5.4-mini".to_string());
        assert_eq!(main.context_limit(), 64_000);
        assert_eq!(main.use_fast_model().context_limit(), 64_000);
    }

    #[test]
    fn exact_registry_beats_the_substring_fallback() {
        let _guard = clean_env();
        // "gpt-5" (400k) is a substring of "gpt-5.6-luna"; the exact entry wins.
        assert_eq!(ModelConfig::context_window_for("gpt-5.6-luna"), 1_050_000);
        // "claude" (200k catch-all) is a substring of "claude-sonnet-5".
        assert_eq!(
            ModelConfig::context_window_for("claude-sonnet-5"),
            1_000_000
        );
        // An id nobody declared still falls back to the pattern table.
        assert_eq!(ModelConfig::context_window_for("qwen3.6:latest"), 262_144);
        // And an id nothing matches gets the default.
        assert_eq!(
            ModelConfig::context_window_for("totally-unknown-model"),
            DEFAULT_CONTEXT_LIMIT
        );
    }
}
