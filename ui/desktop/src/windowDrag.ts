/**
 * Tab tear-off and merge — the main-process geometry.
 *
 * Phase 1 of `docs/design/astryx-adoption/tab-tear-off-and-merge.md`. This is
 * the only party that can answer "which window is under this point", because
 * `BrowserWindow.getBounds()` has no renderer equivalent (D3), and because
 * while the mouse button is held the OS delivers every pointer event to the
 * *source* window — the window being dragged over receives nothing at all and
 * therefore cannot hit-test itself.
 *
 * Three things live here:
 *
 * 1. {@link StripBandRegistry} — where every chat window's tab-strip band(s)
 *    are, in screen space. A window with a split layout has more than one
 *    strip, so an entry holds a *list*.
 * 2. {@link resolveDropTarget} — the pure hit test, returning the
 *    `CrossWindowPhase` discriminant the source renderer acts on.
 * 3. {@link tornOffWindowBounds} — where a torn-off window is placed, which
 *    generalises `branchWindowBounds` in `main.ts` (see the note on that
 *    function below).
 *
 * **No Electron import.** Every value here is a plain rectangle or point, and
 * the two Electron-shaped calls (`screen.screenToDipPoint`,
 * `screen.getDisplayNearestPoint`) are reached only through
 * {@link ScreenGeometry}, which `main.ts` will satisfy with the real `screen`
 * module in Phase 3. That is what lets the whole hit test be unit-tested with
 * overlapping windows on two displays without an Electron runtime — and what
 * keeps this module correct regardless of how the Phase 0 pointer-capture
 * measurement lands, since nothing here assumes anything about capture.
 */

export interface Point {
  x: number;
  y: number;
}

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/**
 * What the source renderer should be doing right now. Derived on every
 * `pointermove`, never latched (§4).
 *
 * - `local` — the point is over the source window's own content rect, so the
 *   existing within-window path (`elementFromPoint`, reorder, pane zones)
 *   owns it and nothing cross-window happens.
 * - `merge` — the point is over another chat window's tab-strip band. The
 *   *insertion index* is not decided here: main knows which window, the target
 *   renderer knows where in its own strip (D3).
 * - `detach` — everything else: empty desktop, another application, one of our
 *   own non-chat windows, or the body of another chat window. A release here
 *   makes a new window (D5 still applies — the source renderer refuses when
 *   the tab is the window's last one).
 */
export type CrossWindowPhase =
  | { kind: 'local' }
  | { kind: 'detach' }
  | { kind: 'merge'; targetWindowId: number };

export const PHASE_LOCAL: CrossWindowPhase = { kind: 'local' };
export const PHASE_DETACH: CrossWindowPhase = { kind: 'detach' };

/** What a chat window reports about itself, in one message. */
export interface StripBandsUpdate {
  /**
   * The window's content rect in screen DIP — `win.getContentBounds()`, taken
   * by main, not by the renderer. Bands are viewport-relative, so this is what
   * turns them into screen rectangles.
   */
  contentBounds: Rect;
  /**
   * Viewport-relative strip rectangles, as measured by the renderer. One per
   * strip: a split window has several, and each is a valid merge target.
   */
  bands: Rect[];
}

export interface RegisteredChatWindow extends StripBandsUpdate {
  windowId: number;
  /**
   * Higher is nearer the front. Electron exposes no z-order query, so main
   * stands focus recency in for it: {@link StripBandRegistry.raise} on
   * `focus`, and a window registered for the first time is assumed frontmost
   * because it was just created or just showed itself.
   */
  stackOrder: number;
  /**
   * `true` while the window is minimised or hidden. Such a window is neither
   * a merge target nor an occluder — it is not on screen, so it must not
   * shadow a visible window behind it (§6).
   */
  hidden: boolean;
}

/** Half-open on the far edges, so two abutting windows never both claim a point. */
export function rectContains(rect: Rect, point: Point): boolean {
  if (rect.width <= 0 || rect.height <= 0) return false;
  return (
    point.x >= rect.x &&
    point.x < rect.x + rect.width &&
    point.y >= rect.y &&
    point.y < rect.y + rect.height
  );
}

