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
const THEME_OVERRIDES_TAG: &str = "<style id=\"biorouter-theme-overrides\">";

fn theme_block() -> String {
    format!("{THEME_TAG}\n{THEME_CSS}\n</style>\n")
}

/// An extra `<style>` layer carrying the app's sanitized accent + custom token
/// overrides. Emitted only when the app declares overrides, so a v1/base app's
/// output is byte-identical to before. Scoped to `:root[data-br-pack]` (an
/// attribute [`set_root_pack`] always writes when overrides exist), so these
/// declarations win over both the base `:root` tokens and any pack layer.
fn theme_overrides_block(manifest: &Manifest) -> Option<String> {
    let accent = manifest.theme.sanitized_accent();
    let tokens = manifest.theme.sanitized_tokens();
    if accent.is_none() && tokens.is_empty() {
        return None;
    }
    let mut decls = String::new();
    if let Some(a) = accent {
        decls.push_str(&format!("--br-accent: {a};"));
    }
    for (k, v) in &tokens {
        decls.push_str(&format!(" {k}: {v};"));
    }
    Some(format!(
        "{THEME_OVERRIDES_TAG}\n:root[data-br-pack] {{ {decls} }}\n</style>\n"
    ))
}

/// The full injected head CSS: the base design system plus, when the app
/// customises its theme, an overrides layer.
fn theme_head(manifest: &Manifest) -> String {
    let mut s = theme_block();
    if let Some(ov) = theme_overrides_block(manifest) {
        s.push_str(&ov);
    }
    s
}

/// The `data-br-pack` value to stamp on `<html>`, or `None` when the base look
/// with no overrides is in effect (so nothing is written and v1 output is
/// unchanged). Present for a non-base pack, or whenever overrides exist (the
/// overrides style keys off the `[data-br-pack]` attribute).
fn pack_attr_value(manifest: &Manifest) -> Option<&str> {
    let pack = manifest.theme.resolved_pack();
    if pack == crate::agent_drafter::manifest::DEFAULT_THEME_PACK && !manifest.theme.has_overrides()
    {
        None
    } else {
        Some(pack)
    }
}

/// Stamp `data-br-pack="<pack>"` onto the document's `<html>` element. Robust to
/// `<html>` and `<html lang="en">`; if the document has no `<html>` tag (a bare
/// fragment) the string is returned unchanged — the base tokens still apply.
fn set_root_pack(html: &str, pack: &str) -> String {
    if let Some(pos) = html.to_lowercase().find("<html") {
        let at = pos + "<html".len();
        let (before, after) = html.split_at(at);
        format!("{before} data-br-pack=\"{}\"{after}", html_escape(pack))
    } else {
        html.to_string()
    }
}

/// The script that hands the app SDK its configuration.
///
/// `endpoint` pins a single agent socket (a remote daemon); leave it `None` so
/// the SDK derives one from the page's own origin, which is what makes an app
/// work no matter which port its daemon landed on. `endpoints` are additional
/// fallbacks tried in order — exports use them to cover a `file://` open.
///
/// `ws_token` is the per-app socket token the SDK must present on the agent
/// WebSocket (`?token=…`) — `serve_index` mints one per daemon run and injects
/// it here so any page this daemon serves can authenticate. Exports pass `None`
/// (their proxy mints its own token in a later phase).
pub fn app_config_script(
    manifest: &Manifest,
    endpoint: Option<&str>,
    endpoints: &[String],
    ws_token: Option<&str>,
) -> String {
    let greeting = manifest.agent.as_ref().and_then(|a| a.greeting.clone());
    let mut cfg = serde_json::json!({
        "appId": manifest.id,
        "endpoint": endpoint,
        "greeting": greeting,
    });
    if !endpoints.is_empty() {
        cfg["endpoints"] = serde_json::json!(endpoints);
    }
    if let Some(token) = ws_token {
        cfg["wsToken"] = serde_json::json!(token);
    }
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
    ws_token: Option<&str>,
) -> String {
    assemble_app_with_endpoints(manifest, index_html, base_href, endpoint, &[], ws_token)
}

/// [`assemble_app`], plus fallback endpoints the SDK tries when the primary one
/// (usually the page's own origin) can't be reached.
pub fn assemble_app_with_endpoints(
    manifest: &Manifest,
    index_html: &str,
    base_href: Option<&str>,
    endpoint: Option<&str>,
    endpoints: &[String],
    ws_token: Option<&str>,
) -> String {
    let mut head = String::new();
    if let Some(base) = base_href {
        head.push_str(&format!("<base href=\"{}\">\n", html_escape(base)));
    }
    head.push_str(&theme_head(manifest));
    let mut html = inject_after(index_html, "<head>", &head);
    // If there was no <head>, inject_after fell back to prepending; ensure the
    // theme is still present (it is, since `head` was prepended).
    if !html.contains(THEME_TAG) {
        html = format!("{}{html}", theme_head(manifest));
    }
    // Select the app's theme pack (and enable its override layer) on <html>.
    if let Some(pack) = pack_attr_value(manifest) {
        html = set_root_pack(&html, pack);
    }

    let mut tail = app_config_script(manifest, endpoint, endpoints, ws_token);
    tail.push_str("<script src=\"dist/app.js\"></script>\n");
    inject_before(&html, "</body>", &tail, true)
}

