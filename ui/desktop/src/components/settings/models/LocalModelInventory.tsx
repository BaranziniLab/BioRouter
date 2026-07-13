import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  llamacppDelete,
  llamacppEnsure,
  llamacppStatus,
  llamacppWarmup,
  type LlamaCppModel,
  type LlamaCppStatusResponse,
} from '../../../api';
import { toastService } from '../../../toasts';
import {
  checkOllamaStatus,
  deleteOllamaModel,
  pullOllamaModel,
  type PullProgress,
} from '../../../utils/ollamaDetection';
import { Button } from '../../ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../../ui/dialog';
import {
  AlertTriangle,
  Download,
  ExternalLink,
  Eye,
  Loader2,
  Play,
  RefreshCw,
  Trash2,
} from '../../icons/app-icons';

const POLL_INTERVAL_MS = 1500;
const INSTALL_TIMEOUT_MS = 60 * 60 * 1000;

const formatContext = (value: number | null | undefined) =>
  typeof value === 'number' ? value.toLocaleString() : 'unknown';

const acceleratorMemoryLabel = (kind: string | undefined) =>
  kind === 'apple_unified' ? 'unified memory' : 'VRAM';

const acceleratorMemoryExplanation = (kind: string | undefined) =>
  kind === 'apple_unified'
    ? 'On Apple Silicon, unified memory is the relevant GPU memory budget.'
    : 'On Intel Macs, Windows, and other discrete-GPU systems, this means VRAM, not regular system RAM.';

const isInstalled = (model: LlamaCppModel) =>
  model.download_status === 'downloaded' || model.fallback_download_status === 'downloaded';

const installedLabel = (model: LlamaCppModel) => {
  if (model.download_status === 'downloaded') {
    return model.download_source === 'ollama' ? 'Downloaded in Ollama' : 'Downloaded';
  }
  if (model.fallback_download_status === 'downloaded') return 'Fallback ready';
  if (model.download_status === 'partial' || model.fallback_download_status === 'partial') {
    return 'Partial download';
  }
  return 'Needs download';
};

const fitLabel = (model: LlamaCppModel) => {
  switch (model.suitability_status) {
    case 'suitable':
      return 'Recommended';
    case 'above_recommendation':
      return `${model.recommended_gpu_memory_gib} GiB GPU memory`;
    case 'unknown_resources':
      return 'VRAM unknown';
    default:
      return 'Unknown';
  }
};

const progressLabel = (progress: PullProgress) => {
  if (progress.total && progress.completed) {
    const pct = Math.round((progress.completed / progress.total) * 100);
    return `${progress.status} ${pct}%`;
  }
  return progress.status;
};

const compactStatusMessage = (message: string) => {
  const compacted = message.replace(/\s+/g, ' ').trim();
  if (compacted.length <= 180) return compacted;
  return `${compacted.slice(0, 177)}...`;
};

function DetailRow({
  label,
  children,
  mono = false,
}: {
  label: string;
  children: React.ReactNode;
  mono?: boolean;
}) {
  return (
    <div className="grid grid-cols-[minmax(8rem,auto)_minmax(0,1fr)] gap-3 text-xs">
      <span className="text-text-muted">{label}</span>
      <span
        className={[
          'min-w-0 text-right text-text-default',
          mono ? 'break-all font-mono text-[11px] leading-relaxed' : 'break-words',
        ]
          .filter(Boolean)
          .join(' ')}
      >
        {children}
      </span>
    </div>
  );
}

