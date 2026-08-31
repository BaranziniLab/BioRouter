//! Live provider calls require an exact named test with `--ignored --exact`.
//! Configured credentials alone never opt an ordinary regression run into network calls.

use anyhow::Result;
use biorouter::conversation::message::{Message, MessageContent};
use biorouter::providers::anthropic::ANTHROPIC_DEFAULT_MODEL;
use biorouter::providers::azure::AZURE_DEFAULT_MODEL;
use biorouter::providers::base::Provider;
use biorouter::providers::bedrock::BEDROCK_DEFAULT_MODEL;
use biorouter::providers::create_with_named_model;
use biorouter::providers::databricks::DATABRICKS_DEFAULT_MODEL;
use biorouter::providers::errors::ProviderError;
use biorouter::providers::google::GOOGLE_DEFAULT_MODEL;
use biorouter::providers::litellm::LITELLM_DEFAULT_MODEL;
use biorouter::providers::ollama::OLLAMA_DEFAULT_MODEL;
use biorouter::providers::openai::OPEN_AI_DEFAULT_MODEL;
use biorouter::providers::sagemaker_tgi::SAGEMAKER_TGI_DEFAULT_MODEL;
use biorouter::providers::snowflake::SNOWFLAKE_DEFAULT_MODEL;
use biorouter::providers::versa_bedrock::VERSA_BEDROCK_DEFAULT_MODEL;
use biorouter::providers::xai::XAI_DEFAULT_MODEL;
use biorouter::providers::xiaomi_mimo::XIAOMI_MIMO_DEFAULT_MODEL;
use biorouter::providers::zai::ZAI_DEFAULT_MODEL;
use dotenvy::dotenv;
use rmcp::model::{AnnotateAble, Content, RawImageContent};
use rmcp::model::{CallToolRequestParams, Tool};
use rmcp::object;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy)]
enum TestStatus {
    Passed,
    Skipped,
    Failed,
}

impl std::fmt::Display for TestStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestStatus::Passed => write!(f, "✅"),
            TestStatus::Skipped => write!(f, "⏭️"),
            TestStatus::Failed => write!(f, "❌"),
        }
    }
}

struct TestReport {
    results: Mutex<HashMap<String, TestStatus>>,
}

impl TestReport {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            results: Mutex::new(HashMap::new()),
        })
    }

    fn record_status(&self, provider: &str, status: TestStatus) {
        let mut results = self.results.lock().unwrap();
        results.insert(provider.to_string(), status);
    }

    fn record_pass(&self, provider: &str) {
        self.record_status(provider, TestStatus::Passed);
    }

    fn record_skip(&self, provider: &str) {
        self.record_status(provider, TestStatus::Skipped);
    }

    fn record_fail(&self, provider: &str) {
        self.record_status(provider, TestStatus::Failed);
    }

    fn print_summary(&self) {
        println!("\n============== Providers ==============");
        let results = self.results.lock().unwrap();
        let mut providers: Vec<_> = results.iter().collect();
        providers.sort_by(|a, b| a.0.cmp(b.0));

        for (provider, status) in providers {
            println!("{} {}", status, provider);
        }
        println!("=======================================\n");
    }
}

lazy_static::lazy_static! {
    static ref TEST_REPORT: Arc<TestReport> = TestReport::new();
    static ref ENV_LOCK: Mutex<()> = Mutex::new(());
}

struct ProviderTester {
    provider: Arc<dyn Provider>,
    name: String,
}

impl ProviderTester {
    fn new(provider: Arc<dyn Provider>, name: String) -> Self {
        Self { provider, name }
    }

    async fn test_basic_response(&self) -> Result<()> {
        let message = Message::user().with_text("Just say hello!");

        let (response, _) = self
            .provider
            .complete("You are a helpful assistant.", &[message], &[])
            .await?;

        assert_eq!(
            response.content.len(),
            1,
            "Expected single content item in response"
        );

        assert!(
            matches!(response.content[0], MessageContent::Text(_)),
            "Expected text response"
        );

        Ok(())
    }

