//! Rendering + scaffolding for Agent Drafter artifacts.
//!
//! - [`assemble_preview`] injects the BioRouter design system (and, for agentic
//!   artifacts, the embedded agent runtime + chat panel) into an artifact's entry
//!   HTML so it can be rendered in BioRouter's sandboxed iframe.
//! - [`scaffold_tauri`] / [`scaffold_web`] turn an artifact into a standalone,
//!   runnable project (Tier B export).

use crate::agent_drafter::store::{AgentConfig, ArtifactKind, Manifest};

pub const THEME_CSS: &str = include_str!("templates/theme.css");
pub const AGENT_JS: &str = include_str!("templates/agent.js");
pub const STARTER_HTML: &str = include_str!("templates/starter.html");

/// Default local endpoint a standalone export uses to reach the bundled
/// `biorouter acp` sidecar (bridged to a WebSocket).
pub const DEFAULT_ACP_WS: &str = "ws://127.0.0.1:11577/acp";

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Build the default entry HTML for a freshly created artifact.
pub fn starter(title: &str, description: &str) -> String {
    STARTER_HTML
        .replace("{{TITLE}}", &html_escape(title))
        .replace("{{DESCRIPTION}}", &html_escape(description))
}

/// Insert `insert` immediately before the first (case-insensitive) `needle`.
/// If `needle` is absent, fall back to `on_missing` (prepend or append).
fn inject_before(html: &str, needle: &str, insert: &str, append_if_missing: bool) -> String {
    if let Some(pos) = html.to_lowercase().find(&needle.to_lowercase()) {
        // `pos` is a byte offset returned by `find`, so it lands on a char
        // boundary and `split_at` is safe.
        let (before, after) = html.split_at(pos);
        let mut out = String::with_capacity(html.len() + insert.len());
        out.push_str(before);
        out.push_str(insert);
        out.push_str(after);
        out
    } else if append_if_missing {
        format!("{html}{insert}")
    } else {
        format!("{insert}{html}")
    }
}

/// The `<script>` that hands the embedded runtime its configuration.
pub fn agent_config_script(agent: &AgentConfig, transport: &str, endpoint: Option<&str>) -> String {
    let cfg = serde_json::json!({
        "transport": transport,
        "endpoint": endpoint,
        "systemPrompt": agent.system_prompt,
        "greeting": agent.greeting,
        "tools": agent.tools,
    });
    let json = serde_json::to_string(&cfg).unwrap_or_else(|_| "{}".to_string());
    // JSON is valid JS; neutralise any `</script>` breakout from string fields.
    let json = json.replace('<', "\\u003c");
    format!("<script>window.BIOROUTER_AGENT_CONFIG = {json};</script>\n")
}

/// Inject the theme into `<head>` and, for agentic artifacts, the agent config +
/// chat panel + runtime before `</body>`. `transport`/`endpoint` select how the
/// embedded agent connects ("bridge" for in-app preview, "acp-ws" for exports).
pub fn assemble(
    manifest: &Manifest,
    entry_html: &str,
    transport: &str,
    endpoint: Option<&str>,
) -> String {
    let style = format!("<style id=\"biorouter-theme\">\n{THEME_CSS}\n</style>\n");
    let mut html = inject_before(entry_html, "</head>", &style, false);

    if manifest.kind == ArtifactKind::Agentic {
        let agent = manifest.agent.clone().unwrap_or_default();
        let mut block = agent_config_script(&agent, transport, endpoint);
        // Auto-mount a chat panel if the author didn't place one themselves.
        if !html.to_lowercase().contains("data-br-chat") {
            block.push_str(
                "<div class=\"br-container\"><div class=\"br-card\" data-br-chat style=\"margin-top:24px\"></div></div>\n",
            );
        }
        block.push_str(&format!("<script>\n{AGENT_JS}\n</script>\n"));
        html = inject_before(&html, "</body>", &block, true);
    }
    html
}

/// Convenience for the in-app preview (MCP-App bridge transport).
pub fn assemble_preview(manifest: &Manifest, entry_html: &str) -> String {
    assemble(manifest, entry_html, "bridge", None)
}

// ---------------------------------------------------------------------------
// Standalone export scaffolding (Tier B)
// ---------------------------------------------------------------------------

fn cargo_toml(id: &str) -> String {
    format!(
        r#"[package]
name = "{id}"
version = "0.1.0"
edition = "2021"

[build-dependencies]
tauri-build = {{ version = "2", features = [] }}

[dependencies]
tauri = {{ version = "2", features = [] }}
tauri-plugin-shell = "2"
serde_json = "1"
"#
    )
}

