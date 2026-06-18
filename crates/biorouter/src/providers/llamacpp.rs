//! Llama Server provider — local models served by a Biorouter-managed
//! llama.cpp `llama-server` sidecar (see [`super::llamacpp_sidecar`]).
//!
//! Unlike the Ollama provider, this requires no third-party install: the
//! desktop app bundles the pinned llama-server binary, models are downloaded
//! from Hugging Face on first use (`-hf`), and the sidecar is started on
//! demand. Setting `LLAMACPP_EXTERNAL_HOST` skips the sidecar entirely and
//! talks to an already-running llama-server (or any OpenAI-compatible
//! llama.cpp endpoint) instead.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::Tool;
use url::Url;

use super::api_client::{ApiClient, AuthMethod, AuthProvider};
use super::base::{ConfigKey, MessageStream, Provider, ProviderMetadata, ProviderUsage, Usage};
use super::errors::ProviderError;
use super::llamacpp_sidecar::{self, LLAMACPP_DEFAULT_PORT};
use super::retry::ProviderRetry;
use super::utils::{
    get_model, handle_response_openai_compat, handle_status_openai_compat, stream_openai_compat,
    RequestLog,
};
use crate::config::BioRouterMode;
use crate::conversation::message::Message;
use crate::conversation::Conversation;
use crate::model::ModelConfig;
use crate::providers::formats::openai::{create_request, get_usage, response_to_message};
use crate::utils::safe_truncate;

pub const LLAMACPP_DEFAULT_MODEL: &str = "qwen3.5-4b";
pub const LLAMACPP_TIMEOUT: u64 = 600;
/// First run of a model includes a multi-GB Hugging Face download.
pub const LLAMACPP_STARTUP_TIMEOUT: u64 = 3600;
pub const LLAMACPP_DOC_URL: &str =
    "https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md";

/// One curated local model. `hf_spec` is what llama-server's `-hf` flag
/// downloads (`owner/repo:quant`); `name` is the friendly id Biorouter uses
/// as the model name and the sidecar serves as `--alias`.
#[derive(Clone, Debug, serde::Serialize)]
pub struct CatalogEntry {
    pub name: &'static str,
    pub display_name: &'static str,
    pub hf_spec: &'static str,
    /// Approximate download size of the quantized GGUF.
    pub download_size: &'static str,
    pub description: &'static str,
    /// Context window Biorouter advertises for the model (the server's
    /// `--ctx-size` is configured separately and defaults lower).
    pub context_limit: usize,
}

/// Curated catalog: Qwen3.5 and Gemma 4 families, Q4_K_M quantizations from
/// verified Hugging Face repos. The default stays at 8B-class or smaller;
/// larger entries are opt-in for capable machines.
pub const MODEL_CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        name: "qwen3.5-4b",
        display_name: "Qwen3.5 4B (recommended)",
        hf_spec: "unsloth/Qwen3.5-4B-GGUF:Q4_K_M",
        download_size: "2.7 GB",
        description: "Best everyday default with strong tool calling; fast on 8 GB+ machines",
        context_limit: 32_768,
    },
    CatalogEntry {
        name: "qwen3.5-9b",
        display_name: "Qwen3.5 9B",
        hf_spec: "unsloth/Qwen3.5-9B-GGUF:Q4_K_M",
        download_size: "5.7 GB",
        description: "Stronger reasoning; comfortable on 16 GB+ machines",
        context_limit: 32_768,
    },
    CatalogEntry {
        name: "qwen3.5-0.8b",
        display_name: "Qwen3.5 0.8B (tiny)",
        hf_spec: "unsloth/Qwen3.5-0.8B-GGUF:Q4_K_M",
        download_size: "0.5 GB",
        description: "Very fast and small; for low-memory machines and quick tests",
        context_limit: 32_768,
    },
    CatalogEntry {
        name: "gemma-4-e2b",
        display_name: "Gemma 4 E2B",
        hf_spec: "unsloth/gemma-4-E2B-it-GGUF:Q4_K_M",
        download_size: "3.1 GB",
        description: "Google's on-device Gemma 4 (effective 2B); light and capable",
        context_limit: 32_768,
    },
    CatalogEntry {
        name: "gemma-4-e4b",
        display_name: "Gemma 4 E4B",
        hf_spec: "unsloth/gemma-4-E4B-it-GGUF:Q4_K_M",
        download_size: "5.0 GB",
        description: "Gemma 4 effective-4B; good quality/speed balance",
        context_limit: 32_768,
    },
    CatalogEntry {
        name: "gemma-4-12b",
        display_name: "Gemma 4 12B (large)",
        hf_spec: "unsloth/gemma-4-12b-it-GGUF:Q4_K_M",
        download_size: "7.1 GB",
        description: "Higher quality; needs 16 GB+ of memory",
        context_limit: 32_768,
    },
    CatalogEntry {
        name: "gemma-4-26b-a4b",
        display_name: "Gemma 4 26B-A4B (massive)",
        hf_spec: "unsloth/gemma-4-26B-A4B-it-GGUF:UD-Q4_K_M",
        download_size: "16.9 GB",
        description: "MoE flagship for 32 GB+ machines; strongest local option",
        context_limit: 32_768,
    },
];

