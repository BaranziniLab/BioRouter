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

  // Minimum window size — the four essential elements must always be visible:
  //   1. Header (title bar w/ name + drag handle)              ~36 px
  //   2. ≥5 lines of model output / tool-call section          ~120 px
  //   3. Intact input section: textarea + full picker row + Send
  //   4. Resize corner — always rendered, never occluded
  // This is ALSO the default spawn size: every new window opens at exactly this
  // size, and dragging the resize corner below this springs it back.
  const MIN_WINDOW_W = 720;
  const MIN_WINDOW_H = 360;
  const minCellSize = useMemo(() => ({ w: MIN_WINDOW_W, h: MIN_WINDOW_H }), []);

  // Auto-compute T1 (max non-overlapping windows) and T2 (T1 + 2 allowed
  // overlap) from the board size and the minimum window size. Replaces the
  // user-facing T1/T2 inputs entirely — the layout adapts to the board.
  const { autoT1, autoT2 } = useMemo(() => {
    if (!boardSize) return { autoT1: 1, autoT2: 3 };
    const GAP = 8;
    const cols = Math.max(1, Math.floor((boardSize.width + GAP) / (MIN_WINDOW_W + GAP)));
    const rows = Math.max(1, Math.floor((boardSize.height + GAP) / (MIN_WINDOW_H + GAP)));
    const t1 = Math.max(1, cols * rows);
    return { autoT1: t1, autoT2: t1 + 2 };
  }, [boardSize]);

  // Keep the provider's T1/T2 in sync with the auto-computed values, so the
  // existing enforceT2 / tuck logic uses board-aware limits.
  useEffect(() => {
    if (dashboard.state.T1 !== autoT1) dashboard.setT1(autoT1);
    if (dashboard.state.T2 !== autoT2) dashboard.setT2(autoT2);
  }, [autoT1, autoT2, dashboard]);

  const layout = useMemo(() => {
    if (!boardSize) return new Map();
    return computeLayout(
      layoutInputs,
      boardSize,
      autoT1,
      autoT2,
      dashboard.state.focusedWindowId,
      // Every auto window renders at the minimum/default size — no comfort
      // scaling. The engine packs them tight with up to 2 allowed overlaps.
      { w: MIN_WINDOW_W, h: MIN_WINDOW_H }
    );
  }, [layoutInputs, boardSize, autoT1, autoT2, dashboard.state.focusedWindowId]);

  const onBoardWindows = dashboard.state.windows.filter((w) => !w.isTucked);
  const sidebarOpen = dashboard.state.windows.some((w) => w.isTucked);

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
