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
import { listBases } from '../../api';
import type { Manifest } from '../../api/types.gen';

const STORAGE_KEY_ACTIVE_KB = 'knowledge_active_kb';

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

export function KnowledgeProvider({ children }: { children: ReactNode }) {
  const [bases, setBases] = useState<Manifest[]>([]);
  const [loading, setLoading] = useState(true);
  const [activeKbId, setActiveKbIdState] = useState<string | null>(() =>
    localStorage.getItem(STORAGE_KEY_ACTIVE_KB)
  );
  const graphRefreshRef = useRef<(() => Promise<void>) | null>(null);

  const setActiveKbId = useCallback((id: string | null) => {
    setActiveKbIdState(id);
    if (id) localStorage.setItem(STORAGE_KEY_ACTIVE_KB, id);
    else localStorage.removeItem(STORAGE_KEY_ACTIVE_KB);
  }, []);

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
