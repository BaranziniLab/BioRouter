import React, { useEffect } from 'react';
import { useDashboard } from '../../contexts/DashboardContext';
import { DashboardBoard } from './DashboardBoard';
import { DashboardToolbar } from './DashboardToolbar';
import { getDashboardShortcutAction } from './dashboardShortcuts';

// Module-level state for the maximize/unmaximize lifecycle. We use a mount counter
// + a deferred-exit timer so React StrictMode's mount→unmount→mount in dev doesn't
// flash the window (maximize → unmaximize → re-maximize). When the cleanup fires,
// we schedule the exit for 200ms later; if the route remounts in that window, we
// cancel the pending exit and the user never sees the unmaximize.
let dashboardMountCount = 0;
let pendingExit: ReturnType<typeof setTimeout> | null = null;

export const DashboardRoute: React.FC = () => {
  const dashboard = useDashboard();

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

  useEffect(() => {
    document.body.classList.add('dashboard-route-active');
    return () => {
      document.body.classList.remove('dashboard-route-active');
    };
  }, []);

  // No auto-spawn on mount: previously this fired when windows.length === 0,
  // which (because the guarding ref lived on the route component) re-fired
  // after Clear + navigate-away + navigate-back, undoing the user's explicit
  // Clear action. The empty-canvas state shows a "Spawn a conversation" CTA
  // (see DashboardBoard) — that's the explicit user-initiated path to create
  // the first window.

  // Dashboard-scoped shortcuts. Cmd+N and Cmd+W are owned by the Electron menu,
  // so canvas controls use Cmd/Ctrl+Alt plus a mnemonic key.
  useEffect(() => {
    const handler = (e: globalThis.KeyboardEvent) => {
      const action = getDashboardShortcutAction(
        e,
        dashboard.state.windows,
        dashboard.state.focusedWindowId
      );
      if (!action) return;
      e.preventDefault();

      switch (action.type) {
        case 'spawn':
          void dashboard.spawnWindow();
          break;
        case 'remove-focused':
          if (dashboard.state.focusedWindowId) {
            dashboard.closeWindow(dashboard.state.focusedWindowId);
          }
          break;
        case 'focus-window':
          dashboard.focusWindow(action.windowId);
          break;
        case 'toggle-window-fold': {
          const win = dashboard.state.windows.find((w) => w.windowId === action.windowId);
          if (win) {
            if (win.folded) dashboard.focusWindow(win.windowId);
            dashboard.foldWindow(win.windowId, !win.folded);
          }
          break;
        }
        case 'toggle-focused-fold': {
          const win = dashboard.state.windows.find(
            (w) => w.windowId === dashboard.state.focusedWindowId
          );
          if (win) {
            if (win.folded) dashboard.focusWindow(win.windowId);
            dashboard.foldWindow(win.windowId, !win.folded);
          }
          break;
        }
        case 'toggle-fold-mode':
          dashboard.setFoldMode(!dashboard.state.foldMode);
          break;
        case 'fold-all':
          dashboard.foldAll();
          break;
        case 'unfold-all':
          dashboard.unfoldAll();
          break;
        case 'organize':
          dashboard.organize();
          break;
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
