//! Agent-driven UI control — the `ui_*` tools an app's agent uses to **drive the
//! app**, not just answer in it.
//!
//! Every other tool an app agent has produces text that lands in the transcript.
//! These produce *commands* that travel down the app's own WebSocket and mutate
//! the page: mount a side panel, compose a dashboard, draw a chart, highlight a
//! section, restyle the app, or block on a form until the user answers.
//!
//! Shape (mirrors `datasql` / `files` / `compute`): an [`AppControlServer`] is
//! constructed **per app session** in `biorouter-server`'s `configure_agent`,
//! carrying a [`UiBridge`] whose sender is drained by that session's WebSocket
//! loop. It therefore can't be a `BUILTIN_EXTENSIONS` entry — it needs the live
//! socket. Scope is the app's own page: no filesystem, no network, no other tab.
//!
//! Round-trip: `ui_ask` emits a command carrying a `requestId`, parks on a
//! oneshot, and the socket loop resolves it when the browser sends back
//! `{"type":"ui_reply","requestId":…,"payload":{…}}`. The tool result *is* the
//! user's answer, so the agent can branch on it inside a single turn.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, Content, ErrorCode, ErrorData, Implementation, ServerCapabilities,
        ServerInfo,
    },
    schemars::JsonSchema,
    tool, tool_handler, tool_router, ServerHandler,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};

use super::manifest::{SurfaceDecl, UiCapability};

/// Catalog / protocol version stamped onto every `ui` frame (`"v"`), so an older
/// SDK can feature-detect and ignore frames it doesn't understand. Bump when the
/// frame vocabulary changes in a backward-incompatible way.
pub const CATALOG_VERSION: u64 = 1;

// ── Shared state document caps (Apps SDK v2, Pillar 2) ──────────────────────
// Enforced after every mutation so an unschema'd app can't become an unbounded
// injection / DoS path; a violating mutation is rejected and the doc is left
// unchanged.
/// Max serialized size of the whole state document.
pub const STATE_MAX_BYTES: usize = 262_144;
/// Max operations in a single RFC-6902 patch.
pub const STATE_MAX_PATCH_OPS: usize = 64;
/// Max nesting depth of the state document.
pub const STATE_MAX_DEPTH: usize = 8;
/// Max total object keys across the whole state document.
pub const STATE_MAX_KEYS: usize = 2000;

/// Widget node kinds the client SDK's `renderWidget` understands. Kept in sync
/// with `templates/sdk.ts` (`WidgetNode`). Validated server-side so a malformed
/// tree comes back to the model as a fixable error instead of a blank panel.
pub const WIDGET_KINDS: &[&str] = &[
    "card", "row", "col", "text", "badge", "table", "chart", "graph", "stat", "divider", "input",
    "select", "checkbox", "button", "form", "progress",
];

/// Placement slots a panel can occupy. `dock` is the SDK-provided right-hand
/// drawer that exists in every app even when the author declared no regions.
pub const PANEL_PLACES: &[&str] = &["dock", "left", "right", "bottom", "main", "modal"];

const MAX_WIDGET_DEPTH: usize = 12;
const MAX_WIDGET_NODES: usize = 600;
const MAX_TABLE_ROWS: usize = 500;
const MAX_CHART_POINTS: usize = 500;

fn invalid(msg: impl Into<String>) -> ErrorData {
    ErrorData::new(ErrorCode::INVALID_PARAMS, msg.into(), None)
}

/// Accept a JSON object/array that the model handed us as a *string*.
///
/// `spec` and `body` are typed `serde_json::Value`, so schemars emits an
/// unconstrained schema and many models (observed with Qwen via Ollama) send
/// `"{\"data\": [...]}"` instead of `{"data": [...]}`. They then retry forever,
/// because "must be an object" doesn't tell them what they actually did. Parse
/// the string once rather than bouncing the call back.
fn unstringify(value: &Value) -> Value {
    match value.as_str() {
        Some(s) => serde_json::from_str::<Value>(s).unwrap_or_else(|_| value.clone()),
        None => value.clone(),
    }
}

/// The error a still-stringified value earns: name the mistake precisely.
fn want_json(field: &str, kind: &str, example: &str) -> String {
    format!(
        "\"{field}\" must be {kind}, not a JSON string. Pass it as literal JSON, e.g. {example}"
    )
}

fn ok_text(msg: impl Into<String>) -> Result<CallToolResult, ErrorData> {
    Ok(CallToolResult::success(vec![Content::text(msg.into())]))
}

// ---------------------------------------------------------------------------
// Shared state document (Apps SDK v2, Pillar 2)
// ---------------------------------------------------------------------------

/// The single shared JSON state document for one app session, plus its version
/// counter. Both the agent (`ui_state` / `ui_patch_state`) and the browser
/// (`state_write`) mutate it; the server (this bridge) is the ordering authority
/// that bumps `version` on every accepted change.
struct StateDoc {
    doc: Value,
    version: u64,
}

impl Default for StateDoc {
    fn default() -> Self {
        Self {
            doc: Value::Object(serde_json::Map::new()),
            version: 0,
        }
    }
}

/// Why a client-originated state write was refused.
#[derive(Debug)]
pub enum StateWriteError {
    /// `base_version` did not match the server's version. Carries the current
    /// authoritative `(doc, version)` so the server can push a fresh snapshot to
    /// the out-of-date client.
    Conflict(Value, u64),
    /// The write was malformed or would violate the state caps; the doc is
    /// unchanged. The message is safe to relay.
    Invalid(String),
}

/// Enforce the structural caps on a candidate state document. Called after every
/// mutation, against the post-mutation value, so a violation rejects the change.
fn validate_state_doc(doc: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec(doc)
        .map(|b| b.len())
        .unwrap_or(usize::MAX);
    if bytes > STATE_MAX_BYTES {
        return Err(format!(
            "state document is {bytes} bytes; cap is {STATE_MAX_BYTES}"
        ));
    }
    let mut keys = 0usize;
    walk_state_doc(doc, 1, &mut keys)
}

