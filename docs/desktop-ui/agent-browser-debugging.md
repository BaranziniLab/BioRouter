# Debugging the dev GUI with agent-browser

> **What this is.** A guide to driving the BioRouter Electron dev GUI from an ordinary
> terminal using the `agent-browser` CLI over the Chrome DevTools Protocol, and an
> explanation of why this repo exposes that protocol on port 9333 rather than the
> Playwright default.
> **Status:** Current.
> **Audience:** developers working on the desktop GUI, and agents driving it.

Reproducing a desktop UI bug means clicking through the real app. This repo ships a
bundled `playwright-electron` MCP (Model Context Protocol) server for that, but its
endpoint is resolved once at session start and cannot be re-pointed afterwards, which
pins it to port 9222. `agent-browser` avoids that constraint: it targets a Chrome
DevTools Protocol (CDP) port **per command**, so you can point it wherever the dev app
actually landed.

[agent-browser](https://github.com/vercel-labs/agent-browser) is a fast native (Rust)
CLI that drives any Chromium browser — including Electron apps — over CDP. Because
BioRouter's desktop app is Electron, the dev GUI already exposes a CDP port, so
agent-browser can snapshot, click, type, read the console, eval JS, and screenshot it.

## One-time install

```bash
npm install -g agent-browser     # ships a native arm64/x64 binary via postinstall
agent-browser --version          # confirm the binary installed
```

This guide was written against agent-browser 0.30.x.

You do not need `agent-browser install` (the Chrome download step) — agent-browser
attaches to the Electron app's own Chromium, not to a standalone browser.

## Why a dedicated port (9333, not 9222)

The Playwright default port is **9222**, but a regular Google Chrome is often already
listening there. If the Electron app loses the race to bind 9222, agent-browser
silently connects to Chrome instead, and you will see Google or YouTube tabs rather
than BioRouter. The `agent-browser-ui` workflow therefore exposes CDP on **9333** via
`PLAYWRIGHT_CDP_PORT`, which `ui/desktop/src/main.ts` honors.

## Launch and drive the app

Terminal 1 — build the debug backend and launch the dev GUI with CDP on 9333, with
config sandboxed under an isolated `BIOROUTER_PATH_ROOT` so the dev app cannot clobber
`~/.config/biorouter`:

```bash
just agent-browser-ui          # or: just agent-browser-ui 9444  to override the port
```

> **Warning.** Set `BIOROUTER_NO_HMR=1` for this workflow. It freezes the renderer — no
> Vite watching, no hot reload. Without it, any save anywhere under `ui/desktop/src/`
> full-reloads the page and destroys the chat session under test, which makes
> agent-browser runs fail in ways that look like app bugs.

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

Re-run `agent-browser snapshot -i` after any navigation or state change to get fresh
refs. `agent-browser skills get electron` and `agent-browser skills get core --full`
print the canonical workflows and the full command reference.

If the app was already running on the wrong port, quit it and relaunch — the
`--remote-debugging-port` switch is only read at startup.

## Optional: the agent-browser MCP server

`.mcp.json` also registers an `agent-browser` MCP server (`agent-browser mcp --tools
all`) so a harness can call the tools directly. As with any MCP server, it is loaded at
session start; use the `connect` or `cdp` tools at runtime to point it at port 9333.

## Related documentation

- [Diverge behavior checklist](diverge-behavior-checklist.md) — the desktop QA script whose `[UI]` items you drive with exactly this setup.
- [Environment variables](../configuration/environment-variables.md) — reference for `PLAYWRIGHT_CDP_PORT`, `BIOROUTER_PATH_ROOT`, and the other knobs used above.
- [Diagnostics and bug reports](../troubleshooting/diagnostics-and-bug-reports.md) — what to collect once you have reproduced a GUI failure.
- [Common problems and fixes](../troubleshooting/common-problems-and-fixes.md) — check here before assuming a dev-GUI symptom is a new bug.
