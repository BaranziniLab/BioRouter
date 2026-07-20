# Versa institutional providers implementation plan

> **What this is.** The task-by-task implementation plan for adding the two UCSF Versa providers (Versa API Azure and Versa API Bedrock) and splitting the desktop Provider Configuration grid into labeled sections. It carries the full original source of both provider files.
> **Status:** Historical record — planned 2026-05-07 and completed. `crates/biorouter/src/providers/versa_azure.rs` and `crates/biorouter/src/providers/versa_bedrock.rs` both exist in the tree and are registered in `factory.rs`, and the sectioned provider grid shipped. The `- [ ]` checkboxes below are the plan's original tracking state, **not** open work.
> **Audience:** agents and developers reconstructing how the Versa providers were built.

This plan executes the spec in [the Versa providers design](versa-providers-design.md); read that first for the reasoning behind each decision. The two documents are deliberately separate: the design states *what* and *why*, this plan states *how*, step by step, and quotes the code as first written.

> **Note.** The Rust and TypeScript source quoted below is a snapshot from 2026-05-07 and has since evolved. Notably the Azure deployment constant (`gpt-5.2-2025-12-11`), the Azure API version (`2024-10-21`), and the Bedrock model list have all been revised in the shipping code. Read `crates/biorouter/src/providers/versa_azure.rs` and `crates/biorouter/src/providers/versa_bedrock.rs` for current values — never copy constants out of this document.

> **Note.** The section order specified here (Institutional, Local, Commercial) is superseded. The shipping `ProviderGrid.tsx` renders **Local Models first**, and a Llama Server card that post-dates this plan now leads the grid.

> **Warning.** Credential *values* have been removed from the validation steps in Task 7 and replaced with placeholders. UCSF-issued Versa credentials are obtained from UCSF and entered through the app's provider modal; they are never recorded in this repository. See [secret storage](../../security/secret-storage.md) for where the app keeps them.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add two UCSF institutional providers (Versa API Azure, Versa API Bedrock) with pre-configured connection details, restructure the Provider Configuration UI into three labeled sections (Institutional / Local / Commercial), and validate with Playwright against a running dev build.

**Architecture:** Two new Rust provider files duplicate the commercial Azure/Bedrock providers with UCSF-specific hardcoded defaults and separately namespaced credential keys. The frontend ProviderGrid splits providers into three sections using a static category map — no API changes required. Playwright validation runs against the app built with `just dev-ui-playwright`.

**Tech stack:** Rust (async_trait, anyhow, aws-config, aws-sdk-bedrockruntime), React 19 + TypeScript, Tailwind CSS, Playwright Model Context Protocol (MCP) via the Chrome DevTools Protocol (CDP) on port 9222.

## Files changed

| File | Action |
|------|--------|
| `crates/biorouter/src/providers/versa_azure.rs` | Create |
| `crates/biorouter/src/providers/versa_bedrock.rs` | Create |
| `crates/biorouter/src/providers/mod.rs` | Modify — export two new modules |
| `crates/biorouter/src/providers/factory.rs` | Modify — register two new providers |
| `ui/desktop/src/components/settings/providers/ProviderGrid.tsx` | Modify — add section categorization |
| `ui/desktop/src/components/settings/providers/modal/subcomponents/forms/DefaultProviderSetupForm.tsx` | Modify — add PROVIDER_KEY_DEFAULTS entries |
| `ui/desktop/src/utils/dependencyChecker.ts` | Modify — add non-blocking AWS CLI check |

## Task 1: Create `versa_azure.rs`

**Files:** create `crates/biorouter/src/providers/versa_azure.rs`.

- [ ] **Step 1: Create the file**

```rust
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
pub const VERSA_AZURE_API_VERSION: &str = "2024-10-21";
pub const VERSA_AZURE_DOC_URL: &str =
    "http://biorouter.ucsf.edu/docs";

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
```

- [ ] **Step 2: Verify the file was created**

```bash
wc -l crates/biorouter/src/providers/versa_azure.rs
```

Expected: ~165 lines.

## Task 2: Create `versa_bedrock.rs`

**Files:** create `crates/biorouter/src/providers/versa_bedrock.rs`.

- [ ] **Step 1: Create the file**

