import React, { useEffect, useMemo, useRef, useState } from 'react';
import { useDashboard } from '../../contexts/DashboardContext';
import { computeLayout, LayoutInputWindow } from './layoutEngine';
import { ChatWindow } from './ChatWindow';
import { TuckSidebar } from './TuckSidebar';

const DEBOUNCE_MS = 80;

export const DashboardBoard: React.FC = () => {
  const lab = useDashboard();
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
      lab.state.windows.map((w) => ({
        windowId: w.windowId,
        isManuallyPlaced: w.isManuallyPlaced,
        isTucked: w.isTucked,
        position: w.position,
        size: w.size,
        lastInteraction: w.lastInteraction,
      })),
    [lab.state.windows]
  );

  const layout = useMemo(() => {
    if (!boardSize) return new Map();
    return computeLayout(
      layoutInputs,
      boardSize,
      lab.state.T1,
      lab.state.T2,
      lab.state.focusedWindowId
    );
  }, [layoutInputs, boardSize, lab.state.T1, lab.state.T2, lab.state.focusedWindowId]);

  const onBoardWindows = lab.state.windows.filter((w) => !w.isTucked);
  const sidebarOpen = lab.state.windows.some((w) => w.isTucked);
  const minCellSize = useMemo(() => {
    if (!boardSize) return { w: 280, h: 200 };
    const cols = Math.max(1, Math.ceil(Math.sqrt(lab.state.T1)));
    const rows = Math.max(1, Math.ceil(lab.state.T1 / cols));
    return {
      w: Math.max(280, (boardSize.width / cols) * 0.6),
      h: Math.max(200, (boardSize.height / rows) * 0.6),
    };
  }, [boardSize, lab.state.T1]);

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
        lab.evokeWindow(windowId, {
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
              onClick={() => lab.spawnWindow()}
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
                isFocused={lab.state.focusedWindowId === w.windowId}
                isSolo={onBoardWindows.length === 1}
                boardSize={boardSize}
                minSize={minCellSize}
                sidebarOpen={sidebarOpen}
                onTuckByDrag={(id) => lab.tuckWindow(id)}
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
      </div>
      <TuckSidebar onCardDragStart={onCardDragStart} />
    </div>
  );
};