fn tauri_conf(title: &str, id: &str) -> String {
    let cfg = serde_json::json!({
        "$schema": "https://schema.tauri.app/config/2",
        "productName": title,
        "version": "0.1.0",
        "identifier": format!("com.biorouter.agentdrafter.{}", id.replace('-', "")),
        "build": { "frontendDist": "../dist" },
        "app": {
            "windows": [{ "title": title, "width": 960, "height": 720 }],
            "security": { "csp": null }
        },
        "bundle": {
            "active": true,
            "targets": "all",
            // Bundle the BioRouter CLI as a sidecar so the artifact is self-contained.
            "externalBin": ["binaries/biorouter"]
        }
    });
    serde_json::to_string_pretty(&cfg).unwrap_or_default()
}

fn tauri_main_rs() -> String {
    // Spawns `biorouter acp` as a sidecar on launch. The embedded agent runtime
    // (agent.js) connects to it over the local ACP WebSocket.
    r#"// Auto-generated by BioRouter Agent Drafter.
use tauri_plugin_shell::ShellExt;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Launch the bundled BioRouter agent (ACP over stdio). A small bridge
            // is expected to expose it on ws://127.0.0.1:11577/acp for agent.js.
            let _ = app.shell().sidecar("biorouter").map(|cmd| cmd.args(["acp"]).spawn());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
"#
    .to_string()
}

fn readme(manifest: &Manifest, runtime: &str) -> String {
    format!(
        r#"# {title}

{desc}

Generated by **BioRouter Agent Drafter** (`{id}`, kind: `{kind:?}`, runtime: `{runtime}`).

## Run

### Web
Serve the `dist/` folder with any static file server, e.g.:

    npx serve dist

### Tauri (desktop, bundles the BioRouter agent)
1. Install the Tauri CLI: `cargo install tauri-cli --version '^2'`
2. Place the `biorouter` binary at `src-tauri/binaries/biorouter-<target-triple>`.
3. `cd src-tauri && cargo tauri dev`

The embedded agent runtime (`agent.js`) connects to the BioRouter agent over the
Agent Client Protocol (ACP). For agentic artifacts the chat panel is wired up
automatically.
"#,
        title = manifest.title,
        desc = manifest.description,
        id = manifest.id,
        kind = manifest.kind,
        runtime = runtime,
    )
}

/// Build the file list for a **web** export: assembled entry HTML + extra files.
pub fn scaffold_web(
    manifest: &Manifest,
    entry_html: &str,
    extra_files: &[(String, String)],
    endpoint: Option<&str>,
) -> Vec<(String, String)> {
    let assembled = assemble(
        manifest,
        entry_html,
        "acp-ws",
        Some(endpoint.unwrap_or(DEFAULT_ACP_WS)),
    );
    let mut files = vec![
        (format!("dist/{}", manifest.entry), assembled),
        ("README.md".to_string(), readme(manifest, "web")),
    ];
    for (path, content) in extra_files {
        if path != &manifest.entry {
            files.push((format!("dist/{path}"), content.clone()));
        }
    }
    files
}

