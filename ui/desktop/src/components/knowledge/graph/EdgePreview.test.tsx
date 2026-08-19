import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { Graph, GraphEdge, GraphNode } from '../../../api/types.gen';
import { EdgePreview } from './EdgePreview';
import { buildGraphModel } from './graphModel';

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
    render(<EdgePreview edge={edge} model={model} onClose={() => undefined} />);
    const panel = screen.getByRole('dialog');
    expect(panel).toHaveAccessibleName('Link Metformin treats Type 2 diabetes');
    expect(within(panel).getByText('treats')).toBeInTheDocument();
  });

  it('shows the provenance triplet — the whole reason an edge is worth opening', () => {
    render(<EdgePreview edge={edge} model={model} onClose={() => undefined} />);
    const provenance = screen.getByRole('region', { name: 'Provenance' });
    expect(within(provenance).getByText('knowledge_assertion')).toBeInTheDocument();
    expect(within(provenance).getByText('manual_agent')).toBeInTheDocument();
    expect(within(provenance).getByText('PMID:12345')).toBeInTheDocument();
  });

  it('says "Not stated" rather than hiding a missing provenance field', () => {
    render(
      <EdgePreview
        edge={{ from: 'metformin', to: 't2d', predicate: 'treats' }}
        model={model}
        onClose={() => undefined}
      />
    );
    // An unsourced claim must not render identically to a sourced one.
    expect(within(screen.getByRole('region', { name: 'Provenance' })).getAllByText('Not stated'))
      .toHaveLength(3);
  });

  it('shows the quantitative bundle and the context qualifiers', () => {
    render(<EdgePreview edge={edge} model={model} onClose={() => undefined} />);
    const quant = screen.getByRole('region', { name: 'Quantitative' });
    expect(within(quant).getByText('effect size')).toBeInTheDocument();
    expect(within(quant).getByText('0.42')).toBeInTheDocument();
    expect(within(quant).getByText('<0.001')).toBeInTheDocument();
    expect(
      within(screen.getByRole('region', { name: 'Context' })).getByText('human')
    ).toBeInTheDocument();
  });

  it('spells a negated claim out, and strikes it through', () => {
    render(
      <EdgePreview
        edge={{ from: 'metformin', to: 't2d', predicate: 'not_treats', negated: true }}
        model={model}
        onClose={() => undefined}
      />
    );
    const predicate = screen.getByText('not treats');
    expect(predicate).toHaveClass('line-through');
    expect(predicate).toHaveClass('text-text-danger');
  });

  it('replaces the triplet for a derived edge instead of reporting three blanks', () => {
    render(
      <EdgePreview
        edge={{ from: 'metformin', to: 't2d', predicate: 'reported_in', synthesized: true }}
        model={model}
        onClose={() => undefined}
      />
    );
    const provenance = screen.getByRole('region', { name: 'Provenance' });
    expect(within(provenance).getByText(/Derived from provenance/)).toBeInTheDocument();
    expect(within(provenance).queryByText('Not stated')).toBeNull();
  });

  it('dismisses with Escape', () => {
    const onClose = vi.fn();
    render(<EdgePreview edge={edge} model={model} onClose={onClose} />);
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
