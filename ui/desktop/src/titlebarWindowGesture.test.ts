import { describe, it, expect, vi } from 'vitest';
import {
  doubleClickWindowAction,
  unzoomedDragOrigin,
  WindowMoveDragController,
  type MovableWindow,
  type WindowMoveTimer,
} from './titlebarWindowGesture';

/**
 * WHAT THIS FILE CAN AND CANNOT PROVE. Everything here is arithmetic and
 * bookkeeping in the main process, which is why it is worth a unit test at all:
 * the part that decides whether the gesture WORKS — that the empty band is
 * hit-testable at every tab count — is a property of Electron's app-region fold
 * and belongs to `scripts/titlebar-appregion-check.mjs` against a running app.
 * Nothing in jsdom or Node can fail on that.
 *
 * What these tests do cover is the half that can silently misbehave in the
 * running app in ways nobody would think to click for: a window that jitters
 * under a plain click, a zoomed window dragged off its own right edge, and a
 * timer that outlives the press that started it.
 */

/** A fake `BrowserWindow` that records what was asked of it. */
function fakeWindow(overrides: Partial<MovableWindow> & { bounds?: Rect } = {}) {
  const state = {
    bounds: overrides.bounds ?? { x: 100, y: 200, width: 800, height: 600 },
    alive: true,
    fullScreen: false,
    maximized: false,
    positions: [] as Array<[number, number]>,
    unmaximizeCalls: 0,
  };
  const win: MovableWindow = {
    id: 7,
    isAlive: () => state.alive,
    isFullScreen: () => state.fullScreen,
    isMaximized: () => state.maximized,
    unmaximize: () => {
      state.unmaximizeCalls += 1;
      state.maximized = false;
      state.bounds = { x: 0, y: 0, width: 400, height: 300 };
    },
    getBounds: () => state.bounds,
    setPosition: (x, y) => state.positions.push([x, y]),
    ...overrides,
  };
  return { win, state };
}

type Rect = { x: number; y: number; width: number; height: number };

/** A controller wired to a hand-cranked clock, cursor and timer. */
function harness(win: MovableWindow) {
  let cursor = { x: 0, y: 0 };
  let now = 1000;
  let ticks: Array<() => void> = [];
  const cancelled: WindowMoveTimer[] = [];

  const controller = new WindowMoveDragController({
    cursorPoint: () => cursor,
    schedule: (fn) => {
      ticks.push(fn);
      return ticks.length;
    },
    cancelScheduled: (timer) => {
      cancelled.push(timer);
      ticks = [];
    },
    now: () => now,
    thresholdPx: 3,
    maxDurationMs: 30_000,
  });

  return {
    controller,
    begin: () => controller.begin(win),
    moveCursorTo: (x: number, y: number) => {
      cursor = { x, y };
    },
    advance: (ms: number) => {
      now += ms;
    },
    tick: () => ticks.forEach((fn) => fn()),
    cancelledCount: () => cancelled.length,
    scheduledCount: () => ticks.length,
  };
}

describe('doubleClickWindowAction — the macOS titlebar preference', () => {
  it('honours "Double-click a window\'s title bar to" on macOS', () => {
    expect(doubleClickWindowAction('darwin', 'Maximize')).toBe('zoom');
    expect(doubleClickWindowAction('darwin', 'Minimize')).toBe('minimize');
    expect(doubleClickWindowAction('darwin', 'None')).toBe('none');
  });

  it('treats an unset preference as Zoom, which is what macOS ships', () => {
    // `getUserDefault` answers '' for a key that was never written, and the
    // shipping default of that setting is Zoom. Reading '' as "do nothing"
    // would make the gesture dead on a machine nobody had configured — which is
    // most of them.
    expect(doubleClickWindowAction('darwin', '')).toBe('zoom');
    expect(doubleClickWindowAction('darwin', null)).toBe('zoom');
    expect(doubleClickWindowAction('darwin', undefined)).toBe('zoom');
    expect(doubleClickWindowAction('darwin', 'SomethingMacOS27Invented')).toBe('zoom');
  });

  it('zooms everywhere else — there is no such preference off macOS', () => {
    expect(doubleClickWindowAction('win32', null)).toBe('zoom');
    expect(doubleClickWindowAction('linux', 'Minimize')).toBe('zoom');
  });
});

