// BR-54 Slice A — one `biorouterd` daemon per app (not per Electron window).
//
// `startBiorouterd` used to run once per `createChat`, so N windows spawned N
// whole daemons: N tokio runtimes, N AgentManagers, N SQLite pools, and N copies
// of every MCP child process. But the daemon is *already* a session-keyed
// singleton (the AgentManager is a process-global OnceCell, the server routes by
// session_id, and the secret key is a stable module constant), so every window
// can safely share ONE daemon and address its own sessions on it. The per-window
// working directory is not lost — it still flows to each session via
// `REQUEST_DIR` / `BIOROUTER_WORKING_DIR` in the window's `appConfig`, and the
// daemon's own spawn cwd is only a fallback the GUI never relies on.
//
// This module keeps the singleton isolated from `main.ts` (which pulls in all of
// Electron) so it can be unit-tested. `start` is injected rather than imported at
// runtime for the same reason — the test never loads `./biorouterd`.
//
// Kill switch: set `BIOROUTER_SHARED_DAEMON=0` (or `false`/`off`/`no`) to revert
// to the previous per-window daemon behavior.
import type { BiorouterdResult, StartBiorouterdOptions } from './biorouterd';

let sharedBackend: Promise<BiorouterdResult> | null = null;

/**
 * Whether windows should share one daemon (the default). Off only when the
 * `BIOROUTER_SHARED_DAEMON` env var is explicitly falsy, so the change is
 * trivially revertible without a rebuild.
 */
export function isSharedDaemonEnabled(
  env: Record<string, string | undefined> = process.env
): boolean {
  const flag = env.BIOROUTER_SHARED_DAEMON;
  if (flag === undefined) return true;
  const v = flag.trim().toLowerCase();
  return v !== '0' && v !== 'false' && v !== 'off' && v !== 'no';
}

/**
 * Return the process-wide `biorouterd` backend, starting it exactly once. All
 * concurrent callers await the same in-flight promise, so racing `createChat`
 * calls (e.g. restoring several windows on launch) share ONE spawn rather than
 * racing several daemons onto several ports.
 *
 * A rejected start is not cached, so a later window can retry (e.g. after the
 * user disables an unreachable external backend).
 */
export function getSharedBackend(
  start: (options: StartBiorouterdOptions) => Promise<BiorouterdResult>,
  options: StartBiorouterdOptions
): Promise<BiorouterdResult> {
  if (!sharedBackend) {
    const pending = start(options).catch((err) => {
      // Only clear if we're still the current attempt — never clobber a newer one.
      if (sharedBackend === pending) sharedBackend = null;
      throw err;
    });
    sharedBackend = pending;
  }
  return sharedBackend;
}

/**
 * Forget the cached backend so the next `getSharedBackend` starts a fresh one.
 * Used when settings that change the daemon (e.g. external-backend config)
 * change and a window retries.
 */
export function resetSharedBackend(): void {
  sharedBackend = null;
}
