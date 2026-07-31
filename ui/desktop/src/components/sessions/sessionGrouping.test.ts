import { describe, expect, it } from 'vitest';
import { groupSessionsByParent, withoutSubagents } from './sessionGrouping';

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

  // Only one level of nesting is rendered, so a grandchild has to come back to
  // top level. Hanging it off a row that is itself a child means nothing ever
  // scans it for children and the row disappears from History entirely —
  // contradicting the promise that nothing becomes unreachable. The backend
  // refuses nesting inside a delegation tree today, so this is latent; the
  // helper should not silently depend on an invariant enforced elsewhere.
  it('renders every row exactly once when the page holds a nested chain', () => {
    const rows = [
      { id: 'p1', session_type: 'user' },
      { id: 'c1', session_type: 'sub_agent', parent_session_id: 'p1' },
      { id: 'g1', session_type: 'sub_agent', parent_session_id: 'c1' },
    ];

    const rendered = groupSessionsByParent(rows)
      .flatMap(({ session, children }) => [session.id, ...children.map((c) => c.id)])
      .sort();

    expect(rendered).toEqual(['c1', 'g1', 'p1']);
  });

  it('does not loop or drop rows when parent links form a cycle', () => {
    const rows = [
      { id: 'a', session_type: 'sub_agent', parent_session_id: 'b' },
      { id: 'b', session_type: 'sub_agent', parent_session_id: 'a' },
    ];

    const rendered = groupSessionsByParent(rows)
      .flatMap(({ session, children }) => [session.id, ...children.map((c) => c.id)])
      .sort();

    expect(rendered).toEqual(['a', 'b']);
  });
});

describe('withoutSubagents', () => {
  it('drops sub_agent rows and preserves the array identity when there are none', () => {
    const clean = [{ id: 'a', session_type: 'user' }];
    // Identity matters: React state setters and memo consumers bail out on an
    // unchanged reference, so a fresh array here costs a render pass per
    // refresh on every surface that filters.
    expect(withoutSubagents(clean)).toBe(clean);

    const mixed = [...clean, { id: 'b', session_type: 'sub_agent', parent_session_id: 'a' }];
    expect(withoutSubagents(mixed).map((r) => r.id)).toEqual(['a']);
  });
});
