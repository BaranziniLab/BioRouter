import { describe, it, expect } from 'vitest';
import { findSpawnPosition, organize, type WindowRect } from './canvasLayout';

describe('findSpawnPosition', () => {
  it('returns the camera-center top-left when no windows exist', () => {
    const pos = findSpawnPosition({
      center: { x: 100, y: 200 },
      size: { w: 520, h: 440 },
      existing: [],
    });
    expect(pos).toEqual({ x: 100 - 260, y: 200 - 220 });
  });

  it('spirals outward when the center overlaps an existing window', () => {
    const existing = [{ x: -260, y: -220, w: 520, h: 440 }];
    const pos = findSpawnPosition({
      center: { x: 0, y: 0 },
      size: { w: 520, h: 440 },
      existing,
    });
    // Should be offset by at least one cell + gap in some direction
    const dx = Math.abs(pos.x - -260);
    const dy = Math.abs(pos.y - -220);
    expect(dx + dy).toBeGreaterThan(520);
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
    // sizes preserved
    expect(a.w).toBe(520);
    expect(a.h).toBe(440);
    expect(b.w).toBe(520);
    expect(b.h).toBe(440);
    // anchor (a) is unmoved
    expect(a.x).toBe(0);
    expect(a.y).toBe(0);
    // overlap resolved
    const overlapW = Math.max(
      0,
      Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x)
    );
    const overlapH = Math.max(
      0,
      Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y)
    );
    expect(overlapW === 0 || overlapH === 0).toBe(true);
  });

  it('leaves non-overlapping windows untouched', () => {
    const windows: WindowRect[] = [
      { id: 'a', x: 0, y: 0, w: 200, h: 200 },
      { id: 'b', x: 300, y: 0, w: 200, h: 200 },
    ];
    const result = organize(windows, 'a', 16);
    expect(result).toEqual(windows);
  });
});
