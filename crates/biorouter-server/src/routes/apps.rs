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
        Path, State,
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
use biorouter::model::ModelConfig;
use biorouter::providers::create as create_provider;
use biorouter::session::SessionType;
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
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
    let manifest = match store().load_manifest(&id) {
        Ok(m) => m,
        Err(_) => return (StatusCode::NOT_FOUND, "no such app").into_response(),
    };
    ws.on_upgrade(move |socket| handle_agent_socket(socket, state, manifest))
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
}

async fn send_json(socket: &mut WebSocket, value: serde_json::Value) -> bool {
    socket
        .send(WsMessage::Text(value.to_string().into()))
        .await
        .is_ok()
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
}

async fn handle_agent_socket(mut socket: WebSocket, state: Arc<AppState>, manifest: Manifest) {
    // One session per connection → conversational memory within the app session.
    let session = match state
        .session_manager()
        .create_session(
            std::env::current_dir().unwrap_or_default(),
            format!("app:{}", manifest.id),
            SessionType::User,
        )
        .await
    {
        Ok(s) => s,
        Err(e) => {
            let _ = send_json(&mut socket, json!({"type":"error","message": format!("session: {e}")})).await;
            return;
        }
    };
    let session_id = session.id.clone();

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
        json!({"type":"ready", "protocol": 2, "capabilities": capabilities}),
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
