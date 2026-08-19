import { describe, expect, it } from 'vitest';
import type { Graph, GraphEdge, GraphNode } from '../../../api/types.gen';
import {
  buildGraphModel,
  edgePredicate,
  isNegated,
  NO_PRIMARY_SOURCE,
  readablePredicate,
  showsCredibility,
  truncateLabel,
} from './graphModel';
import { applyFacets, EMPTY_FACETS, UNTYPED_KEY } from './graphFacets';
import type { FacetState } from './graphFacets';

function node(id: string, extra: Partial<GraphNode> = {}): GraphNode {
  return { id, kind: 'entity', label: id, path: `knowledge/${id}.md`, ...extra };
}

function edge(from: string, to: string, extra: Partial<GraphEdge> = {}): GraphEdge {
  return { from, to, ...extra };
}

function graph(nodes: GraphNode[], edges: GraphEdge[] = []): Graph {
  return { nodes, edges };
}

function facets(over: Partial<FacetState> = {}): FacetState {
  return { ...EMPTY_FACETS, ...over };
}

/**
 * ⚠ **The two synthetic facet keys carry a leading NUL, and it is a COLLISION
 * GUARD rather than a formatting quirk.** Both share a `Set<string>` with values
 * a producer wrote: `NO_PRIMARY_SOURCE` with real `primary_source` identifiers,
 * `UNTYPED_KEY` with real OKF `type` strings that DR-7 forbids rejecting. `none`
 * and `untyped` are both legal things to write, so a bare sentinel would let a
 * real value take over its synthetic row.
 *
 * They were literal 0x00 BYTES in the source, which made the files unreviewable
 * in a diff and invisible to `grep` — and on macOS, whose BSD `grep` has no
 * `-P`, invisible to the very command that looks for them. They are escapes
 * now. These cases pin the VALUE, so the escape cannot be quietly "tidied" into
 * a plain string that reintroduces the collision.
 */
describe('the synthetic facet sentinels', () => {
  it('cannot be spelled by anything a producer could write', () => {
    expect(NO_PRIMARY_SOURCE).toBe('\u0000none');
    expect(UNTYPED_KEY).toBe('\u0000untyped');
    expect(NO_PRIMARY_SOURCE).not.toBe('none');
    expect(UNTYPED_KEY).not.toBe('untyped');
  });

  it('does not swallow a real source literally named "none"', () => {
    const g = graph(
      [node('none', { node_type: 'Publication' }), node('a'), node('b')],
      [edge('a', 'b', { primary_source: 'none' }), edge('a', 'none')]
    );
    // Selecting the REAL source keeps the edge that cites it…
    expect(applyFacets(g, facets({ sources: new Set(['none']) })).passing).toBe(2);
    // …and selecting the synthetic bucket keeps a different edge entirely.
    const synthetic = applyFacets(g, facets({ sources: new Set([NO_PRIMARY_SOURCE]) }));
    expect(synthetic.nodes).toEqual(new Set(['a', 'none']));
  });

  it('does not swallow a real type literally named "untyped"', () => {
    const g = graph([node('x', { node_type: 'untyped' }), node('y')]);
    expect(applyFacets(g, facets({ types: new Set(['untyped']) })).nodes).toEqual(new Set(['x']));
    expect(applyFacets(g, facets({ types: new Set([UNTYPED_KEY]) })).nodes).toEqual(new Set(['y']));
  });
});

describe('the search haystack', () => {
  // The four fields are joined with the same NUL, so a needle can never match
  // ACROSS a boundary. With a plain separator (or none) `gene brca` would match
  // a node whose identifier ends `gene` and whose label starts `brca` — a hit
  // the user cannot see the reason for.
  it('never matches across a field boundary', () => {
    const g = graph([node('n1', { identifier: 'gene', label: 'brca', node_type: 'Gene' })]);
    expect(applyFacets(g, facets({ search: 'gene' })).passing).toBe(1);
    expect(applyFacets(g, facets({ search: 'brca' })).passing).toBe(1);
    expect(applyFacets(g, facets({ search: 'genebrca' })).passing).toBe(0);
  });

  it('matches identifier, label, type and subtype, case-insensitively', () => {
    const g = graph([
      node('n1', { identifier: 'IL6', label: 'Interleukin 6', node_type: 'Gene', subtype: 'pc' }),
    ]);
    for (const needle of ['il6', 'interleukin', 'gene', 'pc']) {
      expect(applyFacets(g, facets({ search: needle })).passing).toBe(1);
    }
  });
});