```rust
use std::collections::HashMap;

use super::base::{ConfigKey, Provider, ProviderMetadata, ProviderUsage};
use super::errors::ProviderError;
use super::retry::{ProviderRetry, RetryConfig};
use crate::conversation::message::Message;
use crate::model::ModelConfig;
use crate::providers::utils::RequestLog;
use anyhow::Result;
use async_trait::async_trait;
use aws_sdk_bedrockruntime::config::ProvideCredentials;
use aws_sdk_bedrockruntime::operation::converse::ConverseError;
use aws_sdk_bedrockruntime::{types as bedrock, Client};
use rmcp::model::Tool;
use serde_json::Value;

use super::formats::bedrock::{
    from_bedrock_message, from_bedrock_usage, to_bedrock_message, to_bedrock_tool_config,
};

pub const VERSA_BEDROCK_DOC_LINK: &str =
    "http://biorouter.ucsf.edu/docs";
pub const VERSA_BEDROCK_DEFAULT_MODEL: &str = "us.anthropic.claude-sonnet-4-6";
pub const VERSA_BEDROCK_KNOWN_MODELS: &[&str] = &[
    "us.anthropic.claude-sonnet-4-6",
    "us.anthropic.claude-sonnet-4-20250514-v1:0",
    "us.anthropic.claude-opus-4-5-20251101-v1:0",
    "us.anthropic.claude-opus-4-1-20250805-v1:0",
];

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

        // Re-export all AWS_ prefixed config values as env vars (same pattern as bedrock.rs).
        let set_aws_env_vars = |res: Result<HashMap<String, Value>, _>| {
            if let Ok(map) = res {
                map.into_iter()
                    .filter(|(key, _)| key.starts_with("AWS_"))
                    .filter_map(|(key, value)| value.as_str().map(|s| (key, s.to_string())))
                    .for_each(|(key, s)| std::env::set_var(key, s));
            }
        };
        set_aws_env_vars(config.all_values());
        set_aws_env_vars(config.all_secrets());

        // Map provider-namespaced keys to standard AWS env vars so the SDK picks them up.
        // Using VERSA_BEDROCK_* names avoids colliding with the commercial aws_bedrock provider.
        if let Ok(v) = config.get_secret("VERSA_BEDROCK_ACCESS_KEY_ID") {
            if !v.is_empty() {
                std::env::set_var("AWS_ACCESS_KEY_ID", &v);
            }
        }
        if let Ok(v) = config.get_secret("VERSA_BEDROCK_SECRET_ACCESS_KEY") {
            if !v.is_empty() {
                std::env::set_var("AWS_SECRET_ACCESS_KEY", &v);
            }
        }

        // Normalize AWS_ENDPOINT_URL_BEDROCK → AWS_ENDPOINT_URL_BEDROCK_RUNTIME.
        if std::env::var("AWS_ENDPOINT_URL_BEDROCK_RUNTIME").is_err() {
            if let Ok(url) = std::env::var("AWS_ENDPOINT_URL_BEDROCK") {
                std::env::set_var("AWS_ENDPOINT_URL_BEDROCK_RUNTIME", url);
            }
        }

        let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest());

        if let Ok(profile_name) = config.get_param::<String>("AWS_PROFILE") {
            if !profile_name.is_empty() {
                loader = loader.profile_name(&profile_name);
            }
        }

        if let Ok(region) = config.get_param::<String>("AWS_REGION") {
            if !region.is_empty() {
                loader = loader.region(aws_config::Region::new(region));
            }
        }

        let sdk_config = loader.load().await;

        sdk_config
            .credentials_provider()
            .ok_or_else(|| anyhow::anyhow!("No AWS credentials provider configured"))?
            .provide_credentials()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to load AWS credentials: {}", e))?;

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
            .map_err(|err| match err.into_service_error() {
                ConverseError::ThrottlingException(e) => ProviderError::RateLimitExceeded {
                    details: format!("Bedrock throttling error: {:?}", e),
                    retry_delay: None,
                },
                ConverseError::AccessDeniedException(e) => {
                    ProviderError::Authentication(format!("Failed to call Bedrock: {:?}", e))
                }
                ConverseError::ValidationException(e)
                    if e.message()
                        .unwrap_or_default()
                        .contains("Input is too long for requested model.") =>
                {
                    ProviderError::ContextLengthExceeded(format!(
                        "Failed to call Bedrock: {:?}",
                        e
                    ))
                }
                ConverseError::ModelErrorException(e) => {
                    ProviderError::ExecutionError(format!("Failed to call Bedrock: {:?}", e))
                }
                err => ProviderError::ServerError(format!("Failed to call Bedrock: {:?}", err)),
            })?;

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
        ProviderMetadata::new(
            "versa_bedrock",
            "Versa API Bedrock",
            "UCSF Anthropic models via Amazon Bedrock. Access key + secret only — region is pre-configured.",
            VERSA_BEDROCK_DEFAULT_MODEL,
            VERSA_BEDROCK_KNOWN_MODELS.to_vec(),
            VERSA_BEDROCK_DOC_LINK,
            vec![
                ConfigKey::new("VERSA_BEDROCK_ACCESS_KEY_ID", true, true, None),
                ConfigKey::new("VERSA_BEDROCK_SECRET_ACCESS_KEY", true, true, None),
                ConfigKey::new("AWS_PROFILE", false, false, Some("default")),
                ConfigKey::new("AWS_REGION", false, false, Some("us-west-2")),
            ],
        )
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

        let (bedrock_message, bedrock_usage) = self
            .with_retry(|| self.converse(system, messages, tools))
            .await?;

        let usage = bedrock_usage
            .as_ref()
            .map(from_bedrock_usage)
            .unwrap_or_default();

        let message = from_bedrock_message(&bedrock_message)?;

        let debug_payload = serde_json::json!({
            "system": system,
            "messages": messages,
            "tools": tools
        });
        let mut log = RequestLog::start(&self.model, &debug_payload)?;
        log.write(
            &serde_json::to_value(&message).unwrap_or_default(),
            Some(&usage),
        )?;

        let provider_usage = ProviderUsage::new(model_name.to_string(), usage);
        Ok((message, provider_usage))
    }
}
```