export default function LocalModelInventory() {
  const [snapshot, setSnapshot] = useState<LlamaCppStatusResponse | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedModel, setSelectedModel] = useState<LlamaCppModel | null>(null);
  const [activeAction, setActiveAction] = useState<string | null>(null);
  const [actionMessage, setActionMessage] = useState<string | null>(null);
  const pollRef = useRef<number | null>(null);

  const stopPolling = useCallback(() => {
    if (pollRef.current !== null) {
      window.clearInterval(pollRef.current);
      pollRef.current = null;
    }
  }, []);

  const refresh = useCallback(async () => {
    try {
      setError(null);
      const res = await llamacppStatus({ throwOnError: true });
      setSnapshot(res.data);
      return res.data;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      return null;
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
    return stopPolling;
  }, [refresh, stopPolling]);

  const catalog = useMemo(() => snapshot?.catalog ?? [], [snapshot]);
  const installedCount = useMemo(() => catalog.filter(isInstalled).length, [catalog]);

  const confirmResources = useCallback(
    (model: LlamaCppModel) => {
      const system = snapshot?.system;
      if (!system || model.suitability_status === 'suitable') return true;

      const detected =
        typeof system.accelerator_memory_gib === 'number'
          ? `${system.accelerator_memory_gib} GiB ${acceleratorMemoryLabel(
              system.accelerator_memory_kind
            )}`
          : `unknown ${acceleratorMemoryLabel(system.accelerator_memory_kind)}`;
      return window.confirm(
        `${model.display_name} recommends ${model.recommended_gpu_memory_gib} GiB GPU-addressable memory.\n\nThis machine reports ${detected}.\n\n${acceleratorMemoryExplanation(system.accelerator_memory_kind)}\n\nContinue anyway?`
      );
    },
    [snapshot?.system]
  );

  const waitForInstall = useCallback(
    async (model: LlamaCppModel) => {
      const startedAt = Date.now();
      return new Promise<void>((resolve, reject) => {
        stopPolling();
        pollRef.current = window.setInterval(async () => {
          try {
            const res = await llamacppStatus({ throwOnError: true });
            setSnapshot(res.data);
            if (res.data.sidecar.detail) {
              setActionMessage(compactStatusMessage(res.data.sidecar.detail));
            }
            if (res.data.sidecar.state === 'error') {
              stopPolling();
              reject(new Error(res.data.sidecar.detail || 'Llama Server failed to install model'));
              return;
            }
            if (res.data.sidecar.state === 'ready' && res.data.sidecar.model === model.name) {
              stopPolling();
              resolve();
              return;
            }
            if (Date.now() - startedAt > INSTALL_TIMEOUT_MS) {
              stopPolling();
              reject(new Error('Timed out waiting for the local model install'));
            }
          } catch {
            if (Date.now() - startedAt > INSTALL_TIMEOUT_MS) {
              stopPolling();
              reject(new Error('Timed out waiting for the local model install'));
            }
          }
        }, POLL_INTERVAL_MS);
      });
    },
    [stopPolling]
  );

  const runInstall = useCallback(
    async (model: LlamaCppModel) => {
      if (!confirmResources(model)) return;
      setActiveAction(`${model.name}:install`);
      setActionMessage('Preparing install...');
      try {
        if (model.ollama_name) {
          const ollama = await checkOllamaStatus();
          if (ollama.isRunning) {
            setActionMessage(`Pulling ${model.ollama_name} from Ollama...`);
            const pulled = await pullOllamaModel(model.ollama_name, (progress) => {
              setActionMessage(progressLabel(progress));
            });
            if (!pulled) throw new Error(`Ollama could not pull ${model.ollama_name}`);
            toastService.success({
              title: 'Local model installed',
              msg: `${model.display_name} was downloaded with Ollama.`,
            });
            await refresh();
            return;
          }
        }

        setActionMessage('Starting Llama Server fallback download...');
        const res = await llamacppEnsure({ body: { model: model.name }, throwOnError: true });
        setSnapshot(res.data);
        await waitForInstall(model);
        toastService.success({
          title: 'Local model installed',
          msg: `${model.display_name} is ready in the Llama Server cache.`,
        });
        await refresh();
      } catch (err) {
        toastService.error({
          title: 'Local model install failed',
          msg: err instanceof Error ? err.message : String(err),
          traceback: err instanceof Error ? err.stack || '' : '',
        });
      } finally {
        stopPolling();
        setActiveAction(null);
        setActionMessage(null);
      }
    },
    [confirmResources, refresh, stopPolling, waitForInstall]
  );

  const runWarmup = useCallback(
    async (model: LlamaCppModel) => {
      if (!confirmResources(model)) return;
      setActiveAction(`${model.name}:warmup`);
      setActionMessage('Warming up model...');
      stopPolling();
      pollRef.current = window.setInterval(async () => {
        try {
          const res = await llamacppStatus({ throwOnError: true });
          setSnapshot(res.data);
          if (res.data.sidecar.detail) {
            setActionMessage(compactStatusMessage(res.data.sidecar.detail));
          }
        } catch {
          // Polling is best-effort while warm-up owns the real error path.
        }
      }, POLL_INTERVAL_MS);

      try {
        const res = await llamacppWarmup({ body: { model: model.name }, throwOnError: true });
        if (!res.data.output.trim()) {
          throw new Error('Llama Server returned an empty warm-up response');
        }
        toastService.success({
          title: 'Local model warmed up',
          msg: `${model.display_name} produced a test response.`,
        });
        await refresh();
      } catch (err) {
        toastService.error({
          title: 'Local model warm-up failed',
          msg: err instanceof Error ? err.message : String(err),
          traceback: err instanceof Error ? err.stack || '' : '',
        });
      } finally {
        stopPolling();
        setActiveAction(null);
        setActionMessage(null);
      }
    },
    [confirmResources, refresh, stopPolling]
  );

  const runDelete = useCallback(
    async (model: LlamaCppModel) => {
      const label = model.ollama_name ?? model.name;
      if (!window.confirm(`Delete ${label} from the local model inventory?`)) return;

      setActiveAction(`${model.name}:delete`);
      setActionMessage('Deleting local model...');
      try {
        let deletedSomething = false;
        if (model.download_source === 'ollama' && model.ollama_name) {
          const ollama = await checkOllamaStatus();
          if (!ollama.isRunning) {
            throw new Error('Ollama must be running to delete an Ollama-managed model.');
          }
          deletedSomething = await deleteOllamaModel(model.ollama_name);
          if (!deletedSomething) throw new Error(`Ollama could not delete ${model.ollama_name}`);
        }

        if (model.fallback_download_status === 'downloaded') {
          const res = await llamacppDelete({ body: { model: model.name }, throwOnError: true });
          deletedSomething = res.data.deleted_fallback_cache || deletedSomething;
          setSnapshot(res.data.status);
        }

        if (!deletedSomething) {
          throw new Error('No cached local files were removed for this model.');
        }
        toastService.success({
          title: 'Local model deleted',
          msg: `${model.display_name} was removed from local storage.`,
        });
        await refresh();
      } catch (err) {
        toastService.error({
          title: 'Local model delete failed',
          msg: err instanceof Error ? err.message : String(err),
          traceback: err instanceof Error ? err.stack || '' : '',
        });
      } finally {
        setActiveAction(null);
        setActionMessage(null);
      }
    },
    [refresh]
  );

  const renderAction = (model: LlamaCppModel) => {
    return (
      <div className="flex shrink-0 flex-wrap items-center justify-end gap-2">
        <Button
          type="button"
          size="xs"
          variant="outline"
          onClick={() => setSelectedModel(model)}
          disabled={!!activeAction}
        >
          <Eye className="h-3 w-3" />
          View Info
        </Button>
        {isInstalled(model) ? (
          <>
            <Button
              type="button"
              size="xs"
              variant="outline"
              onClick={() => void runWarmup(model)}
              disabled={!!activeAction}
            >
              {activeAction === `${model.name}:warmup` ? (
                <Loader2 className="h-3 w-3 animate-spin" />
              ) : (
                <Play className="h-3 w-3" />
              )}
              Warm up
            </Button>
            <Button
              type="button"
              size="xs"
              variant="ghost"
              className="text-text-danger hover:bg-background-danger/10 hover:text-text-danger"
              onClick={() => void runDelete(model)}
              disabled={!!activeAction}
              title="Delete local model"
            >
              {activeAction === `${model.name}:delete` ? (
                <Loader2 className="h-3 w-3 animate-spin" />
              ) : (
                <Trash2 className="h-3 w-3" />
              )}
              Delete
            </Button>
          </>
        ) : (
          <Button
            type="button"
            size="xs"
            onClick={() => void runInstall(model)}
            disabled={!!activeAction}
          >
            {activeAction === `${model.name}:install` ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <Download className="h-3 w-3" />
            )}
            Install
          </Button>
        )}
      </div>
    );
  };

  return (
    <div className="biorouter-settings-section">
      <div className="biorouter-settings-section-header flex items-center justify-between gap-3">
        <div>
          <h2 className="text-[11px] font-medium text-text-muted uppercase tracking-wider">
            Local Model Inventory
          </h2>
          <p className="mt-1 text-xs text-text-muted">
            {isLoading
              ? 'Checking local models...'
              : `${installedCount} installed · ${catalog.length} available`}
          </p>
        </div>
        <Button
          type="button"
          size="xs"
          shape="round"
          variant="ghost"
          onClick={() => void refresh()}
          disabled={isLoading || !!activeAction}
          title="Refresh local model inventory"
        >
          <RefreshCw className={isLoading ? 'h-3 w-3 animate-spin' : 'h-3 w-3'} />
        </Button>
      </div>

      <div className="biorouter-settings-list">
        {error && (
          <div className="biorouter-settings-row flex items-start gap-2 px-3 py-3 text-xs text-text-warning">
            <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
            <span>{error}</span>
          </div>
        )}

        {isLoading ? (
          <div className="biorouter-settings-row px-3 py-3">
            <div className="h-4 w-56 animate-pulse rounded bg-background-medium" />
            <div className="mt-2 h-3 w-80 animate-pulse rounded bg-background-medium" />
          </div>
        ) : (
          catalog.map((model) => {
            const busyLabel =
              activeAction?.startsWith(`${model.name}:`) && actionMessage
                ? compactStatusMessage(actionMessage)
                : null;

            return (
              <div key={model.name} className="biorouter-settings-row px-3 py-3">
                <div className="flex min-w-0 flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
                  <div className="min-w-0 flex-1">
                    <div className="flex min-w-0 flex-wrap items-center gap-2">
                      <p className="min-w-0 truncate text-sm font-medium text-text-default">
                        {model.display_name}
                      </p>
                      <span
                        className={[
                          'rounded-md px-1.5 py-0.5 text-[10px]',
                          isInstalled(model)
                            ? 'bg-background-success/15 text-text-success'
                            : 'bg-background-medium text-text-muted',
                        ].join(' ')}
                      >
                        {installedLabel(model)}
                      </span>
                      <span
                        className={[
                          'rounded-md px-1.5 py-0.5 text-[10px]',
                          model.suitability_status === 'suitable'
                            ? 'bg-background-success/15 text-text-success'
                            : 'bg-background-warning/15 text-text-warning',
                        ].join(' ')}
                      >
                        {fitLabel(model)}
                      </span>
                    </div>
                    <p className="mt-1 truncate text-xs text-text-muted">
                      {model.family} · {model.download_size} · {formatContext(model.context_limit)}{' '}
                      context · {model.ollama_name ?? model.hf_spec}
                    </p>
                  </div>
                  {renderAction(model)}
                </div>
                {busyLabel && (
                  <div className="mt-3 flex min-w-0 items-start gap-2 rounded-md border border-border-default bg-background-medium/45 px-2.5 py-2 text-xs text-text-muted">
                    <Loader2 className="mt-0.5 h-3 w-3 shrink-0 animate-spin" />
                    <p className="min-w-0 whitespace-normal break-words leading-relaxed">
                      {busyLabel}
                    </p>
                  </div>
                )}
              </div>
            );
          })
        )}
      </div>

      <Dialog open={!!selectedModel} onOpenChange={(open) => !open && setSelectedModel(null)}>
        {selectedModel && (
          <DialogContent className="w-[calc(100vw-2rem)] overflow-hidden sm:max-w-[640px]">
            <DialogHeader>
              <DialogTitle>{selectedModel.display_name}</DialogTitle>
              <DialogDescription>{selectedModel.description}</DialogDescription>
            </DialogHeader>

            <div className="max-h-[58vh] space-y-2 overflow-y-auto pr-1">
              <DetailRow label="Status">{installedLabel(selectedModel)}</DetailRow>
              <DetailRow label="Family">{selectedModel.family}</DetailRow>
              <DetailRow label="Download">{selectedModel.download_size}</DetailRow>
              <DetailRow label="Context">
                {formatContext(selectedModel.context_limit)} tokens
              </DetailRow>
              <DetailRow label="Minimum memory">
                {selectedModel.min_gpu_memory_gib} GiB GPU-addressable memory
              </DetailRow>
              <DetailRow label="Recommended memory">
                {selectedModel.recommended_gpu_memory_gib} GiB GPU-addressable memory
              </DetailRow>
              <DetailRow label="Detected memory">
                {typeof snapshot?.system.accelerator_memory_gib === 'number'
                  ? `${snapshot.system.accelerator_memory_gib} GiB ${acceleratorMemoryLabel(
                      snapshot.system.accelerator_memory_kind
                    )}`
                  : acceleratorMemoryLabel(snapshot?.system.accelerator_memory_kind)}
              </DetailRow>
              <DetailRow label="Recommendation">{selectedModel.suitability_message}</DetailRow>
              <DetailRow label="Ollama model" mono>
                {selectedModel.ollama_name ?? 'none'}
              </DetailRow>
              <DetailRow label="Fallback GGUF" mono>
                {selectedModel.hf_spec}
              </DetailRow>
              <DetailRow label="Official URL">
                <button
                  type="button"
                  className="inline-flex min-w-0 items-center justify-end gap-1 text-text-accent hover:underline"
                  onClick={() =>
                    window.open(selectedModel.official_url, '_blank', 'noopener,noreferrer')
                  }
                >
                  <span className="truncate">{selectedModel.official_url}</span>
                  <ExternalLink className="h-3 w-3 shrink-0" />
                </button>
              </DetailRow>
              <DetailRow label="Model store" mono>
                {snapshot?.system.model_cache_dir ?? 'unknown'}
              </DetailRow>
              {selectedModel.model_path && (
                <DetailRow label="Model blob" mono>
                  {selectedModel.model_path}
                </DetailRow>
              )}
            </div>

            <DialogFooter className="flex-col gap-2 pt-2 sm:flex-row">
              <Button
                type="button"
                variant="outline"
                className="w-full sm:w-auto"
                onClick={() => setSelectedModel(null)}
              >
                Close
              </Button>
              {isInstalled(selectedModel) ? (
                <Button
                  type="button"
                  className="w-full sm:w-auto"
                  onClick={() => void runWarmup(selectedModel)}
                  disabled={!!activeAction}
                >
                  Warm up model
                </Button>
              ) : (
                <Button
                  type="button"
                  className="w-full sm:w-auto"
                  onClick={() => void runInstall(selectedModel)}
                  disabled={!!activeAction}
                >
                  Install model
                </Button>
              )}
            </DialogFooter>
          </DialogContent>
        )}
      </Dialog>
    </div>
  );
}
