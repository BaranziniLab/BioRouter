//! Structured result envelope for subagent runs (BR-40).
//!
//! The subagent tool used to return only the child's *last text message*, so a
//! child that ended on a tool call (or an empty message) surfaced the useless
//! string "No text content in last message" to the parent. This module replaces
//! that lossy string with a typed envelope — `{status, summary, error,
//! artifacts, tokens}` — that always carries a meaningful summary and enough
//! metadata for the parent to reason about what the child actually did.

use rmcp::model::{CallToolResult, Content, Role};
use serde::Serialize;

use crate::conversation::message::{Message, MessageContent};
use crate::conversation::Conversation;

/// How a subagent run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentStatus {
    /// Ended with a final text summary (or a response-schema output).
    Completed,
    /// Stopped before an irreversible action because a word in its task pointed
    /// at something it could not identify, and returned the question instead of
    /// guessing. Nothing was changed.
    ///
    /// This exists because stopping to ask is neither a success nor a failure,
    /// and the parent has to be able to tell it from both. With only
    /// `Completed | Incomplete | Error` a child that did exactly the right
    /// thing reported `completed`, the parent read a finished delegation with
    /// no edit behind it, and did the ambiguous work itself, rewriting every
    /// candidate file. That is a rational response to a status that lies.
    Blocked,
    /// Ran but produced no final text (ended on a tool call / empty message).
    /// The summary is salvaged from earlier work so the parent still gets
    /// something meaningful instead of "No text content in last message".
    Incomplete,
    /// Execution failed before a usable result was produced.
    Error,
}

impl SubagentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SubagentStatus::Completed => "completed",
            SubagentStatus::Blocked => "blocked",
            SubagentStatus::Incomplete => "incomplete",
            SubagentStatus::Error => "error",
        }
    }
}

/// The word a child opens its final message with when it stopped to ask.
///
/// A subagent's only return channel is its last text message, so the status has
/// to be read back out of it. The contract is stated in two places the child
/// actually reads (`prompts/subagent_system.md` and the `SUMMARY_INSTRUCTIONS`
/// block appended to every ad-hoc spawn) and parsed by [`blocked_question`].
pub const BLOCKED_MARKER: &str = "BLOCKED";

/// Leading/trailing ornament a model wraps an opening marker in: `**BLOCKED:**`,
/// `## BLOCKED`, `> BLOCKED`. Stripped rather than missed, because a missed
/// marker restores the exact defect this status exists to fix.
fn strip_ornament(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || matches!(c, '#' | '*' | '_' | '`' | '>'))
}

/// The child's question when its closing statement opens with the blocked
/// marker, `None` otherwise.
///
/// Deliberately anchored to the FIRST non-empty line: a summary that merely
/// mentions being blocked somewhere in its prose ("the deploy is blocked on CI")
/// is a report of finished work, not a question, and must not be re-classified.
fn blocked_question(closing: &str) -> Option<String> {
    let mut lines = closing.lines().skip_while(|line| line.trim().is_empty());
    let first = strip_ornament(lines.next()?);
    let word = first.get(..BLOCKED_MARKER.len())?;
    if !word.eq_ignore_ascii_case(BLOCKED_MARKER) {
        return None;
    }
    let rest = first.get(BLOCKED_MARKER.len()..)?;
    // `BLOCKED: q` · `**BLOCKED**: q` · `BLOCKED - q` · `BLOCKED` alone with the
    // question on the next line. Anything else is a word that merely starts with
    // "blocked" ("blockedTests failed"), which is not the marker.
    let rest = strip_ornament(rest);
    let mut after_marker = rest.chars();
    let tail = match after_marker.next() {
        None => "",
        // Stripped a second time: `**BLOCKED:** q` puts the closing emphasis
        // AFTER the separator, so one pass leaves "** q" as the question.
        // `Chars::as_str` gives the remainder without a byte index, which is
        // the difference between this and a slice that clippy will not accept
        // (the em-dash separator is multi-byte).
        Some(':' | '-' | '\u{2013}' | '\u{2014}') => strip_ornament(after_marker.as_str()),
        Some(_) => return None,
    };
    let question = if tail.is_empty() {
        lines
            .map(strip_ornament)
            .find(|line| !line.is_empty())
            .unwrap_or("")
    } else {
        tail
    };
    Some(question.to_string())
}

/// Opens the tool result the parent's model reads. It goes ABOVE the child's
/// summary because the parent reads that summary as an account of finished
/// work, so a question below four sections of completed steps reads as done.
const BLOCKED_HEADER: &str =
    "The subagent STOPPED and changed nothing. It could not tell what its task referred to, \
     so it returned a question instead of guessing.";

