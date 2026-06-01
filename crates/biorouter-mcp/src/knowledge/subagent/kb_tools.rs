//! `KbToolDispatch` — maps tool-name strings to `KnowledgeService` operations.
//!
//! This is the concrete `ToolDispatch` implementation used by the sub-agent
//! macros.  It binds to a specific KB and an optional transaction branch so
//! every write is either committed immediately (no txn) or staged on the txn
//! branch (with txn).

use crate::knowledge::{
    convert::SourceInput,
    log as kb_log,
    paths,
    raw,
    service::KnowledgeService,
    store,
    subagent::loop_::ToolDispatch,
    types::ChangeKind,
};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use rmcp::model::{JsonObject, Tool};
use serde_json::Value;
use std::{borrow::Cow, sync::Arc};

// ---------------------------------------------------------------------------
// KbToolDispatch
// ---------------------------------------------------------------------------

pub struct KbToolDispatch {
    pub svc: KnowledgeService,
    pub kb_id: String,
    /// The currently active transaction branch name, or an empty string when
    /// not inside a transaction (writes then commit directly to main).
    pub txn_branch: String,
}

#[async_trait]
impl ToolDispatch for KbToolDispatch {
    async fn call(&self, name: &str, args: Value) -> Result<String> {
        let kb_root = paths::kb_root(self.svc.root(), &self.kb_id);
        let txn_opt: Option<&str> = if self.txn_branch.is_empty() {
            None
        } else {
            Some(&self.txn_branch)
        };

        match name {
            // ------------------------------------------------------------------
            "kb_list_pages" => {
                let prefix = args
                    .get("path_prefix")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let pages = store::list_pages(&kb_root, prefix.as_deref())?;
                Ok(serde_json::to_string(&pages)?)
            }

            // ------------------------------------------------------------------
            "kb_read_page" => {
                let path = args["path"]
                    .as_str()
                    .ok_or_else(|| anyhow!("kb_read_page: missing 'path'"))?;
                let page = store::read_page(&kb_root, path)?;
                Ok(serde_json::to_string(&page)?)
            }

            // ------------------------------------------------------------------
            "kb_write_page" => {
                let path = args["path"]
                    .as_str()
                    .ok_or_else(|| anyhow!("kb_write_page: missing 'path'"))?;
                let content = args["content"]
                    .as_str()
                    .ok_or_else(|| anyhow!("kb_write_page: missing 'content'"))?;
                let msg = args
                    .get("commit_message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("subagent write");
                let sha = store::write_page(&kb_root, path, content, msg, txn_opt)?;
                Ok(serde_json::json!({ "commit_sha": sha }).to_string())
            }

            // ------------------------------------------------------------------
            "kb_search" => {
                let q = args["query"]
                    .as_str()
                    .ok_or_else(|| anyhow!("kb_search: missing 'query'"))?;
                let limit = args
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(5) as usize;
                let hits = store::search(&kb_root, q, limit)?;
                Ok(serde_json::to_string(&hits)?)
            }

            // ------------------------------------------------------------------
            "kb_append_log" => {
                let summary = args["summary"]
                    .as_str()
                    .ok_or_else(|| anyhow!("kb_append_log: missing 'summary'"))?;
                let kind_str = args.get("kind").and_then(|v| v.as_str()).unwrap_or("manual");
                let kind = parse_change_kind(kind_str)?;
                let delta = args.get("delta").and_then(|v| v.as_str());
                kb_log::append(&kb_root, kind, summary, delta, txn_opt)?;
                Ok(serde_json::json!({ "ok": true }).to_string())
            }

            // ------------------------------------------------------------------
            "kb_add_raw_source" => {
                let input = source_input_from_args(&args)?;
                let result = self.svc.add_raw_source(&self.kb_id, input, txn_opt).await?;
                Ok(serde_json::json!({
                    "source_id": result.source_id,
                    "source_md_path": result.source_md_path,
                }).to_string())
            }

            // ------------------------------------------------------------------
            "kb_classify_source" => {
                let source_id = args["source_id"]
                    .as_str()
                    .ok_or_else(|| anyhow!("kb_classify_source: missing 'source_id'"))?;
                let meta = raw::read_meta(&kb_root, source_id)?;
                Ok(serde_json::to_string(&meta.credibility)?)
            }

            // ------------------------------------------------------------------
            other => anyhow::bail!("KbToolDispatch: unknown tool '{other}'"),
        }
    }
}

fn parse_change_kind(s: &str) -> Result<ChangeKind> {
    Ok(match s {
        "ingest" => ChangeKind::Ingest,
        "link" => ChangeKind::Link,
        "flag" => ChangeKind::Flag,
        "query" => ChangeKind::Query,
        "lint" => ChangeKind::Lint,
        "restore" => ChangeKind::Restore,
        "manual" => ChangeKind::Manual,
        other => anyhow::bail!("invalid ChangeKind: '{other}'"),
    })
}

/// Build a `SourceInput` from a JSON `Value` supplied by the sub-agent.
///
/// Accepted shapes:
/// ```json
/// { "type": "text",  "text": "…",  "title": "optional" }
/// { "type": "url",   "url":  "…" }
/// { "type": "file",  "bytes_b64": "…", "filename": "…", "mime": "optional" }
/// ```
fn source_input_from_args(args: &Value) -> Result<SourceInput> {
    let kind = args
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("text");
    match kind {
        "text" => {
            let text = args["text"]
                .as_str()
                .ok_or_else(|| anyhow!("kb_add_raw_source: missing 'text'"))?
                .to_string();
            let title = args.get("title").and_then(|v| v.as_str()).map(String::from);
            Ok(SourceInput::Text { text, title })
        }
        "url" => {
            let url = args["url"]
                .as_str()
                .ok_or_else(|| anyhow!("kb_add_raw_source: missing 'url'"))?
                .to_string();
            Ok(SourceInput::Url(url))
        }
        "file" => {
            let b64 = args["bytes_b64"]
                .as_str()
                .ok_or_else(|| anyhow!("kb_add_raw_source: missing 'bytes_b64'"))?;
            use base64::Engine as _;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(b64)
                .map_err(|e| anyhow!("kb_add_raw_source: invalid base64: {e}"))?;
            let filename = args["filename"]
                .as_str()
                .ok_or_else(|| anyhow!("kb_add_raw_source: missing 'filename'"))?
                .to_string();
            let mime = args.get("mime").and_then(|v| v.as_str()).map(String::from);
            Ok(SourceInput::File { bytes, filename, mime })
        }
        other => anyhow::bail!("kb_add_raw_source: unknown source type '{other}'"),
    }
}

// ---------------------------------------------------------------------------
// Tool specs for the sub-agent
// ---------------------------------------------------------------------------

/// Build the JSON input schema for a tool (minimal: object with given
/// required + optional properties).
fn make_schema(
    required: &[(&str, &str)],       // (name, type)
    optional: &[(&str, &str)],       // (name, type)
) -> Arc<JsonObject> {
    let mut props = serde_json::Map::new();
    for (n, t) in required.iter().chain(optional.iter()) {
        props.insert(
            n.to_string(),
            serde_json::json!({ "type": t }),
        );
    }
    let req_names: Vec<Value> = required.iter().map(|(n, _)| Value::String(n.to_string())).collect();
    let mut schema = serde_json::Map::new();
    schema.insert("type".into(), Value::String("object".into()));
    schema.insert("properties".into(), Value::Object(props));
    schema.insert("required".into(), Value::Array(req_names));
    Arc::new(schema)
}

/// Returns the `Vec<Tool>` to pass to the sub-agent.
pub fn tool_specs() -> Vec<Tool> {
    vec![
        Tool::new(
            Cow::Borrowed("kb_list_pages"),
            Cow::Borrowed("List knowledge pages. Optional path_prefix filter."),
            make_schema(&[], &[("path_prefix", "string")]),
        ),
        Tool::new(
            Cow::Borrowed("kb_read_page"),
            Cow::Borrowed("Read a knowledge page by logical path."),
            make_schema(&[("path", "string")], &[]),
        ),
        Tool::new(
            Cow::Borrowed("kb_write_page"),
            Cow::Borrowed("Create or overwrite a knowledge page (must be under knowledge/)."),
            make_schema(
                &[("path", "string"), ("content", "string")],
                &[("commit_message", "string")],
            ),
        ),
        Tool::new(
            Cow::Borrowed("kb_search"),
            Cow::Borrowed("BM25 full-text search over knowledge pages and raw sources."),
            make_schema(&[("query", "string")], &[("limit", "integer")]),
        ),
        Tool::new(
            Cow::Borrowed("kb_append_log"),
            Cow::Borrowed("Append an entry to the KB change log."),
            make_schema(&[("summary", "string")], &[("kind", "string"), ("delta", "string")]),
        ),
        Tool::new(
            Cow::Borrowed("kb_add_raw_source"),
            Cow::Borrowed("Ingest a new raw source (text/url/file) into the KB."),
            make_schema(&[("type", "string")], &[
                ("text", "string"),
                ("title", "string"),
                ("url", "string"),
                ("bytes_b64", "string"),
                ("filename", "string"),
                ("mime", "string"),
            ]),
        ),
        Tool::new(
            Cow::Borrowed("kb_classify_source"),
            Cow::Borrowed("Return the credibility metadata for a previously ingested source."),
            make_schema(&[("source_id", "string")], &[]),
        ),
        Tool::new(
            Cow::Borrowed("complete"),
            Cow::Borrowed("Signal that you have finished and the sub-agent loop should exit."),
            make_schema(&[], &[("message", "string")]),
        ),
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::service::KnowledgeService;

    fn fresh_svc() -> (tempfile::TempDir, KnowledgeService) {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("k", "K", None).unwrap();
        (dir, svc)
    }

    #[tokio::test]
    async fn kb_write_then_read_roundtrip() {
        let (_dir, svc) = fresh_svc();
        let dispatch = KbToolDispatch {
            svc,
            kb_id: "k".to_string(),
            txn_branch: String::new(), // no txn — commit directly
        };

        // Write a page
        let write_args = serde_json::json!({
            "path": "knowledge/entities/hrv.md",
            "content": "---\ntitle: HRV\nkind: entity\n---\n\nHeart rate variability.",
            "commit_message": "test write"
        });
        let write_result = dispatch.call("kb_write_page", write_args).await.unwrap();
        let parsed: Value = serde_json::from_str(&write_result).unwrap();
        // Should contain a commit_sha
        assert!(parsed.get("commit_sha").is_some());

        // Read it back
        let read_args = serde_json::json!({ "path": "knowledge/entities/hrv.md" });
        let read_result = dispatch.call("kb_read_page", read_args).await.unwrap();
        let page: Value = serde_json::from_str(&read_result).unwrap();
        let body = page["content"].as_str().unwrap_or("");
        assert!(body.contains("Heart rate variability"), "body was: {body}");
    }
}
