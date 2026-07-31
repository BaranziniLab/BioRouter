import { describe, expect, it } from 'vitest';
import { groupSessionsByParent } from './sessionGrouping';

describe('groupSessionsByParent', () => {
  it('nests sub_agent rows under their parent and keeps orphans top-level', () => {
    const rows = [
      { id: 'p1', session_type: 'user' },
      { id: 'c1', session_type: 'sub_agent', parent_session_id: 'p1' },
      { id: 'c2', session_type: 'sub_agent', parent_session_id: 'gone' },
    ] as const;
    const grouped = groupSessionsByParent([...rows]);
    const parent = grouped.find((g) => g.session.id === 'p1');
    expect(parent?.children.map((c) => c.id)).toEqual(['c1']);
    // A child whose parent is not in the page still shows (top-level, badged).
    expect(grouped.some((g) => g.session.id === 'c2')).toBe(true);
  });
});
