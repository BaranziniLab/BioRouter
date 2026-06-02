use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SubAgentEvent {
    Step {
        index: usize,
        assistant_text: String,
    },
    ToolCall {
        name: String,
        args: serde_json::Value,
    },
    ToolResult {
        name: String,
        ok: bool,
        summary: String,
    },
    Done {
        reason: DoneReason,
        final_text: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DoneReason {
    CompleteSentinel,
    NoMoreToolCalls,
    StepBudgetReached,
    TimeBudgetReached,
    Cancelled,
    Error,
}