- [ ] **Step 2: Verify the file was created**

```bash
wc -l crates/biorouter/src/providers/versa_bedrock.rs
```

Expected: ~200 lines.

## Task 3: Register the providers in `mod.rs` and `factory.rs`

**Files:** modify `crates/biorouter/src/providers/mod.rs` and `crates/biorouter/src/providers/factory.rs`.

- [ ] **Step 1: Add module exports to `mod.rs`**

In `crates/biorouter/src/providers/mod.rs`, add two lines after `pub mod azure;` and `pub mod bedrock;` respectively:

```rust
pub mod versa_azure;
pub mod versa_bedrock;
```

The file should now include (among others):

```rust
pub mod azure;
pub mod azureauth;
pub mod base;
pub mod bedrock;
// ... other modules ...
pub mod versa_azure;
pub mod versa_bedrock;
```

- [ ] **Step 2: Import and register in `factory.rs`**

At the top of `crates/biorouter/src/providers/factory.rs`, add to the `use super::` block:

```rust
use super::{
    // ... existing imports ...
    versa_azure::VersaAzureProvider,
    versa_bedrock::VersaBedrockProvider,
};
```

Inside `init_registry()`, after the `BedrockProvider` registration line, add:

```rust
registry.register::<VersaAzureProvider, _>(|m| Box::pin(VersaAzureProvider::from_env(m)), false);
registry.register::<VersaBedrockProvider, _>(|m| Box::pin(VersaBedrockProvider::from_env(m)), false);
```

- [ ] **Step 3: Verify the Rust workspace compiles**

```bash
cargo build -p biorouter 2>&1 | tail -20
```

Expected: a `Finished` line, no errors. Fix any compile errors before proceeding.

- [ ] **Step 4: Run factory tests to verify both providers are registered**

```bash
cargo test -p biorouter --test-name providers::factory -- --nocapture 2>&1 | grep -E "(PASS|FAIL|versa|test_)"
```

If no existing test covers provider listing, run all tests:

```bash
cargo test -p biorouter 2>&1 | tail -30
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/biorouter/src/providers/versa_azure.rs \
        crates/biorouter/src/providers/versa_bedrock.rs \
        crates/biorouter/src/providers/mod.rs \
        crates/biorouter/src/providers/factory.rs
git commit -m "feat(providers): add Versa API Azure and Versa API Bedrock institutional providers"
```

## Task 4: Pre-populate institutional fields in `DefaultProviderSetupForm.tsx`

**Files:** modify `ui/desktop/src/components/settings/providers/modal/subcomponents/forms/DefaultProviderSetupForm.tsx`.

- [ ] **Step 1: Add `versa_azure` and `versa_bedrock` to `PROVIDER_KEY_DEFAULTS`**

Find the existing `PROVIDER_KEY_DEFAULTS` constant (around line 25) and add two new entries:

