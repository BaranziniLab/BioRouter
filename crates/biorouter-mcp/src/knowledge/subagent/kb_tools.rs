//! `KbToolDispatch` — maps tool-name strings to `KnowledgeService` operations.
//!
//! This is the concrete `ToolDispatch` implementation used by the sub-agent
//! macros.  It binds to a specific KB and an optional transaction branch so
//! every write is either committed immediately (no txn) or staged on the txn
//! branch (with txn).

use crate::knowledge::{
    convert::SourceInput, log as kb_log, paths, raw, service::KnowledgeService, store,
    store::SearchScope, subagent::loop_::ToolDispatch, types::ChangeKind,
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
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
                let include_raw_sources = args
                    .get("include_raw_sources")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let scope = if include_raw_sources {
                    SearchScope::All
                } else {
                    SearchScope::Knowledge
                };
                let hits = store::search_with_scope(&kb_root, q, limit, scope)?;
                Ok(serde_json::to_string(&hits)?)
            }

            // ------------------------------------------------------------------
            "kb_append_log" => {
                let summary = args["summary"]
                    .as_str()
                    .ok_or_else(|| anyhow!("kb_append_log: missing 'summary'"))?;
                let kind_str = args
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("manual");
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
                })
                .to_string())
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
    let kind = args.get("type").and_then(|v| v.as_str()).unwrap_or("text");
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
            Ok(SourceInput::File {
                bytes,
                filename,
                mime,
            })
        }
        other => anyhow::bail!("kb_add_raw_source: unknown source type '{other}'"),
    }
}

// ---------------------------------------------------------------------------
// Tool specs for the sub-agent
// ---------------------------------------------------------------------------

/// One property of a tool's input schema.
///
/// ## Why this is not just `{"type": T}` any more (DR-16)
///
/// It was, and that made a closed vocabulary **unenforceable through the tool
/// interface**. `make_schema` could say a field is a string and nothing else:
/// no `description`, no `enum`, no arrays, no nesting. So the legal values for a
/// BioOKF `type` or `predicate` could only be taught in prose in the system
/// prompt — which the provider cannot use to constrain sampling, which means an
/// invalid value is caught at *dispatch*, as free text, in an error string. A
/// failed tool call is fed back as `error: …` and does not abort, so the model
/// retries, and the retries burn the step budget until the run dies for a reason
/// that has nothing to do with the actual mistake.
///
/// Declaring the vocabulary here instead kills the prompt bloat *and* makes it
/// enforceable — the same fix for two problems.
///
/// ## Nothing in this build uses the rich forms yet
///
/// Every existing spec below still goes through [`make_schema`] and emits
/// exactly the bytes it always did (pinned by
/// `todays_specs_are_byte_identical_to_the_minimal_shape`). Stage 5 is what
/// attaches the 28 node types and 35 predicates to the tools that take them.
/// This change only makes that expressible.
#[derive(Debug, Clone)]
pub struct Prop {
    ty: PropTy,
    description: Option<String>,
    /// The closed vocabulary for this property, empty when it is open.
    allowed: Vec<String>,
}

#[derive(Debug, Clone)]
enum PropTy {
    /// `"string"`, `"integer"`, `"boolean"`, `"number"`.
    Scalar(String),
    Array(Box<Prop>),
    Object {
        required: Vec<(String, Prop)>,
        optional: Vec<(String, Prop)>,
    },
}

impl Prop {
    /// A scalar of the given JSON Schema type name.
    pub fn scalar(ty: &str) -> Self {
        Self {
            ty: PropTy::Scalar(ty.to_string()),
            description: None,
            allowed: Vec::new(),
        }
    }

    pub fn string() -> Self {
        Self::scalar("string")
    }

    /// A homogeneous array. The item is a full [`Prop`], so an array of
    /// enum-constrained strings — a page's `tags`, a `verified` list — is one
    /// call and not a special case.
    pub fn array_of(item: Prop) -> Self {
        Self {
            ty: PropTy::Array(Box::new(item)),
            description: None,
            allowed: Vec::new(),
        }
    }

    /// A nested object with its own required/optional split. Needed for the
    /// shapes the tools already accept in prose — `kb_add_raw_source`'s
    /// `{type, text, title}` union, BioOKF's per-edge provenance triplet —
    /// which today are documented in a doc-comment and validated by hand in
    /// [`source_input_from_args`].
    pub fn object(required: Vec<(&str, Prop)>, optional: Vec<(&str, Prop)>) -> Self {
        let own = |v: Vec<(&str, Prop)>| {
            v.into_iter()
                .map(|(n, p)| (n.to_string(), p))
                .collect::<Vec<_>>()
        };
        Self {
            ty: PropTy::Object {
                required: own(required),
                optional: own(optional),
            },
            description: None,
            allowed: Vec::new(),
        }
    }

