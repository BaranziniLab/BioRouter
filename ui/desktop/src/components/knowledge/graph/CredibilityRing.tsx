// ui/desktop/src/components/knowledge/graph/CredibilityRing.tsx
import { GRAPH_PALETTE } from '../../../styles/graphPalette';
import type { GraphCredibilityKey, GraphMode } from '../../../styles/graphPalette';

/**
 * A node's credibility verdict as a 10px DOM ring (ui-spec §5.5, §4.8 item 3).
 *
 * ⚠ **This is extracted, not new, and the extraction is the point.** The legend
 * already drew this ring as a private `CredibilityGlyph`; §4.8 asks the
 * inspector for "a 10px credibility ring drawn as §5.5 draws it". Two files
 * each drawing a seven-way encoding from the same palette is the identical
 * setup that let the canvas and the inspector disagree about a node's fill —
 * the bug `nodeMark.ts` exists to close. So there is one ring component, the
 * legend and the inspector both call it, and neither can drift.
 *
 * ⚠ **The tier is an ARC COUNT, not a hue** (§5.5.1). A 1.6px stroke subtends
 * 2–3 arcmin, well inside the regime where the visual system reads luminance
 * only, and the seven ring hues collapse to ΔE00 1.13 under tritanopia — `web`
 * versus `retracted` is 3.97 under deuteranopia, and `retracted` is the most
 * important value in the set. Counting is not a colour judgement, so the count
 * survives all of it; hue rides along as the fast channel for trichromats and
 * carries nothing on its own.
 *
 * ⚠ **An arc count, a dashed ring or a solid one — never a filled disc.** The
 * tier hue never sits behind text and nothing anywhere fills a surface with a
 * ring hue, so the centre stays transparent. The seven hues are solved for a
 * 1.6px arc against the graph ground; used as a background under a word they are
 * neither a legible surface nor a passing contrast pair.
 *
 * `aria-hidden` always: the accessible name is the tier WORD, which the badge
 * beside this ring carries. An arc count is a fast channel for someone who can
 * see it and nothing at all for someone who cannot.
 */
export function CredibilityRing({
  tier,
  mode,
  size = 10,
  className,
}: {
  tier: GraphCredibilityKey;
  mode: GraphMode;
  size?: number;
  className?: string;
}) {
  const palette = GRAPH_PALETTE[mode];
  const colour = palette.credibility[tier];
  const treatment = palette.ringArcs[tier];
  const r = 4;
  const circumference = 2 * Math.PI * r;

  if (treatment === 'solid' || treatment === 'dashed') {
    return (
      <svg
        aria-hidden="true"
        width={size}
        height={size}
        viewBox="0 0 10 10"
        className={className}
        style={{ flex: 'none' }}
      >
        <circle
          cx={5}
          cy={5}
          r={r}
          fill="none"
          stroke={colour}
          strokeWidth={1.6}
          // Eight equal dashes. `web` and `personal` are ONE category here —
          // *not academic* — and the inspector's badge is what names which.
          strokeDasharray={
            treatment === 'dashed' ? `${circumference / 16} ${circumference / 16}` : undefined
          }
        />
      </svg>
    );
  }

  const n = treatment;
  // Arcs start at −π/2 (top) and are evenly spaced, each spanning
  // `(2π / N) − gapAngle`. At this radius the canvas's `3 / globalScale / ringR`
  // gap saturates its own `[0.12, 0.5]` clamp, so the constant IS the clamped
  // value rather than a second, looser rule.
  const gap = 0.5;
  const span = (Math.PI * 2) / n - gap;
  const arcs = Array.from({ length: n }, (_, i) => {
    const start = -Math.PI / 2 + ((Math.PI * 2) / n) * i;
    const end = start + span;
    const x0 = 5 + r * Math.cos(start);
    const y0 = 5 + r * Math.sin(start);
    const x1 = 5 + r * Math.cos(end);
    const y1 = 5 + r * Math.sin(end);
    return `M${x0.toFixed(2)},${y0.toFixed(2)}A${r},${r} 0 0 1 ${x1.toFixed(2)},${y1.toFixed(2)}`;
  }).join(' ');

  return (
    <svg
      aria-hidden="true"
      width={size}
      height={size}
      viewBox="0 0 10 10"
      className={className}
      style={{ flex: 'none' }}
    >
      <path d={arcs} fill="none" stroke={colour} strokeWidth={1.6} strokeLinecap="butt" />
    </svg>
  );
}

/**
 * The EXACT tier, spelled for a human.
 *
 * ⚠ **Deliberately not the legend's four labels, and the difference is
 * specified.** The legend names the four treatments the canvas can actually
 * distinguish — "Well sourced", "Weakly sourced", "Not academic", "Retracted" —
 * because §5.5.1's honesty clause is that `web` and `personal` are *not*
 * separable on a 2-arcmin annulus. §5.5.1 then says where the difference does
 * live: "The exact tier is in the inspector and the Source facet." This map is
 * that promise. A legend label here would lose the very distinction the
 * inspector exists to carry.
 */
export const TIER_LABEL: Record<GraphCredibilityKey, string> = {
  peer_reviewed: 'Peer reviewed',
  preprint: 'Preprint',
  book: 'Book',
  gray_lit: 'Grey literature',
  web: 'Web',
  personal: 'Personal communication',
  retracted: 'Retracted',
};
