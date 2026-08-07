import { describe, it, expect } from 'vitest';
import {
  StripBandRegistry,
  bandScreenRects,
  clampToWorkArea,
  electronScreenGeometry,
  grabOffsetFromWire,
  intersectRects,
  normalizeToDip,
  rectContains,
  resolveDropTarget,
  resolveDropTargetForRawPoint,
  screenPointFromWire,
  screenPointToWire,
  tornOffWindowBounds,
  tornOffWindowBoundsForRawPoint,
  type Point,
  type Rect,
  type ScreenGeometry,
  type ScreenPointWire,
} from './windowDrag';

const STRIP_HEIGHT = 36;

/** A chat window at `bounds` with one strip across the top of its content. */
function chatWindow(bounds: Rect, bandCount = 1) {
  const bands: Rect[] = [];
  const bandWidth = Math.floor(bounds.width / bandCount);
  for (let i = 0; i < bandCount; i += 1) {
    bands.push({ x: i * bandWidth, y: 0, width: bandWidth, height: STRIP_HEIGHT });
  }
  return { contentBounds: bounds, bands };
}

/** Registers windows front-to-back in call order: the last one registered is on top. */
function registryOf(...windows: Array<[number, ReturnType<typeof chatWindow>]>) {
  const registry = new StripBandRegistry();
  for (const [id, win] of windows) registry.register(id, win);
  return registry;
}

/** A single 1920x1080 display whose work area excludes a 25px menu bar. */
const SINGLE_DISPLAY: Rect = { x: 0, y: 25, width: 1920, height: 1055 };

/**
 * Two displays with different scale factors, which is the only configuration
 * where the D4 DIP conversion is not the identity. The secondary sits to the
 * right at 2x, so its raw screen X doubles past the primary's edge.
 */
function twoDisplayGeometry(): ScreenGeometry {
  const primaryDip: Rect = { x: 0, y: 0, width: 1920, height: 1080 };
  const secondaryDip: Rect = { x: 1920, y: 0, width: 1280, height: 800 };
  const toDip = (point: Point): Point =>
    point.x < primaryDip.width
      ? point
      : { x: primaryDip.width + (point.x - primaryDip.width) / 2, y: point.y / 2 };
  return {
    screenToDip: toDip,
    dipToScreen: (point) =>
      point.x < primaryDip.width
        ? point
        : { x: primaryDip.width + (point.x - primaryDip.width) * 2, y: point.y * 2 },
    workAreaNearest: (point) =>
      point.x >= secondaryDip.x
        ? { x: 1920, y: 0, width: 1280, height: 780 }
        : { x: 0, y: 25, width: 1920, height: 1055 },
  };
}

describe('rectangle primitives', () => {
  it('is half-open on the far edges so abutting windows never both claim a point', () => {
    const left: Rect = { x: 0, y: 0, width: 100, height: 100 };
    const right: Rect = { x: 100, y: 0, width: 100, height: 100 };
    expect(rectContains(left, { x: 0, y: 0 })).toBe(true);
    expect(rectContains(left, { x: 99, y: 99 })).toBe(true);
    expect(rectContains(left, { x: 100, y: 50 })).toBe(false);
    expect(rectContains(right, { x: 100, y: 50 })).toBe(true);
  });

  it('treats a degenerate rectangle as containing nothing', () => {
    expect(rectContains({ x: 0, y: 0, width: 0, height: 10 }, { x: 0, y: 0 })).toBe(false);
    expect(rectContains({ x: 0, y: 0, width: 10, height: -1 }, { x: 0, y: 0 })).toBe(false);
  });

  it('intersects rectangles and reports non-overlap as null', () => {
    expect(
      intersectRects(
        { x: 0, y: 0, width: 100, height: 100 },
        { x: 50, y: 50, width: 100, height: 100 }
      )
    ).toEqual({ x: 50, y: 50, width: 50, height: 50 });
    // Touching edges are not an overlap.
    expect(
      intersectRects(
        { x: 0, y: 0, width: 100, height: 100 },
        { x: 100, y: 0, width: 10, height: 10 }
      )
    ).toBeNull();
  });
});

