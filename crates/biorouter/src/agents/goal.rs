//! Session goals (`/goal`, Claude Code-style).
//!
//! `/goal <condition>` registers a session-scoped Stop hook whose LLM judge
//! evaluates the condition every time the agent tries to finish its turn. If
//! the condition is not met, the stop is blocked and the judge's feedback is
//! fed back to the model, so the agent keeps working — across as many
//! iterations as needed — until the condition holds (or the user clears the
//! goal). Built entirely on the existing hooks mechanism: the evaluator is a
//! normal `HookDefinition::Prompt` on [`HookEvent::Stop`], subject to the
//! same consecutive-block cap as any other Stop hook.

use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio::sync::Mutex;

use crate::conversation::message::Message;
use crate::conversation::Conversation;
use crate::hooks::{HookDefinition, HookEvent};

use super::Agent;

/// Max messages included in the transcript tail sent to the Stop-hook judge.
const TRANSCRIPT_TAIL_MESSAGES: usize = 10;
/// Per-message character cap in the transcript tail.
const TRANSCRIPT_MESSAGE_MAX_CHARS: usize = 2_000;
/// Total character cap of the transcript tail.
const TRANSCRIPT_TAIL_MAX_CHARS: usize = 8_000;

/// Words accepted as "clear the goal" arguments, mirroring Claude Code.
const CLEAR_WORDS: &[&str] = &["clear", "stop", "off", "reset", "none", "cancel"];

/// An active goal for one session.
#[derive(Debug, Clone)]
pub struct GoalState {
    pub condition: String,
    pub set_at: DateTime<Utc>,
}

/// Per-session goal conditions, keyed by session id.
#[derive(Default)]
pub struct GoalRegistry {
    goals: Mutex<HashMap<String, GoalState>>,
}

/// Truncate to `max` characters on a char boundary, appending an ellipsis.
pub(crate) fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{truncated}…")
}

/// The judge rule installed as the goal's Stop hook. The judge receives the
/// Stop event payload (which includes `transcript_tail`) and replies
/// {"ok": bool, "reason": ...}; `ok: false` blocks the stop.
fn goal_judge_rule(condition: &str) -> String {
    format!(
        "A goal condition is active for this session. The agent just tried to finish \
         its turn. The event JSON contains a `transcript_tail` field with the most \
         recent conversation messages — evaluate ONLY whether the goal condition below \
         is fully met, based on that transcript.\n\n\
         Goal condition:\n{condition}\n\n\
         Respond {{\"ok\": true}} if the condition is verifiably met, or if the agent \
         is blocked on something only the user can resolve (it asked the user a \
         question, a permission was denied, or it failed repeatedly at the same step \
         with no path forward).\n\
         Respond {{\"ok\": false, \"reason\": \"<short, concrete instruction for what \
         to do next>\"}} if the condition is not yet met and the agent can keep working."
    )
}

/// A compact, role-prefixed rendering of the last few conversation messages,
/// for the Stop-hook judge. Returns `None` when there is nothing to show.
pub(crate) fn transcript_tail(conversation: &Conversation) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut budget = TRANSCRIPT_TAIL_MAX_CHARS;
    for message in conversation.messages().iter().rev() {
        if parts.len() >= TRANSCRIPT_TAIL_MESSAGES || budget == 0 {
            break;
        }
        let text = message.as_concat_text();
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let role = match message.role {
            rmcp::model::Role::User => "user",
            rmcp::model::Role::Assistant => "assistant",
        };
        let entry = format!(
            "{role}: {}",
            ellipsize(text, TRANSCRIPT_MESSAGE_MAX_CHARS.min(budget))
        );
        budget = budget.saturating_sub(entry.chars().count());
        parts.push(entry);
    }
    if parts.is_empty() {
        return None;
    }
    parts.reverse();
    Some(parts.join("\n\n"))
}

fn format_elapsed(since: DateTime<Utc>) -> String {
    let secs = (Utc::now() - since).num_seconds().max(0);
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m", secs / 60),
        _ => format!("{}h {}m", secs / 3600, (secs % 3600) / 60),
    }
}

impl Agent {
    /// The goal currently active for a session, if any.
    pub async fn active_goal(&self, session_id: &str) -> Option<GoalState> {
        self.goals.goals.lock().await.get(session_id).cloned()
    }

