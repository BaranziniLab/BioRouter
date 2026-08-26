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
        tools: &[Tool],
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

        if system.contains("You are answering a question against a")
            && tools.iter().any(|tool| tool.name.as_ref() == "kb_search")
        {
            return query_reply(messages);
        }

        Ok(text_reply("Knowledge test mode completed successfully."))
    }
}

fn query_reply(messages: &[LlmMessage]) -> Result<LlmReply> {
    if let Some(content) = latest_tool_result(messages, "kb_read_page") {
        let page = serde_json::from_str::<serde_json::Value>(content)?;
        let path = page
            .get("path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("the selected knowledge page");
        let content = page
            .get("content")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("No readable page content was returned.");
        return Ok(text_reply(&format!(
            "Knowledge test mode found evidence in [the selected page](/{path}):\n\n{}",
            excerpt_from_markdown(content)
        )));
    }

    if let Some(content) = latest_tool_result(messages, "kb_search") {
        let hits = serde_json::from_str::<Vec<serde_json::Value>>(content)?;
        let Some(path) = hits
            .first()
            .and_then(|hit| hit.get("path"))
            .and_then(serde_json::Value::as_str)
        else {
            return Ok(text_reply(
                "Knowledge test mode found no matching knowledge pages.",
            ));
        };
        return Ok(tool_call_reply("kb_read_page", json!({ "path": path })));
    }

    let query = messages
        .iter()
        .rev()
        .find_map(|message| match message {
            LlmMessage::User(text) => Some(text.trim()),
            _ => None,
        })
        .filter(|query| !query.is_empty())
        .unwrap_or("knowledge test query");
    Ok(tool_call_reply(
        "kb_search",
        json!({ "query": query, "limit": 5 }),
    ))
}

fn latest_tool_result<'a>(messages: &'a [LlmMessage], expected_name: &str) -> Option<&'a str> {
    messages.iter().rev().find_map(|message| match message {
        LlmMessage::ToolResults(parts) => parts
            .iter()
            .rev()
            .find(|part| part.name == expected_name)
            .map(|part| part.content.as_str()),
        LlmMessage::ToolResult { name, content, .. } if name == expected_name => {
            Some(content.as_str())
        }
        _ => None,
    })
}

