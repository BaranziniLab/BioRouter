import { useEffect, useRef, useState } from 'react';
import { Clipboard } from 'lucide-react';
import type { ModelRef } from '../../../api/types.gen';
import { checkModel } from '../../../api/sdk.gen';
import { toastError, toastSuccess } from '../../../toasts';
import { Button } from '../../ui/button';
import { DispatchProgress } from '../DispatchProgress';
import { useKnowledge } from '../KnowledgeContext';
import { expandKnowledgePath, knowledgeFetch } from '../hooks/knowledgeRequest';
import { useIngestStream } from '../hooks/useIngestStream';
import { useStagedSources } from '../hooks/useStagedSources';
import { Dropzone } from './Dropzone';
import { IngestModelPicker } from './IngestModelPicker';
import { IngestWarnings } from './IngestWarnings';
import { PasteTextBox } from './PasteTextBox';
import { StagedList } from './StagedList';
import type { FileDropWarning, StagedFileCandidate } from './fileValidation';
import { validateDroppedFiles } from './fileValidation';

const FALLBACK_MODEL: ModelRef = { provider: 'anthropic', model: 'claude-sonnet-4-6' };

function genId(): string {
  return Date.now().toString(36) + Math.random().toString(36).substring(2, 9);
}

export function IngestPanel() {
  const { activeKbId, activeKb, refresh, triggerGraphRefresh } = useKnowledge();
  const { items, add, remove, update, clear } = useStagedSources();
  const stream = useIngestStream();
  const [showPasteBox, setShowPasteBox] = useState(false);
  const [digestState, setDigestState] = useState<'idle' | 'checking' | 'digesting' | 'stopping'>(
    'idle'
  );
  const [warnings, setWarnings] = useState<FileDropWarning[]>([]);
  const [savingDefaultModel, setSavingDefaultModel] = useState(false);
  const stopRequestedRef = useRef(false);

  const [model, setModel] = useState<ModelRef>(FALLBACK_MODEL);
  useEffect(() => {
    setModel(activeKb?.default_model ?? FALLBACK_MODEL);
  }, [activeKb?.default_model]);

  async function onDefaultModelChange(next: ModelRef) {
    const previous = model;
    setModel(next);
    if (!activeKbId) {
      return;
    }

    setSavingDefaultModel(true);
    try {
      const response = await knowledgeFetch(`/knowledge/bases/${activeKbId}/default-model`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ model: next }),
      });
      if (!response.ok) {
        throw new Error(await response.text());
      }
      await refresh();
      toastSuccess({
        title: 'Knowledge model updated',
        msg: `${next.provider} / ${next.model} will digest staged sources and scheduled knowledge jobs.`,
      });
    } catch (err) {
      setModel(previous);
      toastError({
        title: 'Could not save knowledge model',
        msg: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setSavingDefaultModel(false);
    }
  }

  async function stageExpandedPath(path: string) {
    const expanded = await expandKnowledgePath(path);
    for (const entry of expanded.files) {
      add({
        kind: 'path',
        id: genId(),
        path: entry.path,
        label: entry.relative_path,
        status: 'pending',
      });
    }
    return expanded.warnings.map((warning) => ({
      id: `${warning.title}-${warning.message}-${path}`,
      title: warning.title,
      message: warning.message,
      level: warning.level as 'warning' | 'error',
    }));
  }

  async function onFiles(files: StagedFileCandidate[]) {
    const stagedFileFallbacks: StagedFileCandidate[] = [];
    const expansionWarnings: FileDropWarning[] = [];

    for (const candidate of files) {
      const actualPath =
        candidate.path ||
        (candidate.file && typeof window.electron?.getPathForFile === 'function'
          ? window.electron.getPathForFile(candidate.file)
          : '');

      if (actualPath) {
        try {
          expansionWarnings.push(...(await stageExpandedPath(actualPath)));
          continue;
        } catch (err) {
          expansionWarnings.push({
            id: `${candidate.label ?? candidate.file?.name ?? actualPath}-expand-error`,
            title: 'Could not expand dropped path',
            message: err instanceof Error ? err.message : String(err),
            level: 'error',
          });
          if (candidate.file) {
            stagedFileFallbacks.push(candidate);
          }
          continue;
        }
      }

      if (candidate.file) {
        stagedFileFallbacks.push(candidate);
      }
    }

    const result = validateDroppedFiles(stagedFileFallbacks);
    if (result.warnings.length > 0) {
      setWarnings((existing) =>
        [...expansionWarnings, ...result.warnings, ...existing].slice(0, 8)
      );
    } else if (expansionWarnings.length > 0) {
      setWarnings((existing) => [...expansionWarnings, ...existing].slice(0, 8));
    }
    for (const file of result.accepted) {
      if (!file.file) {
        continue;
      }
      add({ kind: 'file', id: genId(), file: file.file, label: file.label, status: 'pending' });
    }
  }

  async function onPathPickRequested() {
    const selected = await window.electron?.selectFileOrDirectory?.();
    if (!selected) {
      return;
    }

    try {
      const expandedWarnings = await stageExpandedPath(selected);
      if (expandedWarnings.length > 0) {
        setWarnings((existing) => [...expandedWarnings, ...existing].slice(0, 8));
      }
    } catch (err) {
      setWarnings((existing) =>
        [
          {
            id: `path-expand-${Date.now()}`,
            title: 'Could not expand selected path',
            message: err instanceof Error ? err.message : String(err),
            level: 'error' as const,
          },
          ...existing,
        ].slice(0, 8)
      );
    }
  }

  async function onDigest() {
    if (!activeKbId || digestState !== 'idle') return;
    stopRequestedRef.current = false;
    setDigestState('checking');
    const queue = [...items];
    const succeededIds: string[] = [];

    // Pre-flight: confirm the model is reachable before iterating staged items.
    try {
      const res = await checkModel({ body: { model } });
      const data = res.data;
      if (!data?.ok) {
        setDigestState('idle');
        window.alert(
          `Model unreachable: ${data?.error ?? 'unknown'}\n\nPlease switch to a different model.`
        );
        return;
      }
    } catch (err) {
      setDigestState('idle');
      window.alert(
        `Model check failed: ${err instanceof Error ? err.message : String(err)}\n\nPlease verify your provider's credentials and try a different model.`
      );
      return;
    }

    setDigestState('digesting');
    try {
      for (const item of queue) {
        if (stopRequestedRef.current) break;
        if (item.status === 'done') continue;

        if (item.kind === 'file') {
          update(item.id, { status: 'ingesting', error: undefined });
          try {
            const formData = new FormData();
            formData.append('file', item.file);
            formData.append('provider', model.provider);
            formData.append('model', model.model);

            const result = await stream.startMultipart(
              `/knowledge/bases/${activeKbId}/ingest`,
              formData
            );

            if (result.status === 'error') {
              update(item.id, {
                status: 'error',
                error: result.error ?? stream.error ?? 'ingest stream error',
              });
            } else if (result.status === 'aborted') {
              update(item.id, {
                status: 'pending',
                error: 'Stopped before completion.',
              });
              break;
            } else {
              update(item.id, { status: 'done' });
              succeededIds.push(item.id);
              triggerGraphRefresh();
            }
          } catch (err) {
            update(item.id, {
              status: 'error',
              error: err instanceof Error ? err.message : String(err),
            });
          }
          continue;
        }

        if (item.kind === 'path') {
          update(item.id, { status: 'ingesting', error: undefined });
          try {
            const result = await stream.start(`/knowledge/bases/${activeKbId}/ingest`, {
              source: { path: item.path },
              model,
            });

            if (result.status === 'error') {
              update(item.id, {
                status: 'error',
                error: result.error ?? stream.error ?? 'ingest stream error',
              });
            } else if (result.status === 'aborted') {
              update(item.id, {
                status: 'pending',
                error: 'Stopped before completion.',
              });
              break;
            } else {
              update(item.id, { status: 'done' });
              succeededIds.push(item.id);
              triggerGraphRefresh();
            }
          } catch (err) {
            update(item.id, {
              status: 'error',
              error: err instanceof Error ? err.message : String(err),
            });
          }
          continue;
        }

        update(item.id, { status: 'ingesting' });
        try {
          // Build source body — the ingest macro handles raw materialization
          // internally (add_raw_source is called as its first step). Do NOT
          // pre-call addRawSource here; doing so would create a duplicate source.
          const sourceBody =
            item.kind === 'url' ? { url: item.url } : { text: item.text, title: item.title };

          // POST /knowledge/bases/:id/ingest — SSE streamed digestion.
          // The macro materialises the raw source and then runs the sub-agent.
          const result = await stream.start(`/knowledge/bases/${activeKbId}/ingest`, {
            source: sourceBody,
            model,
          });

          if (result.status === 'error') {
            update(item.id, {
              status: 'error',
              error: result.error ?? stream.error ?? 'ingest stream error',
            });
          } else if (result.status === 'aborted') {
            update(item.id, {
              status: 'pending',
              error: 'Stopped before completion.',
            });
            break;
          } else {
            update(item.id, { status: 'done' });
            succeededIds.push(item.id);
            triggerGraphRefresh();
          }
        } catch (err) {
          update(item.id, {
            status: 'error',
            error: err instanceof Error ? err.message : String(err),
          });
        }
      }
    } finally {
      setDigestState('idle');
      // Auto-clear successfully ingested items; keep errors visible for user action.
      for (const id of succeededIds) {
        remove(id);
      }
    }
  }

  function onAbort() {
    stopRequestedRef.current = true;
    setDigestState((current) => (current === 'idle' ? current : 'stopping'));
    stream.abort();
  }

  const canDigest = items.length > 0 && !!activeKbId && digestState === 'idle';
  const digestLabel =
    digestState === 'checking'
      ? 'Checking model…'
      : digestState === 'digesting'
        ? 'Digesting…'
        : digestState === 'stopping'
          ? 'Stopping…'
          : 'Digest Staged Sources';

  return (
    <div className="flex flex-col gap-4 pt-3">
      <Dropzone onFiles={onFiles} onPathPickRequested={() => void onPathPickRequested()} />
      <Button
        data-testid="knowledge-ingest-paste-text"
        type="button"
        variant="secondary"
        size="sm"
        onClick={() => setShowPasteBox(true)}
        className="h-10 w-full border border-border-subtle bg-background-default/74 font-medium transition-colors duration-150 hover:bg-background-medium/82"
      >
        <Clipboard className="mr-1.5 h-4 w-4" />
        Paste text
      </Button>
      <IngestWarnings
        warnings={warnings}
        onDismiss={(id) => setWarnings((current) => current.filter((warning) => warning.id !== id))}
        onClear={() => setWarnings([])}
      />

      {showPasteBox && (
        <PasteTextBox
          onCancel={() => setShowPasteBox(false)}
          onStage={(text, title, urls) => {
            add({ kind: 'text', id: genId(), text, title, status: 'pending' });
            for (const url of urls) add({ kind: 'url', id: genId(), url, status: 'pending' });
            setShowPasteBox(false);
          }}
        />
      )}

      <StagedList items={items} onRemove={remove} onClear={clear} />

      <DispatchProgress state={stream} onAbort={onAbort} />

      <div className="flex flex-col gap-2 pt-1">
        <IngestModelPicker
          value={model}
          onChange={(next) => void onDefaultModelChange(next)}
          disabled={!activeKbId || savingDefaultModel}
          saving={savingDefaultModel}
        />
        <Button
          data-testid="knowledge-digest-button"
          variant="default"
          size="sm"
          disabled={!canDigest}
          onClick={() => void onDigest()}
          className="w-full min-h-9"
        >
          {digestLabel}
        </Button>
      </div>

      {!activeKbId && (
        <p className="text-[11px] text-text-muted text-center">
          Focus or create a knowledge base above to enable digestion.
        </p>
      )}
    </div>
  );
}
