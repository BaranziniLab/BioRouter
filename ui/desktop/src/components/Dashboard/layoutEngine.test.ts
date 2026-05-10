import { describe, it, expect } from 'vitest';
import {
  computeLayout,
  hash32,
  EDGE_INSET,
  COMFORT_W,
  COMFORT_H,
  SNAP_GRID,
  Z_TILED,
  LayoutInputWindow,
} from './layoutEngine';

const board = { width: 1200, height: 800 };

describe('hash32', () => {
  it('is deterministic across calls', () => {
    expect(hash32('hello')).toBe(hash32('hello'));
  });
  it('differs across distinct inputs (with high probability)', () => {
    expect(hash32('a')).not.toBe(hash32('b'));
  });
});

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
  it('four windows tile without overlap', () => {
    const ids = ['a', 'b', 'c', 'd'];
    const out = computeLayout(
      ids.map((i) => mkWindow(i)),
      board,
      6,
      8,
      null
    );
    expect(out.size).toBe(4);
    const rects = ids.map((id) => out.get(id)!);
    for (let i = 0; i < rects.length; i++) {
      for (let j = i + 1; j < rects.length; j++) {
        const a = rects[i],
          b = rects[j];
        const ow = Math.max(0, Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x));
        const oh = Math.max(0, Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y));
        expect(ow * oh).toBe(0);
      }
    }
  });

  it('five windows tile without overlap with last row centered', () => {
    const ids = ['a', 'b', 'c', 'd', 'e'];
    const out = computeLayout(
      ids.map((i) => mkWindow(i)),
      board,
      6,
      8,
      null
    );
    expect(out.size).toBe(5);
    const d = out.get('d')!;
    const e = out.get('e')!;
    expect(d.y).toBe(e.y);
    expect(d.x).toBeLessThan(e.x);
    expect(d.x + d.w).toBeLessThanOrEqual(e.x);
  });

  it('six windows tile without overlap', () => {
    const ids = ['a', 'b', 'c', 'd', 'e', 'f'];
    const out = computeLayout(
      ids.map((i) => mkWindow(i)),
      board,
      6,
      8,
      null
    );
    expect(out.size).toBe(6);
    const rects = ids.map((id) => out.get(id)!);
    for (let i = 0; i < rects.length; i++) {
      for (let j = i + 1; j < rects.length; j++) {
        const a = rects[i],
          b = rects[j];
        const ow = Math.max(0, Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x));
        const oh = Math.max(0, Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y));
        expect(ow * oh).toBe(0);
      }
    }
  });

  it('focused window receives top z-index', () => {
    const out = computeLayout([mkWindow('a'), mkWindow('b'), mkWindow('c')], board, 6, 8, 'b');
    const z = (id: string) => out.get(id)!.zIndex;
    expect(z('b')).toBeGreaterThan(z('a'));
    expect(z('b')).toBeGreaterThan(z('c'));
  });

  it('manually-placed window uses its stored position/size, excluded from auto-tile', () => {
    const a = mkWindow('a', {
      isManuallyPlaced: true,
      position: { x: 50, y: 60 },
      size: { w: 300, h: 200 },
    });
    const b = mkWindow('b');
    const out = computeLayout([a, b], board, 6, 8, null);
    expect(out.get('a')!.x).toBe(50);
    expect(out.get('a')!.y).toBe(60);
    expect(out.get('a')!.w).toBe(300);
    expect(out.get('a')!.h).toBe(200);
  });

  it('skips tucked windows entirely', () => {
    const out = computeLayout(
      [mkWindow('a'), mkWindow('b', { isTucked: true })],
      board,
      6,
      8,
      null
    );
    expect(out.has('a')).toBe(true);
    expect(out.has('b')).toBe(false);
  });
});

