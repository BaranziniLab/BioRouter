use anyhow::Result;
use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;

use super::api_client::{ApiClient, AuthMethod, AuthProvider};
use super::azureauth::{AuthError, AzureAuth};
use super::base::{
    ConfigKey, MessageStream, ModelInfo, Provider, ProviderMetadata, ProviderUsage, Usage,
};
use super::errors::ProviderError;
use super::formats::openai::{create_request, get_usage, response_to_message};
use super::retry::ProviderRetry;
use super::utils::{
    azure_chat_completions_path, get_model, handle_response_openai_compat,
    handle_status_openai_compat, stream_openai_compat, ImageFormat,
};
use crate::conversation::message::Message;
use crate::model::ModelConfig;
use crate::providers::utils::RequestLog;
use rmcp::model::Tool;

// gpt-5.4 (GA, retires 2027-03-05) is the default: gpt-5.5 is newer but may
// require a quota request below Tier 5/6 on Azure.
pub const AZURE_DEFAULT_MODEL: &str = "gpt-5.4-2026-03-05";
pub const AZURE_DOC_URL: &str =
    "https://learn.microsoft.com/en-us/azure/ai-services/openai/concepts/models";
// Use 2025-01-01-preview to support o-series alongside GPT models.
pub const AZURE_DEFAULT_API_VERSION: &str = "2025-01-01-preview";
// Verified against the Azure Foundry model catalog + retirement schedule
// (June 2026). Removed (Azure-deprecated): o1-2024-12-17 (retires 2026-07-15),
// o3-mini-2025-01-31 (retires 2026-08-02).
// Deployment names must match the exact deployment name configured in the Azure OpenAI resource.
pub const AZURE_OPENAI_KNOWN_MODELS: &[&str] = &[
    // GPT-5.5 (flagship; may need a quota request on lower tiers)
    "gpt-5.5-2026-04-24",
    // GPT-5.4 family
    "gpt-5.4-2026-03-05",
    "gpt-5.4-mini-2026-03-17",
    "gpt-5.4-nano-2026-03-17",
    // GPT-5.x previous generation (still GA)
    "gpt-5.2-2025-12-11",
    "gpt-5.1-2025-11-13",
    "gpt-5-2025-08-07",
    // GPT-4.1 family (GA until 2026-10-14)
    "gpt-4.1-2025-04-14",
    "gpt-4.1-mini-2025-04-14",
    // GPT-4o (GA until 2026-10-01)
    "gpt-4o-2024-11-20",
    // o-series reasoning models (requires API version >= 2024-12-01-preview)
    "o4-mini-2025-04-16",
    "o3-2025-04-16",
];

fn azure_model_supports_vision(name: &str) -> bool {
    !name.contains("codex")
        && (name.starts_with("gpt-5")
            || name.starts_with("gpt-4.1")
            || name.starts_with("gpt-4o")
            || name.starts_with("o3")
            || name.starts_with("o4"))
}

#[derive(Debug)]
pub struct AzureProvider {
    api_client: ApiClient,
    deployment_name: String,
    api_version: String,
    model: ModelConfig,
    name: String,
}

impl Serialize for AzureProvider {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("AzureProvider", 2)?;
        state.serialize_field("deployment_name", &self.deployment_name)?;
        state.serialize_field("api_version", &self.api_version)?;
        state.end()
    }
}

// Custom auth provider that wraps AzureAuth
struct AzureAuthProvider {
    auth: AzureAuth,
}

#[async_trait]
impl AuthProvider for AzureAuthProvider {
    async fn get_auth_header(&self) -> Result<(String, String)> {
        let auth_token = self
            .auth
            .get_token()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get authentication token: {}", e))?;

        match self.auth.credential_type() {
            super::azureauth::AzureCredentials::ApiKey(_) => {
                Ok(("api-key".to_string(), auth_token.token_value))
            }
            super::azureauth::AzureCredentials::DefaultCredential => Ok((
                "Authorization".to_string(),
                format!("Bearer {}", auth_token.token_value),
            )),
        }
    }
}

impl AzureProvider {
    pub async fn from_env(model: ModelConfig) -> Result<Self> {
        let config = crate::config::Config::global();
        let endpoint: String = config.get_param("AZURE_OPENAI_ENDPOINT")?;
        let deployment_name: String = config.get_param("AZURE_OPENAI_DEPLOYMENT_NAME")?;
        let api_version: String = config
            .get_param("AZURE_OPENAI_API_VERSION")
            .unwrap_or_else(|_| AZURE_DEFAULT_API_VERSION.to_string());

        let api_key = config
            .get_secret("AZURE_OPENAI_API_KEY")
            .ok()
            .filter(|key: &String| !key.is_empty());
        let auth = AzureAuth::new(api_key).map_err(|e| match e {
            AuthError::Credentials(msg) => anyhow::anyhow!("Credentials error: {}", msg),
            AuthError::TokenExchange(msg) => anyhow::anyhow!("Token exchange error: {}", msg),
        })?;

        let auth_provider = AzureAuthProvider { auth };
        let api_client = ApiClient::new(endpoint, AuthMethod::Custom(Box::new(auth_provider)))?;

        Ok(Self {
            api_client,
            deployment_name,
            api_version,
            model,
            name: Self::metadata().name,
        })
    }

