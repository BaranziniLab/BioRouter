// ui/desktop/src/components/knowledge/graph/graphStyle.ts
import type { CredibilityTier } from '../../../api/types.gen';

export const NODE_BASE_RADIUS = 4.5;
export const HUB_RADIUS = 7.5;
export const LABEL_FONT_PX = 11.5;
export const LABEL_FONT_PX_HUB = 12.5;
export const DIMMED_OPACITY = 0.22;

/// Width by source-page credibility tier. The graph uses a single solid-line
/// treatment so edge styling stays visually quiet behind node labels.
export function edgeStyle(tier: CredibilityTier | null | undefined): {
  width: number;
  dash: number[] | null;
} {
  switch (tier) {
    case 'peer_reviewed':
    case 'book':
      return { width: 0.85, dash: null };
    case 'preprint':
      return { width: 0.78, dash: null };
    case 'gray_lit':
      return { width: 0.74, dash: null };
    case 'web':
    case 'personal':
      return { width: 0.68, dash: null };
    default:
      return { width: 0.68, dash: null };
  }
}

/// Top-N degree centrality threshold for "hub" treatment.
export const HUB_TOP_N = 6;

/// The family a `ctx.font` shorthand must name so the graph's labels are drawn
/// in the app's face.
///
/// A canvas cannot read `var(--font-body)` — `ctx.font` is parsed against the
/// canvas element, not the cascade, so a custom property in the string is
/// simply invalid and the assignment is dropped. Every label then paints in the
/// canvas default (`10px sans-serif`), which is why the graph sat on the OS
/// face while the rest of the app moved to Figtree.
///
/// The fix is to RESOLVE the family rather than to hardcode a second literal:
/// the graph container inherits `font-family: var(--font-sans)` from `body`, so
/// its computed style already holds the resolved stack. Hardcoding `'Figtree',
/// …` here would work today and silently rot the first time the token moves.
///
/// `fallback` covers jsdom, where `getComputedStyle` reports no family at all.
export const CANVAS_FONT_FALLBACK = 'ui-sans-serif, system-ui, -apple-system, sans-serif';

export function resolveCanvasFontFamily(el: Element | null | undefined): string {
  if (!el || typeof window === 'undefined' || typeof window.getComputedStyle !== 'function') {
    return CANVAS_FONT_FALLBACK;
  }
  const resolved = window.getComputedStyle(el).fontFamily;
  return resolved && resolved.trim().length > 0 ? resolved : CANVAS_FONT_FALLBACK;
}

/// The ink every canvas glyph and outline is drawn in.
///
/// Same trap as the font family, and it bit the same file: `ctx.fillStyle` is
/// parsed against the canvas, not the cascade, so a `var(--text-default)` in
/// that string is dropped. The label ink was therefore a hardcoded `#1f242c` —
/// a near-black that is correct in a light theme and INVISIBLE in every dark
/// one, where the canvas ground is near-black too. The panel became a set of
/// unlabelled coloured dots.
///
/// The fix is the same as the family's: resolve, do not name. The container
/// inherits `color: var(--text-default)` from `body`, and
/// `getComputedStyle().color` hands back a fully-resolved `rgb(…)`.
///
/// Reading the custom property BY NAME would not work and is the tempting wrong
/// answer: `getPropertyValue('--text-default')` returns the declared value, and
/// in the dark blocks that value is itself `var(--color-neutral-100)` — another
/// reference the canvas cannot resolve. Only the used value is safe here.
export const CANVAS_INK_FALLBACK = '#1f242c';

export function resolveCanvasInk(el: Element | null | undefined): string {
  if (!el || typeof window === 'undefined' || typeof window.getComputedStyle !== 'function') {
    return CANVAS_INK_FALLBACK;
  }
  const resolved = window.getComputedStyle(el).color;
  return resolved && resolved.trim().length > 0 && !resolved.includes('var(')
    ? resolved
    : CANVAS_INK_FALLBACK;
}

/// The node outline alpha. A ring in the label ink separates a node from the
/// ground — and from an overlapping neighbour — in BOTH modes, where the old
/// fixed `rgba(31, 36, 44, 0.5)` only did so on a light one.
export const NODE_RING_ALPHA = 0.5;

/// `color` with `alpha` substituted, for a canvas.
///
/// `getComputedStyle().color` is always a resolved `rgb()`/`rgba()` in a real
/// engine, so the parse below is the whole job; the hex branch covers the
/// fallback constant and any future authored literal. Anything unrecognised is
/// returned unchanged rather than mangled into an invalid colour, which a
/// canvas would silently ignore — leaving the previous fill in place.
export function withAlpha(color: string, alpha: number): string {
  const rgb = /^rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)/i.exec(color);
  if (rgb) {
    return `rgba(${rgb[1]}, ${rgb[2]}, ${rgb[3]}, ${alpha})`;
  }
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(color.trim());
  if (hex) {
    const h = hex[1];
    const full =
      h.length === 3
        ? h
            .split('')
            .map((c) => c + c)
            .join('')
        : h;
    const n = parseInt(full, 16);
    return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${alpha})`;
  }
  return color;
}