    async fn test_tool_usage(&self) -> Result<()> {
        let weather_tool = Tool::new(
            "get_weather",
            "Get the weather for a location",
            object!({
                "type": "object",
                "required": ["location"],
                "properties": {
                    "location": {
                        "type": "string",
                        "description": "The city and state, e.g. San Francisco, CA"
                    }
                }
            }),
        );

        let message = Message::user().with_text("What's the weather like in San Francisco?");

        let (response1, _) = self
            .provider
            .complete(
                "You are a helpful weather assistant.",
                std::slice::from_ref(&message),
                std::slice::from_ref(&weather_tool),
            )
            .await?;

        println!("=== {}::reponse1 ===", self.name);
        dbg!(&response1);
        println!("===================");

        assert!(
            response1
                .content
                .iter()
                .any(|content| matches!(content, MessageContent::ToolRequest(_))),
            "Expected tool request in response"
        );

        let id = &response1
            .content
            .iter()
            .filter_map(|message| message.as_tool_request())
            .next_back()
            .expect("got tool request")
            .id;

        let weather = Message::user().with_tool_response(
            id,
            Ok(rmcp::model::CallToolResult {
                content: vec![Content::text(
                    "
                  50°F°C
                  Precipitation: 0%
                  Humidity: 84%
                  Wind: 2 mph
                  Weather
                  Saturday 9:00 PM
                  Clear",
                )],
                structured_content: None,
                is_error: Some(false),
                meta: None,
            }),
        );

        let (response2, _) = self
            .provider
            .complete(
                "You are a helpful weather assistant.",
                &[message, response1, weather],
                &[weather_tool],
            )
            .await?;

        println!("=== {}::reponse2 ===", self.name);
        dbg!(&response2);
        println!("===================");

        assert!(
            response2
                .content
                .iter()
                .any(|content| matches!(content, MessageContent::Text(_))),
            "Expected text for final response"
        );

        Ok(())
    }

    async fn test_context_length_exceeded_error(&self) -> Result<()> {
        // Ollama and Xiaomi MiMo silently truncate oversized input to their
        // context window (MiMo caps at its ~1M window and returns Ok) rather
        // than returning a context-length error. They are asserted on
        // separately below, and so must still send the request.
        let truncates_silently =
            matches!(self.name.to_lowercase().as_str(), "ollama" | "xiaomi_mimo");

        let large_message_content = if self.name.to_lowercase() == "google" {
            "hello ".repeat(1_300_000)
        } else {
            "hello ".repeat(300_000)
        };

        // An over-limit request is only worth sending when the fixture can
        // actually overflow the window. Against a million-token model it
        // cannot: the request would not error, it would submit a very large
        // *billable* prompt and then fail the assertion below. This used to be
        // a hardcoded `name == "anthropic"` carve-out; the window is the real
        // reason, and stating it that way covers every large-context model
        // rather than the one that happened to be noticed. Each provider's
        // error-payload mapping is covered deterministically in unit tests, and
        // this suite still exercises it live through the basic, tool and image
        // requests.
        //
        // `len() / 4` OVERESTIMATES tokens for this fixture — it is repeated
        // "hello ", roughly 6 chars per token — which is the direction that
        // makes skipping safe: if even the overestimate fits inside the window,
        // the real prompt certainly does.
        let window = self.provider.get_model_config().context_limit();
        let upper_bound_tokens = large_message_content.len() / 4;
        if !truncates_silently && upper_bound_tokens <= window {
            println!(
                "Skipping {} live over-limit request: ~{} tokens cannot exceed a {}-token \
                 window, so the call would be billed without testing anything",
                self.name, upper_bound_tokens, window
            );
            return Ok(());
        }

        let messages = vec![
            Message::user().with_text("hi there. what is 2 + 2?"),
            Message::assistant().with_text("hey! I think it's 4."),
            Message::user().with_text(&large_message_content),
            Message::assistant().with_text("heyy!!"),
            Message::user().with_text("what's the meaning of life?"),
            Message::assistant().with_text("the meaning of life is 42"),
            Message::user().with_text(
                "did I ask you what's 2+2 in this message history? just respond with 'yes' or 'no'",
            ),
        ];

        let result = self
            .provider
            .complete("You are a helpful assistant.", &messages, &[])
            .await;

        println!("=== {}::context_length_exceeded_error ===", self.name);
        dbg!(&result);
        println!("===================");

        if truncates_silently {
            assert!(
                result.is_ok(),
                "Expected to succeed because of default truncation"
            );
            return Ok(());
        }

        assert!(
            result.is_err(),
            "Expected error when context window is exceeded"
        );
        assert!(
            matches!(result.unwrap_err(), ProviderError::ContextLengthExceeded(_)),
            "Expected error to be ContextLengthExceeded"
        );

        Ok(())
    }