```typescript
const PROVIDER_KEY_DEFAULTS: Record<string, Record<string, string>> = {
  azure_openai: {
    AZURE_OPENAI_ENDPOINT: 'https://unified-api.ucsf.edu/general',
    AZURE_OPENAI_API_VERSION: '2024-10-21',
  },
  aws_bedrock: {
    AWS_REGION: 'us-west-2',
  },
  versa_azure: {
    AZURE_OPENAI_ENDPOINT: 'https://unified-api.ucsf.edu/general',
    AZURE_OPENAI_DEPLOYMENT_NAME: 'gpt-5.2-2025-12-11',
    AZURE_OPENAI_API_VERSION: '2024-10-21',
  },
  versa_bedrock: {
    AWS_PROFILE: 'default',
    AWS_REGION: 'us-west-2',
  },
};
```

- [ ] **Step 2: Run the frontend type-check**

```bash
cd ui/desktop && npm run lint:check 2>&1 | tail -20
```

Expected: no TypeScript errors. Fix any before continuing.

- [ ] **Step 3: Commit**

```bash
git add ui/desktop/src/components/settings/providers/modal/subcomponents/forms/DefaultProviderSetupForm.tsx
git commit -m "feat(ui): pre-populate Versa Azure and Versa Bedrock institutional provider defaults"
```

## Task 5: Restructure `ProviderGrid.tsx` into three sections

**Files:** modify `ui/desktop/src/components/settings/providers/ProviderGrid.tsx`.

- [ ] **Step 1: Add the category sets and section-render helper**

Replace the `providerCards` `useMemo` block (lines 163–199) and the `return` statement inside `ProviderCards` (lines 212–246) with the following. The rest of the component (state, callbacks, modals) stays exactly as-is.

Replace the `providerCards` useMemo:

```typescript
const { institutionalCards, localCards, commercialCards } = useMemo(() => {
  const HIDDEN_PROVIDERS = new Set(['claude-code', 'codex', 'cursor-agent']);
  const INSTITUTIONAL = new Set(['versa_azure', 'versa_bedrock']);
  const LOCAL = new Set(['ollama']);

  const priorityOrder: Record<string, number> = {
    versa_azure: 0,
    versa_bedrock: 1,
    ollama: 0,
    azure_openai: 0,
    aws_bedrock: 1,
    anthropic: 2,
    openai: 3,
    google: 4,
  };

  const providersArray = Array.isArray(providers) ? providers : [];
  const visible = providersArray.filter((p) => !HIDDEN_PROVIDERS.has(p.name));

  const makeCards = (subset: ProviderDetails[]) =>
    [...subset]
      .sort((a, b) => {
        const pa = priorityOrder[a.name] ?? 999;
        const pb = priorityOrder[b.name] ?? 999;
        if (pa !== pb) return pa - pb;
        return a.name.localeCompare(b.name);
      })
      .map((provider) => (
        <ProviderCard
          key={provider.name}
          provider={provider}
          onConfigure={() => configureProviderViaModal(provider)}
          onLaunch={() => handleProviderLaunchWithModelSelection(provider)}
          isOnboarding={isOnboarding}
        />
      ));

  return {
    institutionalCards: makeCards(visible.filter((p) => INSTITUTIONAL.has(p.name))),
    localCards: makeCards(visible.filter((p) => LOCAL.has(p.name))),
    commercialCards: makeCards(visible.filter((p) => !INSTITUTIONAL.has(p.name) && !LOCAL.has(p.name))),
  };
}, [providers, isOnboarding, configureProviderViaModal, handleProviderLaunchWithModelSelection]);
```

Replace the `return` block in `ProviderCards` (keeping the modals unchanged after the list):

