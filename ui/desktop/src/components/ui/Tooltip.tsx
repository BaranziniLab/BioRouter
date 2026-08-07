import * as React from 'react';
import * as TooltipPrimitive from '@radix-ui/react-tooltip';

import { cn } from '../../utils';

/**
 * `--background-inverse` fill, `--text-inverse` 12/16, 6px×8px padding, no
 * arrow, 8px offset. `text-supporting` is the 12/16 role; `font-medium` is a
 * DELIBERATE override of its 400 weight — a tooltip sits on an inverse fill and
 * needs the extra weight to hold up, and that is what shipped before the roles
 * existed.
 *
 * Radius is `--radius-container` (12px), not the `--radius-inner` (4px) this
 * carried. Two steps off the ladder, against the repo's own written rules on
 * both sides: `main.css` reserves `--radius-inner` for "inline code, chips,
 * checkbox, swatches — nested inside a control", and the cohesion design says
 * "every floating thing — popover, dropdown, select, mention picker, toast,
 * tooltip — gets the same recipe … 12px radius". The old comment cited
 * "design.md §4.4" as authority for 4px; that document no longer exists.
 *
 * Z — deliberately `--z-modal-dropdown` (500), not `--z-dropdown` (200). A tooltip
 * must paint above whatever surface owns its trigger, and it is portalled to <body>
 * so it cannot know what that is. ContextWindowIndicator is the proof: its gauge
 * ("Compact conversation", "Drag to adjust auto-compact threshold") renders INSIDE a
 * PopoverContent, which sits at 500 — at 200 those tooltips would hide behind the
 * very popover that contains them.
 */
export const TOOLTIP_SURFACE_CLASS_NAME =
  'bg-background-inverse text-text-inverse z-[var(--z-modal-dropdown)] w-max max-w-[min(20rem,calc(100vw-16px))] break-words rounded-container px-2 py-1.5 text-left font-sans text-supporting font-medium whitespace-normal';

function TooltipProvider({
  delayDuration = 500,
  ...props
}: React.ComponentProps<typeof TooltipPrimitive.Provider>) {
  return (
    <TooltipPrimitive.Provider
      data-slot="tooltip-provider"
      delayDuration={delayDuration}
      {...props}
    />
  );
}

function Tooltip({ ...props }: React.ComponentProps<typeof TooltipPrimitive.Root>) {
  return (
    <TooltipProvider>
      <TooltipPrimitive.Root data-slot="tooltip" {...props} />
    </TooltipProvider>
  );
}

function TooltipTrigger({ ...props }: React.ComponentProps<typeof TooltipPrimitive.Trigger>) {
  return <TooltipPrimitive.Trigger data-slot="tooltip-trigger" {...props} />;
}

function TooltipContent({
  className,
  sideOffset = 8,
  children,
  ...props
}: React.ComponentProps<typeof TooltipPrimitive.Content>) {
  return (
    <TooltipPrimitive.Portal>
      <TooltipPrimitive.Content
        data-slot="tooltip-content"
        sideOffset={sideOffset}
        className={cn(
          TOOLTIP_SURFACE_CLASS_NAME,
          'animate-in fade-in-0 duration-[120ms] data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:duration-[120ms]',
          className
        )}
        {...props}
      >
        {children}
      </TooltipPrimitive.Content>
    </TooltipPrimitive.Portal>
  );
}

export { Tooltip, TooltipTrigger, TooltipContent, TooltipProvider };
