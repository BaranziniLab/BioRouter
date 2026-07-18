# Dashboard Mode — design spec (v2)

> **What this is.** The second-generation spec for the multi-conversation
> workspace. It renames Lab Meeting Mode to **Dashboard Mode** and reworks
> window sizing, focus-pop, session naming, per-window pickers and the layout
> engine.
> **Status:** Superseded, and then removed. The infinite-canvas rework
> ([v3 — Canvas dashboard design spec](v3-infinite-canvas-design.md)) replaced
> the board described here, and dashboard mode was removed from the app
> wholesale on 2026-07-18 — the `/dashboard` route, `DashboardProvider`, both
> contexts and the whole component tree are deleted (see the
> [removal record](README.md)). A few incidental changes specified here did
> survive the removal: `SessionNamePill` was relocated to
> `ui/desktop/src/components/SessionNamePill.tsx`, and `BaseChat`'s `coherent`
> default stayed flipped on.
> **Audience:** maintainers reading the dashboard-mode archive.
> **Section key.** Sections are numbered `§0`–`§15`. The
> [v2 implementation plan](v2-dashboard-mode-plan.md) cites those numbers
> (`§7c`, `§8.3`, `§13`), so they are preserved verbatim rather than renumbered.
> **Who "the requester" is.** This document was written against change requests
> from the product owner who commissioned the feature. Where the original text
> said "the user" to mean *that person*, it now says **the requester**; "the
> user" is reserved throughout for the end user of the app.

**Project:** BioRouter
**Feature:** Dashboard Mode (renames Lab Meeting Mode; reworks layout, sizing, naming, chat UI)
**Date:** 2026-05-10
**Original status line:** Approved (pending implementation)

---

## 0. Summary

Lab Meeting Mode (v1, 2026-05-08 — see the [v1 design spec](v1-lab-meeting-mode-design.md)) shipped a working multi-conversation workspace at `/lab-meeting`. This spec covers eight changes the requester asked for after first-pass testing, plus one cross-cutting change from design review.

> **Read §1 first.** The rename table in §1 is the load-bearing artifact of this
> document — it is the complete old-name → new-name mapping that the whole
> codebase sweep was driven from. The list below is a change log, not the design.

1. Rename the feature to **Dashboard Mode** everywhere.
2. Switch the toolbar entry icon from `Users` to `LayoutDashboard`.
3. Spawn windows at the **comfort size** (940×800 — the app's default BrowserWindow size), not screen-filling. Only shrink when many windows exist.
4. Focus-pop must not bleed past board borders.
5. Toolbar polish (drop the `+` glyph; match app button hover); investigate and fix any breakage in **Organize** and **Clear**.
6. When the user navigates away from `/dashboard`, restore the BrowserWindow to its pre-maximize size. No confirmation modal — conversations stay on the dashboard until explicitly closed.
7. Make the standalone `/pair` Chat tab match the coherent dashboard chat aesthetic; give every dashboard window its own complete per-window pickers (model, mode, cwd, extensions, skills, cost).
8. Tab names: default to `Session 1`, `Session 2`, … Auto-rename from biorouterd's session name once a conversation starts. User-set names always win and propagate to History.

Plus one cross-cutting change requested during design review:

9. **New layout engine** — deterministic soft-tile + force-relaxation that respects user-resized windows, minimizes overlap, and produces the same layout every Organize click.

---

## 1. Rename: `LabMeeting` → `Dashboard`

| Surface | Old | New |
|---|---|---|
| Folder | `ui/desktop/src/components/LabMeeting/` | `ui/desktop/src/components/Dashboard/` |
| Provider class | `LabMeetingProvider` | `DashboardProvider` |
| Board / Toolbar / Route / Pill components | `LabMeetingBoard`, `LabMeetingToolbar`, `LabMeetingRoute`, `BackToLabMeetingPill`, `LabMeetingStatusBar` | `DashboardBoard`, `DashboardToolbar`, `DashboardRoute`, `BackToDashboardPill` (and `DashboardStatusBar` is **removed** per §7) |
| Hooks | `useLabMeeting`, `useOptionalLabMeeting`, `useLabMeetingDrag` | `useDashboard`, `useOptionalDashboard`, `useDashboardDrag` |
| Types | `LabMeetingContext`, `LabMeetingState`, `LabMeetingApi`, `LabWindow` | `DashboardContext`, `DashboardState`, `DashboardApi`, `DashboardWindow` |
| Storage helper module | `labMeetingStorage.ts` | `dashboardStorage.ts` |
| Drag hook module | `useLabMeetingDrag.ts` | `useDashboardDrag.ts` |
| Route path | `/lab-meeting` | `/dashboard` |
| IPC channel | `lab-meeting:enter` | `dashboard:enter` (plus new `dashboard:exit`) |
| Preload bridge method | `electron.labMeetingEnter()` | `electron.dashboardEnter()` + `electron.dashboardExit()` |
| localStorage key | `biorouter.labmeeting.v1` | `biorouter.dashboard.v1` |
| Pill label | "Back to Lab Meeting" | "Back to Dashboard" |
| Button tooltip | "Open Lab Meeting Mode" | "Open Dashboard" |

**localStorage migration:** On first hydration, if `biorouter.dashboard.v1` is absent and `biorouter.labmeeting.v1` is present, read the old key, write to the new key, and delete the old one. One-shot, runs at provider mount.

**Documentation:** the original 2026-05-08 spec and plan stay in place as historical record (renaming the spec file would lose git history). This new spec replaces them as the source of truth. They now live alongside this file as [v1 — Lab Meeting Mode design spec](v1-lab-meeting-mode-design.md) and [v1 — Lab Meeting Mode implementation plan](v1-lab-meeting-mode-plan.md).

**Verification:** `grep -rin "lab.meeting\|LabMeeting\|labmeeting" ui/desktop/src` returns zero hits after the sweep.

---

## 2. Icon

Add `LayoutDashboard` from `lucide-react` to the import block in [`app-icons.tsx`](../../../ui/desktop/src/components/icons/app-icons.tsx) and export it (wrapped through the same `light()` helper as siblings). Replace the `Users` icon in [`AppLayout.tsx`](../../../ui/desktop/src/components/Layout/AppLayout.tsx) with `LayoutDashboard`. Tooltip text becomes "Open Dashboard".

`Users` import in AppLayout removed if no other consumer in that file.

---

## 3. Window sizing — comfortable, not screen-filling

The app's BrowserWindow default is `940 × 800` (set in [`ui/desktop/src/main.ts`](../../../ui/desktop/src/main.ts), at lines 573–574 as of this spec's date). That is the most comfortable size for a single chat. Dashboard windows use this as their target size — only smaller when crowded.