    async fn test_image_content_support(&self) -> Result<()> {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
        use biorouter::conversation::message::Message;
        use std::fs;

        let image_path = "crates/biorouter/examples/test_assets/test_image.png";
        let image_data = match fs::read(image_path) {
            Ok(data) => data,
            Err(_) => {
                println!(
                    "Test image not found at {}, skipping image test",
                    image_path
                );
                return Ok(());
            }
        };

        let base64_image = BASE64.encode(image_data);
        let image_content = RawImageContent {
            data: base64_image,
            mime_type: "image/png".to_string(),
            meta: None,
        }
        .no_annotation();

        let message_with_image =
            Message::user().with_image(image_content.data.clone(), image_content.mime_type.clone());

        let result = self
            .provider
            .complete(
                "You are a helpful assistant. Describe what you see in the image briefly.",
                &[message_with_image],
                &[],
            )
            .await;

        println!("=== {}::image_content_support ===", self.name);
        let (response, _) = result?;
        println!("Image response: {:?}", response);
        assert!(
            response
                .content
                .iter()
                .any(|content| matches!(content, MessageContent::Text(_))),
            "Expected text response for image"
        );
        println!("===================");

        let screenshot_tool = Tool::new(
            "get_screenshot",
            "Get a screenshot of the current screen",
            object!({
                "type": "object",
                "properties": {}
            }),
        );

        let user_message = Message::user().with_text("Take a screenshot please");
        let tool_request = Message::assistant().with_tool_request(
            "test_id",
            Ok(CallToolRequestParams {
                task: None,
                meta: None,
                name: "get_screenshot".into(),
                arguments: Some(object!({})),
            }),
        );
        let tool_response = Message::user().with_tool_response(
            "test_id",
            Ok(rmcp::model::CallToolResult {
                content: vec![Content::image(
                    image_content.data.clone(),
                    image_content.mime_type.clone(),
                )],
                structured_content: None,
                is_error: Some(false),
                meta: None,
            }),
        );

        let result2 = self
            .provider
            .complete(
                "You are a helpful assistant.",
                &[user_message, tool_request, tool_response],
                &[screenshot_tool],
            )
            .await;

        println!("=== {}::tool_image_response ===", self.name);
        let (response, _) = result2?;
        println!("Tool image response: {:?}", response);
        println!("===================");

        Ok(())
    }

    async fn run_test_suite(&self) -> Result<()> {
        self.test_basic_response().await?;
        self.test_tool_usage().await?;
        self.test_context_length_exceeded_error().await?;
        self.test_image_content_support().await?;
        Ok(())
    }
}

fn load_env() {
    if let Ok(path) = dotenv() {
        println!("Loaded environment from {:?}", path);
    }
}

/// The broad, credential-**optional** sweep: a provider with no credentials
/// configured records ⏭️ and returns `Ok`, so the suite can be run by anyone
/// with any subset of keys.
///
/// `name` is the provider's **registry key** — `metadata().name`, the exact
/// string [`create_with_named_model`] looks up. It is used verbatim, and it is
/// also the label in the report. It used to be a display name that was
/// `.to_lowercase()`d into a lookup, which silently produced a key that does
/// not exist for three providers ("Bedrock" → `bedrock`, but the registry holds
/// `aws_bedrock`); the resulting "Unknown provider" was then swallowed as one
/// more skip, so the test reported green having called nothing.
/// [`every_registry_key_used_by_this_suite_resolves`] is the guard that keeps
/// that from coming back, and it needs no credentials to catch it.
///
/// Tests that must not be allowed to pass without calling anything use
/// [`run_live_suite`] instead.
async fn test_provider(
    name: &str,
    model_name: &str,
    required_vars: &[&str],
    env_modifications: Option<HashMap<&str, Option<String>>>,
) -> Result<()> {
    TEST_REPORT.record_fail(name);

    let original_env = {
        let _lock = ENV_LOCK.lock().unwrap();

        load_env();

        let mut original_env = HashMap::new();
        for &var in required_vars {
            if let Ok(val) = std::env::var(var) {
                original_env.insert(var, val);
            }
        }
        if let Some(mods) = &env_modifications {
            for &var in mods.keys() {
                if let Ok(val) = std::env::var(var) {
                    original_env.insert(var, val);
                }
            }
        }

        if let Some(mods) = &env_modifications {
            for (&var, value) in mods.iter() {
                match value {
                    Some(val) => std::env::set_var(var, val),
                    None => std::env::remove_var(var),
                }
            }
        }

        let missing_vars = required_vars.iter().any(|var| std::env::var(var).is_err());
        if missing_vars {
            println!("Skipping {} tests - credentials not configured", name);
            TEST_REPORT.record_skip(name);
            return Ok(());
        }

        original_env
    };

    let provider = match create_with_named_model(name, model_name).await {
        Ok(p) => p,
        Err(e) => {
            println!("Skipping {} tests - failed to create provider: {}", name, e);
            TEST_REPORT.record_skip(name);
            return Ok(());
        }
    };

    {
        let _lock = ENV_LOCK.lock().unwrap();
        for (&var, value) in original_env.iter() {
            std::env::set_var(var, value);
        }
        if let Some(mods) = env_modifications {
            for &var in mods.keys() {
                if !original_env.contains_key(var) {
                    std::env::remove_var(var);
                }
            }
        }
    }

    let tester = ProviderTester::new(provider, name.to_string());
    match tester.run_test_suite().await {
        Ok(_) => {
            TEST_REPORT.record_pass(name);
            Ok(())
        }
        Err(e) => {
            println!("{} test failed: {}", name, e);
            TEST_REPORT.record_fail(name);
            Err(e)
        }
    }
}

