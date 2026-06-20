//! Agent Drafter — author interactive artifacts, optionally wired to a live
//! BioRouter agent over ACP.
//!
//! Agent Drafter is BioRouter's answer to "Claude artifacts", but the artifacts
//! it produces can embed *real* AI-agent capability. It exposes MCP tools that
//! let the assistant create, edit, preview, and export self-contained artifacts
//! with a consistent tech stack and a BioRouter-flavored design system:
//!
//!   - **Static** artifacts: plain interactive HTML/CSS/JS pages.
//!   - **Agentic** artifacts: the above, plus an embedded agent runtime that
//!     talks to a BioRouter agent via the Agent Client Protocol (ACP) — or, when
//!     previewed inside BioRouter, via the sandboxed MCP-App bridge.
//!
//! In-app previews are returned as `ui://` HTML resources (rendered in the
//! desktop's sandboxed iframe). `export_artifact` scaffolds a standalone,
//! runnable project (Tauri, bundling the BioRouter CLI as a sidecar — or a plain
//! static web build).

pub mod render;
pub mod store;

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
    tool, tool_handler, tool_router, ServerHandler,
};
use serde::Deserialize;
use std::path::{Path, PathBuf};

use store::{AgentConfig, ArtifactKind, ArtifactStore, Manifest};

