import React from 'react';
import { useDashboard } from '../../contexts/DashboardContext';

export const DashboardToolbar: React.FC = () => {
  const dashboard = useDashboard();
  const onCanvas = dashboard.state.windows.length;

  // Tab-style buttons: no border, no background ring — just text + icon hover
  // behavior matching the sidebar Home/Chat/History buttons. Keeps the canvas
  // visual chrome minimal.
  const btnClass =
    'no-drag h-7 px-3 text-[13.5px] font-normal rounded-md ' +
    'text-text-default/80 hover:text-text-default hover:bg-background-medium/40 ' +
    'active:translate-y-px transition-colors';

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
          title="Resolve overlaps and center on focused window"
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
        {onCanvas} on canvas
      </div>
    </div>
  );
};