describe('band geometry', () => {
  it('translates viewport-relative bands into screen space', () => {
    const win = chatWindow({ x: 300, y: 200, width: 800, height: 600 });
    expect(bandScreenRects(win)).toEqual([{ x: 300, y: 200, width: 800, height: STRIP_HEIGHT }]);
  });

  it('reports one band per strip for a split window', () => {
    const win = chatWindow({ x: 0, y: 0, width: 800, height: 600 }, 2);
    expect(bandScreenRects(win)).toEqual([
      { x: 0, y: 0, width: 400, height: STRIP_HEIGHT },
      { x: 400, y: 0, width: 400, height: STRIP_HEIGHT },
    ]);
  });

  it('clips a stale band to the content rect so it cannot claim screen the window lost', () => {
    // Measured when the window was 800 wide, reported after it shrank to 400.
    const stale = {
      contentBounds: { x: 0, y: 0, width: 400, height: 600 },
      bands: [{ x: 0, y: 0, width: 800, height: STRIP_HEIGHT }],
    };
    expect(bandScreenRects(stale)).toEqual([{ x: 0, y: 0, width: 400, height: STRIP_HEIGHT }]);
  });

  it('drops a band that no longer intersects the window at all', () => {
    const stale = {
      contentBounds: { x: 0, y: 0, width: 400, height: 600 },
      bands: [{ x: 900, y: 0, width: 200, height: STRIP_HEIGHT }],
    };
    expect(bandScreenRects(stale)).toEqual([]);
  });
});

