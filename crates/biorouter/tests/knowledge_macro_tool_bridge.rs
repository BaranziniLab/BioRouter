//! A knowledge macro's own tools reach a coding-agent provider, and the calls
//! the child makes land on the macro's own dispatcher (#109).
//!
//! # What this covers that no unit test does
//!
//! The chain has four links in three crates and each one was a separate defect:
//!
//! 1. `SubAgent::run` hands its dispatcher to the completer
//!    (`Completer::complete_with_dispatch`) instead of keeping it for itself;
//! 2. `ProviderCompleter` notices the provider needs a bridge and builds a
//!    `ProviderToolTurnContext` around it;
//! 3. the context issues a grant whose dispatcher is the *macro's*, not an
//!    `ExtensionManager` — the macro's tools are in no extension at all, and its
//!    dispatcher carries the git transaction every write in the run must land on;
//! 4. the mirrored pairs come back as records, so the loop reports what ran
//!    without dispatching it a second time.
//!
//! Every link can be correct on its own and the feature still not work, which is
//! why this asserts the round trip: a tool the *provider's child* asked for, run
//! by the *macro's* dispatcher, reported in the run's own event log.
//!
//! The provider is a stub rather than a real CLI so this can run in CI. The live
//! version — a real `claude` and a real `codex` driving a real ingest — is
//! `--ignored` in `crates/biorouter-server/tests/tool_bridge_routes.rs`.

use std::sync::Arc;

use async_trait::async_trait;
use rmcp::model::Tool;

use biorouter::conversation::message::Message;
use biorouter::knowledge::provider_completer::ProviderCompleter;
use biorouter::model::ModelConfig;
use biorouter::providers::base::{MessageStream, Provider, ProviderMetadata, ProviderUsage, Usage};
use biorouter::providers::coding_agent::bridge;
use biorouter::providers::coding_agent::mirror;
use biorouter::providers::errors::ProviderError;
use biorouter_mcp::knowledge::subagent::events::{DoneReason, SubAgentEvent};
use biorouter_mcp::knowledge::subagent::loop_::{SubAgent, SubAgentBounds, ToolDispatch};

/// Sandbox this binary's config root, for the reason `tests/agent.rs` gives.
#[ctor::ctor]
fn sandbox_config_root_for_this_test_binary() {
    if std::env::var_os("BIOROUTER_PATH_ROOT").is_some() {
        return;
    }
    let root = tempfile::TempDir::new().expect("a scratch config root");
    std::env::set_var("BIOROUTER_PATH_ROOT", root.path());
    static ROOT: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    let _ = ROOT.set(root);
}

/// The macro's dispatcher: it records what it was asked to run, exactly as
/// `KbToolDispatch` would write to a transaction branch.
#[derive(Default)]
struct RecordingKbDispatch {
    calls: std::sync::Mutex<Vec<(String, serde_json::Value)>>,
}

#[async_trait]
impl ToolDispatch for RecordingKbDispatch {
    async fn call(&self, name: &str, args: serde_json::Value) -> anyhow::Result<String> {
        self.calls
            .lock()
            .unwrap()
            .push((name.to_string(), args.clone()));
        Ok(format!("wrote {}", args["path"].as_str().unwrap_or("?")))
    }
}

/// A provider shaped like `claude_code`: it needs the bridge, it streams, and —
/// standing in for the child process — it CALLS BACK over the bridge before
/// answering, then mirrors what it did.
struct BridgedStub;

#[async_trait]
impl Provider for BridgedStub {
    fn metadata() -> ProviderMetadata
    where
        Self: Sized,
    {
        unimplemented!()
    }
    fn get_name(&self) -> &str {
        "claude_code"
    }
    fn get_model_config(&self) -> ModelConfig {
        ModelConfig::new_or_fail("claude-3-5-sonnet-20241022")
    }
    fn uses_tool_bridge(&self) -> bool {
        true
    }
    fn supports_streaming(&self) -> bool {
        true
    }
    async fn complete_with_model(
        &self,
        _model_config: &ModelConfig,
        _system: &str,
        _messages: &[Message],
        _tools: &[Tool],
    ) -> Result<(Message, ProviderUsage), ProviderError> {
        unreachable!("this stub streams")
    }