/// Resolve a Biorouter model name to the Hugging Face spec llama-server
/// downloads. Catalog names map to pinned specs; anything containing a `/`
/// is treated as a raw `owner/repo[:quant]` spec and passed through.
pub fn resolve_hf_spec(model_name: &str) -> Result<String, ProviderError> {
    if let Some(entry) = MODEL_CATALOG.iter().find(|e| e.name == model_name) {
        return Ok(entry.hf_spec.to_string());
    }
    if model_name.contains('/') {
        // A raw `owner/repo[:quant]` Hugging Face spec. This value is passed
        // verbatim to `llama-server -hf` (which builds cache file paths from it
        // under LLAMA_CACHE) and can reach the sidecar from recipes/config, so
        // validate it strictly: reject whitespace/flag-shaped/path-traversal
        // inputs before they hit argv or the on-disk cache layout.
        validate_raw_hf_spec(model_name)?;
        return Ok(model_name.to_string());
    }
    Err(ProviderError::RequestFailed(format!(
        "Unknown Llama Server model '{model_name}'. Use one of the built-in models ({}) or a \
         Hugging Face GGUF spec like 'owner/repo:Q4_K_M'.",
        MODEL_CATALOG
            .iter()
            .map(|e| e.name)
            .collect::<Vec<_>>()
            .join(", ")
    )))
}

/// Validate a raw `owner/repo[:quant]` Hugging Face spec. Each path component
/// (`owner`, `repo`, optional `quant`) must be non-empty and contain only
/// `[A-Za-z0-9._-]`; no component may be `.` or `..`. This blocks path
/// traversal into the model cache, embedded whitespace/newlines, and
/// flag-shaped values like `repo --host`.
fn validate_raw_hf_spec(spec: &str) -> Result<(), ProviderError> {
    let reject = |why: &str| {
        Err(ProviderError::RequestFailed(format!(
            "Invalid Hugging Face model spec '{spec}': {why}. Expected 'owner/repo' or \
             'owner/repo:QUANT' using only letters, digits, '.', '_' and '-'."
        )))
    };

    // Split off an optional `:quant` suffix, then require exactly owner/repo.
    let (repo_part, quant) = match spec.split_once(':') {
        Some((r, q)) => (r, Some(q)),
        None => (spec, None),
    };
    let mut path = repo_part.split('/');
    let (Some(owner), Some(repo), None) = (path.next(), path.next(), path.next()) else {
        return reject("expected a single 'owner/repo'");
    };

    let valid_component = |c: &str| {
        !c.is_empty()
            && c != "."
            && c != ".."
            && c.chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    };

    for (component, what) in [(owner, "owner"), (repo, "repo")] {
        if !valid_component(component) {
            return reject(&format!("invalid {what} segment"));
        }
    }
    if let Some(q) = quant {
        if !valid_component(q) {
            return reject("invalid quant suffix");
        }
    }
    Ok(())
}

