import * as React from 'react';
import { cn } from '../../utils';

/**
 * The one small-label primitive, in the two tiers §3.4 makes explicit.
 *
 * The distinction is not decorative and it is enforced by the type, because the
 * app previously had four hand-rolled recipes and no way to say which was which:
 *
 * - **badge** (default) — 20px. Carries STATUS: what a thing currently is.
 * - **chip** — 24px. Carries a CATEGORY or a filter: what a thing is filed under.
 *
 * A chip is the larger of the two because it is the one you may eventually be
 * able to act on (§3.4 allows it a 16px remove control); a badge is a read-only
 * fact and gets out of the way. Both are squared at `--radius-inner`: A-04
 * restricts `--radius-full` to status dots, the switch knob and avatars, so the
 * two tiers are separated by height and role rather than by corner — which also
 * means neither can quietly become the other by losing a class.
 *
 * Tone is one formula, not six hand-picked pairs: the hue at 22% behind that same
 * hue's saturated ink (§2.5). `neutral` is the exception on purpose — it has no
 * hue to wash, so it takes a real surface step instead.
 */
const toneClass = {
  neutral: 'bg-background-medium text-text-muted',
  accent: 'bg-background-accent/22 text-text-accent',
  info: 'bg-background-info/22 text-text-info',
  success: 'bg-background-success/22 text-text-success',
  warning: 'bg-background-warning/22 text-text-warning',
  danger: 'bg-background-danger/22 text-text-danger',
} as const;

const variantClass = {
  badge: 'h-5 px-1.5',
  chip: 'h-6 px-2',
} as const;

export type BadgeTone = keyof typeof toneClass;
export type BadgeVariant = keyof typeof variantClass;

export interface BadgeProps extends React.ComponentProps<'span'> {
  tone?: BadgeTone;
  /** `badge` (20px, status) or `chip` (24px, category/filter). */
  variant?: BadgeVariant;
  /** Uppercase micro-label styling, for tags like “KB”, “Focused”. */
  uppercase?: boolean;
}

export function Badge({
  tone = 'neutral',
  variant = 'badge',
  uppercase = false,
  className,
  ...props
}: BadgeProps) {
  return (
    <span
      className={cn(
        'inline-flex flex-shrink-0 items-center gap-1 rounded-inner text-chip',
        variantClass[variant],
        // `text-caps` IS the caps style — it carries the uppercase transform, the
        // +0.08em tracking and the 500 weight together, so there is nothing to
        // add beside it. It shares the `text` merge group with `text-chip` above,
        // which is why the override lands instead of both surviving.
        uppercase && 'text-caps',
        toneClass[tone],
        className
      )}
      {...props}
    />
  );
}
