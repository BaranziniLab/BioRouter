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
    const a = out.get('a')!;
    const b = out.get('b')!;
    expect(a.w).toBe(600);
    expect(a.h).toBe(800);
    expect(a.x).toBe(0);
    expect(b.w).toBe(600);
    expect(b.h).toBe(800);
    expect(b.x).toBe(600);
  });

  it('four windows tile 2×2', () => {
    const ids = ['a', 'b', 'c', 'd'];
    const out = computeLayout(
      ids.map((i) => mkWindow(i)),
      board,
      6,
      8,
      null
    );
    expect(out.size).toBe(4);
    expect(out.get('a')!.w).toBe(600);
    expect(out.get('a')!.h).toBe(400);
    expect(out.get('d')!.x).toBe(600);
    expect(out.get('d')!.y).toBe(400);
  });

  it('five windows: 3×2 with last row centered', () => {
    const ids = ['a', 'b', 'c', 'd', 'e'];
    const out = computeLayout(
      ids.map((i) => mkWindow(i)),
      board,
      6,
      8,
      null
    );
    expect(out.size).toBe(5);
    const e = out.get('e')!;
    const d = out.get('d')!;
    expect(d.y).toBe(400);
    expect(e.y).toBe(400);
    expect(d.x).toBeGreaterThan(0);
    expect(e.x).toBeGreaterThan(d.x);
    const cellW = 400;
    const totalLastRowW = 2 * cellW;
    const expectedLeft = (1200 - totalLastRowW) / 2;
    expect(d.x).toBeCloseTo(expectedLeft, 0);
  });

  it('six windows: 3×2 grid', () => {
    const ids = ['a', 'b', 'c', 'd', 'e', 'f'];
    const out = computeLayout(
      ids.map((i) => mkWindow(i)),
      board,
      6,
      8,
      null
    );
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
    expect(out.get('b')!.w).toBe(1200);
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
    // With 3×2 grid (cellW=400, cellH=400), the closest grid intersection to the
    // board center (600, 400) is at (400, 400) (or (800, 400)), distance 200.
    // Centering the cell on that intersection → top-left (200, 200), with a small
    // i=0 jitter of 0px.
    // Overflow size is capped at half-board (max 600x400), but cellW=400, cellH=400,
    // so for non-degenerate T1 the cap doesn't apply.
    expect(g.x).toBeCloseTo(200, 0);
    expect(g.y).toBeCloseTo(200, 0);
    expect(g.w).toBe(400);
    expect(g.h).toBe(400);
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
