import { describe, expect, it } from 'vitest';
import type { GraphNode } from '../../../api/types.gen';
import { isHollow, isHollowType } from './nodeMark';

const node = (extra: Partial<GraphNode> = {}): GraphNode =>
  ({ id: 'n', label: 'n', kind: 'entity', path: 'knowledge/n.md', ...extra }) as GraphNode;

/**
 * `nodeMark.ts` exists because two surfaces once disagreed about one node.
 *
 * ⚠ **The shape half of that contract is gone.** `shapeFor` and the opt-in
 * shape-channel preference were deleted outright: every node is a circle, and
 * the redundant non-colour channel is now §5.12's `aria-live` announcement plus
 * the always-on labels. What remains for a mark to agree about is the FILL and
 * the solid/hollow split, and both still go through this one module.
 */
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