struct NoAuth;

#[async_trait]
impl AuthProvider for NoAuth {
    async fn get_auth_header(&self) -> Result<(String, String)> {
        Ok(("X-No-Auth".to_string(), "true".to_string()))
    }
}

#[derive(serde::Serialize)]
pub struct LlamaCppProvider {
    model: ModelConfig,
    /// Set when LLAMACPP_EXTERNAL_HOST points at an unmanaged server.
    external_base: Option<String>,
    #[serde(skip)]
    client: tokio::sync::Mutex<Option<(String, Arc<ApiClient>)>>,
    /// The running model's real context window, read from the server's `/props`
    /// once it is up (0 until known). The window is a live property of the
    /// loaded model — not a fixed catalog/config constant — so we cannot know it
    /// at construction (the sidecar starts lazily on first use). It is refreshed
    /// in `ensure_client` and overrides the construction-time fallback in
    /// `get_model_config`, keeping token accounting in sync with the model the
    /// server actually loaded.
    #[serde(skip)]
    live_context_limit: AtomicUsize,
    request_timeout: Duration,
    startup_timeout: Duration,
    name: String,
}

impl LlamaCppProvider {
    pub async fn from_env(mut model: ModelConfig) -> Result<Self> {
        let config = crate::config::Config::global();

        // Construction-time context window for token accounting. This is only a
        // fallback used until the sidecar reports the loaded model's real window
        // (see `live_context_limit`, refreshed in `ensure_client`), since the
        // server starts lazily and isn't up yet here. Preference: a window the
        // sidecar already recorded, then an explicit positive LLAMACPP_CONTEXT_SIZE
        // pin, then the memory-tiered default. A configured `0` means "auto" — it
        // is not a real accounting limit, so the tiered default stands until the
        // live window is known. This matches the `--ctx-size` the sidecar will
        // actually pass (see `configured_context_size`), keeping the pre-server
        // gauge close to the real window.
        if model.context_limit.is_none() {
            let ctx = match llamacpp_sidecar::current_context_size().await {
                Some(live) => live,
                None => config
                    .get_param::<usize>("LLAMACPP_CONTEXT_SIZE")
                    .ok()
                    .filter(|&n| n > 0)
                    .unwrap_or_else(llamacpp_sidecar::default_context_size),
            };
            model = model.with_context_limit(Some(ctx));
        }

        let external_base = config
            .get_param::<String>("LLAMACPP_EXTERNAL_HOST")
            .ok()
            .filter(|h| !h.trim().is_empty())
            .map(|host| {
                let base = if host.starts_with("http://") || host.starts_with("https://") {
                    host
                } else {
                    format!("http://{host}")
                };
                let url = Url::parse(&base)
                    .map_err(|e| anyhow::anyhow!("Invalid LLAMACPP_EXTERNAL_HOST: {e}"))?;
                // Only http(s) is meaningful here; reject file://, ftp://, etc.
                // so a malformed value fails loudly instead of being handed to
                // the HTTP client.
                if !matches!(url.scheme(), "http" | "https") {
                    return Err(anyhow::anyhow!(
                        "Invalid LLAMACPP_EXTERNAL_HOST: scheme '{}' is not http/https",
                        url.scheme()
                    ));
                }
                // Setting this bypasses the managed, loopback-only sidecar and
                // sends full prompts (system + messages + tool schemas) to an
                // unmanaged endpoint with no auth. Warn so a config-injected or
                // non-loopback host is visible rather than a silent exfil path.
                let is_loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
                if !is_loopback {
                    tracing::warn!(
                        "LLAMACPP_EXTERNAL_HOST points at a non-loopback host ({}); \
                         conversation contents will be sent there unauthenticated",
                        url.host_str().unwrap_or("?")
                    );
                }
                Ok::<String, anyhow::Error>(url.to_string())
            })
            .transpose()?;

        let request_timeout = Duration::from_secs(
            config
                .get_param("LLAMACPP_TIMEOUT")
                .unwrap_or(LLAMACPP_TIMEOUT),
        );
        let startup_timeout = Duration::from_secs(
            config
                .get_param("LLAMACPP_STARTUP_TIMEOUT")
                .unwrap_or(LLAMACPP_STARTUP_TIMEOUT),
        );

        Ok(Self {
            model,
            external_base,
            client: tokio::sync::Mutex::new(None),
            live_context_limit: AtomicUsize::new(0),
            request_timeout,
            startup_timeout,
            name: Self::metadata().name,
        })
    }

