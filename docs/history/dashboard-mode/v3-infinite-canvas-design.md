# Canvas dashboard — design spec (v3)

> **What this is.** The design for turning the dashboard board into an
> **infinite pannable canvas**: deleting the tuck sidebar, adding shrink /
> enlarge window chrome, and replacing the inline picker expansion with a
> vertical popup.
> **Status:** Superseded, and then removed. Fold mode
> ([v4 — Dashboard fold mode design spec](v4-window-fold-mode-design.md)) built
> on the canvas described here, and the entire dashboard was deleted on
> 2026-07-18 — `canvasLayout`, `DashboardCanvasContext` and the `/dashboard`
> route are all gone, and the canvas branch was collapsed out of `useDiverge`.
> See the [removal record](README.md).
> **Audience:** maintainers reading the dashboard-mode archive.
> **Lineage note.** The original status line below calls its predecessor "the v1
> dashboard mode". That document calls *itself* the v2 rework of Lab Meeting
> Mode. Both are right about different things — it was the first spec to use the
> name "Dashboard Mode", and the second generation of the feature. This archive
> numbers the whole lineage consistently: v1 Lab Meeting Mode → v2 Dashboard
> Mode → **v3 canvas (this file)** → v4 fold mode.

> **Original status line:** Approved 2026-05-10. Updates the dashboard mode
> defined in [v2 — Dashboard Mode design spec](v2-dashboard-mode-design.md).

## Goal

Turn the dashboard from a fixed-area "tile + tuck" board into an **infinite
canvas** the user can pan. Remove tucking entirely. Spawn windows non-overlapping
relative to existing windows. Give each window quick **shrink / enlarge** chrome
plus a vertical popup for secondary picker controls so narrow windows stay
usable.

## Non-goals

- Backend changes. The backend already auto-generates session names after the
  first user message — we just keep wiring the existing `syncSessionName` event.
- A full-screen mini-map or zoom controls. Pan only; zoom stays fixed at 1.0 for
  this iteration.
- Smooth follow-the-cursor camera. Camera moves only on (a) explicit pan,
  (b) spawn, (c) Organize click.

## Architecture

The dashboard becomes a **viewport + world**:

- **World**: an unbounded coordinate plane on which windows live at absolute
  `(x, y, w, h)`.
- **Viewport**: the DOM container the user sees. It scrolls the world via a
  `cameraOffset = { x, y }` applied as a CSS `translate` to the world layer.
- **Spawn**: new window placed at a non-overlapping point near the current
  camera center; after placement, camera re-centers on the new window.
- **Organize**: iterative spread that resolves overlaps with minimal movement,
  preserves window sizes, recenters camera on focused window.
- **Tucking**: removed. `isTucked` and `TuckSidebar` are deleted. No sidebar.
- **Window chrome**: title bar gains a Shrink button and an Enlarge button left
  of the existing Close button. Order: `Shrink | Enlarge | Close`.
- **Picker collapse**: the `>` chevron now opens a vertical pop-up list above
  the toolbar (instead of inline horizontal expansion). Send button stays put.

```text
┌──── Viewport (overflow:hidden, drag-to-pan) ─────────────────────┐
│  ┌── World layer (translate(camera.x, camera.y)) ──────────────┐ │
│  │   ┌─ Window A ─┐    ┌─ Window B ─┐                          │ │
│  │   │            │    │            │                          │ │
│  │   └────────────┘    └────────────┘                          │ │
│  │            ┌─ Window C ─┐                                   │ │
│  │            │            │                                   │ │
│  │            └────────────┘                                   │ │
│  └──────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────┘
```

## Components and responsibilities

### `DashboardContext` (modify)

- Add `cameraOffset: { x: number; y: number }`.
- Remove `T1`, `T2`, and `isHydrating` clutter related to tuck thresholds.
- Remove `tuckWindow`, `evokeWindow`.
- Add `panBy(dx, dy)`, `centerOn(windowId)`.
- `DashboardWindow.position` becomes **non-nullable**; every window owns its
  world coordinates from spawn onward. (`size` remains in world units.)
- Drop `DashboardWindow.isTucked`.

### `DashboardProvider` (modify)

- **Spawn**: deterministic placement.
    1. Pick a starting point `P` = camera-center in world coords.
    2. While `P` collides with any existing window: spiral outward by
       `MIN_WINDOW_W + GAP` until clear.
    3. Place the window at `P` with size `(MIN_WINDOW_W, MIN_WINDOW_H)`.
    4. `centerOn(newWindow)`: set `cameraOffset` so the new window's center
       maps to the viewport center.
- **closeWindow**, **focusWindow**, **renameWindow**, **moveWindow**,
  **resizeWindow**, **syncSessionName**, **markActivity**: unchanged in shape;
  drop references to `isTucked`.
