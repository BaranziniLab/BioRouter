/**
 * startupSchedule.ts
 *
 * When each piece of background startup work is allowed to run, relative to the
 * first window being created.
 *
 * These live in one file because the bug they fix (#88) was a *scheduling*
 * bug as much as a blocking one: the auto-updater, the dependency check and the
 * extension update check were each given a delay in its own module, none of them
 * aware of the others, and all three landed inside the same few seconds while
 * the renderer was still painting. Written down together, an overlap is visible.
 *
 * The freeze itself came from those subsystems using `spawnSync` on the Electron
 * main thread; that is fixed at the call sites. This file is the second half:
 * keeping the (now async) work out of the window where the user is first
 * clicking around.
 */

/** Auto-updater setup. Its own first network check lands `STARTUP_UPDATE_CHECK_DELAY_MS` later. */
export const STARTUP_UPDATER_SETUP_DELAY_MS = 2_000;

/**
 * Dependency check. Spawns `biorouter doctor`, which measured ~3.5 s warm on an
 * M-series Mac — real work, just no longer blocking work.
 */
export const STARTUP_DEPENDENCY_CHECK_DELAY_MS = 6_000;

/**
 * Extension update check. Last, and by the widest margin: it makes a GitHub API
 * call per installed extension and may then run `uv sync`, which can take
 * minutes.
 */
export const STARTUP_EXTENSION_CHECK_DELAY_MS = 15_000;

/**
 * Every startup delay, in the order they fire. Exported so a test can assert the
 * ordering and the spacing without importing `main.ts` (which pulls in Electron).
 */
export const STARTUP_SCHEDULE: ReadonlyArray<{ name: string; delayMs: number }> = [
  { name: 'auto-updater setup', delayMs: STARTUP_UPDATER_SETUP_DELAY_MS },
  { name: 'dependency check', delayMs: STARTUP_DEPENDENCY_CHECK_DELAY_MS },
  { name: 'extension update check', delayMs: STARTUP_EXTENSION_CHECK_DELAY_MS },
];

/** Minimum gap between two consecutive startup tasks. */
export const MIN_STARTUP_TASK_GAP_MS = 3_000;
