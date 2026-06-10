import React from 'react';
import { cn } from '../utils';

export type ToolCallStatus = 'pending' | 'loading' | 'success' | 'error';

interface ToolCallStatusIndicatorProps {
  status: ToolCallStatus;
  className?: string;
}

export const ToolCallStatusIndicator: React.FC<ToolCallStatusIndicatorProps> = ({
  status,
  className,
}) => {
  const getStatusStyles = () => {
    switch (status) {
      case 'success':
        return 'bg-background-success';
      case 'error':
        return 'bg-background-danger';
      case 'loading':
        return 'bg-background-warning animate-pulse';
      case 'pending':
      default:
        return 'bg-background-strong';
    }
  };

  return (
    <div
      className={cn(
        'absolute -top-0.5 -right-0.5 w-2 h-2 rounded-full border border-background-default',
        getStatusStyles(),
        className
      )}
      aria-label={`Tool status: ${status}`}
    />
  );
};

/**
 * Wrapper component that adds a status indicator to a tool icon
 */
interface ToolIconWithStatusProps {
  ToolIcon: React.ComponentType<{ className?: string }>;
  status: ToolCallStatus;
  className?: string;
}

export const ToolIconWithStatus: React.FC<ToolIconWithStatusProps> = ({
  ToolIcon,
  status,
  className,
}) => {
  return (
    <div className={cn('relative inline-block', className)}>
      <ToolIcon className="w-3 h-3 flex-shrink-0" />
      <ToolCallStatusIndicator status={status} />
    </div>
  );
};
