// ui/desktop/src/components/knowledge/graph/graphModel.ts
import type { Graph, GraphEdge, GraphNode } from '../../../api/types.gen';
import { GRAPH_PALETTE } from '../../../styles/graphPalette';
import { prettyLabel } from './labelText';

/**
 * Everything about a graph that is CONSTANT for its lifetime, computed once.
 *
 * The split matters: the canvas re-paints ~60×/s and force-graph re-enters the
 * node painter once per node per frame, so anything derived here is arithmetic
 * that would otherwise be paid `nodes × 60` times a second for a value that
 * cannot change. DR-9 names the label pass specifically — `prettyLabel` is four
 * regexes and `wrapLabel` was a per-word measure loop, both run for every
 * labelled node every frame.
 */

/** The seven source types §4.6's Source facet lists — and exactly the ones that ring. */
export const SOURCE_TYPES = new Set(['Publication', 'Study', 'Dataset']);

export interface NodeMetrics {
  degree: number;
  radius: number;
  hub: boolean;
  external: boolean;
  /** `prettyLabel` + §5.8's 32-character truncation, computed once. */
  display: string;
  /** The type this node is drawn as, or `null` — DR-28: absence is a real state. */
  type: string | null;
}

export interface GraphModel {
  nodes: Map<string, NodeMetrics>;
  /** Adjacency, for focus dimming and the neighbour label rung. */
  neighbours: Map<string, Set<string>>;
  /** Every distinct `node_type` present, with its count, descending. */
  typeCounts: { type: string; count: number }[];
  /** Every distinct `predicate` present, with its count. */
  predicateCounts: { predicate: string; count: number }[];
  /** Source nodes any edge cites, plus the synthetic "no primary source" bucket. */
  sourceOptions: { id: string; label: string; count: number }[];
  /** Every distinct `status` present. */
  statusValues: string[];
  /** True when NO node carries a `node_type` — a legacy base, drawn untyped. */
  untyped: boolean;
  /**
   * How many nodes carry no `node_type` at all.
   *
   * Counted rather than inferred from `untyped`, because the two answer
   * different questions: `untyped` is "is this a legacy base", this is "how many
   * pages here have no type". A MIXED base makes them disagree, and the Type
   * facet needs the number — its `Untyped` row used to be hardcoded to 0, so on
   * a legacy base it was the one row in the list with no count beside it.
   */
  untypedCount: number;
  /** True when at least one present type is outside the 28-term vocabulary. */
  hasUnrecognisedTypes: boolean;
  hasExternal: boolean;
}

/**
 * §5.6's radius model, replacing the fixed 4.5 / 7.5 pair and `HUB_TOP_N = 6`.
 *
 * ⚠ **The percentile replaces top-N because top-N is SIZE-BLIND**: on a 12-node
 * base it makes half the graph a hub; on a 2,000-node base it makes six. The
 * 82nd percentile is proportional by construction. The same objection kills the
 * fixed radius pair, which encodes no centrality at all — the most connected page
 * in a base looked identical to a leaf.
 */
export function buildGraphModel(graph: Graph): GraphModel {
  const degree = new Map<string, number>();
  const neighbours = new Map<string, Set<string>>();

  const touch = (a: string, b: string) => {
    let set = neighbours.get(a);
    if (!set) {
      set = new Set();
      neighbours.set(a, set);
    }
    set.add(b);
  };

  for (const e of graph.edges) {
    degree.set(e.from, (degree.get(e.from) ?? 0) + 1);
    degree.set(e.to, (degree.get(e.to) ?? 0) + 1);
    touch(e.from, e.to);
    touch(e.to, e.from);
  }

  // `degree` on the wire is authoritative where it exists — the deriver has the
  // deduplicated edge list and the renderer does not — so the local count is a
  // FLOOR under it, never a replacement.
  const deg = (n: GraphNode) => Math.max(degree.get(n.id) ?? 0, n.degree ?? 0);

  const degrees = graph.nodes.map(deg).sort((a, b) => a - b);
  const count = degrees.length;
  const max = Math.max(1, degrees[count - 1] ?? 0);
  const p75 = count > 0 ? degrees[Math.floor((count - 1) * 0.75)] : 0;
  const pivot = Math.max(2, Math.min(Math.max(3, p75), Math.sqrt(max) * 1.6));
  const hubThreshold = Math.max(3, count > 0 ? (degrees[Math.floor((count - 1) * 0.82)] ?? 3) : 3);

  // ⚠ **The VOCABULARY, not a shape map.** This used to read `shapeOf`, which
  // happened to be keyed by every curated type — so deleting the shape channel
  // would have silently broken an unrelated feature (the `Unrecognised type`
  // badge) by making every type look recognised. `types` is the same key set and
  // says what is actually being asked.
  const vocabulary = GRAPH_PALETTE.light.types;

  const nodes = new Map<string, NodeMetrics>();
  const typeCount = new Map<string, number>();
  const statuses = new Set<string>();
  let untyped = true;
  let untypedCount = 0;
  let hasUnrecognisedTypes = false;
  let hasExternal = false;

  for (const n of graph.nodes) {
    const d = deg(n);
    const external = n.external === true;
    if (external) hasExternal = true;
    const centrality = max > 0 ? Math.log1p(d) / Math.log1p(max) : 0;
    let radius = external
      ? clamp(4.5 + 1.4 * centrality, 4.5, 6.2)
      : clamp(5.4 + 7.6 * (1 - Math.exp(-d / pivot)), 5.6, 13.4);
    const hub = !external && d >= hubThreshold;
    if (!Number.isFinite(radius)) radius = external ? 5 : hub ? 10 : 6;

    const type = n.node_type ?? null;
    if (type) {
      untyped = false;
      typeCount.set(type, (typeCount.get(type) ?? 0) + 1);
      if (!(type in vocabulary)) hasUnrecognisedTypes = true;
    } else {
      untypedCount += 1;
    }
    if (n.status) statuses.add(n.status);

    nodes.set(n.id, {
      degree: d,
      radius,
      hub,
      external,
      display: truncateLabel(prettyLabel(n.identifier || n.label, n.kind)),
      type,
    });
  }

  const predicate = new Map<string, number>();
  const source = new Map<string, number>();
  let noSource = 0;
  for (const e of graph.edges) {
    const p = edgePredicate(e);
    if (p) predicate.set(p, (predicate.get(p) ?? 0) + 1);
    if (e.primary_source) source.set(e.primary_source, (source.get(e.primary_source) ?? 0) + 1);
    else noSource += 1;
  }

  const labelOf = (id: string) => nodes.get(id)?.display ?? id;

  return {
    nodes,
    neighbours,
    typeCounts: [...typeCount.entries()]
      .map(([type, c]) => ({ type, count: c }))
      .sort((a, b) => b.count - a.count || a.type.localeCompare(b.type)),
    predicateCounts: [...predicate.entries()]
      .map(([p, c]) => ({ predicate: p, count: c }))
      .sort((a, b) => a.predicate.localeCompare(b.predicate)),
    sourceOptions: [
      ...[...source.entries()]
        .map(([id, c]) => ({ id, label: labelOf(id), count: c }))
        .sort((a, b) => b.count - a.count || a.label.localeCompare(b.label)),
      ...(noSource > 0
        ? [{ id: NO_PRIMARY_SOURCE, label: 'No primary source', count: noSource }]
        : []),
    ],
    statusValues: [...statuses].sort(),
    untyped,
    untypedCount,
    hasUnrecognisedTypes,
    hasExternal,
  };
}

