import * as React from 'react';
import { Slot } from '@radix-ui/react-slot';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '../../utils';

const buttonVariants = cva(
  // Focus is handled globally in main.css (:focus-visible ring); no per-component ring here.
  // transform is in the transition list so the shared active:scale press eases on
  // release (and any hover:scale on a call site animates instead of snapping).
  // NB: Tailwind v4 maps scale-*/hover:scale-*/active:scale-* to the standalone
  // `scale` property (not `transform`), so `scale` must be in the transition list
  // for the press/hover scale to ease rather than snap.
  // `text-label` IS 14/20/500 — it replaces `text-sm font-medium` exactly, and is
  // the one type role every control in the app now shares.
  // The duration/easing annotations are gone: `--default-transition-duration` and
  // `--default-transition-timing-function` are already `--dur-fast` / `--ease-out`
  // in main.css, so any `transition-*` utility picks them up unannotated.
  "inline-flex items-center justify-center gap-2 whitespace-nowrap text-label transition-[color,background-color,border-color,transform,scale,opacity] active:scale-[0.97] cursor-pointer disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg:not([class*='size-'])]:size-4 shrink-0 [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        default: 'bg-background-accent text-text-on-accent hover:bg-background-accent-hover',
        destructive: 'bg-background-danger text-text-on-status hover:opacity-90',
        // `outline` and `ghost` sit on a transparent ground, so their hover IS the
        // shared interaction tint — `bg-overlay-hover` (ink @5%) composites onto
        // whatever surface hosts the button and is correct in every family and mode.
        // `secondary` keeps a surface step: it is FILLED with `--background-medium`,
        // and an overlay tint would replace that fill rather than sit on top of it.
        outline:
          'bg-transparent border border-border-strong text-text-default hover:bg-overlay-hover',
        secondary: 'bg-background-medium text-text-default hover:bg-background-strong',
        ghost: 'bg-transparent text-text-default hover:bg-overlay-hover',
        link: 'text-text-accent underline-offset-4 hover:underline',
      },
      size: {
        xs: 'h-6 gap-1 [&_svg:not([class*=size-])]:size-3',
        default: 'h-9',
        sm: 'h-8 gap-1.5',
        lg: 'h-10',
      },
      // 'pill' is a misnomer — it maps to rounded-element (8px), not a full pill.
      // 'round' is a square icon button (w==h via compound variants), also rounded-element.
      shape: {
        pill: 'rounded-element',
        round: '',
      },
    },
    compoundVariants: [
      {
        shape: 'pill',
        size: 'xs',
        className: 'px-2 has-[>svg]:px-2',
      },
      {
        shape: 'pill',
        size: 'default',
        className: 'px-4 py-2 has-[>svg]:px-4',
      },
      {
        shape: 'pill',
        size: 'sm',
        className: 'px-4 has-[>svg]:px-3',
      },
      {
        shape: 'pill',
        size: 'lg',
        className: 'px-6 has-[>svg]:px-6',
      },
      {
        shape: 'round',
        size: 'xs',
        className: 'w-6 h-6 p-0 rounded-element',
      },
      {
        shape: 'round',
        size: 'default',
        className: 'w-9 h-9 p-0 rounded-element',
      },
      {
        shape: 'round',
        size: 'sm',
        className: 'w-8 h-8 p-0 rounded-element',
      },
      {
        shape: 'round',
        size: 'lg',
        className: 'w-10 h-10 p-0 rounded-element',
      },
    ],
    defaultVariants: {
      variant: 'default',
      size: 'default',
      shape: 'pill',
    },
  }
);

const Button = React.forwardRef<
  HTMLButtonElement,
  React.ComponentProps<'button'> &
    VariantProps<typeof buttonVariants> & {
      asChild?: boolean;
      shape?: 'pill' | 'round';
    }
>(({ className, variant, size, asChild = false, shape = 'pill', ...props }, ref) => {
  const Comp = asChild ? Slot : 'button';

  return (
    <Comp
      data-slot="button"
      className={cn(buttonVariants({ variant, size, shape, className }))}
      ref={ref}
      {...props}
    />
  );
});

Button.displayName = 'Button';

export { Button, buttonVariants };
