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
