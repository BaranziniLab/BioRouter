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
 *      true only for a terminal you can actually see and type in.)
 *   2. the new-terminal-pane registry — every VISIBLE dock registers its "add a
 *      pane" handler while it is open. App.tsx asks it first, and only reaches
 *      for a chat tab when focus is not in a terminal or no terminal is open.
 *   3. the close-terminal-pane registry — the exact mirror for Cmd+W (issue
 *      #21: Cmd+W used to skip the terminal entirely and close the chat tab, or
 *      the whole window). App.tsx asks it before the chat-tab ladder, so Cmd+W
 *      in a terminal closes panes first, then (once the dock is gone) chat
 *      tabs, then the window — the same ladder every terminal emulator and
 *      browser walks.
 *
 * MULTIPLE docks can be open at once (Codex review B6 finding 3): terminals are
 * per-pane, so a 4-way split shows up to four visible docks, each registered
 * here — see ChatGroupsShell, which renders one InAppTerminalDock per pane. A
 * single last-write-wins slot would route Cmd+W to whichever dock happened to
 * render last, closing a pane the user is not even looking at. Registrations
 * are therefore PER DOCK, each carrying a `getRoot` so requests can pick the
 * dock that contains `document.activeElement`; when focus has wandered out of
 * every dock, the last-FOCUSED dock answers, then the newest registration.
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

/** The dock root of the current active element, or null when focus is elsewhere. */
function focusedDockRoot(): Element | null {
  const active = typeof document !== 'undefined' ? document.activeElement : null;
  if (!(active instanceof Element)) return null;
  return active.closest(TERMINAL_DOCK_SELECTOR);
}

/**
 * Resolves a registration's dock DOM root. The dock passes a ref-reader; test
 * registrations that exercise pure registry semantics may omit it (they can
 * then never match a focused dock, only the ordering fallbacks).
 */
export type DockRootGetter = () => Element | null;

interface DockRegistration<H> {
  handler: H;
  getRoot: DockRootGetter;
}

const NULL_ROOT: DockRootGetter = () => null;

/**
 * The dock the user last typed in, tracked with one document-level `focusin`
 * listener that lives only while at least one dock is registered. It breaks the
 * tie when focus has LEFT every dock (e.g. the user clicked the chat) but a
 * pane-level command still has to pick among several visible docks.
 */
let lastFocusedDockRoot: Element | null = null;
let focusTrackingInstalled = false;

function trackDockFocus(event: FocusEvent): void {
  const target = event.target;
  if (!(target instanceof Element)) return;
  const root = target.closest(TERMINAL_DOCK_SELECTOR);
  if (root) lastFocusedDockRoot = root;
}

function syncFocusTracking(): void {
  if (typeof document === 'undefined') return;
  const needed = newPaneRegistrations.length + closeRegistrations.length > 0;
  if (needed && !focusTrackingInstalled) {
    document.addEventListener('focusin', trackDockFocus);
    focusTrackingInstalled = true;
  } else if (!needed && focusTrackingInstalled) {
    document.removeEventListener('focusin', trackDockFocus);
    focusTrackingInstalled = false;
    lastFocusedDockRoot = null;
  }
}

/**
 * Registration is array-based and inherently StrictMode-safe: disposing removes
 * exactly the entry it installed, so React's mount-B-then-dispose-A order can
 * never empty the registry, and a split's OTHER docks survive one dock's
 * unmount untouched.
 */
function register<H>(registrations: DockRegistration<H>[], entry: DockRegistration<H>): () => void {
  registrations.push(entry);
  syncFocusTracking();
  return () => {
    const index = registrations.indexOf(entry);
    if (index !== -1) registrations.splice(index, 1);
    syncFocusTracking();
  };
}

/**
 * Ordering fallbacks for a request made while focus is OUTSIDE every dock:
 * the last-focused dock first (if still registered), then newest-first — the
 * most recently opened dock is the likeliest subject of a pane command.
 */
function fallbackOrder<H>(registrations: DockRegistration<H>[]): DockRegistration<H>[] {
  const ordered = [...registrations].reverse();
  const remembered = lastFocusedDockRoot;
  if (remembered && remembered.isConnected) {
    const index = ordered.findIndex((entry) => entry.getRoot() === remembered);
    if (index > 0) ordered.unshift(...ordered.splice(index, 1));
  }
  return ordered;
}

export type NewTerminalPaneHandler = () => void;

const newPaneRegistrations: DockRegistration<NewTerminalPaneHandler>[] = [];

/**
 * A visible terminal dock registers its `addPane` here, with a reader for its
 * own DOM root so requests can route to the dock that holds focus.
 */
export function registerNewTerminalPane(
  next: NewTerminalPaneHandler,
  getRoot: DockRootGetter = NULL_ROOT
): () => void {
  return register(newPaneRegistrations, { handler: next, getRoot });
}

/**
 * Add a pane to the right visible terminal: the dock containing focus when
 * there is one (a focused dock that never registered claims nothing), else the
 * last-focused, else the newest. Returns false when no terminal can take the
 * pane, so the caller falls through to opening a chat tab.
 */
export function requestNewTerminalPane(): boolean {
  if (newPaneRegistrations.length === 0) return false;
  const focusedRoot = focusedDockRoot();
  if (focusedRoot) {
    const focused = newPaneRegistrations.find((entry) => entry.getRoot() === focusedRoot);
    if (!focused) return false;
    focused.handler();
    return true;
  }
  const [first] = fallbackOrder(newPaneRegistrations);
  first.handler();
  return true;
}

/** Tests only — the singleton must not leak across cases. */
export function resetNewTerminalPaneRegistry(): void {
  newPaneRegistrations.length = 0;
  syncFocusTracking();
}

/**
 * Returns true when a pane was closed; false when there is nothing sensible to
 * close (no dock open, or the dock has no active pane), so the caller falls
 * through to the chat-tab ladder.
 */
export type CloseTerminalPaneHandler = () => boolean;

const closeRegistrations: DockRegistration<CloseTerminalPaneHandler>[] = [];

/**
 * A visible terminal dock registers its "close the focused pane" here — the
 * Cmd+W mirror of registerNewTerminalPane, same per-dock discipline.
 */
export function registerCloseTerminalPane(
  next: CloseTerminalPaneHandler,
  getRoot: DockRootGetter = NULL_ROOT
): () => void {
  return register(closeRegistrations, { handler: next, getRoot });
}

/**
 * Close a pane in the right visible terminal.
 *
 * When focus is INSIDE a dock, that dock and only that dock may answer — the
 * user means the terminal under their cursor, so a decline (no active pane)
 * falls through the Cmd+W ladder rather than reaching into a sibling dock.
 * When focus is outside every dock (issue #21's window-close hazard — see
 * runCloseActiveTabCommand's last-chance rung), the last-focused dock is asked
 * first, then the rest newest-first, so a window is never closed out from
 * under a live pane just because DOM focus wandered.
 */
export function requestCloseTerminalPane(): boolean {
  if (closeRegistrations.length === 0) return false;
  const focusedRoot = focusedDockRoot();
  if (focusedRoot) {
    const focused = closeRegistrations.find((entry) => entry.getRoot() === focusedRoot);
    return focused ? focused.handler() : false;
  }
  for (const entry of fallbackOrder(closeRegistrations)) {
    if (entry.handler()) return true;
  }
  return false;
}

/** Tests only — the singleton must not leak across cases. */
export function resetCloseTerminalPaneRegistry(): void {
  closeRegistrations.length = 0;
  syncFocusTracking();
}
