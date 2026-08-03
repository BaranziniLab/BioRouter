import { ShieldIcon } from 'lucide-react';
import { Badge } from './badge';
import { cn } from '../../utils';
import type { SessionClassification } from '../../api';

export interface PrivacyBadgeProps {
  /** The chat's tier. Same union the daemon sends, so a wire change breaks the build. */
  tier: SessionClassification;
  /**
   * Dense surfaces (History rows, tab strips) where a full pill would crowd the
   * row: render a single dot for Private and nothing at all for Public.
   */
  dense?: boolean;
  className?: string;
}

/**
 * The tier indicator (issue #56, R10).
 *
 * Private is the marked state; Public is the quiet state. A badge on absolutely
 * everything trains people to stop seeing badges, which defeats R10's actual
 * goal — knowing which tier you are in BEFORE hitting a wall.
 *
 * Both states use a FILL, not an outline: measured across all six family × mode
 * scopes with `scripts/lib/theme-tokens.mjs`, no border token in this design
 * system reaches 3:1 against `--background-muted` or `--background-medium`
 * (`--border-subtle` is 1.00–1.38, `--border-strong` 1.35–1.58), so a hairline
 * Public pill would be invisible — literally identical colours in
 * parchment:dark, which measures exactly 1.00. The chosen pair instead measures
 * 12.91–15.13 (Private) and 5.50–7.26 (Public) and is asserted for every scope
 * in `scripts/check-contrast.mjs`.
 *
 * Geometry comes from `Badge` and only from `Badge` — the radius, padding and
 * type scale are never restated here, which is the drift `badge.tsx`'s own
 * doc-comment exists to prevent.
 */
export function PrivacyBadge({ tier, dense = false, className }: PrivacyBadgeProps) {
  if (dense && tier === 'public') return null; // no dot means public

  if (dense) {
    return (
      <span
        data-testid="privacy-badge"
        data-privacy="private"
        // `role`, not a bare title: a plain span maps to role `generic`, which
        // does not take an accessible name, so `title` alone would leave the
        // dot silent to a screen reader — invisible AND unannounced.
        role="img"
        aria-label="Private chat"
        title="Private — only private models can read this chat"
        // `inline-block`, not the default `inline`: width and height do not
        // apply to a non-replaced inline element, so `h-1.5 w-1.5` on a bare
        // span paints NOTHING unless the parent that mounts it happens to be a
        // flex or grid container. `shrink-0` because dense surfaces are tight
        // by definition and a flex child with no shrink guard is the first
        // thing squeezed away. Both belong here, not in each caller: an
        // indicator whose whole job is to be seen cannot delegate being visible.
        className={cn('inline-block h-1.5 w-1.5 shrink-0 rounded-full bg-text-default', className)}
      />
    );
  }

  return (
    <Badge
      data-testid="privacy-badge"
      data-privacy={tier}
      className={cn(
        'bg-background-muted',
        tier === 'private' ? 'text-text-default' : 'text-text-muted',
        className
      )}
    >
      {tier === 'private' ? <ShieldIcon className="h-3 w-3" aria-hidden="true" /> : null}
      {tier === 'private' ? 'Private' : 'Public'}
    </Badge>
  );
}
