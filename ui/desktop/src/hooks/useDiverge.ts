import { useCallback } from 'react';
import { divergeSession } from '../api';
import { useOptionalDashboard } from '../contexts/DashboardContext';
import { useInDashboardCanvas } from '../contexts/DashboardCanvasContext';
import { toastError } from '../toasts';

export interface UseDivergeResult {
  /**
   * Branch the given session into a brand-new session that inherits the
   * conversation history up to the last complete assistant answer, leaving the
   * original untouched.
   *
   * - On a Dashboard canvas window → spawns a new chat box on the canvas
   *   (closeable like any other), leaving the original window in place.
   * - Everywhere else (single chat / Hub / Pair) → opens a NEW focused Electron
   *   window for the branch, leaving the current window exactly where it is.
   *
   * `truncateAfterMs` is the `created` timestamp of the assistant message a
   * per-message Diverge button was clicked on; the branch ends at that answer.
   * Omit it (slash command) to branch from the most recent complete answer.
   *
   * Returns the new session id on success, or null if it failed (a toast is
   * shown on failure).
   */
  diverge: (sessionId: string, truncateAfterMs?: number) => Promise<string | null>;
  /** True when rendered on the Dashboard canvas, so callers can tailor wording. */
  inDashboard: boolean;
}

export function useDiverge(): UseDivergeResult {
  // `useOptionalDashboard` is truthy app-wide (the provider wraps every route),
  // so it cannot tell "in a chat" from "on the canvas". `useInDashboardCanvas`
  // is true ONLY inside a dashboard ChatWindow — that's what decides whether a
  // diverge spawns on-canvas or opens a separate window.
  const dashboard = useOptionalDashboard();
  const onCanvas = useInDashboardCanvas();
  const useCanvasSpawn = onCanvas && dashboard != null;

  const diverge = useCallback(
    async (sessionId: string, truncateAfterMs?: number): Promise<string | null> => {
      if (!sessionId) {
        toastError({ title: 'Diverge failed', msg: 'No active session to diverge.' });
        return null;
      }
      try {
        const response = await divergeSession({
          path: { session_id: sessionId },
          body: truncateAfterMs != null ? { truncateAfter: truncateAfterMs } : {},
          throwOnError: true,
        });

        const newSessionId = response.data?.sessionId;
        const workingDir = response.data?.workingDir;
        if (!newSessionId) {
          throw new Error('Diverge did not return a new session id');
        }

        if (useCanvasSpawn) {
          // On the dashboard canvas: add a new chat box for the branch. The
          // original window stays; the new one becomes focused.
          await dashboard.spawnWindow({ resumeSessionId: newSessionId, cwd: workingDir });
        } else {
          // In a normal chat: open a NEW focused Electron window for the
          // branch. The current window is never navigated or changed.
          window.electron.createChatWindow(undefined, workingDir, undefined, newSessionId, 'pair');
        }
        return newSessionId;
      } catch (err) {
        console.error('Failed to diverge session:', err);
        toastError({
          title: 'Diverge failed',
          msg: err instanceof Error ? err.message : 'Could not branch this conversation.',
        });
        return null;
      }
    },
    [dashboard, useCanvasSpawn]
  );

  return { diverge, inDashboard: useCanvasSpawn };
}
