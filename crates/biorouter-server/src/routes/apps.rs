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

use std::sync::Arc;

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
use futures::StreamExt;
use serde::Deserialize;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use biorouter::agents::extension::PLATFORM_EXTENSIONS;
use biorouter::agents::{AgentEvent, ExtensionConfig, SessionConfig};
use biorouter::conversation::message::{Message, MessageContent};
use biorouter::guardrails::pii::PiiDetector;
use biorouter::model::ModelConfig;
use biorouter::providers::create as create_provider;
use biorouter::session::SessionType;
use biorouter_mcp::agent_drafter::manifest::PiiMode;
use biorouter_mcp::agent_drafter::store::{ArtifactKind, ArtifactStore, Manifest};
use biorouter_mcp::agent_drafter::{bundle, default_root, export_scaffold};

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
    // Ensure a bundle exists for agentic apps (build on demand).
    if manifest.kind == ArtifactKind::Agentic && !st.file_exists(&id, "dist/app.js") {
        let dir = st.artifact_dir(&id);
        if let Ok(report) = tokio::task::spawn_blocking(move || bundle::build_app(&dir)).await {
            if let Ok(r) = report {
                if !r.ok {
                    warn!(app = %id, "on-demand build failed: {}", r.log);
                }
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
    let dir = st.artifact_dir(&id);
    match tokio::task::spawn_blocking(move || bundle::build_app(&dir)).await {
        Ok(Ok(report)) => {
            if report.ok {
                if let Ok(mut m) = st.load_manifest(&id) {
                    m.built_at = Some(
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0),
                    );
                    let _ = st.save_manifest(&m);
                }
            }
            Json(json!({ "ok": report.ok, "used": report.used, "log": report.log }))
                .into_response()
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
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
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
}

async fn send_json(socket: &mut WebSocket, value: serde_json::Value) -> bool {
    socket
        .send(WsMessage::Text(value.to_string().into()))
        .await
        .is_ok()
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
        let Some(file) = src.file.as_ref() else { continue };
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
async fn configure_agent(
    agent: &biorouter::agents::Agent,
    state: &AppState,
    session_id: &str,
    manifest: &Manifest,
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
        if let (Ok(provider), Ok(model)) =
            (global.get_biorouter_provider(), global.get_biorouter_model())
        {
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
    agent.extend_system_prompt(prompt).await;

    // BRSDK guardrails: a one-line `goal` auto-installs the goal Stop-hook so the
    // app's agent keeps working until the goal condition holds — reusing the
    // proven /goal machinery (LLM-judge, iteration cap, stall detection, graceful
    // give-up). Opt-in (deny-by-default): only apps that declare a goal get it.
    // Idempotent: re-installed on each (re)connect via configure_agent.
    if let Some(goal) = cfg.guardrails.as_ref().and_then(|g| g.goal.clone()) {
        if !goal.trim().is_empty() {
            agent.set_goal(session_id, goal).await;
        }
    }
}

async fn handle_agent_socket(
    mut socket: WebSocket,
    state: Arc<AppState>,
    manifest: Manifest,
    client_id: Option<String>,
) {
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
                    let _ = send_json(&mut socket, json!({"type":"error","message": format!("session: {e}")})).await;
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
                let _ = send_json(&mut socket, json!({"type":"error","message": format!("session: {e}")})).await;
                return;
            }
        },
    };
    let session_id = session.id.clone();
    let message_count = session.message_count;

    let agent = match state.get_agent(session_id.clone()).await {
        Ok(a) => a,
        Err(e) => {
            let _ = send_json(&mut socket, json!({"type":"error","message": format!("agent: {e}")})).await;
            return;
        }
    };

    configure_agent(&agent, &state, &session_id, &manifest).await;
    info!(app = %manifest.id, session = %session_id, "app agent session ready");
    // BRSDK protocol v2: advertise capabilities so old apps ignore frames they
    // don't understand and new apps can feature-detect. Deny-by-default — only
    // capabilities the manifest declared are advertised.
    let capabilities = manifest
        .agent
        .as_ref()
        .map(|a| a.capabilities.advertised())
        .unwrap_or_default();
    if !send_json(
        &mut socket,
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
        return;
    }

    while let Some(Ok(msg)) = socket.next().await {
        let text = match msg {
            WsMessage::Text(t) => t.to_string(),
            WsMessage::Close(_) => break,
            _ => continue,
        };
        let frame: ClientFrame = match serde_json::from_str(&text) {
            Ok(f) => f,
            Err(_) => continue,
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
                    &mut socket,
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
                let _ = send_json(&mut socket, json!({"type":"history","messages": messages})).await;
                continue;
            }
        };

        // Content guardrail (input stage): apply the manifest's PII/PHI policy to
        // the user's message at the app boundary — before it reaches the model or
        // the conversation. Local, on-device detection (no provider). Mask rewrites
        // the prompt; Block refuses the turn. Either way a `guardrail` frame tells
        // the app what happened.
        let pii_mode = manifest
            .agent
            .as_ref()
            .and_then(|a| a.guardrails.as_ref())
            .map(|g| g.pii)
            .unwrap_or(PiiMode::Off);
        let prompt_text = match apply_pii_policy(prompt_text, pii_mode) {
            PiiOutcome::Pass(text) => text,
            PiiOutcome::Masked { text, reason } => {
                let _ = send_json(
                    &mut socket,
                    json!({"type":"guardrail","stage":"input","name":"pii","blocked":false,"reason":reason}),
                )
                .await;
                text
            }
            PiiOutcome::Blocked { reason } => {
                let _ = send_json(
                    &mut socket,
                    json!({"type":"guardrail","stage":"input","name":"pii","blocked":true,"reason":reason}),
                )
                .await;
                // End the turn cleanly without running the agent.
                if !send_json(&mut socket, json!({"type":"done"})).await {
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
            retry_config: None,
        };
        let cancel = CancellationToken::new();
        let mut stream = match agent
            .reply(user, session_config, Some(cancel.clone()))
            .await
        {
            Ok(s) => s,
            Err(e) => {
                let _ = send_json(&mut socket, json!({"type":"error","message": e.to_string()}))
                    .await;
                continue;
            }
        };

        let mut errored = false;
        while let Some(event) = stream.next().await {
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
                                Some(json!({"type":"tool","name": name, "status":"pending"}))
                            }
                            MessageContent::ToolResponse(resp) => {
                                let status = match &resp.tool_result {
                                    Ok(r) if r.is_error == Some(true) => "failed",
                                    Ok(_) => "completed",
                                    Err(_) => "failed",
                                };
                                Some(json!({"type":"tool","name":"tool","status": status}))
                            }
                            _ => None,
                        };
                        if let Some(f) = frame {
                            if !send_json(&mut socket, f).await {
                                return;
                            }
                        }
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    let _ =
                        send_json(&mut socket, json!({"type":"error","message": e.to_string()}))
                            .await;
                    errored = true;
                    break;
                }
            }
        }
        if !errored && !send_json(&mut socket, json!({"type":"done"})).await {
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

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/apps", get(list_apps))
        .route("/apps/{id}", get(redirect_to_slash).delete(delete_app_route))
        .route("/apps/{id}/", get(serve_index))
        .route("/apps/{id}/agent", get(agent_ws))
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
                assert!(text.contains("ivacaftor 150mg"), "clinical content preserved");
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
    use biorouter_mcp::agent_drafter::manifest::{DataCapability, DataSource};
    use super::resolve_sql_sources;

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
                src("ok", "sql", Some("sub/cohort.db")),          // in-workspace, exists → kept
                src("escape", "sql", Some("../secret.db")),       // traversal → rejected
                src("abs", "sql", Some("/etc/hosts")),            // absolute-outside → rejected
                src("missing", "sql", Some("nope.db")),           // in-jail but absent → dropped
                src("notsql", "knowledge", Some("sub/cohort.db")),// non-sql → skipped
                src("nofile", "sql", None),                       // no file → skipped
            ],
        };
        let resolved = resolve_sql_sources(&ws, &data);
        assert_eq!(resolved.len(), 1, "only the in-workspace existing sql source survives: {resolved:?}");
        assert!(resolved.contains_key("ok"));
        assert!(!resolved.contains_key("escape"), "traversal source must not escape the workspace");
        assert!(!resolved.contains_key("abs"), "absolute-outside source must be rejected");
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
        // Unknown frame types must fail to parse (caller skips them).
        assert!(serde_json::from_str::<ClientFrame>(r#"{"type":"bogus"}"#).is_err());
    }
}
