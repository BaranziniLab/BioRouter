import { describe, it, expect } from 'vitest';
import { findSpawnPosition, organize, type WindowRect, type Rect } from './canvasLayout';

function rectsOverlap(a: Rect, b: Rect): boolean {
  const ow = Math.max(0, Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x));
  const oh = Math.max(0, Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y));
  return ow > 0 && oh > 0;
}

function gapBetween(a: Rect, b: Rect): number {
  // Minimum signed distance between rects along x and y axes (negative = overlap).
  const xGap = Math.max(b.x - (a.x + a.w), a.x - (b.x + b.w));
  const yGap = Math.max(b.y - (a.y + a.h), a.y - (b.y + b.h));
  // If they overlap in both axes, the "gap" is the more-overlapping one (negative).
  // Otherwise it's the larger separation.
  return Math.max(xGap, yGap);
}

describe('findSpawnPosition', () => {
  it('returns the camera-center top-left when no windows exist and no anchor', () => {
    const pos = findSpawnPosition({
      center: { x: 100, y: 200 },
      size: { w: 520, h: 440 },
      existing: [],
    });
    expect(pos).toEqual({ x: 100 - 260, y: 200 - 220 });
  });

  it('places the new window adjacent to the anchor (right side first)', () => {
    const anchor: Rect = { x: 0, y: 0, w: 520, h: 440 };
    const pos = findSpawnPosition({
      center: { x: 0, y: 0 },
      size: { w: 520, h: 440 },
      existing: [anchor],
      anchor,
      gap: 16,
    });
    // Right of anchor: x = anchor.x + anchor.w + gap = 536, y = anchor.y = 0
    expect(pos).toEqual({ x: 536, y: 0 });
  });

  it('falls back to below the anchor when the right is taken', () => {
    const anchor: Rect = { x: 0, y: 0, w: 520, h: 440 };
    const right: Rect = { x: 536, y: 0, w: 520, h: 440 };
    const pos = findSpawnPosition({
      center: { x: 0, y: 0 },
      size: { w: 520, h: 440 },
      existing: [anchor, right],
      anchor,
      gap: 16,
    });
    // Below: y = anchor.y + anchor.h + gap = 456, x = anchor.x = 0
    expect(pos).toEqual({ x: 0, y: 456 });
  });

  it('returned position is non-overlapping with all existing windows', () => {
    const anchor: Rect = { x: 100, y: 100, w: 520, h: 440 };
    const existing: Rect[] = [
      anchor,
      { x: 636, y: 100, w: 520, h: 440 }, // right
      { x: 100, y: 556, w: 520, h: 440 }, // below
    ];
    const pos = findSpawnPosition({
      center: { x: 200, y: 200 },
      size: { w: 520, h: 440 },
      existing,
      anchor,
      gap: 16,
    });
    const newRect = { x: pos.x, y: pos.y, w: 520, h: 440 };
    for (const r of existing) {
      expect(rectsOverlap(newRect, r)).toBe(false);
    }
  });
});

