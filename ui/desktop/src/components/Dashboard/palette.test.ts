import { describe, it, expect } from 'vitest';
import { ACCENT_PALETTE, pickAccentColor, generateName } from './palette';

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

  it('generateName returns the plain "New Session" placeholder', () => {
    // Numbered names are now applied by the LLM after the first message
    // exchange (with collision disambiguation in useChatStream). The local
    // default is intentionally identical for every spawn.
    expect(generateName(0)).toBe('New Session');
    expect(generateName(1)).toBe('New Session');
    expect(generateName(99)).toBe('New Session');
  });
});
