import { ChatGroupsState, leafGroupIds } from './chatGroupsTypes';
import { insertionIndexFromStrip, StripTabRect } from './tabTearOff';

/**
 * The DOM-facing half of tab tear-off: the four questions the shell has to
 * answer with a real document in front of it, each extracted here so it can be
 * read, reasoned about and tested apart from the wiring that calls it.
 *
 * `tabTearOff.ts` holds the pure geometry (is this point outside, which index
 * does this x land on). `windowDrag.ts` holds the main-process geometry (which
 * window is under this screen point). This file is the seam between them and the
 * live DOM — measurement, not policy.
 *
 * Everything here takes its `Document` as an argument rather than reaching for
 * the global. That is not testing hygiene for its own sake: three of these four
 * functions run in the MERGE TARGET window, which receives no pointer events at
 * all while the button is held (design §3, confirmed by the Phase 0
 * measurement), so they are driven entirely by IPC and their inputs must be
 * explicit.
 */

export interface ViewportRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Where the drag-preview caret should sit in a merge target's strip. */
export interface MergeInsertion {
  groupId: string;
  /** Insert BEFORE this index in the group's `tabs` array. */
  index: number;
  /**
   * The tab the caret is drawn on, or `null` when the caret belongs after the
   * last tab. `[data-dropbefore]` is a hairline on a tab's leading edge, so
   * "after the last tab" has no tab to hang on and simply shows nothing —
   * exactly as the within-window reorder already behaves.
   */
  beforeTabId: string | null;
}

/**
 * Every tab-strip band in this window, in VIEWPORT coordinates.
 *
 * One rectangle per strip: a split window has several, and each is an
 * independent merge target (design §4). Main translates these into screen space
 * with `win.getContentBounds()` — the renderer must not attempt that itself,
 * because `window.screenX` is the WINDOW's origin, not the CONTENT's, and the
 * difference is the title bar, which is exactly the band we are measuring.
 *
 * Zero-sized rectangles are dropped. jsdom measures every box as zero, and a
 * zero-area band would be harmless (`rectContains` rejects it) but would also
 * make the registry's contents meaningless to anyone reading a log.
 */
export function measureStripBands(doc: Document): ViewportRect[] {
  const bands: ViewportRect[] = [];
  for (const el of Array.from(doc.querySelectorAll<HTMLElement>('[data-tab-strip-group]'))) {
    const rect = el.getBoundingClientRect();
    if (!(rect.width > 0) || !(rect.height > 0)) continue;
    bands.push({ x: rect.left, y: rect.top, width: rect.width, height: rect.height });
  }
  return bands;
}

/**
 * Where a merge would land, given a point in THIS window's client coordinates.
 *
 * Resolved from the strip element under the point rather than from a stored
 * group id, for the same reason the within-window drag hit-tests every move: a
 * strip can scroll, a layout can change, and a target computed once at the start
 * of a gesture is a target that can be wrong by the time it is used.
 *
 * ⚠ `elementFromPoint` is MISSING in this repo's jsdom — not "returns null",
 * absent — so a test that drives this must stub it. That is stated on the shell
 * suite too; it is repeated here because this is the function that would throw.
 */
export function resolveMergeInsertion(
  doc: Document,
  clientX: number,
  clientY: number
): MergeInsertion | null {
  const element = doc.elementFromPoint?.(clientX, clientY) ?? null;
  const strip = element?.closest<HTMLElement>('[data-tab-strip-group]') ?? null;
  const groupId = strip?.dataset.tabStripGroup;
  if (!strip || !groupId) return null;

  const tabEls = Array.from(strip.querySelectorAll<HTMLElement>('[data-tab-id]'));
  const rects: StripTabRect[] = [];
  for (const el of tabEls) {
    const tabId = el.dataset.tabId;
    if (!tabId) continue;
    const box = el.getBoundingClientRect();
    rects.push({ tabId, left: box.left, width: box.width });
  }

  const index = insertionIndexFromStrip(clientX, rects);
  return { groupId, index, beforeTabId: rects[index]?.tabId ?? null };
}