/** The overlap of two rectangles, or `null` when they do not overlap. */
export function intersectRects(a: Rect, b: Rect): Rect | null {
  const x = Math.max(a.x, b.x);
  const y = Math.max(a.y, b.y);
  const right = Math.min(a.x + a.width, b.x + b.width);
  const bottom = Math.min(a.y + a.height, b.y + b.height);
  if (right <= x || bottom <= y) return null;
  return { x, y, width: right - x, height: bottom - y };
}

/**
 * A window's strip bands as screen rectangles.
 *
 * Each band is clipped to the content rect: a band measured just before a
 * resize can describe an area the window no longer covers, and a stale band
 * must never claim screen the window does not own.
 */
export function bandScreenRects(entry: StripBandsUpdate): Rect[] {
  const { contentBounds } = entry;
  const rects: Rect[] = [];
  for (const band of entry.bands) {
    const translated: Rect = {
      x: contentBounds.x + band.x,
      y: contentBounds.y + band.y,
      width: band.width,
      height: band.height,
    };
    const clipped = intersectRects(translated, contentBounds);
    if (clipped) rects.push(clipped);
  }
  return rects;
}

/**
 * Every chat window's strip geometry, keyed by `BrowserWindow.id`.
 *
 * Written on mount, on resize and whenever the layout tree changes (a split
 * adds a strip); `hidden` is toggled on `hide`/`minimize`; the entry is
 * dropped on `closed`. All three are stated in §4 and each has an edge case
 * in §6 that depends on it — a window that closes mid-drag must stop being a
 * merge target on the very next `pointermove`.
 *
 * A class rather than module state so a test gets a fresh one per case, and so
 * the ordering it maintains cannot leak between them.
 */
export class StripBandRegistry {
  private readonly windows = new Map<number, RegisteredChatWindow>();
  private nextStackOrder = 1;

  /**
   * Record (or replace) a window's bands. A re-registration keeps the window's
   * existing stack position and visibility: reporting a resize is not the same
   * event as being raised and must not reorder the stack, and a window that
   * re-registers while hidden stays hidden until `setHidden(false)` puts it
   * back in play. A window seen for the first time goes to the front, because
   * it has just been created or just shown.
   */
  register(windowId: number, update: StripBandsUpdate): void {
    const existing = this.windows.get(windowId);
    this.windows.set(windowId, {
      windowId,
      contentBounds: { ...update.contentBounds },
      bands: update.bands.map((band) => ({ ...band })),
      stackOrder: existing ? existing.stackOrder : this.nextStackOrder++,
      hidden: existing ? existing.hidden : false,
    });
  }

  /** Move a window to the front of the stack. Called on `focus`/`show`. */
  raise(windowId: number): void {
    const entry = this.windows.get(windowId);
    if (!entry) return;
    entry.stackOrder = this.nextStackOrder++;
  }

  /** Take a window out of, or put it back into, hit-testing. */
  setHidden(windowId: number, hidden: boolean): void {
    const entry = this.windows.get(windowId);
    if (!entry) return;
    entry.hidden = hidden;
  }

  /** Drop a window entirely. Called on `closed`. */
  remove(windowId: number): boolean {
    return this.windows.delete(windowId);
  }

  get(windowId: number): RegisteredChatWindow | undefined {
    return this.windows.get(windowId);
  }

  has(windowId: number): boolean {
    return this.windows.has(windowId);
  }

  get size(): number {
    return this.windows.size;
  }

  /** Visible windows, frontmost first. */
  stackedFrontToBack(): RegisteredChatWindow[] {
    return [...this.windows.values()]
      .filter((entry) => !entry.hidden)
      .sort((a, b) => b.stackOrder - a.stackOrder);
  }

  clear(): void {
    this.windows.clear();
    this.nextStackOrder = 1;
  }
}