/// A lightweight static preview for inline chat display. No live agent — apps
/// are launched in the browser. Shows the styled UI plus a launch hint banner.
pub fn assemble_card(manifest: &Manifest, index_html: &str) -> String {
    let head = theme_head(manifest);
    let mut html = inject_after(index_html, "<head>", &head);
    if !html.contains(THEME_TAG) {
        html = format!("{head}{html}");
    }
    if let Some(pack) = pack_attr_value(manifest) {
        html = set_root_pack(&html, pack);
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

/// Shared shell prelude: locate a `biorouterd`, install the app into the local
/// store, and bring a daemon up on a known port. Sourced by `run.sh` so the two
/// launch paths can't drift.
///
/// Defines `BIOROUTERD`, `PORT`, and `BASE` (e.g. `http://127.0.0.1:3000`).
fn launcher_lib(id: &str) -> String {
    format!(
        r#"#!/usr/bin/env bash
# Shared launcher helpers for the exported BioRouter app "{id}".
# Sourced by run.sh / run.command; also usable on its own.
APP_ID="{id}"
DIR="${{DIR:-$(cd "$(dirname "${{BASH_SOURCE[0]}}")" && pwd)}}"
STORE="${{XDG_CONFIG_HOME:-$HOME/.config}}/biorouter/agent_drafter/$APP_ID"

die() {{ echo "error: $*" >&2; exit 1; }}

# ── 1. Find biorouterd ────────────────────────────────────────────────────
# A bare `biorouterd` on PATH is the common case, but the desktop app ships its
# own copy and never puts it on PATH — so look there too. (The old launcher
# backgrounded a bare `biorouterd` inside `( ... & )`, which returns 0 even when
# the binary is missing, so `set -e` never caught it and the user just saw a
# dead page 40 seconds later.)
find_biorouterd() {{
  # Guard optional vars with `:-`: run.sh runs under `set -u`, so a bare
  # $BIOROUTERD_BIN (normally UNSET — biorouterd on PATH is the common case)
  # would abort the whole launcher with "unbound variable" before the PATH
  # lookup ever ran.
  if [ -n "${{BIOROUTERD_BIN:-}}" ] && [ -x "${{BIOROUTERD_BIN:-}}" ]; then echo "${{BIOROUTERD_BIN}}"; return 0; fi
  if command -v biorouterd >/dev/null 2>&1; then command -v biorouterd; return 0; fi
  for p in \
    "$HOME/.local/bin/biorouterd" \
    "/usr/local/bin/biorouterd" \
    "/opt/homebrew/bin/biorouterd" \
    "/Applications/BioRouter.app/Contents/Resources/bin/biorouterd" \
    "$HOME/Applications/BioRouter.app/Contents/Resources/bin/biorouterd" \
    "/opt/BioRouter/resources/bin/biorouterd" \
    "/usr/lib/biorouter/resources/bin/biorouterd"
  do
    [ -x "$p" ] && {{ echo "$p"; return 0; }}
  done
  return 1
}}

# ── 2. Install into the local BioRouter store ─────────────────────────────
# The daemon looks apps up by `manifest.json` in the store. Copying only the
# runtime files keeps the launcher/README/package.json out of the app dir, and
# re-syncing every run means `npm run build` edits show up on refresh.
install_app() {{
  mkdir -p "$STORE"
  [ -f "$DIR/manifest.json" ] || die "manifest.json missing from this folder — re-export the app."
  cp "$DIR/manifest.json" "$STORE/manifest.json"
  cp "$DIR/index.html" "$STORE/index.html" 2>/dev/null || true
  for sub in src dist assets; do
    if [ -d "$DIR/$sub" ]; then
      rm -rf "$STORE/$sub"
      cp -R "$DIR/$sub" "$STORE/$sub"
    fi
  done
}}

port_alive() {{ curl -sf -o /dev/null --max-time 1 "http://127.0.0.1:$1/status" 2>/dev/null; }}

# ── 3. Reuse a running daemon, else start one ─────────────────────────────
# Any biorouterd on this machine reads the same store, so once the app is
# installed, whichever daemon is up can serve it.
start_daemon() {{
  for p in "${{BIOROUTERD_PORT:-3000}}" 3000 3001 3002 3003; do
    if port_alive "$p"; then PORT="$p"; echo "Using the BioRouter daemon already on :$PORT"; return 0; fi
  done

  BIOROUTERD="$(find_biorouterd)" || die "biorouterd not found. Install BioRouter, or set BIOROUTERD_BIN=/path/to/biorouterd"

  for p in "${{BIOROUTERD_PORT:-3000}}" 3001 3002 3003; do
    LOG="${{TMPDIR:-/tmp}}/biorouterd-$APP_ID-$p.log"
    echo "Starting $BIOROUTERD on :$p (using your configured BioRouter provider)..."
    BIOROUTER_PORT="$p" "$BIOROUTERD" agent >"$LOG" 2>&1 &
    for _ in $(seq 1 40); do
      port_alive "$p" && {{ PORT="$p"; return 0; }}
      kill -0 "$!" 2>/dev/null || break   # it died; try the next port
      sleep 0.5
    done
    echo "  :$p did not come up (see $LOG)" >&2
  done
  die "could not start biorouterd. Last log: $LOG"
}}

# ── 4. Verify the daemon can actually serve this app ──────────────────────
verify_app() {{
  code="$(curl -s -o /dev/null -w '%{{http_code}}' "$BASE/apps/$APP_ID/" 2>/dev/null || echo 000)"
  [ "$code" = "200" ] || die "the daemon on :$PORT does not serve '$APP_ID' (HTTP $code). Is the store at $STORE?"
}}

open_url() {{
  echo "Opening $1"
  if command -v open >/dev/null 2>&1; then open "$1"
  elif command -v xdg-open >/dev/null 2>&1; then xdg-open "$1"
  else echo "Open this URL in your browser: $1"; fi
}}
"#
    )
}

/// A double-clickable launcher (`run.command` on macOS / `run.sh` elsewhere).
///
/// Serving the page *from the daemon* is what makes this robust: the SDK derives
/// its WebSocket endpoint from the page origin, so the app connects no matter
/// which port the daemon ended up on. Needs no Node — `dist/app.js` ships
/// prebuilt.
fn run_script() -> String {
    r#"#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=biorouter-launch.sh
. "$DIR/biorouter-launch.sh"

install_app
start_daemon
BASE="http://127.0.0.1:$PORT"
verify_app
open_url "$BASE/apps/$APP_ID/"
"#
    .to_string()
}

/// The dev server: static files from *this* folder (so `npm run build` edits are
/// visible on refresh) plus a transparent proxy of `/apps/**` — including the
/// agent WebSocket — to the daemon.
///
/// The proxy is the point. The old `serve.mjs` was a bare static server while
/// `index.html` hard-coded `ws://127.0.0.1:3000`, so the app broke whenever the
/// daemon wasn't on 3000 (the desktop app starts it on an ephemeral port). Now
/// everything is same-origin and the port is discovered at runtime.
fn serve_mjs(id: &str, default_port: u16) -> String {
    format!(
        r#"// Dev server for the exported BioRouter app "{id}".
//
//   node serve.mjs          # installs the app, starts/reuses a daemon, serves + proxies
//   PORT=9000 node serve.mjs
//   BIOROUTERD_BIN=... BIOROUTERD_PORT=... node serve.mjs
//
// Static files come from this folder; anything under /apps/** (including the
// agent WebSocket at /apps/<id>/agent) is proxied to the BioRouter daemon, so
// the page and its agent share an origin and no port is ever hard-coded.
import {{ createServer, request as httpRequest }} from "node:http";
import {{ spawn }} from "node:child_process";
import {{ readFile, mkdir, cp, rm, access }} from "node:fs/promises";
import {{ constants }} from "node:fs";
import {{ extname, normalize, resolve, join, dirname }} from "node:path";
import {{ fileURLToPath }} from "node:url";
import {{ homedir }} from "node:os";

const APP_ID = {id_json};
const PORT = Number(process.env.PORT || {default_port});
const ROOT = dirname(fileURLToPath(import.meta.url));
const STORE = join(process.env.XDG_CONFIG_HOME || join(homedir(), ".config"),
                   "biorouter", "agent_drafter", APP_ID);
const MIME = {{ ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8", ".json": "application/json", ".svg": "image/svg+xml",
  ".png": "image/png", ".jpg": "image/jpeg", ".gif": "image/gif", ".webp": "image/webp",
  ".ico": "image/x-icon", ".woff2": "font/woff2", ".map": "application/json" }};

const exists = (p) => access(p, constants.F_OK).then(() => true, () => false);

const DAEMON_PATHS = [
  process.env.BIOROUTERD_BIN,
  "biorouterd",
  join(homedir(), ".local/bin/biorouterd"),
  "/usr/local/bin/biorouterd",
  "/opt/homebrew/bin/biorouterd",
  "/Applications/BioRouter.app/Contents/Resources/bin/biorouterd",
  join(homedir(), "Applications/BioRouter.app/Contents/Resources/bin/biorouterd"),
  "/opt/BioRouter/resources/bin/biorouterd",
].filter(Boolean);

/** Copy the runtime files into the store so the daemon can resolve the app. */
async function installApp() {{
  if (!(await exists(join(ROOT, "manifest.json")))) {{
    throw new Error("manifest.json missing from this folder - re-export the app.");
  }}
  await mkdir(STORE, {{ recursive: true }});
  await cp(join(ROOT, "manifest.json"), join(STORE, "manifest.json"));
  if (await exists(join(ROOT, "index.html"))) {{
    await cp(join(ROOT, "index.html"), join(STORE, "index.html"));
  }}
  for (const sub of ["src", "dist", "assets"]) {{
    if (await exists(join(ROOT, sub))) {{
      await rm(join(STORE, sub), {{ recursive: true, force: true }});
      await cp(join(ROOT, sub), join(STORE, sub), {{ recursive: true }});
    }}
  }}
}}

function probe(port) {{
  return new Promise((res) => {{
    const r = httpRequest({{ host: "127.0.0.1", port, path: "/status", timeout: 900 }}, (resp) => {{
      resp.resume();
      res(resp.statusCode === 200);
    }});
    r.on("error", () => res(false));
    r.on("timeout", () => {{ r.destroy(); res(false); }});
    r.end();
  }});
}}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Reuse a daemon if one answers; otherwise spawn one and wait for /status. */
async function ensureDaemon() {{
  const preferred = Number(process.env.BIOROUTERD_PORT || 3000);
  for (const p of [preferred, 3000, 3001, 3002, 3003]) {{
    if (await probe(p)) {{ console.log(`Using the BioRouter daemon already on :${{p}}`); return p; }}
  }}
  for (const bin of DAEMON_PATHS) {{
    for (const p of [preferred, 3001, 3002, 3003]) {{
      let child;
      try {{
        child = spawn(bin, ["agent"], {{
          env: {{ ...process.env, BIOROUTER_PORT: String(p) }},
          stdio: "ignore", detached: true,
        }});
      }} catch {{ break; }}          // this binary doesn't exist; try the next one
      let spawnFailed = false;
      child.on("error", () => {{ spawnFailed = true; }});
      child.unref();
      for (let i = 0; i < 40 && !spawnFailed; i++) {{
        if (await probe(p)) {{ console.log(`Started ${{bin}} on :${{p}}`); return p; }}
        await sleep(500);
      }}
      if (spawnFailed) break;
    }}
  }}
  throw new Error(
    "Could not find or start biorouterd. Install BioRouter, or set BIOROUTERD_BIN=/path/to/biorouterd"
  );
}}

// Install the app, then bring up a daemon. On failure, print the actionable
// message (not an unhandled-rejection stack trace) and exit cleanly.
let daemonPort;
try {{
  await installApp();
  daemonPort = await ensureDaemon();
}} catch (e) {{
  console.error("\n" + (e && e.message ? e.message : e) + "\n");
  process.exit(1);
}}

/** Proxy an ordinary HTTP request to the daemon. */
function proxyHttp(req, res) {{
  const up = httpRequest(
    {{ host: "127.0.0.1", port: daemonPort, path: req.url, method: req.method, headers: req.headers }},
    (upRes) => {{
      res.writeHead(upRes.statusCode || 502, upRes.headers);
      upRes.pipe(res);
    }}
  );
  up.on("error", () => {{ res.writeHead(502); res.end("BioRouter daemon unreachable"); }});
  req.pipe(up);
}}

const server = createServer(async (req, res) => {{
  const url = (req.url || "/").split("?")[0];
  if (url === "/apps" || url.startsWith("/apps/")) return proxyHttp(req, res);

  let path = decodeURIComponent(url);
  if (path === "/") path = "/index.html";
  // Resolve under ROOT; reject anything that escapes it (no path traversal).
  const file = resolve(ROOT, "." + normalize(path));
  if (file !== ROOT && !file.startsWith(ROOT + "/")) {{
    res.writeHead(403); res.end("Forbidden"); return;
  }}
  try {{
    const body = await readFile(file);
    res.writeHead(200, {{ "Content-Type": MIME[extname(file)] || "application/octet-stream" }});
    res.end(body);
  }} catch {{
    res.writeHead(404); res.end("Not found");
  }}
}});

// The agent WebSocket. Forward the upgrade verbatim and splice the sockets.
server.on("upgrade", (req, socket, head) => {{
  const up = httpRequest({{
    host: "127.0.0.1", port: daemonPort, path: req.url, method: "GET", headers: req.headers,
  }});
  up.on("upgrade", (upRes, upSocket, upHead) => {{
    const lines = ["HTTP/1.1 101 Switching Protocols"];
    for (const [k, v] of Object.entries(upRes.headers)) lines.push(`${{k}}: ${{v}}`);
    socket.write(lines.join("\r\n") + "\r\n\r\n");
    if (upHead && upHead.length) socket.write(upHead);
    upSocket.pipe(socket);
    socket.pipe(upSocket);
    const bail = () => {{ upSocket.destroy(); socket.destroy(); }};
    upSocket.on("error", bail);
    socket.on("error", bail);
  }});
  up.on("response", (r) => {{ socket.write(`HTTP/1.1 ${{r.statusCode}} \r\n\r\n`); socket.destroy(); }});
  up.on("error", () => socket.destroy());
  if (head && head.length) up.write(head);
  up.end();
}});

// Bind loopback ONLY. This server proxies straight through to the daemon's
// `/apps/**` routes, which are deliberately exempt from the secret-key check
// (a browser tab can't send the header). Listening on 0.0.0.0 would hand the
// whole LAN an unauthenticated agent.
function listen(port, attempt = 0) {{
  server.once("error", (e) => {{
    if (e.code === "EADDRINUSE" && attempt < 10) {{
      console.warn(`:${{port}} is busy, trying :${{port + 1}}`);
      listen(port + 1, attempt + 1);
    }} else {{
      console.error(e.message);
      process.exit(1);
    }}
  }});
  server.listen(port, "127.0.0.1", () => {{
    console.log(`App running at http://127.0.0.1:${{port}}  (agent -> :${{daemonPort}})`);
  }});
}}
listen(PORT);
"#,
        id = id,
        id_json = serde_json::to_string(id).unwrap_or_else(|_| "\"app\"".to_string()),
        default_port = default_port,
    )
}

fn readme(manifest: &Manifest) -> String {
    format!(
        r#"# {title}

{desc}

A standalone **BioRouter app** generated by Agent Drafter (`{id}`). The UI is
prebuilt (`dist/app.js` ships with it); the agent loop runs on a BioRouter
daemon using whatever LLM provider you have configured.

## Run it

Double-click **`run.command`** (macOS) or run **`bash run.sh`** (Linux/WSL).

That's the whole thing. The launcher:

1. installs the app into your local BioRouter store (`manifest.json` and all),
2. reuses a running `biorouterd`, or starts one on the first free port,
3. checks the daemon can actually serve the app, and
4. opens it in your browser.

No Node, no `npm install`, no build step. You need BioRouter installed with a
provider configured (`biorouter configure`). If `biorouterd` lives somewhere
unusual, set `BIOROUTERD_BIN=/path/to/biorouterd`.

## Editing the UI

`src/main.ts` is the app logic; `src/sdk.ts` is the BioRouter App SDK. To
iterate, use the dev server — it serves *this* folder and proxies the agent, so
your rebuilt bundle shows up on refresh:

    npm install
    npm run build
    npm start            # http://localhost:8787

`serve.mjs` starts (or reuses) a daemon for you and proxies `/apps/**` to it,
including the agent WebSocket. Because the page and the agent then share an
origin, nothing hard-codes a port.

## Notes

- **Don't open `index.html` straight off disk.** A `file://` page has no origin
  to derive the agent socket from; it falls back to `ws://127.0.0.1:3000`, which
  only works if a daemon happens to be on that port. Use `run.sh` or `npm start`.
- The daemon uses **your existing BioRouter configuration** — provider, model,
  and credentials from your OS keychain. BioRouter supports many providers
  (Anthropic, OpenAI, Azure, Bedrock, Ollama, Xiaomi MiMo, local llama.cpp, …);
  this app is provider-agnostic. If a key isn't in your keychain, export it
  first: `export <PROVIDER>_API_KEY=...`
- The agent's model, extensions, skills, and knowledge base come from
  `manifest.json`. To point at a remote daemon instead, set
  `BIOROUTER_APP_CONFIG.endpoint` in `index.html`.
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
    // Leave the primary endpoint unset so the SDK derives it from the page's own
    // origin. Both launch paths serve the app from an origin that also answers
    // `/apps/<id>/agent` (the daemon directly, or `serve.mjs` proxying it), so
    // the app connects whatever port the daemon landed on. The loopback :3000
    // fallback only covers someone opening `index.html` off disk.
    //
    // An explicit `endpoint` (a remote daemon) still wins.
    let fallbacks = vec![format!("ws://127.0.0.1:3000/apps/{}/agent", manifest.id)];
    // No `ws_token` in an export: the exported page is served by `serve.mjs` /
    // the launcher's proxy, which mints and injects its own token in a later
    // phase — this daemon's per-run token would be meaningless there.
    let assembled = if endpoint.is_some() {
        assemble_app(manifest, index_html, None, endpoint, None)
    } else {
        assemble_app_with_endpoints(manifest, index_html, None, None, &fallbacks, None)
    };

    // The manifest IS the app's registration: `serve_index` and `agent_ws` both
    // 404 without it. Omitting it made every export un-runnable on any machine
    // where the app wasn't authored — and `run.sh`'s
    // `[ ! -f "$STORE/manifest.json" ]` install guard could never become false.
    let manifest_json = serde_json::to_string_pretty(manifest).unwrap_or_default();

    let mut files = vec![
        (manifest.entry.clone(), assembled),
        ("manifest.json".to_string(), manifest_json),
        ("package.json".to_string(), package_json(manifest)),
        ("serve.mjs".to_string(), serve_mjs(&manifest.id, 8787)),
        ("README.md".to_string(), readme(manifest)),
        // Directly-runnable launchers (double-click on macOS / `bash run.sh`).
        (
            "biorouter-launch.sh".to_string(),
            launcher_lib(&manifest.id),
        ),
        ("run.command".to_string(), run_script()),
        ("run.sh".to_string(), run_script()),
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

/// Files in an export that must be executable for the launcher to be
/// double-clickable. The writer (`export_app`, and the GUI) chmods these.
pub const EXECUTABLE_EXPORT_FILES: &[&str] = &["run.command", "run.sh", "biorouter-launch.sh"];

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
                        ..Default::default()
                    }),
                    extensions: vec!["developer".into()],
                    skills: vec![],
                    knowledge_base: None,
                    max_turns: None,
                    ..Default::default()
                })
            } else {
                None
            },
            width: None,
            height: None,
            built_at: None,
            sdk_hash: None,
            session_id: None,
            surface: crate::agent_drafter::manifest::SurfaceDecl::default(),
            theme: crate::agent_drafter::manifest::ThemeConfig::default(),
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
    fn app_config_script_embeds_ws_token_when_given() {
        let m = manifest(ArtifactKind::Agentic);
        // No token → no wsToken key at all (exports, previews).
        let none = app_config_script(&m, None, &[], None);
        assert!(!none.contains("wsToken"), "absent token must not appear");
        // A token → surfaced verbatim in the config JSON the SDK reads.
        let with = app_config_script(&m, None, &[], Some("deadbeefcafef00d0123456789abcdef"));
        assert!(with.contains("\"wsToken\":\"deadbeefcafef00d0123456789abcdef\""));
        // And it flows through the full assemble path.
        let out = assemble_app(
            &m,
            "<html><head></head><body></body></html>",
            None,
            None,
            Some("deadbeefcafef00d0123456789abcdef"),
        );
        assert!(out.contains("\"wsToken\":\"deadbeefcafef00d0123456789abcdef\""));
    }

    #[test]
    fn assemble_app_handles_missing_head_and_body() {
        let m = manifest(ArtifactKind::Agentic);
        let out = assemble_app(&m, "<div>bare</div>", None, None, None);
        assert!(out.contains("biorouter-theme"));
        assert!(out.contains("dist/app.js"));
    }

    #[test]
    fn config_neutralizes_script_breakout() {
        let mut m = manifest(ArtifactKind::Agentic);
        m.agent.as_mut().unwrap().greeting = Some("</script><script>alert(1)</script>".into());
        let out = assemble_app(
            &m,
            "<html><head></head><body></body></html>",
            None,
            None,
            None,
        );
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

    // ── Theme packs (Apps SDK v2, Pillar 6) ─────────────────────────────────

    #[test]
    fn base_theme_leaves_v1_output_untouched() {
        // The default `biorouter` pack with no overrides must stamp no attribute
        // and inject no overrides layer, so a v1 app renders byte-for-byte as it
        // used to.
        let m = manifest(ArtifactKind::Static);
        assert!(m.theme.is_default());
        let out = assemble_app(
            &m,
            "<html lang=\"en\"><head></head><body>hi</body></html>",
            None,
            None,
            None,
        );
        // `data-br-pack` appears in theme.css pack selectors, so check the
        // stamped ATTRIBUTE specifically (right after `<html`).
        assert!(
            !out.contains("<html data-br-pack"),
            "base pack stamps no attribute"
        );
        assert!(!out.contains("biorouter-theme-overrides"));
    }

    #[test]
    fn named_pack_lands_on_html_element() {
        use crate::agent_drafter::manifest::ThemeConfig;
        let mut m = manifest(ArtifactKind::Static);
        m.theme = ThemeConfig {
            pack: "clinical".into(),
            ..Default::default()
        };
        let out = assemble_app(
            &m,
            "<html lang=\"en\"><head></head><body>hi</body></html>",
            None,
            None,
            None,
        );
        assert!(
            out.contains("<html data-br-pack=\"clinical\" lang=\"en\">"),
            "pack must stamp <html>: {out}"
        );
        // No custom overrides declared → no overrides style layer.
        assert!(!out.contains("biorouter-theme-overrides"));
    }

    #[test]
    fn theme_overrides_are_injected_and_sanitized() {
        use crate::agent_drafter::manifest::ThemeConfig;
        use std::collections::HashMap;
        let mut tokens = HashMap::new();
        tokens.insert("--br-radius".to_string(), "2px".to_string()); // safe
        tokens.insert("color".to_string(), "red".to_string()); // bad key: dropped
        tokens.insert("--br-bg".to_string(), "red; } body{}".to_string()); // breakout: dropped
        let mut m = manifest(ArtifactKind::Agentic);
        m.theme = ThemeConfig {
            pack: "midnight".into(),
            accent: Some("#8b5cf6".into()),
            tokens,
        };
        let out = assemble_app(
            &m,
            "<html><head></head><body></body></html>",
            None,
            None,
            None,
        );
        assert!(out.contains("biorouter-theme-overrides"));
        assert!(
            out.contains("--br-accent: #8b5cf6;"),
            "accent override: {out}"
        );
        assert!(
            out.contains("--br-radius: 2px;"),
            "safe token override kept"
        );
        assert!(!out.contains("color: red"), "unsafe key dropped");
        assert!(!out.contains("red; }"), "breakout value dropped");
        // Overrides key off the pack attribute, which is stamped for midnight.
        assert!(out.contains("<html data-br-pack=\"midnight\">"));
    }

    #[test]
    fn overrides_on_base_pack_still_stamp_the_attribute() {
        use crate::agent_drafter::manifest::ThemeConfig;
        let mut m = manifest(ArtifactKind::Static);
        m.theme = ThemeConfig {
            pack: "biorouter".into(),
            accent: Some("#123456".into()),
            ..Default::default()
        };
        let out = assemble_app(
            &m,
            "<html><head></head><body></body></html>",
            None,
            None,
            None,
        );
        // The base pack needs the attribute too so `:root[data-br-pack]` matches.
        assert!(out.contains("<html data-br-pack=\"biorouter\">"));
        assert!(out.contains("--br-accent: #123456;"));
    }

    #[test]
    fn unknown_pack_falls_back_to_base_and_stamps_nothing() {
        use crate::agent_drafter::manifest::ThemeConfig;
        let mut m = manifest(ArtifactKind::Static);
        m.theme = ThemeConfig {
            pack: "neon-hacker".into(),
            ..Default::default()
        };
        let out = assemble_app(
            &m,
            "<html><head></head><body></body></html>",
            None,
            None,
            None,
        );
        // resolved_pack() → biorouter, no overrides → no attribute stamped.
        assert!(!out.contains("<html data-br-pack"));
    }

    fn export(m: &Manifest, endpoint: Option<&str>) -> Vec<(String, String)> {
        scaffold_standalone(
            m,
            "<html><head></head><body></body></html>",
            &[("src/main.ts".to_string(), "import './sdk';".to_string())],
            &[],
            endpoint,
        )
    }

    fn file<'a>(files: &'a [(String, String)], path: &str) -> &'a str {
        &files.iter().find(|(p, _)| p == path).expect(path).1
    }

    #[test]
    fn standalone_export_is_typescript_project() {
        let m = manifest(ArtifactKind::Agentic);
        let files = export(&m, None);
        let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
        for want in [
            "index.html",
            "package.json",
            "serve.mjs",
            "README.md",
            "src/main.ts",
            "run.sh",
            "run.command",
            "biorouter-launch.sh",
        ] {
            assert!(paths.contains(&want), "export is missing {want}: {paths:?}");
        }
        assert!(file(&files, "package.json").contains("esbuild"));
    }

    /// The bug that made every export unusable: without a manifest the daemon
    /// 404s the app (`serve_index` / `agent_ws` both `load_manifest`), and
    /// `run.sh`'s install guard never flips.
    #[test]
    fn standalone_export_ships_a_manifest_so_the_daemon_can_resolve_the_app() {
        let m = manifest(ArtifactKind::Agentic);
        let files = export(&m, None);
        let parsed: Manifest = serde_json::from_str(file(&files, "manifest.json"))
            .expect("exported manifest.json must round-trip");
        assert_eq!(parsed.id, "demo");
        assert_eq!(parsed.entry, "index.html");
        assert!(parsed.agent.is_some());
    }

    /// Hard-coding `ws://127.0.0.1:3000` broke the app whenever the daemon was
    /// on another port (the desktop app picks an ephemeral one). The primary
    /// endpoint must now be page-origin-derived, with :3000 only as a fallback.
    #[test]
    fn standalone_export_derives_its_endpoint_from_the_page_origin() {
        let m = manifest(ArtifactKind::Agentic);
        let idx = export(&m, None)
            .iter()
            .find(|(p, _)| p == "index.html")
            .unwrap()
            .1
            .clone();
        assert!(
            idx.contains("\"endpoint\":null"),
            "primary endpoint must be unset"
        );
        assert!(
            idx.contains("ws://127.0.0.1:3000/apps/demo/agent"),
            "loopback should remain as a fallback"
        );
        assert!(idx.contains("\"endpoints\":["));
    }

    #[test]
    fn explicit_endpoint_still_wins_for_a_remote_daemon() {
        let m = manifest(ArtifactKind::Agentic);
        let files = export(&m, Some("wss://lab.example/apps/demo/agent"));
        let idx = file(&files, "index.html");
        assert!(idx.contains("wss://lab.example/apps/demo/agent"));
        assert!(!idx.contains("\"endpoints\":["));
    }

    /// `serve.mjs` used to be a bare static server, so `npm start` produced a UI
    /// with no backend. It must now proxy `/apps/**` (including the WS upgrade).
    #[test]
    fn serve_mjs_proxies_the_agent_socket_instead_of_only_serving_files() {
        let m = manifest(ArtifactKind::Agentic);
        let files = export(&m, None);
        let serve = file(&files, "serve.mjs");
        assert!(
            serve.contains("server.on(\"upgrade\""),
            "must proxy the WebSocket"
        );
        assert!(
            serve.contains("startsWith(\"/apps/\")"),
            "must proxy /apps/**"
        );
        assert!(serve.contains("ensureDaemon"), "must start/reuse a daemon");
        assert!(
            serve.contains("\"demo\""),
            "app id must be embedded as JSON"
        );
        // It fronts the daemon's auth-exempt /apps routes, so it must never
        // leave loopback — binding 0.0.0.0 would expose an unauthenticated agent.
        assert!(
            serve.contains(r#"server.listen(port, "127.0.0.1""#),
            "the dev server must bind loopback only"
        );
    }

    #[test]
    fn launcher_finds_the_daemon_and_fails_loudly_when_it_cannot() {
        let m = manifest(ArtifactKind::Agentic);
        let files = export(&m, None);
        let lib = file(&files, "biorouter-launch.sh");
        assert!(lib.contains("BIOROUTERD_BIN"));
        assert!(lib.contains("BioRouter.app/Contents/Resources/bin/biorouterd"));
        assert!(
            lib.contains("BIOROUTER_PORT="),
            "must set the daemon port env var biorouterd actually reads"
        );
        assert!(lib.contains("die \"biorouterd not found"));
        // And the runner opens the DAEMON's own origin, not a static server's.
        let run = file(&files, "run.sh");
        assert!(run.contains("open_url \"$BASE/apps/$APP_ID/\""));
        assert!(run.contains("verify_app"));
    }

    /// The export tests otherwise only match substrings. This actually *parses*
    /// the generated `serve.mjs` (ESM + top-level await) and shell launchers, so
    /// a template edit that produces a syntax error can't ship silently. Skips
    /// cleanly when `node`/`bash` aren't on PATH (e.g. a minimal CI image).
    #[test]
    fn generated_serve_and_launchers_are_syntactically_valid() {
        use std::io::Write;
        let m = manifest(ArtifactKind::Agentic);
        let files = export(&m, None);
        let dir = tempfile::tempdir().unwrap();

        let write = |name: &str| {
            let p = dir.path().join(name);
            let mut f = std::fs::File::create(&p).unwrap();
            f.write_all(file(&files, name).as_bytes()).unwrap();
            p
        };
        let serve = write("serve.mjs");
        let run = write("run.sh");
        let lib = write("biorouter-launch.sh");

        use std::process::Command;
        let has = |prog: &str| Command::new(prog).arg("--version").output().is_ok();

        if has("node") {
            // `node --check` infers ESM from the `.mjs` extension, so top-level
            // await parses. (`--input-type` is stdin/eval-only, not for a file.)
            let out = Command::new("node")
                .arg("--check")
                .arg(&serve)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "generated serve.mjs is not valid ESM:\n{}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        if has("bash") {
            for launcher in [&run, &lib] {
                let out = Command::new("bash")
                    .arg("-n")
                    .arg(launcher)
                    .output()
                    .unwrap();
                assert!(
                    out.status.success(),
                    "generated launcher {} is not valid bash:\n{}",
                    launcher.display(),
                    String::from_utf8_lossy(&out.stderr)
                );
            }

            // `bash -n` only checks syntax. The launcher runs under `set -u`, and
            // `find_biorouterd` once referenced `$BIOROUTERD_BIN` unguarded — so
            // with that var UNSET (the common case: biorouterd on PATH) the whole
            // launcher aborted with "unbound variable" and the app never opened,
            // while every substring test stayed green. Actually EXECUTE it under
            // `set -u`, var unset, with a stub `biorouterd` on PATH.
            let stub_dir = dir.path().join("stub");
            std::fs::create_dir_all(&stub_dir).unwrap();
            let stub = stub_dir.join("biorouterd");
            std::fs::write(&stub, "#!/bin/sh\nexit 0\n").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            let script = format!("set -euo pipefail\n. '{}'\nfind_biorouterd", lib.display());
            let out = Command::new("bash")
                .args(["-c", &script])
                .env_clear()
                .env("PATH", format!("{}:/usr/bin:/bin", stub_dir.display()))
                .env("HOME", dir.path())
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "find_biorouterd aborts under `set -u` with BIOROUTERD_BIN unset:\n{}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(
                String::from_utf8_lossy(&out.stdout).trim(),
                stub.display().to_string(),
                "find_biorouterd should locate the on-PATH biorouterd"
            );
        }
    }
}
