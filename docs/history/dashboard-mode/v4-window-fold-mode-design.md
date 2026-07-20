# Dashboard fold mode — design spec (v4)

> **What this is.** The design for folding dashboard chat windows into compact
> 240×72 cards — individually or all at once — with a live busy indicator and a
> new muted accent palette.
> **Status:** Historical record, then superseded by removal. Fold mode was built
> and **shipped in v1.76.0**, and was deleted on 2026-07-18 along with the whole
> dashboard: `FoldedCard`, `DashboardToolbar`, `WindowTitleBar` and `palette.ts`
> all went with the `ui/desktop/src/components/Dashboard/` tree. See the
> [removal record](README.md). The original header below said only "Approved for
> implementation planning" and recorded neither the ship nor the removal.
> **Ship version corrected 2026-07-20.** This file previously said v1.85.3. The
> [v1.76.0 release notes](../../releases/notes/v1.76.0.md) headline "dashboard
> fold mode" and name five features specified below — the title-bar Fold button,
> the pulsing busy indicator, the Fold all / Unfold all toggle, persisted fold
> state, and the muted accent palette — while v1.85.3's notes mention fold mode
> nowhere.
> **Audience:** maintainers reading the dashboard-mode archive.
> **Prerequisite.** This spec assumes the infinite canvas built by
> [v3 — Canvas dashboard design spec](v3-infinite-canvas-design.md), which is
> where `organize()`, `canvasLayout.ts`, the `MIN_WINDOW_W` / `MIN_WINDOW_H`
> constants and the per-window accent colours come from. Read that first if any
> of those are unfamiliar.

**Date:** 2026-05-29
**Original status line:** Approved for implementation planning
**Area:** `ui/desktop/src/components/Dashboard/*` (deleted 2026-07-18)

## Problem

The dashboard's full-size chat windows take a lot of canvas real estate. Users running many parallel conversations want a way to collapse them into compact "cards" (a small, click-to-expand affordance showing just enough metadata to identify each conversation and its live activity) without losing their position on the canvas or the underlying chat state. They also want a single one-click way to fold or unfold everything at once.

## Goals

- Per-window folded state: any window can be folded into a compact card and unfolded back to a chat window, in place.
- A toolbar-level toggle that folds all windows when off→on and unfolds all when on→off.
- A folded card surfaces: title, working directory, live status (busy vs idle), accent color (via the card itself), and the same resize/close affordances.
- A new title-bar button on each chat window for "fold to card."
- Clicking a folded card re-expands it to a standard small window with the chat input focused.
- `organize()` keeps working with a mix of cards and windows.
- New, calmer accent palette.

## Non-goals

- Cross-session sync of the fold state.
- Animating the fold/unfold transition with FLIP/morph — a simple CSS `transition` on width/height is sufficient.
- Bulk multi-select fold (e.g., "fold these 3 only"). Only single-window and global fold/unfold.
- Adding fold state to the CLI/REST API. This is a UI-only concern.

## State model

### `DashboardWindow` (extend in `ui/desktop/src/contexts/DashboardContext.tsx`)

Add two fields:

```ts
folded: boolean;   // default false; persisted
isBusy: boolean;   // default false; NOT persisted (transient runtime state)
```

`folded` is persisted alongside existing window fields via the same serialize path used by `position`, `size`, `accentColor`, etc. `isBusy` is derived at runtime from `useChatStream`'s loading flag and reset to `false` on hydrate.

### `DashboardContext` API additions

```ts
foldWindow(windowId: string, folded: boolean): void;
foldAll(): void;
unfoldAll(): void;
setWindowBusy(windowId: string, busy: boolean): void;
allFolded: boolean;  // derived selector: windows.every(w => w.folded), false if windows is empty
```

`foldAll` sets `folded = true` on every window. `unfoldAll` sets `folded = false` on every window. Neither changes `position` or `size`.

### Toolbar "Fold" toggle behavior

The toolbar button's ON state is derived from `allFolded`. Click handler:

- If `allFolded` is true → call `unfoldAll()`.
- Else → call `foldAll()`.

This means individual fold/unfold via the title-bar button or card-click can put the global toggle in either state without conflict — the toggle re-syncs to "all folded" or "not all folded" on each interaction.

## Geometry

New constants in `DashboardProvider.tsx`, alongside `MIN_WINDOW_W`/`MIN_WINDOW_H`:

```ts
export const CARD_W = 240;
export const CARD_H = 72;
```

When `window.folded` is true, `ChatWindow` renders at `CARD_W × CARD_H`. The window's stored `size` is **not** overwritten — it's preserved so that unfolding restores the prior dimensions. The render path uses `folded ? {w: CARD_W, h: CARD_H} : size` for the outer frame.

`organize()` in `canvasLayout.ts` already reads each window's effective `w/h`, so it packs cards and windows together without code changes, as long as `DashboardProvider` passes the effective (post-fold) size into the layout call. Verify this at implementation time; if `organize()` reads from `state.windows[i].size` directly, change the call site to compute the effective rect.

## Components

### `DashboardToolbar.tsx`

Insert a 4th button between "Organize" and "Clear" (visual order: Spawn · Organize · Fold · Clear).

- Label: `Fold`
- Right side of the label: a mini switch-style affordance — a 22×12 rounded track (`bg-background-medium/60` off, accent-tinted on) with an 8px thumb that slides on toggle. Driven by `allFolded`.
- `aria-pressed={allFolded}`.
- Same `btnClass` as the other toolbar buttons; mini switch lives inside the button.
- Click handler: `allFolded ? unfoldAll() : foldAll()`.

### `WindowTitleBar.tsx`

Insert a new leftmost button in the right-aligned button cluster (becomes: Fold · Shrink · Enlarge · Close).

- Icon: lucide `Minus`
- Tooltip: `Fold to card`
- Reuses existing `iconBtnClass`.
- Calls a new prop `onFold()` wired by `ChatWindow.tsx` to `dashboard.foldWindow(windowId, true)`.

### `ChatWindow.tsx`

Branch on `window.folded`:

- `false` → existing render (no behavior change beyond passing `onFold` to `WindowTitleBar`).
- `true` → render `<FoldedCard window={w} onUnfold={...} onClose={...} onShrink={...} onEnlarge={...} />` inside the same draggable outer frame.

Add an effect that observes the inner chat's streaming flag (from `useChatStream` via `BaseChat`) and calls `dashboard.setWindowBusy(windowId, isLoading)`. This must run regardless of `folded` so the busy state stays current even while folded (the chat keeps running in the background — same mount, just a hidden subtree).

**Important:** keep the chat subtree mounted while folded. The `FoldedCard` renders alongside or above the chat subtree (chat hidden via `display: none` or similar), so streaming state and message history persist across fold/unfold without remount. This is required for `isBusy` to keep updating while folded.

### `FoldedCard.tsx` (new)

> **Button key for the mock below.** `─` is Fold/unfold (lucide `Minus`), `▭` is
> Shrink (`Minimize2`), `◻` is Enlarge (`Maximize2`), `✕` is Close (`X`). The
> same four are itemised under "Buttons" further down.

Layout (240×72, `rounded-lg`):

```text
┌──────────────────────────────────────────┐
│ ◉  Conversation title…       ─ ▭ ◻ ✕    │  ← row 1, ~28px
│    ~/Desktop/project                     │  ← row 2, cwd, text-xs, mono, truncated
└──────────────────────────────────────────┘
```

Styling:

- Background: `linear-gradient(135deg, ${accent}2E, ${accent}0F)` (~18% → ~6%) over `bg-background-default`.
- Border: `1px solid ${accent}47` (~28%).
- Subtle drop shadow matching existing window shadow tokens.
- Row 1: status indicator (left) · title (flex-1, truncate) · button cluster (right).
- Row 2: working directory in `font-mono text-[11px] text-text-muted/80`, truncated with ellipsis.

