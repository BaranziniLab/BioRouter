# Dashboard fold mode — implementation plan (v4)

> **What this is.** The 13-task plan for a per-window "fold to card" mode in the
> dashboard: a global Fold toolbar toggle, a live busy/idle indicator driven by
> a new `onBusyChange` prop on `BaseChat`, and a muted accent palette.
> **Status:** Superseded by removal. `FoldedCard.tsx`, `DashboardProvider.tsx`,
> `DashboardContext.tsx` and the rest of the dashboard tree are gone as of
> 2026-07-18 — see the [removal record](README.md). Only the `BaseChat.tsx`
> touchpoint survives. The unticked `- [ ]` checkboxes below were ticked during
> execution in a working copy (fold mode shipped in v1.85.3); they are not a
> record of unfinished work.
> **Audience:** maintainers reading the dashboard-mode archive.
> **Cross-reference key.** Two schemes are used below and they are not
> interchangeable. The body refers to its own steps by **task number**
> ("Task 4 step 1"). The closing self-review maps back to the spec by **quoted
> heading** ("Section 'State model · DashboardWindow extensions'"), because the
> spec numbers no sections.

**Date:** 2026-05-29
**Spec:** [v4 — Dashboard fold mode design spec](v4-window-fold-mode-design.md)

> **Warning.** Task steps below pin edits to exact line ranges — for example
> `DashboardContext.tsx:3-21`, `palette.ts:1-14`, `palette.test.ts:5-13`. Those
> ranges were accurate against the working tree of 2026-05-29 only. The files
> themselves have since been deleted, so **no line range in this plan can be
> resolved against any current file**. Read them as pointers to the code shown
> inline in each step, which is complete on its own.

**Goal:** Add a per-window "fold to card" mode to the BioRouter dashboard, a global Fold toggle in the toolbar, a live busy/idle status indicator on cards, and replace the accent palette with muted, more cohesive colors.

**Architecture.** Five decisions carry the design:

- Extend the existing `DashboardWindow` shape with `folded` (persisted) and `isBusy` (transient).
- Add four new actions to `DashboardContext` — `foldWindow`, `foldAll`, `unfoldAll`, `setWindowBusy` — plus a derived `allFolded`.
- `ChatWindow` keeps its chat subtree mounted at all times, switching between the existing title bar / chat / resize render and a new compact `<FoldedCard>` when `folded === true`.
- `BaseChat` gets a single `onBusyChange` callback so the dashboard learns when its inner chat is streaming.
- `organize()` is untouched — it already respects per-window sizes.

**Tech stack:** React 19 + TypeScript, Vite, TailwindCSS, Vitest + React Testing Library, lucide-react icons.

> **Note for agentic workers.** REQUIRED SUB-SKILL: use
> `superpowers:subagent-driven-development` (recommended) or
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

---

## File structure

**New files:**
- `ui/desktop/src/components/Dashboard/FoldedCard.tsx` — compact card render for a folded window.

**Modified files:**
- `ui/desktop/src/components/Dashboard/palette.ts` — replace `ACCENT_PALETTE` with muted hues.
- `ui/desktop/src/components/Dashboard/palette.test.ts` — assert new palette values.
- `ui/desktop/src/contexts/DashboardContext.tsx` — extend `DashboardWindow`, `DashboardApi`.
- `ui/desktop/src/components/Dashboard/dashboardStorage.ts` — extend `SerializedDashboardWindow` with `folded`.
- `ui/desktop/src/components/Dashboard/DashboardProvider.tsx` — implement fold actions, busy action, derived `allFolded`; add `CARD_W` / `CARD_H` exports.
- `ui/desktop/src/components/Dashboard/DashboardProvider.test.tsx` — unit tests for new actions and derived state.
- `ui/desktop/src/components/icons/app-icons.tsx` — export `Minus` from lucide.
- `ui/desktop/src/components/Dashboard/WindowTitleBar.tsx` — add Fold button (leftmost of resize cluster).
- `ui/desktop/src/components/Dashboard/DashboardToolbar.tsx` — add Fold toggle button between Organize and Clear.
- `ui/desktop/src/components/Dashboard/ChatWindow.tsx` — branch on `folded`, wire `onFold`, pipe `isBusy` via `BaseChat.onBusyChange`.
- `ui/desktop/src/components/BaseChat.tsx` — add `onBusyChange` prop; fire on `chatState` transitions in/out of `Idle`.
- `ui/desktop/src/styles/main.css` — add `breathe` keyframes for the busy indicator.

**Single responsibility per file:** `FoldedCard` only renders + delegates clicks; `palette.ts` only defines the color set; `dashboardStorage.ts` only handles serialize/load/migrate; `DashboardProvider.tsx` owns state actions. No cross-cutting refactors.

---

## Task 1: Replace accent palette with muted hues

**Files:**
- Modify: `ui/desktop/src/components/Dashboard/palette.ts:1-14`
- Modify: `ui/desktop/src/components/Dashboard/palette.test.ts:5-13`

- [ ] **Step 1: Update the palette test to assert the new hex values**

Replace the body of the first `it(...)` block (palette.test.ts:5-13) with:

```ts
it('exposes 12 distinct muted hex colors', () => {
  expect(ACCENT_PALETTE).toEqual([
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
  ]);
  expect(new Set(ACCENT_PALETTE).size).toBe(12);
  for (const c of ACCENT_PALETTE) {
    expect(c).toMatch(/^#[0-9a-fA-F]{6}$/);
  }
});
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd ui/desktop && npm run test:run -- src/components/Dashboard/palette.test.ts
```

Expected: FAIL on `expect(ACCENT_PALETTE).toEqual([...])` — current palette is vibrant, new one is muted.

- [ ] **Step 3: Replace the palette**

Replace `ui/desktop/src/components/Dashboard/palette.ts:1-14` with:

```ts
export const ACCENT_PALETTE: readonly string[] = [
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

Leave `pickAccentColor` and `generateName` unchanged.

- [ ] **Step 4: Run the test to verify it passes**

```bash
cd ui/desktop && npm run test:run -- src/components/Dashboard/palette.test.ts
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cd <repo-root>
git add ui/desktop/src/components/Dashboard/palette.ts ui/desktop/src/components/Dashboard/palette.test.ts
git commit -m "feat(dashboard): switch accent palette to muted hues"
```

---

## Task 2: Extend DashboardWindow and DashboardApi types

**Files:**
- Modify: `ui/desktop/src/contexts/DashboardContext.tsx:3-21` (window shape)
- Modify: `ui/desktop/src/contexts/DashboardContext.tsx:52-101` (api shape)

- [ ] **Step 1: Add `folded` and `isBusy` to `DashboardWindow`**

In `ui/desktop/src/contexts/DashboardContext.tsx:3-21`, replace the interface with:

```ts
export interface DashboardWindow {
  windowId: string;
  sessionId: string;
  name: string;
  userSetName: boolean;
  badge: number;
  accentColor: string;
  /** World-space coordinates. Set at spawn; never null. */
  position: { x: number; y: number };
  size: { w: number; h: number };
  isManuallyPlaced: boolean;
  model?: string;
  mode?: string;
  cwd?: string;
  contextDepth?: number;
  costAccumulated?: number;
  lastInteraction: number;
  unreadActivity: boolean;
  /** When true, render as a compact FoldedCard instead of the full chat. */
  folded: boolean;
  /** Transient: true while the inner chat is streaming or running a tool. Not persisted. */
  isBusy: boolean;
}
```

- [ ] **Step 2: Add new methods + derived selector to `DashboardApi`**

In `ui/desktop/src/contexts/DashboardContext.tsx`, inside `DashboardApi` (between `markActivity` on line 100 and the closing brace), add:

```ts
  /** Toggle a single window between folded and unfolded. */
  foldWindow: (windowId: string, folded: boolean) => void;
  /** Set every on-canvas window's `folded` to true. */
  foldAll: () => void;
  /** Set every on-canvas window's `folded` to false. */
  unfoldAll: () => void;
  /** Mark a window's chat as actively streaming/running. Driven by BaseChat. */
  setWindowBusy: (windowId: string, busy: boolean) => void;
  /** Derived: every on-canvas window has folded=true. False when windows is empty. */
  allFolded: boolean;
```

- [ ] **Step 3: Verify type compiles**

```bash
cd ui/desktop && npx tsc --noEmit -p tsconfig.json 2>&1 | grep -E "(error|DashboardContext)" | head -20
```

Expected: errors about `DashboardProvider` not implementing the new fields/methods (we fix in Task 4). No errors mentioning `DashboardContext.tsx` itself.

- [ ] **Step 4: Commit**

```bash
cd <repo-root>
git add ui/desktop/src/contexts/DashboardContext.tsx
git commit -m "feat(dashboard): extend DashboardWindow + Api with fold/busy fields"
```

---

## Task 3: Persist `folded` in localStorage

**Files:**
- Modify: `ui/desktop/src/components/Dashboard/dashboardStorage.ts:12-29`
- Modify: `ui/desktop/src/components/Dashboard/dashboardStorage.ts:67-91` (migration)

- [ ] **Step 1: Add `folded` to `SerializedDashboardWindow`**

In `ui/desktop/src/components/Dashboard/dashboardStorage.ts:12-29`, replace the interface with:

```ts
export interface SerializedDashboardWindow {
  windowId: string;
  sessionId: string;
  name: string;
  userSetName: boolean;
  badge: number;
  accentColor: string;
  position: { x: number; y: number };
  size: { w: number; h: number };
  isManuallyPlaced: boolean;
  model?: string;
  mode?: string;
  cwd?: string;
  contextDepth?: number;
  costAccumulated?: number;
  lastInteraction: number;
  unreadActivity: boolean;
  /** Whether the window is currently rendered as a compact card. */
  folded?: boolean;
}
```

`folded` is optional so existing v2 payloads in localStorage that predate this change deserialize cleanly (the hydrate path defaults `undefined` → `false`).

- [ ] **Step 2: Default `folded` in v1→v2 migration**

In `ui/desktop/src/components/Dashboard/dashboardStorage.ts:67-91`, inside `migrateV1ToV2`, add `folded: false` to the returned window object (so migrated payloads have an explicit value):

```ts
return {
  windowId: w.windowId,
  sessionId: w.sessionId,
  name: w.name,
  userSetName: typeof w.userSetName === 'boolean' ? w.userSetName : false,
  badge: w.badge,
  accentColor: w.accentColor,
  position,
  size,
  isManuallyPlaced: true,
  model: w.model,
  mode: w.mode,
  cwd: w.cwd,
  contextDepth: w.contextDepth,
  costAccumulated: w.costAccumulated,
  lastInteraction: w.lastInteraction,
  unreadActivity: w.unreadActivity,
  folded: false,
};
```

- [ ] **Step 3: Run existing storage tests to verify nothing regressed**

```bash
cd ui/desktop && npm run test:run -- src/components/Dashboard/dashboardStorage.test.ts
```

Expected: PASS (no test changes; the new optional field doesn't break existing assertions).

- [ ] **Step 4: Commit**

```bash
cd <repo-root>
git add ui/desktop/src/components/Dashboard/dashboardStorage.ts
git commit -m "feat(dashboard): persist window.folded across reloads"
```

---

## Task 4: Implement fold actions and busy state in DashboardProvider

**Files:**
- Modify: `ui/desktop/src/components/Dashboard/DashboardProvider.tsx`

- [ ] **Step 1: Add CARD_W / CARD_H constants**

At the top of `ui/desktop/src/components/Dashboard/DashboardProvider.tsx`, just after the existing `MIN_WINDOW_*` constants (line 24-26), add:

```ts
// Folded-card geometry. Used by ChatWindow to render compact cards in place.
export const CARD_W = 240;
export const CARD_H = 72;
```

- [ ] **Step 2: Default `folded` and `isBusy` in `hydrate()`**

In `ui/desktop/src/components/Dashboard/DashboardProvider.tsx:41-61`, update both return branches of `hydrate()` to default the new fields. Replace the body of `hydrate()` with:

```ts
function hydrate(): DashboardState {
  const raw = loadDashboardState();
  if (!raw) {
    return {
      windows: [],
      focusedWindowId: null,
      cameraOffset: { x: 0, y: 0 },
      organizeTick: 0,
      isAnimating: false,
      isHydrating: false,
    };
  }
  return {
    windows: raw.windows.map((w) => ({
      ...w,
      folded: w.folded ?? false,
      isBusy: false,
    })),
    focusedWindowId: raw.focusedWindowId,
    cameraOffset: raw.cameraOffset ?? { x: 0, y: 0 },
    organizeTick: 0,
    isAnimating: false,
    isHydrating: false,
  };
}
```

- [ ] **Step 3: Strip `isBusy` in `serialize()`**

In `ui/desktop/src/components/Dashboard/DashboardProvider.tsx:32-39`, replace `serialize()` with:

```ts
function serialize(state: DashboardState): SerializedDashboardState {
  return {
    version: 2,
    windows: state.windows.map((w) => {
      // isBusy is transient; never persisted.
      // Spread excludes nothing; destructure to drop it.
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      const { isBusy: _isBusy, ...rest } = w;
      return { ...rest };
    }),
    focusedWindowId: state.focusedWindowId,
    cameraOffset: state.cameraOffset,
  };
}
```

- [ ] **Step 4: Default `folded` and `isBusy` on new spawns**

In `ui/desktop/src/components/Dashboard/DashboardProvider.tsx:195-211`, add the two fields to the `newWin` object (before `unreadActivity: false`):

```ts
      const newWin: DashboardWindow = {
        windowId: nextWindowId(),
        sessionId,
        name: overrideName || generateName(prev.windows.length),
        userSetName: Boolean(overrideName),
        badge: prev.windows.reduce((m, w) => Math.max(m, w.badge), 0) + 1,
        accentColor: pickAccentColor(usedColors),
        position: pos,
        size: { w: MIN_WINDOW_W, h: MIN_WINDOW_H },
        isManuallyPlaced: true,
        cwd,
        lastInteraction: now,
        unreadActivity: false,
        folded: false,
        isBusy: false,
      };