/// Closes it. Both halves are needed: the header stops the parent reading the
/// run as finished, and this says what to do instead of the thing it did before
/// (resolve the ambiguity itself and edit every candidate).
const BLOCKED_DIRECTIVE: &str = "\
What to do now, in order:
1. Try to answer the question from this conversation, or settle it with a tool. If that works, \
delegate the task again with the answer written out in full, and say nothing to the user.
2. If neither settles it, put the question to the user in your reply and wait for their answer.

Do NOT pick the most likely candidate, do NOT delegate again with a guess, and do NOT do the work \
yourself to find out which one was meant. The subagent stopped for a reason you share: nothing \
available to either of you says which one is right. Guessing here overwrites work that was not \
yours, and no summary afterwards would show that a guess happened.";

/// Token usage for the subagent's own session (lifetime totals across its turns).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SubagentTokens {
    pub total: i64,
    pub input: i64,
    pub output: i64,
}

/// Structured envelope returned by a subagent run.
#[derive(Debug, Clone, Serialize)]
pub struct SubagentResult {
    pub status: SubagentStatus,
    /// A human-readable recap of the child's work — never empty, never the old
    /// "No text content in last message" placeholder.
    pub summary: String,
    /// Present only when `status == Error`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Present only when `status == Blocked`: the one question the child needs
    /// answered before it can act. The `summary` still carries the candidates it
    /// found and the work it had already done; this is the part a caller can
    /// hand straight to the user.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    /// Best-effort list of files the child created/modified, derived from its
    /// file-writing tool calls.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<String>,
    /// Token usage for the child's session, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<SubagentTokens>,
    /// BR-71: the human typed into the child's tab during the run. Surfaced in
    /// the parent's tool result so it can weigh the summary.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub human_intervened: bool,
}

