import * as React from 'react';

import { cn } from '../../utils';

const Input = React.forwardRef<HTMLInputElement, React.ComponentProps<'input'>>(
  ({ className, type, ...props }, ref) => {
    return (
      <input
        type={type}
        className={cn(
          'flex h-9 w-full rounded-md border border-border-input bg-background-default px-3 py-1 text-sm transition-colors duration-[var(--motion-fast)] file:border-0 file:bg-transparent file:text-sm file:font-medium file:text-text-default placeholder:text-text-muted hover:border-border-strong aria-invalid:border-border-danger disabled:cursor-not-allowed disabled:bg-background-muted disabled:opacity-50',
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
