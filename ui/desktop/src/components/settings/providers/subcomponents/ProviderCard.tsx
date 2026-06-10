import { useMemo } from 'react';
import { Check } from '../../../icons/app-icons';
import DefaultCardButtons from './buttons/DefaultCardButtons';
import { ProviderDetails, ProviderMetadata } from '../../../../api';
import { Tooltip, TooltipContent, TooltipTrigger } from '../../../ui/Tooltip';

type ProviderCardProps = {
  provider: ProviderDetails;
  onConfigure: () => void;
  onLaunch: () => void;
  isOnboarding: boolean;
};

export const ProviderCard = function ProviderCard({
  provider,
  onConfigure,
  onLaunch,
  isOnboarding,
}: ProviderCardProps) {
  const providerMetadata: ProviderMetadata | null = provider?.metadata || null;
  const metadata = useMemo(() => providerMetadata, [providerMetadata]);

  if (!metadata) {
    return <div>ProviderCard error: No metadata provided</div>;
  }

  const isGrayedOut = !provider.is_configured && isOnboarding;
  const displayName = metadata.display_name || provider?.name || 'Unknown Provider';
  const initial = displayName[0].toUpperCase();

  const handleCardClick = () => {
    if (!isOnboarding) onConfigure();
  };

  return (
    <div
      data-testid={`provider-card-${provider.name.toLowerCase()}`}
      onClick={!isGrayedOut ? handleCardClick : undefined}
      className={[
        'flex items-center gap-3 py-3 px-4 rounded-xl',
        'transition-colors duration-150 group',
        isGrayedOut ? 'opacity-50 cursor-default' : 'cursor-pointer hover:bg-background-medium',
      ].join(' ')}
    >
      {/* Provider initial avatar */}
      <div className="w-8 h-8 rounded-lg bg-background-medium flex items-center justify-center flex-shrink-0 text-sm font-semibold text-text-muted select-none">
        {initial}
      </div>

      {/* Name + description */}
      <div className="flex-1 min-w-0">
        <p className="text-sm font-medium text-text-default truncate">{displayName}</p>
        {metadata.description && (
          <Tooltip>
            <TooltipTrigger asChild>
              <p className="text-xs text-text-muted mt-0.5 truncate cursor-default">
                {metadata.description}
              </p>
            </TooltipTrigger>
            <TooltipContent side="bottom" className="max-w-72 text-wrap">
              {metadata.description}
            </TooltipContent>
          </Tooltip>
        )}
      </div>

      {/* Right: configured badge + action buttons */}
      <div className="flex items-center gap-3 flex-shrink-0">
        {provider.is_configured && (
          <span className="flex items-center gap-1 text-xs text-text-success font-medium">
            <Check className="w-3 h-3" />
            Configured
          </span>
        )}
        <div
          className={
            !isOnboarding ? 'opacity-0 group-hover:opacity-100 transition-opacity duration-150' : ''
          }
        >
          <DefaultCardButtons
            provider={provider}
            onConfigure={onConfigure}
            onLaunch={onLaunch}
            isOnboardingPage={isOnboarding}
          />
        </div>
      </div>
    </div>
  );
};
