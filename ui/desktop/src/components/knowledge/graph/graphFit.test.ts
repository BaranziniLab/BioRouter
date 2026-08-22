import { describe, expect, it } from 'vitest';
import { fittedAxisFill, graphFitPadding, NODE_REL_SIZE } from './graphFit';

/**
 * The canvas cannot be rendered in jsdom — force-graph calls
 * `canvas.getContext('2d')` — so the fit is guarded here, at the pure function,
 * which is also where the defect lived: a single padding expression.
 */
describe('graphFitPadding', () => {
  /**
   * The regression this exists to prevent. The old expression was
   * `compact ? 72 : Math.max(112, min(w,h) * 0.16)`, which put the fitted
   * cluster at 68% of the binding axis on a 1400x900 pane and produced a canvas
   * measured at 30–37% full at every size.
   */
  it('leaves the cluster at least 80% of the binding axis on a normal pane', () => {
    for (const [w, h] of [
      [1400, 900],
      [1200, 800],
      [900, 700],
      [1690, 760],
    ] as const) {
      const p = graphFitPadding(w, h, 13.4);
      const fill = fittedAxisFill(Math.min(w, h), p);
      expect(fill, `${w}x${h} padding ${p}`).toBeGreaterThanOrEqual(0.8);
    }
  });

  it('beats the old expression at every size it was measured against', () => {
    const old = (w: number, h: number) =>
      w < 560 || h < 430 ? 72 : Math.max(112, Math.min(w, h) * 0.16);
    for (const [w, h] of [
      [1400, 900],
      [1200, 800],
      [900, 700],
      [520, 400],
    ] as const) {
      expect(graphFitPadding(w, h, 13.4), `${w}x${h}`).toBeLessThan(old(w, h));
    }
  });

  /**
   * force-graph's `getGraphBbox` pads each node by `nodeRelSize`, not by the
   * radius the painter uses, so a bigger hub must buy more padding — otherwise
   * shrinking the base value clips the mark at the pane edge.
   */
  it('pays for the gap between the painted radius and the fit box', () => {
    const small = graphFitPadding(1400, 900, NODE_REL_SIZE);
    const hub = graphFitPadding(1400, 900, 13.4);
    expect(hub).toBeGreaterThan(small);
    expect(hub - small).toBeCloseTo(13.4 - NODE_REL_SIZE, 0);
  });

  it('never returns more than 18% of the smaller dimension, so it cannot drift back', () => {
    for (const [w, h] of [
      [1400, 900],
      [600, 480],
      [3000, 2000],
    ] as const) {
      expect(graphFitPadding(w, h, 40)).toBeLessThanOrEqual(Math.min(w, h) * 0.18);
    }
  });

  it('is tighter on a compact pane, where every pixel is scarcer', () => {
    expect(graphFitPadding(520, 400, 13.4)).toBeLessThan(graphFitPadding(1400, 900, 13.4));
  });
});

describe('fittedAxisFill', () => {
  it('is the identity force-graph itself uses: (D - 2p) / D', () => {
    expect(fittedAxisFill(900, 144)).toBeCloseTo(612 / 900, 6);
    expect(fittedAxisFill(900, 0)).toBe(1);
  });

  it('never goes negative on a padding wider than the pane', () => {
    expect(fittedAxisFill(100, 200)).toBe(0);
  });
});
