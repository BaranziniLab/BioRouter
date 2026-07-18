# Canvas dashboard — implementation plan (v3)

> **What this is.** The 13-task plan for converting the dashboard from a
> tile/tuck board into an infinite pannable canvas: camera offsets, spiral spawn
> placement, and Shrink/Enlarge window chrome.
> **Status:** Superseded, and then removed. Every file this plan modifies —
> `DashboardContext.tsx`, `DashboardProvider.tsx`, `canvasLayout.ts`,
> `TuckSidebar.tsx` — has been deleted along with the whole dashboard;
> `ui/desktop/src/components/Dashboard/` no longer exists. See the
> [removal record](README.md). The unticked `- [ ]` checkboxes below were ticked
> during execution in a working copy; they are not a record of unfinished work.
> **Audience:** maintainers reading the dashboard-mode archive.
> **Checkbox key.** Two conventions coexist below and mean opposite things. Task
> steps use `- [ ]` — work to be done by the executing agent. The closing
> "Self-review checklist" uses `- [x]` — checks the plan's *author* had already
> run before handing the plan over.

**Date:** 2026-05-10
**Spec:** [v3 — Canvas dashboard design spec](v3-infinite-canvas-design.md)

**Goal:** Convert the dashboard from a tile/tuck board into an infinite pannable canvas. Remove tucking entirely. Add window Shrink/Enlarge chrome. Move the `>` picker collapse from horizontal-inline to a vertical popup. Restyle Spawn/Organize/Clear buttons as tab-style.

**Architecture:** Camera-offset model — windows live in world coordinates, the viewport applies a `translate` to a world layer. Spawn places windows non-overlappingly via spiral search; Organize is iterative minimum-move overlap resolution; both recenter camera on the focused/new window.

**Tech stack:** React 19 + TypeScript, Tailwind, Radix Popover, Vitest.

> **Note for agentic workers.** REQUIRED SUB-SKILL: use
> `superpowers:subagent-driven-development` (recommended) or
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

---

## File structure

**Files to modify:**
- `ui/desktop/src/contexts/DashboardContext.tsx` — types: drop tuck/T1/T2, add cameraOffset + pan/center API
- `ui/desktop/src/components/Dashboard/DashboardProvider.tsx` — spawn placement, organize, pan, migration of tuck removal
- `ui/desktop/src/components/Dashboard/DashboardBoard.tsx` — infinite canvas viewport, pan handlers, render world layer
- `ui/desktop/src/components/Dashboard/DashboardToolbar.tsx` — tab-style buttons, remove "tucked" count
- `ui/desktop/src/components/Dashboard/ChatWindow.tsx` — wire Shrink/Enlarge, drop tuck-by-drag
- `ui/desktop/src/components/Dashboard/WindowTitleBar.tsx` — render Shrink/Enlarge/Close
- `ui/desktop/src/components/Dashboard/dashboardStorage.ts` — v1 → v2 migration
- `ui/desktop/src/components/Dashboard/layoutEngine.ts` — leave; export `MIN_WINDOW_W/H` constants for shared use, otherwise no-op
- `ui/desktop/src/components/Dashboard/index.ts` — drop tuck-related exports
- `ui/desktop/src/components/ChatInput.tsx` — vertical Popover for expanded picker group

**Files to create:**
- `ui/desktop/src/components/Dashboard/canvasLayout.ts` — pure functions for spiral placement and organize
- `ui/desktop/src/components/Dashboard/canvasLayout.test.ts` — unit tests
- `ui/desktop/src/components/ui/popover.tsx` — Radix Popover wrapper (if not already present)

**Files to delete:**
- `ui/desktop/src/components/Dashboard/TuckSidebar.tsx`
- `ui/desktop/src/components/Dashboard/TuckedCard.tsx`
- `ui/desktop/src/components/Dashboard/HiddenChatHolder.tsx`

---

## Task 1: Add canvasLayout module with spiral spawn placement

**Files:**
- Create: `ui/desktop/src/components/Dashboard/canvasLayout.ts`
- Create: `ui/desktop/src/components/Dashboard/canvasLayout.test.ts`

- [ ] **Step 1: Write the failing test**

```ts
// canvasLayout.test.ts
import { describe, it, expect } from 'vitest';
import { findSpawnPosition, organize } from './canvasLayout';

describe('findSpawnPosition', () => {
  it('returns the camera center when no windows exist', () => {
    const pos = findSpawnPosition({ center: { x: 100, y: 200 }, size: { w: 520, h: 440 }, existing: [] });
    expect(pos).toEqual({ x: 100 - 260, y: 200 - 220 });
  });

  it('spirals outward when the center overlaps an existing window', () => {
    const existing = [{ x: -260, y: -220, w: 520, h: 440 }];
    const pos = findSpawnPosition({ center: { x: 0, y: 0 }, size: { w: 520, h: 440 }, existing });
    // Should be offset by at least one cell + gap in some direction
    const dx = Math.abs(pos.x - (-260));
    const dy = Math.abs(pos.y - (-220));
    expect(dx + dy).toBeGreaterThan(520);
  });
});

describe('organize', () => {
  it('separates overlapping windows without resizing them', () => {
    const windows = [
      { id: 'a', x: 0, y: 0, w: 520, h: 440 },
      { id: 'b', x: 100, y: 100, w: 520, h: 440 },
    ];
    const result = organize(windows, 'a', 16);
    const a = result.find((w) => w.id === 'a')!;
    const b = result.find((w) => w.id === 'b')!;
    // sizes preserved
    expect(a.w).toBe(520); expect(a.h).toBe(440);
    expect(b.w).toBe(520); expect(b.h).toBe(440);
    // anchor (a) is unmoved
    expect(a.x).toBe(0); expect(a.y).toBe(0);
    // overlap resolved
    const overlapW = Math.max(0, Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x));
    const overlapH = Math.max(0, Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y));
    expect(overlapW === 0 || overlapH === 0).toBe(true);
  });

  it('leaves non-overlapping windows untouched', () => {
    const windows = [
      { id: 'a', x: 0, y: 0, w: 200, h: 200 },
      { id: 'b', x: 300, y: 0, w: 200, h: 200 },
    ];
    const result = organize(windows, 'a', 16);
    expect(result).toEqual(windows);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ui/desktop && npx vitest run src/components/Dashboard/canvasLayout.test.ts`
