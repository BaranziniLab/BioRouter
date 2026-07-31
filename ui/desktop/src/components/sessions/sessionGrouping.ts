/** BR-71: History nests subagent transcripts under the session that spawned
 * them. Orphans (parent outside the fetched page, or deleted) stay top-level
 * so nothing becomes unreachable. */
export type SessionRow = {
  id: string;
  session_type?: string | null;
  parent_session_id?: string | null;
  [key: string]: unknown;
};

/**
 * Drop subagent transcripts from a list.
 *
 * History and Home share ONE session cache, and History's toggle decides what
 * that cache holds — so a surface that never wants subagent runs (Home's
 * recents) or does not want them right now (History with the toggle off) must
 * say so at every point it reads the cache, not just where it fetches. The
 * cache identity governs what is *fetched*; this governs what is *shown*.
 */
export function withoutSubagents<T extends SessionRow>(rows: T[]): T[] {
  const kept = rows.filter((row) => row.session_type !== 'sub_agent');
  // Identity-preserving when nothing was dropped — which is the overwhelmingly
  // common case. React state setters and `useMemo` consumers bail out on an
  // unchanged reference, so returning a fresh array every time would add a
  // render pass to every list refresh for no change on screen.
  return kept.length === rows.length ? rows : kept;
}

export function groupSessionsByParent<T extends SessionRow>(
  rows: T[]
): { session: T; children: T[] }[] {
  const byId = new Map(rows.map((row) => [row.id, row]));

  // Which row, if any, this one renders beneath. Only ONE level of nesting is
  // rendered, so a row nests only under a parent that is itself top-level —
  // resolved recursively rather than by a single lookup. In a p → c → g chain a
  // plain lookup hangs g off c, but c is a child and is never scanned for
  // children of its own, so g vanishes from History entirely. The backend
  // refuses nesting inside a delegation tree today; this helper should not
  // silently depend on an invariant enforced somewhere else.
  const nestUnder = new Map<string, string | null>();
  const resolve = (row: T, seen: Set<string>): string | null => {
    const memo = nestUnder.get(row.id);
    if (memo !== undefined) return memo;
    // Malformed data (a parent cycle) — break out and treat this row as
    // top-level rather than recursing forever or losing it.
    if (seen.has(row.id)) return null;

    const parentId = row.session_type === 'sub_agent' ? row.parent_session_id : null;
    const parent = parentId ? byId.get(parentId) : undefined;
    seen.add(row.id);

    let result: string | null = null;
    if (parentId && parent && resolve(parent, seen) === null) result = parentId;
    nestUnder.set(row.id, result);
    return result;
  };

  const childrenOf = new Map<string, T[]>();
  const topLevel: T[] = [];
  for (const row of rows) {
    const parentId = resolve(row, new Set());
    if (parentId === null) {
      topLevel.push(row);
    } else {
      const list = childrenOf.get(parentId) ?? [];
      list.push(row);
      childrenOf.set(parentId, list);
    }
  }
  return topLevel.map((session) => ({ session, children: childrenOf.get(session.id) ?? [] }));
}
