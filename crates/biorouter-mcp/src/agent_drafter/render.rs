//! Rendering + scaffolding for Agent Drafter apps.
//!
//! - [`assemble_app`] produces the HTML `biorouterd` serves at `/apps/<id>/`:
//!   the author's `index.html` with the BioRouter design system injected, an
//!   app-config script, and the esbuild bundle (`dist/app.js`). The bundle's SDK
//!   opens a WebSocket to the per-app agent backend and streams real answers.
//! - [`assemble_card`] produces a lightweight static preview shown inline in
//!   chat (apps are *used* in the browser, not in a sandboxed iframe).
//! - [`scaffold_standalone`] turns an app into a runnable TypeScript project that
//!   talks to a BioRouter daemon (the export path).

use crate::agent_drafter::store::Manifest;

pub const THEME_CSS: &str = include_str!("templates/theme.css");
pub const STARTER_HTML: &str = include_str!("templates/app-index.html");

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Build the default entry HTML for a freshly created app.
pub fn starter(title: &str, description: &str) -> String {
    STARTER_HTML
        .replace("{{TITLE}}", &html_escape(title))
        .replace("{{DESCRIPTION}}", &html_escape(description))
}

/// Insert `insert` immediately before the first (case-insensitive) `needle`.
/// If `needle` is absent, fall back to prepend/append.
fn inject_before(html: &str, needle: &str, insert: &str, append_if_missing: bool) -> String {
    if let Some(pos) = html.to_lowercase().find(&needle.to_lowercase()) {
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

/// Insert immediately *after* the first occurrence of `needle`.
fn inject_after(html: &str, needle: &str, insert: &str) -> String {
    if let Some(pos) = html.to_lowercase().find(&needle.to_lowercase()) {
        let at = pos + needle.len();
        let (before, after) = html.split_at(at);
        format!("{before}{insert}{after}")
    } else {
        format!("{insert}{html}")
    }
}

const THEME_TAG: &str = "<style id=\"biorouter-theme\">";

fn theme_block() -> String {
    format!("{THEME_TAG}\n{THEME_CSS}\n</style>\n")
}

/// The script that hands the app SDK its configuration.
pub fn app_config_script(manifest: &Manifest, endpoint: Option<&str>) -> String {
    let greeting = manifest.agent.as_ref().and_then(|a| a.greeting.clone());
    let cfg = serde_json::json!({
        "appId": manifest.id,
        "endpoint": endpoint,
        "greeting": greeting,
    });
    let json = serde_json::to_string(&cfg).unwrap_or_else(|_| "{}".to_string());
    // JSON is valid JS; neutralise any `</script>` breakout from string fields.
    let json = json.replace('<', "\\u003c");
    format!("<script>window.BIOROUTER_APP_CONFIG = {json};</script>\n")
}

/// Assemble the HTML `biorouterd` serves for a live app at `/apps/<id>/`.
///
/// `base_href` should be the serving prefix (e.g. `/apps/<id>/`) so that the
/// relative `dist/app.js` and any relative assets resolve correctly. `endpoint`
/// overrides the agent WebSocket URL (None → derived from page location).
pub fn assemble_app(
    manifest: &Manifest,
    index_html: &str,
    base_href: Option<&str>,
    endpoint: Option<&str>,
) -> String {
    let mut head = String::new();
    if let Some(base) = base_href {
        head.push_str(&format!("<base href=\"{}\">\n", html_escape(base)));
    }
    head.push_str(&theme_block());
    let mut html = inject_after(index_html, "<head>", &head);
    // If there was no <head>, inject_after fell back to prepending; ensure the
    // theme is still present (it is, since `head` was prepended).
    if !html.contains(THEME_TAG) {
        html = format!("{}{html}", theme_block());
    }

    let mut tail = app_config_script(manifest, endpoint);
    tail.push_str("<script src=\"dist/app.js\"></script>\n");
    inject_before(&html, "</body>", &tail, true)
}

/// A lightweight static preview for inline chat display. No live agent — apps
/// are launched in the browser. Shows the styled UI plus a launch hint banner.
pub fn assemble_card(manifest: &Manifest, index_html: &str) -> String {
    let head = theme_block();
    let mut html = inject_after(index_html, "<head>", &head);
    if !html.contains(THEME_TAG) {
        html = format!("{head}{html}");
    }
    let banner = format!(
        "<div style=\"position:sticky;top:0;z-index:10;background:var(--br-medium);\
         color:var(--br-text-muted);font-size:12px;padding:6px 12px;border-bottom:1px solid var(--br-border);\">\
         Preview of <strong>{}</strong> — launch from the Applications panel to use the live agent.</div>\n",
        html_escape(&manifest.title)
    );
    inject_after(&html, "<body>", &banner)
}

// ---------------------------------------------------------------------------
// Standalone export scaffolding (TypeScript project against a BioRouter daemon)
// ---------------------------------------------------------------------------

fn package_json(manifest: &Manifest) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "name": manifest.id,
        "version": "0.1.0",
        "private": true,
        "type": "module",
        "scripts": {
            "build": "esbuild src/main.ts --bundle --format=iife --outfile=dist/app.js",
            "start": "node serve.mjs"
        },
        "devDependencies": { "esbuild": "^0.23.0" }
    }))
    .unwrap_or_default()
}