Expected: FAIL with "Cannot find module './canvasLayout'".

- [ ] **Step 3: Write minimal implementation**

```ts
// canvasLayout.ts
export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface WindowRect extends Rect {
  id: string;
}

function overlap(a: Rect, b: Rect): { w: number; h: number } {
  const w = Math.max(0, Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x));
  const h = Math.max(0, Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y));
  return { w, h };
}

interface SpawnParams {
  center: { x: number; y: number };
  size: { w: number; h: number };
  existing: Rect[];
  gap?: number;
}

export function findSpawnPosition({ center, size, existing, gap = 16 }: SpawnParams): {
  x: number;
  y: number;
} {
  // Start at the camera center (translated so the window center sits on it).
  const baseX = center.x - size.w / 2;
  const baseY = center.y - size.h / 2;
  const stepX = size.w + gap;
  const stepY = size.h + gap;
  const collides = (x: number, y: number) =>
    existing.some((r) => {
      const ov = overlap({ x, y, w: size.w, h: size.h }, r);
      return ov.w > 0 && ov.h > 0;
    });

  if (!collides(baseX, baseY)) return { x: baseX, y: baseY };

  // Spiral: expand a square ring at radius r = 1, 2, 3...
  for (let r = 1; r <= 20; r++) {
    for (let dy = -r; dy <= r; dy++) {
      for (let dx = -r; dx <= r; dx++) {
        // Only test the ring boundary (skip interior already tested at smaller r)
        if (Math.max(Math.abs(dx), Math.abs(dy)) !== r) continue;
        const x = baseX + dx * stepX;
        const y = baseY + dy * stepY;
        if (!collides(x, y)) return { x, y };
      }
    }
  }
  // Fallback: place far right of bbox
  const maxX = existing.reduce((m, r) => Math.max(m, r.x + r.w), baseX);
  return { x: maxX + gap, y: baseY };
}

export function organize(windows: readonly WindowRect[], anchorId: string, gap = 16): WindowRect[] {
  const result = windows.map((w) => ({ ...w }));
  const MAX_PASSES = 12;
  for (let pass = 0; pass < MAX_PASSES; pass++) {
    let moved = false;
    for (let i = 0; i < result.length; i++) {
      for (let j = i + 1; j < result.length; j++) {
        const a = result[i];
        const b = result[j];
        const ov = overlap(a, b);
        if (ov.w <= 0 || ov.h <= 0) continue;
        // Push along shorter axis
        const axis: 'x' | 'y' = ov.w < ov.h ? 'x' : 'y';
        const pushTotal = (axis === 'x' ? ov.w : ov.h) + gap;
        const aIsAnchor = a.id === anchorId;
        const bIsAnchor = b.id === anchorId;
        const aShare = aIsAnchor ? 0 : bIsAnchor ? pushTotal : pushTotal / 2;
        const bShare = bIsAnchor ? 0 : aIsAnchor ? pushTotal : pushTotal / 2;
        if (axis === 'x') {
          // Move b right of a (deterministic: whichever has larger x gets pushed positive)
          const aFirst = a.x <= b.x;
          a[axis] += aFirst ? -aShare : aShare;
          b[axis] += aFirst ? bShare : -bShare;
        } else {
          const aFirst = a.y <= b.y;
          a[axis] += aFirst ? -aShare : aShare;
          b[axis] += aFirst ? bShare : -bShare;
        }
        moved = true;
      }
    }
    if (!moved) break;
  }
  return result;
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd ui/desktop && npx vitest run src/components/Dashboard/canvasLayout.test.ts`
Expected: PASS, 3 tests.

- [ ] **Step 5: Commit**

```bash
git add ui/desktop/src/components/Dashboard/canvasLayout.ts ui/desktop/src/components/Dashboard/canvasLayout.test.ts
git commit -m "feat(dashboard): add canvasLayout — spiral spawn placement + organize"
```

---

## Task 2: Update DashboardContext types — drop tuck/T1/T2, add cameraOffset

**Files:**
- Modify: `ui/desktop/src/contexts/DashboardContext.tsx`

- [ ] **Step 1: Replace the file with the new type definitions**

```tsx
import { createContext, useContext } from 'react';

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
}

export interface DashboardState {
  windows: DashboardWindow[];
  focusedWindowId: string | null;
  cameraOffset: { x: number; y: number };
  isHydrating: boolean;
}

export interface DashboardApi {
  state: DashboardState;
  spawnWindow: () => Promise<void>;
  closeWindow: (windowId: string) => void;
  focusWindow: (windowId: string) => void;
  renameWindow: (windowId: string, name: string) => void;
  syncSessionName: (windowId: string, name: string) => void;
  moveWindow: (
    windowId: string,
    position: { x: number; y: number },
    size?: { w: number; h: number }
  ) => void;
  resizeWindow: (
    windowId: string,
    size: { w: number; h: number },
    position?: { x: number; y: number }
  ) => void;
  freezeAllRects: (
    rects: Record<string, { x: number; y: number; w: number; h: number }>
  ) => void;
  organize: () => void;
  clearAll: () => void;
  /** Pan the camera by (dx, dy) in viewport pixels. */
  panBy: (dx: number, dy: number) => void;
  /** Recenter camera so the given window's center maps to the viewport center. */
  centerOn: (windowId: string, viewport: { width: number; height: number }) => void;
  updateWindowField: <K extends keyof DashboardWindow>(
    windowId: string,
    field: K,
    value: DashboardWindow[K]
  ) => void;
  markActivity: (windowId: string) => void;
}

export const DashboardContext = createContext<DashboardApi | null>(null);

export const useDashboard = (): DashboardApi => {
  const ctx = useContext(DashboardContext);
  if (!ctx) throw new Error('useDashboard must be used inside DashboardProvider');
  return ctx;
};

export const useOptionalDashboard = (): DashboardApi | null => useContext(DashboardContext);
```

