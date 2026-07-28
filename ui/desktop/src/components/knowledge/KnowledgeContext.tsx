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

/**
 * What a selection change wants to happen to the primary pointer — the mirror
 * of the daemon's `PrimaryUpdate`. `unchanged` is what a set-only edit sends:
 * the daemon then re-establishes "the primary is a member of the set" itself
 * (promote to the first remaining base, or clear when none remain). Sending the
 * current primary back on a set-only edit would instead be *rejected* the
 * moment the user hides the primary, which is exactly when the repair matters.
 */
type PrimaryUpdate = { kind: 'unchanged' } | { kind: 'clear' } | { kind: 'set'; id: string };

interface KnowledgeContextType {
  bases: Manifest[];
  /** The session's knowledge bases — the one axis. Searchable, readable, usable. */
  visibleBases: Manifest[];
  loading: boolean;
  /** The KB-less write target and the Knowledge view's subject. Always a member of visibleBases, or null. */
  primaryKb: Manifest | null;
  primaryKbId: string | null;
  hiddenKbIds: string[];
  setPrimaryKbId: (id: string | null) => void;
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
  const [primaryKbId, setPrimaryKbIdState] = useState<string | null>(() =>
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
    (primary: PrimaryUpdate, nextHiddenKbIds: string[]) => {
      // Optimistic: show the caller's intent now, then adopt whatever the
      // daemon says it actually applied.
      if (primary.kind === 'set') setPrimaryKbIdState(primary.id);
      if (primary.kind === 'clear') setPrimaryKbIdState(null);
      if (primary.kind === 'unchanged') {
        // A set-only edit can orphan the primary — hiding the primary's own
        // base is precisely the case the daemon's repair exists for. Until that
        // repair comes back the renderer must not keep naming a base this chat
        // no longer includes: IngestPanel passes `primaryKbId` straight into
        // `/knowledge/bases/<id>/ingest`, so a stale pointer aims a digest at a
        // base the user just removed. Drop the subject and adopt the daemon's
        // answer below. Only the in-memory pointer is dropped — the persisted
        // one stays until an authoritative answer replaces it, so a reload
        // during the in-flight window still has a last-known value to show.
        setPrimaryKbIdState((current) =>
          current && nextHiddenKbIds.includes(current) ? null : current
        );
      }
      setHiddenKbIdsState(nextHiddenKbIds);
      if (primary.kind === 'set') localStorage.setItem(storageKey, primary.id);
      if (primary.kind === 'clear') localStorage.removeItem(storageKey);
      localStorage.setItem(hiddenStorageKey, JSON.stringify(nextHiddenKbIds));
      void setActive({
        body: {
          primary_kb: primary.kind === 'set' ? primary.id : undefined,
          clear_primary: primary.kind === 'clear',
          hidden_kbs: nextHiddenKbIds,
          session_id: sessionId || undefined,
        },
        throwOnError: false,
      })
        .then((res) => {
          // The daemon owns the "primary must be a member" repair: hiding the
          // primary promotes to the first remaining base, hiding everything
          // clears it. Adopt its answer instead of re-implementing that rule
          // here, where the two would silently drift apart. A rejected or
          // failed request leaves the local state alone — treating "no data"
          // as "no primary" would let one network hiccup erase the pointer.
          const data = res?.data;
          if (!data) {
            console.warn('setActive (server sync) returned no selection:', res?.error);
            return;
          }
          // `active_kb` is the deprecated mirror. Reading only `primary_kb`
          // would turn a *successful* write against an older daemon into "there
          // is no primary" — and the design forbids inventing one back, so the
          // pointer would stay lost. Same fallback as the hydrate path below.
          const applied = data.primary_kb ?? data.active_kb ?? null;
          setPrimaryKbIdState(applied);
          if (applied) localStorage.setItem(storageKey, applied);
          else localStorage.removeItem(storageKey);
          // Adopt the set too, not just the pointer: the repair moves both, and
          // taking one half of it is how the renderer ends up holding a primary
          // that is not a member of its own visible set. Guarded on the field
          // being present so a daemon that predates `hidden_kbs` in the reply
          // leaves the optimistic set standing instead of erasing it.
          if (Array.isArray(data.hidden_kbs)) {
            const appliedHidden = data.hidden_kbs.filter(
              (id): id is string => typeof id === 'string'
            );
            setHiddenKbIdsState(appliedHidden);
            localStorage.setItem(hiddenStorageKey, JSON.stringify(appliedHidden));
          }
        })
        .catch((err) => {
          console.warn('setActive (server sync) failed:', err);
        });
    },
    [hiddenStorageKey, sessionId, storageKey]
  );

  const setPrimaryKbId = useCallback(
    (id: string | null) => {
      // The primary must be a member of the set, so making a base primary adds
      // it to this chat in the same request — one gesture, one POST.
      const nextHidden = id ? hiddenKbIds.filter((hiddenId) => hiddenId !== id) : hiddenKbIds;
      syncSelection(id ? { kind: 'set', id } : { kind: 'clear' }, nextHidden);
    },
    [hiddenKbIds, syncSelection]
  );

  const setHiddenKbIds = useCallback(
    (ids: string[]) => {
      const nextIds = Array.from(new Set(ids)).sort();
      // A set-only edit never states a primary — see PrimaryUpdate above.
      syncSelection({ kind: 'unchanged' }, nextIds);
    },
    [syncSelection]
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
    // A primary that names a base which no longer exists is cleared, not
    // promoted — deleting a base is destructive, so re-pointing the write
    // target at an unrelated one is the wrong default (D2).
    if (primaryKbId && bases.length > 0 && !bases.some((b) => b.id === primaryKbId)) {
      setPrimaryKbId(null);
    }
  }, [primaryKbId, bases, setPrimaryKbId]);

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
    setPrimaryKbIdState(local);
    setHiddenKbIdsState(localHidden);
    let cancelled = false;

    void (async () => {
      try {
        const res = await getActive({
          query: sessionId ? { session_id: sessionId } : undefined,
          throwOnError: true,
        });
        if (cancelled) return;
        // `active_kb` is the deprecated mirror, read so a fresh renderer keeps
        // working against a daemon that predates `primary_kb`.
        const server = res.data?.primary_kb ?? res.data?.active_kb ?? null;
        const serverHidden = (res.data?.hidden_kbs ?? []).filter(
          (id): id is string => typeof id === 'string'
        );
        setPrimaryKbIdState(server);
        setHiddenKbIdsState(serverHidden);
        if (server) localStorage.setItem(storageKey, server);
        else localStorage.removeItem(storageKey);
        localStorage.setItem(hiddenStorageKey, JSON.stringify(serverHidden));
      } catch (err) {
        if (cancelled) return;
        console.warn('getActive (server hydrate) failed:', err);
        setPrimaryKbIdState(local);
        setHiddenKbIdsState(localHidden);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [hiddenStorageKey, sessionId, storageKey]);

  const primaryKb = useMemo(
    () => bases.find((b) => b.id === primaryKbId) ?? null,
    [bases, primaryKbId]
  );
  const visibleBases = useMemo(
    () => bases.filter((base) => !hiddenKbIds.includes(base.id)),
    [bases, hiddenKbIds]
  );

  const value: KnowledgeContextType = {
    bases,
    visibleBases,
    loading,
    primaryKb,
    primaryKbId,
    hiddenKbIds,
    setPrimaryKbId,
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