    /// Make sure a server is reachable for `model_name` and return a client
    /// pointed at it. Managed mode starts (and waits on) the sidecar; first
    /// use of a model includes its Hugging Face download.
    async fn ensure_client(&self, model_name: &str) -> Result<Arc<ApiClient>, ProviderError> {
        let base_url = if let Some(base) = &self.external_base {
            base.clone()
        } else {
            let hf_spec = resolve_hf_spec(model_name)?;
            let sidecar = llamacpp_sidecar::global();
            sidecar.ensure(model_name, &hf_spec).await.map_err(|e| {
                ProviderError::ExecutionError(format!("Failed to start llama-server: {e}"))
            })?;
            let port = sidecar
                .wait_ready(self.startup_timeout)
                .await
                .map_err(|e| {
                    ProviderError::ExecutionError(format!("llama-server not ready: {e}"))
                })?;
            // The server is up: capture the model's real context window so token
            // accounting reflects the loaded model rather than the
            // construction-time fallback. Best-effort — leave the fallback if
            // the window can't be read.
            if let Some(n) = llamacpp_sidecar::current_context_size().await {
                self.live_context_limit.store(n, Ordering::Relaxed);
            }
            format!("http://127.0.0.1:{port}/")
        };

        let mut cached = self.client.lock().await;
        if let Some((url, client)) = cached.as_ref() {
            if url == &base_url {
                return Ok(client.clone());
            }
        }
        let auth = AuthMethod::Custom(Box::new(NoAuth));
        let client = Arc::new(
            ApiClient::with_timeout(base_url.clone(), auth, self.request_timeout)
                .map_err(|e| ProviderError::ExecutionError(e.to_string()))?,
        );
        *cached = Some((base_url, client.clone()));
        Ok(client)
    }