- [ ] **Step 2: Verify it compiles**

Run: `cd ui/desktop && npx tsc --noEmit -p tsconfig.json`
Expected: TypeScript errors in `DashboardProvider.tsx`, `DashboardBoard.tsx`, `ChatWindow.tsx`, `TuckSidebar.tsx`, etc. — these are expected; we'll fix them in subsequent tasks. Note the error count for sanity.

- [ ] **Step 3: Commit**

```bash
git add ui/desktop/src/contexts/DashboardContext.tsx
git commit -m "refactor(dashboard): drop tuck/T1/T2 from DashboardContext, add cameraOffset + pan/center API"
```

---

## Task 3: Migrate dashboardStorage v1 → v2

**Files:**
- Modify: `ui/desktop/src/components/Dashboard/dashboardStorage.ts`
- Modify: `ui/desktop/src/components/Dashboard/dashboardStorage.test.ts`

- [ ] **Step 1: Inspect current storage shape**

Read the existing file to understand the v1 shape. The current key is
`biorouter.dashboard.v1`. The new key will be `biorouter.dashboard.v2`.

- [ ] **Step 2: Write the migration test**

Add to `dashboardStorage.test.ts`:

```ts
it('migrates v1 records by dropping isTucked and defaulting cameraOffset', () => {
  const v1 = {
    version: 1,
    windows: [
      {
        windowId: 'w1', sessionId: 's1', name: 'A', userSetName: false,
        badge: 1, accentColor: '#abcdef', position: { x: 10, y: 20 },
        size: { w: 520, h: 440 }, isManuallyPlaced: true, isTucked: false,
        lastInteraction: 1, unreadActivity: false,
      },
    ],
    focusedWindowId: 'w1', T1: 6, T2: 8,
  };
  window.localStorage.setItem('biorouter.dashboard.v1', JSON.stringify(v1));
  const loaded = loadDashboardState();
  expect(loaded).toBeTruthy();
  expect(loaded!.windows[0]).not.toHaveProperty('isTucked');
  expect(loaded!.cameraOffset).toEqual({ x: 0, y: 0 });
});

it('drops tucked windows or repositions them at the camera origin on migrate', () => {
  const v1 = {
    version: 1,
    windows: [
      { windowId: 'tucked', sessionId: 's', name: 'T', userSetName: false,
        badge: 1, accentColor: '#abc', position: null, size: null,
        isManuallyPlaced: false, isTucked: true, lastInteraction: 1,
        unreadActivity: false },
    ],
    focusedWindowId: null, T1: 6, T2: 8,
  };
  window.localStorage.setItem('biorouter.dashboard.v1', JSON.stringify(v1));
  const loaded = loadDashboardState();
  expect(loaded!.windows.length).toBe(1);
  expect(loaded!.windows[0].position).toBeDefined();
  expect(loaded!.windows[0].size).toBeDefined();
});
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cd ui/desktop && npx vitest run src/components/Dashboard/dashboardStorage.test.ts`
Expected: FAIL — current code doesn't migrate.

- [ ] **Step 4: Implement migration**

Update the file to read from v2 first, fall back to v1, transform, and re-save under v2:

```ts
const STORAGE_KEY_V2 = 'biorouter.dashboard.v2';
const STORAGE_KEY_V1 = 'biorouter.dashboard.v1';
const DEFAULT_W = 520;
const DEFAULT_H = 440;

export interface SerializedDashboardState {
  version: 2;
  windows: Array<Omit<DashboardWindow, 'position' | 'size'> & {
    position: { x: number; y: number };
    size: { w: number; h: number };
  }>;
  focusedWindowId: string | null;
  cameraOffset: { x: number; y: number };
}

export function loadDashboardState(): SerializedDashboardState | null {
  // Try v2 first
  const v2raw = window.localStorage.getItem(STORAGE_KEY_V2);
  if (v2raw) {
    try { return JSON.parse(v2raw) as SerializedDashboardState; } catch { /* fallthrough */ }
  }
  // Migrate v1 if present
  const v1raw = window.localStorage.getItem(STORAGE_KEY_V1);
  if (!v1raw) return null;
  try {
    const v1 = JSON.parse(v1raw);
    const windows = (v1.windows || []).map((w: any, i: number) => {
      const { isTucked, ...rest } = w;
      const position = w.position ?? { x: i * (DEFAULT_W + 16), y: 0 };
      const size = w.size ?? { w: DEFAULT_W, h: DEFAULT_H };
      return { ...rest, position, size };
    });
    const migrated: SerializedDashboardState = {
      version: 2,
      windows,
      focusedWindowId: v1.focusedWindowId ?? null,
      cameraOffset: { x: 0, y: 0 },
    };
    window.localStorage.setItem(STORAGE_KEY_V2, JSON.stringify(migrated));
    window.localStorage.removeItem(STORAGE_KEY_V1);
    return migrated;
  } catch {
    return null;
  }
}

export function saveDashboardState(state: SerializedDashboardState): void {
  window.localStorage.setItem(STORAGE_KEY_V2, JSON.stringify(state));
}

export function debounceSave(ms: number): (s: SerializedDashboardState) => void {
  let t: ReturnType<typeof setTimeout> | null = null;
  return (s) => {
    if (t) clearTimeout(t);
    t = setTimeout(() => saveDashboardState(s), ms);
  };
}
```

- [ ] **Step 5: Run all storage tests**

Run: `cd ui/desktop && npx vitest run src/components/Dashboard/dashboardStorage.test.ts`
Expected: all green.

- [ ] **Step 6: Commit**

```bash
git add ui/desktop/src/components/Dashboard/dashboardStorage.ts ui/desktop/src/components/Dashboard/dashboardStorage.test.ts
git commit -m "feat(dashboard): migrate localStorage v1→v2, drop tuck state, default cameraOffset"
```

---

## Task 4: Rewrite DashboardProvider — spawn, organize, pan, center

**Files:**
- Modify: `ui/desktop/src/components/Dashboard/DashboardProvider.tsx`

- [ ] **Step 1: Update imports and constants**

Replace the imports block:

