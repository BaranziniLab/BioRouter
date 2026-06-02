//! Bounded sub-agent loop.
//!
//! The `Completer` trait abstracts the LLM call so that:
//!   - `biorouter-mcp` has no cyclic dependency on `biorouter` (which already
//!     depends on `biorouter-mcp`).
//!   - Tests use a trivial `MockCompleter` without needing a real provider.
//!   - Production callers (in `biorouter` or a macro layer that has access to
//!     both crates) wrap `Arc<dyn Provider>` in an adapter.

use crate::knowledge::subagent::events::{DoneReason, SubAgentEvent};
use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::Tool;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tokio::time::Duration;

// ---------------------------------------------------------------------------
// Message types used inside the loop (provider-agnostic)
// ---------------------------------------------------------------------------

/// One "tool request" emitted by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmToolCall {
    /// The unique request-id assigned by the provider.
    pub id: String,
    /// Tool name.
    pub name: String,
    /// JSON arguments (may be Value::Object or Value::Null).
    pub args: serde_json::Value,
}

/// The reply we get back from the LLM after one `complete()` call.
#[derive(Debug, Clone)]
pub struct LlmReply {
    /// The plain-text portion of the assistant's response.
    pub text: String,
    /// Any tool calls embedded in the response (may be empty).
    pub tool_calls: Vec<LlmToolCall>,
}

// ---------------------------------------------------------------------------
// Completer trait  (thin LLM abstraction, loop-internal)
// ---------------------------------------------------------------------------

/// One tool-result part within a compound `LlmMessage::ToolResults` turn.
#[derive(Debug, Clone)]
pub struct ToolResultPart {
    /// Matches `LlmToolCall::id` from the corresponding assistant reply.
    pub request_id: String,
    /// Tool name (for debugging / logging).
    pub name: String,
    /// The string the tool returned.
    pub content: String,
}

/// A conversation turn in the format the Completer expects.
#[derive(Debug, Clone)]
pub enum LlmMessage {
    /// Initial or follow-up user text.
    User(String),
    /// The assistant's previous reply (needed so the provider can see context).
    Assistant(LlmReply),
    /// The result we are feeding back for a specific tool request.
    ///
    /// Kept for backward compatibility (single tool call per turn).  For turns
    /// with multiple tool calls, prefer `ToolResults` so all results are bundled
    /// into a single user message — required by Bedrock's strict validation.
    ToolResult {
        /// Matches `LlmToolCall::id` from the corresponding assistant reply.
        request_id: String,
        /// Tool name (for debugging / logging).
        name: String,
        /// The string the tool returned.
        content: String,
    },
    /// All tool results from a single assistant turn, bundled together.
    ///
    /// Bedrock (and the Anthropic spec) require that when an assistant turn
    /// contains N `tool_use` blocks, ALL N `tool_result` blocks MUST appear
    /// in a SINGLE subsequent user message.  Using this variant instead of N
    /// separate `ToolResult` entries satisfies that constraint.
    ToolResults(Vec<ToolResultPart>),
}

/// Minimal LLM capability needed by the sub-agent loop.
///
/// Callers that hold an `Arc<dyn biorouter::providers::base::Provider>` can
/// provide a thin adapter in their crate.  Inside `biorouter-mcp` we only
/// use the `MockCompleter` in tests.
#[async_trait]
pub trait Completer: Send + Sync {
    /// Send the current conversation (system + messages) to the LLM and
    /// return the next assistant reply.
    async fn complete(
        &self,
        system: &str,
        messages: &[LlmMessage],
        tools: &[Tool],
    ) -> Result<LlmReply>;
}

// ---------------------------------------------------------------------------
// Bounds / result types
// ---------------------------------------------------------------------------

pub struct SubAgentBounds {
    pub max_steps: usize,
    pub max_wall: Duration,
    pub max_tokens: u64,
}

impl Default for SubAgentBounds {
    fn default() -> Self {
        Self {
            max_steps: 30,
            max_wall: Duration::from_secs(300),
            max_tokens: 200_000,
        }
    }
}

pub struct SubAgentResult {
    pub final_text: String,
    pub events: Vec<SubAgentEvent>,
    pub reason: DoneReason,
    pub steps_used: usize,
}

// ---------------------------------------------------------------------------
// ToolDispatch trait
// ---------------------------------------------------------------------------

/// Trait the caller implements: given a tool name + JSON args, run it and
/// return a string result.
#[async_trait]
pub trait ToolDispatch: Send + Sync {
    async fn call(&self, name: &str, args: serde_json::Value) -> Result<String>;
}

