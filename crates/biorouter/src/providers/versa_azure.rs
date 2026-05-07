use anyhow::Result;
use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;

use super::api_client::{ApiClient, AuthMethod, AuthProvider};
use super::azureauth::{AuthError, AzureAuth};
use super::base::{ConfigKey, Provider, ProviderMetadata, ProviderUsage, Usage};
use super::errors::ProviderError;
use super::formats::openai::{create_request, get_usage, response_to_message};
use super::retry::ProviderRetry;
use super::utils::{get_model, handle_response_openai_compat, ImageFormat};
use crate::conversation::message::Message;
use crate::model::ModelConfig;
use crate::providers::utils::RequestLog;
use rmcp::model::Tool;

pub const VERSA_AZURE_ENDPOINT: &str = "https://unified-api.ucsf.edu/general";
pub const VERSA_AZURE_DEPLOYMENT: &str = "gpt-5.2-2025-12-11";
pub const VERSA_AZURE_API_VERSION: &str = "2025-01-01-preview";
pub const VERSA_AZURE_DOC_URL: &str =
    "https://baranzinilab.github.io/biorouter-landing/docs.html";

pub const VERSA_AZURE_KNOWN_MODELS: &[&str] = &[
    "gpt-5.2-2025-12-11",
    "gpt-4.1-2025-04-14",
    "gpt-4.1-mini-2025-04-14",
    "gpt-4o-2024-11-20",
    "o4-mini-2025-04-16",
    "o3-mini-2025-01-31",
    "o1-2024-12-17",
];

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

    async fn post(&self, payload: &Value) -> Result<Value, ProviderError> {
        let path = format!(
            "openai/deployments/{}/chat/completions?api-version={}",
            self.deployment_name, self.api_version
        );
        let response = self.api_client.response_post(&path, payload).await?;
        handle_response_openai_compat(response).await
    }
}

#[async_trait]
impl Provider for VersaAzureProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            "versa_azure",
            "Versa API Azure",
            "UCSF ChatGPT via Azure OpenAI. API Key only — endpoint and deployment are pre-configured.",
            VERSA_AZURE_DEPLOYMENT,
            VERSA_AZURE_KNOWN_MODELS.to_vec(),
            VERSA_AZURE_DOC_URL,
            vec![
                ConfigKey::new("VERSA_AZURE_API_KEY", true, true, None),
                ConfigKey::new("AZURE_OPENAI_ENDPOINT", false, false, Some(VERSA_AZURE_ENDPOINT)),
                ConfigKey::new("AZURE_OPENAI_DEPLOYMENT_NAME", false, false, Some(VERSA_AZURE_DEPLOYMENT)),
                ConfigKey::new("AZURE_OPENAI_API_VERSION", false, false, Some(VERSA_AZURE_API_VERSION)),
            ],
        )
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
        let response = self
            .with_retry(|| async {
                let payload_clone = payload.clone();
                self.post(&payload_clone).await
            })
            .await?;

        let message = response_to_message(&response)?;
        let usage = response.get("usage").map(get_usage).unwrap_or_else(|| {
            tracing::debug!("Failed to get usage data");
            Usage::default()
        });
        let response_model = get_model(&response);
        let mut log = RequestLog::start(model_config, &payload)?;
        log.write(&response, Some(&usage))?;
        Ok((message, ProviderUsage::new(response_model, usage)))
    }
}