/**
 * Which window, if any, the point is over — and if it is a chat window, whether
 * the point is on one of its strips.
 *
 * The rule is occlusion-first, and that is the whole point of the z-order:
 * find the **frontmost** visible chat window whose *content rect* contains the
 * point, and answer from that window alone. A band belonging to a window
 * further back is not a drop target even if the point is inside it, because
 * the user cannot see it — something is on top of it.
 *
 * Answers, in order:
 *
 * - nothing registered under the point ⇒ `detach` (desktop, another app, or
 *   one of our own non-chat windows — see the limitation below)
 * - the frontmost window there is the source ⇒ `local`; the source renderer's
 *   existing in-window path owns the point, including the case of dragging
 *   back in after having been outside (D6b)
 * - the point is on one of that window's bands ⇒ `merge`
 * - otherwise (the window's body) ⇒ `detach`
 *
 * That last line is where this implementation departs from the letter of D3,
 * which names a fourth `{ kind: 'none' }` for a window body. §4's
 * `CrossWindowPhase` has only three arms, and the §5 release table gives a
 * body drop the same outcome as a desktop drop (a new window), which is also
 * Chrome's behaviour. `detach` is therefore the honest encoding of both; the
 * door §7 wanted to keep open — cross-window pane drops — is reopened by
 * adding the arm back, not by preserving a distinction nothing consumes.
 *
 * **Known limitation.** Only chat windows are registered, so a launcher,
 * artifact or app window of ours sitting *above* a chat window's strip cannot
 * be seen here and the point will resolve as a merge. Electron exposes no
 * global window hit test to fix it with, and the consequence is a merge the
 * user may not have aimed at rather than anything destructive.
 *
 * @param screenPoint DIP screen coordinates. Normalise raw `screenX`/`screenY`
 *   with {@link normalizeToDip} first — on Windows with per-monitor DPI the two
 *   spaces differ, and on macOS the conversion is a no-op (D4).
 */
export function resolveDropTarget(
  screenPoint: Point,
  registry: StripBandRegistry,
  sourceWindowId: number
): CrossWindowPhase {
  for (const entry of registry.stackedFrontToBack()) {
    if (!rectContains(entry.contentBounds, screenPoint)) continue;
    if (entry.windowId === sourceWindowId) return PHASE_LOCAL;
    for (const band of bandScreenRects(entry)) {
      if (rectContains(band, screenPoint)) {
        return { kind: 'merge', targetWindowId: entry.windowId };
      }
    }
    return PHASE_DETACH;
  }
  return PHASE_DETACH;
}

/** Clamp a rectangle so it sits wholly inside `workArea` where it can. */
export function clampToWorkArea(rect: Rect, workArea: Rect): Rect {
  const width = Math.min(rect.width, workArea.width);
  const height = Math.min(rect.height, workArea.height);
  const maxX = workArea.x + workArea.width - width;
  const maxY = workArea.y + workArea.height - height;
  return {
    x: Math.min(Math.max(rect.x, workArea.x), maxX),
    y: Math.min(Math.max(rect.y, workArea.y), maxY),
    width,
    height,
  };
}

/**
 * Where a torn-off window goes.
 *
 * Size is copied from the source window and capped at the destination work
 * area — exactly what `branchWindowBounds` (`main.ts`) does, and the reason
 * this is a generalisation of that function rather than a second rule.
 *
 * The origin is what differs, and it differs because the gesture does.
 * `branchWindowBounds` has no cursor: it offsets a *new* window from an anchor
 * by 40px and, when that would overflow, flips to the other side. Here the
 * origin is dictated by the drop — `screenPoint − grabOffset` puts the torn
 * tab back under the cursor that is holding it — so flipping would throw the
 * window away from the pointer. Clamping keeps it as close to the cursor as
 * the work area allows, which is the same intent (stay on screen) expressed
 * for a gesture that has a preferred position.
 *
 * @param screenPoint the drop point, in DIP.
 * @param grabOffset where inside the tab the pointer grabbed it, as recorded
 *   by `beginDrag`; subtracting it lands the tab under the cursor.
 * @param sourceBounds the source window's full bounds (`getBounds()`), for size.
 * @param workArea the work area of the display the drop landed on —
 *   `getDisplayNearestPoint(dropPoint).workArea` (D4), so a drop on a second
 *   monitor makes a window on that monitor.
 */
