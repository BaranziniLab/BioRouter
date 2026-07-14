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
const STORAGE_KEY_HIDDEN_KBS = 'knowledge_hidden_kbs';

function storageKeyForSession(sessionId: string | null | undefined): string {
  return sessionId ? `${STORAGE_KEY_ACTIVE_KB}:${sessionId}` : STORAGE_KEY_ACTIVE_KB;
}

function hiddenStorageKeyForSession(sessionId: string | null | undefined): string {
  return sessionId ? `${STORAGE_KEY_HIDDEN_KBS}:${sessionId}` : STORAGE_KEY_HIDDEN_KBS;
}

interface KnowledgeContextType {
  bases: Manifest[];
  visibleBases: Manifest[];
  loading: boolean;
  activeKb: Manifest | null;
  activeKbId: string | null;
  hiddenKbIds: string[];
  setActiveKbId: (id: string | null) => void;
  setHiddenKbIds: (ids: string[]) => void;
  toggleKbHidden: (id: string) => void;
  hideAllKnowledgeBases: () => void;
  showAllKnowledgeBases: () => void;
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
  const hiddenStorageKey = useMemo(() => hiddenStorageKeyForSession(sessionId), [sessionId]);
  const [activeKbId, setActiveKbIdState] = useState<string | null>(() =>
    localStorage.getItem(storageKeyForSession(sessionId))
  );
  const [hiddenKbIds, setHiddenKbIdsState] = useState<string[]>(() => {
    try {
      const raw = localStorage.getItem(hiddenStorageKeyForSession(sessionId));
      if (!raw) {
        return [];
      }
      const parsed = JSON.parse(raw);
      return Array.isArray(parsed)
        ? parsed.filter((id): id is string => typeof id === 'string')
        : [];
    } catch {
      return [];
    }
  });
  const graphRefreshRef = useRef<(() => Promise<void>) | null>(null);

  const syncSelection = useCallback(
    (nextActiveKbId: string | null, nextHiddenKbIds: string[]) => {
      setActiveKbIdState(nextActiveKbId);
      setHiddenKbIdsState(nextHiddenKbIds);
      if (nextActiveKbId) localStorage.setItem(storageKey, nextActiveKbId);
      else localStorage.removeItem(storageKey);
      localStorage.setItem(hiddenStorageKey, JSON.stringify(nextHiddenKbIds));
      void setActive({
        body: {
          kb_id: nextActiveKbId,
          hidden_kbs: nextHiddenKbIds,
          session_id: sessionId || undefined,
        },
        throwOnError: false,
      }).catch((err) => {
        console.warn('setActive (server sync) failed:', err);
      });
    },
    [hiddenStorageKey, sessionId, storageKey]
  );

  const setActiveKbId = useCallback(
    (id: string | null) => {
      syncSelection(id, hiddenKbIds);
    },
    [hiddenKbIds, syncSelection]
  );

  const setHiddenKbIds = useCallback(
    (ids: string[]) => {
      const nextIds = Array.from(new Set(ids)).sort();
      syncSelection(activeKbId, nextIds);
    },
    [activeKbId, syncSelection]
  );

  const toggleKbHidden = useCallback(
    (id: string) => {
      const nextIds = hiddenKbIds.includes(id)
        ? hiddenKbIds.filter((hiddenId) => hiddenId !== id)
        : [...hiddenKbIds, id];
      setHiddenKbIds(nextIds);
    },
    [hiddenKbIds, setHiddenKbIds]
  );

  const hideAllKnowledgeBases = useCallback(() => {
    setHiddenKbIds(bases.map((base) => base.id));
  }, [bases, setHiddenKbIds]);

  const showAllKnowledgeBases = useCallback(() => {
    setHiddenKbIds([]);
  }, [setHiddenKbIds]);

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
    const validIds = new Set(bases.map((base) => base.id));
    const nextHiddenKbIds = hiddenKbIds.filter((id) => validIds.has(id));
    if (nextHiddenKbIds.length !== hiddenKbIds.length) {
      setHiddenKbIds(nextHiddenKbIds);
    }
  }, [bases, hiddenKbIds, setHiddenKbIds]);

  useEffect(() => {
    const local = localStorage.getItem(storageKey);
    let localHidden: string[] = [];
    try {
      const rawHidden = localStorage.getItem(hiddenStorageKey);
      if (rawHidden) {
        const parsed = JSON.parse(rawHidden);
        if (Array.isArray(parsed)) {
          localHidden = parsed.filter((id): id is string => typeof id === 'string');
        }
      }
    } catch {
      localHidden = [];
    }
    setActiveKbIdState(local);
    setHiddenKbIdsState(localHidden);
    let cancelled = false;

    void (async () => {
      try {
        const res = await getActive({
          query: sessionId ? { session_id: sessionId } : undefined,
          throwOnError: true,
        });
        if (cancelled) return;
        const server = res.data?.active_kb ?? null;
        const serverHidden = (res.data?.hidden_kbs ?? []).filter(
          (id): id is string => typeof id === 'string'
        );
        setActiveKbIdState(server);
        setHiddenKbIdsState(serverHidden);
        if (server) localStorage.setItem(storageKey, server);
        else localStorage.removeItem(storageKey);
        localStorage.setItem(hiddenStorageKey, JSON.stringify(serverHidden));
      } catch (err) {
        if (cancelled) return;
        console.warn('getActive (server hydrate) failed:', err);
        setActiveKbIdState(local);
        setHiddenKbIdsState(localHidden);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [hiddenStorageKey, sessionId, storageKey]);

  const activeKb = useMemo(
    () => bases.find((b) => b.id === activeKbId) ?? null,
    [bases, activeKbId]
  );
  const visibleBases = useMemo(
    () => bases.filter((base) => !hiddenKbIds.includes(base.id)),
    [bases, hiddenKbIds]
  );

  const value: KnowledgeContextType = {
    bases,
    visibleBases,
    loading,
    activeKb,
    activeKbId,
    hiddenKbIds,
    setActiveKbId,
    setHiddenKbIds,
    toggleKbHidden,
    hideAllKnowledgeBases,
    showAllKnowledgeBases,
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
