//! End-to-end agent-loop tests for the soft-interrupt ("steer") path wired to
//! the desktop client by BR-61.
//!
//! Drives the *real* reply loop with a mock provider (no network, no keychain).
//! The interesting case is a steer that lands while the model's **final**
//! response is being produced: it arrives after that iteration's drain, so a
//! turn about to exit would strand it on the queue until some later, unrelated
//! turn injected it. The loop must instead stay alive for one more step, inject
//! the steer as a user message, and let the model answer it in the same turn.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use anyhow::Result;
use async_trait::async_trait;
use biorouter::agents::{Agent, AgentConfig, AgentEvent, SessionConfig};
use biorouter::config::permission::PermissionManager;
use biorouter::config::BioRouterMode;
use biorouter::conversation::message::{Message, MessageContent};
use biorouter::model::ModelConfig;
use biorouter::providers::base::{Provider, ProviderMetadata, ProviderUsage, Usage};
use biorouter::providers::errors::ProviderError;
use biorouter::session::session_manager::SessionType;
use biorouter::session::SessionManager;
use futures::StreamExt;
use rmcp::model::Tool;
use tempfile::TempDir;

/// Text-only provider that queues a soft interrupt on its first completion —
/// i.e. the user hits "steer" while the model is writing what would have been
/// the last message of the turn.
struct SteeringProvider {
    calls: AtomicUsize,
    agent: OnceLock<Arc<Agent>>,
    steer: String,
    /// Messages the provider saw on each call, so the test can assert the steer
    /// actually reached the model.
    seen: std::sync::Mutex<Vec<Vec<String>>>,
}

impl SteeringProvider {
    fn new(steer: &str) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            agent: OnceLock::new(),
            steer: steer.to_string(),
            seen: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn call_count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn texts_seen_on_call(&self, n: usize) -> Vec<String> {
        self.seen
            .lock()
            .unwrap()
            .get(n)
            .cloned()
            .unwrap_or_default()
    }
}

#[async_trait]
impl Provider for SteeringProvider {
    async fn complete(
        &self,
        _system_prompt: &str,
        messages: &[Message],
        _tools: &[Tool],
    ) -> Result<(Message, ProviderUsage), ProviderError> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        self.seen.lock().unwrap().push(
            messages
                .iter()
                .flat_map(|m| m.content.iter())
                .filter_map(|c| match c {
                    MessageContent::Text(t) => Some(t.text.clone()),
                    _ => None,
                })
                .collect(),
        );

        let usage = ProviderUsage::new(
            "mock-model".to_string(),
            Usage::new(Some(10), Some(5), Some(15)),
        );

        if n == 0 {
            // The steer lands mid-stream, after this iteration already drained.
            if let Some(agent) = self.agent.get() {
                agent.queue_soft_interrupt(self.steer.clone());
            }
            return Ok((
                Message::assistant().with_text("Done — I used Python."),
                usage,
            ));
        }

        Ok((Message::assistant().with_text("Redone in R."), usage))
    }

    async fn complete_with_model(
        &self,
        _model_config: &ModelConfig,
        system_prompt: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<(Message, ProviderUsage), ProviderError> {
        self.complete(system_prompt, messages, tools).await
    }

    fn get_model_config(&self) -> ModelConfig {
        ModelConfig::new("mock-model").unwrap()
    }

    fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            name: "mock".to_string(),
            display_name: "Mock Provider".to_string(),
            description: "Mock provider for testing".to_string(),
            default_model: "mock-model".to_string(),
            known_models: vec![],
            model_doc_link: String::new(),
            config_keys: vec![],
            allows_unlisted_models: false,
        }
    }

    fn get_name(&self) -> &str {
        "mock-test"
    }
}

async fn agent_with_provider(provider: Arc<SteeringProvider>) -> (Arc<Agent>, String, TempDir) {
    let work_dir = TempDir::new().unwrap();
    let data_dir = TempDir::new().unwrap();
    let session_manager = Arc::new(SessionManager::new(data_dir.path().to_path_buf()));
    let config = AgentConfig::new(
        session_manager.clone(),
        PermissionManager::instance(),
        None,
        BioRouterMode::Auto,
    );
    let agent = Agent::with_config(config);

    let session = session_manager
        .create_session(
            work_dir.path().to_path_buf(),
            "soft-interrupt-test".to_string(),
            SessionType::Hidden,
        )
        .await
        .unwrap();

    agent
        .update_provider(provider.clone() as Arc<dyn Provider>, &session.id)
        .await
        .unwrap();

    let agent = Arc::new(agent);
    let _ = provider.agent.set(agent.clone());

    // The session store lives under data_dir for the life of the agent.
    std::mem::forget(data_dir);
    (agent, session.id, work_dir)
}