/// Depth + total-key-count check. `depth` is 1 for the root value; each level of
/// object/array nesting adds one.
fn walk_state_doc(v: &Value, depth: usize, keys: &mut usize) -> Result<(), String> {
    if depth > STATE_MAX_DEPTH {
        return Err(format!(
            "state document is nested deeper than {STATE_MAX_DEPTH} levels"
        ));
    }
    match v {
        Value::Object(map) => {
            *keys += map.len();
            if *keys > STATE_MAX_KEYS {
                return Err(format!("state document exceeds {STATE_MAX_KEYS} keys"));
            }
            for child in map.values() {
                walk_state_doc(child, depth + 1, keys)?;
            }
        }
        Value::Array(arr) => {
            for child in arr {
                walk_state_doc(child, depth + 1, keys)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Set `value` at an RFC-6901 JSON Pointer within `root`, creating intermediate
/// objects as needed. An empty pointer replaces the whole document. Returns an
/// error only when the path runs *through* an existing non-object node.
fn pointer_set(root: &mut Value, pointer: &str, value: Value) -> Result<(), String> {
    if pointer.is_empty() {
        *root = value;
        return Ok(());
    }
    if !pointer.starts_with('/') {
        return Err(format!(
            "JSON pointer must be empty or start with '/', got {pointer:?}"
        ));
    }
    let tokens: Vec<String> = pointer[1..]
        .split('/')
        .map(|t| t.replace("~1", "/").replace("~0", "~"))
        .collect();
    let n = tokens.len();
    let mut cur = root;
    for (i, tok) in tokens.into_iter().enumerate() {
        let obj = object_for_set(cur)?;
        if i + 1 == n {
            obj.insert(tok, value);
            return Ok(());
        }
        cur = obj
            .entry(tok)
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
    }
    Ok(())
}

/// Coerce a node into a mutable object for pointer-set: `null` becomes a fresh
/// object (so absent intermediates are created), anything else must already be
/// an object.
fn object_for_set(v: &mut Value) -> Result<&mut serde_json::Map<String, Value>, String> {
    if v.is_null() {
        *v = Value::Object(serde_json::Map::new());
    }
    v.as_object_mut()
        .ok_or_else(|| "cannot set through a non-object node in the state document".to_string())
}

// ---------------------------------------------------------------------------
// Bridge: tools → WebSocket, and WebSocket → parked `ui_ask` tools
// ---------------------------------------------------------------------------

/// The state a single app session's UI shares between its agent's tools and its
/// WebSocket loop. Cloneable handle around shared interior state.
///
/// **Rebindable on purpose.** `AppState::get_agent` caches one agent per session
/// and `add_inprocess_server` is idempotent by name, so a reconnecting browser
/// reuses the *same* [`AppControlServer`] that the first connection injected. If
/// the outbound sender were owned by that server, every reload would leave the
/// `ui_*` tools writing into a dead channel. Instead the sender lives here behind
/// a lock: each connection calls [`UiBridge::attach`] to install a fresh channel
/// and [`UiBridge::detach`] on close.
#[derive(Clone)]
pub struct UiBridge {
    inner: Arc<BridgeInner>,
}

struct BridgeInner {
    /// Outbound half of the *current* connection. `None` between connections.
    tx: Mutex<Option<mpsc::UnboundedSender<Value>>>,
    /// Monotonic id of the current connection. Bumped by every `attach`; a
    /// `detach` only tears down when it still owns this generation, so a stale
    /// connection closing *after* a reload can't kill the fresh connection's
    /// channel or cancel its parked `ui_ask`.
    generation: AtomicU64,
    /// `requestId` → responder for a parked `ui_ask`.
    pending: Mutex<HashMap<String, oneshot::Sender<Value>>>,
    /// The app session's shared state document + version. Both the agent
    /// (`ui_state` / `ui_patch_state`) and the browser (`state_write`) mutate it;
    /// this bridge is the ordering authority.
    state: Mutex<StateDoc>,
    /// Panel ids currently mounted, oldest first.
    panels: Mutex<Vec<String>>,
    /// Last surface report the browser sent (regions, element ids, title).
    surface: Mutex<Option<Value>>,
    /// Fingerprint of the last `ui_describe` payload, so a repeat says "unchanged"
    /// (H3: nudges the model to stop re-polling within a turn).
    last_describe: Mutex<Option<String>>,
    seq: AtomicU64,
}

/// A handle to one attached connection. Passed back to [`UiBridge::detach`] so
/// only the connection that owns the current generation can tear it down.
#[derive(Debug, Clone, Copy)]
pub struct ConnToken(u64);

impl Default for UiBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl UiBridge {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(BridgeInner {
                tx: Mutex::new(None),
                generation: AtomicU64::new(0),
                pending: Mutex::new(HashMap::new()),
                state: Mutex::new(StateDoc::default()),
                panels: Mutex::new(Vec::new()),
                surface: Mutex::new(None),
                last_describe: Mutex::new(None),
                seq: AtomicU64::new(0),
            }),
        }
    }

    /// Bind this bridge to a new connection, returning the receiver its socket
    /// loop must drain plus a [`ConnToken`] to pass back to [`detach`]. Any
    /// previous connection's channel is dropped and its parked asks cancelled,
    /// and the per-connection view (mounted panels, reported surface) is reset —
    /// the reloaded page has none of it. The semantic `state` bag survives and is
    /// re-emitted so the fresh page rehydrates.
    pub fn attach(&self) -> (mpsc::UnboundedReceiver<Value>, ConnToken) {
        // Claim the next generation first: any concurrent stale detach now sees a
        // newer generation and becomes a no-op.
        let gen = self.inner.generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.cancel_all();
        let (tx, rx) = mpsc::unbounded_channel();
        if let Ok(mut slot) = self.inner.tx.lock() {
            *slot = Some(tx);
        }
        if let Ok(mut p) = self.inner.panels.lock() {
            p.clear();
        }
        if let Ok(mut s) = self.inner.surface.lock() {
            *s = None;
        }
        let (doc, version) = self.state_snapshot();
        let has_state = version > 0 || doc.as_object().map(|m| !m.is_empty()).unwrap_or(false);
        if has_state {
            let _ = self.emit(json!({
                "cmd": "state",
                "mode": "snapshot",
                "doc": doc,
                "version": version,
            }));
        }
        (rx, ConnToken(gen))
    }

    /// Release a connection. A no-op unless `token` is the current generation, so
    /// a stale connection closing *after* a reload cannot tear down the fresh
    /// one. Parked `ui_ask` calls are unblocked so no tool outlives its socket.
    pub fn detach(&self, token: ConnToken) {
        if self.inner.generation.load(Ordering::Acquire) != token.0 {
            return; // a newer connection owns the bridge now
        }
        if let Ok(mut slot) = self.inner.tx.lock() {
            *slot = None;
        }
        self.cancel_all();
    }

    fn next_id(&self, prefix: &str) -> String {
        let n = self.inner.seq.fetch_add(1, Ordering::Relaxed);
        format!("{prefix}-{n}")
    }

    /// Push a command frame at the browser. Fails when no socket is attached,
    /// which is the honest answer for the model: the app it was driving went away.
    fn emit(&self, mut cmd: Value) -> Result<(), ErrorData> {
        if let Some(obj) = cmd.as_object_mut() {
            obj.insert("type".into(), json!("ui"));
            // Stamp the catalog version on every ui frame so an older SDK can
            // feature-detect and ignore frames it doesn't understand.
            obj.insert("v".into(), json!(CATALOG_VERSION));
        }
        let gone = || {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                "the app window is no longer connected; its UI cannot be updated".to_string(),
                None,
            )
        };
        let slot = self.inner.tx.lock().map_err(|_| gone())?;
        let tx = slot.as_ref().ok_or_else(gone)?;
        tx.send(cmd).map_err(|_| gone())
    }

    /// Record the surface report the browser sends after `ready`.
    pub fn set_surface(&self, surface: Value) {
        if let Ok(mut s) = self.inner.surface.lock() {
            *s = Some(surface);
        }
    }

    /// The current shared state document and its version. The server uses this to
    /// send a fresh `snapshot` on connect and to persist the doc.
    pub fn state_snapshot(&self) -> (Value, u64) {
        self.inner
            .state
            .lock()
            .map(|s| (s.doc.clone(), s.version))
            .unwrap_or_else(|_| (Value::Object(serde_json::Map::new()), 0))
    }

    /// Seed the state document from durable persistence. Applies **only** while
    /// the session is still fresh (`version == 0`), so a restore can never clobber
    /// live in-session state; call it before the first [`attach`](Self::attach).
    pub fn seed_state(&self, doc: Value, version: u64) {
        if let Ok(mut s) = self.inner.state.lock() {
            if s.version == 0 {
                s.doc = doc;
                s.version = version;
            }
        }
    }

    /// Apply a browser-originated state write (from a `state_write` frame). The
    /// caller passes exactly one of `set` (a JSON Pointer + value) or `patch` (an
    /// RFC-6902 op array). `base_version` must match the server's current version
    /// (last-writer-wins with an optimistic-concurrency check); a mismatch returns
    /// [`StateWriteError::Conflict`] carrying the authoritative `(doc, version)` so
    /// the server can resnapshot the client. On success returns the RFC-6902 ops
    /// actually applied (for rebroadcast) and the new version.
    pub fn apply_client_write(
        &self,
        set: Option<(String, Value)>,
        patch: Option<Value>,
        base_version: u64,
    ) -> Result<(Value, u64), StateWriteError> {
        let mut guard =
            self.inner.state.lock().map_err(|_| {
                StateWriteError::Invalid("the app state is unavailable".to_string())
            })?;
        if base_version != guard.version {
            return Err(StateWriteError::Conflict(guard.doc.clone(), guard.version));
        }

        // Mutate a clone; commit only once it validates, so a rejected write
        // leaves the live document untouched.
        let mut next = guard.doc.clone();
        let applied_ops: Value = match (set, patch) {
            (Some(_), Some(_)) => {
                return Err(StateWriteError::Invalid(
                    "provide either \"set\" or \"patch\", not both".to_string(),
                ))
            }
            (None, None) => {
                return Err(StateWriteError::Invalid(
                    "a state_write needs a \"set\" or a \"patch\"".to_string(),
                ))
            }
            (Some((pointer, value)), None) => {
                pointer_set(&mut next, &pointer, value.clone())
                    .map_err(StateWriteError::Invalid)?;
                // Rebroadcast the set as a single RFC-6902 add (JSON Pointer
                // semantics: add creates or replaces the member).
                json!([{ "op": "add", "path": pointer, "value": value }])
            }
            (None, Some(patch)) => {
                let ops = patch.as_array().ok_or_else(|| {
                    StateWriteError::Invalid(
                        "\"patch\" must be an array of RFC-6902 operations".to_string(),
                    )
                })?;
                if ops.len() > STATE_MAX_PATCH_OPS {
                    return Err(StateWriteError::Invalid(format!(
                        "\"patch\" has {} operations; cap is {STATE_MAX_PATCH_OPS}",
                        ops.len()
                    )));
                }
                let parsed: json_patch::Patch =
                    serde_json::from_value(patch.clone()).map_err(|e| {
                        StateWriteError::Invalid(format!(
                            "\"patch\" is not a valid RFC-6902 JSON Patch: {e}"
                        ))
                    })?;
                json_patch::patch(&mut next, &parsed).map_err(|e| {
                    StateWriteError::Invalid(format!(
                        "\"patch\" could not be applied to the current state: {e}"
                    ))
                })?;
                patch
            }
        };

        validate_state_doc(&next).map_err(StateWriteError::Invalid)?;
        let new_version = guard.version + 1;
        guard.doc = next;
        guard.version = new_version;
        Ok((applied_ops, new_version))
    }

    /// Resolve a parked `ui_ask`. Returns whether a tool was actually waiting.
    pub fn resolve(&self, request_id: &str, payload: Value) -> bool {
        let responder = self
            .inner
            .pending
            .lock()
            .ok()
            .and_then(|mut p| p.remove(request_id));
        match responder {
            Some(tx) => tx.send(payload).is_ok(),
            None => false,
        }
    }

    /// Fail every parked `ui_ask` (socket closed / turn cancelled) so no tool
    /// hangs past the life of the connection.
    pub fn cancel_all(&self) {
        let drained: Vec<_> = self
            .inner
            .pending
            .lock()
            .map(|mut p| p.drain().map(|(_, tx)| tx).collect())
            .unwrap_or_default();
        for tx in drained {
            let _ = tx.send(json!({ "cancelled": true }));
        }
    }

    fn register_ask(&self, request_id: String) -> oneshot::Receiver<Value> {
        let (tx, rx) = oneshot::channel();
        if let Ok(mut p) = self.inner.pending.lock() {
            p.insert(request_id, tx);
        }
        rx
    }

    fn forget_ask(&self, request_id: &str) {
        if let Ok(mut p) = self.inner.pending.lock() {
            p.remove(request_id);
        }
    }
}

// ---------------------------------------------------------------------------
// Widget-tree validation
// ---------------------------------------------------------------------------

/// Validate an agent-authored widget tree against what the SDK can render.
/// Returns a message aimed at the model (it will see it as a tool error and
/// retry), not at the user.
pub fn validate_widget(node: &Value, depth: usize, budget: &mut usize) -> Result<(), String> {
    if depth > MAX_WIDGET_DEPTH {
        return Err(format!("widget tree nested deeper than {MAX_WIDGET_DEPTH}"));
    }
    *budget = budget
        .checked_sub(1)
        .ok_or_else(|| format!("widget tree exceeds {MAX_WIDGET_NODES} nodes"))?;

    let obj = node
        .as_object()
        .ok_or_else(|| "each widget node must be a JSON object".to_string())?;
    let t = obj
        .get("t")
        .and_then(Value::as_str)
        .ok_or_else(|| "each widget node needs a \"t\" (type) string".to_string())?;
    if !WIDGET_KINDS.contains(&t) {
        return Err(format!(
            "unknown widget type \"{t}\"; use one of: {}",
            WIDGET_KINDS.join(", ")
        ));
    }

    match t {
        "text" | "badge" => {
            if !obj.get("value").is_some_and(Value::is_string) {
                return Err(format!("\"{t}\" needs a string \"value\""));
            }
        }
        "stat" => {
            if !obj
                .get("value")
                .is_some_and(|v| v.is_string() || v.is_number())
            {
                return Err("\"stat\" needs a string or number \"value\"".to_string());
            }
        }
        "table" => {
            let cols = obj
                .get("columns")
                .and_then(Value::as_array)
                .ok_or_else(|| "\"table\" needs a \"columns\" array of strings".to_string())?;
            let rows = obj
                .get("rows")
                .and_then(Value::as_array)
                .ok_or_else(|| "\"table\" needs a \"rows\" array of arrays".to_string())?;
            if rows.len() > MAX_TABLE_ROWS {
                return Err(format!(
                    "\"table\" has {} rows; cap is {MAX_TABLE_ROWS} (aggregate or paginate)",
                    rows.len()
                ));
            }
            for (i, row) in rows.iter().enumerate() {
                let cells = row
                    .as_array()
                    .ok_or_else(|| format!("\"table\" row {i} is not an array"))?;
                if cells.len() != cols.len() {
                    return Err(format!(
                        "\"table\" row {i} has {} cells but there are {} columns",
                        cells.len(),
                        cols.len()
                    ));
                }
            }
        }
        "chart" => validate_chart(obj.get("spec").unwrap_or(&Value::Null))?,
        "graph" => validate_graph(obj.get("spec").unwrap_or(&Value::Null))?,
        "input" | "select" | "checkbox" => {
            if !obj.get("name").is_some_and(Value::is_string) {
                return Err(format!("\"{t}\" needs a string \"name\" (the field key)"));
            }
            if t == "select" {
                let opts = obj
                    .get("options")
                    .and_then(Value::as_array)
                    .ok_or_else(|| "\"select\" needs an \"options\" array".to_string())?;
                if opts.is_empty() {
                    return Err("\"select\" needs at least one option".to_string());
                }
            }
        }
        "button" => {
            if !obj.get("label").is_some_and(Value::is_string)
                || !obj.get("action").is_some_and(Value::is_string)
            {
                return Err("\"button\" needs string \"label\" and \"action\"".to_string());
            }
        }
        "progress" => {
            if !obj.get("value").is_some_and(Value::is_number) {
                return Err("\"progress\" needs a numeric \"value\" (0..1)".to_string());
            }
        }
        _ => {}
    }

    if let Some(children) = obj.get("children") {
        let arr = children
            .as_array()
            .ok_or_else(|| format!("\"{t}\".children must be an array"))?;
        for child in arr {
            validate_widget(child, depth + 1, budget)?;
        }
    } else if matches!(t, "card" | "row" | "col" | "form") {
        return Err(format!("\"{t}\" needs a \"children\" array"));
    }
    Ok(())
}

/// Coerce, validate, and return the widget array a tool was handed.
fn checked_body(body: &Value) -> Result<Value, ErrorData> {
    let body = unstringify(body);
    let nodes = body.as_array().ok_or_else(|| {
        invalid(want_json(
            "body",
            "an array of widget nodes",
            r#"[{"t":"text","value":"hello"}]"#,
        ))
    })?;
    if nodes.is_empty() {
        return Err(invalid("\"body\" must contain at least one widget node"));
    }
    let mut budget = MAX_WIDGET_NODES;
    for n in nodes {
        validate_widget(n, 0, &mut budget).map_err(invalid)?;
    }
    Ok(body)
}

fn validate_chart(spec: &Value) -> Result<(), String> {
    let obj = spec.as_object().ok_or_else(|| {
        want_json(
            "spec",
            "a chart object",
            r#"{"type":"bar","title":"Counts","data":[{"label":"A","value":1}]}"#,
        )
    })?;
    if let Some(t) = obj.get("type").and_then(Value::as_str) {
        if !["bar", "line", "pie"].contains(&t) {
            return Err(format!("chart type \"{t}\" must be bar, line, or pie"));
        }
    }
    // Validate one series' data array (the shared inner shape).
    let check_data = |data: &Vec<Value>, where_: &str| -> Result<(), String> {
        if data.is_empty() {
            return Err(format!("chart {where_} is empty"));
        }
        if data.len() > MAX_CHART_POINTS {
            return Err(format!(
                "chart {where_} has {} points; cap is {MAX_CHART_POINTS}",
                data.len()
            ));
        }
        for (i, p) in data.iter().enumerate() {
            let po = p
                .as_object()
                .ok_or_else(|| format!("chart {where_}[{i}] must be an object"))?;
            if !po.get("label").is_some_and(Value::is_string) {
                return Err(format!("chart {where_}[{i}] needs a string \"label\""));
            }
            let v = po
                .get("value")
                .and_then(Value::as_f64)
                .ok_or_else(|| format!("chart {where_}[{i}] needs a numeric \"value\""))?;
            if !v.is_finite() {
                return Err(format!("chart {where_}[{i}].value must be finite"));
            }
        }
        Ok(())
    };
    // Multi-series (`series:[{name,data}]`) OR single (`data:[…]`).
    if let Some(series) = obj.get("series").and_then(Value::as_array) {
        if series.is_empty() {
            return Err("chart \"series\" is empty".to_string());
        }
        if series.len() > 12 {
            return Err(format!("chart has {} series; cap is 12", series.len()));
        }
        for (si, s) in series.iter().enumerate() {
            let so = s
                .as_object()
                .ok_or_else(|| format!("chart series[{si}] must be an object with \"data\""))?;
            let data = so.get("data").and_then(Value::as_array).ok_or_else(|| {
                format!("chart series[{si}] needs a \"data\" array of {{label, value}}")
            })?;
            check_data(data, &format!("series[{si}].data"))?;
        }
        return Ok(());
    }
    let data = obj.get("data").and_then(Value::as_array).ok_or_else(|| {
        "chart spec needs a \"data\" array of {label, value} (or \"series\":[{name, data}] for multiple lines)"
            .to_string()
    })?;
    check_data(data, "data")
}

fn validate_graph(spec: &Value) -> Result<(), String> {
    let obj = spec.as_object().ok_or_else(|| {
        want_json(
            "spec",
            "a graph object",
            r#"{"nodes":[{"id":"A"}],"edges":[{"source":"A","target":"B"}]}"#,
        )
    })?;
    let nodes = obj.get("nodes").and_then(Value::as_array);
    let edges = obj.get("edges").and_then(Value::as_array);
    if nodes.is_none() && edges.is_none() {
        return Err("graph spec needs \"nodes\" and/or \"edges\"".to_string());
    }
    if let Some(edges) = edges {
        for (i, e) in edges.iter().enumerate() {
            if e.is_string() {
                continue; // "A -> B : label" edge-list form
            }
            let eo = e
                .as_object()
                .ok_or_else(|| format!("graph edges[{i}] must be an object or an edge string"))?;
            if !eo.get("source").is_some_and(Value::is_string)
                || !eo.get("target").is_some_and(Value::is_string)
            {
                return Err(format!(
                    "graph edges[{i}] needs string \"source\" and \"target\""
                ));
            }
        }
    }
    Ok(())
}

/// A `target` selector accepted by `ui_render` / `ui_chart` / `ui_highlight`.
/// `@region:<name>` / `@panel:<id>` / `@chat` / `@main` are SDK-resolved aliases;
/// anything else is treated as a CSS selector by the client.
fn validate_target(target: &str) -> Result<(), ErrorData> {
    let t = target.trim();
    if t.is_empty() {
        return Err(invalid("\"target\" must not be empty"));
    }
    if t.len() > 200 {
        return Err(invalid("\"target\" is unreasonably long"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tool parameters
// ---------------------------------------------------------------------------

// Schema-only mirrors. The runtime types stay `serde_json::Value` (so we can
// also accept a stringified object — see `unstringify`), but a bare `Value`
// generates a permissive `true` schema that tells the model nothing. Attaching
// these via `#[schemars(with = ...)]` gives it the real shape up front.

/// Schema for `ui_chart.spec`.
#[derive(JsonSchema)]
#[schemars(inline)]
#[allow(dead_code)]
struct ChartSpecSchema {
    /// "bar" (default), "line", or "pie".
    r#type: Option<String>,
    title: Option<String>,
    data: Vec<ChartPointSchema>,
}

#[derive(JsonSchema)]
#[schemars(inline)]
#[allow(dead_code)]
struct ChartPointSchema {
    label: String,
    value: f64,
}

/// Schema for `ui_graph.spec`.
#[derive(JsonSchema)]
#[schemars(inline)]
#[allow(dead_code)]
struct GraphSpecSchema {
    title: Option<String>,
    nodes: Option<Vec<GraphNodeSchema>>,
    edges: Option<Vec<GraphEdgeSchema>>,
}

#[derive(JsonSchema)]
#[schemars(inline)]
#[allow(dead_code)]
struct GraphNodeSchema {
    id: String,
    label: Option<String>,
    group: Option<String>,
}

#[derive(JsonSchema)]
#[schemars(inline)]
#[allow(dead_code)]
struct GraphEdgeSchema {
    source: String,
    target: String,
    label: Option<String>,
}

/// Schema for a `body` widget node. `children` stays untyped because the tree is
/// recursive; the tool description carries the full node grammar.
#[derive(JsonSchema)]
#[schemars(inline)]
#[allow(dead_code)]
struct WidgetNodeSchema {
    /// One of: card, row, col, text, badge, table, chart, graph, stat, divider,
    /// progress, input, select, checkbox, button, form.
    t: String,
    title: Option<String>,
    value: Option<Value>,
    label: Option<String>,
    children: Option<Vec<Value>>,
    spec: Option<Value>,
    columns: Option<Vec<String>>,
    rows: Option<Vec<Vec<Value>>>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct PanelParams {
    /// Stable panel id. Calling `ui_panel` again with the same id REPLACES that
    /// panel's contents (so you can refresh a dashboard in place).
    pub id: String,
    /// Heading shown on the panel.
    #[serde(default)]
    pub title: Option<String>,
    /// Where to mount it. Either an SDK dock slot — "dock" (default; a right-hand
    /// drawer the SDK always provides), "left", "right", "bottom", "main", or
    /// "modal" — OR a target that names part of the author's page:
    /// "@region:<name>" (an author-declared `data-br-region`), "@panel:<id>", or
    /// a CSS selector. Use a `@region:` target to drop a titled dashboard card
    /// straight into a region the app author laid out.
    #[serde(default)]
    pub place: Option<String>,
    /// The panel contents: an array of widget nodes. See the tool description
    /// for the node grammar.
    #[serde(default)]
    #[schemars(with = "Option<Vec<WidgetNodeSchema>>")]
    pub body: Option<Value>,
    /// Shorthand alternative to `body`: render this markdown as the panel body.
    #[serde(default)]
    pub markdown: Option<String>,
    /// Let the user collapse the panel. Default true.
    #[serde(default)]
    pub collapsible: Option<bool>,
    /// Remove the panel with this id instead of creating/replacing it.
    #[serde(default)]
    pub remove: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct RenderParams {
    /// Where to render: `@region:<name>` (an author-declared
    /// `data-br-region="<name>"`), `@panel:<id>`, `@chat`, `@main`, or a CSS
    /// selector like `#results`. Call `ui_describe` to see what exists.
    pub target: String,
    /// Array of widget nodes to render.
    #[schemars(with = "Vec<WidgetNodeSchema>")]
    pub body: Value,
    /// "replace" (default) or "append".
    #[serde(default)]
    pub mode: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ChartParams {
    /// Where to draw it (see `ui_render.target`). Defaults to a dock panel.
    #[serde(default)]
    pub target: Option<String>,
    /// Panel id when `target` is omitted (so repeat calls replace, not stack).
    #[serde(default)]
    pub id: Option<String>,
    /// `{"type":"bar"|"line"|"pie", "title":"…", "data":[{"label":"A","value":1}]}`
    #[schemars(with = "ChartSpecSchema")]
    pub spec: Value,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GraphParams {
    /// Where to draw it (see `ui_render.target`). Defaults to a dock panel.
    #[serde(default)]
    pub target: Option<String>,
    /// Panel id when `target` is omitted.
    #[serde(default)]
    pub id: Option<String>,
    /// `{"title":"…","nodes":[{"id":"A"}],"edges":[{"source":"A","target":"B","label":"binds"}]}`
    #[schemars(with = "GraphSpecSchema")]
    pub spec: Value,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct HighlightParams {
    /// What to highlight (see `ui_render.target`).
    pub target: String,
    /// "outline" (default), "pulse", "focus" (dim everything else), or "clear".
    #[serde(default)]
    pub mode: Option<String>,
    /// A short callout rendered next to the highlighted element.
    #[serde(default)]
    pub note: Option<String>,
    /// Scroll the element into view. Default true.
    #[serde(default)]
    pub scroll: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ThemeParams {
    /// Accent colour as a CSS colour (e.g. "#2f6f4e", "tomato").
    #[serde(default)]
    pub accent: Option<String>,
    /// "light", "dark", or "auto".
    #[serde(default)]
    pub mode: Option<String>,
    /// "comfortable" (default) or "compact".
    #[serde(default)]
    pub density: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct LayoutParams {
    /// "single" (default), "sidebar-right", "sidebar-left", "split", or
    /// "dashboard" (a responsive grid of panels).
    pub preset: String,
    /// Sidebar width in CSS px (sidebar presets only).
    #[serde(default)]
    pub sidebar_width: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct NotifyParams {
    /// The message to show.
    pub message: String,
    /// "info" (default), "success", "warn", or "error".
    #[serde(default)]
    pub level: Option<String>,
    /// Auto-dismiss after this many ms (default 4000; 0 = sticky).
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct StateParams {
    /// Keys to set/overwrite in the app's shared state document (merged at the root).
    #[serde(default)]
    pub set: Option<Value>,
    /// Top-level keys to delete.
    #[serde(default)]
    pub remove: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct PatchStateParams {
    /// An RFC-6902 JSON Patch: an array of operation objects, e.g.
    /// `[{"op":"add","path":"/cohort/count","value":42}]`. Capped at 64 ops.
    pub patch: Value,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[schemars(inline)]
pub struct AskField {
    /// Key this field's answer appears under in the returned payload.
    pub name: String,
    /// Visible label.
    #[serde(default)]
    pub label: Option<String>,
    /// "text" (default), "number", "textarea", "select", or "checkbox".
    #[serde(default)]
    pub r#type: Option<String>,
    /// Options for `type: "select"`, as plain strings.
    #[serde(default)]
    pub options: Option<Vec<String>>,
    /// Pre-filled value.
    #[serde(default)]
    pub value: Option<String>,
    /// Placeholder text.
    #[serde(default)]
    pub placeholder: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AskParams {
    /// Question / instructions shown above the form.
    pub prompt: String,
    /// The fields the user fills in.
    pub fields: Vec<AskField>,
    /// Form heading.
    #[serde(default)]
    pub title: Option<String>,
    /// Submit button label (default "Submit").
    #[serde(default)]
    pub submit_label: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DescribeParams {}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

/// The `ui_*` tool server for one app session.
#[derive(Clone)]
pub struct AppControlServer {
    tool_router: ToolRouter<Self>,
    bridge: UiBridge,
    cap: UiCapability,
    /// The app's declared surface (state schema, actions, signals, components),
    /// reported to the model via `ui_describe`.
    surface: SurfaceDecl,
}

impl AppControlServer {
    pub fn new(bridge: UiBridge, cap: UiCapability, surface: SurfaceDecl) -> Self {
        Self {
            tool_router: Self::tool_router(),
            bridge,
            cap,
            surface,
        }
    }

    fn denied(what: &str) -> ErrorData {
        invalid(format!(
            "this app does not grant {what}; ask the user to enable it in the app's manifest"
        ))
    }

    /// The app's declared surface, shaped for `ui_describe`: the actions the
    /// agent may call, the signals it may subscribe to, the custom components,
    /// and whether a state schema is declared.
    fn declared_surface(&self) -> Value {
        let s = &self.surface;
        json!({
            "actions": s.actions.iter().map(|a| json!({
                "name": a.name,
                "description": a.description,
                "params": a.params,
            })).collect::<Vec<_>>(),
            "signals": s.signals.iter().map(|sig| json!({
                "name": sig.name,
                "payload": sig.payload,
                "coalesceMs": sig.coalesce_ms,
            })).collect::<Vec<_>>(),
            "components": s.components.iter().map(|c| json!({
                "name": c.name,
                "props": c.props,
            })).collect::<Vec<_>>(),
            "hasStateSchema": s.state_schema.is_some(),
        })
    }

    /// Track a panel id, evicting the oldest when over the cap so a runaway loop
    /// can't paper the window with panels.
    fn note_panel(&self, id: &str) -> Option<String> {
        let mut panels = self.bridge.inner.panels.lock().ok()?;
        if panels.iter().any(|p| p == id) {
            return None;
        }
        panels.push(id.to_string());
        if panels.len() > self.cap.max_panels {
            return Some(panels.remove(0));
        }
        None
    }

    fn drop_panel(&self, id: &str) {
        if let Ok(mut panels) = self.bridge.inner.panels.lock() {
            panels.retain(|p| p != id);
        }
    }
}

#[tool_router(router = tool_router)]
impl AppControlServer {
    #[tool(
        name = "ui_describe",
        description = "Inspect the app's live UI: which regions the author declared \
                       (data-br-region), which element ids exist, which agent panels are \
                       currently mounted, and the shared state bag. Call this FIRST if you \
                       intend to render into an existing part of the page."
    )]
    pub async fn ui_describe(
        &self,
        Parameters(_): Parameters<DescribeParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let surface = self
            .bridge
            .inner
            .surface
            .lock()
            .ok()
            .and_then(|s| s.clone())
            .unwrap_or_else(|| json!({ "regions": [], "ids": [] }));
        let panels = self
            .bridge
            .inner
            .panels
            .lock()
            .map(|p| p.clone())
            .unwrap_or_default();
        let (doc, version) = self.bridge.state_snapshot();
        let keys: Vec<String> = doc
            .as_object()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        let out = json!({
            "surface": surface,
            "panels": panels,
            "state": {
                "version": version,
                "keys": keys,
            },
            "declared": self.declared_surface(),
            "allowed": {
                "theme": self.cap.allow_theme,
                "layout": self.cap.allow_layout,
                "ask": self.cap.allow_ask,
                "maxPanels": self.cap.max_panels,
            },
        });
        let body = serde_json::to_string_pretty(&out).unwrap_or_default();
        // If nothing about the page changed since the last describe, say so — the
        // model shouldn't burn turn budget re-polling a stable surface.
        let unchanged = self
            .bridge
            .inner
            .last_describe
            .lock()
            .map(|prev| prev.as_deref() == Some(body.as_str()))
            .unwrap_or(false);
        if let Ok(mut prev) = self.bridge.inner.last_describe.lock() {
            *prev = Some(body.clone());
        }
        if unchanged {
            ok_text(format!(
                "(Surface unchanged since your last ui_describe — no need to call it \
                 again this turn.)\n{body}"
            ))
        } else {
            ok_text(body)
        }
    }

    #[tool(
        name = "ui_panel",
        description = "Mount (or replace, or remove) a panel in the app — a side panel, a \
                       dashboard, an inspector. This is the main way to give the user \
                       something richer than a paragraph.\n\n\
                       `body` is an array of widget nodes. Node shapes:\n\
                       {\"t\":\"card\",\"title\":\"…\",\"children\":[…]}\n\
                       {\"t\":\"row\"|\"col\",\"children\":[…]}\n\
                       {\"t\":\"text\",\"value\":\"…\",\"markdown\":true,\"muted\":false}\n\
                       {\"t\":\"stat\",\"label\":\"Median\",\"value\":42,\"unit\":\"ms\",\"delta\":\"+3%\"}\n\
                       {\"t\":\"badge\",\"value\":\"beta\"}\n\
                       {\"t\":\"divider\"}\n\
                       {\"t\":\"progress\",\"value\":0.62,\"label\":\"…\"}\n\
                       {\"t\":\"table\",\"columns\":[…],\"rows\":[[…],[…]]}\n\
                       {\"t\":\"chart\",\"spec\":{\"type\":\"bar\",\"title\":\"…\",\"data\":[{\"label\":\"A\",\"value\":1}]}}  (or \"series\":[{\"name\":\"train\",\"data\":[…]},…] for multiple lines)\n\
                       {\"t\":\"graph\",\"spec\":{\"nodes\":[{\"id\":\"A\"}],\"edges\":[{\"source\":\"A\",\"target\":\"B\"}]}}\n\
                       {\"t\":\"input\"|\"select\"|\"checkbox\",\"name\":\"…\",\"label\":\"…\"}\n\
                       {\"t\":\"button\",\"label\":\"Run\",\"action\":\"run\",\"submit\":true}\n\
                       A `button` sends its action (and, with submit:true, the form fields) \
                       back to you as your next turn.\n\n\
                       `place` picks where it mounts: a dock slot (\"dock\" default / \
                       \"left\" / \"right\" / \"bottom\" / \"main\" / \"modal\"), OR a target \
                       naming the author's own layout — \"@region:<name>\", \"@panel:<id>\", \
                       or a CSS selector. To fill a region the author declared (e.g. \
                       @region:dashboard), pass it as `place` directly."
    )]
    pub async fn ui_panel(
        &self,
        Parameters(p): Parameters<PanelParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let id = p.id.trim();
        if id.is_empty() {
            return Err(invalid("\"id\" must not be empty"));
        }

        if p.remove == Some(true) {
            self.drop_panel(id);
            self.bridge
                .emit(json!({ "cmd": "panel", "id": id, "remove": true }))?;
            return ok_text(format!("Removed panel \"{id}\"."));
        }

        // `place` is either an SDK-owned dock slot (dock/left/right/bottom/main/
        // modal) OR a target that names part of the author's page — `@region:x`,
        // `@panel:x`, `@main`, or a CSS selector — in which case the panel's card
        // mounts *into* that element. The latter is the overwhelmingly common
        // "put a titled dashboard into @region:dashboard" case; rejecting it made
        // the model burn 5–7 retries before falling back to `ui_render`.
        let place = p.place.as_deref().unwrap_or("dock");
        let is_target = place.starts_with('@') || !PANEL_PLACES.contains(&place);
        if is_target {
            validate_target(place)?;
        }

        let body = match (&p.body, &p.markdown) {
            (Some(b), _) => checked_body(b)?,
            (None, Some(md)) => json!([{ "t": "text", "value": md, "markdown": true }]),
            (None, None) => {
                return Err(invalid(
                    "provide either \"body\" (widget nodes) or \"markdown\"",
                ))
            }
        };

        let evicted = self.note_panel(id);
        if let Some(old) = &evicted {
            self.bridge
                .emit(json!({ "cmd": "panel", "id": old, "remove": true }))?;
        }
        self.bridge.emit(json!({
            "cmd": "panel",
            "id": id,
            "title": p.title,
            "place": place,
            "collapsible": p.collapsible.unwrap_or(true),
            "body": body,
        }))?;

        let mut msg = format!("Panel \"{id}\" is now showing in the app ({place}).");
        if let Some(old) = evicted {
            msg.push_str(&format!(
                " Evicted the oldest panel \"{old}\" (limit {}).",
                self.cap.max_panels
            ));
        }
        ok_text(msg)
    }

    #[tool(
        name = "ui_render",
        description = "Render widget nodes into an existing part of the app — an \
                       author-declared region (`@region:results`), a panel you created \
                       (`@panel:summary`), or a CSS selector (`#out`). Use `ui_describe` to \
                       discover targets. Same `body` node schema as `ui_panel`."
    )]
    pub async fn ui_render(
        &self,
        Parameters(p): Parameters<RenderParams>,
    ) -> Result<CallToolResult, ErrorData> {
        validate_target(&p.target)?;
        let body = checked_body(&p.body)?;
        let mode = p.mode.as_deref().unwrap_or("replace");
        if !["replace", "append"].contains(&mode) {
            return Err(invalid("\"mode\" must be \"replace\" or \"append\""));
        }
        self.bridge.emit(json!({
            "cmd": "render",
            "target": p.target,
            "mode": mode,
            "body": body,
        }))?;
        ok_text(format!("Rendered into {} ({mode}).", p.target))
    }

    #[tool(
        name = "ui_chart",
        description = "Draw a bar/line/pie chart in the app.\n\
                       Single series: {\"type\":\"bar\",\"title\":\"…\",\"data\":[{\"label\":\"A\",\"value\":12}]}.\n\
                       Multiple series on one axis (e.g. train vs validation loss, actual vs forecast, \
                       two arms): {\"type\":\"line\",\"title\":\"…\",\"series\":[{\"name\":\"train\",\
                       \"data\":[{\"label\":\"e1\",\"value\":0.9}, …]},{\"name\":\"val\",\"data\":[…]}]} — \
                       overlaid lines / grouped bars with a legend, series index-aligned by label. \
                       Omit `target` to put it in a dock panel."
    )]
    pub async fn ui_chart(
        &self,
        Parameters(p): Parameters<ChartParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let spec = unstringify(&p.spec);
        validate_chart(&spec).map_err(invalid)?;
        let node = json!({ "t": "chart", "spec": spec });
        self.emit_visual(p.target, p.id, "chart", node, &spec)
    }

    #[tool(
        name = "ui_graph",
        description = "Draw a node/edge graph (network, pathway, relationship map) in the app. \
                       spec: {\"title\":\"…\",\"nodes\":[{\"id\":\"TP53\"}],\"edges\":[{\"source\":\"TP53\",\"target\":\"MDM2\",\"label\":\"inhibits\"}]}. \
                       Omit `target` to put it in a dock panel."
    )]
    pub async fn ui_graph(
        &self,
        Parameters(p): Parameters<GraphParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let spec = unstringify(&p.spec);
        validate_graph(&spec).map_err(invalid)?;
        let node = json!({ "t": "graph", "spec": spec });
        self.emit_visual(p.target, p.id, "graph", node, &spec)
    }

    #[tool(
        name = "ui_highlight",
        description = "Draw the user's eye to part of the app: outline it, pulse it, or dim \
                       everything else (\"focus\"). Optionally attach a short note. Use \
                       mode:\"clear\" to remove all highlights. Target syntax matches ui_render."
    )]
    pub async fn ui_highlight(
        &self,
        Parameters(p): Parameters<HighlightParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let mode = p.mode.as_deref().unwrap_or("outline");
        if !["outline", "pulse", "focus", "clear"].contains(&mode) {
            return Err(invalid("\"mode\" must be outline, pulse, focus, or clear"));
        }
        if mode != "clear" {
            validate_target(&p.target)?;
        }
        self.bridge.emit(json!({
            "cmd": "highlight",
            "target": p.target,
            "mode": mode,
            "note": p.note,
            "scroll": p.scroll.unwrap_or(true),
        }))?;
        ok_text(if mode == "clear" {
            "Cleared highlights.".to_string()
        } else {
            format!("Highlighted {} ({mode}).", p.target)
        })
    }

    #[tool(
        name = "ui_theme",
        description = "Restyle the app: accent colour, light/dark mode, density. Use this to \
                       make the app *feel* like what it is showing (e.g. a warning palette \
                       for a safety review)."
    )]
    pub async fn ui_theme(
        &self,
        Parameters(p): Parameters<ThemeParams>,
    ) -> Result<CallToolResult, ErrorData> {
        if !self.cap.allow_theme {
            return Err(Self::denied("theme control"));
        }
        if let Some(m) = p.mode.as_deref() {
            if !["light", "dark", "auto"].contains(&m) {
                return Err(invalid("\"mode\" must be light, dark, or auto"));
            }
        }
        if let Some(d) = p.density.as_deref() {
            if !["comfortable", "compact"].contains(&d) {
                return Err(invalid("\"density\" must be comfortable or compact"));
            }
        }
        if let Some(a) = p.accent.as_deref() {
            // The client writes this into a CSS custom property; keep it to a
            // colour-ish token so it can't smuggle `};` and escape the rule.
            if a.len() > 32 || a.contains([';', '}', '{', '<', '>', '(', ')', '"', '\'', '\\', '/'])
            {
                return Err(invalid(
                    "\"accent\" must be a simple CSS colour like \"#2f6f4e\" or \"tomato\"",
                ));
            }
        }
        self.bridge.emit(json!({
            "cmd": "theme",
            "accent": p.accent,
            "mode": p.mode,
            "density": p.density,
        }))?;
        ok_text("Theme updated.")
    }

    #[tool(
        name = "ui_layout",
        description = "Switch the app's layout: \"single\", \"sidebar-right\", \"sidebar-left\", \
                       \"split\", or \"dashboard\" (a responsive grid). Pair with ui_panel to \
                       fill the new regions."
    )]
    pub async fn ui_layout(
        &self,
        Parameters(p): Parameters<LayoutParams>,
    ) -> Result<CallToolResult, ErrorData> {
        if !self.cap.allow_layout {
            return Err(Self::denied("layout control"));
        }
        const PRESETS: &[&str] = &[
            "single",
            "sidebar-right",
            "sidebar-left",
            "split",
            "dashboard",
        ];
        if !PRESETS.contains(&p.preset.as_str()) {
            return Err(invalid(format!(
                "\"preset\" must be one of: {}",
                PRESETS.join(", ")
            )));
        }
        self.bridge.emit(json!({
            "cmd": "layout",
            "preset": p.preset,
            "sidebarWidth": p.sidebar_width,
        }))?;
        ok_text(format!("Layout set to {}.", p.preset))
    }

    #[tool(
        name = "ui_notify",
        description = "Show a transient toast in the app (progress, warnings, completion)."
    )]
    pub async fn ui_notify(
        &self,
        Parameters(p): Parameters<NotifyParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let level = p.level.as_deref().unwrap_or("info");
        if !["info", "success", "warn", "error"].contains(&level) {
            return Err(invalid("\"level\" must be info, success, warn, or error"));
        }
        if p.message.trim().is_empty() {
            return Err(invalid("\"message\" must not be empty"));
        }
        self.bridge.emit(json!({
            "cmd": "notify",
            "message": p.message,
            "level": level,
            "timeoutMs": p.timeout_ms.unwrap_or(4000),
        }))?;
        ok_text("Notification shown.")
    }

    #[tool(
        name = "ui_state",
        description = "Read/write the app's shared state document by top-level key. Keys you set \
                       are merged into the document and mirrored into the page (`br.state`), so \
                       the app's own code and bound elements react. Returns the full document \
                       after the update. For surgical edits to nested paths, prefer \
                       `ui_patch_state`."
    )]
    pub async fn ui_state(
        &self,
        Parameters(p): Parameters<StateParams>,
    ) -> Result<CallToolResult, ErrorData> {
        // Track whether anything actually changed, so an identical repeat becomes
        // a cheap no-op that tells the model to stop re-sending it (H3: models
        // cascade ui_state with unchanged values and exhaust the turn budget).
        let mut guard = self
            .bridge
            .inner
            .state
            .lock()
            .map_err(|_| invalid("the app state is unavailable"))?;

        // Merge into a clone so a cap violation rejects the change and leaves the
        // live document untouched. The document is always an object at the root.
        let mut next = if guard.doc.is_object() {
            guard.doc.clone()
        } else {
            json!({})
        };
        let obj = next.as_object_mut().expect("normalized to an object");
        let mut changed = false;
        if let Some(set) = &p.set {
            let set = unstringify(set);
            let set_obj = set
                .as_object()
                .ok_or_else(|| invalid(want_json("set", "a JSON object", r#"{"gene":"TP53"}"#)))?;
            for (k, v) in set_obj {
                if obj.get(k) != Some(v) {
                    changed = true;
                    obj.insert(k.clone(), v.clone());
                }
            }
        }
        if let Some(remove) = &p.remove {
            for k in remove {
                if obj.remove(k).is_some() {
                    changed = true;
                }
            }
        }

        let version = if changed {
            validate_state_doc(&next).map_err(invalid)?;
            guard.doc = next;
            guard.version += 1;
            guard.version
        } else {
            guard.version
        };
        let snapshot = guard.doc.clone();
        drop(guard);

        // Only touch the page when the document actually moved. An unchanged call
        // emits no frame and says so, so the model doesn't keep re-sending it.
        if changed {
            self.bridge.emit(json!({
                "cmd": "state",
                "mode": "snapshot",
                "doc": snapshot,
                "version": version,
            }))?;
            ok_text(serde_json::to_string(&snapshot).unwrap_or_default())
        } else if p.set.is_none() && p.remove.is_none() {
            // A read: return the current document.
            ok_text(serde_json::to_string(&snapshot).unwrap_or_default())
        } else {
            ok_text(format!(
                "No change — the state document already holds these values, so nothing was \
                 re-sent. Current state: {}",
                serde_json::to_string(&snapshot).unwrap_or_default()
            ))
        }
    }

    #[tool(
        name = "ui_patch_state",
        description = "Apply an RFC-6902 JSON Patch to the app's shared state document — an array \
                       of operations, e.g. [{\"op\":\"add\",\"path\":\"/cohort/count\",\"value\":42}]. \
                       Capped at 64 operations. Bumps the document version and pushes the delta to \
                       the page (bound elements re-render). Prefer this over `ui_state` for \
                       surgical updates to nested paths."
    )]
    pub async fn ui_patch_state(
        &self,
        Parameters(p): Parameters<PatchStateParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let patch = unstringify(&p.patch);
        let ops = patch.as_array().ok_or_else(|| {
            invalid(want_json(
                "patch",
                "an array of RFC-6902 operations",
                r#"[{"op":"add","path":"/count","value":3}]"#,
            ))
        })?;
        if ops.is_empty() {
            return Err(invalid("\"patch\" must contain at least one operation"));
        }
        if ops.len() > STATE_MAX_PATCH_OPS {
            return Err(invalid(format!(
                "\"patch\" has {} operations; cap is {STATE_MAX_PATCH_OPS}",
                ops.len()
            )));
        }
        let parsed: json_patch::Patch = serde_json::from_value(patch.clone())
            .map_err(|e| invalid(format!("\"patch\" is not a valid RFC-6902 JSON Patch: {e}")))?;

        let mut guard = self
            .bridge
            .inner
            .state
            .lock()
            .map_err(|_| invalid("the app state is unavailable"))?;
        // Apply against a clone; commit only once it applies and validates, so a
        // rejected patch leaves the live document untouched.
        let mut next = guard.doc.clone();
        json_patch::patch(&mut next, &parsed).map_err(|e| {
            invalid(format!(
                "\"patch\" could not be applied to the current state: {e}"
            ))
        })?;
        validate_state_doc(&next).map_err(invalid)?;
        guard.doc = next;
        guard.version += 1;
        let version = guard.version;
        drop(guard);

        self.bridge.emit(json!({
            "cmd": "state",
            "mode": "patch",
            "patch": patch,
            "version": version,
        }))?;
        ok_text(
            serde_json::to_string(&json!({ "ok": true, "version": version })).unwrap_or_default(),
        )
    }

    #[tool(
        name = "ui_ask",
        description = "Render a form in the app and WAIT for the user to submit it; the tool \
                       result is their answers, so you branch on them without ending your turn.\n\n\
                       ALWAYS use this instead of asking the user a question in prose. If you are \
                       about to write \"please provide…\", \"paste your data\", \"which would you \
                       like?\" or otherwise stop and wait for a reply — call `ui_ask` instead. \
                       Many apps have no chat box, so a prose question reaches nobody."
    )]
    pub async fn ui_ask(
        &self,
        Parameters(p): Parameters<AskParams>,
    ) -> Result<CallToolResult, ErrorData> {
        if !self.cap.allow_ask {
            return Err(Self::denied("interactive prompts"));
        }
        if p.fields.is_empty() {
            return Err(invalid("\"fields\" must contain at least one field"));
        }
        if p.fields.len() > 24 {
            return Err(invalid("\"fields\" is capped at 24"));
        }
        for f in &p.fields {
            if f.name.trim().is_empty() {
                return Err(invalid("every field needs a non-empty \"name\""));
            }
            let ty = f.r#type.as_deref().unwrap_or("text");
            if !["text", "number", "textarea", "select", "checkbox"].contains(&ty) {
                return Err(invalid(format!(
                    "field \"{}\": type must be text, number, textarea, select, or checkbox",
                    f.name
                )));
            }
            if ty == "select" && f.options.as_ref().is_none_or(Vec::is_empty) {
                return Err(invalid(format!(
                    "field \"{}\": a select needs a non-empty \"options\" list",
                    f.name
                )));
            }
        }

        let request_id = self.bridge.next_id("ask");
        let rx = self.bridge.register_ask(request_id.clone());
        let emit = self.bridge.emit(json!({
            "cmd": "ask",
            "requestId": request_id,
            "title": p.title,
            "prompt": p.prompt,
            "submitLabel": p.submit_label.unwrap_or_else(|| "Submit".to_string()),
            "fields": p.fields,
        }));
        if let Err(e) = emit {
            self.bridge.forget_ask(&request_id);
            return Err(e);
        }

        let timeout = Duration::from_secs(self.cap.ask_timeout_s.clamp(5, 3600));
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(payload)) => {
                if payload.get("cancelled").and_then(Value::as_bool) == Some(true) {
                    return ok_text(
                        "The user dismissed the form without answering. Continue without their input, \
                         or explain what you need.",
                    );
                }
                ok_text(serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string()))
            }
            // Sender dropped: the socket closed.
            Ok(Err(_)) => {
                self.bridge.forget_ask(&request_id);
                Err(ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    "the app window closed before the user answered".to_string(),
                    None,
                ))
            }
            Err(_) => {
                self.bridge.forget_ask(&request_id);
                self.bridge
                    .emit(json!({ "cmd": "ask_close", "requestId": request_id }))?;
                ok_text(format!(
                    "The user did not answer within {}s. Proceed with sensible defaults and say what you assumed.",
                    timeout.as_secs()
                ))
            }
        }
    }
}

