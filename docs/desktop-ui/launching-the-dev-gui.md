# Launching the dev GUI from a shell without a TTY

> **What this is.** The procedure for starting the Electron dev GUI from an agent
> shell, a CI step, or any other context without a TTY — and the four failure modes
> that make a working app look broken.
> **Status:** Current.
> **Audience:** agents driving the desktop app, and developers automating a GUI launch.

`just run-dev` is the right command **at a human terminal**. It does not survive being
launched without a TTY, and the ways it fails all look like application bugs rather
than launcher bugs. This page is the procedure that works and, more usefully, the four
wrong turns that each cost a debugging cycle, so the next session recognises them
instead of rediscovering them.

## The procedure

```bash
cd ui/desktop
source ../../bin/activate-hermit

# 1. Renderer dev server — MUST name the renderer config (see failure 3).
BIOROUTER_NO_HMR=1 npx vite --config vite.renderer.config.mts \
  --port 5173 --strictPort > /tmp/vite.log 2>&1 &

# 2. Electron against the built main process, detached from this shell.
#    `env -u ELECTRON_RUN_AS_NODE` is load-bearing (see failure 1).
env -u ELECTRON_RUN_AS_NODE -u ELECTRON_NO_ATTACH_CONSOLE \
  BIOROUTER_NO_HMR=1 MAIN_WINDOW_VITE_DEV_SERVER_URL=http://localhost:5173 \
  node_modules/electron/dist/Electron.app/Contents/MacOS/Electron \
  .vite/build/main.js --remote-debugging-port=9333 > /tmp/electron.log 2>&1 &
```

`.vite/build/main.js` must exist. If it does not, build it once with
`npx electron-forge start` (it will exit immediately — that is failure 2 — but
it leaves the bundles behind), or run `just run-dev` at a real terminal.

Then drive and inspect it over the Chrome DevTools Protocol (CDP) rather than by
screenshotting the desktop:

```text
agent_browser_connect  target: 9333
agent_browser_screenshot
agent_browser_console
agent_browser_eval
```

Verify it actually came up healthy:

```bash
pgrep -f "MacOS/Electron .vite"        # renderer process
pgrep -f "target/debug/biorouterd"     # the daemon the app spawned
curl -s http://localhost:9333/json/list | grep '"title"'   # -> "Biorouter"
```

## The four failures, and how to recognise each

**1. `ELECTRON_RUN_AS_NODE=1` in the environment — Electron exits instantly.**
Agent shells commonly export this. Electron then runs `main.js` as plain Node
and quits with no window and *no error*. Symptom: forge prints
`✔ Launched Electron app` and the command returns 0 seconds later. Always
`env -u ELECTRON_RUN_AS_NODE`. `ELECTRON_NO_ATTACH_CONSOLE` is worth clearing
alongside it.

**2. `electron-forge start` exits the moment stdin closes.** Forge reads stdin
for its interactive `rs` (restart) command, so `nohup … < /dev/null &` gives it
EOF and it tears the app down with it. Identical symptom to failure 1 —
`✔ Launched Electron app`, then nothing — which is what makes the two so easy to
confuse. Wrapping it in `script -q /dev/null` to fake a pty does **not** fix it.
Launch the Electron binary directly instead, as above.

**3. A bare `npx vite` serves the app with no CSS.** The renderer's Tailwind
plugin is configured in `vite.renderer.config.mts`; plain `npx vite` picks up
`vite.config.*` (absent here) and silently serves an unstyled page. Symptom: the
app *works* — sidebar entries, chat history, everything present and clickable —
but renders as unstyled HTML in serif type with no layout. It reads as "the app
is broken"; it is actually the launcher. Always pass
`--config vite.renderer.config.mts`.

**4. Do not verify with `screencapture` of the whole screen.** It captures
whatever the user has open — mail, browser history, private documents — and the
app window is usually *behind* the editor anyway, so the capture does not even
show what you need. `AXRaise`/`frontmost` via System Events is unreliable here.
Use the CDP screenshot: it captures the renderer only, needs no focus, and
cannot pick up the user's other windows.

## Two things that are NOT the problem

Both were suspected and ruled out with evidence, so don't re-litigate them:

- **Electron can open windows from an agent shell.** A minimal Electron app
  launched the same way fires `ready`, creates a `BrowserWindow` and stays
  alive. The window-server attachment is fine; the failures above are all
  environmental or configuration.
- **The staged binaries are usually fine.** `ui/desktop/src/bin/{biorouter,biorouterd}`
  being Linux ELF from a prior cross-build *is* a real failure mode (the app
  quits when the backend cannot exec), but check it with
  `file ui/desktop/src/bin/biorouterd` before assuming it — a healthy tree shows
  `Mach-O 64-bit executable arm64` and `--version` prints the workspace version.

## Three standing cautions

- `BIOROUTER_NO_HMR=1` freezes the renderer. Without it any save under
  `ui/desktop/src/` full-reloads the page and destroys the chat session under
  test, which makes agent-driven runs fail in ways that look like app bugs.
- Launching the dev GUI can overwrite `~/.config/biorouter`. Back it up, or
  sandbox with `XDG_CONFIG_HOME`, before a session that will touch settings.
- Pick a CDP port that is not 9222 — that is usually the developer's own Chrome,
  and a debugger client will silently attach to it instead of the app.

## Related documentation

- [Debugging the dev GUI with agent-browser](agent-browser-debugging.md) — driving the app over CDP once it is running, and why this repo uses port 9333
- [Diverge behavior checklist](diverge-behavior-checklist.md) — what to exercise once the app is in front of you
- [System overview](../architecture/system-overview.md) — how the Electron main process, the daemon and the renderer fit together
