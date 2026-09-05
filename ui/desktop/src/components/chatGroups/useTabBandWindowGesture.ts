import { useCallback, useEffect, useRef } from 'react';

/**
 * The empty part of the tab band behaves like a titlebar: press-and-drag moves
 * the window, double-click zooms it.
 *
 * ⚠ **This must not be done with `-webkit-app-region`, and the obvious version
 * of it has already shipped once and been reverted.** The empty area is empty
 * because the tabs are not there yet, so a `drag` rect over it is a rect whose
 * geometry depends on the tab list — and Blink only re-collects app-region
 * rects on a paint lifecycle, so until that reaches the browser process macOS
 * routes with the PREVIOUS set. A just-created tab then sits in a stale `drag`
 * rect and a press on it moves the window instead of selecting the tab. See
 * `ChatTabStrip.appRegion.test.tsx` for the measurements, and
 * `titlebarWindowGesture.ts` (main) for the rest of the reasoning.
 *
 * Pointer events are hit-tested against the live layout tree, so they carry no
 * such staleness. The strip's app-region set is untouched by this hook: one
 * static `no-drag` box over the scroll box, one static `TAB_BAND_DRAG_GUTTER`
 * on the wrap.
 *
 * Absent `window.electron` — the browser build (`biorouter serve`), the
 * artifact harness, and every test that renders the strip bare — every handler
 * is a no-op. There is no window to move there.
 */

interface WindowGestureBridge {
  windowDragStart: () => void;
  windowDragEnd: () => void;
  windowToggleZoom: () => void;
}

function gestureBridge(): WindowGestureBridge | null {
  const desktop = typeof window !== 'undefined' ? window.electron : undefined;
  // Feature-detected rather than assumed from the type: preload is a separate
  // bundle, so an app running against an older preload has the object without
  // these methods and a bare call would throw on every press in the band.
  if (!desktop || typeof desktop.windowDragStart !== 'function') return null;
  return desktop as unknown as WindowGestureBridge;
}

/**
 * The band background, and only it.
 *
 * `target === currentTarget` is the whole test: a tab, its label button, its
 * close control and the end slot are all DESCENDANTS, so a press on any of them
 * has a deeper target and is left alone. The 3px gaps between tabs do resolve
 * to the strip itself, and dragging the window from one of those is correct —
 * it is band, not tab.
 */
function isBandBackground(event: { target: EventTarget | null; currentTarget: EventTarget }) {
  return event.target === event.currentTarget;
}

export interface TabBandWindowGesture {
  onPointerDown: (event: React.PointerEvent<HTMLElement>) => void;
  onPointerUp: () => void;
  onPointerCancel: () => void;
  onLostPointerCapture: () => void;
  onDoubleClick: (event: React.MouseEvent<HTMLElement>) => void;
}

export function useTabBandWindowGesture(): TabBandWindowGesture {
  const draggingRef = useRef(false);
  const detachRef = useRef<(() => void) | null>(null);

  const endDrag = useCallback(() => {
    detachRef.current?.();
    detachRef.current = null;
    if (!draggingRef.current) return;
    draggingRef.current = false;
    gestureBridge()?.windowDragEnd();
  }, []);

  // The main process drives the move from its own timer and CANNOT see the
  // mouse button come up, so an unmount mid-drag that sent no end message would
  // leave the window following the cursor until main's backstop fires.
  useEffect(() => endDrag, [endDrag]);

  const onPointerDown = useCallback(
    (event: React.PointerEvent<HTMLElement>) => {
      if (!isBandBackground(event)) return;
      // Primary button only. `beginDrag` guards tab drags the same way, and it
      // is what keeps a right-click (context menu) and a middle-click from
      // grabbing the window.
      if (event.button !== 0) return;
      // ⚠ NO `event.detail > 1` GUARD, and the obvious one is a lie. `detail`
      // is 0 on EVERY pointer event per the Pointer Events spec — measured in
      // the running app with real CGEventPost input, both presses of a genuine
      // double-click arrive as `detail: 0`, so such a guard fires only under
      // synthetic events and is dead where it matters. The second press does
      // start a drag, and two other things make that harmless: main's movement
      // threshold means a stationary press moves nothing, and `onDoubleClick`
      // below ends the drag before it asks for the zoom.
      const bridge = gestureBridge();
      if (!bridge) return;

      const element = event.currentTarget;
      // Pointer capture so the release still reaches us once the window has run
      // out of screen and slid out from under the cursor. Feature-detected:
      // jsdom has no capture API, and losing it must not cost us the drag.
      try {
        element.setPointerCapture?.(event.pointerId);
      } catch {
        /* capture is an optimisation here, never a precondition */
      }

      // BELT, and it is load-bearing rather than defensive: `pointerup` on the
      // element is the ordinary path, but a capture that was never granted (or
      // is revoked when this element re-renders out from under the gesture)
      // would leave main's timer running against a button that is already up —
      // and main has no way to notice. A capture-phase listener on the window
      // cannot be lost that way. `blur` covers the window losing focus to
      // something that swallowed the release entirely.
      const stop = () => endDrag();
      window.addEventListener('pointerup', stop, true);
      window.addEventListener('pointercancel', stop, true);
      window.addEventListener('blur', stop);
      detachRef.current = () => {
        window.removeEventListener('pointerup', stop, true);
        window.removeEventListener('pointercancel', stop, true);
        window.removeEventListener('blur', stop);
      };

      draggingRef.current = true;
      bridge.windowDragStart();
    },
    [endDrag]
  );

  const onDoubleClick = useCallback(
    (event: React.MouseEvent<HTMLElement>) => {
      if (!isBandBackground(event)) return;
      // Whatever the press started, the double-click supersedes it — main ends
      // any open drag on this channel too, so the two cannot both act on the
      // window's bounds.
      endDrag();
      gestureBridge()?.windowToggleZoom();
    },
    [endDrag]
  );

  return {
    onPointerDown,
    onPointerUp: endDrag,
    onPointerCancel: endDrag,
    onLostPointerCapture: endDrag,
    onDoubleClick,
  };
}
