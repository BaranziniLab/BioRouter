//! Agent Drafter — author interactive **BioRouter apps**: TypeScript front-ends
//! wired to a *real* BioRouter agent backend.
//!
//! An Agent-Drafter app is a self-contained project the assistant builds for the
//! user. The UI is authored in TypeScript (bundled with esbuild) and the app
//! talks to BioRouter over a per-app WebSocket: when the user sends a message,
//! the BioRouter backend runs the **full agent loop** — the app's own model,
//! extensions, skills and knowledge base — and streams the answer (text /
//! markdown / tool activity) straight back into the app. Apps are *launched in
//! the browser* (GUI) or via a printed URL (CLI), not embedded in a chat iframe.
//!
//! Apps live under `~/.config/biorouter/agent_drafter/<id>/` (a project dir with
//! `manifest.json`, `index.html`, `src/*.ts`, `dist/app.js`). `biorouterd` serves
//! them at `/apps/<id>/` and exposes the agent socket at `/apps/<id>/agent`.
//! `export_app` produces a standalone runnable TypeScript project.

pub mod bundle;
pub mod manifest;
pub mod render;
pub mod store;
pub mod vault;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use etcetera::{choose_app_strategy, AppStrategy};
use indoc::formatdoc;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, Content, ErrorCode, ErrorData, Implementation, ResourceContents, Role,
        ServerCapabilities, ServerInfo,
    },
    schemars::JsonSchema,
    service::RequestContext,
    tool, tool_handler, tool_router, RoleServer, ServerHandler,
};
use serde::Deserialize;
use std::path::PathBuf;

/// Meta key the agent loop uses to pass the current chat session id into MCP
/// tool calls. Must match `biorouter::session_context::SESSION_ID_HEADER`
/// (duplicated here to avoid a circular dependency on the `biorouter` crate,
/// the same way `knowledge::server` does).
const SESSION_ID_META_KEY: &str = "biorouter-session-id";

use store::{AgentConfig, ArtifactKind, ArtifactStore, Manifest, ModelSelection};

/// Optional suggestions only. Apps are **provider-agnostic**: by default an app
/// pins no model and inherits whatever provider/model the user has configured in
/// BioRouter (any supported provider). A specific provider+model is stored only
/// when the caller explicitly chooses one. These constants are not auto-applied.
pub const DEFAULT_APP_PROVIDER: &str = "xiaomi_mimo";
pub const DEFAULT_APP_MODEL: &str = "mimo-v2.5";

