use super::base::{ConfigKey, ModelInfo, Provider, ProviderMetadata, ProviderUsage};
use super::errors::ProviderError;
use super::retry::{ProviderRetry, RetryConfig};
use crate::conversation::message::Message;
use crate::model::ModelConfig;
use crate::providers::utils::RequestLog;
use anyhow::Result;
use async_trait::async_trait;
use aws_sdk_bedrockruntime::config::{Credentials, ProvideCredentials};
use aws_sdk_bedrockruntime::{types as bedrock, Client};
use rmcp::model::Tool;

use super::formats::bedrock::{
    classify_bedrock_converse_error, from_bedrock_message, from_bedrock_usage, to_bedrock_message,
    to_bedrock_tool_config,
};

pub const VERSA_BEDROCK_DOC_LINK: &str = "http://biorouter.ucsf.edu/docs";
pub const VERSA_BEDROCK_DEFAULT_MODEL: &str = "us.anthropic.claude-opus-4-6-v1";
// Ordered newest → oldest. The UI auto-selects the first entry as the default
// model when switching providers. Model IDs follow the AWS Bedrock format
// documented at
// https://platform.claude.com/docs/en/about-claude/models, prefixed with the
// `us.` cross-region inference profile required by the UCSF MuleSoft proxy.
//
// Only models that UCSF's Bedrock account is entitled to invoke are listed —
// every entry below has been verified end-to-end via the Versa proxy. Opus 4.7
// and 4.8 are intentionally omitted because UCSF returns AccessDeniedException
// for them; users can still type a newer ID via the "Enter a model not
// listed..." option once UCSF enables it.
pub const VERSA_BEDROCK_KNOWN_MODELS: &[&str] = &[
    // Claude 4.6 (1M context)
    "us.anthropic.claude-opus-4-6-v1",
    "us.anthropic.claude-sonnet-4-6",
    // Claude 4.5 (200K context)
    "us.anthropic.claude-opus-4-5-20251101-v1:0",
    // Haiku 4.5 (200K context)
    "us.anthropic.claude-haiku-4-5-20251001-v1:0",
    // Sonnet 4 removed: Anthropic retires claude-sonnet-4-20250514 on
    // June 15, 2026 (Bedrock marked it Legacy in April 2026).
];

// UCSF MuleSoft Bedrock proxy. UCSF-issued access keys are signed against this
// endpoint instead of public AWS, so this must be set for Versa Bedrock to work.
pub const VERSA_BEDROCK_DEFAULT_ENDPOINT: &str = "https://unified-api.ucsf.edu/general/awsai";
pub const VERSA_BEDROCK_DEFAULT_REGION: &str = "us-west-2";

pub const VERSA_BEDROCK_DEFAULT_MAX_RETRIES: usize = 6;
pub const VERSA_BEDROCK_DEFAULT_INITIAL_RETRY_INTERVAL_MS: u64 = 2000;
pub const VERSA_BEDROCK_DEFAULT_BACKOFF_MULTIPLIER: f64 = 2.0;
pub const VERSA_BEDROCK_DEFAULT_MAX_RETRY_INTERVAL_MS: u64 = 120_000;

#[derive(Debug, serde::Serialize)]
pub struct VersaBedrockProvider {
    #[serde(skip)]
    client: Client,
    model: ModelConfig,
    #[serde(skip)]
    retry_config: RetryConfig,
    #[serde(skip)]
    name: String,
}