#[test]
fn live_provider_tests_require_explicit_opt_in() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--list", "--ignored"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let output = String::from_utf8(output.stdout).unwrap();
    let ignored: Vec<_> = output
        .lines()
        .filter_map(|line| line.strip_suffix(": test"))
        .collect();
    for name in [
        "test_openai_provider",
        "test_azure_provider",
        "test_bedrock_provider_long_term_credentials",
        "test_bedrock_provider_aws_profile_credentials",
        "test_bedrock_provider_sonnet_5",
        "test_versa_bedrock_provider",
        "test_versa_bedrock_provider_sonnet_4_6",
        "test_databricks_provider",
        "test_ollama_provider",
        "test_anthropic_provider",
        "test_openrouter_provider",
        "test_google_provider",
        "test_snowflake_provider",
        "test_sagemaker_tgi_provider",
        "test_litellm_provider",
        "test_xai_provider",
        "test_zai_provider",
        "test_xiaomi_mimo_provider",
    ] {
        assert!(
            ignored.contains(&name),
            "live test must require opt-in: {name}"
        );
    }
    assert!(!ignored.contains(&"every_registry_key_used_by_this_suite_resolves"));
    assert!(!ignored.contains(&"live_provider_tests_require_explicit_opt_in"));
}

#[tokio::test]
#[ignore = "live OpenAI call; run an exact named test with --ignored --exact"]
async fn test_openai_provider() -> Result<()> {
    test_provider("openai", OPEN_AI_DEFAULT_MODEL, &["OPENAI_API_KEY"], None).await
}

#[tokio::test]
#[ignore = "live Azure call; run an exact named test with --ignored --exact"]
async fn test_azure_provider() -> Result<()> {
    test_provider(
        "azure_openai",
        AZURE_DEFAULT_MODEL,
        &[
            "AZURE_OPENAI_API_KEY",
            "AZURE_OPENAI_ENDPOINT",
            "AZURE_OPENAI_DEPLOYMENT_NAME",
        ],
        None,
    )
    .await
}

// ===========================================================================
// Live Bedrock / Versa Bedrock checks
//
// These make REAL, BILLED API calls, so they are `#[ignore]`d and must be asked
// for by name. In exchange for being opt-in they are held to a stricter rule
// than the sweep above: **no early exit may return `Ok`**. Every one of them is
// an `Err`, because the state being replaced was a pair of tests that reported
// green while calling nothing at all — a missing credential returned `Ok`, and
// so did an "Unknown provider" from a registry key that never existed.
//
//     cargo test -p biorouter --test providers test_versa_bedrock_provider -- --ignored --exact --test-threads=1
// ===========================================================================

