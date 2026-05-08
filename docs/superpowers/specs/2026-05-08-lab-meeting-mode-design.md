# Lab Meeting Mode — Design Spec

**Project:** BioRouter
**Feature:** Lab Meeting Mode (multi-conversation parallel workspace)
**Date:** 2026-05-08
**Status:** Approved (pending implementation plan)

---

## 1. Overview

Lab Meeting Mode is a multi-conversation workspace within BioRouter that lets researchers run several AI chat sessions simultaneously on a single screen. Each conversation lives in its own resizable, movable window on a shared **board** to the right of the standard sidebar. Tucked overflow lives in a right-side panel.

The mode is implemented as a new route (`/lab-meeting`) with a global state provider mounted above the router so background autonomous chats keep streaming when the user navigates elsewhere.

This document is the canonical design. The user-supplied prose spec (which inspired this design) is preserved in §13 for reference.

---

## 2. Entry & Exit

### 2.1 Activation

- A toggle is added in [`AppLayout.tsx`](../../../ui/desktop/src/components/Layout/AppLayout.tsx) immediately to the right of the existing `+` "new window" button (top-left of the app, near the sidebar trigger). Icon: lucide `Users` (`Users` from `components/icons/app-icons.tsx`). Tooltip: "Open Lab Meeting Mode".
- Clicking the toggle navigates to `/lab-meeting`.
- On entry the renderer asks Electron `main` to maximize the BrowserWindow (matches user's spec — "enlarges to fit the full visible screen area; not full-screen"). The user retains the OS full-screen affordance.
- The standard sidebar (Home, Chat, History, Workflows, Scheduler, Extensions, Skills, Apps, Settings) remains mounted and unchanged.
- If the user had an active conversation in `/pair`, that conversation is *not* automatically pulled in. The board hydrates from `localStorage` if non-empty; otherwise a single new conversation is spawned to fill the board.

### 2.2 Deactivation

- "Exit Lab Meeting Mode" is achieved by navigating to any sidebar destination, by clicking a "Back to Lab Meeting" pill (which only appears once Lab Meeting state is non-empty and the user is on a different route), or by closing the app.
- All conversations persist in the standard BioRouter session store. Board state (positions, sizes, T1/T2, tucked list, accent colors, names) persists in `localStorage` under `biorouter.labmeeting.v1`.
- The Electron BrowserWindow is **not** automatically restored on exit — the user can resize it manually. (Reasoning: heuristically restoring window bounds creates jarring transitions; users who maximize once usually want to stay maximized.)

---

## 3. Architecture

### 3.1 Routing

A new top-level route is registered in [`App.tsx`](../../../ui/desktop/src/App.tsx):

```tsx
<Route path="lab-meeting" element={<LabMeetingRoute />} />
```

It sits inside the `AppLayout` shell so the standard sidebar and top-left controls remain.

### 3.2 State

A new context — `LabMeetingProvider` — is mounted at the app root, above `<Routes>`, so background streaming continues when the user navigates away. Its state shape:

```ts
type WindowId = string;          // stable client-side id
type SessionId = string;         // biorouterd session id

interface LabWindow {
  windowId: WindowId;
  sessionId: SessionId;
  name: string;                   // editable; default from name generator
  badge: number;                  // monotonic per-board (#1, #2, ...)
  accentColor: string;            // hex from a 12-color palette
  position: { x: number; y: number } | null;  // null = use auto-tile
  size:     { w: number; h: number } | null;  // null = use auto-tile
  isManuallyPlaced: boolean;
  isTucked: boolean;
  // per-window state overrides (initialized from app-level defaults)
  model?: string;
  mode?: string;
  cwd?: string;
  contextDepth?: number;
  costAccumulated?: number;
  // bookkeeping
  lastInteraction: number;        // for "oldest non-focused" tucking
  unreadActivity: boolean;        // pulse dot for autonomous activity
}

interface LabMeetingState {
  windows: LabWindow[];           // both on-board and tucked
  focusedWindowId: WindowId | null;
  T1: number;                     // grid limit, default 6
  T2: number;                     // board limit, default 8
}
```

Commands exposed by the provider:
`spawnWindow()`, `closeWindow(id)`, `focusWindow(id)`, `renameWindow(id, name)`,
`moveWindow(id, pos)`, `resizeWindow(id, size)`, `tuckWindow(id)`, `evokeWindow(id, dropPos?)`,
`organize()`, `clearAll()`, `setT1(n)`, `setT2(n)`, `markActivity(id)`, `recordInteraction(id)`.

### 3.3 Persistence

Whole `LabMeetingState` is debounced-serialized (~250ms) to `localStorage`. On hydrate we filter out `LabWindow`s whose `sessionId` no longer resolves in biorouterd (defensive — a session deleted via History should not leave a ghost window). Filtering happens lazily on first mount of `LabMeetingProvider`.

### 3.4 Per-window chat instances

Each `<ChatWindow>` renders:

```
<ChatProvider chat={chatStub} setChat={...} contextKey={`lab-${sessionId}`}>
  <BaseChat sessionId={sessionId} coherent hideStatusBar setChat={...} />
</ChatProvider>
```

Because `useChatStream` keys off `sessionId`, multiple windows hold independent streams. Existing concurrency in biorouterd already supports concurrent requests across sessions.

### 3.5 App-level status bar

To avoid a deep refactor of [`ChatInput.tsx`](../../../ui/desktop/src/components/ChatInput.tsx):

- Each window renders its own `ChatInput` for sending — but with a new `hideStatusBar` prop that suppresses the model/mode/cost/cwd footer inside the input.
- A new `<LabMeetingStatusBar>` is rendered once at the route level beneath the board. It reads from the focused window's state in `LabMeetingContext` and writes back to the same window's state.
- Existing model selector, mode selector, working-dir picker, cost display, and context-depth widget are reused as standalone components rebound to focused-window setters.

---

## 4. Component tree

```
<LabMeetingProvider>            (mounted at app root, above <Routes>)
  └─ <Routes>
      └─ /lab-meeting → <LabMeetingRoute>
          ├─ <LabMeetingToolbar>             (Spawn, Clear, Organize, T1/T2, status)
          ├─ <LabMeetingBoardContainer>
          │   ├─ <LabMeetingBoard>
          │   │   └─ <ChatWindow> × N        (one per non-tucked window)
          │   │       ├─ <WindowTitleBar>    (dot, name, #N, ×, drag handle)
          │   │       ├─ <BaseChat coherent hideStatusBar />
          │   │       └─ <ResizeHandle />
          │   └─ <TuckSidebar>
          │       └─ <TuckedCard> × M        (dot, name, #N, preview, ×, pulse)
          └─ <LabMeetingStatusBar>           (focused-window-aware, app-level row)
```

A `<BackToLabMeetingPill>` lives in `AppLayout` and renders only when `LabMeetingState` is non-empty AND the current route ≠ `/lab-meeting`.

---

## 5. The Lab Meeting Board

The board is the bounded rectangle to the right of the sidebar. It does **not** scroll or pan. All window positions are coordinates relative to the board's top-left.

### 5.1 Background

Subtle dotted grid (8px), low contrast vs the app background, theme-tinted — visually distinguishes Lab Meeting from the single-conversation view.

### 5.2 Coordinate system

Windows constrained to keep at least 80×40 px of the title bar within board bounds.

---

## 6. Conversation Windows

### 6.1 Anatomy

1. **Title bar** (~40 px tall, draggable except over interactive children):
   - Accent color dot (12-color palette: teal, indigo, amber, rose, lime, sky, violet, coral, mint, gold, magenta, slate).
   - Conversation name in medium weight (double-click → inline edit).
   - Numeric badge `#N` in subdued mono.
   - Close `×` button.

2. **Coherent body** (messages + input as a single surface):
   - Rounded `rounded-2xl` outer container with neumorphic shadow.
   - Scrollable message area takes most of the height; the input area is flush below with no horizontal divider — visual continuity is reinforced by a soft gradient mask between regions and a single shared border-radius on the outer container.
   - All standard message types (user, assistant, tool blocks, code blocks, file refs, todo lists, etc.) render unchanged via the existing `ProgressiveMessageList`.
   - Independent scroll position per window.
   - `ChatInput` renders with `coherent` and `hideStatusBar` props.

### 6.2 Editable names

Default names come from `palette.ts`'s name generator (Atlas, Nova, Prism, Echo, Lyra, Orion, Sage, Vega, Wren, Zephyr, ...). Double-click the title text → controlled `<input>`. Enter / blur commits, Esc cancels. Rename propagates to the conversation name in History.

### 6.3 Focus & "pop"

- Exactly one window is `focusedWindowId` at a time.
- Focused window gets CSS class `is-focused` → `transform: translateY(-2px) scale(1.01)` plus a stronger neumorphic shadow.
- Focused window has the highest z-index.
- When `windows.length === 1`, an `is-solo` class disables the translate/scale and leaves only the shadow change (avoids pushing the window outside the board).
- Click anywhere on a non-focused window (except `×`) focuses it. Focus updates `lastInteraction` (used by oldest-non-focused tucking).

### 6.4 Moving (drag)

`pointerdown` on the title bar (excluding the inline-name editor and `×`) starts a drag.
- Suppresses CSS transitions while active.
- Intensifies shadow (strong neumorphic lift).
- On release, sets `isManuallyPlaced=true`. The window stays at the dropped position until `Organize` is triggered.

### 6.5 Resizing

Bottom-right resize handle (visible on hover or when focused).
- Floor at the T1-grid cell size for the current board+T1 (so every window stays usable).
- On release, `isManuallyPlaced=true`.

### 6.6 Closing

`×` plays a 180ms scale-down + fade-out, then removes the `LabWindow`. Session history persists in BioRouter History. Remaining windows re-tile if they were in auto-layout. If the closed window was focused, focus transfers to the most recently-interacted-with remaining window.

---

## 7. Capacity Thresholds & the Sidebar Tuck System

### 7.1 Definitions

| Threshold | Default | Meaning |
|---|---|---|
| **T1** (Grid Limit) | 6 | Max windows that tile without overlap. ≤ T1 → clean grid. |
| **T2** (Board Limit) | 8 | Max windows on the board (including overlap). > T2 → tucked. |

Constraint: `T2 ≥ T1`. Both editable in the toolbar; changes trigger immediate reflow.

### 7.2 Layout behavior — pure function

`computeLayout(windows, board, T1, T2)` lives in [`layoutEngine.ts`](../../../ui/desktop/src/components/LabMeeting/layoutEngine.ts). Returns `Map<WindowId, {x, y, w, h, zIndex}>`.

Three modes by on-board count `n`:

**`n ≤ T1` — clean grid.** Pick the (cols × rows) configuration minimizing aspect-ratio deviation from a target ~1.3:1, given the board dimensions. Last row centers items if `n % cols !== 0`. Manually-placed windows are excluded from the auto-tile pass and rendered at their stored coords; remaining auto windows tile into the leftover area.

Examples (T1=6, landscape board):
- 1 → fills board.
- 2 → 2×1.
- 3 → 3×1.
- 4 → 2×2.
- 5 → 3×2 with last row centered (2 in last row).
- 6 → 3×2.

**`T1 < n ≤ T2` — overflow at intersections.**
1. First T1 windows tile in the standard grid.
2. Overflow windows (n − T1) sized to the T1-cell size, centered on grid intersection points:
   - Collect all horizontal & vertical edge lines from the tiled grid.
   - Generate all line crossings.
   - Sort by Euclidean distance from board center (prefer central).
   - Dedupe within 40px horizontal / 30px vertical.
   - If overflow count > unique points, apply `(8px × i)` jitter to subsequent ones.
   - Overflow renders above tiled (higher z-index).

**`n > T2` — tuck.** Windows beyond T2 (chosen as oldest non-focused by `lastInteraction`) get `isTucked=true`. Sidebar opens.

### 7.3 The tuck sidebar

A panel slides in on the right edge of the board when `windows.some(w => w.isTucked)`. Header: "Tucked Chats". Vertical scroll. Each card:

- Accent color dot.
- Conversation name (reflects edits).
- Numeric badge `#N`.
- Preview: last 2–3 message text snippets, updated in real-time even while tucked.
- `×` button (destroys the conversation).
- Pulse dot if `unreadActivity` (autonomous-mode activity since user last viewed).

Click-to-evoke: removes from sidebar, places on board as an active window with focus, auto-tucks the oldest non-focused if at T2.

When the sidebar empties, it collapses and the board reclaims full width.

### 7.4 Drag between board and sidebar

- **Board → Sidebar (tuck by drag):** Right 12% strip of the board is the drop zone (expands to 20% if the sidebar is already open). Visual highlight while pointer is in zone. Release tucks; the window animates into a card in the list.
- **Sidebar → Board (evoke by drag):** `pointerdown` on a card spawns a translucent T1-cell-sized ghost following the cursor. Release on board → `evokeWindow(id, dropPos)`, auto-tuck oldest non-focused if at T2. Release on sidebar (or outside board) cancels.

### 7.5 Persistence of message history

Tucked windows have their DOM removed for performance — but the `LabWindow` data object persists in state, and the underlying biorouterd session retains all messages. Re-evoking re-renders full history.

---

## 8. Toolbar

Renders top-right of the board (right of the existing `≡ + 👥` cluster, doesn't fight the title-bar drag region).

| Control | Behavior |
|---|---|
| **Spawn** (+) | `spawnWindow()`: create new biorouterd session, append `LabWindow` with next badge & next palette color, auto-tuck oldest non-focused if at T2, focus the new window. |
| **Clear** | `clearAll()`: brief animation closes all windows; sessions persist in History. |
| **Organize** | `organize()`: clears all `position`/`size`/`isManuallyPlaced` and re-tiles. **Focused window retains top z-index and the pop effect.** |
| **T1 input** | Numeric stepper; reflow on change. |
| **T2 input** | Numeric stepper; constrained `≥ T1`. Lowering below current on-board count tucks excess (oldest non-focused). |
| **Status indicators** | Layout mode hint ("3×2 grid", "overlap", "compact") and a count: `N on board · M tucked`. |

---

## 9. Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `Cmd+N` / `Ctrl+N` | Spawn window (route-scoped — no global override). |
| `Cmd+W` / `Ctrl+W` | Close focused window. |
| `Tab` (within board) | Cycle focus to next window (future consideration; out of scope for v1 but reserved). |
| `Esc` | Cancel inline rename / drag. (Future: exit Lab Meeting Mode.) |

Shortcuts are bound at the `<LabMeetingRoute>` level via a `keydown` handler; they don't fire outside `/lab-meeting`.

---

## 10. Interaction with Existing BioRouter Features

### 10.1 Status bar

App-level `<LabMeetingStatusBar>` reflects focused window. Clicks on its model/mode/cwd selectors mutate only that window's per-window state. Token cost accumulates per window starting at 0.

### 10.2 Sidebar navigation

Clicking sidebar items (Home, Chat, History, Workflows, Scheduler, Extensions, Skills, Apps, Settings) navigates away normally. `LabMeetingProvider` lives above the route so state is preserved. A `<BackToLabMeetingPill>` shows in the top-left area while non-empty Lab Meeting state exists and the user is elsewhere.

### 10.3 Autonomous mode

- On-board, non-focused: window updates in real time (existing streaming).
- Tucked: card preview updates live; `unreadActivity = true` sets a pulse dot.
- Evoking re-renders full history; the conversation continues from where it was.

### 10.4 File and tool access

Each window operates with its own working directory and tool context. No implicit sharing. Cross-referencing is manual (copy-paste, file paths).

---

## 11. Performance

- **DOM:** only on-board windows mount. Tucked = data only.
- **Message virtualization:** existing `ProgressiveMessageList` already handles long histories; no change needed.
- **Concurrent AI requests:** independent `useChatStream` per window; biorouterd handles concurrency; UI shows per-window loading states.
- **Reflow:** `ResizeObserver` on board → debounced 80ms → `computeLayout`.
- **Animation budget:** 60fps. All window transitions use `transform` + `box-shadow` (compositor-friendly). Drag suppresses transitions for immediate feedback.

---

## 12. Edge Cases

| Scenario | Behavior |
|---|---|
| App window very small | If board < `minBoardW × minBoardH` (where minimum = one T1-cell size), tuck all but the focused window. |
| `T1 = 1` | All except first window are overflow at intersections (or tucked beyond T2). |
| `T1 = T2` | No overlap zone; (T1+1)th spawn tucks immediately. |
| All closed | Empty-board CTA: "Spawn a conversation". |
| Enter mode w/ no prior state | Auto-spawn one window so the board isn't empty. |
| Network disconnect mid-autonomous in tucked window | Existing reconnection path; tucked card shows last-received message; pulse dot persists. |
| User changes model in a window | Updates `windows[i].model` only. |
| Session deleted via History while tucked | On next `LabMeetingProvider` mount or on focus, the dead window is filtered out and removed from state. |

---

## 13. Files — created or modified

### Created (under [`ui/desktop/src/components/LabMeeting/`](../../../ui/desktop/src/components/LabMeeting/))
- `LabMeetingRoute.tsx`
- `LabMeetingBoard.tsx`
- `LabMeetingToolbar.tsx`
- `LabMeetingStatusBar.tsx`
- `ChatWindow.tsx`
- `WindowTitleBar.tsx`
- `ResizeHandle.tsx`
- `TuckSidebar.tsx`
- `TuckedCard.tsx`
- `BackToLabMeetingPill.tsx`
- `layoutEngine.ts` + `layoutEngine.test.ts`
- `palette.ts` (12-color accent palette + name generator)
- `useLabMeetingDrag.ts` (drag/resize hooks)
- `labMeetingStorage.ts` (debounced localStorage)
- `LabMeetingProvider.tsx` + `LabMeetingProvider.test.tsx`
- `index.ts`

### Created (context)
- [`ui/desktop/src/contexts/LabMeetingContext.tsx`](../../../ui/desktop/src/contexts/LabMeetingContext.tsx)

### Modified
- [`App.tsx`](../../../ui/desktop/src/App.tsx) — register `/lab-meeting` route; mount `LabMeetingProvider` at root.
- [`components/Layout/AppLayout.tsx`](../../../ui/desktop/src/components/Layout/AppLayout.tsx) — add `Users` icon button; render `BackToLabMeetingPill`.
- [`components/BaseChat.tsx`](../../../ui/desktop/src/components/BaseChat.tsx) — add `coherent` and `hideStatusBar` props.
- [`components/ChatInput.tsx`](../../../ui/desktop/src/components/ChatInput.tsx) — respect `hideStatusBar`; coherent visual mode.
- [`main.ts`](../../../ui/desktop/src/main.ts) — IPC `labMeeting:enter` (maximize) / `labMeeting:exit` (no-op currently).
- [`preload.ts`](../../../ui/desktop/src/preload.ts) — expose those IPCs.

### Not modified
Backend Rust crates are untouched. We only call existing `createSession` and chat-stream APIs.

---

## 14. Testing

### Unit
- `layoutEngine.test.ts` — assert grid configs for `n ∈ {1..8}`, intersection placement with overflow, dedupe + jitter, `T1=1` and `T1=T2` corner cases, manually-placed windows excluded from tile pass.

### Component
- `LabMeetingProvider.test.tsx` — spawn / close / tuck / evoke / focus state transitions; localStorage hydration; filtering of dead sessions.

### E2E (Playwright, follow [`ui/desktop/playwright/`](../../../ui/desktop/playwright/) conventions)
- Enter mode → board renders with one auto-spawned window.
- Spawn 3, 6, 8, 9 → assert tile / overflow / tuck transitions.
- Drag-to-tuck and drag-to-evoke.
- Inline rename round-trip.
- Close window animation completes; remaining tile.
- Navigate away and back preserves state.

---

## 15. Out of scope (v1)

- Cross-window message references / drag-and-drop content between windows.
- "Pin" a window to keep it on the board even if it would be tucked by oldest-non-focused.
- Tab-cycle focus shortcut (reserved).
- Esc-to-exit-mode (reserved).
- Multi-monitor / native OS window per chat.
- Auto-restoring BrowserWindow size on exit.

These are tracked here so future iterations have a clear scope boundary.

---

## 16. Reference — original prose spec

The original prose design doc inspired this spec. Anything in this document supersedes the prose where they diverge (most notably: routing, persistence, status-bar approach, and explicit file boundaries). Original is preserved in conversation history at the time of design approval.
