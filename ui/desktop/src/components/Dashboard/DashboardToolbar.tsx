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
  const lab = useDashboard();
  const onBoard = lab.state.windows.filter((w) => !w.isTucked).length;
  const tucked = lab.state.windows.filter((w) => w.isTucked).length;
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
  if (onBoard > 0 && onBoard <= lab.state.T1)
    mode = `${pickGridLabel(onBoard, boardRatio.w, boardRatio.h)} grid`;
  else if (onBoard > lab.state.T1 && onBoard <= lab.state.T2) mode = 'overlap';
  else if (onBoard > lab.state.T2) mode = 'compact';

  return (
    <div className="flex items-center gap-2 px-4 py-2 border-b border-border-subtle/30 bg-background-muted/40 backdrop-blur-sm">
      <Button
        size="xs"
        variant="ghost"
        onClick={() => lab.spawnWindow()}
        title="Spawn (⌘⇧N)"
        className="hover:bg-background-medium transition-colors duration-150"
      >
        <span className="text-xs">Spawn</span>
      </Button>
      <Button
        size="xs"
        variant="ghost"
        onClick={() => lab.organize()}
        title="Re-tile"
        className="hover:bg-background-medium transition-colors duration-150"
      >
        <span className="text-xs">Organize</span>
      </Button>
      <Button
        size="xs"
        variant="ghost"
        onClick={() => lab.clearAll()}
        title="Close all"
        className="hover:bg-background-medium transition-colors duration-150"
      >
        <span className="text-xs">Clear</span>
      </Button>
      <div className="ml-3 flex items-center gap-2">
        <label className="text-xs text-text-muted">T1</label>
        <input
          type="number"
          min={1}
          max={lab.state.T2}
          value={lab.state.T1}
          onChange={(e) => lab.setT1(Number(e.target.value))}
          className="w-12 text-xs px-1 py-0.5 rounded border border-border-subtle bg-background-default"
        />
        <label className="text-xs text-text-muted ml-1">T2</label>
        <input
          type="number"
          min={lab.state.T1}
          value={lab.state.T2}
          onChange={(e) => lab.setT2(Number(e.target.value))}
          className="w-12 text-xs px-1 py-0.5 rounded border border-border-subtle bg-background-default"
        />
      </div>
      <div className="ml-auto text-xs text-text-muted">
        {mode} · {onBoard} on board · {tucked} tucked
      </div>
    </div>
  );
};