- **organize**: minimal non-overlap pass. See "Organize algorithm" below.
- **clearAll**: unchanged.
- **localStorage migration**: bump key from `biorouter.dashboard.v1` to
  `biorouter.dashboard.v2`. v1 records are migrated by dropping `isTucked` and
  defaulting `cameraOffset` to `{ x: 0, y: 0 }`. v1 windows with
  `isManuallyPlaced=false` get an initial spiral placement on load.

### Canvas pan (new behavior in `DashboardBoard`)

- **Mouse drag**: pointerdown on viewport **background** (`e.target ===
  viewportEl` or the dotted-grid layer — not on any window) starts pan. Track
  `(dx, dy)` and write to `cameraOffset`.
- **Trackpad scroll**: listen to `wheel` event on viewport. If
  `ev.deltaMode === 0` (pixel) and the user is two-finger-scrolling, decrement
  `cameraOffset.x` by `ev.deltaX`, same for `y`. Call `ev.preventDefault()` to
  prevent the page from scrolling. Don't apply scroll if the wheel event
  originated inside a window (chat needs its own scroll).
- Pan grabs cursor: viewport background shows `cursor: grab`, active drag shows
  `cursor: grabbing`.

### Organize algorithm

Constraints: preserve window sizes; move windows the **minimum** distance to
eliminate overlap; recenter camera on the focused window after.

Approach: iterative relaxation, keeping the focused (or oldest, if none
focused) window anchored.

> **Note.** The block below is language-neutral pseudocode, not the shipping
> implementation. It uses Python-style `for … in 1..N:` loops; the real module
> is TypeScript (`canvasLayout.ts`).

```text
function organize(windows, focusedId):
    pin = focusedId or windows[0].id
    for pass in 1..MAX_PASSES (e.g. 8):
        moved = false
        for each pair (a, b) of windows where a ≠ b:
            ov = overlapRect(a, b)
            if ov.w > 0 and ov.h > 0:
                # push the smaller-area window away from the larger
                # along the shorter axis of overlap
                axis = ov.w < ov.h ? 'x' : 'y'
                push = (ov.w if axis=='x' else ov.h) / 2 + GAP/2
                moveAlongAxis(a, b, axis, push, anchorId=pin)
                moved = true
        if not moved: break
    centerOn(pin)
```

`moveAlongAxis(a, b, axis, push, anchorId)`: if `a.id == anchorId`, move only
`b` by `2*push`; if `b.id == anchorId`, move only `a` by `2*push`; otherwise
move each by `push` in opposite directions.

Edge cases:
- All windows already non-overlapping: no movement, but camera still recenters
  on focused window.
- A pinned window dragged off-canvas: no clamp — the canvas is infinite.

### Window chrome — Shrink / Enlarge / Close

`WindowTitleBar` accepts two new callbacks:

- `onShrink()` — resize to `(MIN_WINDOW_W, MIN_WINDOW_H)` from current top-left.
- `onEnlarge()` — resize to `(940, 800)` from current top-left, the
  `COMFORT_W × COMFORT_H` constants already in `layoutEngine.ts`.

`ChatWindow.tsx` wires those callbacks to `dashboard.resizeWindow(...)` using
the current position. Buttons render as small icon buttons matching the close-X
style; placed in this order, right-aligned: **Shrink | Enlarge | Close**.

Icons: `Minimize2` and `Maximize2` from lucide-react (or app-icons re-export).

### Picker collapse — vertical popup

`ChatInput.tsx` changes the expanded-state rendering:

- Inline horizontal expansion (current behavior) → replaced by a vertical
  pop-up positioned above the `>` button.
- The pop-up uses Radix `Popover` (already a dependency via `@radix-ui/*`) so
  Portal + outside-click dismiss are free.
- Pop-up width is fixed (~240px); height grows with content. Items stack
  vertically: Cost / Model picker / Mode picker / Workflow / Diagnostics.
- All controls remain fully functional inside the pop-up.
- Send button no longer disappears at narrow widths because the expanded
  group never claims horizontal row space.

### Toolbar style

`DashboardToolbar` Spawn / Organize / Clear buttons:

- Lose `border border-border-subtle`, `bg-background-default`, and the rounded
  pill outline.
- Become text buttons matching the sidebar tabs (Home/Chat/History): plain
  text + icon, `text-text-default/80`, on hover `text-text-default` +
  `bg-background-medium/40`, no ring or border.

## Spawn placement details