Status indicator (10×10 px, replaces the old "dot"):

- **idle**: hollow ring, `border: 1.5px solid ${accent}`, transparent fill.
- **busy**: filled disc in `${accent}`, with `animate-[breathe_1.4s_ease-in-out_infinite]` (opacity 1 → 0.55 → 1) plus a 16×16 outer ring at `${accent}40` doing a slower pulse (`scale(1) → scale(1.4)`, fade out).

Define the `breathe` keyframes in the existing global stylesheet (Tailwind config or `index.css`) — do not inline in JSX.

Buttons (right cluster, reuse `iconBtnClass`):

- `Minus` — unfold (calls `onUnfold()` which calls `foldWindow(id, false)`).
- `Minimize2` — shrink (unfolds and sets size to `MIN_WINDOW_W × MIN_WINDOW_H`).
- `Maximize2` — enlarge (unfolds and sets size to `ENLARGE_W × ENLARGE_H`).
- `X` — close (calls `closeWindow(id)`).

Card click target: the entire card area **except** the 4 buttons. `onCardClick` → `onUnfold()`. After unfold, the existing `BaseChat` input-focus-on-mount path (already in place) takes the cursor; if focus doesn't already auto-grab on the existing chat mount, add a focus call gated by a "just unfolded" flag passed through props.

Drag/move plumbing: card uses the same outer wrapper that today handles drag for the chat window. The buttons stop propagation; the rest of the card propagates pointer events so dragging works exactly like dragging a window header.

### `palette.ts`

Replace `ACCENT_PALETTE` with a muted, more cohesive 12-color set:

```ts
export const ACCENT_PALETTE = [
  '#7fae9f', // sage
  '#8b8fc4', // dusk
  '#c4a47a', // wheat
  '#c49096', // clay-rose
  '#a8b884', // olive
  '#8ab0c4', // dust-blue
  '#a89ac4', // lilac
  '#c49a96', // shell
  '#8fb8a3', // seafoam
  '#bdb084', // sand
  '#b894b4', // mauve
  '#8a96a3', // smoke
] as const;
```

Same `pickAccentColor()` cycling logic; only the values change. Existing assigned colors on already-spawned windows are persisted hex strings, so they keep their old vibrant hue until the user clears/respawns. No migration needed — the new palette applies to new spawns only.

### `BaseChat.tsx`

Single small addition: if rendered inside a dashboard window (detect via the existing dashboard context or via a `windowId` prop already passed by `ChatWindow.tsx`), add a `useEffect` that calls `dashboard.setWindowBusy(windowId, isLoading)` whenever `useChatStream`'s loading flag changes. Outside a dashboard window, this is a no-op.

## Data flow

Two independent flows feed the fold feature. The first carries streaming state
from a chat up to its card's status indicator:

```text
useChatStream.isLoading (per chat)
        │
        ▼
BaseChat useEffect ──► DashboardContext.setWindowBusy(id, bool)
                              │
                              ▼
                    state.windows[i].isBusy
                              │
                              ▼
                    FoldedCard status indicator
```

The second carries a toolbar click down to every window's render branch:

```text
Toolbar Fold button click
        │
        ▼
allFolded ? unfoldAll() : foldAll()
        │
        ▼
state.windows[*].folded = true|false
        │
        ▼
ChatWindow renders FoldedCard vs full chat
```

## Persistence

`folded` is included in the existing window serialization (same path as `position`, `size`, `accentColor`). `isBusy` is **not** persisted — on hydrate it defaults to `false` and is rewritten by the next streaming effect tick.

## Edge cases