```tsx
return (
  <>
    <div className="space-y-8">
      {institutionalCards.length > 0 && (
        <div>
          <h2 className="text-xs font-medium text-text-muted uppercase tracking-wider mb-3 flex items-center gap-2">
            <span className="w-1.5 h-1.5 bg-indigo-500 rounded-full flex-shrink-0" />
            Institutional Models
          </h2>
          <div className="divide-y divide-border-subtle">
            {institutionalCards}
          </div>
        </div>
      )}

      {localCards.length > 0 && (
        <div>
          <h2 className="text-xs font-medium text-text-muted uppercase tracking-wider mb-3 flex items-center gap-2">
            <span className="w-1.5 h-1.5 bg-green-500 rounded-full flex-shrink-0" />
            Local Models
          </h2>
          <div className="divide-y divide-border-subtle">
            {localCards}
          </div>
        </div>
      )}

      <div>
        <h2 className="text-xs font-medium text-text-muted uppercase tracking-wider mb-3 flex items-center gap-2">
          <span className="w-1.5 h-1.5 bg-amber-500 rounded-full flex-shrink-0" />
          Commercial Models
        </h2>
        <div className="divide-y divide-border-subtle">
          {commercialCards}
          <CustomProviderCard onClick={() => setShowCustomProviderModal(true)} />
        </div>
      </div>
    </div>

    <Dialog open={showCustomProviderModal} onOpenChange={handleCloseModal}>
      <DialogContent className="sm:max-w-[600px]">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        <CustomProviderForm
          initialData={initialData}
          isEditable={editable}
          onSubmit={editingProvider ? handleUpdateCustomProvider : handleCreateCustomProvider}
          onCancel={handleCloseModal}
        />
      </DialogContent>
    </Dialog>{' '}
    {configuringProvider && (
      <ProviderConfigurationModal
        provider={configuringProvider}
        onClose={onCloseProviderConfig}
        onConfigured={onProviderConfigured}
      />
    )}
    {showSwitchModelModal && (
      <SwitchModelModal
        sessionId={null}
        onClose={onCloseSwitchModelModal}
        setView={handleSetView}
        onModelSelected={onModelSelected}
        initialProvider={switchModelProvider}
        titleOverride="Choose Model"
      />
    )}
  </>
);
```

- [ ] **Step 2: Run the type-check**

```bash
cd ui/desktop && npm run lint:check 2>&1 | tail -20
```

Expected: no TypeScript errors.

- [ ] **Step 3: Run the frontend unit tests**

```bash
cd ui/desktop && npm run test:run 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add ui/desktop/src/components/settings/providers/ProviderGrid.tsx
git commit -m "feat(ui): split Provider Configuration into Institutional/Local/Commercial sections"
```

## Task 6: Add an AWS CLI check to `dependencyChecker.ts`

**Files:** modify `ui/desktop/src/utils/dependencyChecker.ts`.

- [ ] **Step 1: Extend the `DependencyInfo.name` type**

Find the `DependencyInfo` interface (around line 22) and extend the `name` union:

```typescript
export interface DependencyInfo {
  name: 'git' | 'python' | 'uv' | 'npm' | 'aws';
  displayName: string;
  version: string | null;
  installed: boolean;
  installCmd: string;
  requiresSudo: boolean;
  downloadUrl: string;
}
```

- [ ] **Step 2: Add an `'aws'` case to `buildInstallInfo`**

The function signature currently takes `dep: 'git' | 'python' | 'uv' | 'npm'`. Update it to also accept `'aws'` and add platform cases:

```typescript
function buildInstallInfo(
  dep: 'git' | 'python' | 'uv' | 'npm' | 'aws',
  distro: LinuxDistro,
): { cmd: string; requiresSudo: boolean; downloadUrl: string } {
```

In the `darwin` block, add before the closing brace:

```typescript
case 'aws':
  return {
    cmd: 'brew install awscli',
    requiresSudo: false,
    downloadUrl: 'http://biorouter.ucsf.edu/docs',
  };
```

In the `win32` block, add before the closing brace:

```typescript
case 'aws':
  return {
    cmd: 'winget install Amazon.AWSCLI',
    requiresSudo: false,
    downloadUrl: 'http://biorouter.ucsf.edu/docs',
  };
```

After the `if (dep === 'uv')` block for Linux, add:

```typescript
if (dep === 'aws') {
  if (distro === 'deb') {
    return { cmd: 'sudo apt-get install -y awscli', requiresSudo: true, downloadUrl: 'http://biorouter.ucsf.edu/docs' };
  }
  if (distro === 'rpm') {
    return { cmd: 'sudo dnf install -y awscli', requiresSudo: true, downloadUrl: 'http://biorouter.ucsf.edu/docs' };
  }
  return { cmd: 'pip install awscli', requiresSudo: false, downloadUrl: 'http://biorouter.ucsf.edu/docs' };
}
```

- [ ] **Step 3: Add the AWS CLI to `checkAllDependencies`**

Find the `checks` array in `checkAllDependencies` (around line 255). Add one new entry at the end of the array:

```typescript
{
  name: 'aws' as const,
  displayName: 'AWS CLI (optional)',
  probes: [['aws', ['--version']]],
},
```

- [ ] **Step 4: Run the type-check**

```bash
cd ui/desktop && npm run lint:check 2>&1 | tail -20
```

Expected: no TypeScript errors.

- [ ] **Step 5: Commit**

```bash
git add ui/desktop/src/utils/dependencyChecker.ts
git commit -m "feat(deps): add non-blocking AWS CLI check to dependency checker"
```

