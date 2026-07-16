import { listSessions, type Session } from '../api';

let cachedSessions: Session[] | null = null;
let inFlightRequest: Promise<Session[]> | null = null;
const listeners = new Set<() => void>();

function emitChange(): void {
  for (const listener of listeners) listener();
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
