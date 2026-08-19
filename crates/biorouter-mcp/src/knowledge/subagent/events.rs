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
    /// A budget ran out while the sub-agent was still retrying a value that is
    /// not in a closed vocabulary (DR-16).
    ///
    /// Its own reason, and it is the one that pays for itself. A rejected tool
    /// call is fed back as `error: …` and does not abort, so the model retries
    /// — which is right for a bad path or a missing argument and is a trap for
    /// a controlled vocabulary, where "try again" cannot succeed without the
    /// list. What the run then reports is `StepBudgetReached`, and downstream
    /// the ingest txn aborts with *"wrote no knowledge pages"*, which points
    /// the investigator at the model's page authoring. Both statements are
    /// true and neither names the cause. This one does.
    VocabularyRetriesExhausted,
    Cancelled,
    Error,
}