describe('resolveDropTarget', () => {
  const source = chatWindow({ x: 0, y: 0, width: 800, height: 600 });
  const other = chatWindow({ x: 900, y: 100, width: 800, height: 600 });

  it('answers local for a point over the source window itself (D6b)', () => {
    const registry = registryOf([1, source], [2, other]);
    expect(resolveDropTarget({ x: 400, y: 300 }, registry, 1)).toEqual({ kind: 'local' });
    // Including over the source window's own strip: that is an ordinary reorder.
    expect(resolveDropTarget({ x: 400, y: 10 }, registry, 1)).toEqual({ kind: 'local' });
  });

  it('answers merge for a point inside another window strip band', () => {
    const registry = registryOf([1, source], [2, other]);
    expect(resolveDropTarget({ x: 1000, y: 120 }, registry, 1)).toEqual({
      kind: 'merge',
      targetWindowId: 2,
    });
  });

  it('answers merge for the second strip of a split window', () => {
    const split = chatWindow({ x: 900, y: 100, width: 800, height: 600 }, 2);
    const registry = registryOf([1, source], [2, split]);
    expect(resolveDropTarget({ x: 1500, y: 110 }, registry, 1)).toEqual({
      kind: 'merge',
      targetWindowId: 2,
    });
  });

  it('answers detach for a point in another window body, not merge', () => {
    const registry = registryOf([1, source], [2, other]);
    expect(resolveDropTarget({ x: 1000, y: 400 }, registry, 1)).toEqual({ kind: 'detach' });
  });

  it('answers detach over empty desktop and over unregistered windows', () => {
    const registry = registryOf([1, source], [2, other]);
    expect(resolveDropTarget({ x: 2500, y: 900 }, registry, 1)).toEqual({ kind: 'detach' });
    expect(resolveDropTarget({ x: -40, y: -40 }, registry, 1)).toEqual({ kind: 'detach' });
  });

  it('answers detach when nothing at all is registered', () => {
    expect(resolveDropTarget({ x: 10, y: 10 }, new StripBandRegistry(), 1)).toEqual({
      kind: 'detach',
    });
  });

  describe('overlapping windows and z-order', () => {
    // Two windows stacked at the same origin, so every point is contested.
    const back = chatWindow({ x: 200, y: 200, width: 600, height: 400 });
    const front = chatWindow({ x: 200, y: 200, width: 600, height: 400 });

    it('gives the topmost window the point when two bands overlap', () => {
      const registry = registryOf([1, chatWindow({ x: 0, y: 700, width: 400, height: 300 })]);
      registry.register(2, back);
      registry.register(3, front); // registered last ⇒ frontmost
      expect(resolveDropTarget({ x: 300, y: 210 }, registry, 1)).toEqual({
        kind: 'merge',
        targetWindowId: 3,
      });
      registry.raise(2);
      expect(resolveDropTarget({ x: 300, y: 210 }, registry, 1)).toEqual({
        kind: 'merge',
        targetWindowId: 2,
      });
    });

    it('does not merge into a band that a nearer window body covers', () => {
      // The front window's body sits over the back window's strip.
      const backWin = chatWindow({ x: 200, y: 200, width: 600, height: 400 });
      const frontWin = chatWindow({ x: 200, y: 150, width: 600, height: 400 });
      const registry = registryOf([1, chatWindow({ x: 0, y: 700, width: 400, height: 300 })]);
      registry.register(2, backWin);
      registry.register(3, frontWin);
      // (300, 210) is inside window 2's strip, but window 3's body is on top of it.
      expect(rectContains(bandScreenRects(backWin)[0], { x: 300, y: 210 })).toBe(true);
      expect(resolveDropTarget({ x: 300, y: 210 }, registry, 3)).toEqual({ kind: 'local' });
      expect(resolveDropTarget({ x: 300, y: 210 }, registry, 1)).toEqual({ kind: 'detach' });
    });

    it('lets the source window occlude a band behind it', () => {
      const registry = registryOf([2, back]);
      registry.register(1, front); // source, frontmost
      expect(resolveDropTarget({ x: 300, y: 210 }, registry, 1)).toEqual({ kind: 'local' });
    });

    it('resolves the point to the window behind once the front one is raised away', () => {
      const registry = registryOf([2, back], [3, front]);
      // Front window moved off the contested point entirely.
      registry.register(3, chatWindow({ x: 1200, y: 200, width: 600, height: 400 }));
      expect(resolveDropTarget({ x: 300, y: 210 }, registry, 1)).toEqual({
        kind: 'merge',
        targetWindowId: 2,
      });
    });
  });

  describe('registry lifecycle', () => {
    it('stops offering a de-registered window and stops it occluding', () => {
      const back = chatWindow({ x: 200, y: 200, width: 600, height: 400 });
      const front = chatWindow({ x: 200, y: 150, width: 600, height: 400 });
      const registry = registryOf([2, back], [3, front]);
      expect(resolveDropTarget({ x: 300, y: 210 }, registry, 1)).toEqual({ kind: 'detach' });

      // Window 3 closes mid-drag (§6): the next move resolves to what is behind it.
      expect(registry.remove(3)).toBe(true);
      expect(registry.remove(3)).toBe(false);
      expect(resolveDropTarget({ x: 300, y: 210 }, registry, 1)).toEqual({
        kind: 'merge',
        targetWindowId: 2,
      });

      registry.remove(2);
      expect(registry.size).toBe(0);
      expect(resolveDropTarget({ x: 300, y: 210 }, registry, 1)).toEqual({ kind: 'detach' });
    });

    it('a hidden window is neither a target nor an occluder', () => {
      const back = chatWindow({ x: 200, y: 200, width: 600, height: 400 });
      const front = chatWindow({ x: 200, y: 150, width: 600, height: 400 });
      const registry = registryOf([2, back], [3, front]);

      registry.setHidden(3, true);
      expect(resolveDropTarget({ x: 300, y: 210 }, registry, 1)).toEqual({
        kind: 'merge',
        targetWindowId: 2,
      });
      registry.setHidden(2, true);
      expect(resolveDropTarget({ x: 300, y: 210 }, registry, 1)).toEqual({ kind: 'detach' });

      registry.setHidden(3, false);
      expect(resolveDropTarget({ x: 300, y: 210 }, registry, 1)).toEqual({ kind: 'detach' });
      expect(resolveDropTarget({ x: 300, y: 160 }, registry, 1)).toEqual({
        kind: 'merge',
        targetWindowId: 3,
      });
    });

    it('a resize re-registration keeps stack position and hidden state', () => {
      const registry = registryOf(
        [2, chatWindow({ x: 0, y: 0, width: 600, height: 400 })],
        [3, chatWindow({ x: 0, y: 0, width: 600, height: 400 })]
      );
      registry.setHidden(2, true);
      registry.register(2, chatWindow({ x: 0, y: 0, width: 700, height: 400 }));
      expect(registry.get(2)?.hidden).toBe(true);
      // Still behind 3, so reporting a resize did not raise it.
      registry.setHidden(2, false);
      expect(registry.stackedFrontToBack().map((entry) => entry.windowId)).toEqual([3, 2]);
    });

    it('ignores raise/hide for a window it does not know', () => {
      const registry = new StripBandRegistry();
      expect(() => registry.raise(99)).not.toThrow();
      expect(() => registry.setHidden(99, true)).not.toThrow();
      expect(registry.has(99)).toBe(false);
    });

    it('copies the rectangles it is given so a later mutation cannot corrupt it', () => {
      const bounds: Rect = { x: 0, y: 0, width: 600, height: 400 };
      const band: Rect = { x: 0, y: 0, width: 600, height: STRIP_HEIGHT };
      const registry = new StripBandRegistry();
      registry.register(2, { contentBounds: bounds, bands: [band] });
      bounds.x = 5000;
      band.height = 1;
      expect(registry.get(2)?.contentBounds.x).toBe(0);
      expect(registry.get(2)?.bands[0].height).toBe(STRIP_HEIGHT);
    });
  });
});

