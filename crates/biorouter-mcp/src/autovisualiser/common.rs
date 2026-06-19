//! Shared infrastructure for every Auto Visualiser tool.
//!
//! The render pipeline is deliberately uniform: each tool validates its input,
//! turns it into a JSON string, and hands a template + the libraries it needs to
//! [`finish`]. This module centralises the parts that used to be copy-pasted into
//! every tool (asset injection, HTML/JSON escaping, the debug dump, the
//! `CallToolResult` shape) plus the robustness guards (size limits, semantic
//! checks, lenient enum parsing helpers).

use base64::{engine::general_purpose::STANDARD, Engine as _};
use rmcp::model::{CallToolResult, Content, ErrorCode, ErrorData, ResourceContents, Role};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Limits — guard against pathological payloads that would freeze the renderer.
// They are generous (well beyond any sensible figure) and only exist to turn an
// out-of-memory / hung-iframe into a clear, actionable error.
// ---------------------------------------------------------------------------

pub const MAX_NODES: usize = 10_000;
pub const MAX_LINKS: usize = 50_000;
pub const MAX_MATRIX_DIM: usize = 500;
pub const MAX_MARKERS: usize = 100_000;
pub const MAX_VALUES: usize = 500_000;
pub const MAX_TREE_DEPTH: usize = 100;
pub const MAX_MERMAID_LEN: usize = 200_000;
pub const MAX_LABELS: usize = 10_000;

/// Build an `INVALID_PARAMS` error with a helpful, model-actionable message.
pub fn invalid(msg: impl Into<String>) -> ErrorData {
    ErrorData::new(ErrorCode::INVALID_PARAMS, msg.into(), None)
}

// ---------------------------------------------------------------------------
// Input validation
// ---------------------------------------------------------------------------

/// Validates that the `data` field of a serialized params value is a real JSON
/// object/array rather than a stringified blob. Retained as a reusable guard for
/// any future loosely-typed (`serde_json::Value`) tool and exercised by tests.
#[allow(dead_code)]
pub fn validate_data_param(params: &Value, allow_array: bool) -> Result<Value, ErrorData> {
    let data_value = params
        .get("data")
        .ok_or_else(|| invalid("Missing 'data' parameter"))?;

    if data_value.is_string() {
        return Err(invalid(
            "The 'data' parameter must be a JSON object, not a JSON string. \
             Please provide valid JSON without comments.",
        ));
    }

    if allow_array {
        if !data_value.is_object() && !data_value.is_array() {
            return Err(invalid(
                "The 'data' parameter must be a JSON object or array.",
            ));
        }
    } else if !data_value.is_object() {
        return Err(invalid("The 'data' parameter must be a JSON object."));
    }

    Ok(data_value.clone())
}

