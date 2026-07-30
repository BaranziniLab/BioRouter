use crate::knowledge::subagent::loop_::{Completer, LlmMessage, LlmReply, LlmToolCall};
use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::Tool;
use serde_json::json;

const SOURCE_ID_MARKER: &str = "source-id=";

/// `BIOROUTER_KNOWLEDGE_TEST_MODE=empty-reply` — every completion comes back
/// with no text and no tool calls.
///
/// That is the shape a provider request that failed or was cut short leaves
/// behind: Google returns a candidate with no `parts` on a MAX_TOKENS or SAFETY
/// stop, and the decoder turns it into a content-free assistant message. It is
/// indistinguishable from "the agent has no more tool calls", which is how a
/// dead request came to be reported as a completed digest (issue #71). This mode
/// is what lets the real HTTP ingest stream be driven through that failure
/// without a live provider.
const MODE_EMPTY_REPLY: &str = "empty-reply";

fn mode() -> Option<String> {
    let value = std::env::var("BIOROUTER_KNOWLEDGE_TEST_MODE").ok()?;
    let value = value.trim().to_ascii_lowercase();
    match value.as_str() {
        "1" | "true" | "yes" | "on" | MODE_EMPTY_REPLY => Some(value),
        _ => None,
    }
}

pub fn env_enabled() -> bool {
    mode().is_some()
}

fn simulate_empty_reply() -> bool {
    mode().as_deref() == Some(MODE_EMPTY_REPLY)
}

pub struct TestModeCompleter;

#[async_trait]
impl Completer for TestModeCompleter {
    async fn complete(
        &self,
        system: &str,
        messages: &[LlmMessage],
        _tools: &[Tool],
    ) -> Result<LlmReply> {
        if simulate_empty_reply() {
            return Ok(text_reply(""));
        }

        if system.contains("connectivity test") {
            return Ok(text_reply("OK"));
        }

        if let Some(source_id) = source_id_from_messages(messages) {
            return ingest_reply(&source_id, messages);
        }

        Ok(text_reply("Knowledge test mode completed successfully."))
    }
}

fn ingest_reply(source_id: &str, messages: &[LlmMessage]) -> Result<LlmReply> {
    if let Some(raw_markdown) = latest_raw_markdown(messages, source_id) {
        if source_page_already_written(messages, source_id) {
            return Ok(text_reply(&format!("Integrated {source_id}.")));
        }

        let title =
            title_from_markdown(&raw_markdown).unwrap_or_else(|| title_from_source_id(source_id));
        let content = render_source_page(source_id, &title, &raw_markdown)?;
        return Ok(tool_call_reply(
            "kb_write_page",
            json!({
                "path": source_page_path(source_id),
                "content": content,
                "commit_message": format!("digest source {source_id}"),
            }),
        ));
    }

    Ok(tool_call_reply(
        "kb_read_page",
        json!({ "path": raw_source_path(source_id) }),
    ))
}

fn source_id_from_messages(messages: &[LlmMessage]) -> Option<String> {
    messages.iter().find_map(|message| match message {
        LlmMessage::User(text) => text
            .split_whitespace()
            .find_map(|part| part.strip_prefix(SOURCE_ID_MARKER))
            .map(|value| {
                value
                    .trim_matches(|ch: char| matches!(ch, ',' | '.' | ';'))
                    .to_string()
            }),
        _ => None,
    })
}

fn latest_raw_markdown(messages: &[LlmMessage], source_id: &str) -> Option<String> {
    let raw_path = raw_source_path(source_id);
    messages.iter().rev().find_map(|message| match message {
        LlmMessage::ToolResults(parts) => parts
            .iter()
            .rev()
            .find_map(|part| page_content_from_tool_result(&part.name, &part.content, &raw_path)),
        LlmMessage::ToolResult { name, content, .. } => {
            page_content_from_tool_result(name, content, &raw_path)
        }
        _ => None,
    })
}

fn page_content_from_tool_result(name: &str, content: &str, raw_path: &str) -> Option<String> {
    if name != "kb_read_page" {
        return None;
    }

    let page = serde_json::from_str::<serde_json::Value>(content).ok()?;
    let path = page.get("path")?.as_str()?;
    if path != raw_path {
        return None;
    }

    page.get("content")?.as_str().map(ToOwned::to_owned)
}