// ---------------------------------------------------------------------------
// SubAgent
// ---------------------------------------------------------------------------

pub struct SubAgent {
    pub completer: Box<dyn Completer>,
    pub tools: Vec<Tool>,
    pub system_prompt: String,
    pub bounds: SubAgentBounds,
}

impl SubAgent {
    #[allow(clippy::too_many_lines)]
    pub async fn run(
        &self,
        user_message: &str,
        dispatch: &dyn ToolDispatch,
        cancel: Option<&tokio::sync::Notify>,
        event_sink: Option<&tokio::sync::mpsc::UnboundedSender<SubAgentEvent>>,
    ) -> Result<SubAgentResult> {
        let mut events: Vec<SubAgentEvent> = Vec::new();
        let mut messages: Vec<LlmMessage> = vec![LlmMessage::User(user_message.to_string())];
        let started = Instant::now();
        let mut steps = 0usize;

        loop {
            // --- Bound checks before calling the LLM ---
            if steps >= self.bounds.max_steps {
                return Ok(make_result(
                    events,
                    DoneReason::StepBudgetReached,
                    "step budget reached",
                    steps,
                ));
            }
            if started.elapsed() > self.bounds.max_wall {
                return Ok(make_result(
                    events,
                    DoneReason::TimeBudgetReached,
                    "time budget reached",
                    steps,
                ));
            }
            if let Some(c) = cancel {
                // Non-blocking poll: fires if notify_one() was called.
                // We use a manual flag via try_recv-style polling.
                if cancel_was_signalled(c) {
                    return Ok(make_result(
                        events,
                        DoneReason::Cancelled,
                        "cancelled",
                        steps,
                    ));
                }
            }

            // --- LLM call ---
            let reply = self
                .completer
                .complete(&self.system_prompt, &messages, &self.tools)
                .await?;

            let step_ev = SubAgentEvent::Step {
                index: steps,
                assistant_text: reply.text.clone(),
            };
            if let Some(tx) = event_sink {
                let _ = tx.send(step_ev.clone());
            }
            events.push(step_ev);

            if reply.tool_calls.is_empty() {
                return Ok(make_result(
                    events,
                    DoneReason::NoMoreToolCalls,
                    &reply.text,
                    steps,
                ));
            }

            // Check for the complete() sentinel
            if reply.tool_calls.iter().any(|t| t.name == "complete") {
                return Ok(make_result(
                    events,
                    DoneReason::CompleteSentinel,
                    &reply.text,
                    steps,
                ));
            }

            // Store the assistant turn in the conversation
            messages.push(LlmMessage::Assistant(reply.clone()));

            // Dispatch each tool call and collect results.
            //
            // All results are then pushed as a SINGLE `LlmMessage::ToolResults`
            // containing every result block.  Bedrock (and the Anthropic spec)
            // require that when an assistant turn has N `tool_use` blocks, ALL N
            // `tool_result` blocks appear in one subsequent user message; emitting
            // separate messages causes a ValidationException.
            let mut result_parts: Vec<ToolResultPart> = Vec::new();
            for call in &reply.tool_calls {
                let call_ev = SubAgentEvent::ToolCall {
                    name: call.name.clone(),
                    args: call.args.clone(),
                };
                if let Some(tx) = event_sink {
                    let _ = tx.send(call_ev.clone());
                }
                events.push(call_ev);

                let result_content = match dispatch.call(&call.name, call.args.clone()).await {
                    Ok(s) => {
                        let result_ev = SubAgentEvent::ToolResult {
                            name: call.name.clone(),
                            ok: true,
                            summary: s.chars().take(120).collect(),
                        };
                        if let Some(tx) = event_sink {
                            let _ = tx.send(result_ev.clone());
                        }
                        events.push(result_ev);
                        s
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        let result_ev = SubAgentEvent::ToolResult {
                            name: call.name.clone(),
                            ok: false,
                            summary: msg.clone(),
                        };
                        if let Some(tx) = event_sink {
                            let _ = tx.send(result_ev.clone());
                        }
                        events.push(result_ev);
                        format!("error: {msg}")
                    }
                };
                result_parts.push(ToolResultPart {
                    request_id: call.id.clone(),
                    name: call.name.clone(),
                    content: result_content,
                });
            }
            // Bundle all results into one message so Bedrock sees a single user
            // turn paired against the assistant turn above.
            if !result_parts.is_empty() {
                messages.push(LlmMessage::ToolResults(result_parts));
            }
            steps += 1;
        }
    }
}

