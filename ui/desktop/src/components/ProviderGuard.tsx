import { useEffect, useState, useMemo, useRef, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';
import { useConfig } from './ConfigContext';
import WelcomeBioRouterLogo from './WelcomeBioRouterLogo';
import { toastService } from '../toasts';
import InstitutionalSetupCard from './onboarding/InstitutionalSetupCard';
import LlamaServerInlineCard from './onboarding/LlamaServerInlineCard';
import OllamaInlineCard from './onboarding/OllamaInlineCard';
import CommercialSetupCard from './onboarding/CommercialSetupCard';
import { SwitchModelModal } from './settings/models/subcomponents/SwitchModelModal';
import { createNavigationHandler } from '../utils/navigationUtils';

import { BioRouter } from './icons';

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

  const handleCommercialSuccess = async (provider: string, _model: string, apiKey: string) => {
    const keyName = `${provider.toUpperCase()}_API_KEY`;
    await upsert(keyName, apiKey, true);
    await upsert('BIOROUTER_PROVIDER', provider, false);
    setSwitchModelProvider(provider);
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
      <div className="h-screen w-full bg-background-default flex items-center justify-center">
        <WelcomeBioRouterLogo />
      </div>
    );
  }

  if (!hasProvider && showFirstTimeSetup) {
    return (
      <div className="fixed inset-0 bg-background-muted flex flex-col">
        {/* Flat page header */}
        <div className="px-6 sm:px-8 pt-10 sm:pt-12 pb-5 sm:pb-6 flex-shrink-0 border-b border-border-subtle">
          <div className="max-w-2xl mx-auto">
            <div className="mb-3 sm:mb-4 biorouter-icon-animation origin-bottom-left">
              <BioRouter className="size-8" />
            </div>
            <h1 className="text-2xl font-semibold tracking-tight text-text-default">
              Welcome to BioRouter
            </h1>
            <p className="text-sm text-text-muted mt-1.5 leading-relaxed">
              An integrated research environment that connects local, institution-hosted, and
              commercial AI models in one interface — built for biomedical discovery.
            </p>
          </div>
        </div>

        {/* Scrollable body */}
        <div
          ref={scrollContainerRef}
          onScroll={checkScrollPosition}
          className="flex-1 min-h-0 overflow-y-auto bg-background-muted"
        >
          <div className="max-w-2xl mx-auto px-6 sm:px-8">
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
        {showScrollIndicator && (
          <div className="absolute bottom-4 left-1/2 -translate-x-1/2 pointer-events-none transition-opacity duration-300 opacity-50 animate-bounce">
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
        )}

        {showSwitchModelModal && (
          <SwitchModelModal
            sessionId={null}
            onClose={handleSwitchModelClose}
            setView={setView}
            onModelSelected={handleModelSelected}
            initialProvider={switchModelProvider}
            titleOverride="Choose Model"
          />
        )}
      </div>
    );
  }

  return <>{children}</>;
}
