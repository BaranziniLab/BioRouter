/**
 * mainThreadWatchdog.ts
 *
 * Notices when the Electron main process stops servicing its event loop.
 *
 * Issue #88 shipped and survived because nothing reported it. A `spawnSync` on
 * the main thread froze the window for seconds at every launch, and the only
 * evidence was a user saying the app felt stuck — no error, no log line, no test
 * failure. The main thread runs window compositing, IPC and input, so a stall
 * here is a frozen UI one-for-one, and it is worth one cheap timer to see.
 *
 * How it works: a timer that should fire every `TICK_MS` records how late it
 * actually was. Lateness beyond the threshold means the loop was blocked, since
 * a timer cannot fire while a synchronous call holds the thread. It cannot
 * measure the stall from inside the stall — during a block the callback does not
 * run at all — so lateness on the NEXT tick is the measurement.
 */
import log from './logger';

/** How often the probe runs. Cheap: an empty callback twice a second. */
export const TICK_MS = 500;

/**
 * Report stalls at or above this. A frame is 16 ms and a little jitter under GC
 * or heavy paint is normal; a quarter second is where a person notices, and a
 * whole second is where macOS shows the wait cursor.
 */
export const STALL_THRESHOLD_MS = 250;

/** Above this it is not jank, it is a freeze. */
export const FREEZE_THRESHOLD_MS = 1_000;

export interface StallReport {
  stalledMs: number;
  severity: 'jank' | 'freeze';
  sinceStartupMs: number;
}

/** Pure: given the timer's lateness, what (if anything) to report. */
export function classifyTick(
  lateByMs: number,
  sinceStartupMs: number,
  threshold = STALL_THRESHOLD_MS
): StallReport | null {
  if (lateByMs < threshold) return null;
  return {
    stalledMs: Math.round(lateByMs),
    severity: lateByMs >= FREEZE_THRESHOLD_MS ? 'freeze' : 'jank',
    sinceStartupMs: Math.round(sinceStartupMs),
  };
}

let timer: NodeJS.Timeout | null = null;

/**
 * Start watching. Idempotent.
 *
 * `onStall` exists so a test can observe reports without asserting on log calls.
 */
export function startMainThreadWatchdog(onStall?: (report: StallReport) => void): () => void {
  if (timer) return stopMainThreadWatchdog;

  const startedAt = Date.now();
  let expected = Date.now() + TICK_MS;

  timer = setInterval(() => {
    const now = Date.now();
    const lateBy = now - expected;
    expected = now + TICK_MS;

    const report = classifyTick(lateBy, now - startedAt);
    if (!report) return;

    if (report.severity === 'freeze') {
      log.warn(
        `[MainThreadWatchdog] main process blocked for ${report.stalledMs}ms ` +
          `(${(report.sinceStartupMs / 1000).toFixed(1)}s after start). The window was frozen ` +
          'for that whole time. Something on the main thread ran synchronously — look for a ' +
          'spawnSync/execSync, a large readFileSync, or a synchronous unzip/hash.'
      );
    } else {
      log.info(
        `[MainThreadWatchdog] main process stalled ${report.stalledMs}ms ` +
          `(${(report.sinceStartupMs / 1000).toFixed(1)}s after start)`
      );
    }
    onStall?.(report);
  }, TICK_MS);

  // Never hold the process open on this probe alone.
  timer.unref?.();
  return stopMainThreadWatchdog;
}

export function stopMainThreadWatchdog(): void {
  if (timer) {
    clearInterval(timer);
    timer = null;
  }
}
