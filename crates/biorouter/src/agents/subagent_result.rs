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
            SubagentStatus::Incomplete => "incomplete",
            SubagentStatus::Error => "error",
        }
    }
}

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
            artifacts: Vec::new(),
            tokens: None,
            human_intervened: false,
        }
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

        // A response schema is an explicit, structured final output — trust it.
        if let Some(output) = final_output {
            return Self {
                status: SubagentStatus::Completed,
                summary: output,
                error: None,
                artifacts,
                tokens: None,
                human_intervened: false,
            };
        }

        let (status, summary) = if return_last_only {
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

        Self {
            status,
            summary,
            error: None,
            artifacts,
            tokens: None,
            human_intervened: false,
        }
    }

    /// Render the text the parent LLM sees: the summary plus a compact footer
    /// carrying status / artifacts / tokens.
    pub fn to_agent_text(&self) -> String {
        let mut out = self.summary.clone();
        // BR-71 §4.5: said only when it happened. A "human_intervened: false"
        // line would read to the parent model as an assertion that someone
        // checked, when in fact nothing was observed either way.
        if self.human_intervened {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
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
        if parts.len() == 1 && self.status == SubagentStatus::Completed {
            return String::new();
        }
        format!("[{}]", parts.join(" · "))
    }

    /// Convert into the tool result returned to the parent agent: a text block
    /// for the model plus the full structured envelope for programmatic use.
    pub fn into_call_tool_result(self) -> CallToolResult {
        let is_error = self.status == SubagentStatus::Error;
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
            text.to_lowercase().contains("intervened"),
            "the model reads TEXT, not structured_content, so the note must be there: {text}"
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