    async fn stream(
        &self,
        _system: &str,
        _messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        // Read at CONSTRUCTION — the task-local scope is gone by the time the
        // returned stream is polled. Both real providers do exactly this.
        let url = bridge::active_bridge_url()
            .ok_or_else(|| ProviderError::ExecutionError("no bridge was offered".into()))?;
        let nonce = url.rsplit('/').next().unwrap_or_default().to_string();
        let grant = bridge::lookup(&nonce)
            .ok_or_else(|| ProviderError::ExecutionError("the grant was not live".into()))?;

        // The grant must advertise the MACRO's tools, not a session's extensions.
        assert!(
            grant
                .tools()
                .iter()
                .any(|t| t.name.as_ref() == "kb_write_page"),
            "the macro's own tool surface must be what crosses the bridge; got {:?}",
            grant.tools().iter().map(|t| &t.name).collect::<Vec<_>>()
        );
        assert_eq!(
            grant.tools().len(),
            tools.len(),
            "the grant must advertise exactly what the macro passed"
        );

        let args = serde_json::json!({ "path": "knowledge/topic/tocilizumab.md" });
        let call = rmcp::model::CallToolRequestParams {
            name: "kb_write_page".into(),
            arguments: args.as_object().cloned(),
            meta: None,
            task: None,
        };
        let result = grant
            .call(call.clone())
            .await
            .map_err(ProviderError::ExecutionError)?;
        let output = result
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect::<Vec<_>>()
            .join("");

        let request = mirror::request_message(
            "call-1",
            "mcp__biorouter__kb_write_page",
            args,
            mirror::Execution::Bridged,
        );
        let response = mirror::response_message(
            "call-1",
            vec![rmcp::model::Content::text(output)],
            false,
            mirror::Execution::Bridged,
        );
        let done = Message::assistant().with_text("Recorded the source page.");
        let usage = ProviderUsage::new("claude_code".into(), Usage::new(None, None, None));

        Ok(Box::pin(futures::stream::iter(vec![
            Ok((Some(request), None, None)),
            Ok((Some(response), None, None)),
            Ok((Some(done), Some(usage), None)),
        ])))
    }
}

fn kb_tools() -> Vec<Tool> {
    vec![Tool::new(
        "kb_write_page",
        "Write a knowledge page.",
        Arc::new(
            serde_json::from_value(serde_json::json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }))
            .expect("a valid schema"),
        ),
    )]
}

#[tokio::test]
async fn a_macro_run_under_a_coding_agent_executes_its_own_tools() {
    bridge::publish_base_url("http://127.0.0.1:65535");

    let dispatch = Arc::new(RecordingKbDispatch::default());
    let (completer, _tier, _affiliation) =
        ProviderCompleter::paired(Arc::new(BridgedStub) as Arc<dyn Provider>);

    let agent = SubAgent {
        completer: Box::new(completer.in_session("kb-macro-e2e")),
        tools: kb_tools(),
        system_prompt: "You are a knowledge curator.".to_string(),
        bounds: SubAgentBounds::default(),
    };

    let result = agent
        .run(
            "Digest this source.",
            Arc::clone(&dispatch) as Arc<dyn ToolDispatch>,
            None,
            None,
        )
        .await
        .expect("the run completes");

    // 1. The macro's OWN dispatcher ran the call the child made.
    let calls = dispatch.calls.lock().unwrap().clone();
    assert_eq!(
        calls.len(),
        1,
        "the child's call must land on the macro's dispatcher: {calls:?}"
    );
    assert_eq!(calls[0].0, "kb_write_page");
    assert_eq!(calls[0].1["path"], "knowledge/topic/tocilizumab.md");

    // 2. It is in the run log, so a reader cannot tell — and should not have to —
    //    whether the loop or a child agent ran it.
    let logged: Vec<&SubAgentEvent> = result
        .events
        .iter()
        .filter(|e| {
            matches!(
                e,
                SubAgentEvent::ToolCall { .. } | SubAgentEvent::ToolResult { .. }
            )
        })
        .collect();
    assert_eq!(logged.len(), 2, "one call and one result: {logged:?}");
    assert!(matches!(
        logged[1],
        SubAgentEvent::ToolResult { ok: true, .. }
    ));

    // 3. And it ran exactly ONCE. A loop that mistook the mirrored record for a
    //    request would write the page a second time — invisible, and a real
    //    corruption of the base rather than a display glitch.
    assert_eq!(
        dispatch.calls.lock().unwrap().len(),
        1,
        "a mirrored record must never be dispatched again"
    );

    assert_eq!(result.reason, DoneReason::NoMoreToolCalls);
    assert!(
        result.final_text.contains("Recorded the source page"),
        "the child's answer must survive: {:?}",
        result.final_text
    );
}
