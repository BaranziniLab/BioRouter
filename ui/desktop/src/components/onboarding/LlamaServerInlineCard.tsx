import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useConfig } from '../ConfigContext';
import { toastService } from '../../toasts';
import { Button } from '../ui/button';
import {
  llamacppEnsure,
  llamacppStatus,
  llamacppWarmup,
  type LlamaCppModel,
  type LlamaCppSystemInfo,
  type SidecarStatus,
} from '../../api';
import OnboardingSectionLabel from './OnboardingSectionLabel';

interface LlamaServerInlineCardProps {
  onSuccess: () => void;
}

const POLL_INTERVAL_MS = 1500;

const acceleratorMemoryLabel = (kind: string | undefined) =>
  kind === 'apple_unified' ? 'unified memory' : 'VRAM';

const acceleratorMemoryExplanation = (kind: string | undefined) =>
  kind === 'apple_unified'
    ? 'On Apple Silicon, unified memory is the relevant GPU memory budget.'
    : 'On Intel Macs, Windows, and other discrete-GPU systems, this means VRAM, not regular system RAM.';

const modelDownloadLabel = (model: LlamaCppModel | undefined) => {
  switch (model?.download_status) {
    case 'downloaded':
      return model.download_source === 'ollama' ? 'Downloaded in Ollama' : 'Downloaded';
    case 'partial':
      return 'Partial download';
    default:
      return 'Needs download';
  }
};

const modelDownloadNote = (model: LlamaCppModel | undefined) => {
  switch (model?.download_status) {
    case 'downloaded':
      return model.download_source === 'ollama'
        ? 'Already pulled by Ollama; Llama Server tries that local blob first and falls back to a compatible GGUF if needed.'
        : 'Already downloaded on this machine; startup should only need model load and warm-up.';
    case 'partial':
      return 'A previous download is incomplete; startup will resume or redownload before loading.';
    default:
      return 'Not downloaded yet; first startup may take a while before warm-up can run.';
  }
};

const fallbackDownloadLabel = (model: LlamaCppModel | undefined) => {
  switch (model?.fallback_download_status) {
    case 'downloaded':
      return 'Fallback ready';
    case 'partial':
      return 'Fallback partial';
    case 'not_downloaded':
      return model?.ollama_name ? 'Fallback may download' : null;
    default:
      return null;
  }
};

const modelFitLabel = (model: LlamaCppModel | undefined) => {
  switch (model?.suitability_status) {
    case 'suitable':
      return 'Recommended here';
    case 'above_recommendation':
      return `Needs ${model.recommended_gpu_memory_gib} GiB GPU memory`;
    case 'unknown_resources':
      return 'VRAM unknown';
    default:
      return null;
  }
};