    fn filter_reasoning_tokens(text: &str) -> String {
        let mut filtered = text.to_string();
        for tag in ["<think>", "</think>", "<thinking>", "</thinking>"] {
            filtered = filtered.replace(tag, "");
        }
        filtered
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[async_trait]
impl Provider for LlamaCppProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            "llamacpp",
            "Llama Server",
            "Built-in local models (llama.cpp): private, free, no setup required",
            LLAMACPP_DEFAULT_MODEL,
            MODEL_CATALOG.iter().map(|e| e.name).collect(),
            LLAMACPP_DOC_URL,
            vec![
                ConfigKey::new(
                    "LLAMACPP_PORT",
                    true,
                    false,
                    Some(&LLAMACPP_DEFAULT_PORT.to_string()),
                ),
                // Default "0" = use the loaded model's native context window
                // (llama-server `--ctx-size 0`). Pre-filling a small fixed
                // number here previously led users to pin e.g. 32k and cap a
                // model that supports far more. Set a positive value only to cap
                // the window for memory reasons.
                ConfigKey::new("LLAMACPP_CONTEXT_SIZE", false, false, Some("0")),
                ConfigKey::new(
                    "LLAMACPP_TIMEOUT",
                    false,
                    false,
                    Some(&LLAMACPP_TIMEOUT.to_string()),
                ),
                ConfigKey::new("LLAMACPP_ENABLE_THINKING", false, false, Some("false")),
                ConfigKey::new("LLAMACPP_EXTERNAL_HOST", false, false, None),
            ],
        )
        .with_unlisted_models()
    }

    fn get_name(&self) -> &str {
        &self.name
    }

    fn get_model_config(&self) -> ModelConfig {
        // Prefer the running model's real context window (read from `/props`)
        // over the construction-time fallback, so callers (token accounting,
        // context gauges) see the actual window — e.g. a 262k model instead of
        // the conservative 32k default. Falls back to the stored value until the
        // server has been contacted at least once.
        let live = self.live_context_limit.load(Ordering::Relaxed);
        if live > 0 {
            self.model.clone().with_context_limit(Some(live))
        } else {
            self.model.clone()
        }
    }

    #[tracing::instrument(
        skip(self, model_config, system, messages, tools),
        fields(model_config, input, output, input_tokens, output_tokens, total_tokens)
    )]
    async fn complete_with_model(
        &self,
        model_config: &ModelConfig,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<(Message, ProviderUsage), ProviderError> {
        let config = crate::config::Config::global();
        let biorouter_mode = config.get_biorouter_mode().unwrap_or(BioRouterMode::Auto);
        let filtered_tools = if biorouter_mode == BioRouterMode::Chat {
            &[]
        } else {
            tools
        };

        let client = self.ensure_client(&model_config.model_name).await?;

        let payload = create_request(
            model_config,
            system,
            messages,
            filtered_tools,
            &super::utils::ImageFormat::OpenAi,
            false,
        )?;

        let mut log = RequestLog::start(model_config, &payload)?;
        let response = self
            .with_retry(|| async {
                let response = client
                    .response_post("v1/chat/completions", &payload)
                    .await?;
                handle_response_openai_compat(response).await
            })
            .await
            .inspect_err(|e| {
                let _ = log.error(e);
            })?;

        let message = response_to_message(&response)?;
        let usage = response.get("usage").map(get_usage).unwrap_or_else(|| {
            tracing::debug!("Failed to get usage data");
            Usage::default()
        });
        let response_model = get_model(&response);
        log.write(&response, Some(&usage))?;
        Ok((message, ProviderUsage::new(response_model, usage)))
    }

    async fn generate_session_name(
        &self,
        messages: &Conversation,
    ) -> Result<String, ProviderError> {
        let context = self.get_initial_user_messages(messages);
        let message = Message::user().with_text(self.create_session_name_prompt(&context));
        let result = self
            .complete(
                "You are a title generator. Output only the requested title of 4 words or less, with no additional text, reasoning, or explanations.",
                &[message],
                &[],
            )
            .await?;

        let description = Self::filter_reasoning_tokens(&result.0.as_concat_text());
        Ok(safe_truncate(&description, 100))
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn stream(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        let config = crate::config::Config::global();
        let biorouter_mode = config.get_biorouter_mode().unwrap_or(BioRouterMode::Auto);
        let filtered_tools = if biorouter_mode == BioRouterMode::Chat {
            &[]
        } else {
            tools
        };

        let client = self.ensure_client(&self.model.model_name).await?;

        let payload = create_request(
            &self.model,
            system,
            messages,
            filtered_tools,
            &super::utils::ImageFormat::OpenAi,
            true,
        )?;
        let mut log = RequestLog::start(&self.model, &payload)?;

        let response = self
            .with_retry(|| async {
                let resp = client
                    .response_post("v1/chat/completions", &payload)
                    .await?;
                handle_status_openai_compat(resp).await
            })
            .await
            .inspect_err(|e| {
                let _ = log.error(e);
            })?;
        stream_openai_compat(response, log)
    }

    /// The curated catalog, default first. Unlike Ollama there is no local
    /// registry to enumerate — models download on first use — so the catalog
    /// IS the list; unlisted Hugging Face specs are still accepted.
    async fn fetch_supported_models(&self) -> Result<Option<Vec<String>>, ProviderError> {
        Ok(Some(
            MODEL_CATALOG.iter().map(|e| e.name.to_string()).collect(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_model_is_in_catalog_and_8b_or_smaller() {
        let entry = MODEL_CATALOG
            .iter()
            .find(|e| e.name == LLAMACPP_DEFAULT_MODEL)
            .expect("default model must be in the catalog");
        // The everyday default stays small: a 4B-class model.
        assert!(entry.name.contains("4b"));
        assert_eq!(entry.hf_spec, "unsloth/Qwen3.5-4B-GGUF:Q4_K_M");
    }

    #[test]
    fn catalog_includes_qwen35_and_gemma4_families() {
        assert!(MODEL_CATALOG.iter().any(|e| e.name.starts_with("qwen3.5")));
        assert!(MODEL_CATALOG.iter().any(|e| e.name.starts_with("gemma-4")));
    }

    #[test]
    fn resolve_catalog_name() {
        assert_eq!(
            resolve_hf_spec("qwen3.5-4b").unwrap(),
            "unsloth/Qwen3.5-4B-GGUF:Q4_K_M"
        );
        assert_eq!(
            resolve_hf_spec("gemma-4-e4b").unwrap(),
            "unsloth/gemma-4-E4B-it-GGUF:Q4_K_M"
        );
    }

    #[test]
    fn resolve_raw_hf_spec_passthrough() {
        assert_eq!(
            resolve_hf_spec("bartowski/some-model-GGUF:Q8_0").unwrap(),
            "bartowski/some-model-GGUF:Q8_0"
        );
    }

    #[test]
    fn resolve_unknown_name_errors_with_catalog_hint() {
        let err = resolve_hf_spec("gpt-4o").unwrap_err().to_string();
        assert!(err.contains("qwen3.5-4b"));
    }

    #[test]
    fn raw_hf_spec_accepts_valid_forms() {
        for ok in [
            "owner/repo",
            "owner/repo:Q4_K_M",
            "bartowski/some-model-GGUF:UD-Q4_K_M",
            "a.b_c-d/e.f_g-h:Q8_0",
        ] {
            assert!(validate_raw_hf_spec(ok).is_ok(), "should accept {ok}");
        }
    }

    #[test]
    fn raw_hf_spec_rejects_traversal_flags_and_whitespace() {
        for bad in [
            "../../etc/passwd",    // path traversal
            "owner/../repo",       // dot-dot component
            "owner/repo extra",    // embedded whitespace / extra token
            "owner/repo --host",   // flag-shaped value
            "owner/repo/extra:Q4", // too many path components
            "owner/repo:Q4 K_M",   // whitespace in quant
            "owner/",              // empty repo
            "/repo",               // empty owner
            "owner/repo:",         // empty quant
        ] {
            assert!(validate_raw_hf_spec(bad).is_err(), "should reject {bad:?}");
            // And the public entrypoint rejects it too (raw specs contain '/').
            if bad.contains('/') {
                assert!(
                    resolve_hf_spec(bad).is_err(),
                    "resolve should reject {bad:?}"
                );
            }
        }
    }

    #[test]
    fn metadata_has_zero_config_defaults() {
        let meta = LlamaCppProvider::metadata();
        assert_eq!(meta.name, "llamacpp");
        assert_eq!(meta.display_name, "Llama Server");
        assert_eq!(meta.default_model, LLAMACPP_DEFAULT_MODEL);
        assert!(meta.allows_unlisted_models);
        let port_key = meta
            .config_keys
            .iter()
            .find(|k| k.name == "LLAMACPP_PORT")
            .unwrap();
        assert!(port_key.required);
        assert_eq!(port_key.default.as_deref(), Some("11543"));
    }
}
