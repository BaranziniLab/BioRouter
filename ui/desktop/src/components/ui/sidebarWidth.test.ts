import { describe, expect, it } from 'vitest';
import {
  SIDEBAR_DEFAULT_WIDTH,
  SIDEBAR_MAX_WIDTH,
  SIDEBAR_MIN_WIDTH,
  SIDEBAR_WIDTH_STORAGE_KEY,
  clampSidebarWidth,
  readStoredSidebarWidth,
  writeStoredSidebarWidth,
} from './sidebarWidth';

/**
 * The sidebar's bounds, tested where they are decidable.
 *
 * ⚠ **jsdom computes no layout**, so nothing that renders the sidebar can
 * observe how wide it actually got — a component test would pass identically
 * with the clamp deleted. What IS decidable without a browser is the arithmetic
 * and the storage round-trip, which is exactly why they live in a module with
 * no React and no DOM. The drag's geometry is verified by driving the real app.
 */

/** A `Storage` stand-in, so the round-trip is tested without touching a global. */
function fakeStorage(initial?: Record<string, string>) {
  const map = new Map(Object.entries(initial ?? {}));
  return {
    getItem: (key: string) => map.get(key) ?? null,
    setItem: (key: string, value: string) => void map.set(key, value),
    read: () => map.get(SIDEBAR_WIDTH_STORAGE_KEY) ?? null,
  };
}

describe('clampSidebarWidth', () => {
  it('leaves the default and both endpoints untouched', () => {
    expect(clampSidebarWidth(SIDEBAR_DEFAULT_WIDTH)).toBe(SIDEBAR_DEFAULT_WIDTH);
    expect(clampSidebarWidth(SIDEBAR_MIN_WIDTH)).toBe(SIDEBAR_MIN_WIDTH);
    expect(clampSidebarWidth(SIDEBAR_MAX_WIDTH)).toBe(SIDEBAR_MAX_WIDTH);
  });

  it('pulls anything outside the range back to the nearest bound', () => {
    expect(clampSidebarWidth(SIDEBAR_MIN_WIDTH - 1)).toBe(SIDEBAR_MIN_WIDTH);
    expect(clampSidebarWidth(SIDEBAR_MAX_WIDTH + 1)).toBe(SIDEBAR_MAX_WIDTH);
    // A drag that runs the pointer off either edge of the screen.
    expect(clampSidebarWidth(-4000)).toBe(SIDEBAR_MIN_WIDTH);
    expect(clampSidebarWidth(4000)).toBe(SIDEBAR_MAX_WIDTH);
    expect(clampSidebarWidth(0)).toBe(SIDEBAR_MIN_WIDTH);
  });

  it('rounds, so the width never lands on a fractional pixel', () => {
    expect(clampSidebarWidth(SIDEBAR_DEFAULT_WIDTH + 0.4)).toBe(SIDEBAR_DEFAULT_WIDTH);
    expect(clampSidebarWidth(SIDEBAR_DEFAULT_WIDTH + 0.6)).toBe(SIDEBAR_DEFAULT_WIDTH + 1);
  });

  /**
   * The width is interpolated straight into a CSS variable, where `NaN` would
   * collapse the sidebar to nothing and read as a rendering bug rather than as
   * a bad number arriving from somewhere.
   */
  it('resolves a non-finite input to the default rather than propagating it', () => {
    expect(clampSidebarWidth(Number.NaN)).toBe(SIDEBAR_DEFAULT_WIDTH);
    expect(clampSidebarWidth(Number.POSITIVE_INFINITY)).toBe(SIDEBAR_DEFAULT_WIDTH);
    expect(clampSidebarWidth(Number.NEGATIVE_INFINITY)).toBe(SIDEBAR_DEFAULT_WIDTH);
  });
});

describe('the stored width', () => {
  it('round-trips a width the user chose', () => {
    const storage = fakeStorage();
    writeStoredSidebarWidth(320, storage);
    expect(storage.read()).toBe('320');
    expect(readStoredSidebarWidth(storage)).toBe(320);
  });

  it('falls back to the default when nothing has been stored', () => {
    expect(readStoredSidebarWidth(fakeStorage())).toBe(SIDEBAR_DEFAULT_WIDTH);
  });

  /**
   * ⚠ The reason the clamp is on the READ and not only on the write. A width
   * stored by an earlier build — 240 when the sidebar was flat, or anything from
   * a range that has since moved — must not place today's sidebar outside
   * today's bounds. Writing correctly is not enough: the value that has to be
   * survivable is the one already on disk.
   */
  it('clamps a value left behind by a previous build', () => {
    expect(readStoredSidebarWidth(fakeStorage({ [SIDEBAR_WIDTH_STORAGE_KEY]: '120' }))).toBe(
      SIDEBAR_MIN_WIDTH
    );
    expect(readStoredSidebarWidth(fakeStorage({ [SIDEBAR_WIDTH_STORAGE_KEY]: '900' }))).toBe(
      SIDEBAR_MAX_WIDTH
    );
  });

  it('clamps on the way in as well, so nothing out of range is ever stored', () => {
    const storage = fakeStorage();
    writeStoredSidebarWidth(9000, storage);
    expect(storage.read()).toBe(String(SIDEBAR_MAX_WIDTH));
  });

  it('resolves an unparseable stored value to the default', () => {
    expect(readStoredSidebarWidth(fakeStorage({ [SIDEBAR_WIDTH_STORAGE_KEY]: 'wide' }))).toBe(
      SIDEBAR_DEFAULT_WIDTH
    );
    expect(readStoredSidebarWidth(fakeStorage({ [SIDEBAR_WIDTH_STORAGE_KEY]: '' }))).toBe(
      SIDEBAR_DEFAULT_WIDTH
    );
  });

  /**
   * A private window, or a browser with site data switched off, throws on the
   * accessor itself. Losing the preference is acceptable; losing the ability to
   * drag the sidebar for this session is not.
   */
  it('survives a storage that throws, in both directions', () => {
    const hostile = {
      getItem: () => {
        throw new Error('access denied');
      },
      setItem: () => {
        throw new Error('access denied');
      },
    };
    expect(readStoredSidebarWidth(hostile)).toBe(SIDEBAR_DEFAULT_WIDTH);
    expect(() => writeStoredSidebarWidth(300, hostile)).not.toThrow();
    expect(readStoredSidebarWidth(null)).toBe(SIDEBAR_DEFAULT_WIDTH);
    expect(() => writeStoredSidebarWidth(300, null)).not.toThrow();
  });
});

describe('the bounds themselves', () => {
  it('put the default strictly inside a range wide enough to be worth dragging', () => {
    expect(SIDEBAR_MIN_WIDTH).toBeLessThan(SIDEBAR_DEFAULT_WIDTH);
    expect(SIDEBAR_DEFAULT_WIDTH).toBeLessThan(SIDEBAR_MAX_WIDTH);
  });

  /**
   * The point of the change: the default is meaningfully wider than the flat
   * 240px the sidebar shipped at, because at 240 a conversation title ran out of
   * room mid-phrase. Pinned against the old value rather than against 288, so
   * this reads as the requirement rather than as a restatement of the constant.
   */
  it('widens the default over the 240px that shipped', () => {
    expect(SIDEBAR_DEFAULT_WIDTH).toBeGreaterThan(240);
  });

  /** …while still letting a user who wants the canvas back go under it. */
  it('lets the user go narrower than the old fixed width', () => {
    expect(SIDEBAR_MIN_WIDTH).toBeLessThan(240);
  });
});
