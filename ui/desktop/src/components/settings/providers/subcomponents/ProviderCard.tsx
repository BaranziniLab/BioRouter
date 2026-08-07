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
        'flex items-center gap-3 py-3 px-4 rounded-container',
        'transition-colors group',
        isGrayedOut ? 'cursor-default' : 'cursor-pointer tint-interactive',
      ].join(' ')}
    >
      {/* Provider initial avatar (dimmed when unconfigured, but not the action buttons) */}
      <div
        className={[
          'w-8 h-8 rounded-element bg-background-medium flex items-center justify-center flex-shrink-0 text-sm font-semibold text-text-muted select-none',
          isGrayedOut ? 'opacity-50' : '',
        ].join(' ')}
      >
        {initial}
      </div>

      {/* Name + description */}
      <div className={['flex-1 min-w-0', isGrayedOut ? 'opacity-50' : ''].join(' ')}>
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
