//! End-to-end agent-loop tests for the hooks wiring in `Agent::reply()`.
//!
//! These drive the *real* reply loop with a mock provider (no network, no
//! keychain) and a per-test working directory holding a project
//! `.biorouter/hooks.yaml`, asserting that the integration points added to
//! agent.rs actually fire: UserPromptSubmit block + context injection,
//! PreToolUse deny feeding back into the tool result, and the Stop-hook
//! block loop. Mirrors the MockToolProvider pattern in tests/agent.rs.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

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
use rmcp::model::{CallToolRequestParams, Tool};
use rmcp::object;
use tempfile::TempDir;

/// A provider that emits a `developer__shell` tool call on the first turn,
/// then plain text on subsequent turns (so the loop terminates).
struct ScriptedProvider {
    calls: AtomicUsize,
    /// When true the first turn requests a shell tool; otherwise text only.
    emit_tool: bool,
    final_text: String,
}

impl ScriptedProvider {
    fn new(emit_tool: bool, final_text: &str) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            emit_tool,
            final_text: final_text.to_string(),
        }
    }
}

#[async_trait]
impl Provider for ScriptedProvider {
    async fn complete(
        &self,
        _system_prompt: &str,
        _messages: &[Message],
        _tools: &[Tool],
    ) -> Result<(Message, ProviderUsage), ProviderError> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        let usage = ProviderUsage::new(
            "mock-model".to_string(),
            Usage::new(Some(10), Some(5), Some(15)),
        );
        if self.emit_tool && n == 0 {
            let tool_call = CallToolRequestParams {
                task: None,
                meta: None,
                name: "developer__shell".into(),
                arguments: Some(object!({"command": "echo hi"})),
            };
            let message = Message::assistant().with_tool_request("call_1", Ok(tool_call));
            return Ok((message, usage));
        }
        Ok((
            Message::assistant().with_text(self.final_text.clone()),
            usage,
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

/// Build an agent whose working dir is a temp dir containing the given
/// project hooks YAML, with project hooks enabled.
async fn agent_with_project_hooks(
    hooks_yaml: &str,
    provider: Arc<dyn Provider>,
) -> (Agent, String, TempDir) {
    // SAFETY: tests run single-threaded per process for this env var; it only
    // flips on the project-hook opt-in the HooksManager reads at construction.
    std::env::set_var("BIOROUTER_ALLOW_PROJECT_HOOKS", "1");

    let work_dir = TempDir::new().unwrap();
    std::fs::create_dir_all(work_dir.path().join(".biorouter")).unwrap();
    std::fs::write(work_dir.path().join(".biorouter/hooks.yaml"), hooks_yaml).unwrap();

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
            "hook-loop-test".to_string(),
            SessionType::Hidden,
        )
        .await
        .unwrap();

    agent.update_provider(provider, &session.id).await.unwrap();

    // data_dir must outlive the agent; leak it into the returned tempdir slot
    // by keeping work_dir (sessions live under data_dir, but the session row
    // is already created and cached). We keep work_dir alive for the hooks file.
    std::mem::forget(data_dir);
    (agent, session.id, work_dir)
}

fn collect_texts(messages: &[Message]) -> String {
    messages
        .iter()
        .flat_map(|m| m.content.iter())
        .filter_map(|c| match c {
            MessageContent::Text(t) => Some(t.text.clone()),
            MessageContent::SystemNotification(n) => Some(n.msg.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

async fn drain(agent: &Agent, user: &str, session_id: &str) -> Result<Vec<Message>> {
    let session_config = SessionConfig {
        id: session_id.to_string(),
        schedule_id: None,
        max_turns: Some(8),
        max_tool_calls: None,
        budget: None,
        retry_config: None,
    };
    let stream = agent
        .reply(Message::user().with_text(user), session_config, None)
        .await?;
    tokio::pin!(stream);
    let mut out = Vec::new();
    while let Some(ev) = stream.next().await {
        if let AgentEvent::Message(m) = ev? {
            // Auto-approve any tool confirmation so the loop progresses.
            if let Some(MessageContent::ActionRequired(action)) = m.content.first() {
                if let biorouter::conversation::message::ActionRequiredData::ToolConfirmation {
                    id,
                    ..
                } = &action.data
                {
                    agent
                        .handle_confirmation(
                            id.clone(),
                            biorouter::permission::PermissionConfirmation {
                                principal_type: biorouter::permission::permission_confirmation::PrincipalType::Tool,
                                permission: biorouter::permission::Permission::AllowOnce,
                            },
                        )
                        .await;
                }
            }
            out.push(m);
        }
    }
    Ok(out)
}

#[tokio::test]
async fn user_prompt_submit_block_short_circuits_reply() {
    let provider = Arc::new(ScriptedProvider::new(
        false,
        "this should never be produced",
    ));
    let (agent, session_id, _work) = agent_with_project_hooks(
        r#"hooks:
  UserPromptSubmit:
    - hooks:
        - type: command
          command: "echo 'prompt blocked by policy' >&2; exit 2"
"#,
        provider.clone(),
    )
    .await;

    let messages = drain(&agent, "do something", &session_id).await.unwrap();
    let text = collect_texts(&messages);
    assert!(
        text.contains("Prompt blocked by hook: prompt blocked by policy"),
        "expected the block notice, got: {text}"
    );
    // The provider must never have been called: the prompt was blocked first.
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn user_prompt_submit_context_is_injected_for_the_model() {
    let provider = Arc::new(ScriptedProvider::new(false, "done"));
    let (agent, session_id, _work) = agent_with_project_hooks(
        r#"hooks:
  UserPromptSubmit:
    - hooks:
        - type: command
          command: "echo 'remember: the codeword is ZEBRAFISH'"
"#,
        provider.clone(),
    )
    .await;

    let _ = drain(&agent, "hello", &session_id).await.unwrap();

    // The injected context should be persisted as a hidden user message.
    let session = agent
        .config
        .session_manager
        .get_session(&session_id, true)
        .await
        .unwrap();
    let convo = session.conversation.unwrap();
    let all = collect_texts(convo.messages());
    assert!(
        all.contains("ZEBRAFISH") && all.contains("hook-context"),
        "expected injected hook context in conversation, got: {all}"
    );
    assert!(provider.calls.load(Ordering::SeqCst) >= 1);
}

#[tokio::test]
async fn pre_tool_use_deny_feeds_reason_back_to_model() {
    let provider = Arc::new(ScriptedProvider::new(true, "acknowledged"));
    let (agent, session_id, _work) = agent_with_project_hooks(
        r#"hooks:
  PreToolUse:
    - matcher: "developer__shell"
      hooks:
        - type: command
          command: "echo 'shell is not allowed in this project' >&2; exit 2"
"#,
        provider.clone(),
    )
    .await;

    let _ = drain(&agent, "run a shell command", &session_id)
        .await
        .unwrap();

    let session = agent
        .config
        .session_manager
        .get_session(&session_id, true)
        .await
        .unwrap();
    let convo = session.conversation.unwrap();
    // The denied tool response must carry the hook's reason back to the model.
    let found = convo.messages().iter().any(|m| {
        m.content.iter().any(|c| {
            if let MessageContent::ToolResponse(tr) = c {
                if let Ok(result) = &tr.tool_result {
                    return result.content.iter().any(|content| {
                        matches!(&content.raw, rmcp::model::RawContent::Text(t)
                            if t.text.contains("Hook feedback: shell is not allowed in this project"))
                    });
                }
            }
            false
        })
    });
    assert!(found, "expected hook deny reason in the tool response");
}

#[tokio::test]
async fn stop_hook_blocks_then_releases() {
    // Stop hook blocks exactly once (via a marker file), then allows. The
    // loop should run an extra iteration and surface the block notice.
    let marker = TempDir::new().unwrap();
    let marker_path = marker.path().join("block-once");
    std::fs::write(&marker_path, "1").unwrap();

    let provider = Arc::new(ScriptedProvider::new(false, "all done"));
    let yaml = format!(
        r#"hooks:
  Stop:
    - hooks:
        - type: command
          command: "if [ -f '{p}' ]; then rm '{p}'; echo 'finish the task first' >&2; exit 2; fi"
"#,
        p = marker_path.display()
    );
    let (agent, session_id, _work) = agent_with_project_hooks(&yaml, provider.clone()).await;

    let messages = drain(&agent, "hi", &session_id).await.unwrap();
    let text = collect_texts(&messages);
    assert!(
        text.contains("Stop hook blocked completion: finish the task first"),
        "expected the stop-block notice, got: {text}"
    );
    // Blocked once then released → provider called at least twice.
    assert!(
        provider.calls.load(Ordering::SeqCst) >= 2,
        "stop block should have forced another turn"
    );
}