```tsx
import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  DashboardContext,
  DashboardApi,
  DashboardState,
  DashboardWindow,
} from '../../contexts/DashboardContext';
import { generateName, pickAccentColor } from './palette';
import {
  loadDashboardState,
  SerializedDashboardState,
  debounceSave,
} from './dashboardStorage';
import { findSpawnPosition, organize as organizeLayout } from './canvasLayout';
import { createSession } from '../../sessions';
import { getInitialWorkingDir } from '../../utils/workingDir';

const MIN_WINDOW_W = 520;
const MIN_WINDOW_H = 440;
const GAP = 16;
```

- [ ] **Step 2: Rewrite `hydrate()` and remove `enforceT2Pure`**

```tsx
function hydrate(): DashboardState {
  const raw = loadDashboardState();
  if (!raw) {
    return {
      windows: [],
      focusedWindowId: null,
      cameraOffset: { x: 0, y: 0 },
      isHydrating: false,
    };
  }
  return {
    windows: raw.windows.map((w) => ({ ...w })),
    focusedWindowId: raw.focusedWindowId,
    cameraOffset: raw.cameraOffset ?? { x: 0, y: 0 },
    isHydrating: false,
  };
}

function serialize(state: DashboardState): SerializedDashboardState {
  return {
    version: 2,
    windows: state.windows.map((w) => ({ ...w })),
    focusedWindowId: state.focusedWindowId,
    cameraOffset: state.cameraOffset,
  };
}
```

Delete `enforceT2Pure`.

- [ ] **Step 3: Rewrite `spawnWindow`**

```tsx
const spawnWindow: DashboardApi['spawnWindow'] = useCallback(async () => {
  const cwd = getInitialWorkingDir();
  const session = await createSession(cwd);
  const sessionId = session.id;
  const now = Date.now();
  setState((prev) => {
    const existing = prev.windows.map((w) => ({
      x: w.position.x, y: w.position.y, w: w.size.w, h: w.size.h,
    }));
    // Spawn near camera center: in world coords, the camera center is at
    // (-cameraOffset.x, -cameraOffset.y).
    const center = { x: -prev.cameraOffset.x, y: -prev.cameraOffset.y };
    const pos = findSpawnPosition({
      center, size: { w: MIN_WINDOW_W, h: MIN_WINDOW_H }, existing, gap: GAP,
    });
    const usedColors = prev.windows.map((w) => w.accentColor);
    const newWin: DashboardWindow = {
      windowId: 'dw_' + Math.random().toString(36).slice(2, 10),
      sessionId,
      name: generateName(prev.windows.length),
      userSetName: false,
      badge: prev.windows.reduce((m, w) => Math.max(m, w.badge), 0) + 1,
      accentColor: pickAccentColor(usedColors),
      position: pos,
      size: { w: MIN_WINDOW_W, h: MIN_WINDOW_H },
      isManuallyPlaced: true,
      cwd,
      lastInteraction: now,
      unreadActivity: false,
    };
    return {
      ...prev,
      windows: [...prev.windows, newWin],
      focusedWindowId: newWin.windowId,
    };
  });
  // Recenter happens via a useEffect that watches focusedWindowId; see Task 6.
}, []);
```

- [ ] **Step 4: Replace `tuckWindow`/`evokeWindow` with `panBy`/`centerOn`**

Delete `tuckWindow`, `evokeWindow`, `setT1`, `setT2`.

```tsx
const panBy: DashboardApi['panBy'] = useCallback((dx, dy) => {
  setState((prev) => ({
    ...prev,
    cameraOffset: { x: prev.cameraOffset.x + dx, y: prev.cameraOffset.y + dy },
  }));
}, []);

const centerOn: DashboardApi['centerOn'] = useCallback((windowId, viewport) => {
  setState((prev) => {
    const w = prev.windows.find((x) => x.windowId === windowId);
    if (!w) return prev;
    const cx = w.position.x + w.size.w / 2;
    const cy = w.position.y + w.size.h / 2;
    return {
      ...prev,
      cameraOffset: {
        x: viewport.width / 2 - cx,
        y: viewport.height / 2 - cy,
      },
    };
  });
}, []);
```

- [ ] **Step 5: Rewrite `organize`**

```tsx
const organize: DashboardApi['organize'] = useCallback(() => {
  setState((prev) => {
    if (prev.windows.length < 2) return prev;
    const anchor = prev.focusedWindowId ?? prev.windows[0].windowId;
    const rects = prev.windows.map((w) => ({
      id: w.windowId, x: w.position.x, y: w.position.y, w: w.size.w, h: w.size.h,
    }));
    const out = organizeLayout(rects, anchor, GAP);
    const byId = new Map(out.map((r) => [r.id, r]));
    return {
      ...prev,
      windows: prev.windows.map((w) => {
        const r = byId.get(w.windowId);
        return r ? { ...w, position: { x: r.x, y: r.y } } : w;
      }),
    };
    // centerOn is called by the consumer via a useEffect.
  });
}, []);
```

- [ ] **Step 6: Update API memo**

```tsx
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
  }),
  [
    state, spawnWindow, closeWindow, focusWindow, renameWindow, syncSessionName,
    moveWindow, resizeWindow, freezeAllRects, organize, clearAll, panBy,
    centerOn, updateWindowField, markActivity,
  ]
);
```

- [ ] **Step 7: Run type check**

Run: `cd ui/desktop && npx tsc --noEmit -p tsconfig.json`
Expected: errors remain only in `DashboardBoard.tsx`, `ChatWindow.tsx`, `DashboardToolbar.tsx`, `TuckSidebar.tsx`, `HiddenChatHolder.tsx` — fix in later tasks.

- [ ] **Step 8: Run existing provider tests**

Run: `cd ui/desktop && npx vitest run src/components/Dashboard/DashboardProvider.test.tsx`
Expected: some tests will fail (they reference tuck behavior). Update them in this task:

- Drop any test asserting `tuckWindow`, `evokeWindow`, `setT1`, `setT2`.
- Add test: `spawn places window non-overlappingly`.
- Add test: `organize preserves sizes and resolves overlaps`.

Refer to `canvasLayout.test.ts` patterns for shape.

