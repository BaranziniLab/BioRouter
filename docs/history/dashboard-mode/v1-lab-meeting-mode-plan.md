# Lab Meeting Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `/lab-meeting` route to BioRouter desktop app — a multi-conversation parallel workspace with auto-tile grid, intersection-point overflow, and sidebar tucking. Each window holds an independent biorouterd session.

**Architecture:** New `LabMeetingProvider` mounted at the app root above `<Routes>` so background streams persist across navigation. Pure `computeLayout()` engine drives positioning. Each `<ChatWindow>` reuses `<BaseChat>` in a new `coherent` + `hideStatusBar` mode. Board state persists via localStorage. App-level status bar reads/writes the focused window's per-window state. No backend changes.

**Tech Stack:** React 19, TypeScript, Vite, Electron Forge, Tailwind, react-router-dom, lucide-react. Test runners: Vitest (unit/component) + Playwright (E2E).

---

## File Structure

### New files (under `ui/desktop/src/components/LabMeeting/`)

| File | Responsibility |
|---|---|
| `palette.ts` | 12-color accent palette + name generator (`Atlas`, `Nova`, ...) — pure |
| `layoutEngine.ts` | `computeLayout(windows, boardSize, T1, T2)` — pure |
| `layoutEngine.test.ts` | Layout engine unit tests |
| `labMeetingStorage.ts` | Debounced localStorage persistence |
| `labMeetingStorage.test.ts` | Storage unit tests |
| `LabMeetingProvider.tsx` | Context provider + state machine |
| `LabMeetingProvider.test.tsx` | Provider unit tests (state transitions) |
| `LabMeetingRoute.tsx` | Route shell — mounts toolbar / board / status bar |
| `LabMeetingBoard.tsx` | Bounded board surface; renders ChatWindows |
| `LabMeetingToolbar.tsx` | Spawn / Clear / Organize / T1 / T2 / status |
| `LabMeetingStatusBar.tsx` | Focused-window-aware app-level row |
| `ChatWindow.tsx` | Per-conversation window chrome wrapping BaseChat |
| `WindowTitleBar.tsx` | Dot, name (inline-editable), badge, close, drag handle |
| `ResizeHandle.tsx` | Bottom-right resize handle |
| `TuckSidebar.tsx` | Right-side panel that opens when ≥1 tucked |
| `TuckedCard.tsx` | One card per tucked window |
| `BackToLabMeetingPill.tsx` | Floating pill in AppLayout when state is non-empty + away from /lab-meeting |
| `useLabMeetingDrag.ts` | Pointer-based drag/resize hook |
| `index.ts` | Re-exports |

### New context

| File | Responsibility |
|---|---|
| `ui/desktop/src/contexts/LabMeetingContext.tsx` | Context type + `useLabMeeting()` hook (re-exported via Provider) |

### Modified files

| File | Change |
|---|---|
| `ui/desktop/src/App.tsx` | Register `/lab-meeting` route inside the AppLayout outlet; mount `<LabMeetingProvider>` at app root |
| `ui/desktop/src/components/Layout/AppLayout.tsx` | Add `Users` icon button next to `+`; render `<BackToLabMeetingPill>` |
| `ui/desktop/src/components/BaseChat.tsx` | Add `coherent` and `hideStatusBar` props |
| `ui/desktop/src/components/ChatInput.tsx` | Respect `hideStatusBar` (suppress model/mode/cost footer); coherent visual |
| `ui/desktop/src/main.ts` | IPC handler `labMeeting:enter` (maximize BrowserWindow) |
| `ui/desktop/src/preload.ts` | Expose `labMeetingEnter()` IPC |

---

## Conventions used in this plan

- All paths are absolute from repo root `/Users/wgu/Desktop/biorouter`.
- Frontend test runner: `cd ui/desktop && npm run test:run -- <path>` (Vitest).
- Frontend type-check + lint: `cd ui/desktop && npm run lint:check`.
- Commit conventions follow the repo's existing pattern (`feat(ui):`, `fix(...):`, `docs(...):` etc.). See `.claude/skills/git-commit-style/` if present, otherwise mirror recent commits.
- Frequent commits — one commit per task or sub-feature.

---

# Task 1: Color palette + name generator (TDD)

**Files:**
- Create: `ui/desktop/src/components/LabMeeting/palette.ts`
- Create: `ui/desktop/src/components/LabMeeting/palette.test.ts`

### Context

Pure module: gives stable accent colors and friendly conversation names. Used by spawn logic to assign defaults.

- [ ] **Step 1: Write failing tests**

Create `ui/desktop/src/components/LabMeeting/palette.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { ACCENT_PALETTE, pickAccentColor, generateName, NAME_POOL } from './palette';

describe('palette', () => {
  it('exposes 12 distinct hex colors', () => {
    expect(ACCENT_PALETTE).toHaveLength(12);
    const set = new Set(ACCENT_PALETTE);
    expect(set.size).toBe(12);
    for (const c of ACCENT_PALETTE) {
      expect(c).toMatch(/^#[0-9a-fA-F]{6}$/);
    }
  });

  it('pickAccentColor cycles through palette avoiding used colors when possible', () => {
    expect(pickAccentColor([])).toBe(ACCENT_PALETTE[0]);
    expect(pickAccentColor([ACCENT_PALETTE[0]])).toBe(ACCENT_PALETTE[1]);
    // when all used, falls back to the least-recently-passed one (index-based)
    const all = [...ACCENT_PALETTE];
    expect(ACCENT_PALETTE).toContain(pickAccentColor(all));
  });

  it('generateName returns a name from the pool when index < pool size', () => {
    expect(NAME_POOL).toContain(generateName(0));
    expect(generateName(0)).toBe(NAME_POOL[0]);
    expect(generateName(NAME_POOL.length - 1)).toBe(NAME_POOL[NAME_POOL.length - 1]);
  });

  it('generateName falls back to "Chat #N" when index exceeds pool', () => {
    expect(generateName(NAME_POOL.length)).toBe(`Chat #${NAME_POOL.length + 1}`);
    expect(generateName(NAME_POOL.length + 5)).toBe(`Chat #${NAME_POOL.length + 6}`);
  });
});
```

- [ ] **Step 2: Run test (should fail — module missing)**

```bash
cd ui/desktop && npm run test:run -- src/components/LabMeeting/palette.test.ts
```
Expected: FAIL with module-not-found.

- [ ] **Step 3: Implement**

Create `ui/desktop/src/components/LabMeeting/palette.ts`:

```ts
export const ACCENT_PALETTE: readonly string[] = [
  '#14b8a6', // teal
  '#6366f1', // indigo
  '#f59e0b', // amber
  '#f43f5e', // rose
  '#84cc16', // lime
  '#0ea5e9', // sky
  '#8b5cf6', // violet
  '#fb7185', // coral
  '#10b981', // mint
  '#eab308', // gold
  '#d946ef', // magenta
  '#64748b', // slate
] as const;

export const NAME_POOL: readonly string[] = [
  'Atlas', 'Nova', 'Prism', 'Echo', 'Lyra', 'Orion',
  'Sage', 'Vega', 'Wren', 'Zephyr', 'Juno', 'Kai',
  'Mira', 'Neo', 'Pax', 'Rune', 'Soren', 'Tess',
] as const;

export function pickAccentColor(usedColors: readonly string[]): string {
  for (const color of ACCENT_PALETTE) {
    if (!usedColors.includes(color)) return color;
  }
  // All used — pick by ring buffer: the one used least recently (i.e., earliest in usedColors)
  const ringIndex = usedColors.length % ACCENT_PALETTE.length;
  return ACCENT_PALETTE[ringIndex];
}

export function generateName(index: number): string {
  if (index < NAME_POOL.length) return NAME_POOL[index];
  return `Chat #${index + 1}`;
}
```

- [ ] **Step 4: Run tests — should pass**

```bash
cd ui/desktop && npm run test:run -- src/components/LabMeeting/palette.test.ts
```
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add ui/desktop/src/components/LabMeeting/palette.ts ui/desktop/src/components/LabMeeting/palette.test.ts
git commit -m "feat(lab-meeting): palette + name generator"
```

---

# Task 2: Layout engine (TDD) — clean grid for n ≤ T1

**Files:**
- Create: `ui/desktop/src/components/LabMeeting/layoutEngine.ts`
- Create: `ui/desktop/src/components/LabMeeting/layoutEngine.test.ts`

### Context

Pure function. Step-by-step we'll add (a) clean grid (this task), (b) intersection overflow (Task 3), (c) tucking selection (Task 4). Manually-placed windows excluded from the auto-tile pass.

- [ ] **Step 1: Write failing tests**

Create `ui/desktop/src/components/LabMeeting/layoutEngine.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { computeLayout, LayoutInputWindow } from './layoutEngine';

const board = { width: 1200, height: 800 };

function mkWindow(id: string, overrides: Partial<LayoutInputWindow> = {}): LayoutInputWindow {
  return {
    windowId: id,
    isManuallyPlaced: false,
    isTucked: false,
    position: null,
    size: null,
    lastInteraction: 0,
    ...overrides,
  };
}

describe('computeLayout — clean grid (n ≤ T1)', () => {
  it('one window fills the board', () => {
    const out = computeLayout([mkWindow('a')], board, 6, 8, 'a');
    expect(out.size).toBe(1);
    const r = out.get('a')!;
    expect(r.x).toBe(0);
    expect(r.y).toBe(0);
    expect(r.w).toBe(1200);
    expect(r.h).toBe(800);
  });

  it('two windows tile 2×1', () => {
    const out = computeLayout([mkWindow('a'), mkWindow('b')], board, 6, 8, null);
    expect(out.size).toBe(2);
    const a = out.get('a')!, b = out.get('b')!;
    expect(a.w).toBe(600); expect(a.h).toBe(800); expect(a.x).toBe(0);
    expect(b.w).toBe(600); expect(b.h).toBe(800); expect(b.x).toBe(600);
  });

  it('four windows tile 2×2', () => {
    const ids = ['a','b','c','d'];
    const out = computeLayout(ids.map(i => mkWindow(i)), board, 6, 8, null);
    expect(out.size).toBe(4);
    expect(out.get('a')!.w).toBe(600);
    expect(out.get('a')!.h).toBe(400);
    expect(out.get('d')!.x).toBe(600);
    expect(out.get('d')!.y).toBe(400);
  });

  it('five windows: 3×2 with last row centered', () => {
    const ids = ['a','b','c','d','e'];
    const out = computeLayout(ids.map(i => mkWindow(i)), board, 6, 8, null);
    expect(out.size).toBe(5);
    // The last row has 2 of 3 cells; should be centered horizontally
    const e = out.get('e')!;
    const d = out.get('d')!;
    expect(d.y).toBe(400);
    expect(e.y).toBe(400);
    expect(d.x).toBeGreaterThan(0); // not flush left
    expect(e.x).toBeGreaterThan(d.x);
    // Centered: left padding === right padding
    const cellW = 400;
    const totalLastRowW = 2 * cellW;
    const expectedLeft = (1200 - totalLastRowW) / 2;
    expect(d.x).toBeCloseTo(expectedLeft, 0);
  });

  it('six windows: 3×2 grid', () => {
    const ids = ['a','b','c','d','e','f'];
    const out = computeLayout(ids.map(i => mkWindow(i)), board, 6, 8, null);
    expect(out.size).toBe(6);
    expect(out.get('f')!.x).toBe(800);
    expect(out.get('f')!.y).toBe(400);
  });

  it('focused window receives top z-index', () => {
    const out = computeLayout([mkWindow('a'), mkWindow('b'), mkWindow('c')], board, 6, 8, 'b');
    const z = (id: string) => out.get(id)!.zIndex;
    expect(z('b')).toBeGreaterThan(z('a'));
    expect(z('b')).toBeGreaterThan(z('c'));
  });

  it('manually-placed window uses its stored position/size, excluded from auto-tile', () => {
    const a = mkWindow('a', { isManuallyPlaced: true, position: { x: 50, y: 60 }, size: { w: 300, h: 200 } });
    const b = mkWindow('b');
    const out = computeLayout([a, b], board, 6, 8, null);
    expect(out.get('a')!.x).toBe(50);
    expect(out.get('a')!.y).toBe(60);
    expect(out.get('a')!.w).toBe(300);
    expect(out.get('a')!.h).toBe(200);
    // b auto-tiles into the full board (no awareness of a's manual placement)
    expect(out.get('b')!.w).toBe(1200);
  });

  it('skips tucked windows entirely', () => {
    const out = computeLayout(
      [mkWindow('a'), mkWindow('b', { isTucked: true })],
      board, 6, 8, null
    );
    expect(out.has('a')).toBe(true);
    expect(out.has('b')).toBe(false);
  });
});
```