export default function LlamaServerInlineCard({ onSuccess }: LlamaServerInlineCardProps) {
  const navigate = useNavigate();
  const { upsert } = useConfig();
  const [isChecking, setIsChecking] = useState(true);
  const [sidecar, setSidecar] = useState<SidecarStatus | null>(null);
  const [catalog, setCatalog] = useState<LlamaCppModel[]>([]);
  const [system, setSystem] = useState<LlamaCppSystemInfo | null>(null);
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
        setSystem(res.data.system);
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
    if (selectedEntry && system) {
      const detected = system.accelerator_memory_gib;
      const exceedsRecommendation =
        typeof detected !== 'number' || detected < selectedEntry.recommended_gpu_memory_gib;
      if (exceedsRecommendation) {
        const detectedText =
          typeof detected === 'number'
            ? `${detected} GiB ${acceleratorMemoryLabel(system.accelerator_memory_kind)}`
            : `unknown ${acceleratorMemoryLabel(system.accelerator_memory_kind)}`;
        const proceed = window.confirm(
          `${selectedEntry.display_name} recommends ${selectedEntry.recommended_gpu_memory_gib} GiB GPU-addressable memory.\n\nThis machine reports ${detectedText}.\n\n${acceleratorMemoryExplanation(system.accelerator_memory_kind)}\n\nLoad this model anyway?`
        );
        if (!proceed) {
          return;
        }
      }
    }

    setIsStarting(true);
    try {
      const res = await llamacppEnsure({ body: { model }, throwOnError: true });
      setSidecar(res.data.sidecar);
      setSystem(res.data.system);
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
        setSystem(res.data.system);
        if (res.data.sidecar.state === 'error') {
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

    try {
      const warmed = await llamacppWarmup({ body: { model }, throwOnError: true });
      if (!warmed.data.output.trim()) {
        throw new Error('Llama Server returned an empty warm-up response');
      }
      stopPolling();
      setSidecar(warmed.data.sidecar);
      setIsStarting(false);
      await connect(model);
    } catch (error) {
      stopPolling();
      setIsStarting(false);
      toastService.error({
        title: 'Llama Server warm-up failed',
        msg: error instanceof Error ? error.message : String(error),
        traceback: error instanceof Error ? error.stack || '' : '',
      });
    }
  };

  const selectedEntry = catalog.find((m) => m.name === selectedModel);
  const resourceWarnings = useMemo(() => {
    if (!system || !selectedEntry) return [];

    const warnings: string[] = [];
    const detected = system.accelerator_memory_gib;
    const memoryLabel = acceleratorMemoryLabel(system.accelerator_memory_kind);
    if (typeof detected === 'number' && detected < selectedEntry.recommended_gpu_memory_gib) {
      warnings.push(
        `This machine reports ${detected} GiB ${memoryLabel}; ${selectedEntry.display_name} recommends ${selectedEntry.recommended_gpu_memory_gib} GiB GPU-addressable memory.`
      );
    } else if (detected == null) {
      warnings.push(
        `BioRouter could not detect VRAM; ${selectedEntry.display_name} recommends ${selectedEntry.recommended_gpu_memory_gib} GiB GPU-addressable memory.`
      );
    }
    if (selectedEntry.recommended_gpu_memory_gib > 16) {
      warnings.push(
        `This model is above the 16 GB laptop tier; Gemma 4 is the laptop default. ${acceleratorMemoryExplanation(system.accelerator_memory_kind)}`
      );
    }
    if (system.os.toLowerCase().includes('windows')) {
      warnings.push(
        'Windows needs enough free VRAM for the selected model and context window; regular system RAM does not satisfy the GPU memory recommendation.'
      );
    }
    return warnings;
  }, [selectedEntry, system]);
  const isRunningForSelected = sidecar?.state === 'ready' && sidecar.model === selectedModel;
  const isReadyForSelected = isRunningForSelected && sidecar?.warmed;
  const binaryMissing = sidecar?.state === 'no_binary';
  const startButtonLabel = (() => {
    if (isStarting) return 'Setting up…';
    if (isRunningForSelected) return 'Warm up model';
    if (
      selectedEntry?.download_status === 'downloaded' &&
      selectedEntry.fallback_download_status === 'not_downloaded'
    ) {
      return 'Run & warm up (fallback may download)';
    }
    if (selectedEntry?.download_status === 'downloaded') return 'Run & warm up';
    if (selectedEntry?.download_status === 'partial') {
      return `Resume download & run (${selectedEntry.download_size})`;
    }
    return `Download & run${selectedEntry ? ` (${selectedEntry.download_size})` : ''}`;
  })();

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
    if (isRunningForSelected) {
      return (
        <span className="inline-flex items-center gap-1.5 text-[11px] text-text-muted">
          <span className="w-1.5 h-1.5 rounded-full bg-background-warning" />
          Running · warm-up needed
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
              className="h-9 px-2 rounded-md border border-border-subtle bg-background-default text-sm text-text-default focus:border-border-strong transition-colors duration-150"
            >
              {catalog.map((m) => (
                <option key={m.name} value={m.name}>
                  {[
                    m.display_name,
                    m.download_size,
                    modelDownloadLabel(m),
                    fallbackDownloadLabel(m),
                    modelFitLabel(m),
                  ]
                    .filter(Boolean)
                    .join(' · ')}
                </option>
              ))}
            </select>
          </div>
          {selectedEntry && (
            <div className="space-y-1">
              <p className="text-[11px] text-text-muted">{selectedEntry.description}</p>
              <p className="text-[11px] text-text-muted">
                {modelDownloadLabel(selectedEntry)} · {modelDownloadNote(selectedEntry)}
              </p>
              {fallbackDownloadLabel(selectedEntry) && (
                <p className="text-[11px] text-text-muted">
                  Llama Server fallback: {fallbackDownloadLabel(selectedEntry)} ·{' '}
                  {selectedEntry.hf_spec}
                </p>
              )}
              <p className="text-[11px] text-text-muted">
                {selectedEntry.ollama_name
                  ? `Ollama model: ${selectedEntry.ollama_name}`
                  : `Fallback model: ${selectedEntry.hf_spec}`}
              </p>
              {selectedEntry.model_path && (
                <p className="truncate font-mono text-[11px] text-text-muted">
                  {selectedEntry.model_path}
                </p>
              )}
              {system?.model_cache_dir && (
                <p className="truncate font-mono text-[11px] text-text-muted">
                  Store: {system.model_cache_dir}
                </p>
              )}
              {selectedEntry.suitability_message && (
                <p className="text-[11px] text-text-muted">{selectedEntry.suitability_message}</p>
              )}
            </div>
          )}
          {resourceWarnings.length > 0 && (
            <div className="space-y-1 rounded-md border border-border-warning bg-background-warning/10 p-2 text-[11px] text-text-default">
              {resourceWarnings.map((warning) => (
                <p key={warning}>{warning}</p>
              ))}
            </div>
          )}

          {isStarting && (
            <div className="rounded-md border border-border-subtle bg-background-default p-3">
              <div className="flex items-center gap-2 text-xs text-text-default">
                <div className="w-3 h-3 border-2 border-current border-t-transparent rounded-full animate-spin flex-shrink-0" />
                <span>
                  {sidecar?.state === 'ready' && sidecar.model === selectedModel
                    ? `Running warm-up prompt for ${selectedModel}...`
                    : sidecar?.state === 'starting'
                      ? `Preparing ${selectedModel} — loading or downloading on first use…`
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
                {startButtonLabel}
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