// ---------------------------------------------------------------------------
// Tool parameter structs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FileSpec {
    /// Path relative to the artifact root (e.g. "css/app.css").
    pub path: String,
    /// File contents.
    pub content: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateArtifactParams {
    /// Human-readable title; also used to derive the artifact id.
    pub title: String,
    /// Short description of what the artifact does.
    #[serde(default)]
    pub description: String,
    /// "static" (default) or "agentic" (embeds a BioRouter agent).
    #[serde(default)]
    pub kind: Option<String>,
    /// Entry HTML for the artifact. If omitted, a BioRouter-styled starter is used.
    #[serde(default)]
    pub html: Option<String>,
    /// Additional files to write alongside the entry HTML.
    #[serde(default)]
    pub files: Vec<FileSpec>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetArtifactSizeParams {
    /// Artifact id.
    pub id: String,
    /// Preferred preview width in CSS px. Omit/null to fill the panel.
    #[serde(default)]
    pub width: Option<u32>,
    /// Preferred preview height in CSS px. Omit/null for the auto-growing default.
    #[serde(default)]
    pub height: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateArtifactParams {
    /// Artifact id.
    pub id: String,
    /// File to modify (defaults to the artifact's entry HTML).
    #[serde(default)]
    pub path: Option<String>,
    /// Full new contents for the file (write mode).
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
pub struct ListArtifactsParams {
    /// Optional filter: "static" or "agentic".
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadArtifactParams {
    /// Artifact id.
    pub id: String,
    /// File to read. If omitted, returns the manifest.
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ArtifactIdParams {
    /// Artifact id.
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddAgentCapabilityParams {
    /// Artifact id.
    pub id: String,
    /// System prompt defining the embedded agent's behavior.
    pub system_prompt: String,
    /// Optional greeting shown when the chat panel mounts.
    #[serde(default)]
    pub greeting: Option<String>,
    /// Tool / MCP-extension names the embedded agent may use.
    #[serde(default)]
    pub tools: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExportArtifactParams {
    /// Artifact id.
    pub id: String,
    /// Destination directory (created if missing).
    pub target_dir: String,
    /// "tauri" (default, bundles the BioRouter agent) or "web".
    #[serde(default)]
    pub runtime: Option<String>,
    /// Override the ACP WebSocket endpoint the exported artifact connects to.
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

fn default_root() -> PathBuf {
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

/// Recursively collect an artifact's files (relative path → contents),
/// skipping `manifest.json`.
fn collect_files(base: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
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

#[tool_router(router = tool_router)]
impl AgentDrafterServer {
    pub fn new() -> Self {
        Self::with_root(default_root())
    }

    pub fn with_root(root: PathBuf) -> Self {
        let instructions = formatdoc! {r#"
            Agent Drafter lets you build interactive *artifacts* for the user —
            self-contained HTML/CSS/JS apps — that can optionally embed a live
            BioRouter agent. Think "Claude artifacts", but the artifacts can carry
            real AI-agent capability powered by the Agent Client Protocol (ACP).

            Two kinds of artifact:
            - "static": a plain interactive page (dashboards, tools, visualizations,
              forms). No agent.
            - "agentic": a page with an embedded agent runtime + chat panel that
              talks to a BioRouter agent. Use this when the artifact should reason,
              call tools, or hold a conversation.

            Tech stack & conventions (keep artifacts consistent):
            - One entry file, "index.html", plus optional CSS/JS files.
            - The BioRouter design system is injected automatically at preview/export
              time and mirrors the app's own look (warm neutral palette, white
              cards with a soft shadow, a restrained near-black accent, thin
              borders, 6px/12px radii, system font). ALWAYS compose with the
              provided classes — `br-container` (page wrapper), `br-card`,
              `br-btn` (+ `br-btn--secondary`, `br-btn--ghost`), `br-input`,
              `br-textarea`, `br-label`, `br-field`, `br-row`, `br-badge`,
              `br-chat` — and the CSS variables (`var(--br-text)`,
              `var(--br-text-muted)`, `var(--br-accent)`, `var(--br-border)`,
              etc.). Do NOT paste your own colors, fonts, or a <style> theme and
              do NOT pull in external CSS frameworks — rely on the system so the
              artifact looks native to BioRouter, not generic.
            - Sizing: previews fill the panel width and auto-grow in height. For
              wide layouts (dashboards, side-by-side panels) call
              `set_artifact_size` with a larger width like 1000 so the user gets
              room to work; omit a dimension to fill/auto-grow.
            - For agentic artifacts you do not need to write any networking code:
              call `add_agent_capability` and the runtime (`window.BioRouterAgent`)
              plus a chat panel are wired in for you. To place the chat yourself,
              add an element with the `data-br-chat` attribute.
            - Exported artifacts default to a Tauri desktop bundle that ships the
              BioRouter CLI as a sidecar (fully self-contained), or a plain web build.

            Typical workflow:
            1. `create_artifact` with a title, description, kind, and (optionally) your
               own HTML. A preview is returned and shown to the user.
            2. Iterate with `update_artifact` (full `content` write, or `old_str`/
               `new_str` replace). Call `preview_artifact` to re-render.
            3. For agentic artifacts, `add_agent_capability` with a system prompt
               (and optionally a greeting and the tools the agent may use).
            4. `export_artifact` to produce a standalone, runnable project.

            Use `list_artifacts` and `read_artifact` to inspect existing work.
            Always confirm the artifact looks right via a preview before exporting.
        "#};
        Self {
            tool_router: Self::tool_router(),
            instructions,
            root,
        }
    }

    fn store(&self) -> ArtifactStore {
        ArtifactStore::new(self.root.clone())
    }

    /// Build the two-part preview result: a `ui://` HTML resource for the user and
    /// a short assistant-audience text confirmation.
    fn preview_result(&self, manifest: &Manifest, note: &str) -> Result<CallToolResult, ErrorData> {
        let store = self.store();
        let entry_html = store
            .read_file(&manifest.id, &manifest.entry)
            .map_err(|e| err(ErrorCode::INTERNAL_ERROR, format!("read entry: {e}")))?;
        let html = render::assemble_preview(manifest, &entry_html);
        let blob = STANDARD.encode(html.as_bytes());

        // Tell the host (MCP-UI) the preferred frame size. Width fills the panel
        // unless the agent pinned one; height defaults comfortably and then
        // auto-grows (the runtime posts `ui-size-change`).
        let width_css = manifest
            .width
            .map(|w| format!("{w}px"))
            .unwrap_or_else(|| "100%".to_string());
        let height_css = manifest
            .height
            .map(|h| format!("{h}px"))
            .unwrap_or_else(|| "560px".to_string());
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
        name = "create_artifact",
        description = "Create a new interactive artifact (static HTML/JS, or agentic with an embedded BioRouter agent). Returns a live preview."
    )]
    pub async fn create_artifact(
        &self,
        params: Parameters<CreateArtifactParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
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
            None => ArtifactKind::Static,
        };
        let entry = "index.html";
        let entry_html = p
            .html
            .unwrap_or_else(|| render::starter(&p.title, &p.description));

        let mut files = vec![(entry.to_string(), entry_html)];
        for f in p.files {
            if f.path != entry {
                files.push((f.path, f.content));
            }
        }

        let store = self.store();
        let manifest = store
            .create(&p.title, &p.description, kind, entry, &files)
            .map_err(internal)?;

        self.preview_result(
            &manifest,
            &format!(
                "Created {kind:?} artifact '{}' (id: {}). Preview shown to the user.",
                manifest.title, manifest.id
            ),
        )
    }

    #[tool(
        name = "set_artifact_size",
        description = "Set an artifact's preferred preview size in CSS px (width/height). Use a larger size for dashboards or wide layouts; omit a value to fill the panel / auto-grow."
    )]
    pub async fn set_artifact_size(
        &self,
        params: Parameters<SetArtifactSizeParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        let store = self.store();
        let mut manifest = store
            .load_manifest(&p.id)
            .map_err(|_| err(ErrorCode::INVALID_PARAMS, format!("no artifact '{}'", p.id)))?;
        manifest.width = p.width;
        manifest.height = p.height;
        store.save_manifest(&manifest).map_err(internal)?;
        store.touch(&p.id).map_err(internal)?;
        self.preview_result(
            &manifest,
            &format!(
                "Set preview size for '{}' (width: {:?}, height: {:?}).",
                p.id, p.width, p.height
            ),
        )
    }

    #[tool(
        name = "update_artifact",
        description = "Edit a file in an artifact: provide full `content` to overwrite, or `old_str`+`new_str` to replace a snippet. Defaults to the entry HTML."
    )]
    pub async fn update_artifact(
        &self,
        params: Parameters<UpdateArtifactParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        let store = self.store();
        let manifest = store
            .load_manifest(&p.id)
            .map_err(|_| err(ErrorCode::INVALID_PARAMS, format!("no artifact '{}'", p.id)))?;
        let path = p.path.clone().unwrap_or_else(|| manifest.entry.clone());

        if let Some(content) = p.content {
            store.write_file(&p.id, &path, &content).map_err(internal)?;
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
            let updated = current.replacen(old.as_str(), new.as_str(), 1);
            store.write_file(&p.id, &path, &updated).map_err(internal)?;
        } else {
            return Err(err(
                ErrorCode::INVALID_PARAMS,
                "provide either `content` or both `old_str` and `new_str`",
            ));
        }
        store.touch(&p.id).map_err(internal)?;

        if path == manifest.entry {
            self.preview_result(&manifest, &format!("Updated {path} in '{}'.", p.id))
        } else {
            Ok(CallToolResult::success(vec![Content::text(format!(
                "Updated {path} in '{}'.",
                p.id
            ))]))
        }
    }

    #[tool(
        name = "list_artifacts",
        description = "List all artifacts (optionally filtered by kind)."
    )]
    pub async fn list_artifacts(
        &self,
        params: Parameters<ListArtifactsParams>,
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
                "No artifacts yet.".to_string(),
            )]));
        }
        let lines: Vec<String> = list
            .iter()
            .map(|m| format!("- {} [{:?}] — {}", m.id, m.kind, m.title))
            .collect();
        Ok(CallToolResult::success(vec![Content::text(
            lines.join("\n"),
        )]))
    }

    #[tool(
        name = "read_artifact",
        description = "Read an artifact's manifest, or a specific file within it."
    )]
    pub async fn read_artifact(
        &self,
        params: Parameters<ReadArtifactParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        let store = self.store();
        match p.path {
            None => {
                let m = store.load_manifest(&p.id).map_err(|_| {
                    err(ErrorCode::INVALID_PARAMS, format!("no artifact '{}'", p.id))
                })?;
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
        name = "preview_artifact",
        description = "Render an artifact and return a live preview for the user."
    )]
    pub async fn preview_artifact(
        &self,
        params: Parameters<ArtifactIdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        let manifest = self
            .store()
            .load_manifest(&p.id)
            .map_err(|_| err(ErrorCode::INVALID_PARAMS, format!("no artifact '{}'", p.id)))?;
        self.preview_result(&manifest, &format!("Preview of '{}'.", p.id))
    }

    #[tool(
        name = "add_agent_capability",
        description = "Turn an artifact into an agentic artifact: embed a BioRouter agent (system prompt, optional greeting, allowed tools) and a chat panel."
    )]
    pub async fn add_agent_capability(
        &self,
        params: Parameters<AddAgentCapabilityParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        if p.system_prompt.trim().is_empty() {
            return Err(err(
                ErrorCode::INVALID_PARAMS,
                "system_prompt must not be empty",
            ));
        }
        let store = self.store();
        let mut manifest = store
            .load_manifest(&p.id)
            .map_err(|_| err(ErrorCode::INVALID_PARAMS, format!("no artifact '{}'", p.id)))?;
        manifest.kind = ArtifactKind::Agentic;
        manifest.agent = Some(AgentConfig {
            system_prompt: p.system_prompt,
            greeting: p.greeting,
            tools: p.tools,
        });
        store.save_manifest(&manifest).map_err(internal)?;
        store.touch(&p.id).map_err(internal)?;
        self.preview_result(
            &manifest,
            &format!("'{}' is now an agentic artifact. Preview shown.", p.id),
        )
    }

    #[tool(
        name = "export_artifact",
        description = "Scaffold a standalone, runnable project from an artifact (Tauri desktop bundle with the BioRouter agent sidecar, or a static web build)."
    )]
    pub async fn export_artifact(
        &self,
        params: Parameters<ExportArtifactParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        let runtime = p.runtime.as_deref().unwrap_or("tauri").to_lowercase();
        if runtime != "tauri" && runtime != "web" {
            return Err(err(
                ErrorCode::INVALID_PARAMS,
                "runtime must be 'tauri' or 'web'",
            ));
        }
        let store = self.store();
        let manifest = store
            .load_manifest(&p.id)
            .map_err(|_| err(ErrorCode::INVALID_PARAMS, format!("no artifact '{}'", p.id)))?;

        // Gather the artifact's files.
        let artifact_dir = store.root().join(&manifest.id);
        let mut all_files = Vec::new();
        collect_files(&artifact_dir, &artifact_dir, &mut all_files);
        let entry_html = all_files
            .iter()
            .find(|(path, _)| path == &manifest.entry)
            .map(|(_, c)| c.clone())
            .ok_or_else(|| err(ErrorCode::INTERNAL_ERROR, "entry file missing"))?;
        let extras: Vec<(String, String)> = all_files
            .into_iter()
            .filter(|(path, _)| path != &manifest.entry)
            .collect();

        let scaffold = if runtime == "tauri" {
            render::scaffold_tauri(&manifest, &entry_html, &extras, p.endpoint.as_deref())
        } else {
            render::scaffold_web(&manifest, &entry_html, &extras, p.endpoint.as_deref())
        };

        let target = PathBuf::from(&p.target_dir);
        for (rel, content) in &scaffold {
            let full = target.join(rel);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).map_err(internal)?;
            }
            std::fs::write(&full, content).map_err(internal)?;
        }

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Exported '{}' as a {} project to {} ({} files). See README.md for run instructions.",
            manifest.id,
            runtime,
            target.display(),
            scaffold.len()
        ))]))
    }

    #[tool(
        name = "delete_artifact",
        description = "Delete an artifact and all of its files."
    )]
    pub async fn delete_artifact(
        &self,
        params: Parameters<ArtifactIdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        let store = self.store();
        if !store.exists(&p.id) {
            return Err(err(
                ErrorCode::INVALID_PARAMS,
                format!("no artifact '{}'", p.id),
            ));
        }
        store.delete(&p.id).map_err(internal)?;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Deleted artifact '{}'.",
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

    #[tokio::test]
    async fn create_returns_preview_and_persists() {
        let (_d, s) = server();
        let res = s
            .create_artifact(Parameters(CreateArtifactParams {
                title: "Dashboard".into(),
                description: "a dash".into(),
                kind: None,
                html: None,
                files: vec![],
            }))
            .await
            .unwrap();
        assert!(has_ui_resource(&res));
        assert!(text_of(&res).contains("dashboard"));
        // Persisted entry uses the BioRouter starter (design-system classes).
        let html = s.store().read_file("dashboard", "index.html").unwrap();
        assert!(html.contains("br-container"));
    }

    #[tokio::test]
    async fn set_artifact_size_persists_and_previews() {
        let (_d, s) = server();
        s.create_artifact(Parameters(CreateArtifactParams {
            title: "Wide".into(),
            description: String::new(),
            kind: Some("agentic".into()),
            html: None,
            files: vec![],
        }))
        .await
        .unwrap();
        let res = s
            .set_artifact_size(Parameters(SetArtifactSizeParams {
                id: "wide".into(),
                width: Some(1000),
                height: None,
            }))
            .await
            .unwrap();
        assert!(has_ui_resource(&res));
        let m = s.store().load_manifest("wide").unwrap();
        assert_eq!(m.width, Some(1000));
        assert_eq!(m.height, None);
        assert!(s
            .set_artifact_size(Parameters(SetArtifactSizeParams {
                id: "nope".into(),
                width: Some(1),
                height: None,
            }))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn create_rejects_empty_title_and_bad_kind() {
        let (_d, s) = server();
        assert!(s
            .create_artifact(Parameters(CreateArtifactParams {
                title: "  ".into(),
                description: String::new(),
                kind: None,
                html: None,
                files: vec![],
            }))
            .await
            .is_err());
        assert!(s
            .create_artifact(Parameters(CreateArtifactParams {
                title: "X".into(),
                description: String::new(),
                kind: Some("bogus".into()),
                html: None,
                files: vec![],
            }))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn update_write_and_str_replace() {
        let (_d, s) = server();
        s.create_artifact(Parameters(CreateArtifactParams {
            title: "Edit Me".into(),
            description: String::new(),
            kind: None,
            html: Some("<html><body>ORIGINAL</body></html>".into()),
            files: vec![],
        }))
        .await
        .unwrap();

        // str-replace
        s.update_artifact(Parameters(UpdateArtifactParams {
            id: "edit-me".into(),
            path: None,
            content: None,
            old_str: Some("ORIGINAL".into()),
            new_str: Some("CHANGED".into()),
        }))
        .await
        .unwrap();
        assert!(s
            .store()
            .read_file("edit-me", "index.html")
            .unwrap()
            .contains("CHANGED"));

        // missing old_str errors
        assert!(s
            .update_artifact(Parameters(UpdateArtifactParams {
                id: "edit-me".into(),
                path: None,
                content: None,
                old_str: Some("NOPE".into()),
                new_str: Some("x".into()),
            }))
            .await
            .is_err());

        // full content write to a new file
        s.update_artifact(Parameters(UpdateArtifactParams {
            id: "edit-me".into(),
            path: Some("css/app.css".into()),
            content: Some("body{color:red}".into()),
            old_str: None,
            new_str: None,
        }))
        .await
        .unwrap();
        assert_eq!(
            s.store().read_file("edit-me", "css/app.css").unwrap(),
            "body{color:red}"
        );

        // neither mode → error
        assert!(s
            .update_artifact(Parameters(UpdateArtifactParams {
                id: "edit-me".into(),
                path: None,
                content: None,
                old_str: None,
                new_str: None,
            }))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn add_agent_capability_makes_artifact_agentic() {
        let (_d, s) = server();
        s.create_artifact(Parameters(CreateArtifactParams {
            title: "Helper".into(),
            description: String::new(),
            kind: None,
            html: Some("<html><head></head><body></body></html>".into()),
            files: vec![],
        }))
        .await
        .unwrap();

        let res = s
            .add_agent_capability(Parameters(AddAgentCapabilityParams {
                id: "helper".into(),
                system_prompt: "You are a research assistant.".into(),
                greeting: Some("How can I help?".into()),
                tools: vec!["developer".into()],
            }))
            .await
            .unwrap();
        assert!(has_ui_resource(&res));

        let m = s.store().load_manifest("helper").unwrap();
        assert_eq!(m.kind, ArtifactKind::Agentic);
        let agent = m.agent.unwrap();
        assert_eq!(agent.system_prompt, "You are a research assistant.");
        assert_eq!(agent.tools, vec!["developer".to_string()]);

        // empty system prompt rejected
        assert!(s
            .add_agent_capability(Parameters(AddAgentCapabilityParams {
                id: "helper".into(),
                system_prompt: "   ".into(),
                greeting: None,
                tools: vec![],
            }))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn list_and_read_and_delete() {
        let (_d, s) = server();
        s.create_artifact(Parameters(CreateArtifactParams {
            title: "One".into(),
            description: "first".into(),
            kind: None,
            html: None,
            files: vec![],
        }))
        .await
        .unwrap();
        s.create_artifact(Parameters(CreateArtifactParams {
            title: "Two".into(),
            description: String::new(),
            kind: Some("agentic".into()),
            html: None,
            files: vec![],
        }))
        .await
        .unwrap();

        let all = s
            .list_artifacts(Parameters(ListArtifactsParams { kind: None }))
            .await
            .unwrap();
        assert!(text_of(&all).contains("one"));
        assert!(text_of(&all).contains("two"));

        let agentic = s
            .list_artifacts(Parameters(ListArtifactsParams {
                kind: Some("agentic".into()),
            }))
            .await
            .unwrap();
        assert!(text_of(&agentic).contains("two"));
        assert!(!text_of(&agentic).contains("one"));

        // read manifest
        let m = s
            .read_artifact(Parameters(ReadArtifactParams {
                id: "one".into(),
                path: None,
            }))
            .await
            .unwrap();
        assert!(text_of(&m).contains("\"title\": \"One\""));

        // read entry file
        let f = s
            .read_artifact(Parameters(ReadArtifactParams {
                id: "one".into(),
                path: Some("index.html".into()),
            }))
            .await
            .unwrap();
        assert!(text_of(&f).contains("br-container"));

        // delete
        s.delete_artifact(Parameters(ArtifactIdParams { id: "one".into() }))
            .await
            .unwrap();
        assert!(!s.store().exists("one"));
    }

    #[tokio::test]
    async fn export_tauri_writes_runnable_project() {
        let (_d, s) = server();
        s.create_artifact(Parameters(CreateArtifactParams {
            title: "Exporter".into(),
            description: String::new(),
            kind: Some("agentic".into()),
            html: Some("<html><head></head><body>hi</body></html>".into()),
            files: vec![FileSpec {
                path: "css/app.css".into(),
                content: "body{}".into(),
            }],
        }))
        .await
        .unwrap();
        s.add_agent_capability(Parameters(AddAgentCapabilityParams {
            id: "exporter".into(),
            system_prompt: "help".into(),
            greeting: None,
            tools: vec![],
        }))
        .await
        .unwrap();

        let out = TempDir::new().unwrap();
        let res = s
            .export_artifact(Parameters(ExportArtifactParams {
                id: "exporter".into(),
                target_dir: out.path().to_string_lossy().to_string(),
                runtime: Some("tauri".into()),
                endpoint: None,
            }))
            .await
            .unwrap();
        assert!(text_of(&res).contains("tauri"));

        assert!(out.path().join("dist/index.html").exists());
        assert!(out.path().join("dist/css/app.css").exists());
        assert!(out.path().join("src-tauri/Cargo.toml").exists());
        assert!(out.path().join("src-tauri/tauri.conf.json").exists());
        assert!(out.path().join("src-tauri/src/main.rs").exists());
        assert!(out.path().join("README.md").exists());

        let index = std::fs::read_to_string(out.path().join("dist/index.html")).unwrap();
        assert!(index.contains("acp-ws"));
        assert!(index.contains("BioRouterAgent"));
    }

    #[tokio::test]
    async fn export_rejects_bad_runtime_and_missing_artifact() {
        let (_d, s) = server();
        assert!(s
            .export_artifact(Parameters(ExportArtifactParams {
                id: "ghost".into(),
                target_dir: "/tmp/x".into(),
                runtime: Some("web".into()),
                endpoint: None,
            }))
            .await
            .is_err());
    }
}
