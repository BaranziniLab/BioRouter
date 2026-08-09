import React from 'react';
import { Tooltip, TooltipContent, TooltipTrigger } from '../ui/Tooltip';

const EnvironmentBadge: React.FC = () => {
  const isAlpha = process.env.ALPHA;
  const isDevelopment = import.meta.env.DEV;

  // Don't show badge in production
  if (!isDevelopment && !isAlpha) {
    return null;
  }

  const tooltipText = isAlpha ? 'Alpha' : 'Development build';
  const bgColor = isAlpha ? 'bg-background-info' : 'bg-background-warning';

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div
          className={`${bgColor} w-3 h-3 rounded-full cursor-default`}
          data-testid="environment-badge"
          aria-label={tooltipText}
        />
      </TooltipTrigger>
      <TooltipContent side="right">{tooltipText}</TooltipContent>
    </Tooltip>
  );
};

export default EnvironmentBadge;