```

- [ ] **Step 5: Implement `foldWindow`, `foldAll`, `unfoldAll`, `setWindowBusy`**

In `ui/desktop/src/components/Dashboard/DashboardProvider.tsx`, just after `markActivity` (around line 426), add:

```ts
  const foldWindow: DashboardApi['foldWindow'] = useCallback((windowId, folded) => {
    setState((prev) => ({
      ...prev,
      windows: prev.windows.map((w) =>
        w.windowId === windowId && w.folded !== folded ? { ...w, folded } : w
      ),
    }));
  }, []);

  const foldAll: DashboardApi['foldAll'] = useCallback(() => {
    setState((prev) => ({
      ...prev,
      windows: prev.windows.map((w) => (w.folded ? w : { ...w, folded: true })),
    }));
  }, []);

  const unfoldAll: DashboardApi['unfoldAll'] = useCallback(() => {
    setState((prev) => ({
      ...prev,
      windows: prev.windows.map((w) => (!w.folded ? w : { ...w, folded: false })),
    }));
  }, []);

  const setWindowBusy: DashboardApi['setWindowBusy'] = useCallback((windowId, busy) => {
    setState((prev) => {
      let mutated = false;
      const windows = prev.windows.map((w) => {
        if (w.windowId !== windowId || w.isBusy === busy) return w;
        mutated = true;
        return { ...w, isBusy: busy };
      });
      return mutated ? { ...prev, windows } : prev;
    });
  }, []);
```

- [ ] **Step 6: Compute `allFolded` derived value and add to api**

In `ui/desktop/src/components/Dashboard/DashboardProvider.tsx`, just before the `const api = useMemo(...)` block (around line 428), add:

```ts
  const allFolded =
    state.windows.length > 0 && state.windows.every((w) => w.folded);
```

Then add the new fields to the `api` object (line 428-463) — both in the value and in the deps array. The full block becomes:

```ts
  const api: DashboardApi = useMemo(
    () => ({
      state,
      spawnWindow,
      closeWindow,
      focusWindow,
      renameWindow,
      syncSessionName,
      moveWindow,
      resizeWindow,
      freezeAllRects,
      organize,
      clearAll,
      panBy,
      centerOn,
      updateWindowField,
      markActivity,
      foldWindow,
      foldAll,
      unfoldAll,
      setWindowBusy,
      allFolded,
    }),
    [
      state,
      spawnWindow,
      closeWindow,
      focusWindow,
      renameWindow,
      syncSessionName,
      moveWindow,
      resizeWindow,
      freezeAllRects,
      organize,
      clearAll,
      panBy,
      centerOn,
      updateWindowField,
      markActivity,
      foldWindow,
      foldAll,
      unfoldAll,
      setWindowBusy,
      allFolded,
    ]
  );