describe('organize', () => {
  it('separates overlapping windows without resizing them', () => {
    const windows: WindowRect[] = [
      { id: 'a', x: 0, y: 0, w: 520, h: 440 },
      { id: 'b', x: 100, y: 100, w: 520, h: 440 },
    ];
    const result = organize(windows, 'a', 16);
    const a = result.find((w) => w.id === 'a')!;
    const b = result.find((w) => w.id === 'b')!;
    expect(a.w).toBe(520);
    expect(a.h).toBe(440);
    expect(b.w).toBe(520);
    expect(b.h).toBe(440);
    expect(rectsOverlap(a, b)).toBe(false);
  });

  it('packs spread-out windows close to the anchor', () => {
    const windows: WindowRect[] = [
      { id: 'anchor', x: 0, y: 0, w: 520, h: 440 },
      { id: 'far', x: 3000, y: 0, w: 520, h: 440 },
    ];
    const result = organize(windows, 'anchor', 16);
    const anchor = result.find((w) => w.id === 'anchor')!;
    const far = result.find((w) => w.id === 'far')!;
    // Anchor unmoved.
    expect(anchor.x).toBe(0);
    expect(anchor.y).toBe(0);
    // Far window should end up just past the anchor (gap=16 to the right).
    expect(far.x).toBeCloseTo(536, 0);
    expect(far.y).toBeCloseTo(0, 0);
  });

  it('maintains the gap margin between any pair of windows post-organize', () => {
    const windows: WindowRect[] = [
      { id: 'a', x: 0, y: 0, w: 300, h: 200 },
      { id: 'b', x: 1000, y: 500, w: 300, h: 200 },
      { id: 'c', x: -500, y: 800, w: 300, h: 200 },
      { id: 'd', x: 700, y: -400, w: 300, h: 200 },
    ];
    const result = organize(windows, 'a', 16);
    // No overlaps.
    for (let i = 0; i < result.length; i++) {
      for (let j = i + 1; j < result.length; j++) {
        expect(rectsOverlap(result[i], result[j])).toBe(false);
      }
    }
    // Every non-anchor window should be no farther than ~2 cell widths from the
    // anchor center — i.e., closer than its original position.
    const a = result.find((w) => w.id === 'a')!;
    const ax = a.x + a.w / 2;
    const ay = a.y + a.h / 2;
    for (const w of result) {
      if (w.id === 'a') continue;
      const dist = Math.hypot(w.x + w.w / 2 - ax, w.y + w.h / 2 - ay);
      expect(dist).toBeLessThan(1500);
    }
  });

  it('enforces gap margin even when rects are separated but too close', () => {
    // Regression: previously Phase A only handled true rect overlap, so
    // when an enlarged window pushed a neighbor into the gap zone but not
    // into overlap with the anchor, the result had < `gap` spacing.
    const windows: WindowRect[] = [
      { id: 'left', x: 0, y: 0, w: 200, h: 200 },
      { id: 'mid', x: 100, y: 0, w: 200, h: 200 }, // overlaps left by 100
      { id: 'anchor', x: 300, y: 0, w: 200, h: 200 }, // gap=0 with mid
    ];
    const result = organize(windows, 'anchor', 16);
    const left = result.find((w) => w.id === 'left')!;
    const mid = result.find((w) => w.id === 'mid')!;
    const anchor = result.find((w) => w.id === 'anchor')!;
    // Anchor unmoved.
    expect(anchor.x).toBe(300);
    // Every adjacent pair must have >= 16 - epsilon gap.
    const epsilon = 0.5;
    expect(anchor.x - (mid.x + mid.w)).toBeGreaterThanOrEqual(16 - epsilon);
    expect(mid.x - (left.x + left.w)).toBeGreaterThanOrEqual(16 - epsilon);
  });

  it('leaves already-adjacent windows alone (no oscillation)', () => {
    const windows: WindowRect[] = [
      { id: 'a', x: 0, y: 0, w: 200, h: 200 },
      { id: 'b', x: 216, y: 0, w: 200, h: 200 }, // exactly 16 gap to the right
    ];
    const result = organize(windows, 'a', 16);
    // Anchor unmoved.
    const a = result.find((w) => w.id === 'a')!;
    const b = result.find((w) => w.id === 'b')!;
    expect(a.x).toBe(0);
    expect(a.y).toBe(0);
    // b stays put (or very close to it).
    expect(b.x).toBeCloseTo(216, 0);
    expect(b.y).toBeCloseTo(0, 0);
    expect(gapBetween(a, b)).toBeGreaterThanOrEqual(15);
  });

  it('lays out 3 equal-size windows in a single row', () => {
    const windows: WindowRect[] = [
      { id: 'a', x: 0, y: 0, w: 100, h: 100 },
      { id: 'b', x: 500, y: 0, w: 100, h: 100 },
      { id: 'c', x: 1000, y: 0, w: 100, h: 100 },
    ];
    const result = organize(windows, 'b', 16);
    const a = result.find((w) => w.id === 'a')!;
    const b = result.find((w) => w.id === 'b')!;
    const c = result.find((w) => w.id === 'c')!;
    // All same y (single row)
    expect(a.y).toBe(b.y);
    expect(b.y).toBe(c.y);
    // a then b then c, each separated by 100 + 16 = 116.
    expect(b.x - a.x).toBe(116);
    expect(c.x - b.x).toBe(116);
    // Anchor (b) is unchanged.
    expect(b.x).toBe(500);
  });

  it('preserves a 2x2 arrangement and snaps each window to the grid', () => {
    // Windows arranged in a rough 2x2 — top-left, top-right, bottom-left,
    // bottom-right. Anchor is top-left. After organize, the relative
    // positions are preserved and each pair is at gap=16 distance.
    const windows: WindowRect[] = [
      { id: 'tl', x: 0, y: 0, w: 100, h: 100 },
      { id: 'tr', x: 300, y: 0, w: 100, h: 100 },
      { id: 'bl', x: 0, y: 300, w: 100, h: 100 },
      { id: 'br', x: 300, y: 300, w: 100, h: 100 },
    ];
    const result = organize(windows, 'tl', 16);
    const tl = result.find((w) => w.id === 'tl')!;
    const tr = result.find((w) => w.id === 'tr')!;
    const bl = result.find((w) => w.id === 'bl')!;
    const br = result.find((w) => w.id === 'br')!;
    // tl unmoved.
    expect(tl.x).toBe(0);
    expect(tl.y).toBe(0);
    // tr is to the right of tl on the same row.
    expect(tr.y).toBe(tl.y);
    expect(tr.x - (tl.x + tl.w)).toBe(16);
    // bl is below tl on the same column.
    expect(bl.x).toBe(tl.x);
    expect(bl.y - (tl.y + tl.h)).toBe(16);
    // br is in the diagonal corner.
    expect(br.x).toBe(tr.x);
    expect(br.y).toBe(bl.y);
  });

  it('preserves a single-row arrangement and snaps to gap-aligned spacing', () => {
    // 5 windows roughly in a row — organize should keep them in a row,
    // not collapse them into a tidy 2x3 grid. Respecting the user's
    // implied arrangement is the contract.
    const windows: WindowRect[] = Array.from({ length: 5 }, (_, i) => ({
      id: 'w' + i,
      x: i * 200,
      y: 0,
      w: 100,
      h: 100,
    }));
    const result = organize(windows, 'w0', 16);
    // All same y — single row preserved.
    const ys = Array.from(new Set(result.map((w) => w.y)));
    expect(ys.length).toBe(1);
    // Each adjacent pair is exactly (100 + 16) = 116 px apart on x.
    const sorted = [...result].sort((a, b) => a.x - b.x);
    for (let i = 1; i < sorted.length; i++) {
      expect(sorted[i].x - sorted[i - 1].x).toBe(116);
    }
    // Anchor (w0) unmoved.
    expect(sorted[0].id).toBe('w0');
    expect(sorted[0].x).toBe(0);
  });

  it('respects relative ordering after a manual swap', () => {
    // User had a, b, c left-to-right; then dragged a and c to swap. After
    // organize, the new arrangement (c, b, a left-to-right) must be the
    // one preserved — not the original input order.
    const windows: WindowRect[] = [
      { id: 'a', x: 1000, y: 0, w: 100, h: 100 }, // moved to right
      { id: 'b', x: 500, y: 0, w: 100, h: 100 }, // anchor, middle
      { id: 'c', x: 0, y: 0, w: 100, h: 100 }, // moved to left
    ];
    const result = organize(windows, 'b', 16);
    const a = result.find((w) => w.id === 'a')!;
    const b = result.find((w) => w.id === 'b')!;
    const c = result.find((w) => w.id === 'c')!;
    // c left of b left of a — user's swapped ordering preserved.
    expect(c.x).toBeLessThan(b.x);
    expect(b.x).toBeLessThan(a.x);
    // Same y (single row), gap=16 between each adjacent pair.
    expect(a.y).toBe(b.y);
    expect(b.y).toBe(c.y);
    expect(a.x - (b.x + b.w)).toBe(16);
    expect(b.x - (c.x + c.w)).toBe(16);
  });

  it('packs vertically when the user arranged windows in a column', () => {
    // 4 windows in a vertical column — organize keeps them in a column.
    const windows: WindowRect[] = Array.from({ length: 4 }, (_, i) => ({
      id: 'w' + i,
      x: 0,
      y: i * 300,
      w: 100,
      h: 100,
    }));
    const result = organize(windows, 'w1', 16);
    const xs = Array.from(new Set(result.map((w) => w.x)));
    expect(xs.length).toBe(1);
    const sorted = [...result].sort((a, b) => a.y - b.y);
    for (let i = 1; i < sorted.length; i++) {
      expect(sorted[i].y - sorted[i - 1].y).toBe(116);
    }
  });

  it('packs mixed sizes into shelves without overlap', () => {
    const windows: WindowRect[] = [
      { id: 'big', x: 0, y: 0, w: 940, h: 800 },
      { id: 's1', x: 0, y: 0, w: 200, h: 200 },
      { id: 's2', x: 0, y: 0, w: 200, h: 200 },
      { id: 's3', x: 0, y: 0, w: 200, h: 200 },
    ];
    const result = organize(windows, 'big', 16);
    // No overlapping pair, all sizes preserved.
    for (let i = 0; i < result.length; i++) {
      for (let j = i + 1; j < result.length; j++) {
        expect(rectsOverlap(result[i], result[j])).toBe(false);
      }
    }
    const big = result.find((w) => w.id === 'big')!;
    expect(big.w).toBe(940);
    expect(big.h).toBe(800);
  });
});
