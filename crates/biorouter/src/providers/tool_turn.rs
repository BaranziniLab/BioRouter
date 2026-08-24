//! Run one provider turn with Biorouter's tools available — whichever provider
//! it is (#109).
//!
//! # Why this exists
//!
//! Most providers receive their tools in the request. The two coding-agent
//! providers cannot: `claude_code` and `codex` drive a whole agent in a child
//! process, so the only way a Biorouter tool reaches them is a callback over the
//! MCP bridge, and the only way the bridge exists is that somebody issued a
//! grant and scoped its URL around the provider call.
//!
//! For a long time exactly one caller did that: `Agent::reply`. Everything else
//! that runs an agentic loop — the knowledge ingest / query / lint macros,
//! scheduled workflows, bounded sub-agents — calls `Provider::complete` directly
//! with a `tools` argument, which those two providers **discard**. The result was
//! not an error. The model produced a complete, correct plan with every call
//! written out as prose, invented its own `<tool_response>OK</tool_response>`
//! replies to continue against, and nothing reached disk. The user waited out a
//! full model run for nothing, and the UI's answer was a hardcoded provider
//! denylist.
//!
//! That is a structural mismatch, not a model-quality problem: the workflow *had*
//! tools; the provider had no grant. This is the missing piece — the one place
//! that knows how to give any provider Biorouter's tools for the length of one
//! turn.
//!
//! # What it is not
//!
//! Not a second gate stack. A bridged call still passes through
//! [`crate::providers::coding_agent::bridge::BridgeGrant::call`]: the inspectors,
//! the permission decision (including the human approval round trip added by
//! #107), privacy Gate C, `.biorouterignore`, the vault, the `PreToolUse`
//! rewrite and re-judgement, and the turn's cancellation token. This decides
//! *which* tools and *where they land*, never *whether a call may run*.
//!
//! And not a provider switch. The whole point of the coding-agent providers is
//! that a user's existing Anthropic or ChatGPT plan pays for the turn; nothing
//! here reaches for an API key or silently substitutes a billed model.

use std::sync::Arc;

use rmcp::model::Tool;
use tokio_util::sync::CancellationToken;

use crate::agents::extension_manager::ExtensionManager;
use crate::config::BioRouterMode;
use crate::conversation::message::Message;
use crate::conversation::Conversation;
use crate::hooks::HooksManager;
use crate::permission::tool_risk::ToolRiskRegistry;
use crate::privacy::CallCapability;
use crate::providers::base::{Provider, ProviderUsage};
use crate::providers::coding_agent::{bridge, mirror};
use crate::providers::errors::ProviderError;
use crate::session::session_manager::Session;
use crate::tool_inspection::ToolInspectionManager;

/// Everything one turn needs in order to offer Biorouter's tools to any
/// provider, coding agents included.
///
/// Built once per workflow and reused across that workflow's turns; the grant it
/// issues lives for exactly one provider call. A grant that outlived its call
/// would be a capability with no owner, which is why [`Self::run`] holds the
/// lease rather than handing it out.
pub struct ProviderToolTurnContext {
    session: Session,
    mode: BioRouterMode,
    dispatcher: Arc<dyn bridge::BridgeToolDispatch>,
    inspections: Arc<ToolInspectionManager>,
    capability: CallCapability,
    conversation: Conversation,
    cancel: Option<CancellationToken>,
    hooks: Arc<HooksManager>,
    vault: Option<Arc<crate::agents::vault_refs::VaultRefs>>,
    tool_risks: Arc<ToolRiskRegistry>,
}