- [ ] **Step 9: Commit**

```bash
git add ui/desktop/src/components/Dashboard/DashboardProvider.tsx ui/desktop/src/components/Dashboard/DashboardProvider.test.tsx
git commit -m "refactor(dashboard): canvas provider — spiral spawn, organize, pan, center"
```

---

## Task 5: Delete tuck-related components and clean up index.ts

**Files:**
- Delete: `ui/desktop/src/components/Dashboard/TuckSidebar.tsx`
- Delete: `ui/desktop/src/components/Dashboard/TuckedCard.tsx`
- Delete: `ui/desktop/src/components/Dashboard/HiddenChatHolder.tsx`
- Modify: `ui/desktop/src/components/Dashboard/index.ts`

- [ ] **Step 1: Delete the files**

```bash
git rm ui/desktop/src/components/Dashboard/TuckSidebar.tsx \
       ui/desktop/src/components/Dashboard/TuckedCard.tsx \
       ui/desktop/src/components/Dashboard/HiddenChatHolder.tsx
```

- [ ] **Step 2: Remove tuck exports from index.ts**

Open `ui/desktop/src/components/Dashboard/index.ts` and delete any line referencing `TuckSidebar`, `TuckedCard`, or `HiddenChatHolder`.

- [ ] **Step 3: Type check**

Run: `cd ui/desktop && npx tsc --noEmit -p tsconfig.json`
Expected: errors only in `DashboardBoard.tsx` (still imports/uses deleted components).

- [ ] **Step 4: Commit**

```bash
git add ui/desktop/src/components/Dashboard/index.ts
git commit -m "chore(dashboard): delete TuckSidebar/TuckedCard/HiddenChatHolder"
```

---

## Task 6: Rewrite DashboardBoard as canvas viewport with pan

**Files:**
- Modify: `ui/desktop/src/components/Dashboard/DashboardBoard.tsx`

- [ ] **Step 1: Replace the file**

```tsx
import React, { useEffect, useRef, useState } from 'react';
import { useDashboard } from '../../contexts/DashboardContext';
import { ChatWindow } from './ChatWindow';

const MIN_WINDOW_W = 520;
const MIN_WINDOW_H = 440;

export const DashboardBoard: React.FC = () => {
  const dashboard = useDashboard();
  const viewportRef = useRef<HTMLDivElement>(null);
  const [viewport, setViewport] = useState<{ width: number; height: number }>({ width: 0, height: 0 });

  // Track viewport size for centerOn() calls.
  useEffect(() => {
    const el = viewportRef.current;
    if (!el) return;
    const update = () => {
      const r = el.getBoundingClientRect();
      setViewport({ width: r.width, height: r.height });
    };
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Recenter on focused window whenever it changes.
  const lastFocusedRef = useRef<string | null>(null);
  useEffect(() => {
    const id = dashboard.state.focusedWindowId;
    if (id && id !== lastFocusedRef.current && viewport.width > 0) {
      dashboard.centerOn(id, viewport);
    }
    lastFocusedRef.current = id;
  }, [dashboard.state.focusedWindowId, viewport.width, viewport.height]);

  // Pan via pointer drag on viewport background.
  const panStateRef = useRef<{ active: boolean; lastX: number; lastY: number }>({
    active: false, lastX: 0, lastY: 0,
  });
  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.target !== e.currentTarget) return; // only background
    panStateRef.current = { active: true, lastX: e.clientX, lastY: e.clientY };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  };
  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!panStateRef.current.active) return;
    const dx = e.clientX - panStateRef.current.lastX;
    const dy = e.clientY - panStateRef.current.lastY;
    panStateRef.current.lastX = e.clientX;
    panStateRef.current.lastY = e.clientY;
    dashboard.panBy(dx, dy);
  };
  const onPointerUp = (e: React.PointerEvent<HTMLDivElement>) => {
    panStateRef.current.active = false;
    (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
  };

  // Trackpad two-finger pan via wheel events.
  useEffect(() => {
    const el = viewportRef.current;
    if (!el) return;
    const handler = (ev: WheelEvent) => {
      // Skip wheels originating inside a window (so chat-scroll still works).
      const target = ev.target as HTMLElement | null;
      if (target && target !== el && target.closest('[data-dashboard-window]')) return;
      ev.preventDefault();
      dashboard.panBy(-ev.deltaX, -ev.deltaY);
    };
    el.addEventListener('wheel', handler, { passive: false });
    return () => el.removeEventListener('wheel', handler);
  }, [dashboard]);

  const minSize = { w: MIN_WINDOW_W, h: MIN_WINDOW_H };
  const { cameraOffset, windows } = dashboard.state;

  return (
    <div
      ref={viewportRef}
      className="relative flex-1 overflow-hidden cursor-grab active:cursor-grabbing"
      style={{
        backgroundImage:
          'radial-gradient(circle at 1px 1px, rgba(120,120,120,0.18) 1px, transparent 0)',
        backgroundSize: '16px 16px',
        backgroundPosition: `${cameraOffset.x}px ${cameraOffset.y}px`,
      }}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
    >
      {windows.length === 0 && (
        <div className="absolute inset-0 flex items-center justify-center text-text-muted pointer-events-none">
          <button
            type="button"
            className="px-4 py-2 rounded-xl border border-border-subtle hover:bg-background-medium pointer-events-auto"
            onClick={() => dashboard.spawnWindow()}
          >
            Spawn a conversation
          </button>
        </div>
      )}
      <div
        className="absolute inset-0 pointer-events-none"
        style={{ transform: `translate(${cameraOffset.x}px, ${cameraOffset.y}px)` }}
      >
        {windows.map((w) => (
          <div key={w.windowId} data-dashboard-window className="pointer-events-auto">
            <ChatWindow
              win={w}
              rect={{ x: w.position.x, y: w.position.y, w: w.size.w, h: w.size.h, zIndex: dashboard.state.focusedWindowId === w.windowId ? 100 : 1 }}
              isFocused={dashboard.state.focusedWindowId === w.windowId}
              isSolo={windows.length === 1}
              boardSize={viewport}
              minSize={minSize}
              onManipulateStart={() => {
                // No-op for canvas mode: windows are already absolute world coords.
              }}
            />
          </div>
        ))}
      </div>
    </div>
  );
};
```

