import { describe, it, expect } from 'vitest';
import { zoneFromRect, EDGE_FRACTION } from './dropZones';

// A 400x200 box at the origin. Edge band = 25% => 100px horizontally, 50px
// vertically. Deliberately NOT square: a square box hides an entire class of bug
// (comparing raw pixel distances instead of normalized fractions would still
// pass on a square, and fail the moment a pane is wider than it is tall — which
// every real chat group is).
const RECT = { left: 0, top: 0, width: 400, height: 200 };

describe('zoneFromRect', () => {
  it('returns center in the middle', () => {
    expect(zoneFromRect(RECT, 200, 100)).toBe('center');
  });

  it('returns the edge for each of the four edge bands', () => {
    expect(zoneFromRect(RECT, 10, 100)).toBe('left');
    expect(zoneFromRect(RECT, 390, 100)).toBe('right');
    expect(zoneFromRect(RECT, 200, 5)).toBe('top');
    expect(zoneFromRect(RECT, 200, 195)).toBe('bottom');
  });

  it('treats the whole central band as center, right up to each threshold', () => {
    // Just inside the left band's inner boundary (100px) => still center.
    expect(zoneFromRect(RECT, 101, 100)).toBe('center');
    expect(zoneFromRect(RECT, 299, 100)).toBe('center');
    // Vertically the band is 50px, because the box is 200 tall, not 400 wide.
    // This is the assertion that fails if the zones are computed from pixels.
    expect(zoneFromRect(RECT, 200, 51)).toBe('center');
    expect(zoneFromRect(RECT, 200, 149)).toBe('center');
  });

  it('is exclusive at the threshold: exactly EDGE_FRACTION in is center', () => {
    // depth >= EDGE_FRACTION is center. 400 * 0.25 = 100.
    expect(zoneFromRect(RECT, 100, 100)).toBe('center');
    expect(zoneFromRect(RECT, 99.9, 100)).toBe('left');
    expect(EDGE_FRACTION).toBe(0.25);
  });

  describe('corners — the single deepest incursion wins, so a corner is unambiguous', () => {
    it('picks left in the top-left corner when the left incursion is deeper', () => {
      // x=4/400 = 1% into the left band; y=8/200 = 4% into the top band.
      // Left is the deeper incursion.
      expect(zoneFromRect(RECT, 4, 8)).toBe('left');
    });

    it('picks top in the top-left corner when the top incursion is deeper', () => {
      // x=40/400 = 10%; y=2/200 = 1%. Top is deeper. Same corner, other answer —
      // this is the pair that proves it is depth and not declaration order.
      expect(zoneFromRect(RECT, 40, 2)).toBe('top');
    });

    it('picks bottom in the bottom-right corner when bottom is deeper', () => {
      // fromRight = 40/400 = 10%; fromBottom = 2/200 = 1%.
      expect(zoneFromRect(RECT, 360, 198)).toBe('bottom');
    });

    it('picks right in the bottom-right corner when right is deeper', () => {
      // fromRight = 4/400 = 1%; fromBottom = 8/200 = 4%.
      expect(zoneFromRect(RECT, 396, 192)).toBe('right');
    });

    it('resolves an exact tie deterministically (declaration order)', () => {
      // fromLeft = 20/400 = 5%; fromTop = 10/200 = 5%. A true diagonal tie.
      // Which one wins is arbitrary; that it is STABLE is not — a flickering
      // corner is unusable.
      expect(zoneFromRect(RECT, 20, 10)).toBe('left');
      expect(zoneFromRect(RECT, 20, 10)).toBe('left');
    });
  });

  it('is stable along an edge: x wobble near the top does not flicker to left', () => {
    // Dragging along the top edge at y=2 (1% deep). Every one of these is deeper
    // into `top` than into `left`, so the zone must not flip as x moves.
    for (const x of [60, 70, 80, 90, 99]) {
      expect(zoneFromRect(RECT, x, 2)).toBe('top');
    }
  });

  it('honours a rect that is not at the origin', () => {
    // The real group boxes never sit at 0,0 — there is a sidebar to the left. A
    // zoneFromRect that forgot rect.left/rect.top would pass every test above.
    const offset = { left: 1000, top: 500, width: 400, height: 200 };
    expect(zoneFromRect(offset, 1200, 600)).toBe('center');
    expect(zoneFromRect(offset, 1010, 600)).toBe('left');
    expect(zoneFromRect(offset, 1390, 600)).toBe('right');
    expect(zoneFromRect(offset, 1200, 505)).toBe('top');
    expect(zoneFromRect(offset, 1200, 695)).toBe('bottom');
  });

  it('calls a zero-sized box center rather than dividing by zero', () => {
    // jsdom's getBoundingClientRect returns all zeroes. Without the guard every
    // fraction is NaN, every `< EDGE_FRACTION` is false, and the answer would be
    // `center` BY ACCIDENT — right result, no reasoning. Assert it on purpose.
    expect(zoneFromRect({ left: 0, top: 0, width: 0, height: 0 }, 0, 0)).toBe('center');
    expect(zoneFromRect({ left: 0, top: 0, width: 400, height: 0 }, 200, 0)).toBe('center');
  });
});