/// Enforce that a count does not exceed a limit, with a clear message.
pub fn check_limit(count: usize, limit: usize, what: &str) -> Result<(), ErrorData> {
    if count > limit {
        return Err(invalid(format!(
            "Too many {what}: {count} exceeds the maximum of {limit}. \
             Aggregate or sample the data before visualizing."
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Escaping — both data-into-<script> and text-into-HTML are user/LLM influenced.
// ---------------------------------------------------------------------------

/// Serialize a JSON value for safe embedding inside a `<script>` block.
///
/// JSON is valid JS, but a literal `</script>` (or `<!--`) inside a string would
/// terminate the script element and allow markup injection. We neutralise the
/// `<` so the payload can never break out of the script context, and escape the
/// Unicode line separators that are valid in JSON strings but illegal in JS.
pub fn js_data(v: &Value) -> Result<String, ErrorData> {
    let s = serde_json::to_string(v).map_err(|e| invalid(format!("Invalid JSON data: {e}")))?;
    Ok(s.replace('<', "\\u003c")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029"))
}

/// Serialize an already-typed value to a JS-safe JSON literal.
pub fn js_value<T: serde::Serialize>(v: &T) -> Result<String, ErrorData> {
    let value = serde_json::to_value(v).map_err(|e| invalid(format!("Invalid data: {e}")))?;
    js_data(&value)
}

/// Escape a plain string for safe embedding in HTML text / attribute context.
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

// ---------------------------------------------------------------------------
// Assets — the libraries each template needs, injected as either inlined
// `<script>`/`<style>` (default: fully offline & self-contained) or pinned CDN
// tags (BIOROUTER_AUTOVIS_CDN=1: tiny persisted blobs, the size fix).
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asset {
    D3,
    D3Sankey,
    ChartJs,
    Leaflet,
    Mermaid,
}

// Vendored libraries (compiled into the binary for offline use).
const D3_MIN: &str = include_str!("templates/assets/d3.min.js");
const D3_SANKEY: &str = include_str!("templates/assets/d3.sankey.min.js");
const CHART_MIN: &str = include_str!("templates/assets/chart.min.js");
const LEAFLET_JS: &str = include_str!("templates/assets/leaflet.min.js");
const LEAFLET_CSS: &str = include_str!("templates/assets/leaflet.min.css");
const MARKERCLUSTER_JS: &str = include_str!("templates/assets/leaflet.markercluster.min.js");
const MERMAID_MIN: &str = include_str!("templates/assets/mermaid.min.js");

// Pinned CDN URLs (used only when BIOROUTER_AUTOVIS_CDN is enabled).
const CDN_D3: &str = "https://cdn.jsdelivr.net/npm/d3@7/dist/d3.min.js";
const CDN_D3_SANKEY: &str = "https://cdn.jsdelivr.net/npm/d3-sankey@0.12/dist/d3-sankey.min.js";
const CDN_CHART: &str = "https://cdn.jsdelivr.net/npm/chart.js@4/dist/chart.umd.min.js";
const CDN_LEAFLET_JS: &str = "https://cdn.jsdelivr.net/npm/leaflet@1.9.4/dist/leaflet.js";
const CDN_LEAFLET_CSS: &str = "https://cdn.jsdelivr.net/npm/leaflet@1.9.4/dist/leaflet.css";
const CDN_MARKERCLUSTER_JS: &str =
    "https://cdn.jsdelivr.net/npm/leaflet.markercluster@1.5.3/dist/leaflet.markercluster.js";
const CDN_MERMAID: &str = "https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.min.js";

/// Whether to reference libraries from a pinned CDN instead of inlining them.
///
/// CDN mode shrinks the persisted/transported HTML blob from megabytes (Mermaid
/// alone is ~3 MB) to a few KB, which is the recommended mitigation for the
/// "visualization cannot be generated on reopen" issue with very large diagrams.
/// It requires network access at render time. Inlining (the default) keeps the
/// figure fully self-contained and offline.
pub fn use_cdn() -> bool {
    matches!(
        std::env::var("BIOROUTER_AUTOVIS_CDN").ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("yes")
    )
}

fn script_inline(src: &str) -> String {
    format!("<script>{src}</script>\n")
}

fn script_src(url: &str) -> String {
    format!("<script src=\"{url}\" crossorigin=\"anonymous\"></script>\n")
}

/// Render the `<head>` asset tags (scripts + stylesheets) for the given libraries.
pub fn asset_html(assets: &[Asset]) -> String {
    let cdn = use_cdn();
    let mut out = String::new();
    for a in assets {
        match a {
            Asset::D3 => out.push_str(&if cdn { script_src(CDN_D3) } else { script_inline(D3_MIN) }),
            Asset::D3Sankey => out.push_str(&if cdn {
                script_src(CDN_D3_SANKEY)
            } else {
                script_inline(D3_SANKEY)
            }),
            Asset::ChartJs => out.push_str(&if cdn {
                script_src(CDN_CHART)
            } else {
                script_inline(CHART_MIN)
            }),
            Asset::Leaflet => {
                if cdn {
                    out.push_str(&format!(
                        "<link rel=\"stylesheet\" href=\"{CDN_LEAFLET_CSS}\"/>\n"
                    ));
                    out.push_str(&script_src(CDN_LEAFLET_JS));
                    out.push_str(&script_src(CDN_MARKERCLUSTER_JS));
                } else {
                    out.push_str(&format!("<style>{LEAFLET_CSS}</style>\n"));
                    out.push_str(&script_inline(LEAFLET_JS));
                    out.push_str(&script_inline(MARKERCLUSTER_JS));
                }
            }
            Asset::Mermaid => out.push_str(&if cdn {
                // Mermaid 11 ships as an ESM module on the CDN.
                format!(
                    "<script type=\"module\">import mermaid from '{CDN_MERMAID}/+esm';window.mermaid=mermaid;</script>\n"
                )
            } else {
                script_inline(MERMAID_MIN)
            }),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Template assembly + result construction
// ---------------------------------------------------------------------------

/// Shared client runtime (theme, palette, auto-resize, error card) injected via `{{COMMON}}`.
pub const COMMON_JS: &str = include_str!("templates/_common.js");

/// Assemble a template: inject `{{ASSETS}}`, `{{COMMON}}`, then any extra `{{KEY}}`
/// substitutions (which callers must have already escaped appropriately).
pub fn assemble(template: &str, assets: &[Asset], subs: &[(&str, &str)]) -> String {
    let mut html = template
        .replace("{{ASSETS}}", &asset_html(assets))
        .replace("{{COMMON}}", COMMON_JS);
    for (key, val) in subs {
        html = html.replace(key, val);
    }
    html
}

/// Optionally dump the generated HTML to the cache dir for debugging.
///
/// Only runs when `BIOROUTER_AUTOVIS_DEBUG` is set (or in debug builds), writes to
/// a per-process unique file in the app cache dir (never the world-writable,
/// race-prone, Windows-nonexistent `/tmp`).
pub fn debug_dump(name: &str, html: &str) {
    let enabled = cfg!(debug_assertions)
        || matches!(
            std::env::var("BIOROUTER_AUTOVIS_DEBUG").ok().as_deref(),
            Some("1") | Some("true")
        );
    if !enabled {
        return;
    }
    let Ok(strategy) = etcetera::choose_app_strategy(crate::APP_STRATEGY.clone()) else {
        return;
    };
    use etcetera::AppStrategy;
    let dir = strategy.cache_dir().join("autovisualiser");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let file = dir.join(format!("{}-{}.html", name, std::process::id()));
    match std::fs::write(&file, html) {
        Ok(_) => tracing::info!("Auto Visualiser debug HTML saved to {}", file.display()),
        Err(e) => tracing::warn!("Failed to write Auto Visualiser debug HTML: {e}"),
    }
}

/// Build the standard two-part tool result: a `ui://` HTML resource for the user
/// and an assistant-audience text confirmation (so the model gets a non-empty
/// result and does not loop retrying).
pub fn finish(uri: &str, debug_name: &str, label: &str, html: String) -> CallToolResult {
    debug_dump(debug_name, &html);
    let blob = STANDARD.encode(html.as_bytes());
    let resource = ResourceContents::BlobResourceContents {
        uri: uri.to_string(),
        mime_type: Some("text/html".to_string()),
        blob,
        meta: None,
    };
    CallToolResult::success(vec![
        Content::resource(resource).with_audience(vec![Role::User]),
        Content::text(label.to_string()).with_audience(vec![Role::Assistant]),
    ])
}

/// Convenience: assemble a template and build the result in one step.
pub fn render(
    uri: &str,
    debug_name: &str,
    label: &str,
    template: &str,
    assets: &[Asset],
    subs: &[(&str, &str)],
) -> Result<CallToolResult, ErrorData> {
    let html = assemble(template, assets, subs);
    Ok(finish(uri, debug_name, label, html))
}

// ---------------------------------------------------------------------------
// Lenient enum parsing — accept any case/whitespace for small closed vocabularies
// so "Line"/"LINE"/" line " all work instead of failing at the rmcp layer
// (a primary cause of "visualization cannot be generated" on live generation).
// ---------------------------------------------------------------------------

/// Deserialize a `data` field that may arrive either as a real JSON object/array
/// **or** as a JSON string containing that object (some models — e.g. Xiaomi
/// MiMo — stringify nested tool-call arguments). Without this, a stringified
/// `data` fails to deserialize into the typed struct and the tool is rejected at
/// the rmcp layer ("interpreted as a string rather than a structured object").
///
/// Use via `#[serde(deserialize_with = "common::de_flexible")]` on `data` fields.
pub fn de_flexible<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    use serde::Deserialize;
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::String(s) => serde_json::from_str(&s).map_err(|e| {
            serde::de::Error::custom(format!(
                "the 'data' argument was a JSON string that could not be parsed \
                 into the expected structure: {e}. Pass it as a JSON object, not a string."
            ))
        }),
        other => serde_json::from_value(other).map_err(serde::de::Error::custom),
    }
}

/// Deserialize a string field leniently against a set of accepted lowercase
/// keywords, returning the canonical form. Use from a manual `Deserialize` impl.
pub fn parse_keyword<E: serde::de::Error>(raw: &str, accepted: &[&str]) -> Result<String, E> {
    let norm = raw.trim().to_lowercase();
    if accepted.contains(&norm.as_str()) {
        Ok(norm)
    } else {
        Err(E::custom(format!(
            "unknown value '{raw}', expected one of: {}",
            accepted.join(", ")
        )))
    }
}