## Task 7: Set up the Playwright debugger and validate live

**Files:** none — validation only.

- [ ] **Step 1: Start the app in dev mode with Playwright CDP enabled**

Run in a separate terminal and keep it running:

```bash
just dev-ui-playwright
```

This builds the debug binary and launches Electron with `ENABLE_PLAYWRIGHT=true`, which opens CDP on port 9222.

- [ ] **Step 2: Verify CDP is accessible**

```bash
curl -s http://localhost:9222/json/version | python3 -m json.tool
```

Expected: JSON with `"Browser"` and `"webSocketDebuggerUrl"` keys. If this fails, the app is not running or CDP is not enabled.

- [ ] **Step 3: Invoke the playwright-debug skill**

Use the `playwright-debug` skill (or `debug-ui` skill) to connect Playwright MCP to the running app. The `.mcp.json` at the repo root already points to `http://localhost:9222`.

- [ ] **Step 4: Navigate to Provider Configuration and verify three sections**

Using Playwright MCP tools:

- [ ] Take a screenshot — confirm the app is running.
- [ ] Navigate to Settings → Provider Configuration.
- [ ] Take a snapshot — verify the page contains "Institutional Models", "Local Models", and "Commercial Models" headings.
- [ ] Verify "Versa API Azure" and "Versa API Bedrock" appear under Institutional Models.
- [ ] Verify "Ollama" appears under Local Models.
- [ ] Verify "Azure OpenAI" and "Amazon Bedrock" appear under Commercial Models.

- [ ] **Step 5: Configure Versa API Azure with UCSF credentials**

- [ ] Click the "Versa API Azure" row — the config modal opens.
- [ ] Take a snapshot — verify only "API Key" (`VERSA_AZURE_API_KEY`) is shown above the fold.
- [ ] Click "Show N options" — verify the collapsible shows endpoint, deployment, and API version pre-filled.
- [ ] Enter the UCSF-issued Versa Azure API key (value not recorded here — obtain it from UCSF).
- [ ] Save the configuration.
- [ ] Verify a "Configured" badge appears on the Versa API Azure row.

- [ ] **Step 6: Configure Versa API Bedrock with UCSF credentials**

- [ ] Click the "Versa API Bedrock" row — the config modal opens.
- [ ] Take a snapshot — verify only "Access Key ID" (`VERSA_BEDROCK_ACCESS_KEY_ID`) and "Secret Access Key" (`VERSA_BEDROCK_SECRET_ACCESS_KEY`) are shown above the fold.
- [ ] Click "Show N options" — verify `AWS_PROFILE` is pre-filled as `default` and `AWS_REGION` as `us-west-2`.
- [ ] Enter the UCSF-issued Versa Bedrock access key ID (value not recorded here — obtain it from UCSF).
- [ ] Enter the matching secret access key (value not recorded here — obtain it from UCSF).
- [ ] Save the configuration.
- [ ] Verify a "Configured" badge appears on the Versa API Bedrock row.

- [ ] **Step 7: Verify commercial providers are unaffected**

- [ ] Click "Azure OpenAI" in the Commercial section — verify it opens normally and its credentials are not pre-populated with UCSF values.
- [ ] Click "Amazon Bedrock" in the Commercial section — verify `AWS_PROFILE` / `AWS_REGION` are shown, not the `VERSA_BEDROCK_*` keys.

- [ ] **Step 8: Final commit**

```bash
git add -A  # should be no staged changes at this point — validation only
git status  # confirm clean working tree
```

If the validation revealed any bugs, fix them before this step and commit the fixes.

## Post-implementation checklist

- [ ] `cargo test -p biorouter` passes
- [ ] `cd ui/desktop && npm run test:run` passes
- [ ] `cd ui/desktop && npm run lint:check` passes
- [ ] Both Versa providers show "Configured" in a live Playwright session
- [ ] Commercial Azure OpenAI and Bedrock credentials are unaffected

## Related documentation

- [Versa providers design](versa-providers-design.md) — the approved spec this plan implements, with the rationale for the namespaced credential keys and the section split.
- [Choosing a model provider](../../getting-started/choosing-a-model-provider.md) — the user-facing provider reference and where the Versa providers fit.
- [Secret storage](../../security/secret-storage.md) — how the app stores the credentials these tasks enter through the modal.
- [Debugging the dev GUI with agent-browser](../../desktop-ui/agent-browser-debugging.md) — the current approach to driving the desktop app, alongside the `just dev-ui-playwright` path used in Task 7.