describe('tornOffWindowBounds', () => {
  const sourceBounds: Rect = { x: 100, y: 100, width: 800, height: 600 };
  const grab: Point = { x: 60, y: 12 };

  it('puts the grabbed point of the tab under the cursor', () => {
    expect(tornOffWindowBounds({ x: 700, y: 400 }, grab, sourceBounds, SINGLE_DISPLAY)).toEqual({
      x: 640,
      y: 388,
      width: 800,
      height: 600,
    });
  });

  it('copies the source window size', () => {
    const bounds = tornOffWindowBounds({ x: 700, y: 400 }, grab, sourceBounds, SINGLE_DISPLAY);
    expect(bounds.width).toBe(sourceBounds.width);
    expect(bounds.height).toBe(sourceBounds.height);
  });

  it('clamps at the left edge', () => {
    const bounds = tornOffWindowBounds({ x: 10, y: 400 }, grab, sourceBounds, SINGLE_DISPLAY);
    expect(bounds.x).toBe(SINGLE_DISPLAY.x);
  });

  it('clamps at the top edge, respecting a work area that starts below zero', () => {
    const bounds = tornOffWindowBounds({ x: 700, y: 4 }, grab, sourceBounds, SINGLE_DISPLAY);
    expect(bounds.y).toBe(SINGLE_DISPLAY.y);
  });

  it('clamps at the right edge', () => {
    const bounds = tornOffWindowBounds({ x: 1910, y: 400 }, grab, sourceBounds, SINGLE_DISPLAY);
    expect(bounds.x).toBe(SINGLE_DISPLAY.x + SINGLE_DISPLAY.width - sourceBounds.width);
    expect(bounds.x + bounds.width).toBe(SINGLE_DISPLAY.x + SINGLE_DISPLAY.width);
  });

  it('clamps at the bottom edge', () => {
    const bounds = tornOffWindowBounds({ x: 700, y: 1075 }, grab, sourceBounds, SINGLE_DISPLAY);
    expect(bounds.y + bounds.height).toBe(SINGLE_DISPLAY.y + SINGLE_DISPLAY.height);
  });

  it('clamps both axes at a corner', () => {
    const bounds = tornOffWindowBounds({ x: 1919, y: 1079 }, grab, sourceBounds, SINGLE_DISPLAY);
    expect(bounds).toEqual({
      x: SINGLE_DISPLAY.x + SINGLE_DISPLAY.width - 800,
      y: SINGLE_DISPLAY.y + SINGLE_DISPLAY.height - 600,
      width: 800,
      height: 600,
    });
  });

  it('caps a window larger than the work area, exactly as branchWindowBounds does', () => {
    const huge: Rect = { x: 0, y: 0, width: 4000, height: 3000 };
    const bounds = tornOffWindowBounds({ x: 700, y: 400 }, grab, huge, SINGLE_DISPLAY);
    expect(bounds).toEqual(SINGLE_DISPLAY);
  });

  it('rounds to whole pixels — Electron bounds are integers', () => {
    const bounds = tornOffWindowBounds({ x: 700.4, y: 400.6 }, grab, sourceBounds, SINGLE_DISPLAY);
    expect(Number.isInteger(bounds.x)).toBe(true);
    expect(Number.isInteger(bounds.y)).toBe(true);
  });
});

