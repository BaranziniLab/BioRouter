import { describe, expect, it } from 'vitest';
import type { GraphNode } from '../../../api/types.gen';
import { isHollow, isHollowType, shapeFor } from './nodeMark';
import { typeShape } from '../../../styles/graphPalette';

const node = (extra: Partial<GraphNode> = {}): GraphNode =>
  ({ id: 'n', label: 'n', kind: 'entity', path: 'knowledge/n.md', ...extra }) as GraphNode;

/**
 * `nodeMark.ts` exists because two surfaces once disagreed about one node. The
 * shape channel reintroduces exactly that risk: it is a PREFERENCE, so a reader
 * that forgets to thread it draws a silhouette where the canvas draws a circle,
 * or the reverse. These tests pin the contract at the one function they all go
 * through.
 */
describe('shapeFor — the shape channel is opt-in and uniform', () => {
  it('draws every node as a circle by default (R-04)', () => {
    expect(shapeFor(node({ node_type: 'Gene' }), 'light')).toBe('circle');
    expect(shapeFor(node({ node_type: 'Anatomy' }), 'light')).toBe('circle');
    expect(shapeFor(node({ node_type: 'Publication' }), 'light')).toBe('circle');
  });

  it('restores the family silhouette when the channel is on', () => {
    // Whatever the generated palette says — asserted against it rather than
    // against a literal, so a shape reassignment moves both together.
    expect(shapeFor(node({ node_type: 'Gene' }), 'light', true)).toBe(typeShape('Gene', 'light'));
    expect(shapeFor(node({ node_type: 'Anatomy' }), 'light', true)).toBe(
      typeShape('Anatomy', 'light')
    );
  });

  it('keeps an untyped node circular in both states, because a universal marker carries nothing', () => {
    expect(shapeFor(node(), 'light')).toBe('circle');
    expect(shapeFor(node(), 'light', true)).toBe('circle');
  });
});

describe('isHollow — the redundant channel that survives the all-circle default', () => {
  it('draws Provenance & Context hollow and Biomedical Entities solid', () => {
    expect(isHollow(node({ node_type: 'Publication' }), 'light')).toBe(true);
    expect(isHollow(node({ node_type: 'Study' }), 'light')).toBe(true);
    expect(isHollow(node({ node_type: 'Gene' }), 'light')).toBe(false);
    expect(isHollow(node({ node_type: 'Disease' }), 'light')).toBe(false);
  });

  it('never hollows an untyped node: in a legacy base every node would be', () => {
    expect(isHollow(node(), 'light')).toBe(false);
  });

  it('answers the same question for a bare type string, so the legend agrees', () => {
    expect(isHollowType('Publication', 'light')).toBe(true);
    expect(isHollowType('Gene', 'light')).toBe(false);
    // The legend and the canvas must not diverge across modes either.
    expect(isHollowType('Publication', 'dark')).toBe(true);
  });
});
