// ui/desktop/src/components/knowledge/graph/graphFacets.ts
import type { Graph, GraphEdge, GraphNode } from '../../../api/types.gen';
import { edgePredicate, NO_PRIMARY_SOURCE } from './graphModel';

/**
 * The facet state and the one function that applies it (ui-spec §4.6).
 *
 * **Semantics: OR within a facet, AND across facets.** A node that fails takes
 * the search-miss alpha and stays in place — one dimming mechanism, never a
 * second, because "does not match my filter" and "is not related to what I am
 * looking at" are different states and one constant cannot say both.
 *
 * Kept out of the component deliberately: this is set arithmetic over the whole
 * graph, it is the thing a future test can exercise without a canvas, and the
 * §4.6 rule that a Source selection is defined by EDGE INCIDENCE (a node has no
 * `primary_source` in the contract — only an edge does) is exactly the kind of
 * rule that gets quietly re-invented if it lives inside a render.
 */
export interface FacetState {
  search: string;
  types: Set<string>;
  predicates: Set<string>;
  sources: Set<string>;
  statuses: Set<string>;
}

export const EMPTY_FACETS: FacetState = {
  search: '',
  types: new Set(),
  predicates: new Set(),
  sources: new Set(),
  statuses: new Set(),
};

export function facetsActive(f: FacetState): boolean {
  return (
    f.search.trim().length > 0 ||
    f.types.size > 0 ||
    f.predicates.size > 0 ||
    f.sources.size > 0 ||
    f.statuses.size > 0
  );
}

export function activeFacetCount(f: FacetState): number {
  return f.types.size + f.predicates.size + f.sources.size + f.statuses.size;
}

export interface FacetResult {
  /** Node ids that PASS. `null` when no facet is active — "everything", cheaply. */
  nodes: Set<string> | null;
  /** How many nodes pass, for the `Showing N of M` readout. */
  passing: number;
  total: number;
}

/**
 * Apply the facets.
 *
 * ⚠ **The Source facet is defined by INCIDENCE, and the draft's wording was not
 * implementable.** It said "nodes and edges whose `primary_source` resolves to
 * it" — but a node carries no `primary_source` in the §2.1 contract, only an
 * edge does. So selecting a source keeps every edge whose `primary_source`
 * resolves to it and every node INCIDENT to such an edge, which is the only
 * reading under which the node half is defined at all.
 *
 * ⚠ **The Predicate facet is likewise an edge property projected onto nodes.**
 * Selecting `treats` keeps the endpoints of every `treats` edge; it does not
 * keep a node merely because it *could* be treated.
 */
export function applyFacets(graph: Graph, f: FacetState): FacetResult {
  const total = graph.nodes.length;
  if (!facetsActive(f)) return { nodes: null, passing: total, total };

  const needle = f.search.trim().toLowerCase();

  // Text / type / status are node-local; predicate and source are edge-derived.
  const nodeLocal = (n: GraphNode): boolean => {
    if (needle) {
      // `\u0000` joins the four fields so a needle can never match ACROSS a
      // boundary: with a plain separator (or none), searching `gene brca` would
      // match a node whose identifier ends `gene` and whose label starts `brca`,
      // a hit whose reason the user cannot see. NUL cannot occur in any of the
      // four values, so it is the one separator that admits no false positives.
      // Written as an ESCAPE, never a raw byte — see `NO_PRIMARY_SOURCE`.
      const haystack = `${n.identifier ?? ''}\u0000${n.label}\u0000${n.node_type ?? ''}\u0000${
        n.subtype ?? ''
      }`.toLowerCase();
      if (!haystack.includes(needle)) return false;
    }
    if (f.types.size > 0) {
      // A node with NO type can never match a type selection, and that is the
      // right answer rather than an omission: DR-28 makes absence a real state,
      // and `Untyped` is offered as its own facet row so it stays selectable.
      const t = n.node_type ?? UNTYPED_KEY;
      if (!f.types.has(t)) return false;
    }
    if (f.statuses.size > 0) {
      // §5.4 says an absent `status` READS as `stable`, so a `stable` selection
      // has to include the pages that simply do not say so — otherwise the facet
      // would hide most of a base for asserting the default.
      const s = n.status ?? 'stable';
      if (!f.statuses.has(s)) return false;
    }
    return true;
  };

  const edgeIncident = new Set<string>();
  const usesEdgeFacet = f.predicates.size > 0 || f.sources.size > 0;
  if (usesEdgeFacet) {
    for (const e of graph.edges) {
      if (!edgePasses(e, f)) continue;
      edgeIncident.add(e.from);
      edgeIncident.add(e.to);
    }
  }

  const nodes = new Set<string>();
  for (const n of graph.nodes) {
    if (!nodeLocal(n)) continue;
    if (usesEdgeFacet && !edgeIncident.has(n.id)) continue;
    nodes.add(n.id);
  }

  return { nodes, passing: nodes.size, total };
}

/**
 * The facet key a node with no `node_type` is filed under.
 *
 * Same collision guard as `NO_PRIMARY_SOURCE`: this key shares `FacetState.types`
 * with real OKF type strings, and DR-7 forbids rejecting a page over its `type`,
 * so `untyped` is a legal type name a producer may genuinely have written. The
 * `\u0000` prefix is what keeps the synthetic row from swallowing it.
 */
export const UNTYPED_KEY = '\u0000untyped';

function edgePasses(e: GraphEdge, f: FacetState): boolean {
  if (f.predicates.size > 0) {
    const p = edgePredicate(e);
    if (!p || !f.predicates.has(p)) return false;
  }
  if (f.sources.size > 0) {
    const s = e.primary_source ?? NO_PRIMARY_SOURCE;
    if (!f.sources.has(s)) return false;
  }
  return true;
}

/** Immutable toggle, so the component never mutates a `Set` React is holding. */
export function toggle(set: Set<string>, value: string): Set<string> {
  const next = new Set(set);
  if (next.has(value)) next.delete(value);
  else next.add(value);
  return next;
}