```text
GAP = 16
MIN_WINDOW_W = 520
MIN_WINDOW_H = 440   // bumped from 360 to fit the 4 popular-topic cards
SPIRAL_STEP_X = MIN_WINDOW_W + GAP
SPIRAL_STEP_Y = MIN_WINDOW_H + GAP

# Centered at camera center; spiral in 8 directions until clear.
candidates = [
    (0, 0), (1, 0), (1, 1), (0, 1), (-1, 1),
    (-1, 0), (-1, -1), (0, -1), (1, -1),
    (2, 0), (2, 1), ... (expanding ring)
]
```

## State migration

`dashboardStorage.ts`:

- `STORAGE_KEY` bumps from `biorouter.dashboard.v1` to `biorouter.dashboard.v2`.
- On load: if `v1` is found, transform each window:
    - drop `isTucked`
    - if `position == null`, assign via spiral placement
    - if `size == null`, default to `(MIN_WINDOW_W, MIN_WINDOW_H)`
- Default `cameraOffset = { x: 0, y: 0 }`.

## Window minimum-size enforcement

`ResizeHandle` already clamps to `minSize` at drag-end. Keep that. `Shrink`
calls `resizeWindow(MIN_WINDOW_W, MIN_WINDOW_H)`. `Enlarge` calls
`resizeWindow(940, 800)`. Both are unconditional (no spring-back checks needed
since values are exact).

## Session auto-rename

The backend already calls `provider.generate_session_name(&conversation)` after
the first user message (see
[`session_manager.rs`](../../../crates/biorouter/src/session/session_manager.rs),
at line 347 as of this spec's date).
The frontend already invokes `dashboard.syncSessionName(...)` from
`ui/desktop/src/components/Dashboard/ChatWindow.tsx` (line 160 at the time) on
the `onSessionUpdate` callback; that file has since been deleted with the rest
of the dashboard tree. Verify the pipeline still fires after our changes — no
new code needed unless tests reveal a regression.

## Removals

Each of these existed only to serve the tuck system that this spec deletes. The
`T1` / `T2` thresholds are defined in the
[v1 design spec, §7.1](v1-lab-meeting-mode-design.md#71-threshold-definitions):
`T1` was the grid limit, `T2` the board limit beyond which windows were tucked
into a side panel.

- `TuckSidebar.tsx`, `TuckedCard.tsx`, `HiddenChatHolder.tsx` — delete. These
  rendered the side panel, one card per tucked window, and the off-board holder
  that kept tucked chats alive.
- `DashboardWindow.isTucked` — remove. The per-window flag for "this chat is in
  the sidebar, not on the board".
- `DashboardApi.tuckWindow`, `evokeWindow` — remove. The two actions that moved
  a window into and back out of the sidebar.
- `DashboardState.T1`, `T2` — remove. The two capacity thresholds.
- `DashboardApi.setT1`, `setT2` — remove. Their toolbar setters.
- `enforceT2Pure` helper in `DashboardProvider` — remove. The pure function that
  tucked the oldest non-focused windows whenever the board exceeded `T2`.
- `onTuckByDrag` plumbing in `ChatWindow` and `DashboardBoard` — remove. The
  drop-zone handling that tucked a window dragged into the board's right edge.

## Testing strategy

Unit tests (Vitest):
- `dashboardStorage.test.ts`: v1 → v2 migration drops `isTucked`, assigns spiral
  positions, defaults `cameraOffset`.
- `DashboardProvider.test.tsx`: spawn places non-overlapping; organize resolves
  overlaps without resizing; closeWindow / focusWindow unchanged behavior.
- New `organize.test.ts`: pinned-window anchoring, minimal movement.

Manual via Playwright debugger (final check):
- Spawn 6+ windows, verify no overlap and camera centers on each new spawn.
- Resize one window to overlap a neighbor; click Organize; verify they separate
  without changing size and camera recenters on focused window.
- Pan canvas via mouse drag and trackpad scroll.
- Toggle `>` popup; verify Send remains visible and clicking outside dismisses.
- Click Shrink / Enlarge; verify sizing.

## Resolved questions

No open questions remained at approval. The clarifying questions raised in
design review were answered as follows:

| Topic | Decision |
|---|---|
| Canvas pan input | Mouse drag on background **and** trackpad scroll. |
| `>` popup dismissal | Click-outside dismisses. |
| Auto-rename source | LLM summary, already backend-owned. |
| Minimum window size | Hard floor on manual resize. |

## Related documentation

- [Dashboard mode — removal record and archive index](README.md) — what became of the canvas, and what was deleted with it.
- [v2 — Dashboard Mode design spec](v2-dashboard-mode-design.md) — the bounded-board design this one replaces; its §9 layout engine is what the canvas retires.
- [v3 — Canvas dashboard implementation plan](v3-infinite-canvas-plan.md) — the 13-task plan that executed this spec.
- [v4 — Dashboard fold mode design spec](v4-window-fold-mode-design.md) — the successor, which folds windows on this canvas into compact cards.
