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

/** The shape both selection endpoints answer with — GET /active and POST /active. */
type SelectionPayload =
  | { primary_kb?: string | null; active_kb?: string | null; hidden_kbs?: string[] | null }
  | undefined;

/** `active_kb` is the deprecated mirror, read so a fresh renderer keeps working
 * against a daemon that predates `primary_kb`. */
function readPrimary(data: SelectionPayload): string | null {
  return data?.primary_kb ?? data?.active_kb ?? null;
}

/** `null` means "this answer did not state a set" (a daemon that predates the
 * field) — distinct from an empty set, and the caller must leave what it has
 * rather than erase the session's whole working set. */
function readHidden(data: SelectionPayload): string[] | null {
  return Array.isArray(data?.hidden_kbs)
    ? data.hidden_kbs.filter((id): id is string => typeof id === 'string')
    : null;
}

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
  // Has a base list ever arrived? Until it has, `bases` being empty says nothing
  // about which bases exist, so nothing may be pruned against it.
  const [basesLoaded, setBasesLoaded] = useState(false);
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
  // Every selection round-trip — a write, its recovery read, a hydrate — takes a
  // generation. Only the newest may write state back, so a slow answer cannot
  // reinstate a selection the user has already clicked past.
  const selectionGenerationRef = useRef(0);

  const applyPrimary = useCallback(
    (primary: string | null) => {
      setPrimaryKbIdState(primary);
      if (primary) localStorage.setItem(storageKey, primary);
      else localStorage.removeItem(storageKey);
    },
    [storageKey]
  );

  const applyHidden = useCallback(
    (hidden: string[]) => {
      setHiddenKbIdsState(hidden);
      localStorage.setItem(hiddenStorageKey, JSON.stringify(hidden));
    },
    [hiddenStorageKey]
  );

  /** Re-read the authoritative selection after a write that did not land. */
  const rehydrateSelection = useCallback(
    async (generation: number) => {
      try {
        const res = await getActive({
          query: sessionId ? { session_id: sessionId } : undefined,
          throwOnError: true,
        });
        if (generation !== selectionGenerationRef.current) return;
        applyPrimary(readPrimary(res.data));
        const hidden = readHidden(res.data);
        if (hidden) applyHidden(hidden);
      } catch (err) {
        // Both the write and the recovery read failed. Keep what is on screen —
        // there is nothing better to show, and the next hydrate will settle it.
        console.warn('getActive (recovering a failed selection write) failed:', err);
      }
    },
    [applyHidden, applyPrimary, sessionId]
  );

  const syncSelection = useCallback(
    (primary: PrimaryUpdate, nextHiddenKbIds: string[]) => {
      const generation = ++selectionGenerationRef.current;
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
          // A newer edit is already in flight (or has already answered): this
          // answer describes a selection the user has clicked past, so applying
          // it would silently undo their newer choice.
          if (generation !== selectionGenerationRef.current) return;
          // The daemon owns the "primary must be a member" repair: hiding the
          // primary promotes to the first remaining base, hiding everything
          // clears it. Adopt its answer instead of re-implementing that rule
          // here, where the two would silently drift apart.
          const data = res?.data;
          if (!data) {
            // The write did not land. Keeping the optimistic value would leave
            // the chip, the Knowledge view and the ingest target describing a
            // selection the daemon never applied, with nothing to correct it —
            // so re-read the truth instead of guessing at it.
            console.warn('setActive (server sync) returned no selection:', res?.error);
            void rehydrateSelection(generation);
            return;
          }
          applyPrimary(readPrimary(data));
          // Adopt the set too, not just the pointer: the repair moves both, and
          // taking one half of it is how the renderer ends up holding a primary
          // that is not a member of its own visible set.
          const appliedHidden = readHidden(data);
          if (appliedHidden) applyHidden(appliedHidden);
        })
        .catch((err) => {
          if (generation !== selectionGenerationRef.current) return;
          console.warn('setActive (server sync) failed:', err);
          void rehydrateSelection(generation);
        });
    },
    [applyHidden, applyPrimary, hiddenStorageKey, rehydrateSelection, sessionId, storageKey]
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
      setBasesLoaded(true);
    } catch (err) {
      // Keep the list we already had. A failed request is not a list of zero
      // bases, and emptying it here is what let the prune below read "every
      // stored id names a base that no longer exists".
      console.error('listBases failed:', err);
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
    // Drop ids naming bases that no longer exist — but only once a list has
    // actually arrived. Pruning against the empty list this starts out with
    // would persist an empty set on every mount, erasing the session's working
    // set before the daemon had said a word about which bases exist.
    if (!basesLoaded) return;
    const validIds = new Set(bases.map((base) => base.id));
    const nextHiddenKbIds = hiddenKbIds.filter((id) => validIds.has(id));
    if (nextHiddenKbIds.length !== hiddenKbIds.length) {
      setHiddenKbIds(nextHiddenKbIds);
    }
  }, [basesLoaded, bases, hiddenKbIds, setHiddenKbIds]);

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
    // The hydrate takes a generation of its own: switching sessions must not
    // leave a write from the previous one able to answer into the new one.
    const generation = ++selectionGenerationRef.current;

    void (async () => {
      try {
        const res = await getActive({
          query: sessionId ? { session_id: sessionId } : undefined,
          throwOnError: true,
        });
        if (cancelled || generation !== selectionGenerationRef.current) return;
        applyPrimary(readPrimary(res.data));
        applyHidden(readHidden(res.data) ?? []);
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
  }, [applyHidden, applyPrimary, hiddenStorageKey, sessionId, storageKey]);

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
