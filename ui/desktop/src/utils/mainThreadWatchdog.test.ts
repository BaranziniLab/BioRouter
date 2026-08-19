import { describe, it, expect, vi, afterEach } from 'vitest';
// CI installs npm dependencies without the Electron binary, so evaluating the real
// `electron` module throws "Electron failed to install correctly" at IMPORT time —
// the suite never loads and every test in it is silently skipped rather than
// failing. Anything reaching `electron` (here via `./logger`) must be mocked, and
// a local run cannot catch it: this machine has the binary.
vi.mock('electron', () => ({
  app: { getVersion: () => '0.0.0-test', getPath: () => '/tmp', isPackaged: false },
  ipcMain: { handle: () => {} },
  BrowserWindow: { getAllWindows: () => [] },
}));
vi.mock('./logger', () => ({
  default: { info: () => {}, warn: () => {}, error: () => {} },
}));

import {
  classifyTick,
  startMainThreadWatchdog,
  stopMainThreadWatchdog,
  STALL_THRESHOLD_MS,
  FREEZE_THRESHOLD_MS,
  TICK_MS,
} from './mainThreadWatchdog';

afterEach(() => {
  stopMainThreadWatchdog();
  vi.useRealTimers();
});

describe('classifyTick', () => {
  it('ignores a tick that arrived on time', () => {
    expect(classifyTick(0, 1_000)).toBeNull();
    expect(classifyTick(STALL_THRESHOLD_MS - 1, 1_000)).toBeNull();
  });

  it('calls a quarter-second stall jank', () => {
    const r = classifyTick(STALL_THRESHOLD_MS, 4_000);
    expect(r?.severity).toBe('jank');
    expect(r?.stalledMs).toBe(STALL_THRESHOLD_MS);
  });

  it('calls a whole second a freeze — that is where the wait cursor appears', () => {
    expect(classifyTick(FREEZE_THRESHOLD_MS, 4_000)?.severity).toBe('freeze');
    // The shape of #88: a 3.4s `biorouter doctor` on the main thread.
    expect(classifyTick(3_450, 4_000)?.severity).toBe('freeze');
  });

  it('reports when it happened, so a stall can be tied to a startup task', () => {
    expect(classifyTick(2_000, 4_012)?.sinceStartupMs).toBe(4_012);
  });
});

/**
 * The wall clock must be driven SEPARATELY from the timer queue here.
 *
 * Vitest's default fake timers also fake `Date`, advancing both in lockstep — so
 * an interval always appears to fire exactly on schedule and `lateBy` is always
 * zero. That is the opposite of what is being tested. Faking only the timer
 * functions, and moving `Date.now` by hand, is what reproduces a blocked loop:
 * the clock jumps while the queue does not drain.
 */
function withDecoupledClock(run: (clock: { advance: (ms: number) => void }) => void) {
  vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] });
  let now = 1_000_000;
  const spy = vi.spyOn(Date, 'now').mockImplementation(() => now);
  try {
    run({ advance: (ms) => (now += ms) });
  } finally {
    spy.mockRestore();
  }
}

describe('startMainThreadWatchdog', () => {
  it('reports a block that a synchronous call would have caused', () => {
    withDecoupledClock((clock) => {
      const seen: number[] = [];
      startMainThreadWatchdog((r) => seen.push(r.stalledMs));

      // A tick that arrives when it should: nothing to report.
      clock.advance(TICK_MS);
      vi.advanceTimersByTime(TICK_MS);
      expect(seen).toEqual([]);

      // Now the loop is blocked for 3 s — the shape of #88. The timer cannot run
      // during the block, so it lands 3 s late and that lateness IS the stall.
      clock.advance(TICK_MS + 3_000);
      vi.advanceTimersByTime(TICK_MS);
      expect(seen.length).toBe(1);
      expect(seen[0]).toBeGreaterThanOrEqual(FREEZE_THRESHOLD_MS);
    });
  });

  it('is idempotent — a second start does not double-report', () => {
    withDecoupledClock((clock) => {
      const seen: number[] = [];
      startMainThreadWatchdog((r) => seen.push(r.stalledMs));
      startMainThreadWatchdog((r) => seen.push(r.stalledMs));

      clock.advance(TICK_MS + 3_000);
      vi.advanceTimersByTime(TICK_MS);
      expect(seen.length).toBe(1);
    });
  });

  it('stops cleanly', () => {
    withDecoupledClock((clock) => {
      const seen: number[] = [];
      startMainThreadWatchdog((r) => seen.push(r.stalledMs));
      stopMainThreadWatchdog();

      clock.advance(10_000);
      vi.advanceTimersByTime(TICK_MS * 4);
      expect(seen).toEqual([]);
    });
  });
});