fn serve_mjs(default_port: u16) -> String {
    format!(
        r#"// Minimal static server for the exported BioRouter app.
//
// The app talks to a BioRouter daemon for its agent loop. Start one with:
//   biorouterd                 # the BioRouter REST/WS server (serves /apps too)
// or point BR_AGENT_ENDPOINT at an existing daemon's per-app socket.
//
// This server only serves the static files; the SDK connects to the daemon set
// in index.html's BIOROUTER_APP_CONFIG.endpoint.
import {{ createServer }} from "node:http";
import {{ readFile }} from "node:fs/promises";
import {{ extname, normalize, resolve }} from "node:path";

const PORT = process.env.PORT || {default_port};
const ROOT = new URL(".", import.meta.url).pathname;
const MIME = {{ ".html": "text/html", ".js": "text/javascript", ".css": "text/css",
  ".json": "application/json", ".svg": "image/svg+xml", ".png": "image/png" }};

createServer(async (req, res) => {{
  let path = decodeURIComponent((req.url || "/").split("?")[0]);
  if (path === "/") path = "/index.html";
  // Resolve under ROOT; reject anything that escapes it (no path traversal).
  const base = ROOT.replace(/\/$/, "");
  const file = resolve(base, "." + normalize(path));
  if (file !== base && !file.startsWith(base + "/")) {{
    res.writeHead(403); res.end("Forbidden"); return;
  }}
  try {{
    const body = await readFile(file);
    res.writeHead(200, {{ "Content-Type": MIME[extname(file)] || "application/octet-stream" }});
    res.end(body);
  }} catch {{
    res.writeHead(404);
    res.end("Not found");
  }}
}}).listen(PORT, () => console.log(`App running at http://localhost:${{PORT}}`));
"#
    )
}

