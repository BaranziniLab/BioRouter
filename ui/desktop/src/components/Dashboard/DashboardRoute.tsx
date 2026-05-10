import React, { useEffect, useRef } from 'react';
import { useDashboard } from '../../contexts/DashboardContext';
import { DashboardBoard } from './DashboardBoard';
import { DashboardToolbar } from './DashboardToolbar';

export const DashboardRoute: React.FC = () => {
  const lab = useDashboard();
  const didAutoSpawn = useRef(false);

  // Maximize the BrowserWindow on entry (Electron IPC).
  useEffect(() => {
    const electron = (
      window as unknown as {
        electron?: {
          dashboardEnter?: () => Promise<void> | void;
          dashboardExit?: () => Promise<void> | void;
        };
      }
    ).electron;
    electron?.dashboardEnter?.();
    return () => {
      electron?.dashboardExit?.();
    };
  }, []);

  // Auto-spawn one window if state is completely empty.
  useEffect(() => {
    if (didAutoSpawn.current) return;
    if (lab.state.windows.length === 0) {
      didAutoSpawn.current = true;
      void lab.spawnWindow();
    }
  }, [lab.state.windows.length, lab]);

  // Keyboard shortcuts. Cmd+N and Cmd+W are owned by the Electron menu
  // ("New Window" / OS "Close Window") and never reach the renderer first, so
  // we use Shift modifiers to stay out of their way:
  //   Cmd/Ctrl+Shift+N : spawn a window on the board
  //   Cmd/Ctrl+Shift+W : close the focused window
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey;
      if (!meta || !e.shiftKey) return;
      if (e.key === 'N' || e.key === 'n') {
        e.preventDefault();
        void lab.spawnWindow();
      } else if (e.key === 'W' || e.key === 'w') {
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
      <DashboardToolbar />
      <DashboardBoard />
    </div>
  );
};
