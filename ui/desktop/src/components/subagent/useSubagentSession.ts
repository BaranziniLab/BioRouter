/**
 * BR-71 §4.5: everything the subagent tab header needs, from the generated
 * client. `getSession` and `cancelTurn` are two of the functions the store
 * already imports (the `from '../api'` brace list in chatStreamStore.tsx).
 * `getSessionExtensions` is NOT in that list; it is a third generated function
 * from the same module (`sdk.gen.ts`, alongside `getSession` and `cancelTurn`).
 */
import { useCallback, useEffect, useState } from 'react';
import { cancelTurn, getSession, getSessionExtensions } from '../../api';

type SubagentSessionInfo = {
  isSubagent: boolean;
  parentSessionId?: string;
  spawnContext?: string;
  extensions: string[];
  stop: () => Promise<void>;
};

/** BR-71: the child's KB grants, from the one place they are recorded. */
export function extractKnowledgeBases(spawnContext?: string): string[] {
  const section = spawnContext?.split('### Knowledge bases')[1]?.split('###')[0]?.trim();
  if (!section || section === '(none)') return [];
  return section
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);
}

export function useSubagentSession(sessionId: string): SubagentSessionInfo {
  const [info, setInfo] = useState<Omit<SubagentSessionInfo, 'stop'>>({
    isSubagent: false,
    extensions: [],
  });

  useEffect(() => {
    let cancelled = false;
    (async () => {
      // BaseChat mounts with an empty sessionId before the sidebar's "New
      // Session" has created one; a GET /sessions/ then 404s on every such
      // mount. There is nothing to look up, so do not ask.
      if (!sessionId) return;
      const session = (await getSession({ path: { session_id: sessionId } })).data;
      if (cancelled || !session || session.session_type !== 'sub_agent') return;
      // The spawn-context record: first message stamped provenance spawn_context
      // (Task 32). Casing verified against the generated client: `MessageMetadata`
      // is camelCase with `provenance?: MessageProvenance | null`,
      // `MessageProvenance` is `{ fromSessionId?, fromSessionName?, kind }`, and
      // `ProvenanceKind` is the snake_case union
      // `'agent_injection' | 'user_direct' | 'spawn_context'`.
      const record = (session.conversation ?? []).find(
        (m) => m?.metadata?.provenance?.kind === 'spawn_context'
      );
      const spawnContext = record?.content?.map((c) => ('text' in c ? c.text : '')).join('\n');
      const extensionsResponse = (await getSessionExtensions({ path: { session_id: sessionId } }))
        .data;
      if (cancelled) return;
      setInfo({
        isSubagent: true,
        parentSessionId: session.parent_session_id ?? undefined,
        spawnContext,
        extensions: (extensionsResponse?.extensions ?? []).map((e) => e.name),
      });
    })().catch(() => {
      /* a failed load renders no header — never breaks the chat */
    });
    return () => {
      cancelled = true;
    };
  }, [sessionId]);

  const stop = useCallback(async () => {
    await cancelTurn({ body: { session_id: sessionId } });
  }, [sessionId]);

  return { ...info, stop };
}
