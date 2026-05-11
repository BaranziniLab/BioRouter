import React, { useEffect, useMemo, useRef, useState } from 'react';
import { useDashboard } from '../../contexts/DashboardContext';
import { computeLayout, LayoutInputWindow } from './layoutEngine';
import { ChatWindow } from './ChatWindow';
import { HiddenChatHolder } from './HiddenChatHolder';
import { TuckSidebar } from './TuckSidebar';

const DEBOUNCE_MS = 80;

export const DashboardBoard: React.FC = () => {
  const dashboard = useDashboard();
  const [boardSize, setBoardSize] = useState<{ width: number; height: number } | null>(null);
  const ref = useRef<HTMLDivElement>(null);

  // Track board size via ResizeObserver, debounced.
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    let t: ReturnType<typeof setTimeout> | null = null;
    // Initial measure synchronously
    const r = el.getBoundingClientRect();
    setBoardSize({ width: r.width, height: r.height });
    const ro = new ResizeObserver((entries) => {
      const e = entries[0];
      if (!e) return;
      const w = e.contentRect.width;
      const h = e.contentRect.height;
      if (t) clearTimeout(t);
      t = setTimeout(() => setBoardSize({ width: w, height: h }), DEBOUNCE_MS);
    });
    ro.observe(el);
    return () => {
      ro.disconnect();
      if (t) clearTimeout(t);
    };
  }, []);

  const layoutInputs: LayoutInputWindow[] = useMemo(
    () =>
      dashboard.state.windows.map((w) => ({
        windowId: w.windowId,
        isManuallyPlaced: w.isManuallyPlaced,
        isTucked: w.isTucked,
        position: w.position,
        size: w.size,
        lastInteraction: w.lastInteraction,
      })),
    [dashboard.state.windows]
  );

  const layout = useMemo(() => {
    if (!boardSize) return new Map();
    return computeLayout(
      layoutInputs,
      boardSize,
      dashboard.state.T1,
      dashboard.state.T2,
      dashboard.state.focusedWindowId
    );
  }, [layoutInputs, boardSize, dashboard.state.T1, dashboard.state.T2, dashboard.state.focusedWindowId]);

  const onBoardWindows = dashboard.state.windows.filter((w) => !w.isTucked);
  const sidebarOpen = dashboard.state.windows.some((w) => w.isTucked);
  // Minimum window size:
  //   - width: must fit the full ChatInput picker row (DirSwitcher + model + mode
  //     + extensions + skills + cost + diagnostics). Empirically ~640px.
  //   - height: enough for ≥5 lines of model output PLUS the intact input section
  //     (title bar + ~5 lines × 24px + input row + pickers row ≈ 360-400px).
  // The user can never drag below these — the resize handler clamps and the
  // window springs back to the floor.
  const MIN_WINDOW_W = 640;
  const MIN_WINDOW_H = 400;
  const minCellSize = useMemo(() => {
    if (!boardSize) return { w: MIN_WINDOW_W, h: MIN_WINDOW_H };
    return {
      w: MIN_WINDOW_W,
      h: MIN_WINDOW_H,
    };
  }, [boardSize]);

  // Drag-from-sidebar ghost state (Task 17)
  const [ghost, setGhost] = useState<{ windowId: string; x: number; y: number } | null>(null);

  const onCardDragStart = (windowId: string) => (e: React.PointerEvent) => {
    e.preventDefault();
    let suppressClick = false;
    const handleMove = (ev: PointerEvent) => {
      const r = ref.current?.getBoundingClientRect();
      if (!r) return;
      suppressClick = true;
      setGhost({ windowId, x: ev.clientX - r.left, y: ev.clientY - r.top });
    };
    const handleUp = (ev: PointerEvent) => {
      window.removeEventListener('pointermove', handleMove);
      window.removeEventListener('pointerup', handleUp);
      const r = ref.current?.getBoundingClientRect();
      setGhost(null);
      if (!r || !suppressClick) return; // pure click, let onClick handler do its job
      const x = ev.clientX - r.left;
      const y = ev.clientY - r.top;
      const insideBoard = x >= 0 && x <= r.width && y >= 0 && y <= r.height;
      if (insideBoard) {
        dashboard.evokeWindow(windowId, {
          x: Math.max(0, x - minCellSize.w / 2),
          y: Math.max(0, y - 18),
        });
      }
    };
    window.addEventListener('pointermove', handleMove);
    window.addEventListener('pointerup', handleUp);
  };

  return (
    <div className="flex flex-1 min-h-0">
      <div
        ref={ref}
        className="relative flex-1 overflow-hidden"
        style={{
          backgroundImage:
            'radial-gradient(circle at 1px 1px, rgba(120,120,120,0.18) 1px, transparent 0)',
          backgroundSize: '16px 16px',
        }}
      >
        {boardSize && onBoardWindows.length === 0 && (
          <div className="absolute inset-0 flex items-center justify-center text-text-muted">
            <button
              type="button"
              className="px-4 py-2 rounded-xl border border-border-subtle hover:bg-background-medium"
              onClick={() => dashboard.spawnWindow()}
            >
              Spawn a conversation
            </button>
          </div>
        )}
        {boardSize &&
          onBoardWindows.map((w) => {
            const rect = layout.get(w.windowId);
            if (!rect) return null;
            return (
              <ChatWindow
                key={w.windowId}
                win={w}
                rect={rect}
                isFocused={dashboard.state.focusedWindowId === w.windowId}
                isSolo={onBoardWindows.length === 1}
                boardSize={boardSize}
                minSize={minCellSize}
                sidebarOpen={sidebarOpen}
                onTuckByDrag={(id) => dashboard.tuckWindow(id)}
                onManipulateStart={() => {
                  // Snapshot the current layout for every on-board window and
                  // freeze them all in place. After this, drag/resize on one
                  // window will never reflow the others.
                  const rects: Record<string, { x: number; y: number; w: number; h: number }> = {};
                  for (const [id, r] of layout.entries()) {
                    rects[id] = { x: r.x, y: r.y, w: r.w, h: r.h };
                  }
                  dashboard.freezeAllRects(rects);
                }}
              />
            );
          })}
        {ghost && boardSize && (
          <div
            className="absolute pointer-events-none rounded-2xl border-2 border-dashed border-border-subtle bg-background-default/40 backdrop-blur-sm"
            style={{
              width: minCellSize.w,
              height: minCellSize.h,
              transform: `translate(${ghost.x - minCellSize.w / 2}px, ${ghost.y - 18}px)`,
              zIndex: 200,
            }}
          />
        )}
        {/* Tucked windows render here in a `display: none` container so their
            BaseChat / useChatStream subscriptions stay live — the AI agent keeps
            working while the user has the window tucked. React keeps the
            components mounted; the browser just skips paint + layout. */}
        <div aria-hidden style={{ display: 'none' }}>
          {dashboard.state.windows
            .filter((w) => w.isTucked)
            .map((w) => (
              <HiddenChatHolder key={w.windowId} win={w} />
            ))}
        </div>
      </div>
      <TuckSidebar onCardDragStart={onCardDragStart} />
    </div>
  );
};
