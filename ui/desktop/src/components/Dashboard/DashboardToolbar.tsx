import React from 'react';
import { useDashboard } from '../../contexts/DashboardContext';

export const DashboardToolbar: React.FC = () => {
  const dashboard = useDashboard();
  const onBoard = dashboard.state.windows.filter((w) => !w.isTucked).length;
  const tucked = dashboard.state.windows.filter((w) => w.isTucked).length;

  // Layout: actions (Spawn/Organize/Clear) centered horizontally so they don't
  // collide with the macOS traffic-light buttons at the upper-left when the
  // sidebar is collapsed. Status indicator (count) sits on the right.
  // T1/T2 are auto-computed by DashboardBoard from the board size — no more
  // user-facing threshold controls.
  //
  // The Electron titlebar-drag-region is `position:fixed; top:0; z-index:50;
  // height:32px` and overlaps the top 32px of the app — including this toolbar.
  // We sit at z-[60] so DOM clicks reach the buttons. The `.no-drag` class
  // suppresses OS-level window dragging on the buttons themselves.
  const btnClass =
    'no-drag h-7 px-3 text-[13.5px] font-normal rounded-lg border border-border-subtle ' +
    'bg-background-default text-text-default hover:bg-background-medium ' +
    'active:translate-y-px transition-all';
  return (
    <div className="relative z-[60] flex items-center gap-2 px-4 py-1.5 border-b border-border-subtle/30 bg-background-muted/40 backdrop-blur-sm">
      <div className="absolute left-1/2 -translate-x-1/2 flex items-center gap-2 no-drag">
        <button
          type="button"
          onClick={() => dashboard.spawnWindow()}
          title="Spawn (⌘⇧N)"
          className={btnClass}
        >
          Spawn
        </button>
        <button
          type="button"
          onClick={() => dashboard.organize()}
          title="Re-tile"
          className={btnClass}
        >
          Organize
        </button>
        <button
          type="button"
          onClick={() => dashboard.clearAll()}
          title="Close all"
          className={btnClass}
        >
          Clear
        </button>
      </div>
      <div className="ml-auto flex items-center gap-2 no-drag text-xs text-text-muted">
        {onBoard} on board · {tucked} tucked
      </div>
    </div>
  );
};
