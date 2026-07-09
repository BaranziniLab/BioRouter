# Debugging the dev GUI with agent-browser

[agent-browser](https://github.com/vercel-labs/agent-browser) is a fast native
(Rust) CLI that drives any Chromium browser — including Electron apps — over the
Chrome DevTools Protocol (CDP). Because BioRouter's desktop app is Electron, the
dev GUI already exposes a CDP port, so agent-browser can snapshot, click, type,
read the console, eval JS, and screenshot it from a normal terminal.

It is an alternative to the bundled `playwright-electron` MCP server. The key
advantage for this repo: agent-browser targets a CDP port **per command**, so it
is **not** subject to the "MCP endpoint is cached at session start and can't be
re-pointed" limitation that pins `playwright-electron` to port 9222.

## One-time install

```bash
npm install -g agent-browser     # ships a native arm64/x64 binary via postinstall
agent-browser --version          # 0.30.x
```

No `agent-browser install` (Chrome download) is needed — we connect to the
Electron app's own Chromium, not a standalone browser.

## Why a dedicated port (9333, not 9222)

The Playwright default port is **9222**, but on this machine a regular Google
Chrome is often already listening on 9222. If the Electron app loses the race to
bind 9222, agent-browser silently connects to Chrome instead (you'll see
Google/YouTube tabs, not BioRouter). The `agent-browser-ui` workflow therefore
exposes CDP on **9333** via `PLAYWRIGHT_CDP_PORT`, which `ui/desktop/src/main.ts`
honors. (See the `dev-gui-cdp-port-conflict` note.)

## Launch + drive

Terminal 1 — build the debug backend and launch the dev GUI with CDP on 9333,
config sandboxed under an isolated `BIOROUTER_PATH_ROOT` (so the dev app can't
clobber `~/.config/biorouter`):

```bash
just agent-browser-ui          # or: just agent-browser-ui 9444  to override the port
```

Terminal 2 — connect once, then interact:

```bash
agent-browser connect 9333         # binds this session to the dev app's CDP
agent-browser snapshot -i          # accessibility snapshot with refs (@e1, @e2, ...)
agent-browser click @e5            # interact by ref
agent-browser fill @e3 "hello"
agent-browser screenshot ui.png    # visual state
agent-browser console --json       # renderer console + errors
agent-browser errors               # just the errors
agent-browser eval "window.location.href"
agent-browser close                # detach (does NOT quit the app)
```

Re-run `agent-browser snapshot -i` after any navigation or state change to get
fresh refs. `agent-browser skills get electron` and `agent-browser skills get
core --full` print the canonical workflows and full command reference.

If the app was already running on the wrong port, quit it and relaunch — the
`--remote-debugging-port` switch is only read at startup.

## MCP server (optional, future sessions)

`.mcp.json` also registers an `agent-browser` MCP server
(`agent-browser mcp --tools all`) so the harness can call the tools directly. As
with any MCP server, it is loaded at session start; use the `connect`/`cdp` tools
at runtime to point it at port 9333.
