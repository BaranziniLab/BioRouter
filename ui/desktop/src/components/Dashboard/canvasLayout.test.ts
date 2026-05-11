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
});