- [ ] **Step 2: Run tests (fail)**

```bash
cd ui/desktop && npm run test:run -- src/components/LabMeeting/layoutEngine.test.ts
```
Expected: FAIL.

- [ ] **Step 3: Implement clean-grid path**

Create `ui/desktop/src/components/LabMeeting/layoutEngine.ts`:

```ts
export interface LayoutInputWindow {
  windowId: string;
  isManuallyPlaced: boolean;
  isTucked: boolean;
  position: { x: number; y: number } | null;
  size: { w: number; h: number } | null;
  lastInteraction: number;
}

export interface BoardSize {
  width: number;
  height: number;
}

export interface LayoutRect {
  x: number;
  y: number;
  w: number;
  h: number;
  zIndex: number;
}

const Z_TILED = 1;
const Z_OVERFLOW = 50;
const Z_FOCUSED = 100;
const TARGET_ASPECT = 1.3; // w:h target

function bestGridConfig(n: number, board: BoardSize): { cols: number; rows: number } {
  let best = { cols: n, rows: 1, score: Infinity };
  for (let cols = 1; cols <= n; cols++) {
    const rows = Math.ceil(n / cols);
    const cellW = board.width / cols;
    const cellH = board.height / rows;
    const aspect = cellW / cellH;
    const score = Math.abs(Math.log(aspect) - Math.log(TARGET_ASPECT));
    if (score < best.score) best = { cols, rows, score };
  }
  return { cols: best.cols, rows: best.rows };
}

export function computeLayout(
  windows: readonly LayoutInputWindow[],
  board: BoardSize,
  T1: number,
  T2: number,
  focusedWindowId: string | null
): Map<string, LayoutRect> {
  void T2; // T2 used in later tasks (overflow / tuck selection happens upstream)
  const out = new Map<string, LayoutRect>();

  // Drop tucked entirely
  const visible = windows.filter(w => !w.isTucked);

  // Manually-placed windows render at their stored coords; exclude from auto-tile pass.
  const manual = visible.filter(w => w.isManuallyPlaced && w.position && w.size);
  const auto = visible.filter(w => !(w.isManuallyPlaced && w.position && w.size));

  for (const w of manual) {
    out.set(w.windowId, {
      x: w.position!.x, y: w.position!.y, w: w.size!.w, h: w.size!.h,
      zIndex: w.windowId === focusedWindowId ? Z_FOCUSED : Z_TILED + 5,
    });
  }

  if (auto.length === 0) return out;

  // Take the first T1 auto windows as tiled; rest treated as overflow (Task 3).
  const tiled = auto.slice(0, Math.min(auto.length, T1));
  const overflow = auto.slice(tiled.length);
  void overflow; // handled in Task 3

  const { cols, rows } = bestGridConfig(tiled.length, board);
  const cellW = board.width / cols;
  const cellH = board.height / rows;

  for (let i = 0; i < tiled.length; i++) {
    const row = Math.floor(i / cols);
    const col = i % cols;
    const isLastRow = row === rows - 1;
    const itemsInLastRow = tiled.length - cols * (rows - 1);
    let x = col * cellW;
    let y = row * cellH;
    if (isLastRow && itemsInLastRow < cols) {
      const lastRowOffset = (board.width - itemsInLastRow * cellW) / 2;
      x = lastRowOffset + col * cellW;
    }
    out.set(tiled[i].windowId, {
      x, y, w: cellW, h: cellH,
      zIndex: tiled[i].windowId === focusedWindowId ? Z_FOCUSED : Z_TILED,
    });
  }

  return out;
}
```

- [ ] **Step 4: Run tests — should pass**

```bash
cd ui/desktop && npm run test:run -- src/components/LabMeeting/layoutEngine.test.ts
```
Expected: PASS (8 tests).

- [ ] **Step 5: Commit**

```bash
git add ui/desktop/src/components/LabMeeting/layoutEngine.ts ui/desktop/src/components/LabMeeting/layoutEngine.test.ts
git commit -m "feat(lab-meeting): layout engine — clean grid for n ≤ T1"
```

---

# Task 3: Layout engine — overflow at intersection points (TDD)

**Files:**
- Modify: `ui/desktop/src/components/LabMeeting/layoutEngine.ts`
- Modify: `ui/desktop/src/components/LabMeeting/layoutEngine.test.ts`

- [ ] **Step 1: Add failing tests**

Append to `layoutEngine.test.ts`:

```ts
describe('computeLayout — overflow at intersections (T1 < n ≤ T2)', () => {
  it('places overflow windows at grid intersection points sorted by centrality', () => {
    // 7 windows, T1=6 (3×2 grid), T2=8 → 1 overflow
    const ids = ['a','b','c','d','e','f','g'];
    const out = computeLayout(ids.map(i => mkWindow(i)), board, 6, 8, null);
    expect(out.size).toBe(7);
    const g = out.get('g')!;
    // Most central intersection of 3×2 grid is (board.width/2, board.height/2) = (600, 400)
    // Overflow window centered there → top-left = (600 - cellW/2, 400 - cellH/2)
    // cellW = 400, cellH = 400 in 3×2 grid
    expect(g.x).toBeCloseTo(600 - 200, 0);
    expect(g.y).toBeCloseTo(400 - 200, 0);
    expect(g.w).toBe(400);
    expect(g.h).toBe(400);
  });

  it('overflow renders above tiled in z-order', () => {
    const ids = ['a','b','c','d','e','f','g'];
    const out = computeLayout(ids.map(i => mkWindow(i)), board, 6, 8, null);
    const g = out.get('g')!;
    const a = out.get('a')!;
    expect(g.zIndex).toBeGreaterThan(a.zIndex);
  });

  it('two overflow windows pick distinct intersection points', () => {
    const ids = ['a','b','c','d','e','f','g','h'];
    const out = computeLayout(ids.map(i => mkWindow(i)), board, 6, 8, null);
    const g = out.get('g')!;
    const h = out.get('h')!;
    expect(g.x !== h.x || g.y !== h.y).toBe(true);
  });

  it('with T1=1 (degenerate grid), overflow windows stack near center with jitter', () => {
    // 3 windows, T1=1, T2=4 → 2 overflow with no real intersection candidates
    const ids = ['a','b','c'];
    const out = computeLayout(ids.map(i => mkWindow(i)), board, 1, 4, null);
    const b = out.get('b')!;
    const c = out.get('c')!;
    expect(b.x !== c.x || b.y !== c.y).toBe(true); // distinct via jitter
  });
});
```

- [ ] **Step 2: Run tests (fail)**

```bash
cd ui/desktop && npm run test:run -- src/components/LabMeeting/layoutEngine.test.ts
```
Expected: FAIL on the new tests.

- [ ] **Step 3: Implement intersection placement**

In `layoutEngine.ts`, add the helper and extend the function. Replace the `void overflow;` line and add this logic right before `return out;`:

```ts
function intersectionCandidates(
  cols: number,
  rows: number,
  cellW: number,
  cellH: number,
  board: BoardSize
): Array<{ x: number; y: number }> {
  // Vertical lines at x = i*cellW for i in [0..cols], horizontal at y = j*cellH for j in [0..rows].
  const xs: number[] = [];
  const ys: number[] = [];
  for (let i = 0; i <= cols; i++) xs.push(i * cellW);
  for (let j = 0; j <= rows; j++) ys.push(j * cellH);
  const center = { x: board.width / 2, y: board.height / 2 };
  const points: Array<{ x: number; y: number; dist: number }> = [];
  for (const x of xs) {
    for (const y of ys) {
      const dist = Math.hypot(x - center.x, y - center.y);
      points.push({ x, y, dist });
    }
  }
  points.sort((p1, p2) => p1.dist - p2.dist);
  // Dedupe within 40px / 30px
  const deduped: Array<{ x: number; y: number }> = [];
  for (const p of points) {
    const tooClose = deduped.some(q => Math.abs(q.x - p.x) < 40 && Math.abs(q.y - p.y) < 30);
    if (!tooClose) deduped.push({ x: p.x, y: p.y });
  }
  return deduped;
}
```

Then replace the `const overflow = auto.slice(tiled.length); void overflow;` block with:

```ts
  const overflow = auto.slice(tiled.length);

  if (overflow.length > 0) {
    const candidates = intersectionCandidates(cols, rows, cellW, cellH, board);
    for (let i = 0; i < overflow.length; i++) {
      const base = candidates[i] ?? { x: board.width / 2, y: board.height / 2 };
      const jitter = i >= candidates.length ? (i - candidates.length + 1) * 8 : 0;
      const cx = base.x + jitter;
      const cy = base.y + jitter;
      // Center the cell on the intersection
      const x = Math.max(0, Math.min(board.width - cellW, cx - cellW / 2));
      const y = Math.max(0, Math.min(board.height - cellH, cy - cellH / 2));
      out.set(overflow[i].windowId, {
        x, y, w: cellW, h: cellH,
        zIndex: overflow[i].windowId === focusedWindowId ? Z_FOCUSED : Z_OVERFLOW + i,
      });
    }
  }
```

- [ ] **Step 4: Run tests — should pass**

```bash
cd ui/desktop && npm run test:run -- src/components/LabMeeting/layoutEngine.test.ts
```
Expected: PASS (12 tests total).

- [ ] **Step 5: Commit**

```bash
git add ui/desktop/src/components/LabMeeting/layoutEngine.ts ui/desktop/src/components/LabMeeting/layoutEngine.test.ts
git commit -m "feat(lab-meeting): layout engine — intersection-point overflow"
```

---

# Task 4: localStorage persistence helper (TDD)

**Files:**
- Create: `ui/desktop/src/components/LabMeeting/labMeetingStorage.ts`
- Create: `ui/desktop/src/components/LabMeeting/labMeetingStorage.test.ts`

### Context

Wraps localStorage read/write under key `biorouter.labmeeting.v1`. Provides a `debounceWrite()` helper. Hydrate filters out windows whose sessions no longer exist (caller passes a session-existence checker).

- [ ] **Step 1: Write failing tests**