    /// The one sentence the model reads about this field.
    #[must_use]
    pub fn describe(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }

    /// The closed vocabulary. This is the whole point of the rewrite: an `enum`
    /// here is a constraint the provider can apply while sampling, rather than a
    /// list the model may or may not have read.
    #[must_use]
    pub fn one_of(mut self, values: &[&str]) -> Self {
        self.allowed = values.iter().map(|v| (*v).to_string()).collect();
        self
    }

    fn to_json(&self) -> Value {
        let mut out = serde_json::Map::new();
        match &self.ty {
            PropTy::Scalar(ty) => {
                out.insert("type".into(), Value::String(ty.clone()));
            }
            PropTy::Array(item) => {
                out.insert("type".into(), Value::String("array".into()));
                out.insert("items".into(), item.to_json());
            }
            PropTy::Object { required, optional } => {
                out.insert("type".into(), Value::String("object".into()));
                out.insert(
                    "properties".into(),
                    Value::Object(properties(required, optional)),
                );
                out.insert("required".into(), required_names(required));
            }
        }
        // Only when set, so a bare property is still exactly `{"type": T}` and
        // every spec that predates this builder is byte-for-byte unchanged.
        if let Some(d) = &self.description {
            out.insert("description".into(), Value::String(d.clone()));
        }
        if !self.allowed.is_empty() {
            out.insert(
                "enum".into(),
                Value::Array(
                    self.allowed
                        .iter()
                        .map(|v| Value::String(v.clone()))
                        .collect(),
                ),
            );
        }
        Value::Object(out)
    }
}

fn properties(
    required: &[(String, Prop)],
    optional: &[(String, Prop)],
) -> serde_json::Map<String, Value> {
    required
        .iter()
        .chain(optional.iter())
        .map(|(name, prop)| (name.clone(), prop.to_json()))
        .collect()
}

fn required_names(required: &[(String, Prop)]) -> Value {
    Value::Array(
        required
            .iter()
            .map(|(name, _)| Value::String(name.clone()))
            .collect(),
    )
}

/// Build a tool's top-level input schema from named properties.
///
/// The rich form: every property is a [`Prop`], so it may carry a description,
/// an `enum`, an item type or a nested object.
fn schema_of(required: Vec<(&str, Prop)>, optional: Vec<(&str, Prop)>) -> Arc<JsonObject> {
    let Value::Object(map) = Prop::object(required, optional).to_json() else {
        unreachable!("Prop::object always renders an object");
    };
    Arc::new(map)
}