/// Non-blocking cancellation check.
///
/// We can't `.await` the `Notify::notified()` future without blocking; instead
/// we poll it using `futures::poll!` which returns `Poll::Ready` iff the
/// notify was already signalled.
fn cancel_was_signalled(notify: &tokio::sync::Notify) -> bool {
    use futures::FutureExt; // for `.now_or_never()`
    notify.notified().now_or_never().is_some()
}

fn make_result(
    events: Vec<SubAgentEvent>,
    reason: DoneReason,
    text: &str,
    steps: usize,
) -> SubAgentResult {
    SubAgentResult {
        final_text: text.to_string(),
        events,
        reason,
        steps_used: steps,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::{Mutex, Notify};

    /// A MockCompleter that pops canned `LlmReply` values from a queue.
    struct MockCompleter {
        replies: Mutex<Vec<LlmReply>>,
    }

    impl MockCompleter {
        fn new(replies: Vec<LlmReply>) -> Self {
            Self {
                replies: Mutex::new(replies),
            }
        }
    }

    #[async_trait]
    impl Completer for MockCompleter {
        async fn complete(
            &self,
            _system: &str,
            _messages: &[LlmMessage],
            _tools: &[Tool],
        ) -> Result<LlmReply> {
            let mut q = self.replies.lock().await;
            if q.is_empty() {
                panic!("MockCompleter ran out of canned replies");
            }
            Ok(q.remove(0))
        }
    }

    /// A dispatcher that always succeeds.
    struct EchoDispatch;

    #[async_trait]
    impl ToolDispatch for EchoDispatch {
        async fn call(&self, _name: &str, _args: serde_json::Value) -> Result<String> {
            Ok("ok".to_string())
        }
    }

    fn tool_call_reply(tool_name: &str) -> LlmReply {
        LlmReply {
            text: String::new(),
            tool_calls: vec![LlmToolCall {
                id: "req-1".into(),
                name: tool_name.to_string(),
                args: serde_json::Value::Object(Default::default()),
            }],
        }
    }

    fn two_tool_call_reply() -> LlmReply {
        LlmReply {
            text: String::new(),
            tool_calls: vec![
                LlmToolCall {
                    id: "tc-1".into(),
                    name: "kb_search".to_string(),
                    args: serde_json::Value::Object(Default::default()),
                },
                LlmToolCall {
                    id: "tc-2".into(),
                    name: "kb_read_page".to_string(),
                    args: serde_json::Value::Object(Default::default()),
                },
            ],
        }
    }

    fn text_reply(text: &str) -> LlmReply {
        LlmReply {
            text: text.to_string(),
            tool_calls: vec![],
        }
    }

    fn make_agent(completer: MockCompleter, max_steps: usize) -> SubAgent {
        SubAgent {
            completer: Box::new(completer),
            tools: vec![],
            system_prompt: "sys".into(),
            bounds: SubAgentBounds {
                max_steps,
                ..Default::default()
            },
        }
    }

    /// A MockCompleter that also records the `messages` slice it receives each call.
    ///
    /// The `received` field is an `Arc<Mutex<…>>` so the test can clone a reference
    /// before moving the completer into `Box<dyn Completer>` and inspect recordings
    /// after the run completes.
    struct RecordingCompleter {
        replies: Mutex<Vec<LlmReply>>,
        received: Arc<Mutex<Vec<Vec<LlmMessage>>>>,
    }

    #[async_trait]
    impl Completer for RecordingCompleter {
        async fn complete(
            &self,
            _system: &str,
            messages: &[LlmMessage],
            _tools: &[Tool],
        ) -> Result<LlmReply> {
            self.received.lock().await.push(messages.to_vec());
            let mut q = self.replies.lock().await;
            if q.is_empty() {
                panic!("RecordingCompleter ran out of canned replies");
            }
            Ok(q.remove(0))
        }
    }

    /// Test 1: one tool call then a text-only reply → Done(NoMoreToolCalls) in 1 iteration.
    #[tokio::test]
    async fn two_step_happy_path() {
        let completer = MockCompleter::new(vec![
            tool_call_reply("kb_search"), // step 0: issues a tool call
            text_reply("all done"),       // step 1: no tool calls → done
        ]);
        let agent = make_agent(completer, 10);
        let result = agent.run("hello", &EchoDispatch, None, None).await.unwrap();
        assert_eq!(result.reason, DoneReason::NoMoreToolCalls);
        // steps_used reflects the loop counter at the time of Done, which
        // is 1 (incremented after the first tool-dispatch round)
        assert_eq!(result.steps_used, 1);
        assert_eq!(result.final_text, "all done");
    }

    /// Test 2: provider always returns a tool call → hits max_steps.
    #[tokio::test]
    async fn step_budget_exceeded() {
        // 35 replies, but max_steps = 5 so only 5 are consumed.
        let replies: Vec<LlmReply> = (0..35).map(|_| tool_call_reply("kb_search")).collect();
        let completer = MockCompleter::new(replies);
        let agent = make_agent(completer, 5);
        let result = agent.run("hello", &EchoDispatch, None, None).await.unwrap();
        assert_eq!(result.reason, DoneReason::StepBudgetReached);
        assert!(result.steps_used <= 5);
    }

    /// Test 3: cancellation via Notify before the loop starts → returns Cancelled.
    #[tokio::test]
    async fn cancellation_via_notify() {
        let notify = Arc::new(Notify::new());
        // Signal before run() is called so the first iteration detects it.
        notify.notify_one();

        let completer = MockCompleter::new(vec![text_reply("never reached")]);
        let agent = make_agent(completer, 30);
        let result = agent
            .run("hello", &EchoDispatch, Some(&notify), None)
            .await
            .unwrap();
        assert_eq!(result.reason, DoneReason::Cancelled);
    }

    /// Test: when the assistant returns 2 tool calls in one turn, the loop must
    /// push exactly ONE `LlmMessage::ToolResults` (not two separate `ToolResult`
    /// entries) so that Bedrock sees a single user message with both results.
    #[tokio::test]
    async fn loop_bundles_multiple_tool_calls_into_one_tool_results_message() {
        // Shared recording store so we can inspect what the completer saw after run().
        let received: Arc<Mutex<Vec<Vec<LlmMessage>>>> = Arc::new(Mutex::new(Vec::new()));
        let received_clone = received.clone();

        let recording = RecordingCompleter {
            replies: Mutex::new(vec![
                two_tool_call_reply(), // step 0: assistant issues 2 tool calls
                text_reply("done"),    // step 1: no more tool calls → loop exits
            ]),
            received: received_clone,
        };

        let agent = SubAgent {
            completer: Box::new(recording),
            tools: vec![],
            system_prompt: "sys".into(),
            bounds: SubAgentBounds {
                max_steps: 10,
                ..Default::default()
            },
        };
        let result = agent.run("go", &EchoDispatch, None, None).await.unwrap();
        assert_eq!(result.reason, DoneReason::NoMoreToolCalls);

        // calls[0] = first complete() call = [User]  (initial)
        // calls[1] = second complete() call = [User, Assistant(2 tool calls), ToolResults(2 parts)]
        let calls = received.lock().await;
        assert!(
            calls.len() >= 2,
            "expected at least 2 completer calls, got {}",
            calls.len()
        );
        let msgs_after_first_turn = &calls[1];

        // The message right after the Assistant turn must be a single ToolResults,
        // not two separate ToolResult entries.
        let tool_result_msgs: Vec<&LlmMessage> = msgs_after_first_turn
            .iter()
            .filter(|m| {
                matches!(
                    m,
                    LlmMessage::ToolResults(_) | LlmMessage::ToolResult { .. }
                )
            })
            .collect();

        assert_eq!(
            tool_result_msgs.len(),
            1,
            "all tool results from one turn must collapse into exactly ONE message; got {}",
            tool_result_msgs.len()
        );

        // And that one message must be the ToolResults compound variant.
        assert!(
            matches!(tool_result_msgs[0], LlmMessage::ToolResults(parts) if parts.len() == 2),
            "the single tool-result message must be LlmMessage::ToolResults with 2 parts"
        );
    }

    /// Test 4: with an event_sink, events arrive in the channel as the loop runs.
    #[tokio::test]
    async fn run_emits_events_to_sink_live() {
        use tokio::sync::mpsc;

        let (tx, mut rx) = mpsc::unbounded_channel::<SubAgentEvent>();

        // Two-step run: one tool call, then a text-only reply → NoMoreToolCalls.
        let completer = MockCompleter::new(vec![tool_call_reply("kb_search"), text_reply("done")]);
        let agent = make_agent(completer, 10);

        let _ = agent
            .run("test", &EchoDispatch, None, Some(&tx))
            .await
            .unwrap();

        // Close the sender so rx drains to exhaustion.
        drop(tx);

        let mut count = 0usize;
        while rx.recv().await.is_some() {
            count += 1;
        }
        assert!(
            count > 0,
            "event sink must have received at least one event"
        );
    }
}
