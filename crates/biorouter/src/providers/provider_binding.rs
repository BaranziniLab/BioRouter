use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, Serialize};

use crate::model::ModelConfig;

pub(crate) const RESTORE_CONFIG_KEY: &str = "__biorouter_provider_restore";
pub(crate) const STANDALONE_RESTORE_CONFIG_KEY: &str = "__biorouter_standalone_provider_restore";

const STANDALONE_RESTORE_FORMAT_VERSION: u32 = 1;

const MAX_RETRIES: usize = 100;
const MAX_RETRY_INTERVAL_MS: u64 = 3_600_000;
const MAX_OPERATION_TIMEOUT_SECS: u64 = 86_400;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct SecretFreeEndpoint(String);

impl SecretFreeEndpoint {
    pub(crate) fn new(endpoint: String) -> Result<Self> {
        let parsed = url::Url::parse(&endpoint).context("invalid provider endpoint")?;
        anyhow::ensure!(
            parsed.scheme() == "https" && parsed.host_str().is_some(),
            "provider endpoint must be an HTTPS URL with a host"
        );
        anyhow::ensure!(
            parsed.username().is_empty()
                && parsed.password().is_none()
                && parsed.query().is_none()
                && parsed.fragment().is_none(),
            "provider endpoint must not contain credentials, a query, or a fragment"
        );
        Ok(Self(endpoint))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn into_string(self) -> String {
        self.0
    }
}

impl<'de> Deserialize<'de> for SecretFreeEndpoint {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let endpoint = String::deserialize(deserializer)?;
        Self::new(endpoint).map_err(|_| serde::de::Error::custom("invalid provider endpoint"))
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct AbsoluteCommandPath(PathBuf);

impl AbsoluteCommandPath {
    pub(crate) fn from_resolved(path: PathBuf) -> Self {
        Self(path)
    }

    pub(crate) fn resolve(path: PathBuf) -> Result<Self> {
        let path = if path.is_absolute() {
            path
        } else {
            std::env::current_dir()
                .context("could not resolve the configured provider command")?
                .join(path)
        };
        Self::new(path)
    }

    pub(crate) fn new(path: PathBuf) -> Result<Self> {
        anyhow::ensure!(
            path.is_absolute(),
            "persisted provider command must be an absolute path"
        );
        let metadata = std::fs::metadata(&path)
            .context("persisted provider command does not exist or cannot be inspected")?;
        anyhow::ensure!(
            metadata.is_file(),
            "persisted provider command is not a regular file"
        );
        Ok(Self(path))
    }

    #[cfg(test)]
    fn as_path(&self) -> &std::path::Path {
        &self.0
    }