export function tornOffWindowBounds(
  screenPoint: Point,
  grabOffset: Point,
  sourceBounds: Rect,
  workArea: Rect
): Rect {
  return clampToWorkArea(
    {
      x: Math.round(screenPoint.x - grabOffset.x),
      y: Math.round(screenPoint.y - grabOffset.y),
      width: sourceBounds.width,
      height: sourceBounds.height,
    },
    workArea
  );
}

/**
 * The Electron `screen` calls this module needs, and nothing else.
 *
 * Satisfied in Phase 3 by {@link electronScreenGeometry}; satisfied in tests by
 * a literal with two displays and a fractional scale factor, which is the only
 * way the D4 conversion gets exercised at all — on macOS it is the identity.
 */
export interface ScreenGeometry {
  /** Raw OS screen coordinates → DIP. `screen.screenToDipPoint`. */
  screenToDip(point: Point): Point;
  /** DIP → raw OS screen coordinates. `screen.dipToScreenPoint`. */
  dipToScreen(point: Point): Point;
  /** Work area of the display nearest a DIP point. `screen.getDisplayNearestPoint`. */
  workAreaNearest(point: Point): Rect;
}

/**
 * The `screen` module's shape, structurally — so this file imports no Electron.
 *
 * THE TWO DIP CONVERTERS ARE OPTIONAL BECAUSE ON macOS THEY DO NOT EXIST.
 *
 * `screen.screenToDipPoint` and `screen.dipToScreenPoint` are `@platform
 * win32,linux`: Electron compiles them out of the macOS build entirely (neither
 * name appears anywhere in the strings of `Electron Framework` on darwin, while
 * `getDisplayNearestPoint` and `getCursorScreenPoint` do). Reading one gives
 * `undefined`; calling it throws `TypeError: screen.screenToDipPoint is not a
 * function`.
 *
 * Electron's `.d.ts` declares them unconditionally, so `tsc` never noticed —
 * and the throw landed in an `ipcMain` listener and an `ipcMain.handle`, where
 * the first killed the preview silently and the second rejected into a
 * `.catch(() => {})`. Tear-off and merge were dead on the primary platform with
 * no visible error at all.
 *
 * Declaring them optional here is what makes the absence a fact the type system
 * carries, so {@link electronScreenGeometry} is forced to answer for it.
 */
export interface ElectronScreenLike {
  screenToDipPoint?(point: Point): Point;
  dipToScreenPoint?(point: Point): Point;
  getDisplayNearestPoint(point: Point): { workArea: Rect };
}

/**
 * The conversion on a platform that does not scale screen coordinates.
 *
 * macOS reports pointer and window geometry in points, which ARE device
 * independent pixels — the backing scale factor never enters the coordinate
 * space the way it does under Windows per-monitor DPI. So identity is not a
 * degraded fallback here, it is the correct conversion; the bug was calling a
 * method that does not exist in order to compute it.
 *
 * A fresh object rather than the argument, so a caller can never alias a point
 * it is about to mutate into the geometry's answer.
 */
function identityPoint(point: Point): Point {
  return { x: point.x, y: point.y };
}

/**
 * Adapt Electron's `screen` module, tolerating the platforms that only have
 * part of it.
 *
 * Feature-detected once, at construction: whether a native method exists is
 * fixed at Electron's build time, and `main.ts` builds this lazily on first use
 * (after `app` is ready) so there is no window in which the module is only
 * half-populated.
 */
export function electronScreenGeometry(screen: ElectronScreenLike): ScreenGeometry {
  const screenToDip =
    typeof screen.screenToDipPoint === 'function'
      ? (point: Point) => screen.screenToDipPoint!(point)
      : identityPoint;
  const dipToScreen =
    typeof screen.dipToScreenPoint === 'function'
      ? (point: Point) => screen.dipToScreenPoint!(point)
      : identityPoint;
  return {
    screenToDip,
    dipToScreen,
    workAreaNearest: (point) => screen.getDisplayNearestPoint(point).workArea,
  };
}

/**
 * Normalise the renderer's raw `event.screenX`/`screenY` into DIP.
 *
 * Every point that reaches {@link resolveDropTarget} or
 * {@link tornOffWindowBounds} must go through here first: `getBounds()` and
 * `getContentBounds()` are DIP, and on Windows with per-monitor DPI raw screen
 * coordinates are not. The bug it prevents — a tab dropped on a second monitor
 * landing on the wrong display or off-screen — is invisible on macOS, where
 * the conversion is the identity (D4).
 */
