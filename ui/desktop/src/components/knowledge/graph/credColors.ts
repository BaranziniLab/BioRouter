// ui/desktop/src/components/knowledge/graph/credColors.ts
import type { CredibilityTier, PageKind } from '../../../api/types.gen';

// Mirrors the spec's Credibility palette (docs/history/knowledge-base-buildout/founding-design.md ~L195).
// Blues = academic credibility ramp; warmer colors = web/personal/retracted.
export const credColor: Record<CredibilityTier, string> = {
  peer_reviewed: '#49659a',
  book: '#6d7fa8',
  preprint: '#8da0c0',
  gray_lit: '#b8c4d7',
  web: '#d39a73',
  personal: '#c49aac',
};
// `retracted` is a separate flag on the source meta, not a tier — color separately:
export const retractedColor = '#cf6f6f';

// Non-source page kinds keep neutral colors so credibility coloring does not
// collide with node-kind coloring.
export const kindColor: Record<Exclude<PageKind, 'source'>, string> = {
  entity: '#6e9cbe',
  concept: '#83ac88',
  hub: '#d6b06a',
  note: '#a4a4a4',
  flag: '#d48282',
};

export function nodeFill(node: {
  kind: PageKind;
  credibility_tier?: CredibilityTier | null;
}): string {
  if (node.kind === 'source' && node.credibility_tier) {
    return credColor[node.credibility_tier];
  }
  if (node.kind === 'source') return '#b8c4d7';
  return kindColor[node.kind as Exclude<PageKind, 'source'>] ?? '#9a9a9a';
}