/// The minimal form, kept because most fields really are just "a string": name
/// and JSON Schema type name, nothing else.
fn make_schema(
    required: &[(&str, &str)], // (name, type)
    optional: &[(&str, &str)], // (name, type)
) -> Arc<JsonObject> {
    fn lift<'a>(pairs: &[(&'a str, &str)]) -> Vec<(&'a str, Prop)> {
        pairs
            .iter()
            .map(|(name, ty)| (*name, Prop::scalar(ty)))
            .collect()
    }
    schema_of(lift(required), lift(optional))
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
            Cow::Borrowed(
                "BM25 full-text search over curated knowledge pages. Set include_raw_sources true only when the user specifically asks for original/raw sources.",
            ),
            make_schema(
                &[("query", "string")],
                &[("limit", "integer"), ("include_raw_sources", "boolean")],
            ),
        ),
        Tool::new(
            Cow::Borrowed("kb_append_log"),
            Cow::Borrowed("Append an entry to the KB change log."),
            make_schema(
                &[("summary", "string")],
                &[("kind", "string"), ("delta", "string")],
            ),
        ),
        Tool::new(
            Cow::Borrowed("kb_add_raw_source"),
            Cow::Borrowed("Ingest a new raw source (text/url/file) into the KB."),
            make_schema(
                &[("type", "string")],
                &[
                    ("text", "string"),
                    ("title", "string"),
                    ("url", "string"),
                    ("bytes_b64", "string"),
                    ("filename", "string"),
                    ("mime", "string"),
                ],
            ),
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
    use crate::knowledge::{page_fixtures::valid_page, service::KnowledgeService};

    fn fresh_svc() -> (tempfile::TempDir, KnowledgeService) {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("k", "K", None).unwrap();
        (dir, svc)
    }

    /// The minimal shape, spelled out independently of the builder: an object
    /// whose every property is exactly `{"type": T}`, in `type`/`properties`/
    /// `required` order.
    fn minimal_shape(required: &[(&str, &str)], optional: &[(&str, &str)]) -> Value {
        let mut props = serde_json::Map::new();
        for (n, t) in required.iter().chain(optional.iter()) {
            props.insert((*n).to_string(), serde_json::json!({ "type": t }));
        }
        serde_json::json!({
            "type": "object",
            "properties": props,
            "required": required.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
        })
    }

    /// DR-16 replaced `make_schema` with a builder that *can* express
    /// descriptions, enums, arrays and nesting — and must not have changed a
    /// single byte of what today's eight tools declare while doing it. Stage 5
    /// is where the richer forms get used; a change here now would be a change
    /// to what the model is told, smuggled in under a refactor.
    #[test]
    fn todays_specs_are_byte_identical_to_the_minimal_shape() {
        let expected: &[(&str, Value)] = &[
            (
                "kb_list_pages",
                minimal_shape(&[], &[("path_prefix", "string")]),
            ),
            ("kb_read_page", minimal_shape(&[("path", "string")], &[])),
            (
                "kb_write_page",
                minimal_shape(
                    &[("path", "string"), ("content", "string")],
                    &[("commit_message", "string")],
                ),
            ),
            (
                "kb_search",
                minimal_shape(
                    &[("query", "string")],
                    &[("limit", "integer"), ("include_raw_sources", "boolean")],
                ),
            ),
            (
                "kb_append_log",
                minimal_shape(
                    &[("summary", "string")],
                    &[("kind", "string"), ("delta", "string")],
                ),
            ),
            (
                "kb_add_raw_source",
                minimal_shape(
                    &[("type", "string")],
                    &[
                        ("text", "string"),
                        ("title", "string"),
                        ("url", "string"),
                        ("bytes_b64", "string"),
                        ("filename", "string"),
                        ("mime", "string"),
                    ],
                ),
            ),
            (
                "kb_classify_source",
                minimal_shape(&[("source_id", "string")], &[]),
            ),
            ("complete", minimal_shape(&[], &[("message", "string")])),
        ];

        let specs = tool_specs();
        assert_eq!(specs.len(), expected.len(), "a tool was added or removed");
        for (name, want) in expected {
            let tool = specs
                .iter()
                .find(|t| t.name == *name)
                .unwrap_or_else(|| panic!("{name} is no longer declared"));
            let got = Value::Object((*tool.input_schema).clone());
            assert_eq!(&got, want, "{name}'s schema changed");
        }
    }

    #[test]
    fn a_closed_vocabulary_reaches_the_provider_as_an_enum() {
        // The half that was impossible before: the legal values as data, where
        // the provider can constrain sampling with them, instead of as a
        // sentence in the system prompt that the model may not honour and the
        // provider cannot read at all.
        let schema = schema_of(
            vec![(
                "predicate",
                Prop::string()
                    .describe("The BioOKF relation this edge asserts.")
                    .one_of(&["treats", "associated_with"]),
            )],
            vec![],
        );
        let got = Value::Object((*schema).clone());
        assert_eq!(
            got["properties"]["predicate"],
            serde_json::json!({
                "type": "string",
                "description": "The BioOKF relation this edge asserts.",
                "enum": ["treats", "associated_with"],
            })
        );
        assert_eq!(got["required"], serde_json::json!(["predicate"]));
    }

    #[test]
    fn arrays_and_nested_objects_are_expressible() {
        // The shape `kb_add_raw_source` already accepts and documents only in a
        // doc-comment, plus the list form an `edges:` argument needs.
        let schema = schema_of(
            vec![(
                "source",
                Prop::object(
                    vec![("type", Prop::string().one_of(&["text", "url", "file"]))],
                    vec![("title", Prop::string())],
                ),
            )],
            vec![("tags", Prop::array_of(Prop::string()))],
        );
        let got = Value::Object((*schema).clone());
        assert_eq!(got["properties"]["source"]["type"], "object");
        assert_eq!(
            got["properties"]["source"]["properties"]["type"]["enum"],
            serde_json::json!(["text", "url", "file"])
        );
        assert_eq!(
            got["properties"]["source"]["required"],
            serde_json::json!(["type"])
        );
        assert_eq!(
            got["properties"]["tags"],
            serde_json::json!({ "type": "array", "items": { "type": "string" } })
        );
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
            "content": valid_page("entity", "HRV", "Heart rate variability."),
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