impl AppControlServer {
    /// Shared tail of `ui_chart` / `ui_graph`: render into an explicit target, or
    /// wrap in a dock panel when none was given.
    fn emit_visual(
        &self,
        target: Option<String>,
        id: Option<String>,
        kind: &str,
        node: Value,
        spec: &Value,
    ) -> Result<CallToolResult, ErrorData> {
        let title = spec
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_string);
        match target {
            Some(t) => {
                validate_target(&t)?;
                self.bridge.emit(json!({
                    "cmd": "render",
                    "target": t,
                    "mode": "replace",
                    "body": [node],
                }))?;
                ok_text(format!("Drew the {kind} into {t}."))
            }
            None => {
                let panel_id = id.unwrap_or_else(|| self.bridge.next_id(kind));
                let evicted = self.note_panel(&panel_id);
                if let Some(old) = &evicted {
                    self.bridge
                        .emit(json!({ "cmd": "panel", "id": old, "remove": true }))?;
                }
                self.bridge.emit(json!({
                    "cmd": "panel",
                    "id": panel_id,
                    "title": title,
                    "place": "dock",
                    "collapsible": true,
                    "body": [node],
                }))?;
                ok_text(format!("Drew the {kind} in panel \"{panel_id}\"."))
            }
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AppControlServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: "biorouter-appcontrol".to_string(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
                title: None,
                icons: None,
                website_url: None,
            },
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            instructions: Some(
                "You can drive this app's interface, not just write text into it. Build panels \
                 and dashboards (ui_panel), render into the author's own regions (ui_render), \
                 draw charts and graphs (ui_chart / ui_graph), direct attention (ui_highlight), \
                 restyle (ui_theme), rearrange (ui_layout), notify (ui_notify), share state \
                 (ui_state), and ask the user structured questions mid-turn (ui_ask). Call \
                 ui_describe first to learn what the page already offers."
                    .to_string(),
            ),
            ..Default::default()
        }
    }
}

