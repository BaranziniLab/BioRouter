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
    // when all used, falls back to a palette color (ring buffer)
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
