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

        let mut last_call: Option<InternalToolCall> = None;
        let mut repeat_count = 0u32;

        for call in messages
            .iter()
            .flat_map(|message| message.content.iter())
            .filter_map(|content| content.as_tool_request())
            .filter_map(InternalToolCall::from_request)
        {
            if last_call.as_ref().is_some_and(|last| last.matches(&call)) {
                repeat_count += 1;
            } else {
                repeat_count = 1;
                last_call = Some(call);
            }
        }

        for tool_request in tool_requests {
            if let Some(call) = InternalToolCall::from_request(tool_request) {
                if last_call.as_ref().is_some_and(|last| last.matches(&call)) {
                    repeat_count += 1;
                } else {
                    repeat_count = 1;
                    last_call = Some(call);
                }

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