```

- [ ] **Step 7: Build + typecheck**

```bash
cd ui/desktop && npx tsc --noEmit -p tsconfig.json 2>&1 | head -20
```

Expected: no errors related to `DashboardProvider.tsx` or `DashboardContext.tsx`. There may be unrelated pre-existing errors elsewhere in the tree — ignore them.

- [ ] **Step 8: Commit**

```bash
cd <repo-root>
git add ui/desktop/src/components/Dashboard/DashboardProvider.tsx
git commit -m "feat(dashboard): implement foldWindow/foldAll/unfoldAll/setWindowBusy"
```

---

## Task 5: Test the fold/busy provider actions

**Files:**
- Modify: `ui/desktop/src/components/Dashboard/DashboardProvider.test.tsx`

- [ ] **Step 1: Write failing tests for the new actions**

Append the following tests to the bottom of the existing `describe('DashboardProvider (canvas mode)', ...)` block in `ui/desktop/src/components/Dashboard/DashboardProvider.test.tsx`, just before the final closing `});`:

```ts
  describe('fold mode', () => {
    it('foldWindow flips a single window', async () => {
      const { result } = renderHook(() => useDashboard(), { wrapper });
      await act(async () => {
        await result.current.spawnWindow();
      });
      const id = result.current.state.windows[0].windowId;
      expect(result.current.state.windows[0].folded).toBe(false);
      act(() => result.current.foldWindow(id, true));
      expect(result.current.state.windows[0].folded).toBe(true);
      act(() => result.current.foldWindow(id, false));
      expect(result.current.state.windows[0].folded).toBe(false);
    });

    it('foldAll folds every window; unfoldAll unfolds every window', async () => {
      const { result } = renderHook(() => useDashboard(), { wrapper });
      await act(async () => {
        await result.current.spawnWindow();
        await result.current.spawnWindow();
        await result.current.spawnWindow();
      });
      act(() => result.current.foldAll());
      expect(result.current.state.windows.every((w) => w.folded)).toBe(true);
      act(() => result.current.unfoldAll());
      expect(result.current.state.windows.every((w) => !w.folded)).toBe(true);
    });

    it('allFolded reflects derived state', async () => {
      const { result } = renderHook(() => useDashboard(), { wrapper });
      expect(result.current.allFolded).toBe(false); // empty
      await act(async () => {
        await result.current.spawnWindow();
        await result.current.spawnWindow();
      });
      expect(result.current.allFolded).toBe(false);
      act(() => result.current.foldAll());
      expect(result.current.allFolded).toBe(true);
      const firstId = result.current.state.windows[0].windowId;
      act(() => result.current.foldWindow(firstId, false));
      expect(result.current.allFolded).toBe(false);
    });

    it('setWindowBusy updates only the matching window', async () => {
      const { result } = renderHook(() => useDashboard(), { wrapper });
      await act(async () => {
        await result.current.spawnWindow();
        await result.current.spawnWindow();
      });
      const [a, b] = result.current.state.windows.map((w) => w.windowId);
      act(() => result.current.setWindowBusy(a, true));
      const winA = result.current.state.windows.find((w) => w.windowId === a)!;
      const winB = result.current.state.windows.find((w) => w.windowId === b)!;
      expect(winA.isBusy).toBe(true);
      expect(winB.isBusy).toBe(false);
    });

    it('isBusy is not persisted across hydrate', async () => {
      const { result, unmount } = renderHook(() => useDashboard(), { wrapper });
      await act(async () => {
        await result.current.spawnWindow();
      });
      const id = result.current.state.windows[0].windowId;
      act(() => result.current.setWindowBusy(id, true));
      expect(result.current.state.windows[0].isBusy).toBe(true);
      unmount(); // flushes save synchronously via the unmount-cleanup effect
      const { result: result2 } = renderHook(() => useDashboard(), { wrapper });
      expect(result2.current.state.windows[0].isBusy).toBe(false);
    });

    it('folded is persisted across hydrate', async () => {
      const { result, unmount } = renderHook(() => useDashboard(), { wrapper });
      await act(async () => {
        await result.current.spawnWindow();
      });
      const id = result.current.state.windows[0].windowId;
      act(() => result.current.foldWindow(id, true));
      unmount();
      const { result: result2 } = renderHook(() => useDashboard(), { wrapper });
      expect(result2.current.state.windows[0].folded).toBe(true);
    });
  });
```

- [ ] **Step 2: Run the tests to verify they pass**

```bash
cd ui/desktop && npm run test:run -- src/components/Dashboard/DashboardProvider.test.tsx
```

Expected: PASS (the implementation from Task 4 already covers these). If any fail, fix the implementation before moving on — do not amend tests.

- [ ] **Step 3: Commit**

```bash
cd <repo-root>
git add ui/desktop/src/components/Dashboard/DashboardProvider.test.tsx
git commit -m "test(dashboard): cover fold/unfold/busy provider actions"
```

---

## Task 6: Export `Minus` icon

**Files:**
- Modify: `ui/desktop/src/components/icons/app-icons.tsx`

- [ ] **Step 1: Import `Minus` from lucide**

In `ui/desktop/src/components/icons/app-icons.tsx:7-101`, in the lucide-react import block, add `Minus as _Minus,` alphabetically (after `MessageSquareText`):

```ts
  MessageSquare as _MessageSquare,
  MessageSquareText as _MessageSquareText,
  Minus as _Minus,
  Monitor as _Monitor,
```

- [ ] **Step 2: Export the wrapped icon**

In the same file, just after `export const MessageSquareText = light(_MessageSquareText);` (line 170), add:

```ts
export const Minus = light(_Minus);
```

- [ ] **Step 3: Typecheck**

```bash
cd ui/desktop && npx tsc --noEmit -p tsconfig.json 2>&1 | grep -E "app-icons" | head -5
```

Expected: no errors mentioning `app-icons.tsx`.

- [ ] **Step 4: Commit**

```bash
cd <repo-root>
git add ui/desktop/src/components/icons/app-icons.tsx
git commit -m "feat(icons): export Minus icon"
```

---

## Task 7: Add `breathe` keyframes to global CSS

**Files:**
- Modify: `ui/desktop/src/styles/main.css`

- [ ] **Step 1: Append the keyframes**

At the end of `ui/desktop/src/styles/main.css`, append:

```css
@keyframes breathe {
  0%, 100% { opacity: 1; }
  50%      { opacity: 0.55; }
}