describe('clampToWorkArea', () => {
  it('leaves a rectangle already inside untouched', () => {
    const rect: Rect = { x: 300, y: 300, width: 400, height: 300 };
    expect(clampToWorkArea(rect, SINGLE_DISPLAY)).toEqual(rect);
  });

  it('works on a work area with a negative origin (a display left of the primary)', () => {
    const left: Rect = { x: -1920, y: 0, width: 1920, height: 1080 };
    expect(clampToWorkArea({ x: -3000, y: -500, width: 800, height: 600 }, left)).toEqual({
      x: -1920,
      y: 0,
      width: 800,
      height: 600,
    });
  });
});

describe('DIP normalisation and the screen shim (D4)', () => {
  it('is the identity on a single 1x display', () => {
    const geometry = twoDisplayGeometry();
    expect(normalizeToDip({ x: 640, y: 480 }, geometry)).toEqual({ x: 640, y: 480 });
  });

  it('converts a raw point on a 2x secondary display before hit-testing', () => {
    const geometry = twoDisplayGeometry();
    // A window whose DIP content rect is on the secondary display.
    const secondary = chatWindow({ x: 1920, y: 0, width: 1280, height: 800 });
    const registry = registryOf(
      [1, chatWindow({ x: 0, y: 0, width: 800, height: 600 })],
      [2, secondary]
    );

    // Raw coordinates: 1920 + (100 * 2) across, 20 * 2 down.
    const raw: Point = { x: 2120, y: 40 };
    expect(normalizeToDip(raw, geometry)).toEqual({ x: 2020, y: 20 });
    expect(resolveDropTargetForRawPoint(raw, geometry, registry, 1)).toEqual({
      kind: 'merge',
      targetWindowId: 2,
    });
    // Without the conversion the same raw point misses the window entirely,
    // which is the bug D4 exists to prevent.
    expect(resolveDropTarget(raw, registry, 1)).toEqual({ kind: 'detach' });
  });

  it('places a torn-off window on the display the drop landed on', () => {
    const geometry = twoDisplayGeometry();
    const bounds = tornOffWindowBoundsForRawPoint(
      { x: 2120, y: 40 },
      { x: 60, y: 12 },
      { x: 0, y: 0, width: 800, height: 600 },
      geometry
    );
    // Secondary work area is { x: 1920, y: 0, w: 1280, h: 780 }.
    expect(bounds).toEqual({ x: 1960, y: 8, width: 800, height: 600 });
  });

  it('clamps into the secondary display work area, not the primary one', () => {
    const geometry = twoDisplayGeometry();
    const bounds = tornOffWindowBoundsForRawPoint(
      { x: 4478, y: 1596 }, // DIP (3199, 798): the far corner of the secondary
      { x: 0, y: 0 },
      { x: 0, y: 0, width: 800, height: 600 },
      geometry
    );
    expect(bounds).toEqual({ x: 1920 + 1280 - 800, y: 780 - 600, width: 800, height: 600 });
  });

  it('adapts the real screen module through the shim', () => {
    const calls: string[] = [];
    const geometry = electronScreenGeometry({
      screenToDipPoint: (point) => {
        calls.push('screenToDipPoint');
        return { x: point.x / 2, y: point.y / 2 };
      },
      dipToScreenPoint: (point) => {
        calls.push('dipToScreenPoint');
        return { x: point.x * 2, y: point.y * 2 };
      },
      getDisplayNearestPoint: (point) => {
        calls.push('getDisplayNearestPoint');
        return { workArea: { x: 0, y: 0, width: point.x + 100, height: 900 } };
      },
    });
    expect(geometry.screenToDip({ x: 100, y: 200 })).toEqual({ x: 50, y: 100 });
    expect(geometry.dipToScreen({ x: 50, y: 100 })).toEqual({ x: 100, y: 200 });
    expect(geometry.workAreaNearest({ x: 100, y: 0 }).width).toBe(200);
    expect(calls).toEqual(['screenToDipPoint', 'dipToScreenPoint', 'getDisplayNearestPoint']);
  });
});

