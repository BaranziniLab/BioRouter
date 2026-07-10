import * as React from 'react';
import * as SwitchPrimitives from '@radix-ui/react-switch';
import { cn } from '../../utils';

export const Switch = React.forwardRef<
  React.ElementRef<typeof SwitchPrimitives.Root>,
  React.ComponentPropsWithoutRef<typeof SwitchPrimitives.Root> & {
    // Retained for API compatibility; both values render the one canonical switch.
    variant?: 'default' | 'mono';
  }
>(({ className, variant: _variant = 'default', ...props }, ref) => (
  <SwitchPrimitives.Root
    className={cn(
      // Track: 36x20 rounded-full, 2px transparent border insets the 16px knob.
      // Focus is the global outline (main.css) — no per-component ring here.
      'peer inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full border-2 border-transparent transition-colors duration-[var(--motion-fast)] ease-[var(--ease-out)] disabled:cursor-not-allowed disabled:opacity-50',
      'bg-background-strong data-[state=checked]:bg-background-accent',
      className
    )}
    {...props}
    ref={ref}
  >
    <SwitchPrimitives.Thumb
      className={cn(
        'pointer-events-none block h-4 w-4 rounded-full bg-white ring-0 transition-transform duration-[var(--motion-fast)] ease-[var(--ease-out)]',
        'data-[state=unchecked]:translate-x-0 data-[state=checked]:translate-x-4'
      )}
    />
  </SwitchPrimitives.Root>
));
Switch.displayName = SwitchPrimitives.Root.displayName;
