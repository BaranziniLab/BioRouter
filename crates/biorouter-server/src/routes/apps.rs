//! HTTP + WebSocket routes for **BioRouter apps** (built by Agent Drafter).
//!
//! `biorouterd` serves each app's static bundle and exposes a per-app WebSocket
//! that runs the *real* agent loop configured with that app's model, extensions,
//! skills and knowledge base. Browser-facing GET routes are exempt from the
//! secret-key middleware (see `auth::check_token`) since a browser tab can't send
//! the header — the daemon binds to localhost only, like the MCP UI proxy.
//!
//! Routes:
//!   GET    /apps                      → list app manifests (JSON)
//!   GET    /apps/{id}                 → redirect to /apps/{id}/
//!   GET    /apps/{id}/                → assembled index.html
//!   GET    /apps/{id}/dist/{*path}    → built bundle files
//!   GET    /apps/{id}/assets/{*path}  → static assets
//!   GET    /apps/{id}/agent           → per-app agent WebSocket
//!   POST   /apps/{id}/build           → (re)bundle the TypeScript (secret-key)
//!   DELETE /apps/{id}                 → delete the app (secret-key)

use std::collections::VecDeque;
use std::sync::{Arc, LazyLock, Mutex};

use axum::{
    extract::{
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::{header, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use biorouter::agents::extension::PLATFORM_EXTENSIONS;
use biorouter::agents::{AgentEvent, ExtensionConfig, SessionConfig};
use biorouter::conversation::message::{ActionRequiredData, Message, MessageContent};
use biorouter::guardrails::pii::PiiDetector;
use biorouter::guardrails::run_state::{PendingTool, RunState};
use biorouter::model::ModelConfig;
use biorouter::permission::permission_confirmation::PrincipalType;
use biorouter::permission::{Permission, PermissionConfirmation};
use biorouter::providers::create as create_provider;
use biorouter::session::SessionType;
use biorouter_mcp::agent_drafter::control::UiBridge;
use biorouter_mcp::agent_drafter::manifest::PiiMode;
use biorouter_mcp::agent_drafter::store::{ArtifactStore, Manifest};
use biorouter_mcp::agent_drafter::{
    bundle_is_stale, default_root, export_scaffold, rebuild_and_stamp,
};

use crate::state::AppState;

/// Safe default bound on an app agent's tool-calling loop per user message.
/// Workflow-style apps can raise this via `agent.max_turns` in the manifest.
const DEFAULT_MAX_TURNS: u32 = 24;

fn store() -> ArtifactStore {
    ArtifactStore::new(default_root())
}

fn mime_for(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

/// GET /apps — list all app manifests.
async fn list_apps() -> Json<Vec<Manifest>> {
    Json(store().list())
}

/// GET /apps/{id} — redirect to the trailing-slash form so relative URLs resolve.
async fn redirect_to_slash(Path(id): Path<String>) -> Redirect {
    Redirect::temporary(&format!("/apps/{id}/"))
}

/// GET /apps/{id}/ — the assembled, served index.html.
async fn serve_index(Path(id): Path<String>) -> Response {
    let st = store();
    let manifest = match st.load_manifest(&id) {
        Ok(m) => m,
        Err(_) => return (StatusCode::NOT_FOUND, "no such app").into_response(),
    };
    // Build on demand when the app has no bundle — or when its bundle predates
    // the App SDK this daemon ships. Apps vendor their own `src/sdk.ts`, so
    // without the second check an app authored before a protocol addition keeps
    // running the old runtime and silently ignores frames we now send.
    if bundle_is_stale(&st, &id, &manifest) {
        let st2 = st.clone();
        let id2 = id.clone();
        if let Ok(Ok(r)) = tokio::task::spawn_blocking(move || rebuild_and_stamp(&st2, &id2)).await
        {
            if !r.ok {
                warn!(app = %id, "on-demand build failed: {}", r.log);
            }
        }
    }
    let entry_html = match st.read_file(&id, &manifest.entry) {
        Ok(h) => h,
        Err(_) => return (StatusCode::NOT_FOUND, "entry missing").into_response(),
    };
    let base_href = format!("/apps/{id}/");
    let html = biorouter_mcp::agent_drafter::render::assemble_app(
        &manifest,
        &entry_html,
        Some(&base_href),
        None,
    );
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], html).into_response()
}

/// GET /apps/{id}/dist/{*path} and /apps/{id}/assets/{*path} — serve a file.
async fn serve_file(Path((id, sub)): Path<(String, String)>, prefix: &str) -> Response {
    let rel = format!("{prefix}/{sub}");
    match store().read_bytes(&id, &rel) {
        Ok(bytes) => ([(header::CONTENT_TYPE, mime_for(&rel))], bytes).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

async fn serve_dist(path: Path<(String, String)>) -> Response {
    serve_file(path, "dist").await
}

async fn serve_assets(path: Path<(String, String)>) -> Response {
    serve_file(path, "assets").await
}

/// POST /apps/{id}/build — (re)bundle the TypeScript.
async fn build_app_route(Path(id): Path<String>) -> Response {
    let st = store();
    if !st.exists(&id) {
        return (StatusCode::NOT_FOUND, "no such app").into_response();
    }
    let st2 = st.clone();
    let id2 = id.clone();
    match tokio::task::spawn_blocking(move || rebuild_and_stamp(&st2, &id2)).await {
        Ok(Ok(report)) => {
            Json(json!({ "ok": report.ok, "used": report.used, "log": report.log })).into_response()
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "build error").into_response(),
    }
}

/// GET /apps/{id}/export — return the standalone export scaffold as a JSON file
/// map ({ files: { "<path>": "<content>" } }). The caller writes the files,
/// `npm run build`, `npm start`, and runs a `biorouterd` for the agent backend.
async fn export_app_route(Path(id): Path<String>) -> Response {
    // export_scaffold may run esbuild (blocking) when a bundle is stale, so do
    // it off the async runtime to keep the server responsive under load.
    let id2 = id.clone();
    let result =
        tokio::task::spawn_blocking(move || export_scaffold(&default_root(), &id2, None)).await;
    match result {
        Ok(Ok(files)) => {
            let map: serde_json::Map<String, serde_json::Value> = files
                .into_iter()
                .map(|(p, c)| (p, serde_json::Value::String(c)))
                .collect();
            Json(json!({ "id": id, "files": map })).into_response()
        }
        _ => (StatusCode::NOT_FOUND, "no such app").into_response(),
    }
}

/// DELETE /apps/{id} — delete the app.
async fn delete_app_route(Path(id): Path<String>) -> Response {
    let st = store();
    if !st.exists(&id) {
        return (StatusCode::NOT_FOUND, "no such app").into_response();
    }
    match st.delete(&id) {
        Ok(_) => Json(json!({ "ok": true })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /apps/{id}/agent — per-app agent WebSocket.
async fn agent_ws(
    Path(id): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
    headers: axum::http::HeaderMap,
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
    // This socket runs full agent turns and carries its own tool-approval
    // frames, so whoever reaches it can prompt the agent and then approve the
    // agent's own tool calls. It is exempt from the secret-key middleware (a
    // browser-opened app cannot set request headers), and CORS does not govern
    // WebSocket handshakes -- so the origin has to be checked right here, or a
    // page on any web origin can drive it against the loopback port.
    if let Some(origin) = headers.get(axum::http::header::ORIGIN) {
        let origin = origin.to_str().unwrap_or_default();
        if !super::is_local_origin(origin) {
            tracing::warn!(%origin, app = %id, "rejected cross-origin app agent WebSocket");
            return (StatusCode::FORBIDDEN, "cross-origin connect rejected").into_response();
        }
    }

    let manifest = match store().load_manifest(&id) {
        Ok(m) => m,
        Err(_) => return (StatusCode::NOT_FOUND, "no such app").into_response(),
    };
    // Stable per-client handle for durable, resumable sessions (the SDK persists
    // it in localStorage and passes it as ?client_id=…).
    let client_id = params
        .get("client_id")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    ws.on_upgrade(move |socket| handle_agent_socket(socket, state, manifest, client_id))
}

#[derive(Deserialize)]
struct ImageInput {
    #[serde(rename = "mimeType")]
    mime_type: String,
    data: String,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum ClientFrame {
    Prompt {
        text: String,
        #[serde(default)]
        images: Vec<ImageInput>,
    },
    Cancel,
    /// BRSDK context API: request current token usage vs the model's window.
    Tokens,
    /// BRSDK durable sessions: request this connection's own message backlog so
    /// a reloaded app can repaint its chat. Served over the WS (which is already
    /// bound to the resolved session) — no guessable id, no auth-exempt route.
    History,
    /// BRSDK model surface: live-switch the session's provider/model.
    ModelSelect {
        #[serde(default)]
        provider: Option<String>,
        #[serde(default)]
        model: Option<String>,
    },
    /// BRSDK HITL: approve a pending tool. `action` ∈ {allow_once, always_allow}.
    Approve {
        request: String,
        #[serde(default, rename = "action")]
        _action: String,
    },
    /// BRSDK HITL: reject a pending tool, with an optional human reason.
    Reject {
        request: String,
        #[serde(default, rename = "reason")]
        _reason: Option<String>,
    },
    /// BRSDK widgets: a submit/button action from an agent-rendered widget, fed
    /// back into the agent as the next turn (closing the interactive loop).
    #[serde(rename = "widget_action")]
    WidgetAction {
        #[serde(rename = "widgetId", default)]
        widget_id: String,
        #[serde(default)]
        action: String,
        #[serde(default)]
        payload: serde_json::Value,
    },
    /// UI control: the answer to a `ui_ask`, resolving the tool call that is
    /// currently parked inside the agent's turn.
    #[serde(rename = "ui_reply")]
    UiReply {
        #[serde(rename = "requestId", default)]
        request_id: String,
        #[serde(default)]
        payload: serde_json::Value,
    },
    /// UI control: the browser's report of what it can be told to change —
    /// author-declared regions, element ids, mounted panels. Answers `ui_describe`.
    #[serde(rename = "ui_surface")]
    UiSurface {
        #[serde(default)]
        surface: serde_json::Value,
    },
}

/// The write half of an app's WebSocket. Split from the read half so the loop can
/// stream agent events, drain agent-issued UI commands, and read client frames
/// (a `ui_ask` answer, a `cancel`) *concurrently* — a `ui_ask` tool parks inside
/// `agent.reply`, so its answer must arrive while the reply stream is pending.
type WsSink = SplitSink<WebSocket, WsMessage>;
type WsSource = SplitStream<WebSocket>;

async fn send_json(sink: &mut WsSink, value: serde_json::Value) -> bool {
    sink.send(WsMessage::Text(value.to_string().into()))
        .await
        .is_ok()
}

/// Live [`UiBridge`]s keyed by session id.
///
/// `AppState::get_agent` caches one agent per session and `add_inprocess_server`
/// is idempotent by name, so a reconnecting browser reuses the `AppControlServer`
/// injected by the first connection. We must therefore hand that *same* bridge
/// back and rebind it to the new socket (`UiBridge::attach`), or the `ui_*` tools
/// would keep writing into the closed connection's channel. Entries are retained
/// for the life of the process, mirroring the agent cache they shadow.
static UI_BRIDGES: LazyLock<Mutex<std::collections::HashMap<String, UiBridge>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

fn ui_bridge_for(session_id: &str) -> UiBridge {
    let mut map = match UI_BRIDGES.lock() {
        Ok(m) => m,
        // A poisoned lock only means some other task panicked mid-update; the
        // map itself is still coherent enough to hand out a bridge.
        Err(p) => p.into_inner(),
    };
    map.entry(session_id.to_string())
        .or_insert_with(UiBridge::new)
        .clone()
}

/// User-controlled opt-in for the BRSDK safety frameworks. **Default ALL OFF.**
///
/// A manifest-declared guardrail / encryption / tracing only activates when the
/// user has explicitly enabled it in Settings — so these features NEVER
/// auto-apply (and never touch normal, non-app BioRouter usage at all). Backed
/// by config params the Settings panel writes.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct BrsdkSettings {
    /// Local PII/PHI masking on app input (config key `brsdk_pii_guardrail`).
    pub pii_guardrail: bool,
    /// LLM-as-just-in-time guardrails: the goal Stop-hook judge + (future)
    /// injection/groundedness/moderation judges (config key `brsdk_llm_guardrails`).
    pub llm_guardrails: bool,
    /// Per-app encrypted vault (config key `brsdk_encryption`).
    pub encryption: bool,
    /// Agent trace timeline (config key `brsdk_tracing`).
    pub tracing: bool,
}

impl BrsdkSettings {
    pub(crate) fn from_config(c: &biorouter::config::Config) -> Self {
        let flag = |k: &str| c.get_param::<bool>(k).unwrap_or(false);
        Self {
            pii_guardrail: flag("brsdk_pii_guardrail"),
            llm_guardrails: flag("brsdk_llm_guardrails"),
            encryption: flag("brsdk_encryption"),
            tracing: flag("brsdk_tracing"),
        }
    }

    pub(crate) fn current() -> Self {
        Self::from_config(biorouter::config::Config::global())
    }
}

fn advertised_app_capabilities(manifest: &Manifest, settings: BrsdkSettings) -> Vec<String> {
    let mut capabilities = manifest
        .agent
        .as_ref()
        .map(|a| a.capabilities.advertised())
        .unwrap_or_default();

    capabilities.retain(|capability| match capability.as_str() {
        "vault" => settings.encryption,
        "tracing" => settings.tracing,
        _ => true,
    });
    capabilities
}

/// Load (or create + persist) the per-app AES-256 vault key from the OS keyring.
/// Returns `None` if a fresh key can't be persisted — we refuse to encrypt with
/// an ephemeral key whose secrets could never be read back.
fn load_or_create_vault_key(app_id: &str) -> Option<biorouter_mcp::agent_drafter::vault::DataKey> {
    use biorouter::config::ConfigError;
    use biorouter_mcp::agent_drafter::vault::{generate_key, DataKey, KEY_LEN};
    let cfg = biorouter::config::Config::global();
    let key_id = format!("brsdk_vault_key_{app_id}");
    match cfg.get_secret::<Vec<u8>>(&key_id) {
        Ok(bytes) if bytes.len() == KEY_LEN => {
            let mut k: DataKey = [0u8; KEY_LEN];
            k.copy_from_slice(&bytes);
            Some(k)
        }
        Ok(_) => {
            // A stored-but-wrong-length blob is corrupt. Refuse rather than
            // overwrite — overwriting would make any sealed secrets unreadable.
            warn!(app = %app_id, "stored vault key has wrong length; refusing to use");
            None
        }
        Err(ConfigError::NotFound(_)) => {
            // Genuinely absent → generate + persist exactly once.
            let key = generate_key();
            match cfg.set_secret(&key_id, &key.to_vec()) {
                Ok(()) => Some(key),
                Err(e) => {
                    warn!(app = %app_id, "could not persist vault key: {e}");
                    None
                }
            }
        }
        Err(e) => {
            // Transient/other keyring failure: do NOT generate a new key — that
            // would clobber the real one and orphan previously-sealed secrets.
            warn!(app = %app_id, "vault key read failed (not overwriting): {e}");
            None
        }
    }
}

/// Decrypt an app's allow-listed secrets into a name→value map. Only names in
/// `allowed` (the manifest's `vault.encrypted` list) are loaded — a stored but
/// non-allow-listed secret is never exposed — and missing names are skipped.
fn load_vault_secrets(
    vault: &biorouter_mcp::agent_drafter::vault::Vault,
    allowed: &[String],
) -> std::collections::HashMap<String, String> {
    let mut secrets = std::collections::HashMap::new();
    for name in allowed {
        if vault.contains(name) {
            if let Ok(value) = vault.get(name) {
                secrets.insert(name.clone(), value);
            }
        }
    }
    secrets
}

/// Materialize a manifest-declared sub-agent into a minimal workflow recipe
/// (JSON — a valid `biorouter::workflow::Workflow`) that the engine's subagent
/// tool can load by path. This is how an app's `orchestration.sub_agents` become
/// agents-as-tools the main agent can delegate to.
fn materialize_subagent_recipe(
    name: &str,
    m: &biorouter_mcp::agent_drafter::manifest::SubAgentManifest,
) -> String {
    let description = if m.description.trim().is_empty() {
        format!("Specialist sub-agent '{name}'.")
    } else {
        m.description.clone()
    };
    // Workflow needs instructions OR prompt; default so the sub-agent is runnable.
    let instructions = if m.system_prompt.trim().is_empty() {
        format!(
            "You are the '{name}' specialist sub-agent. Complete the delegated task and report back concisely."
        )
    } else {
        m.system_prompt.clone()
    };
    let mut doc = serde_json::json!({
        "version": "1.0.0",
        "title": name,
        "description": description,
        "instructions": instructions,
    });
    if !m.skills.is_empty() {
        doc["skills"] = serde_json::json!(m.skills);
    }
    serde_json::to_string_pretty(&doc).unwrap_or_else(|_| "{}".to_string())
}

/// Resolve an app's declared `sql` data sources to jailed, existing db paths.
///
/// Pure (no global state) so it is unit-testable: each `source.file` must
/// resolve INSIDE `workspace` (a read-only jail) and exist on disk; sources that
/// escape the jail, don't exist, aren't `sql`, or duplicate a name are dropped
/// (and logged). Returns name → resolved path. This is the security boundary
/// that keeps an app's data sources confined to its own workspace.
fn resolve_sql_sources(
    workspace: &std::path::Path,
    data: &biorouter_mcp::agent_drafter::manifest::DataCapability,
) -> std::collections::HashMap<String, std::path::PathBuf> {
    let jail = biorouter_mcp::developer::jail::Jail::new(workspace, false);
    let mut sources: std::collections::HashMap<String, std::path::PathBuf> =
        std::collections::HashMap::new();
    for src in &data.sources {
        if src.kind != "sql" {
            continue; // extension-backed sources (knowledge/spoke/…) are wired elsewhere
        }
        let Some(file) = src.file.as_ref() else {
            continue;
        };
        match jail.resolve(file, false) {
            Ok(path) if path.exists() => {
                if sources.contains_key(&src.name) {
                    warn!(source = %src.name, "duplicate data source name; keeping the first");
                    continue;
                }
                sources.insert(src.name.clone(), path);
            }
            Ok(_) => warn!(source = %src.name, "data source file not found in workspace"),
            Err(_) => warn!(source = %src.name, "data source rejected by workspace jail"),
        }
    }
    sources
}

/// Configure a freshly-created session's agent from the app manifest: model,
/// extensions, skills, knowledge base, and persona. Errors are logged, not fatal
/// (the agent falls back to the global config where possible).
#[allow(clippy::too_many_lines)]
async fn configure_agent(
    agent: &biorouter::agents::Agent,
    state: &AppState,
    session_id: &str,
    manifest: &Manifest,
    ui_bridge: &UiBridge,
) {
    let Some(cfg) = manifest.agent.as_ref() else {
        return;
    };

    // Model / provider. Try the app's configured provider+model; if that can't
    // be created (e.g. its API key isn't available), fall back to BioRouter's
    // global provider/model so the agent always has *a* provider — otherwise
    // `reply` fails with a cryptic "Provider not set".
    let mut provider_set = false;
    if let Some(sel) = cfg.model.as_ref() {
        if let (Some(provider), Some(model)) = (sel.provider.as_ref(), sel.model.as_ref()) {
            match ModelConfig::new(model) {
                Ok(mc) => match create_provider(provider, mc).await {
                    Ok(p) => match agent.update_provider(p, session_id).await {
                        Ok(()) => provider_set = true,
                        Err(e) => warn!(app = %manifest.id, "update_provider failed: {e}"),
                    },
                    Err(e) => warn!(app = %manifest.id, "create provider {provider} failed: {e}"),
                },
                Err(e) => warn!(app = %manifest.id, "bad model config {model}: {e}"),
            }
        }
    }
    if !provider_set {
        let global = biorouter::config::Config::global();
        if let (Ok(provider), Ok(model)) = (
            global.get_biorouter_provider(),
            global.get_biorouter_model(),
        ) {
            if let Ok(mc) = ModelConfig::new(&model) {
                match create_provider(&provider, mc).await {
                    Ok(p) => {
                        if let Err(e) = agent.update_provider(p, session_id).await {
                            warn!(app = %manifest.id, "fallback update_provider failed: {e}");
                        } else {
                            info!(app = %manifest.id, "using global provider fallback ({provider})");
                        }
                    }
                    Err(e) => warn!(app = %manifest.id, "fallback provider {provider} failed: {e}"),
                }
            }
        }
    }

    // Extensions (+ knowledge if a KB is set).
    let mut extensions = cfg.extensions.clone();
    if cfg.knowledge_base.is_some() && !extensions.iter().any(|e| e == "knowledge") {
        extensions.push("knowledge".to_string());
    }
    if !cfg.skills.is_empty() && !extensions.iter().any(|e| e == "skills") {
        extensions.push("skills".to_string());
    }
    for name in extensions {
        let config = if PLATFORM_EXTENSIONS.contains_key(name.as_str()) {
            ExtensionConfig::Platform {
                name: name.clone(),
                bundled: None,
                description: name.clone(),
                available_tools: Vec::new(),
            }
        } else {
            ExtensionConfig::Builtin {
                name: name.clone(),
                display_name: None,
                timeout: None,
                bundled: None,
                description: name.clone(),
                available_tools: Vec::new(),
            }
        };
        if let Err(e) = agent.add_extension(config).await {
            warn!(app = %manifest.id, extension = %name, "add_extension failed: {e}");
        }
    }

    // BRSDK data capability: inject a per-app read-only SQL server for any
    // `sql` data sources, each resolved INSIDE the app's workspace jail (so a
    // source can't point at a db file outside the app). Deny-by-default: only
    // apps that declared `capabilities.data` get the tools.
    if let Some(data) = cfg.capabilities.data.as_ref() {
        let workspace = store().artifact_dir(&manifest.id).join("workspace");
        // Ensure the jail root exists so a fresh app's sources resolve (rather
        // than every source failing canonicalize on a missing dir).
        let _ = std::fs::create_dir_all(&workspace);
        let sources = resolve_sql_sources(&workspace, data);
        if !sources.is_empty() {
            let server = biorouter_mcp::datasql::server::DataSqlServer::new(sources);
            if let Err(e) = agent
                .extension_manager
                .add_inprocess_server("datasql", server)
                .await
            {
                warn!(app = %manifest.id, "datasql injection failed: {e}");
            }
        }
    }

    // BRSDK files capability: inject a jailed file server confined to the app
    // workspace (read/write/list, jail-enforced). Deny-by-default.
    if cfg.capabilities.files.is_some() {
        let workspace = store().artifact_dir(&manifest.id).join("workspace");
        let _ = std::fs::create_dir_all(&workspace);
        let server = biorouter_mcp::files_server::for_workspace(workspace, true);
        if let Err(e) = agent
            .extension_manager
            .add_inprocess_server("files", server)
            .await
        {
            warn!(app = %manifest.id, "files injection failed: {e}");
        }
    }

    // BRSDK compute capability: inject a sandboxed compute server (local or
    // Docker per the manifest) over the app workspace. Deny-by-default
    // (sandbox=="none" → no server). If the requested backend can't be built we
    // log + skip rather than silently fall through to unsandboxed host exec.
    if let Some(compute) = cfg.capabilities.compute.as_ref() {
        if compute.sandbox != "none" {
            let workspace = store().artifact_dir(&manifest.id).join("workspace");
            let _ = std::fs::create_dir_all(&workspace);
            match biorouter_mcp::compute_server::for_capability(workspace, compute) {
                Some(server) => {
                    if let Err(e) = agent
                        .extension_manager
                        .add_inprocess_server("compute", server)
                        .await
                    {
                        warn!(app = %manifest.id, "compute injection failed: {e}");
                    }
                }
                None => warn!(
                    app = %manifest.id,
                    sandbox = %compute.sandbox,
                    "compute sandbox could not be constructed; compute tools NOT granted"
                ),
            }
        }
    }

    // BRSDK ui capability: inject the app-control server so the agent can DRIVE
    // the app (panels, dashboards, charts, highlights, theme, and `ui_ask`), not
    // just answer inside it. Unlike files/data/compute this is on by default —
    // its blast radius is the app's own page, and it is what makes an app an app
    // instead of a chat box. `add_inprocess_server` is idempotent, so on a
    // reconnect the *existing* server is kept and the bridge is simply rebound to
    // the new socket by the caller (see `ui_bridge_for`).
    if cfg.capabilities.ui.enabled {
        let server = biorouter_mcp::agent_drafter::control::AppControlServer::new(
            ui_bridge.clone(),
            cfg.capabilities.ui.clone(),
        );
        if let Err(e) = agent
            .extension_manager
            .add_inprocess_server("appcontrol", server)
            .await
        {
            warn!(app = %manifest.id, "appcontrol injection failed: {e}");
        }
    }

    // BRSDK encryption capability: decrypt the app's allow-listed secrets and
    // install them on the agent so `{{vault:NAME}}` resolves at tool-dispatch
    // (plaintext never reaches the model). Opt-in: only when the user enabled
    // encryption in Settings AND the manifest declares a vault.
    if BrsdkSettings::current().encryption {
        if let Some(vault_cap) = cfg.capabilities.vault.as_ref() {
            if !vault_cap.encrypted.is_empty() {
                let workspace = store().artifact_dir(&manifest.id).join("workspace");
                let _ = std::fs::create_dir_all(&workspace);
                if let Some(key) = load_or_create_vault_key(&manifest.id) {
                    let vault = biorouter_mcp::agent_drafter::vault::Vault::new(&workspace, key);
                    let secrets = load_vault_secrets(&vault, &vault_cap.encrypted);
                    if !secrets.is_empty() {
                        agent
                            .set_vault(Arc::new(biorouter::agents::VaultRefs::new(secrets)))
                            .await;
                        info!(app = %manifest.id, count = vault_cap.encrypted.len(), "vault installed");
                    }
                }
            }
        }
    }

    // BRSDK orchestration: register manifest-declared sub-agents as
    // agents-as-tools. Each becomes a recipe the engine's subagent tool can
    // invoke by name (the tool auto-lists them once registered). A functional
    // capability, opt-in via the manifest — no global safety gate.
    if !cfg.orchestration.sub_agents.is_empty() {
        let dir = store().artifact_dir(&manifest.id).join("subagents");
        let _ = std::fs::create_dir_all(&dir);
        let mut subs = Vec::new();
        for (name, sa) in &cfg.orchestration.sub_agents {
            // Sanitize the filename; keep the original name as the callable id.
            // Append a hash of the original name so distinct names that sanitize
            // to the same string (e.g. "a.b" and "a/b") don't collide on one file.
            let safe: String = name
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            let disambig = {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                name.hash(&mut h);
                h.finish()
            };
            let path = dir.join(format!("{safe}-{disambig:x}.json"));
            if std::fs::write(&path, materialize_subagent_recipe(name, sa)).is_ok() {
                subs.push(biorouter::workflow::SubWorkflow {
                    name: name.clone(),
                    path: path.to_string_lossy().into_owned(),
                    values: None,
                    sequential_when_repeated: false,
                    description: Some(if sa.description.trim().is_empty() {
                        format!("Specialist sub-agent '{name}'")
                    } else {
                        sa.description.clone()
                    }),
                });
            }
        }
        if !subs.is_empty() {
            let n = subs.len();
            agent.add_sub_workflows(subs).await;
            info!(app = %manifest.id, count = n, "registered sub-agents as tools");
        }
    }

    // Knowledge base scoping.
    if let Some(kb) = cfg.knowledge_base.as_ref() {
        if let Err(e) = state
            .knowledge_service
            .set_active_for_session(session_id, Some(kb))
        {
            warn!(app = %manifest.id, kb = %kb, "set active KB failed: {e}");
        }
    }

    // Persona + app context + skill guidance.
    let mut prompt = String::new();
    prompt.push_str(&format!(
        "You are the agent powering the BioRouter app \"{}\".",
        manifest.title
    ));
    if !manifest.description.is_empty() {
        prompt.push_str(&format!(" {}", manifest.description));
    }
    if !cfg.system_prompt.trim().is_empty() {
        prompt.push_str("\n\n");
        prompt.push_str(&cfg.system_prompt);
    }
    if !cfg.skills.is_empty() {
        prompt.push_str(&format!(
            "\n\nWhen relevant, use these skills: {}.",
            cfg.skills.join(", ")
        ));
    }
    // Tell the model the `ui_*` tools exist and, more importantly, when to reach
    // for them instead of writing another paragraph.
    if cfg.capabilities.ui.enabled {
        prompt.push_str(&biorouter_mcp::agent_drafter::control::ui_system_prompt(
            &cfg.capabilities.ui,
        ));
    }
    agent.extend_system_prompt(prompt).await;

    // BRSDK guardrails: a one-line `goal` auto-installs the goal Stop-hook so the
    // app's agent keeps working until the goal condition holds — reusing the
    // proven /goal machinery (LLM-judge, iteration cap, stall detection, graceful
    // give-up). Opt-in (deny-by-default): only apps that declare a goal get it.
    // Idempotent: re-installed on each (re)connect via configure_agent.
    // Opt-in gate: the goal Stop-hook is an LLM-as-just-in-time guardrail, so it
    // only installs if the user enabled LLM guardrails in Settings (default off).
    if BrsdkSettings::current().llm_guardrails {
        if let Some(goal) = cfg.guardrails.as_ref().and_then(|g| g.goal.clone()) {
            if !goal.trim().is_empty() {
                agent.set_goal(session_id, goal).await;
            }
        }
    }
}

/// Cap on client frames buffered while a turn is running. A well-behaved app
/// sends at most a couple (an impatient click); anything beyond this is a
/// runaway loop, and dropping is safer than growing without bound.
const MAX_QUEUED_FRAMES: usize = 32;

/// What the turn loop is woken by. Modelled as one enum so the `select!` branches
/// only *bind* values — nothing in a branch body borrows the socket or the agent
/// stream, which is what lets the handler below use both afterwards.
enum TurnWake {
    /// A `ui_*` tool wants to change the page.
    Ui(serde_json::Value),
    /// A frame arrived from the browser mid-turn.
    Client(Option<Result<WsMessage, axum::Error>>),
    /// The agent produced an event (or finished).
    Agent(Option<anyhow::Result<AgentEvent>>),
}

/// Handle a client frame that arrived *during* a turn.
///
/// `ui_reply` and `ui_surface` are consumed here (a parked `ui_ask` is waiting on
/// the former). `cancel` now actually cancels — before the socket was split the
/// loop couldn't read while the reply stream was pending, so it never could.
/// Anything that starts new work (`prompt`, `widget_action`) is queued for after
/// the turn instead of being dropped.
fn handle_midturn_frame(
    text: &str,
    ui_bridge: &UiBridge,
    cancel: &CancellationToken,
    queued: &mut VecDeque<ClientFrame>,
) {
    let Ok(frame) = serde_json::from_str::<ClientFrame>(text) else {
        return;
    };
    match frame {
        ClientFrame::UiReply {
            request_id,
            payload,
        } => {
            if !ui_bridge.resolve(&request_id, payload) {
                warn!(request = %request_id, "ui_reply for an unknown or expired request");
            }
        }
        ClientFrame::UiSurface { surface } => ui_bridge.set_surface(surface),
        ClientFrame::Cancel => {
            cancel.cancel();
            // A parked `ui_ask` must not survive the turn it belongs to.
            ui_bridge.cancel_all();
        }
        // Consumed inline by `handle_action_required` during an approval pause;
        // arriving here they are stray.
        ClientFrame::Approve { .. } | ClientFrame::Reject { .. } => {}
        // Read-only requests are cheap but need the sink, so defer them too.
        other => {
            if queued.len() < MAX_QUEUED_FRAMES {
                queued.push_back(other);
            } else {
                warn!("dropping a client frame: more than {MAX_QUEUED_FRAMES} queued mid-turn");
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
async fn handle_agent_socket(
    socket: WebSocket,
    state: Arc<AppState>,
    manifest: Manifest,
    client_id: Option<String>,
) {
    // Split so the loop can await agent events, agent-issued UI commands, and
    // inbound client frames at the same time. A `ui_ask` tool call parks *inside*
    // `agent.reply`, so its answer has to be readable while that stream is pending.
    let (mut socket_tx, mut socket_rx) = socket.split();
    // BRSDK durable sessions: when the app opts in (default) and the client sent
    // a stable client_id, bind the session to "app:<id>:<client-id>" so a reload
    // RESUMES the same conversation. Otherwise fall back to a fresh per-connection
    // session (the pre-BRSDK behavior).
    let durable = manifest
        .agent
        .as_ref()
        .map(|a| a.durable_session())
        .unwrap_or(true);
    let workdir = std::env::current_dir().unwrap_or_default();
    let name = format!("app:{}", manifest.id);

    let (session, resumed) = match (durable, client_id.as_ref()) {
        (true, Some(cid)) => {
            let key = format!("app:{}:{}", manifest.id, cid);
            match state
                .session_manager()
                .get_or_create_by_external_key(&key, workdir, name, SessionType::User)
                .await
            {
                Ok(pair) => pair,
                Err(e) => {
                    let _ = send_json(
                        &mut socket_tx,
                        json!({"type":"error","message": format!("session: {e}")}),
                    )
                    .await;
                    return;
                }
            }
        }
        _ => match state
            .session_manager()
            .create_session(workdir, name, SessionType::User)
            .await
        {
            Ok(s) => (s, false),
            Err(e) => {
                let _ = send_json(
                    &mut socket_tx,
                    json!({"type":"error","message": format!("session: {e}")}),
                )
                .await;
                return;
            }
        },
    };
    let session_id = session.id.clone();
    let message_count = session.message_count;

    let agent = match state.get_agent(session_id.clone()).await {
        Ok(a) => a,
        Err(e) => {
            let _ = send_json(
                &mut socket_tx,
                json!({"type":"error","message": format!("agent: {e}")}),
            )
            .await;
            return;
        }
    };

    // Rebind this session's UI bridge to *this* socket, then configure the agent
    // (which injects the app-control server the first time round, and reuses it
    // afterwards). `attach` must precede `configure_agent` so a replayed state
    // command lands in the new channel.
    let ui_bridge = ui_bridge_for(&session_id);
    let (mut ui_rx, conn_token) = ui_bridge.attach();

    configure_agent(&agent, &state, &session_id, &manifest, &ui_bridge).await;
    info!(app = %manifest.id, session = %session_id, "app agent session ready");
    // BRSDK protocol v2: advertise capabilities so old apps ignore frames they
    // don't understand and new apps can feature-detect. Deny-by-default — only
    // capabilities the manifest declared are advertised.
    let capabilities = advertised_app_capabilities(&manifest, BrsdkSettings::current());
    if !send_json(
        &mut socket_tx,
        json!({
            "type": "ready",
            "protocol": 2,
            "capabilities": capabilities,
            "sessionId": session_id,
            "resumed": resumed,
            "messageCount": message_count,
        }),
    )
    .await
    {
        ui_bridge.detach(conn_token);
        return;
    }

    // Frames the browser sent while a turn was still running.
    let mut queued: VecDeque<ClientFrame> = VecDeque::new();

    loop {
        // Between turns, still forward UI commands (a previous turn's tool may
        // have queued one) and wait for the next client frame.
        let frame = match queued.pop_front() {
            Some(f) => f,
            None => {
                let next = loop {
                    let woken = tokio::select! {
                        biased;
                        Some(cmd) = ui_rx.recv() => TurnWake::Ui(cmd),
                        inbound = socket_rx.next() => TurnWake::Client(inbound),
                    };
                    match woken {
                        TurnWake::Ui(cmd) => {
                            if !send_json(&mut socket_tx, cmd).await {
                                ui_bridge.detach(conn_token);
                                return;
                            }
                        }
                        TurnWake::Client(Some(Ok(WsMessage::Text(t)))) => {
                            match serde_json::from_str::<ClientFrame>(&t) {
                                Ok(ClientFrame::UiSurface { surface }) => {
                                    ui_bridge.set_surface(surface);
                                }
                                // Outside a turn nothing is parked on it.
                                Ok(ClientFrame::UiReply { .. }) => {}
                                Ok(f) => break f,
                                Err(_) => {}
                            }
                        }
                        TurnWake::Client(Some(Ok(WsMessage::Close(_))))
                        | TurnWake::Client(Some(Err(_)))
                        | TurnWake::Client(None) => {
                            ui_bridge.detach(conn_token);
                            return;
                        }
                        TurnWake::Client(Some(Ok(_))) => {}
                        TurnWake::Agent(_) => unreachable!("no agent stream between turns"),
                    }
                };
                next
            }
        };

        let (prompt_text, images) = match frame {
            ClientFrame::Prompt { text, images } => (text, images),
            ClientFrame::Cancel => continue,
            ClientFrame::Tokens => {
                // Report current context usage vs the model's window.
                let used = state
                    .session_manager()
                    .get_token_counts(&session_id)
                    .await
                    .ok()
                    .and_then(|c| c.total_tokens)
                    .unwrap_or(0)
                    .max(0) as u64;
                let limit = match agent.provider().await {
                    Ok(p) => p.get_model_config().context_limit() as u64,
                    Err(_) => 0,
                };
                let ratio = if limit > 0 {
                    used as f64 / limit as f64
                } else {
                    0.0
                };
                let _ = send_json(
                    &mut socket_tx,
                    json!({"type":"context","used":used,"limit":limit,"ratio":ratio}),
                )
                .await;
                continue;
            }
            ClientFrame::History => {
                // Backlog for THIS connection's own session only. The WS is
                // already bound to the resolved session, so there's no id to
                // guess and no cross-session access.
                let messages = backlog_for(&state, &session_id).await;
                let _ = send_json(
                    &mut socket_tx,
                    json!({"type":"history","messages": messages}),
                )
                .await;
                continue;
            }
            // Consumed by the mid-turn reader; between turns they are no-ops.
            ClientFrame::UiReply { .. } => continue,
            ClientFrame::UiSurface { surface } => {
                ui_bridge.set_surface(surface);
                continue;
            }
            ClientFrame::ModelSelect { provider, model } => {
                // Live-switch the session's provider/model (BRSDK model surface).
                let model_name = model.unwrap_or_default();
                let provider_name = provider.unwrap_or_default();
                let ok = if provider_name.is_empty() || model_name.is_empty() {
                    false
                } else {
                    match ModelConfig::new(&model_name) {
                        Ok(mc) => match create_provider(&provider_name, mc).await {
                            Ok(p) => agent.update_provider(p, &session_id).await.is_ok(),
                            Err(_) => false,
                        },
                        Err(_) => false,
                    }
                };
                let _ = send_json(
                    &mut socket_tx,
                    json!({"type":"model","ok": ok, "provider": provider_name, "model": model_name}),
                )
                .await;
                continue;
            }
            ClientFrame::Approve { .. } | ClientFrame::Reject { .. } => {
                // Approve/Reject are consumed inline during an approval pause
                // (handle_action_required, while the reply stream is parked). One
                // arriving outside a pause is stray — ignore it.
                continue;
            }
            ClientFrame::WidgetAction {
                widget_id,
                action,
                payload,
            } => {
                // A widget submit becomes the next user turn — the agent sees
                // what was submitted and continues. Falls through (no `continue`).
                let payload_str = serde_json::to_string(&payload).unwrap_or_default();
                let text = format!(
                    "[widget:{widget_id}] The user submitted action '{action}' with values: {payload_str}"
                );
                (text, Vec::new())
            }
        };

        // Content guardrail (input stage): apply the manifest's PII/PHI policy to
        // the user's message at the app boundary — before it reaches the model or
        // the conversation. Local, on-device detection (no provider). Mask rewrites
        // the prompt; Block refuses the turn. Either way a `guardrail` frame tells
        // the app what happened.
        // Opt-in gate: the manifest's PII policy applies ONLY if the user enabled
        // the content guardrail in Settings (default off → never auto-applies).
        let pii_mode = if BrsdkSettings::current().pii_guardrail {
            manifest
                .agent
                .as_ref()
                .and_then(|a| a.guardrails.as_ref())
                .map(|g| g.pii)
                .unwrap_or(PiiMode::Off)
        } else {
            PiiMode::Off
        };
        let prompt_text = match apply_pii_policy(prompt_text, pii_mode) {
            PiiOutcome::Pass(text) => text,
            PiiOutcome::Masked { text, reason } => {
                let _ = send_json(
                    &mut socket_tx,
                    json!({"type":"guardrail","stage":"input","name":"pii","blocked":false,"reason":reason}),
                )
                .await;
                text
            }
            PiiOutcome::Blocked { reason } => {
                let _ = send_json(
                    &mut socket_tx,
                    json!({"type":"guardrail","stage":"input","name":"pii","blocked":true,"reason":reason}),
                )
                .await;
                // End the turn cleanly without running the agent.
                if !send_json(&mut socket_tx, json!({"type":"done"})).await {
                    ui_bridge.detach(conn_token);
                    return;
                }
                continue;
            }
        };

        let mut user = Message::user().with_text(prompt_text);
        for img in images {
            user = user.with_image(img.data, img.mime_type);
        }

        // Bound the agent's tool-calling loop (guardrail against runaway loops;
        // also the knob workflow-style apps raise to chain more steps). Defaults
        // to a safe cap when the app doesn't specify one.
        let max_turns = manifest
            .agent
            .as_ref()
            .and_then(|a| a.max_turns)
            .unwrap_or(DEFAULT_MAX_TURNS);
        let session_config = SessionConfig {
            id: session_id.clone(),
            schedule_id: None,
            max_turns: Some(max_turns),
            max_tool_calls: None,
            retry_config: None,
        };
        let cancel = CancellationToken::new();
        let mut stream = match agent
            .reply(user, session_config, Some(cancel.clone()))
            .await
        {
            Ok(s) => s,
            Err(e) => {
                let _ = send_json(
                    &mut socket_tx,
                    json!({"type":"error","message": e.to_string()}),
                )
                .await;
                continue;
            }
        };

        let mut errored = false;
        // call id → tool name, so a ToolResponse can be reported by name.
        let mut tool_names: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        loop {
            // Three sources, biased so a UI command a tool just issued reaches the
            // page before the `tool completed` frame that follows it. Every branch
            // only binds — the bodies below are outside the `select!`, so they may
            // borrow the socket and the stream freely.
            let woken = tokio::select! {
                biased;
                Some(cmd) = ui_rx.recv() => TurnWake::Ui(cmd),
                inbound = socket_rx.next() => TurnWake::Client(inbound),
                event = stream.next() => TurnWake::Agent(event),
            };

            let event = match woken {
                TurnWake::Ui(cmd) => {
                    if !send_json(&mut socket_tx, cmd).await {
                        ui_bridge.detach(conn_token);
                        return;
                    }
                    continue;
                }
                TurnWake::Client(Some(Ok(WsMessage::Text(t)))) => {
                    handle_midturn_frame(&t, &ui_bridge, &cancel, &mut queued);
                    continue;
                }
                TurnWake::Client(Some(Ok(WsMessage::Close(_))))
                | TurnWake::Client(Some(Err(_)))
                | TurnWake::Client(None) => {
                    // The page went away mid-turn: stop the agent and unblock any
                    // `ui_ask` it left parked, rather than leaking a live turn.
                    cancel.cancel();
                    ui_bridge.detach(conn_token);
                    return;
                }
                TurnWake::Client(Some(Ok(_))) => continue,
                TurnWake::Agent(Some(e)) => e,
                TurnWake::Agent(None) => break,
            };

            match event {
                Ok(AgentEvent::Message(message)) => {
                    for content in &message.content {
                        let frame = match content {
                            MessageContent::Text(t) => {
                                Some(json!({"type":"message","delta": t.text}))
                            }
                            MessageContent::Thinking(t) => {
                                Some(json!({"type":"thought","delta": t.thinking}))
                            }
                            MessageContent::ToolRequest(tr) => {
                                let name = tr
                                    .tool_call
                                    .as_ref()
                                    .map(|c| c.name.to_string())
                                    .unwrap_or_else(|_| "tool".to_string());
                                // Remember the name against the call id so the
                                // response frame can report it too. A timeline of
                                // "tool completed" rows says nothing about what ran
                                // — which matters now that tools redraw the page.
                                tool_names.insert(tr.id.clone(), name.clone());
                                Some(
                                    json!({"type":"tool","name": name, "id": tr.id, "status":"pending"}),
                                )
                            }
                            MessageContent::ToolResponse(resp) => {
                                let status = match &resp.tool_result {
                                    Ok(r) if r.is_error == Some(true) => "failed",
                                    Ok(_) => "completed",
                                    Err(_) => "failed",
                                };
                                let name = tool_names
                                    .remove(&resp.id)
                                    .unwrap_or_else(|| "tool".to_string());
                                Some(
                                    json!({"type":"tool","name": name, "id": resp.id, "status": status}),
                                )
                            }
                            MessageContent::ActionRequired(ar) => {
                                // HITL: pause for human approval over this socket,
                                // then resume. Returns no frame (it sends its own).
                                handle_action_required(
                                    &mut socket_tx,
                                    &mut socket_rx,
                                    &state,
                                    &agent,
                                    &session_id,
                                    &manifest.id,
                                    &ui_bridge,
                                    conn_token,
                                    ar,
                                )
                                .await;
                                None
                            }
                            _ => None,
                        };
                        if let Some(f) = frame {
                            if !send_json(&mut socket_tx, f).await {
                                ui_bridge.detach(conn_token);
                                return;
                            }
                        }
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    let _ = send_json(
                        &mut socket_tx,
                        json!({"type":"error","message": e.to_string()}),
                    )
                    .await;
                    errored = true;
                    break;
                }
            }
        }

        // The reply stream is done, but a tool's last UI command may still be in
        // flight. Flush before `done` so the page is settled when the app's
        // `prompt()` promise resolves (tests and app code both rely on that).
        while let Ok(cmd) = ui_rx.try_recv() {
            if !send_json(&mut socket_tx, cmd).await {
                ui_bridge.detach(conn_token);
                return;
            }
        }

        // Reply loop ended — trigger the best-effort LLM session rename. Always
        // runs here regardless of how the loop exited, so app sessions get a
        // content-derived name instead of staying on the placeholder.
        {
            let agent_for_rename = agent.clone();
            let session_id_for_rename = session_id.clone();
            tokio::spawn(async move {
                agent_for_rename
                    .maybe_rename_session(&session_id_for_rename)
                    .await;
            });
        }

        if !errored && !send_json(&mut socket_tx, json!({"type":"done"})).await {
            ui_bridge.detach(conn_token);
            return;
        }
    }
}

/// Result of applying the input-stage PII/PHI policy to a user prompt.
enum PiiOutcome {
    /// No policy / no PII found — use the text as-is.
    Pass(String),
    /// PII found and masked; `reason` summarizes what for the `guardrail` frame.
    Masked { text: String, reason: String },
    /// PII found under a Block policy — refuse the turn.
    Blocked { reason: String },
}

/// Apply the manifest's PII/PHI policy to a prompt. Pure + on-device (no
/// provider, no network), so it is unit-testable in isolation.
fn apply_pii_policy(text: String, mode: PiiMode) -> PiiOutcome {
    if mode == PiiMode::Off {
        return PiiOutcome::Pass(text);
    }
    let detector = PiiDetector::new();
    let found = detector.scan(&text);
    if found.is_empty() {
        return PiiOutcome::Pass(text);
    }
    let kinds = found
        .iter()
        .map(|m| m.kind.tag().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    match mode {
        PiiMode::Block => PiiOutcome::Blocked {
            reason: format!("Message blocked: it contains PII/PHI ({kinds})."),
        },
        PiiMode::Mask => {
            let (masked, _) = detector.mask(&text);
            PiiOutcome::Masked {
                text: masked,
                reason: format!("Masked PII/PHI in your message ({kinds})."),
            }
        }
        PiiMode::Off => PiiOutcome::Pass(text),
    }
}

/// The user-visible transcript backlog for a session, as `{role, text}` pairs.
/// Agent-only compaction summaries/continuations and empty (tool/thinking-only)
/// turns are filtered out. Used by the WS `history` request, which is already
/// scoped to the connection's own resolved session.
/// Persist a paused-run snapshot into the session so a reconnecting app can
/// re-surface a pending approval. Best-effort (a persistence error is logged
/// upstream by the session layer; HITL still works in-process).
async fn save_run_state(state: &AppState, session_id: &str, rs: &RunState) {
    let sm = state.session_manager();
    if let Ok(mut sd) = sm.get_session(session_id, false).await {
        rs.store_into(&mut sd.extension_data);
        let _ = sm
            .update(session_id)
            .extension_data(sd.extension_data)
            .apply()
            .await;
    }
}

/// Clear any persisted paused-run snapshot once the approval is resolved.
async fn clear_run_state(state: &AppState, session_id: &str) {
    let sm = state.session_manager();
    if let Ok(mut sd) = sm.get_session(session_id, false).await {
        RunState::clear(&mut sd.extension_data);
        let _ = sm
            .update(session_id)
            .extension_data(sd.extension_data)
            .apply()
            .await;
    }
}

/// Load a persisted paused-run snapshot, if any.
async fn load_run_state(state: &AppState, session_id: &str) -> Option<RunState> {
    let sm = state.session_manager();
    let sd = sm.get_session(session_id, false).await.ok()?;
    RunState::load_from(&sd.extension_data)
}

/// HITL approval at the app boundary. The agent **yields** the ToolConfirmation
/// message BEFORE it awaits the confirmation channel, so while the reply stream
/// is parked we: (1) persist the paused state, (2) surface an `approval` frame,
/// (3) read the user's decision from the SAME socket, (4) feed it back via
/// `handle_confirmation`, and (5) clear the snapshot. No separate route and no
/// reply-loop concurrency change — the decision rides the app's own authed WS.
#[allow(clippy::too_many_arguments)]
async fn handle_action_required(
    socket_tx: &mut WsSink,
    socket_rx: &mut WsSource,
    state: &AppState,
    agent: &Arc<biorouter::agents::Agent>,
    session_id: &str,
    app_id: &str,
    ui_bridge: &UiBridge,
    conn_token: biorouter_mcp::agent_drafter::control::ConnToken,
    ar: &biorouter::conversation::message::ActionRequired,
) {
    let ActionRequiredData::ToolConfirmation {
        id,
        tool_name,
        arguments,
        prompt,
    } = &ar.data
    else {
        return; // only tool-confirmation approvals are handled here
    };

    let rs = RunState::awaiting_approval(
        id.clone(),
        session_id,
        app_id,
        PendingTool {
            request_id: id.clone(),
            name: tool_name.clone(),
            args: serde_json::Value::Object(arguments.clone()),
        },
        prompt.clone().unwrap_or_default(),
        0,
    );
    save_run_state(state, session_id, &rs).await;

    let _ = send_json(
        socket_tx,
        json!({"type":"approval","requestId": id, "tool": tool_name, "args": arguments, "prompt": prompt}),
    )
    .await;

    // Read the decision from this socket (the reply stream is parked, consuming
    // nothing). Default to deny if the client vanishes, so the agent never hangs.
    let permission = loop {
        match socket_rx.next().await {
            Some(Ok(WsMessage::Text(t))) => match serde_json::from_str::<ClientFrame>(&t) {
                Ok(ClientFrame::Approve { request, .. }) if request == *id => {
                    break Permission::AllowOnce;
                }
                Ok(ClientFrame::Reject { request, .. }) if request == *id => {
                    break Permission::DenyOnce;
                }
                Ok(ClientFrame::Cancel) => {
                    ui_bridge.cancel_all();
                    break Permission::DenyOnce;
                }
                // A `ui_ask` can be parked *behind* this approval (the agent
                // asked, then a later tool needed consent). Answering it here
                // rather than ignoring the frame keeps that ask from timing out.
                Ok(ClientFrame::UiReply {
                    request_id,
                    payload,
                }) => {
                    ui_bridge.resolve(&request_id, payload);
                    continue;
                }
                Ok(ClientFrame::UiSurface { surface }) => {
                    ui_bridge.set_surface(surface);
                    continue;
                }
                _ => continue, // ignore unrelated frames while awaiting a decision
            },
            Some(Ok(WsMessage::Close(_))) | Some(Err(_)) | None => {
                ui_bridge.detach(conn_token);
                break Permission::DenyOnce;
            }
            _ => continue,
        }
    };

    agent
        .handle_confirmation(
            id.clone(),
            PermissionConfirmation {
                principal_type: PrincipalType::Tool,
                permission,
            },
        )
        .await;

    clear_run_state(state, session_id).await;
}

async fn backlog_for(state: &AppState, session_id: &str) -> Vec<serde_json::Value> {
    match state.session_manager().get_session(session_id, true).await {
        Ok(s) => s
            .conversation
            .map(|c| c.user_visible_messages())
            .unwrap_or_default()
            .iter()
            .map(|m| {
                let text: String = m
                    .content
                    .iter()
                    .filter_map(|c| match c {
                        MessageContent::Text(t) => Some(t.text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");
                json!({ "role": m.role, "text": text })
            })
            .filter(|m| !m["text"].as_str().unwrap_or("").is_empty())
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// The model catalog the per-app model surface exposes: every provider the
/// build knows, with its display name, default model, and known models. Apps use
/// it to let the user pick a provider/model — the provider-agnostic headline.
async fn model_catalog() -> Vec<serde_json::Value> {
    biorouter::providers::providers()
        .await
        .into_iter()
        .map(|(m, _)| {
            json!({
                "name": m.name,
                "displayName": m.display_name,
                "defaultModel": m.default_model,
                "models": m.known_models.iter().map(|mi| mi.name.clone()).collect::<Vec<_>>(),
                "allowsUnlisted": m.allows_unlisted_models,
            })
        })
        .collect()
}

#[derive(Deserialize)]
struct VaultPut {
    name: String,
    value: String,
}

/// POST /apps/{id}/vault — store (encrypt) a secret in the app's vault. A
/// management verb, so it requires the secret-key header (auth.rs exempts only
/// GET-under-/apps). The secret is AES-256-GCM-sealed with the app's keyring key
/// and is only ever loaded back for names the manifest allow-lists.
async fn put_vault_secret(Path(id): Path<String>, Json(body): Json<VaultPut>) -> Response {
    // Defense-in-depth: reject a traversal-ish id even though store().exists()
    // requires a real manifest at this path.
    if id.contains('/') || id.contains('\\') || id.contains("..") {
        return (StatusCode::BAD_REQUEST, "invalid app id").into_response();
    }
    if !store().exists(&id) {
        return (StatusCode::NOT_FOUND, "no such app").into_response();
    }
    // Secret names map 1:1 to filenames; restrict to a safe charset so two names
    // can't collide on the sanitized path (and nothing can escape .vault).
    let name = body.name.trim();
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return (
            StatusCode::BAD_REQUEST,
            "name must be non-empty and use only [A-Za-z0-9_-]",
        )
            .into_response();
    }
    let workspace = store().artifact_dir(&id).join("workspace");
    let _ = std::fs::create_dir_all(&workspace);
    let key = match load_or_create_vault_key(&id) {
        Some(k) => k,
        None => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "vault key unavailable").into_response()
        }
    };
    let vault = biorouter_mcp::agent_drafter::vault::Vault::new(&workspace, key);
    match vault.put(name, &body.value) {
        Ok(()) => (StatusCode::OK, "stored").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /apps/{id}/runstate?session=<sid> — the current paused-run snapshot so a
/// reconnecting app can re-surface a pending approval. `{pending:false}` if none.
async fn get_run_state(
    Path(id): Path<String>,
    Query(q): Query<std::collections::HashMap<String, String>>,
    State(state): State<Arc<AppState>>,
) -> Response {
    if !store().exists(&id) {
        return (StatusCode::NOT_FOUND, "no such app").into_response();
    }
    let Some(session) = q.get("session") else {
        return (StatusCode::BAD_REQUEST, "session required").into_response();
    };
    match load_run_state(&state, session).await {
        // Bind the snapshot to THIS app: the route is auth-exempt (GET under
        // /apps) and session ids are enumerable, so without `rs.app_id == id`
        // any local caller could read another app's pending tool name + args.
        Some(rs) if rs.is_pending() && rs.app_id == id => Json(json!({
            "pending": true,
            "requestId": rs.run_id,
            "tool": rs.pending_tool.name,
            "args": rs.pending_tool.args,
            "prompt": rs.reason,
        }))
        .into_response(),
        _ => Json(json!({ "pending": false })).into_response(),
    }
}

/// GET /apps/{id}/models — the provider/model catalog for the per-app model
/// surface (`br.model.list()`).
async fn list_models(Path(id): Path<String>) -> Response {
    if !store().exists(&id) {
        return (StatusCode::NOT_FOUND, "no such app").into_response();
    }
    Json(json!({ "providers": model_catalog().await })).into_response()
}

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/apps", get(list_apps))
        .route(
            "/apps/{id}",
            get(redirect_to_slash).delete(delete_app_route),
        )
        .route("/apps/{id}/", get(serve_index))
        .route("/apps/{id}/agent", get(agent_ws))
        .route("/apps/{id}/models", get(list_models))
        .route("/apps/{id}/runstate", get(get_run_state))
        .route("/apps/{id}/vault", post(put_vault_secret))
        .route("/apps/{id}/build", post(build_app_route))
        .route("/apps/{id}/export", get(export_app_route))
        .route("/apps/{id}/dist/{*path}", get(serve_dist))
        .route("/apps/{id}/assets/{*path}", get(serve_assets))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::{apply_pii_policy, ClientFrame, PiiMode, PiiOutcome};

    #[test]
    fn pii_policy_off_passes_through_even_with_phi() {
        let out = apply_pii_policy("SSN 123-45-6789".to_string(), PiiMode::Off);
        assert!(matches!(out, PiiOutcome::Pass(t) if t.contains("123-45-6789")));
    }

    #[test]
    fn pii_policy_mask_redacts_and_keeps_clinical_text() {
        let out = apply_pii_policy(
            "Patient MRN: A1234567 on ivacaftor 150mg".to_string(),
            PiiMode::Mask,
        );
        match out {
            PiiOutcome::Masked { text, reason } => {
                assert!(!text.contains("A1234567"), "PHI must be masked");
                assert!(
                    text.contains("ivacaftor 150mg"),
                    "clinical content preserved"
                );
                assert!(reason.contains("MRN"));
            }
            other => panic!("expected Masked, got {:?}", std::mem::discriminant(&other)),
        }
    }

    #[test]
    fn pii_policy_block_refuses_on_phi() {
        let out = apply_pii_policy("call me at 415-555-0188".to_string(), PiiMode::Block);
        assert!(matches!(out, PiiOutcome::Blocked { .. }));
    }

    #[test]
    fn pii_policy_passes_clean_text() {
        let out = apply_pii_policy(
            "Run differential expression on the cohort".to_string(),
            PiiMode::Block,
        );
        assert!(matches!(out, PiiOutcome::Pass(_)));
    }

    // --- data-source jail boundary (the security boundary of the data feature) ---
    use super::resolve_sql_sources;
    use biorouter_mcp::agent_drafter::manifest::{DataCapability, DataSource};

    fn src(name: &str, kind: &str, file: Option<&str>) -> DataSource {
        DataSource {
            name: name.to_string(),
            kind: kind.to_string(),
            file: file.map(|f| f.to_string()),
            ref_id: None,
            read_only: true,
        }
    }

    #[test]
    fn resolve_sql_sources_keeps_in_workspace_and_rejects_escapes() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("workspace");
        std::fs::create_dir_all(ws.join("sub")).unwrap();
        std::fs::write(ws.join("sub/cohort.db"), b"x").unwrap();
        // A real db OUTSIDE the workspace that an attacker might try to reach.
        std::fs::write(dir.path().join("secret.db"), b"y").unwrap();

        let data = DataCapability {
            sources: vec![
                src("ok", "sql", Some("sub/cohort.db")), // in-workspace, exists → kept
                src("escape", "sql", Some("../secret.db")), // traversal → rejected
                src("abs", "sql", Some("/etc/hosts")),   // absolute-outside → rejected
                src("missing", "sql", Some("nope.db")),  // in-jail but absent → dropped
                src("notsql", "knowledge", Some("sub/cohort.db")), // non-sql → skipped
                src("nofile", "sql", None),              // no file → skipped
            ],
        };
        let resolved = resolve_sql_sources(&ws, &data);
        assert_eq!(
            resolved.len(),
            1,
            "only the in-workspace existing sql source survives: {resolved:?}"
        );
        assert!(resolved.contains_key("ok"));
        assert!(
            !resolved.contains_key("escape"),
            "traversal source must not escape the workspace"
        );
        assert!(
            !resolved.contains_key("abs"),
            "absolute-outside source must be rejected"
        );
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn resolve_sql_sources_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("workspace");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(dir.path().join("outside.db"), b"y").unwrap();
        // A symlink inside the workspace pointing at the outside db.
        symlink(dir.path().join("outside.db"), ws.join("link.db")).unwrap();
        let data = DataCapability {
            sources: vec![src("sneaky", "sql", Some("link.db"))],
        };
        let resolved = resolve_sql_sources(&ws, &data);
        assert!(
            resolved.is_empty(),
            "a symlink escaping the workspace must be rejected: {resolved:?}"
        );
    }

    #[test]
    fn brsdk_settings_default_off_and_opt_in() {
        use super::BrsdkSettings;
        let dir = tempfile::tempdir().unwrap();
        let cfg = biorouter::config::Config::new_with_file_secrets(
            dir.path().join("config.yaml"),
            dir.path().join("secrets.yaml"),
        )
        .unwrap();

        // Default: every safety framework is OFF (never auto-applies).
        let s = BrsdkSettings::from_config(&cfg);
        assert!(!s.pii_guardrail);
        assert!(!s.llm_guardrails);
        assert!(!s.encryption);
        assert!(!s.tracing);

        // Opting in flips exactly the chosen flag.
        cfg.set_param("brsdk_encryption", true).unwrap();
        cfg.set_param("brsdk_llm_guardrails", true).unwrap();
        let s = BrsdkSettings::from_config(&cfg);
        assert!(s.encryption, "encryption opt-in honored");
        assert!(s.llm_guardrails, "LLM-guardrail opt-in honored");
        assert!(!s.pii_guardrail, "un-set flags stay off");
        assert!(!s.tracing);
    }

    fn manifest_with_brsdk_capabilities() -> biorouter_mcp::agent_drafter::store::Manifest {
        use biorouter_mcp::agent_drafter::{
            manifest::{Capabilities, TracingCapability, VaultCapability},
            store::{AgentConfig, ArtifactKind, Manifest},
        };

        let capabilities = Capabilities {
            vault: Some(VaultCapability {
                encrypted: vec!["API_KEY".to_string()],
            }),
            tracing: TracingCapability {
                enabled: true,
                ..Default::default()
            },
            ..Default::default()
        };

        Manifest {
            id: "demo".to_string(),
            title: "Demo".to_string(),
            description: String::new(),
            kind: ArtifactKind::Agentic,
            entry: "index.html".to_string(),
            created_at: 0,
            updated_at: 0,
            agent: Some(AgentConfig {
                capabilities,
                ..Default::default()
            }),
            width: None,
            height: None,
            built_at: None,
            sdk_hash: None,
            session_id: None,
        }
    }

    #[test]
    fn advertised_app_capabilities_hide_gated_features_until_enabled() {
        let manifest = manifest_with_brsdk_capabilities();
        let caps = super::advertised_app_capabilities(&manifest, super::BrsdkSettings::default());
        assert!(!caps.contains(&"vault".to_string()));
        assert!(!caps.contains(&"tracing".to_string()));
    }

    #[test]
    fn advertised_app_capabilities_include_gated_features_when_enabled() {
        let manifest = manifest_with_brsdk_capabilities();
        let caps = super::advertised_app_capabilities(
            &manifest,
            super::BrsdkSettings {
                encryption: true,
                tracing: true,
                ..Default::default()
            },
        );
        assert!(caps.contains(&"vault".to_string()));
        assert!(caps.contains(&"tracing".to_string()));
    }

    #[test]
    fn materialize_subagent_recipe_parses_as_workflow() {
        use biorouter_mcp::agent_drafter::manifest::SubAgentManifest;
        let m = SubAgentManifest {
            description: "Biostatistics specialist".into(),
            system_prompt: "You are a careful biostatistician. Use FDR correction.".into(),
            skills: vec!["clinical-biostatistics".into()],
            ..Default::default()
        };
        let recipe = super::materialize_subagent_recipe("stats", &m);
        // It MUST parse into a valid engine Workflow the subagent tool can load.
        let wf: biorouter::workflow::Workflow = serde_json::from_str(&recipe).unwrap();
        assert_eq!(wf.title, "stats");
        assert_eq!(
            wf.instructions.as_deref(),
            Some("You are a careful biostatistician. Use FDR correction.")
        );
        assert_eq!(wf.description, "Biostatistics specialist");
        assert_eq!(
            wf.skills.as_deref(),
            Some(&["clinical-biostatistics".to_string()][..])
        );
    }

    #[test]
    fn materialize_subagent_recipe_defaults_empty_fields_to_runnable() {
        use biorouter_mcp::agent_drafter::manifest::SubAgentManifest;
        // An under-specified sub-agent still produces a runnable recipe (non-empty
        // instructions + description), so the subagent tool won't fail to build.
        let recipe = super::materialize_subagent_recipe("helper", &SubAgentManifest::default());
        let wf: biorouter::workflow::Workflow = serde_json::from_str(&recipe).unwrap();
        assert!(wf.instructions.as_deref().unwrap_or("").contains("helper"));
        assert!(!wf.description.is_empty());
    }

    #[test]
    fn load_vault_secrets_respects_allowlist() {
        use biorouter_mcp::agent_drafter::vault::{generate_key, Vault};
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("workspace");
        std::fs::create_dir_all(&ws).unwrap();
        let vault = Vault::new(&ws, generate_key());
        vault.put("API_KEY", "sk-123").unwrap();
        vault.put("EXTRA", "should-not-load").unwrap();

        // Only allow-listed AND present names load: EXTRA is stored but not
        // allow-listed; MISSING is allow-listed but not stored. Both excluded.
        let allowed = vec!["API_KEY".to_string(), "MISSING".to_string()];
        let secrets = super::load_vault_secrets(&vault, &allowed);
        assert_eq!(secrets.len(), 1);
        assert_eq!(secrets.get("API_KEY").map(String::as_str), Some("sk-123"));
        assert!(
            !secrets.contains_key("EXTRA"),
            "a stored but non-allow-listed secret must never load"
        );
        assert!(!secrets.contains_key("MISSING"));
    }

    #[test]
    fn resolve_sql_sources_dedups_names() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("workspace");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(ws.join("a.db"), b"x").unwrap();
        std::fs::write(ws.join("b.db"), b"x").unwrap();
        let data = DataCapability {
            sources: vec![
                src("dup", "sql", Some("a.db")),
                src("dup", "sql", Some("b.db")),
            ],
        };
        let resolved = resolve_sql_sources(&ws, &data);
        assert_eq!(resolved.len(), 1, "duplicate names collapse to one");
    }

    // Guards the lowercase serde contract between the SDK (which sends
    // `{type:"tokens"}` / `{type:"history"}`) and the Rust enum. A casing drift
    // here would silently route these frames to the parser's skip path and hang
    // the SDK's tokens()/history() promises.
    #[test]
    fn client_frame_parses_v2_variants() {
        assert!(matches!(
            serde_json::from_str::<ClientFrame>(r#"{"type":"prompt","text":"hi"}"#).unwrap(),
            ClientFrame::Prompt { .. }
        ));
        assert!(matches!(
            serde_json::from_str::<ClientFrame>(r#"{"type":"cancel"}"#).unwrap(),
            ClientFrame::Cancel
        ));
        assert!(matches!(
            serde_json::from_str::<ClientFrame>(r#"{"type":"tokens"}"#).unwrap(),
            ClientFrame::Tokens
        ));
        assert!(matches!(
            serde_json::from_str::<ClientFrame>(r#"{"type":"history"}"#).unwrap(),
            ClientFrame::History
        ));
        assert!(matches!(
            serde_json::from_str::<ClientFrame>(
                r#"{"type":"modelselect","provider":"anthropic","model":"claude-opus-4-8"}"#
            )
            .unwrap(),
            ClientFrame::ModelSelect { .. }
        ));
        // The widget_action frame uses the underscore form (not lowercase-collapsed).
        assert!(matches!(
            serde_json::from_str::<ClientFrame>(
                r#"{"type":"widget_action","widgetId":"w1","action":"submit","payload":{"dose":5}}"#
            )
            .unwrap(),
            ClientFrame::WidgetAction { .. }
        ));
        // Agent-driven UI: the answer to a parked `ui_ask`, and the browser's
        // surface report. Both use the underscore form too.
        assert!(matches!(
            serde_json::from_str::<ClientFrame>(
                r#"{"type":"ui_reply","requestId":"ask-1","payload":{"pvalue":"0.01"}}"#
            )
            .unwrap(),
            ClientFrame::UiReply { .. }
        ));
        assert!(matches!(
            serde_json::from_str::<ClientFrame>(
                r#"{"type":"ui_surface","surface":{"regions":["results"],"ids":["out"]}}"#
            )
            .unwrap(),
            ClientFrame::UiSurface { .. }
        ));
        // HITL approve/reject frames.
        assert!(matches!(
            serde_json::from_str::<ClientFrame>(
                r#"{"type":"approve","request":"tr_9","action":"allow_once"}"#
            )
            .unwrap(),
            ClientFrame::Approve { .. }
        ));
        assert!(matches!(
            serde_json::from_str::<ClientFrame>(
                r#"{"type":"reject","request":"tr_9","reason":"wrong recipient"}"#
            )
            .unwrap(),
            ClientFrame::Reject { .. }
        ));
        // Unknown frame types must fail to parse (caller skips them).
        assert!(serde_json::from_str::<ClientFrame>(r#"{"type":"bogus"}"#).is_err());
    }

    #[tokio::test]
    async fn model_catalog_lists_providers_and_models() {
        let cat = super::model_catalog().await;
        assert!(!cat.is_empty(), "the build knows providers");
        // Every entry is well-formed: a name and a models array.
        for p in &cat {
            assert!(p["name"].as_str().is_some(), "provider has a name: {p}");
            assert!(p["models"].is_array(), "provider has a models array: {p}");
        }
    }

    // --- mid-turn client frames (the split-socket path) -------------------
    //
    // Before the socket was split, the loop could not read while `agent.reply`
    // was pending, so a `ui_ask` answer could never arrive and `cancel` never
    // landed mid-turn. These pin the dispatch that replaced it.

    use super::{handle_midturn_frame, MAX_QUEUED_FRAMES};
    use biorouter_mcp::agent_drafter::control::UiBridge;
    use std::collections::VecDeque;
    use tokio_util::sync::CancellationToken;

    fn midturn_ctx() -> (UiBridge, CancellationToken, VecDeque<ClientFrame>) {
        let bridge = UiBridge::new();
        let _ = bridge.attach();
        (bridge, CancellationToken::new(), VecDeque::new())
    }

    #[test]
    fn midturn_ui_reply_resolves_the_parked_ask() {
        let (bridge, cancel, mut queued) = midturn_ctx();
        // Nothing is parked yet, so the resolve is a no-op — but it must not be
        // queued as a new turn either.
        handle_midturn_frame(
            r#"{"type":"ui_reply","requestId":"ask-0","payload":{"x":1}}"#,
            &bridge,
            &cancel,
            &mut queued,
        );
        assert!(queued.is_empty(), "ui_reply must never become a new prompt");
        assert!(!cancel.is_cancelled());
    }

    #[test]
    fn midturn_ui_surface_is_recorded_not_queued() {
        let (bridge, cancel, mut queued) = midturn_ctx();
        handle_midturn_frame(
            r#"{"type":"ui_surface","surface":{"regions":["results"]}}"#,
            &bridge,
            &cancel,
            &mut queued,
        );
        assert!(queued.is_empty());
        assert!(!cancel.is_cancelled());
    }

    #[test]
    fn midturn_cancel_stops_the_turn() {
        let (bridge, cancel, mut queued) = midturn_ctx();
        handle_midturn_frame(r#"{"type":"cancel"}"#, &bridge, &cancel, &mut queued);
        assert!(
            cancel.is_cancelled(),
            "cancel must reach the agent mid-turn"
        );
        assert!(queued.is_empty());
    }

    #[test]
    fn midturn_prompt_is_queued_for_after_the_turn_not_dropped() {
        let (bridge, cancel, mut queued) = midturn_ctx();
        handle_midturn_frame(
            r#"{"type":"prompt","text":"next"}"#,
            &bridge,
            &cancel,
            &mut queued,
        );
        assert_eq!(queued.len(), 1);
        assert!(matches!(queued[0], ClientFrame::Prompt { .. }));
    }

    #[test]
    fn midturn_approvals_are_left_to_the_approval_pause() {
        let (bridge, cancel, mut queued) = midturn_ctx();
        handle_midturn_frame(
            r#"{"type":"approve","request":"tr_1","action":"allow_once"}"#,
            &bridge,
            &cancel,
            &mut queued,
        );
        handle_midturn_frame(
            r#"{"type":"reject","request":"tr_1","reason":"no"}"#,
            &bridge,
            &cancel,
            &mut queued,
        );
        assert!(queued.is_empty(), "stray approvals are dropped, not queued");
    }

    #[test]
    fn midturn_queue_is_bounded_against_a_runaway_client() {
        let (bridge, cancel, mut queued) = midturn_ctx();
        for _ in 0..(MAX_QUEUED_FRAMES + 10) {
            handle_midturn_frame(
                r#"{"type":"prompt","text":"x"}"#,
                &bridge,
                &cancel,
                &mut queued,
            );
        }
        assert_eq!(queued.len(), MAX_QUEUED_FRAMES);
    }

    #[test]
    fn midturn_garbage_is_ignored() {
        let (bridge, cancel, mut queued) = midturn_ctx();
        handle_midturn_frame("not json", &bridge, &cancel, &mut queued);
        handle_midturn_frame(r#"{"type":"bogus"}"#, &bridge, &cancel, &mut queued);
        assert!(queued.is_empty());
        assert!(!cancel.is_cancelled());
    }

    /// A reconnect reuses the cached agent's already-injected `AppControlServer`,
    /// so the registry must hand the SAME bridge back for a session id — that is
    /// what lets `attach` re-point the old server's tools at the new socket.
    #[tokio::test]
    async fn ui_bridge_registry_returns_one_bridge_per_session() {
        use biorouter_mcp::agent_drafter::control::{AppControlServer, NotifyParams};
        use biorouter_mcp::agent_drafter::manifest::UiCapability;
        use rmcp::handler::server::wrapper::Parameters;

        let first = super::ui_bridge_for("sess-a");
        // A server injected on the first connection holds `first`.
        let server = AppControlServer::new(first.clone(), UiCapability::default());

        // Second connection: same session id → same bridge → rebind its channel.
        let again = super::ui_bridge_for("sess-a");
        let (mut rx, _tok) = again.attach();

        // The OLD server's tool must now write into the NEW connection's channel.
        server
            .ui_notify(Parameters(NotifyParams {
                message: "reconnected".into(),
                level: None,
                timeout_ms: None,
            }))
            .await
            .expect("a reused server must still reach the reattached socket");
        assert_eq!(rx.try_recv().unwrap()["message"], "reconnected");

        // A different session gets its own bridge, unaffected by the above.
        let other = super::ui_bridge_for("sess-b");
        let (mut rx_other, _tok) = other.attach();
        assert!(rx_other.try_recv().is_err(), "no cross-session bleed");
    }

    #[test]
    fn advertised_capabilities_include_ui_by_default() {
        let m = manifest_with_brsdk_capabilities();
        let caps = super::advertised_app_capabilities(&m, super::BrsdkSettings::default());
        assert!(
            caps.contains(&"ui".to_string()),
            "apps drive their own UI by default: {caps:?}"
        );
    }

    /// The whole reason the socket is split: a `ui_ask` tool parks *inside*
    /// `agent.reply`, so its answer has to be read while the reply stream is
    /// still pending. This drives the real tool against the real mid-turn
    /// dispatcher and asserts it unparks — if it didn't, the turn would hang
    /// until the ask timeout.
    #[tokio::test]
    async fn a_parked_ui_ask_is_unparked_by_a_midturn_ui_reply() {
        use biorouter_mcp::agent_drafter::control::{AppControlServer, AskField, AskParams};
        use biorouter_mcp::agent_drafter::manifest::UiCapability;
        use rmcp::handler::server::wrapper::Parameters;

        let bridge = UiBridge::new();
        let (mut ui_rx, _tok) = bridge.attach();
        let server = AppControlServer::new(bridge.clone(), UiCapability::default());

        // The agent calls ui_ask; it blocks until the browser answers.
        let asking = tokio::spawn(async move {
            server
                .ui_ask(Parameters(AskParams {
                    prompt: "threshold?".into(),
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

        // The socket loop drains the `ask` command and learns its requestId.
        let cmd = tokio::time::timeout(std::time::Duration::from_secs(2), ui_rx.recv())
            .await
            .expect("the ask command must reach the socket")
            .expect("channel open");
        assert_eq!(cmd["cmd"], "ask");
        let request_id = cmd["requestId"].as_str().unwrap().to_string();

        // The browser replies mid-turn; the dispatcher must route it to the tool.
        let cancel = CancellationToken::new();
        let mut queued = VecDeque::new();
        handle_midturn_frame(
            &format!(
                r#"{{"type":"ui_reply","requestId":"{request_id}","payload":{{"p":"0.01"}}}}"#
            ),
            &bridge,
            &cancel,
            &mut queued,
        );
        assert!(queued.is_empty(), "a ui_reply is not a new turn");

        let result = tokio::time::timeout(std::time::Duration::from_secs(2), asking)
            .await
            .expect("ui_ask must not hang once its reply arrives")
            .unwrap()
            .unwrap();
        let text: String = result
            .content
            .iter()
            .flat_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect();
        assert!(
            text.contains("0.01"),
            "the tool returns the user's answer: {text}"
        );
    }

    /// `cancel` mid-turn must also release a parked ask, or the agent keeps
    /// waiting on a form the user has already abandoned.
    #[tokio::test]
    async fn midturn_cancel_releases_a_parked_ui_ask() {
        use biorouter_mcp::agent_drafter::control::{AppControlServer, AskField, AskParams};
        use biorouter_mcp::agent_drafter::manifest::UiCapability;
        use rmcp::handler::server::wrapper::Parameters;

        let bridge = UiBridge::new();
        let (mut ui_rx, _tok) = bridge.attach();
        let server = AppControlServer::new(bridge.clone(), UiCapability::default());
        let asking = tokio::spawn(async move {
            server
                .ui_ask(Parameters(AskParams {
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
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), ui_rx.recv())
            .await
            .expect("ask command emitted");

        let cancel = CancellationToken::new();
        let mut queued = VecDeque::new();
        handle_midturn_frame(r#"{"type":"cancel"}"#, &bridge, &cancel, &mut queued);
        assert!(cancel.is_cancelled());

        let result = tokio::time::timeout(std::time::Duration::from_secs(2), asking)
            .await
            .expect("cancel must unpark ui_ask")
            .unwrap()
            .unwrap();
        let text: String = result
            .content
            .iter()
            .flat_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect();
        assert!(text.contains("dismissed"), "{text}");
    }
}
