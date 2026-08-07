import { ChatTab } from './chatGroupsTypes';

/**
 * The cross-window half of a tab drag, derived fresh on every pointermove and
 * never latched (design §4).
 *
 *  - `local`  — the pointer is inside this window's content rect. This is
 *               EXACTLY today's behaviour: reorder, pane move, split zones.
 *  - `detach` — the pointer left the window and no other chat window's tab
 *               strip is under it, so releasing here tears the tab off into a
 *               new window.
 *  - `merge`  — the pointer is over another chat window's tab strip; releasing
 *               inserts the tab there.
 *
 * The SOURCE RENDERER can only ever tell `local` from "outside", because
 * `BrowserWindow.getBounds()` has no renderer equivalent — only the main
 * process knows where the other windows sit (design D3). So the hook emits
 * `local` or `detach`, and the main process refines a `detach` into a `merge`
 * when its registry says a strip band is under the screen point. `detach` is
 * therefore the correct *default* answer for "outside", not a guess: over the
 * desktop, over another application, and over one of our own non-chat windows
 * all resolve to a new window (design D3), and those are the majority of the
 * plane.
 *
 * Structurally identical to `CrossWindowPhase` in `src/windowDrag.ts`, the
 * main-process half of the same protocol. Declared twice on purpose for now: the
 * two halves landed as independent, uncontested pieces (design §8, Phases 1 and
 * 2), and a renderer module importing a main-process one to borrow a three-arm
 * union is a coupling that costs more than the duplication. Phase 3 wires them
 * together and is the right moment to collapse this into one declaration.
 */
export type CrossWindowPhase =
  | { kind: 'local' }
  | { kind: 'detach' }
  | { kind: 'merge'; targetWindowId: number };

/**
 * A screen-space point, as read off `PointerEvent.screenX/screenY`.
 *
 * Deliberately NOT normalised here. Main converts with `screen.screenToDipPoint`
 * before comparing against window bounds (design D4) — on macOS that is a no-op,
 * on per-monitor-DPI Windows it is the difference between landing on the right
 * display and landing off-screen. Doing it in the renderer would need a second
 * IPC round trip per pointermove.
 */
export interface ScreenPoint {
  screenX: number;
  screenY: number;
}

/**
 * Everything about a tab that may cross a window boundary.
 *
 * This is the serialisable subset of {@link ChatTab}, and what it LEAVES OUT is
 * the whole point of the type:
 *
 *  - `tabId` is window-local identity (chatGroupsTypes.ts:6-10) — a tab keeps it
 *    across a session bind so React keys and strip order survive. The receiving
 *    window mints its own from its own `seq`; carrying the source's id would
 *    collide with a tab that window already has.
 *  - `pendingInitialMessage` / `pendingInitialAttachments` are route-state cargo
 *    consumed exactly once by BaseChat on mount. A queued message that re-sends
 *    in a second window is the same data bug `chatGroupsStorage.stripTransient`
 *    already refuses to persist, arriving by a different door.
 *
 * Both exclusions are enforced by construction — this builds a fresh object
 * rather than spreading and deleting, so a field added to `ChatTab` later
 * cannot leak across a window by default. It has to be added here on purpose.
 */
export interface TabMovePayload {
  sessionId: string;
  title: string;
  userSetName: boolean;
  cwd?: string;
  workflowId?: string;
}

/** @see TabMovePayload — the exclusions are the contract. */
export function payloadFromTab(tab: ChatTab): TabMovePayload {
  const payload: TabMovePayload = {
    sessionId: tab.sessionId,
    title: tab.title,
    userSetName: tab.userSetName,
  };
  // Optional keys are OMITTED rather than set to undefined: the payload rides an
  // IPC boundary, where `{cwd: undefined}` and `{}` are the same message but not
  // the same object, and a test asserting the absence of a key should not have
  // to know which one it got.
  if (tab.cwd !== undefined) payload.cwd = tab.cwd;
  if (tab.workflowId !== undefined) payload.workflowId = tab.workflowId;
  return payload;
}

/**
 * Is this client point outside the window's own content rect?
 *
 * The one test the source renderer can answer without the main process
 * (design D2): `0 <= clientX <= innerWidth`, likewise Y. Inside means today's
 * code path runs untouched; outside is the only door to the cross-window half.
 *
 * BOTH degenerate cases FAIL CLOSED — they answer `false`, "inside", which is
 * the branch that can only ever reorder a tab. A non-finite point (a synthetic
 * event, a detached measurement) and a zero-sized viewport (jsdom before layout,
 * a window mid-minimise) are exactly the moments when a wrong answer would tear
 * a chat into a new window nobody asked for. Comparison against NaN is already
 * false in every direction, so this guard makes the safe answer deliberate
 * rather than accidental.
 */
export function isOutsideViewport(
  point: { x: number; y: number },
  viewport: { width: number; height: number }
): boolean {
  if (!Number.isFinite(point.x) || !Number.isFinite(point.y)) return false;
  if (!(viewport.width > 0) || !(viewport.height > 0)) return false;
  return point.x < 0 || point.y < 0 || point.x > viewport.width || point.y > viewport.height;
}

/** One tab's horizontal extent in its strip, in that strip's client coordinates. */
export interface StripTabRect {
  tabId: string;
  left: number;
  width: number;
}

/**
 * Where in a strip a point lands: the index the dragged tab would be INSERTED
 * BEFORE, so `0` is "first" and `rects.length` is "after the last one".
 *
 * Computed in the TARGET window, because it is the only party that knows its own
 * tab rects (design D3). Main forwards a screen point; the target converts it to
 * client coordinates and calls this.
 *
 * MIDPOINTS, NOT EDGES. Asking "which tab is the cursor over?" leaves the gaps
 * between tabs — and the whole strip past the last tab — with no answer, and
 * makes the caret jump a full tab width as the cursor crosses a boundary. The
 * index is instead the count of tabs whose midpoint the cursor has passed, which
 * is defined everywhere on the axis, moves one step at a time, and is monotonic
 * in `clientX`. A cursor exactly on a midpoint counts as passed, so the caret
 * moves on the way in rather than sticking for a pixel.
 *
 * Array order IS strip order, the same invariant `ChatGroup.tabs` carries
 * (chatGroupsTypes.ts:37). Zero-width rects (jsdom measures every box as zero)
 * collapse to their `left`, which still yields a stable, ordered answer instead
 * of NaN. A non-finite `clientX` appends: an unusable coordinate must not
 * silently mean "insert first" and reorder the target window's strip.
 */
export function insertionIndexFromStrip(clientX: number, rects: StripTabRect[]): number {
  if (!Number.isFinite(clientX)) return rects.length;
  let index = 0;
  for (const rect of rects) {
    if (clientX < rect.left + rect.width / 2) break;
    index += 1;
  }
  return index;
}