/**
 * How many tabs this window holds, across every group in its layout.
 *
 * This is obligation 2 (design §8): tearing out a window's ONLY tab is a no-op,
 * because destroying a window to build an identical one is expensive here in a
 * way it is not in a browser — a renderer boots and the extensions reload per
 * session (~4.6s).
 *
 * ⚠ THIS IS NOW THE PRIMARY PATH, not a backstop, and the change is worth
 * knowing about. It used to rarely fire: with one tab the press never reached
 * React at all, because `-webkit-app-region: drag` on the strip claimed it and
 * the OS moved the WINDOW (measured both ways in Phase 0), so D5 was really
 * enforced a level below the renderer. The strip declares `no-drag` across its
 * whole box now — it had to, or a just-created tab kept landing in a stale
 * `drag` rect and the same OS grab ate presses on tabs that were plainly there
 * (see the note on the scroll box in ChatTabStrip.tsx). So the press arrives,
 * the ghost appears, and `isOnlyTab` is what main refuses the tear-off on.
 *
 * The gesture that stops being impossible as a result is the one D6a wants:
 * dragging a lone tab INTO another window now merges and closes this one.
 *
 * Counts across ALL leaves, not just the active group: a window split into two
 * panes with one tab each still has two tabs, and tearing either one out leaves
 * a window with something in it.
 */
export function countWindowTabs(state: ChatGroupsState): number {
  let total = 0;
  for (const groupId of leafGroupIds(state.layout)) {
    total += state.groups[groupId]?.tabs.length ?? 0;
  }
  return total;
}

/**
 * Keep the drag ghost inside the window it is being dragged out of (design D7,
 * obligation 4).
 *
 * A RENDERER cannot paint outside its own window's frame, so once the cursor
 * leaves, this ghost physically cannot follow it. Left alone it flies past the
 * edge and gets clipped, which reads as the tab having been eaten rather than as
 * a tab that is about to become a window. Clamped, it stays pinned to the edge
 * the cursor left through — and the `data-detach` state change (flat, outlined)
 * is what carries the meaning, exactly as D7 specifies.
 *
 * ⚠ THIS IS THE FALLBACK PATH NOW, AND IT IS STILL REACHED. Phase 4b (issue #75)
 * gave the gesture a ghost that DOES follow the cursor onto the desktop, but it
 * is a second `BrowserWindow` opened by main (`dragGhostWindow.ts`), not this
 * element — a renderer's reach has not changed. Between the first move that
 * leaves the window and that window appearing, and on every path where it cannot
 * be built (the DOM probe fails, the page fails to load, a platform where it is
 * refused), the clamped ghost below is what the user sees. Deleting it would
 * restore the original defect on exactly the frames the OS ghost does not cover.
 *
 * Takes the ghost's MEASURED size rather than assuming one. A tab's width
 * depends on its title (88–190px per `.br-tab`), so a constant here would clamp
 * a long title short and let a short one hang off the edge.
 *
 * Degenerate inputs pass the point through untouched: an unmeasured ghost (zero
 * size, the first frame, jsdom) or an unmeasured viewport must not move a ghost
 * the user can see, and "leave it where the hook put it" is exactly today's
 * behaviour.
 */
export function clampGhostToViewport(
  point: { x: number; y: number },
  ghost: { width: number; height: number },
  viewport: { width: number; height: number }
): { x: number; y: number } {
  if (!(ghost.width > 0) || !(ghost.height > 0)) return point;
  if (!(viewport.width > 0) || !(viewport.height > 0)) return point;
  return {
    x: Math.min(Math.max(point.x, 0), Math.max(0, viewport.width - ghost.width)),
    y: Math.min(Math.max(point.y, 0), Math.max(0, viewport.height - ghost.height)),
  };
}
