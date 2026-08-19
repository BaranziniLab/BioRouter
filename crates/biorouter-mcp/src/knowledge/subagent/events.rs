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
    /// The conversation grew past `SubAgentBounds::max_tokens`.
    ///
    /// Its own reason rather than folded into `StepBudgetReached`, because the
    /// two say different things to whoever reads the run: a step budget means
    /// the sub-agent was still working and ran out of turns, a token budget
    /// means its context got too big — which is usually one enormous page or a
    /// tool result that should have been summarised, and is fixed differently.
    TokenBudgetReached,
    Cancelled,
    Error,
}