    /// Set (or replace) the session goal and install its Stop-hook evaluator.
    pub async fn set_goal(&self, session_id: &str, condition: String) {
        self.hooks_manager
            .set_session_hooks(
                session_id,
                HookEvent::Stop,
                vec![HookDefinition::Prompt {
                    prompt: goal_judge_rule(&condition),
                    model: None,
                    provider: None,
                    timeout: None,
                }],
            )
            .await;
        self.hooks_manager.reset_stop_blocks(session_id).await;
        self.goals.goals.lock().await.insert(
            session_id.to_string(),
            GoalState {
                condition,
                set_at: Utc::now(),
            },
        );
    }

    /// Clear the session goal and its Stop-hook evaluator. Returns the goal
    /// that was active, if any.
    pub async fn clear_goal(&self, session_id: &str) -> Option<GoalState> {
        self.hooks_manager
            .clear_session_hooks(session_id, HookEvent::Stop)
            .await;
        self.hooks_manager.reset_stop_blocks(session_id).await;
        self.goals.goals.lock().await.remove(session_id)
    }

    /// `/goal` slash command: no args shows status, a clear word clears, and
    /// anything else sets the condition and immediately starts working toward
    /// it (the returned user-role message continues into the agent loop).
    pub(crate) async fn handle_goal_command(
        &self,
        params_str: &str,
        session_id: &str,
    ) -> Result<Option<Message>> {
        let arg = params_str.trim();

        if arg.is_empty() {
            let text = match self.active_goal(session_id).await {
                Some(goal) => format!(
                    "🎯 Active goal (set {} ago):\n{}\n\nThe goal clears automatically once \
                     its condition is met; `/goal clear` stops it early.",
                    format_elapsed(goal.set_at),
                    goal.condition
                ),
                None => "No active goal. Set one with `/goal <condition>` — a verifiable end \
                         state, e.g. `/goal cargo test exits 0 and git status is clean`. \
                         Biorouter keeps working until an automatic evaluator confirms the \
                         condition is met."
                    .to_string(),
            };
            return Ok(Some(Message::assistant().with_text(text)));
        }

        if CLEAR_WORDS.contains(&arg.to_ascii_lowercase().as_str()) {
            let text = match self.clear_goal(session_id).await {
                Some(goal) => format!("Goal cleared: {}", ellipsize(&goal.condition, 200)),
                None => "No active goal to clear.".to_string(),
            };
            return Ok(Some(Message::assistant().with_text(text)));
        }

        self.set_goal(session_id, arg.to_string()).await;

        // A user-role message continues into the agent loop, so work toward
        // the goal starts immediately (mirrors workflow slash commands).
        Ok(Some(Message::user().with_text(format!(
            "A goal has been set for this session. Work toward it now and keep working \
             until it is verifiably met. When you believe it is met, finish your turn — \
             an automatic evaluator checks the condition and sends feedback if it is \
             not met yet, in which case you must keep going. Only stop early if you are \
             blocked on input that only the user can provide.\n\n\
             Goal condition:\n{arg}"
        ))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ellipsize_respects_char_boundaries() {
        assert_eq!(ellipsize("short", 10), "short");
        let long = "éééééééééééé"; // 13 two-byte chars
        let cut = ellipsize(long, 5);
        assert_eq!(cut.chars().count(), 5);
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn transcript_tail_formats_roles_and_caps_messages() {
        let mut conversation = Conversation::default();
        conversation.push(Message::user().with_text("please fix the tests"));
        conversation.push(Message::assistant().with_text("done, all 5 tests pass"));
        let tail = transcript_tail(&conversation).unwrap();
        assert!(tail.starts_with("user: please fix the tests"));
        assert!(tail.contains("assistant: done, all 5 tests pass"));
    }

    #[test]
    fn transcript_tail_empty_conversation_is_none() {
        assert!(transcript_tail(&Conversation::default()).is_none());
    }

    #[test]
    fn judge_rule_embeds_condition() {
        let rule = goal_judge_rule("npm test exits 0");
        assert!(rule.contains("npm test exits 0"));
        assert!(rule.contains("transcript_tail"));
    }
}
