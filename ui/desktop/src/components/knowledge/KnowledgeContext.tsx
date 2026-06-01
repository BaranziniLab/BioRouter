import { createContext, ReactNode, useCallback, useContext, useEffect, useMemo, useState } from 'react';
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
}

const KnowledgeContext = createContext<KnowledgeContextType | null>(null);

export function KnowledgeProvider({ children }: { children: ReactNode }) {
  const [bases, setBases] = useState<Manifest[]>([]);
  const [loading, setLoading] = useState(true);
  const [activeKbId, setActiveKbIdState] = useState<string | null>(() =>
    localStorage.getItem(STORAGE_KEY_ACTIVE_KB)
  );

  // TODO Plan 6: Also sync to server-side active-KB state via a new
  // POST /knowledge/active endpoint (currently kb_set_active is MCP-only).
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

  useEffect(() => { void refresh(); }, [refresh]);

  // If activeKbId points to a base that no longer exists, clear it.
  useEffect(() => {
    if (activeKbId && bases.length > 0 && !bases.some((b) => b.id === activeKbId)) {
      setActiveKbId(null);
    }
  }, [activeKbId, bases, setActiveKbId]);

  const activeKb = useMemo(
    () => bases.find((b) => b.id === activeKbId) ?? null,
    [bases, activeKbId]
  );

  const value: KnowledgeContextType = { bases, loading, activeKb, activeKbId, setActiveKbId, refresh };
  return <KnowledgeContext.Provider value={value}>{children}</KnowledgeContext.Provider>;
}

export function useKnowledge(): KnowledgeContextType {
  const ctx = useContext(KnowledgeContext);
  if (!ctx) throw new Error('useKnowledge must be used inside <KnowledgeProvider>');
  return ctx;
}
