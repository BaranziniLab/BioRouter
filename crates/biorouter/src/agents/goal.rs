//! Session goals (`/goal`, Claude Code-style).
//!
//! `/goal <condition>` registers a session-scoped Stop hook whose LLM judge
//! evaluates the condition every time the agent tries to finish its turn. If
//! the condition is not met, the stop is blocked and the judge's feedback is
//! fed back to the model, so the agent keeps working until the condition holds.
//!
//! Robustness (see the `/Users/wgu/Downloads/hook failure.json` case that
//! motivated it — a "summarize all 400 sites in chat" goal that looped
//! forever):
//!
//! 1. **Truncation-aware judging.** The judge only ever sees a recent
//!    `transcript_tail`, never the whole conversation. A goal that emits a
//!    large volume in-chat will always look "incomplete" in the tail, so the
//!    judge rule explicitly forbids treating "output not fully visible in this
//!    excerpt" as proof the work wasn't done — it must verify the *deliverable
//!    / end state*, not require the entire output inline.
//! 2. **A real iteration cap.** The generic Stop-hook block cap resets on every
//!    tool call ([`crate::hooks::STOP_HOOK_BLOCK_CAP`]); a goal agent always
//!    runs a tool before stopping, so that cap never fires. Goals therefore
//!    keep their own iteration counter in [`GoalState`] that does NOT reset on
//!    tool calls, bounded by [`GOAL_MAX_ITERATIONS`].
//! 3. **Stall detection.** If the judge keeps returning essentially the same
//!    feedback ([`GOAL_STALL_LIMIT`] times running), the loop is not
//!    converging and is stopped early.
//! 4. **Graceful give-up.** When the cap or a stall is hit, the goal is cleared
//!    and the agent is told to deliver its best-effort answer (what it did,
//!    what's incomplete, the blocker) instead of spinning — control returns to
//!    the user rather than looping indefinitely.

use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio::sync::Mutex;

use crate::conversation::message::Message;
use crate::conversation::Conversation;
use crate::hooks::{HookDefinition, HookEvent};

use super::Agent;

/// Max messages included in the transcript tail sent to the Stop-hook judge.
const TRANSCRIPT_TAIL_MESSAGES: usize = 14;
/// Per-message character cap in the transcript tail.
const TRANSCRIPT_MESSAGE_MAX_CHARS: usize = 2_000;
/// Total character cap of the transcript tail.
const TRANSCRIPT_TAIL_MAX_CHARS: usize = 12_000;

/// Hard upper bound on goal iterations (judge blocks) before the loop gives up
/// and hands a best-effort answer back to the user. Does NOT reset on tool
/// calls, unlike the generic Stop-hook block cap.
pub const GOAL_MAX_ITERATIONS: u32 = 20;
/// Consecutive near-identical judge reasons that count as "not converging".
pub const GOAL_STALL_LIMIT: u32 = 3;
/// Jaccard similarity (over word sets) at/above which two judge reasons are
/// treated as "the same demand" for stall detection.
const GOAL_STALL_SIMILARITY: f32 = 0.5;

/// Words accepted as "clear the goal" arguments, mirroring Claude Code.
const CLEAR_WORDS: &[&str] = &["clear", "stop", "off", "reset", "none", "cancel"];

/// An active goal for one session.
#[derive(Debug, Clone)]
pub struct GoalState {
    pub condition: String,
    pub set_at: DateTime<Utc>,
    /// Number of times the judge has blocked the stop for this goal.
    pub iterations: u32,
    /// The judge's most recent feedback, for stall comparison.
    pub last_reason: Option<String>,
    /// Consecutive iterations whose feedback resembled the previous one.
    pub stall_count: u32,
}

impl GoalState {
    fn new(condition: String) -> Self {
        Self {
            condition,
            set_at: Utc::now(),
            iterations: 0,
            last_reason: None,
            stall_count: 0,
        }
    }
}

/// What the agent loop should do after the judge blocks a goal's stop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoalOutcome {
    /// Keep working: feed the judge's feedback back to the model.
    Continue,
    /// Stop looping and have the agent deliver a best-effort answer.
    GiveUp { attempts: u32, stalled: bool },
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

/// Lowercase alphanumeric word set of a string, for similarity comparison.
fn word_set(s: &str) -> std::collections::HashSet<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(|w| w.to_ascii_lowercase())
        .collect()
}