export function normalizeToDip(rawScreenPoint: Point, geometry: ScreenGeometry): Point {
  return geometry.screenToDip(rawScreenPoint);
}

/**
 * The full main-process answer for one `pointermove`: normalise, then resolve.
 */
export function resolveDropTargetForRawPoint(
  rawScreenPoint: Point,
  geometry: ScreenGeometry,
  registry: StripBandRegistry,
  sourceWindowId: number
): CrossWindowPhase {
  return resolveDropTarget(normalizeToDip(rawScreenPoint, geometry), registry, sourceWindowId);
}

/**
 * THE RENDERER'S POINT ON THE WIRE — `{screenX, screenY}`, NOT `{x, y}`.
 *
 * A pointer event names its screen coordinates `screenX`/`screenY`, and the
 * renderer forwards them under those names (preload's `TabDragScreenPoint`).
 * Every geometry function in this module speaks {@link Point}, whose fields are
 * `x`/`y`. The two shapes share no field.
 *
 * They met in an `ipcMain` listener, whose payload argument is `any`, so the
 * wire object was handed straight to {@link resolveDropTargetForRawPoint} and
 * TypeScript said nothing. `undefined >= rect.x` is `false` for every rectangle,
 * so EVERY drop resolved to `detach` — a merge was unreachable — and
 * `Math.round(undefined - grabOffset.x)` made a torn-off window at `NaN, NaN`.
 *
 * This type and {@link screenPointFromWire} exist so the conversion has one
 * name, one place, and a test. The seam cannot silently come back, because the
 * IPC payloads in `main.ts` are typed `unknown` and only these functions can
 * open them.
 */
export interface ScreenPointWire {
  screenX: number;
  screenY: number;
}

/**
 * Validate a renderer-supplied wire point and convert it to geometry space.
 *
 * `null` for anything that is not two finite numbers — including the `NaN` and
 * `Infinity` a renderer can put on the wire, which would otherwise poison every
 * comparison downstream rather than failing here where the caller can answer
 * "keep the tab".
 */
export function screenPointFromWire(input: unknown): Point | null {
  const wire = input as Partial<ScreenPointWire> | null | undefined;
  if (typeof wire?.screenX !== 'number' || typeof wire?.screenY !== 'number') return null;
  if (!Number.isFinite(wire.screenX) || !Number.isFinite(wire.screenY)) return null;
  return { x: wire.screenX, y: wire.screenY };
}

/** Geometry space back to the wire, for the messages a target renderer reads. */
export function screenPointToWire(point: Point): ScreenPointWire {
  return { screenX: point.x, screenY: point.y };
}

/**
 * The grab offset, which IS `{x, y}` on the wire — but still arrives as `any`.
 *
 * `{0, 0}` rather than `null` for a malformed one: an offset is a refinement of
 * where the new window lands, never a reason to refuse the drop, and dropping
 * the tab at the cursor's top-left is a perfectly usable answer.
 */
export function grabOffsetFromWire(input: unknown): Point {
  const offset = input as Partial<Point> | null | undefined;
  if (typeof offset?.x !== 'number' || typeof offset?.y !== 'number') return { x: 0, y: 0 };
  if (!Number.isFinite(offset.x) || !Number.isFinite(offset.y)) return { x: 0, y: 0 };
  return { x: offset.x, y: offset.y };
}

/**
 * The full main-process answer for a detach release: normalise the drop point,
 * pick the display it landed on, and place the window there.
 */
export function tornOffWindowBoundsForRawPoint(
  rawScreenPoint: Point,
  grabOffset: Point,
  sourceBounds: Rect,
  geometry: ScreenGeometry
): Rect {
  const dropPoint = normalizeToDip(rawScreenPoint, geometry);
  return tornOffWindowBounds(
    dropPoint,
    grabOffset,
    sourceBounds,
    geometry.workAreaNearest(dropPoint)
  );
}

/** Why a release may not become a new window, or `null` if it may. */
export type DetachRefusal = 'only-tab' | 'no-session' | null;

