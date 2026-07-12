import * as React from 'react';
import { cn } from '../../utils';
import { CheckCircle, AlertCircle, AlertTriangle, Info, Loader2, X } from '../icons/app-icons';

/**
 * Shared notification / alert layout primitive (design.md §4.2–§4.3, Direction B).
 *
 * ONE component owns the layout that both the transient toast and the inline
 * banner render through, so the two long-standing defects are fixed at the
 * source instead of per-call-site:
 *   1. the status icon lives in a fixed leading column, `self-start`, so it
 *      anchors to the title's first line at ANY height (never floats to the
 *      vertical midpoint of a tall toast);
 *   2. the close-button gutter is reserved ONCE on the container — callers add
 *      no `pr-*`, so text/actions can never slide under the ×.
 * Status is expressed only by the tinted icon chip; the surface stays neutral
 * ("colour is evidence; surfaces stay neutral").
 */
export type NotificationStatus = 'success' | 'error' | 'warning' | 'info' | 'loading';

const STATUS_ICON: Record<NotificationStatus, React.FC<{ className?: string }>> = {
  success: CheckCircle,
  error: AlertCircle,
  warning: AlertTriangle,
  info: Info,
  loading: Loader2,
};

// Tailwind's JIT only sees class strings that appear literally in source, so the
// per-status tints are written out in full (no interpolation). The chip fill is
// the status hue at a low alpha over the neutral surface; dark mode needs a touch
// more to read against the deeper ground.
const CHIP_TINT: Record<NotificationStatus, string> = {
  success: 'bg-background-success/10 dark:bg-background-success/20 text-text-success',
  error: 'bg-background-danger/10 dark:bg-background-danger/20 text-text-danger',
  warning: 'bg-background-warning/10 dark:bg-background-warning/20 text-text-warning',
  info: 'bg-background-info/10 dark:bg-background-info/20 text-text-info',
  loading: 'bg-background-medium text-text-muted',
};

export interface NotificationContentProps {
  status: NotificationStatus;
  title?: React.ReactNode;
  message?: React.ReactNode;
  /** Action-button row, rendered beneath the message. */
  actions?: React.ReactNode;
  /** Expanded region (e.g. the grouped-extension detail list). */
  children?: React.ReactNode;
  /** Override the default status glyph. */
  icon?: React.ReactNode;
  className?: string;
}

const hasText = (v: React.ReactNode): boolean => v != null && v !== '';

/**
 * The chip + text block. Used directly as the body of a react-toastify toast
 * (where the toast wrapper owns the surface + the reserved close gutter) and
 * wrapped by {@link NotificationSurface} for inline banners.
 */
export function NotificationContent({
  status,
  title,
  message,
  actions,
  children,
  icon,
  className,
}: NotificationContentProps) {
  const Icon = STATUS_ICON[status];
  return (
    <div className={cn('flex gap-3', className)}>
      <span
        data-status={status}
        aria-hidden="true"
        className={cn(
          'flex-none self-start flex items-center justify-center w-7 h-7 rounded-md',
          CHIP_TINT[status]
        )}
      >
        {icon ?? <Icon className={cn('w-4 h-4', status === 'loading' && 'animate-spin')} />}
      </span>
      <div className="flex-1 min-w-0">
        {hasText(title) && (
          <div className="text-[13px] leading-[18px] font-semibold text-text-default [overflow-wrap:anywhere]">
            {title}
          </div>
        )}
        {hasText(message) && (
          // With a title the message is secondary (muted); on its own it IS the
          // content and reads at full strength.
          <div
            className={cn(
              'text-[13px] leading-[18px] [overflow-wrap:anywhere]',
              hasText(title) ? 'mt-0.5 text-text-muted' : 'text-text-default'
            )}
          >
            {message}
          </div>
        )}
        {actions && <div className="mt-2.5 flex flex-wrap gap-2">{actions}</div>}
        {children}
      </div>
    </div>
  );
}

export interface NotificationSurfaceProps extends NotificationContentProps {
  /** Render a dismiss control; the gutter is reserved automatically. */
  onClose?: () => void;
  /** Toasts float (elevation); inline banners are flat (default). */
  elevated?: boolean;
  role?: string;
  dismissLabel?: string;
}

/**
 * Inline banner shell: the neutral card (border + radius + optional elevation)
 * around {@link NotificationContent}, with an optional dismiss control that
 * lives inside the once-reserved gutter.
 */
export function NotificationSurface({
  onClose,
  elevated = false,
  role,
  dismissLabel = 'Dismiss',
  className,
  ...content
}: NotificationSurfaceProps) {
  return (
    <div
      data-testid="notification-surface"
      role={role}
      className={cn(
        'relative rounded-xl border border-border-subtle bg-background-default p-3',
        elevated && 'shadow-popover',
        onClose && 'pr-10',
        className
      )}
    >
      <NotificationContent {...content} />
      {onClose && (
        <button
          type="button"
          onClick={onClose}
          aria-label={dismissLabel}
          className="absolute right-2.5 top-2.5 inline-flex h-5 w-5 items-center justify-center rounded-sm text-text-subtle transition-colors hover:bg-background-medium hover:text-text-default"
        >
          <X className="h-3.5 w-3.5" />
        </button>
      )}
    </div>
  );
}