describe('unzoomedDragOrigin — pulling a zoomed window down under the cursor', () => {
  it('keeps the grab point at the same FRACTION of the width, not the same offset', () => {
    // The failure this rules out: a press 1425px along a 1900px zoomed band,
    // carried over as a pixel offset into a 900px restored window, puts the
    // window's left edge 525px to the RIGHT of the cursor — the window flies
    // off to the left and the band is no longer under the pointer.
    const zoomed = { x: 0, y: 0, width: 1900, height: 1200 };
    const origin = unzoomedDragOrigin(
      { x: 1425, y: 10 },
      zoomed,
      { width: 900, height: 700 },
      { x: 1425, y: 10 }
    );
    // 0.75 of the way along 900px = 675px in from the left edge.
    expect(origin).toEqual({ x: 1425 - 675, y: 0 });
  });

  it('clamps the vertical grab inside the restored height', () => {
    const origin = unzoomedDragOrigin(
      { x: 50, y: 900 },
      { x: 0, y: 0, width: 1000, height: 1000 },
      { width: 500, height: 300 },
      { x: 50, y: 900 }
    );
    expect(origin.y).toBe(900 - 299);
  });

  it('survives a zero-width zoomed rect by grabbing the middle', () => {
    const origin = unzoomedDragOrigin(
      { x: 10, y: 0 },
      { x: 0, y: 0, width: 0, height: 0 },
      { width: 400, height: 300 },
      { x: 10, y: 0 }
    );
    expect(origin.x).toBe(10 - 200);
  });
});

