/**
 * The in-app terminal's session registry — who owns which shell, and when a
 * shell is released.
 *
 * This lives outside main.ts because both of its rules were bugs first:
 *
 * 1. **A slot must be released on every teardown path, not just the happy one.**
 *    The registry used to be freed by exactly two events: an explicit
 *    `terminal:dispose` from the renderer, and `webContents` `destroyed`. A
 *    renderer *reload* is neither. Cmd+R (main.ts wires it directly), View >
 *    Reload, and the `reload-app` IPC all replace the document without
 *    destroying the webContents — React never runs its effect cleanups, so the
 *    renderer's `terminal:dispose` calls never fire, and every shell that window
 *    had open stayed in this map forever holding a slot no UI could reach. The
 *    user-visible symptom was the cap below firing "when nowhere near 8
 *    terminals are open". `releaseOwner` is the fix, and main.ts calls it from
 *    the document-replacing navigation and crash paths too.
 *
 * 2. **The cap is a safety rail, not a product limit.** It exists so a runaway
 *    loop cannot fork-bomb a window, and it is deliberately far above what any
 *    human opens by hand. It is configurable, and the refusal message says both
 *    numbers so the reader knows what to change.
 *
 * Nothing here imports Electron: an owner is just a numeric `webContents.id` and
 * a session is just its disposer. That is what makes the rules testable.
 */

/**
 * How many shells one window may run at once.
 *
 * Generous on purpose — this is a fork-bomb rail, not a UX limit. Override with
 * `BIOROUTER_MAX_TERMINAL_SESSIONS`; a non-positive or unparseable value falls
 * back to the default rather than disabling the rail.
 */
export const DEFAULT_MAX_TERMINAL_SESSIONS_PER_OWNER = 64;

export function maxTerminalSessionsPerOwner(
  env: Record<string, string | undefined> = process.env
): number {
  const raw = env.BIOROUTER_MAX_TERMINAL_SESSIONS;
  if (typeof raw !== 'string') return DEFAULT_MAX_TERMINAL_SESSIONS_PER_OWNER;
  const parsed = Number.parseInt(raw.trim(), 10);
  if (!Number.isFinite(parsed) || parsed <= 0) return DEFAULT_MAX_TERMINAL_SESSIONS_PER_OWNER;
  return parsed;
}

export interface RegisteredTerminalSession {
  /** `webContents.id` of the renderer that created this session. */
  ownerId: number;
  /** Tear the shell down. Called at most once per session by the registry. */
  dispose: () => void;
  /** Drop the `destroyed` listener this session added to its owner. */
  removeOwnerDestroyedListener: () => void;
}

/**
 * The live shells, keyed by session id.
 *
 * Exported as a class (rather than module-level state) so a test gets a fresh
 * one per case and cannot leak into the next — the very failure mode this
 * module exists to prevent.
 */
export class TerminalSessionRegistry<S extends RegisteredTerminalSession> {
  private readonly sessions = new Map<string, S>();

  constructor(private readonly onDisposeError?: (error: unknown) => void) {}

  get size(): number {
    return this.sessions.size;
  }

  add(sessionId: string, session: S): void {
    this.sessions.set(sessionId, session);
  }

  get(sessionId: string): S | undefined {
    return this.sessions.get(sessionId);
  }

  /** The session `sessionId`, but only if `ownerId` is the renderer that made it. */
  getOwned(sessionId: string, ownerId: number): S | undefined {
    const session = this.sessions.get(sessionId);
    if (!session || session.ownerId !== ownerId) return undefined;
    return session;
  }

  countForOwner(ownerId: number): number {
    let count = 0;
    for (const session of this.sessions.values()) {
      if (session.ownerId === ownerId) count += 1;
    }
    return count;
  }

  /** Forget a session without disposing it — the shell already exited on its own. */
  forget(sessionId: string): S | undefined {
    const session = this.sessions.get(sessionId);
    if (session) this.sessions.delete(sessionId);
    return session;
  }

  /** Tear one session down. Returns false when it was already gone. */
  release(sessionId: string): boolean {
    const session = this.sessions.get(sessionId);
    if (!session) return false;
    this.sessions.delete(sessionId);
    session.removeOwnerDestroyedListener();
    try {
      session.dispose();
    } catch (error) {
      this.onDisposeError?.(error);
    }
    return true;
  }

  /**
   * Tear down EVERY session a renderer owns, and report how many there were.
   *
   * This is the reload/navigate/crash path. It must be safe to call when the
   * owner holds nothing (the common case) and must never touch another
   * window's shells.
   */
  releaseOwner(ownerId: number): number {
    const doomed: string[] = [];
    for (const [sessionId, session] of this.sessions) {
      if (session.ownerId === ownerId) doomed.push(sessionId);
    }
    for (const sessionId of doomed) this.release(sessionId);
    return doomed.length;
  }
}

/**
 * The refusal a renderer sees when a window is already at the rail.
 *
 * Names the limit and how to raise it: the old message said only "at most 8",
 * which told the reader neither number nor knob.
 */
export function terminalSessionLimitMessage(limit: number): string {
  return (
    `This window is already running ${limit} terminal sessions, the safety limit. ` +
    `Close one, or raise the limit with the BIOROUTER_MAX_TERMINAL_SESSIONS environment variable.`
  );
}
