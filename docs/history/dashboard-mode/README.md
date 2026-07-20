# Dashboard mode — removal record and archive index

This folder holds the complete paper trail of **dashboard mode**, a desktop-app
feature that let a user spawn many chat windows at once on a free-floating
canvas. It contains the four generations of design specs and implementation
plans that built it, and — in this file — the record of its removal. Nothing
here describes a feature that still exists; the whole folder is an archive.

**Date of removal record:** 2026-07-18
**Status:** Historical record — dashboard mode was removed from the desktop app
on 2026-07-18. The `/dashboard` route, both React contexts, the entire
`ui/desktop/src/components/Dashboard/` tree, the Electron IPC pair and the
body-class CSS are all deleted. This file's forward-looking guidance — what was
deliberately kept, and what shares the name but must not be cleaned up — is
still load-bearing.
**Audience:** maintainers working on the BioRouter desktop UI.

> **Version key.** `v1`–`v4` in the filenames are this archive's own lineage
> numbering for the four generations of the feature, not release versions. They
> run: v1 Lab Meeting Mode → v2 Dashboard Mode (a rename plus rework) → v3
> infinite canvas → v4 window fold mode.

## Files in this folder

| File | What it is |
|---|---|
| [v1 — Lab Meeting Mode design spec](v1-lab-meeting-mode-design.md) | The original design: a multi-conversation board at `/lab-meeting` with draggable, resizable windows and a tile / overflow / tuck layout engine. |
| [v1 — Lab Meeting Mode implementation plan](v1-lab-meeting-mode-plan.md) | The 20-task plan that built that route, its provider and its 18-file component tree. |
| [v2 — Dashboard Mode design spec](v2-dashboard-mode-design.md) | Renames Lab Meeting Mode to Dashboard Mode and reworks sizing, focus-pop, session naming, per-window pickers and the layout engine. |
| [v2 — Dashboard Mode implementation plan](v2-dashboard-mode-plan.md) | The 12-task plan for the end-to-end rename plus the deterministic soft-tile layout engine and `Session N` default naming. |
| [v3 — Canvas dashboard design spec](v3-infinite-canvas-design.md) | Turns the board into an infinite pannable canvas, deletes the tuck sidebar, and adds shrink / enlarge window chrome. |
| [v3 — Canvas dashboard implementation plan](v3-infinite-canvas-plan.md) | The 13-task plan for camera offsets, spiral spawn placement and the vertical picker popup. |
| [v4 — Dashboard fold mode design spec](v4-window-fold-mode-design.md) | Collapsing chat windows into compact 240×72 cards, individually or all at once, with a live busy indicator and a muted accent palette. |
| [v4 — Dashboard fold mode implementation plan](v4-window-fold-mode-plan.md) | The 13-task plan for the fold actions, the `FoldedCard` component and the `onBusyChange` prop on `BaseChat`. |
| [Dashboard mode removal specification](dashboard-mode-removal-spec.md) | The specification the removal was carried out from, kept for provenance. This index carries the same removal record in conformed prose; the two disagree about which releases shipped dashboard mode and fold mode, and neither has been corrected. |
| [Boot splash design](2026-07-18-boot-splash-design.md) | The centred `BR` monogram that assembles itself over a theme-correct ground while the backend starts. Written and built alongside the removal, on the same branch; it replaced a loader that never covered the slow case and was broken in dark mode. Not a dashboard-mode document — it is filed here because it shipped with this work. |

The four specs and four plans remain true of their own moment. **Do not rewrite
them** to match current reality; they are dated records, and each one carries a
header saying what superseded it.

## What dashboard mode was

A free-floating **canvas** at the `/dashboard` route on which the user could
spawn many chat "boxes" at once: draggable, resizable, foldable cards, each one
a full chat session with its own agent, laid out on an infinite pannable
surface. It shipped in v1.76.0 (with fold mode following in v1.85.3) and grew a
supporting cast: a layout/packing engine, pixel snapping, a colour palette per
box, keyboard shortcuts, localStorage persistence of box geometry, and a
window-maximizing IPC pair so entering the canvas gave it the whole screen.

## Why it was removed

