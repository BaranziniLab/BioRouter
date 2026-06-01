// ui/desktop/src/components/knowledge/graph/credColors.ts
import type { CredibilityTier, PageKind } from '../../../api/types.gen';

// Mirrors the spec's Credibility palette (docs/superpowers/specs/2026-05-30-knowledge-design.md ~L195).
// Blues = academic credibility ramp; warmer colors = web/personal/retracted.
export const credColor: Record<CredibilityTier, string> = {
  peer_reviewed: '#3d4878',
  book:          '#5a6394',
  preprint:      '#7d83b0',
  gray_lit:      '#a8acc8',
  web:           '#c9866a',
  personal:      '#b08aa8',
};
// `retracted` is a separate flag on the source meta, not a tier — color separately:
export const retractedColor = '#c98b8b';

// Non-source page kinds keep neutral colors so credibility coloring does not
// collide with node-kind coloring.
export const kindColor: Record<Exclude<PageKind, 'source'>, string> = {
  entity:   '#5b8aa5',
  concept:  '#7aa57c',
  hub:      '#c8a05b',
  note:     '#9a9a9a',
  flag:     '#c98b8b',
};

export function nodeFill(node: { kind: PageKind; credibility_tier?: CredibilityTier | null }): string {
  if (node.kind === 'source' && node.credibility_tier) {
    return credColor[node.credibility_tier];
  }
  if (node.kind === 'source') return '#a8acc8'; // unclassified source → gray-lit shade
  return kindColor[node.kind as Exclude<PageKind, 'source'>] ?? '#9a9a9a';
}
