import React, { useEffect, useRef } from 'react';
import { useDashboard } from '../../contexts/DashboardContext';
import { DashboardBoard } from './DashboardBoard';
import { DashboardToolbar } from './DashboardToolbar';

// Module-level state for the maximize/unmaximize lifecycle. We use a mount counter
// + a deferred-exit timer so React StrictMode's mount→unmount→mount in dev doesn't
// flash the window (maximize → unmaximize → re-maximize). When the cleanup fires,
// we schedule the exit for 200ms later; if the route remounts in that window, we
// cancel the pending exit and the user never sees the unmaximize.
let dashboardMountCount = 0;
let pendingExit: ReturnType<typeof setTimeout> | null = null;

export const DashboardRoute: React.FC = () => {
  const dashboard = useDashboard();
  const didAutoSpawn = useRef(false);

  useEffect(() => {
    const electron = (
      window as unknown as {
        electron?: {
          dashboardEnter?: () => Promise<void> | void;
          dashboardExit?: () => Promise<void> | void;
        };
      }
    ).electron;
    dashboardMountCount += 1;
    if (pendingExit) {
      clearTimeout(pendingExit);
      pendingExit = null;
    }
    if (dashboardMountCount === 1) {
      electron?.dashboardEnter?.();
    }
    return () => {
      dashboardMountCount -= 1;
      if (dashboardMountCount === 0) {
        pendingExit = setTimeout(() => {
          if (dashboardMountCount === 0) {
            electron?.dashboardExit?.();
          }
          pendingExit = null;
        }, 200);
      }
    };
  }, []);

  // Auto-spawn one window if state is completely empty.
  useEffect(() => {
    if (didAutoSpawn.current) return;
    if (dashboard.state.windows.length === 0) {
      didAutoSpawn.current = true;
      void dashboard.spawnWindow();
    }
  }, [dashboard.state.windows.length, dashboard]);

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
        void dashboard.spawnWindow();
      } else if (e.key === 'W' || e.key === 'w') {
        if (dashboard.state.focusedWindowId) {
          e.preventDefault();
          dashboard.closeWindow(dashboard.state.focusedWindowId);
        }
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [dashboard]);

  return (
    <div className="h-full w-full flex flex-col min-h-0 bg-background-muted">
      <DashboardToolbar />
      <DashboardBoard />
    </div>
  );
};
