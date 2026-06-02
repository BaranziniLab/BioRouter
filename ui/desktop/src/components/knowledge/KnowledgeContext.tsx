import {
  createContext,
  ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { listBases, getActive, setActive } from '../../api';
import type { Manifest } from '../../api/types.gen';

const STORAGE_KEY_ACTIVE_KB = 'knowledge_active_kb';

function storageKeyForSession(sessionId: string | null | undefined): string {
  return sessionId ? `${STORAGE_KEY_ACTIVE_KB}:${sessionId}` : STORAGE_KEY_ACTIVE_KB;
}

interface KnowledgeContextType {
  bases: Manifest[];
  loading: boolean;
  activeKb: Manifest | null;
  activeKbId: string | null;
  setActiveKbId: (id: string | null) => void;
  refresh: () => Promise<void>;
  /// Registered by KnowledgeGraphPanel so IngestPanel can request a re-fetch
  /// after each successful ingest. No-op if no graph is mounted.
  registerGraphRefresh: (fn: (() => Promise<void>) | null) => void;
  triggerGraphRefresh: () => void;
}

const KnowledgeContext = createContext<KnowledgeContextType | null>(null);

export function KnowledgeProvider({
  children,
  sessionId = null,
}: {
  children: ReactNode;
  sessionId?: string | null;
}) {
  const [bases, setBases] = useState<Manifest[]>([]);
  const [loading, setLoading] = useState(true);
  const storageKey = useMemo(() => storageKeyForSession(sessionId), [sessionId]);
  const [activeKbId, setActiveKbIdState] = useState<string | null>(() =>
    localStorage.getItem(storageKeyForSession(sessionId))
  );
  const graphRefreshRef = useRef<(() => Promise<void>) | null>(null);

  const setActiveKbId = useCallback((id: string | null) => {
    setActiveKbIdState(id);
    if (id) localStorage.setItem(storageKey, id);
    else localStorage.removeItem(storageKey);
    void setActive({
      body: { kb_id: id, session_id: sessionId || undefined },
      throwOnError: false,
    }).catch((err) => {
      console.warn('setActive (server sync) failed:', err);
    });
  }, [sessionId, storageKey]);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const res = await listBases({ throwOnError: true });
      setBases(res.data || []);
    } catch (err) {
      console.error('listBases failed:', err);
      setBases([]);
    } finally {
      setLoading(false);
    }
  }, []);

  const registerGraphRefresh = useCallback((fn: (() => Promise<void>) | null) => {
    graphRefreshRef.current = fn;
  }, []);

  const triggerGraphRefresh = useCallback(() => {
    void graphRefreshRef.current?.();
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (activeKbId && bases.length > 0 && !bases.some((b) => b.id === activeKbId)) {
      setActiveKbId(null);
    }
  }, [activeKbId, bases, setActiveKbId]);

  useEffect(() => {
    const local = localStorage.getItem(storageKey);
    setActiveKbIdState(local);
    let cancelled = false;

    void (async () => {
      try {
        const res = await getActive({
          query: sessionId ? { session_id: sessionId } : undefined,
          throwOnError: true,
        });
        if (cancelled) return;
        const server = res.data?.active_kb ?? null;
        setActiveKbIdState(server);
        if (server) localStorage.setItem(storageKey, server);
        else localStorage.removeItem(storageKey);
      } catch (err) {
        if (cancelled) return;
        console.warn('getActive (server hydrate) failed:', err);
        setActiveKbIdState(local);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [sessionId, storageKey]);

  const activeKb = useMemo(
    () => bases.find((b) => b.id === activeKbId) ?? null,
    [bases, activeKbId]
  );

  const value: KnowledgeContextType = {
    bases,
    loading,
    activeKb,
    activeKbId,
    setActiveKbId,
    refresh,
    registerGraphRefresh,
    triggerGraphRefresh,
  };

  return <KnowledgeContext.Provider value={value}>{children}</KnowledgeContext.Provider>;
}

export function useKnowledge(): KnowledgeContextType {
  const ctx = useContext(KnowledgeContext);
  if (!ctx) throw new Error('useKnowledge must be used inside <KnowledgeProvider>');
  return ctx;
}
