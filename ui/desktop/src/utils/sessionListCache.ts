import { listSessions, type Session } from '../api';
import { subscribeSessionNameChanges } from './sessionNameSync';

let cachedSessions: Session[] | null = null;
let inFlightRequest: Promise<Session[]> | null = null;
const listeners = new Set<() => void>();

function emitChange(): void {
  for (const listener of listeners) listener();
}

// A session rename rides the name channel, not the list channel — but the
// See-all view and Home recents read THIS cache, so patch the cached name when
// one arrives. Without this a rename made in the tab pill or the sidebar never
// reached those two surfaces until they remounted, and the same session showed
// two different names in two panels at once.
subscribeSessionNameChanges(({ sessionId, name, userSetName }) => {
  if (!cachedSessions) return;
  const idx = cachedSessions.findIndex((s) => s.id === sessionId);
  if (idx === -1) return;
  const next = cachedSessions.slice();
  next[idx] = { ...next[idx], name, user_set_name: userSetName };
  cachedSessions = next;
  emitChange();
});

// ── Cross-window "the set of sessions changed" signal ──────────────────────
// A sibling to sessionNameSync's name channel, for list MEMBERSHIP: a session
// created, diverged, deleted or imported. Every list surface — sidebar Recents,
// the See-all view, Home recents — subscribes and re-reads, so a branch created
// in one window appears in ALL of them without a manual refresh. (A rename is
// NOT membership; it uses the name channel above.)
type ListListener = () => void;
const listChangeListeners = new Set<ListListener>();
let listChannel: BroadcastChannel | null = null;

function getListChannel(): BroadcastChannel | null {
  if (listChannel) return listChannel;
  if (typeof BroadcastChannel === 'undefined') return null;
  listChannel = new BroadcastChannel('biorouter:session-list');
  listChannel.onmessage = () => {
    // Another window changed the set. Refresh this window's own cache (so its
    // See-all/Home update) and fan out to hooks that fetch their own way.
    void refreshSessionList().catch(() => undefined);
    for (const l of listChangeListeners) l();
  };
  return listChannel;
}

/**
 * Subscribe to list-membership changes (this window and every sibling window).
 * For surfaces that keep their own list (the sidebar Recents uses a different
 * endpoint than this cache) and just need a "re-read now" nudge.
 */
export function subscribeSessionListChanges(listener: ListListener): () => void {
  getListChannel();
  listChangeListeners.add(listener);
  return () => {
    listChangeListeners.delete(listener);
  };
}

/**
 * Announce that the set of sessions changed — call after create, diverge,
 * delete or import. Refreshes this window's cache immediately and notifies every
 * other window to do the same.
 */
export function notifySessionListChanged(): void {
  void refreshSessionList().catch(() => undefined);
  for (const l of listChangeListeners) l();
  getListChannel()?.postMessage({ at: Date.now() });
}

export function getCachedSessionList(): Session[] | null {
  return cachedSessions;
}

export function updateCachedSessionList(
  update: Session[] | ((sessions: Session[]) => Session[])
): void {
  const current = cachedSessions ?? [];
  cachedSessions = typeof update === 'function' ? update(current) : update;
  emitChange();
}

export function subscribeSessionList(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export async function refreshSessionList(): Promise<Session[]> {
  if (inFlightRequest) return inFlightRequest;

  inFlightRequest = listSessions<true>({ throwOnError: true })
    .then((response) => {
      cachedSessions = response.data.sessions;
      emitChange();
      return cachedSessions;
    })
    .finally(() => {
      inFlightRequest = null;
    });

  return inFlightRequest;
}

export function preloadSessionList(): void {
  if (cachedSessions !== null) return;
  void refreshSessionList().catch(() => undefined);
}

export function clearSessionListCache(): void {
  cachedSessions = null;
  inFlightRequest = null;
  emitChange();
}