// ---------------------------------------------------------------------------
// Tool parameter structs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FileSpec {
    /// Path relative to the app root (e.g. "src/main.ts", "assets/logo.svg").
    pub path: String,
    /// File contents.
    pub content: String,
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct ModelParam {
    /// Provider name (e.g. "xiaomi_mimo", "anthropic", "openai").
    #[serde(default)]
    pub provider: Option<String>,
    /// Model name (e.g. "mimo-v2.5", "claude-opus-4-8").
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateAppParams {
    /// Human-readable title; also used to derive the app id when `id` is omitted.
    pub title: String,
    /// Optional explicit app id (slugified). If given and an app already exists
    /// at that id, it is REPLACED — so re-creating the same app is idempotent
    /// and the app stays addressable. Omit to auto-derive a unique id from the
    /// title.
    #[serde(default)]
    pub id: Option<String>,
    /// Short description of what the app does.
    #[serde(default)]
    pub description: String,
    /// "agentic" (default — wired to a BioRouter agent) or "static".
    #[serde(default)]
    pub kind: Option<String>,
    /// Entry HTML (index.html). If omitted, a BioRouter-styled starter is used.
    #[serde(default)]
    pub html: Option<String>,
    /// Additional files (TypeScript under `src/`, assets, etc.). Provide your own
    /// `src/main.ts` to drive a custom UI; otherwise a starter is written.
    #[serde(default)]
    pub files: Vec<FileSpec>,
    /// System prompt defining the app agent's behavior/persona.
    #[serde(default)]
    pub system_prompt: Option<String>,
    /// Greeting shown when the chat panel mounts.
    #[serde(default)]
    pub greeting: Option<String>,
    /// Provider+model the app's agent runs on (any BioRouter-supported provider).
    /// Omit to inherit the user's configured provider/model (provider-agnostic).
    #[serde(default)]
    pub model: Option<ModelParam>,
    /// Builtin/platform extension names the agent may use (e.g. "developer",
    /// "autovisualiser", "computercontroller", "knowledge").
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Skill ids the agent should have available.
    #[serde(default)]
    pub skills: Vec<String>,
    /// Knowledge base id to scope the agent to.
    #[serde(default)]
    pub knowledge_base: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConfigureAppParams {
    /// App id.
    pub id: String,
    /// New system prompt / persona.
    #[serde(default)]
    pub system_prompt: Option<String>,
    /// New greeting.
    #[serde(default)]
    pub greeting: Option<String>,
    /// Provider+model override.
    #[serde(default)]
    pub model: Option<ModelParam>,
    /// Replace the extension list.
    #[serde(default)]
    pub extensions: Option<Vec<String>>,
    /// Replace the skills list.
    #[serde(default)]
    pub skills: Option<Vec<String>>,
    /// Set (or clear, with empty string) the knowledge base id.
    #[serde(default)]
    pub knowledge_base: Option<String>,
    /// Bound the agent's tool-calling loop per message. Raise this for
    /// workflow-style apps that chain many tool calls; lower it to keep apps
    /// snappy. Unset → a safe server default.
    #[serde(default)]
    pub max_turns: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetAppSizeParams {
    /// App id.
    pub id: String,
    /// Preferred width in CSS px (omit to fill).
    #[serde(default)]
    pub width: Option<u32>,
    /// Preferred height in CSS px (omit for auto).
    #[serde(default)]
    pub height: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateAppParams {
    /// App id.
    pub id: String,
    /// File to modify (defaults to the entry HTML).
    #[serde(default)]
    pub path: Option<String>,
    /// Full new contents (write mode).
    #[serde(default)]
    pub content: Option<String>,
    /// Exact substring to replace (str-replace mode; requires `new_str`).
    #[serde(default)]
    pub old_str: Option<String>,
    /// Replacement text for `old_str`.
    #[serde(default)]
    pub new_str: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListAppsParams {
    /// Optional filter: "static" or "agentic".
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadAppParams {
    /// App id.
    pub id: String,
    /// File to read. If omitted, returns the manifest.
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AppIdParams {
    /// App id.
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExportAppParams {
    /// App id.
    pub id: String,
    /// Destination directory (created if missing).
    pub target_dir: String,
    /// Override the agent WebSocket endpoint the exported app connects to.
    #[serde(default)]
    pub endpoint: Option<String>,
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

/// Agent Drafter MCP server.
#[derive(Clone)]
pub struct AgentDrafterServer {
    tool_router: ToolRouter<Self>,
    instructions: String,
    root: PathBuf,
}

impl Default for AgentDrafterServer {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared default store root (`~/.config/biorouter/agent_drafter`). Public so the
/// server's `/apps` routes resolve the same location.
pub fn default_root() -> PathBuf {
    choose_app_strategy(crate::APP_STRATEGY.clone())
        .map(|s| s.in_config_dir("agent_drafter"))
        .unwrap_or_else(|_| PathBuf::from(".config/biorouter/agent_drafter"))
}

fn err(code: ErrorCode, msg: impl Into<String>) -> ErrorData {
    ErrorData::new(code, msg.into(), None)
}

fn internal(e: impl std::fmt::Display) -> ErrorData {
    err(ErrorCode::INTERNAL_ERROR, e.to_string())
}

impl From<ModelParam> for ModelSelection {
    fn from(p: ModelParam) -> Self {
        ModelSelection {
            provider: p.provider.filter(|s| !s.trim().is_empty()),
            model: p.model.filter(|s| !s.trim().is_empty()),
            ..Default::default()
        }
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Recursively collect an app's files (relative path → contents), skipping
/// `manifest.json`.
fn collect_files(base: &std::path::Path, dir: &std::path::Path, out: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(base, &path, out);
        } else if let Ok(rel) = path.strip_prefix(base) {
            let rel = rel.to_string_lossy().replace('\\', "/");
            if rel == "manifest.json" {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&path) {
                out.push((rel, content));
            }
        }
    }
}

/// Gather an app's files and produce the standalone export scaffold (a file map
/// of relative-path → contents). Shared by the `export_app` tool and the
/// server's `GET /apps/{id}/export` route. `endpoint` overrides the agent
/// WebSocket the exported app connects to (None → a local biorouterd).
pub fn export_scaffold(
    root: &std::path::Path,
    id: &str,
    endpoint: Option<&str>,
) -> std::io::Result<Vec<(String, String)>> {
    let store = ArtifactStore::new(root.to_path_buf());
    let manifest = store.load_manifest(id)?;
    let dir = store.artifact_dir(id);
    let mut all_files = Vec::new();
    collect_files(&dir, &dir, &mut all_files);
    let entry_html = all_files
        .iter()
        .find(|(p, _)| p == &manifest.entry)
        .map(|(_, c)| c.clone())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "entry file missing"))?;
    let src_files: Vec<(String, String)> = all_files
        .iter()
        .filter(|(p, _)| p.starts_with("src/"))
        .cloned()
        .collect();
    let extra_files: Vec<(String, String)> = all_files
        .iter()
        .filter(|(p, _)| p != &manifest.entry && !p.starts_with("src/") && !p.starts_with("dist/"))
        .cloned()
        .collect();
    let mut scaffold =
        render::scaffold_standalone(&manifest, &entry_html, &src_files, &extra_files, endpoint);
    // Ship a prebuilt bundle so the export is directly runnable with no build
    // step (the launcher / a static server can serve it as-is). Build on demand.
    if manifest.kind == ArtifactKind::Agentic {
        if !store.file_exists(id, "dist/app.js") {
            let _ = bundle::build_app(&dir);
        }
        if let Ok(js) = store.read_file(id, "dist/app.js") {
            scaffold.push(("dist/app.js".to_string(), js));
        }
    }
    Ok(scaffold)
}

#[tool_router(router = tool_router)]
impl AgentDrafterServer {
    pub fn new() -> Self {
        Self::with_root(default_root())
    }

    /// The chat session id carried in a tool call's request meta, if present.
    fn session_id_from_context(context: &RequestContext<RoleServer>) -> Option<String> {
        context
            .meta
            .0
            .get(SESSION_ID_META_KEY)
            .and_then(|v| v.as_str())
            .map(str::to_string)
    }

    pub fn with_root(root: PathBuf) -> Self {
        let instructions = formatdoc! {r#"
            Agent Drafter builds interactive **BioRouter apps** for the user:
            TypeScript front-ends wired to a real BioRouter agent. Think "Claude
            artifacts", but each app embeds a genuine BioRouter backend — when the
            user sends a message, BioRouter runs the full agent loop (the app's own
            model, extensions, skills, knowledge base) and streams the answer back
            into the app. Apps open in the user's browser (GUI) or via a printed
            URL (CLI); they are NOT shown in a chat iframe.

            Two kinds:
            - "agentic" (default): a UI plus a live BioRouter agent + chat. Use this
              for assistants, dashboards that reason over results, search tools, etc.
            - "static": a plain interactive page with no agent.

            Project layout (kept consistent):
            - `index.html` — the UI shell you author.
            - `src/main.ts` — your app logic (TypeScript), `import`ing `./sdk`.
            - `src/sdk.ts` — the BioRouter App SDK (provided): opens the agent
              WebSocket, streams markdown, handles multimodal (image) input, and can
              auto-mount a chat panel into any element with `data-br-chat`.
            - `dist/app.js` — the esbuild bundle (produced by `build_app`).

            AESTHETICS — default to BioRouter's look unless the user asks for a
            different style. BioRouter's design language is calm, minimal, and
            informative: a warm neutral palette (cream/taupe), white cards on a
            light ground, generous whitespace, thin hairline borders, soft flat
            shadows, a restrained near-black accent with a single coral spark,
            modest radii (6px controls / 12px cards), a plain system typeface, and
            quiet hover transitions. The design system is injected automatically.
            ALWAYS compose with the provided classes (`br-container`, `br-card`,
            `br-btn` [+ `--secondary`/`--ghost`], `br-input`, `br-textarea`,
            `br-label`, `br-field`, `br-row`, `br-badge`, `br-chat`) and CSS
            variables (`var(--br-text)`, `var(--br-accent)`, `var(--br-coral)`,
            `var(--br-border)`, `var(--br-muted)`, …).
            Do NOT: paste your own color values or fonts, pull in external CSS
            frameworks/CDNs, use gradients/neon/glassmorphism, emoji-stuffed
            headings, or other flashy "generic AI" looks. Prefer filled shades
            over outlines, clear hierarchy, and restraint. Aim for taste:
            well-aligned, breathable layouts that look native to BioRouter and feel
            simple yet polished. If (and only if) the user specifies a different
            visual style — now or later in the conversation — follow their
            direction instead.

            Driving the agent from `src/main.ts`:
              import {{ createApp }} from "./sdk";
              const br = createApp({{ autoChat: false }});   // false → build your OWN UI
              await br.run("...prompt...", '#out');            // stream markdown+charts into the result element
              const text = await br.ask("...");               // collect full reply as a string
              await br.prompt("...", {{ images: [{{ mimeType, data }}] }}); // multimodal
              br.on("message", (e) => {{ if (e.type === "message") {{}} }}); // low-level stream

            VARY THE INTERFACE — do NOT make every app a chat box. Prefer a custom
            UI driven by `createApp({{ autoChat: false }})` and wire controls to
            `br.run(prompt, target)`. The design system provides themed,
            BioRouter-native controls — use a mix that fits the task:
            - buttons / button grids: `br-btn`, `br-grid`
            - dropdowns: `<select class="br-select">`
            - sliders: `<input type="range" class="br-slider">` (+ `br-slider-val`)
            - toggles: `<label class="br-switch"><input type="checkbox"><span class="br-switch__track"></span></label>`
            - checkboxes/radios: `br-check`; selectable chips/tags: `br-chips`/`br-chip`
            - tabs: `br-tabs`/`br-tab`; cards: `br-card`; layout: `br-grid`/`br-row`
            - drag & drop: `br-dropzone` (drop files/text) and `br-draglist`/`br-dragitem` (reorder)
            - region/map pick: `br-mapgrid`/`br-region` (clickable cells; no external map lib)
            - results: a `<div class="br-output" data-placeholder="…">` target for `br.run`
            Build the prompt from the control state (slider values, selected
            chips, dropdown choice, dragged order, clicked region, dropped text)
            and call `br.run(...)` on `change`/`click`/`drop`. Each new app should
            look and interact differently from the others.

            Typical workflow:
            1. `create_app` (title, description, optional html/files, system_prompt,
               greeting, model, extensions, skills, knowledge_base). A preview card
               is shown to the user.
            2. Author the UI: `update_app` the entry HTML and `src/main.ts`.
            3. `configure_app` to change the model/extensions/skills/knowledge/persona.
            4. `build_app` to bundle the TypeScript.
            5. `launch_app` to open it in the browser (returns the URL).
            6. `export_app` for a standalone runnable project.

            Use `list_apps`, `read_app`, and `preview_app` to inspect existing apps —
            you can query and modify any previously-built app.

            WORKFLOW-STYLE APPS (multi-step agentic loops, not just chat): every
            user message runs BioRouter's full agent loop — the agent can call
            many tools in sequence and reason over the results before replying, so
            an app can encode a real pipeline. Design one by: (a) giving it the
            extensions/skills/knowledge it needs, (b) writing a system_prompt that
            spells out the ordered procedure ("1. search … 2. extract … 3.
            summarize as a table … 4. emit a ```chart block"), and (c) raising
            `max_turns` (via `configure_app`) so it can chain enough tool calls.
            `max_turns` also bounds the loop (a guardrail against runaway/cost).
            The app surfaces each step to the user as a tool event.

            BUILD HARNESS / guardrails: `build_app` (and `lint_app`) run a
            validation harness on whatever you generate and report findings. It
            enforces three things — fix any ERRORs before `launch_app`/`export_app`:
            1. Backend wiring: `src/main.ts` imports from "./sdk" and calls the
               agent (`br.run`/`br.prompt`/`br.ask`) or enables autoChat.
            2. Self-contained: no external `<script>`/`<link>`/CDN in index.html
               and no non-local imports in `src/main.ts` (so exports run offline).
            3. On-theme: uses `br-*` classes/CSS variables, not raw hex colors or
               a custom `<style>` theme; includes a result surface (`.br-output`
               or `[data-br-chat]`).
            Always `build_app` after editing `src/`, address the harness findings,
            and verify via `launch_app` before `export_app`.
        "#};
        Self {
            tool_router: Self::tool_router(),
            instructions,
            root,
        }
    }

    pub fn root(&self) -> &std::path::Path {
        &self.root
    }

    fn store(&self) -> ArtifactStore {
        ArtifactStore::new(self.root.clone())
    }

    /// Build a card-preview result (ui:// blob for the user + assistant note).
    fn card_result(&self, manifest: &Manifest, note: &str) -> Result<CallToolResult, ErrorData> {
        let store = self.store();
        let entry_html = store
            .read_file(&manifest.id, &manifest.entry)
            .map_err(|e| err(ErrorCode::INTERNAL_ERROR, format!("read entry: {e}")))?;
        let html = render::assemble_card(manifest, &entry_html);
        let blob = STANDARD.encode(html.as_bytes());

        let width_css = manifest
            .width
            .map(|w| format!("{w}px"))
            .unwrap_or_else(|| "100%".to_string());
        let height_css = manifest
            .height
            .map(|h| format!("{h}px"))
            .unwrap_or_else(|| "420px".to_string());
        let mut meta_obj = serde_json::Map::new();
        meta_obj.insert(
            "mcpui.dev/ui-preferred-frame-size".to_string(),
            serde_json::json!([width_css, height_css]),
        );

        let resource = ResourceContents::BlobResourceContents {
            uri: format!("ui://agent-drafter/{}", manifest.id),
            mime_type: Some("text/html".to_string()),
            blob,
            meta: Some(rmcp::model::Meta(meta_obj)),
        };
        Ok(CallToolResult::success(vec![
            Content::resource(resource).with_audience(vec![Role::User]),
            Content::text(note.to_string()).with_audience(vec![Role::Assistant]),
        ]))
    }

    #[tool(
        name = "create_app",
        description = "Create a new BioRouter app: a TypeScript UI wired to a live BioRouter agent (kind 'agentic', default) or a static page. Returns a preview card."
    )]
    pub async fn create_app(
        &self,
        params: Parameters<CreateAppParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        // Record the chat session this app was built in, so the GUI can reopen
        // that conversation to keep iterating. (Absent in headless/CLI calls
        // that don't carry session meta.)
        self.create_app_inner(params.0, Self::session_id_from_context(&context))
            .await
    }

    async fn create_app_inner(
        &self,
        p: CreateAppParams,
        session_id: Option<String>,
    ) -> Result<CallToolResult, ErrorData> {
        if p.title.trim().is_empty() {
            return Err(err(ErrorCode::INVALID_PARAMS, "title must not be empty"));
        }
        let kind = match p.kind.as_deref() {
            Some(k) => ArtifactKind::parse(k).ok_or_else(|| {
                err(
                    ErrorCode::INVALID_PARAMS,
                    "kind must be 'static' or 'agentic'",
                )
            })?,
            None => ArtifactKind::Agentic,
        };
        let entry = "index.html";
        let entry_html = p
            .html
            .unwrap_or_else(|| render::starter(&p.title, &p.description));

        // Compose file set: entry + default TS sources + caller overrides.
        let mut files: Vec<(String, String)> = vec![(entry.to_string(), entry_html)];
        let provided: std::collections::HashSet<&str> =
            p.files.iter().map(|f| f.path.as_str()).collect();
        if kind == ArtifactKind::Agentic {
            for (path, content) in bundle::default_sources() {
                let ps = path.to_string_lossy().to_string();
                if ps != entry && !provided.contains(ps.as_str()) {
                    files.push((ps, content));
                }
            }
        }
        for f in p.files {
            if f.path != entry {
                files.push((f.path, f.content));
            }
        }

        let store = self.store();
        let mut manifest = match p.id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(explicit) => {
                store.create_with_id(explicit, &p.title, &p.description, kind, entry, &files)
            }
            None => store.create(&p.title, &p.description, kind, entry, &files),
        }
        .map_err(internal)?;

        manifest.session_id = session_id.clone();

        if kind == ArtifactKind::Agentic {
            // Provider-agnostic by default: leave the model unset so the app uses
            // whatever provider/model the user has configured in BioRouter. Pin a
            // specific provider+model only when the caller explicitly chose one —
            // any BioRouter-supported provider works.
            let model = p.model.map(ModelSelection::from).filter(|m| m.is_set());
            manifest.agent = Some(AgentConfig {
                system_prompt: p.system_prompt.unwrap_or_default(),
                greeting: p.greeting,
                tools: Vec::new(),
                model,
                extensions: p.extensions,
                skills: p.skills,
                knowledge_base: p.knowledge_base.filter(|s| !s.trim().is_empty()),
                max_turns: None,
                ..Default::default()
            });
            store.save_manifest(&manifest).map_err(internal)?;
        } else if session_id.is_some() {
            // Static apps were already persisted by `create`; re-save so the
            // freshly-stamped session id lands on disk.
            store.save_manifest(&manifest).map_err(internal)?;
        }

        self.card_result(
            &manifest,
            &format!(
                "Created {kind:?} app '{}' (id: {}). Author src/main.ts and index.html, then build_app + launch_app.",
                manifest.title, manifest.id
            ),
        )
    }

    #[tool(
        name = "configure_app",
        description = "Set an app's agent config: system prompt/persona, greeting, model (provider+model), extensions, skills, knowledge base, and max_turns (bound/raise the tool-calling loop for workflow-style apps). Makes the app agentic if it wasn't."
    )]
    pub async fn configure_app(
        &self,
        params: Parameters<ConfigureAppParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        let store = self.store();
        let mut manifest = store
            .load_manifest(&p.id)
            .map_err(|_| err(ErrorCode::INVALID_PARAMS, format!("no app '{}'", p.id)))?;
        manifest.kind = ArtifactKind::Agentic;
        let mut agent = manifest.agent.take().unwrap_or_default();
        if let Some(sp) = p.system_prompt {
            agent.system_prompt = sp;
        }
        if let Some(g) = p.greeting {
            agent.greeting = Some(g).filter(|s| !s.is_empty());
        }
        if let Some(m) = p.model {
            // Empty provider+model clears the pin → inherit the user's configured
            // provider/model. Any BioRouter-supported provider may be pinned.
            agent.model = Some(ModelSelection::from(m)).filter(|s| s.is_set());
        }
        if let Some(ext) = p.extensions {
            agent.extensions = ext;
        }
        if let Some(sk) = p.skills {
            agent.skills = sk;
        }
        if let Some(kb) = p.knowledge_base {
            agent.knowledge_base = Some(kb).filter(|s| !s.trim().is_empty());
        }
        if let Some(mt) = p.max_turns {
            agent.max_turns = Some(mt).filter(|&n| n > 0);
        }
        manifest.agent = Some(agent);
        store.save_manifest(&manifest).map_err(internal)?;
        store.touch(&p.id).map_err(internal)?;
        self.card_result(&manifest, &format!("Configured app '{}'.", p.id))
    }

    #[tool(
        name = "set_app_size",
        description = "Set an app's preferred preview-card size in CSS px (width/height). Omit a value to fill/auto."
    )]
    pub async fn set_app_size(
        &self,
        params: Parameters<SetAppSizeParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        let store = self.store();
        let mut manifest = store
            .load_manifest(&p.id)
            .map_err(|_| err(ErrorCode::INVALID_PARAMS, format!("no app '{}'", p.id)))?;
        manifest.width = p.width;
        manifest.height = p.height;
        store.save_manifest(&manifest).map_err(internal)?;
        store.touch(&p.id).map_err(internal)?;
        self.card_result(&manifest, &format!("Set preview size for '{}'.", p.id))
    }

    #[tool(
        name = "update_app",
        description = "Edit a file in an app: provide full `content` to overwrite, or `old_str`+`new_str` to replace a snippet. Defaults to index.html. Editing src/ marks the bundle stale (re-run build_app)."
    )]
    pub async fn update_app(
        &self,
        params: Parameters<UpdateAppParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        let store = self.store();
        let mut manifest = store
            .load_manifest(&p.id)
            .map_err(|_| err(ErrorCode::INVALID_PARAMS, format!("no app '{}'", p.id)))?;
        let path = p.path.clone().unwrap_or_else(|| manifest.entry.clone());

        let updated_content = if let Some(content) = p.content {
            content
        } else if let (Some(old), Some(new)) = (p.old_str.as_ref(), p.new_str.as_ref()) {
            let current = store
                .read_file(&p.id, &path)
                .map_err(|e| err(ErrorCode::INVALID_PARAMS, format!("read {path}: {e}")))?;
            if !current.contains(old.as_str()) {
                return Err(err(
                    ErrorCode::INVALID_PARAMS,
                    format!("old_str not found in {path}"),
                ));
            }
            current.replacen(old.as_str(), new.as_str(), 1)
        } else {
            return Err(err(
                ErrorCode::INVALID_PARAMS,
                "provide either `content` or both `old_str` and `new_str`",
            ));
        };

        if path == "manifest.json" {
            let parsed: Manifest = serde_json::from_str(&updated_content).map_err(|e| {
                err(
                    ErrorCode::INVALID_PARAMS,
                    format!("manifest.json must be valid Agent Drafter manifest JSON: {e}"),
                )
            })?;
            if parsed.id != p.id {
                return Err(err(
                    ErrorCode::INVALID_PARAMS,
                    format!(
                        "manifest.json id '{}' must match the app id '{}'",
                        parsed.id, p.id
                    ),
                ));
            }
        }

        store
            .write_file(&p.id, &path, &updated_content)
            .map_err(internal)?;
        // Editing sources invalidates the build.
        if path.starts_with("src/") {
            manifest.built_at = None;
            store.save_manifest(&manifest).map_err(internal)?;
        }
        store.touch(&p.id).map_err(internal)?;

        if path == "manifest.json" {
            Ok(CallToolResult::success(vec![Content::text(format!(
                "Updated manifest.json in '{}'.",
                p.id
            ))]))
        } else if path == manifest.entry {
            self.card_result(&manifest, &format!("Updated {path} in '{}'.", p.id))
        } else {
            let hint = if path.starts_with("src/") {
                " (run build_app to rebundle)"
            } else {
                ""
            };
            Ok(CallToolResult::success(vec![Content::text(format!(
                "Updated {path} in '{}'.{hint}",
                p.id
            ))]))
        }
    }

    #[tool(
        name = "build_app",
        description = "Bundle the app's TypeScript (src/main.ts → dist/app.js) with esbuild. Run after editing src/. Returns the build log."
    )]
    pub async fn build_app(
        &self,
        params: Parameters<AppIdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        let store = self.store();
        let mut manifest = store
            .load_manifest(&p.id)
            .map_err(|_| err(ErrorCode::INVALID_PARAMS, format!("no app '{}'", p.id)))?;
        let dir = store.artifact_dir(&p.id);
        let report = tokio::task::spawn_blocking(move || bundle::build_app(&dir))
            .await
            .map_err(internal)?
            .map_err(internal)?;
        if report.ok {
            manifest.built_at = Some(now_secs());
            store.save_manifest(&manifest).map_err(internal)?;
            // Run the guardrail harness and surface findings so the agent can
            // self-correct (SDK-wired, self-contained, on-theme).
            let lint = bundle::lint_app(&store.artifact_dir(&p.id));
            Ok(CallToolResult::success(vec![Content::text(format!(
                "Built '{}' with {} → dist/app.js.\n{}\n\n{}",
                p.id,
                report.used,
                bundle::format_lint(&lint),
                report.log
            ))]))
        } else {
            Err(err(
                ErrorCode::INTERNAL_ERROR,
                format!("build failed for '{}':\n{}", p.id, report.log),
            ))
        }
    }

    #[tool(
        name = "lint_app",
        description = "Run the build harness guardrails on an app and report findings: does it reach the backend via the App SDK, is it self-contained (no CDN/external assets), and is it on-theme (BioRouter classes/tokens)? Fix ERRORs before launch/export."
    )]
    pub async fn lint_app(
        &self,
        params: Parameters<AppIdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        let store = self.store();
        if !store.exists(&p.id) {
            return Err(err(ErrorCode::INVALID_PARAMS, format!("no app '{}'", p.id)));
        }
        let findings = bundle::lint_app(&store.artifact_dir(&p.id));
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Harness check for '{}':\n{}",
            p.id,
            bundle::format_lint(&findings)
        ))]))
    }

    #[tool(
        name = "launch_app",
        description = "Build (if needed) and launch a BioRouter app. Returns the URL to open in the browser (GUI auto-opens; CLI prints it)."
    )]
    pub async fn launch_app(
        &self,
        params: Parameters<AppIdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        let store = self.store();
        let mut manifest = store
            .load_manifest(&p.id)
            .map_err(|_| err(ErrorCode::INVALID_PARAMS, format!("no app '{}'", p.id)))?;

        // Ensure a fresh bundle exists.
        if manifest.kind == ArtifactKind::Agentic
            && (manifest.built_at.is_none() || !store.file_exists(&p.id, "dist/app.js"))
        {
            let dir = store.artifact_dir(&p.id);
            let report = tokio::task::spawn_blocking(move || bundle::build_app(&dir))
                .await
                .map_err(internal)?
                .map_err(internal)?;
            if !report.ok {
                return Err(err(
                    ErrorCode::INTERNAL_ERROR,
                    format!("build failed before launch:\n{}", report.log),
                ));
            }
            manifest.built_at = Some(now_secs());
            store.save_manifest(&manifest).map_err(internal)?;
        }

        let lint = bundle::lint_app(&store.artifact_dir(&p.id));
        if lint.iter().any(|f| f.level == bundle::LintLevel::Error) {
            return Err(err(
                ErrorCode::INTERNAL_ERROR,
                format!(
                    "harness ERRORs block launch for '{}':\n{}",
                    p.id,
                    bundle::format_lint(&lint)
                ),
            ));
        }

        let path = format!("/apps/{}/", manifest.id);
        let base = std::env::var("BIOROUTER_APP_BASE_URL").ok();
        let url = base
            .as_ref()
            .map(|b| format!("{}{}", b.trim_end_matches('/'), path))
            .unwrap_or_else(|| path.clone());

        // Surface a launch marker the GUI can act on, plus a human URL line.
        let mut meta = serde_json::Map::new();
        meta.insert(
            "biorouter/launch-app".to_string(),
            serde_json::json!(manifest.id),
        );
        meta.insert("biorouter/app-path".to_string(), serde_json::json!(path));
        let mut result = CallToolResult::success(vec![Content::text(format!(
            "App '{}' is ready. Open it in your browser: {}\n(In the desktop GUI use the Applications panel's Launch button; in the CLI open the URL above with a running biorouterd.)",
            manifest.id, url
        ))]);
        result.meta = Some(rmcp::model::Meta(meta));
        Ok(result)
    }

    #[tool(
        name = "list_apps",
        description = "List all BioRouter apps (optionally filtered by kind)."
    )]
    pub async fn list_apps(
        &self,
        params: Parameters<ListAppsParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let filter = match params.0.kind.as_deref() {
            Some(k) => Some(ArtifactKind::parse(k).ok_or_else(|| {
                err(
                    ErrorCode::INVALID_PARAMS,
                    "kind must be 'static' or 'agentic'",
                )
            })?),
            None => None,
        };
        let list: Vec<Manifest> = self
            .store()
            .list()
            .into_iter()
            .filter(|m| filter.map(|k| k == m.kind).unwrap_or(true))
            .collect();
        if list.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "No apps yet.".to_string(),
            )]));
        }
        let lines: Vec<String> = list
            .iter()
            .map(|m| {
                let model = m
                    .agent
                    .as_ref()
                    .and_then(|a| a.model.as_ref())
                    .and_then(|s| s.model.clone())
                    .unwrap_or_else(|| "default".into());
                format!("- {} [{:?}, model: {}] — {}", m.id, m.kind, model, m.title)
            })
            .collect();
        Ok(CallToolResult::success(vec![Content::text(
            lines.join("\n"),
        )]))
    }

    #[tool(
        name = "read_app",
        description = "Read an app's manifest, or a specific file within it."
    )]
    pub async fn read_app(
        &self,
        params: Parameters<ReadAppParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        let store = self.store();
        match p.path {
            None => {
                let m = store
                    .load_manifest(&p.id)
                    .map_err(|_| err(ErrorCode::INVALID_PARAMS, format!("no app '{}'", p.id)))?;
                let json = serde_json::to_string_pretty(&m).map_err(internal)?;
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Some(path) => {
                let content = store
                    .read_file(&p.id, &path)
                    .map_err(|e| err(ErrorCode::INVALID_PARAMS, format!("read {path}: {e}")))?;
                Ok(CallToolResult::success(vec![Content::text(content)]))
            }
        }
    }

    #[tool(
        name = "preview_app",
        description = "Render an app's preview card for the user."
    )]
    pub async fn preview_app(
        &self,
        params: Parameters<AppIdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        let manifest = self
            .store()
            .load_manifest(&p.id)
            .map_err(|_| err(ErrorCode::INVALID_PARAMS, format!("no app '{}'", p.id)))?;
        self.card_result(&manifest, &format!("Preview of '{}'.", p.id))
    }

    #[tool(
        name = "export_app",
        description = "Export an app as a standalone, runnable TypeScript project (esbuild build + a tiny static server) that talks to a BioRouter daemon."
    )]
    pub async fn export_app(
        &self,
        params: Parameters<ExportAppParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        let store = self.store();
        if !store.exists(&p.id) {
            return Err(err(ErrorCode::INVALID_PARAMS, format!("no app '{}'", p.id)));
        }
        let lint = bundle::lint_app(&store.artifact_dir(&p.id));
        if lint.iter().any(|f| f.level == bundle::LintLevel::Error) {
            return Err(err(
                ErrorCode::INTERNAL_ERROR,
                format!(
                    "harness ERRORs block export for '{}':\n{}",
                    p.id,
                    bundle::format_lint(&lint)
                ),
            ));
        }
        let scaffold = export_scaffold(self.root(), &p.id, p.endpoint.as_deref())
            .map_err(|_| err(ErrorCode::INVALID_PARAMS, format!("no app '{}'", p.id)))?;

        let target = PathBuf::from(&p.target_dir);
        for (rel, content) in &scaffold {
            let full = target.join(rel);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).map_err(internal)?;
            }
            std::fs::write(&full, content).map_err(internal)?;
        }

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Exported '{}' as a standalone TypeScript project to {} ({} files). README.md: npm install && npm run build && npm start (with a biorouterd running).",
            p.id,
            target.display(),
            scaffold.len()
        ))]))
    }

    #[tool(
        name = "delete_app",
        description = "Delete an app and all of its files."
    )]
    pub async fn delete_app(
        &self,
        params: Parameters<AppIdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        let store = self.store();
        if !store.exists(&p.id) {
            return Err(err(ErrorCode::INVALID_PARAMS, format!("no app '{}'", p.id)));
        }
        store.delete(&p.id).map_err(internal)?;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Deleted app '{}'.",
            p.id
        ))]))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AgentDrafterServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: "biorouter-agent-drafter".to_string(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
                title: Some("Agent Drafter".to_string()),
                icons: None,
                website_url: None,
            },
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            instructions: Some(self.instructions.clone()),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::RawContent;
    use tempfile::TempDir;

    fn server() -> (TempDir, AgentDrafterServer) {
        let dir = TempDir::new().unwrap();
        let s = AgentDrafterServer::with_root(dir.path().to_path_buf());
        (dir, s)
    }

    fn text_of(result: &CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|c| match &c.raw {
                RawContent::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn has_ui_resource(result: &CallToolResult) -> bool {
        result
            .content
            .iter()
            .any(|c| matches!(&c.raw, RawContent::Resource(_)))
    }

    fn create(title: &str, kind: Option<&str>) -> CreateAppParams {
        CreateAppParams {
            title: title.into(),
            id: None,
            description: String::new(),
            kind: kind.map(|k| k.to_string()),
            html: None,
            files: vec![],
            system_prompt: None,
            greeting: None,
            model: None,
            extensions: vec![],
            skills: vec![],
            knowledge_base: None,
        }
    }

    #[tokio::test]
    async fn create_app_writes_ts_project_and_defaults() {
        let (_d, s) = server();
        let mut p = create("Dashboard", None);
        p.system_prompt = Some("You analyze data.".into());
        p.extensions = vec!["autovisualiser".into()];
        let res = s.create_app_inner(p, None).await.unwrap();
        assert!(has_ui_resource(&res));
        assert!(s.store().read_file("dashboard", "src/main.ts").is_ok());
        assert!(s.store().read_file("dashboard", "src/sdk.ts").is_ok());
        let html = s.store().read_file("dashboard", "index.html").unwrap();
        assert!(html.contains("br-container"));
        let m = s.store().load_manifest("dashboard").unwrap();
        assert_eq!(m.kind, ArtifactKind::Agentic);
        let agent = m.agent.unwrap();
        // Provider-agnostic: no model pinned unless explicitly chosen → inherits
        // the user's configured provider/model.
        assert!(agent.model.is_none());
        assert_eq!(agent.extensions, vec!["autovisualiser".to_string()]);
        assert_eq!(agent.system_prompt, "You analyze data.");
    }

    #[tokio::test]
    async fn create_app_records_session_id_when_present() {
        let (_d, s) = server();
        // Agentic app carries the session id through the agent-config save path.
        s.create_app_inner(create("Sessioned", None), Some("sess-123".into()))
            .await
            .unwrap();
        let m = s.store().load_manifest("sessioned").unwrap();
        assert_eq!(m.session_id.as_deref(), Some("sess-123"));

        // Static app gets the id too (re-saved after the initial create).
        s.create_app_inner(create("StaticOne", Some("static")), Some("sess-999".into()))
            .await
            .unwrap();
        let sm = s.store().load_manifest("staticone").unwrap();
        assert_eq!(sm.session_id.as_deref(), Some("sess-999"));

        // No session meta (headless/CLI) leaves it unset.
        s.create_app_inner(create("NoSession", None), None)
            .await
            .unwrap();
        assert_eq!(
            s.store().load_manifest("nosession").unwrap().session_id,
            None
        );
    }

    #[tokio::test]
    async fn configure_app_sets_model_and_extensions() {
        let (_d, s) = server();
        s.create_app_inner(create("Cfg", Some("static")), None)
            .await
            .unwrap();
        s.configure_app(Parameters(ConfigureAppParams {
            id: "cfg".into(),
            system_prompt: Some("Be terse.".into()),
            greeting: None,
            model: Some(ModelParam {
                provider: Some("anthropic".into()),
                model: Some("claude-opus-4-8".into()),
            }),
            extensions: Some(vec!["developer".into(), "knowledge".into()]),
            skills: Some(vec!["scientific-research".into()]),
            knowledge_base: Some("my-kb".into()),
            max_turns: Some(40),
        }))
        .await
        .unwrap();
        let m = s.store().load_manifest("cfg").unwrap();
        assert_eq!(m.kind, ArtifactKind::Agentic);
        let a = m.agent.unwrap();
        assert_eq!(a.model.unwrap().provider.unwrap(), "anthropic");
        assert_eq!(a.extensions, vec!["developer", "knowledge"]);
        assert_eq!(a.skills, vec!["scientific-research"]);
        assert_eq!(a.knowledge_base.unwrap(), "my-kb");
        assert_eq!(a.max_turns, Some(40));
    }

    #[tokio::test]
    async fn creates_static_and_agentic_kinds_with_expected_defaults() {
        let (_d, s) = server();
        s.create_app_inner(create("Plain Widget", Some("static")), None)
            .await
            .unwrap();
        s.create_app_inner(create("Agent Workspace", Some("agentic")), None)
            .await
            .unwrap();

        let static_manifest = s.store().load_manifest("plain-widget").unwrap();
        assert_eq!(static_manifest.kind, ArtifactKind::Static);
        assert!(static_manifest.agent.is_none());
        assert!(!s.store().file_exists("plain-widget", "src/main.ts"));

        let agentic_manifest = s.store().load_manifest("agent-workspace").unwrap();
        assert_eq!(agentic_manifest.kind, ArtifactKind::Agentic);
        assert!(agentic_manifest.agent.is_some());
        assert!(s.store().file_exists("agent-workspace", "src/main.ts"));
        assert!(s.store().file_exists("agent-workspace", "src/sdk.ts"));
    }

    #[tokio::test]
    async fn custom_layout_and_workflow_prompt_build_and_pass_launch_harness() {
        let (_d, s) = server();
        let mut p = create("Cohort Review Console", None);
        p.id = Some("cohort-review-console".into());
        p.description = "Control-driven clinical cohort review workflow".into();
        p.extensions = vec!["knowledge".into(), "developer".into()];
        p.skills = vec!["clinical-biostatistics".into()];
        p.knowledge_base = Some("trial-kb".into());
        p.system_prompt = Some(
            "Follow this workflow: 1. inspect the selected cohort; 2. search the knowledge base; \
             3. delegate statistical checks to the stats sub-agent when appropriate; 4. if context \
             usage is above 80%, summarize before continuing; 5. never download external assets."
                .into(),
        );
        p.html = Some(
            r#"<html>
              <head><title>Cohort Review Console</title></head>
              <body>
                <main class="br-container">
                  <section class="br-card">
                    <label class="br-label" for="assay">Assay</label>
                    <select id="assay" class="br-select">
                      <option>single-cell RNA-seq</option>
                      <option>flow cytometry</option>
                    </select>
                    <label class="br-label" for="cohorts">Cohorts to compare</label>
                    <input id="cohorts" class="br-input" value="responders vs non-responders" />
                    <button id="run" class="br-btn">Run workflow</button>
                  </section>
                  <section id="out" class="br-output" data-placeholder="Workflow output"></section>
                </main>
              </body>
            </html>"#
                .into(),
        );
        p.files = vec![FileSpec {
            path: "src/main.ts".into(),
            content: r##"import { createApp } from "./sdk";

const br = createApp({ autoChat: false });
const assay = document.getElementById("assay") as HTMLSelectElement;
const cohorts = document.getElementById("cohorts") as HTMLInputElement;
const run = document.getElementById("run") as HTMLButtonElement;

run.addEventListener("click", async () => {
  const ctx = await br.context.tokens();
  const contextPlan =
    ctx.ratio > 0.8
      ? "First compact/summarize the working context before continuing."
      : "Continue without compaction unless the context grows past 80%.";
  await br.run(
    `Run the cohort review workflow for ${cohorts.value} using ${assay.value}. ${contextPlan} Use the configured tools and sub-agents when useful, and do not download external assets.`,
    "#out"
  );
});
"##
            .into(),
        }];

        s.create_app_inner(p, None).await.unwrap();
        s.configure_app(Parameters(ConfigureAppParams {
            id: "cohort-review-console".into(),
            system_prompt: None,
            greeting: Some("Choose a cohort and run the review.".into()),
            model: None,
            extensions: None,
            skills: None,
            knowledge_base: None,
            max_turns: Some(72),
        }))
        .await
        .unwrap();

        let build = s
            .build_app(Parameters(AppIdParams {
                id: "cohort-review-console".into(),
            }))
            .await
            .unwrap();
        let build_text = text_of(&build);
        assert!(build_text.contains("dist/app.js"));
        assert!(build_text.contains("passes all guardrails"));

        let launch = s
            .launch_app(Parameters(AppIdParams {
                id: "cohort-review-console".into(),
            }))
            .await
            .unwrap();
        assert!(text_of(&launch).contains("/apps/cohort-review-console/"));

        let manifest = s.store().load_manifest("cohort-review-console").unwrap();
        let agent = manifest.agent.unwrap();
        assert_eq!(agent.max_turns, Some(72));
        assert_eq!(agent.extensions, vec!["knowledge", "developer"]);
        assert_eq!(agent.skills, vec!["clinical-biostatistics"]);
        assert_eq!(agent.knowledge_base.as_deref(), Some("trial-kb"));
        assert!(agent.system_prompt.contains("above 80%"));
    }

    #[tokio::test]
    async fn manifest_update_supports_advanced_workflow_and_security_config() {
        use crate::agent_drafter::manifest::{
            Capabilities, ComputeCapability, FilesCapability, GuardrailsConfig, PiiMode,
            ReliabilityConfig, SubAgentManifest, WorkflowManifest, WorkflowStep,
        };
        use std::collections::HashMap;

        let (_d, s) = server();
        s.create_app_inner(create("Advanced Agent", None), None)
            .await
            .unwrap();
        let mut manifest = s.store().load_manifest("advanced-agent").unwrap();
        let agent = manifest.agent.as_mut().unwrap();

        let mut capabilities = Capabilities::default();
        capabilities.files = Some(FilesCapability {
            entries: Vec::new(),
            max_file_bytes: Some(256 * 1024),
        });
        capabilities.compute = Some(ComputeCapability {
            sandbox: "docker".into(),
            timeout_s: 45,
            network: "none".into(),
            max_mem: Some("512m".into()),
            cpus: Some(1.0),
            image: None,
        });
        capabilities.events = vec!["tool".into(), "compaction".into(), "handoff".into()];
        agent.capabilities = capabilities;
        agent.guardrails = Some(GuardrailsConfig {
            goal: Some("finish with a cited risk summary".into()),
            business_scope: Some("clinical research workflow drafting".into()),
            pii: PiiMode::Block,
            needs_approval: vec!["compute__exec".into(), "developer__shell".into()],
            approvals_require_persistence: true,
            ..Default::default()
        });
        agent.reliability = Some(ReliabilityConfig {
            tool_timeout_s: Some(30),
            parallel_tools: true,
            ..Default::default()
        });
        agent.output_type = Some(serde_json::json!({
            "type": "object",
            "required": ["summary", "next_steps"],
            "properties": {
                "summary": { "type": "string" },
                "next_steps": { "type": "array", "items": { "type": "string" } }
            }
        }));

        let mut sub_agents = HashMap::new();
        sub_agents.insert(
            "stats".into(),
            SubAgentManifest {
                description: "Biostatistics specialist".into(),
                system_prompt: "Check statistical assumptions and effect sizes.".into(),
                skills: vec!["clinical-biostatistics".into()],
                max_steps: Some(8),
                max_wall_s: Some(120),
                ..Default::default()
            },
        );
        let mut workflows = HashMap::new();
        workflows.insert(
            "triage".into(),
            WorkflowManifest {
                steps: vec![
                    WorkflowStep::Agent {
                        agent: "stats".into(),
                        input_template: "{{cohort_summary}}".into(),
                        guardrail: None,
                        on_error: "abort".into(),
                    },
                    WorkflowStep::Tool {
                        tool: "knowledge__query".into(),
                        args_template: serde_json::json!({ "q": "{{finding}}" }),
                        guardrail: Some(serde_json::json!({ "kind": "pii", "mode": "block" })),
                        on_error: "continue".into(),
                    },
                ],
            },
        );
        agent.orchestration.sub_agents = sub_agents;
        agent.orchestration.workflows = workflows;

        s.update_app(Parameters(UpdateAppParams {
            id: "advanced-agent".into(),
            path: Some("manifest.json".into()),
            content: Some(serde_json::to_string_pretty(&manifest).unwrap()),
            old_str: None,
            new_str: None,
        }))
        .await
        .unwrap();

        let read = s
            .read_app(Parameters(ReadAppParams {
                id: "advanced-agent".into(),
                path: None,
            }))
            .await
            .unwrap();
        let roundtrip: Manifest = serde_json::from_str(&text_of(&read)).unwrap();
        let cfg = roundtrip.agent.unwrap();
        assert_eq!(cfg.capabilities.compute.as_ref().unwrap().network, "none");
        assert!(cfg.capabilities.advertised().contains(&"files".to_string()));
        assert!(cfg
            .capabilities
            .advertised()
            .contains(&"compute".to_string()));
        assert!(cfg
            .capabilities
            .advertised()
            .contains(&"event:compaction".to_string()));
        assert_eq!(cfg.guardrails.as_ref().unwrap().pii, PiiMode::Block);
        assert!(cfg
            .guardrails
            .as_ref()
            .unwrap()
            .needs_approval
            .contains(&"compute__exec".to_string()));
        assert!(cfg.reliability.as_ref().unwrap().parallel_tools);
        assert!(cfg.orchestration.sub_agents.contains_key("stats"));
        assert!(cfg.orchestration.workflows.contains_key("triage"));
        assert!(cfg.output_type.is_some());
    }

    #[tokio::test]
    async fn manifest_update_rejects_invalid_json_and_id_mismatch() {
        let (_d, s) = server();
        s.create_app_inner(create("Manifest Safe", None), None)
            .await
            .unwrap();
        let original = s.store().load_manifest("manifest-safe").unwrap();

        assert!(s
            .update_app(Parameters(UpdateAppParams {
                id: "manifest-safe".into(),
                path: Some("manifest.json".into()),
                content: Some("{ not json".into()),
                old_str: None,
                new_str: None,
            }))
            .await
            .is_err());

        let mut wrong_id = original.clone();
        wrong_id.id = "other-app".into();
        assert!(s
            .update_app(Parameters(UpdateAppParams {
                id: "manifest-safe".into(),
                path: Some("manifest.json".into()),
                content: Some(serde_json::to_string_pretty(&wrong_id).unwrap()),
                old_str: None,
                new_str: None,
            }))
            .await
            .is_err());

        let still_valid = s.store().load_manifest("manifest-safe").unwrap();
        assert_eq!(still_valid.id, original.id);
        assert_eq!(still_valid.title, original.title);
    }

    #[tokio::test]
    async fn harness_errors_block_launch_and_export() {
        let (_d, s) = server();
        let mut p = create("Broken Harness", None);
        p.html = Some(
            r#"<html><body><main class="br-container"><button id="go" class="br-btn">Run</button></main></body></html>"#
                .into(),
        );
        p.files = vec![FileSpec {
            path: "src/main.ts".into(),
            content: r##"import { createApp } from "./sdk";
const br = createApp({ autoChat: false });
br.run("hello", "#missing");
"##
            .into(),
        }];
        s.create_app_inner(p, None).await.unwrap();

        let build = s
            .build_app(Parameters(AppIdParams {
                id: "broken-harness".into(),
            }))
            .await
            .unwrap();
        let build_text = text_of(&build);
        assert!(build_text.contains("ERROR"));
        assert!(build_text.contains("#missing"));

        assert!(s
            .launch_app(Parameters(AppIdParams {
                id: "broken-harness".into(),
            }))
            .await
            .is_err());

        let out = TempDir::new().unwrap();
        assert!(s
            .export_app(Parameters(ExportAppParams {
                id: "broken-harness".into(),
                target_dir: out.path().to_string_lossy().to_string(),
                endpoint: None,
            }))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn create_rejects_empty_title_and_bad_kind() {
        let (_d, s) = server();
        assert!(s.create_app_inner(create("  ", None), None).await.is_err());
        assert!(s
            .create_app_inner(create("X", Some("bogus")), None)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn update_marks_bundle_stale_for_src() {
        let (_d, s) = server();
        let mut p = create("Edit Me", None);
        p.html = Some("<html><body>ORIGINAL</body></html>".into());
        s.create_app_inner(p, None).await.unwrap();

        s.update_app(Parameters(UpdateAppParams {
            id: "edit-me".into(),
            path: Some("src/main.ts".into()),
            content: Some("import './sdk'; console.log(1);".into()),
            old_str: None,
            new_str: None,
        }))
        .await
        .unwrap();
        assert_eq!(s.store().load_manifest("edit-me").unwrap().built_at, None);

        let res = s
            .update_app(Parameters(UpdateAppParams {
                id: "edit-me".into(),
                path: None,
                content: None,
                old_str: Some("ORIGINAL".into()),
                new_str: Some("CHANGED".into()),
            }))
            .await
            .unwrap();
        assert!(has_ui_resource(&res));
        assert!(s
            .store()
            .read_file("edit-me", "index.html")
            .unwrap()
            .contains("CHANGED"));
    }

    #[tokio::test]
    async fn build_then_launch_returns_url() {
        let (_d, s) = server();
        s.create_app_inner(create("Launchy", None), None)
            .await
            .unwrap();
        let res = s
            .build_app(Parameters(AppIdParams {
                id: "launchy".into(),
            }))
            .await
            .unwrap();
        assert!(text_of(&res).contains("dist/app.js"));
        assert!(s.store().file_exists("launchy", "dist/app.js"));
        assert!(s
            .store()
            .load_manifest("launchy")
            .unwrap()
            .built_at
            .is_some());

        let res = s
            .launch_app(Parameters(AppIdParams {
                id: "launchy".into(),
            }))
            .await
            .unwrap();
        assert!(text_of(&res).contains("/apps/launchy/"));
    }

    #[tokio::test]
    async fn list_read_delete() {
        let (_d, s) = server();
        s.create_app_inner(create("One", None), None).await.unwrap();
        let all = s
            .list_apps(Parameters(ListAppsParams { kind: None }))
            .await
            .unwrap();
        assert!(text_of(&all).contains("one"));

        let m = s
            .read_app(Parameters(ReadAppParams {
                id: "one".into(),
                path: None,
            }))
            .await
            .unwrap();
        assert!(text_of(&m).contains("\"title\": \"One\""));

        s.delete_app(Parameters(AppIdParams { id: "one".into() }))
            .await
            .unwrap();
        assert!(!s.store().exists("one"));
    }

    #[tokio::test]
    async fn export_writes_standalone_ts_project() {
        let (_d, s) = server();
        let mut p = create("Exporter", None);
        p.html = Some("<html><head></head><body>hi</body></html>".into());
        p.system_prompt = Some("help".into());
        s.create_app_inner(p, None).await.unwrap();

        let out = TempDir::new().unwrap();
        let res = s
            .export_app(Parameters(ExportAppParams {
                id: "exporter".into(),
                target_dir: out.path().to_string_lossy().to_string(),
                endpoint: None,
            }))
            .await
            .unwrap();
        assert!(text_of(&res).contains("standalone"));
        assert!(out.path().join("index.html").exists());
        assert!(out.path().join("package.json").exists());
        assert!(out.path().join("serve.mjs").exists());
        assert!(out.path().join("src/main.ts").exists());
        assert!(out.path().join("src/sdk.ts").exists());
        let index = std::fs::read_to_string(out.path().join("index.html")).unwrap();
        assert!(index.contains("dist/app.js"));
        assert!(index.contains("BIOROUTER_APP_CONFIG"));
    }

    #[tokio::test]
    async fn export_rejects_missing_app() {
        let (_d, s) = server();
        assert!(s
            .export_app(Parameters(ExportAppParams {
                id: "ghost".into(),
                target_dir: "/tmp/x".into(),
                endpoint: None,
            }))
            .await
            .is_err());
    }
}