describe('applyFacets', () => {
  it('returns a null node set when nothing is active — "everything", cheaply', () => {
    const g = graph([node('a'), node('b')]);
    const out = applyFacets(g, EMPTY_FACETS);
    expect(out.nodes).toBeNull();
    expect(out.passing).toBe(2);
    expect(out.total).toBe(2);
  });

  it('is OR within a facet and AND across facets', () => {
    const g = graph([
      node('a', { node_type: 'Gene', status: 'draft' }),
      node('b', { node_type: 'Disease', status: 'draft' }),
      node('c', { node_type: 'Gene', status: 'deprecated' }),
    ]);
    // OR within: two types both pass.
    expect(applyFacets(g, facets({ types: new Set(['Gene', 'Disease']) })).passing).toBe(3);
    // AND across: type AND status.
    expect(
      applyFacets(g, facets({ types: new Set(['Gene']), statuses: new Set(['draft']) })).nodes
    ).toEqual(new Set(['a']));
  });

  it('reads an absent status as `stable`, so the facet does not hide most of a base', () => {
    const g = graph([node('a'), node('b', { status: 'draft' })]);
    expect(applyFacets(g, facets({ statuses: new Set(['stable']) })).nodes).toEqual(new Set(['a']));
  });

  // A node carries no `primary_source` in the contract — only an edge does — so
  // the node half of a Source selection is defined by INCIDENCE or it is not
  // defined at all. Same for predicates: selecting `treats` keeps the endpoints
  // of every `treats` edge, not everything that could be treated.
  it('projects the edge facets onto nodes by incidence', () => {
    const g = graph(
      [node('drug'), node('disease'), node('unrelated')],
      [edge('drug', 'disease', { predicate: 'treats', primary_source: 'pub-1' })]
    );
    expect(applyFacets(g, facets({ predicates: new Set(['treats']) })).nodes).toEqual(
      new Set(['drug', 'disease'])
    );
    expect(applyFacets(g, facets({ sources: new Set(['pub-1']) })).nodes).toEqual(
      new Set(['drug', 'disease'])
    );
  });
});

describe('buildGraphModel', () => {
  it('floors the local degree at the server-supplied one', () => {
    // The deriver has the deduplicated edge list and the renderer does not, so
    // `degree` on the wire is authoritative where it exists.
    const g = graph([node('a', { degree: 9 }), node('b')], [edge('a', 'b')]);
    expect(g.nodes[0].degree).toBe(9);
    expect(buildGraphModel(g).nodes.get('a')!.degree).toBe(9);
    expect(buildGraphModel(g).nodes.get('b')!.degree).toBe(1);
  });

  it('gives an external node its own smaller radius band', () => {
    const g = graph([node('e', { external: true, degree: 40 }), node('n', { degree: 40 })], []);
    const m = buildGraphModel(g);
    expect(m.nodes.get('e')!.radius).toBeLessThanOrEqual(6.2);
    expect(m.nodes.get('e')!.external).toBe(true);
    expect(m.nodes.get('e')!.hub).toBe(false);
    expect(m.hasExternal).toBe(true);
  });

  it('counts types and predicates, and reports an unrecognised type', () => {
    const g = graph(
      [node('a', { node_type: 'Gene' }), node('b', { node_type: 'Gene' }), node('c', { node_type: 'Klingon' })],
      [edge('a', 'b', { predicate: 'treats' }), edge('a', 'c', { relation: 'is_a' })]
    );
    const m = buildGraphModel(g);
    expect(m.typeCounts[0]).toEqual({ type: 'Gene', count: 2 });
    expect(m.predicateCounts.map((p) => p.predicate)).toEqual(['is_a', 'treats']);
    expect(m.untyped).toBe(false);
    expect(m.hasUnrecognisedTypes).toBe(true);
  });

  it('reports a legacy base as untyped rather than inventing a type', () => {
    const m = buildGraphModel(graph([node('a'), node('b')]));
    expect(m.untyped).toBe(true);
    expect(m.hasUnrecognisedTypes).toBe(false);
    expect(m.nodes.get('a')!.type).toBeNull();
  });

  it('adds the synthetic no-source bucket only when an edge lacks one', () => {
    const withSource = buildGraphModel(
      graph([node('a'), node('b')], [edge('a', 'b', { primary_source: 'a' })])
    );
    expect(withSource.sourceOptions.map((s) => s.id)).toEqual(['a']);

    const without = buildGraphModel(graph([node('a'), node('b')], [edge('a', 'b')]));
    expect(without.sourceOptions.map((s) => s.id)).toEqual([NO_PRIMARY_SOURCE]);
  });
});