/// A double-clickable launcher (`run.command` on macOS / `run.sh` elsewhere) that
/// makes the exported folder *directly runnable*: self-install into the local
/// BioRouter store (idempotent + portable), start `biorouterd` if needed, and
/// open the app in the default browser. Requires `biorouterd` on PATH with a
/// configured provider (any BioRouter-supported LLM).
fn run_script(id: &str) -> String {
    format!(
        r#"#!/usr/bin/env bash
set -e
APP_ID="{id}"
DIR="$(cd "$(dirname "$0")" && pwd)"
STORE="$HOME/.config/biorouter/agent_drafter/$APP_ID"

# 1. Install this app into the local BioRouter store (idempotent, portable).
if [ ! -f "$STORE/manifest.json" ]; then
  mkdir -p "$STORE"
  cp -R "$DIR/." "$STORE/" 2>/dev/null || true
  rm -f "$STORE/run.command" "$STORE/run.sh" "$STORE/serve.mjs" "$STORE/README.md" "$STORE/package.json" 2>/dev/null || true
  echo "Installed '$APP_ID' into BioRouter."
fi

# 2. Start a BioRouter daemon if nothing is serving on :3000.
if ! curl -sf -o /dev/null http://127.0.0.1:3000/status 2>/dev/null; then
  echo "Starting biorouterd (uses your configured BioRouter provider)..."
  (biorouterd agent >"/tmp/biorouterd-$APP_ID.log" 2>&1 &)
  for i in $(seq 1 40); do curl -sf -o /dev/null http://127.0.0.1:3000/status 2>/dev/null && break; sleep 1; done
fi

# 3. Open the app in the default browser.
URL="http://127.0.0.1:3000/apps/$APP_ID/"
echo "Opening $URL"
if command -v open >/dev/null 2>&1; then open "$URL"
elif command -v xdg-open >/dev/null 2>&1; then xdg-open "$URL"
else echo "Open this URL in your browser: $URL"; fi
"#
    )
}

fn readme(manifest: &Manifest) -> String {
    format!(
        r#"# {title}

{desc}

A standalone **BioRouter app** generated by Agent Drafter (`{id}`). The UI is
TypeScript (a prebuilt `dist/app.js` is included); the agent loop runs on a
BioRouter daemon using whatever LLM provider you've configured.

## Easiest: double-click `run.command` (macOS) or `bash run.sh`

The launcher installs this app into your local BioRouter, starts `biorouterd`
if needed, and opens it in your browser. You only need `biorouter` installed
with a provider configured (`biorouter configure`).

## Manual

1. (Optional) rebuild the UI bundle after editing `src/`:

       npm install
       npm run build

2. Start a BioRouter daemon so the app has a backend to talk to:

       biorouterd agent      # headless backend on :3000 (no GUI needed)

   The daemon uses **your existing BioRouter configuration** — whatever LLM
   provider and model you've set up (`biorouter configure`), with credentials
   from your OS keychain. BioRouter supports many providers (Anthropic, OpenAI,
   Azure, Bedrock, Ollama, Xiaomi MiMo, local llama.cpp, …); this app is
   provider-agnostic and runs on whichever you've configured. If a provider key
   isn't in your keychain, export its key first, e.g.:

       export <PROVIDER>_API_KEY=...   # only if not already configured

   The app's agent runs the model/extensions/skills/knowledge from
   `manifest.json` and connects at `ws://127.0.0.1:3000/apps/{id}/agent`
   (override `BIOROUTER_APP_CONFIG.endpoint` in `index.html` for a remote daemon).

3. Serve the app and open it:

       npm start             # http://localhost:8787

`src/main.ts` is your app logic; `src/sdk.ts` is the BioRouter App SDK (opens the
agent WebSocket, streams markdown + charts, handles multimodal input). Edit,
re-run `npm run build`, refresh.

> The UI is fully self-contained and runs anywhere; only the agent backend
> requires a reachable `biorouterd` with valid provider credentials.
"#,
        title = manifest.title,
        desc = manifest.description,
        id = manifest.id,
    )
}