/// What one turn produced.
#[derive(Debug)]
pub struct ProviderTurn {
    /// Every message the turn yielded, in order: the assistant's prose, and —
    /// for a coding-agent provider — the mirrored `ToolRequest`/`ToolResponse`
    /// pairs recording what the child ran through the bridge.
    ///
    /// A `Vec`, not one `Message`, because that is what a turn *is* on the
    /// streaming path, and flattening it here would throw away the ordering a
    /// caller needs to write a faithful run log.
    pub messages: Vec<Message>,
    pub usage: ProviderUsage,
    /// Tool calls the **child already ran** over the bridge, in order.
    ///
    /// Empty for every provider that receives its tools in the request: those
    /// return tool *requests* for the caller to dispatch, and this stays empty so
    /// the caller's existing loop is untouched.
    ///
    /// ⚠ **A caller must not dispatch these.** They have already executed, behind
    /// the full gate stack, on Biorouter's side of the bridge. Re-running a
    /// `developer__shell` because a loop could not tell a record from a request
    /// is not a display glitch — it is the command running twice. This is the
    /// same hazard `Agent::reply` closes with `mirror::contains_provider_executed`,
    /// surfaced here as data rather than left for each caller to rediscover.
    pub executed: Vec<ExecutedToolCall>,
}

impl ProviderTurn {
    /// The assistant's prose, concatenated.
    pub fn text(&self) -> String {
        self.messages
            .iter()
            .map(Message::as_concat_text)
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Tool calls the caller must still dispatch.
    ///
    /// **Mirrored requests are excluded.** They have already run, behind the
    /// full gate stack, on Biorouter's side of the bridge, and a caller that
    /// dispatched one would execute it a second time — a shell command run
    /// twice, not a display glitch. That exclusion is the whole reason this
    /// accessor exists instead of leaving each caller to walk `messages` and
    /// rediscover the marker.
    pub fn pending_tool_calls(&self) -> Vec<&crate::conversation::message::ToolRequest> {
        use crate::conversation::message::MessageContent;
        self.messages
            .iter()
            .flat_map(|m| m.content.iter())
            .filter_map(|c| match c {
                MessageContent::ToolRequest(r) if mirror::request_execution(r).is_none() => Some(r),
                _ => None,
            })
            .collect()
    }
}

/// One call the child made and Biorouter ran.
#[derive(Debug, Clone)]
pub struct ExecutedToolCall {
    pub id: String,
    /// The tool's own name, with the child's `mcp__biorouter__` prefix already
    /// stripped by the mirror.
    pub name: String,
    pub arguments: serde_json::Value,
    /// The result as text, and whether the tool reported failure.
    pub output: String,
    pub is_error: bool,
}

impl ProviderToolTurnContext {
    /// Eleven arguments, flat, for the reason
    /// [`bridge::BridgeGrant::new`] gives at length: each one is a distinct
    /// thing a caller has to remember to hand over, and three of the grant's
    /// were found missing precisely by reading the list against what a chat turn
    /// does. A wrapper struct would move the omission one file away without
    /// removing a field.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session: Session,
        mode: BioRouterMode,
        dispatcher: Arc<dyn bridge::BridgeToolDispatch>,
        inspections: Arc<ToolInspectionManager>,
        capability: CallCapability,
        conversation: Conversation,
        cancel: Option<CancellationToken>,
        hooks: Arc<HooksManager>,
        vault: Option<Arc<crate::agents::vault_refs::VaultRefs>>,
        tool_risks: Arc<ToolRiskRegistry>,
    ) -> Self {
        Self {
            session,
            mode,
            dispatcher,
            inspections,
            capability,
            conversation,
            cancel,
            hooks,
            vault,
            tool_risks,
        }
    }

    /// The context a bounded workflow wants: its own tool surface, its own
    /// dispatcher, and the defaults for everything a chat turn carries that a
    /// workflow does not have.
    ///
    /// `capability` is **taken**, not derived, because a workflow's privacy
    /// capability belongs to the provider it is about to bind — sampling it here
    /// would be a second read of the master switch and exactly the race
    /// `CallCapability` exists to close.
    pub fn for_workflow(
        session: Session,
        dispatcher: Arc<dyn bridge::BridgeToolDispatch>,
        capability: CallCapability,
        cancel: Option<CancellationToken>,
    ) -> Self {
        let tool_risks = Arc::new(ToolRiskRegistry::new());
        Self::new(
            session,
            // Auto: a workflow the user explicitly started is an authorised
            // step, and its tool surface is the small one the workflow chose
            // rather than the machine. Genuinely sensitive operations are still
            // caught — the security and sensitive-ops inspectors sit above the
            // permission one and escalate regardless of mode, and since #107 an
            // escalation raises a real dialog instead of dead-ending.
            BioRouterMode::Auto,
            dispatcher,
            Arc::new(workflow_inspectors(Arc::clone(&tool_risks))),
            capability,
            Conversation::new_unvalidated(vec![]),
            cancel,
            Arc::new(HooksManager::with_config(
                Default::default(),
                false,
                Arc::new(tokio::sync::Mutex::new(None)),
            )),
            None,
            tool_risks,
        )
    }

