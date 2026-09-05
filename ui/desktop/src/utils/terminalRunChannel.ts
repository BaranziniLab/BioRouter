/**
 * The chat transcript → terminal pane hand-off for "Run this code block".
 *
 * The problem this exists to solve is that the two ends have no shared state
 * and cannot get one cheaply. A pane's backend session id — the only handle
 * that can reach its pty — lives in `backendSessionIdRef` inside
 * `TerminalPaneView`, and it is deliberately private: it is assigned after an
 * async spawn, cleared on exit, and re-minted whenever the pane's cwd changes.
 * Lifting it into `TerminalDockContext` would put a value with that lifecycle
 * into shared state and, worse, route Run AROUND the pane — losing
 * `pendingInputRef`, the buffer that already absorbs the "dock is up, pty is
 * not yet" race for every keystroke.
 *
 * So the request travels as a COMMAND and the pane, which still owns its own
 * `writeToBackend`, does the writing. Same reasoning as `annotationChannel.ts`
 * next door: siblings with no shared state, and threading a callback down
 * would force a prop onto mount sites that must not have one.
 *
 * A plain module registry rather than that file's `window` CustomEvent: both
 * ends are in the same renderer bundle, an Electron window is its own realm, and
 * a direct call is deterministic to test. Nothing here crosses a window.
 *
 * ## Keyed by dock, never by focus
 *
 * `terminalFocus.ts` routes Cmd+T/Cmd+W to whichever dock holds focus, which is
 * right for a keystroke and WRONG here. Run is clicked in a specific chat, and
 * its command belongs to that chat's terminal even when another pane's terminal
 * is the one with the cursor in it. So delivery is keyed by the dock key —
 * `terminalKey ?? sessionId`, the same id `TerminalDockContext` is keyed by.
 */

/** How long a request may wait for a pane to appear before it is dropped. */
export const PENDING_RUN_TTL_MS = 20_000;

/** How many queued requests one dock key may hold. */
const MAX_PENDING_PER_KEY = 4;

export type TerminalRunHandler = (command: string) => void;

type PendingRun = { command: string; queuedAt: number };

const handlers = new Map<string, TerminalRunHandler[]>();
const pending = new Map<string, PendingRun[]>();

function fresh(entries: PendingRun[], now: number): PendingRun[] {
  return entries.filter((entry) => now - entry.queuedAt < PENDING_RUN_TTL_MS);
}

/**
 * Send `command` to the terminal for `dockKey`.
 *
 * When no pane is listening yet the request is QUEUED, because the ordinary
 * case is a chat with no terminal open at all: the click opens the dock, the
 * dock creates a pane, and the pane subscribes — three React commits after the
 * click. `pendingInputRef` in the pane covers the later "pane is up, pty is
 * not" gap; this covers the earlier one.
 *
 * Queued requests EXPIRE. A command that never found a pane is not a command
 * waiting to be delivered — it is one the user watched fail to run, and firing
 * it into a terminal they open ten minutes later would be an ambush.
 */
export function runInTerminal(dockKey: string, command: string): void {
  const listeners = handlers.get(dockKey);
  // Newest listener wins: when panes switch, the arriving pane registers before
  // the departing one's cleanup runs, and the arriving one is the visible pane.
  const listener = listeners?.[listeners.length - 1];
  if (listener) {
    listener(command);
    return;
  }
  const now = Date.now();
  const queue = fresh(pending.get(dockKey) ?? [], now);
  queue.push({ command, queuedAt: now });
  while (queue.length > MAX_PENDING_PER_KEY) queue.shift();
  pending.set(dockKey, queue);
}

/**
 * Subscribe the terminal for `dockKey`, draining anything already queued for it.
 *
 * Exactly one pane per dock should hold a subscription — the ACTIVE one, which
 * is the pane the user is looking at and the only one a command should land in.
 */
export function onTerminalRunRequest(
  dockKey: string | null | undefined,
  handler: TerminalRunHandler
): () => void {
  if (!dockKey) return () => {};
  const listeners = handlers.get(dockKey) ?? [];
  listeners.push(handler);
  handlers.set(dockKey, listeners);

  const queued = fresh(pending.get(dockKey) ?? [], Date.now());
  pending.delete(dockKey);
  for (const entry of queued) handler(entry.command);

  return () => {
    const current = handlers.get(dockKey);
    if (!current) return;
    const index = current.indexOf(handler);
    if (index !== -1) current.splice(index, 1);
    if (current.length === 0) handlers.delete(dockKey);
  };
}

/** Tests only — the singleton must not leak across cases. */
export function resetTerminalRunChannelForTests(): void {
  handlers.clear();
  pending.clear();
}