/**
 * The synthetic Source-facet entry for edges that name no `primary_source`.
 *
 * The leading `\u0000` is a COLLISION GUARD, not decoration: this id shares a
 * `Set<string>` and a `sourceOptions` list with real `primary_source` values,
 * which are node ids or verbatim identifiers written by whoever authored the
 * page. `none` on its own is a legal identifier, so a base with a source called
 * `none` would silently take over the 'No primary source' row. NUL cannot appear
 * in a YAML scalar the daemon will parse, so nothing real can spell this.
 *
 * Written as an ESCAPE, never as a raw byte: a literal 0x00 in the source makes
 * the file unreviewable in a diff, invisible to `grep`, and — on macOS, whose
 * BSD `grep` has no `-P` — undetectable by the very command that looks for it.
 */
export const NO_PRIMARY_SOURCE = '\u0000none';

function clamp(v: number, lo: number, hi: number): number {
  return v < lo ? lo : v > hi ? hi : v;
}

/**
 * §5.8's truncation — by CHARACTER COUNT, not width.
 *
 * Width would need `measureText`, which is exactly the per-frame cost DR-9 is
 * about; a character cap is a pure function of the string and therefore
 * memoisable into the model.
 */
export function truncateLabel(text: string): string {
  return text.length > 32 ? `${text.slice(0, 31)}…` : text;
}

/**
 * The edge's predicate.
 *
 * `relation` is the DEPRECATED alias carrying the identical value, kept for one
 * release because it is the only relation field the generated client has ever
 * had. Reading `predicate` first and falling back keeps a graph derived by
 * either generation of the daemon renderable — and returning `null` rather than
 * `''` is what makes "this edge has no type" answerable instead of inferred.
 */
export function edgePredicate(e: GraphEdge): string | null {
  return e.predicate ?? e.relation ?? null;
}

/**
 * A negated predicate spelled out for a human: `not_prevents` → `not prevents`.
 *
 * §5.8 requires the word-level redundancy on the one edge that carries a label,
 * because the dash is the channel that is always present and the word is the
 * confirmation once the user has committed attention.
 */
export function readablePredicate(e: GraphEdge): string {
  const p = edgePredicate(e) ?? 'links to';
  const stripped = p.startsWith('not_') ? p.slice(4) : p;
  return e.negated ? `not ${stripped.replace(/_/g, ' ')}` : stripped.replace(/_/g, ' ');
}

/**
 * True when the edge asserts a NEGATIVE claim.
 *
 * `negated` is the field to read: the deriver emits it explicitly rather than
 * leaving the renderer to infer polarity, precisely because the `not_` prefix is
 * only ONE of two spellings on disk (the other is a legacy `negated: true`
 * attribute) and a renderer that knows only the prefix draws a negative claim as
 * a positive one. The prefix check stays as a belt on the braces — it costs one
 * `startsWith` and it fails in the safe direction.
 */
export function isNegated(e: GraphEdge): boolean {
  if (e.negated === true) return true;
  const p = edgePredicate(e);
  return p != null && p.startsWith('not_');
}

/**
 * §5.5's gate: which nodes show a credibility ring.
 *
 * Written in `node_type`, not `kind`. The pre-OKF `kind` (`source` / `entity` /
 * `concept` / `hub` / `note` / `flag`) survives untouched and is deliberately
 * used by NOTHING in this specification — the three types below are exactly the
 * members of the Provenance family the Source facet lists, so the facet and the
 * ring agree by construction.
 *
 * A source with no tier keeps the neutral separation ring: **absence of a
 * verdict is not a verdict.**
 */
export function showsCredibility(n: GraphNode): boolean {
  return n.credibility_tier != null && n.node_type != null && SOURCE_TYPES.has(n.node_type);
}
