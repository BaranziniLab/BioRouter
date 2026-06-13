import { useCallback, useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useConfig } from '../ConfigContext';
import { toastService } from '../../toasts';
import { Button } from '../ui/button';
import { llamacppEnsure, llamacppStatus, type LlamaCppModel, type SidecarStatus } from '../../api';
import OnboardingSectionLabel from './OnboardingSectionLabel';

interface LlamaServerInlineCardProps {
  onSuccess: () => void;
}

const POLL_INTERVAL_MS = 1500;

export default function LlamaServerInlineCard({ onSuccess }: LlamaServerInlineCardProps) {
  const navigate = useNavigate();
  const { upsert } = useConfig();
  const [isChecking, setIsChecking] = useState(true);
  const [sidecar, setSidecar] = useState<SidecarStatus | null>(null);
  const [catalog, setCatalog] = useState<LlamaCppModel[]>([]);
  const [selectedModel, setSelectedModel] = useState<string>('');
  const [isStarting, setIsStarting] = useState(false);
  const [isConnecting, setIsConnecting] = useState(false);
  const pollRef = useRef<number | null>(null);
  const connectingRef = useRef(false);

  const stopPolling = useCallback(() => {
    if (pollRef.current !== null) {
      window.clearInterval(pollRef.current);
      pollRef.current = null;
    }
  }, []);

  const connect = useCallback(
    async (model: string) => {
      if (connectingRef.current) return;
      connectingRef.current = true;
      setIsConnecting(true);
      try {
        // Explicitly setting the (defaulted) port marks the provider configured.
        await upsert('LLAMACPP_PORT', '11543', false);
        await upsert('BIOROUTER_PROVIDER', 'llamacpp', false);
        await upsert('BIOROUTER_MODEL', model, false);
        toastService.success({
          title: 'Local model ready!',
          msg: `Llama Server is running ${model} on your computer.`,
        });
        onSuccess();
      } catch (error) {
        connectingRef.current = false;
        setIsConnecting(false);
        toastService.error({
          title: 'Connection Failed',
          msg: `Failed to configure Llama Server: ${error instanceof Error ? error.message : String(error)}`,
          traceback: error instanceof Error ? error.stack || '' : '',
        });
      }
    },
    [onSuccess, upsert]
  );

  useEffect(() => {
    const checkInitial = async () => {
      try {
        const res = await llamacppStatus({ throwOnError: true });
        setSidecar(res.data.sidecar);
        setCatalog(res.data.catalog);
        const defaultModel =
          res.data.catalog.find((m) => m.is_default)?.name ?? res.data.catalog[0]?.name ?? '';
        setSelectedModel((prev) => prev || res.data.sidecar.model || defaultModel);
      } catch (error) {
        console.error('Failed to check Llama Server status:', error);
      } finally {
        setIsChecking(false);
      }
    };
    checkInitial();
    return stopPolling;
  }, [stopPolling]);

  const handleStart = async (model: string) => {
    setIsStarting(true);
    try {
      const res = await llamacppEnsure({ body: { model }, throwOnError: true });
      setSidecar(res.data.sidecar);
    } catch (error) {
      setIsStarting(false);
      toastService.error({
        title: 'Could not start Llama Server',
        msg: error instanceof Error ? error.message : String(error),
        traceback: error instanceof Error ? error.stack || '' : '',
      });
      return;
    }

    stopPolling();
    pollRef.current = window.setInterval(async () => {
      try {
        const res = await llamacppStatus({ throwOnError: true });
        setSidecar(res.data.sidecar);
        if (res.data.sidecar.state === 'ready' && res.data.sidecar.model === model) {
          stopPolling();
          setIsStarting(false);
          await connect(model);
        } else if (res.data.sidecar.state === 'error') {
          stopPolling();
          setIsStarting(false);
          toastService.error({
            title: 'Llama Server failed to start',
            msg: res.data.sidecar.detail || 'Unknown error — see logs.',
            traceback: '',
          });
        }
      } catch {
        // Transient polling errors are fine; keep polling.
      }
    }, POLL_INTERVAL_MS);
  };

  const selectedEntry = catalog.find((m) => m.name === selectedModel);
  const isReadyForSelected = sidecar?.state === 'ready' && sidecar.model === selectedModel;
  const binaryMissing = sidecar?.state === 'no_binary';

  const statusPill = (() => {
    if (isChecking || !sidecar) return null;
    if (binaryMissing) {
      return (
        <span className="inline-flex items-center gap-1.5 text-[11px] text-text-muted">
          <span className="w-1.5 h-1.5 rounded-full bg-background-warning" />
          llama-server binary not found
        </span>
      );
    }
    if (isReadyForSelected) {
      return (
        <span className="inline-flex items-center gap-1.5 text-[11px] text-text-muted">
          <span className="w-1.5 h-1.5 rounded-full bg-background-success" />
          Running · {sidecar.model} ready
        </span>
      );
    }
    return (
      <span className="inline-flex items-center gap-1.5 text-[11px] text-text-muted">
        <span className="w-1.5 h-1.5 rounded-full bg-background-success" />
        Built in · no install needed
      </span>
    );
  })();

  return (
    <section className="py-7 border-b border-border-default">
      <OnboardingSectionLabel category="local" label="Local · Run on your computer" />
      <h2 className="text-base font-medium text-text-default mt-2">Llama Server</h2>
      <p className="text-sm text-text-muted mt-1 mb-5 leading-relaxed">
        Built-in local models — pick one and start chatting in minutes. Free, private, offline,
        nothing else to install.
      </p>

      {isChecking ? (
        <div className="flex items-center gap-2 text-xs text-text-muted">
          <div className="w-3 h-3 border-2 border-current border-t-transparent rounded-full animate-spin flex-shrink-0" />
          <span>Checking Llama Server…</span>
        </div>
      ) : binaryMissing ? (
        <div className="space-y-3">
          <div>{statusPill}</div>
          <p className="text-xs text-text-muted">
            The bundled llama-server binary is missing (development build?). Install llama.cpp (e.g.{' '}
            <code>brew install llama.cpp</code>) or set <code>BIOROUTER_LLAMACPP_BIN</code>.
          </p>
        </div>
      ) : (
        <div className="space-y-3">
          <div>{statusPill}</div>

          <div className="flex flex-wrap items-center gap-2">
            <select
              value={selectedModel}
              onChange={(e) => setSelectedModel(e.target.value)}
              disabled={isStarting || isConnecting}
              data-testid="llamacpp-model-select"
              className="h-9 px-2 rounded-md border border-border-default bg-background-default text-sm text-text-default"
            >
              {catalog.map((m) => (
                <option key={m.name} value={m.name}>
                  {m.display_name} · {m.download_size}
                </option>
              ))}
            </select>
          </div>
          {selectedEntry && (
            <p className="text-[11px] text-text-muted">{selectedEntry.description}</p>
          )}

          {isStarting && (
            <div className="rounded-md border border-border-default bg-background-default p-3">
              <div className="flex items-center gap-2 text-xs text-text-default">
                <div className="w-3 h-3 border-2 border-current border-t-transparent rounded-full animate-spin flex-shrink-0" />
                <span>
                  {sidecar?.state === 'starting'
                    ? `Preparing ${selectedModel} — downloading on first use…`
                    : `Starting llama-server…`}
                </span>
              </div>
              {sidecar?.detail && (
                <p className="text-[11px] text-text-muted mt-1 font-mono truncate">
                  {sidecar.detail}
                </p>
              )}
            </div>
          )}

          <div className="flex flex-wrap items-center gap-x-4 gap-y-3">
            {isReadyForSelected ? (
              <Button
                onClick={() => connect(selectedModel)}
                disabled={isConnecting}
                className="h-9 px-4"
                data-testid="llamacpp-connect"
              >
                {isConnecting ? 'Connecting…' : 'Use Llama Server'}
              </Button>
            ) : (
              <Button
                onClick={() => handleStart(selectedModel)}
                disabled={isStarting || isConnecting || !selectedModel}
                className="h-9 px-4"
                data-testid="llamacpp-start"
              >
                {isStarting
                  ? 'Setting up…'
                  : `Download & run${selectedEntry ? ` (${selectedEntry.download_size})` : ''}`}
              </Button>
            )}
            <button
              type="button"
              onClick={() => navigate('/welcome', { replace: true })}
              className="text-xs text-text-muted hover:text-text-default transition-colors duration-150"
            >
              View all local providers →
            </button>
          </div>
        </div>
      )}
    </section>
  );
}
