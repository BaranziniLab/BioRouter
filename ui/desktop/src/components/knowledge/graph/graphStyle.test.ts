import { describe, expect, it } from 'vitest';
import {
  CANVAS_FONT_FALLBACK,
  CANVAS_INK_FALLBACK,
  resolveCanvasFontFamily,
  resolveCanvasInk,
  withAlpha,
} from './graphStyle';

// The graph paints its labels with `ctx.font`, which is parsed against the
// canvas and NOT against the cascade — so `var(--font-body)` in that string is
// invalid and the whole assignment is silently dropped. The graph therefore has
// to resolve the family itself. The bug this replaces was the other way of
// getting a family into that string: a hardcoded `ui-sans-serif` literal, which
// kept the graph on the OS face after the app moved to Figtree.
describe('resolveCanvasFontFamily', () => {
  it('reads the family off the element rather than naming one', () => {
    const el = document.createElement('div');
    el.style.fontFamily = 'Figtree, ui-sans-serif, sans-serif';
    document.body.appendChild(el);

    expect(resolveCanvasFontFamily(el)).toContain('Figtree');

    el.remove();
  });

  it('never hands a canvas an unresolved custom property', () => {
    const el = document.createElement('div');
    el.style.fontFamily = 'Figtree, sans-serif';
    document.body.appendChild(el);

    // `ctx.font = '12px var(--font-body)'` is not an error — it is a silent
    // no-op that leaves the canvas default in place. Resolving through the
    // computed style is what keeps a `var(` out of the shorthand.
    expect(resolveCanvasFontFamily(el)).not.toContain('var(');

    el.remove();
  });

  it('falls back rather than emitting an empty family', () => {
    expect(resolveCanvasFontFamily(null)).toBe(CANVAS_FONT_FALLBACK);
    // An element outside any document reports no computed family in jsdom; an
    // empty string in `ctx.font` would make the assignment a no-op.
    expect(resolveCanvasFontFamily(document.createElement('div'))).not.toBe('');
  });
});

// The same trap, one property over. `ctx.fillStyle = '#1f242c'` was a hardcoded
// near-black drawn onto a near-black canvas in every dark theme, so the graph's
// labels vanished. jsdom has no layout engine and no themes, so nothing here can
// prove the graph *looks* right — what these pin is the one property that made
// the ink theme-blind: that it is READ from the element, and that a value the
// canvas cannot parse never reaches it.
describe('resolveCanvasInk', () => {
  it('reads the ink off the element rather than naming one', () => {
    const el = document.createElement('div');
    el.style.color = 'rgb(244, 240, 230)';
    document.body.appendChild(el);

    // A light ink — i.e. what a dark theme resolves to. The old literal could
    // only ever return the dark one.
    expect(resolveCanvasInk(el)).toBe('rgb(244, 240, 230)');

    el.remove();
  });

  it('never hands a canvas an unresolved custom property', () => {
    // `--text-default` is itself `var(--color-neutral-100)` in the dark blocks,
    // so reading the custom property by name — the tempting shortcut — yields a
    // reference the canvas silently drops, leaving the previous fill in place.
    const el = document.createElement('div');
    el.style.color = 'var(--text-default)';
    document.body.appendChild(el);

    expect(resolveCanvasInk(el)).not.toContain('var(');

    el.remove();
  });

  it('falls back rather than emitting an empty ink', () => {
    expect(resolveCanvasInk(null)).toBe(CANVAS_INK_FALLBACK);
    expect(resolveCanvasInk(document.createElement('div'))).not.toBe('');
  });
});

describe('withAlpha', () => {
  it('re-alphas a resolved rgb(), which is what getComputedStyle returns', () => {
    expect(withAlpha('rgb(244, 240, 230)', 0.5)).toBe('rgba(244, 240, 230, 0.5)');
    expect(withAlpha('rgba(42, 37, 32, 0.9)', 0.5)).toBe('rgba(42, 37, 32, 0.5)');
  });

  it('handles the hex fallback constant', () => {
    expect(withAlpha('#1f242c', 0.5)).toBe('rgba(31, 36, 44, 0.5)');
    expect(withAlpha('#fff', 0.25)).toBe('rgba(255, 255, 255, 0.25)');
  });

  it('returns an unrecognised colour unchanged rather than mangling it', () => {
    // An invalid `strokeStyle` is a silent no-op that leaves the PREVIOUS
    // stroke in place — the node ring would then inherit a neighbour's colour.
    expect(withAlpha('canvastext', 0.5)).toBe('canvastext');
  });
});
