import { useCallback, useEffect, useRef, useState } from 'react';
import { listSidebarSessions, type SessionSummary } from '../../api';
import { subscribeSessionNameChanges } from '../../utils/sessionNameSync';

export const SIDEBAR_SESSION_PAGE_SIZE = 10;

export interface SidebarSessionsState {
  sessions: SessionSummary[];
  hasMore: boolean;
  isLoading: boolean;
  loadMore: () => void;
}

export function appendSessionPage(
  current: SessionSummary[],
  incoming: SessionSummary[]
): SessionSummary[] {
  const merged = [...current];
  const positions = new Map(merged.map((session, index) => [session.id, index]));

  for (const session of incoming) {
    const existingIndex = positions.get(session.id);
    if (existingIndex === undefined) {
      positions.set(session.id, merged.length);
      merged.push(session);
    } else {
      merged[existingIndex] = session;
    }
  }

  return merged;
}

export default function useSidebarSessions(): SidebarSessionsState {
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [hasMore, setHasMore] = useState(true);
  const [isLoading, setIsLoading] = useState(true);
  const sessionsRef = useRef<SessionSummary[]>([]);
  const nextOffsetRef = useRef(0);
  const hasMoreRef = useRef(true);
  const hasLoadedRef = useRef(false);
  const loadingRef = useRef(false);

  const loadPage = useCallback(async (reset: boolean) => {
    if (loadingRef.current || (!reset && !hasMoreRef.current)) return;

    const offset = reset ? 0 : nextOffsetRef.current;
    loadingRef.current = true;
    setIsLoading(true);

    try {
      const response = await listSidebarSessions<true>({
        query: { limit: SIDEBAR_SESSION_PAGE_SIZE, offset },
        throwOnError: true,
      });
      const page = response.data;
      const mergedSessions = appendSessionPage(sessionsRef.current, page.sessions);
      const pageHasMore =
        reset && hasLoadedRef.current ? hasMoreRef.current || page.has_more : page.has_more;

      sessionsRef.current = mergedSessions;
      setSessions(mergedSessions);
      nextOffsetRef.current = reset
        ? mergedSessions.length
        : (page.next_offset ?? offset + page.sessions.length);
      hasMoreRef.current = pageHasMore;
      hasLoadedRef.current = true;
      setHasMore(pageHasMore);
    } catch {
      hasMoreRef.current = false;
      setHasMore(false);
    } finally {
      loadingRef.current = false;
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadPage(true);

    let refreshTimer: number | undefined;
    const scheduleRefresh = () => {
      if (refreshTimer !== undefined) window.clearTimeout(refreshTimer);
      refreshTimer = window.setTimeout(() => {
        refreshTimer = undefined;
        void loadPage(true);
      }, 250);
    };

    const unsubscribeNames = subscribeSessionNameChanges(({ sessionId, name }) => {
      const renamedSessions = sessionsRef.current.map((session) =>
        session.id === sessionId ? { ...session, name } : session
      );
      sessionsRef.current = renamedSessions;
      setSessions(renamedSessions);
    });

    window.addEventListener('session-created', scheduleRefresh);
    window.addEventListener('message-stream-finished', scheduleRefresh);

    return () => {
      unsubscribeNames();
      if (refreshTimer !== undefined) window.clearTimeout(refreshTimer);
      window.removeEventListener('session-created', scheduleRefresh);
      window.removeEventListener('message-stream-finished', scheduleRefresh);
    };
  }, [loadPage]);

  const loadMore = useCallback(() => {
    void loadPage(false);
  }, [loadPage]);

  return {
    sessions,
    hasMore,
    isLoading,
    loadMore,
  };
}
