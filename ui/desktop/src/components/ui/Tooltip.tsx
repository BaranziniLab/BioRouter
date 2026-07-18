import * as React from 'react';
import * as TooltipPrimitive from '@radix-ui/react-tooltip';

import { cn } from '../../utils';

/**
 * design.md §4.4 — `--background-inverse` fill, `--text-inverse` 12/16, 6px×8px
 * padding, `--radius-sm`, no arrow, 8px offset. This already matches canonical.
 *
 * Z — deliberately `--z-modal-dropdown` (500), not `--z-dropdown` (200). A tooltip
 * must paint above whatever surface owns its trigger, and it is portalled to <body>
 * so it cannot know what that is. ContextWindowIndicator is the proof: its gauge
 * ("Compact conversation", "Drag to adjust auto-compact threshold") renders INSIDE a
 * PopoverContent, which sits at 500 — at 200 those tooltips would hide behind the
 * very popover that contains them.
 */
export const TOOLTIP_SURFACE_CLASS_NAME =
  'bg-background-inverse text-text-inverse z-[var(--z-modal-dropdown)] w-max max-w-[min(20rem,calc(100vw-16px))] break-words rounded-sm px-2 py-1.5 text-left font-sans text-xs font-medium leading-4 whitespace-normal';

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