/**
 * THE macOS `screen` MODULE, WHICH IS THE ONE THIS APP SHIPS ON MOST.
 *
 * Every geometry test above hands `electronScreenGeometry` a literal with all
 * three methods present, so none of them could see that the real macOS module
 * has only one. `screenToDipPoint`/`dipToScreenPoint` are `@platform
 * win32,linux` and are compiled out of the darwin build — the strings do not
 * even appear in `Electron Framework` on this machine, while
 * `getDisplayNearestPoint` does.
 *
 * These cases model that module by OMITTING the two methods, which is the only
 * faithful stand-in and the thing the stub above cannot be.
 */
function macScreenModule(workArea: Rect = SINGLE_DISPLAY) {
  // Deliberately not `screenToDipPoint: undefined` — the property is ABSENT.
  return { getDisplayNearestPoint: () => ({ workArea }) };
}

describe('electronScreenGeometry — macOS, where the DIP converters do not exist', () => {
  it('does not throw normalising a point: identity, not a missing method', () => {
    const geometry = electronScreenGeometry(macScreenModule());
    expect(() => normalizeToDip({ x: 640, y: 26 }, geometry)).not.toThrow();
    expect(normalizeToDip({ x: 640, y: 26 }, geometry)).toEqual({ x: 640, y: 26 });
    expect(geometry.dipToScreen({ x: 640, y: 26 })).toEqual({ x: 640, y: 26 });
  });

  it('resolves a real merge instead of dying in the ipcMain listener', () => {
    const geometry = electronScreenGeometry(macScreenModule());
    const registry = registryOf([7, chatWindow({ x: 0, y: 0, width: 1000, height: 800 })]);
    expect(
      resolveDropTargetForRawPoint({ x: 500, y: 18 }, geometry, registry, 99)
    ).toEqual({ kind: 'merge', targetWindowId: 7 });
  });

  it('places a torn-off window at the drop point rather than at NaN', () => {
    const geometry = electronScreenGeometry(macScreenModule());
    const bounds = tornOffWindowBoundsForRawPoint(
      { x: 700, y: 300 },
      { x: 40, y: 12 },
      { x: 0, y: 0, width: 800, height: 600 },
      geometry
    );
    expect(bounds).toEqual({ x: 660, y: 288, width: 800, height: 600 });
  });

  it('still uses the native converters wherever they DO exist', () => {
    // The same assertion from the other side: the fallback must not shadow a
    // platform that really can scale, or Windows per-monitor DPI silently
    // regresses to the bug this fallback is fixing on macOS.
    const geometry = electronScreenGeometry({
      screenToDipPoint: (point) => ({ x: point.x / 2, y: point.y / 2 }),
      dipToScreenPoint: (point) => ({ x: point.x * 2, y: point.y * 2 }),
      getDisplayNearestPoint: () => ({ workArea: SINGLE_DISPLAY }),
    });
    expect(normalizeToDip({ x: 640, y: 26 }, geometry)).toEqual({ x: 320, y: 13 });
  });
});