/// Build the file list for a standalone TypeScript export. Includes the author's
/// files, the SDK, a package.json/esbuild build, a tiny static server, and a
/// README. The served index points its SDK endpoint at a local daemon.
pub fn scaffold_standalone(
    manifest: &Manifest,
    index_html: &str,
    src_files: &[(String, String)],
    extra_files: &[(String, String)],
    endpoint: Option<&str>,
) -> Vec<(String, String)> {
    // The exported app talks to a running BioRouter daemon's per-app agent
    // socket (same protocol the App SDK speaks). Default to a local biorouterd
    // on :3000; the user can override via the endpoint arg (e.g. a remote
    // daemon). `biorouterd` must be running with the provider auth available.
    let default_endpoint = format!("ws://127.0.0.1:3000/apps/{}/agent", manifest.id);
    let endpoint = endpoint.unwrap_or(&default_endpoint);
    let assembled = assemble_app(manifest, index_html, None, Some(endpoint));
    let launcher = run_script(&manifest.id);
    let mut files = vec![
        (manifest.entry.clone(), assembled),
        ("package.json".to_string(), package_json(manifest)),
        ("serve.mjs".to_string(), serve_mjs(8787)),
        ("README.md".to_string(), readme(manifest)),
        // Directly-runnable launchers (double-click on macOS / `bash run.sh`).
        ("run.command".to_string(), launcher.clone()),
        ("run.sh".to_string(), launcher),
    ];
    for (path, content) in src_files {
        files.push((path.clone(), content.clone()));
    }
    for (path, content) in extra_files {
        if path != &manifest.entry && !path.starts_with("src/") {
            files.push((path.clone(), content.clone()));
        }
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_drafter::store::{AgentConfig, ArtifactKind, ModelSelection};

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
                    tools: vec![],
                    model: Some(ModelSelection {
                        provider: Some("xiaomi_mimo".into()),
                        model: Some("mimo-v2.5".into()),
                    }),
                    extensions: vec!["developer".into()],
                    skills: vec![],
                    knowledge_base: None,
                    max_turns: None,
                })
            } else {
                None
            },
            width: None,
            height: None,
            built_at: None,
            session_id: None,
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
    fn assemble_app_injects_theme_base_config_and_bundle() {
        let m = manifest(ArtifactKind::Agentic);
        let out = assemble_app(
            &m,
            "<html><head></head><body>hi</body></html>",
            Some("/apps/demo/"),
            None,
        );
        assert!(out.contains("biorouter-theme"));
        assert!(out.contains("<base href=\"/apps/demo/\">"));
        assert!(out.contains("BIOROUTER_APP_CONFIG"));
        assert!(out.contains("\"appId\":\"demo\""));
        assert!(out.contains("dist/app.js"));
        // theme precedes the bundle script
        assert!(out.find("biorouter-theme").unwrap() < out.find("dist/app.js").unwrap());
    }

    #[test]
    fn assemble_app_handles_missing_head_and_body() {
        let m = manifest(ArtifactKind::Agentic);
        let out = assemble_app(&m, "<div>bare</div>", None, None);
        assert!(out.contains("biorouter-theme"));
        assert!(out.contains("dist/app.js"));
    }

    #[test]
    fn config_neutralizes_script_breakout() {
        let mut m = manifest(ArtifactKind::Agentic);
        m.agent.as_mut().unwrap().greeting = Some("</script><script>alert(1)</script>".into());
        let out = assemble_app(&m, "<html><head></head><body></body></html>", None, None);
        assert!(!out.contains("</script><script>alert(1)"));
        assert!(out.contains("\\u003c"));
    }

    #[test]
    fn card_has_launch_banner_and_theme() {
        let m = manifest(ArtifactKind::Static);
        let out = assemble_card(&m, "<html><head></head><body><h1>Hi</h1></body></html>");
        assert!(out.contains("biorouter-theme"));
        assert!(out.contains("Applications panel"));
        assert!(out.contains("<h1>Hi</h1>"));
    }

    #[test]
    fn standalone_export_is_typescript_project() {
        let m = manifest(ArtifactKind::Agentic);
        let files = scaffold_standalone(
            &m,
            "<html><head></head><body></body></html>",
            &[("src/main.ts".to_string(), "import './sdk';".to_string())],
            &[],
            None,
        );
        let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&"index.html"));
        assert!(paths.contains(&"package.json"));
        assert!(paths.contains(&"serve.mjs"));
        assert!(paths.contains(&"README.md"));
        assert!(paths.contains(&"src/main.ts"));
        let pkg = &files.iter().find(|(p, _)| p == "package.json").unwrap().1;
        assert!(pkg.contains("esbuild"));
        let idx = &files.iter().find(|(p, _)| p == "index.html").unwrap().1;
        // Exported app points at a biorouterd per-app agent socket (App SDK
        // protocol), not the old ACP endpoint.
        assert!(idx.contains("ws://127.0.0.1:3000/apps/demo/agent"));
    }
}
