import * as React from 'react';
import { cn } from '../utils';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from './ui/dialog';

/**
 * The one dialog shell for Biorouter's app-level modals (Astryx §3.6).
 *
 * Every modal outside `components/ui/` renders through this: it owns the size
 * scale, the section geometry (header / scrolling body / footer), the single
 * close affordance, and the dismissal policy. Before it there were six
 * hand-rolled shells and fifteen distinct `max-w` values; two of those shells
 * (`SetupModal`, `InterruptionHandler`) painted their own scrim and are gone,
 * and `BaseModal` lost its last consumer to this file.
 *
 * It COMPOSES `components/ui/dialog` rather than replacing it — the Radix
 * primitive still owns the portal, the scrim, the focus trap and the ×. In
 * particular `DialogContent` carries `.biorouter-modal-surface`, whose
 * unlayered `z-index: var(--z-modal)` is the structural floor that keeps a
 * modal above its own `--z-overlay` scrim. A shell that paints its own
 * full-screen overlay has no such floor, which is how a modal once ended up
 * *under* the pointer-eating scrim and read as "the whole app froze". Nothing
 * here sets a `z-*` class, on purpose.
 *
 * If `DialogContent` ever grows `size` / `purpose` props of its own, this file
 * should collapse into them; the scale below is the one to hoist.
 */

/**
 * Three widths, replacing the fifteen that were in use. `full` is deliberately
 * absent: nothing needs it, and it cannot be built correctly from here because
 * `.biorouter-modal-surface` sets the corner radius from an unlayered rule that
 * no `rounded-none` utility can beat.
 */
export const MODAL_SIZE = {
  /** S — confirmations and single-decision notices. */
  sm: 'sm:max-w-[400px]',
  /** M — forms. */
  md: 'sm:max-w-[480px]',
  /** L — palettes, reports, anything with a list. */
  lg: 'sm:max-w-[640px]',
} as const;

export type ModalSize = keyof typeof MODAL_SIZE;

/**
 * Astryx's `purpose` axis, taken wholesale because Biorouter had the bug it
 * prevents — a half-filled parameter form that a stray backdrop click threw
 * away.
 *
 * - `info` — dismisses on Escape, on backdrop click, and via the ×.
 * - `form` — Escape and × still leave; a backdrop click does NOT, so typed
 *   input survives a misclick.
 * - `required` — must be answered. No ×, no Escape, no backdrop. Use it for
 *   the transient "work in flight" state too (`purpose={busy ? 'required' :
 *   'form'}`), so a dismissal cannot orphan an install.
 */
export type ModalPurpose = 'info' | 'form' | 'required';

export interface ModalShellProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  size?: ModalSize;
  purpose?: ModalPurpose;
  title: React.ReactNode;
  /** Status ink for the title (danger/warning). Layout never changes with it. */
  titleClassName?: string;
  /** Optional 12px line under the title. */
  subtitle?: React.ReactNode;
  /** Right-aligned action row. */
  footer?: React.ReactNode;
  children?: React.ReactNode;
  /**
   * The body scrolls internally instead of growing the dialog, and the
   * header/footer gain their hairlines so the scroll edge is legible.
   */
  scrollBody?: boolean;
  className?: string;
  bodyClassName?: string;
}

export function ModalShell({
  open,
  onOpenChange,
  size = 'md',
  purpose = 'info',
  title,
  titleClassName,
  subtitle,
  footer,
  children,
  scrollBody = false,
  className,
  bodyClassName,
}: ModalShellProps) {
  const dismissible = purpose !== 'required';

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        dismissible={dismissible}
        // `dismissible` already blocks Escape and the backdrop for `required`.
        // This is the one extra rule the purpose axis adds: a form keeps its
        // Escape route but ignores the backdrop.
        onPointerDownOutside={(event) => {
          if (purpose === 'form') event.preventDefault();
        }}
        // Radix warns when a dialog has no description. A subtitle supplies one
        // (and must NOT be overridden, or the aria linkage breaks); without a
        // subtitle we opt out explicitly rather than leave a console warning.
        {...(subtitle ? {} : { 'aria-describedby': undefined })}
        className={cn(
          'flex max-h-[85vh] flex-col gap-0 overflow-hidden p-0',
          MODAL_SIZE[size],
          className
        )}
      >
        <div
          className={cn(
            // The × is 32px inset 16px, so the title column stops at pr-12 and
            // a long title can never slide under it.
            'flex-none px-4 pt-4 pb-3',
            dismissible && 'pr-12',
            scrollBody && 'border-b border-border-subtle'
          )}
        >
          <DialogTitle className={cn('min-w-0 [overflow-wrap:anywhere]', titleClassName)}>
            {title}
          </DialogTitle>
          {subtitle ? (
            <DialogDescription className="mt-1 text-supporting">{subtitle}</DialogDescription>
          ) : null}
        </div>

        {children != null && (
          <div
            className={cn(
              'min-w-0 px-4',
              // The header's pb-3 and the footer's pt-3 are the body's vertical
              // gutters; the body adds none of its own, so a scrolled body runs
              // to the hairline instead of stranding a dead band above it.
              scrollBody ? 'min-h-0 flex-1 overflow-y-auto' : 'flex-none',
              bodyClassName
            )}
          >
            {children}
          </div>
        )}

        {footer != null && (
          <div
            className={cn(
              'flex-none flex flex-wrap items-center justify-end gap-2 px-4 pt-3 pb-4',
              scrollBody && 'border-t border-border-subtle'
            )}
          >
            {footer}
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
