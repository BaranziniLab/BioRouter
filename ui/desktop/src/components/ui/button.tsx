import * as React from 'react';
import { Slot } from '@radix-ui/react-slot';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '../../utils';

const buttonVariants = cva(
  // Focus is handled globally in main.css (:focus-visible ring); no per-component ring here.
  "inline-flex items-center justify-center gap-2 whitespace-nowrap text-sm font-medium transition-colors duration-[var(--motion-fast)] cursor-pointer disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg:not([class*='size-'])]:size-4 shrink-0 [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        default:
          'bg-background-accent text-text-on-accent hover:bg-background-accent-hover active:translate-y-px',
        destructive: 'bg-background-danger text-text-on-status hover:opacity-90',
        outline:
          'bg-transparent border border-border-strong text-text-default hover:bg-background-medium active:translate-y-px',
        secondary: 'bg-background-medium text-text-default hover:bg-background-strong',
        ghost: 'bg-transparent text-text-default hover:bg-background-medium',
        link: 'text-text-accent underline-offset-4 hover:underline',
      },
      size: {
        xs: 'h-6 gap-1 [&_svg:not([class*=size-])]:size-3',
        default: 'h-9',
        sm: 'h-8 gap-1.5',
        lg: 'h-10',
      },
      // 'pill' is a misnomer — it maps to rounded-md (8px), not a full pill.
      // 'round' is a square icon button (w==h via compound variants) also at rounded-md.
      shape: {
        pill: 'rounded-md',
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
        className: 'w-6 h-6 p-0 rounded-md',
      },
      {
        shape: 'round',
        size: 'default',
        className: 'w-9 h-9 p-0 rounded-md',
      },
      {
        shape: 'round',
        size: 'sm',
        className: 'w-8 h-8 p-0 rounded-md',
      },
      {
        shape: 'round',
        size: 'lg',
        className: 'w-10 h-10 p-0 rounded-md',
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