/**
 * May this release tear the tab out into a NEW WINDOW?
 *
 * Both refusals apply to the DETACH branch alone — a merge of the same tab into
 * an existing window is a real move in both cases — which is why the question
 * is asked in main at all: only main knows which branch a drop took.
 *
 *   `only-tab`   — D5. Destroying a window to build an identical one costs a
 *                  renderer boot and a per-session extension reload (~4.6s) for
 *                  no change the user asked for.
 *
 *   `no-session` — a fresh tab carries `sessionId: ''` until the user sends
 *                  something, and the torn-off window is SEEDED from that id.
 *                  With an empty one `createChat` had nothing to resume, so the
 *                  new window opened on Home with no tab while the source still
 *                  removed the tab: the chat appeared to vanish. Nothing of
 *                  value was lost — the tab was blank — but "looks like data
 *                  loss" is a defect on its own.
 *
 *                  The alternative, minting a session so the gesture has
 *                  something to carry, was rejected: a session is what sending
 *                  a message creates, and a drag that abandons its blank tab
 *                  would leave one behind in the store and in Chat history.
 *
 * Both answer `noop` at the call site, which is the word the renderer already
 * understands as "the tab stays put" (ChatGroupsShell.commitCrossWindow).
 */
export function detachRefusal(request: {
  isOnlyTab?: boolean;
  tab?: { sessionId?: string };
}): DetachRefusal {
  if (request.isOnlyTab) return 'only-tab';
  if (!request.tab?.sessionId) return 'no-session';
  return null;
}

// ═══════════════════════════════════════════════════════════════════════════
// THE PREVIEW AND MERGE BROKER
//
// One drag is in flight at a time, and exactly one window may be painting a
// merge caret for it. That is a small amount of state with a large number of
// ways to be left wrong, so it lives here — beside the geometry, with no
// Electron import — rather than as loose module variables in `main.ts` where
// nothing could reach it to test it.
//
// Two defects are what moved it:
//
//   D4 — the caret was cleared only when the window HOLDING it went away. A
//        dying SOURCE (Cmd+R, a renderer crash, a closed window) left the
//        target painting an insertion hairline forever, because a document swap
//        runs no React effect cleanups and the renderer's own `tab-drag:end`
//        never arrives. `registerTerminalOwnerTeardown` had already solved
//        exactly this for shells; the broker gives tab drag the same door.
//
//   D5 — the merge round trip had a deadline on ONE side. The source gave up
//        after 2s and kept its tab; the target had no deadline at all, so a
//        window busy with a heavy streaming turn could insert the tab late and
//        acknowledge into a request that no longer existed. Net result: the
//        same session open in two windows.
// ═══════════════════════════════════════════════════════════════════════════

/** What the broker needs of a live window. `main.ts` satisfies it with `BrowserWindow`. */
export interface TabDragWindow {
  /**
   * `webContents.id`. An ack is only believed from the window it was sent to —
   * `tabDragAckMerge` is on EVERY renderer's preload, and the handler used to
   * take a bare request id from anyone who sent one.
   */
  readonly webContentsId: number;
  isAlive(): boolean;
  send(channel: string, payload: unknown): void;
}

/** Opaque handle for whatever the host's timer function returns. */
export type TabDragTimer = unknown;

export interface TabDragBrokerOptions {
  /** Live window by id, or `null` once it is gone. */
  resolveWindow: (windowId: number) => TabDragWindow | null;
  /** How long the SOURCE waits for the target to confirm an insert. */
  ackTimeoutMs?: number;
  /**
   * How much of that budget is reserved for the ack to travel back.
   *
   * The target is given `ackTimeoutMs - ackGraceMs`, so a target that decides
   * to insert has done so with this much time still on main's clock. Without
   * it the two deadlines are the same instant and the round trip is a coin
   * flip; with it, the only way to lose is for main to be blocked for the
   * whole grace period — which the deferred settle below then covers.
   */
  ackGraceMs?: number;
  now?: () => number;
  schedule?: (fn: () => void, ms: number) => TabDragTimer;
  cancelScheduled?: (timer: TabDragTimer) => void;
  /**
   * Run after the current I/O has been drained. `setImmediate` in the real
   * process, and the reason it is here rather than inlined:
   *
   * Node runs the TIMERS phase before the POLL phase. If main's loop is blocked
   * past the deadline, the timeout callback and an ack that is already sitting
   * on the IPC channel both become runnable, and the timer wins — main answers
   * "keep the tab" to a target that has already inserted it. Deferring the
   * settle to the CHECK phase (which runs after POLL) lets that queued ack be
   * delivered first, and it then finds the request still pending and settles it
   * truthfully. Costs one tick on a path that has already waited seconds.
   */
  deferSettle?: (fn: () => void) => void;
}

