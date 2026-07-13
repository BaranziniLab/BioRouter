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
    fn as_str(self) -> &'static str {
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
        }
    }

    /// Render the text the parent LLM sees: the summary plus a compact footer
    /// carrying status / artifacts / tokens.
    pub fn to_agent_text(&self) -> String {
        let mut out = self.summary.clone();
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
fn concatenate_text(conversation: &Conversation) -> String {
    let parts: Vec<String> = conversation
        .iter()
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
}
