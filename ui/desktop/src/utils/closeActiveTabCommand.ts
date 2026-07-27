import { closeActiveTab } from '../components/chatGroups/closeActiveTabRegistry';
import { isTerminalFocused, requestCloseTerminalPane } from './terminalFocus';

/** Which rung of the Cmd+W ladder consumed the keystroke. */
export type CloseCommandOutcome = 'terminal-pane' | 'chat-tab' | 'window';

/**
 * The Cmd+W ladder, in one place (issue #21).
 *
 * Cmd+W arrives as IPC from the File menu's "Close Tab" item (main.ts — the
 * menu owns the accelerator, the renderer never sees the keydown), and App.tsx
 * answers it with this function. The rungs, in order:
 *
 *   1. focus is in the visible in-app terminal → close the focused terminal
 *      PANE (registerCloseTerminalPane). Closing the last pane destroys the
 *      dock, focus falls back to the chat, and the NEXT Cmd+W hits rung 2 —
 *      the same ladder every terminal emulator and browser walks. Before this
 *      rung existed, a muscle-memory Cmd+W in the terminal closed the chat tab
 *      (taking its terminal with it) or, with no tab active, the whole window.
 *   2. a chat tab is active → close it (closeActiveTabRegistry; the provider
 *      is mounted only under /pair, so this claims nothing elsewhere).
 *   3. no chat tab claimed it, but a VISIBLE terminal pane still exists →
 *      close that pane (Codex review B6 finding 4). This is the last-chance
 *      rung for the gated zero-tab /pair with a dock open on the empty pane:
 *      if the user's focus wandered out of the terminal (a click on the
 *      surface, a dismissed dialog), rung 1 stands down and rung 2 has
 *      nothing, and pre-fix the WINDOW closed out from under a live pane.
 *      Only registered (= visible) docks can answer; requestCloseTerminalPane
 *      picks the last-focused one when none holds focus. On Settings and
 *      other dock-less routes this rung claims nothing.
 *   4. nothing claimed the keystroke → close the window, which is what a macOS
 *      user expects from Cmd+W on a tabless window.
 *
 * This is a separate module rather than an inline handler so the ladder the
 * tests drive is the ladder the app runs — the keystroke never reaches the
 * DOM, so registry-level tests are the only honest gate (see
 * closeActiveTabCommand.test.tsx and keyboardResubmitGuard.test.tsx).
 */
export function runCloseActiveTabCommand(closeWindow: () => void): CloseCommandOutcome {
  if (isTerminalFocused() && requestCloseTerminalPane()) return 'terminal-pane';
  if (closeActiveTab()) return 'chat-tab';
  if (requestCloseTerminalPane()) return 'terminal-pane';
  closeWindow();
  return 'window';
}
