//! Turning BioRouter's `Vec<Message>` into the single prompt a coding-agent CLI
//! accepts.
//!
//! ## Why flattening, and not replay
//!
//! [`crate::providers::base::Provider`] hands every call the *entire*
//! conversation, and both vendor CLIs take a *prompt*. The obvious idea — replay
//! the history as alternating turns over `--input-format stream-json` — was
//! measured and does not work: that channel is an **interactive** one, so each
//! user message it receives starts its own complete turn, and an injected
//! assistant message is ignored outright. A three-message replay produced two
//! separate answers rather than one continued conversation.
//!
//! The other candidate, `--session-id` then `--resume`, does work across
//! processes, and is a legitimate later optimisation. It is not the default
//! because it moves the authoritative history into the child, where BioRouter's
//! compaction, message editing and `.biorouterignore`-driven redaction cannot
//! reach it — the two copies would silently diverge exactly when a long session
//! gets compacted. Flattening keeps BioRouter's transcript authoritative, which
//! is the property the `Provider` contract already assumes.
//!
//! The cost is re-sending the conversation each turn. Both vendors prompt-cache
//! a stable prefix, so the marginal cost is far below the naive reading.

use crate::conversation::message::{Message, MessageContent};
use rmcp::model::Role;

/// Wrapper tag for prior turns. A tag rather than `--- ` markers because both
/// models treat an XML-ish block as data far more reliably than a horizontal
/// rule, which they will sometimes continue as prose.
const HISTORY_OPEN: &str = "<conversation_history>";
const HISTORY_CLOSE: &str = "</conversation_history>";

/// How much of one tool result to keep. A single `shell` or SQL result can be
/// megabytes, and the whole transcript has to fit in one prompt; the cap keeps a
/// pathological result from evicting the actual conversation. Generous enough
/// that ordinary results are untouched.
const TOOL_RESULT_CHAR_BUDGET: usize = 4_000;