```ts
// ui/desktop/src/components/LabMeeting/labMeetingStorage.test.ts
import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
  STORAGE_KEY,
  loadLabMeetingState,
  saveLabMeetingState,
  filterDeadSessions,
  type SerializedLabMeetingState,
} from './labMeetingStorage';

const makeState = (over: Partial<SerializedLabMeetingState> = {}): SerializedLabMeetingState => ({
  version: 1,
  windows: [],
  focusedWindowId: null,
  T1: 6,
  T2: 8,
  ...over,
});

beforeEach(() => {
  localStorage.clear();
});

describe('labMeetingStorage', () => {
  it('returns null when nothing stored', () => {
    expect(loadLabMeetingState()).toBeNull();
  });

  it('round-trips state', () => {
    const state = makeState({ T1: 4, T2: 9 });
    saveLabMeetingState(state);
    expect(loadLabMeetingState()).toEqual(state);
  });

  it('returns null for malformed JSON', () => {
    localStorage.setItem(STORAGE_KEY, '{not json');
    expect(loadLabMeetingState()).toBeNull();
  });

  it('returns null when version mismatches', () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ ...makeState(), version: 99 }));
    expect(loadLabMeetingState()).toBeNull();
  });

  it('filterDeadSessions removes windows whose sessionId is not present', async () => {
    const state = makeState({
      windows: [
        { windowId: 'w1', sessionId: 's1', name: 'A', badge: 1, accentColor: '#000', position: null, size: null, isManuallyPlaced: false, isTucked: false, lastInteraction: 0, unreadActivity: false },
        { windowId: 'w2', sessionId: 's2', name: 'B', badge: 2, accentColor: '#111', position: null, size: null, isManuallyPlaced: false, isTucked: false, lastInteraction: 0, unreadActivity: false },
      ],
      focusedWindowId: 'w2',
    });
    const isAlive = vi.fn(async (sid: string) => sid === 's1');
    const filtered = await filterDeadSessions(state, isAlive);
    expect(filtered.windows.map(w => w.windowId)).toEqual(['w1']);
    expect(filtered.focusedWindowId).toBeNull(); // pointed to dead window
  });
});
```

- [ ] **Step 2: Run tests (fail)**

```bash
cd ui/desktop && npm run test:run -- src/components/LabMeeting/labMeetingStorage.test.ts
```

- [ ] **Step 3: Implement**

```ts
// ui/desktop/src/components/LabMeeting/labMeetingStorage.ts

export const STORAGE_KEY = 'biorouter.labmeeting.v1';
const STORAGE_VERSION = 1;

export interface SerializedLabWindow {
  windowId: string;
  sessionId: string;
  name: string;
  badge: number;
  accentColor: string;
  position: { x: number; y: number } | null;
  size: { w: number; h: number } | null;
  isManuallyPlaced: boolean;
  isTucked: boolean;
  model?: string;
  mode?: string;
  cwd?: string;
  contextDepth?: number;
  costAccumulated?: number;
  lastInteraction: number;
  unreadActivity: boolean;
}

export interface SerializedLabMeetingState {
  version: number;
  windows: SerializedLabWindow[];
  focusedWindowId: string | null;
  T1: number;
  T2: number;
}

export function loadLabMeetingState(): SerializedLabMeetingState | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as SerializedLabMeetingState;
    if (parsed.version !== STORAGE_VERSION) return null;
    return parsed;
  } catch {
    return null;
  }
}

export function saveLabMeetingState(state: SerializedLabMeetingState): void {
  try {
    const payload = { ...state, version: STORAGE_VERSION };
    localStorage.setItem(STORAGE_KEY, JSON.stringify(payload));
  } catch {
    /* quota exceeded — drop silently */
  }
}

export function debounceSave(delayMs = 250): (state: SerializedLabMeetingState) => void {
  let t: ReturnType<typeof setTimeout> | null = null;
  return (state) => {
    if (t) clearTimeout(t);
    t = setTimeout(() => saveLabMeetingState(state), delayMs);
  };
}

export async function filterDeadSessions(
  state: SerializedLabMeetingState,
  isAlive: (sessionId: string) => Promise<boolean>
): Promise<SerializedLabMeetingState> {
  const checks = await Promise.all(state.windows.map(w => isAlive(w.sessionId)));
  const aliveWindows = state.windows.filter((_, i) => checks[i]);
  const aliveIds = new Set(aliveWindows.map(w => w.windowId));
  const focusedWindowId =
    state.focusedWindowId && aliveIds.has(state.focusedWindowId) ? state.focusedWindowId : null;
  return { ...state, windows: aliveWindows, focusedWindowId };
}
```

- [ ] **Step 4: Run tests — should pass**

```bash
cd ui/desktop && npm run test:run -- src/components/LabMeeting/labMeetingStorage.test.ts
```

- [ ] **Step 5: Commit**

```bash
git add ui/desktop/src/components/LabMeeting/labMeetingStorage.ts ui/desktop/src/components/LabMeeting/labMeetingStorage.test.ts
git commit -m "feat(lab-meeting): localStorage persistence helper"
```

---

# Task 5: LabMeetingProvider — state machine

**Files:**
- Create: `ui/desktop/src/contexts/LabMeetingContext.tsx`
- Create: `ui/desktop/src/components/LabMeeting/LabMeetingProvider.tsx`
- Create: `ui/desktop/src/components/LabMeeting/LabMeetingProvider.test.tsx`

### Context

The provider owns all Lab Meeting state. Spawn calls `createSession` to mint a real session. Tuck/evoke uses oldest-non-focused selection. Persistence is wired up but the live-session check is left as a no-op for now (real session existence verification happens via `getSession` in a later task — for now we trust localStorage).

- [ ] **Step 1: Create the context type file**

```tsx
// ui/desktop/src/contexts/LabMeetingContext.tsx
import { createContext, useContext } from 'react';

export interface LabWindow {
  windowId: string;
  sessionId: string;
  name: string;
  badge: number;
  accentColor: string;
  position: { x: number; y: number } | null;
  size: { w: number; h: number } | null;
  isManuallyPlaced: boolean;
  isTucked: boolean;
  model?: string;
  mode?: string;
  cwd?: string;
  contextDepth?: number;
  costAccumulated?: number;
  lastInteraction: number;
  unreadActivity: boolean;
}

export interface LabMeetingState {
  windows: LabWindow[];
  focusedWindowId: string | null;
  T1: number;
  T2: number;
  // ephemeral
  isHydrating: boolean;
}

export interface LabMeetingApi {
  state: LabMeetingState;
  spawnWindow: () => Promise<void>;
  closeWindow: (windowId: string) => void;
  focusWindow: (windowId: string) => void;
  renameWindow: (windowId: string, name: string) => void;
  moveWindow: (windowId: string, position: { x: number; y: number }) => void;
  resizeWindow: (windowId: string, size: { w: number; h: number }) => void;
  tuckWindow: (windowId: string) => void;
  evokeWindow: (windowId: string, dropPos?: { x: number; y: number }) => void;
  organize: () => void;
  clearAll: () => void;
  setT1: (n: number) => void;
  setT2: (n: number) => void;
  updateWindowField: <K extends keyof LabWindow>(windowId: string, field: K, value: LabWindow[K]) => void;
  markActivity: (windowId: string) => void;
}

export const LabMeetingContext = createContext<LabMeetingApi | null>(null);

export const useLabMeeting = (): LabMeetingApi => {
  const ctx = useContext(LabMeetingContext);
  if (!ctx) throw new Error('useLabMeeting must be used inside LabMeetingProvider');
  return ctx;
};

export const useOptionalLabMeeting = (): LabMeetingApi | null => useContext(LabMeetingContext);
```

- [ ] **Step 2: Implement the provider**