- **Folding the focused window**: focus survives — `focusedWindowId` is unchanged. Card click on a non-focused folded window focuses it as part of unfold.
- **Unfolding to off-screen position**: the stored `position` is preserved, so unfold renders at the original spot. If the prior `size` would extend the window past current viewport, the existing `organize()` doesn't auto-trigger — accept that (matches today's behavior for resized windows).
- **Spawning a new window while global toggle is "on"**: new windows spawn unfolded (default `folded: false`). The toolbar toggle's derived `allFolded` flips to false on the next render. Acceptable.
- **Empty dashboard**: `allFolded` is false (vacuous-true short-circuited). Toggle is off-state.
- **Toggle clicked twice rapidly**: idempotent — `foldAll`/`unfoldAll` are setters, no race.
- **Drag a folded card**: works through the existing drag handler since the card wraps in the same outer frame.

## Testing

> **Warning.** This spec deliberately proposed **no automated tests** — the
> stated reason was that this is a UI feature and the existing dashboard had no
> test harness for layout interactions. That gap was never closed and no owner
> was named for closing it; the feature shipped in v1.76.0 covered only by the
> manual walkthrough below.

Manual smoke:

1. Spawn 3 windows; verify accent colors come from the new muted palette.
2. Click the title-bar `−` button on one → folds to 240×72 card, position preserved.
3. Click the card body → unfolds to its prior size at the same position, input is focused.
4. Click toolbar "Fold" toggle → all 3 fold simultaneously; toggle shows ON state.
5. Click toggle again → all 3 unfold; toggle shows OFF state.
6. Fold one window manually, then click the toolbar toggle → all fold (toggle stays/goes ON).
7. Start a generation in a folded window via a different mechanism (or unfold, send, refold) → status dot pulses while streaming; reverts to hollow ring when done.
8. Click "Organize" with a mix of cards and full windows → packing respects the mixed sizes.
9. Resize buttons on a folded card (`▭`, `◻`) → unfolds to shrink/enlarge dimensions.
10. Close button (`✕`) on a folded card → removes the window from the dashboard.

## Files touched

> **Note.** Every path below under `ui/desktop/src/components/Dashboard/` and
> `ui/desktop/src/contexts/DashboardContext.tsx` was deleted on 2026-07-18. They
> are recorded here as written; only the `BaseChat.tsx` and `main.css`
> touchpoints still exist.

- `ui/desktop/src/contexts/DashboardContext.tsx` — extend `DashboardWindow`, extend context API.
- `ui/desktop/src/components/Dashboard/DashboardProvider.tsx` — implement `foldWindow`, `foldAll`, `unfoldAll`, `setWindowBusy`, `allFolded`; add `CARD_W`/`CARD_H`; ensure organize uses effective rect.
- `ui/desktop/src/components/Dashboard/DashboardToolbar.tsx` — add Fold toggle button with mini switch.
- `ui/desktop/src/components/Dashboard/ChatWindow.tsx` — branch on `folded`; wire `onFold` to title bar; keep chat subtree mounted while folded.
- `ui/desktop/src/components/Dashboard/WindowTitleBar.tsx` — add `Minus` button.
- `ui/desktop/src/components/Dashboard/FoldedCard.tsx` — new component.
- `ui/desktop/src/components/Dashboard/palette.ts` — replace `ACCENT_PALETTE`.
- [`ui/desktop/src/components/BaseChat.tsx`](../../../ui/desktop/src/components/BaseChat.tsx) — pipe `isLoading` to `setWindowBusy` when inside a dashboard window.
- `ui/desktop/src/index.css` (or Tailwind config) — add `breathe` keyframes.

No backend (Rust) changes. No OpenAPI changes.

## Related documentation

- [Dashboard mode — removal record and archive index](README.md) — the record of fold mode shipping in v1.76.0 and coming out on 2026-07-18.
- [v3 — Canvas dashboard design spec](v3-infinite-canvas-design.md) — defines the canvas, `organize()`, `canvasLayout.ts` and the accent palette this spec builds on.
- [v4 — Dashboard fold mode implementation plan](v4-window-fold-mode-plan.md) — the 13-task plan that executed this spec, including the `onBusyChange` prop on `BaseChat`.
- [v2 — Dashboard Mode design spec](v2-dashboard-mode-design.md) — where `MIN_WINDOW_W`-style sizing and the per-window window model were established.
