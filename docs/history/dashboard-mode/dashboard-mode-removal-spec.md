# Dashboard mode — removal record (2026-07-18)

Status: **done**. Dashboard mode has been removed from the desktop app. This is
the record of what it was, why it went, exactly what came out, and what was
deliberately kept.

Companion historical documents — **do not rewrite them**, they remain true of
their own moment:

- [`../plans/2026-05-10-dashboard-mode.md`](../plans/2026-05-10-dashboard-mode.md)
  and [`2026-05-10-dashboard-mode-design.md`](2026-05-10-dashboard-mode-design.md)
- [`../plans/2026-05-10-canvas-dashboard.md`](../plans/2026-05-10-canvas-dashboard.md)
  and [`2026-05-10-canvas-dashboard-design.md`](2026-05-10-canvas-dashboard-design.md)
- [`../plans/2026-05-29-dashboard-fold-mode.md`](../plans/2026-05-29-dashboard-fold-mode.md)
  and [`2026-05-29-dashboard-fold-mode-design.md`](2026-05-29-dashboard-fold-mode-design.md)
- `docs/design/chat-groups-plan.md`, `docs/design/EXECUTION.md` — the successor design
- `docs/release-notes/v1.76.0.md` — the release that shipped dashboard **fold mode** as its
  headline. Dashboard mode itself shipped in v1.75.0, which has no release-notes file;
  the `landing/about.html` changelog is the record for it.

## What dashboard mode was

A free-floating **canvas** at the `/dashboard` route on which the user could
spawn many chat "boxes" at once: draggable, resizable, foldable cards, each one
a full chat session with its own agent, laid out on an infinite pannable
surface. It shipped in v1.75.0 (with fold mode following in v1.76.0) and grew a
supporting cast: a layout/packing engine, pixel snapping, a colour palette per
box, keyboard shortcuts, localStorage persistence of box geometry, and a
window-maximizing IPC pair so entering the canvas gave it the whole screen.

## Why it was removed

It was **superseded by tabs, chat groups and split panes**. The canvas answered
"I want several conversations visible at once", and tabs + split panes answer
the same need inside the normal window chrome: no separate route, no bespoke
layout engine, no second persistence format, no mode to enter and exit, and the
same conversations reachable from History rather than only from a canvas that
had to be hydrated. Keeping both meant two parallel multi-chat surfaces with
divergent behaviour — most visibly in Diverge, which had to branch on
canvas-vs-chat and spawn a *box* in one case and an Electron *window* in the
other. That branch was the source of a real isolation bug and a real data-loss
bug, both of which simply cease to exist with one surface.

## Exactly what was removed

| Thing | Detail |
|---|---|
| Component tree | All of `ui/desktop/src/components/Dashboard/` — `DashboardBoard`, `DashboardRoute`, `DashboardToolbar`, `ChatWindow`, `WindowTitleBar`, `FoldedCard`, `ResizeHandle`, `useDashboardDrag`, `canvasLayout`, `dashboardShortcuts`, `dashboardStorage`, `palette`, `pixelSnap`, plus the already-dead `layoutEngine` and every co-located test (~5,000 lines) |
| Two React contexts | `contexts/DashboardContext.tsx` and `contexts/DashboardCanvasContext.tsx` |
| Route + provider | The `/dashboard` route and the app-wide `DashboardProvider` wrapper in `App.tsx` |
| Entry points | The two remaining "Add to dashboard" actions — the session-row menu in `SessionListView.tsx` and the workflow-card menu in `WorkflowsView.tsx` (the titlebar control had gone earlier) |
| Electron IPC | The `dashboard:enter` / `dashboard:exit` handler pair in `main.ts`, its `preDashboardBounds` window-bounds map, the `dashboardEnter` / `dashboardExit` preload bridge + types, and the matching no-op stubs in `renderer.tsx` |
| CSS | The `body.dashboard-route-active` rule in `styles/main.css` and the class toggle that set it in `Dashboard/DashboardRoute.tsx` |
| localStorage | Three keys no longer written: `biorouter.dashboard.v2`, `biorouter.dashboard.v1`, `biorouter.labmeeting.v1` |

**The Rust backend contained zero dashboard-mode code.** Dashboard mode was
purely a renderer-side arrangement of sessions the daemon already served; no
crate, route, schema or migration referenced it, so nothing on the backend
changed.

## What was deliberately kept, and why

- **`SessionNamePill`** was *relocated*, not deleted: it moved from
  `components/Dashboard/` to `components/SessionNamePill.tsx` because `BaseChat`
  renders it on **every** chat surface — it was never canvas-specific, it just
  happened to have been written there first.
- **`useDiverge` was simplified, not deleted.** Diverge is very much alive; only
  its canvas branch came out. The hook now unconditionally calls
  `window.electron.createDivergedChatWindow(...)`, which is what the chat path
  always did. See [`../../diverge-behavior-checklist.md`](../../diverge-behavior-checklist.md)
  for the post-removal behaviour spec.
- **`ResetPanel` still clears the three legacy localStorage keys.** Nothing
  writes them any more, but installs that ran an older build still carry the
  payload; Reset stays responsible for clearing it so upgrading installs do not
  keep an orphaned blob forever. Drop that list once those versions are out of
  circulation (`DISCONTINUED_DASHBOARD_KEYS` in
  `ui/desktop/src/components/settings/app/ResetPanel.tsx`).

## Unrelated things named "dashboard" — untouched

Two live features share the word and have nothing to do with the removed UI
mode. Do not "clean them up":

- The **Auto Visualiser `render_dashboard` tool** — the composite multi-figure
  report artifact (`crates/biorouter-mcp/src/autovisualiser/tools_dashboard.rs`,
  `docs/autovis-stress-test/*`).
- The **Agent Drafter `dashboard` app archetype** — a KPI-grid starter for
  generated BioRouter apps (`docs/agent-drafter-apps.md`,
  `docs/apps-sdk-reference.md`).

Likewise "provider dashboard" / "billing dashboard" in provider docs means the
LLM vendor's own web console.
