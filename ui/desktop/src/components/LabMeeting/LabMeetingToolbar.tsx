import React from 'react';
import { useLabMeeting } from '../../contexts/LabMeetingContext';
import { Plus } from '../icons/app-icons';
import { Button } from '../ui/button';

function pickGridLabel(n: number): string {
  if (n === 0) return '';
  const cols = Math.min(n, Math.ceil(Math.sqrt(n * 1.3)));
  const rows = Math.ceil(n / cols);
  return `${cols}×${rows}`;
}

export const LabMeetingToolbar: React.FC = () => {
  const lab = useLabMeeting();
  const onBoard = lab.state.windows.filter((w) => !w.isTucked).length;
  const tucked = lab.state.windows.filter((w) => w.isTucked).length;

  let mode = 'empty';
  if (onBoard > 0 && onBoard <= lab.state.T1) mode = `${pickGridLabel(onBoard)} grid`;
  else if (onBoard > lab.state.T1 && onBoard <= lab.state.T2) mode = 'overlap';
  else if (onBoard > lab.state.T2) mode = 'compact';

  return (
    <div className="flex items-center gap-2 px-4 py-2 border-b border-border-subtle/30 bg-background-muted/40 backdrop-blur-sm">
      <Button size="xs" variant="ghost" onClick={() => lab.spawnWindow()} title="Spawn (⌘N)">
        <Plus className="w-4 h-4" /> <span className="ml-1 text-xs">Spawn</span>
      </Button>
      <Button size="xs" variant="ghost" onClick={() => lab.organize()} title="Re-tile">
        <span className="text-xs">Organize</span>
      </Button>
      <Button size="xs" variant="ghost" onClick={() => lab.clearAll()} title="Close all">
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