fn ingest_reply(source_id: &str, messages: &[LlmMessage]) -> Result<LlmReply> {
    if let Some(raw_markdown) = latest_raw_markdown(messages, source_id) {
        let biookf_source = biookf_source_identifier(messages);
        let page_path = curated_page_path(source_id, biookf_source.is_some());
        if page_already_written(messages, &page_path) {
            return Ok(text_reply(&format!("Integrated {source_id}.")));
        }

        let title =
            title_from_markdown(&raw_markdown).unwrap_or_else(|| title_from_source_id(source_id));
        let content = match biookf_source {
            Some(source_identifier) => {
                render_biookf_page(source_id, &title, &source_identifier, &raw_markdown)?
            }
            None => render_okf_page(source_id, &title, &raw_markdown)?,
        };
        return Ok(tool_call_reply(
            "kb_write_page",
            json!({
                "path": page_path,
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

fn page_already_written(messages: &[LlmMessage], expected_path: &str) -> bool {
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

fn biookf_source_identifier(messages: &[LlmMessage]) -> Option<String> {
    const MARKER: &str = " with identifier: ";
    messages.iter().find_map(|message| {
        let LlmMessage::User(text) = message else {
            return None;
        };
        text.lines().find_map(|line| {
            let (_, identifier) = line.split_once(MARKER)?;
            let identifier = identifier.trim();
            (!identifier.is_empty()).then(|| identifier.to_string())
        })
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

fn render_okf_page(source_id: &str, title: &str, markdown: &str) -> Result<String> {
    let excerpt = excerpt_from_markdown(markdown);
    let mut frontmatter = serde_yaml::Mapping::new();
    frontmatter.insert("type".into(), "Source".into());
    frontmatter.insert("identifier".into(), title.into());
    render_page(
        frontmatter,
        &format!(
            "# {title}\n\nImported from raw source `{source_id}` in knowledge test mode.\n\n## Extracted notes\n\n{excerpt}"
        ),
    )
}

fn render_biookf_page(
    source_id: &str,
    title: &str,
    source_identifier: &str,
    markdown: &str,
) -> Result<String> {
    let identifier = format!("{title} finding");
    let mut edge = serde_yaml::Mapping::new();
    edge.insert("predicate".into(), "reported_in".into());
    edge.insert("object".into(), source_identifier.into());
    edge.insert("knowledge_level".into(), "knowledge_assertion".into());
    edge.insert("agent_type".into(), "automated_agent".into());
    edge.insert("primary_source".into(), source_identifier.into());

    let mut frontmatter = serde_yaml::Mapping::new();
    frontmatter.insert("type".into(), "Concept".into());
    frontmatter.insert("identifier".into(), identifier.as_str().into());
    frontmatter.insert(
        "edges".into(),
        serde_yaml::Value::Sequence(vec![serde_yaml::Value::Mapping(edge)]),
    );

    render_page(
        frontmatter,
        &format!(
            "# {identifier}\n\nCurated from `{source_identifier}` (`raw/{source_id}/source.md`) in knowledge test mode.\n\n## Extracted notes\n\n{}",
            excerpt_from_markdown(markdown)
        ),
    )
}

fn render_page(frontmatter: serde_yaml::Mapping, body: &str) -> Result<String> {
    let yaml = serde_yaml::to_string(&serde_yaml::Value::Mapping(frontmatter))?;
    Ok(format!(
        "---\n{yaml}---\n\n{}\n",
        body.trim_end_matches('\n')
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

fn curated_page_path(source_id: &str, biookf: bool) -> String {
    if biookf {
        format!("knowledge/concept/{source_id}-finding.md")
    } else {
        format!("knowledge/source/{source_id}.md")
    }
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
                    // Deliberately NOT a `valid_page`. This is the body of a
                    // `raw/` source — whatever the converter produced from a
                    // PDF or a paste — and `write_raw` stores it verbatim, with
                    // no frontmatter of any kind. Giving the digest's *input*
                    // the format of its output is the one change that would
                    // stop this test saying anything: it asserts a conformant
                    // page is built from unstructured markdown.
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
            Some("knowledge/source/alpha.md")
        );
        let content = reply.tool_calls[0].args["content"].as_str().unwrap();
        assert!(content.contains("type: Source"));
        assert!(content.contains("identifier: Example page"));
        assert!(!content.contains("kind:"));
    }

    #[test]
    fn biookf_writes_a_provenance_linked_concept_without_duplicating_the_source_node() {
        let messages = vec![
            LlmMessage::User(
                "New source to integrate: source-id=alpha. Focus hints:\n\n\
                 The source node for it already exists at knowledge/publication/alpha.md with \
                 identifier: Example paper"
                    .into(),
            ),
            LlmMessage::ToolResults(vec![ToolResultPart {
                request_id: "test".into(),
                name: "kb_read_page".into(),
                content: json!({
                    "path": "raw/alpha/source.md",
                    "content": "# Example finding\n\nUseful details",
                })
                .to_string(),
            }]),
        ];

        let reply = ingest_reply("alpha", &messages).unwrap();
        assert_eq!(
            reply.tool_calls[0].args["path"],
            "knowledge/concept/alpha-finding.md"
        );
        let content = reply.tool_calls[0].args["content"].as_str().unwrap();
        assert!(content.contains("type: Concept"));
        assert!(content.contains("predicate: reported_in"));
        assert!(content.contains("object: Example paper"));
        assert!(content.contains("primary_source: Example paper"));
        assert!(!content.contains("knowledge/source/alpha.md"));
    }

    #[test]
    fn query_searches_then_reads_the_top_hit_before_answering() {
        let initial = vec![LlmMessage::User(
            "What does the evidence say about delegation durability?".into(),
        )];
        let search = query_reply(&initial).unwrap();
        assert_eq!(search.tool_calls[0].name, "kb_search");
        assert_eq!(
            search.tool_calls[0].args["query"],
            "What does the evidence say about delegation durability?"
        );

        let with_search_result = vec![
            initial[0].clone(),
            LlmMessage::Assistant(search),
            LlmMessage::ToolResults(vec![ToolResultPart {
                request_id: "test-kb_search".into(),
                name: "kb_search".into(),
                content: json!([{
                    "kb_id": "roundtrip-okf",
                    "path": "knowledge/source/delegation.md",
                    "score": 3.5,
                    "snippet": "Delegated work remains durable."
                }])
                .to_string(),
            }]),
        ];
        let read = query_reply(&with_search_result).unwrap();
        assert_eq!(read.tool_calls[0].name, "kb_read_page");
        assert_eq!(
            read.tool_calls[0].args["path"],
            "knowledge/source/delegation.md"
        );

        let with_page = vec![
            with_search_result[0].clone(),
            LlmMessage::Assistant(read),
            LlmMessage::ToolResults(vec![ToolResultPart {
                request_id: "test-kb_read_page".into(),
                name: "kb_read_page".into(),
                content: json!({
                    "path": "knowledge/source/delegation.md",
                    "content": "# Delegation durability\n\nA restarted parent can collect its child."
                })
                .to_string(),
            }]),
        ];
        let answer = query_reply(&with_page).unwrap();
        assert!(answer.tool_calls.is_empty());
        assert!(answer.text.contains("knowledge/source/delegation.md"));
        assert!(answer.text.contains("restarted parent can collect"));
    }

    #[test]
    fn query_reports_no_matches_without_fabricating_a_page_read() {
        let messages = vec![LlmMessage::ToolResults(vec![ToolResultPart {
            request_id: "test-kb_search".into(),
            name: "kb_search".into(),
            content: "[]".into(),
        }])];

        let answer = query_reply(&messages).unwrap();
        assert!(answer.tool_calls.is_empty());
        assert!(answer.text.contains("no matching knowledge pages"));
    }
}
