// ui/desktop/src/components/knowledge/graph/graphStyle.ts
import type { CredibilityTier } from '../../../api/types.gen';

export const NODE_BASE_RADIUS = 4.5;
export const HUB_RADIUS = 7.5;
export const LABEL_FONT_PX = 11.5;
export const LABEL_FONT_PX_HUB = 12.5;
export const DIMMED_OPACITY = 0.22;

/// Width by source-page credibility tier. The graph uses a single solid-line
/// treatment so edge styling stays visually quiet behind node labels.
export function edgeStyle(tier: CredibilityTier | null | undefined): { width: number; dash: number[] | null } {
  switch (tier) {
    case 'peer_reviewed':
    case 'book':       return { width: 0.85, dash: null };
    case 'preprint':   return { width: 0.78, dash: null };
    case 'gray_lit':   return { width: 0.74, dash: null };
    case 'web':
    case 'personal':   return { width: 0.68, dash: null };
    default:           return { width: 0.68, dash: null };
  }
}

/// Top-N degree centrality threshold for "hub" treatment.
export const HUB_TOP_N = 6;
