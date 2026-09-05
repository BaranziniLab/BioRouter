/**
 * The two window gestures the tab band owes a titlebar — MOVE and ZOOM — done
 * with ordinary DOM events and IPC instead of `-webkit-app-region`.
 *
 * WHY NOT AN APP REGION, WHICH IS THE OBVIOUS ANSWER. The empty part of the tab
 * band is empty *because the tabs are not there yet*, so a `drag` rect over it
 * is a rect whose width is a function of the tab list. Blink only re-collects
 * app-region rects on a paint lifecycle and ships them to the browser process
 * over IPC; until that lands macOS routes with the PREVIOUS set. That is the
 * exact race `ChatTabStrip.appRegion.test.tsx` documents and closes: a
 * just-created tab sat inside the strip's stale `drag` rect and a press on it
 * moved the WINDOW instead of reaching the renderer (measured: 0/11 sample
 * points arrived with one tab open, 11/11 with two, 1/11 for the 2.5s after a
 * tab was created). Giving the leftover space to a `drag` spacer re-creates it
 * wearing a different hat — the strip's `no-drag` box would then widen on every
 * tab open, and the new tab would again land in a region the OS has not been
 * told about.
 *
 * Pointer events carry no such staleness: they are hit-tested against the live
 * layout tree in the renderer. So the strip's app-region set stays exactly as
 * it is — one static `no-drag` box over the scroll box, one static
 * `TAB_BAND_DRAG_GUTTER` on the wrap — and the empty area is handled here.
 *
 * WHY MAIN READS THE CURSOR ITSELF RATHER THAN BEING TOLD WHERE IT IS. Two
 * reasons, and the first is the one that decides it:
 *
 *  1. A window that tracks the cursor exactly does not move RELATIVE to it, so
 *     a renderer-driven `pointermove` loop is asking the moving frame to report
 *     its own motion. Whether Blink still dispatches those moves is a detail of
 *     the compositor's input path, not a contract — driving the move from a
 *     timer on this side does not depend on the answer.
 *  2. `MouseEvent.screenX` is CSS pixels, and under Windows per-monitor DPI
 *     that is not DIP — the trap `windowDrag.ts`'s `normalizeToDip` exists for.
 *     `screen.getCursorScreenPoint()` is already DIP, so no coordinate crosses
 *     the wire and there is nothing to convert.
 *
 * The cost is that the renderer OWES this side an end message: nothing here can
 * see the mouse button come up. Hence `isAlive()` covering the renderer as well
 * as the window, and `maxDurationMs` as the last backstop.
 */

import type { Point, Rect } from './windowDrag';

/**
 * What a double-click on the titlebar should do.
 *
 * macOS has a system setting for this (Desktop & Dock → "Double-click a
 * window's title bar to"), and a titlebar that ignores it is a titlebar that
 * behaves differently from every other window on the machine. The preference is
 * `AppleActionOnDoubleClick`; it is UNSET by default, and unset means Zoom.
 * Everywhere else there is no such preference and zoom is the only sensible
 * reading of a titlebar double-click.
 */
export type DoubleClickWindowAction = 'zoom' | 'minimize' | 'none';

export function doubleClickWindowAction(
  platform: string,
  macPreference?: string | null
): DoubleClickWindowAction {
  if (platform !== 'darwin') return 'zoom';
  switch (macPreference) {
    case 'Minimize':
      return 'minimize';
    case 'None':
      return 'none';
    // 'Maximize', '' (the key is unset, which is the shipping default) and
    // anything a future macOS invents all mean "the ordinary thing".
    default:
      return 'zoom';
  }
}

/**
 * Where a ZOOMED window's top-left should go when a drag pulls it back down to
 * its restored size — the thing the native titlebar does when you drag a
 * maximized window.
 *
 * The grab point keeps its FRACTION of the width, not its pixel offset: a press
 * three quarters of the way along a 1900px zoomed band must not land 1425px
 * into a 900px window, which is off its right edge entirely. Vertically the
 * pixel offset is kept (the band is at the top of the window in both states)
 * and merely clamped inside the restored height.
 */