impl VersaBedrockProvider {
    pub async fn from_env(model: ModelConfig) -> Result<Self> {
        let config = crate::config::Config::global();

        // Required UCSF-issued credentials. Stored as secrets in Biorouter config.
        let access_key_id: String = config
            .get_secret::<String>("VERSA_BEDROCK_ACCESS_KEY_ID")
            .map_err(|_| {
                anyhow::anyhow!(
                    "VERSA_BEDROCK_ACCESS_KEY_ID is not configured. \
                     Add it under Versa API Bedrock in Settings."
                )
            })?;
        let secret_access_key: String = config
            .get_secret::<String>("VERSA_BEDROCK_SECRET_ACCESS_KEY")
            .map_err(|_| {
                anyhow::anyhow!(
                    "VERSA_BEDROCK_SECRET_ACCESS_KEY is not configured. \
                     Add it under Versa API Bedrock in Settings."
                )
            })?;
        if access_key_id.trim().is_empty() || secret_access_key.trim().is_empty() {
            return Err(anyhow::anyhow!(
                "Versa Bedrock access key id / secret access key is empty"
            ));
        }

        // Endpoint: configurable, but always falls back to the UCSF MuleSoft proxy
        // so a fresh install with just the key + secret works out of the box.
        let endpoint_url: String = config
            .get_param::<String>("AWS_ENDPOINT_URL_BEDROCK")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                std::env::var("AWS_ENDPOINT_URL_BEDROCK")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
            })
            .or_else(|| {
                std::env::var("AWS_ENDPOINT_URL_BEDROCK_RUNTIME")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
            })
            .unwrap_or_else(|| VERSA_BEDROCK_DEFAULT_ENDPOINT.to_string());

        let region: String = config
            .get_param::<String>("AWS_REGION")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                std::env::var("AWS_REGION")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
            })
            .unwrap_or_else(|| VERSA_BEDROCK_DEFAULT_REGION.to_string());

        // Build credentials explicitly. This bypasses the AWS default credential
        // chain (profile files, IMDS, env vars), so users who only set the
        // access key + secret in Biorouter — and have nothing in ~/.aws — still
        // get a working setup.
        let credentials =
            Credentials::new(access_key_id, secret_access_key, None, None, "VersaBedrock");

        let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .credentials_provider(credentials)
            .region(aws_config::Region::new(region))
            .endpoint_url(endpoint_url);
        // Bound a hung/stalled endpoint so a turn can't wait forever (see
        // `bedrock_timeout_config`). Without this, a proxy that accepts the
        // connection but never answers freezes the agent with no error.
        if let Some(timeout_config) = super::formats::bedrock::bedrock_timeout_config(config) {
            loader = loader.timeout_config(timeout_config);
        }
        let sdk_config = loader.load().await;

        sdk_config
            .credentials_provider()
            .ok_or_else(|| anyhow::anyhow!("No AWS credentials provider configured"))?
            .provide_credentials()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to load Versa Bedrock credentials: {}", e))?;

        let client = Client::new(&sdk_config);
        let retry_config = Self::load_retry_config(config);

        Ok(Self {
            client,
            model,
            retry_config,
            name: Self::metadata().name,
        })
    }

    fn load_retry_config(config: &crate::config::Config) -> RetryConfig {
        let max_retries = config
            .get_param::<usize>("BEDROCK_MAX_RETRIES")
            .unwrap_or(VERSA_BEDROCK_DEFAULT_MAX_RETRIES);
        let initial_interval_ms = config
            .get_param::<u64>("BEDROCK_INITIAL_RETRY_INTERVAL_MS")
            .unwrap_or(VERSA_BEDROCK_DEFAULT_INITIAL_RETRY_INTERVAL_MS);
        let backoff_multiplier = config
            .get_param::<f64>("BEDROCK_BACKOFF_MULTIPLIER")
            .unwrap_or(VERSA_BEDROCK_DEFAULT_BACKOFF_MULTIPLIER);
        let max_interval_ms = config
            .get_param::<u64>("BEDROCK_MAX_RETRY_INTERVAL_MS")
            .unwrap_or(VERSA_BEDROCK_DEFAULT_MAX_RETRY_INTERVAL_MS);
        RetryConfig {
            max_retries,
            initial_interval_ms,
            backoff_multiplier,
            max_interval_ms,
        }
    }

    async fn converse(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<(bedrock::Message, Option<bedrock::TokenUsage>), ProviderError> {
        let model_name = &self.model.model_name;

        let mut request = self
            .client
            .converse()
            .system(bedrock::SystemContentBlock::Text(system.to_string()))
            .model_id(model_name.to_string())
            .set_messages(Some(
                messages
                    .iter()
                    .filter(|m| m.is_agent_visible())
                    .map(to_bedrock_message)
                    .collect::<Result<_>>()?,
            ));

        if !tools.is_empty() {
            request = request.tool_config(to_bedrock_tool_config(tools)?);
        }

        let response = request
            .send()
            .await
            .map_err(classify_bedrock_converse_error)?;

        match response.output {
            Some(bedrock::ConverseOutput::Message(message)) => Ok((message, response.usage)),
            _ => Err(ProviderError::RequestFailed(
                "No output from Bedrock".to_string(),
            )),
        }
    }
}

#[async_trait]
impl Provider for VersaBedrockProvider {
    fn metadata() -> ProviderMetadata {
        let models: Vec<ModelInfo> = VERSA_BEDROCK_KNOWN_MODELS
            .iter()
            .map(|&name| {
                ModelInfo::new(name, ModelConfig::new_or_fail(name).context_limit()).with_vision()
            })
            .collect();

        ProviderMetadata::with_models(
            "versa_bedrock",
            "Versa API Bedrock",
            "UCSF Anthropic models via Amazon Bedrock. Access key + secret only — endpoint and region are pre-configured.",
            VERSA_BEDROCK_DEFAULT_MODEL,
            models,
            VERSA_BEDROCK_DOC_LINK,
            vec![
                ConfigKey::new("VERSA_BEDROCK_ACCESS_KEY_ID", true, true, None),
                ConfigKey::new("VERSA_BEDROCK_SECRET_ACCESS_KEY", true, true, None),
                ConfigKey::new(
                    "AWS_ENDPOINT_URL_BEDROCK",
                    false,
                    false,
                    Some(VERSA_BEDROCK_DEFAULT_ENDPOINT),
                ),
                ConfigKey::new(
                    "AWS_REGION",
                    false,
                    false,
                    Some(VERSA_BEDROCK_DEFAULT_REGION),
                ),
            ],
        )
        .with_unlisted_models()
    }

    fn get_name(&self) -> &str {
        &self.name
    }

    fn retry_config(&self) -> RetryConfig {
        self.retry_config.clone()
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
        let model_name = model_config.model_name.clone();

        let debug_payload = serde_json::json!({
            "system": system,
            "messages": messages,
            "tools": tools
        });
        let mut log = RequestLog::start(&self.model, &debug_payload)?;

        let (bedrock_message, bedrock_usage) = self
            .with_retry(|| self.converse(system, messages, tools))
            .await
            .inspect_err(|e| {
                let _ = log.error(e);
            })?;

        let usage = bedrock_usage
            .as_ref()
            .map(from_bedrock_usage)
            .unwrap_or_default();

        let message = from_bedrock_message(&bedrock_message)?;

        log.write(
            &serde_json::to_value(&message).unwrap_or_default(),
            Some(&usage),
        )?;

        let provider_usage = ProviderUsage::new(model_name.to_string(), usage);
        Ok((message, provider_usage))
    }
}
