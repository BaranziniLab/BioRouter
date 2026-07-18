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
    get_model, handle_response_openai_compat, handle_status_openai_compat, stream_openai_compat,
    ImageFormat,
};
use crate::conversation::message::Message;
use crate::model::ModelConfig;
use crate::providers::utils::RequestLog;
use rmcp::model::Tool;

pub const VERSA_AZURE_ENDPOINT: &str = "https://unified-api.ucsf.edu/general";
pub const VERSA_AZURE_DEPLOYMENT: &str = "gpt-5.5-2026-04-24";
pub const VERSA_AZURE_API_VERSION: &str = "2025-01-01-preview";
pub const VERSA_AZURE_DOC_URL: &str = "http://biorouter.ucsf.edu/docs";

// Versa proxies Azure OpenAI deployments; the authoritative list lives on the
// (login-gated) UCSF wiki "Models, deployments, and API endpoints in UCSF
// Versa". Public UCSF Versa docs are MyAccess-gated, so keep this list to
// deployments verified against the UCSF endpoint. Removed: o1-2024-12-17 and
// o3-mini-2025-01-31 (deprecated on Azure, retiring Jul/Aug 2026).
pub const VERSA_AZURE_KNOWN_MODELS: &[&str] = &[
    "gpt-5.5-2026-04-24",
    "gpt-5.4-mini-2026-03-17",
    "gpt-5.4-nano-2026-03-17",
    "gpt-5.2-2025-12-11",
    "gpt-5-2025-08-07",
    "gpt-4.1-2025-04-14",
    "gpt-4.1-mini-2025-04-14",
    "gpt-4o-2024-11-20",
    "o4-mini-2025-04-16",
];

fn versa_azure_model_supports_vision(name: &str) -> bool {
    !name.contains("codex")
        && (name.starts_with("gpt-5")
            || name.starts_with("gpt-4.1")
            || name.starts_with("gpt-4o")
            || name.starts_with("o3")
            || name.starts_with("o4"))
}

#[derive(Debug)]
pub struct VersaAzureProvider {
    api_client: ApiClient,
    deployment_name: String,
    api_version: String,
    model: ModelConfig,
    name: String,
}

impl Serialize for VersaAzureProvider {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("VersaAzureProvider", 2)?;
        state.serialize_field("deployment_name", &self.deployment_name)?;
        state.serialize_field("api_version", &self.api_version)?;
        state.end()
    }
}

struct VersaAzureAuthProvider {
    auth: AzureAuth,
}

#[async_trait]
impl AuthProvider for VersaAzureAuthProvider {
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

impl VersaAzureProvider {
    pub async fn from_env(model: ModelConfig) -> Result<Self> {
        let config = crate::config::Config::global();

        let endpoint: String = config
            .get_param("AZURE_OPENAI_ENDPOINT")
            .unwrap_or_else(|_| VERSA_AZURE_ENDPOINT.to_string());
        let deployment_name: String = config
            .get_param("AZURE_OPENAI_DEPLOYMENT_NAME")
            .unwrap_or_else(|_| VERSA_AZURE_DEPLOYMENT.to_string());
        let api_version: String = config
            .get_param("AZURE_OPENAI_API_VERSION")
            .unwrap_or_else(|_| VERSA_AZURE_API_VERSION.to_string());

        let api_key = config
            .get_secret("VERSA_AZURE_API_KEY")
            .ok()
            .filter(|key: &String| !key.is_empty());
        let auth = AzureAuth::new(api_key).map_err(|e| match e {
            AuthError::Credentials(msg) => anyhow::anyhow!("Credentials error: {}", msg),
            AuthError::TokenExchange(msg) => anyhow::anyhow!("Token exchange error: {}", msg),
        })?;

        let auth_provider = VersaAzureAuthProvider { auth };
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
        build_chat_completions_path(&self.deployment_name, &self.api_version)
    }

