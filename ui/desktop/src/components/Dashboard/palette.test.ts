import { describe, it, expect } from 'vitest';
import { ACCENT_PALETTE, pickAccentColor, generateName } from './palette';

describe('palette', () => {
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
