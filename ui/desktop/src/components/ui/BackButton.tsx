import React, { useEffect, useCallback } from 'react';
import { ArrowLeft } from '../icons/app-icons';
import { Button } from './button';
import type { VariantProps } from 'class-variance-authority';
import { buttonVariants } from './button';
import { cn } from '../../utils';

interface BackButtonProps extends VariantProps<typeof buttonVariants> {
  onClick?: () => void;
  className?: string;
  showText?: boolean;
  shape?: 'pill' | 'round';
}

const BackButton: React.FC<BackButtonProps> = ({
  onClick,
  className = '',
  variant = 'ghost',
  size = 'sm',
  shape = 'pill',
  showText = true,
  ...props
}) => {
  const handleExit = useCallback(() => {
    if (onClick) {
      onClick(); // Custom onClick handler passed via props
    } else if (window.history.length > 1) {
      window.history.back(); // Navigate to the previous page
    } else {
      console.warn('No history to go back to');
    }
  }, [onClick]);

  // Set up mouse back button event listener.
  useEffect(() => {
    const handleMouseBack = () => {
      handleExit();
    };

    if (window.electron) {
      const mouseBackHandler = (e: MouseEvent) => {
        // MouseButton 3 or 4 is typically back button.
        if (e.button === 3 || e.button === 4) {
          handleExit();
          e.preventDefault();
        }
      };

      const disposeMouseBack = window.electron.on('mouse-back-button-clicked', handleMouseBack);

      // Also listen for mouseup events directly, for better OS compatibility.
      document.addEventListener('mouseup', mouseBackHandler);

      return () => {
        disposeMouseBack?.();
        document.removeEventListener('mouseup', mouseBackHandler);
      };
    }

    return undefined;
  }, [handleExit]);

  return (
    <Button
      onClick={handleExit}
      variant={variant}
      size={size}
      shape={shape}
      className={cn('flex items-center gap-1.5 text-text-muted hover:text-text-default', className)}
      {...props}
    >
      <ArrowLeft />
      {showText && 'Back'}
    </Button>
  );
};

export default BackButton;
