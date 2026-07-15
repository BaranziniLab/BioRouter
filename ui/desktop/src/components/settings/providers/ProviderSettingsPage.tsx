import { useEffect, useState, useCallback, useRef, useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import { ScrollArea } from '../../ui/scroll-area';
import BackButton from '../../ui/BackButton';
import ProviderGrid from './ProviderGrid';
import { useConfig } from '../../ConfigContext';
import { ProviderDetails } from '../../../api';
import { createNavigationHandler } from '../../../utils/navigationUtils';

interface ProviderSettingsProps {
  onClose: () => void;
  isOnboarding: boolean;
  onProviderLaunched?: (model?: string) => void;
}

export default function ProviderSettings({
  onClose,
  isOnboarding,
  onProviderLaunched,
}: ProviderSettingsProps) {
  const { getProviders } = useConfig();
  const navigate = useNavigate();
  const [loading, setLoading] = useState(true);
  const [providers, setProviders] = useState<ProviderDetails[]>([]);
  const initialLoadDone = useRef(false);

  const setView = useMemo(() => createNavigationHandler(navigate), [navigate]);

  // Create a function to load providers that can be called multiple times
  const loadProviders = useCallback(async () => {
    setLoading(true);
    try {
      // Only force refresh when explicitly requested, not on initial load
      const result = await getProviders(!initialLoadDone.current);
      if (result) {
        setProviders(result);
        initialLoadDone.current = true;
      }
    } catch (error) {
      console.error('Failed to load providers:', error);
    } finally {
      setLoading(false);
    }
  }, [getProviders]);

  // Load providers only once when component mounts
  useEffect(() => {
    loadProviders();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []); // Intentionally not including loadProviders in deps to prevent reloading

  // This function will be passed to ProviderGrid for manual refreshes after config changes
  const refreshProviders = useCallback(() => {
    if (initialLoadDone.current) {
      getProviders(true).then((result) => {
        if (result) setProviders(result);
      });
    }
  }, [getProviders]);

  return (
    <div className="h-screen w-full flex flex-col bg-background-muted text-text-default">
      <ScrollArea className="flex-1 w-full">
        {/* Flat page header */}
        <div className="w-full max-w-3xl mx-auto px-8 pt-10 pb-6 border-b border-border-subtle">
          <div className="flex items-center mb-4 no-drag">
            <BackButton onClick={onClose} />
          </div>
          <h1
            className="text-2xl font-semibold tracking-tight mb-1"
            data-testid="provider-selection-heading"
          >
            {isOnboarding ? 'Other Providers' : 'Provider Configuration'}
          </h1>
          <p className="text-sm text-text-muted">
            {isOnboarding
              ? 'Select a provider to get started. API keys are encrypted and stored locally. You can switch providers any time in settings.'
              : 'Configure your AI model providers. API keys are encrypted and stored locally.'}
          </p>
        </div>

        {/* Provider list */}
        <div className="w-full max-w-3xl mx-auto px-8 py-6">
          {loading ? (
            <div className="text-sm text-text-muted">Loading providers…</div>
          ) : (
            <ProviderGrid
              providers={providers}
              isOnboarding={isOnboarding}
              refreshProviders={refreshProviders}
              setView={setView}
              onModelSelected={onProviderLaunched}
            />
          )}
        </div>
      </ScrollArea>
    </div>
  );
}
