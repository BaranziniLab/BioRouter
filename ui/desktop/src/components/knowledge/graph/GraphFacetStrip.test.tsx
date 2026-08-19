import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { Graph, GraphNode } from '../../../api/types.gen';
import { GraphFacetStrip } from './GraphFacetStrip';
import { EMPTY_FACETS } from './graphFacets';
import { buildGraphModel } from './graphModel';

function node(id: string, extra: Partial<GraphNode> = {}): GraphNode {
  return { id, label: id, kind: 'entity', path: `knowledge/${id}.md`, ...extra } as GraphNode;
}

const typed: Graph = {
  nodes: [
    node('a', { node_type: 'Drug' }),
    node('b', { node_type: 'Drug' }),
    node('c', { node_type: 'Disease' }),
    // Three pages the base never gave a type. DR-28: a real state, not a hole.
    node('d'),
    node('e'),
    node('f'),
  ],
  edges: [],
};

const legacy: Graph = { nodes: [node('x'), node('y')], edges: [] };

function renderStrip(graph: Graph, active = false) {
  const model = buildGraphModel(graph);
  return render(
    <GraphFacetStrip
      model={model}
      mode="light"
      facets={EMPTY_FACETS}
      onChange={vi.fn()}
      passing={active ? 2 : graph.nodes.length}
      total={graph.nodes.length}
      active={active}
    />
  );
}

describe('GraphFacetStrip', () => {
  /**
   * The readout and its undo are the only evidence a filter did anything, and
   * with one facet active the strip's content measured 769px inside a 550px
   * box — so both were off the right edge, with no scrollbar affordance to say
   * they were there. Nothing in jsdom can measure that, so the assertion is
   * STRUCTURAL: they must not live inside the scroller at all.
   */
  it('keeps the readout and Clear filters outside the horizontal scroller', () => {
    renderStrip(typed, true);
    const strip = screen.getByTestId('knowledge-graph-facets');

    const readout = screen.getByText(/Showing 2 of 6/);
    const clear = screen.getByRole('button', { name: 'Clear filters' });

    for (const el of [readout, clear]) {
      expect(strip.contains(el)).toBe(true);
      expect(el.closest('.overflow-x-auto')).toBeNull();
    }

    // …and the pickers, which ARE re-reachable by scrolling, stay inside it.
    expect(
      screen.getByTestId('knowledge-graph-facet-type').closest('.overflow-x-auto')
    ).not.toBeNull();
  });

  it('says nothing about a filter until one is active', () => {
    renderStrip(typed, false);
    expect(screen.queryByText(/Showing/)).toBeNull();
    expect(screen.queryByRole('button', { name: 'Clear filters' })).toBeNull();
  });

  it('counts the Untyped row instead of hardcoding it to zero', async () => {
    renderStrip(typed);
    await userEvent.click(screen.getByTestId('knowledge-graph-facet-type'));

    const untyped = await screen.findByRole('option', { name: /Untyped/ });
    // Three pages carry no `node_type`. The row used to read "Untyped" with no
    // number beside it while every other row had one, which reads as "none".
    expect(within(untyped).getByText('3')).toBeInTheDocument();
  });

  it('counts every page of a wholly legacy base', async () => {
    renderStrip(legacy);
    await userEvent.click(screen.getByTestId('knowledge-graph-facet-type'));

    const untyped = await screen.findByRole('option', { name: /Untyped/ });
    expect(within(untyped).getByText('2')).toBeInTheDocument();
  });
});
