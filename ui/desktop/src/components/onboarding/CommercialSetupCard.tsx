import { useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { detectProvider } from '../../api';
import { Button } from '../ui/button';
import { ArrowRight } from '../icons/ArrowRight';
import OnboardingSectionLabel from './OnboardingSectionLabel';

interface CommercialSetupCardProps {
  onSuccess: (provider: string, model: string, apiKey: string) => void;
  onStartTesting?: () => void;
}

interface DetectionResult {
  provider: string;
  model: string;
  totalModels: number;
}

export default function CommercialSetupCard({
  onSuccess,
  onStartTesting,
}: CommercialSetupCardProps) {
  const navigate = useNavigate();
  const [apiKey, setApiKey] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [result, setResult] = useState<DetectionResult | null>(null);
  const [error, setError] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const testApiKey = async () => {
    const actualValue = inputRef.current?.value || apiKey;
    if (!actualValue.trim()) return;

    onStartTesting?.();
    setIsLoading(true);
    setResult(null);
    setError(false);

    try {
      const response = await detectProvider({
        body: { api_key: actualValue },
        throwOnError: true,
      });

      if (response.data) {
        const { provider_name, models } = response.data;
        setResult({ provider: provider_name, model: models[0], totalModels: models.length });
        setTimeout(() => onSuccess(provider_name, models[0], actualValue), 1500);
      }
    } catch {
      setError(true);
    } finally {
      setIsLoading(false);
    }
  };

  const hasInput = apiKey.trim().length > 0;
  const canSubmit = hasInput && !isLoading;

  return (
    <section className="py-7">
      <OnboardingSectionLabel category="commercial" label="Commercial APIs" />
      <h2 className="text-base font-medium text-text-default mt-2">Auto-detect from API key</h2>
      <p className="text-sm text-text-muted mt-1 mb-5 leading-relaxed">
        Paste a key from OpenAI, Anthropic, Google, Groq, or xAI — we'll detect the provider for
        you.
      </p>

      <div className="flex gap-2">
        <input
          ref={inputRef}
          type="password"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          placeholder="Paste your API key here…"
          className="flex-1 h-9 px-3 text-sm border border-border-subtle rounded-md bg-background-default text-text-default placeholder:text-text-muted focus:outline-none focus:border-border-strong transition-colors duration-150"
          disabled={isLoading}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && canSubmit) testApiKey();
          }}
        />
        <Button onClick={testApiKey} disabled={!canSubmit} className="h-9 px-3">
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
        <div className="mt-3 text-sm p-3 rounded-md bg-green-50 dark:bg-green-900/20 text-green-800 dark:text-green-300 border border-green-200 dark:border-green-800/50 flex items-center gap-2">
          <span className="flex-shrink-0">✓</span>
          <div className="flex-1 min-w-0">
            <div className="font-medium">Detected {result.provider}</div>
            <div className="text-green-700 dark:text-green-400 text-xs mt-0.5">
              {result.model} · {result.totalModels} models available
            </div>
          </div>
        </div>
      )}

      {error && (
        <div className="mt-3 space-y-2">
          <div className="text-sm p-3 rounded-md bg-red-50 dark:bg-red-900/20 text-red-800 dark:text-red-300 border border-red-200 dark:border-red-800/50 flex items-center gap-2">
            <span className="flex-shrink-0">✕</span>
            <div className="flex-1">
              <div className="font-medium">Could not detect provider</div>
              <div className="text-red-700 dark:text-red-400 text-xs mt-0.5">
                Check that the key is complete and valid
              </div>
            </div>
          </div>
          <ul className="text-xs text-text-muted space-y-1 pl-1">
            <li>· Supported providers: OpenAI, Anthropic, Google, Groq, xAI</li>
            <li>· Verify the key is active and has sufficient credits</li>
            <li>· For local models, use the Local card above</li>
          </ul>
        </div>
      )}

      <div className="mt-4">
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
