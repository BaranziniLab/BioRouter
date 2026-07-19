import { useSyncExternalStore } from 'react';

/**
 * ONE 1 Hz interval process-wide, shared by every elapsed-time display.
 * The interval only exists while at least one component is subscribed, so an
 * idle transcript costs nothing. Follows the same useSyncExternalStore shape as
 * chatStreamStore so React batches ticks with stream updates.
 */
const listeners = new Set<() => void>();
let timer: number | null = null;
let now = Date.now();

const getNow = () => now;

const subscribe = (listener: () => void): (() => void) => {
  listeners.add(listener);
  if (timer === null) {
    now = Date.now();
    timer = window.setInterval(() => {
      now = Date.now();
      for (const l of listeners) l();
    }, 1000);
  }
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0 && timer !== null) {
      window.clearInterval(timer);
      timer = null;
    }
  };
};

/**
 * Milliseconds since `since`, re-rendering once a second. `undefined` disables
 * the readout entirely — that is how a caller says "I have no trustworthy clock
 * origin", which must render no number rather than a fabricated one.
 *
 * The tick is a wall-clock read, not an accumulating counter, so a throttled
 * background Electron window (whose timers clamp when occluded) still shows the
 * correct elapsed time the moment it comes back to the foreground.
 */
export function useElapsedMs(since: number | undefined): number | null {
  const tick = useSyncExternalStore(subscribe, getNow, getNow);
  if (since === undefined) return null;
  return Math.max(0, tick - since);
}