impl SubagentResult {
    /// A genuine execution failure — the child never produced a usable result.
    pub fn from_error(message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            status: SubagentStatus::Error,
            summary: format!("Subagent failed: {message}"),
            error: Some(message),
            question: None,
            artifacts: Vec::new(),
            tokens: None,
            human_intervened: false,
        }
    }

    /// Reclassify a partially produced result when the run's cancellation token
    /// won. Preserve any useful work and artifacts, but never report a stopped
    /// child as completed or as a provider failure.
    pub fn mark_cancelled(&mut self) {
        self.status = SubagentStatus::Incomplete;
        self.error = None;
        self.question = None;
        let recap = self.summary.trim();
        self.summary = if recap.is_empty() {
            "Subagent was cancelled before completion and produced no summary.".to_string()
        } else {
            format!(
                "Subagent was cancelled before completion. Best-effort recap from its work:\n\n{recap}"
            )
        };
    }

    /// The child's turn was **aborted** — a provider failure, a tool loop, a
    /// worker timeout. The run did not finish, so the envelope says `error` no
    /// matter how much prose the child left behind.
    ///
    /// This exists because the aborting message the loop writes into the child's
    /// conversation ("Ran into this error: …") is, structurally, an ordinary
    /// assistant text message. Handing that conversation to
    /// [`Self::from_conversation`] yields `completed` with `is_error: false`, so
    /// a subagent that never ran renders in chat exactly like one that
    /// succeeded — and the parent model is told the delegation completed.
    ///
    /// Work the child *did* manage before failing is preserved: its artifacts,
    /// and its last substantive message when that message is not simply the
    /// abort notice repeated.
    pub fn from_aborted_turn(
        conversation: &Conversation,
        code: &str,
        message: impl Into<String>,
    ) -> Self {
        let message = message.into();
        let artifacts = collect_artifacts(conversation);
        let salvaged = last_nonempty_text(conversation).filter(|text| !text.contains(&message));
        let summary = match salvaged {
            Some(text) => format!(
                "Subagent failed ({code}): {message}\n\nWhat it produced before failing:\n\n{text}"
            ),
            None => format!("Subagent failed ({code}): {message}"),
        };
        Self {
            status: SubagentStatus::Error,
            summary,
            error: Some(message),
            question: None,
            artifacts,
            tokens: None,
            human_intervened: false,
        }
    }

    /// Build an envelope from a completed subagent conversation.
    ///
    /// * `final_output` — the response-schema value when the workflow defined
    ///   one (an explicit, structured final output).
    /// * `return_last_only` — mirrors the `summary` tool param: last message
    ///   only vs. all text concatenated.
    pub fn from_conversation(
        conversation: &Conversation,
        final_output: Option<String>,
        return_last_only: bool,
    ) -> Self {
        let artifacts = collect_artifacts(conversation);

        // The blocked marker lives in the child's CLOSING statement, never in
        // the middle of a transcript, so it is read from that statement
        // directly rather than from `summary`, which under `summary: false` is
        // the whole run and opens with the child's first words.
        let closing = final_output
            .clone()
            .or_else(|| last_message_text(conversation));
        let question = closing.as_deref().and_then(blocked_question);

        // A response schema is an explicit, structured final output — trust it.
        let (status, summary) = if let Some(output) = final_output {
            (SubagentStatus::Completed, output)
        } else if return_last_only {
            if let Some(text) = last_message_text(conversation) {
                (SubagentStatus::Completed, text)
            } else if let Some(text) = last_nonempty_text(conversation) {
                // The child ended on a tool call / empty message. Salvage the
                // most recent substantive text so the parent isn't left blind.
                (
                    SubagentStatus::Incomplete,
                    format!(
                        "Subagent ended without a final summary (last step was not text). \
                         Best-effort recap from its work:\n\n{text}"
                    ),
                )
            } else {
                (SubagentStatus::Incomplete, describe_activity(conversation))
            }
        } else {
            let concatenated = concatenate_text(conversation);
            if concatenated.trim().is_empty() {
                (SubagentStatus::Incomplete, describe_activity(conversation))
            } else {
                (SubagentStatus::Completed, concatenated)
            }
        };

        // The child's own report is what tells us it stopped, so the promotion
        // happens here rather than in the branches above: only a run that would
        // otherwise have been filed as finished can be blocked. An `Incomplete`
        // or aborted run already tells the parent not to trust it.
        //
        // Written out per variant rather than as `(status, _) => status`, so a
        // status added later has to state whether a returned question can
        // promote it instead of inheriting a pass-through nobody chose.
        let status = match status {
            SubagentStatus::Completed if question.is_some() => SubagentStatus::Blocked,
            SubagentStatus::Completed => SubagentStatus::Completed,
            SubagentStatus::Blocked => SubagentStatus::Blocked,
            SubagentStatus::Incomplete => SubagentStatus::Incomplete,
            SubagentStatus::Error => SubagentStatus::Error,
        };
        // The question rides only on the status that means "answer me". Same
        // reason for spelling out the other three.
        let question = match status {
            SubagentStatus::Blocked => question,
            SubagentStatus::Completed | SubagentStatus::Incomplete | SubagentStatus::Error => None,
        };

        Self {
            status,
            summary,
            error: None,
            question,
            artifacts,
            tokens: None,
            human_intervened: false,
        }
    }

    /// Render the text the parent LLM sees: the summary plus a compact footer
    /// carrying status / artifacts / tokens.
    pub fn to_agent_text(&self) -> String {
        // The parent model reads TEXT, not `structured_content`, so a status it
        // has to act differently on has to be spelled out here or it does not
        // exist. Exhaustive on purpose: a new status decides at this line
        // whether it needs framing instead of falling silently into the plain
        // path, which is how `Blocked` came to be indistinguishable from
        // `Completed` in the first place.
        let framing = match self.status {
            SubagentStatus::Blocked => Some((BLOCKED_HEADER, BLOCKED_DIRECTIVE)),
            SubagentStatus::Completed | SubagentStatus::Incomplete | SubagentStatus::Error => None,
        };

        let mut out = String::new();
        if let Some((header, _)) = framing {
            out.push_str(header);
            out.push_str("\n\n");
        }
        out.push_str(&self.summary);
        // BR-71 §4.5: said only when it happened. A "human_intervened: false"
        // line would read to the parent model as an assertion that someone
        // checked, when in fact nothing was observed either way.
        if self.human_intervened {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str("{\"human_intervened\":true}\n");
            out.push_str(
                "Note: the user intervened directly in this subagent's tab during the run.",
            );
        }
        let footer = self.footer_line();
        if !footer.is_empty() {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str(&footer);
        }
        if let Some((_, directive)) = framing {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str(directive);
        }
        out
    }

    fn footer_line(&self) -> String {
        let mut parts = vec![format!("subagent {}", self.status.as_str())];
        if !self.artifacts.is_empty() {
            let noun = if self.artifacts.len() == 1 {
                "file"
            } else {
                "files"
            };
            parts.push(format!("{} {}", self.artifacts.len(), noun));
        }
        if let Some(tokens) = &self.tokens {
            parts.push(format!("{} tokens", tokens.total));
        }
        // Nothing beyond a bare "completed" status is worth a footer line.
        // Every other status has to announce itself even with nothing else to
        // report, because it changes what the parent should do next.
        let silent_when_bare = match self.status {
            SubagentStatus::Completed => true,
            SubagentStatus::Blocked | SubagentStatus::Incomplete | SubagentStatus::Error => false,
        };
        if parts.len() == 1 && silent_when_bare {
            return String::new();
        }
        format!("[{}]", parts.join(" · "))
    }

    /// Convert into the tool result returned to the parent agent: a text block
    /// for the model plus the full structured envelope for programmatic use.
    pub fn into_call_tool_result(self) -> CallToolResult {
        // `Blocked` is deliberately NOT an error. The child did the right
        // thing, and a tool result flagged `is_error` invites the parent to
        // retry the same spawn, the one response that cannot help, since the
        // second run is blocked on the same unanswered question.
        let is_error = match self.status {
            SubagentStatus::Error => true,
            SubagentStatus::Completed | SubagentStatus::Blocked | SubagentStatus::Incomplete => {
                false
            }
        };
        let text = self.to_agent_text();
        let structured = serde_json::to_value(&self).ok();
        CallToolResult {
            content: vec![Content::text(text)],
            structured_content: structured,
            is_error: Some(is_error),
            meta: None,
        }
    }
}

