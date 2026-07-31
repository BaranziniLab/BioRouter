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
  return rows.filter((row) => row.session_type !== 'sub_agent');
}

export function groupSessionsByParent<T extends SessionRow>(
  rows: T[]
): { session: T; children: T[] }[] {
  const byId = new Map(rows.map((row) => [row.id, row]));
  const childrenOf = new Map<string, T[]>();
  const topLevel: T[] = [];
  for (const row of rows) {
    const parent = row.session_type === 'sub_agent' ? row.parent_session_id : null;
    if (parent && byId.has(parent)) {
      const list = childrenOf.get(parent) ?? [];
      list.push(row);
      childrenOf.set(parent, list);
    } else {
      topLevel.push(row);
    }
  }
  return topLevel.map((session) => ({ session, children: childrenOf.get(session.id) ?? [] }));
}
