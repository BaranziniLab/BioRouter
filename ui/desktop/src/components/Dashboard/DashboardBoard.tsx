import React, { useEffect, useRef, useState } from 'react';
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
  const [viewport, setViewport] = useState<{ width: number; height: number }>({
    width: 0,
    height: 0,
  });

  // Track viewport size for centerOn() calls.
  useEffect(() => {
    const el = viewportRef.current;
    if (!el) return;
    const update = () => {
      const r = el.getBoundingClientRect();
      setViewport({ width: r.width, height: r.height });
    };
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Recenter on focused window whenever the focus changes (covers spawn, since
  // spawnWindow sets focusedWindowId to the new window).
  const lastFocusedRef = useRef<string | null>(null);
  useEffect(() => {
    const id = dashboard.state.focusedWindowId;
    if (id && id !== lastFocusedRef.current && viewport.width > 0) {
      dashboard.centerOn(id, viewport);
    }
    lastFocusedRef.current = id;
  }, [dashboard.state.focusedWindowId, dashboard, viewport]);

  // Organize triggers a re-center on the focused window.
  const lastOrganizeTickRef = useRef(0);
  useEffect(() => {
    const tick = dashboard.state.organizeTick;
    if (
      tick > lastOrganizeTickRef.current &&
      dashboard.state.focusedWindowId &&
      viewport.width > 0
    ) {
      dashboard.centerOn(dashboard.state.focusedWindowId, viewport);
    }
    lastOrganizeTickRef.current = tick;
  }, [dashboard.state.organizeTick, dashboard, viewport]);

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
              className="px-4 py-2 rounded-xl border border-border-subtle hover:bg-background-medium pointer-events-auto"
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
          className="absolute inset-0 pointer-events-none"
          style={{
            transform: `translate(${cameraOffset.x}px, ${cameraOffset.y}px)`,
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
