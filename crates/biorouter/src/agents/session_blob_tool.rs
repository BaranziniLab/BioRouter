//! BR-7: chat-side handler for the `platform__read_session_blob` tool.
//!
//! The retrieval half of externalized tool results. When
//! `BIOROUTER_SESSION_BLOB_LAZY_LOAD` is on, a reloaded conversation keeps the
//! *stub* of an oversized tool result — a size summary, a head/tail preview and
//! a blob handle — and never deserializes the payload. This tool is how the
//! model pulls the parts it actually needs back out of the session store,
//! instead of re-running the tool that produced them.
//!
//! It is deliberately a *slicing* reader, not a dump: a payload that was too big
//! for the conversation is still too big for the conversation. Grep it with
//! `pattern`, page it with `offset`/`limit`, and the output is capped either way.

use rmcp::model::{Content, ErrorCode, ErrorData};
use serde_json::Value;

use super::Agent;
use crate::mcp_utils::ToolResult;
use crate::session::session_manager::{Session, SessionManager};

/// Most lines one call returns. Above this the reply is truncated with an
/// explicit hint to narrow the query.
const MAX_LINES: usize = 500;
/// Hard character cap, so a payload of very long lines (minified JSON, a single
/// base64 line) cannot blow the context through the `MAX_LINES` door.
const MAX_CHARS: usize = 40_000;
/// Lines returned when the caller gives neither `pattern` nor `limit`.
const DEFAULT_LIMIT: usize = 200;

impl Agent {
    pub async fn handle_read_session_blob(
        &self,
        arguments: Value,
        session: &Session,
    ) -> ToolResult<Vec<Content>> {
        let blob_id = arguments
            .get("blob_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    "`blob_id` is required: use the id printed in the stub that replaced the tool result".to_string(),
                    None,
                )
            })?;

        // Scoped to the calling session: a handle from another session (or a
        // hallucinated one) simply does not resolve.
        let payload = SessionManager::instance()
            .get_message_blob(&session.id, blob_id)
            .await
            .map_err(|e| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Could not read stored tool output: {e}"),
                    None,
                )
            })?
            .ok_or_else(|| {
                ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    format!(
                        "No stored tool output with blob id `{blob_id}` in this session. Use the \
                         exact id from the stub; if the result was dropped by compaction it is \
                         gone, so re-run the tool with a narrower query."
                    ),
                    None,
                )
            })?;

        let pattern = arguments
            .get("pattern")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|p| !p.is_empty());
        let offset = arguments
            .get("offset")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .max(1) as usize;
        let limit = arguments
            .get("limit")
            .and_then(Value::as_u64)
            .map(|n| n.max(1) as usize)
            .unwrap_or(DEFAULT_LIMIT)
            .min(MAX_LINES);

        let view = match pattern {
            Some(pattern) => grep(&payload, pattern, limit)?,
            None => slice(&payload, offset, limit),
        };
        Ok(vec![Content::text(view)])
    }
}

/// Matching lines, with their 1-based line numbers so the model can follow up
/// with an `offset`/`limit` read around a hit.
fn grep(payload: &str, pattern: &str, limit: usize) -> ToolResult<String> {
    let re = regex::Regex::new(pattern).map_err(|e| {
        ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            format!("`pattern` is not a valid regular expression: {e}"),
            None,
        )
    })?;

    let total = payload.lines().count();
    let mut matches = 0usize;
    let mut kept = Vec::new();
    for (i, line) in payload.lines().enumerate() {
        if !re.is_match(line) {
            continue;
        }
        matches += 1;
        if kept.len() < limit {
            kept.push(format!("{}: {}", i + 1, line));
        }
    }

    if matches == 0 {
        return Ok(format!(
            "No line of the stored output ({total} lines) matches `{pattern}`."
        ));
    }

    let shown = kept.len();
    let mut out = format!(
        "{matches} of {total} lines match `{pattern}`; showing {shown}.\n(line numbers are 1-based; read around a hit with `offset`/`limit`)\n\n"
    );
    out.push_str(&kept.join("\n"));
    Ok(cap(out, matches > shown))
}

/// A 1-based line range of the payload.
fn slice(payload: &str, offset: usize, limit: usize) -> String {
    let lines: Vec<&str> = payload.lines().collect();
    let total = lines.len();
    if offset > total {
        return format!("The stored output has {total} lines; `offset` {offset} is past the end.");
    }

    let start = offset - 1;
    let end = (start + limit).min(total);
    let more = end < total;
    let mut out = format!(
        "Stored tool output: {total} lines, {bytes} bytes. Showing lines {first}-{last}.\n\n",
        bytes = payload.len(),
        first = offset,
        last = end,
    );
    out.push_str(&lines[start..end].join("\n"));
    cap(out, more)
}

/// Bound the reply and always say so when something was left out, so the model
/// knows to narrow rather than assume it saw everything.
fn cap(mut out: String, more_lines: bool) -> String {
    let truncated = out.chars().count() > MAX_CHARS;
    if truncated {
        out = out.chars().take(MAX_CHARS).collect();
    }
    if truncated || more_lines {
        out.push_str(
            "\n\n[output truncated; narrow it with a more specific `pattern`, or a smaller \
             `offset`/`limit` range]",
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload() -> String {
        (1..=1_000)
            .map(|i| format!("row {i} value={}\n", i * 2))
            .collect()
    }

    #[test]
    fn slice_returns_the_requested_line_range() {
        let out = slice(&payload(), 3, 2);
        assert!(out.contains("Showing lines 3-4"));
        assert!(out.contains("row 3 value=6"));
        assert!(out.contains("row 4 value=8"));
        assert!(!out.contains("row 5 "));
        // More remains, and the model is told so.
        assert!(out.contains("output truncated"));
    }

    #[test]
    fn slice_of_the_whole_payload_is_not_marked_truncated() {
        let out = slice("a\nb\nc", 1, 200);
        assert!(out.contains("Showing lines 1-3"));
        assert!(!out.contains("output truncated"));
    }

    #[test]
    fn slice_past_the_end_says_so_instead_of_panicking() {
        let out = slice("a\nb", 99, 10);
        assert!(out.contains("past the end"));
    }

    #[test]
    fn grep_returns_numbered_matching_lines_only() {
        let out = grep(&payload(), r"^row 7 ", 10).unwrap();
        assert!(out.contains("1 of 1000 lines match"));
        assert!(out.contains("7: row 7 value=14"));
        assert!(!out.contains("row 8 "));
    }

    #[test]
    fn grep_caps_its_hits_and_reports_the_full_count() {
        let out = grep(&payload(), "value=", 5).unwrap();
        assert!(out.contains("1000 of 1000 lines match"));
        assert!(out.contains("showing 5"));
        assert!(out.contains("output truncated"));
    }

    #[test]
    fn grep_with_no_hits_is_not_an_error() {
        let out = grep(&payload(), "nothing-here", 10).unwrap();
        assert!(out.contains("No line of the stored output"));
    }

    #[test]
    fn an_invalid_regex_is_a_parameter_error_not_a_crash() {
        let err = grep(&payload(), "row [", 10).unwrap_err();
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(err.message.contains("not a valid regular expression"));
    }

    /// A payload of a few very long lines must not slip past the line cap and
    /// blow the context — the char cap is the backstop.
    #[test]
    fn very_long_lines_are_capped_by_characters() {
        let wide: String = (0..5)
            .map(|_| format!("{}\n", "x".repeat(50_000)))
            .collect();
        let out = slice(&wide, 1, 5);
        assert!(out.chars().count() <= MAX_CHARS + 200);
        assert!(out.contains("output truncated"));
    }
}