describe('computeLayout — overflow at intersections (T1 < n ≤ T2)', () => {
  it('places overflow windows at grid intersection points sorted by centrality', () => {
    const ids = ['a', 'b', 'c', 'd', 'e', 'f', 'g'];
    const out = computeLayout(
      ids.map((i) => mkWindow(i)),
      board,
      6,
      8,
      null
    );
    expect(out.size).toBe(7);
    const g = out.get('g')!;
    expect(g.zIndex).toBeGreaterThan(Z_TILED);
    expect(g.x).toBeGreaterThanOrEqual(0);
    expect(g.x + g.w).toBeLessThanOrEqual(board.width);
    expect(g.y).toBeGreaterThanOrEqual(0);
    expect(g.y + g.h).toBeLessThanOrEqual(board.height);
  });

  it('overflow renders above tiled in z-order', () => {
    const ids = ['a', 'b', 'c', 'd', 'e', 'f', 'g'];
    const out = computeLayout(
      ids.map((i) => mkWindow(i)),
      board,
      6,
      8,
      null
    );
    const g = out.get('g')!;
    const a = out.get('a')!;
    expect(g.zIndex).toBeGreaterThan(a.zIndex);
  });

  it('two overflow windows pick distinct intersection points', () => {
    const ids = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'];
    const out = computeLayout(
      ids.map((i) => mkWindow(i)),
      board,
      6,
      8,
      null
    );
    const g = out.get('g')!;
    const h = out.get('h')!;
    expect(g.x !== h.x || g.y !== h.y).toBe(true);
  });

  it('with T1=1 (degenerate grid), overflow windows stack near center with jitter', () => {
    const ids = ['a', 'b', 'c'];
    const out = computeLayout(
      ids.map((i) => mkWindow(i)),
      board,
      1,
      4,
      null
    );
    const b = out.get('b')!;
    const c = out.get('c')!;
    expect(b.x !== c.x || b.y !== c.y).toBe(true);
  });
});