/// How a live test establishes that a credential is actually available.
#[derive(Clone, Copy)]
enum Credential {
    /// Present only if the process environment carries it.
    Env(&'static str),
    /// Present if the environment carries it **or** Biorouter's secret store
    /// does. Versa Bedrock resolves its keys through `Config::get_secret`,
    /// which reads the environment first and the OS keychain second — so on a
    /// machine where the UCSF keys live in the keychain (the normal desktop
    /// install) there is no environment variable to find, and an env-only check
    /// would hard-fail the exact configuration this test exists to cover.
    EnvOrSecret(&'static str),
}

impl Credential {
    fn key(self) -> &'static str {
        match self {
            Credential::Env(key) | Credential::EnvOrSecret(key) => key,
        }
    }

    fn is_present(self) -> bool {
        match self {
            Credential::Env(key) => std::env::var(key).is_ok(),
            Credential::EnvOrSecret(key) => {
                std::env::var(key).is_ok()
                    || biorouter::config::Config::global()
                        .get_secret::<String>(key)
                        .is_ok()
            }
        }
    }
}

/// Run the full provider suite live, failing loudly instead of skipping.
///
/// `registry_key` is `metadata().name` and is passed to the factory verbatim.
/// `label` is only the report row, so two tests of the same provider do not
/// overwrite each other's status.
async fn run_live_suite(
    label: &str,
    registry_key: &str,
    model_name: &str,
    required: &[Credential],
    env_modifications: Option<HashMap<&str, Option<String>>>,
) -> Result<()> {
    TEST_REPORT.record_fail(label);

    let original_env = {
        let _lock = ENV_LOCK.lock().unwrap();

        load_env();

        let mut original_env = HashMap::new();
        for credential in required {
            if let Ok(val) = std::env::var(credential.key()) {
                original_env.insert(credential.key(), val);
            }
        }
        if let Some(mods) = &env_modifications {
            for &var in mods.keys() {
                if let Ok(val) = std::env::var(var) {
                    original_env.insert(var, val);
                }
            }
        }

        if let Some(mods) = &env_modifications {
            for (&var, value) in mods.iter() {
                match value {
                    Some(val) => std::env::set_var(var, val),
                    None => std::env::remove_var(var),
                }
            }
        }

        original_env
    };

    // The environment modifications above are part of what is under test (the
    // AWS_PROFILE variant works by *removing* the long-term keys), so they stay
    // applied across the whole run and are restored exactly once, on every
    // path, before the result is inspected.
    let outcome = live_suite_inner(registry_key, model_name, required).await;

    {
        let _lock = ENV_LOCK.lock().unwrap();
        for (&var, value) in original_env.iter() {
            std::env::set_var(var, value);
        }
        if let Some(mods) = env_modifications {
            for &var in mods.keys() {
                if !original_env.contains_key(var) {
                    std::env::remove_var(var);
                }
            }
        }
    }

    match outcome {
        Ok(()) => {
            TEST_REPORT.record_pass(label);
            Ok(())
        }
        Err(e) => {
            println!("{} live test failed: {:#}", label, e);
            TEST_REPORT.record_fail(label);
            Err(e)
        }
    }
}

async fn live_suite_inner(
    registry_key: &str,
    model_name: &str,
    required: &[Credential],
) -> Result<()> {
    let missing: Vec<&str> = required
        .iter()
        .copied()
        .filter(|credential| !credential.is_present())
        .map(Credential::key)
        .collect();
    if !missing.is_empty() {
        anyhow::bail!(
            "{} live test requires credentials that are not configured: {}. \
             This test is #[ignore]d, so reaching it means it was asked for by \
             name — it fails rather than skipping, because a skipped live test \
             that reports success is worse than no test.",
            registry_key,
            missing.join(", ")
        );
    }

    let provider = create_with_named_model(registry_key, model_name)
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "failed to construct provider {:?} with model {:?}: {}. A registry-key \
                 typo surfaces here as \"Unknown provider\"; the valid keys are asserted \
                 by every_registry_key_used_by_this_suite_resolves.",
                registry_key,
                model_name,
                e
            )
        })?;

    ProviderTester::new(provider, registry_key.to_string())
        .run_test_suite()
        .await
}

