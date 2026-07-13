use crate::config::BioRouterMode;
use crate::conversation::message::{Message, ToolRequest};
use crate::tool_inspection::{InspectionAction, InspectionResult, ToolInspector};
use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::CallToolRequestParams;
use serde_json::Value;

/// Inspector name used by the repetition guard; the agent keys the honest
/// "repetition, not user decline" deny message off this.
pub const REPETITION_INSPECTOR_NAME: &str = "repetition";

/// Finding id for the hard stop (the call is denied).
pub const REPETITION_HARD_FINDING_ID: &str = "REP-001";

/// Finding id for the non-blocking soft warning (the call still runs, but the
/// model is told it is repeating itself) — BR-29.
pub const REPETITION_SOFT_FINDING_ID: &str = "REP-002";

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

/// Staged repetition guard (BR-29).
///
/// Consecutive identical tool calls (same name, byte-identical arguments) are
/// counted across history + the current batch. Two thresholds, both expressed as
/// "the Nth identical call in a row":
///
/// * `soft_warn_at` — the call still runs, but a non-blocking warning is emitted
///   (`InspectionAction::Warn`) and injected into the model's context so it can
///   change approach before it is stopped.
/// * `hard_stop_at` — the call is denied (`InspectionAction::Deny`).
///
/// A single hard deny with no prior nudge was the old behavior; the soft stage
/// gives the model one chance to break the loop itself.
#[derive(Debug)]
pub struct RepetitionInspector {
    /// Nth identical call in a row that earns a non-blocking warning.
    /// `None` (or `>= hard_stop_at`) disables the soft stage.
    soft_warn_at: Option<u32>,
    /// Nth identical call in a row that is denied. `None` disables the guard.
    hard_stop_at: Option<u32>,
}

impl RepetitionInspector {
    /// Hard-stop-only guard: deny once a call has repeated *more than*
    /// `max_repetitions` times in a row. No soft stage.
    pub fn new(max_repetitions: Option<u32>) -> Self {
        Self {
            soft_warn_at: None,
            hard_stop_at: max_repetitions.map(|max| max.saturating_add(1)),
        }
    }

    /// Staged guard: warn on the `soft_warn_at`-th identical call, deny on the
    /// `hard_stop_at`-th. If `soft_warn_at >= hard_stop_at` the soft stage never
    /// fires and this degrades to a hard stop.
    pub fn staged(soft_warn_at: u32, hard_stop_at: u32) -> Self {
        Self {
            soft_warn_at: Some(soft_warn_at),
            hard_stop_at: Some(hard_stop_at),
        }
    }

    /// The message the model sees in place of the tool result when a call is
    /// hard-stopped. It states the *real* reason — the repetition guard fired —
    /// rather than claiming the user declined (which was the old, misleading
    /// `DECLINED_RESPONSE`).
    fn hard_stop_reason(tool_name: &str, repeat_count: u32) -> String {
        format!(
            "BioRouter stopped this tool call: '{tool_name}' has now been called \
             with identical arguments {repeat_count} times in a row. The user did \
             NOT decline it — this is an automatic repetition guard. Repeating the \
             same call will not produce a different result. Change approach: vary \
             the arguments, use a different tool, or explain what is blocking you \
             and stop."
        )
    }

    /// Non-blocking nudge injected into the model's context. The call still ran.
    fn soft_warn_reason(tool_name: &str, repeat_count: u32, hard_stop_at: u32) -> String {
        format!(
            "Repetition warning: you have called '{tool_name}' with identical \
             arguments {repeat_count} times in a row. It will be stopped \
             automatically on the {hard_stop_at}th consecutive identical call. \
             Change approach now: vary the arguments, use a different tool, or \
             explain what is blocking you and stop."
        )
    }
}

#[async_trait]
impl ToolInspector for RepetitionInspector {
    fn name(&self) -> &'static str {
        REPETITION_INSPECTOR_NAME
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
        let Some(hard_stop_at) = self.hard_stop_at else {
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

                let tool_name = tool_request
                    .tool_call
                    .as_ref()
                    .map(|tool_call| tool_call.name.to_string())
                    .unwrap_or_else(|_| "unknown".to_string());

                if repeat_count >= hard_stop_at {
                    results.push(InspectionResult {
                        tool_request_id: tool_request.id.clone(),
                        action: InspectionAction::Deny,
                        reason: Self::hard_stop_reason(&tool_name, repeat_count),
                        confidence: 1.0,
                        inspector_name: REPETITION_INSPECTOR_NAME.to_string(),
                        finding_id: Some(REPETITION_HARD_FINDING_ID.to_string()),
                    });
                } else if self
                    .soft_warn_at
                    .is_some_and(|soft_warn_at| repeat_count >= soft_warn_at)
                {
                    results.push(InspectionResult {
                        tool_request_id: tool_request.id.clone(),
                        action: InspectionAction::Warn,
                        reason: Self::soft_warn_reason(&tool_name, repeat_count, hard_stop_at),
                        confidence: 1.0,
                        inspector_name: REPETITION_INSPECTOR_NAME.to_string(),
                        finding_id: Some(REPETITION_SOFT_FINDING_ID.to_string()),
                    });
                }
            }
        }

        Ok(results)
    }
}