interface PendingMerge {
  targetWindowId: number;
  /** The only webContents whose ack counts for this request. */
  targetWebContentsId: number;
  timer: TabDragTimer;
  settle: (inserted: boolean) => void;
}

/** The message a target renderer receives when it is asked to take a tab. */
export interface TabDragMergeRequest extends ScreenPointWire {
  requestId: number;
  tab: unknown;
  /**
   * Wall clock (`Date.now()`) after which the target must REFUSE rather than
   * insert. Already backed off from main's own deadline by `ackGraceMs`, so the
   * target never needs to know a grace exists — it just gets a deadline that is
   * by construction earlier than the one it is racing.
   */
  expiresAt: number;
}

export const TAB_DRAG_MERGE_ACK_TIMEOUT_MS = 2000;
export const TAB_DRAG_MERGE_ACK_GRACE_MS = 400;

export class TabDragBroker {
  private previewWindowId: number | null = null;
  private sourceWindowId: number | null = null;
  private seq = 0;
  private readonly pending = new Map<number, PendingMerge>();

  private readonly resolveWindow: (windowId: number) => TabDragWindow | null;
  private readonly ackTimeoutMs: number;
  private readonly ackGraceMs: number;
  private readonly now: () => number;
  private readonly schedule: (fn: () => void, ms: number) => TabDragTimer;
  private readonly cancelScheduled: (timer: TabDragTimer) => void;
  private readonly deferSettle: (fn: () => void) => void;

  constructor(options: TabDragBrokerOptions) {
    this.resolveWindow = options.resolveWindow;
    this.ackTimeoutMs = options.ackTimeoutMs ?? TAB_DRAG_MERGE_ACK_TIMEOUT_MS;
    this.ackGraceMs = options.ackGraceMs ?? TAB_DRAG_MERGE_ACK_GRACE_MS;
    this.now = options.now ?? (() => Date.now());
    this.schedule = options.schedule ?? ((fn, ms) => setTimeout(fn, ms));
    this.cancelScheduled =
      options.cancelScheduled ?? ((timer) => clearTimeout(timer as ReturnType<typeof setTimeout>));
    this.deferSettle =
      options.deferSettle ??
      ((fn) => {
        if (typeof setImmediate === 'function') setImmediate(fn);
        else setTimeout(fn, 0);
      });
  }

  /** Read-only view, for `main.ts` logging and for tests. */
  get previewWindow(): number | null {
    return this.previewWindowId;
  }
  get dragSourceWindow(): number | null {
    return this.sourceWindowId;
  }
  get pendingMergeCount(): number {
    return this.pending.size;
  }

  /**
   * Remember which window is holding the pointer.
   *
   * Recorded on every move rather than on a start message because there is no
   * start message: the gesture only becomes main's business once it leaves the
   * source window. Without this the broker cannot tell a source's death from
   * any other window's, which is precisely how D4's stranded caret survived.
   */
  noteDragSource(windowId: number): void {
    this.sourceWindowId = windowId;
  }

  /**
   * Point exactly one window at a merge caret, and un-point the previous one.
   *
   * The target is NOT raised or focused: raising it would take the drag away
   * from the source window holding the pointer capture, and the gesture would
   * die mid-air (D3).
   */
  showPreview(targetWindowId: number, point: ScreenPointWire): void {
    if (this.previewWindowId !== null && this.previewWindowId !== targetWindowId) {
      this.sendPreviewOff(this.previewWindowId);
    }
    const target = this.resolveWindow(targetWindowId);
    if (!target || !target.isAlive()) {
      this.previewWindowId = null;
      return;
    }
    this.previewWindowId = targetWindowId;
    target.send('tab-drag:preview', {
      active: true,
      screenX: point.screenX,
      screenY: point.screenY,
    });
  }