fn source_page_already_written(messages: &[LlmMessage], source_id: &str) -> bool {
    let expected_path = source_page_path(source_id);
    messages.iter().any(|message| match message {
        LlmMessage::Assistant(reply) => reply.tool_calls.iter().any(|call| {
            call.name == "kb_write_page"
                && call
                    .args
                    .get("path")
                    .and_then(|value| value.as_str())
                    .map(|path| path == expected_path)
                    .unwrap_or(false)
        }),
        _ => false,
    })
}

fn title_from_markdown(markdown: &str) -> Option<String> {
    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed == "---" || trimmed.starts_with('|') {
            continue;
        }
        if let Some(title) = trimmed.strip_prefix('#') {
            let normalized = title.trim().trim_matches('#').trim();
            if !normalized.is_empty() {
                return Some(normalized.to_string());
            }
        }
        if trimmed.len() >= 3 {
            return Some(trimmed.chars().take(80).collect());
        }
    }
    None
}

fn title_from_source_id(source_id: &str) -> String {
    source_id
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(capitalize_word)
        .collect::<Vec<_>>()
        .join(" ")
}

fn capitalize_word(word: &str) -> String {
    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    format!(
        "{}{}",
        first.to_ascii_uppercase(),
        chars.as_str().to_ascii_lowercase()
    )
}

fn render_source_page(source_id: &str, title: &str, markdown: &str) -> Result<String> {
    let frontmatter = serde_yaml::to_string(&json!({
        "title": title,
        "kind": "source",
    }))?;
    let excerpt = excerpt_from_markdown(markdown);
    Ok(format!(
        "---\n{frontmatter}---\n\n# {title}\n\nImported from raw source `{source_id}` in knowledge test mode.\n\n## Extracted notes\n\n{excerpt}\n"
    ))
}

fn excerpt_from_markdown(markdown: &str) -> String {
    let mut lines = Vec::new();
    let mut total_chars = 0usize;

    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed == "---" {
            continue;
        }
        total_chars += trimmed.len();
        lines.push(trimmed.to_string());
        if lines.len() >= 10 || total_chars >= 900 {
            break;
        }
    }

    if lines.is_empty() {
        "No readable text was extracted from the source.".to_string()
    } else {
        lines.join("\n\n")
    }
}

fn raw_source_path(source_id: &str) -> String {
    format!("raw/{source_id}/source.md")
}

fn source_page_path(source_id: &str) -> String {
    format!("knowledge/sources/{source_id}.md")
}

fn tool_call_reply(name: &str, args: serde_json::Value) -> LlmReply {
    LlmReply {
        text: String::new(),
        tool_calls: vec![LlmToolCall {
            id: format!("test-{name}"),
            name: name.to_string(),
            args,
        }],
    }
}

fn text_reply(text: &str) -> LlmReply {
    LlmReply {
        text: text.to_string(),
        tool_calls: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::subagent::loop_::ToolResultPart;

    #[test]
    fn extracts_source_id_from_user_message() {
        let messages = vec![LlmMessage::User(
            "New source to integrate: source-id=alpha-123. Focus hints: ".to_string(),
        )];
        assert_eq!(
            source_id_from_messages(&messages).as_deref(),
            Some("alpha-123")
        );
    }

    #[test]
    fn writes_source_page_after_raw_page_is_available() {
        let messages = vec![
            LlmMessage::User("New source to integrate: source-id=alpha. Focus hints: ".into()),
            LlmMessage::ToolResults(vec![ToolResultPart {
                request_id: "test".into(),
                name: "kb_read_page".into(),
                content: json!({
                    "path": "raw/alpha/source.md",
                    "content": "# Example page\n\nUseful details",
                    "frontmatter": null,
                })
                .to_string(),
            }]),
        ];

        let reply = ingest_reply("alpha", &messages).unwrap();
        assert_eq!(reply.tool_calls.len(), 1);
        assert_eq!(reply.tool_calls[0].name, "kb_write_page");
        assert_eq!(
            reply.tool_calls[0]
                .args
                .get("path")
                .and_then(|value| value.as_str()),
            Some("knowledge/sources/alpha.md")
        );
    }
}
