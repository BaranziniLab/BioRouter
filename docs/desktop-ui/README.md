# Desktop UI testing and debugging

This folder covers exercising the BioRouter Electron desktop app as a running
program: how to launch the dev GUI and drive it from a terminal, and what behavior to
check once it is in front of you. Come here when you need to reproduce a desktop bug by
clicking through the real app, when you are about to run a manual QA pass on a chat
surface, or when you are writing automated tests and need the behavioral spec they
encode.

This is not where the desktop UI is designed or explained. Visual and interaction
design explorations live in `docs/design/` (theme studios, the home-screen and
UI-cohesion redesigns); how the Electron main process, daemon, and renderer fit
together is in [the architecture overview](../architecture/system-overview.md); the
terminal surfaces — the CLI and the TUI — have their own guides and QA script in
`docs/cli/`; and removed desktop features are archived under `docs/history/`. If you
arrived looking for one of those, leave now.

## Documents

| Document | What it covers |
|----------|----------------|
| [Debugging the dev GUI with agent-browser](agent-browser-debugging.md) | How to drive the Electron dev GUI from an ordinary terminal with the `agent-browser` CLI over the Chrome DevTools Protocol, and why this repo exposes that protocol on port 9333 rather than the Playwright default of 9222. Current. |
| [Diverge behavior checklist](diverge-behavior-checklist.md) | A catalog of 68 user actions for Diverge — the feature that branches a conversation into a new session — each paired with the behavior BioRouter must exhibit, serving as both a manual QA script and the spec the automated tests encode. Current; last revised 2026-07-18, when the dashboard-canvas items were deleted alongside dashboard mode itself. |

The two documents are meant to be used together: the checklist marks each item **[T]**
(covered by an automated test) or **[UI]** (requires driving the real app), and the
agent-browser guide is how you drive the app for every `[UI]` item.

## Related documentation

- [CLI QA checklist](../cli/qa-checklist.md) — the sibling manual test script covering the terminal surfaces, where this folder covers the desktop ones.
- [Environment variables](../configuration/environment-variables.md) — reference for `PLAYWRIGHT_CDP_PORT`, `BIOROUTER_PATH_ROOT`, and the other knobs the dev-GUI launch depends on.
- [Diagnostics and bug reports](../troubleshooting/diagnostics-and-bug-reports.md) — what to collect once you have reproduced a GUI failure here.
- [Dashboard mode history](../history/dashboard-mode/README.md) — the archived removal record for the desktop feature whose items were deleted from the Diverge checklist.