  /** No window should be showing a caret. Safe to call when none is. */
  clearPreview(): void {
    if (this.previewWindowId === null) return;
    const previewWindowId = this.previewWindowId;
    this.previewWindowId = null;
    this.sendPreviewOff(previewWindowId);
  }

  /** The gesture is over, however it ended. */
  endDrag(): void {
    this.clearPreview();
    this.sourceWindowId = null;
  }

  /**
   * Ask a target to insert the tab, and wait for it to say it did.
   *
   * D6a's ordering, and the reason it is a round trip rather than a fire: the
   * source removes its tab ONLY on this acknowledgement, so an insert that
   * fails leaves the tab exactly where it was instead of deleting it into a
   * window that never took it. `false` is the safe answer everywhere — it means
   * "keep the tab".
   */
  requestMerge(targetWindowId: number, tab: unknown, point: ScreenPointWire): Promise<boolean> {
    const target = this.resolveWindow(targetWindowId);
    if (!target || !target.isAlive()) return Promise.resolve(false);

    const requestId = ++this.seq;
    return new Promise<boolean>((resolve) => {
      const settle = (inserted: boolean) => {
        const entry = this.pending.get(requestId);
        if (!entry) return; // already settled — a late ack, or a second timeout
        this.pending.delete(requestId);
        this.cancelScheduled(entry.timer);
        resolve(inserted);
      };
      const timer = this.schedule(() => this.deferSettle(() => settle(false)), this.ackTimeoutMs);
      this.pending.set(requestId, {
        targetWindowId,
        targetWebContentsId: target.webContentsId,
        timer,
        settle,
      });
      const request: TabDragMergeRequest = {
        requestId,
        tab,
        screenX: point.screenX,
        screenY: point.screenY,
        expiresAt: this.now() + Math.max(0, this.ackTimeoutMs - this.ackGraceMs),
      };
      target.send('tab-drag:merge', request);
    });
  }

  /**
   * A target answered. Returns whether the ack was believed, so `main.ts` can
   * log the ones that were not.
   *
   * An ack is only accepted from the webContents the request was SENT to. The
   * preload exposes `tabDragAckMerge` to every renderer, so without this any
   * window — a launcher, an app window, a second chat window — could answer
   * "inserted" for a request it never received and make the source drop a tab
   * nobody took.
   */
  ackMerge(requestId: number, inserted: boolean, fromWebContentsId: number): boolean {
    const entry = this.pending.get(requestId);
    if (!entry) return false;
    if (entry.targetWebContentsId !== fromWebContentsId) return false;
    entry.settle(inserted);
    return true;
  }

  /**
   * A window is gone — closed, crashed, or its document replaced by a reload.
   *
   * Three obligations, and only the first was met before:
   *
   *  1. it can no longer HOLD a caret;
   *  2. if it was the drag SOURCE, nobody is coming to clear the caret it
   *     caused, so clear it now (D4). This is the whole bug: `tabDragEnd` is
   *     sent by the source renderer, and a source that reloaded or crashed
   *     never sends it, leaving the target painting a hairline indefinitely;
   *  3. any merge waiting on it as a TARGET can never be answered, so settle it
   *     `false` at once rather than making the user watch a 2s stall for a
   *     window that is already gone.
   */
  forgetWindow(windowId: number): void {
    // `settle` does its own removal and timer cancellation, so iterate a copy.
    for (const entry of [...this.pending.values()]) {
      if (entry.targetWindowId === windowId) entry.settle(false);
    }
    if (this.previewWindowId === windowId) {
      // The holder itself is gone: drop the pointer without sending into it.
      this.previewWindowId = null;
    }
    if (this.sourceWindowId === windowId) {
      this.sourceWindowId = null;
      this.clearPreview();
    }
  }

  private sendPreviewOff(windowId: number): void {
    const win = this.resolveWindow(windowId);
    if (win && win.isAlive()) win.send('tab-drag:preview', { active: false });
  }
}