export function unzoomedDragOrigin(
  grab: Point,
  zoomed: Rect,
  restored: { width: number; height: number },
  cursor: Point
): Point {
  const fractionX = zoomed.width > 0 ? (grab.x - zoomed.x) / zoomed.width : 0.5;
  const clampedFraction = Math.min(Math.max(fractionX, 0), 1);
  const offsetX = clampedFraction * restored.width;
  const offsetY = Math.min(Math.max(grab.y - zoomed.y, 0), Math.max(restored.height - 1, 0));
  return { x: Math.round(cursor.x - offsetX), y: Math.round(cursor.y - offsetY) };
}

/** Opaque handle for whatever the host's timer function returns. */
export type WindowMoveTimer = unknown;

/**
 * The slice of `BrowserWindow` a move drag needs. An interface rather than the
 * class so the controller's arithmetic can be exercised without an Electron
 * window — the same reason `TabDragBroker` takes `TabDragWindow`.
 */
export interface MovableWindow {
  /** `BrowserWindow.id`, so an `end` can name the drag it means to end. */
  readonly id: number;
  /**
   * Window AND renderer. The renderer is the only thing that can tell us the
   * button came up, so a renderer that is gone or crashed must end the drag
   * even though its window is still perfectly alive.
   */
  isAlive(): boolean;
  isFullScreen(): boolean;
  isMaximized(): boolean;
  unmaximize(): void;
  getBounds(): Rect;
  setPosition(x: number, y: number): void;
}

export interface WindowMoveDragOptions {
  /** DIP screen coordinates — `screen.getCursorScreenPoint()` in the real process. */
  cursorPoint: () => Point;
  schedule: (fn: () => void, ms: number) => WindowMoveTimer;
  cancelScheduled: (timer: WindowMoveTimer) => void;
  now?: () => number;
  /** How often the window is re-positioned. ~60Hz. */
  tickMs?: number;
  /**
   * How far the cursor must travel before the window moves at all.
   *
   * A press that never becomes a drag is a CLICK, and the first half of a
   * double-click is one of those. Without this, the hand tremor between the two
   * presses of a zoom gesture would nudge the window a few pixels first.
   */
  thresholdPx?: number;
  /**
   * Last backstop, not a normal path. Nothing on this side can see the mouse
   * button come up, so if the renderer never sends its end message — it was
   * reloaded mid-press, say — the window would otherwise follow the cursor
   * forever. No real window drag lasts this long.
   */
  maxDurationMs?: number;
}

interface ActiveDrag {
  win: MovableWindow;
  timer: WindowMoveTimer;
  /** Cursor position that `origin` was captured against. */
  anchor: Point;
  /** Window top-left at `anchor`. */
  origin: Point;
  /** Has the threshold been crossed? Until it has, the window does not move. */
  moved: boolean;
  /** Was the window zoomed at press time? If so, the first move restores it. */
  zoomed: boolean;
  startedAt: number;
}

export const WINDOW_MOVE_TICK_MS = 16;
/// ⚠ KNOWN GAP, Windows-only, deliberately not fixed yet (2026-09-04).
///
/// This constant is compared against a delta the RENDERER measures with
/// `event.screenX` / `event.screenY` (`useTabBandWindowGesture.ts:174`), which
/// is in **CSS pixels**, while the main process reasons about window geometry in
/// **device-independent pixels**. On macOS and on Windows at 100% scaling the
/// two are the same number, so the 3px "did the user drag, or merely click?"
/// threshold is correct there.
///
/// On Windows with per-monitor DPI scaling (125%, 150%, …) they diverge by the
/// scale factor, so the threshold is effectively 3 × scale. The visible symptom
/// is small: a double-click on a scaled display whose second press wanders a
/// pixel or two may be read as a drag and therefore not zoom.
///
/// It is not fixed because it cannot be VERIFIED from a Mac — it needs a Windows
/// machine with fractional scaling in front of a person. Fixing it blind would
/// mean converting units by a factor nobody here can observe, which is how a
/// cosmetic gap becomes a real one. When it is fixed, the conversion belongs at
/// the IPC boundary (one place), not at each comparison site.
export const WINDOW_MOVE_THRESHOLD_PX = 3;
export const WINDOW_MOVE_MAX_DURATION_MS = 30_000;