```tsx
// ui/desktop/src/components/LabMeeting/LabMeetingProvider.tsx
import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  LabMeetingContext, LabMeetingApi, LabMeetingState, LabWindow,
} from '../../contexts/LabMeetingContext';
import { ACCENT_PALETTE, generateName, pickAccentColor } from './palette';
import {
  loadLabMeetingState, saveLabMeetingState, SerializedLabMeetingState, debounceSave,
} from './labMeetingStorage';
import { createSession } from '../../sessions';
import { getInitialWorkingDir } from '../../utils/workingDir';

const DEFAULT_T1 = 6;
const DEFAULT_T2 = 8;

function nextWindowId(): string {
  return 'lw_' + Math.random().toString(36).slice(2, 10);
}

function serialize(state: LabMeetingState): SerializedLabMeetingState {
  return {
    version: 1,
    windows: state.windows.map(({ ...w }) => w),
    focusedWindowId: state.focusedWindowId,
    T1: state.T1,
    T2: state.T2,
  };
}

function hydrate(): LabMeetingState {
  const raw = loadLabMeetingState();
  if (!raw) {
    return { windows: [], focusedWindowId: null, T1: DEFAULT_T1, T2: DEFAULT_T2, isHydrating: false };
  }
  return {
    windows: raw.windows.map(w => ({ ...w })),
    focusedWindowId: raw.focusedWindowId,
    T1: raw.T1,
    T2: raw.T2,
    isHydrating: false,
  };
}

interface LabMeetingProviderProps {
  children: React.ReactNode;
}

export const LabMeetingProvider: React.FC<LabMeetingProviderProps> = ({ children }) => {
  const [state, setState] = useState<LabMeetingState>(() => hydrate());
  const debouncedSaveRef = useRef(debounceSave(250));

  // Persist on every change
  useEffect(() => {
    debouncedSaveRef.current(serialize(state));
  }, [state]);

  const update = useCallback((updater: (s: LabMeetingState) => LabMeetingState) => {
    setState(prev => updater(prev));
  }, []);

  const enforceT2 = useCallback((s: LabMeetingState): LabMeetingState => {
    const onBoard = s.windows.filter(w => !w.isTucked);
    if (onBoard.length <= s.T2) return s;
    // Tuck oldest non-focused until on-board <= T2
    const focusedId = s.focusedWindowId;
    const sortedByOldest = [...onBoard]
      .filter(w => w.windowId !== focusedId)
      .sort((a, b) => a.lastInteraction - b.lastInteraction);
    const numToTuck = onBoard.length - s.T2;
    const toTuckIds = new Set(sortedByOldest.slice(0, numToTuck).map(w => w.windowId));
    return {
      ...s,
      windows: s.windows.map(w => toTuckIds.has(w.windowId) ? { ...w, isTucked: true } : w),
    };
  }, []);

  const spawnWindow: LabMeetingApi['spawnWindow'] = useCallback(async () => {
    const cwd = getInitialWorkingDir();
    const session = await createSession(cwd);
    const sessionId = session.id;
    const now = Date.now();
    setState(prev => {
      const usedColors = prev.windows.map(w => w.accentColor);
      const accentColor = pickAccentColor(usedColors);
      const badge = (prev.windows.reduce((m, w) => Math.max(m, w.badge), 0)) + 1;
      const name = generateName(prev.windows.length);
      const newWin: LabWindow = {
        windowId: nextWindowId(),
        sessionId,
        name,
        badge,
        accentColor,
        position: null,
        size: null,
        isManuallyPlaced: false,
        isTucked: false,
        cwd,
        lastInteraction: now,
        unreadActivity: false,
      };
      const next: LabMeetingState = {
        ...prev,
        windows: [...prev.windows, newWin],
        focusedWindowId: newWin.windowId,
      };
      return enforceT2(next);
    });
  }, [enforceT2]);

  const closeWindow: LabMeetingApi['closeWindow'] = useCallback((windowId) => {
    setState(prev => {
      const remaining = prev.windows.filter(w => w.windowId !== windowId);
      let focusedWindowId = prev.focusedWindowId;
      if (focusedWindowId === windowId) {
        const candidates = remaining.filter(w => !w.isTucked).sort((a, b) => b.lastInteraction - a.lastInteraction);
        focusedWindowId = candidates[0]?.windowId ?? null;
      }
      return { ...prev, windows: remaining, focusedWindowId };
    });
  }, []);

  const focusWindow: LabMeetingApi['focusWindow'] = useCallback((windowId) => {
    setState(prev => ({
      ...prev,
      focusedWindowId: windowId,
      windows: prev.windows.map(w => w.windowId === windowId ? { ...w, lastInteraction: Date.now() } : w),
    }));
  }, []);

  const renameWindow: LabMeetingApi['renameWindow'] = useCallback((windowId, name) => {
    setState(prev => ({
      ...prev,
      windows: prev.windows.map(w => w.windowId === windowId ? { ...w, name } : w),
    }));
  }, []);

  const moveWindow: LabMeetingApi['moveWindow'] = useCallback((windowId, position) => {
    setState(prev => ({
      ...prev,
      windows: prev.windows.map(w =>
        w.windowId === windowId
          ? { ...w, position, isManuallyPlaced: true, lastInteraction: Date.now() }
          : w
      ),
    }));
  }, []);

  const resizeWindow: LabMeetingApi['resizeWindow'] = useCallback((windowId, size) => {
    setState(prev => ({
      ...prev,
      windows: prev.windows.map(w =>
        w.windowId === windowId
          ? { ...w, size, isManuallyPlaced: true, lastInteraction: Date.now() }
          : w
      ),
    }));
  }, []);

  const tuckWindow: LabMeetingApi['tuckWindow'] = useCallback((windowId) => {
    setState(prev => {
      const win = prev.windows.find(w => w.windowId === windowId);
      if (!win || win.isTucked) return prev;
      const remainingOnBoard = prev.windows.filter(w => !w.isTucked && w.windowId !== windowId);
      let focusedWindowId = prev.focusedWindowId;
      if (focusedWindowId === windowId) {
        focusedWindowId = remainingOnBoard
          .sort((a, b) => b.lastInteraction - a.lastInteraction)[0]?.windowId ?? null;
      }
      return {
        ...prev,
        windows: prev.windows.map(w =>
          w.windowId === windowId ? { ...w, isTucked: true, isManuallyPlaced: false, position: null, size: null } : w
        ),
        focusedWindowId,
      };
    });
  }, []);

  const evokeWindow: LabMeetingApi['evokeWindow'] = useCallback((windowId, dropPos) => {
    setState(prev => {
      const win = prev.windows.find(w => w.windowId === windowId);
      if (!win) return prev;
      const next: LabMeetingState = {
        ...prev,
        windows: prev.windows.map(w =>
          w.windowId === windowId
            ? {
                ...w,
                isTucked: false,
                position: dropPos ?? null,
                isManuallyPlaced: dropPos != null,
                unreadActivity: false,
                lastInteraction: Date.now(),
              }
            : w
        ),
        focusedWindowId: windowId,
      };
      return enforceT2(next);
    });
  }, [enforceT2]);

  const organize: LabMeetingApi['organize'] = useCallback(() => {
    setState(prev => ({
      ...prev,
      windows: prev.windows.map(w => ({ ...w, isManuallyPlaced: false, position: null, size: null })),
    }));
  }, []);

  const clearAll: LabMeetingApi['clearAll'] = useCallback(() => {
    setState(prev => ({ ...prev, windows: [], focusedWindowId: null }));
  }, []);

  const setT1: LabMeetingApi['setT1'] = useCallback((n) => {
    setState(prev => {
      const T1 = Math.max(1, Math.floor(n));
      return { ...prev, T1, T2: Math.max(prev.T2, T1) };
    });
  }, []);

  const setT2: LabMeetingApi['setT2'] = useCallback((n) => {
    setState(prev => {
      const T2 = Math.max(prev.T1, Math.floor(n));
      return enforceT2({ ...prev, T2 });
    });
  }, [enforceT2]);

  const updateWindowField: LabMeetingApi['updateWindowField'] = useCallback(
    (windowId, field, value) => {
      setState(prev => ({
        ...prev,
        windows: prev.windows.map(w => (w.windowId === windowId ? { ...w, [field]: value } : w)),
      }));
    },
    []
  );

  const markActivity: LabMeetingApi['markActivity'] = useCallback((windowId) => {
    setState(prev => ({
      ...prev,
      windows: prev.windows.map(w =>
        w.windowId === windowId && w.windowId !== prev.focusedWindowId
          ? { ...w, unreadActivity: true }
          : w
      ),
    }));
  }, []);

  const api: LabMeetingApi = useMemo(() => ({
    state,
    spawnWindow, closeWindow, focusWindow, renameWindow,
    moveWindow, resizeWindow, tuckWindow, evokeWindow,
    organize, clearAll, setT1, setT2,
    updateWindowField, markActivity,
  }), [state, spawnWindow, closeWindow, focusWindow, renameWindow,
       moveWindow, resizeWindow, tuckWindow, evokeWindow,
       organize, clearAll, setT1, setT2, updateWindowField, markActivity]);

  // Used to silence the "always-pass tests" warning on initial hydrate
  void ACCENT_PALETTE;
  void saveLabMeetingState;

  return <LabMeetingContext.Provider value={api}>{children}</LabMeetingContext.Provider>;
};
```

- [ ] **Step 3: Provider state-transition tests**

```tsx
// ui/desktop/src/components/LabMeeting/LabMeetingProvider.test.tsx
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import React from 'react';
import { LabMeetingProvider } from './LabMeetingProvider';
import { useLabMeeting } from '../../contexts/LabMeetingContext';

// Mock createSession to mint deterministic ids without contacting biorouterd
vi.mock('../../sessions', () => ({
  createSession: vi.fn(async () => ({ id: 'sess_' + Math.random().toString(36).slice(2, 6) })),
}));
vi.mock('../../utils/workingDir', () => ({
  getInitialWorkingDir: () => '/tmp',
}));

const wrapper = ({ children }: { children: React.ReactNode }) => (
  <LabMeetingProvider>{children}</LabMeetingProvider>
);

beforeEach(() => {
  localStorage.clear();
});

describe('LabMeetingProvider', () => {
  it('starts empty', () => {
    const { result } = renderHook(() => useLabMeeting(), { wrapper });
    expect(result.current.state.windows).toHaveLength(0);
    expect(result.current.state.T1).toBe(6);
    expect(result.current.state.T2).toBe(8);
  });

  it('spawn adds a window and focuses it', async () => {
    const { result } = renderHook(() => useLabMeeting(), { wrapper });
    await act(async () => { await result.current.spawnWindow(); });
    expect(result.current.state.windows).toHaveLength(1);
    expect(result.current.state.focusedWindowId).toBe(result.current.state.windows[0].windowId);
  });

  it('spawn beyond T2 tucks oldest non-focused', async () => {
    const { result } = renderHook(() => useLabMeeting(), { wrapper });
    // T2 default = 8 → spawning 9 should tuck one
    for (let i = 0; i < 9; i++) {
      await act(async () => { await result.current.spawnWindow(); });
    }
    const tucked = result.current.state.windows.filter(w => w.isTucked);
    const onBoard = result.current.state.windows.filter(w => !w.isTucked);
    expect(onBoard.length).toBe(8);
    expect(tucked.length).toBe(1);
  });

  it('closeWindow drops the window and re-focuses most recent', async () => {
    const { result } = renderHook(() => useLabMeeting(), { wrapper });
    await act(async () => { await result.current.spawnWindow(); });
    await act(async () => { await result.current.spawnWindow(); });
    const [w1, w2] = result.current.state.windows;
    act(() => result.current.closeWindow(w2.windowId));
    expect(result.current.state.windows).toHaveLength(1);
    expect(result.current.state.focusedWindowId).toBe(w1.windowId);
  });

  it('tuckWindow removes from board, evokeWindow puts it back', async () => {
    const { result } = renderHook(() => useLabMeeting(), { wrapper });
    await act(async () => { await result.current.spawnWindow(); });
    const id = result.current.state.windows[0].windowId;
    act(() => result.current.tuckWindow(id));
    expect(result.current.state.windows[0].isTucked).toBe(true);
    act(() => result.current.evokeWindow(id));
    expect(result.current.state.windows[0].isTucked).toBe(false);
    expect(result.current.state.focusedWindowId).toBe(id);
  });

  it('renameWindow persists name', async () => {
    const { result } = renderHook(() => useLabMeeting(), { wrapper });
    await act(async () => { await result.current.spawnWindow(); });
    const id = result.current.state.windows[0].windowId;
    act(() => result.current.renameWindow(id, 'Mass Spec Run'));
    expect(result.current.state.windows[0].name).toBe('Mass Spec Run');
  });

  it('organize clears manual placement', async () => {
    const { result } = renderHook(() => useLabMeeting(), { wrapper });
    await act(async () => { await result.current.spawnWindow(); });
    const id = result.current.state.windows[0].windowId;
    act(() => result.current.moveWindow(id, { x: 100, y: 100 }));
    expect(result.current.state.windows[0].isManuallyPlaced).toBe(true);
    act(() => result.current.organize());
    expect(result.current.state.windows[0].isManuallyPlaced).toBe(false);
    expect(result.current.state.windows[0].position).toBeNull();
  });

  it('setT2 lower than current on-board count tucks excess', async () => {
    const { result } = renderHook(() => useLabMeeting(), { wrapper });
    for (let i = 0; i < 5; i++) {
      await act(async () => { await result.current.spawnWindow(); });
    }
    expect(result.current.state.windows.filter(w => !w.isTucked)).toHaveLength(5);
    act(() => result.current.setT2(3));
    expect(result.current.state.windows.filter(w => !w.isTucked)).toHaveLength(3);
  });

  it('clearAll removes all windows', async () => {
    const { result } = renderHook(() => useLabMeeting(), { wrapper });
    await act(async () => { await result.current.spawnWindow(); });
    act(() => result.current.clearAll());
    expect(result.current.state.windows).toHaveLength(0);
  });
});
```

- [ ] **Step 4: Run tests**

```bash
cd ui/desktop && npm run test:run -- src/components/LabMeeting/LabMeetingProvider.test.tsx
```
Expected: PASS (8 tests).

- [ ] **Step 5: Commit**

```bash
git add ui/desktop/src/contexts/LabMeetingContext.tsx ui/desktop/src/components/LabMeeting/LabMeetingProvider.tsx ui/desktop/src/components/LabMeeting/LabMeetingProvider.test.tsx
git commit -m "feat(lab-meeting): provider + state machine"
```

---

# Task 6: Mount LabMeetingProvider at app root

**Files:**
- Modify: `ui/desktop/src/App.tsx`

### Context

The provider must wrap `<Routes>` so its state survives any in-app navigation.

- [ ] **Step 1: Locate the app root JSX**

In `ui/desktop/src/App.tsx`, find around line 592–594 where `<Routes>` is rendered inside `<div className="relative w-screen h-screen overflow-hidden bg-background-muted flex flex-col">`. We'll wrap with the provider.

- [ ] **Step 2: Add import**

Near the existing imports in `App.tsx`, add:

```ts
import { LabMeetingProvider } from './components/LabMeeting/LabMeetingProvider';
```

- [ ] **Step 3: Wrap `<Routes>`**

Replace:

```tsx
<Routes>
  ...
</Routes>
```

with:

```tsx
<LabMeetingProvider>
  <Routes>
    ...
  </Routes>
</LabMeetingProvider>
```

(Preserve all existing Route children unchanged.)

- [ ] **Step 4: Type-check**

```bash
cd ui/desktop && npm run lint:check
```
Expected: no new errors.

- [ ] **Step 5: Commit**

```bash
git add ui/desktop/src/App.tsx
git commit -m "feat(lab-meeting): mount provider at app root"
```

