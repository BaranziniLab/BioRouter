import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { Graph, GraphEdge, GraphNode } from '../../../api/types.gen';
import { EdgePreview, mergeQuantitative } from './EdgePreview';
import { buildGraphModel } from './graphModel';
import { fillFor, shapeFor } from './nodeMark';
import { svgPathForShape } from './nodeShapes';

function node(id: string, extra: Partial<GraphNode> = {}): GraphNode {
  return { id, label: id, kind: 'entity', path: `knowledge/${id}.md`, ...extra } as GraphNode;
}

const graph: Graph = {
  nodes: [
    node('metformin', { identifier: 'Metformin', node_type: 'Drug' }),
    node('t2d', { identifier: 'Type 2 diabetes', node_type: 'Disease' }),
  ],
  edges: [],
};

const model = buildGraphModel(graph);
const byId = new Map(graph.nodes.map((n) => [n.id, n]));
const nodeById = (id: string) => byId.get(id);

/** Every case renders the real component; only the edge under test varies. */
function renderEdge(edge: GraphEdge, overrides: Record<string, unknown> = {}) {
  return render(
    <EdgePreview
      edge={edge}
      model={model}
      nodeById={nodeById}
      mode="light"
      onClose={() => undefined}
      {...overrides}
    />
  );
}

const edge: GraphEdge = {
  from: 'metformin',
  to: 't2d',
  predicate: 'treats',
  knowledge_level: 'knowledge_assertion',
  agent_type: 'manual_agent',
  primary_source: 'PMID:12345',
  quantitative: { effect_size: 0.42, p_value: '<0.001' },
  qualifiers: { species_context: 'human' },
};

describe('EdgePreview', () => {
  it('names the claim and both of its endpoints', () => {
    renderEdge(edge);
    const panel = screen.getByRole('dialog');
    expect(panel).toHaveAccessibleName('Link Metformin treats Type 2 diabetes');
    expect(within(panel).getByText('treats')).toBeInTheDocument();
  });

  it('shows the provenance triplet — the whole reason an edge is worth opening', () => {
    renderEdge(edge);
    const provenance = screen.getByRole('region', { name: 'Provenance' });
    expect(within(provenance).getByText('knowledge_assertion')).toBeInTheDocument();
    expect(within(provenance).getByText('manual_agent')).toBeInTheDocument();
    expect(within(provenance).getByText('PMID:12345')).toBeInTheDocument();
  });

  it('says "Not stated" rather than hiding a missing provenance field', () => {
    renderEdge({ from: 'metformin', to: 't2d', predicate: 'treats' });
    // An unsourced claim must not render identically to a sourced one.
    expect(
      within(screen.getByRole('region', { name: 'Provenance' })).getAllByText('Not stated')
    ).toHaveLength(3);
  });

  it('shows the quantitative bundle and the context qualifiers', () => {
    renderEdge(edge);
    const quant = screen.getByRole('region', { name: 'Quantitative' });
    expect(within(quant).getByText('effect size')).toBeInTheDocument();
    expect(within(quant).getByText('0.42')).toBeInTheDocument();
    expect(within(quant).getByText('<0.001')).toBeInTheDocument();
    expect(
      within(screen.getByRole('region', { name: 'Context' })).getByText('human')
    ).toBeInTheDocument();
  });

  it('spells a negated claim out, and strikes it through', () => {
    renderEdge({ from: 'metformin', to: 't2d', predicate: 'not_treats', negated: true });
    const predicate = screen.getByText('not treats');
    expect(predicate).toHaveClass('line-through');
    // A refuted claim is not a missing one, so the head says so in words too —
    // the strike alone is a single channel carrying the most reversible fact
    // on the panel.
    expect(screen.getByText('Negative edge')).toBeInTheDocument();
  });

  it('heads a positive edge with a neutral badge, not the danger one', () => {
    renderEdge(edge);
    expect(screen.getByText('Edge')).toBeInTheDocument();
    expect(screen.queryByText('Negative edge')).toBeNull();
  });

  it('replaces the triplet for a derived edge instead of reporting three blanks', () => {
    renderEdge({ from: 'metformin', to: 't2d', predicate: 'reported_in', synthesized: true });
    const provenance = screen.getByRole('region', { name: 'Provenance' });
    expect(within(provenance).getByText(/Implicit link derived/)).toBeInTheDocument();
    expect(within(provenance).queryByText('Not stated')).toBeNull();
  });

  it('draws each endpoint with the mark the canvas draws for that node', () => {
    const { container } = renderEdge(edge);
    const claim = within(screen.getByRole('region', { name: 'Claim' }));
    const paths = Array.from(
      (claim.getByText('Metformin').closest('[class*="items-center"]') ?? container).querySelectorAll(
        'svg path'
      )
    );
    // Not "a colour" — the CANVAS's colour and shape for Metformin, resolved
    // through the one function all three surfaces call.
    const drug = byId.get('metformin')!;
    expect(paths[0]).toHaveAttribute('fill', fillFor(drug, 'light'));
    expect(paths[0]).toHaveAttribute('d', svgPathForShape(shapeFor(drug, 'light')));
  });

  it('selects an endpoint node, so an edge is a way into its own ends', () => {
    const onSelectNode = vi.fn();
    renderEdge(edge, { onSelectNode });
    fireEvent.click(screen.getByText('Type 2 diabetes'));
    expect(onSelectNode).toHaveBeenCalledWith(byId.get('t2d'));
  });

  it('dismisses with Escape', () => {
    const onClose = vi.fn();
    renderEdge(edge, { onClose });
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});

describe('mergeQuantitative', () => {
  it('merges the two CI bounds into one interval — nobody reports half of one', () => {
    expect(mergeQuantitative({ ci_lower: 1.2, ci_upper: 3.4 })).toEqual([['95% CI', '1.2 – 3.4']]);
  });

  it('keeps every other key uniform, so a vocabulary addition needs no code', () => {
    expect(mergeQuantitative({ effect_size: 0.42, some_future_slot: 'x' })).toEqual([
      ['effect size', '0.42'],
      ['some future slot', 'x'],
    ]);
  });

  it('still reports a lone bound rather than dropping it', () => {
    expect(mergeQuantitative({ ci_lower: 1.2 })).toEqual([['95% CI lower', '1.2']]);
  });

  it('is empty for an edge with no quantitative bundle at all', () => {
    expect(mergeQuantitative(undefined)).toEqual([]);
  });
});