/// Jaccard similarity of two reasons' word sets, in `[0, 1]`.
fn reason_similarity(a: &str, b: &str) -> f32 {
    let (sa, sb) = (word_set(a), word_set(b));
    if sa.is_empty() && sb.is_empty() {
        return 1.0;
    }
    let inter = sa.intersection(&sb).count() as f32;
    let union = sa.union(&sb).count() as f32;
    if union == 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// The judge rule installed as the goal's Stop hook. `attempt` is the upcoming
/// evaluation number and `previous_feedback` is the judge's last note (if any),
/// so the judge can detect stagnation and decide to hand control back rather
/// than block forever.
fn goal_judge_rule(condition: &str, attempt: u32, previous_feedback: Option<&str>) -> String {
    let history = match previous_feedback {
        Some(prev) => format!(
            "\nThis is evaluation attempt #{attempt}. Your previous feedback was:\n\"{}\"\n\
             If the agent has NOT meaningfully progressed past that feedback, or it is \
             repeating the same kind of output without converging, the loop is stuck — \
             prefer returning control to the user (see below).",
            ellipsize(prev, 600)
        ),
        None => String::new(),
    };
    format!(
        "A goal condition is active for this session. The agent just tried to finish its \
         turn. Decide whether the goal is met.\n\n\
         Goal condition:\n{condition}\n{history}\n\n\
         IMPORTANT — how to read the evidence:\n\
         • The event JSON's `transcript_tail` is only a RECENT EXCERPT of the \
         conversation, not the whole thing. Earlier output has scrolled out of view. \
         Do NOT treat \"the full output isn't visible in this excerpt\" as proof the \
         work wasn't done.\n\
         • Verify the DELIVERABLE / end state, not whether the entire output is pasted \
         inline. Evidence of completion includes the agent reporting it finished with \
         concrete results (counts, a saved file path, a clear final answer), tool \
         results showing success, or work products written to disk.\n\
         • If the user has since relaxed or clarified the goal in the transcript, honor \
         that relaxed version.\n\n\
         Respond {{\"ok\": true}} (let the agent stop) when ANY of these hold:\n\
         • the goal is verifiably met or substantially satisfied;\n\
         • the agent is blocked on something only the user can resolve (it asked a \
         question, a permission was denied);\n\
         • the goal cannot be satisfied as literally specified (e.g. it would require \
         far more output than fits in chat), OR the agent has stopped making real \
         progress across attempts — in these cases returning control to the user with a \
         best-effort answer is the right outcome.\n\n\
         Respond {{\"ok\": false, \"reason\": \"<short, concrete next step>\"}} ONLY when \
         the goal is genuinely not met AND a specific, different action would move it \
         forward. When in doubt after several attempts, prefer {{\"ok\": true}} — do not \
         keep the agent looping."
    )
}

/// The instruction fed to the model when a goal gives up: deliver a best-effort
/// answer instead of starting another pass.
pub(crate) fn giveup_instruction(last_reason: &str) -> String {
    format!(
        "The goal has been stopped and cleared after repeated attempts without a \
         verifiable result. Do NOT start another pass or call more tools to redo the \
         work. Give the user your best final answer now, in this turn:\n\
         1. A concise summary of what you DID accomplish.\n\
         2. What remains incomplete or could not be done.\n\
         3. The specific blocker (for example: the full output is too large to place in \
         chat; some resources were unreachable; the request was ambiguous).\n\
         If the goal was too broad, suggest a narrower follow-up the user could set. \
         The evaluator's most recent note was: \"{}\"",
        ellipsize(last_reason, 400)
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

    /// Install (or replace) the goal's Stop-hook evaluator with the given
    /// attempt context.
    async fn install_goal_hook(
        &self,
        session_id: &str,
        condition: &str,
        attempt: u32,
        previous_feedback: Option<&str>,
    ) {
        self.hooks_manager
            .set_session_hooks(
                session_id,
                HookEvent::Stop,
                vec![HookDefinition::Prompt {
                    prompt: goal_judge_rule(condition, attempt, previous_feedback),
                    model: None,
                    provider: None,
                    timeout: None,
                }],
            )
            .await;
    }

    /// Set (or replace) the session goal and install its Stop-hook evaluator.
    pub async fn set_goal(&self, session_id: &str, condition: String) {
        self.install_goal_hook(session_id, &condition, 1, None).await;
        self.hooks_manager.reset_stop_blocks(session_id).await;
        self.goals
            .goals
            .lock()
            .await
            .insert(session_id.to_string(), GoalState::new(condition));
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

    /// Record a judge block against the active goal and decide whether to keep
    /// looping or give up. Updates the iteration / stall counters and, when
    /// continuing, refreshes the judge hook with progress context so the next
    /// evaluation is stagnation-aware. Returns `None` if no goal is active.
    pub(crate) async fn record_goal_block(
        &self,
        session_id: &str,
        reason: &str,
    ) -> Option<GoalOutcome> {
        let (condition, attempt, outcome) = {
            let mut goals = self.goals.goals.lock().await;
            let state = goals.get_mut(session_id)?;
            state.iterations += 1;

            let similar = state
                .last_reason
                .as_deref()
                .map(|prev| reason_similarity(prev, reason) >= GOAL_STALL_SIMILARITY)
                .unwrap_or(false);
            state.stall_count = if similar { state.stall_count + 1 } else { 0 };
            state.last_reason = Some(reason.to_string());

            let stalled = state.stall_count >= GOAL_STALL_LIMIT;
            let capped = state.iterations >= GOAL_MAX_ITERATIONS;
            let outcome = if capped || stalled {
                GoalOutcome::GiveUp {
                    attempts: state.iterations,
                    stalled,
                }
            } else {
                GoalOutcome::Continue
            };
            (state.condition.clone(), state.iterations, outcome)
        };

        if outcome == GoalOutcome::Continue {
            // Refresh the judge with the new attempt # and latest feedback so it
            // can recognize stagnation on the next stop.
            self.install_goal_hook(session_id, &condition, attempt + 1, Some(reason))
                .await;
        }
        Some(outcome)
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
                    "🎯 Active goal (set {} ago, {} evaluation(s) so far):\n{}\n\nThe goal \
                     clears automatically once met — or after {} attempts if it can't \
                     converge; `/goal clear` stops it early.",
                    format_elapsed(goal.set_at),
                    goal.iterations,
                    goal.condition,
                    GOAL_MAX_ITERATIONS,
                ),
                None => "No active goal. Set one with `/goal <condition>` — a verifiable end \
                         state, e.g. `/goal cargo test exits 0 and git status is clean`. \
                         Biorouter keeps working until an automatic evaluator confirms the \
                         condition is met, then hands back a best-effort answer if it can't."
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
             an automatic evaluator checks the condition and sends feedback if it is not \
             met yet, in which case you must keep going. Prefer durable deliverables \
             (e.g. saving large output to a file) over pasting huge volumes into chat. \
             Stop early if you are blocked on input only the user can provide.\n\n\
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
    fn judge_rule_embeds_condition_and_truncation_guard() {
        let rule = goal_judge_rule("npm test exits 0", 1, None);
        assert!(rule.contains("npm test exits 0"));
        assert!(rule.contains("transcript_tail"));
        // The truncation guard is the core fix.
        assert!(rule.contains("RECENT EXCERPT"));
    }

    #[test]
    fn judge_rule_includes_attempt_and_prior_feedback() {
        let rule = goal_judge_rule("do x", 4, Some("you only showed part of the list"));
        assert!(rule.contains("attempt #4"));
        assert!(rule.contains("only showed part of the list"));
    }

    #[test]
    fn similar_reasons_score_high() {
        let a = "The transcript only shows the start of the 407-entry list, continue outputting";
        let b = "The transcript still only shows the beginning of the 407-entry list, continue";
        assert!(reason_similarity(a, b) >= GOAL_STALL_SIMILARITY);
    }

    #[test]
    fn different_reasons_score_low() {
        let a = "Run cargo test and make it pass";
        let b = "The website is unreachable, document the failure reason";
        assert!(reason_similarity(a, b) < GOAL_STALL_SIMILARITY);
    }

    #[test]
    fn giveup_instruction_mentions_blocker_and_reason() {
        let text = giveup_instruction("output too large for chat");
        assert!(text.contains("best final answer"));
        assert!(text.contains("output too large for chat"));
    }
}