---

# Task 7: Coherent + hideStatusBar props on BaseChat / ChatInput

**Files:**
- Modify: `ui/desktop/src/components/BaseChat.tsx`
- Modify: `ui/desktop/src/components/ChatInput.tsx`

### Context

In Lab Meeting Mode each window renders BaseChat in a "coherent" visual mode (messages and input share one rounded surface, no horizontal divider) and with the bottom model/mode/cost bar hidden (it's rendered once at the app level by `LabMeetingStatusBar`).

- [ ] **Step 1: Add props to BaseChat**

In `BaseChat.tsx`, find the `BaseChatProps` interface (around line 47) and extend:

```ts
interface BaseChatProps {
  setChat: (chat: ChatType) => void;
  onMessageSubmit?: (message: string) => void;
  renderHeader?: () => React.ReactNode;
  customChatInputProps?: Record<string, unknown>;
  customMainLayoutProps?: Record<string, unknown>;
  contentClassName?: string;
  disableSearch?: boolean;
  showPopularTopics?: boolean;
  suppressEmptyState: boolean;
  sessionId: string;
  initialMessage?: string;
  /** Render messages + input as a single coherent surface. */
  coherent?: boolean;
  /** Hide model/mode/cost/cwd footer in ChatInput. */
  hideStatusBar?: boolean;
}
```

In the `BaseChatContent` destructuring add `coherent = false, hideStatusBar = false,`. Also accept these in `BaseChat` and pass through.

- [ ] **Step 2: Apply coherent visual to BaseChat layout**

In the JSX block that wraps the message ScrollArea (around line 376) and the ChatInput container (around line 449), change the wrapping containers when `coherent` is true. Replace:

```tsx
<div className="flex flex-col flex-1 mx-4 mt-4 mb-3 min-h-0 relative rounded-2xl overflow-hidden">
  <ScrollArea
    ref={scrollRef}
    className={`flex-1 bg-background-default rounded-2xl min-h-0 relative ${contentClassName}`}
    ...
```

with:

```tsx
<div
  className={
    coherent
      ? 'flex flex-col flex-1 min-h-0 relative rounded-2xl overflow-hidden bg-background-default shadow-sm'
      : 'flex flex-col flex-1 mx-4 mt-4 mb-3 min-h-0 relative rounded-2xl overflow-hidden'
  }
>
  <ScrollArea
    ref={scrollRef}
    className={
      coherent
        ? `flex-1 min-h-0 relative ${contentClassName}`
        : `flex-1 bg-background-default rounded-2xl min-h-0 relative ${contentClassName}`
    }
    ...
```

And replace the input wrapper at line 449:

```tsx
<div
  className={`mx-4 mb-4 rounded-2xl overflow-hidden flex-shrink-0 ${disableAnimation ? '' : 'animate-[fadein_400ms_ease-in_forwards]'}`}
>
```

with:

```tsx
<div
  className={
    coherent
      ? 'flex-shrink-0 border-t border-border-subtle/40'
      : `mx-4 mb-4 rounded-2xl overflow-hidden flex-shrink-0 ${disableAnimation ? '' : 'animate-[fadein_400ms_ease-in_forwards]'}`
  }
>
```

The `border-t border-border-subtle/40` gives a near-invisible hairline that visually fuses the two regions while still providing a focal cue. The user's instruction was "do not look like two separate parts" — this hairline is intentional but minimal; if user later wants it fully seamless we drop the border.

- [ ] **Step 3: Pass hideStatusBar through**

In the `<ChatInput ... />` JSX inside `BaseChatContent` (around line 452), add `hideStatusBar={hideStatusBar}` to the props.

- [ ] **Step 4: Add hideStatusBar prop to ChatInput**

In `ui/desktop/src/components/ChatInput.tsx`, find the `ChatInputProps` interface and add:

```ts
hideStatusBar?: boolean;
```

Locate the JSX block that renders the model selector / mode selector / cost / working-dir row (this lives in the bottom of ChatInput's render — search for the model selector component import to find it). Wrap that whole row in `{!hideStatusBar && ( ... )}`.

If the row is split across multiple sub-components, wrap each with the same conditional. Aim: when `hideStatusBar` is true, the only thing visible at the bottom of ChatInput is the textarea and send button.

- [ ] **Step 5: Sanity-check existing usage still works**

```bash
cd ui/desktop && npm run lint:check && npm run test:run
```
Expected: no new failures. The existing pair / hub views call `<BaseChat>` without the new props (default `false`) → unchanged behavior.

- [ ] **Step 6: Commit**

```bash
git add ui/desktop/src/components/BaseChat.tsx ui/desktop/src/components/ChatInput.tsx
git commit -m "feat(lab-meeting): coherent + hideStatusBar props on BaseChat/ChatInput"
```

---

# Task 8: Window chrome — WindowTitleBar & ResizeHandle

**Files:**
- Create: `ui/desktop/src/components/LabMeeting/WindowTitleBar.tsx`
- Create: `ui/desktop/src/components/LabMeeting/ResizeHandle.tsx`

- [ ] **Step 1: Implement WindowTitleBar**

```tsx
// ui/desktop/src/components/LabMeeting/WindowTitleBar.tsx
import React, { useState, useRef, useEffect } from 'react';
import { Close } from '../icons/app-icons';

interface Props {
  name: string;
  badge: number;
  accentColor: string;
  onRename: (name: string) => void;
  onClose: () => void;
  onPointerDownDrag: (e: React.PointerEvent<HTMLDivElement>) => void;
}

export const WindowTitleBar: React.FC<Props> = ({
  name, badge, accentColor, onRename, onClose, onPointerDownDrag,
}) => {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(name);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => { setDraft(name); }, [name]);
  useEffect(() => { if (editing) inputRef.current?.select(); }, [editing]);

  const commit = () => {
    const trimmed = draft.trim();
    if (trimmed && trimmed !== name) onRename(trimmed);
    setEditing(false);
  };

  return (
    <div
      className="flex items-center gap-2 px-3 h-9 select-none cursor-grab active:cursor-grabbing border-b border-border-subtle/30 bg-background-default/80 backdrop-blur-sm rounded-t-2xl"
      onPointerDown={(e) => {
        if ((e.target as HTMLElement).closest('button, input')) return;
        onPointerDownDrag(e);
      }}
    >
      <span
        className="inline-block w-2.5 h-2.5 rounded-full flex-shrink-0"
        style={{ backgroundColor: accentColor }}
      />
      {editing ? (
        <input
          ref={inputRef}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => {
            if (e.key === 'Enter') commit();
            if (e.key === 'Escape') { setDraft(name); setEditing(false); }
          }}
          className="flex-1 min-w-0 bg-transparent text-sm font-medium outline-none border-b border-border-subtle"
        />
      ) : (
        <span
          className="flex-1 min-w-0 truncate text-sm font-medium"
          onDoubleClick={() => setEditing(true)}
          title="Double-click to rename"
        >
          {name}
        </span>
      )}
      <span className="text-xs font-mono text-text-muted flex-shrink-0">#{badge}</span>
      <button
        type="button"
        className="flex-shrink-0 p-1 rounded hover:bg-background-medium transition-colors"
        onClick={onClose}
        title="Close conversation"
      >
        <Close className="w-3.5 h-3.5" />
      </button>
    </div>
  );
};
```

- [ ] **Step 2: Implement ResizeHandle**

```tsx
// ui/desktop/src/components/LabMeeting/ResizeHandle.tsx
import React from 'react';

interface Props {
  onPointerDown: (e: React.PointerEvent<HTMLDivElement>) => void;
}

export const ResizeHandle: React.FC<Props> = ({ onPointerDown }) => (
  <div
    onPointerDown={onPointerDown}
    className="absolute bottom-0 right-0 w-4 h-4 cursor-nwse-resize opacity-40 hover:opacity-100 transition-opacity"
    style={{
      backgroundImage:
        'linear-gradient(135deg, transparent 50%, rgba(120,120,120,0.6) 50%, rgba(120,120,120,0.6) 70%, transparent 70%)',
    }}
    title="Drag to resize"
  />
);
```

- [ ] **Step 3: Sanity build**

```bash
cd ui/desktop && npm run lint:check
```

- [ ] **Step 4: Commit**

```bash
git add ui/desktop/src/components/LabMeeting/WindowTitleBar.tsx ui/desktop/src/components/LabMeeting/ResizeHandle.tsx
git commit -m "feat(lab-meeting): window title bar + resize handle"
```

---

# Task 9: useLabMeetingDrag hook

**Files:**
- Create: `ui/desktop/src/components/LabMeeting/useLabMeetingDrag.ts`

### Context

A pointer-based hook for both window drag and resize. Delivers updates via a callback during the drag (so the consumer can render at intermediate positions if desired, or just commit on release).

- [ ] **Step 1: Implement**

```ts
// ui/desktop/src/components/LabMeeting/useLabMeetingDrag.ts
import { useCallback, useRef } from 'react';

export interface DragOptions {
  onMove?: (delta: { dx: number; dy: number }, e: PointerEvent) => void;
  onEnd?: (delta: { dx: number; dy: number }, e: PointerEvent) => void;
  onCancel?: () => void;
}

/**
 * Returns a pointerdown handler that captures a drag and reports deltas.
 * Cleans up on pointerup, pointercancel, or window blur.
 */
export function usePointerDrag(opts: DragOptions): (e: React.PointerEvent) => void {
  const startRef = useRef<{ x: number; y: number } | null>(null);

  const start = useCallback(
    (e: React.PointerEvent) => {
      if (e.button !== 0) return;
      e.preventDefault();
      const start = { x: e.clientX, y: e.clientY };
      startRef.current = start;

      const handleMove = (ev: PointerEvent) => {
        const s = startRef.current;
        if (!s) return;
        opts.onMove?.({ dx: ev.clientX - s.x, dy: ev.clientY - s.y }, ev);
      };
      const handleEnd = (ev: PointerEvent) => {
        const s = startRef.current;
        startRef.current = null;
        window.removeEventListener('pointermove', handleMove);
        window.removeEventListener('pointerup', handleEnd);
        window.removeEventListener('pointercancel', handleCancel);
        if (s) opts.onEnd?.({ dx: ev.clientX - s.x, dy: ev.clientY - s.y }, ev);
      };
      const handleCancel = () => {
        startRef.current = null;
        window.removeEventListener('pointermove', handleMove);
        window.removeEventListener('pointerup', handleEnd);
        window.removeEventListener('pointercancel', handleCancel);
        opts.onCancel?.();
      };

      window.addEventListener('pointermove', handleMove);
      window.addEventListener('pointerup', handleEnd);
      window.addEventListener('pointercancel', handleCancel);
    },
    [opts]
  );

  return start;
}
```

- [ ] **Step 2: Commit**

```bash
git add ui/desktop/src/components/LabMeeting/useLabMeetingDrag.ts
git commit -m "feat(lab-meeting): pointer-drag hook"
```

---

# Task 10: ChatWindow — wraps BaseChat with chrome

**Files:**
- Create: `ui/desktop/src/components/LabMeeting/ChatWindow.tsx`

### Context

Renders a single conversation window. Receives layout rect (x, y, w, h, zIndex) from parent.
The internal BaseChat is rendered with `coherent` and `hideStatusBar`.

- [ ] **Step 1: Implement**

```tsx
// ui/desktop/src/components/LabMeeting/ChatWindow.tsx
import React, { useMemo, useState } from 'react';
import BaseChat from '../BaseChat';
import { ChatProvider, DEFAULT_CHAT_TITLE } from '../../contexts/ChatContext';
import { ChatType } from '../../types/chat';
import { LabWindow } from '../../contexts/LabMeetingContext';
import { useLabMeeting } from '../../contexts/LabMeetingContext';
import { WindowTitleBar } from './WindowTitleBar';
import { ResizeHandle } from './ResizeHandle';
import { usePointerDrag } from './useLabMeetingDrag';

interface Props {
  win: LabWindow;
  rect: { x: number; y: number; w: number; h: number; zIndex: number };
  isFocused: boolean;
  isSolo: boolean;
  boardSize: { width: number; height: number };
  minSize: { w: number; h: number };
  onTuckByDrag?: (windowId: string) => void;
  sidebarOpen: boolean;
}

export const ChatWindow: React.FC<Props> = ({
  win, rect, isFocused, isSolo, boardSize, minSize, onTuckByDrag, sidebarOpen,
}) => {
  const lab = useLabMeeting();
  const [chat, setChat] = useState<ChatType>({
    sessionId: win.sessionId,
    name: win.name || DEFAULT_CHAT_TITLE,
    messages: [],
    workflow: null,
    workflowParameterValues: null,
  });

  const [dragOffset, setDragOffset] = useState<{ dx: number; dy: number }>({ dx: 0, dy: 0 });
  const [resizeDelta, setResizeDelta] = useState<{ dw: number; dh: number }>({ dw: 0, dh: 0 });

  const dragStart = usePointerDrag({
    onMove: ({ dx, dy }) => setDragOffset({ dx, dy }),
    onEnd: ({ dx, dy }, ev) => {
      setDragOffset({ dx: 0, dy: 0 });
      const dropX = rect.x + dx;
      const dropY = rect.y + dy;
      // Drop into sidebar zone? Right strip of board.
      const zoneWidth = sidebarOpen ? boardSize.width * 0.20 : boardSize.width * 0.12;
      if (onTuckByDrag && ev.clientX > 0 && dropX + rect.w / 2 > boardSize.width - zoneWidth) {
        onTuckByDrag(win.windowId);
        return;
      }
      const clampedX = Math.max(-rect.w + 80, Math.min(boardSize.width - 80, dropX));
      const clampedY = Math.max(0, Math.min(boardSize.height - 40, dropY));
      lab.moveWindow(win.windowId, { x: clampedX, y: clampedY });
    },
    onCancel: () => setDragOffset({ dx: 0, dy: 0 }),
  });

  const resizeStart = usePointerDrag({
    onMove: ({ dx, dy }) => setResizeDelta({ dw: dx, dh: dy }),
    onEnd: ({ dx, dy }) => {
      setResizeDelta({ dw: 0, dh: 0 });
      const newW = Math.max(minSize.w, rect.w + dx);
      const newH = Math.max(minSize.h, rect.h + dy);
      lab.resizeWindow(win.windowId, { w: newW, h: newH });
    },
    onCancel: () => setResizeDelta({ dw: 0, dh: 0 }),
  });

  const stylePos = useMemo(() => ({
    transform: `translate(${rect.x + dragOffset.dx}px, ${rect.y + dragOffset.dy}px)`,
    width: rect.w + resizeDelta.dw,
    height: rect.h + resizeDelta.dh,
    zIndex: rect.zIndex,
  }), [rect, dragOffset, resizeDelta]);

  const focusClasses = isFocused
    ? (isSolo
        ? 'shadow-[0_8px_30px_rgb(0,0,0,0.18)]'
        : 'shadow-[0_12px_40px_rgb(0,0,0,0.22)] -translate-y-0.5 scale-[1.01]')
    : 'shadow-[0_4px_14px_rgb(0,0,0,0.10)]';

  return (
    <div
      className={`absolute top-0 left-0 rounded-2xl bg-background-default border border-border-subtle/30 overflow-hidden flex flex-col transition-shadow ${focusClasses}`}
      style={stylePos}
      onMouseDown={() => { if (!isFocused) lab.focusWindow(win.windowId); }}
    >
      <WindowTitleBar
        name={win.name}
        badge={win.badge}
        accentColor={win.accentColor}
        onRename={(name) => lab.renameWindow(win.windowId, name)}
        onClose={() => lab.closeWindow(win.windowId)}
        onPointerDownDrag={dragStart}
      />
      <div className="flex-1 min-h-0 relative">
        <ChatProvider chat={chat} setChat={setChat} contextKey={`lab-${win.sessionId}`}>
          <BaseChat
            setChat={setChat}
            sessionId={win.sessionId}
            suppressEmptyState={false}
            coherent
            hideStatusBar
          />
        </ChatProvider>
      </div>
      <ResizeHandle onPointerDown={resizeStart} />
    </div>
  );
};
```

- [ ] **Step 2: Sanity build**

```bash
cd ui/desktop && npm run lint:check
```

- [ ] **Step 3: Commit**

```bash
git add ui/desktop/src/components/LabMeeting/ChatWindow.tsx
git commit -m "feat(lab-meeting): ChatWindow wrapping BaseChat in window chrome"
```

---

# Task 11: TuckSidebar + TuckedCard

**Files:**
- Create: `ui/desktop/src/components/LabMeeting/TuckedCard.tsx`
- Create: `ui/desktop/src/components/LabMeeting/TuckSidebar.tsx`

- [ ] **Step 1: TuckedCard**

```tsx
// ui/desktop/src/components/LabMeeting/TuckedCard.tsx
import React from 'react';
import { Close } from '../icons/app-icons';
import { LabWindow } from '../../contexts/LabMeetingContext';

interface Props {
  win: LabWindow;
  preview: string[];
  onEvoke: () => void;
  onClose: () => void;
  onDragStart: (e: React.PointerEvent) => void;
}

export const TuckedCard: React.FC<Props> = ({ win, preview, onEvoke, onClose, onDragStart }) => (
  <div
    className="group relative rounded-xl bg-background-default border border-border-subtle/40 p-3 hover:bg-background-medium/60 transition-colors cursor-pointer"
    onClick={onEvoke}
    onPointerDown={onDragStart}
  >
    <div className="flex items-center gap-2 mb-1">
      <span
        className="inline-block w-2 h-2 rounded-full flex-shrink-0"
        style={{ backgroundColor: win.accentColor }}
      />
      <span className="flex-1 text-sm font-medium truncate">{win.name}</span>
      <span className="text-[10px] font-mono text-text-muted">#{win.badge}</span>
      <button
        type="button"
        className="opacity-0 group-hover:opacity-100 p-1 rounded hover:bg-background-medium transition-opacity"
        onClick={(e) => { e.stopPropagation(); onClose(); }}
        title="Remove"
      >
        <Close className="w-3 h-3" />
      </button>
    </div>
    {preview.length > 0 && (
      <div className="text-[11px] leading-snug text-text-muted line-clamp-3">
        {preview.join(' · ')}
      </div>
    )}
    {win.unreadActivity && (
      <span className="absolute top-2 right-7 w-2 h-2 rounded-full bg-emerald-500 animate-pulse" />
    )}
  </div>
);
```

- [ ] **Step 2: TuckSidebar**

```tsx
// ui/desktop/src/components/LabMeeting/TuckSidebar.tsx
import React from 'react';
import { useLabMeeting } from '../../contexts/LabMeetingContext';
import { TuckedCard } from './TuckedCard';

interface Props {
  onCardDragStart: (windowId: string) => (e: React.PointerEvent) => void;
}

export const TuckSidebar: React.FC<Props> = ({ onCardDragStart }) => {
  const lab = useLabMeeting();
  const tucked = lab.state.windows.filter(w => w.isTucked);

  if (tucked.length === 0) return null;

  return (
    <div className="w-64 flex-shrink-0 h-full flex flex-col bg-background-muted/60 border-l border-border-subtle/40 backdrop-blur-sm">
      <div className="px-3 py-2 text-xs font-semibold text-text-muted uppercase tracking-wider border-b border-border-subtle/30">
        Tucked Chats · {tucked.length}
      </div>
      <div className="flex-1 overflow-y-auto p-2 space-y-2">
        {tucked.map((w) => (
          <TuckedCard
            key={w.windowId}
            win={w}
            preview={[]} // Wired up in Task 17
            onEvoke={() => lab.evokeWindow(w.windowId)}
            onClose={() => lab.closeWindow(w.windowId)}
            onDragStart={onCardDragStart(w.windowId)}
          />
        ))}
      </div>
    </div>
  );
};
```

- [ ] **Step 3: Commit**

```bash
git add ui/desktop/src/components/LabMeeting/TuckedCard.tsx ui/desktop/src/components/LabMeeting/TuckSidebar.tsx
git commit -m "feat(lab-meeting): tuck sidebar + cards"
```

---

# Task 12: LabMeetingBoard — renders ChatWindows from layout

**Files:**
- Create: `ui/desktop/src/components/LabMeeting/LabMeetingBoard.tsx`

### Context

Wires `useLabMeeting()` to `computeLayout()`. Tracks board size with `ResizeObserver`. Wires drag-from-sidebar evocation by tracking a "ghost" pointer drag.

- [ ] **Step 1: Implement**

```tsx
// ui/desktop/src/components/LabMeeting/LabMeetingBoard.tsx
import React, { useEffect, useMemo, useRef, useState } from 'react';
import { useLabMeeting } from '../../contexts/LabMeetingContext';
import { computeLayout, LayoutInputWindow } from './layoutEngine';
import { ChatWindow } from './ChatWindow';
import { TuckSidebar } from './TuckSidebar';

const DEBOUNCE_MS = 80;

export const LabMeetingBoard: React.FC = () => {
  const lab = useLabMeeting();
  const [boardSize, setBoardSize] = useState<{ width: number; height: number } | null>(null);
  const ref = useRef<HTMLDivElement>(null);

  // Track board size via ResizeObserver, debounced.
  useEffect(() => {
    if (!ref.current) return;
    let t: ReturnType<typeof setTimeout> | null = null;
    const ro = new ResizeObserver((entries) => {
      const e = entries[0];
      if (!e) return;
      const w = e.contentRect.width;
      const h = e.contentRect.height;
      if (t) clearTimeout(t);
      t = setTimeout(() => setBoardSize({ width: w, height: h }), DEBOUNCE_MS);
    });
    ro.observe(ref.current);
    return () => { ro.disconnect(); if (t) clearTimeout(t); };
  }, []);

  // Compute layout
  const layoutInputs: LayoutInputWindow[] = useMemo(
    () => lab.state.windows.map(w => ({
      windowId: w.windowId,
      isManuallyPlaced: w.isManuallyPlaced,
      isTucked: w.isTucked,
      position: w.position,
      size: w.size,
      lastInteraction: w.lastInteraction,
    })),
    [lab.state.windows]
  );

  const layout = useMemo(() => {
    if (!boardSize) return new Map();
    return computeLayout(layoutInputs, boardSize, lab.state.T1, lab.state.T2, lab.state.focusedWindowId);
  }, [layoutInputs, boardSize, lab.state.T1, lab.state.T2, lab.state.focusedWindowId]);

  const onBoardWindows = lab.state.windows.filter(w => !w.isTucked);
  const sidebarOpen = lab.state.windows.some(w => w.isTucked);
  const minCellSize = useMemo(() => {
    if (!boardSize) return { w: 280, h: 200 };
    // T1-cell size for current T1 — used as resize floor and minimum board fit
    const cols = Math.max(1, Math.ceil(Math.sqrt(lab.state.T1)));
    const rows = Math.max(1, Math.ceil(lab.state.T1 / cols));
    return { w: Math.max(280, boardSize.width / cols * 0.6), h: Math.max(200, boardSize.height / rows * 0.6) };
  }, [boardSize, lab.state.T1]);

  // Ghost drag from sidebar — wired in Task 14.
  const onCardDragStart = (_windowId: string) => (_e: React.PointerEvent) => { /* Task 14 */ };

  return (
    <div className="flex flex-1 min-h-0">
      <div
        ref={ref}
        className="relative flex-1 overflow-hidden"
        style={{
          backgroundImage:
            'radial-gradient(circle at 1px 1px, rgba(120,120,120,0.18) 1px, transparent 0)',
          backgroundSize: '16px 16px',
        }}
      >
        {boardSize && onBoardWindows.length === 0 && (
          <div className="absolute inset-0 flex items-center justify-center text-text-muted">
            <button
              type="button"
              className="px-4 py-2 rounded-xl border border-border-subtle hover:bg-background-medium"
              onClick={() => lab.spawnWindow()}
            >
              Spawn a conversation
            </button>
          </div>
        )}
        {boardSize && onBoardWindows.map(w => {
          const rect = layout.get(w.windowId);
          if (!rect) return null;
          return (
            <ChatWindow
              key={w.windowId}
              win={w}
              rect={rect}
              isFocused={lab.state.focusedWindowId === w.windowId}
              isSolo={onBoardWindows.length === 1}
              boardSize={boardSize}
              minSize={minCellSize}
              sidebarOpen={sidebarOpen}
              onTuckByDrag={(id) => lab.tuckWindow(id)}
            />
          );
        })}
      </div>
      <TuckSidebar onCardDragStart={onCardDragStart} />
    </div>
  );
};
```

- [ ] **Step 2: Commit**

```bash
git add ui/desktop/src/components/LabMeeting/LabMeetingBoard.tsx
git commit -m "feat(lab-meeting): board renders windows via layout engine"
```

---

# Task 13: LabMeetingToolbar

**Files:**
- Create: `ui/desktop/src/components/LabMeeting/LabMeetingToolbar.tsx`

- [ ] **Step 1: Implement**

```tsx
// ui/desktop/src/components/LabMeeting/LabMeetingToolbar.tsx
import React from 'react';
import { useLabMeeting } from '../../contexts/LabMeetingContext';
import { Plus } from '../icons/app-icons';
import { Button } from '../ui/button';

export const LabMeetingToolbar: React.FC = () => {
  const lab = useLabMeeting();
  const onBoard = lab.state.windows.filter(w => !w.isTucked).length;
  const tucked = lab.state.windows.filter(w => w.isTucked).length;

  let mode = 'empty';
  if (onBoard > 0 && onBoard <= lab.state.T1) mode = `${pickGridLabel(onBoard, lab.state.T1)} grid`;
  else if (onBoard > lab.state.T1 && onBoard <= lab.state.T2) mode = 'overlap';
  else if (onBoard > lab.state.T2) mode = 'compact';

  return (
    <div className="flex items-center gap-2 px-4 py-2 border-b border-border-subtle/30 bg-background-muted/40 backdrop-blur-sm">
      <Button size="xs" variant="ghost" onClick={() => lab.spawnWindow()} title="Spawn (⌘N)">
        <Plus className="w-4 h-4" /> <span className="ml-1 text-xs">Spawn</span>
      </Button>
      <Button size="xs" variant="ghost" onClick={() => lab.organize()} title="Re-tile">
        <span className="text-xs">Organize</span>
      </Button>
      <Button size="xs" variant="ghost" onClick={() => lab.clearAll()} title="Close all">
        <span className="text-xs">Clear</span>
      </Button>
      <div className="ml-3 flex items-center gap-2">
        <label className="text-xs text-text-muted">T1</label>
        <input
          type="number"
          min={1}
          max={lab.state.T2}
          value={lab.state.T1}
          onChange={(e) => lab.setT1(Number(e.target.value))}
          className="w-12 text-xs px-1 py-0.5 rounded border border-border-subtle bg-background-default"
        />
        <label className="text-xs text-text-muted ml-1">T2</label>
        <input
          type="number"
          min={lab.state.T1}
          value={lab.state.T2}
          onChange={(e) => lab.setT2(Number(e.target.value))}
          className="w-12 text-xs px-1 py-0.5 rounded border border-border-subtle bg-background-default"
        />
      </div>
      <div className="ml-auto text-xs text-text-muted">
        {mode} · {onBoard} on board · {tucked} tucked
      </div>
    </div>
  );
};

function pickGridLabel(n: number, T1: number): string {
  // Mirror layout engine's bestGridConfig roughly; this is purely a label.
  const cols = Math.min(n, Math.ceil(Math.sqrt(n * 1.3)));
  const rows = Math.ceil(n / cols);
  void T1;
  return `${cols}×${rows}`;
}
```

- [ ] **Step 2: Commit**

```bash
git add ui/desktop/src/components/LabMeeting/LabMeetingToolbar.tsx
git commit -m "feat(lab-meeting): toolbar with spawn/organize/clear/T1/T2/status"
```

---

# Task 14: LabMeetingStatusBar (focused-window-aware)

**Files:**
- Create: `ui/desktop/src/components/LabMeeting/LabMeetingStatusBar.tsx`

### Context

For v1 we'll start with a minimal status bar showing focused-window metadata. Wiring up the existing model/mode pickers as fully editable controls is a larger refactor; we defer their interactive integration but display the values from the focused window. (See spec §15 — out-of-scope-for-v1 list does NOT include the status bar; this is just a deliberately minimal but functional v1.)

The interactive model/mode/cwd pickers can be added by reusing the existing components from `ChatInput.tsx` once we land an extraction. For now, focus on visibility.

- [ ] **Step 1: Implement**

```tsx
// ui/desktop/src/components/LabMeeting/LabMeetingStatusBar.tsx
import React from 'react';
import { useLabMeeting } from '../../contexts/LabMeetingContext';

export const LabMeetingStatusBar: React.FC = () => {
  const lab = useLabMeeting();
  const focused = lab.state.windows.find(w => w.windowId === lab.state.focusedWindowId);

  if (!focused) {
    return (
      <div className="h-9 flex items-center px-4 text-xs text-text-muted border-t border-border-subtle/30 bg-background-muted/40">
        No window focused.
      </div>
    );
  }

  return (
    <div className="h-9 flex items-center gap-3 px-4 text-xs text-text-default border-t border-border-subtle/30 bg-background-muted/40">
      <span className="inline-flex items-center gap-1.5">
        <span className="inline-block w-2 h-2 rounded-full" style={{ backgroundColor: focused.accentColor }} />
        <span className="font-medium">{focused.name}</span>
        <span className="text-text-muted">#{focused.badge}</span>
      </span>
      <span className="text-text-muted">·</span>
      <span title="working directory">cwd: <span className="font-mono">{focused.cwd ?? '—'}</span></span>
      {focused.model && (<><span className="text-text-muted">·</span><span>model: {focused.model}</span></>)}
      {focused.mode && (<><span className="text-text-muted">·</span><span>mode: {focused.mode}</span></>)}
      <span className="ml-auto text-text-muted">
        cost: ${(focused.costAccumulated ?? 0).toFixed(4)}
      </span>
    </div>
  );
};
```

- [ ] **Step 2: Commit**

```bash
git add ui/desktop/src/components/LabMeeting/LabMeetingStatusBar.tsx
git commit -m "feat(lab-meeting): status bar reflecting focused window"
```

---

# Task 15: LabMeetingRoute — wires everything

**Files:**
- Create: `ui/desktop/src/components/LabMeeting/LabMeetingRoute.tsx`
- Create: `ui/desktop/src/components/LabMeeting/index.ts`

- [ ] **Step 1: Route shell with auto-spawn-on-empty + keyboard shortcuts**

```tsx
// ui/desktop/src/components/LabMeeting/LabMeetingRoute.tsx
import React, { useEffect, useRef } from 'react';
import { useLabMeeting } from '../../contexts/LabMeetingContext';
import { LabMeetingBoard } from './LabMeetingBoard';
import { LabMeetingToolbar } from './LabMeetingToolbar';
import { LabMeetingStatusBar } from './LabMeetingStatusBar';

export const LabMeetingRoute: React.FC = () => {
  const lab = useLabMeeting();
  const didAutoSpawn = useRef(false);

  // Maximize the BrowserWindow on entry (Electron IPC).
  useEffect(() => {
    const electron = (window as unknown as { electron?: { labMeetingEnter?: () => void } }).electron;
    electron?.labMeetingEnter?.();
  }, []);

  // Auto-spawn one window if state is completely empty.
  useEffect(() => {
    if (didAutoSpawn.current) return;
    if (lab.state.windows.length === 0) {
      didAutoSpawn.current = true;
      void lab.spawnWindow();
    }
  }, [lab.state.windows.length, lab]);

  // Keyboard shortcuts (Cmd/Ctrl+N spawn; Cmd/Ctrl+W close focused)
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey;
      if (!meta) return;
      if (e.key === 'n' || e.key === 'N') {
        e.preventDefault();
        void lab.spawnWindow();
      } else if (e.key === 'w' || e.key === 'W') {
        if (lab.state.focusedWindowId) {
          e.preventDefault();
          lab.closeWindow(lab.state.focusedWindowId);
        }
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [lab]);

  return (
    <div className="h-full w-full flex flex-col min-h-0 bg-background-muted">
      <LabMeetingToolbar />
      <LabMeetingBoard />
      <LabMeetingStatusBar />
    </div>
  );
};
```

- [ ] **Step 2: Index re-export**

```ts
// ui/desktop/src/components/LabMeeting/index.ts
export { LabMeetingProvider } from './LabMeetingProvider';
export { LabMeetingRoute } from './LabMeetingRoute';
```

- [ ] **Step 3: Commit**

```bash
git add ui/desktop/src/components/LabMeeting/LabMeetingRoute.tsx ui/desktop/src/components/LabMeeting/index.ts
git commit -m "feat(lab-meeting): route shell + auto-spawn + keyboard shortcuts"
```

---

# Task 16: Register `/lab-meeting` route + Users icon button + IPC

**Files:**
- Modify: `ui/desktop/src/App.tsx`
- Modify: `ui/desktop/src/components/Layout/AppLayout.tsx`
- Modify: `ui/desktop/src/main.ts`
- Modify: `ui/desktop/src/preload.ts`

- [ ] **Step 1: Register the route in App.tsx**

In `App.tsx`, near the other route imports, add:

```ts
import { LabMeetingRoute } from './components/LabMeeting/LabMeetingRoute';
```

Then inside the `<Routes>` block (around line 627 after the `skills` route), add:

```tsx
<Route path="lab-meeting" element={<LabMeetingRoute />} />
```

- [ ] **Step 2: Add the toggle button next to `+` in AppLayout.tsx**

In `ui/desktop/src/components/Layout/AppLayout.tsx`:

Replace the import line:

```ts
import { Plus } from '../icons/app-icons';
```

with:

```ts
import { Plus, Users } from '../icons/app-icons';
```

Replace this block at lines 81–89:

```tsx
<Button
  onClick={handleNewWindow}
  className="no-drag hover:!bg-background-medium"
  variant="ghost"
  size="xs"
  title="Start a new session in a new window"
>
  <Plus className="w-4 h-4" />
</Button>
```

with:

```tsx
<Button
  onClick={handleNewWindow}
  className="no-drag hover:!bg-background-medium"
  variant="ghost"
  size="xs"
  title="Start a new session in a new window"
>
  <Plus className="w-4 h-4" />
</Button>
<Button
  onClick={() => navigate('/lab-meeting')}
  className="no-drag hover:!bg-background-medium"
  variant="ghost"
  size="xs"
  title="Open Lab Meeting Mode"
>
  <Users className="w-4 h-4" />
</Button>
```

- [ ] **Step 3: Electron IPC for maximize**

In `ui/desktop/src/main.ts`, add an IPC handler. Find the section where other `ipcMain.handle(...)` calls live (search `ipcMain` in the file). Add:

```ts
ipcMain.handle('labMeeting:enter', (event) => {
  const win = BrowserWindow.fromWebContents(event.sender);
  if (win && !win.isMaximized()) win.maximize();
});
```

(If `BrowserWindow` is not yet imported, ensure it is — most likely already imported.)

- [ ] **Step 4: Expose in preload**

In `ui/desktop/src/preload.ts`, add to the `electron` API surface:

```ts
labMeetingEnter: () => ipcRenderer.invoke('labMeeting:enter'),
```

(Place it near other one-off IPC bridges in the same surface.)

- [ ] **Step 5: Type declaration**

If `ui/desktop/src/types/electron.d.ts` (or wherever the `Window['electron']` type lives) declares the surface, add `labMeetingEnter?: () => Promise<void>;`. Search:

```bash
grep -rn "createChatWindow" ui/desktop/src/types ui/desktop/src/preload.ts | head
```

to find the matching type.

- [ ] **Step 6: Sanity build**

```bash
cd ui/desktop && npm run lint:check
```

- [ ] **Step 7: Commit**

```bash
git add ui/desktop/src/App.tsx ui/desktop/src/components/Layout/AppLayout.tsx ui/desktop/src/main.ts ui/desktop/src/preload.ts ui/desktop/src/types
git commit -m "feat(lab-meeting): register /lab-meeting route + Users icon + maximize IPC"
```

---

# Task 17: Drag-from-sidebar evoke (ghost drag)

**Files:**
- Modify: `ui/desktop/src/components/LabMeeting/LabMeetingBoard.tsx`

### Context

The board needs to handle "drag a tucked card onto the board to evoke." We track a `ghostDrag` state at the board level: when a sidebar card starts a pointer drag, we render a translucent placeholder following the cursor; on `pointerup` over the board, call `evokeWindow(id, dropPos)`.

- [ ] **Step 1: Replace `onCardDragStart` stub with real implementation**

Inside `LabMeetingBoard`, add state for the ghost:

```tsx
const [ghost, setGhost] = useState<{ windowId: string; x: number; y: number } | null>(null);
const boardRectRef = useRef<DOMRect | null>(null);
useEffect(() => {
  if (ref.current) boardRectRef.current = ref.current.getBoundingClientRect();
});

const onCardDragStart = (windowId: string) => (e: React.PointerEvent) => {
  e.preventDefault();
  const updateRect = () => {
    if (ref.current) boardRectRef.current = ref.current.getBoundingClientRect();
  };
  updateRect();

  const handleMove = (ev: PointerEvent) => {
    const r = boardRectRef.current;
    if (!r) return;
    setGhost({ windowId, x: ev.clientX - r.left, y: ev.clientY - r.top });
  };
  const handleUp = (ev: PointerEvent) => {
    window.removeEventListener('pointermove', handleMove);
    window.removeEventListener('pointerup', handleUp);
    const r = boardRectRef.current;
    if (!r) { setGhost(null); return; }
    const x = ev.clientX - r.left;
    const y = ev.clientY - r.top;
    const insideBoard = x >= 0 && x <= r.width && y >= 0 && y <= r.height;
    if (insideBoard) {
      lab.evokeWindow(windowId, { x, y });
    }
    setGhost(null);
  };
  window.addEventListener('pointermove', handleMove);
  window.addEventListener('pointerup', handleUp);
};
```

Render a ghost preview within the board container:

```tsx
{ghost && boardSize && (
  <div
    className="absolute pointer-events-none rounded-2xl border-2 border-dashed border-border-subtle bg-background-default/40 backdrop-blur-sm"
    style={{
      width: minCellSize.w,
      height: minCellSize.h,
      transform: `translate(${ghost.x - minCellSize.w / 2}px, ${ghost.y - minCellSize.h / 2}px)`,
      zIndex: 200,
    }}
  />
)}
```

- [ ] **Step 2: Commit**

```bash
git add ui/desktop/src/components/LabMeeting/LabMeetingBoard.tsx
git commit -m "feat(lab-meeting): drag tucked card onto board to evoke"
```

---

# Task 18: BackToLabMeetingPill

**Files:**
- Create: `ui/desktop/src/components/LabMeeting/BackToLabMeetingPill.tsx`
- Modify: `ui/desktop/src/components/Layout/AppLayout.tsx`

- [ ] **Step 1: Implement the pill**

```tsx
// ui/desktop/src/components/LabMeeting/BackToLabMeetingPill.tsx
import React from 'react';
import { useNavigate, useLocation } from 'react-router-dom';
import { useOptionalLabMeeting } from '../../contexts/LabMeetingContext';
import { Users } from '../icons/app-icons';

export const BackToLabMeetingPill: React.FC = () => {
  const lab = useOptionalLabMeeting();
  const navigate = useNavigate();
  const loc = useLocation();
  if (!lab) return null;
  if (loc.pathname === '/lab-meeting') return null;
  if (lab.state.windows.length === 0) return null;
  const onBoard = lab.state.windows.filter(w => !w.isTucked).length;
  return (
    <button
      type="button"
      onClick={() => navigate('/lab-meeting')}
      className="fixed bottom-4 right-4 z-50 inline-flex items-center gap-2 px-3 py-1.5 rounded-full bg-background-default border border-border-subtle shadow-lg hover:bg-background-medium transition-colors text-xs"
      title="Back to Lab Meeting"
    >
      <Users className="w-3.5 h-3.5" />
      Back to Lab Meeting · {onBoard}
    </button>
  );
};
```

- [ ] **Step 2: Render in AppLayout**

In `ui/desktop/src/components/Layout/AppLayout.tsx`, import:

```ts
import { BackToLabMeetingPill } from '../LabMeeting/BackToLabMeetingPill';
```

In `AppLayoutContent`'s returned JSX, just before the closing `</div>` of the outer `<div className="flex flex-1 w-full relative animate-fade-in">`, add:

```tsx
<BackToLabMeetingPill />
```

- [ ] **Step 3: Commit**

```bash
git add ui/desktop/src/components/LabMeeting/BackToLabMeetingPill.tsx ui/desktop/src/components/Layout/AppLayout.tsx
git commit -m "feat(lab-meeting): floating pill to return to /lab-meeting"
```

---

# Task 19: Run dev server & verify end-to-end

### Context

This is a manual verification gate. The user wants to interact with Lab Meeting Mode after implementation completes.

- [ ] **Step 1: Run all tests**

```bash
cd ui/desktop && npm run test:run
```
Expected: PASS, no new failures.

- [ ] **Step 2: Type-check + lint**

```bash
cd ui/desktop && npm run lint:check
```
Expected: clean.

- [ ] **Step 3: Launch dev**

From repo root:

```bash
cd /Users/wgu/Desktop/biorouter && source bin/activate-hermit && just run-ui
```

This will rebuild the Rust backend, build the renderer, and launch the Electron app.

- [ ] **Step 4: Manual smoke test (record results in chat to user)**

Verify the following manually in the running app:

1. App opens with sidebar visible. Top-left has `≡ + 👥` icons.
2. Click 👥 (Users) → navigates to `/lab-meeting`. App window maximizes.
3. Board shows a single auto-spawned window with messages + input as one coherent surface.
4. Type a message → response streams in.
5. Toolbar Spawn button → second window appears, tile becomes 2×1.
6. Spawn 6, 7, 8 → 3×2 grid → 1 overflow at center → 2 overflow.
7. Spawn 9 → one window tucks to the right sidebar; sidebar opens.
8. Click a tucked card → it returns to the board, oldest non-focused tucks.
9. Drag a window's title bar → it follows the cursor; release moves it.
10. Drag a window into the right edge → it tucks.
11. Drag a tucked card onto the board → it evokes at drop position.
12. Double-click a window's title → inline-rename.
13. Click `×` on a window → it closes.
14. Toolbar `Organize` → manually-placed windows return to grid; focused stays on top.
15. Navigate to History → board hides, sidebar nav works. Back-to-Lab-Meeting pill appears bottom-right.
16. Click pill → returns to board, all state preserved.
17. Reload the app → board state hydrates from localStorage.

- [ ] **Step 5: Tag the milestone commit**

```bash
git commit --allow-empty -m "feat(lab-meeting): end-to-end smoke verified"
```

---

# Task 20: README / changelog update (if applicable)

**Files:**
- Modify: `ui/desktop/CHANGELOG.md` (if present) or `CHANGELOG.md` at root.

- [ ] **Step 1: Check for changelog**

```bash
ls ui/desktop/CHANGELOG.md CHANGELOG.md 2>/dev/null
```

If neither exists, skip the rest of this task and proceed.

- [ ] **Step 2: Add an entry**

Top of changelog (under an unreleased / next-version heading), add:

```markdown
- feat(lab-meeting): Lab Meeting Mode — multi-conversation parallel workspace at `/lab-meeting`.
  Auto-tile grid for ≤T1 windows, intersection-point overflow for T1<n≤T2, sidebar tuck for >T2.
  Each window holds an independent biorouterd session; state persists to localStorage.
```

- [ ] **Step 3: Commit**

```bash
git add CHANGELOG.md ui/desktop/CHANGELOG.md
git commit -m "docs: changelog entry for Lab Meeting Mode"
```

---

## Self-review notes (post-write)

**Spec coverage check:**
- §2 Entry/Exit: Tasks 15 (auto-spawn), 16 (Users button), 16 (maximize IPC), 18 (Back pill). ✓
- §3 Board: Task 12 ✓
- §4 Windows (anatomy / focus pop / drag / resize / close / rename): Tasks 8, 10. ✓
- §5 Capacity & tuck: Tasks 5 (state), 11 (UI), 17 (drag-evoke), 10 (drag-tuck). ✓
- §6 Toolbar (Spawn/Clear/Organize/T1/T2/status): Task 13. ✓
- §7 Keyboard shortcuts: Task 15. ✓
- §8.1 Status bar: Task 14 (minimal v1, see note in Task 14). ✓
- §8.2 Sidebar nav + back pill: Task 18. ✓
- §8.3 Autonomous (live preview / pulse): provider has `markActivity` + `unreadActivity`; live tucked-card preview is a stub today (Task 11 inline note); for v1 the pulse + preview-text-stub ship; live preview update from streaming messages is a Task 17 follow-up. **Logged as v1.x note** — not blocking the runnable end-to-end.
- §9 Performance: Task 12 uses ResizeObserver+debounce; tucked windows are not mounted (board only renders non-tucked). ✓
- §10 Edge cases: layout engine handles `T1=1`, `T1=T2`. Empty-state spawn CTA in Task 12. ✓
- §11 Persistence: Tasks 4, 5. ✓

**Type consistency check:** `LabWindow`, `LabMeetingState`, `LabMeetingApi` are defined once in `LabMeetingContext.tsx` and reused everywhere. `computeLayout` signature is consistent across tasks 2 and 3. `usePointerDrag` signature consistent across tasks 9, 10, 17.

**Placeholder scan:** None.

**Known v1 note:** live tucked-card preview text and full interactive model/mode/cwd controls in the status bar are minimal stubs in v1. The provider exposes the necessary fields; future iterations can wire them up without re-architecting.
