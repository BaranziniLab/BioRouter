import React from 'react';
import { useDashboard } from '../../contexts/DashboardContext';

export const DashboardToolbar: React.FC = () => {
  const dashboard = useDashboard();
  const onCanvas = dashboard.state.windows.length;
  const allFolded = dashboard.allFolded;

  const btnClass =
    'no-drag h-7 px-3 text-[13.5px] font-normal rounded-md ' +
    'text-text-default/80 hover:text-text-default hover:bg-background-medium/40 ' +
    'active:translate-y-px transition-colors';

  // Mini switch styling — only visible inside the Fold button.
  const switchTrack =
    'relative inline-block w-[22px] h-[12px] rounded-full transition-colors ' +
    (allFolded ? 'bg-text-default/70' : 'bg-background-medium');
  const switchThumb =
    'absolute top-[2px] w-[8px] h-[8px] rounded-full bg-background-default transition-all ' +
    (allFolded ? 'left-[12px]' : 'left-[2px]');

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
          onClick={() => (allFolded ? dashboard.unfoldAll() : dashboard.foldAll())}
          title={allFolded ? 'Unfold all windows' : 'Fold all to cards'}
          aria-pressed={allFolded}
          className={`${btnClass} inline-flex items-center gap-2`}
          disabled={onCanvas === 0}
        >
          Fold
          <span className={switchTrack} aria-hidden="true">
            <span className={switchThumb} />
          </span>
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
