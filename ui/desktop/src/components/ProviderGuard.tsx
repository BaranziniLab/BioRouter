import { useEffect, useState, useMemo, useRef, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';
import { useConfig } from './ConfigContext';
import WelcomeBioRouterLogo from './WelcomeBioRouterLogo';
import { toastService } from '../toasts';
import { OllamaSetup } from './OllamaSetup';
import ApiKeyTester from './ApiKeyTester';
import { SwitchModelModal } from './settings/models/subcomponents/SwitchModelModal';
import { createNavigationHandler } from '../utils/navigationUtils';
import TelemetrySettings from './settings/app/TelemetrySettings';
import {
  trackOnboardingStarted,
  trackOnboardingProviderSelected,
  trackOnboardingCompleted,
  trackOnboardingAbandoned,
} from '../utils/analytics';

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
  const [showOllamaSetup, setShowOllamaSetup] = useState(false);
  const [userInActiveSetup, setUserInActiveSetup] = useState(false);
  const [showSwitchModelModal, setShowSwitchModelModal] = useState(false);
  const [switchModelProvider, setSwitchModelProvider] = useState<string | null>(null);
  const onboardingTracked = useRef(false);
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

  const handleApiKeySuccess = async (provider: string, _model: string, apiKey: string) => {
    trackOnboardingProviderSelected('api_key');
    const keyName = `${provider.toUpperCase()}_API_KEY`;
    await upsert(keyName, apiKey, true);
    await upsert('BIOROUTER_PROVIDER', provider, false);

    setSwitchModelProvider(provider);
    setShowSwitchModelModal(true);
  };

  const handleModelSelected = (model: string) => {
    if (switchModelProvider) {
      trackOnboardingCompleted(switchModelProvider, model);
    }
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
    trackOnboardingCompleted('ollama');
    setShowOllamaSetup(false);
    setShowFirstTimeSetup(false);
    setHasProvider(true);
    navigate('/', { replace: true });
  };

  const handleOllamaCancel = () => {
    trackOnboardingAbandoned('ollama_setup');
    setShowOllamaSetup(false);
  };

  useEffect(() => {
    const checkProvider = async () => {
      try {
        const provider = ((await read('BIOROUTER_PROVIDER', false)) as string) || '';
        const hasConfiguredProvider = provider.trim() !== '';

        // If user is actively testing keys, don't redirect
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
    if (!isChecking && !hasProvider && showFirstTimeSetup && !onboardingTracked.current) {
      trackOnboardingStarted();
      onboardingTracked.current = true;
    }
  }, [isChecking, hasProvider, showFirstTimeSetup]);

  useEffect(() => {
    if (!isChecking && !hasProvider && showFirstTimeSetup) {
      // Check scroll position after content renders
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

  if (showOllamaSetup) {
    return <OllamaSetup onSuccess={handleOllamaComplete} onCancel={handleOllamaCancel} />;
  }

  if (!hasProvider && showFirstTimeSetup) {
    return (
      <div className="h-screen w-full bg-background-muted overflow-hidden relative">
        <div
          ref={scrollContainerRef}
          onScroll={checkScrollPosition}
          className="h-full overflow-y-auto"
        >
          <div className="min-h-full flex flex-col items-center justify-center py-12 px-4">
            <div className="max-w-lg w-full mx-auto">

              {/* Page header */}
              <div className="mb-8">
                <div className="mb-4 biorouter-icon-animation origin-bottom-left">
                  <BioRouter className="size-8" />
                </div>
                <h1 className="text-2xl font-semibold tracking-tight text-text-default">
                  Welcome to BioRouter
                </h1>
                <p className="text-sm text-text-muted mt-2 leading-relaxed max-w-md">
                  UCSF Biorouter unifies commercial, institution-hosted, and local LLMs, AI
                  agents, and customizable workflows into one extensible research environment.
                  Let's connect an AI provider to get started.
                </p>
              </div>

              {/* Quick setup card */}
              <ApiKeyTester
                onSuccess={handleApiKeySuccess}
                onStartTesting={() => setUserInActiveSetup(true)}
              />

              {/* Divider */}
              <div className="relative my-5">
                <div className="absolute inset-0 flex items-center">
                  <div className="w-full border-t border-border-subtle" />
                </div>
                <div className="relative flex justify-center">
                  <span className="bg-background-muted px-3 text-xs text-text-muted">or</span>
                </div>
              </div>

              {/* Other providers */}
              <div className="p-5 rounded-xl border border-border-subtle bg-background-default">
                <p className="text-[11px] font-medium uppercase tracking-wider text-text-muted mb-1">
                  Other Providers
                </p>
                <p className="text-sm text-text-muted mt-1 mb-3">
                  Set up local or institution-hosted models manually through provider settings.
                </p>
                <button
                  onClick={() => navigate('/welcome', { replace: true })}
                  className="text-sm font-medium text-text-default hover:text-text-muted transition-colors duration-150"
                >
                  Go to Provider Settings →
                </button>
              </div>

              {/* Telemetry */}
              <div className="mt-5">
                <TelemetrySettings isWelcome />
              </div>

            </div>
          </div>
        </div>

        {/* Scroll indicator */}
        {showScrollIndicator && (
          <div className="absolute bottom-4 left-1/2 -translate-x-1/2 pointer-events-none transition-opacity duration-300 opacity-50 animate-bounce">
            <div className="flex flex-col items-center gap-1 text-text-muted">
              <span className="text-xs">Scroll for more</span>
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
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