@keyframes breathe-pulse {
  0%   { transform: scale(1);   opacity: 0.45; }
  100% { transform: scale(1.6); opacity: 0;    }
}
```

- [ ] **Step 2: Commit**

```bash
cd <repo-root>
git add ui/desktop/src/styles/main.css
git commit -m "style(dashboard): add breathe keyframes for busy indicator"
```

---

## Task 8: Add Fold button to WindowTitleBar

**Files:**
- Modify: `ui/desktop/src/components/Dashboard/WindowTitleBar.tsx`

- [ ] **Step 1: Add `onFold` prop and `Minus` import**

In `ui/desktop/src/components/Dashboard/WindowTitleBar.tsx:1-2`, replace the icon import with:

```ts
import React, { useState, useRef, useEffect } from 'react';
import { X, Minimize2, Maximize2, Minus } from '../icons/app-icons';
```

Then extend the `Props` interface (line 4-12) to include `onFold`:

```ts
interface Props {
  name: string;
  accentColor: string;
  onRename: (name: string) => void;
  onClose: () => void;
  onShrink: () => void;
  onEnlarge: () => void;
  onFold: () => void;
  onPointerDownDrag: (e: React.PointerEvent<HTMLDivElement>) => void;
}
```

And destructure it in the component signature (line 14-22):

```ts
export const WindowTitleBar: React.FC<Props> = ({
  name,
  accentColor,
  onRename,
  onClose,
  onShrink,
  onEnlarge,
  onFold,
  onPointerDownDrag,
}) => {
```

- [ ] **Step 2: Insert Fold button leftmost of the right cluster**

In `ui/desktop/src/components/Dashboard/WindowTitleBar.tsx`, replace the comment + Shrink button (lines 79-87) with:

```tsx
      {/* Order: Fold | Shrink | Enlarge | Close — right-aligned. */}
      <button
        type="button"
        className={iconBtnClass}
        onClick={onFold}
        title="Fold to card"
      >
        <Minus className="w-3.5 h-3.5" />
      </button>
      <button
        type="button"
        className={iconBtnClass}
        onClick={onShrink}
        title="Shrink to minimum size"
      >
        <Minimize2 className="w-3.5 h-3.5" />
      </button>
```

- [ ] **Step 3: Typecheck**

```bash
cd ui/desktop && npx tsc --noEmit -p tsconfig.json 2>&1 | grep -E "WindowTitleBar|ChatWindow" | head -10
```

Expected: error on `ChatWindow.tsx` for not passing the new required `onFold` prop. That's wired in Task 10.

- [ ] **Step 4: Commit**

```bash
cd <repo-root>
git add ui/desktop/src/components/Dashboard/WindowTitleBar.tsx
git commit -m "feat(dashboard): add Fold button to WindowTitleBar"
```

---

## Task 9: Create FoldedCard component

**Files:**
- Create: `ui/desktop/src/components/Dashboard/FoldedCard.tsx`

- [ ] **Step 1: Write the component**

Create `ui/desktop/src/components/Dashboard/FoldedCard.tsx` with:

```tsx
import React from 'react';
import { X, Minimize2, Maximize2, Minus } from '../icons/app-icons';

interface Props {
  name: string;
  cwd?: string;
  accentColor: string;
  isBusy: boolean;
  onUnfold: () => void;
  onShrink: () => void;
  onEnlarge: () => void;
  onClose: () => void;
  onPointerDownDrag: (e: React.PointerEvent<HTMLDivElement>) => void;
}

const iconBtnClass =
  'flex-shrink-0 p-1 rounded hover:bg-background-medium/60 transition-colors';

export const FoldedCard: React.FC<Props> = ({
  name,
  cwd,
  accentColor,
  isBusy,
  onUnfold,
  onShrink,
  onEnlarge,
  onClose,
  onPointerDownDrag,
}) => {
  // 18% alpha → "2E"; 6% → "0F"; 28% (border) → "47"; 40% (pulse ring) → "66".
  const bg = `linear-gradient(135deg, ${accentColor}2E, ${accentColor}0F)`;
  const border = `${accentColor}47`;

  const indicator = isBusy ? (
    <span
      className="relative inline-flex w-2.5 h-2.5 flex-shrink-0"
      aria-label="busy"
    >
      <span
        className="absolute inset-0 rounded-full"
        style={{ backgroundColor: accentColor, animation: 'breathe 1.4s ease-in-out infinite' }}
      />
      <span
        className="absolute inset-0 rounded-full"
        style={{
          backgroundColor: `${accentColor}66`,
          animation: 'breathe-pulse 1.4s ease-out infinite',
        }}
      />
    </span>
  ) : (
    <span
      className="inline-block w-2.5 h-2.5 rounded-full flex-shrink-0"
      style={{ border: `1.5px solid ${accentColor}`, backgroundColor: 'transparent' }}
      aria-label="idle"
    />
  );

  return (
    <div
      className="h-full w-full rounded-2xl overflow-hidden select-none cursor-grab active:cursor-grabbing flex flex-col"
      style={{
        background: bg,
        border: `1px solid ${border}`,
      }}
      onPointerDown={(e) => {
        if ((e.target as HTMLElement).closest('button')) return;
        onPointerDownDrag(e);
      }}
      onClick={(e) => {
        if ((e.target as HTMLElement).closest('button')) return;
        onUnfold();
      }}
      title="Click to unfold"
    >
      {/* Row 1: status · title · buttons */}
      <div className="flex items-center gap-2 px-3 pt-2">
        {indicator}
        <span className="flex-1 min-w-0 truncate text-sm font-medium">{name}</span>
        <button
          type="button"
          className={iconBtnClass}
          onClick={(e) => { e.stopPropagation(); onUnfold(); }}
          title="Unfold"
        >
          <Minus className="w-3.5 h-3.5" />
        </button>
        <button
          type="button"
          className={iconBtnClass}
          onClick={(e) => { e.stopPropagation(); onShrink(); }}
          title="Shrink to minimum size"
        >
          <Minimize2 className="w-3.5 h-3.5" />
        </button>
        <button
          type="button"
          className={iconBtnClass}
          onClick={(e) => { e.stopPropagation(); onEnlarge(); }}
          title="Enlarge"
        >
          <Maximize2 className="w-3.5 h-3.5" />
        </button>
        <button
          type="button"
          className={iconBtnClass}
          onClick={(e) => { e.stopPropagation(); onClose(); }}
          title="Close"
        >
          <X className="w-3.5 h-3.5" />
        </button>
      </div>
      {/* Row 2: working directory */}
      <div className="px-3 pb-2 mt-0.5 text-[11px] font-mono text-text-muted/80 truncate">
        {cwd ?? ''}
      </div>
    </div>
  );
};
```

- [ ] **Step 2: Typecheck**

```bash
cd ui/desktop && npx tsc --noEmit -p tsconfig.json 2>&1 | grep -E "FoldedCard" | head -10
```

Expected: no errors mentioning `FoldedCard.tsx`.

- [ ] **Step 3: Commit**

```bash
cd <repo-root>
git add ui/desktop/src/components/Dashboard/FoldedCard.tsx
git commit -m "feat(dashboard): add FoldedCard component"
```

---

## Task 10: Wire ChatWindow to render FoldedCard when `folded`

**Files:**
- Modify: `ui/desktop/src/components/Dashboard/ChatWindow.tsx`

- [ ] **Step 1: Import FoldedCard and CARD_W/CARD_H**

At the top of `ui/desktop/src/components/Dashboard/ChatWindow.tsx`, after the existing imports (line 1-12), add:

```ts
import { FoldedCard } from './FoldedCard';
import { CARD_W, CARD_H } from './DashboardProvider';
```

- [ ] **Step 2: Compute effective render rect and busy state**

In `ui/desktop/src/components/Dashboard/ChatWindow.tsx`, just before the `stylePos` useMemo (around line 100), add:

```ts
  const effectiveW = win.folded ? CARD_W : rect.w;
  const effectiveH = win.folded ? CARD_H : rect.h;
```

Then update `stylePos` (line 102-111) to use these effective dimensions instead of `rect.w` / `rect.h`:

```ts
  const stylePos = useMemo(
    () => ({
      transform: `translate(${tx}px, ${ty}px)`,
      width: effectiveW + (win.folded ? 0 : resizeDelta.dw),
      height: effectiveH + (win.folded ? 0 : resizeDelta.dh),
      zIndex: rect.zIndex,
      transition,
    }),
    [tx, ty, effectiveW, effectiveH, rect.zIndex, resizeDelta, transition, win.folded]
  );
```

- [ ] **Step 3: Branch the render on `win.folded`, keeping BaseChat mounted**

The chat subtree (`ChatProvider` + `BaseChat`) must stay mounted regardless of `folded` so streaming state, message history, and the `onBusyChange` effect continue to update while a card is showing. The strategy: always render the chat subtree; layer the `FoldedCard` on top when folded, and hide the title bar + chat surface via `display: none`.

In `ui/desktop/src/components/Dashboard/ChatWindow.tsx`, replace the entire returned JSX (line 122-220) with:

```tsx
  return (
    <div
      className={`absolute top-0 left-0 rounded-2xl bg-background-default border border-border-subtle/30 overflow-hidden transition-shadow ${focusClasses}`}
      style={{ ...stylePos, ...popStyle }}
      onMouseDown={(e) => {
        if (isFocused) return;
        const target = e.target as HTMLElement;
        const isInteractive =
          target.closest(
            'button, input, textarea, select, a, [role="button"], [data-radix-popper-content-wrapper], [data-slot^="dropdown-menu"]'
          ) !== null;
        if (isInteractive) return;
        dashboard.focusWindow(win.windowId);
      }}
    >
      {/* Full window chrome — hidden while folded so BaseChat stays mounted. */}
      <div
        className="absolute inset-0 flex flex-col"
        style={{ display: win.folded ? 'none' : 'flex' }}
      >
        <WindowTitleBar
          name={win.name}
          accentColor={win.accentColor}
          onRename={(name) => dashboard.renameWindow(win.windowId, name)}
          onClose={() => dashboard.closeWindow(win.windowId)}
          onShrink={() => {
            if (!isFocused) dashboard.focusWindow(win.windowId);
            dashboard.resizeWindow(
              win.windowId,
              { w: minSize.w, h: minSize.h },
              { x: rect.x, y: rect.y }
            );
          }}
          onEnlarge={() => {
            if (!isFocused) dashboard.focusWindow(win.windowId);
            dashboard.resizeWindow(
              win.windowId,
              { w: ENLARGE_W, h: ENLARGE_H },
              { x: rect.x, y: rect.y }
            );
          }}
          onFold={() => dashboard.foldWindow(win.windowId, true)}
          onPointerDownDrag={dragStart}
        />
        <div className="flex-1 min-h-0 relative">
          <ChatProvider chat={chat} setChat={setChat} contextKey={`dashboard-${win.sessionId}`}>
            <BaseChat
              setChat={setChat}
              sessionId={win.sessionId}
              suppressEmptyState={false}
              coherent
              hideSessionNamePill
              compactPicker
              showPopularTopics={false}
              accentColor={win.accentColor}
              onBusyChange={(busy) => dashboard.setWindowBusy(win.windowId, busy)}
              onRenameSession={(newName) => {
                dashboard.renameWindow(win.windowId, newName);
                announceSessionName({
                  sessionId: win.sessionId,
                  name: newName,
                  userSetName: true,
                  origin: 'user',
                });
                void renameSession(win.sessionId, newName, 'user').catch((err) => {
                  if (win.name) {
                    dashboard.renameWindow(win.windowId, win.name);
                    announceSessionName({
                      sessionId: win.sessionId,
                      name: win.name,
                      userSetName: win.userSetName,
                      origin: 'sync',
                    });
                  }
                  toastError({
                    title: 'Failed to rename session',
                    msg: errorMessage(err),
                  });
                });
              }}
              onSessionUpdate={(s) => {
                if (s?.name) {
                  dashboard.syncSessionName(win.windowId, s.name, {
                    userSetName: s.userSetName,
                  });
                }
              }}
            />
          </ChatProvider>
        </div>
        <ResizeHandle onPointerDown={resizeStart} />
      </div>
      {/* Folded card — layered on top when folded. */}
      {win.folded && (
        <div className="absolute inset-0">
          <FoldedCard
            name={win.name}
            cwd={win.cwd}
            accentColor={win.accentColor}
            isBusy={win.isBusy}
            onUnfold={() => {
              if (!isFocused) dashboard.focusWindow(win.windowId);
              dashboard.foldWindow(win.windowId, false);
            }}
            onShrink={() => {
              if (!isFocused) dashboard.focusWindow(win.windowId);
              dashboard.foldWindow(win.windowId, false);
              dashboard.resizeWindow(
                win.windowId,
                { w: minSize.w, h: minSize.h },
                { x: rect.x, y: rect.y }
              );
            }}
            onEnlarge={() => {
              if (!isFocused) dashboard.focusWindow(win.windowId);
              dashboard.foldWindow(win.windowId, false);
              dashboard.resizeWindow(
                win.windowId,
                { w: ENLARGE_W, h: ENLARGE_H },
                { x: rect.x, y: rect.y }
              );
            }}
            onClose={() => dashboard.closeWindow(win.windowId)}
            onPointerDownDrag={dragStart}
          />
        </div>
      )}
    </div>
  );
```

This preserves the spec's "BaseChat stays mounted" contract: the chat subtree is always rendered. When folded, it's `display: none` but its hooks (including the `onBusyChange` effect added in Task 11) continue running, so `isBusy` keeps updating live.

**Caveat on focusing the input after unfold:** because BaseChat stays mounted, its chat-input element is the same DOM node before fold and after unfold. There's no remount-driven autofocus — focus survives if the user had it before fold and isn't actively re-grabbed on unfold. The spec calls for "cursor focused on the input box" after unfold. The minimal way to get there: extend `BaseChat` with an imperative `focusInput()` ref or a `focusTrigger` prop that increments on unfold and calls `inputRef.current?.focus()` in an effect. **Out of scope for this plan** — the spec was approved knowing BaseChat's default mount behavior focuses the input on first render. After unfold, the input remains focusable on click. If user testing in Task 13 step 3 finds the focus behavior unacceptable, add a Task 14 to wire an explicit focus trigger.

- [ ] **Step 4: Typecheck**

```bash
cd ui/desktop && npx tsc --noEmit -p tsconfig.json 2>&1 | grep -E "ChatWindow|BaseChat" | head -20
```

Expected: error on `BaseChat` not accepting `onBusyChange` — fixed in Task 11.

- [ ] **Step 5: Commit**

```bash
cd <repo-root>
git add ui/desktop/src/components/Dashboard/ChatWindow.tsx
git commit -m "feat(dashboard): render FoldedCard when window.folded is true"
```

---

## Task 11: Add `onBusyChange` prop to BaseChat

**Files:**
- Modify: `ui/desktop/src/components/BaseChat.tsx`

- [ ] **Step 1: Add the prop to `BaseChatProps`**

In `ui/desktop/src/components/BaseChat.tsx`, find `BaseChatProps` (line 51-83) and add `onBusyChange` to the interface, just after `compactPicker`:

```ts
  compactPicker?: boolean;
  /** Fires when the inner chat transitions between idle and any non-idle state
   * (streaming, thinking, tool-running, etc.). Used by DashboardContext to
   * drive the per-window busy indicator on folded cards. */
  onBusyChange?: (busy: boolean) => void;
```

- [ ] **Step 2: Destructure the prop**

In `BaseChatContent` (line 87-101), add `onBusyChange` to the destructure:

```ts
function BaseChatContent({
  setChat,
  renderHeader,
  customChatInputProps = {},
  customMainLayoutProps = {},
  sessionId,
  initialMessage,
  coherent = true,
  onRenameSession,
  onSessionUpdate,
  accentColor,
  hideSessionNamePill = false,
  compactPicker = false,
  showPopularTopics: showPopularTopicsProp = true,
  onBusyChange,
}: BaseChatProps) {
```

- [ ] **Step 3: Fire the callback when `chatState` changes**

In `ui/desktop/src/components/BaseChat.tsx`, just after the `useChatStream({...})` destructure block (around line 144, immediately before `// Generate command history...`), add:

```ts
  // Pipe chatState transitions to the parent (dashboard window). Busy = any
  // non-idle state (Thinking, Streaming, WaitingForUserInput, Compacting, etc.).
  // ChatState.LoadingConversation counts as busy too — the session is still
  // resolving and the user should see that as activity.
  useEffect(() => {
    if (!onBusyChange) return;
    onBusyChange(chatState !== ChatState.Idle);
  }, [chatState, onBusyChange]);
```

Verify `ChatState` is imported. If not present at the top of the file, add to existing imports (look for `from '../types/chatState'`; the file uses `chatState` from `useChatStream`, so the enum may already be imported). If `ChatState` isn't imported, add this import near the top:

```ts
import { ChatState } from '../types/chatState';
```

- [ ] **Step 4: Typecheck**

```bash
cd ui/desktop && npx tsc --noEmit -p tsconfig.json 2>&1 | grep -E "BaseChat|ChatWindow" | head -10
```

Expected: no errors mentioning `BaseChat.tsx` or `ChatWindow.tsx`.

- [ ] **Step 5: Commit**

```bash
cd <repo-root>
git add ui/desktop/src/components/BaseChat.tsx
git commit -m "feat(chat): emit onBusyChange when chatState leaves Idle"
```

---

## Task 12: Add Fold toggle to DashboardToolbar

**Files:**
- Modify: `ui/desktop/src/components/Dashboard/DashboardToolbar.tsx`

- [ ] **Step 1: Add the Fold toggle button**

Replace the entire body of `ui/desktop/src/components/Dashboard/DashboardToolbar.tsx` (lines 1-49) with:

```tsx
import React from 'react';
import { useDashboard } from '../../contexts/DashboardContext';

export const DashboardToolbar: React.FC = () => {
  const dashboard = useDashboard();
  const onCanvas = dashboard.state.windows.length;
  const allFolded = dashboard.allFolded;

  const btnClass =
    'no-drag h-7 px-3 text-[13.5px] font-normal rounded-md ' +
    'text-text-default/80 hover:text-text-default hover:bg-background-medium/40 ' +
    'active:translate-y-px transition-colors';

  // Mini switch styling — only visible inside the Fold button.
  const switchTrack =
    'relative inline-block w-[22px] h-[12px] rounded-full transition-colors ' +
    (allFolded ? 'bg-text-default/70' : 'bg-background-medium');
  const switchThumb =
    'absolute top-[2px] w-[8px] h-[8px] rounded-full bg-background-default transition-all ' +
    (allFolded ? 'left-[12px]' : 'left-[2px]');

  return (
    <div className="relative z-[60] flex items-center gap-2 px-4 py-1.5 border-b border-border-subtle/30 bg-background-muted/40 backdrop-blur-sm">
      <div className="absolute left-1/2 -translate-x-1/2 flex items-center gap-2 no-drag">
        <button
          type="button"
          onClick={() => dashboard.spawnWindow()}
          title="Spawn (⌘⇧N)"
          className={btnClass}
        >
          Spawn
        </button>
        <button
          type="button"
          onClick={() => dashboard.organize()}
          title="Resolve overlaps and center on focused window"
          className={btnClass}
        >
          Organize
        </button>
        <button
          type="button"
          onClick={() => (allFolded ? dashboard.unfoldAll() : dashboard.foldAll())}
          title={allFolded ? 'Unfold all windows' : 'Fold all to cards'}
          aria-pressed={allFolded}
          className={`${btnClass} inline-flex items-center gap-2`}
          disabled={onCanvas === 0}
        >
          Fold
          <span className={switchTrack} aria-hidden="true">
            <span className={switchThumb} />
          </span>
        </button>
        <button
          type="button"
          onClick={() => dashboard.clearAll()}
          title="Close all"
          className={btnClass}
        >
          Clear
        </button>
      </div>
      <div className="ml-auto flex items-center gap-2 no-drag text-xs text-text-muted">
        {onCanvas} on canvas
      </div>
    </div>
  );
};
```

- [ ] **Step 2: Typecheck**

```bash
cd ui/desktop && npx tsc --noEmit -p tsconfig.json 2>&1 | grep -E "DashboardToolbar" | head -5
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
cd <repo-root>
git add ui/desktop/src/components/Dashboard/DashboardToolbar.tsx
git commit -m "feat(dashboard): add Fold toggle to toolbar"
```

---

## Task 13: Manual smoke test in the running app

**Files:** (no files modified; verification only)

- [ ] **Step 1: Run lint + format checks**

```bash
cd ui/desktop && npm run lint:check
```

Expected: PASS. If there are ESLint or Prettier complaints, fix and re-run, then commit any fixes with `style(dashboard): lint fixes`.

- [ ] **Step 2: Run the full Vitest suite**

```bash
cd ui/desktop && npm run test:run
```

Expected: PASS for everything Dashboard-related. Unrelated failures (if any pre-existed) are out of scope — note them but don't fix.

- [ ] **Step 3: Run the GUI**

```bash
cd <repo-root>
just run-dev
```

Wait until the Electron window is up.

- [ ] **Step 4: Walk the smoke test**

In the running app, navigate to the Dashboard and verify each item below. Note any failures as a follow-up task at the bottom of this plan rather than amending earlier tasks.

1. **Palette:** Spawn 3 windows. The 3 accent dots should appear in muted hues (sage, dusk, wheat — first three from the new palette).
2. **Title-bar fold:** Click the `−` (Minus) button in the title bar of one window. It collapses to a 240×72 card at the same top-left position. Card background is a muted gradient in the window's accent color; the dot is replaced by a hollow ring.
3. **Click-to-unfold:** Click anywhere on the card body (not on a button). The card expands back to the prior size at the same position. The chat input is focused (cursor blinking inside it).
4. **Live busy indicator:** Unfold the card, send a message. While the agent is generating, fold the card (via the `−` button). Because BaseChat stays mounted (just hidden) while folded, `onBusyChange` continues firing — the status indicator shows the pulsing filled dot for the entire duration of streaming. When the agent stops, the indicator returns to the hollow ring without any user interaction.
5. **Card resize buttons:** Click the `▭` button on a folded card — card unfolds to min size. Click `◻` on a folded card — unfolds to enlarge size. Click `✕` — closes the window.
6. **Toolbar toggle:** With 3 windows on canvas (mix of folded and unfolded), click "Fold" in the toolbar. The switch thumb slides right; all windows fold. Click again — switch slides left; all unfold.
7. **Spawn while toggle is ON:** Click "Fold" to fold all. Spawn a new window. The toggle should flip back to OFF state (since the new window is unfolded by default).
8. **Organize:** Fold 2 of 3 windows, leave 1 unfolded. Click "Organize". The 1 full window and 2 cards pack adjacent to one another without overlap.
9. **Drag a card:** Drag a folded card by its body — drag works the same as dragging a window.
10. **Persistence:** Fold one window, then close + reopen the app via `just run-dev`. The folded state survives. The busy indicator resets to "idle" on reopen.

- [ ] **Step 5: If everything passes, commit a "verified" marker (no code change)**

If all 10 smoke checks pass, append a CHANGELOG note or simply commit the existing `Cargo.lock` and `package-lock.json` updates produced by the dev build, with message:

```bash
cd <repo-root>
git add -u
git commit --allow-empty -m "chore(dashboard): manual fold-mode smoke test passed"
```

If any step fails, add a Task 14+ to this plan with the specific fix needed before proceeding.

---

## Self-review notes

**Spec coverage** (cross-checked against the [v4 design spec](v4-window-fold-mode-design.md)):

- Section "State Model · DashboardWindow extensions" → Task 2.
- Section "State Model · DashboardContext API additions" → Task 2 + Task 4.
- Section "State Model · Toolbar toggle behavior" → Task 12.
- Section "Geometry" → Task 4 step 1 + Task 10 step 2.
- Section "Components · DashboardToolbar.tsx" → Task 12.
- Section "Components · WindowTitleBar.tsx" → Task 8.
- Section "Components · ChatWindow.tsx" → Task 10.
- Section "Components · FoldedCard.tsx" → Task 9.
- Section "Components · palette.ts" → Task 1.
- Section "Components · BaseChat.tsx" → Task 11.
- Section "Data Flow" → Tasks 10 + 11.
- Section "Persistence" → Task 3 + Task 4 step 2/3.
- Section "Edge Cases" → covered behaviorally; tested in Task 5 + Task 13.

**Spec adherence:** Task 10 keeps the chat subtree mounted while folded (via `display: none` on the chrome wrapper), matching the spec's explicit requirement. The one open item is auto-focusing the chat input after unfold — Task 10 step 3 documents that the existing BaseChat mount-focus path doesn't fire on unfold (since BaseChat never unmounts) and notes a follow-up Task 14 path (`focusTrigger` prop) if smoke testing finds the focus behavior wanting.

**Type consistency check:** new methods named identically across spec, context interface, provider implementation, and toolbar consumer: `foldWindow`, `foldAll`, `unfoldAll`, `setWindowBusy`, `allFolded`. New constants: `CARD_W`, `CARD_H` — used in `DashboardProvider.tsx` (exported) and consumed in `ChatWindow.tsx`. New prop names: `onFold` (WindowTitleBar), `onBusyChange` (BaseChat) — single canonical names, no synonyms.

**Placeholder scan:** clean. Every step has full code blocks; every command has explicit expected output.

## Related documentation

- [v4 — Dashboard fold mode design spec](v4-window-fold-mode-design.md) — the spec this plan implements, and the target of the self-review's quoted-heading references.
- [v3 — Canvas dashboard implementation plan](v3-infinite-canvas-plan.md) — builds the canvas, `canvasLayout.ts` and `organize()` that this plan folds cards onto.
- [Dashboard mode — removal record and archive index](README.md) — records fold mode shipping in v1.85.3 and its removal on 2026-07-18.
- [v2 — Dashboard Mode implementation plan](v2-dashboard-mode-plan.md) — introduces the `palette.ts` and `DashboardProvider` that Tasks 1 and 4 modify.
