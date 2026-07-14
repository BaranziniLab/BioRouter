//! A tool the model cannot see is a tool it cannot call.
//!
//! The runaway `ui_describe` loop in the test drive was **caused by BioRouter's
//! own guard**, not merely missed by it:
//!
//!   1. `RepetitionInspector` denies a call once it repeats past `max_repetitions`;
//!   2. `handle_denied_tools` answers with *"The user has declined to run this
//!      tool"* — a lie, and one the model cannot distinguish from a real human
//!      refusal; and
//!   3. **the loop simply continues.** Nothing removes the tool from the next
//!      provider request, and nothing ends the turn.
//!
//! So the model retried, was denied, retried, was denied — bounded only by
//! `max_turns` (100 by default, 24 for apps). Every one of those iterations is a
//! billed provider call. The whole guard was, literally, a sentence in a tool
//! result: prose-only enforcement, which is exactly the failure mode this campaign
//! exists to eliminate.
//!
//! [`TurnToolGuard`] makes it structural. A blocked tool is **removed from the
//! tool list** for the remainder of the turn, so the model cannot emit the call at
//! all — there is no schema for it in the request. And a second attempt at an
//! already-blocked signature ends the turn deterministically, rather than trusting
//! the model to read "DO NOT call this again".

use std::collections::{HashMap, HashSet};

use rmcp::model::Tool;

use super::turn_abort::TurnAbortCode;

/// What the loop guard decided about one call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardVerdict {
    /// Nothing to see here.
    Allow,
    /// This call was already blocked once. The turn ends.
    Abort(TurnAbortCode),
}

/// Per-turn state for the tool-loop guard. Created fresh at the start of each
/// turn — a tool blocked last turn must be available again this turn, because the
/// user's new message may be exactly what makes the call sensible.
#[derive(Debug, Default)]
pub struct TurnToolGuard {
    /// Tool names withheld from the provider request for the rest of this turn.
    disabled: HashSet<String>,
    /// How many times each blocked signature has been attempted *since* it was
    /// blocked. One is a stale in-flight call; two is a model that has stopped
    /// responding to feedback.
    blocked_attempts: HashMap<String, u32>,
}

impl TurnToolGuard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that the loop guard blocked `tool`. It is withheld from every
    /// subsequent provider call this turn.
    pub fn block(&mut self, tool: &str) {
        self.disabled.insert(tool.to_string());
    }

    /// Whether `tool` is currently withheld.
    pub fn is_blocked(&self, tool: &str) -> bool {
        self.disabled.contains(tool)
    }

    /// Remove every blocked tool from the list about to be sent to the provider.
    ///
    /// This is the enforcement. Everything else in this module is a backstop: the
    /// model cannot call a tool whose schema is not in the request.
    pub fn filter_tools(&self, tools: &mut Vec<Tool>) {
        if self.disabled.is_empty() {
            return;
        }
        tools.retain(|t| !self.disabled.contains(t.name.as_ref()));
    }

    /// Note an attempt to call a tool that is already blocked.
    ///
    /// Reaching here at all means the model emitted a call for a tool that was not
    /// in its tool list (a stale call already in flight, or a provider echoing a
    /// prior turn). Once is forgivable. Twice means the loop is not converging, and
    /// the turn is terminated rather than left to burn the turn budget.
    pub fn note_blocked_call(&mut self, tool: &str) -> GuardVerdict {
        let n = self.blocked_attempts.entry(tool.to_string()).or_insert(0);
        *n += 1;
        if *n >= 2 {
            GuardVerdict::Abort(TurnAbortCode::ToolLoop {
                tool: tool.to_string(),
                repeats: *n,
            })
        } else {
            GuardVerdict::Allow
        }
    }

    /// Tools blocked this turn, for logging.
    pub fn blocked(&self) -> Vec<&str> {
        self.disabled.iter().map(String::as_str).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;
    use std::sync::Arc;

    fn tool(name: &str) -> Tool {
        Tool {
            name: Cow::Owned(name.to_string()),
            description: None,
            input_schema: Arc::new(serde_json::Map::new()),
            output_schema: None,
            annotations: None,
            title: None,
            icons: None,
            meta: None,
        }
    }

    /// The enforcement: a blocked tool is GONE from the request, so the model has
    /// no schema to call it with. Nothing else in this module matters if this
    /// doesn't hold.
    #[test]
    fn a_blocked_tool_is_removed_from_the_tool_list() {
        let mut guard = TurnToolGuard::new();
        guard.block("ui_describe");

        let mut tools = vec![tool("ui_describe"), tool("ui_render"), tool("app_call")];
        guard.filter_tools(&mut tools);

        let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        assert_eq!(names, vec!["ui_render", "app_call"]);
    }

    /// The guard must be inert until it fires — a healthy turn keeps every tool.
    #[test]
    fn an_untriggered_guard_changes_nothing() {
        let guard = TurnToolGuard::new();
        let mut tools = vec![tool("ui_describe"), tool("ui_render")];
        guard.filter_tools(&mut tools);
        assert_eq!(tools.len(), 2);
    }

    /// One stale in-flight call is tolerated; a second is a model that is not
    /// responding to feedback, and the turn ends.
    #[test]
    fn a_second_attempt_at_a_blocked_tool_aborts_the_turn() {
        let mut guard = TurnToolGuard::new();
        guard.block("ui_describe");

        assert_eq!(
            guard.note_blocked_call("ui_describe"),
            GuardVerdict::Allow,
            "one stale call in flight is forgivable"
        );

        match guard.note_blocked_call("ui_describe") {
            GuardVerdict::Abort(TurnAbortCode::ToolLoop { tool, repeats }) => {
                assert_eq!(tool, "ui_describe");
                assert_eq!(repeats, 2);
            }
            other => panic!("the turn must terminate, got {other:?}"),
        }
    }

    /// Blocking one tool must not disarm the others.
    #[test]
    fn blocking_is_per_tool() {
        let mut guard = TurnToolGuard::new();
        guard.block("ui_describe");

        assert!(guard.is_blocked("ui_describe"));
        assert!(!guard.is_blocked("ui_render"));

        let mut tools = vec![tool("ui_render")];
        guard.filter_tools(&mut tools);
        assert_eq!(tools.len(), 1, "an unrelated tool stays available");
    }
}
