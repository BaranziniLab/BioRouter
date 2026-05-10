import React, { useEffect, useState } from 'react';
import { useDashboard } from '../../contexts/DashboardContext';
import { Button } from '../ui/button';

// Mirror layoutEngine's bestGridConfig: pick the (cols, rows) configuration that
// minimizes the deviation of cell aspect from the target 1.3:1, given the board's
// actual aspect ratio. This matches what the layout engine actually renders.
function pickGridLabel(n: number, boardW: number, boardH: number): string {
  if (n === 0) return '';
  const TARGET_ASPECT = 1.3;
  let bestCols = n;
  let bestRows = 1;
  let bestScore = Infinity;
  for (let cols = 1; cols <= n; cols++) {
    const rows = Math.ceil(n / cols);
    const cellW = boardW / cols;
    const cellH = boardH / rows;
    const aspect = cellW / cellH;
    const score = Math.abs(Math.log(aspect) - Math.log(TARGET_ASPECT));
    if (score < bestScore) {
      bestScore = score;
      bestCols = cols;
      bestRows = rows;
    }
  }
  return `${bestCols}×${bestRows}`;
}

export const DashboardToolbar: React.FC = () => {
  const dashboard = useDashboard();
  const onBoard = dashboard.state.windows.filter((w) => !w.isTucked).length;
  const tucked = dashboard.state.windows.filter((w) => w.isTucked).length;
  const [boardRatio, setBoardRatio] = useState<{ w: number; h: number }>({ w: 16, h: 9 });

  useEffect(() => {
    const measure = () => {
      const board = document.querySelector('div[style*="radial-gradient"]') as HTMLElement | null;
      if (!board) return;
      const r = board.getBoundingClientRect();
      if (r.width > 0 && r.height > 0) {
        setBoardRatio({ w: r.width, h: r.height });
      }
    };
    measure();
    window.addEventListener('resize', measure);
    return () => window.removeEventListener('resize', measure);
  }, [onBoard]);

  let mode = 'empty';
  if (onBoard > 0 && onBoard <= dashboard.state.T1)
    mode = `${pickGridLabel(onBoard, boardRatio.w, boardRatio.h)} grid`;
  else if (onBoard > dashboard.state.T1 && onBoard <= dashboard.state.T2) mode = 'overlap';
  else if (onBoard > dashboard.state.T2) mode = 'compact';

  // Layout: actions (Spawn/Organize/Clear) centered horizontally so they don't
  // collide with the macOS traffic-light buttons at the upper-left when the
  // sidebar is collapsed. Thresholds + status sit on the right.
  // The `no-drag` class is required because the Electron titlebar-drag-region is
  // position:fixed, z-index:50, height:32px across the top of the app — without
  // it, pointer events on these buttons go to the OS drag handler instead.
  return (
    <div className="relative flex items-center gap-2 px-4 py-2 border-b border-border-subtle/30 bg-background-muted/40 backdrop-blur-sm">
      <div className="absolute left-1/2 -translate-x-1/2 flex items-center gap-2 no-drag">
        <Button
          size="sm"
          variant="outline"
          onClick={() => dashboard.spawnWindow()}
          title="Spawn (⌘⇧N)"
          className="no-drag"
        >
          Spawn
        </Button>
        <Button
          size="sm"
          variant="outline"
          onClick={() => dashboard.organize()}
          title="Re-tile"
          className="no-drag"
        >
          Organize
        </Button>
        <Button
          size="sm"
          variant="outline"
          onClick={() => dashboard.clearAll()}
          title="Close all"
          className="no-drag"
        >
          Clear
        </Button>
      </div>
      <div className="ml-auto flex items-center gap-2 no-drag">
        <label className="text-xs text-text-muted">T1</label>
        <input
          type="number"
          min={1}
          max={dashboard.state.T2}
          value={dashboard.state.T1}
          onChange={(e) => dashboard.setT1(Number(e.target.value))}
          className="no-drag w-12 text-xs px-1 py-0.5 rounded border border-border-subtle bg-background-default"
        />
        <label className="text-xs text-text-muted ml-1">T2</label>
        <input
          type="number"
          min={dashboard.state.T1}
          value={dashboard.state.T2}
          onChange={(e) => dashboard.setT2(Number(e.target.value))}
          className="no-drag w-12 text-xs px-1 py-0.5 rounded border border-border-subtle bg-background-default"
        />
        <div className="ml-2 text-xs text-text-muted">
          {mode} · {onBoard} on board · {tucked} tucked
        </div>
      </div>
    </div>
  );
};
