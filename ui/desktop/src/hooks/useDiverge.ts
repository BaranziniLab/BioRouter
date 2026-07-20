import { useCallback } from 'react';
import { divergeSession } from '../api';
import { toastError } from '../toasts';
import { notifySessionListChanged } from '../utils/sessionListCache';

export interface UseDivergeResult {
  /**
   * Branch the given session into a brand-new session that inherits the
   * conversation history up to the last complete assistant answer, leaving the
   * original untouched.
   *
   * The branch always opens in a NEW focused Electron window, leaving the
   * current window exactly where it is.
   *
   * `truncateAfterMs` and `truncateAfterId` identify the assistant message a
   * per-message Diverge button was clicked on; the durable id takes precedence
   * and the timestamp remains as a compatibility fallback. Omit both to branch
   * from the most recent complete answer.
   *
   * Returns the new session id on success, or null if it failed (a toast is
   * shown on failure).
   */
  diverge: (
    sessionId: string,
    truncateAfterMs?: number,
    truncateAfterId?: string
  ) => Promise<string | null>;
}

export function useDiverge(): UseDivergeResult {

  const diverge = useCallback(
    async (
      sessionId: string,
      truncateAfterMs?: number,
      truncateAfterId?: string
    ): Promise<string | null> => {
      if (!sessionId) {
        toastError({ title: 'Diverge failed', msg: 'No active session to diverge.' });
        return null;
      }
      try {
        const response = await divergeSession({
          path: { session_id: sessionId },
          body: {
            ...(truncateAfterMs != null ? { truncateAfter: truncateAfterMs } : {}),
            ...(truncateAfterId ? { truncateAfterId } : {}),
          },
          throwOnError: true,
        });

        const newSessionId = response.data?.sessionId;
        const workingDir = response.data?.workingDir;
        // The backend resolves and LOCKS the canonical branch name (e.g.
        // "Foo (branch 1)") and hands it back here. Thread it into the new
        // window so the branch's tab is born with the real name instead of the
        // "New Session" placeholder — which is what made the tab name and the
        // Recents name disagree for a diverged session.
        const branchName = response.data?.name;
        if (!newSessionId) {
          throw new Error('Diverge did not return a new session id');
        }

        // The branch is a brand-new session created purely over HTTP; it fires
        // no `session-created` window event, and those are per-renderer anyway.
        // Announce the membership change so every window's Recents / See-all /
        // Home picks up the branch without waiting for an unrelated turn.
        notifySessionListChanged();

        // Open a NEW focused Electron window for the branch. The current
        // window is never navigated or changed.
        window.electron.createDivergedChatWindow(workingDir, newSessionId, branchName);
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
    []
  );

  return { diverge };
}
