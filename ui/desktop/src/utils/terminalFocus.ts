/**
 * Focus-aware Cmd+T / Cmd+W: does the browser-tab reflex mean a CHAT tab or a
 * TERMINAL pane?
 *
 * Cmd+T and Cmd+W are Electron menu accelerators delivered as IPC (see App.tsx,
 * newTabRegistry and closeActiveTabRegistry — the menu owns the keys, so the
 * renderer never sees the keydown). By default they act on chat tabs. But when
 * the user is typing in the in-app terminal, the browser-tab reflex should act
 * on terminal PANES instead — Cmd+T adds one, Cmd+W closes the focused one —
 * and only fall back to the chat tab when the chat is what's focused.
 *
 * Three pieces make that decision:
 *   1. `isTerminalFocused` — is the active element inside a visible terminal
 *      dock? (A hidden dock is display:none and cannot hold focus, so this is
 *      true only for the terminal you can actually see and type in.)
 *   2. the new-terminal-pane registry — the visible dock registers its "add a
 *      pane" handler while it is open, mirroring newTabRegistry. App.tsx asks it
 *      first, and only reaches for a chat tab when focus is not in a terminal or
 *      no terminal is open.
 *   3. the close-terminal-pane registry — the exact mirror for Cmd+W (issue
 *      #21: Cmd+W used to skip the terminal entirely and close the chat tab, or
 *      the whole window). The visible dock registers "close the focused pane";
 *      App.tsx asks it before the chat-tab ladder, so Cmd+W in a terminal
 *      closes panes first, then (once the dock is gone) chat tabs, then the
 *      window — the same ladder every terminal emulator and browser walks.
 */

/** The dock section carries this testid; xterm focus lands on a child of it. */
export const TERMINAL_DOCK_SELECTOR = '[data-testid="in-app-terminal-dock"]';

/**
 * True when focus is inside a visible in-app terminal dock.
 *
 * Pure and DOM-only so it is unit-testable: pass an element, or let it read
 * `document.activeElement`. A `display:none` (hidden) dock can never be the
 * ancestor of `activeElement`, so a background tab's running-but-hidden terminal
 * correctly reads as "not focused".
 */
export function isTerminalFocused(
  active: Element | null = typeof document !== 'undefined' ? document.activeElement : null
): boolean {
  if (!(active instanceof Element)) return false;
  return active.closest(TERMINAL_DOCK_SELECTOR) !== null;
}

export type NewTerminalPaneHandler = () => void;

let handler: NewTerminalPaneHandler | null = null;

/**
 * The currently-visible terminal dock registers its `addPane` here.
 *
 * Only one dock is ever open at a time (the shell gates `open` on the active
 * tab), so last-write-wins is correct. The disposer clears the slot only if it
 * still holds the handler we installed — the same StrictMode-safe discipline as
 * newTabRegistry, so a mount-B-then-dispose-A order cannot empty the registry.
 */
export function registerNewTerminalPane(next: NewTerminalPaneHandler): () => void {
  handler = next;
  return () => {
    if (handler === next) handler = null;
  };
}

/**
 * Add a pane to the visible terminal. Returns false when no terminal is open, so
 * the caller falls through to opening a chat tab.
 */
export function requestNewTerminalPane(): boolean {
  if (!handler) return false;
  handler();
  return true;
}

/** Tests only — the singleton must not leak across cases. */
export function resetNewTerminalPaneRegistry(): void {
  handler = null;
}

/**
 * Returns true when a pane was closed; false when there is nothing sensible to
 * close (no dock open, or the dock has no active pane), so the caller falls
 * through to the chat-tab ladder.
 */
export type CloseTerminalPaneHandler = () => boolean;

let closeHandler: CloseTerminalPaneHandler | null = null;

/**
 * The currently-visible terminal dock registers its "close the focused pane"
 * here — the Cmd+W mirror of registerNewTerminalPane, with the same
 * last-write-wins + StrictMode-safe disposer discipline (only one dock is ever
 * open, and a mount-B-then-dispose-A order must not empty the registry).
 */
export function registerCloseTerminalPane(next: CloseTerminalPaneHandler): () => void {
  closeHandler = next;
  return () => {
    if (closeHandler === next) closeHandler = null;
  };
}

/**
 * Close the visible terminal's focused pane. Returns false when no terminal is
 * open (or it declined), so the caller falls through to closing a chat tab.
 */
export function requestCloseTerminalPane(): boolean {
  if (!closeHandler) return false;
  return closeHandler();
}

/** Tests only — the singleton must not leak across cases. */
export function resetCloseTerminalPaneRegistry(): void {
  closeHandler = null;
}