/// The system-prompt section appended for apps that hold the `ui` capability.
/// Tells the model the tools exist and, crucially, *when* to reach for them.
pub fn ui_system_prompt(cap: &UiCapability) -> String {
    let mut s = String::from(
        "\n\n## Driving this app's interface\n\
         You are not limited to writing text. This app grants you `ui_*` tools that change the \
         page the user is looking at. Two rules override your usual habits:\n\n\
         **1. Never ask the user for information in prose.** If you are missing an input — data \
         to analyse, a threshold, a cohort, a file, a confirmation — call `ui_ask`. It renders a \
         form and returns their answers to you as the tool result, so you keep going in the same \
         turn. Writing \"please paste your data\" and stopping is a failure: many of these apps \
         have no chat box at all, so the user has nowhere to answer you. Check `hasChat` in \
         `ui_describe` if unsure.\n\n\
         **2. Put structured results on the page, not in a paragraph.** Before you finish a turn, \
         anything with structure belongs in the UI:\n\
         - Comparisons, distributions, time series → `ui_chart`.\n\
         - Entities and their relationships → `ui_graph`.\n\
         - Several metrics at once → a `ui_panel` of `stat` cards, or `ui_layout(\"dashboard\")`.\n\
         - Rows of evidence → a `table` node via `ui_panel` / `ui_render`.\n\
         - Pointing at something already on screen → `ui_highlight`.\n\
         - Long jobs → `ui_notify` for progress.\n\n\
         Call `ui_describe` once, up front, so you render into regions that actually exist. \
         Panels are addressed by a stable `id`: reuse the id to refresh a panel in place rather \
         than stacking near-duplicates. Then keep your prose short — say what changed and why it \
         matters, not what the panel already shows.\n\n\
         Be economical with tool calls — you have a bounded number of actions per turn. Call \
         `ui_describe` ONCE (the page rarely changes mid-turn). Do NOT re-send `ui_state` values \
         you already set — set a key only when it actually changes; identical repeats do nothing \
         and waste the budget. Batch your UI updates: build each panel/chart once with its final \
         content rather than nudging it repeatedly. A turn is for acting on the user's input, not \
         for polling the page.",
    );
    if !cap.allow_ask {
        s.push_str("\n\nNote: `ui_ask` is disabled for this app.");
    }
    if !cap.allow_theme {
        s.push_str("\n\nNote: `ui_theme` is disabled for this app.");
    }
    if !cap.allow_layout {
        s.push_str("\n\nNote: `ui_layout` is disabled for this app.");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> (AppControlServer, mpsc::UnboundedReceiver<Value>) {
        server_with_surface(SurfaceDecl::default())
    }

    fn server_with_surface(
        surface: SurfaceDecl,
    ) -> (AppControlServer, mpsc::UnboundedReceiver<Value>) {
        let bridge = UiBridge::new();
        let (rx, _tok) = bridge.attach();
        (
            AppControlServer::new(bridge, UiCapability::default(), surface),
            rx,
        )
    }

    fn text_of(r: &CallToolResult) -> String {
        r.content
            .iter()
            .flat_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn validates_widget_kinds_and_shapes() {
        let mut b = MAX_WIDGET_NODES;
        assert!(validate_widget(&json!({"t":"text","value":"hi"}), 0, &mut b).is_ok());

        let mut b = MAX_WIDGET_NODES;
        let e = validate_widget(&json!({"t":"blah"}), 0, &mut b).unwrap_err();
        assert!(e.contains("unknown widget type"), "{e}");

        let mut b = MAX_WIDGET_NODES;
        let e = validate_widget(&json!({"t":"text"}), 0, &mut b).unwrap_err();
        assert!(e.contains("string \"value\""), "{e}");

        // A card must declare children.
        let mut b = MAX_WIDGET_NODES;
        assert!(validate_widget(&json!({"t":"card","title":"x"}), 0, &mut b).is_err());
    }

    #[test]
    fn rejects_ragged_tables() {
        let mut b = MAX_WIDGET_NODES;
        let node = json!({"t":"table","columns":["a","b"],"rows":[["1","2"],["3"]]});
        let e = validate_widget(&node, 0, &mut b).unwrap_err();
        assert!(e.contains("row 1 has 1 cells"), "{e}");
    }

    #[test]
    fn rejects_nonfinite_and_empty_charts() {
        assert!(validate_chart(&json!({"data":[]})).is_err());
        assert!(validate_chart(&json!({"type":"donut","data":[{"label":"a","value":1}]})).is_err());
        assert!(validate_chart(&json!({"data":[{"label":"a"}]})).is_err());
        assert!(validate_chart(&json!({"data":[{"label":"a","value":1.5}]})).is_ok());
    }

    /// H2: multi-series charts (train vs val loss, comparisons, forecasts).
    #[test]
    fn accepts_multi_series_charts() {
        let ok = json!({"type":"line","title":"Loss","series":[
            {"name":"train","data":[{"label":"e1","value":0.9},{"label":"e2","value":0.5}]},
            {"name":"val","data":[{"label":"e1","value":1.0},{"label":"e2","value":0.7}]}
        ]});
        assert!(validate_chart(&ok).is_ok(), "two-series line must validate");
        // a series missing its data array is a precise error
        let bad = json!({"type":"line","series":[{"name":"x"}]});
        let e = bad_err(&bad);
        assert!(e.contains("series[0]") && e.contains("data"), "{e}");
        // a non-finite point inside a series is caught with a series-scoped path
        let nf = json!({"series":[{"name":"a","data":[{"label":"p","value":1}]},
                                  {"name":"b","data":[{"label":"p","value":"NaN"}]}]});
        assert!(bad_err(&nf).contains("series[1].data"));
        // empty series array rejected
        assert!(validate_chart(&json!({"series":[]})).is_err());
        // the single-series error now points the model at `series` too
        assert!(bad_err(&json!({"type":"bar"})).contains("series"));
    }

    fn bad_err(spec: &Value) -> String {
        validate_chart(spec).unwrap_err()
    }

    #[test]
    fn widget_depth_and_node_budget_are_bounded() {
        // Build a chain deeper than MAX_WIDGET_DEPTH.
        let mut node = json!({"t":"text","value":"leaf"});
        for _ in 0..(MAX_WIDGET_DEPTH + 2) {
            node = json!({"t":"col","children":[node]});
        }
        let mut b = MAX_WIDGET_NODES;
        let e = validate_widget(&node, 0, &mut b).unwrap_err();
        assert!(e.contains("nested deeper"), "{e}");

        // And a wide-but-shallow tree exhausts the node budget.
        let children: Vec<Value> = (0..MAX_WIDGET_NODES + 5)
            .map(|i| json!({"t":"text","value": i.to_string()}))
            .collect();
        let mut b = MAX_WIDGET_NODES;
        let e = validate_widget(&json!({"t":"col","children":children}), 0, &mut b).unwrap_err();
        assert!(e.contains("exceeds"), "{e}");
    }

    #[tokio::test]
    async fn panel_emits_a_ui_frame() {
        let (s, mut rx) = server();
        let r = s
            .ui_panel(Parameters(PanelParams {
                id: "summary".into(),
                title: Some("Summary".into()),
                place: None,
                body: Some(json!([{"t":"text","value":"hello"}])),
                markdown: None,
                collapsible: None,
                remove: None,
            }))
            .await
            .unwrap();
        assert!(text_of(&r).contains("summary"));
        let cmd = rx.try_recv().unwrap();
        assert_eq!(cmd["type"], "ui");
        assert_eq!(cmd["cmd"], "panel");
        assert_eq!(cmd["id"], "summary");
        assert_eq!(cmd["place"], "dock");
    }

    /// H1: a real model (GPT-5.5) naturally mounts a dashboard into an author
    /// region with `place:"@region:dashboard"`. That used to be rejected
    /// ("must be one of: dock, left, …") and cost 5–7 retries. It must be
    /// accepted and passed through as the place.
    #[tokio::test]
    async fn panel_accepts_a_region_target_as_place() {
        let (s, mut rx) = server();
        let r = s
            .ui_panel(Parameters(PanelParams {
                id: "sentiment-dashboard".into(),
                title: Some("Sentiment".into()),
                place: Some("@region:dashboard".into()),
                body: Some(
                    json!([{"t":"row","children":[{"t":"stat","label":"Positive","value":2}]}]),
                ),
                markdown: None,
                collapsible: Some(false),
                remove: None,
            }))
            .await
            .expect("a @region: place must be accepted");
        assert!(text_of(&r).contains("dashboard"));
        let cmd = rx.try_recv().unwrap();
        assert_eq!(cmd["place"], "@region:dashboard");
    }

    #[tokio::test]
    async fn panel_still_rejects_an_empty_place_target() {
        let (s, _rx) = server();
        // An empty/whitespace place is neither a dock slot nor a valid target.
        let e = s
            .ui_panel(Parameters(PanelParams {
                id: "x".into(),
                title: None,
                place: Some("   ".into()),
                body: Some(json!([{"t":"text","value":"y"}])),
                markdown: None,
                collapsible: None,
                remove: None,
            }))
            .await
            .unwrap_err();
        assert!(e.message.to_lowercase().contains("empty") || e.message.contains("target"));
    }

    #[tokio::test]
    async fn panel_markdown_shorthand_becomes_a_text_node() {
        let (s, mut rx) = server();
        s.ui_panel(Parameters(PanelParams {
            id: "notes".into(),
            title: None,
            place: Some("right".into()),
            body: None,
            markdown: Some("# Hi".into()),
            collapsible: None,
            remove: None,
        }))
        .await
        .unwrap();
        let cmd = rx.try_recv().unwrap();
        assert_eq!(cmd["body"][0]["t"], "text");
        assert_eq!(cmd["body"][0]["markdown"], true);
    }

    #[tokio::test]
    async fn panel_cap_evicts_the_oldest() {
        let bridge = UiBridge::new();
        let (mut rx, _tok) = bridge.attach();
        let cap = UiCapability {
            max_panels: 2,
            ..Default::default()
        };
        let s = AppControlServer::new(bridge, cap, SurfaceDecl::default());
        for id in ["a", "b", "c"] {
            s.ui_panel(Parameters(PanelParams {
                id: id.into(),
                title: None,
                place: None,
                body: Some(json!([{"t":"text","value":"x"}])),
                markdown: None,
                collapsible: None,
                remove: None,
            }))
            .await
            .unwrap();
        }
        let cmds: Vec<Value> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        // a, b, then (remove a), c
        let removes: Vec<&Value> = cmds.iter().filter(|c| c["remove"] == true).collect();
        assert_eq!(removes.len(), 1);
        assert_eq!(removes[0]["id"], "a");
    }

    #[tokio::test]
    async fn theme_rejects_css_injection_in_accent() {
        let (s, _rx) = server();
        let e = s
            .ui_theme(Parameters(ThemeParams {
                accent: Some("red; background:url(http://x)".into()),
                mode: None,
                density: None,
            }))
            .await
            .unwrap_err();
        assert!(e.message.contains("simple CSS colour"), "{}", e.message);
    }

    #[tokio::test]
    async fn theme_and_layout_respect_capability_gates() {
        let bridge = UiBridge::new();
        let (_rx, _tok) = bridge.attach();
        let cap = UiCapability {
            allow_theme: false,
            allow_layout: false,
            ..Default::default()
        };
        let s = AppControlServer::new(bridge, cap, SurfaceDecl::default());
        assert!(s
            .ui_theme(Parameters(ThemeParams {
                accent: None,
                mode: Some("dark".into()),
                density: None
            }))
            .await
            .is_err());
        assert!(s
            .ui_layout(Parameters(LayoutParams {
                preset: "dashboard".into(),
                sidebar_width: None
            }))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn state_round_trips_and_emits() {
        let (s, mut rx) = server();
        let r = s
            .ui_state(Parameters(StateParams {
                set: Some(json!({"gene":"TP53","n":3})),
                remove: None,
            }))
            .await
            .unwrap();
        assert!(text_of(&r).contains("TP53"));
        let cmd = rx.try_recv().unwrap();
        assert_eq!(cmd["cmd"], "state");
        assert_eq!(cmd["mode"], "snapshot");
        assert_eq!(cmd["doc"]["gene"], "TP53");
        assert_eq!(cmd["version"], 1);

        let r = s
            .ui_state(Parameters(StateParams {
                set: None,
                remove: Some(vec!["gene".into()]),
            }))
            .await
            .unwrap();
        assert!(!text_of(&r).contains("TP53"));
    }

    /// H3: re-sending the SAME state is a cheap no-op that emits no frame and
    /// tells the model so — this is what breaks the ui_state cascade that
    /// exhausts the turn budget.
    #[tokio::test]
    async fn state_noop_when_unchanged_emits_nothing() {
        let (s, mut rx) = server();
        s.ui_state(Parameters(StateParams {
            set: Some(json!({"dose": 1000})),
            remove: None,
        }))
        .await
        .unwrap();
        assert_eq!(rx.try_recv().unwrap()["cmd"], "state"); // first change emits

        // identical re-send: no frame, "no change" message
        let r = s
            .ui_state(Parameters(StateParams {
                set: Some(json!({"dose": 1000})),
                remove: None,
            }))
            .await
            .unwrap();
        assert!(
            text_of(&r).to_lowercase().contains("no change"),
            "{}",
            text_of(&r)
        );
        assert!(
            rx.try_recv().is_err(),
            "an unchanged ui_state must emit no frame"
        );

        // a real change emits again
        s.ui_state(Parameters(StateParams {
            set: Some(json!({"dose": 1250})),
            remove: None,
        }))
        .await
        .unwrap();
        assert_eq!(rx.try_recv().unwrap()["doc"]["dose"], 1250);
    }

    /// H3: a repeated ui_describe over an unchanged page is flagged so the model
    /// stops re-polling.
    #[tokio::test]
    async fn describe_flags_an_unchanged_repeat() {
        let (s, _rx) = server();
        let first = text_of(&s.ui_describe(Parameters(DescribeParams {})).await.unwrap());
        assert!(!first.to_lowercase().contains("unchanged"));
        let second = text_of(&s.ui_describe(Parameters(DescribeParams {})).await.unwrap());
        assert!(second.to_lowercase().contains("unchanged"), "{second}");
    }

    #[tokio::test]
    async fn ask_parks_then_resolves_with_the_users_payload() {
        let bridge = UiBridge::new();
        let (mut rx, _tok) = bridge.attach();
        let s = AppControlServer::new(
            bridge.clone(),
            UiCapability::default(),
            SurfaceDecl::default(),
        );

        let task = tokio::spawn(async move {
            s.ui_ask(Parameters(AskParams {
                prompt: "Pick a threshold".into(),
                fields: vec![AskField {
                    name: "p".into(),
                    label: None,
                    r#type: Some("number".into()),
                    options: None,
                    value: None,
                    placeholder: None,
                }],
                title: None,
                submit_label: None,
            }))
            .await
        });

        let cmd = loop {
            match rx.recv().await {
                Some(c) if c["cmd"] == "ask" => break c,
                Some(_) => continue,
                None => panic!("channel closed"),
            }
        };
        let rid = cmd["requestId"].as_str().unwrap().to_string();
        assert!(bridge.resolve(&rid, json!({"p": "0.05"})));

        let r = task.await.unwrap().unwrap();
        assert!(text_of(&r).contains("0.05"), "{}", text_of(&r));
    }

    #[tokio::test]
    async fn ask_returns_a_usable_result_when_the_user_dismisses() {
        let bridge = UiBridge::new();
        let (mut rx, _tok) = bridge.attach();
        let s = AppControlServer::new(
            bridge.clone(),
            UiCapability::default(),
            SurfaceDecl::default(),
        );
        let task = tokio::spawn(async move {
            s.ui_ask(Parameters(AskParams {
                prompt: "?".into(),
                fields: vec![AskField {
                    name: "x".into(),
                    label: None,
                    r#type: None,
                    options: None,
                    value: None,
                    placeholder: None,
                }],
                title: None,
                submit_label: None,
            }))
            .await
        });
        // Drain the emitted ask frame, then simulate the socket closing.
        let _ = rx.recv().await;
        bridge.cancel_all();
        let r = task.await.unwrap().unwrap();
        assert!(text_of(&r).contains("dismissed"), "{}", text_of(&r));
    }

    #[tokio::test]
    async fn ask_rejects_a_select_without_options() {
        let (s, _rx) = server();
        let e = s
            .ui_ask(Parameters(AskParams {
                prompt: "?".into(),
                fields: vec![AskField {
                    name: "k".into(),
                    label: None,
                    r#type: Some("select".into()),
                    options: None,
                    value: None,
                    placeholder: None,
                }],
                title: None,
                submit_label: None,
            }))
            .await
            .unwrap_err();
        assert!(e.message.contains("non-empty \"options\""), "{}", e.message);
    }

    /// A reload reuses the cached agent (and therefore the already-injected
    /// `AppControlServer`). Re-attaching must point its tools at the NEW socket,
    /// otherwise every `ui_*` call after the first reload fails forever.
    #[tokio::test]
    async fn rebinding_the_bridge_keeps_a_reused_server_working() {
        let bridge = UiBridge::new();
        let (first, _tok1) = bridge.attach();
        let s = AppControlServer::new(
            bridge.clone(),
            UiCapability::default(),
            SurfaceDecl::default(),
        );

        // The browser reloads: old channel dropped, new one attached.
        drop(first);
        let (mut second, _tok2) = bridge.attach();

        s.ui_notify(Parameters(NotifyParams {
            message: "still here".into(),
            level: None,
            timeout_ms: None,
        }))
        .await
        .expect("ui_* must work after a reconnect");
        let cmd = second.try_recv().unwrap();
        assert_eq!(cmd["cmd"], "notify");
        assert_eq!(cmd["message"], "still here");
    }

    /// A reload race: the new connection attaches, then the OLD connection's
    /// socket-close handler finally runs `detach`. Without the generation guard,
    /// that stale detach would null the fresh connection's channel and cancel its
    /// parked `ui_ask`. The token makes the stale detach a no-op.
    #[tokio::test]
    async fn a_stale_detach_after_reattach_is_a_noop() {
        let bridge = UiBridge::new();
        let (_first_rx, first_tok) = bridge.attach();
        let s = AppControlServer::new(
            bridge.clone(),
            UiCapability::default(),
            SurfaceDecl::default(),
        );

        // Browser reloads: fresh connection attaches…
        let (mut second_rx, _second_tok) = bridge.attach();
        // …then the OLD connection's close handler fires, late.
        bridge.detach(first_tok);

        // The fresh connection's tools must still work.
        s.ui_notify(Parameters(NotifyParams {
            message: "alive".into(),
            level: None,
            timeout_ms: None,
        }))
        .await
        .expect("a stale detach must not tear down the fresh connection");
        assert_eq!(second_rx.try_recv().unwrap()["message"], "alive");

        // And a parked ask on the fresh connection is NOT cancelled by it.
        let s2 = s.clone();
        let ask = tokio::spawn(async move {
            s2.ui_ask(Parameters(AskParams {
                prompt: "?".into(),
                fields: vec![AskField {
                    name: "x".into(),
                    label: None,
                    r#type: None,
                    options: None,
                    value: None,
                    placeholder: None,
                }],
                title: None,
                submit_label: None,
            }))
            .await
        });
        // Drain the ask frame, fire the stale detach again — still a no-op.
        loop {
            match second_rx.recv().await {
                Some(c) if c["cmd"] == "ask" => break,
                Some(_) => continue,
                None => panic!("channel closed"),
            }
        }
        bridge.detach(first_tok);
        // The ask is still parked (not cancelled): a short wait must time out.
        assert!(
            tokio::time::timeout(Duration::from_millis(300), ask)
                .await
                .is_err(),
            "a stale detach must not cancel the fresh connection's parked ask"
        );
    }

    /// Panels are per-connection (the reloaded DOM has none), but the semantic
    /// state bag survives and is replayed so the fresh page rehydrates.
    #[tokio::test]
    async fn attach_resets_panels_and_replays_state() {
        let bridge = UiBridge::new();
        let (mut first, _tok1) = bridge.attach();
        let s = AppControlServer::new(
            bridge.clone(),
            UiCapability::default(),
            SurfaceDecl::default(),
        );
        s.ui_state(Parameters(StateParams {
            set: Some(json!({"gene":"BRCA1"})),
            remove: None,
        }))
        .await
        .unwrap();
        s.ui_panel(Parameters(PanelParams {
            id: "p".into(),
            title: None,
            place: None,
            body: Some(json!([{"t":"text","value":"x"}])),
            markdown: None,
            collapsible: None,
            remove: None,
        }))
        .await
        .unwrap();
        while first.try_recv().is_ok() {}

        let (mut second, _tok2) = bridge.attach();
        // State is replayed to the fresh page as one snapshot frame…
        let replay = second.try_recv().unwrap();
        assert_eq!(replay["cmd"], "state");
        assert_eq!(replay["mode"], "snapshot");
        assert_eq!(replay["doc"]["gene"], "BRCA1");
        assert_eq!(replay["v"], 1);
        // …and the panel registry starts empty again.
        let r = s.ui_describe(Parameters(DescribeParams {})).await.unwrap();
        let described: Value = serde_json::from_str(&text_of(&r)).unwrap();
        assert_eq!(described["panels"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn detach_unblocks_a_parked_ask() {
        let bridge = UiBridge::new();
        let (mut rx, tok) = bridge.attach();
        let s = AppControlServer::new(
            bridge.clone(),
            UiCapability::default(),
            SurfaceDecl::default(),
        );
        let task = tokio::spawn(async move {
            s.ui_ask(Parameters(AskParams {
                prompt: "?".into(),
                fields: vec![AskField {
                    name: "x".into(),
                    label: None,
                    r#type: None,
                    options: None,
                    value: None,
                    placeholder: None,
                }],
                title: None,
                submit_label: None,
            }))
            .await
        });
        let _ = rx.recv().await;
        bridge.detach(tok);
        // Must not hang: detach() cancels every parked ask.
        let r = tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .expect("ui_ask should not outlive its socket")
            .unwrap()
            .unwrap();
        assert!(text_of(&r).contains("dismissed"));
    }

    /// Live runs against a small local model showed the failure this text exists
    /// to prevent: asked for missing data, the agent wrote "please paste your
    /// abstracts" and ended the turn — into an app with no chat box. The prompt
    /// must state the `ui_ask` rule imperatively, not as a preference.
    #[test]
    fn system_prompt_forbids_asking_in_prose() {
        let s = ui_system_prompt(&UiCapability::default());
        assert!(
            s.contains("Never ask the user for information in prose"),
            "the ui_ask rule must be imperative"
        );
        assert!(s.contains("`ui_ask`"));
        assert!(
            s.contains("hasChat"),
            "tell the agent how to check for a chat box"
        );
        // And every tool the capability grants is named, so the model knows they exist.
        for tool in [
            "ui_chart",
            "ui_graph",
            "ui_panel",
            "ui_highlight",
            "ui_notify",
            "ui_describe",
        ] {
            assert!(s.contains(tool), "system prompt never mentions {tool}");
        }
    }

    #[test]
    fn system_prompt_announces_revoked_switches() {
        let cap = UiCapability {
            allow_ask: false,
            allow_theme: false,
            allow_layout: false,
            ..Default::default()
        };
        let s = ui_system_prompt(&cap);
        assert!(s.contains("`ui_ask` is disabled"));
        assert!(s.contains("`ui_theme` is disabled"));
        assert!(s.contains("`ui_layout` is disabled"));
    }

    #[tokio::test]
    async fn tools_fail_cleanly_once_the_socket_is_gone() {
        let (s, rx) = server();
        drop(rx);
        let e = s
            .ui_notify(Parameters(NotifyParams {
                message: "hi".into(),
                level: None,
                timeout_ms: None,
            }))
            .await
            .unwrap_err();
        assert!(e.message.contains("no longer connected"), "{}", e.message);
    }

    #[tokio::test]
    async fn describe_reports_surface_panels_and_state() {
        let bridge = UiBridge::new();
        let (_rx, _tok) = bridge.attach();
        let s = AppControlServer::new(
            bridge.clone(),
            UiCapability::default(),
            SurfaceDecl::default(),
        );
        bridge.set_surface(json!({"regions":["results"],"ids":["out"]}));
        s.ui_panel(Parameters(PanelParams {
            id: "p1".into(),
            title: None,
            place: None,
            body: Some(json!([{"t":"text","value":"x"}])),
            markdown: None,
            collapsible: None,
            remove: None,
        }))
        .await
        .unwrap();
        let r = s.ui_describe(Parameters(DescribeParams {})).await.unwrap();
        let t = text_of(&r);
        assert!(t.contains("results"), "{t}");
        assert!(t.contains("p1"), "{t}");
    }

    /// Observed live with qwen3.6 via Ollama: because `spec` is a bare `Value`,
    /// the model JSON-*encodes* it into a string. It then retried three times and
    /// never recovered. Accept it.
    #[tokio::test]
    async fn chart_accepts_a_spec_the_model_stringified() {
        let (s, mut rx) = server();
        s.ui_chart(Parameters(ChartParams {
            target: None,
            id: Some("gene-counts".into()),
            spec: json!(
                r#"{"type": "bar", "title": "Gene Counts", "data": [{"label": "TP53", "value": 120}]}"#
            ),
        }))
        .await
        .expect("a stringified spec must be accepted, not bounced back");
        let cmd = rx.try_recv().unwrap();
        assert_eq!(cmd["title"], "Gene Counts");
        assert_eq!(cmd["body"][0]["spec"]["data"][0]["label"], "TP53");
        assert_eq!(cmd["body"][0]["spec"]["type"], "bar");
    }

    #[tokio::test]
    async fn render_and_panel_and_state_accept_stringified_json() {
        let (s, mut rx) = server();
        s.ui_render(Parameters(RenderParams {
            target: "@region:results".into(),
            body: json!(r#"[{"t": "text", "value": "hi"}]"#),
            mode: None,
        }))
        .await
        .unwrap();
        assert_eq!(rx.try_recv().unwrap()["body"][0]["value"], "hi");

        s.ui_panel(Parameters(PanelParams {
            id: "p".into(),
            title: None,
            place: None,
            body: Some(json!(r#"[{"t": "badge", "value": "b"}]"#)),
            markdown: None,
            collapsible: None,
            remove: None,
        }))
        .await
        .unwrap();
        assert_eq!(rx.try_recv().unwrap()["body"][0]["t"], "badge");

        s.ui_state(Parameters(StateParams {
            set: Some(json!(r#"{"gene": "TP53"}"#)),
            remove: None,
        }))
        .await
        .unwrap();
        assert_eq!(rx.try_recv().unwrap()["doc"]["gene"], "TP53");
    }

    /// A string that isn't JSON at all still fails — but with a message that
    /// names the mistake instead of a bare "must be an object".
    #[tokio::test]
    async fn a_non_json_string_fails_with_an_actionable_message() {
        let (s, _rx) = server();
        let e = s
            .ui_chart(Parameters(ChartParams {
                target: None,
                id: None,
                spec: json!("just some prose"),
            }))
            .await
            .unwrap_err();
        assert!(e.message.contains("not a JSON string"), "{}", e.message);
        assert!(e.message.contains(r#"{"type":"bar""#), "{}", e.message);
    }

    /// A bare `Value` generates a permissive `true` schema, which is what made
    /// the model guess. The tool must advertise the real shape.
    #[test]
    fn chart_and_graph_advertise_a_real_spec_schema() {
        let router = AppControlServer::tool_router();
        let tools: Vec<_> = router.list_all();
        let chart = tools
            .iter()
            .find(|t| t.name == "ui_chart")
            .expect("ui_chart");
        let spec = &chart.input_schema["properties"]["spec"];
        assert_eq!(spec["type"], "object", "spec schema: {spec}");
        assert!(
            spec["properties"]["data"].is_object(),
            "spec schema: {spec}"
        );

        let graph = tools
            .iter()
            .find(|t| t.name == "ui_graph")
            .expect("ui_graph");
        let gspec = &graph.input_schema["properties"]["spec"];
        assert_eq!(gspec["type"], "object", "graph spec schema: {gspec}");

        let render = tools
            .iter()
            .find(|t| t.name == "ui_render")
            .expect("ui_render");
        let body = &render.input_schema["properties"]["body"];
        assert_eq!(body["type"], "array", "body schema: {body}");
    }

    /// Subschemas must be inlined. `$ref`/`$defs` are legal JSON Schema, but
    /// several providers (notably Ollama-hosted models) mishandle them, and a
    /// tool whose schema the model can't read is a tool it guesses at.
    #[test]
    fn tool_schemas_contain_no_refs() {
        for tool in AppControlServer::tool_router().list_all() {
            let schema = serde_json::to_string(&tool.input_schema).unwrap();
            assert!(
                !schema.contains("$ref") && !schema.contains("$defs"),
                "{} schema must be self-contained: {schema}",
                tool.name
            );
        }
    }

    #[tokio::test]
    async fn chart_without_target_wraps_itself_in_a_dock_panel() {
        let (s, mut rx) = server();
        s.ui_chart(Parameters(ChartParams {
            target: None,
            id: None,
            spec: json!({"type":"bar","title":"Counts","data":[{"label":"a","value":1}]}),
        }))
        .await
        .unwrap();
        let cmd = rx.try_recv().unwrap();
        assert_eq!(cmd["cmd"], "panel");
        assert_eq!(cmd["place"], "dock");
        assert_eq!(cmd["title"], "Counts");
        assert_eq!(cmd["body"][0]["t"], "chart");
    }

    // ── Apps SDK v2: shared state document ──────────────────────────────────

    /// Every emitted ui frame is stamped with the catalog version.
    #[tokio::test]
    async fn ui_frames_carry_the_catalog_version() {
        let (s, mut rx) = server();
        s.ui_notify(Parameters(NotifyParams {
            message: "hi".into(),
            level: None,
            timeout_ms: None,
        }))
        .await
        .unwrap();
        assert_eq!(rx.try_recv().unwrap()["v"], CATALOG_VERSION);
    }

    #[tokio::test]
    async fn patch_state_applies_and_bumps_version() {
        let bridge = UiBridge::new();
        let (mut rx, _tok) = bridge.attach();
        let s = AppControlServer::new(
            bridge.clone(),
            UiCapability::default(),
            SurfaceDecl::default(),
        );
        let r = s
            .ui_patch_state(Parameters(PatchStateParams {
                patch: json!([{"op":"add","path":"/count","value":3}]),
            }))
            .await
            .unwrap();
        assert!(text_of(&r).contains("\"version\":1"), "{}", text_of(&r));
        assert!(text_of(&r).contains("\"ok\":true"), "{}", text_of(&r));

        let cmd = rx.try_recv().unwrap();
        assert_eq!(cmd["cmd"], "state");
        assert_eq!(cmd["mode"], "patch");
        assert_eq!(cmd["version"], 1);
        assert_eq!(cmd["patch"][0]["op"], "add");
        assert_eq!(cmd["v"], CATALOG_VERSION);

        let (doc, version) = bridge.state_snapshot();
        assert_eq!(doc["count"], 3);
        assert_eq!(version, 1);
    }

    #[tokio::test]
    async fn patch_state_rejects_too_many_ops() {
        let (s, _rx) = server();
        let ops: Vec<Value> = (0..(STATE_MAX_PATCH_OPS + 1))
            .map(|i| json!({"op":"add","path":format!("/k{i}"),"value":i}))
            .collect();
        let e = s
            .ui_patch_state(Parameters(PatchStateParams { patch: json!(ops) }))
            .await
            .unwrap_err();
        assert!(e.message.contains("cap is 64"), "{}", e.message);
    }

    #[test]
    fn validate_state_doc_enforces_the_size_cap() {
        let big = "x".repeat(STATE_MAX_BYTES + 1_000);
        let e = validate_state_doc(&json!({ "blob": big })).unwrap_err();
        assert!(e.contains("bytes"), "{e}");
        assert!(validate_state_doc(&json!({ "a": 1 })).is_ok());
    }

    #[test]
    fn validate_state_doc_enforces_the_depth_cap() {
        // Nest well past the depth cap.
        let mut v = json!(1);
        for _ in 0..(STATE_MAX_DEPTH + 4) {
            v = json!({ "n": v });
        }
        let e = validate_state_doc(&v).unwrap_err();
        assert!(e.contains("nested deeper"), "{e}");
        // A shallow document is fine.
        assert!(validate_state_doc(&json!({ "a": { "b": 1 } })).is_ok());
    }

    /// A patch that would blow a cap is rejected and the live document is left
    /// untouched (applied against a clone, committed only when valid).
    #[tokio::test]
    async fn patch_state_over_cap_leaves_the_doc_unchanged() {
        let bridge = UiBridge::new();
        let (_rx, _tok) = bridge.attach();
        let s = AppControlServer::new(
            bridge.clone(),
            UiCapability::default(),
            SurfaceDecl::default(),
        );
        let big = "x".repeat(STATE_MAX_BYTES + 1_000);
        let e = s
            .ui_patch_state(Parameters(PatchStateParams {
                patch: json!([{"op":"add","path":"/blob","value":big}]),
            }))
            .await
            .unwrap_err();
        assert!(e.message.contains("bytes"), "{}", e.message);
        let (doc, version) = bridge.state_snapshot();
        assert_eq!(version, 0, "a rejected patch must not bump the version");
        assert!(doc.as_object().unwrap().is_empty());
    }

    #[test]
    fn client_write_conflict_returns_the_current_doc() {
        let bridge = UiBridge::new();
        // First write advances the version to 1.
        let (_ops, v1) = bridge
            .apply_client_write(Some(("/x".into(), json!(1))), None, 0)
            .expect("first write applies");
        assert_eq!(v1, 1);
        // A stale write against base_version 0 conflicts and hands back the truth.
        match bridge.apply_client_write(Some(("/y".into(), json!(2))), None, 0) {
            Err(StateWriteError::Conflict(doc, ver)) => {
                assert_eq!(ver, 1);
                assert_eq!(doc["x"], 1);
                assert!(doc.get("y").is_none(), "the losing write must not apply");
            }
            other => panic!("expected a Conflict, got {other:?}"),
        }
    }

    #[test]
    fn client_write_pointer_set_creates_intermediate_objects() {
        let bridge = UiBridge::new();
        let (ops, ver) = bridge
            .apply_client_write(Some(("/a/b/c".into(), json!(5))), None, 0)
            .expect("pointer set applies");
        assert_eq!(ver, 1);
        assert_eq!(ops[0]["op"], "add");
        assert_eq!(ops[0]["path"], "/a/b/c");
        let (doc, _) = bridge.state_snapshot();
        assert_eq!(doc["a"]["b"]["c"], 5);
    }

    #[test]
    fn client_write_patch_round_trips() {
        let bridge = UiBridge::new();
        let (ops, ver) = bridge
            .apply_client_write(
                None,
                Some(json!([{"op":"add","path":"/hits","value":42}])),
                0,
            )
            .expect("patch applies");
        assert_eq!(ver, 1);
        assert_eq!(ops[0]["value"], 42);
        assert_eq!(bridge.state_snapshot().0["hits"], 42);
    }

    #[test]
    fn seed_state_only_applies_while_fresh() {
        let bridge = UiBridge::new();
        bridge.seed_state(json!({ "a": 1 }), 7);
        let (doc, ver) = bridge.state_snapshot();
        assert_eq!(ver, 7);
        assert_eq!(doc["a"], 1);
        // A second seed on a non-fresh doc is ignored.
        bridge.seed_state(json!({ "b": 2 }), 99);
        let (doc2, ver2) = bridge.state_snapshot();
        assert_eq!(ver2, 7);
        assert!(doc2.get("b").is_none());
    }

    /// A reconnect replays a single snapshot frame (not a `ui_state` bag), and
    /// only when there is state worth sending.
    #[tokio::test]
    async fn attach_replays_a_state_snapshot() {
        let bridge = UiBridge::new();
        // A fresh, empty session replays nothing.
        let (mut empty, _t0) = bridge.attach();
        assert!(empty.try_recv().is_err());

        bridge.seed_state(json!({ "gene": "BRCA1" }), 3);
        let (mut rx, _t1) = bridge.attach();
        let frame = rx.try_recv().unwrap();
        assert_eq!(frame["cmd"], "state");
        assert_eq!(frame["mode"], "snapshot");
        assert_eq!(frame["doc"]["gene"], "BRCA1");
        assert_eq!(frame["version"], 3);
        assert_eq!(frame["v"], CATALOG_VERSION);
    }

    #[tokio::test]
    async fn describe_reports_the_declared_surface() {
        use crate::agent_drafter::manifest::{ActionDecl, ComponentDecl, SignalDecl};
        let surface = SurfaceDecl {
            state_schema: Some(json!({ "type": "object" })),
            actions: vec![ActionDecl {
                name: "move_avatar".into(),
                description: "Move the avatar".into(),
                params: json!({ "type": "object" }),
            }],
            signals: vec![SignalDecl {
                name: "node_selected".into(),
                payload: None,
                coalesce_ms: 250,
            }],
            components: vec![ComponentDecl {
                name: "pathway_map".into(),
                props: json!({}),
            }],
        };
        let (s, _rx) = server_with_surface(surface);
        let r = s.ui_describe(Parameters(DescribeParams {})).await.unwrap();
        let d: Value = serde_json::from_str(&text_of(&r)).unwrap();
        assert_eq!(d["declared"]["hasStateSchema"], true);
        assert_eq!(d["declared"]["actions"][0]["name"], "move_avatar");
        assert_eq!(
            d["declared"]["actions"][0]["description"],
            "Move the avatar"
        );
        assert_eq!(d["declared"]["signals"][0]["name"], "node_selected");
        assert_eq!(d["declared"]["signals"][0]["coalesceMs"], 250);
        assert_eq!(d["declared"]["components"][0]["name"], "pathway_map");
        assert_eq!(d["state"]["version"], 0);
        assert!(d["state"]["keys"].as_array().unwrap().is_empty());
    }
}
