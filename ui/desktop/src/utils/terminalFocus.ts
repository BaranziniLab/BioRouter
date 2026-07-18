/**
 * Focus-aware Cmd+T: does "new tab" mean a new CHAT tab or a new TERMINAL pane?
 *
 * Cmd+T is an Electron menu accelerator delivered as IPC (see App.tsx and
 * newTabRegistry — the menu owns the key, so the renderer never sees the
 * keydown). By default it opens a new chat tab. But when the user is typing in
 * the in-app terminal, the browser-tab reflex should add a terminal pane
 * instead, and only fall back to a chat tab when the chat is what's focused.
 *
 * Two pieces make that decision:
 *   1. `isTerminalFocused` — is the active element inside a visible terminal
 *      dock? (A hidden dock is display:none and cannot hold focus, so this is
 *      true only for the terminal you can actually see and type in.)
 *   2. the new-terminal-pane registry — the visible dock registers its "add a
 *      pane" handler while it is open, mirroring newTabRegistry. App.tsx asks it
 *      first, and only reaches for a chat tab when focus is not in a terminal or
 *      no terminal is open.
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