describe('WindowMoveDragController', () => {
  it('does not move the window until the cursor clears the threshold', () => {
    // THE CLICK CASE, and the reason the threshold exists at all: a press that
    // never becomes a drag is a click, and the first half of a double-click is
    // exactly that. A window that shifts two pixels every time you double-click
    // to zoom reads as a broken titlebar.
    const { win, state } = fakeWindow();
    const h = harness(win);
    h.moveCursorTo(500, 300);
    expect(h.begin()).toBe(true);

    h.moveCursorTo(502, 301);
    h.tick();
    expect(state.positions).toEqual([]);
    expect(h.controller.hasMoved()).toBe(false);

    h.moveCursorTo(510, 301);
    h.tick();
    expect(state.positions).toEqual([[110, 201]]);
    expect(h.controller.hasMoved()).toBe(true);
  });

  it('tracks the cursor by delta, so the grab point stays under the pointer', () => {
    const { win, state } = fakeWindow({ bounds: { x: 100, y: 200, width: 800, height: 600 } });
    const h = harness(win);
    h.moveCursorTo(500, 300);
    h.begin();

    h.moveCursorTo(560, 340);
    h.tick();
    h.moveCursorTo(300, 100);
    h.tick();

    expect(state.positions).toEqual([
      [160, 240],
      [-100, 0],
    ]);
  });

  it('refuses a full-screen window — leaving full screen is the green button’s job', () => {
    const { win, state } = fakeWindow({ isFullScreen: () => true });
    const h = harness(win);
    expect(h.begin()).toBe(false);
    h.moveCursorTo(900, 900);
    h.tick();
    expect(state.positions).toEqual([]);
    expect(h.controller.activeWindowId()).toBeNull();
  });

  it('un-zooms on the first real move and lands the restored window under the cursor', () => {
    const { win, state } = fakeWindow({ bounds: { x: 0, y: 0, width: 1600, height: 1000 } });
    state.maximized = true;
    const h = harness(win);
    h.moveCursorTo(1200, 20); // three quarters along the zoomed band
    h.begin();

    // Below the threshold: still zoomed, still still.
    h.moveCursorTo(1201, 20);
    h.tick();
    expect(state.unmaximizeCalls).toBe(0);

    h.moveCursorTo(1210, 30);
    h.tick();
    expect(state.unmaximizeCalls).toBe(1);
    // fakeWindow restores to 400x300 — 0.75 * 400 = 300 in from the left, and
    // the anchor is re-taken at the CURRENT cursor so there is no jump.
    expect(state.positions).toEqual([[1210 - 300, 30 - 20]]);
  });

  it('drags a window that refuses to un-zoom rather than not dragging it', () => {
    const { win, state } = fakeWindow({ bounds: { x: 0, y: 0, width: 1600, height: 1000 } });
    state.maximized = true;
    // A non-resizable window, or a platform that declined: `unmaximize` is a
    // no-op and `isMaximized()` is still true afterwards.
    const stubborn: MovableWindow = { ...win, unmaximize: () => {}, isMaximized: () => true };
    const h = harness(stubborn);
    h.moveCursorTo(100, 10);
    h.begin();
    h.moveCursorTo(140, 50);
    h.tick();
    expect(state.positions).toEqual([[40, 40]]);
  });

  it('ends when the window or its renderer goes away', () => {
    // The renderer is the ONLY thing that can say the button came up, so a dead
    // renderer with a live window is exactly the state that would otherwise
    // leave the window following the cursor.
    const { win, state } = fakeWindow();
    const h = harness(win);
    h.moveCursorTo(500, 300);
    h.begin();
    state.alive = false;
    h.moveCursorTo(600, 400);
    h.tick();
    expect(state.positions).toEqual([]);
    expect(h.controller.activeWindowId()).toBeNull();
    expect(h.cancelledCount()).toBe(1);
  });

  it('gives up after the backstop duration', () => {
    const { win, state } = fakeWindow();
    const h = harness(win);
    h.moveCursorTo(500, 300);
    h.begin();
    h.advance(30_001);
    h.moveCursorTo(900, 900);
    h.tick();
    expect(state.positions).toEqual([]);
    expect(h.controller.activeWindowId()).toBeNull();
  });

  it('end() is idempotent and only ends the drag it names', () => {
    // The renderer fires the end from four redundant places (pointerup,
    // pointercancel, lostpointercapture, unmount) and a double-click adds a
    // fifth, so every one of them lands on a drag that may already be over.
    const { win } = fakeWindow();
    const h = harness(win);
    h.begin();
    h.controller.end(999); // some other window
    expect(h.controller.activeWindowId()).toBe(7);
    h.controller.end(7);
    expect(h.controller.activeWindowId()).toBeNull();
    h.controller.end(7);
    h.controller.end();
    expect(h.cancelledCount()).toBe(1);
  });

  it('a second begin supersedes the first instead of leaking its timer', () => {
    const { win } = fakeWindow();
    const h = harness(win);
    h.begin();
    h.begin();
    expect(h.cancelledCount()).toBe(1);
    expect(h.scheduledCount()).toBe(1);
  });

  it('reads the cursor fresh on every tick rather than trusting the wire', () => {
    // Deliberate design note in test form: no coordinate crosses IPC, because a
    // MouseEvent's screenX is CSS pixels and under Windows per-monitor DPI that
    // is not DIP (the trap windowDrag.ts's normalizeToDip exists for).
    const cursorPoint = vi.fn(() => ({ x: 0, y: 0 }));
    const controller = new WindowMoveDragController({
      cursorPoint,
      schedule: () => 1,
      cancelScheduled: () => {},
    });
    const { win } = fakeWindow();
    controller.begin(win);
    expect(cursorPoint).toHaveBeenCalledTimes(1);
  });
});
