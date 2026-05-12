import React, { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { useDashboard } from '../../contexts/DashboardContext';
import { ChatWindow } from './ChatWindow';

// Kept in sync with DashboardProvider — minimum + default spawn size.
const MIN_WINDOW_W = 520;
const MIN_WINDOW_H = 440;

const Z_FOCUSED = 100;
const Z_TILED = 1;

export const DashboardBoard: React.FC = () => {
  const dashboard = useDashboard();
  const viewportRef = useRef<HTMLDivElement>(null);
  const worldRef = useRef<HTMLDivElement>(null);
  const [viewport, setViewport] = useState<{ width: number; height: number }>({
    width: 0,
    height: 0,
  });

  // Tracking ref for the camera re-centering effect below. Declared up here
  // so the synchronous mount useLayoutEffect can also seed it. Tracks the
  // viewport size used for the last centering pass, so we can re-center
  // when the viewport itself changes shape — this happens on entry to the
  // dashboard, because `dashboardEnter()` asynchronously maximizes the
  // Electron window AFTER our initial mount, growing the viewport from the
  // chat-window size to the full-screen size. Without re-centering on that
  // resize, the camera stays calibrated for the old (small) viewport and
  // the focused window ends up off-screen in the new (large) one.
  const lastCenteringRef = useRef<{
    id: string | null;
    tick: number;
    vw: number;
    vh: number;
  }>({ id: null, tick: 0, vw: 0, vh: 0 });

  // Synchronous mount-time pass: measure the viewport AND center on the
  // hydrated focused window before the first paint. Without this, the
  // dashboard renders one frame at the persisted cameraOffset (wherever the
  // user was panned when they last left the canvas) before the regular
  // centering effect fires — visible as a flash of "random position" on
  // every navigation into the dashboard. Runs only on mount.
  useLayoutEffect(() => {
    const el = viewportRef.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) return;
    setViewport({ width: r.width, height: r.height });
    const id = dashboard.state.focusedWindowId;
    if (id) {
      dashboard.centerOn(id, { width: r.width, height: r.height });
      lastCenteringRef.current = {
        id,
        tick: dashboard.state.organizeTick,
        vw: r.width,
        vh: r.height,
      };
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Track viewport size for centerOn() calls. ResizeObserver only — initial
  // measurement is handled by the useLayoutEffect above.
  useEffect(() => {
    const el = viewportRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      const r = el.getBoundingClientRect();
      setViewport({ width: r.width, height: r.height });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Camera re-centering — single effect handling BOTH focus changes (spawn,
  // click-to-focus, enlarge/shrink) AND organize ticks. Two separate effects
  // raced when state changed multiple times in quick succession: each effect
  // updated its own tracking ref unconditionally at the end of its run, so a
  // transient state (viewport not yet measured, focusedWindowId momentarily
  // null between renders, or focus changing before the other effect fired)
  // could leave a ref equal to the current id while centerOn was skipped —
  // and from then on no re-center would ever fire because the ref already
  // "matched" the current focus. Combining them, and only advancing the
  // tracking refs AFTER centerOn actually fires, removes that whole class
  // of stuck states.
  useEffect(() => {
    if (viewport.width === 0 || viewport.height === 0) return;
    const id = dashboard.state.focusedWindowId;
    const tick = dashboard.state.organizeTick;
    const last = lastCenteringRef.current;
    if (!id) {
      lastCenteringRef.current = { id: null, tick, vw: viewport.width, vh: viewport.height };
      return;
    }
    const focusChanged = id !== last.id;
    const organized = tick !== last.tick;
    const viewportChanged = last.vw !== viewport.width || last.vh !== viewport.height;
    if (focusChanged || organized || viewportChanged) {
      dashboard.centerOn(id, viewport);
      lastCenteringRef.current = {
        id,
        tick,
        vw: viewport.width,
        vh: viewport.height,
      };
    }
  }, [
    dashboard.state.focusedWindowId,
    dashboard.state.organizeTick,
    dashboard,
    viewport,
  ]);

  // Pan via pointer drag on the viewport background.
  const panStateRef = useRef<{ active: boolean; lastX: number; lastY: number }>({
    active: false,
    lastX: 0,
    lastY: 0,
  });
  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    // Only pan when the press lands on the viewport background itself, not on a window.
    if (e.target !== e.currentTarget) return;
    panStateRef.current = { active: true, lastX: e.clientX, lastY: e.clientY };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  };
  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!panStateRef.current.active) return;
    const dx = e.clientX - panStateRef.current.lastX;
    const dy = e.clientY - panStateRef.current.lastY;
    panStateRef.current.lastX = e.clientX;
    panStateRef.current.lastY = e.clientY;
    dashboard.panBy(dx, dy);
  };
  const onPointerUp = (e: React.PointerEvent<HTMLDivElement>) => {
    panStateRef.current.active = false;
    try {
      (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
    } catch {
      /* pointer may have already been released */
    }
  };

  // Force the Electron/Chrome compositor to commit the latest cameraOffset
  // transform by briefly clearing and restoring the transform. Without this,
  // the rapid stream of setCameraOffset that fires during the Electron-side
  // window resize on dashboardEnter leaves the world layer's GPU layer in
  // a stale state: getComputedStyle returns the latest matrix but the
  // visual paint (and getBoundingClientRect) reflects an earlier value.
  // Toggling the transform string forces the compositor to commit. Done
  // inside requestAnimationFrame so the clear+restore happens off the
  // initial paint and the user never sees the cleared state.
  useEffect(() => {
    const el = worldRef.current;
    if (!el) return;
    const t = el.style.transform;
    el.style.transform = 'none';
    void el.offsetHeight;
    el.style.transform = t;
    void el.offsetHeight;
  }, [dashboard.state.cameraOffset]);

  // Trackpad two-finger pan via wheel events.
  useEffect(() => {
    const el = viewportRef.current;
    if (!el) return;
    const handler = (ev: WheelEvent) => {
      const target = ev.target as HTMLElement | null;
      // Skip wheels that originate inside a window so chat content can scroll normally.
      if (target && target !== el && target.closest('[data-dashboard-window]')) return;
      ev.preventDefault();
      dashboard.panBy(-ev.deltaX, -ev.deltaY);
    };
    el.addEventListener('wheel', handler, { passive: false });
    return () => el.removeEventListener('wheel', handler);
  }, [dashboard]);

  const minSize = { w: MIN_WINDOW_W, h: MIN_WINDOW_H };
  const { cameraOffset, windows, focusedWindowId } = dashboard.state;

  return (
    <div className="flex flex-1 min-h-0">
      <div
        ref={viewportRef}
        className="relative flex-1 overflow-hidden cursor-grab active:cursor-grabbing"
        style={{
          backgroundImage:
            'radial-gradient(circle at 1px 1px, rgba(120,120,120,0.18) 1px, transparent 0)',
          backgroundSize: '16px 16px',
          backgroundPosition: `${cameraOffset.x}px ${cameraOffset.y}px`,
        }}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
      >
        {windows.length === 0 && (
          <div className="absolute inset-0 flex items-center justify-center text-text-muted pointer-events-none">
            <button
              type="button"
              // Borderless empty-state CTA — matches the sidebar item
              // typography (text-[13.5px], px-3 py-2, hover:bg-background-medium)
              // so it reads as a quiet inline action rather than a heavy
              // bordered button.
              className="px-3 py-2 rounded-lg text-[13.5px] hover:bg-background-medium hover:text-text-default transition-colors duration-150 pointer-events-auto cursor-pointer"
              onClick={() => dashboard.spawnWindow()}
            >
              Spawn a conversation
            </button>
          </div>
        )}
        {/* World layer — translated by cameraOffset. The layer itself is
            pointer-events:none so pointer events fall through to the viewport
            (for pan), and each window re-enables pointer-events on its own.
            Transition is applied only during programmatic camera moves
            (centerOn after focus or organize) — pan stays instant. */}
        <div
          ref={worldRef}
          className="absolute inset-0 pointer-events-none"
          style={{
            transform: `translate3d(${cameraOffset.x}px, ${cameraOffset.y}px, 0)`,
            // Pin transform origin and promote to its own compositor layer.
            // Without these, the rapid stream of cameraOffset updates that
            // happens as Electron animates the window to dashboard size
            // (ResizeObserver → setViewport → centerOn → setCameraOffset,
            // potentially 10+ times in a few hundred ms) would sometimes
            // commit the inline transform value without committing the
            // corresponding visual paint — leaving the windows positioned
            // 1–2 viewport-widths off-center. `will-change: transform`
            // keeps the layer GPU-promoted so every transform change
            // paints; `transformOrigin: 0 0` removes the default
            // center-origin recomputation that can introduce drift at
            // large translate values.
            transformOrigin: '0 0',
            willChange: 'transform',
            transition: dashboard.state.isAnimating
              ? 'transform 200ms cubic-bezier(0.2, 0.8, 0.2, 1)'
              : 'none',
          }}
        >
          {windows.map((w) => (
            <div
              key={w.windowId}
              data-dashboard-window
              className="pointer-events-auto"
              style={{ position: 'absolute', top: 0, left: 0 }}
            >
              <ChatWindow
                win={w}
                rect={{
                  x: w.position.x,
                  y: w.position.y,
                  w: w.size.w,
                  h: w.size.h,
                  zIndex: focusedWindowId === w.windowId ? Z_FOCUSED : Z_TILED,
                }}
                isFocused={focusedWindowId === w.windowId}
                isSolo={windows.length === 1}
                boardSize={viewport}
                minSize={minSize}
                onManipulateStart={() => {
                  // No-op in canvas mode: windows already own absolute world coords.
                }}
              />
            </div>
          ))}
        </div>
      </div>
    </div>
  );
};
