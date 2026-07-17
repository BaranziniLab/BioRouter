import { useEffect, useState, useMemo, useRef, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';
import { useConfig } from './ConfigContext';
import { BioRouterMark } from './icons/BioRouterMark';
import { toastService } from '../toasts';
import InstitutionalSetupCard from './onboarding/InstitutionalSetupCard';
import LlamaServerInlineCard from './onboarding/LlamaServerInlineCard';
import OllamaInlineCard from './onboarding/OllamaInlineCard';
import CommercialSetupCard from './onboarding/CommercialSetupCard';
import type { DetectedProviderSetup } from './onboarding/CommercialSetupCard';
import { SwitchModelModal } from './settings/models/subcomponents/SwitchModelModal';
import { createNavigationHandler } from '../utils/navigationUtils';


interface ProviderGuardProps {
  didSelectProvider: boolean;
  children: React.ReactNode;
}

export default function ProviderGuard({ didSelectProvider, children }: ProviderGuardProps) {
  const { read, upsert } = useConfig();
  const navigate = useNavigate();
  const [isChecking, setIsChecking] = useState(true);
  const [hasProvider, setHasProvider] = useState(false);
  const [showFirstTimeSetup, setShowFirstTimeSetup] = useState(false);
  const [userInActiveSetup, setUserInActiveSetup] = useState(false);
  const [showSwitchModelModal, setShowSwitchModelModal] = useState(false);
  const [switchModelProvider, setSwitchModelProvider] = useState<string | null>(null);
  const [switchModel, setSwitchModel] = useState<string | null>(null);
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const [showScrollIndicator, setShowScrollIndicator] = useState(true);

  const checkScrollPosition = useCallback(() => {
    const container = scrollContainerRef.current;
    if (!container) return;

    const { scrollTop, scrollHeight, clientHeight } = container;
    const isNearBottom = scrollTop + clientHeight >= scrollHeight - 50;
    const canScroll = scrollHeight > clientHeight;

    setShowScrollIndicator(canScroll && !isNearBottom);
  }, []);

  const setView = useMemo(() => createNavigationHandler(navigate), [navigate]);

  const handleCommercialSuccess = async ({
    provider,
    model,
    apiKey,
    apiKeyConfigKey,
    extraConfig,
  }: DetectedProviderSetup) => {
    await upsert(apiKeyConfigKey, apiKey, true);
    // Persist any non-secret endpoint config (e.g. a regional host) so the saved
    // provider targets the same endpoint detection validated against.
    for (const [key, value] of Object.entries(extraConfig)) {
      await upsert(key, value, false);
    }
    await upsert('BIOROUTER_PROVIDER', provider, false);
    setSwitchModelProvider(provider);
    setSwitchModel(model || null);
    setShowSwitchModelModal(true);
  };

  const handleInstitutionalSuccess = (provider: string) => {
    setSwitchModelProvider(provider);
    setShowSwitchModelModal(true);
  };

  const handleModelSelected = (_model: string) => {
    setShowSwitchModelModal(false);
    setUserInActiveSetup(false);
    setShowFirstTimeSetup(false);
    setHasProvider(true);
    navigate('/', { replace: true });
  };

  const handleSwitchModelClose = () => {
    setShowSwitchModelModal(false);
  };

  const handleOllamaComplete = () => {
    setUserInActiveSetup(false);
    setShowFirstTimeSetup(false);
    setHasProvider(true);
    navigate('/', { replace: true });
  };

  const handleLlamaServerComplete = () => {
    setUserInActiveSetup(false);
    setShowFirstTimeSetup(false);
    setHasProvider(true);
    navigate('/', { replace: true });
  };

  useEffect(() => {
    const checkProvider = async () => {
      try {
        const provider = ((await read('BIOROUTER_PROVIDER', false)) as string) || '';
        const hasConfiguredProvider = provider.trim() !== '';

        if (userInActiveSetup) {
          setHasProvider(false);
          setShowFirstTimeSetup(true);
        } else if (hasConfiguredProvider || didSelectProvider) {
          setHasProvider(true);
          setShowFirstTimeSetup(false);
        } else {
          setHasProvider(false);
          setShowFirstTimeSetup(true);
        }
      } catch (error) {
        console.error('Error checking provider:', error);
        toastService.error({
          title: 'Configuration Error',
          msg: 'Failed to check provider configuration.',
          traceback: error instanceof Error ? error.stack || '' : '',
        });
        setHasProvider(false);
        setShowFirstTimeSetup(true);
      } finally {
        setIsChecking(false);
      }
    };

    checkProvider();
  }, [read, didSelectProvider, userInActiveSetup]);

  useEffect(() => {
    if (!isChecking && !hasProvider && showFirstTimeSetup) {
      const timer = setTimeout(checkScrollPosition, 100);
      return () => clearTimeout(timer);
    }
    return undefined;
  }, [isChecking, hasProvider, showFirstTimeSetup, checkScrollPosition]);

  if (isChecking) {
    return (
      <div className="h-screen w-full bg-background-muted flex items-center justify-center">
        <BioRouterMark className="h-20 w-20" />
      </div>
    );
  }

  if (!hasProvider && showFirstTimeSetup) {
    return (
      <div className="fixed inset-0 flex flex-col overflow-hidden bg-background-muted">
        {/* Flat page header */}
        <div className="flex-shrink-0 border-b border-border-subtle px-5 pb-5 pt-8 sm:px-8 sm:pb-6 sm:pt-10">
          <div className="max-w-2xl mx-auto">
            <div className="mb-3 sm:mb-4 biorouter-icon-animation origin-bottom-left">
              <BioRouterMark className="size-8" />
            </div>
            <h1 className="text-2xl font-semibold tracking-tight text-text-default">
              Welcome to Biorouter
            </h1>
            <p className="text-sm text-text-muted mt-1.5 leading-relaxed">
              An integrated research environment that connects local, institution-hosted, and
              commercial AI models in one interface, built for biomedical discovery.
            </p>
          </div>
        </div>

        {/* Scrollable body */}
        <div
          ref={scrollContainerRef}
          onScroll={checkScrollPosition}
          className="flex-1 min-h-0 overflow-y-auto bg-background-muted"
        >
          <div className="mx-auto max-w-2xl space-y-4 px-4 py-4 sm:px-8 sm:py-6">
            <LlamaServerInlineCard onSuccess={handleLlamaServerComplete} />
            <OllamaInlineCard onSuccess={handleOllamaComplete} />
            <InstitutionalSetupCard
              onSuccess={handleInstitutionalSuccess}
              onStartTesting={() => setUserInActiveSetup(true)}
            />
            <CommercialSetupCard
              onSuccess={handleCommercialSuccess}
              onStartTesting={() => setUserInActiveSetup(true)}
            />
          </div>
        </div>

        {/* Scroll indicator */}
        <div className="h-10 flex-shrink-0 bg-background-muted flex items-center justify-center">
          <div
            className={`pointer-events-none transition-opacity duration-300 ${
              showScrollIndicator ? 'opacity-50 animate-bounce' : 'opacity-0'
            }`}
          >
            <div className="flex flex-col items-center gap-1 text-text-muted">
              <span className="text-xs">Scroll for more</span>
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M19 9l-7 7-7-7"
                />
              </svg>
            </div>
          </div>
        </div>

        {showSwitchModelModal && (
          <SwitchModelModal
            sessionId={null}
            onClose={handleSwitchModelClose}
            setView={setView}
            onModelSelected={handleModelSelected}
            initialProvider={switchModelProvider}
            initialModel={switchModel}
            titleOverride="Choose Model"
          />
        )}
      </div>
    );
  }

  return <>{children}</>;
}
