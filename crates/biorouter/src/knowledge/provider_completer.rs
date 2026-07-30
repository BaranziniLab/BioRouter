//! Bridges `biorouter::providers::Provider` → `biorouter_mcp::knowledge::subagent::loop_::Completer`.
//!
//! The `Completer` trait was introduced in Plan 2 to avoid a circular dep on `biorouter`
//! from within `biorouter-mcp`. This adapter lives in `biorouter` (which already depends on
//! `biorouter-mcp`) and lets HTTP handlers in `biorouter-server` pass a user-selected
//! `Provider` into the Plan-2 macros (ingest / query / lint / agentic credibility).

use crate::conversation::message::{Message, MessageContent};
use crate::providers::base::Provider;
use anyhow::Result;
use async_trait::async_trait;
use biorouter_mcp::knowledge::subagent::loop_::{
    Completer, LlmMessage, LlmReply, LlmToolCall, ToolResultPart,
};
use rmcp::model::{CallToolRequestParams, CallToolResult, Content, Tool};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

pub struct ProviderCompleter {
    pub provider: Arc<dyn Provider>,
}

impl ProviderCompleter {
    pub fn new(provider: Arc<dyn Provider>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl Completer for ProviderCompleter {
    async fn complete(
        &self,
        system_prompt: &str,
        messages: &[LlmMessage],
        tools: &[Tool],
    ) -> Result<LlmReply> {
        // 1. Translate LlmMessage list to biorouter::conversation::message::Message list.
        let provider_messages: Vec<Message> =
            messages.iter().map(llm_to_provider_message).collect();

        // 2. The Tool type in loop_ is already rmcp::model::Tool — clone through unchanged.
        //    (The Completer trait re-uses rmcp::model::Tool directly.)
        let provider_tools: Vec<Tool> = tools.to_vec();

        // 3. Call the real provider.
        let (reply_msg, _usage) = self
            .provider
            .complete(system_prompt, &provider_messages, &provider_tools)
            .await
            .map_err(|e| anyhow::anyhow!("provider.complete failed: {e}"))?;

        // 4. Extract assistant text and tool calls from the returned Message.
        let text = reply_msg.as_concat_text();
        let tool_calls = extract_tool_calls(&reply_msg)?;

        Ok(LlmReply { text, tool_calls })
    }
}

// ---------------------------------------------------------------------------
// Translation helpers
// ---------------------------------------------------------------------------

/// Convert one `LlmMessage` into the `biorouter::conversation::message::Message`
/// shape that `Provider::complete` expects.
///
/// Mapping:
/// - `LlmMessage::User(text)`  → `Message::user()` with a single text content item.
/// - `LlmMessage::Assistant(reply)` → `Message::assistant()` with one Text block for the
///   assistant text (if non-empty) plus one `ToolRequest` block per tool call.
/// - `LlmMessage::ToolResult { … }` → `Message::user()` with a single `ToolResponse` block.
///   Providers expect tool results on the *user* role (this matches the Anthropic / OpenAI
///   convention that biorouter's Anthropic/OpenAI provider adapters implement).
fn llm_to_provider_message(m: &LlmMessage) -> Message {
    match m {
        LlmMessage::User(text) => Message::user().with_text(text.clone()),

        LlmMessage::Assistant(reply) => {
            let mut msg = Message::assistant();
            if !reply.text.is_empty() {
                msg = msg.with_text(reply.text.clone());
            }
            for call in &reply.tool_calls {
                // Build a CallToolRequestParams from the LlmToolCall.
                let args_obj = match &call.args {
                    serde_json::Value::Object(obj) => Some(obj.clone()),
                    _ => None,
                };
                let params = CallToolRequestParams {
                    name: call.name.clone().into(),
                    arguments: args_obj,
                    task: None,
                    meta: None,
                };
                msg = msg.with_tool_request(call.id.clone(), Ok(params));
            }
            msg
        }

        LlmMessage::ToolResult {
            request_id,
            name: _,
            content,
        } => {
            // Wrap the result string as a text Content inside a CallToolResult.
            let call_result = CallToolResult::success(vec![Content::text(content.clone())]);
            Message::user().with_content(MessageContent::tool_response(
                request_id.clone(),
                Ok(call_result),
            ))
        }

        LlmMessage::ToolResults(parts) => {
            // Bundle ALL tool-result blocks into ONE user-role message.
            //
            // Bedrock (and the Anthropic spec) require that when an assistant turn
            // emits N `tool_use` blocks, ALL N `tool_result` blocks appear in a
            // single subsequent user message.  Emitting one message per result
            // causes a ValidationException ("Expected toolResult blocks at
            // messages.N.content for the following tool_use_id").
            let mut msg = Message::user();
            for ToolResultPart {
                request_id,
                name: _,
                content,
            } in parts
            {
                let call_result = CallToolResult::success(vec![Content::text(content.clone())]);
                msg = msg.with_tool_response(request_id.clone(), Ok(call_result));
            }
            msg
        }
    }
}

/// Walk a provider-returned `Message` and collect any `ToolRequest` content
/// blocks into `LlmToolCall` values.
///
/// A `ToolRequest` whose `tool_call` is `Err` is a call the model asked for and
/// the provider adapter could not decode — Google mints one for every
/// `functionCall` whose name breaks its character rule, and the streaming
/// decoders do the same for unparseable arguments. Skipping those quietly
/// emptied `tool_calls`, and an empty `tool_calls` is precisely the sub-agent
/// loop's signal that the agent has finished: the digest stopped early and was
/// reported as a success (issue #71). Refusing the whole completion is the only
/// answer that stays honest — the run failed, and the user is told why.
fn extract_tool_calls(msg: &Message) -> Result<Vec<LlmToolCall>> {
    let mut out = Vec::new();
    for c in &msg.content {
        let MessageContent::ToolRequest(req) = c else {
            continue;
        };
        match &req.tool_call {
            Ok(params) => {
                let args = params
                    .arguments
                    .as_ref()
                    .map(|obj| serde_json::Value::Object(obj.clone()))
                    .unwrap_or(serde_json::Value::Null);
                out.push(LlmToolCall {
                    id: req.id.clone(),
                    name: params.name.to_string(),
                    args,
                });
            }
            Err(e) => {
                anyhow::bail!(
                    "the model requested a tool call Biorouter could not decode: {}",
                    e.message
                );
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ModelConfig;
    use crate::providers::base::{ProviderMetadata, ProviderUsage, Usage};
    use crate::providers::errors::ProviderError;
    use rmcp::model::{ErrorCode, ErrorData, Tool};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    /// A minimal mock Provider that returns a single canned Message.
    struct MockProvider {
        response: Message,
    }

    #[async_trait]
    impl Provider for MockProvider {
        fn metadata() -> ProviderMetadata
        where
            Self: Sized,
        {
            unimplemented!("MockProvider::metadata not needed in this test")
        }

        fn get_name(&self) -> &str {
            "mock"
        }

        async fn complete_with_model(
            &self,
            _model_config: &ModelConfig,
            _system: &str,
            _messages: &[Message],
            _tools: &[Tool],
        ) -> Result<(Message, ProviderUsage), ProviderError> {
            let usage = ProviderUsage::new("mock".into(), Usage::new(None, None, None));
            Ok((self.response.clone(), usage))
        }

        fn get_model_config(&self) -> ModelConfig {
            ModelConfig::new_or_fail("claude-3-5-sonnet-20241022")
        }
    }

    /// A mock Provider that records every `messages` slice it receives so tests
    /// can inspect what was actually sent to the provider.
    struct RecordingMockProvider {
        response: Message,
        received: Mutex<Vec<Vec<Message>>>,
    }

    impl RecordingMockProvider {
        fn new(response: Message) -> Self {
            Self {
                response,
                received: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl Provider for RecordingMockProvider {
        fn metadata() -> ProviderMetadata
        where
            Self: Sized,
        {
            unimplemented!()
        }

        fn get_name(&self) -> &str {
            "recording-mock"
        }

        async fn complete_with_model(
            &self,
            _model_config: &ModelConfig,
            _system: &str,
            messages: &[Message],
            _tools: &[Tool],
        ) -> Result<(Message, ProviderUsage), ProviderError> {
            self.received.lock().await.push(messages.to_vec());
            let usage = ProviderUsage::new("recording-mock".into(), Usage::new(None, None, None));
            Ok((self.response.clone(), usage))
        }

        fn get_model_config(&self) -> ModelConfig {
            ModelConfig::new_or_fail("claude-3-5-sonnet-20241022")
        }
    }

    #[tokio::test]
    async fn roundtrips_text_and_tool_calls() {
        // Build a provider response: assistant text "ok" + one tool call kb_search(query="x")
        let params = CallToolRequestParams {
            name: "kb_search".into(),
            arguments: Some({
                let mut m = serde_json::Map::new();
                m.insert(
                    "query".to_string(),
                    serde_json::Value::String("x".to_string()),
                );
                m
            }),
            task: None,
            meta: None,
        };
        let response = Message::assistant()
            .with_text("ok")
            .with_tool_request("req-1", Ok(params));

        let provider = Arc::new(MockProvider { response });
        let completer = ProviderCompleter::new(provider);

        let reply = completer
            .complete("sys", &[LlmMessage::User("hi".into())], &[])
            .await
            .unwrap();

        assert_eq!(reply.text, "ok");
        assert_eq!(reply.tool_calls.len(), 1);
        assert_eq!(reply.tool_calls[0].name, "kb_search");
        assert_eq!(reply.tool_calls[0].args["query"], "x");
    }

    /// Issue #71. A provider can hand back a `ToolRequest` it could not decode —
    /// Google emits one for every `functionCall` whose name fails its character
    /// rule, and the streaming decoders do the same for unparseable arguments.
    /// Dropping it left `tool_calls` empty, which the sub-agent loop reads as
    /// "the agent is finished": the digest ended early and reported success. A
    /// tool call the model asked for and Biorouter could not run is a failed
    /// completion, and must be reported as one.
    #[tokio::test]
    async fn an_undecodable_tool_request_fails_the_completion() {
        let broken = ErrorData {
            code: ErrorCode::INVALID_REQUEST,
            message: std::borrow::Cow::from(
                "The provided function name 'kb.write page' had invalid characters",
            ),
            data: None,
        };
        let response = Message::assistant()
            .with_text("writing the page now")
            .with_tool_request("req-1", Err(broken));

        let provider = Arc::new(MockProvider { response });
        let completer = ProviderCompleter::new(provider);

        let err = completer
            .complete("sys", &[LlmMessage::User("hi".into())], &[])
            .await
            .expect_err("an undecodable tool request must not look like a finished turn")
            .to_string();

        assert!(
            err.contains("invalid characters"),
            "the provider's own explanation must reach the user, got: {err}"
        );
    }

    /// The mirror image: a well-formed tool request alongside a broken one must
    /// still fail rather than silently running only half of what the model asked
    /// for.
    #[tokio::test]
    async fn one_broken_tool_request_fails_even_beside_a_good_one() {
        let good = CallToolRequestParams {
            name: "kb_search".into(),
            arguments: None,
            task: None,
            meta: None,
        };
        let broken = ErrorData {
            code: ErrorCode::INVALID_REQUEST,
            message: std::borrow::Cow::from("arguments were not valid JSON"),
            data: None,
        };
        let response = Message::assistant()
            .with_tool_request("ok-1", Ok(good))
            .with_tool_request("bad-1", Err(broken));

        let provider = Arc::new(MockProvider { response });
        let completer = ProviderCompleter::new(provider);

        let err = completer
            .complete("sys", &[LlmMessage::User("hi".into())], &[])
            .await
            .expect_err("a partially undecodable turn must not be executed as if intact")
            .to_string();

        assert!(
            err.contains("not valid JSON"),
            "the provider's own explanation must reach the user, got: {err}"
        );
    }

    #[tokio::test]
    async fn tool_result_message_is_translated() {
        // Feed a ToolResult LlmMessage; the provider receives it as a user Message
        // with a ToolResponse content block. The provider echoes back a text reply.
        let response = Message::assistant().with_text("done");
        let provider = Arc::new(MockProvider { response });
        let completer = ProviderCompleter::new(provider);

        let msgs = vec![
            LlmMessage::User("start".into()),
            LlmMessage::Assistant(LlmReply {
                text: String::new(),
                tool_calls: vec![LlmToolCall {
                    id: "r1".into(),
                    name: "kb_search".into(),
                    args: serde_json::json!({}),
                }],
            }),
            LlmMessage::ToolResult {
                request_id: "r1".into(),
                name: "kb_search".into(),
                content: "search results here".into(),
            },
        ];

        let reply = completer.complete("sys", &msgs, &[]).await.unwrap();
        assert_eq!(reply.text, "done");
        assert!(reply.tool_calls.is_empty());
    }

    /// Verify that the `tool_use_id` from the assistant's `tool_use` block is
    /// preserved on the `tool_result` user message that the provider receives.
    ///
    /// Bedrock (and Anthropic) require strict pairing: every `tool_use` in an
    /// assistant message must be matched by a `tool_result` with the same id in
    /// the immediately following user message.  If the id is lost or replaced the
    /// provider will reject the request with a `ValidationException`.
    #[tokio::test]
    async fn tool_result_round_trip_preserves_tool_use_id() {
        let tool_use_id = "abc123";

        // Recording mock: captures the messages it receives, replies with plain text.
        let response = Message::assistant().with_text("finished");
        let recording = Arc::new(RecordingMockProvider::new(response));
        let completer = ProviderCompleter::new(recording.clone());

        // Build a three-message conversation:
        //   [0] user prompt
        //   [1] assistant reply that issued one tool call with id=tool_use_id
        //   [2] tool result that references the same id
        let msgs = vec![
            LlmMessage::User("hello".into()),
            LlmMessage::Assistant(LlmReply {
                text: String::new(),
                tool_calls: vec![LlmToolCall {
                    id: tool_use_id.into(),
                    name: "kb_read_page".into(),
                    args: serde_json::json!({ "path": "raw/src/source.md" }),
                }],
            }),
            LlmMessage::ToolResult {
                request_id: tool_use_id.into(),
                name: "kb_read_page".into(),
                content: "page content here".into(),
            },
        ];

        let _ = completer.complete("sys", &msgs, &[]).await.unwrap();

        // Inspect what the provider saw.
        let calls = recording.received.lock().await;
        assert_eq!(
            calls.len(),
            1,
            "provider should have been called exactly once"
        );
        let provider_msgs = &calls[0];

        // message[1] must be an assistant message containing a ToolRequest with the
        // expected id.
        let assistant_msg = &provider_msgs[1];
        let tool_req_id = assistant_msg
            .content
            .iter()
            .find_map(|c| {
                if let MessageContent::ToolRequest(req) = c {
                    Some(req.id.clone())
                } else {
                    None
                }
            })
            .expect("assistant message must contain a ToolRequest block");
        assert_eq!(
            tool_req_id, tool_use_id,
            "ToolRequest id must match the original tool_use_id"
        );

        // message[2] must be a user message containing a ToolResponse whose id
        // matches the tool_use_id — that's what Bedrock checks.
        let tool_result_msg = &provider_msgs[2];
        let tool_resp_id = tool_result_msg
            .content
            .iter()
            .find_map(|c| {
                if let MessageContent::ToolResponse(resp) = c {
                    Some(resp.id.clone())
                } else {
                    None
                }
            })
            .expect("tool-result user message must contain a ToolResponse block");
        assert_eq!(
            tool_resp_id, tool_use_id,
            "ToolResponse id must equal the tool_use_id so Bedrock can pair them"
        );
    }

    /// Bedrock requires that N tool_use blocks from one assistant turn are answered
    /// by exactly ONE user message containing N tool_result blocks.  Verify that
    /// `LlmMessage::ToolResults` (the compound variant) is collapsed into a single
    /// user-role `Message` carrying all ToolResponse content blocks.
    #[tokio::test]
    async fn multiple_tool_results_collapse_into_single_user_message() {
        let response = Message::assistant().with_text("all done");
        let recording = Arc::new(RecordingMockProvider::new(response));
        let completer = ProviderCompleter::new(recording.clone());

        // Build a conversation: user → assistant (2 tool calls) → ToolResults (2 parts).
        let msgs = vec![
            LlmMessage::User("hello".into()),
            LlmMessage::Assistant(LlmReply {
                text: String::new(),
                tool_calls: vec![
                    LlmToolCall {
                        id: "tc-1".into(),
                        name: "kb_search".into(),
                        args: serde_json::json!({}),
                    },
                    LlmToolCall {
                        id: "tc-2".into(),
                        name: "kb_read_page".into(),
                        args: serde_json::json!({}),
                    },
                ],
            }),
            LlmMessage::ToolResults(vec![
                ToolResultPart {
                    request_id: "tc-1".into(),
                    name: "kb_search".into(),
                    content: "result one".into(),
                },
                ToolResultPart {
                    request_id: "tc-2".into(),
                    name: "kb_read_page".into(),
                    content: "result two".into(),
                },
            ]),
        ];

        let _ = completer.complete("sys", &msgs, &[]).await.unwrap();

        let calls = recording.received.lock().await;
        assert_eq!(calls.len(), 1);
        let provider_msgs = &calls[0];

        // The provider must see exactly 3 messages (user, assistant, tool-results).
        assert_eq!(
            provider_msgs.len(),
            3,
            "expected 3 provider messages, got {}",
            provider_msgs.len()
        );

        // The third message must be user-role and contain exactly 2 ToolResponse blocks.
        let tool_result_msg = &provider_msgs[2];
        let tool_resp_ids: Vec<String> = tool_result_msg
            .content
            .iter()
            .filter_map(|c| {
                if let MessageContent::ToolResponse(resp) = c {
                    Some(resp.id.clone())
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(
            tool_resp_ids.len(),
            2,
            "both tool results must be in a single user message; got {} ToolResponse block(s)",
            tool_resp_ids.len()
        );
        assert!(
            tool_resp_ids.contains(&"tc-1".to_string()),
            "ToolResponse for tc-1 missing"
        );
        assert!(
            tool_resp_ids.contains(&"tc-2".to_string()),
            "ToolResponse for tc-2 missing"
        );
    }
}
