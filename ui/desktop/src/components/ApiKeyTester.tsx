import { useState, useRef } from 'react';
import { detectProvider } from '../api';
import { ArrowRight } from './icons/ArrowRight';
import { Button } from './ui/button';

interface ApiKeyTesterProps {
  onSuccess: (provider: string, model: string, apiKey: string) => void;
  onStartTesting?: () => void;
}

interface DetectionResult {
  provider: string;
  model: string;
  totalModels: number;
}

export default function ApiKeyTester({ onSuccess, onStartTesting }: ApiKeyTesterProps) {
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
    <div className="p-5 rounded-xl border border-border-subtle bg-background-default">
      {/* Card header */}
      <div className="flex items-center justify-between mb-4">
        <p className="text-[11px] font-medium uppercase tracking-wider text-text-muted">
          Quick Setup
        </p>
        <span className="text-[11px] font-medium text-text-muted border border-border-subtle px-2 py-0.5 rounded-md">
          Recommended
        </span>
      </div>

      <p className="text-sm font-medium text-text-default mb-1">Auto-detect from API Key</p>
      <p className="text-xs text-text-muted mb-4 leading-relaxed">
        Enter any API key and we'll automatically detect which provider it belongs to (OpenAI,
        Anthropic, Google, etc.).
      </p>

      {/* Input row */}
      <div className="flex gap-2">
        <input
          ref={inputRef}
          type="password"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          placeholder="Paste your API key here…"
          className="flex-1 h-9 px-3 text-sm border border-border-subtle rounded-lg bg-background-default text-text-default placeholder:text-text-muted focus:outline-none focus:border-border-strong transition-colors duration-150"
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

      {/* Loading */}
      {isLoading && (
        <div className="flex items-center gap-2 mt-3 px-3 py-2 bg-background-muted rounded-lg text-xs text-text-muted">
          <div className="w-3 h-3 border-2 border-current border-t-transparent rounded-full animate-spin flex-shrink-0" />
          <span>Detecting provider and validating key…</span>
        </div>
      )}

      {/* Success */}
      {result && (
        <div className="flex items-center gap-2 mt-3 text-sm p-3 rounded-lg bg-green-50 dark:bg-green-900/20 text-green-800 dark:text-green-300 border border-green-200 dark:border-green-800/50">
          <span className="flex-shrink-0">✓</span>
          <div className="flex-1 min-w-0">
            <div className="font-medium">Detected {result.provider}</div>
            <div className="text-green-700 text-xs mt-0.5">
              {result.model} · {result.totalModels} models available
            </div>
          </div>
        </div>
      )}

      {/* Error */}
      {error && (
        <div className="mt-3 space-y-2">
          <div className="flex items-center gap-2 text-sm p-3 rounded-lg bg-red-50 dark:bg-red-900/20 text-red-800 dark:text-red-300 border border-red-200 dark:border-red-800/50">
            <span className="flex-shrink-0">✕</span>
            <div className="flex-1">
              <div className="font-medium">Could not detect provider</div>
              <div className="text-red-700 text-xs mt-0.5">
                Check that the key is complete and valid
              </div>
            </div>
          </div>
          <ul className="text-xs text-text-muted space-y-1 pl-1">
            <li>· Supported providers: OpenAI, Anthropic, Google, Groq, xAI</li>
            <li>· Verify the key is active and has sufficient credits</li>
            <li>· For local Ollama setup, use "Other Providers" below</li>
          </ul>
        </div>
      )}
    </div>
  );
}
