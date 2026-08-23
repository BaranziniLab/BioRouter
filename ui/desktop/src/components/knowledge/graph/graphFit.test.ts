import { describe, expect, it } from 'vitest';
import { fittedAxisFill, graphFitPadding, NODE_REL_SIZE } from './graphFit';

/**
 * The canvas cannot be rendered in jsdom — force-graph calls
 * `canvas.getContext('2d')` — so the fit is guarded here, at the pure function,
 * which is also where the defect lived: a single padding expression.
 *
 * ⚠ **These are CANVAS boxes, not pane boxes, and the distinction invalidated
 * an earlier version of this file.** `ForceGraphCanvas` calls
 * `graphFitPadding(size.width, size.height, …)` where `size` comes from a
 * ResizeObserver on the canvas wrapper — so the header, the subject band, the
 * filter bar and the card padding are already gone. Measured in a browser: a
 * 1690x760 pane yields an 880x486 canvas, i.e. ~274px of chrome above it.
 *
 * The first fixture set here was pane-sized (900, 800 and 700 tall) and asserted
 * a fill this function never actually produces in the app, because at the app's
 * own window sizes the canvas is 294–800 tall and the binding axis is almost
 * always the SHORT one. A fixture that cannot occur is worse than no fixture: it
 * passes, and it reports a number nobody will see.
 */
const CANVAS_BOXES = [
  [694, 294], // ~600px window, the app's minimum — canvas is compact here
  [694, 486], // 992px viewport / 760px pane, the single-column step
  [880, 486], // 1690px pane, three columns
  [1098, 594], // ~900px window
  [1240, 774], // ~1080px window
] as const;

describe('graphFitPadding', () => {
  /**
   * The regression this exists to prevent. The old expression was
   * `compact ? 72 : Math.max(112, min(w,h) * 0.16)`, and across the canvas boxes
   * above it left the cluster on only 51–68% of the binding axis, which is the
   * 30–37% *area* the canvas measured at.
   *
   * 0.70 is the floor because it is what the shipped function actually
   * guarantees across the real range — 71.4% at the compact end rising to 83.2%
   * at the largest. Asserting 0.80 here would be asserting the best case and
   * calling it the contract.
   */
  it('leaves the cluster at least 70% of the binding axis on every real canvas', () => {
    for (const [w, h] of CANVAS_BOXES) {
      const p = graphFitPadding(w, h, 13.4);
      const fill = fittedAxisFill(Math.min(w, h), p);
      expect(fill, `${w}x${h} padding ${p}`).toBeGreaterThanOrEqual(0.7);
    }
  });

  /** The other half of the contract: the gain is real at every size, not on average. */
  it('gains at least 14 points of fill over the old expression everywhere', () => {
    const old = (w: number, h: number) =>
      w < 560 || h < 430 ? 72 : Math.max(112, Math.min(w, h) * 0.16);
    for (const [w, h] of CANVAS_BOXES) {
      const d = Math.min(w, h);
      const gain = fittedAxisFill(d, graphFitPadding(w, h, 13.4)) - fittedAxisFill(d, old(w, h));
      expect(gain, `${w}x${h}`).toBeGreaterThanOrEqual(0.14);
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
