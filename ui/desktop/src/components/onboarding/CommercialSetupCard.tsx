import { useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { detectProvider, getDetectableProviders } from '../../api';
import { Button } from '../ui/button';
import { ArrowRight } from '../icons/ArrowRight';
import OnboardingSectionLabel from './OnboardingSectionLabel';

interface CommercialSetupCardProps {
  onSuccess: (
    provider: string,
    model: string,
    apiKey: string,
    extraConfig: Record<string, string>
  ) => void;
  onStartTesting?: () => void;
}

interface DetectionResult {
  provider: string;
  model: string;
  totalModels: number;
}

// Fallback list shown if the backend list can't be fetched. The authoritative
// list comes from GET /config/detectable-providers (single source of truth in
// the Rust `auto_detect` module).
const FALLBACK_PROVIDERS = ['OpenAI', 'Anthropic', 'Google', 'Groq', 'xAI', 'z.ai', 'Xiaomi MiMo'];

// Map a backend failure reason code to actionable copy.
function messageForReason(reason: string | null | undefined): { title: string; detail: string } {
  switch (reason) {
    case 'timeout':
      return {
        title: 'Detection timed out',
        detail: 'The provider took too long to respond. Check your connection and try again.',
      };
    case 'network':
      return {
        title: 'Could not reach the provider',
        detail: 'A network error occurred while validating the key. Check your connection.',
      };
    case 'invalid_key':
      return {
        title: 'Key was rejected',
        detail: "The key matched a provider but was rejected. Check that it's complete and active.",
      };
    case 'no_match':
    default:
      return {
        title: 'Could not detect provider',
        detail: "This key didn't match a supported provider. Pick yours from the full list below.",
      };
  }
}

export default function CommercialSetupCard({
  onSuccess,
  onStartTesting,
}: CommercialSetupCardProps) {
  const navigate = useNavigate();
  const [apiKey, setApiKey] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [result, setResult] = useState<DetectionResult | null>(null);
  const [errorReason, setErrorReason] = useState<string | null>(null);
  const [supported, setSupported] = useState<string[]>(FALLBACK_PROVIDERS);
  const inputRef = useRef<HTMLInputElement>(null);

  // Pull the supported-provider list from the backend so this UI never drifts
  // from what detection actually supports.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const response = await getDetectableProviders();
        const names = response.data?.providers?.map((p) => p.display_name).filter(Boolean);
        if (!cancelled && names && names.length > 0) {
          setSupported(names);
        }
      } catch {
        // keep the fallback list
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const testApiKey = async () => {
    const actualValue = inputRef.current?.value || apiKey;
    if (!actualValue.trim()) return;

    onStartTesting?.();
    setIsLoading(true);
    setResult(null);
    setErrorReason(null);

    try {
      const response = await detectProvider({ body: { api_key: actualValue } });
      const data = response.data;

      if (data && data.provider_name) {
        const model = data.default_model ?? data.models?.[0] ?? '';
        const extraConfig = data.extra_config ?? {};
        setResult({
          provider: data.provider_name,
          model,
          totalModels: data.models?.length ?? 0,
        });
        setTimeout(
          () => onSuccess(data.provider_name as string, model, actualValue, extraConfig),
          1500
        );
      } else {
        setErrorReason(data?.reason ?? 'no_match');
      }
    } catch {
      // Transport-level failure (server unreachable, etc.)
      setErrorReason('network');
    } finally {
      setIsLoading(false);
    }
  };

  const hasInput = apiKey.trim().length > 0;
  const canSubmit = hasInput && !isLoading;
  const supportedLabel = supported.join(', ');

  return (
    <section
      aria-labelledby="commercial-setup-title"
      className="min-w-0 overflow-hidden rounded-xl border border-border-subtle bg-background-card p-5 sm:p-6"
    >
      <OnboardingSectionLabel category="commercial" label="Commercial APIs" />
      <h2 id="commercial-setup-title" className="mt-2 text-base font-medium text-text-default">
        Auto-detect from API key
      </h2>
      <p className="text-sm text-text-muted mt-1 mb-5 leading-relaxed">
        Paste a key from {supportedLabel}. We'll detect the provider for you.
      </p>

      <div className="grid min-w-0 grid-cols-[minmax(0,1fr)_auto] gap-2">
        <label htmlFor="commercial-provider-api-key" className="sr-only">
          Commercial provider API key
        </label>
        <input
          id="commercial-provider-api-key"
          ref={inputRef}
          type="password"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          placeholder="Paste your API key here…"
          className="h-9 min-w-0 rounded-md border border-border-subtle bg-background-default px-3 text-sm text-text-default transition-colors duration-150 placeholder:text-text-muted focus:border-border-strong"
          disabled={isLoading}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && canSubmit) testApiKey();
          }}
        />
        <Button
          onClick={testApiKey}
          disabled={!canSubmit}
          aria-label="Detect provider"
          className="h-9 px-3"
        >
          {isLoading ? (
            <div className="w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin" />
          ) : (
            <ArrowRight className="w-4 h-4" />
          )}
        </Button>
      </div>

      {isLoading && (
        <div className="flex items-center gap-2 mt-3 text-xs text-text-muted">
          <div className="w-3 h-3 border-2 border-current border-t-transparent rounded-full animate-spin flex-shrink-0" />
          <span>Detecting provider and validating key…</span>
        </div>
      )}

      {result && (
        <div className="mt-3 text-sm p-3 rounded-md bg-background-success/10 text-text-success border border-border-success/40 flex items-center gap-2">
          <span className="flex-shrink-0">✓</span>
          <div className="flex-1 min-w-0">
            <div className="font-medium">Detected {result.provider}</div>
            <div className="text-text-success text-xs mt-0.5">
              {result.model} · {result.totalModels} models available
            </div>
          </div>
        </div>
      )}

      {errorReason && (
        <div className="mt-3 space-y-2">
          <div className="text-sm p-3 rounded-md bg-background-danger/10 text-text-danger border border-border-danger/40 flex items-center gap-2">
            <span className="flex-shrink-0">✕</span>
            <div className="flex-1">
              <div className="font-medium">{messageForReason(errorReason).title}</div>
              <div className="text-text-danger text-xs mt-0.5">
                {messageForReason(errorReason).detail}
              </div>
            </div>
          </div>
          <ul className="text-xs text-text-muted space-y-1 pl-1">
            <li>· Supported providers: {supportedLabel}</li>
            <li>· Verify the key is active and has sufficient credits</li>
            <li>· For local models, use the Local card above</li>
          </ul>
        </div>
      )}

      <div className="mt-4 border-t border-border-subtle pt-4">
        <button
          type="button"
          onClick={() => navigate('/welcome', { replace: true })}
          className="text-xs text-text-muted hover:text-text-default transition-colors duration-150"
        >
          View all commercial providers →
        </button>
      </div>
    </section>
  );
}