    async fn post(&self, payload: &Value) -> Result<Value, ProviderError> {
        let path = self.chat_completions_path();
        let response = self.api_client.response_post(&path, payload).await?;
        handle_response_openai_compat(response).await
    }
}

fn build_chat_completions_path(deployment_name: &str, api_version: &str) -> String {
    format!(
        "openai/deployments/{}/chat/completions?api-version={}",
        deployment_name, api_version
    )
}

#[async_trait]
impl Provider for VersaAzureProvider {
    fn metadata() -> ProviderMetadata {
        let models = VERSA_AZURE_KNOWN_MODELS
            .iter()
            .map(|&name| {
                let info = ModelInfo::new(name, ModelConfig::new_or_fail(name).context_limit());
                if versa_azure_model_supports_vision(name) {
                    info.with_vision()
                } else {
                    info
                }
            })
            .collect();

        ProviderMetadata::with_models(
            "versa_azure",
            "Versa API Azure",
            "UCSF ChatGPT via Azure OpenAI. API Key only — endpoint and deployment are pre-configured.",
            VERSA_AZURE_DEPLOYMENT,
            models,
            VERSA_AZURE_DOC_URL,
            vec![
                ConfigKey::new("VERSA_AZURE_API_KEY", true, true, None),
                ConfigKey::new("AZURE_OPENAI_ENDPOINT", false, false, Some(VERSA_AZURE_ENDPOINT)),
                ConfigKey::new("AZURE_OPENAI_DEPLOYMENT_NAME", false, false, Some(VERSA_AZURE_DEPLOYMENT)),
                ConfigKey::new("AZURE_OPENAI_API_VERSION", false, false, Some(VERSA_AZURE_API_VERSION)),
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
        // `create_request` with `for_streaming = true` sets both `stream: true`
        // and `stream_options: {"include_usage": true}` (formats/openai.rs:850),
        // which is what makes Azure OpenAI emit a final usage-bearing chunk.
        // Without it, usage/cost tracking silently reports zeros on this path.
        let payload = create_request(
            &self.model,
            system,
            messages,
            tools,
            &ImageFormat::OpenAi,
            true,
        )?;
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
    use crate::providers::formats::openai::create_request;

    /// The Azure deployment path is shared by `complete` and `stream`; if these
    /// ever drift, streaming silently 404s while completion keeps working.
    #[test]
    fn chat_completions_path_is_azure_deployment_shaped() {
        let path = build_chat_completions_path("gpt-5.5-2026-04-24", "2025-01-01-preview");
        assert_eq!(
            path,
            "openai/deployments/gpt-5.5-2026-04-24/chat/completions?api-version=2025-01-01-preview"
        );
        assert!(
            !path.starts_with("chat/completions"),
            "versa_azure must not post to the plain OpenAI path"
        );
    }

    /// Guards the real trap: Azure only reports token usage on a streamed
    /// response when `stream_options.include_usage` is set.
    #[test]
    fn streaming_payload_sets_stream_and_usage_options() {
        let model = ModelConfig::new_or_fail(VERSA_AZURE_DEPLOYMENT);
        let payload = create_request(&model, "sys", &[], &[], &ImageFormat::OpenAi, true)
            .expect("streaming request should build");

        assert_eq!(payload["stream"], serde_json::json!(true));
        assert_eq!(
            payload["stream_options"]["include_usage"],
            serde_json::json!(true),
            "Azure needs stream_options.include_usage or usage/cost tracking breaks"
        );
    }

    #[test]
    fn non_streaming_payload_does_not_set_stream() {
        let model = ModelConfig::new_or_fail(VERSA_AZURE_DEPLOYMENT);
        let payload = create_request(&model, "sys", &[], &[], &ImageFormat::OpenAi, false)
            .expect("request should build");

        assert!(payload.get("stream").is_none());
        assert!(payload.get("stream_options").is_none());
    }
}
