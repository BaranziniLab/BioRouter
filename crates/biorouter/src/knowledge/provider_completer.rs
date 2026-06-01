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
use biorouter_mcp::knowledge::subagent::loop_::{Completer, LlmMessage, LlmReply, LlmToolCall};
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
        let tool_calls = extract_tool_calls(&reply_msg);

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
            Message::user()
                .with_content(MessageContent::tool_response(request_id.clone(), Ok(call_result)))
        }
    }
}

/// Walk a provider-returned `Message` and collect any `ToolRequest` content
/// blocks into `LlmToolCall` values.
fn extract_tool_calls(msg: &Message) -> Vec<LlmToolCall> {
    msg.content
        .iter()
        .filter_map(|c| {
            if let MessageContent::ToolRequest(req) = c {
                if let Ok(params) = &req.tool_call {
                    let args = params
                        .arguments
                        .as_ref()
                        .map(|obj| serde_json::Value::Object(obj.clone()))
                        .unwrap_or(serde_json::Value::Null);
                    Some(LlmToolCall {
                        id: req.id.clone(),
                        name: params.name.to_string(),
                        args,
                    })
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect()
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
    use rmcp::model::Tool;
    use std::sync::Arc;

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
}