/// Every registry key this file passes to the factory must actually exist.
///
/// This is the cheap instrument the live tests lacked: no credentials, no
/// network, no billing, and it runs in the ordinary `cargo test` sweep. The
/// defect it pins is that `create_with_named_model("bedrock", …)` fails with
/// "Unknown provider: bedrock" — the registry key is `aws_bedrock` — and that
/// failure was being swallowed as a skip, so the test stayed green. Azure
/// (`azure_openai`) and SageMaker TGI (`sagemaker_tgi`) were broken the same
/// way and just as invisibly.
#[tokio::test]
async fn every_registry_key_used_by_this_suite_resolves() {
    const KEYS_USED: &[&str] = &[
        "anthropic",
        "aws_bedrock",
        "azure_openai",
        "databricks",
        "google",
        "litellm",
        "ollama",
        "openai",
        "openrouter",
        "sagemaker_tgi",
        "snowflake",
        "versa_bedrock",
        "xai",
        "xiaomi_mimo",
        "zai",
    ];

    let registered: Vec<String> = biorouter::providers::providers()
        .await
        .into_iter()
        .map(|(metadata, _)| metadata.name)
        .collect();

    let unknown: Vec<&str> = KEYS_USED
        .iter()
        .copied()
        .filter(|key| !registered.iter().any(|name| name == key))
        .collect();

    assert!(
        unknown.is_empty(),
        "these tests look up provider keys that the registry does not hold: {:?}.\n\
         A lookup miss is reported as a skip, so the affected tests pass without \
         calling anything. Registered keys: {:?}",
        unknown,
        registered
    );
}

#[tokio::test]
#[ignore = "live billed Bedrock call; run deliberately with --ignored"]
async fn test_bedrock_provider_long_term_credentials() -> Result<()> {
    run_live_suite(
        "aws_bedrock (long-term keys)",
        "aws_bedrock",
        BEDROCK_DEFAULT_MODEL,
        &[
            Credential::Env("AWS_ACCESS_KEY_ID"),
            Credential::Env("AWS_SECRET_ACCESS_KEY"),
        ],
        None,
    )
    .await
}

#[tokio::test]
#[ignore = "live billed Bedrock call; run deliberately with --ignored"]
async fn test_bedrock_provider_aws_profile_credentials() -> Result<()> {
    // Removing the long-term keys is the point: without this the SDK would
    // satisfy the request from them and the profile path would go untested.
    let env_mods =
        HashMap::from_iter([("AWS_ACCESS_KEY_ID", None), ("AWS_SECRET_ACCESS_KEY", None)]);

    run_live_suite(
        "aws_bedrock (AWS_PROFILE)",
        "aws_bedrock",
        BEDROCK_DEFAULT_MODEL,
        &[Credential::Env("AWS_PROFILE")],
        Some(env_mods),
    )
    .await
}

/// The model in the issue #87 reports.
///
/// The two tests above pin [`BEDROCK_DEFAULT_MODEL`], which has never been
/// `claude-sonnet-5`, so no live test reached the model actually being reported
/// on. Pinned literally rather than to the default constant, so that moving the
/// default cannot quietly stop covering it.
#[tokio::test]
#[ignore = "live billed Bedrock call; run deliberately with --ignored"]
async fn test_bedrock_provider_sonnet_5() -> Result<()> {
    run_live_suite(
        "aws_bedrock (sonnet-5)",
        "aws_bedrock",
        "us.anthropic.claude-sonnet-5",
        &[
            Credential::Env("AWS_ACCESS_KEY_ID"),
            Credential::Env("AWS_SECRET_ACCESS_KEY"),
        ],
        None,
    )
    .await
}

/// Versa Bedrock — the UCSF MuleSoft proxy — had no live test at all, despite
/// being the path a UCSF `config.yaml` actually routes to. Its credentials are
/// normally in the OS keychain rather than the environment, hence
/// [`Credential::EnvOrSecret`].
#[tokio::test]
#[ignore = "live billed Versa Bedrock call; run deliberately with --ignored"]
async fn test_versa_bedrock_provider() -> Result<()> {
    run_live_suite(
        "versa_bedrock (default model)",
        "versa_bedrock",
        VERSA_BEDROCK_DEFAULT_MODEL,
        &[
            Credential::EnvOrSecret("VERSA_BEDROCK_ACCESS_KEY_ID"),
            Credential::EnvOrSecret("VERSA_BEDROCK_SECRET_ACCESS_KEY"),
        ],
        None,
    )
    .await
}

/// Sonnet 4.6 over the Versa proxy. Not in `VERSA_BEDROCK_KNOWN_MODELS`' first
/// position and not the default, so nothing else exercises it; UCSF entitlement
/// is per-account, so a failure here is a real signal about this account rather
/// than about the client.
#[tokio::test]
#[ignore = "live billed Versa Bedrock call; run deliberately with --ignored"]
async fn test_versa_bedrock_provider_sonnet_4_6() -> Result<()> {
    run_live_suite(
        "versa_bedrock (sonnet-4-6)",
        "versa_bedrock",
        "us.anthropic.claude-sonnet-4-6",
        &[
            Credential::EnvOrSecret("VERSA_BEDROCK_ACCESS_KEY_ID"),
            Credential::EnvOrSecret("VERSA_BEDROCK_SECRET_ACCESS_KEY"),
        ],
        None,
    )
    .await
}