    /// The single source of truth for the Azure deployment path, shared by the
    /// blocking and streaming paths so they cannot drift.
    fn chat_completions_path(&self) -> String {
        // Shared with versa_azure via providers::utils so the two cannot drift
        // — a change made in one file used to be silently missable in the other.
        azure_chat_completions_path(&self.deployment_name, &self.api_version)
    }

    async fn post(&self, payload: &Value) -> Result<Value, ProviderError> {
        let path = self.chat_completions_path();
        let response = self.api_client.response_post(&path, payload).await?;
        handle_response_openai_compat(response).await
    }

    /// The exact request body `stream()` posts. See the equivalent on
    /// `VersaAzureProvider` for why this is extracted rather than inlined.
    fn build_stream_payload(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<Value> {
        // `for_streaming = true` also sets stream_options.include_usage, which
        // is what makes Azure emit a final usage-bearing chunk.
        create_request(
            &self.model,
            system,
            messages,
            tools,
            &ImageFormat::OpenAi,
            true,
        )
    }
}

#[async_trait]
impl Provider for AzureProvider {
    fn metadata() -> ProviderMetadata {
        let models = AZURE_OPENAI_KNOWN_MODELS
            .iter()
            .map(|&name| {
                let info = ModelInfo::new(name, ModelConfig::new_or_fail(name).context_limit());
                if azure_model_supports_vision(name) {
                    info.with_vision()
                } else {
                    info
                }
            })
            .collect();

        ProviderMetadata::with_models(
            "azure_openai",
            "Azure OpenAI",
            "Models through Azure OpenAI Service (uses Azure credential chain by default).",
            AZURE_DEFAULT_MODEL,
            models,
            AZURE_DOC_URL,
            vec![
                ConfigKey::new(
                    "AZURE_OPENAI_ENDPOINT",
                    true,
                    false,
                    Some("https://unified-api.ucsf.edu/general"),
                ),
                ConfigKey::new("AZURE_OPENAI_DEPLOYMENT_NAME", true, false, None),
                ConfigKey::new(
                    "AZURE_OPENAI_API_VERSION",
                    true,
                    false,
                    Some(AZURE_DEFAULT_API_VERSION),
                ),
                ConfigKey::new("AZURE_OPENAI_API_KEY", false, true, Some("")),
            ],
        )
        .with_unlisted_models()
    }

    fn get_name(&self) -> &str {
        &self.name
    }

    fn get_model_config(&self) -> ModelConfig {
        self.model.clone()
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
        let payload = create_request(
            model_config,
            system,
            messages,
            tools,
            &ImageFormat::OpenAi,
            false,
        )?;
        let mut log = RequestLog::start(model_config, &payload)?;
        let response = self
            .with_retry(|| async {
                let payload_clone = payload.clone();
                self.post(&payload_clone).await
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

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn stream(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        let payload = self.build_stream_payload(system, messages, tools)?;
        let mut log = RequestLog::start(&self.model, &payload)?;

        let path = self.chat_completions_path();
        let response = self
            .with_retry(|| async {
                let resp = self.api_client.response_post(&path, &payload).await?;
                handle_status_openai_compat(resp).await
            })
            .await
            .inspect_err(|e| {
                let _ = log.error(e);
            })?;

        stream_openai_compat(response, log)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// azure and versa_azure received the identical streaming change, but only
    /// versa_azure was tested. These assertions exist so the Azure path
    /// convention is pinned in both files: `supports_streaming()` is hardcoded
    /// true here, so a wrong path 404s every streaming turn with no fallback to
    /// `complete()` — the provider is broken outright, not degraded.
    fn test_provider() -> AzureProvider {
        let api_client = ApiClient::new(
            "https://example-resource.openai.azure.com".to_string(),
            AuthMethod::ApiKey {
                header_name: "api-key".to_string(),
                key: "test-key".to_string(),
            },
        )
        .expect("api client builds");

        AzureProvider {
            api_client,
            deployment_name: AZURE_DEFAULT_MODEL.to_string(),
            api_version: AZURE_DEFAULT_API_VERSION.to_string(),
            model: ModelConfig::new_or_fail(AZURE_DEFAULT_MODEL),
            name: "azure_openai".to_string(),
        }
    }

    #[test]
    fn chat_completions_path_is_azure_deployment_shaped() {
        let provider = test_provider();
        let path = provider.chat_completions_path();

        assert_eq!(
            path,
            "openai/deployments/gpt-5.4-2026-03-05/chat/completions?api-version=2025-01-01-preview"
        );
        assert!(
            !path.starts_with("chat/completions"),
            "azure must not post to the plain OpenAI path"
        );
    }

    #[test]
    fn provider_stream_payload_opts_into_streaming_with_usage() {
        let provider = test_provider();
        let payload = provider
            .build_stream_payload("sys", &[], &[])
            .expect("streaming payload builds");

        assert_eq!(payload["stream"], serde_json::json!(true));
        assert_eq!(
            payload["stream_options"]["include_usage"],
            serde_json::json!(true),
            "Azure needs stream_options.include_usage or usage/cost tracking breaks"
        );
    }

    #[test]
    fn provider_advertises_streaming() {
        assert!(
            test_provider().supports_streaming(),
            "azure must advertise streaming; without it the agent takes the blocking \
             complete() path and tool cards only appear at end of generation"
        );
    }
}
