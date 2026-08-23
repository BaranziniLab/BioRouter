import { useEffect, useState, useMemo, useRef, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';
import { useConfig } from './ConfigContext';
import { ChevronDown } from './icons/app-icons';
import { BioRouterMark } from './icons/BioRouterMark';
import { BioRouterWordmark } from './icons/BioRouterWordmark';
import { toastService } from '../toasts';
import InstitutionalSetupCard from './onboarding/InstitutionalSetupCard';
import LlamaServerInlineCard from './onboarding/LlamaServerInlineCard';
import OllamaInlineCard from './onboarding/OllamaInlineCard';
import CodingAgentInlineCard from './onboarding/CodingAgentInlineCard';
import CommercialSetupCard from './onboarding/CommercialSetupCard';
import type { DetectedProviderSetup } from './onboarding/CommercialSetupCard';
import { SwitchModelModal } from './settings/models/subcomponents/SwitchModelModal';
import { createNavigationHandler } from '../utils/navigationUtils';
import { isBrowserSurface } from '../utils/surface';
import { HostManagedModelPanel } from './privacy/HostManagedModelPanel';
import { HOST_SERVE_COMMAND } from './privacy/hostManagedModelCopy';

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

  /**
   * A card that did its own config writes (including `BIOROUTER_PROVIDER`) and has
   * nothing left but model selection. Shared by the institutional card and the
   * coding-agent card — both write their keys themselves and hand back only the
   * provider id, unlike `CommercialSetupCard` (whose key is a secret this component
   * persists) and the two local cards (which pick their own model).
   */
  const handleProviderReady = (provider: string) => {
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
          title: 'Configuration error',
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
        {/* 84px, not Tailwind's h-20 (80px), to match the pre-React boot
            splash's `.br-mark` exactly. This loader is what the splash hands
            off to, so a 4px difference read as the logo resizing mid-boot.
            The splash centres the same mark on the same point — see the
            `.br-sweep` note in index.html. */}
        <BioRouterMark className="h-[84px] w-[84px]" />
      </div>
    );
  }

  if (!hasProvider && showFirstTimeSetup) {
    /**
     * SD-1's dead end, closed. Every card below writes `BIOROUTER_PROVIDER`, and
     * on a browser-served surface all five of those writes are refused with a
     * 409 whose body is addressed to an AI agent. Offering the picker and then
     * refusing it is the exact failure SD-1 rules out, so the browser gets the
     * one instruction that actually works — go and run it on the host — and the
     * cards are not rendered at all.
     */
    const hostManaged = isBrowserSurface();
    return (
      <div className="fixed inset-0 flex flex-col overflow-hidden bg-background-muted">
        {/* Flat page header */}
        <div className="flex-shrink-0 border-b border-border-subtle px-5 pb-5 pt-8 sm:px-8 sm:pb-6 sm:pt-10">
          <div className="max-w-2xl mx-auto">
            <div className="mb-4 sm:mb-5 biorouter-icon-animation origin-bottom-left">
              <BioRouterWordmark className="h-9 w-auto" />
            </div>
            <h1 className="text-2xl font-semibold tracking-tight text-text-default">
              Welcome to Biorouter
            </h1>
            <p className="text-sm text-text-muted mt-1.5 leading-relaxed">
              {hostManaged
                ? `Biorouter is being served to this browser by ${HOST_SERVE_COMMAND}. One more step is needed on that machine before you can start a chat.`
                : 'An integrated research environment that connects local, institution-hosted, and commercial AI models in one interface, built for biomedical discovery.'}
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
            {hostManaged ? (
              <HostManagedModelPanel />
            ) : (
              <>
                <LlamaServerInlineCard onSuccess={handleLlamaServerComplete} />
                <OllamaInlineCard onSuccess={handleOllamaComplete} />
                <InstitutionalSetupCard
                  onSuccess={handleProviderReady}
                  onStartTesting={() => setUserInActiveSetup(true)}
                />
                {/* Above CommercialSetupCard on purpose: a user who already pays for a
                    coding-agent plan needs no key at all, so the cheaper path is offered
                    before the one that asks them to paste a secret. */}
                <CodingAgentInlineCard onSuccess={handleProviderReady} />
                <CommercialSetupCard
                  onSuccess={handleCommercialSuccess}
                  onStartTesting={() => setUserInActiveSetup(true)}
                />
              </>
            )}
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
              <ChevronDown className="w-4 h-4" />
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