/// BR-71 §4.5: true when the child's conversation contains any message the
/// human injected directly through the subagent tab. The parent weighs the
/// summary accordingly.
pub fn conversation_has_user_direct(conversation: &Conversation) -> bool {
    use crate::conversation::message::ProvenanceKind;
    conversation.messages().iter().any(|m| {
        m.metadata
            .provenance
            .as_ref()
            .is_some_and(|p| p.kind == ProvenanceKind::UserDirect)
    })
}

/// Non-whitespace text of the last message, but only if it's the child's own
/// (assistant) message — i.e. the child ended by writing a summary.
fn last_message_text(conversation: &Conversation) -> Option<String> {
    let last = conversation.messages().last()?;
    if last.role != Role::Assistant {
        return None;
    }
    message_text(last)
}

/// Most recent non-whitespace assistant text anywhere in the conversation.
/// Assistant-only so a child that produced nothing doesn't echo the user's task
/// back as its own "recap".
fn last_nonempty_text(conversation: &Conversation) -> Option<String> {
    conversation
        .messages()
        .iter()
        .rev()
        .filter(|m| m.role == Role::Assistant)
        .find_map(message_text)
}

/// First non-whitespace text content of a single message.
fn message_text(message: &Message) -> Option<String> {
    message.content.iter().find_map(|content| match content {
        MessageContent::Text(text) if !text.text.trim().is_empty() => Some(text.text.clone()),
        _ => None,
    })
}

/// All text + tool-result text, concatenated (the `summary=false` path).
///
/// Agent-invisible rows are skipped. `summary: false` means "give the parent the
/// child's transcript", and a row the child's own model was not allowed to see is
/// not part of that transcript:
///
/// * BR-71 Task 32 makes the child's first stored message its entire rendered
///   spawn context (`agent_visible: false`). A child that compacts mid-run gets
///   `AgentEvent::HistoryReplaced`, which swaps the handler's local conversation
///   for the stored one — so without this filter the child's whole system prompt
///   would be concatenated into the parent's tool result.
/// * Compaction's hidden originals are already represented by the summary that
///   replaced them, so carrying both duplicated the transcript on top of the
///   bloat. That leak predates BR-71; this filter closes it too.
///
/// The empty case is safe: `from_conversation` falls back to `describe_activity`
/// when the concatenation is blank.
fn concatenate_text(conversation: &Conversation) -> String {
    let parts: Vec<String> = conversation
        .iter()
        .filter(|message| message.is_agent_visible())
        .flat_map(|message| {
            message.content.iter().filter_map(|content| match content {
                MessageContent::Text(text) => Some(text.text.clone()),
                MessageContent::ToolResponse(tool_response) => {
                    let result = tool_response.tool_result.as_ref().ok()?;
                    let texts: Vec<String> = result
                        .content
                        .iter()
                        .filter_map(|c| match &c.raw {
                            rmcp::model::RawContent::Text(raw) => Some(raw.text.clone()),
                            _ => None,
                        })
                        .collect();
                    if texts.is_empty() {
                        None
                    } else {
                        Some(format!("Tool result: {}", texts.join("\n")))
                    }
                }
                _ => None,
            })
        })
        .collect();
    parts.join("\n")
}

/// Describe a child's activity when it produced no text at all — so an empty
/// run still reports something concrete instead of a placeholder.
fn describe_activity(conversation: &Conversation) -> String {
    let mut steps = 0usize;
    let mut last_tool: Option<String> = None;
    for message in conversation.messages() {
        for content in &message.content {
            if let MessageContent::ToolRequest(req) = content {
                steps += 1;
                if let Ok(call) = &req.tool_call {
                    last_tool = Some(call.name.to_string());
                }
            }
        }
    }
    match last_tool {
        Some(name) => format!(
            "Subagent produced no final text; it made {steps} tool call(s) and ended on `{name}`."
        ),
        None => "Subagent produced no text output.".to_string(),
    }
}

