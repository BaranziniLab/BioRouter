import { describe, expect, it } from 'vitest';
import {
  announce,
  highestDegree,
  nextNodeInDirection,
  stepThrough,
  tabOrder,
} from './graphKeyboard';
import type { KeyboardNode } from './graphKeyboard';

const n = (id: string, x: number, y: number): KeyboardNode => ({ id, x, y });

describe('nextNodeInDirection', () => {
  const origin = n('o', 0, 0);

  /**
   * Canvas y grows DOWNWARD. Getting this backwards inverts the whole traversal
   * and would still pass a test that only checked "something was returned".
   */
  it('treats up as -y and down as +y, because this is screen space', () => {
    const above = n('above', 0, -100);
    const below = n('below', 0, 100);
    expect(nextNodeInDirection(origin, [above, below], 'up')?.id).toBe('above');
    expect(nextNodeInDirection(origin, [above, below], 'down')?.id).toBe('below');
  });

  it('prefers the nearest candidate inside the ±60° cone', () => {
    const near = n('near', 100, 0);
    const far = n('far', 300, 0);
    expect(nextNodeInDirection(origin, [far, near], 'right')?.id).toBe('near');
  });

  /**
   * The cone must win even when a half-plane candidate is physically closer —
   * otherwise a press drifts sideways and stops feeling directional.
   */
  it('prefers a distant in-cone node over a closer off-cone one', () => {
    const inCone = n('inCone', 200, 0); // 0° off axis
    const offCone = n('offCone', 30, 120); // ~76° off axis, much nearer
    expect(nextNodeInDirection(origin, [offCone, inCone], 'right')?.id).toBe('inCone');
  });

  /**
   * …but a press must never be a no-op when something lies that way, or the key
   * reads as broken.
   */
  it('falls back to the nearest node in the half-plane when the cone is empty', () => {
    const offCone = n('offCone', 30, 200); // well outside ±60°, still to the right
    expect(nextNodeInDirection(origin, [offCone], 'right')?.id).toBe('offCone');
  });

  it('returns null when nothing lies in that direction at all', () => {
    expect(nextNodeInDirection(origin, [n('behind', -100, 0)], 'right')).toBeNull();
    expect(nextNodeInDirection(origin, [], 'up')).toBeNull();
  });

  it('ignores the focused node and anything exactly on top of it', () => {
    // A coincident node has no direction; without the guard the projection is
    // 0/0 = NaN and the candidate is silently lost rather than skipped.
    const stacked = n('stacked', 0, 0);
    const real = n('real', 50, 0);
    expect(nextNodeInDirection(origin, [origin, stacked, real], 'right')?.id).toBe('real');
  });

  it('excludes a perpendicular node, which belongs to neither half-plane', () => {
    expect(nextNodeInDirection(origin, [n('perp', 0, 100)], 'right')).toBeNull();
  });
});

describe('tabOrder', () => {
  const degrees: Record<string, number> = { a: 1, b: 9, c: 5, d: 9 };
  const deg = (id: string) => degrees[id] ?? 0;

  it('walks descending degree, hubs first', () => {
    const order = tabOrder([n('a', 0, 0), n('b', 0, 0), n('c', 0, 0)], deg);
    expect(order.map((x) => x.id)).toEqual(['b', 'c', 'a']);
  });

  /**
   * Ties must break deterministically. Without it two equal-degree nodes can
   * swap between renders and Tab appears to jump backwards.
   */
  it('breaks ties on id, so the order is stable across renders', () => {
    const once = tabOrder([n('d', 0, 0), n('b', 0, 0)], deg).map((x) => x.id);
    const twice = tabOrder([n('b', 0, 0), n('d', 0, 0)], deg).map((x) => x.id);
    expect(once).toEqual(['b', 'd']);
    expect(twice).toEqual(['b', 'd']);
  });
});

describe('stepThrough', () => {
  const order = [n('a', 0, 0), n('b', 0, 0), n('c', 0, 0)];

  it('wraps rather than stopping, so the key is never dead', () => {
    expect(stepThrough(order, 'c', 1)?.id).toBe('a');
    expect(stepThrough(order, 'a', -1)?.id).toBe('c');
  });

  it('enters at the first node forwards and the last backwards', () => {
    expect(stepThrough(order, null, 1)?.id).toBe('a');
    expect(stepThrough(order, null, -1)?.id).toBe('c');
  });

  it('recovers to the first node if the current id has left the set', () => {
    // A filter change can remove the focused node underneath the user.
    expect(stepThrough(order, 'gone', 1)?.id).toBe('a');
  });

  it('returns null on an empty set rather than throwing', () => {
    expect(stepThrough([], 'a', 1)).toBeNull();
  });
});

describe('announce — the channel that replaced the shape encoding', () => {
  it('reads identifier, type, family', () => {
    expect(
      announce({ identifier: 'IL6', label: 'il6.md', nodeType: 'Gene', family: 'Genomic' })
    ).toBe('IL6, Gene, Genomic');
  });

  it('falls back to the label when a node carries no identifier', () => {
    expect(announce({ identifier: null, label: 'il6.md', nodeType: 'Gene', family: 'Genomic' })).toBe(
      'il6.md, Gene, Genomic'
    );
    expect(announce({ identifier: '   ', label: 'il6.md' })).toBe('il6.md');
  });

  /**
   * A legacy or plain-OKF base has no families at all, and an untyped page is a
   * real state. Announcing "undefined" would be worse than announcing less.
   */
  it('drops the parts a base does not have rather than announcing empties', () => {
    expect(announce({ label: 'notes.md' })).toBe('notes.md');
    expect(announce({ label: 'notes.md', nodeType: 'Gene' })).toBe('notes.md, Gene');
    expect(announce({ label: 'notes.md', nodeType: null, family: null })).toBe('notes.md');
  });
});

describe('highestDegree', () => {
  it('is what Home focuses', () => {
    const deg = (id: string) => ({ a: 2, b: 7 })[id as 'a' | 'b'] ?? 0;
    expect(highestDegree([n('a', 0, 0), n('b', 0, 0)], deg)?.id).toBe('b');
  });

  it('returns null on an empty graph', () => {
    expect(highestDegree([], () => 0)).toBeNull();
  });
});
