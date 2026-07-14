use crate::config::BioRouterMode;
use crate::conversation::message::{Message, ToolRequest};
use crate::tool_inspection::{InspectionAction, InspectionResult, ToolInspector};
use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::CallToolRequestParams;
use serde_json::Value;
use std::collections::HashMap;

// Helper struct for internal tracking
#[derive(Debug, Clone)]
struct InternalToolCall {
    name: String,
    parameters: Value,
}

impl InternalToolCall {
    fn matches(&self, other: &InternalToolCall) -> bool {
        self.name == other.name && self.parameters == other.parameters
    }

    /// A stable key for "this exact call". Two calls with the same name and the
    /// same arguments have the same signature — regardless of what the model did
    /// in between.
    fn signature(&self) -> String {
        // A tool name can never contain a newline, so this cannot collide.
        format!("{}\n{}", self.name, self.parameters)
    }

    fn from_tool_call(tool_call: &CallToolRequestParams) -> Self {
        let name = tool_call.name.to_string();
        let parameters = tool_call
            .arguments
            .as_ref()
            .map(|obj| Value::Object(obj.clone()))
            .unwrap_or(Value::Null);
        Self { name, parameters }
    }

    fn from_request(tool_request: &ToolRequest) -> Option<Self> {
        tool_request
            .tool_call
            .as_ref()
            .ok()
            .map(Self::from_tool_call)
    }
}

#[derive(Debug)]
pub struct RepetitionInspector {
    max_repetitions: Option<u32>,
    last_call: Option<InternalToolCall>,
    repeat_count: u32,
    call_counts: HashMap<String, u32>,
}

impl RepetitionInspector {
    pub fn new(max_repetitions: Option<u32>) -> Self {
        Self {
            max_repetitions,
            last_call: None,
            repeat_count: 0,
            call_counts: HashMap::new(),
        }
    }

    pub fn check_tool_call(&mut self, tool_call: CallToolRequestParams) -> bool {
        let internal_call = InternalToolCall::from_tool_call(&tool_call);
        let total_calls = self
            .call_counts
            .entry(internal_call.name.clone())
            .or_insert(0);
        *total_calls += 1;

        if self.max_repetitions.is_none() {
            self.last_call = Some(internal_call);
            self.repeat_count = 1;
            return true;
        }

        if let Some(last) = &self.last_call {
            if last.matches(&internal_call) {
                self.repeat_count += 1;
                if self.repeat_count > self.max_repetitions.unwrap() {
                    return false;
                }
            } else {
                self.repeat_count = 1;
            }
        } else {
            self.repeat_count = 1;
        }

        self.last_call = Some(internal_call);
        true
    }

    pub fn reset(&mut self) {
        self.last_call = None;
        self.repeat_count = 0;
        self.call_counts.clear();
    }
}

#[async_trait]
impl ToolInspector for RepetitionInspector {
    fn name(&self) -> &'static str {
        "repetition"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    async fn inspect(
        &self,
        tool_requests: &[ToolRequest],
        messages: &[Message],
        _biorouter_mode: BioRouterMode,
        _session: &crate::session::Session,
    ) -> Result<Vec<InspectionResult>> {
        let mut results = Vec::new();
        let Some(max_repetitions) = self.max_repetitions else {
            return Ok(results);
        };

        // Count each (name, args) SIGNATURE across the whole conversation, not just
        // consecutive repeats.
        //
        // This used to track only the immediately preceding call, so `A, B, A, B,
        // A, B` reset the counter every time and never tripped at all — which is
        // how a runaway `ui_describe` / `ui_render` alternation ran to the turn cap
        // while the guard sat there believing nothing was wrong. A loop is a loop
        // whether or not the model interleaves something else between iterations.
        let mut seen: HashMap<String, u32> = HashMap::new();

        for call in messages
            .iter()
            .flat_map(|message| message.content.iter())
            .filter_map(|content| content.as_tool_request())
            .filter_map(InternalToolCall::from_request)
        {
            *seen.entry(call.signature()).or_insert(0) += 1;
        }

        for tool_request in tool_requests {
            if let Some(call) = InternalToolCall::from_request(tool_request) {
                let count = seen.entry(call.signature()).or_insert(0);
                *count += 1;
                let repeat_count = *count;

                if repeat_count > max_repetitions {
                    let tool_name = tool_request
                        .tool_call
                        .as_ref()
                        .map(|tool_call| tool_call.name.to_string())
                        .unwrap_or_else(|_| "unknown".to_string());
                    results.push(InspectionResult {
                        tool_request_id: tool_request.id.clone(),
                        action: InspectionAction::Deny,
                        reason: format!("Tool '{}' has exceeded maximum repetitions", tool_name),
                        confidence: 1.0,
                        inspector_name: "repetition".to_string(),
                        finding_id: Some("REP-001".to_string()),
                    });
                }
            }
        }

        Ok(results)
    }
}