describe('computeLayout — soft-tile pipeline (deterministic, comfort-capped)', () => {
  const wideBoard = { width: 2112, height: 973 };
  const hugeBoard = { width: 4000, height: 2400 };

  it('n=1 → one window at comfort size, centered', () => {
    const out = computeLayout([mkWindow('a')], wideBoard, 6, 8, 'a');
    const r = out.get('a')!;
    expect(r.w).toBe(940);
    expect(r.h).toBe(800);
    expect(Math.abs(r.x + r.w / 2 - wideBoard.width / 2)).toBeLessThanOrEqual(SNAP_GRID);
    expect(Math.abs(r.y + r.h / 2 - wideBoard.height / 2)).toBeLessThanOrEqual(SNAP_GRID);
  });

  it('n=2 → two comfort-size windows side by side', () => {
    const out = computeLayout([mkWindow('a'), mkWindow('b')], wideBoard, 6, 8, null);
    const a = out.get('a')!;
    const b = out.get('b')!;
    expect(a.w).toBe(940);
    expect(a.h).toBe(800);
    expect(b.w).toBe(940);
    expect(b.h).toBe(800);
    expect(a.x).toBeLessThan(b.x);
    expect(a.x + a.w).toBeLessThanOrEqual(b.x);
  });

  it('n=4 on a huge board caps cells at comfort size (not stretched)', () => {
    const ids = ['a', 'b', 'c', 'd'];
    const out = computeLayout(
      ids.map((i) => mkWindow(i)),
      hugeBoard,
      6,
      8,
      null
    );
    for (const id of ids) {
      const r = out.get(id)!;
      expect(r.w).toBeLessThanOrEqual(COMFORT_W);
      expect(r.h).toBeLessThanOrEqual(COMFORT_H);
    }
  });

  it('determinism: 50 invocations with identical inputs produce equal output', () => {
    const ids = ['a', 'b', 'c', 'd', 'e', 'f', 'g'];
    const inputs = ids.map((i) => mkWindow(i));
    const first = computeLayout(inputs, wideBoard, 6, 8, null);
    for (let i = 0; i < 49; i++) {
      const again = computeLayout(inputs, wideBoard, 6, 8, null);
      for (const id of ids) {
        expect(again.get(id)).toEqual(first.get(id));
      }
    }
  });

  it('shuffle stability: same per-id output across input orderings', () => {
    const ids = ['a', 'b', 'c', 'd', 'e', 'f'];
    const baseInputs = ids.map((i) => mkWindow(i));
    const ref = computeLayout(baseInputs, wideBoard, 6, 8, null);
    const rev = computeLayout([...baseInputs].reverse(), wideBoard, 6, 8, null);
    for (const id of ids) {
      expect(rev.get(id)).toEqual(ref.get(id));
    }
  });

  it('pinned avoidance: auto windows do not overlap pinned > 5% area', () => {
    const pinned = mkWindow('p', {
      isManuallyPlaced: true,
      position: { x: 700, y: 250 },
      size: { w: 700, h: 500 },
    });
    const autos = ['a', 'b', 'c', 'd'].map((i) => mkWindow(i));
    const out = computeLayout([pinned, ...autos], wideBoard, 6, 8, null);
    const pRect = out.get('p')!;
    expect(pRect.x).toBe(700);
    expect(pRect.y).toBe(250);
    expect(pRect.w).toBe(700);
    expect(pRect.h).toBe(500);
    for (const id of ['a', 'b', 'c', 'd']) {
      const r = out.get(id)!;
      const oxL = Math.max(r.x, pRect.x);
      const oxR = Math.min(r.x + r.w, pRect.x + pRect.w);
      const oyT = Math.max(r.y, pRect.y);
      const oyB = Math.min(r.y + r.h, pRect.y + pRect.h);
      const overlap = Math.max(0, oxR - oxL) * Math.max(0, oyB - oyT);
      const fraction = overlap / (r.w * r.h);
      expect(fraction).toBeLessThanOrEqual(0.05);
    }
  });

  it('feasible-exit preference: pinned near board edge → auto exits to the open side', () => {
    // Pinned rect hugs the LEFT edge. The auto rect's initial Stage 3 slot
    // straddles the pinned, so Stage 4 must repulse it. The available exits are:
    //   west  → clamps against EDGE_INSET wall (infeasible)
    //   east  → plenty of room (feasible)
    //   north → clamps against the top wall (infeasible at these y values)
    //   south → clamps against the bottom wall (infeasible at these y values)
    // The engine must therefore pick east. We use a single auto so the result
    // isn't confounded by Stage 5's auto-vs-auto tug-of-war (which can drag the
    // auto partway back over pinned territory) — the structural property under
    // test is the Stage 4 exit choice itself.
    const pinned = mkWindow('p', {
      isManuallyPlaced: true,
      position: { x: EDGE_INSET, y: 300 }, // hugging the left edge
      size: { w: 500, h: 400 },
    });
    const auto = mkWindow('a');
    const out = computeLayout([pinned, auto], wideBoard, 6, 8, null);
    const pRect = out.get('p')!;
    const r = out.get('a')!;
    // No overlap with pinned
    const oxL = Math.max(r.x, pRect.x);
    const oxR = Math.min(r.x + r.w, pRect.x + pRect.w);
    const oyT = Math.max(r.y, pRect.y);
    const oyB = Math.min(r.y + r.h, pRect.y + pRect.h);
    const ow = Math.max(0, oxR - oxL);
    const oh = Math.max(0, oyB - oyT);
    expect(ow * oh).toBe(0);
    // Auto sits east of pinned (the feasible exit), not clamped at x=EDGE_INSET
    // (which would mean Stage 4 picked the infeasible west exit and clamped).
    expect(r.x).toBeGreaterThanOrEqual(pRect.x + pRect.w);
  });

  it('edge guarantee: every rect inside [EDGE_INSET, board - EDGE_INSET]', () => {
    const ids = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'];
    const out = computeLayout(
      ids.map((i) => mkWindow(i)),
      wideBoard,
      6,
      8,
      null
    );
    for (const id of ids) {
      const r = out.get(id)!;
      expect(r.x).toBeGreaterThanOrEqual(EDGE_INSET);
      expect(r.y).toBeGreaterThanOrEqual(EDGE_INSET);
      expect(r.x + r.w).toBeLessThanOrEqual(wideBoard.width - EDGE_INSET);
      expect(r.y + r.h).toBeLessThanOrEqual(wideBoard.height - EDGE_INSET);
    }
  });
});
