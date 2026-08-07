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
