import { describe, expect, it } from 'vitest';
import { CANVAS_FONT_FALLBACK, resolveCanvasFontFamily } from './graphStyle';

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