/**
 * Moves one window at a time to follow the cursor. Single-slot on purpose: a
 * second `begin` without an `end` is a bug in the caller, not a second drag, so
 * it supersedes rather than accumulating a second timer nobody holds a handle
 * to.
 */
export class WindowMoveDragController {
  private active: ActiveDrag | null = null;

  constructor(private readonly options: WindowMoveDragOptions) {}

  /** The window currently being dragged, or null. For tests and diagnostics. */
  activeWindowId(): number | null {
    return this.active?.win.id ?? null;
  }

  /** True once the threshold has been crossed and the window has really moved. */
  hasMoved(): boolean {
    return this.active?.moved ?? false;
  }

  begin(win: MovableWindow): boolean {
    this.end();
    if (!win.isAlive()) return false;
    // A full-screen window has nowhere to go, and dragging one out of full
    // screen is the green button's job, never the titlebar's.
    if (win.isFullScreen()) return false;

    const bounds = win.getBounds();
    const anchor = this.options.cursorPoint();
    const drag: ActiveDrag = {
      win,
      timer: null,
      anchor,
      origin: { x: bounds.x, y: bounds.y },
      moved: false,
      zoomed: win.isMaximized(),
      startedAt: (this.options.now ?? Date.now)(),
    };
    this.active = drag;
    drag.timer = this.options.schedule(
      () => this.tick(),
      this.options.tickMs ?? WINDOW_MOVE_TICK_MS
    );
    return true;
  }

  /**
   * Idempotent, and safe to call for a window that is not the one being
   * dragged: the renderer fires this from several redundant places (pointerup,
   * pointercancel, lostpointercapture, unmount) and every one of them may
   * arrive after the drag has already ended some other way.
   */
  end(windowId?: number): void {
    const drag = this.active;
    if (!drag) return;
    if (windowId !== undefined && drag.win.id !== windowId) return;
    this.active = null;
    this.options.cancelScheduled(drag.timer);
  }

  private tick(): void {
    const drag = this.active;
    if (!drag) return;
    if (!drag.win.isAlive()) {
      this.end();
      return;
    }
    const now = (this.options.now ?? Date.now)();
    if (now - drag.startedAt > (this.options.maxDurationMs ?? WINDOW_MOVE_MAX_DURATION_MS)) {
      this.end();
      return;
    }

    const cursor = this.options.cursorPoint();
    if (!drag.moved) {
      const threshold = this.options.thresholdPx ?? WINDOW_MOVE_THRESHOLD_PX;
      if (
        Math.abs(cursor.x - drag.anchor.x) < threshold &&
        Math.abs(cursor.y - drag.anchor.y) < threshold
      ) {
        return;
      }
      drag.moved = true;
      if (drag.zoomed) this.restoreForDrag(drag, cursor);
    }

    drag.win.setPosition(
      Math.round(drag.origin.x + cursor.x - drag.anchor.x),
      Math.round(drag.origin.y + cursor.y - drag.anchor.y)
    );
  }

  /** Pull a zoomed window back to its restored size, under the cursor. */
  private restoreForDrag(drag: ActiveDrag, cursor: Point): void {
    const zoomedBounds = drag.win.getBounds();
    drag.win.unmaximize();
    // A window that refuses to un-zoom (not resizable, or the platform declined)
    // is dragged as it is rather than not at all — `origin` is still the bounds
    // captured at press time, so the delta below is already correct for it.
    if (drag.win.isMaximized()) return;
    const restored = drag.win.getBounds();
    drag.origin = unzoomedDragOrigin(drag.anchor, zoomedBounds, restored, cursor);
    drag.anchor = cursor;
  }
}