    pub(crate) fn into_path_buf(self) -> PathBuf {
        self.0
    }
}

impl<'de> Deserialize<'de> for AbsoluteCommandPath {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let path = PathBuf::deserialize(deserializer)?;
        Self::new(path).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VersaAzureCredentialSource {
    ApiKey,
    AzureCli,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PersistedRetryConfig {
    pub(crate) max_retries: usize,
    pub(crate) initial_interval_ms: u64,
    pub(crate) backoff_multiplier: f64,
    pub(crate) max_interval_ms: u64,
}

impl PersistedRetryConfig {
    pub(crate) fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.max_retries <= MAX_RETRIES,
            "persisted provider retry count is out of range"
        );
        anyhow::ensure!(
            self.initial_interval_ms <= MAX_RETRY_INTERVAL_MS
                && self.max_interval_ms <= MAX_RETRY_INTERVAL_MS,
            "persisted provider retry interval is out of range"
        );
        anyhow::ensure!(
            self.backoff_multiplier.is_finite() && (1.0..=100.0).contains(&self.backoff_multiplier),
            "persisted provider retry multiplier is out of range"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderRestoreBinding {
    Registry {
        provider_name: String,
        model: ModelConfig,
    },
    VersaAzure {
        model: ModelConfig,
        endpoint: SecretFreeEndpoint,
        deployment: String,
        api_version: String,
        credential_source: VersaAzureCredentialSource,
    },
    VersaBedrock {
        model: ModelConfig,
        endpoint: SecretFreeEndpoint,
        region: String,
        retry: PersistedRetryConfig,
        operation_timeout_secs: Option<u64>,
    },
    Codex {
        model: ModelConfig,
        command: AbsoluteCommandPath,
    },
    ClaudeCode {
        model: ModelConfig,
        command: AbsoluteCommandPath,
    },
}

/// Exact, secret-free provider construction state stored with an ordinary
/// session. The binding contains route/auth-source references only; credentials
/// are resolved again when the provider is reconstructed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PersistedStandaloneProviderBinding {
    format_version: u32,
    binding: ProviderRestoreBinding,
}

impl PersistedStandaloneProviderBinding {
    pub(crate) fn new(binding: ProviderRestoreBinding) -> Result<Self> {
        binding.validate()?;
        Ok(Self {
            format_version: STANDALONE_RESTORE_FORMAT_VERSION,
            binding,
        })
    }

    pub(crate) fn from_model_config(model: &ModelConfig) -> Result<Option<Self>> {
        let Some(params) = model.request_params.as_ref() else {
            return Ok(None);
        };
        let Some(value) = params.get(STANDALONE_RESTORE_CONFIG_KEY) else {
            return Ok(None);
        };
        anyhow::ensure!(
            !params.contains_key(RESTORE_CONFIG_KEY),
            "conflicting persisted provider restore markers"
        );
        let persisted: Self = serde_json::from_value(value.clone())
            .context("invalid persisted standalone provider binding")?;
        anyhow::ensure!(
            persisted.format_version == STANDALONE_RESTORE_FORMAT_VERSION,
            "unsupported persisted standalone provider binding version"
        );
        persisted.binding.validate()?;
        Ok(Some(persisted))
    }

    pub(crate) fn to_model_config(&self) -> Result<ModelConfig> {
        let mut model = model_without_restore_marker(self.binding.model().clone());
        model
            .request_params
            .get_or_insert_with(Default::default)
            .insert(
                STANDALONE_RESTORE_CONFIG_KEY.to_string(),
                serde_json::to_value(self)
                    .context("persisted standalone provider binding serialization")?,
            );
        Ok(model)
    }

    pub(crate) fn into_binding(self, provider_name: &str) -> Result<ProviderRestoreBinding> {
        anyhow::ensure!(
            self.binding.provider_name() == provider_name,
            "persisted standalone provider binding does not match provider '{provider_name}'"
        );
        Ok(self.binding)
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum UncheckedProviderRestoreBinding {
    Registry {
        provider_name: String,
        model: ModelConfig,
    },
    VersaAzure {
        model: ModelConfig,
        endpoint: SecretFreeEndpoint,
        deployment: String,
        api_version: String,
        credential_source: VersaAzureCredentialSource,
    },
    VersaBedrock {
        model: ModelConfig,
        endpoint: SecretFreeEndpoint,
        region: String,
        retry: PersistedRetryConfig,
        operation_timeout_secs: Option<u64>,
    },
    Codex {
        model: ModelConfig,
        command: AbsoluteCommandPath,
    },
    ClaudeCode {
        model: ModelConfig,
        command: AbsoluteCommandPath,
    },
}

impl<'de> Deserialize<'de> for ProviderRestoreBinding {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let unchecked = UncheckedProviderRestoreBinding::deserialize(deserializer)?;
        Self::try_from(unchecked).map_err(serde::de::Error::custom)
    }
}

impl TryFrom<UncheckedProviderRestoreBinding> for ProviderRestoreBinding {
    type Error = anyhow::Error;

    fn try_from(value: UncheckedProviderRestoreBinding) -> Result<Self> {
        let binding = match value {
            UncheckedProviderRestoreBinding::Registry {
                provider_name,
                model,
            } => Self::Registry {
                provider_name,
                model,
            },
            UncheckedProviderRestoreBinding::VersaAzure {
                model,
                endpoint,
                deployment,
                api_version,
                credential_source,
            } => Self::VersaAzure {
                model,
                endpoint,
                deployment,
                api_version,
                credential_source,
            },
            UncheckedProviderRestoreBinding::VersaBedrock {
                model,
                endpoint,
                region,
                retry,
                operation_timeout_secs,
            } => Self::VersaBedrock {
                model,
                endpoint,
                region,
                retry,
                operation_timeout_secs,
            },
            UncheckedProviderRestoreBinding::Codex { model, command } => {
                Self::Codex { model, command }
            }
            UncheckedProviderRestoreBinding::ClaudeCode { model, command } => {
                Self::ClaudeCode { model, command }
            }
        };
        binding.validate()?;
        Ok(binding)
    }
}

impl ProviderRestoreBinding {
    pub(crate) fn registry(provider_name: String, model: ModelConfig) -> Self {
        Self::Registry {
            provider_name,
            model: model_without_restore_marker(model),
        }
    }

    pub(crate) fn model(&self) -> &ModelConfig {
        match self {
            Self::Registry { model, .. }
            | Self::VersaAzure { model, .. }
            | Self::VersaBedrock { model, .. }
            | Self::Codex { model, .. }
            | Self::ClaudeCode { model, .. } => model,
        }
    }

    pub(crate) fn model_mut(&mut self) -> &mut ModelConfig {
        match self {
            Self::Registry { model, .. }
            | Self::VersaAzure { model, .. }
            | Self::VersaBedrock { model, .. }
            | Self::Codex { model, .. }
            | Self::ClaudeCode { model, .. } => model,
        }
    }

    pub(crate) fn provider_name(&self) -> &str {
        match self {
            Self::Registry { provider_name, .. } => provider_name,
            Self::VersaAzure { .. } => "versa_azure",
            Self::VersaBedrock { .. } => "versa_bedrock",
            Self::Codex { .. } => "codex",
            Self::ClaudeCode { .. } => "claude_code",
        }
    }

    pub(crate) fn requires_exact_restore(&self) -> bool {
        !matches!(self, Self::Registry { .. })
    }

    pub(crate) fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            !self.provider_name().trim().is_empty(),
            "persisted provider name is empty"
        );
        anyhow::ensure!(
            !model_has_restore_marker(self.model()),
            "nested persisted provider configuration is not allowed"
        );
        match self {
            Self::VersaAzure {
                deployment,
                api_version,
                ..
            } => {
                validate_path_component(deployment, "deployment")?;
                validate_path_component(api_version, "API version")?;
            }
            Self::VersaBedrock {
                region,
                retry,
                operation_timeout_secs,
                ..
            } => {
                validate_region(region)?;
                retry.validate()?;
                if let Some(secs) = operation_timeout_secs {
                    anyhow::ensure!(
                        (1..=MAX_OPERATION_TIMEOUT_SECS).contains(secs),
                        "persisted provider operation timeout is out of range"
                    );
                }
            }
            _ => {}
        }
        Ok(())
    }
}

pub(crate) fn model_without_restore_marker(mut model: ModelConfig) -> ModelConfig {
    if let Some(params) = model.request_params.as_mut() {
        params.remove(RESTORE_CONFIG_KEY);
        params.remove(STANDALONE_RESTORE_CONFIG_KEY);
        if params.is_empty() {
            model.request_params = None;
        }
    }
    model
}

pub(crate) fn ensure_no_restore_marker(model: &ModelConfig) -> Result<()> {
    anyhow::ensure!(
        !model_has_restore_marker(model),
        "provider restore parameters are reserved for trusted session state"
    );
    Ok(())
}

fn model_has_restore_marker(model: &ModelConfig) -> bool {
    model.request_params.as_ref().is_some_and(|params| {
        params.contains_key(RESTORE_CONFIG_KEY)
            || params.contains_key(STANDALONE_RESTORE_CONFIG_KEY)
    })
}

fn validate_path_component(value: &str, label: &str) -> Result<()> {
    anyhow::ensure!(
        !value.is_empty()
            && value.len() <= 256
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            && value != "."
            && value != "..",
        "persisted provider {label} is not a valid path component"
    );
    Ok(())
}

fn validate_region(region: &str) -> Result<()> {
    anyhow::ensure!(
        !region.is_empty()
            && region.len() <= 128
            && region
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')),
        "persisted provider region is invalid"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> ModelConfig {
        ModelConfig::new_or_fail("test-model")
    }

    #[test]
    fn endpoint_rejects_credential_bearing_urls_without_echoing_them() {
        for endpoint in [
            "https://user:password@example.test/path",
            "https://example.test/path?api_key=secret-sentinel",
            "https://example.test/path#secret-sentinel",
        ] {
            let error = SecretFreeEndpoint::new(endpoint.to_string()).unwrap_err();
            assert!(!error.to_string().contains("secret-sentinel"));
            assert!(!error.to_string().contains("password"));
        }
    }

    #[test]
    fn unknown_binding_fields_and_types_are_rejected() {
        let unknown_field = serde_json::json!({
            "kind": "registry",
            "provider_name": "test",
            "model": model(),
            "unexpected": true
        });
        assert!(serde_json::from_value::<ProviderRestoreBinding>(unknown_field).is_err());
        assert!(
            serde_json::from_value::<ProviderRestoreBinding>(serde_json::json!({
                "kind": "future_provider"
            }))
            .is_err()
        );
    }

    #[test]
    fn command_bindings_require_an_existing_absolute_regular_file() {
        let relative = serde_json::json!({
            "kind": "codex",
            "model": model(),
            "command": "relative/codex"
        });
        assert!(serde_json::from_value::<ProviderRestoreBinding>(relative).is_err());

        let missing = std::env::temp_dir().join(format!("missing-{}", uuid::Uuid::new_v4()));
        let missing = serde_json::json!({
            "kind": "claude_code",
            "model": model(),
            "command": missing
        });
        assert!(serde_json::from_value::<ProviderRestoreBinding>(missing).is_err());
    }

    #[test]
    fn relative_resolved_command_is_persisted_as_an_absolute_path() {
        let file = tempfile::Builder::new().tempfile_in(".").unwrap();
        let relative = file
            .path()
            .strip_prefix(std::env::current_dir().unwrap())
            .unwrap_or(file.path())
            .to_path_buf();
        let command = AbsoluteCommandPath::resolve(relative).unwrap();
        assert!(command.as_path().is_absolute());
        let persisted = serde_json::to_value(command).unwrap();
        let persisted = PathBuf::from(persisted.as_str().unwrap());
        assert_eq!(
            std::fs::canonicalize(persisted).unwrap(),
            std::fs::canonicalize(file.path()).unwrap()
        );
    }

    #[test]
    fn nested_restore_markers_are_rejected() {
        let mut nested = model();
        nested.request_params = Some(std::collections::HashMap::from([(
            RESTORE_CONFIG_KEY.to_string(),
            serde_json::json!({"type": "nested"}),
        )]));
        let value = serde_json::json!({
            "kind": "registry",
            "provider_name": "test",
            "model": nested
        });
        assert!(serde_json::from_value::<ProviderRestoreBinding>(value).is_err());
    }

    #[test]
    fn standalone_restore_envelopes_are_versioned_and_provider_bound() {
        let envelope = PersistedStandaloneProviderBinding::new(ProviderRestoreBinding::registry(
            "test-provider".into(),
            model(),
        ))
        .unwrap();
        let model = envelope.to_model_config().unwrap();
        let restored = PersistedStandaloneProviderBinding::from_model_config(&model)
            .unwrap()
            .unwrap();
        assert!(restored.into_binding("different-provider").is_err());

        let mut unsupported = model;
        unsupported
            .request_params
            .as_mut()
            .unwrap()
            .get_mut(STANDALONE_RESTORE_CONFIG_KEY)
            .unwrap()["format_version"] = serde_json::json!(999);
        assert!(
            PersistedStandaloneProviderBinding::from_model_config(&unsupported).is_err(),
            "an unknown standalone restore version was silently accepted"
        );
    }

    #[test]
    fn every_binding_variant_round_trips() {
        let command_file = tempfile::NamedTempFile::new().unwrap();
        let command = AbsoluteCommandPath::new(command_file.path().to_path_buf()).unwrap();
        let endpoint = SecretFreeEndpoint::new("https://example.test/route".into()).unwrap();
        let values = vec![
            ProviderRestoreBinding::registry("test".into(), model()),
            ProviderRestoreBinding::VersaAzure {
                model: model(),
                endpoint: endpoint.clone(),
                deployment: "deployment-1".into(),
                api_version: "2026-01-01-preview".into(),
                credential_source: VersaAzureCredentialSource::ApiKey,
            },
            ProviderRestoreBinding::VersaBedrock {
                model: model(),
                endpoint,
                region: "us-west-2".into(),
                retry: PersistedRetryConfig {
                    max_retries: 6,
                    initial_interval_ms: 2_000,
                    backoff_multiplier: 2.0,
                    max_interval_ms: 120_000,
                },
                operation_timeout_secs: Some(300),
            },
            ProviderRestoreBinding::Codex {
                model: model(),
                command: command.clone(),
            },
            ProviderRestoreBinding::ClaudeCode {
                model: model(),
                command,
            },
        ];

        for binding in values {
            let encoded = serde_json::to_value(&binding).unwrap();
            let decoded: ProviderRestoreBinding = serde_json::from_value(encoded.clone()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), encoded);
        }
    }
}