    /// Replace the inspection stack. A workflow that wants the real inspectors
    /// (rather than the empty default) hands them here.
    #[must_use]
    pub fn with_inspections(mut self, inspections: Arc<ToolInspectionManager>) -> Self {
        self.inspections = inspections;
        self
    }

    /// Run one turn.
    ///
    /// For a provider that receives its tools in the request this is
    /// `provider.complete(...)` and nothing else — no grant is issued, because
    /// issuing one would leave a live capability on every turn in the process
    /// for no benefit.
    ///
    /// For a coding-agent provider it issues a one-turn grant over `tools`,
    /// scopes its URL around the call, and drops the lease the moment the call
    /// returns.
    ///
    /// ⚠ **The URL rides a task-local and must be read at construction time.**
    /// The scope below wraps the awaited call that *builds* the response, not the
    /// consumption of anything it returns. That is why this uses `complete`
    /// rather than `stream`: a stream's poll happens after the scope is gone.
    pub async fn run(
        &self,
        provider: &Arc<dyn Provider>,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<ProviderTurn, ProviderError> {
        if !provider.uses_tool_bridge() {
            let (message, usage) = provider.complete(system, messages, tools).await?;
            return Ok(ProviderTurn {
                messages: vec![message],
                usage,
                executed: Vec::new(),
            });
        }
        self.run_bridged(provider, system, messages, tools).await
    }

    async fn run_bridged(
        &self,
        provider: &Arc<dyn Provider>,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<ProviderTurn, ProviderError> {
        // BR-18's grades, for THIS turn's surface. The registry starts from the
        // built-in table and learns the rest from the tools it is shown, exactly
        // as `Agent::prepare_tools` refreshes it — a workflow's tools are its
        // own, so nothing else could have taught it about them, and an ungraded
        // tool would reach an approval card with nothing to say about its risk.
        self.tool_risks.refresh_from_tools(tools);

        let grant = bridge::BridgeGrant::new(
            self.session.clone(),
            self.mode,
            Arc::clone(&self.dispatcher),
            Arc::clone(&self.inspections),
            self.capability,
            tools.to_vec(),
            self.conversation.clone(),
            self.cancel.clone(),
            Arc::clone(&self.hooks),
            self.vault.clone(),
            Arc::clone(&self.tool_risks),
        );

        // `None` means no HTTP server in this process — a CLI run, a unit test.
        // The child then runs tool-less, which for a *chat* turn is the right
        // degradation (an answer from the conversation beats a failed turn) but
        // for a workflow made of tool calls is a guaranteed silent failure: the
        // model narrates the calls and writes nothing. So this is the one place
        // that refuses instead, before a full model run is spent.
        let Some(lease) = bridge::issue(grant) else {
            return Err(ProviderError::ExecutionError(format!(
                "`{}` runs its tools by calling Biorouter back over HTTP, and this process has \
                 no server for it to reach. Run this through the Biorouter desktop app or a \
                 running `biorouterd`, or choose a provider that receives its tools directly.",
                provider.get_name()
            )));
        };

        let url = lease.url().to_string();

        // Streaming, when the provider offers it, and NOT for latency: the
        // mirrored `ToolRequest`/`ToolResponse` pairs recording what the child
        // ran exist only on the streaming path. `complete_with_model` returns the
        // final text alone, so a workflow driven through it would execute every
        // tool correctly and be unable to say afterwards which ones — no run log,
        // no attribution, nothing for the user to audit.
        //
        // ⚠ The stream is **constructed inside the scope and consumed outside**.
        // The task-local wraps the awaited call that builds the stream, not the
        // polling of what it returns, and both coding-agent providers read the
        // URL and spawn the child at construction for exactly that reason. The
        // *lease* is not the constraint — it is bound here and lives to the end
        // of this function, which outlasts the consumption below.
        if provider.supports_streaming() {
            let stream = bridge::ACTIVE_BRIDGE_URL
                .scope(Some(url), provider.stream(system, messages, tools))
                .await?;
            return collect_stream(stream, provider.get_name()).await;
        }

        let (message, usage) = bridge::ACTIVE_BRIDGE_URL
            .scope(Some(url), provider.complete(system, messages, tools))
            .await?;
        let executed = executed_calls(std::slice::from_ref(&message));
        Ok(ProviderTurn {
            messages: vec![message],
            usage,
            executed,
        })
    }
}

/// Drain a provider stream into one turn.
async fn collect_stream(
    mut stream: crate::providers::base::MessageStream,
    provider_name: &str,
) -> Result<ProviderTurn, ProviderError> {
    use futures::StreamExt;

    let mut messages: Vec<Message> = Vec::new();
    let mut usage: Option<ProviderUsage> = None;
    while let Some(item) = stream.next().await {
        let (message, chunk_usage, _pending) = item?;
        if let Some(message) = message {
            messages.push(message);
        }
        // Last snapshot wins, exactly as the agent loop records it: a provider
        // that reports usage on more than one chunk must not be counted twice.
        if let Some(chunk_usage) = chunk_usage {
            usage = Some(chunk_usage);
        }
    }
    let usage = usage.unwrap_or_else(|| {
        ProviderUsage::new(
            provider_name.to_string(),
            crate::providers::base::Usage::new(None, None, None),
        )
    });
    let executed = executed_calls(&messages);
    Ok(ProviderTurn {
        messages,
        usage,
        executed,
    })
}

/// Read the calls the child already executed off a mirrored reply.
///
/// The mirror stamps every pair it mints with `biorouterProviderExecuted`, and
/// this reads only the pairs marked `bridged` — a `child` marker means the call
/// ran inside the child's own sandbox under **none** of Biorouter's gates, and a
/// workflow must not record one as its own work.
fn executed_calls(messages: &[Message]) -> Vec<ExecutedToolCall> {
    use crate::conversation::message::MessageContent;

    let mut by_id: std::collections::HashMap<String, ExecutedToolCall> =
        std::collections::HashMap::new();
    let mut order: Vec<String> = Vec::new();

    for content in messages.iter().flat_map(|m| m.content.iter()) {
        match content {
            MessageContent::ToolRequest(request)
                if mirror::request_execution(request) == Some(mirror::Execution::Bridged) =>
            {
                let Ok(call) = request.tool_call.as_ref() else {
                    continue;
                };
                order.push(request.id.clone());
                by_id.insert(
                    request.id.clone(),
                    ExecutedToolCall {
                        id: request.id.clone(),
                        name: call.name.to_string(),
                        arguments: call
                            .arguments
                            .clone()
                            .map(serde_json::Value::Object)
                            .unwrap_or(serde_json::Value::Null),
                        output: String::new(),
                        is_error: false,
                    },
                );
            }
            MessageContent::ToolResponse(response) => {
                let Some(entry) = by_id.get_mut(&response.id) else {
                    continue;
                };
                match response.tool_result.as_ref() {
                    Ok(result) => {
                        entry.is_error = result.is_error.unwrap_or(false);
                        entry.output = result
                            .content
                            .iter()
                            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
                            .collect::<Vec<_>>()
                            .join("\n");
                    }
                    Err(e) => {
                        entry.is_error = true;
                        entry.output = e.message.to_string();
                    }
                }
            }
            _ => {}
        }
    }

    order
        .into_iter()
        .filter_map(|id| by_id.remove(&id))
        .collect()
}

/// The gate stack a workflow turn runs behind.
///
/// The inspectors a chat turn registers, minus the two that need agent-owned
/// state (hooks, repetition history) and are meaningless for a bounded run with
/// no conversation. It is a real stack and not an empty one for a reason that
/// bites immediately: `BridgeGrant` refuses a call whose verdict is "no decision
/// was reached", because an absent decision must never read as approval — so an
/// empty manager does not mean "allow everything", it means every bridged call
/// is refused.
///
/// The **security** and **sensitive-ops** inspectors are the load-bearing half.
/// They escalate regardless of mode, so `BioRouterMode::Auto` above is a
/// statement about ordinary authorised steps, not a blanket grant: a workflow
/// that reaches for something genuinely dangerous still raises the #107 dialog.
fn workflow_inspectors(tool_risks: Arc<ToolRiskRegistry>) -> ToolInspectionManager {
    use crate::config::permission::PermissionManager;
    use crate::managed::ManagedPolicy;
    use crate::permission::managed_inspector::ManagedPolicyInspector;
    use crate::permission::permission_inspector::PermissionInspector;
    use crate::security::security_inspector::SecurityInspector;
    use crate::security::sensitive_ops::SensitiveOpsInspector;

    let managed = Arc::new(ManagedPolicy::empty());
    let mut manager = ToolInspectionManager::new();
    manager.add_inspector(Box::new(ManagedPolicyInspector::new(Arc::clone(&managed))));
    manager.add_inspector(Box::new(SecurityInspector::new()));
    manager.add_inspector(Box::new(SensitiveOpsInspector));
    manager.add_inspector(Box::new(PermissionInspector::new(
        tool_risks,
        PermissionManager::instance(),
        managed,
        // No provider: a workflow has no lead/worker pair to grade against, and
        // the inspector's only use for one is smart-approve's model call.
        Arc::new(tokio::sync::Mutex::new(None)),
    )));
    manager
}

/// The session's whole extension surface, as a bridge dispatcher.
///
/// A named helper rather than a bare `as` cast at each call site, because the
/// cast is where a reader asks "which tools is this child getting?" and the
/// answer deserves a name.
pub fn session_tools(extensions: Arc<ExtensionManager>) -> Arc<dyn bridge::BridgeToolDispatch> {
    extensions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::MessageContent;
    use crate::model::ModelConfig;
    use crate::providers::base::{ProviderMetadata, Usage};
    use async_trait::async_trait;
    use rmcp::model::{CallToolRequestParams, CallToolResult, Content};

    struct Stub {
        name: &'static str,
        bridged: bool,
        reply: Message,
        /// Set when the provider ran inside a live bridge scope.
        saw_bridge: Arc<std::sync::atomic::AtomicBool>,
    }

    #[async_trait]
    impl Provider for Stub {
        fn metadata() -> ProviderMetadata
        where
            Self: Sized,
        {
            unimplemented!()
        }
        fn get_name(&self) -> &str {
            self.name
        }
        fn get_model_config(&self) -> ModelConfig {
            ModelConfig::new_or_fail("claude-3-5-sonnet-20241022")
        }
        fn uses_tool_bridge(&self) -> bool {
            self.bridged
        }
        async fn complete_with_model(
            &self,
            _model_config: &ModelConfig,
            _system: &str,
            _messages: &[Message],
            _tools: &[Tool],
        ) -> Result<(Message, ProviderUsage), ProviderError> {
            self.saw_bridge.store(
                bridge::active_bridge_url().is_some(),
                std::sync::atomic::Ordering::SeqCst,
            );
            Ok((
                self.reply.clone(),
                ProviderUsage::new(self.name.into(), Usage::new(None, None, None)),
            ))
        }
    }

    struct RecordingDispatch;

    #[async_trait]
    impl bridge::BridgeToolDispatch for RecordingDispatch {
        async fn dispatch(
            &self,
            _session_id: &str,
            call: CallToolRequestParams,
            _capability: CallCapability,
            _cancel: CancellationToken,
        ) -> Result<CallToolResult, String> {
            Ok(CallToolResult::success(vec![Content::text(format!(
                "ran {}",
                call.name
            ))]))
        }
    }

    fn context() -> ProviderToolTurnContext {
        ProviderToolTurnContext::for_workflow(
            Session {
                id: format!("tool-turn-{:016x}", rand::random::<u64>()),
                ..Session::default()
            },
            Arc::new(RecordingDispatch),
            CallCapability::public_enforced(),
            None,
        )
    }

    fn stub(
        name: &'static str,
        bridged: bool,
        reply: Message,
    ) -> (Arc<dyn Provider>, Arc<std::sync::atomic::AtomicBool>) {
        let saw = Arc::new(std::sync::atomic::AtomicBool::new(false));
        (
            Arc::new(Stub {
                name,
                bridged,
                reply,
                saw_bridge: Arc::clone(&saw),
            }),
            saw,
        )
    }

    /// The whole point of #109: a coding-agent provider reached from a workflow
    /// gets a live bridge, where before it got a `tools` argument it discarded.
    #[tokio::test]
    async fn a_coding_agent_provider_runs_inside_a_live_bridge() {
        bridge::publish_base_url("http://127.0.0.1:65535");
        let (provider, saw_bridge) = stub("codex", true, Message::assistant().with_text("ok"));
        let turn = context()
            .run(&provider, "sys", &[Message::user().with_text("hi")], &[])
            .await
            .expect("the turn runs");
        assert!(
            saw_bridge.load(std::sync::atomic::Ordering::SeqCst),
            "the provider must be able to read a bridge URL during its own call"
        );
        assert_eq!(turn.text(), "ok");
    }

    /// And the lease is gone the moment the call returns: a grant that outlived
    /// its turn would be a live capability onto the workflow's tools with
    /// nothing owning it.
    #[tokio::test]
    async fn the_bridge_is_gone_once_the_turn_returns() {
        bridge::publish_base_url("http://127.0.0.1:65535");
        let (provider, _) = stub("codex", true, Message::assistant().with_text("ok"));
        let _ = context()
            .run(&provider, "sys", &[Message::user().with_text("hi")], &[])
            .await
            .expect("the turn runs");
        assert!(
            bridge::active_bridge_url().is_none(),
            "the task-local must not survive the call"
        );
    }

    /// A provider that receives its tools in the request must not be given a
    /// grant. Issuing one anyway would leave a live capability on every turn in
    /// the process for no benefit.
    #[tokio::test]
    async fn an_ordinary_provider_gets_no_grant() {
        bridge::publish_base_url("http://127.0.0.1:65535");
        let (provider, saw_bridge) = stub("anthropic", false, Message::assistant().with_text("ok"));
        let turn = context()
            .run(&provider, "sys", &[Message::user().with_text("hi")], &[])
            .await
            .expect("the turn runs");
        assert!(
            !saw_bridge.load(std::sync::atomic::Ordering::SeqCst),
            "an ordinary provider must not see a bridge URL"
        );
        assert!(
            turn.executed.is_empty(),
            "nothing ran on Biorouter's side, so the caller's own loop is untouched"
        );
    }

    /// Mirrored pairs come back as *records*, so a caller's loop can see what
    /// already ran instead of dispatching it a second time.
    #[tokio::test]
    async fn a_mirrored_pair_is_reported_as_already_executed() {
        bridge::publish_base_url("http://127.0.0.1:65535");
        let request = mirror::request_message(
            "call-1",
            "mcp__biorouter__kb_write_page",
            serde_json::json!({"path": "knowledge/a.md"}),
            mirror::Execution::Bridged,
        );
        let response = mirror::response_message(
            "call-1",
            vec![Content::text("written")],
            false,
            mirror::Execution::Bridged,
        );
        let (provider, _) = streaming_stub("claude_code", vec![request, response]);

        let turn = context()
            .run(&provider, "sys", &[Message::user().with_text("hi")], &[])
            .await
            .expect("the turn runs");

        assert_eq!(turn.executed.len(), 1, "one bridged call ran");
        assert_eq!(
            turn.executed[0].name, "kb_write_page",
            "the child's `mcp__biorouter__` prefix must be stripped"
        );
        assert_eq!(turn.executed[0].arguments["path"], "knowledge/a.md");
        assert!(!turn.executed[0].is_error);
        assert!(
            turn.executed[0].output.contains("written"),
            "the tool's own output must survive: {:?}",
            turn.executed[0].output
        );
        // And it is NOT offered back to the caller to dispatch — running it a
        // second time is a second write, not a redraw.
        assert!(
            turn.pending_tool_calls().is_empty(),
            "a mirrored request must never be handed back as work to do"
        );
        assert!(
            turn.messages
                .iter()
                .flat_map(|m| m.content.iter())
                .any(|c| matches!(c, MessageContent::ToolRequest(_))),
            "the record is still in the turn, for the transcript"
        );
    }

    /// An UNMARKED tool request is the caller's to run. This is the other half
    /// of the same safety property: the accessor must not swallow real work.
    #[tokio::test]
    async fn an_unmarked_request_is_still_the_callers_to_dispatch() {
        bridge::publish_base_url("http://127.0.0.1:65535");
        let call = CallToolRequestParams {
            name: "kb_search".into(),
            arguments: None,
            meta: None,
            task: None,
        };
        let reply = Message::assistant().with_tool_request("req-1", Ok(call));
        let (provider, _) = stub("anthropic", false, reply);
        let turn = context()
            .run(&provider, "sys", &[Message::user().with_text("hi")], &[])
            .await
            .expect("the turn runs");
        assert_eq!(turn.pending_tool_calls().len(), 1);
        assert!(turn.executed.is_empty());
    }

    /// A workflow made of tool calls must fail *before* spending a model run
    /// when there is no server for the child to call back into. The chat loop's
    /// degradation — run tool-less and answer from the conversation — is exactly
    /// wrong here: the model narrates the calls and writes nothing.
    // The "no HTTP server" refusal is pinned in its own integration binary,
    // `tests/provider_tool_turn_no_server.rs`. It cannot live here: the
    // published base URL is process-global (it must be — the HTTP handler runs
    // on a different task from the turn that issued the grant), so a unit test
    // that unpublished it would break every concurrently-running bridge test in
    // this crate. A separate binary is a separate process that has simply never
    // published one, which is the state being tested.

    /// A streaming stub, for the tests that need mirrored pairs: those exist
    /// only on the streaming path, because `complete_with_model` returns the
    /// final text alone.
    fn streaming_stub(
        name: &'static str,
        yielded: Vec<Message>,
    ) -> (Arc<dyn Provider>, Arc<std::sync::atomic::AtomicBool>) {
        let saw = Arc::new(std::sync::atomic::AtomicBool::new(false));
        (
            Arc::new(StreamingStub {
                name,
                yielded,
                saw_bridge: Arc::clone(&saw),
            }),
            saw,
        )
    }

    struct StreamingStub {
        name: &'static str,
        yielded: Vec<Message>,
        saw_bridge: Arc<std::sync::atomic::AtomicBool>,
    }

    #[async_trait]
    impl Provider for StreamingStub {
        fn metadata() -> ProviderMetadata
        where
            Self: Sized,
        {
            unimplemented!()
        }
        fn get_name(&self) -> &str {
            self.name
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
            _tools: &[Tool],
        ) -> Result<crate::providers::base::MessageStream, ProviderError> {
            // Read at CONSTRUCTION, exactly as both real providers do: the scope
            // is gone by the time the returned stream is polled.
            self.saw_bridge.store(
                bridge::active_bridge_url().is_some(),
                std::sync::atomic::Ordering::SeqCst,
            );
            let items: Vec<_> = self
                .yielded
                .iter()
                .cloned()
                .map(|m| Ok((Some(m), None, None)))
                .collect();
            Ok(Box::pin(futures::stream::iter(items)))
        }
    }

    /// The streaming path's task-local discipline, asserted rather than assumed:
    /// a provider that reads the URL when its stream is *built* must find one,
    /// even though the stream is drained after the scope has closed.
    #[tokio::test]
    async fn a_streaming_provider_reads_the_url_at_construction() {
        bridge::publish_base_url("http://127.0.0.1:65535");
        let (provider, saw_bridge) =
            streaming_stub("claude_code", vec![Message::assistant().with_text("done")]);
        let turn = context()
            .run(&provider, "sys", &[Message::user().with_text("hi")], &[])
            .await
            .expect("the turn runs");
        assert!(
            saw_bridge.load(std::sync::atomic::Ordering::SeqCst),
            "the URL must be readable while the stream is being built"
        );
        assert_eq!(turn.text(), "done");
    }
}