#[tokio::test]
#[ignore = "live Databricks call; run an exact named test with --ignored --exact"]
async fn test_databricks_provider() -> Result<()> {
    test_provider(
        "databricks",
        DATABRICKS_DEFAULT_MODEL,
        &["DATABRICKS_HOST", "DATABRICKS_TOKEN"],
        None,
    )
    .await
}

#[tokio::test]
#[ignore = "live Ollama call; run an exact named test with --ignored --exact"]
async fn test_ollama_provider() -> Result<()> {
    test_provider("ollama", OLLAMA_DEFAULT_MODEL, &["OLLAMA_HOST"], None).await
}

#[tokio::test]
#[ignore = "live Anthropic call; run an exact named test with --ignored --exact"]
async fn test_anthropic_provider() -> Result<()> {
    test_provider(
        "anthropic",
        ANTHROPIC_DEFAULT_MODEL,
        &["ANTHROPIC_API_KEY"],
        None,
    )
    .await
}

#[tokio::test]
#[ignore = "live OpenRouter call; run an exact named test with --ignored --exact"]
async fn test_openrouter_provider() -> Result<()> {
    test_provider(
        "openrouter",
        OPEN_AI_DEFAULT_MODEL,
        &["OPENROUTER_API_KEY"],
        None,
    )
    .await
}

#[tokio::test]
#[ignore = "live Google call; run an exact named test with --ignored --exact"]
async fn test_google_provider() -> Result<()> {
    test_provider("google", GOOGLE_DEFAULT_MODEL, &["GOOGLE_API_KEY"], None).await
}

#[tokio::test]
#[ignore = "live Snowflake call; run an exact named test with --ignored --exact"]
async fn test_snowflake_provider() -> Result<()> {
    test_provider(
        "snowflake",
        SNOWFLAKE_DEFAULT_MODEL,
        &["SNOWFLAKE_HOST", "SNOWFLAKE_TOKEN"],
        None,
    )
    .await
}

#[tokio::test]
#[ignore = "live SageMaker call; run an exact named test with --ignored --exact"]
async fn test_sagemaker_tgi_provider() -> Result<()> {
    test_provider(
        "sagemaker_tgi",
        SAGEMAKER_TGI_DEFAULT_MODEL,
        &["SAGEMAKER_ENDPOINT_NAME"],
        None,
    )
    .await
}

#[tokio::test]
#[ignore = "live LiteLLM call; run an exact named test with --ignored --exact"]
async fn test_litellm_provider() -> Result<()> {
    if std::env::var("LITELLM_HOST").is_err() {
        println!("LITELLM_HOST not set, skipping test");
        TEST_REPORT.record_skip("litellm");
        return Ok(());
    }

    let env_mods = HashMap::from_iter([
        ("LITELLM_HOST", Some("http://localhost:4000".to_string())),
        ("LITELLM_API_KEY", Some("".to_string())),
    ]);

    test_provider("litellm", LITELLM_DEFAULT_MODEL, &[], Some(env_mods)).await
}

#[tokio::test]
#[ignore = "live xAI call; run an exact named test with --ignored --exact"]
async fn test_xai_provider() -> Result<()> {
    test_provider("xai", XAI_DEFAULT_MODEL, &["XAI_API_KEY"], None).await
}

#[tokio::test]
#[ignore = "live Zai call; run an exact named test with --ignored --exact"]
async fn test_zai_provider() -> Result<()> {
    test_provider("zai", ZAI_DEFAULT_MODEL, &["ZAI_API_KEY"], None).await
}

#[tokio::test]
#[ignore = "live Xiaomi MiMo call; run an exact named test with --ignored --exact"]
async fn test_xiaomi_mimo_provider() -> Result<()> {
    test_provider(
        "xiaomi_mimo",
        XIAOMI_MIMO_DEFAULT_MODEL,
        &["XIAOMI_MIMO_API_KEY"],
        None,
    )
    .await
}

#[ctor::dtor]
fn print_test_report() {
    TEST_REPORT.print_summary();
}
