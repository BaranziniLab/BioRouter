use super::api_client::{ApiClient, AuthMethod};
use super::errors::ProviderError;
use super::retry::ProviderRetry;
use super::utils::{
    get_model, handle_response_openai_compat, handle_status_openai_compat, stream_openai_compat,
    RequestLog,
};
use crate::conversation::message::Message;
use crate::model::ModelConfig;
use crate::providers::base::{
    ConfigKey, MessageStream, Provider, ProviderMetadata, ProviderUsage, Usage,
};
use crate::providers::formats::openai::{create_request, get_usage, response_to_message};
use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::Tool;
use serde_json::Value;

// z.ai is the international platform of Zhipu AI; it serves the GLM family of
// models through an OpenAI-compatible API (and a separate Anthropic-compatible
// surface used by Claude Code — not used here). Verified live against
// docs.z.ai (June 2026): base URL `/api/paas/v4`, Bearer-token auth.
pub const ZAI_API_HOST: &str = "https://api.z.ai/api/paas/v4";
pub const ZAI_DEFAULT_MODEL: &str = "glm-4.6";
pub const ZAI_KNOWN_MODELS: &[&str] = &[
    // GLM-4 family
    "glm-4.7",
    "glm-4.6",
    "glm-4.5",
    "glm-4.5-air",
    // GLM-5 family
    "glm-5.2",
    "glm-5.1",
    "glm-5",
    "glm-5-turbo",
];

pub const ZAI_DOC_URL: &str = "https://docs.z.ai/guides/overview/pricing";

#[derive(serde::Serialize)]
pub struct ZaiProvider {
    #[serde(skip)]
    api_client: ApiClient,
    model: ModelConfig,
    supports_streaming: bool,
    #[serde(skip)]
    name: String,
}

impl ZaiProvider {
    pub async fn from_env(model: ModelConfig) -> Result<Self> {
        let config = crate::config::Config::global();
        let api_key: String = config.get_secret("ZAI_API_KEY")?;
        let host: String = config
            .get_param("ZAI_HOST")
            .unwrap_or_else(|_| ZAI_API_HOST.to_string());

        let auth = AuthMethod::BearerToken(api_key);
        let api_client = ApiClient::new(host, auth)?;

        Ok(Self {
            api_client,
            model,
            supports_streaming: true,
            name: Self::metadata().name,
        })
    }

    async fn post(&self, payload: Value) -> Result<Value, ProviderError> {
        let response = self
            .api_client
            .response_post("chat/completions", &payload)
            .await?;

        handle_response_openai_compat(response).await
    }
}

#[async_trait]
impl Provider for ZaiProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            "zai",
            "z.ai",
            "GLM models from z.ai (Zhipu AI), including the GLM-4 and GLM-5 families via an OpenAI-compatible API",
            ZAI_DEFAULT_MODEL,
            ZAI_KNOWN_MODELS.to_vec(),
            ZAI_DOC_URL,
            vec![
                ConfigKey::new("ZAI_API_KEY", true, true, None),
                ConfigKey::new("ZAI_HOST", false, false, Some(ZAI_API_HOST)),
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
            &super::utils::ImageFormat::OpenAi,
            false,
        )?;

        let mut log = RequestLog::start(&self.model, &payload)?;
        let response = self.with_retry(|| self.post(payload.clone())).await?;

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
        self.supports_streaming
    }

    async fn stream(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        let payload = create_request(
            &self.model,
            system,
            messages,
            tools,
            &super::utils::ImageFormat::OpenAi,
            true,
        )?;
        let mut log = RequestLog::start(&self.model, &payload)?;

        let response = self
            .with_retry(|| async {
                let resp = self
                    .api_client
                    .response_post("chat/completions", &payload)
                    .await?;
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

    #[test]
    fn test_metadata_structure() {
        let metadata = ZaiProvider::metadata();

        assert_eq!(metadata.name, "zai");
        assert_eq!(metadata.default_model, "glm-4.6");
        assert!(metadata.known_models.iter().any(|m| m.name == "glm-4.6"));
        assert!(!metadata.known_models.is_empty());

        assert_eq!(metadata.config_keys.len(), 2);
        assert_eq!(metadata.config_keys[0].name, "ZAI_API_KEY");
        assert_eq!(metadata.config_keys[1].name, "ZAI_HOST");
        // Host default points at the OpenAI-compatible base URL.
        assert_eq!(
            metadata.config_keys[1].default,
            Some(ZAI_API_HOST.to_string())
        );
    }

    #[tokio::test]
    async fn test_registered_in_factory() {
        let all = crate::providers::providers().await;
        assert!(
            all.iter().any(|(m, _)| m.name == "zai"),
            "zai provider must be registered in the factory registry"
        );
    }
}