- [ ] **Step 2: Type check**

Run: `cd ui/desktop && npx tsc --noEmit -p tsconfig.json`
Expected: errors only in `ChatWindow.tsx` (still references `sidebarOpen`, `onTuckByDrag`) and `DashboardToolbar.tsx`.

- [ ] **Step 3: Commit**

```bash
git add ui/desktop/src/components/Dashboard/DashboardBoard.tsx
git commit -m "feat(dashboard): convert board to infinite canvas with pan"
```

---

## Task 7: Update ChatWindow — drop tuck-by-drag, support Shrink/Enlarge

**Files:**
- Modify: `ui/desktop/src/components/Dashboard/ChatWindow.tsx`

- [ ] **Step 1: Update Props interface**

Remove `sidebarOpen`, `onTuckByDrag` from `Props`. Make `boardSize`'s purpose only "for focus-pop edge detection". Add nothing new — the title bar callbacks come from `dashboard` directly.

- [ ] **Step 2: Update drag handler — remove tuck zone**

In `dragStart`'s `onEnd`, delete the block that checks `dropX + rect.w / 2 > boardSize.width - zoneWidth` and tucks. Replace with a straight `moveWindow` call. Drop the `clampedX`/`clampedY` clamping to `boardSize` since the canvas is infinite — just call `dashboard.moveWindow(win.windowId, { x: dropX, y: dropY }, { w: rect.w, h: rect.h })`.

- [ ] **Step 3: Wire title bar Shrink/Enlarge callbacks**

```tsx
<WindowTitleBar
  name={win.name}
  accentColor={win.accentColor}
  onRename={(name) => dashboard.renameWindow(win.windowId, name)}
  onClose={() => dashboard.closeWindow(win.windowId)}
  onShrink={() => dashboard.resizeWindow(win.windowId, { w: minSize.w, h: minSize.h }, { x: rect.x, y: rect.y })}
  onEnlarge={() => dashboard.resizeWindow(win.windowId, { w: 940, h: 800 }, { x: rect.x, y: rect.y })}
  onPointerDownDrag={dragStart}
/>
```

- [ ] **Step 4: Type check**

Run: `cd ui/desktop && npx tsc --noEmit -p tsconfig.json`
Expected: error in `WindowTitleBar.tsx` (missing props) and `DashboardToolbar.tsx`.

- [ ] **Step 5: Commit**

```bash
git add ui/desktop/src/components/Dashboard/ChatWindow.tsx
git commit -m "feat(dashboard): wire ChatWindow shrink/enlarge, drop tuck-by-drag"
```

---

## Task 8: Add Shrink/Enlarge buttons to WindowTitleBar

**Files:**
- Modify: `ui/desktop/src/components/Dashboard/WindowTitleBar.tsx`

- [ ] **Step 1: Update Props + import icons**

```tsx
import React, { useState, useRef, useEffect } from 'react';
import { X, Minimize2, Maximize2 } from 'lucide-react';

interface Props {
  name: string;
  accentColor: string;
  onRename: (name: string) => void;
  onClose: () => void;
  onShrink: () => void;
  onEnlarge: () => void;
  onPointerDownDrag: (e: React.PointerEvent<HTMLDivElement>) => void;
}
```

If `lucide-react` isn't already imported elsewhere this way, check `app-icons.tsx` — most icons there are re-exports. Use whatever the repo standard is.

- [ ] **Step 2: Render the buttons in order Shrink | Enlarge | Close**

Replace the close-button block with:

```tsx
<button
  type="button"
  className="flex-shrink-0 p-1 rounded hover:bg-background-medium transition-colors"
  onClick={onShrink}
  title="Shrink to minimum size"
>
  <Minimize2 className="w-3.5 h-3.5" />
</button>
<button
  type="button"
  className="flex-shrink-0 p-1 rounded hover:bg-background-medium transition-colors"
  onClick={onEnlarge}
  title="Enlarge to default chat size"
>
  <Maximize2 className="w-3.5 h-3.5" />
</button>
<button
  type="button"
  className="flex-shrink-0 p-1 rounded hover:bg-background-medium transition-colors"
  onClick={onClose}
  title="Close conversation"
>
  <X className="w-3.5 h-3.5" />
</button>
```

- [ ] **Step 3: Type check + verify icons render**

Run: `cd ui/desktop && npx tsc --noEmit -p tsconfig.json`
Expected: no errors related to WindowTitleBar.

- [ ] **Step 4: Commit**

```bash
git add ui/desktop/src/components/Dashboard/WindowTitleBar.tsx
git commit -m "feat(dashboard): add Shrink/Enlarge title-bar buttons"
```

---

## Task 9: Restyle DashboardToolbar — tab-style buttons, drop tucked count

**Files:**
- Modify: `ui/desktop/src/components/Dashboard/DashboardToolbar.tsx`

- [ ] **Step 1: Replace btnClass and the count display**

```tsx
export const DashboardToolbar: React.FC = () => {
  const dashboard = useDashboard();
  const onBoard = dashboard.state.windows.length;

  // Tab-style: no border, no background, just text + hover bg-medium.
  // Matches the sidebar Home/Chat/History buttons.
  const btnClass =
    'no-drag h-7 px-3 text-[13.5px] font-normal rounded-md ' +
    'text-text-default/80 hover:text-text-default hover:bg-background-medium/40 ' +
    'active:translate-y-px transition-all';

  return (
    <div className="relative z-[60] flex items-center gap-2 px-4 py-1.5 border-b border-border-subtle/30 bg-background-muted/40 backdrop-blur-sm">
      <div className="absolute left-1/2 -translate-x-1/2 flex items-center gap-2 no-drag">
        <button type="button" onClick={() => dashboard.spawnWindow()} title="Spawn (⌘⇧N)" className={btnClass}>
          Spawn
        </button>
        <button type="button" onClick={() => dashboard.organize()} title="Re-tile" className={btnClass}>
          Organize
        </button>
        <button type="button" onClick={() => dashboard.clearAll()} title="Close all" className={btnClass}>
          Clear
        </button>
      </div>
      <div className="ml-auto flex items-center gap-2 no-drag text-xs text-text-muted">
        {onBoard} on canvas
      </div>
    </div>
  );
};
```

