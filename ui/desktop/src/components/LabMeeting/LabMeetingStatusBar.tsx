import React from 'react';
import { useLabMeeting } from '../../contexts/LabMeetingContext';

export const LabMeetingStatusBar: React.FC = () => {
  const lab = useLabMeeting();
  const focused = lab.state.windows.find((w) => w.windowId === lab.state.focusedWindowId);

  if (!focused) {
    return (
      <div className="h-9 flex items-center px-4 text-xs text-text-muted border-t border-border-subtle/30 bg-background-muted/40">
        No window focused.
      </div>
    );
  }

  return (
    <div className="h-9 flex items-center gap-3 px-4 text-xs text-text-default border-t border-border-subtle/30 bg-background-muted/40">
      <span className="inline-flex items-center gap-1.5">
        <span
          className="inline-block w-2 h-2 rounded-full"
          style={{ backgroundColor: focused.accentColor }}
        />
        <span className="font-medium">{focused.name}</span>
        <span className="text-text-muted">#{focused.badge}</span>
      </span>
      <span className="text-text-muted">·</span>
      <span title="working directory">
        cwd: <span className="font-mono">{focused.cwd ?? '—'}</span>
      </span>
      {focused.model && (
        <>
          <span className="text-text-muted">·</span>
          <span>model: {focused.model}</span>
        </>
      )}
      {focused.mode && (
        <>
          <span className="text-text-muted">·</span>
          <span>mode: {focused.mode}</span>
        </>
      )}
      <span className="ml-auto text-text-muted">
        cost: ${(focused.costAccumulated ?? 0).toFixed(4)}
      </span>
    </div>
  );
};