describe('truncateLabel', () => {
  // By CHARACTER COUNT, not width: width needs `measureText`, which is the
  // per-frame cost DR-9 is about. A character cap is a pure function of the
  // string and therefore memoisable into the model.
  it('caps at 32 characters with an ellipsis', () => {
    expect(truncateLabel('x'.repeat(32))).toBe('x'.repeat(32));
    expect(truncateLabel('x'.repeat(40))).toBe(`${'x'.repeat(31)}…`);
  });
});

describe('edge polarity', () => {
  // `negated` is the field to read: the `not_` prefix is only ONE of two
  // spellings on disk, and a renderer that knows only the prefix draws a
  // negative claim as a positive one. The prefix check is the belt.
  it('reads the explicit flag first and the prefix as a belt', () => {
    expect(isNegated(edge('a', 'b', { predicate: 'prevents', negated: true }))).toBe(true);
    expect(isNegated(edge('a', 'b', { predicate: 'not_prevents' }))).toBe(true);
    expect(isNegated(edge('a', 'b', { predicate: 'prevents' }))).toBe(false);
  });

  it('prefers `predicate` over the deprecated `relation` alias', () => {
    expect(edgePredicate(edge('a', 'b', { predicate: 'treats', relation: 'old' }))).toBe('treats');
    expect(edgePredicate(edge('a', 'b', { relation: 'old' }))).toBe('old');
    // `null` rather than `''`, so "this edge has no type" is answerable.
    expect(edgePredicate(edge('a', 'b'))).toBeNull();
  });

  it('spells a negation out for the one edge that carries a label', () => {
    expect(readablePredicate(edge('a', 'b', { predicate: 'not_prevents', negated: true }))).toBe(
      'not prevents'
    );
    expect(readablePredicate(edge('a', 'b', { predicate: 'increases_risk_of' }))).toBe(
      'increases risk of'
    );
  });
});

describe('showsCredibility', () => {
  // The gate is written in `node_type`, NOT `kind`. The three source types are
  // exactly the members of the Provenance family the Source facet lists, so the
  // facet and the ring agree by construction.
  it('rings the three source types and nothing else', () => {
    for (const type of ['Publication', 'Study', 'Dataset']) {
      expect(showsCredibility(node('s', { node_type: type, credibility_tier: 'web' }))).toBe(true);
    }
    expect(showsCredibility(node('g', { node_type: 'Gene', credibility_tier: 'web' }))).toBe(false);
    // `kind: 'source'` is the pre-OKF axis and is used by nothing here.
    expect(showsCredibility(node('s', { kind: 'source', credibility_tier: 'web' }))).toBe(false);
  });

  it('leaves a source with no verdict unringed — absence of a verdict is not a verdict', () => {
    expect(showsCredibility(node('s', { node_type: 'Publication' }))).toBe(false);
  });
});