It was **superseded by tabs, chat groups and split panes**. The canvas answered
"I want several conversations visible at once", and tabs + split panes answer
the same need inside the normal window chrome: no separate route, no bespoke
layout engine, no second persistence format, no mode to enter and exit, and the
same conversations reachable from History rather than only from a canvas that
had to be hydrated.

Keeping both meant two parallel multi-chat surfaces with divergent behaviour —
most visibly in Diverge, which had to branch on canvas-vs-chat and spawn a *box*
in one case and an Electron *window* in the other. That branch was the source of
a real isolation bug and a real data-loss bug, both of which simply cease to
exist with one surface.

> **Note.** Those two bugs are asserted here without issue numbers or links; the
> removal record did not cite them, so they cannot be looked up from this
> document.

## Exactly what was removed

| Thing | Detail |
|---|---|
| Component tree | All of `ui/desktop/src/components/Dashboard/` — `DashboardBoard`, `DashboardRoute`, `DashboardToolbar`, `ChatWindow`, `WindowTitleBar`, `FoldedCard`, `ResizeHandle`, `useDashboardDrag`, `canvasLayout`, `dashboardShortcuts`, `dashboardStorage`, `palette`, `pixelSnap`, plus the already-dead `layoutEngine` and every co-located test (~5,000 lines) |
| Two React contexts | `contexts/DashboardContext.tsx` and `contexts/DashboardCanvasContext.tsx` |
| Route + provider | The `/dashboard` route and the app-wide `DashboardProvider` wrapper in `App.tsx` |
| Entry points | The two remaining "Add to dashboard" actions — the session-row menu in `SessionListView.tsx` and the workflow-card menu in `WorkflowsView.tsx` (the titlebar control had gone earlier) |
| Electron IPC | The `dashboard:enter` / `dashboard:exit` handler pair in `main.ts`, its `preDashboardBounds` window-bounds map, and the `dashboardEnter` / `dashboardExit` preload bridge + types |
| CSS | The `body.dashboard-route-active` rule in `styles/main.css` and the class toggle in `renderer.tsx` |
| localStorage | Three keys no longer written: `biorouter.dashboard.v2`, `biorouter.dashboard.v1`, `biorouter.labmeeting.v1` |

**The Rust backend contained zero dashboard-mode code.** Dashboard mode was
purely a renderer-side arrangement of sessions the daemon already served; no
crate, route, schema or migration referenced it, so nothing on the backend
changed.

## What was deliberately kept, and why

- **`SessionNamePill`** was *relocated*, not deleted: it moved from
  `components/Dashboard/` to `ui/desktop/src/components/SessionNamePill.tsx`
  because `BaseChat` renders it on **every** chat surface — it was never
  canvas-specific, it just happened to have been written there first.
- **`useDiverge` was simplified, not deleted.** Diverge is very much alive; only
  its canvas branch came out. The hook now unconditionally calls
  `window.electron.createDivergedChatWindow(...)`, which is what the chat path
  always did. See the
  [Diverge behavior checklist](../../desktop-ui/diverge-behavior-checklist.md)
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
  report artifact (`crates/biorouter-mcp/src/autovisualiser/tools_dashboard.rs`;
  exercised in the [Auto Visualiser stress test](../autovis-stress-test/README.md)).
- The **Agent Drafter `dashboard` app archetype** — a KPI-grid starter for
  generated BioRouter apps (see the
  [Agent Drafter apps platform design](../../agent-drafter/apps-platform-design.md)
  and the [Apps SDK reference](../../apps-sdk/sdk-reference.md)).

Likewise "provider dashboard" / "billing dashboard" in provider docs means the
LLM vendor's own web console.

## Related documentation

- [Chat groups: design judgement and reduced plan](../chat-groups/design-judgement-and-plan.md) — the successor design; the tabs-in-a-group work that made the canvas redundant.
- [UI overhaul — execution status](../../design/ui-overhaul/execution-status.md) — the status record for the UI cohesion and chat-groups branch that carried the removal.
- [Diverge behavior checklist](../../desktop-ui/diverge-behavior-checklist.md) — the post-removal spec for the one feature that had to branch on canvas-vs-chat.
- [v1 — Lab Meeting Mode design spec](v1-lab-meeting-mode-design.md) — where the lineage starts, if you want the original shape of the idea.

The release that shipped dashboard mode as a headline feature was v1.76.0; its
notes live under `docs/release-notes/` in the repository that carries them.
