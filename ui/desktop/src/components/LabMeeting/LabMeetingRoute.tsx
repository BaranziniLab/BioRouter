import React, { useEffect, useRef } from 'react';
import { useLabMeeting } from '../../contexts/LabMeetingContext';
import { LabMeetingBoard } from './LabMeetingBoard';
import { LabMeetingToolbar } from './LabMeetingToolbar';
import { LabMeetingStatusBar } from './LabMeetingStatusBar';

export const LabMeetingRoute: React.FC = () => {
  const lab = useLabMeeting();
  const didAutoSpawn = useRef(false);

  // Maximize the BrowserWindow on entry (Electron IPC).
  useEffect(() => {
    const electron = (
      window as unknown as { electron?: { labMeetingEnter?: () => Promise<void> | void } }
    ).electron;
    electron?.labMeetingEnter?.();
  }, []);

  // Auto-spawn one window if state is completely empty.
  useEffect(() => {
    if (didAutoSpawn.current) return;
    if (lab.state.windows.length === 0) {
      didAutoSpawn.current = true;
      void lab.spawnWindow();
    }
  }, [lab.state.windows.length, lab]);

  // Keyboard shortcuts (Cmd/Ctrl+N spawn; Cmd/Ctrl+W close focused)
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey;
      if (!meta) return;
      if (e.key === 'n' || e.key === 'N') {
        e.preventDefault();
        void lab.spawnWindow();
      } else if (e.key === 'w' || e.key === 'W') {
        if (lab.state.focusedWindowId) {
          e.preventDefault();
          lab.closeWindow(lab.state.focusedWindowId);
        }
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [lab]);

  return (
    <div className="h-full w-full flex flex-col min-h-0 bg-background-muted">
      <LabMeetingToolbar />
      <LabMeetingBoard />
      <LabMeetingStatusBar />
    </div>
  );
};
