import * as React from 'react';
import * as SwitchPrimitives from '@radix-ui/react-switch';
import { cn } from '../../utils';

/**
 * The switch (§3.3): a 40×24 track whose thumb GROWS 16 → 20px as it slides.
 *
 * That growth is the one micro-delight the system allows itself, and it is worth
 * the four extra classes: it makes the on state read as settled rather than just
 * translated, and it costs nothing but two size properties on an element that was
 * already transitioning. It stays inside the calm register — no bounce, no
 * overshoot — because it rides `--ease-out` and `--dur-fast` like every other
 * control state, which the bare `transition-*` utilities now pick up unannotated.
 *
 * The thumb is `--text-on-accent`, not `white`. A literal white knob is
 * theme-blind: it happens to work on Parchment and is a coin toss on any family
 * added later. `--text-on-accent` is the one token whose contract is "legible on
 * the accent fill", which is exactly the surface the knob sits on when checked,
 * and it clears the neutral track when unchecked in every shipped family.
 */
export const Switch = React.forwardRef<
  React.ElementRef<typeof SwitchPrimitives.Root>,
  React.ComponentPropsWithoutRef<typeof SwitchPrimitives.Root> & {
    // Retained for API compatibility; both values render the one canonical switch.
    variant?: 'default' | 'mono';
  }
>(({ className, variant: _variant = 'default', ...props }, ref) => (
  <SwitchPrimitives.Root
    className={cn(
      // Track: 40x24. The 2px transparent border insets the travel, leaving a
      // 36px inner run — so a 20px checked thumb lands flush at translate-x-4.
      // Focus is the global outline (main.css) — no per-component ring here.
      'peer inline-flex h-6 w-10 shrink-0 cursor-pointer items-center rounded-full border-2 border-transparent transition-colors disabled:cursor-not-allowed disabled:opacity-50',
      'bg-background-strong data-[state=checked]:bg-background-accent',
      className
    )}
    {...props}
    ref={ref}
  >
    <SwitchPrimitives.Thumb
      className={cn(
        'pointer-events-none block rounded-full bg-text-on-accent ring-0',
        'transition-[translate,width,height]',
        'h-4 w-4 translate-x-0',
        'data-[state=checked]:h-5 data-[state=checked]:w-5 data-[state=checked]:translate-x-4'
      )}
    />
  </SwitchPrimitives.Root>
));
Switch.displayName = SwitchPrimitives.Root.displayName;
