import * as React from 'react';

import { cn } from '../../utils';

const Input = React.forwardRef<HTMLInputElement, React.ComponentProps<'input'>>(
  ({ className, type, ...props }, ref) => {
    return (
      <input
        type={type}
        className={cn(
          'flex h-9 w-full rounded-element border border-border-input bg-background-default px-3 py-1 text-label transition-colors file:border-0 file:bg-transparent file:text-label file:text-text-default placeholder:text-text-muted hover:border-border-strong aria-invalid:border-border-danger disabled:cursor-not-allowed disabled:bg-background-muted disabled:opacity-50',
          className
        )}
        ref={ref}
        {...props}
      />
    );
  }
);
Input.displayName = 'Input';

export { Input };