/// Best-effort list of files the child created/modified, from its tool calls.
/// Conservative on purpose (mirrors the desktop artifact detector): only known
/// file-writing tool shapes count, so reads/views don't pollute the list.
fn collect_artifacts(conversation: &Conversation) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for message in conversation.messages() {
        for content in &message.content {
            if let MessageContent::ToolRequest(req) = content {
                if let Ok(call) = &req.tool_call {
                    if let Some(path) = artifact_path_from_call(&call.name, call.arguments.as_ref())
                    {
                        if !seen.contains(&path) {
                            seen.push(path);
                        }
                    }
                }
            }
        }
    }
    seen
}

fn artifact_path_from_call(
    name: &str,
    arguments: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Option<String> {
    let args = arguments?;
    // Tool names may be extension-prefixed (e.g. "developer__text_editor").
    let base = name.rsplit("__").next().unwrap_or(name);
    match base {
        "text_editor" => {
            let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
            if matches!(
                command,
                "write" | "create" | "str_replace" | "insert" | "diff"
            ) {
                args.get("path")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            } else {
                None
            }
        }
        "write_file" | "create_file" | "edit_file" => args
            .get("path")
            .or_else(|| args.get("file_path"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::Message;
    use rmcp::model::CallToolRequestParams;
    use serde_json::json;

    fn tool_call(name: &str, args: serde_json::Value) -> CallToolRequestParams {
        CallToolRequestParams {
            name: name.to_string().into(),
            arguments: args.as_object().cloned(),
            meta: None,
            task: None,
        }
    }

    fn conv(messages: Vec<Message>) -> Conversation {
        Conversation::new_unvalidated(messages)
    }

    #[test]
    fn completed_when_last_message_has_text() {
        let c = conv(vec![
            Message::user().with_text("do it"),
            Message::assistant().with_text("Here is the summary of my work."),
        ]);
        let r = SubagentResult::from_conversation(&c, None, true);
        assert_eq!(r.status, SubagentStatus::Completed);
        assert_eq!(r.summary, "Here is the summary of my work.");
    }

    #[test]
    fn salvages_earlier_text_when_child_ends_on_tool_call() {
        let c = conv(vec![
            Message::user().with_text("do it"),
            Message::assistant().with_text("Progress: found the answer is 42."),
            Message::assistant()
                .with_tool_request("t1", Ok(tool_call("shell", json!({"command": "ls"})))),
        ]);
        let r = SubagentResult::from_conversation(&c, None, true);
        assert_eq!(r.status, SubagentStatus::Incomplete);
        assert!(r.summary.contains("found the answer is 42"));
        // Must never surface the old lossy placeholder.
        assert!(!r.summary.contains("No text content in last message"));
    }

    #[test]
    fn describes_activity_when_no_text_at_all() {
        let c = conv(vec![
            Message::user().with_text("do it"),
            Message::assistant()
                .with_tool_request("t1", Ok(tool_call("shell", json!({"command": "ls"})))),
        ]);
        let r = SubagentResult::from_conversation(&c, None, true);
        assert_eq!(r.status, SubagentStatus::Incomplete);
        assert!(r.summary.contains("`shell`"));
        assert!(!r.summary.contains("No text content in last message"));
    }

    #[test]
    fn final_output_is_completed() {
        let c = conv(vec![Message::user().with_text("do it")]);
        let r = SubagentResult::from_conversation(&c, Some("structured answer".into()), true);
        assert_eq!(r.status, SubagentStatus::Completed);
        assert_eq!(r.summary, "structured answer");
    }

    #[test]
    fn collects_write_artifacts_but_not_reads() {
        let c = conv(vec![
            Message::assistant().with_tool_request(
                "t1",
                Ok(tool_call(
                    "developer__text_editor",
                    json!({"command": "write", "path": "/tmp/out.py", "file_text": "x=1"}),
                )),
            ),
            Message::assistant().with_tool_request(
                "t2",
                Ok(tool_call(
                    "developer__text_editor",
                    json!({"command": "view", "path": "/tmp/other.py"}),
                )),
            ),
            Message::assistant().with_tool_request(
                "t3",
                Ok(tool_call("write_file", json!({"path": "/tmp/report.md"}))),
            ),
            Message::assistant().with_text("done"),
        ]);
        let r = SubagentResult::from_conversation(&c, None, true);
        assert_eq!(r.artifacts, vec!["/tmp/out.py", "/tmp/report.md"]);
    }

    #[test]
    fn concatenate_path_joins_all_text() {
        let c = conv(vec![
            Message::assistant().with_text("first"),
            Message::assistant().with_text("second"),
        ]);
        let r = SubagentResult::from_conversation(&c, None, false);
        assert_eq!(r.status, SubagentStatus::Completed);
        assert!(r.summary.contains("first"));
        assert!(r.summary.contains("second"));
    }

    /// BR-71 Task 32: the child's first stored message is now its whole
    /// rendered spawn context, `agent_visible: false`. If the child compacts
    /// mid-run, `AgentEvent::HistoryReplaced` swaps the handler's local
    /// conversation for the STORED one — which contains that record — and the
    /// `summary: false` path concatenates every message. Without a visibility
    /// filter the parent's tool result would carry the child's entire system
    /// prompt. Same rule for the agent-invisible originals a compaction leaves
    /// behind: they are already represented by the summary that replaced them,
    /// so including both duplicates the transcript as well as bloating it.
    #[test]
    fn concatenate_path_skips_agent_invisible_rows() {
        let hidden = Message::user()
            .with_text("## Subagent spawn context\n### Rendered system prompt\nSECRET PROMPT")
            .with_metadata(
                crate::conversation::message::MessageMetadata::default().with_agent_invisible(),
            );
        let c = conv(vec![
            hidden,
            Message::assistant().with_text("first"),
            Message::assistant().with_text("second"),
        ]);
        let r = SubagentResult::from_conversation(&c, None, false);
        assert_eq!(r.status, SubagentStatus::Completed);
        assert!(r.summary.contains("first"));
        assert!(r.summary.contains("second"));
        assert!(
            !r.summary.contains("SECRET PROMPT"),
            "the child's spawn context must not ride back to the parent's model: {}",
            r.summary
        );
    }

    /// Flatten prose before asserting on it, so re-flowing a paragraph cannot
    /// break a test and train someone to loosen the assertion.
    fn flatten_prose(s: &str) -> String {
        s.to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// **The core of the fix.** A child that stopped to ask must not be filed as
    /// `Completed`.
    ///
    /// Measured before this variant existed: given "fix it and make sure it's
    /// consistent with the other one", the child stopped, named both candidates
    /// and asked which was meant, which was exactly right, and the envelope
    /// reported `completed`. The parent read a finished delegation with no edit
    /// behind it and rewrote both files itself, three runs out of three.
    #[test]
    fn a_child_that_stopped_to_ask_is_not_reported_as_completed() {
        let c = conv(vec![
            Message::user().with_text("fix it and make sure it's consistent with the other one"),
            Message::assistant().with_text(
                "BLOCKED: which file did you mean by \"it\"?\n\n\
                 I found two candidates and changed neither: `a/config.rs` and `b/config.rs`.",
            ),
        ]);
        let r = SubagentResult::from_conversation(&c, None, true);

        assert_eq!(
            r.status,
            SubagentStatus::Blocked,
            "a returned question is neither a completed task nor an error: {r:?}"
        );
        assert_ne!(r.status, SubagentStatus::Completed);
        assert_eq!(
            r.question.as_deref(),
            Some("which file did you mean by \"it\"?"),
            "the question is lifted out so a caller can hand it straight to the user"
        );
        // The candidates and the work done survive in the summary.
        assert!(r.summary.contains("a/config.rs") && r.summary.contains("b/config.rs"));

        let call = r.into_call_tool_result();
        let structured = call.structured_content.expect("structured envelope");
        assert_eq!(structured["status"], "blocked");
        // Stopping to ask is not a failure. `is_error: true` here would invite
        // the parent to retry the identical spawn, which is blocked on the same
        // unanswered question.
        assert_eq!(call.is_error, Some(false));
    }

    /// The parent model reads TEXT, never `structured_content`, so the whole
    /// fix is worth nothing unless the rendered result says both what happened
    /// and what to do about it.
    #[test]
    fn a_blocked_result_tells_the_parent_to_ask_rather_than_act() {
        let c =
            conv(vec![Message::assistant().with_text(
                "BLOCKED: which of the two config files did you mean?",
            )]);
        let result = SubagentResult::from_conversation(&c, None, true);
        let text = flatten_prose(&result.to_agent_text());

        // What happened, stated ABOVE the child's own report: a parent reads
        // that report as an account of finished work.
        assert!(
            text.starts_with("the subagent stopped and changed nothing"),
            "the outcome must lead, not trail: {text}"
        );
        // The child's question survives into the text.
        assert!(text.contains("which of the two config files did you mean?"));
        // The status is legible on its own.
        assert!(text.contains("[subagent blocked]"), "{text}");
        // What to do: settle it if you can, otherwise ask the user.
        assert!(
            text.contains("put the question to the user"),
            "the parent must be told where an unanswerable question goes: {text}"
        );
        assert!(
            text.contains("delegate the task again with the answer written out in full"),
            "the cheap path (parent can settle it) must be named first: {text}"
        );
        // And the three things it actually did instead, each named.
        assert!(
            text.contains("do not pick the most likely candidate"),
            "{text}"
        );
        assert!(
            text.contains("do not delegate again with a guess"),
            "{text}"
        );
        assert!(
            text.contains("do not do the work yourself to find out which one was meant"),
            "this is the measured failure: the parent overrode the child and edited both files: \
             {text}"
        );
    }

    /// The marker is a signal a language model emits, so it arrives dressed up.
    /// Missing it restores the defect in full, so the parser strips ornament
    /// rather than insisting on one spelling.
    #[test]
    fn the_blocked_marker_survives_the_decoration_a_model_adds() {
        for opening in [
            "BLOCKED: which one?",
            "**BLOCKED:** which one?",
            "**BLOCKED**: which one?",
            "## BLOCKED: which one?",
            "> BLOCKED - which one?",
            "`BLOCKED`: which one?",
            "blocked: which one?",
            "BLOCKED \u{2014} which one?",
            "BLOCKED\n\nwhich one?",
        ] {
            let c = conv(vec![Message::assistant().with_text(opening)]);
            let r = SubagentResult::from_conversation(&c, None, true);
            assert_eq!(
                r.status,
                SubagentStatus::Blocked,
                "opening {opening:?} must be recognised as a returned question"
            );
            assert_eq!(r.question.as_deref(), Some("which one?"), "{opening:?}");
        }
    }

    /// The other direction, and the reason the marker is anchored to the first
    /// line: an ordinary report that happens to use the word must stay
    /// `Completed`, or a finished delegation turns into a question to the user.
    #[test]
    fn prose_that_merely_mentions_being_blocked_is_still_completed() {
        for summary in [
            "Done. The deploy is blocked on CI approval, which is expected.",
            "Done — the child reached terminal, returned exactly `CHILD_ORIGINAL`, and its \
             transcript shows no tool calls.",
            "I fixed `a/config.rs`.\n\nBLOCKED: is this a note you wanted?",
            "blockedTests failed, so I skipped them.",
        ] {
            let c = conv(vec![Message::assistant().with_text(summary)]);
            let r = SubagentResult::from_conversation(&c, None, true);
            assert_eq!(
                r.status,
                SubagentStatus::Completed,
                "{summary:?} is a report of work, not a returned question"
            );
            assert_eq!(r.question, None);
        }
    }

    /// `summary: false` hands the parent the whole transcript, which OPENS with
    /// the child's first words, so the marker cannot be read off `summary` and
    /// is read off the closing message instead. Without this the blocked status
    /// silently vanishes on one of the two summary modes.
    #[test]
    fn blocked_is_detected_in_whole_transcript_mode_too() {
        let c = conv(vec![
            Message::assistant().with_text("Looking for the file now."),
            Message::assistant().with_text("BLOCKED: which config file did you mean?"),
        ]);
        let r = SubagentResult::from_conversation(&c, None, false);
        assert_eq!(r.status, SubagentStatus::Blocked);
        assert_eq!(
            r.question.as_deref(),
            Some("which config file did you mean?")
        );
        assert!(r.summary.contains("Looking for the file now."));
    }

    /// A run that never produced a usable result stays an error, and one that
    /// died mid-turn stays an error, whatever prose it left behind. `Blocked`
    /// only ever promotes a run that would otherwise have been called finished.
    #[test]
    fn blocked_never_masks_a_failure() {
        let aborted = SubagentResult::from_aborted_turn(
            &conv(vec![Message::assistant().with_text("BLOCKED: which one?")]),
            "provider_error",
            "the model hung up",
        );
        assert_eq!(aborted.status, SubagentStatus::Error);
        assert_eq!(aborted.question, None);

        assert_eq!(
            SubagentResult::from_error("boom").status,
            SubagentStatus::Error
        );

        // …and a child that ended on a tool call is still Incomplete: the
        // parent already knows not to trust that one.
        let c = conv(vec![
            Message::assistant().with_text("BLOCKED: which one?"),
            Message::assistant()
                .with_tool_request("t1", Ok(tool_call("shell", json!({"command": "ls"})))),
        ]);
        let r = SubagentResult::from_conversation(&c, None, true);
        assert_eq!(r.status, SubagentStatus::Incomplete);
        assert_eq!(r.question, None);
    }

    #[test]
    fn cancellation_preserves_work_but_never_reports_completed_or_error() {
        let mut result = SubagentResult::from_conversation(
            &conv(vec![Message::assistant().with_text("partial work")]),
            None,
            true,
        );
        result.artifacts.push("partial.txt".into());
        result.mark_cancelled();

        assert_eq!(result.status, SubagentStatus::Incomplete);
        assert!(result.error.is_none());
        assert!(result.question.is_none());
        assert!(result.summary.contains("cancelled before completion"));
        assert!(result.summary.contains("partial work"));
        assert_eq!(result.artifacts, vec!["partial.txt"]);
    }

    /// `as_str` is the name the parent reads in a handle listing; the serde
    /// rename is the name a programmatic consumer reads. A variant whose two
    /// names disagree is a variant one of the two audiences cannot match on.
    #[test]
    fn every_status_renders_the_same_name_on_both_channels() {
        for status in [
            SubagentStatus::Completed,
            SubagentStatus::Blocked,
            SubagentStatus::Incomplete,
            SubagentStatus::Error,
        ] {
            let serialized = serde_json::to_value(status).expect("status serializes");
            assert_eq!(
                serialized.as_str(),
                Some(status.as_str()),
                "{status:?} renders differently on the two channels"
            );
        }
    }

    #[test]
    fn error_result_flags_is_error_and_structured_content() {
        let r = SubagentResult::from_error("provider blew up");
        assert_eq!(r.status, SubagentStatus::Error);
        assert_eq!(r.error.as_deref(), Some("provider blew up"));
        let call = r.into_call_tool_result();
        assert_eq!(call.is_error, Some(true));
        let structured = call.structured_content.expect("structured content present");
        assert_eq!(structured["status"], "error");
        assert_eq!(structured["error"], "provider blew up");
    }

    #[test]
    fn footer_reports_tokens_and_artifacts() {
        let mut r = SubagentResult::from_conversation(
            &conv(vec![Message::assistant().with_text("clean summary")]),
            None,
            true,
        );
        r.tokens = Some(SubagentTokens {
            total: 1234,
            input: 1000,
            output: 234,
        });
        let text = r.to_agent_text();
        assert!(text.starts_with("clean summary"));
        assert!(text.contains("subagent completed"));
        assert!(text.contains("1234 tokens"));
    }

    #[test]
    fn clean_completed_summary_has_no_footer_noise() {
        let r = SubagentResult::from_conversation(
            &conv(vec![Message::assistant().with_text("just the summary")]),
            None,
            true,
        );
        // No tokens, no artifacts, completed -> bare summary, no footer.
        assert_eq!(r.to_agent_text(), "just the summary");
    }

    #[test]
    fn human_intervention_is_detected_from_provenance() {
        use crate::conversation::message::{Message, MessageProvenance, ProvenanceKind};
        let clean = Conversation::new_unvalidated(vec![Message::user().with_text("task")]);
        assert!(!conversation_has_user_direct(&clean));

        let steered = Conversation::new_unvalidated(vec![
            Message::user().with_text("task"),
            Message::user()
                .with_text("actually, stop and use Python")
                .with_provenance(MessageProvenance {
                    kind: ProvenanceKind::UserDirect,
                    from_session_id: None,
                    from_session_name: None,
                }),
        ]);
        assert!(conversation_has_user_direct(&steered));

        // Another AGENT's injection is not a human intervention. Without this
        // case, `provenance.is_some()` passes the test above and turns every
        // `workspace_send_prompt` into a reported human steer.
        let agent_injected = Conversation::new_unvalidated(vec![Message::user()
            .with_text("from the parent")
            .with_provenance(MessageProvenance {
                kind: ProvenanceKind::AgentInjection,
                from_session_id: Some("s-parent".into()),
                from_session_name: Some("Planning chat".into()),
            })]);
        assert!(!conversation_has_user_direct(&agent_injected));
    }

    /// The flag has to reach the PARENT, which means it has to survive
    /// `into_call_tool_result` — the only thing the parent model ever reads.
    /// Nothing else in this task looks at that boundary: `conversation_has_user_direct`
    /// is pure and its assignment in `run_complete_subagent_task` is a one-liner
    /// with no test, so a field that is set correctly and then dropped on
    /// serialization is invisible until Task 40's live pass.
    #[test]
    fn human_intervened_reaches_the_parent_through_the_tool_result() {
        let mut result = SubagentResult::from_error("boom");
        result.human_intervened = true;
        let rendered = result.into_call_tool_result();

        let structured = rendered.structured_content.expect("structured envelope");
        assert_eq!(structured["human_intervened"], true);

        let text: String = rendered
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect();
        assert!(
            text.contains("{\"human_intervened\":true}"),
            "the model reads TEXT, not structured_content, so the machine marker must be there: {text}"
        );

        // …and a clean run says nothing, rather than "human_intervened: false",
        // which a model would read as an assertion that it checked.
        let clean = SubagentResult::from_error("boom").into_call_tool_result();
        let clean_structured = clean.structured_content.expect("structured envelope");
        assert!(
            clean_structured.get("human_intervened").is_none(),
            "skip_serializing_if keeps a false flag out of the envelope entirely"
        );
        let clean_text: String = clean
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect();
        assert!(
            !clean_text.to_lowercase().contains("intervened"),
            "{clean_text}"
        );
    }
}
