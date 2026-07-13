//! Full-text-search helpers for chat recall (BR-17, Phase 1).
//!
//! Turns a user's free-text query into a safe FTS5 `MATCH` expression and
//! flattens message content into the plain text that is both indexed and
//! displayed, so the index and the on-screen recall snippet can never drift.

use crate::conversation::message::MessageContent;

/// Build a safe FTS5 `MATCH` expression from a free-text user query.
///
/// Every whitespace-separated token becomes a quoted prefix term (`"token"*`)
/// and the terms are OR-joined, so a query matches any of its words and
/// tolerates partial words. Wrapping each token in double quotes makes FTS5
/// treat it as a literal phrase, which neutralises the FTS5 query operators
/// (`AND` / `OR` / `NOT` / `NEAR` / `*` / `"` / `(` / `)` / `:` / `-`) a user
/// might type — an unsanitised query is otherwise parsed as an expression and
/// can raise a syntax error. Any embedded double quote is escaped by doubling.
/// Tokens with no alphanumeric character are dropped (a lone operator would
/// tokenise to nothing and error). Returns an empty string when nothing
/// searchable remains, which the caller treats as "no results".
pub fn sanitize_fts_query(user_query: &str) -> String {
    user_query
        .split_whitespace()
        .filter(|tok| tok.chars().any(|c| c.is_alphanumeric()))
        .map(|tok| format!("\"{}\"*", tok.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" OR ")
}

/// Flatten message content into the searchable text parts used both for
/// indexing and for the displayed recall snippet. Non-text content (tool
/// calls, responses, thinking) is rendered as a short placeholder so a session
/// that only exchanged tool traffic is still discoverable.
pub fn searchable_parts(content: &[MessageContent]) -> Vec<String> {
    content
        .iter()
        .filter_map(|content| match content {
            MessageContent::Text(tc) => Some(tc.text.clone()),
            MessageContent::ToolRequest(tr) => Some(format!("[Tool: {}]", tr.to_readable_string())),
            MessageContent::ToolResponse(_) => Some("[Tool Response]".to_string()),
            MessageContent::Thinking(t) => Some(format!("[Thinking: {}]", t.thinking)),
            _ => None,
        })
        .collect()
}

/// The joined plain text of a message, as stored in the FTS index.
pub fn extract_searchable_text(content: &[MessageContent]) -> String {
    searchable_parts(content).join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::MessageContent;

    #[test]
    fn sanitize_builds_prefix_or_terms() {
        assert_eq!(
            sanitize_fts_query("heart disease"),
            "\"heart\"* OR \"disease\"*"
        );
        assert_eq!(sanitize_fts_query("mitochondria"), "\"mitochondria\"*");
    }

    #[test]
    fn sanitize_neutralises_operators() {
        // AND/OR/NEAR/NOT are quoted, so they are searched literally rather
        // than parsed as FTS operators.
        assert_eq!(
            sanitize_fts_query("foo AND bar"),
            "\"foo\"* OR \"AND\"* OR \"bar\"*"
        );
        // Stray operator characters and quotes can't break out of the phrase.
        assert_eq!(sanitize_fts_query("a\"b* (c)"), "\"a\"\"b*\"* OR \"(c)\"*");
    }

    #[test]
    fn sanitize_drops_nonalpha_and_empty() {
        assert_eq!(sanitize_fts_query(""), "");
        assert_eq!(sanitize_fts_query("   "), "");
        assert_eq!(sanitize_fts_query("* -- \""), "");
    }

    #[test]
    fn searchable_text_joins_parts() {
        let content = vec![
            MessageContent::text("hello world"),
            MessageContent::text("second line"),
        ];
        assert_eq!(
            extract_searchable_text(&content),
            "hello world\nsecond line"
        );
        assert_eq!(
            searchable_parts(&content).join("\n"),
            extract_searchable_text(&content)
        );
    }
}