Constants in the engine:

```ts
const COMFORT_W = 940;
const COMFORT_H = 800;
const MIN_W = 320;
const MIN_H = 240;
const EDGE_INSET = 6;       // board margin (focus-pop safety)
const FILL_FACTOR_MAX = 0.7;
```

Sizing pipeline:

1. `availableArea = (board.w − 2·EDGE_INSET) × (board.h − 2·EDGE_INSET) − Σ(pinned.area)`
2. `totalComfortArea = nAuto × COMFORT_W × COMFORT_H`
3. `fillFactor = totalComfortArea / availableArea`
4. If `fillFactor ≤ FILL_FACTOR_MAX`: cell size = `(COMFORT_W, COMFORT_H)`. Windows are at full comfort.
5. Else: uniform scale `s = sqrt(FILL_FACTOR_MAX / fillFactor)`. Cell size = `(max(MIN_W, COMFORT_W·s), max(MIN_H, COMFORT_H·s))`.

On a maximized 2112×973 board:
- n=1 → one 940×800 window centered.
- n=2 → two 940×800 windows side-by-side with equal horizontal gap, vertical center.
- n=3 → three windows tiled at 704×486 (board.w/3 × board.h/2 = capped only when it'd exceed comfort).
- n=6 → 3×2 grid at 704×486.
- n=8 → 3×2 grid + 2 intersection overflow, all 704×486.
- n=9+ → oldest non-focused auto-tucked into sidebar.

---

## 4. Focus-pop must not exceed board borders

Current focus class on `ChatWindow` adds `−translate-y-0.5 scale-[1.01]` with default `transform-origin: center`. A window flush against a board edge visibly pokes past it.

Fix: choose `transform-origin` per window from which edges its rect touches.

```ts
const TOUCH = 2;  // px tolerance
const origin = {
  x: rect.x <= EDGE_INSET + TOUCH ? 'left'  : rect.x + rect.w >= board.w - EDGE_INSET - TOUCH ? 'right'  : 'center',
  y: rect.y <= EDGE_INSET + TOUCH ? 'top'   : rect.y + rect.h >= board.h - EDGE_INSET - TOUCH ? 'bottom' : 'center',
};
const transformOrigin = `${origin.x} ${origin.y}`;
```

Applied as an inline `style.transformOrigin` on the window root. The translate also flips: when `origin.y === 'top'`, the `−translate-y-0.5` is dropped (or becomes `+translate-y-0.5`) so the window grows downward, not upward into the toolbar.

The layout engine additionally enforces `EDGE_INSET = 6 px` so that even the un-popped rect sits at least 6 px inside every board edge — guaranteed headroom for the scale.

---

## 5. Toolbar polish, and the Organize/Clear investigation

### 5.1 Buttons

- The Spawn button currently renders `<Plus> Spawn`. Drop the icon; render just the text `Spawn`. Organize and Clear are already text-only — Spawn now matches.
- Hover treatment: align with the standard ghost button hover used by the sidebar nav items in [`AppSidebar.tsx`](../../../ui/desktop/src/components/BioRouterSidebar/AppSidebar.tsx) — `hover:bg-background-medium transition-colors`. Whatever class is used there is what we use here; do not invent a new hover style.
- Active/click state: same as the rest of the app — let the `Button` component's variant handle it (we already use `variant="ghost" size="xs"`).

### 5.2 Organize and Clear bug investigation

> **Note.** This subsection is a bug investigation, not forward-looking design.
> It records what was observed in a debugging session and what still had to be
> confirmed against the running app, and is kept here because the fix landed as
> part of this spec's work.

The requester reports both don't work. During Playwright validation in the prior session, both **updated state in storage**, but the visible reflow may not have appeared to do anything because:

- **Clear** correctly empties `state.windows` → board shows empty CTA. Looks broken if the user expected an animated dismissal.
- **Organize** sets `isManuallyPlaced=false` and clears `position/size` for all windows → but if no window had been moved manually, the layout output is unchanged, so nothing visibly happens.

Plan: use Playwright debugger against the running app to reproduce what the requester is seeing, then fix the underlying issue. Two specific things to confirm:

- Clicking Organize after a window has been *resized*: does the window snap back to the engine-computed size? (Expected yes after this spec — Organize is the canonical "redo layout from scratch with the new engine.")
- Clicking Clear: do all on-board and tucked windows disappear, and does the sidebar collapse? (Expected yes.)

Any bug found in the buttons themselves (click handler not bound, state not updating) is fixed here. Any UX mismatch (the requester expected Organize to do something visible when no manual placement exists) is addressed by ensuring Organize always re-runs the **new** layout engine, which can produce different output than the current grid only after the engine itself is in place (phase 3). So Organize becomes meaningfully visible once both phases land.

---

## 6. Window-size restore on exit (no confirmation modal)

Per the requester's clarification: when the user clicks Home, Chat, or any sidebar destination, the dashboard's windows are **not closed**. Conversations live until the user explicitly closes them via the window `×` button or the toolbar Clear. The user can return to the dashboard via the LayoutDashboard toolbar button or the floating "Back to Dashboard" pill — state is fully preserved.

The only thing that changes on navigation is the **BrowserWindow size**:

- Entering `/dashboard` already calls IPC `dashboard:enter` → `win.maximize()`. Electron's `windowStateKeeper` (used in `main.ts`) remembers the pre-maximize bounds.
- New IPC `dashboard:exit` → `if (win.isMaximized()) win.unmaximize()`. This restores to the pre-maximize bounds — i.e., the default `940 × 800` (or whatever the user had).
- Wired in `DashboardRoute`: `useEffect` on mount calls `dashboardEnter()`; the cleanup function returned by the same `useEffect` calls `dashboardExit()` when the route unmounts. So any navigation away from `/dashboard` triggers unmaximize automatically.

No confirmation modal. No closing of windows. Just shrink + go.

---

## 7. Coherent `/pair` and per-window pickers

### 7a. `BaseChat` coherent by default

The `coherent` prop on `BaseChat` was added in v1 to flip on the single-surface visual treatment (no horizontal divider between message scroll area and input). It defaulted to `false` for backward compatibility with the standalone `/pair` route.

Flip the default to `true`. Standalone `/pair` now uses the same single-rounded-surface aesthetic as a dashboard window. Code paths that explicitly want the two-surface look can pass `coherent={false}` — though nothing in the codebase currently does.

The `hideStatusBar` prop is also removed (see §7b).

### 7b. Per-window full ChatInput

Remove `hideStatusBar={true}` from the `<BaseChat>` invocation inside `ChatWindow`. Each dashboard window now renders its own complete ChatInput including:

- `DirSwitcher` (per-window working directory)
- Attach button
- `ModelsBottomBar` (per-window model)
- `BottomMenuModeSelection` (per-window mode)
- `BottomMenuExtensionSelection` (per-window extensions)
- `BottomMenuSkillSelection` (per-window skills)
- `CostTracker` (per-window cost)
- Diagnostics button

Each window is now a fully self-contained `/pair` instance, just wrapped in window chrome. Changing the model in window A does not affect window B.

Initial values for these come from the app-level defaults at spawn time (existing behavior — biorouterd's `createSession` already picks them up from config).

### 7c. Remove `DashboardStatusBar`

With per-window pickers, the app-level status bar at the bottom of the dashboard route is redundant. Remove the component and its render in `DashboardRoute`. The route's vertical layout becomes:

```tsx
<DashboardToolbar />
<DashboardBoard />          // takes remaining height
```

No app-level status row.

### 7d. `hideStatusBar` and `coherent` props on `ChatInput` cleanup

`ChatInput` still accepts the `hideStatusBar` prop introduced in v1. Since nothing uses it after this change, remove it. Keep `coherent` as a visual modifier — `BaseChat` still threads it through.

---

## 8. Tab names

### 8a. Default names

Replace `palette.NAME_POOL` (Atlas, Nova, …) with sequential `Session 1`, `Session 2`, … The next number is computed as `max(window.badge for w in windows) + 1` on spawn — already what we do for the badge.

Drop the `#N` badge from the title bar — `Session 3` is self-numbering. The colored accent dot stays for differentiation.

### 8b. Auto-rename from biorouterd

biorouterd already auto-names sessions based on first message content (`session.name` updates after the first assistant reply). Each `ChatWindow` already mounts a `useChatStream` which surfaces the `session` object. Listen for changes:

```ts
// inside ChatWindow
useEffect(() => {
  if (!session?.name) return;
  if (win.userSetName) return;            // user override wins
  if (session.name === win.name) return;
  dashboard.updateWindowField(win.windowId, 'name', session.name);
}, [session?.name, win.userSetName, win.name, win.windowId]);
```

### 8c. User-set takes precedence

Add `userSetName: boolean` to `DashboardWindow`. Default `false`. Set to `true` whenever the user submits an inline rename (in either the dashboard title bar OR the new in-chat name pill — see §8d).

`DashboardWindow` shape additions (subset):

```ts
interface DashboardWindow {
  windowId: string;
  sessionId: string;
  name: string;            // current display name
  userSetName: boolean;    // NEW
  badge: number;
  // … rest unchanged
}
```

### 8d. Editable name pill in BaseChat header

Add a small editable name pill to the top of `<BaseChat>` content area — visible in both `/pair` and inside dashboard windows. Double-click → inline edit → Enter commits. When edited:

- For `/pair`: rename biorouterd's session via the existing rename endpoint (look up in `crates/biorouter-server/src/routes/sessions.rs` — `PATCH /sessions/{id}` or equivalent).
- For dashboard window: also update `dashboardWindow.name` and set `userSetName=true`.

For dashboard windows, the title-bar rename and the in-chat pill rename are kept in sync via the same `renameWindow(id, name)` action which also propagates to biorouterd. Both surfaces stay consistent.

### 8e. History propagation

History reads from biorouterd's `session.name`. As long as user renames write through to biorouterd (which they do, per §8d), History reflects user names. Nothing else to change.

---

## 9. Layout engine — deterministic soft-tile + relaxation

> **What changed from v1.** The function signature is identical to v1's engine
> (§9.1); only the internals are replaced. v1 computed a clean grid, then placed
> overflow windows on grid intersection points, and had no notion of respecting
> a user-resized window. v2 replaces that with the six-stage pipeline in §9.3 —
> the new parts are comfort-capped cell sizing (Stage 2), treating user-placed
> windows as *pinned* obstacles to route around (Stage 4), force relaxation to
> spread residual overlap (Stage 5), and a hard determinism contract with zero
> RNG (§9.4, §9.7).

### 9.1 Inputs / outputs

```ts
function computeLayout(
  windows: readonly LayoutInputWindow[],
  board: BoardSize,
  T1: number, T2: number,
  focusedWindowId: string | null,
): Map<WindowId, LayoutRect>
```

Signature unchanged from v1. Internals rewritten.

### 9.2 Constants

```ts
const COMFORT_W = 940;
const COMFORT_H = 800;
const MIN_W = 320;
const MIN_H = 240;
const EDGE_INSET = 6;
const FILL_FACTOR_MAX = 0.7;
const TARGET_ASPECT = 1.3;
const RELAX_PASSES = 6;
const RELAX_STEP_MAX = 20;
const SNAP_GRID = 4;
const Z_TILED = 1;
const Z_OVERFLOW = 50;
const Z_PINNED = 5;
const Z_FOCUSED = 100;
```

### 9.3 Pipeline

```text
1. Partition  → pinned, auto, tucked (tucked skipped entirely)
2. Cell-size  → cellW × cellH for auto windows (comfort cap, fill-factor 0.7)
3. Slot       → initial position per auto window
                  - nAuto ≤ 2: centered comfort row
                  - 3 ≤ nAuto ≤ T1: bestGridConfig with cell cap + last-row centered
                  - T1 < nAuto ≤ T2: tile T1 + intersection-point overflow (existing)
4. Repulse    → push each auto window out of any pinned overlap
5. Relax      → 6 deterministic relaxation passes spreading auto-vs-auto overlaps
6. Snap       → 4px grid; assign z-index
```

### 9.4 Pure function, no RNG

Zero `Math.random()` calls. Every "perturbation" comes from `hash32(windowId)` (FNV-1a or similar pure hash) or `hash32(idA + '|' + idB)`. The hash function is deterministic and stable across runs.

### 9.5 Stage 4 — Repulse pinned collision

For each auto rect, against each pinned rect:

```text
overlap = intersect(autoRect, pinnedRect)
if overlap.area == 0: continue
// Choose cardinal exit with the smallest displacement
candidates = [
  { dx: pinnedRect.left - autoRect.right,  dy: 0 },  // exit west
  { dx: pinnedRect.right - autoRect.left,  dy: 0 },  // exit east
  { dx: 0, dy: pinnedRect.top - autoRect.bottom },   // exit north
  { dx: 0, dy: pinnedRect.bottom - autoRect.top },   // exit south
]
pick min |dx| + |dy|
tie-break by hash32(autoRect.id) & 1
apply, clamp to board interior
```

### 9.6 Stage 5 — Relaxation

```text
for pass in 0..RELAX_PASSES:
  delta := {}
  for each pair (A, B) in auto × auto with i < j:
    overlapArea = max(0, overlapW) * max(0, overlapH)
    if overlapArea == 0: continue
    vec = centerA - centerB
    if vec == 0: vec = deterministicUnit(hash32(A.id ^ B.id))
    mag = min(RELAX_STEP_MAX, sqrt(overlapArea) * 0.5)
    delta[A] += unit(vec) * mag
    delta[B] -= unit(vec) * mag
  for each auto window W:
    W.position += delta[W]
    clamp W into [EDGE_INSET, board.w - EDGE_INSET - W.w] × [EDGE_INSET, board.h - EDGE_INSET - W.h]
```

Pinned windows are not moved.

### 9.7 Determinism contract (test-enforced)

- `computeLayout(windows, board, T1, T2, focused)` is referentially transparent — for fixed inputs, every invocation returns equal outputs.
- Shuffling the `windows` input array produces the same output positions per `windowId`. (Stable sort by `windowId` is applied at Stage 1.)
- Repeated `organize()` clicks (which clear pinning and call computeLayout) produce identical visual output.

### 9.8 Test additions to `layoutEngine.test.ts`

New tests beyond v1's 12:

- n=1 → centered 940×800 on 2112×973 board.
- n=2 → two 940×800 side-by-side with computed gap.
- n=3 on 2112×973 → 3-up, cell ≤ comfort (704×486).
- n=4 on a 4000×2400 board → 2×2, cells **capped at 940×800** (not stretched to 2000×1200), block centered.
- n=6 → 3×2 grid, cell = (704, 486).
- Determinism: 50 invocations return deep-equal outputs.
- Idempotence: `organize()` (provider-level operation) produces identical state across repeated invocations.
- Shuffle stability: passing the windows array in 5 different orders yields the same per-id positions.
- Pinned avoidance: a single pinned 800×600 in the board center; 4 auto windows all sit outside that rect with > 95% area outside.
- Repulsion convergence: two auto windows seeded at identical positions are pushed apart after relaxation; final overlap == 0.
- Hash stability: same windowIds across multiple test runs produce same tie-break directions.
- Edge guarantee: no window's rect exceeds `[EDGE_INSET, board.w − EDGE_INSET]` × `[EDGE_INSET, board.h − EDGE_INSET]`.

---

## 10. User-resize flow

Resize handle already lives at the bottom-right corner of each `ChatWindow` (v1). The existing `usePointerDrag` hook fires `resizeWindow(id, {w, h})` and sets `isManuallyPlaced=true`. With the new engine:

- Resized windows count as pinned (Stage 1). Their rect is committed verbatim.
- Auto windows are placed around them (Stage 4 ensures no auto-vs-pinned overlap).
- Floor at `(MIN_W, MIN_H)` — the resize handle won't allow smaller. Ceiling at the board minus inset.

Drag-to-move (title bar) similarly sets `isManuallyPlaced=true`; pinning applies the same way.

Organize clears `isManuallyPlaced` for **all** windows, then runs `computeLayout`. This is the canonical "redo from scratch" — deterministic.

---

## 11. Files — created / modified / removed

### Modified
- `App.tsx` — `/lab-meeting` → `/dashboard`, provider rename.
- `components/Layout/AppLayout.tsx` — icon swap, button label, pill render.
- `components/BaseChat.tsx` — `coherent` default true; remove `hideStatusBar`; new name-pill header.
- `components/ChatInput.tsx` — remove `hideStatusBar` prop (no consumer left).
- `main.ts` — IPC channel rename + new `dashboard:exit`.
- `preload.ts` — bridge method rename + new method.

### Renamed (directory `LabMeeting/` → `Dashboard/`)
All `Lab*` and `lab*` modules. Re-export shapes preserved (`index.ts`). All test files renamed alongside.

### Removed
- `DashboardStatusBar.tsx` (was `LabMeetingStatusBar.tsx`).
- `hideStatusBar` prop on `ChatInput`.
- `palette.NAME_POOL` (replaced by `Session N` generator).

### Created
- `components/Dashboard/SessionNamePill.tsx` — the editable in-chat name pill.

---

## 12. Out of scope (deferred)

- Cross-window message references / drag-and-drop content.
- Pin/unpin a specific window to keep it on-board even past T2.
- Multi-monitor support / native OS window per chat.
- Auto-rename via biorouterd is consumed but not authored by us. We rely on biorouterd's existing naming.
- The relaxation engine here is intentionally non-physical (no momentum, no springs). YAGNI for v2.

---

## 13. Migration plan

- v1 storage key (`biorouter.labmeeting.v1`) is read at provider mount; if present and the new key is absent, contents are copied into the new key (`biorouter.dashboard.v1`) and the old key is deleted. Single shot per install.
- The v1 spec and plan files stay where they are, for reference. This document replaces them as the source of truth.

> **Note.** The original text here said replacement "is communicated via this
> doc's status header". It was not — the header of this spec as written said
> only "Approved (pending implementation)" and never named what it superseded.
> The superseded-by relationship is now stated explicitly in the context header
> at the top of both this file and the [v1 design spec](v1-lab-meeting-mode-design.md).

---

## 14. Testing and validation

- Unit (Vitest): full `layoutEngine.test.ts` rewrite (~22 cases); provider tests updated for renamed surface.
- Component (Vitest): `DashboardProvider.test.tsx` — `userSetName` flag, biorouterd-name sync, default `Session N` numbering, localStorage migration from v1 key.
- E2E (Playwright via CDP): walk every change against the running app — rename icon visible, dashboard route up, n=1 window at comfort size, focus pop stays inside borders, Organize/Clear visible behavior, navigate-away shrinks window, /pair coherent surface, per-window pickers independent, rename in either surface propagates to History.

---

## 15. Open risks

- **Coherent `/pair` default flip** — touches the most-used route. Manual smoke test before merging.
- **biorouterd session-name sync** — depends on biorouterd's existing behavior; if it stops emitting name updates the auto-rename falls silent (user can still rename manually).
- **localStorage migration** — single-shot, fails gracefully (if old key parse fails, just start fresh on the new key).

## Related documentation

- [Dashboard mode — removal record and archive index](README.md) — what became of this feature, and which parts of this spec survived the removal.
- [v1 — Lab Meeting Mode design spec](v1-lab-meeting-mode-design.md) — the design this one renames and reworks; §1's table maps its every identifier.
- [v2 — Dashboard Mode implementation plan](v2-dashboard-mode-plan.md) — the 12-task plan that executed this spec and cites its `§N` numbers.
- [v3 — Canvas dashboard design spec](v3-infinite-canvas-design.md) — the direct successor: throws away the bounded board and the `T1`/`T2` tuck system for an infinite canvas.