- [ ] **Step 2: Type check**

Run: `cd ui/desktop && npx tsc --noEmit -p tsconfig.json`
Expected: no errors in toolbar.

- [ ] **Step 3: Commit**

```bash
git add ui/desktop/src/components/Dashboard/DashboardToolbar.tsx
git commit -m "feat(dashboard): tab-style toolbar buttons; drop tucked count"
```

---

## Task 10: Wire Organize → centerOn focused

**Files:**
- Modify: `ui/desktop/src/components/Dashboard/DashboardBoard.tsx`

- [ ] **Step 1: Track when organize was just called**

The clean approach: add a `useEffect` that fires `centerOn(focusedWindowId, viewport)` whenever any window's position changes AND a tick counter set by `organize` increments. We already have the focus-changed recenter; that handles spawn. We need an explicit organize-triggered recenter.

Update `DashboardProvider` first: add `organizeTick: number` to state, bumped each time organize runs.

```tsx
// In DashboardProvider state:
interface DashboardState {
  // ... existing fields
  organizeTick: number;
}
// hydrate() initializes organizeTick: 0
// organize() returns ..., organizeTick: prev.organizeTick + 1
```

Update `DashboardContext.tsx` to include `organizeTick` in `DashboardState`.

Then in `DashboardBoard.tsx`:

```tsx
const lastOrganizeTickRef = useRef(0);
useEffect(() => {
  const tick = dashboard.state.organizeTick;
  if (tick > lastOrganizeTickRef.current && dashboard.state.focusedWindowId && viewport.width > 0) {
    dashboard.centerOn(dashboard.state.focusedWindowId, viewport);
  }
  lastOrganizeTickRef.current = tick;
}, [dashboard.state.organizeTick, viewport.width, viewport.height]);
```

- [ ] **Step 2: Type check**

Run: `cd ui/desktop && npx tsc --noEmit -p tsconfig.json`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add ui/desktop/src/contexts/DashboardContext.tsx ui/desktop/src/components/Dashboard/DashboardProvider.tsx ui/desktop/src/components/Dashboard/DashboardBoard.tsx
git commit -m "feat(dashboard): center camera on focused window after Organize"
```

---

## Task 11: Vertical popover for ChatInput collapsible group

**Files:**
- Create (if missing): `ui/desktop/src/components/ui/popover.tsx`
- Modify: `ui/desktop/src/components/ChatInput.tsx`

- [ ] **Step 1: Verify or create the Popover primitive**

```bash
ls ui/desktop/src/components/ui/popover.tsx 2>/dev/null || echo MISSING
```

If MISSING, create:

```tsx
'use client';
import * as React from 'react';
import * as PopoverPrimitive from '@radix-ui/react-popover';
import { cn } from '../../utils';

export const Popover = PopoverPrimitive.Root;
export const PopoverTrigger = PopoverPrimitive.Trigger;
export const PopoverPortal = PopoverPrimitive.Portal;

export const PopoverContent = React.forwardRef<
  React.ElementRef<typeof PopoverPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof PopoverPrimitive.Content>
>(({ className, align = 'center', sideOffset = 4, ...props }, ref) => (
  <PopoverPrimitive.Portal>
    <PopoverPrimitive.Content
      ref={ref}
      align={align}
      sideOffset={sideOffset}
      className={cn(
        'z-[1000] w-60 rounded-xl border bg-background-default p-2 shadow-lg outline-none',
        className
      )}
      {...props}
    />
  </PopoverPrimitive.Portal>
));
PopoverContent.displayName = 'PopoverContent';
```

Check that `@radix-ui/react-popover` is in `package.json`. If not:

```bash
cd ui/desktop && npm install @radix-ui/react-popover
```

- [ ] **Step 2: Rewrite the picker collapse in ChatInput.tsx**

In the picker row of `ChatInput.tsx`, replace the inline-expand block with a Popover. The trigger is the `>` chevron button. The content is a vertical stack of: Cost / Model / Mode / Workflow / Diagnostics.

Find the section:

```tsx
{/* Expand/collapse toggle for the secondary picker group */}
<Tooltip>
  <TooltipTrigger asChild>
    <Button ... onClick={() => setPickerExpanded((v) => !v)} ...>
```

…through the end of the `{pickerExpanded && (...)}` block, and replace with:

```tsx
<Popover open={pickerExpanded} onOpenChange={setPickerExpanded}>
  <PopoverTrigger asChild>
    <Button
      type="button"
      variant="ghost"
      size="sm"
      className="flex items-center justify-center text-text-default/70 hover:text-text-default text-xs cursor-pointer ml-1"
      aria-label={pickerExpanded ? 'Collapse extra controls' : 'Expand extra controls'}
    >
      {pickerExpanded ? <ChevronLeft className="w-4 h-4" /> : <ChevronRight className="w-4 h-4" />}
    </Button>
  </PopoverTrigger>
  <PopoverContent side="top" align="start" className="flex flex-col gap-2">
    {COST_TRACKING_ENABLED && (
      <div className="flex items-center px-1">
        <CostTracker
          inputTokens={accumulatedInputTokens}
          outputTokens={accumulatedOutputTokens}
          sessionCosts={sessionCosts}
        />
      </div>
    )}
    <div className="flex items-center px-1">
      <ModelsBottomBar
        sessionId={sessionId}
        dropdownRef={dropdownRef}
        setView={setView}
        alerts={alerts}
      />
    </div>
    <div className="flex items-center px-1">
      <BottomMenuModeSelection />
    </div>
    {sessionId && (
      <div className="flex items-center px-1">
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              onClick={() => {
                if (workflow) { trackEditWorkflowOpened(); setShowEditWorkflowModal(true); }
                else { trackCreateWorkflowOpened(); setShowCreateWorkflowModal(true); }
              }}
              variant="ghost" size="sm"
              className="flex items-center gap-2 text-text-default/70 hover:text-text-default text-xs cursor-pointer w-full justify-start"
            >
              <Pipeline size={16} />
              <span>{workflow ? 'View/Edit Workflow' : 'Create Workflow'}</span>
            </Button>
          </TooltipTrigger>
        </Tooltip>
      </div>
    )}
    {sessionId && (
      <div className="flex items-center px-1">
        <Button
          type="button"
          onClick={() => { trackDiagnosticsOpened(); setDiagnosticsOpen(true); }}
          variant="ghost" size="sm"
          className="flex items-center gap-2 text-text-default/70 hover:text-text-default text-xs cursor-pointer w-full justify-start"
        >
          <CodeAnalysis className="w-4 h-4" />
          <span>Diagnostics</span>
        </Button>
      </div>
    )}
  </PopoverContent>
