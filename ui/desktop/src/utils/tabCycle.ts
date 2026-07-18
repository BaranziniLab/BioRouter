/**
 * Ctrl+Tab — the browser tab-cycling rule, shared by the two tab strips.
 *
 * Both the chat strip (chatGroups) and the artifact/preview strip cycle on
 * Ctrl+Tab, and they must agree on three things or the app develops a split
 * personality: what counts as the gesture, which tab is next, and — the hard
 * one — WHICH STRIP ANSWERS. That last question is why this module exists
 * rather than each strip rolling its own predicate.
 *
 * ## Why Ctrl+Tab and not Cmd+Tab, even on macOS
 *
 * Cmd+Tab is the macOS application switcher. It is claimed by the window
 * server before any application sees it and cannot be intercepted — Safari and
 * Chrome both use Ctrl+Tab on macOS for exactly this reason. So Ctrl+Tab is not
 * a Windows convention we are leaking onto the Mac; it IS the Mac convention.
 *
 * ## Why this is a DOM listener when Cmd+W had to be a menu item
 *
 * An Electron menu accelerator is consumed before the renderer sees the
 * keydown, which is what forced Cmd+W (and Cmd+T) through the menu + IPC. A
 * live dump of the built application menu shows no item claiming Ctrl+Tab —
 * so, unlike those two, Ctrl+Tab genuinely reaches the DOM and a keydown
 * listener is the honest mechanism. Verified by driving the real app, not
 * assumed: jsdom cannot prove a menu did not eat a key.
 */

/** The gesture: Ctrl+Tab, with Shift meaning "backwards". */
export function isTabCycleEvent(event: {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
}): boolean {
  // Ctrl is required, Meta/Alt must be absent. Requiring ctrlKey is also what
  // keeps plain Tab working as the focus key — this predicate never sees it.
  //
  // There is deliberately NO text-input guard. Tab alone edits/moves focus, but
  // Ctrl+Tab has no editing meaning in any text field, and a browser switches
  // tabs on Ctrl+Tab whether or not you are typing in a box. Guarding on
  // "focus is in a textarea" would make the shortcut dead exactly where the
  // user spends all their time — inside the composer.
  return event.key === 'Tab' && event.ctrlKey && !event.metaKey && !event.altKey;
}

/** Shift reverses direction. */
export function tabCycleOffset(event: { shiftKey: boolean }): 1 | -1 {
  return event.shiftKey ? -1 : 1;
}

/**
 * The index of the tab to activate, or null when there is nothing to do.
 *
 * Null for 0 tabs, for 1 tab (cycling to yourself is not a move), and for an
 * unknown active index. Wraps in both directions — the last tab's Ctrl+Tab
 * lands on the first, which is the browser behaviour users are reaching for.
 */
export function nextTabIndex(length: number, activeIndex: number, offset: 1 | -1): number | null {
  if (length < 2) return null;
  if (activeIndex < 0 || activeIndex >= length) return null;
  // `+ length` before the modulo: JS's % keeps the sign of the dividend, so
  // (0 - 1) % 3 is -1, not 2, and Ctrl+Shift+Tab off the first tab would index
  // out of the array instead of wrapping to the last.
  return (activeIndex + offset + length) % length;
}

/**
 * The marker the artifact/preview panel puts on its root element.
 *
 * Focus is the arbiter between the two strips, so it needs a stable anchor. A
 * `data-testid` would have worked mechanically, but a testid is a promise to
 * tests, not to production code — behaviour hanging off one is how a harmless
 * testid rename becomes a broken shortcut.
 */
export const ARTIFACT_PANEL_ATTR = 'data-br-panel-artifact';

/**
 * Is the event aimed at the preview panel?
 *
 * This is the SOLE arbiter, and both strips consult it, so their answers cannot
 * disagree regardless of which window listener happens to run first. The chat
 * strip returns early when this is true; the preview strip returns early when
 * it is false. Order-independence matters because both listen on window in the
 * capture phase, and capture order is registration order — i.e. mount order,
 * i.e. not something either strip should be relying on.
 *
 * Known limitation, and it is inherent rather than an oversight: an artifact
 * renders inside a sandboxed iframe, and a keydown inside a cross-document
 * iframe never reaches this document at all. With focus INSIDE a preview's
 * iframe body, Ctrl+Tab does nothing — no listener here ever runs. Clicking the
 * panel's chrome (its tab strip, its toolbar) puts focus back in this document
 * and the shortcut works again.
 */
export function isWithinArtifactPanel(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return target.closest(`[${ARTIFACT_PANEL_ATTR}]`) !== null;
}
