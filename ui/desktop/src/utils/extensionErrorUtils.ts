/**
 * Shared constants and utilities for extension error handling
 */

import { ExtensionLoadResult } from '../api/types.gen';
import { toastService, ExtensionLoadingStatus } from '../toasts';

export const MAX_ERROR_MESSAGE_LENGTH = 70;

/**
 * Creates recovery hints for the "Ask biorouter" feature when extension loading fails
 */
export function createExtensionRecoverHints(errorMsg: string): string {
  return (
    `Explain the following error: ${errorMsg}. ` +
    'This happened while trying to install an extension. Look out for issues where the ' +
    "extension attempted to execute something incorrectly, didn't exist, or there was trouble with " +
    'the network configuration - VPNs like WARP often cause issues.'
  );
}

/**
 * Formats an error message for display, truncating long messages with a fallback
 * @param errorMsg - The full error message
 * @param fallback - The fallback message to show if the error is too long
 * @returns The formatted error message
 */
export function formatExtensionErrorMessage(
  errorMsg: string,
  fallback: string = 'Failed to add extension'
): string {
  return errorMsg.length < MAX_ERROR_MESSAGE_LENGTH ? errorMsg : fallback;
}

/**
 * Which chat the user is actually looking at, or null when nobody has claimed
 * focus (single-chat hosts like the Dashboard never register). `null` means
 * "permissive" — a host that does not report focus still gets its toasts.
 */
let focusedChatSessionId: string | null = null;

/** True while the extension toast currently on screen is reporting a failure. */
let extensionToastIsFailure = false;

/**
 * Called by the chat shell as the active group changes. Readiness is only news
 * for the chat you are reading; see `showExtensionLoadResults`.
 */
export function setFocusedChatSession(sessionId: string | null): void {
  focusedChatSessionId = sessionId;
}

/** Test seam — module state would otherwise leak between cases. */
export function resetExtensionToastState(): void {
  focusedChatSessionId = null;
  extensionToastIsFailure = false;
}

/**
 * Shows toast notifications for extension load results.
 * Uses grouped toast for multiple extensions, individual error toast for single failed extension.
 *
 * Progressive conversation loading made this multi-tab-sensitive: extensions are
 * per-session, so four split groups each finish their own load and each would
 * have something to say. Two rules keep that to one honest toast:
 *
 *  1. **A clean load only toasts for the chat you are focused on.** "Everything
 *     worked" in a pane you are not reading is not news, and four of them
 *     stacking over the transcript you ARE reading is a regression. Background
 *     successes are dropped silently.
 *  2. **A failure always toasts, whichever chat it came from**, and never gets
 *     overwritten by a later success. A broken extension resurfaces later as a
 *     broken tool call, so it must not be swallowed — and since the grouped
 *     toast reuses one toast id, four chats hitting the same broken extension
 *     coalesce into one toast rather than stacking four.
 *
 * @param results - Array of extension load results from the backend
 * @param sessionId - The chat these results belong to. Omit for non-chat callers
 *                    (settings, manual extension add), which always toast.
 */
export function showExtensionLoadResults(
  results: ExtensionLoadResult[] | null | undefined,
  sessionId?: string
): void {
  if (!results || results.length === 0) {
    return;
  }

  const failedExtensions = results.filter((r) => !r.success);

  if (failedExtensions.length === 0) {
    // Rule 1: a background chat's clean load is silent.
    const isBackgroundChat =
      sessionId !== undefined &&
      focusedChatSessionId !== null &&
      focusedChatSessionId !== sessionId;
    // Rule 2: never downgrade a live failure toast into a success.
    if (isBackgroundChat || (extensionToastIsFailure && toastService.isExtensionToastActive())) {
      return;
    }
  }

  extensionToastIsFailure = failedExtensions.length > 0;

  if (results.length === 1 && failedExtensions.length === 1) {
    const failed = failedExtensions[0];
    const errorMsg = failed.error || 'Unknown error';
    const recoverHints = createExtensionRecoverHints(errorMsg);
    const displayMsg = formatExtensionErrorMessage(errorMsg, 'Failed to load extension');

    toastService.error({
      title: failed.name,
      msg: displayMsg,
      traceback: errorMsg,
      recoverHints,
    });
    return;
  }

  const extensionStatuses: ExtensionLoadingStatus[] = results.map((r) => {
    const errorMsg = r.error || 'Unknown error';
    return {
      name: r.name,
      status: r.success ? 'success' : 'error',
      error: r.success ? undefined : errorMsg,
      recoverHints: r.success ? undefined : createExtensionRecoverHints(errorMsg),
    };
  });

  toastService.extensionLoading(extensionStatuses, results.length, true);
}
