/** BR-71: History nests subagent transcripts under the session that spawned
 * them. Orphans (parent outside the fetched page, or deleted) stay top-level
 * so nothing becomes unreachable. */
export type SessionRow = {
  id: string;
  session_type?: string | null;
  parent_session_id?: string | null;
  [key: string]: unknown;
};

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
