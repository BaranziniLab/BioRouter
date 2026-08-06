/**
 * Where a window learns the `turn_id` of a turn it did not start.
 *
 * The live-turn stream contract makes ATTACH a re-POST of the same `turn_id`
 * (`.stream-contract.md` §3.2). That only helps if a window that is not the one
 * which minted the id can find it, and there are exactly two moments where a
 * client needs to:
 *
 *   1. **Reload / crash.** The window that started the turn comes back with an
 *      empty heap. Its own `ChatStreamController` is new; the id it minted is
 *      gone with the old renderer.
 *   2. **Tab handoff.** A tab moves to another BrowserWindow. The receiving
 *      window has never seen the id.
 *
 * `localStorage` covers both: BrowserWindows of the same origin share it, and
 * it survives a reload. It is deliberately the WEAKER half of the answer — the
 * durable one is the server naming the in-flight turn on `/agent/resume` (see
 * the report note on `active_turn`), which needs no client bookkeeping and
 * cannot go stale. Until that exists this is what makes attach reachable.
 *
 * Staleness is the failure mode to design against: a window that dies without
 * clearing its entry leaves a pointer to a turn that has since finished. Two
 * defences, and neither is sufficient alone:
 *   - a TTL here, so an ancient pointer is never followed at all; and
 *   - the server answering a re-POST of a COMPLETED turn with a clean terminal
 *     frame rather than a replay of its backlog. Without that second half, an
 *     attach after a crash would re-render a turn the session load already
 *     painted — the duplicated-paragraph bug, arriving by the back door.
 */

const KEY_PREFIX = 'biorouter.activeTurn.';

/**
 * How long a remembered turn id is worth following. A real turn can run for
 * many minutes (long tool chains, big compactions), so this must comfortably
 * exceed one; an hour is far past any turn and still short enough that a
 * pointer left by a crash weeks ago is never resurrected.
 */
export const ACTIVE_TURN_TTL_MS = 60 * 60 * 1000;

interface StoredActiveTurn {
  turnId: string;
  startedAt: number;
}

function storage(): Storage | null {
  try {
    // Guarded: not every renderer context (workers, some test envs) has it, and
    // Safari-style "storage disabled" throws on ACCESS, not on use.
    return typeof localStorage === 'undefined' ? null : localStorage;
  } catch {
    return null;
  }
}

/** Record that `turnId` is the turn now in flight for `sessionId`. */
export function rememberActiveTurn(sessionId: string, turnId: string): void {
  const store = storage();
  if (!store || !sessionId || !turnId) return;
  const entry: StoredActiveTurn = { turnId, startedAt: Date.now() };
  try {
    store.setItem(KEY_PREFIX + sessionId, JSON.stringify(entry));
  } catch {
    // A full or disabled store costs us the reload-resume, not correctness.
  }
}

/**
 * Drop the pointer for `sessionId`. `turnId`, when given, makes this a
 * compare-and-clear: a turn that ended must not erase the pointer of the turn
 * that replaced it (which is exactly what a late `Finish` from an abandoned
 * stream would otherwise do).
 */
export function forgetActiveTurn(sessionId: string, turnId?: string | null): void {
  const store = storage();
  if (!store || !sessionId) return;
  try {
    if (turnId) {
      const current = readActiveTurn(sessionId);
      if (current && current !== turnId) return;
    }
    store.removeItem(KEY_PREFIX + sessionId);
  } catch {
    // ignore
  }
}

/**
 * The turn id last recorded for `sessionId`, or undefined when there is none or
 * it is older than {@link ACTIVE_TURN_TTL_MS}. Expired entries are removed on
 * read so a dead pointer is followed at most zero times.
 */
export function readActiveTurn(sessionId: string): string | undefined {
  const store = storage();
  if (!store || !sessionId) return undefined;
  try {
    const raw = store.getItem(KEY_PREFIX + sessionId);
    if (!raw) return undefined;
    const parsed = JSON.parse(raw) as Partial<StoredActiveTurn>;
    if (typeof parsed?.turnId !== 'string' || typeof parsed?.startedAt !== 'number') {
      store.removeItem(KEY_PREFIX + sessionId);
      return undefined;
    }
    if (Date.now() - parsed.startedAt > ACTIVE_TURN_TTL_MS) {
      store.removeItem(KEY_PREFIX + sessionId);
      return undefined;
    }
    return parsed.turnId;
  } catch {
    return undefined;
  }
}