async fn drain(agent: &Agent, user: &str, session_id: &str) -> Result<Vec<Message>> {
    let session_config = SessionConfig {
        id: session_id.to_string(),
        schedule_id: None,
        max_turns: Some(8),
        max_tool_calls: None,
        retry_config: None,
        budget: None,
        reasoning_effort: None,
    };
    let stream = agent
        .reply(Message::user().with_text(user), session_config, None)
        .await?;
    tokio::pin!(stream);
    let mut out = Vec::new();
    while let Some(ev) = stream.next().await {
        if let AgentEvent::Message(m) = ev? {
            out.push(m);
        }
    }
    Ok(out)
}

fn user_texts(messages: &[Message]) -> Vec<String> {
    messages
        .iter()
        .filter(|m| m.role == rmcp::model::Role::User)
        .flat_map(|m| m.content.iter())
        .filter_map(|c| match c {
            MessageContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn steer_landing_at_turn_exit_is_injected_in_the_same_turn() {
    let provider = Arc::new(SteeringProvider::new("Actually, use R instead."));
    let (agent, session_id, _work_dir) = agent_with_provider(provider.clone()).await;

    let messages = drain(&agent, "Plot the data", &session_id).await.unwrap();

    // The loop stayed alive for one more step instead of exiting on the
    // no-tool-call response, so the model saw the steer.
    assert_eq!(
        provider.call_count(),
        2,
        "a pending steer must keep the turn alive for one more provider call"
    );
    assert!(
        provider
            .texts_seen_on_call(1)
            .iter()
            .any(|t| t.contains("Actually, use R instead.")),
        "the steer must be part of the context of the follow-up provider call, saw: {:?}",
        provider.texts_seen_on_call(1)
    );

    // It also surfaces to the client as a normal user message in the stream.
    assert!(
        user_texts(&messages)
            .iter()
            .any(|t| t == "Actually, use R instead."),
        "the steer must be streamed back as a user message, saw: {:?}",
        user_texts(&messages)
    );

    // The queue is drained (no leftovers to leak into the next turn) and the
    // turn ends normally once the steer is answered.
    assert!(!agent.has_soft_interrupts());
}

#[tokio::test(flavor = "multi_thread")]
async fn no_steer_means_no_extra_provider_call() {
    // Control: without a pending soft interrupt the loop exits as before.
    struct QuietProvider(AtomicUsize);

    #[async_trait]
    impl Provider for QuietProvider {
        async fn complete(
            &self,
            _system_prompt: &str,
            _messages: &[Message],
            _tools: &[Tool],
        ) -> Result<(Message, ProviderUsage), ProviderError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok((
                Message::assistant().with_text("All done."),
                ProviderUsage::new(
                    "mock-model".to_string(),
                    Usage::new(Some(1), Some(1), Some(2)),
                ),
            ))
        }

        async fn complete_with_model(
            &self,
            _model_config: &ModelConfig,
            system_prompt: &str,
            messages: &[Message],
            tools: &[Tool],
        ) -> Result<(Message, ProviderUsage), ProviderError> {
            self.complete(system_prompt, messages, tools).await
        }

        fn get_model_config(&self) -> ModelConfig {
            ModelConfig::new("mock-model").unwrap()
        }

        fn metadata() -> ProviderMetadata {
            SteeringProvider::metadata()
        }

        fn get_name(&self) -> &str {
            "mock-test"
        }
    }

    let work_dir = TempDir::new().unwrap();
    let data_dir = TempDir::new().unwrap();
    let session_manager = Arc::new(SessionManager::new(data_dir.path().to_path_buf()));
    let agent = Agent::with_config(AgentConfig::new(
        session_manager.clone(),
        PermissionManager::instance(),
        None,
        BioRouterMode::Auto,
    ));
    let session = session_manager
        .create_session(
            work_dir.path().to_path_buf(),
            "soft-interrupt-control".to_string(),
            SessionType::Hidden,
        )
        .await
        .unwrap();
    let provider = Arc::new(QuietProvider(AtomicUsize::new(0)));
    agent
        .update_provider(provider.clone() as Arc<dyn Provider>, &session.id)
        .await
        .unwrap();
    std::mem::forget(data_dir);

    drain(&agent, "Say hi", &session.id).await.unwrap();

    assert_eq!(provider.0.load(Ordering::SeqCst), 1);
    assert!(!agent.has_soft_interrupts());
}