/// Build the file list for a **Tauri** export: a web `dist/` plus a `src-tauri/`
/// project that bundles and launches the BioRouter agent sidecar.
pub fn scaffold_tauri(
    manifest: &Manifest,
    entry_html: &str,
    extra_files: &[(String, String)],
    endpoint: Option<&str>,
) -> Vec<(String, String)> {
    let mut files = scaffold_web(manifest, entry_html, extra_files, endpoint);
    files.push(("src-tauri/Cargo.toml".to_string(), cargo_toml(&manifest.id)));
    files.push((
        "src-tauri/tauri.conf.json".to_string(),
        tauri_conf(&manifest.title, &manifest.id),
    ));
    files.push(("src-tauri/src/main.rs".to_string(), tauri_main_rs()));
    files.push((
        "src-tauri/build.rs".to_string(),
        "fn main() { tauri_build::build() }\n".to_string(),
    ));
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_drafter::store::{AgentConfig, ArtifactKind, Manifest};

    fn manifest(kind: ArtifactKind) -> Manifest {
        Manifest {
            id: "demo".into(),
            title: "Demo <App>".into(),
            description: "d".into(),
            kind,
            entry: "index.html".into(),
            created_at: 0,
            updated_at: 0,
            agent: if kind == ArtifactKind::Agentic {
                Some(AgentConfig {
                    system_prompt: "be helpful".into(),
                    greeting: Some("hi".into()),
                    tools: vec!["developer".into()],
                })
            } else {
                None
            },
        }
    }

    #[test]
    fn starter_escapes_and_substitutes() {
        let html = starter("A <b> & C", "desc");
        assert!(html.contains("A &lt;b&gt; &amp; C"));
        assert!(!html.contains("{{TITLE}}"));
        assert!(!html.contains("{{DESCRIPTION}}"));
    }

    #[test]
    fn inject_before_head_inserts_theme() {
        let m = manifest(ArtifactKind::Static);
        let out = assemble_preview(&m, "<html><head></head><body>hi</body></html>");
        assert!(out.contains("biorouter-theme"));
        assert!(out.contains(THEME_CSS));
        // theme goes before </head>
        let style_pos = out.find("biorouter-theme").unwrap();
        let head_close = out.find("</head>").unwrap();
        assert!(style_pos < head_close);
    }

    #[test]
    fn inject_before_missing_head_prepends() {
        let m = manifest(ArtifactKind::Static);
        let out = assemble_preview(&m, "<div>no head</div>");
        assert!(out.contains("biorouter-theme"));
        assert!(out.starts_with("<style"));
    }

    #[test]
    fn static_artifact_has_no_agent_runtime() {
        let m = manifest(ArtifactKind::Static);
        let out = assemble_preview(&m, "<html><head></head><body></body></html>");
        assert!(!out.contains("BIOROUTER_AGENT_CONFIG"));
        assert!(!out.contains("data-br-chat"));
    }

    #[test]
    fn agentic_artifact_injects_config_chat_and_runtime() {
        let m = manifest(ArtifactKind::Agentic);
        let out = assemble_preview(&m, "<html><head></head><body></body></html>");
        assert!(out.contains("BIOROUTER_AGENT_CONFIG"));
        assert!(out.contains("\"transport\":\"bridge\""));
        assert!(out.contains("be helpful"));
        assert!(out.contains("data-br-chat"));
        assert!(out.contains("BioRouterAgent")); // runtime present
    }

    #[test]
    fn agentic_config_neutralizes_script_breakout() {
        let mut m = manifest(ArtifactKind::Agentic);
        m.agent = Some(AgentConfig {
            system_prompt: "</script><script>alert(1)</script>".into(),
            greeting: None,
            tools: vec![],
        });
        let out = assemble_preview(&m, "<html><head></head><body></body></html>");
        assert!(!out.contains("</script><script>alert(1)"));
        assert!(out.contains("\\u003c"));
    }

    #[test]
    fn does_not_add_second_chat_when_author_supplies_one() {
        let m = manifest(ArtifactKind::Agentic);
        let out = assemble_preview(&m, "<body><div data-br-chat></div></body>");
        // The author's panel is preserved and no auto-mounted panel is appended
        // (the auto-mounted one carries the distinctive inline margin marker).
        assert!(out.contains("<div data-br-chat></div>"));
        assert!(!out.contains("margin-top:24px"));
    }

    #[test]
    fn web_export_assembles_with_acp_ws_transport() {
        let m = manifest(ArtifactKind::Agentic);
        let files = scaffold_web(&m, "<html><head></head><body></body></html>", &[], None);
        let (path, content) = &files[0];
        assert_eq!(path, "dist/index.html");
        assert!(content.contains("\"transport\":\"acp-ws\""));
        assert!(content.contains(DEFAULT_ACP_WS));
        assert!(files.iter().any(|(p, _)| p == "README.md"));
    }

    #[test]
    fn tauri_export_includes_sidecar_project() {
        let m = manifest(ArtifactKind::Agentic);
        let files = scaffold_tauri(&m, "<html><head></head><body></body></html>", &[], None);
        let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&"dist/index.html"));
        assert!(paths.contains(&"src-tauri/Cargo.toml"));
        assert!(paths.contains(&"src-tauri/tauri.conf.json"));
        assert!(paths.contains(&"src-tauri/src/main.rs"));
        assert!(paths.contains(&"src-tauri/build.rs"));
        let main_rs = &files
            .iter()
            .find(|(p, _)| p == "src-tauri/src/main.rs")
            .unwrap()
            .1;
        assert!(main_rs.contains("sidecar(\"biorouter\")"));
        assert!(main_rs.contains("acp"));
        let conf = &files
            .iter()
            .find(|(p, _)| p == "src-tauri/tauri.conf.json")
            .unwrap()
            .1;
        assert!(conf.contains("externalBin"));
    }

    #[test]
    fn web_export_includes_extra_files_under_dist() {
        let m = manifest(ArtifactKind::Static);
        let files = scaffold_web(
            &m,
            "<html></html>",
            &[("css/app.css".to_string(), "body{}".to_string())],
            None,
        );
        assert!(files
            .iter()
            .any(|(p, c)| p == "dist/css/app.css" && c == "body{}"));
    }
}
