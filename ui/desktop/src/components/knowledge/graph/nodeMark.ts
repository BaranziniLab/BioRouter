// ui/desktop/src/components/knowledge/graph/nodeMark.ts
import type { GraphNode } from '../../../api/types.gen';
import { GRAPH_PALETTE, hashedFill, typeFill } from '../../../styles/graphPalette';
import type { GraphCredibilityKey, GraphMode } from '../../../styles/graphPalette';
import { showsCredibility } from './graphModel';

/**
 * A node's MARK — the fill and the silhouette — in exactly one place.
 *
 * ⚠ **This module exists because two surfaces disagreed about the same node.**
 * The canvas painted a node from `node_type` through `GRAPH_PALETTE`; the
 * inspector painted it from the legacy `kind` field through `credColors.ts`.
 * The deriver writes `kind: 'hub'` for every typed page, so in an OKF or BioOKF
 * base the inspector showed the identical gold swatch and the word "hub" for
 * every node in the base, while the canvas beside it drew Metformin as a teal
 * diamond. Two readers of one node cannot be kept in step by convention, so
 * they now read one function.
 *
 * ⚠ **`node_type` absent is a REAL state (DR-28), not a missing value.** Every
 * page in a legacy base has no type, so the fallback has to be a first-class
 * rendering rather than an invented type. It is keyed on the legacy `kind`
 * field — the only identity such a page has — but resolved through DR-11's
 * hash rather than `credColors.ts`'s six literals: the hash is deterministic,
 * solved against the same ground the 28 curated fills were solved against, and
 * has a dark-mode value, which those literals never did.
 */

/** The fill for a node: curated by type, hashed by type, or hashed by legacy `kind`. */
export function fillFor(n: GraphNode, mode: GraphMode): string {
  return n.node_type ? typeFill(n.node_type, mode) : hashedFill(n.kind, mode);
}

/**
 * The name of the family whose members are drawn HOLLOW.
 *
 * ⚠ **This is the redundant channel that survives the all-circle default**, and
 * it is keyed to the vocabulary's own top-level division rather than invented:
 * BioOKF splits its 28 types into 20 Biomedical Entities — the science — and 8
 * Provenance & Context types, which are where the science came from. Drawing
 * the second group as open rings says exactly that, stays a circle, and
 * survives monochrome, which is more than hue can claim here.
 *
 * Matched by NAME against the generated palette rather than by a hardcoded list
 * of eight type strings, so a vocabulary change moves this with it.
 */
const HOLLOW_FAMILY = 'Provenance & context';

/**
 * Whether a node is drawn as an open ring rather than a filled disc.
 *
 * Untyped nodes are never hollow: in a legacy base every page would be, and a
 * marker that applies to every node carries nothing — the same argument
 * `hashedFill` makes for not giving OKF nodes a marker.
 */
export function isHollow(n: GraphNode, mode: GraphMode): boolean {
  return !!n.node_type && isHollowType(n.node_type, mode);
}

/**
 * The same question for a bare type string, which is what the LEGEND has.
 *
 * The legend must draw the hollow variant for exactly the types the canvas
 * draws hollow — a key that disagrees with the mark teaches the wrong thing,
 * which is the whole reason this module exists.
 */
export function isHollowType(type: string, mode: GraphMode): boolean {
  const family = GRAPH_PALETTE[mode].families[HOLLOW_FAMILY];
  return !!family && family.members.includes(type);
}

/**
 * The credibility key a node rings in, or `null` when it shows no orbit ring.
 *
 * ⚠ **Third reader of one node, same rule as the fill.** This lived privately in
 * `ForceGraphCanvas.tsx` while the inspector showed the tier as a bare word and
 * the legend drew its own ring — so "does this node ring, and in what?" had one
 * implementation and two surfaces guessing around it. It sits beside `fillFor`
 * for the same reason `fillFor` exists.
 *
 * ⚠ **Retraction OVERRIDES the tier.** It is a flag rather than a rung, and it
 * is the more important fact about a source: a retracted peer-reviewed paper
 * rings `retracted`, not `peer_reviewed`.
 *
 * ⚠ **A tier alone does not earn a ring** (§5.5). The gate is `showsCredibility`
 * — written in `node_type`, over the three Provenance-family source types — so
 * the ring and §4.6's Source facet agree by construction. A non-source page that
 * somehow carries a tier gets `null` here, and the inspector then shows the
 * tier's WORD with no ring, which is exactly what the canvas shows: nothing.
 */
export function credibilityKey(n: GraphNode): GraphCredibilityKey | null {
  if (n.retracted) return 'retracted';
  return showsCredibility(n) ? (n.credibility_tier as GraphCredibilityKey) : null;
}