/**
 * THE IPC SEAM — `{screenX, screenY}` IN, `{x, y}` OUT.
 *
 * The wire shape and the geometry shape share no field name, and an `ipcMain`
 * listener's payload is `any`, so main handed one straight to the other and
 * `tsc` was silent. `undefined >= rect.x` is false for every rectangle, so every
 * drop resolved to `detach` and a merge was unreachable on all three platforms.
 *
 * Each case below feeds the EXACT object literal `ChatGroupsShell` puts on the
 * wire, not a hand-written `{x, y}`.
 */
describe('screenPointFromWire — the shape the renderer actually sends', () => {
  const geometry = electronScreenGeometry(macScreenModule());
  /** Byte for byte what ChatGroupsShell sends (`{ screenX, screenY }`). */
  const wirePoint: unknown = { screenX: 500, screenY: 18 };

  it('converts the wire point into geometry space', () => {
    expect(screenPointFromWire(wirePoint)).toEqual({ x: 500, y: 18 });
  });

  it('a wire point converted at the boundary resolves as a MERGE', () => {
    const registry = registryOf([7, chatWindow({ x: 0, y: 0, width: 1000, height: 800 })]);
    const converted = screenPointFromWire(wirePoint);
    expect(converted).not.toBeNull();
    expect(resolveDropTargetForRawPoint(converted!, geometry, registry, 99)).toEqual({
      kind: 'merge',
      targetWindowId: 7,
    });
  });

  it('a wire point converted at the boundary gives finite torn-off bounds', () => {
    const bounds = tornOffWindowBoundsForRawPoint(
      screenPointFromWire({ screenX: 700, screenY: 300 })!,
      grabOffsetFromWire({ x: 40, y: 12 }),
      { x: 0, y: 0, width: 800, height: 600 },
      geometry
    );
    expect(Number.isFinite(bounds.x)).toBe(true);
    expect(Number.isFinite(bounds.y)).toBe(true);
    expect(bounds).toEqual({ x: 660, y: 288, width: 800, height: 600 });
  });

  it('refuses a payload that is not two finite numbers', () => {
    expect(screenPointFromWire(undefined)).toBeNull();
    expect(screenPointFromWire(null)).toBeNull();
    expect(screenPointFromWire({})).toBeNull();
    // A geometry point is NOT a wire point. Rejecting it is the whole guarantee:
    // if the two shapes were interchangeable the seam would still be open.
    expect(screenPointFromWire({ x: 1, y: 2 })).toBeNull();
    expect(screenPointFromWire({ screenX: 1 })).toBeNull();
    expect(screenPointFromWire({ screenX: NaN, screenY: 2 })).toBeNull();
    expect(screenPointFromWire({ screenX: 1, screenY: Infinity })).toBeNull();
    expect(screenPointFromWire({ screenX: '1', screenY: '2' })).toBeNull();
  });

  it('round-trips back to the wire for the messages a target renderer reads', () => {
    expect(screenPointToWire({ x: 500, y: 18 })).toEqual({ screenX: 500, screenY: 18 });
  });

  it('falls the grab offset back to the origin rather than poisoning the bounds', () => {
    expect(grabOffsetFromWire({ x: 4, y: 9 })).toEqual({ x: 4, y: 9 });
    expect(grabOffsetFromWire(undefined)).toEqual({ x: 0, y: 0 });
    expect(grabOffsetFromWire({ x: NaN, y: 9 })).toEqual({ x: 0, y: 0 });
    // The wire POINT shape is not an offset either — same trap, other direction.
    expect(grabOffsetFromWire({ screenX: 4, screenY: 9 })).toEqual({ x: 0, y: 0 });
  });

  it('keeps the two shapes structurally incompatible (compile-time)', () => {
    // `tsc --noEmit` runs in `lint:check`. If anyone ever widens `Point` to
    // accept the wire fields — the change that would quietly reopen this seam —
    // the expected error disappears and `@ts-expect-error` becomes the failure.
    const wire: ScreenPointWire = { screenX: 1, screenY: 2 };
    // @ts-expect-error a wire point is not a geometry Point and must never be
    const notAPoint: Point = wire;
    expect(notAPoint).toBe(wire);
  });
});
