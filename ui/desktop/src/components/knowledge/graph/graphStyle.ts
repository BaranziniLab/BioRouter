// ui/desktop/src/components/knowledge/graph/graphStyle.ts
import type { CredibilityTier } from '../../../api/types.gen';

export const NODE_BASE_RADIUS = 5;
export const HUB_RADIUS = 9;
export const LABEL_FONT_PX = 11;
export const LABEL_FONT_PX_HUB = 13;
export const DIMMED_OPACITY = 0.35;

/// Width + dashed-ness from the source page's credibility tier.
/// peer_reviewed/book → solid 1.6px, preprint solid 1.3px, gray_lit solid 1.2px,
/// web/personal dashed 1.0px. Default solid 1.0px when tier unknown.
export function edgeStyle(tier: CredibilityTier | null | undefined): { width: number; dash: number[] | null } {
  switch (tier) {
    case 'peer_reviewed':
    case 'book':       return { width: 1.6, dash: null };
    case 'preprint':   return { width: 1.3, dash: null };
    case 'gray_lit':   return { width: 1.2, dash: null };
    case 'web':
    case 'personal':   return { width: 1.0, dash: [4, 3] };
    default:           return { width: 1.0, dash: null };
  }
}

/// Top-N degree centrality threshold for "hub" treatment.
export const HUB_TOP_N = 6;