</Popover>
```

Add the imports at top:

```tsx
import { Popover, PopoverTrigger, PopoverContent } from './ui/popover';
```

Remove the now-unused `pickerExpanded && (...)` inline block.

- [ ] **Step 3: Type check**

Run: `cd ui/desktop && npx tsc --noEmit -p tsconfig.json`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add ui/desktop/src/components/ui/popover.tsx ui/desktop/src/components/ChatInput.tsx
git commit -m "feat(chat-input): vertical popover for collapsible picker group"
```

---

## Task 12: Verify auto-rename pipeline still fires

**Files:**
- Read-only check, no code unless a bug is found.

- [ ] **Step 1: Read the rename pipeline**

Check `ui/desktop/src/components/BaseChat.tsx` for the `onSessionUpdate` callback wiring. Confirm it fires `onSessionUpdate({ id, name })` after each session refresh.

- [ ] **Step 2: Read `ChatWindow.tsx`**

Confirm `onSessionUpdate={(s) => { if (s?.name) dashboard.syncSessionName(...) }}` is still wired.

- [ ] **Step 3: Read `DashboardProvider.syncSessionName`**

Confirm it updates the window's name iff `userSetName === false`.

- [ ] **Step 4: If all three are intact, no code change needed**

Otherwise, restore the wiring in the file that lost it.

- [ ] **Step 5: Commit if changes were needed**

```bash
git add <files>
git commit -m "fix(dashboard): restore session auto-rename wiring"
```

If no changes, skip the commit.

---

## Task 13: Final validation via Playwright debugger

**Files:**
- No code changes; this is the end-to-end check.

> **Note.** This task is not reproducible from the document alone. It depends on
> a live Electron app driven over CDP through the Playwright MCP tools, and its
> final step writes a screenshot to a throwaway `/tmp` path that existed only
> for the original review. Read it as a record of what was checked, not as a
> runnable script.

- [ ] **Step 1: Kill any old Electron and start fresh**

```bash
killall -9 Electron "Electron Helper" "Electron Helper (GPU)" "Electron Helper (Renderer)" 2>/dev/null
osascript -e 'tell application "Terminal" to activate' \
          -e 'tell application "Terminal" to do script "cd <repo-root>/ui/desktop && ENABLE_PLAYWRIGHT=1 npm run start-gui 2>&1 | tee /tmp/biorouter-start.log"'
```

Wait for CDP via Monitor until `curl -sf http://localhost:9222/json/version` returns 200.

- [ ] **Step 2: Connect and test pan**

In Playwright MCP:
1. `browser_snapshot` to confirm dashboard renders.
2. Click "Open Dashboard".
3. Use `browser_evaluate` to pointer-down on the viewport background and drag — verify `cameraOffset` increments.
4. Use `browser_evaluate` to dispatch a wheel event — verify `cameraOffset` updates.

- [ ] **Step 3: Test spawn non-overlap**

Click Spawn 5 times. Use `browser_evaluate` to read each window's `style.transform` and confirm no two windows have overlapping rects.

- [ ] **Step 4: Test Shrink / Enlarge**

Click Enlarge on a window. Confirm new size = 940×800. Click Shrink. Confirm new size = 520×440.

- [ ] **Step 5: Test Organize**

Manually resize one window large enough to overlap a neighbor (via `browser_evaluate` to set state directly or by clicking Enlarge then dragging). Click Organize. Confirm: windows separated, sizes unchanged, camera recenters on focused window.

- [ ] **Step 6: Test `>` vertical popup**

Click the `>` chevron in a chat. Confirm the popover opens above the input row with Cost / Model / Mode / Workflow / Diagnostics stacked vertically. Click outside — popover dismisses. Send button still visible throughout.

- [ ] **Step 7: Test toolbar style**

Hover Spawn/Organize/Clear — confirm only background highlight on hover, no border ring. Visual must match sidebar tab style.

- [ ] **Step 8: Take a final screenshot**

`browser_take_screenshot` and save as `/tmp/canvas-dashboard-final.png` for the user to review.

- [ ] **Step 9: No commit needed**

Validation only.

---

## Self-review checklist (already run by the plan's author)

- [x] Spec coverage: every requirement (taller MIN, shrink/enlarge, no-tuck, canvas pan, organize-recenter, spawn-non-overlap, auto-rename, vertical popup, tab-style toolbar) has at least one task.
- [x] Placeholder scan: no TBDs, no "add error handling later", every code block is complete.
- [x] Type consistency: `DashboardWindow.position` is `{ x, y }` everywhere (non-nullable); `cameraOffset` is `{ x, y }` everywhere; `Rect` type lives in `canvasLayout.ts` and is used consistently.

## Related documentation

- [v3 — Canvas dashboard design spec](v3-infinite-canvas-design.md) — the spec this plan implements, including the organize algorithm and spawn placement.
- [v2 — Dashboard Mode implementation plan](v2-dashboard-mode-plan.md) — builds the tile/tuck board and layout engine that this plan tears out.
- [v4 — Dashboard fold mode implementation plan](v4-window-fold-mode-plan.md) — the direct successor, which folds windows on the canvas this plan creates.
- [Dashboard mode — removal record and archive index](README.md) — records the deletion of `canvasLayout.ts` and the rest of the canvas.
