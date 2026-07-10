import * as React from 'react';
import { cn } from '../../utils';

// The one small-label primitive for the app. Chips, tags, and status pills all
// render through this so they share a single radius (--radius-sm, 4px), size,
// and weight. Depth is a flat surface token, never an ad-hoc opacity tint; the
// status tones carry the documented ~12% fill so they read as evidence, not
// decoration.
const toneClass = {
  neutral: 'bg-background-medium text-text-muted',
  accent: 'bg-background-accent/12 text-text-accent',
  info: 'bg-background-info/15 text-text-info',
  success: 'bg-background-success/15 text-text-success',
  warning: 'bg-background-warning/15 text-text-warning',
  danger: 'bg-background-danger/15 text-text-danger',
} as const;

export type BadgeTone = keyof typeof toneClass;

export interface BadgeProps extends React.ComponentProps<'span'> {
  tone?: BadgeTone;
  /** Uppercase micro-label styling (letter-spaced), for tags like “KB”, “Focused”. */
  uppercase?: boolean;
}

export function Badge({ tone = 'neutral', uppercase = false, className, ...props }: BadgeProps) {
  return (
    <span
      className={cn(
        'inline-flex flex-shrink-0 items-center gap-1 rounded-sm px-1.5 py-0.5 text-[11px] font-medium leading-none',
        uppercase && 'uppercase tracking-wider',
        toneClass[tone],
        className
      )}
      {...props}
    />
  );
}