/// Render one message's content as transcript text.
///
/// Tool traffic is included, as text. The child cannot be handed BioRouter's
/// tool *calls* structurally — it is a different agent with its own loop — but it
/// very much needs to know what was already looked up, or it will ask again.
fn render_content(content: &MessageContent) -> Option<String> {
    match content {
        MessageContent::Text(t) => {
            let text = t.text.trim();
            (!text.is_empty()).then(|| text.to_string())
        }
        MessageContent::Thinking(_) | MessageContent::RedactedThinking(_) => None,
        MessageContent::Image(i) => Some(format!("[image omitted: {}]", i.mime_type)),
        MessageContent::ToolRequest(r) => {
            Some(format!("[called tool: {}]", r.to_readable_string()))
        }
        MessageContent::ToolResponse(r) => Some(match &r.tool_result {
            Ok(result) => {
                let body = result
                    .content
                    .iter()
                    .filter_map(|c| c.as_text().map(|t| t.text.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!(
                    "[tool result: {}]",
                    truncate(body.trim(), TOOL_RESULT_CHAR_BUDGET)
                )
            }
            Err(e) => format!("[tool error: {e}]"),
        }),
        // Control-plane content the child has no use for and cannot act on.
        MessageContent::ToolConfirmationRequest(_)
        | MessageContent::ActionRequired(_)
        | MessageContent::FrontendToolRequest(_)
        | MessageContent::SystemNotification(_) => None,
    }
}

fn truncate(s: &str, budget: usize) -> String {
    if s.chars().count() <= budget {
        return s.to_string();
    }
    let kept: String = s.chars().take(budget).collect();
    format!("{kept}\n… [truncated, {} chars total]", s.chars().count())
}

fn render_message(message: &Message) -> Option<String> {
    let body = message
        .content
        .iter()
        .filter_map(render_content)
        .collect::<Vec<_>>()
        .join("\n");
    let body = body.trim();
    if body.is_empty() {
        return None;
    }
    let who = match message.role {
        Role::User => "User",
        Role::Assistant => "Assistant",
    };
    Some(format!("{who}: {body}"))
}

/// Flatten a conversation into one prompt string.
///
/// Shape:
///
/// * A single user message with no history becomes that message verbatim. This is
///   the common case and adding scaffolding to it only degrades the answer.
/// * Otherwise the earlier turns go inside [`HISTORY_OPEN`]/[`HISTORY_CLOSE`] and
///   the final user message follows *outside* the block, so the live instruction
///   is unambiguous. Without that separation both models tend to summarise the
///   transcript instead of answering it.
///
/// Returns `None` when there is nothing a model could answer — every message
/// hidden from the agent, or no user turn at all. Callers surface that as a
/// request error rather than spawning a process to say nothing.
pub fn flatten(messages: &[Message]) -> Option<String> {
    let visible: Vec<&Message> = messages.iter().filter(|m| m.is_agent_visible()).collect();

    // The live instruction is the last *user* turn. Anything after it (a partial
    // assistant message, tool traffic) is history, not instruction.
    let last_user = visible.iter().rposition(|m| m.role == Role::User)?;

    let latest = render_message(visible[last_user])?;
    // Strip the "User: " label from the live instruction — it is not part of what
    // the user asked, and its presence invites the model to answer in transcript
    // form.
    let latest_body = latest.strip_prefix("User: ").unwrap_or(&latest).to_string();

    let history: Vec<String> = visible[..last_user]
        .iter()
        .filter_map(|m| render_message(m))
        .collect();
    // Trailing content after the last user turn (e.g. tool results already
    // gathered this turn) is real context and must not be dropped.
    let trailing: Vec<String> = visible[last_user + 1..]
        .iter()
        .filter_map(|m| render_message(m))
        .collect();

    if history.is_empty() && trailing.is_empty() {
        return Some(latest_body);
    }

    let mut out = String::new();
    out.push_str(HISTORY_OPEN);
    out.push('\n');
    for line in history.iter().chain(trailing.iter()) {
        out.push_str(line);
        out.push_str("\n\n");
    }
    out.push_str(HISTORY_CLOSE);
    out.push_str("\n\n");
    out.push_str(&latest_body);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(text: &str) -> Message {
        Message::user().with_text(text)
    }
    fn assistant(text: &str) -> Message {
        Message::assistant().with_text(text)
    }

    /// The single-turn case is passed through untouched. Scaffolding a lone
    /// question measurably degrades the answer, and this is by far the most
    /// common shape.
    #[test]
    fn a_lone_user_message_is_the_prompt_verbatim() {
        let out = flatten(&[user("What gene is linked to MS?")]).unwrap();
        assert_eq!(out, "What gene is linked to MS?");
        assert!(!out.contains(HISTORY_OPEN));
    }

    /// Earlier turns are wrapped; the live instruction sits outside the wrapper
    /// and carries no role label.
    #[test]
    fn earlier_turns_are_wrapped_and_the_live_instruction_is_bare() {
        let out = flatten(&[
            user("My favourite gene is FOXP2."),
            assistant("Noted."),
            user("Which gene did I name?"),
        ])
        .unwrap();

        assert!(out.starts_with(HISTORY_OPEN));
        assert!(out.contains("User: My favourite gene is FOXP2."));
        assert!(out.contains("Assistant: Noted."));
        assert!(out.trim_end().ends_with("Which gene did I name?"));
        // The live instruction must not be inside the history block.
        let close = out
            .find(HISTORY_CLOSE)
            .expect("the history block is closed");
        let (before, after) = out.split_at(close);
        assert!(after.contains("Which gene did I name?"));
        assert!(!before.contains("Which gene did I name?"));
    }

    /// Messages hidden from the agent stay hidden. This is the same predicate the
    /// rest of the loop honours, and a coding agent is not an exception to it.
    #[test]
    fn agent_invisible_messages_are_excluded() {
        let mut hidden = user("SECRET-DO-NOT-SEND");
        hidden.metadata.agent_visible = false;
        let out = flatten(&[hidden, user("Say hi")]).unwrap();
        assert!(!out.contains("SECRET-DO-NOT-SEND"));
    }

    /// With no user turn there is nothing to answer, and we must not spawn a
    /// process to discover that.
    #[test]
    fn a_conversation_with_no_user_turn_is_none() {
        assert!(flatten(&[assistant("unprompted")]).is_none());
        assert!(flatten(&[]).is_none());
    }

    /// A pathological tool result cannot evict the conversation.
    #[test]
    fn oversized_tool_results_are_truncated() {
        let huge = "x".repeat(TOOL_RESULT_CHAR_BUDGET * 3);
        let msg = Message::user().with_tool_response(
            "call-1",
            Ok(rmcp::model::CallToolResult::success(vec![
                rmcp::model::Content::text(huge),
            ])),
        );
        let out = flatten(&[user("first"), msg, user("now answer")]).unwrap();
        assert!(out.contains("[truncated,"));
        assert!(
            out.len() < TOOL_RESULT_CHAR_BUDGET * 2,
            "truncation did not bound the prompt"
        );
    }

    /// Thinking blocks are dropped: they are the *previous* model's private
    /// reasoning, they are often signed, and replaying them as text into a
    /// different vendor's agent is noise at best.
    #[test]
    fn thinking_is_not_replayed() {
        let msg = Message::assistant()
            .with_thinking("internal deliberation", "sig")
            .with_text("the answer");
        let out = flatten(&[user("q"), msg, user("follow up")]).unwrap();
        assert!(!out.contains("internal deliberation"));
        assert!(out.contains("the answer"));
    }
}
