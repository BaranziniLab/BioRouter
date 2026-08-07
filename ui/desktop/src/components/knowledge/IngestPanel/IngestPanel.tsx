import { useRef, useState } from 'react';
import { Clipboard } from '../../icons/app-icons';
import type { ModelRef } from '../../../api/types.gen';
import { checkModel } from '../../../api/sdk.gen';
import { toastError, toastSuccess } from '../../../toasts';
import { useModelAndProvider } from '../../ModelAndProviderContext';
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
import { resolveIngestModel } from './resolveIngestModel';

function genId(): string {
  return Date.now().toString(36) + Math.random().toString(36).substring(2, 9);
}

export function IngestPanel() {
  const {
    primaryKbId,
    primaryKb,
    loading: basesLoading,
    basesError,
    refresh,
    triggerGraphRefresh,
  } = useKnowledge();
  // The provider/model the app is configured with — the same pair the chat
  // composer's model selector shows.
  const { currentProvider, currentModel, modelConfigStatus } = useModelAndProvider();
  const { items, add, remove, update, clear } = useStagedSources();
  const stream = useIngestStream();
  const [showPasteBox, setShowPasteBox] = useState(false);
  const [digestState, setDigestState] = useState<'idle' | 'checking' | 'digesting' | 'stopping'>(
    'idle'
  );
  const [warnings, setWarnings] = useState<FileDropWarning[]>([]);
  const [savingDefaultModel, setSavingDefaultModel] = useState(false);
  const stopRequestedRef = useRef(false);

  // Only the user's explicit pick lives in state, and it is stamped with the
  // base it was made for. Everything else is derived below, in this render,
  // from this render's inputs.
  //
  // Mirroring the resolved model into state through an effect instead meant the
  // panel spent a commit — and every click landing in it — displaying and
  // dispatching the model belonging to the base or provider the user had just
  // navigated away from. Worse, when neither base carried its own
  // `default_model` the effect's dependencies did not change at all on a base
  // switch, so a model chosen for one base silently became the digest target
  // for the next one.
  const [modelOverride, setModelOverride] = useState<{ kbId: string; model: ModelRef } | null>(
    null
  );

  // Everything a digest is aimed at comes off the manifest we actually hold,
  // never off the stored pointer — an id with nothing behind it is precisely
  // the stale-or-deleted id a digest must not be dispatched to.
  const dispatchKbId = primaryKb?.id || null;

  // A primary id with no manifest behind it. The base's own `default_model`
  // outranks the app config, so until the manifest arrives there is nothing to
  // resolve *from*: falling through to the app's would name, and dispatch, a
  // model this base may override — at an id nothing has confirmed still exists.
  // This is a property of the manifest, not of the request that would have
  // carried it: a list read that FAILED ends with `basesLoading` false and the
  // stored id retained, which is exactly when a stale id is most likely.
  const primaryKbUnresolved = Boolean(primaryKbId) && !dispatchKbId;
  // Three states, three different things to tell the user, and only the last is
  // a verdict on their setup: still arriving; arrived and this base is not in
  // it (or the read failed); everything resolved and nothing is configured.
  const kbPending = primaryKbUnresolved && basesLoading;
  const kbUnavailable = primaryKbUnresolved && !basesLoading;

  // The base's own default wins; otherwise fall back to the app's configured
  // model. Never a hardcoded vendor — an unresolvable model leaves this null and
  // digestion stays disabled (issue #46).
  const resolvedModel = primaryKbUnresolved
    ? null
    : resolveIngestModel(primaryKb?.default_model, currentProvider, currentModel);
  // An override belongs to the base it was picked for, and an unresolved base
  // has no `dispatchKbId` to match — so it cannot revive a model here either.
  const model =
    modelOverride && modelOverride.kbId === dispatchKbId ? modelOverride.model : resolvedModel;

  // Whether a null `model` means "nothing is configured" or only "not known
  // yet". Both inputs to the resolver arrive asynchronously, and reporting the
  // first while either is in flight told users with a perfectly good
  // configuration to go and set one up.
  const modelPending = !model && (kbPending || modelConfigStatus === 'loading');
  const modelValueState = model
    ? 'resolved'
    : kbUnavailable
      ? 'unavailable'
      : modelPending
        ? 'loading'
        : 'resolved';

  async function onDefaultModelChange(next: ModelRef) {
    // Saving a default to an id whose manifest never arrived writes to a base
    // we have not seen — the same unresolved target the digest guard refuses.
    if (!dispatchKbId) {
      return;
    }
    const kbId = dispatchKbId;
    const previousOverride = modelOverride;
    setModelOverride({ kbId, model: next });

    setSavingDefaultModel(true);
    try {
      const response = await knowledgeFetch(`/knowledge/bases/${kbId}/default-model`, {
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
      setModelOverride(previousOverride);
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
    if (!dispatchKbId || !model || digestState !== 'idle') return;
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
        toastError({
          title: 'Model unreachable',
          msg: `${data?.error ?? 'Unknown model error'}. Please switch to a different model.`,
        });
        return;
      }
    } catch (err) {
      setDigestState('idle');
      toastError({
        title: 'Model check failed',
        msg: `${err instanceof Error ? err.message : String(err)}. Verify your provider credentials and try a different model.`,
      });
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
              `/knowledge/bases/${dispatchKbId}/ingest`,
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
            const result = await stream.start(`/knowledge/bases/${dispatchKbId}/ingest`, {
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
          const result = await stream.start(`/knowledge/bases/${dispatchKbId}/ingest`, {
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

  const busy = digestState !== 'idle';
  // K-04: the one primary action stays full-opacity even with nothing staged,
  // guarded by a cursor + helper line, so it never trains the eye to ignore a
  // permanently half-lit button.
  const nothingToDigest = !dispatchKbId || !model || items.length === 0;
  const digestBlockedReason = !primaryKbId
    ? 'Choose or create a primary knowledge base above to enable digestion.'
    : // Ahead of every model verdict: with no manifest, "which model" has no
      // answer yet, and "no model is configured" would send the user to fix a
      // configuration that is not what is broken.
      kbUnavailable
      ? basesError
        ? 'Could not load your knowledge bases, so digestion is on hold.'
        : 'This knowledge base is unavailable, so digestion is on hold.'
      : modelPending
        ? 'Checking which model this knowledge base digests with…'
        : !model
          ? 'No model is configured. Choose a model above to enable digestion.'
          : items.length === 0
            ? 'Stage a file to digest.'
            : null;
  const digestLabel =
    digestState === 'checking'
      ? 'Checking model…'
      : digestState === 'digesting'
        ? 'Digesting…'
        : digestState === 'stopping'
          ? 'Stopping…'
          : 'Digest Staged Sources';

  return (
    <div className="flex flex-col">
      <div className="flex flex-col gap-4 p-4">
        <Dropzone onFiles={onFiles} onPathPickRequested={() => void onPathPickRequested()} />
        <Button
          data-testid="knowledge-ingest-paste-text"
          type="button"
          variant="outline"
          size="sm"
          onClick={() => setShowPasteBox(true)}
          className="w-full"
        >
          <Clipboard className="mr-1.5 h-4 w-4" />
          Paste text
        </Button>
        <IngestWarnings
          warnings={warnings}
          onDismiss={(id) =>
            setWarnings((current) => current.filter((warning) => warning.id !== id))
          }
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
      </div>

      <div className="flex flex-col gap-2 border-t border-border-subtle p-4">
        <IngestModelPicker
          value={model}
          valueState={modelValueState}
          onChange={(next) => void onDefaultModelChange(next)}
          disabled={!dispatchKbId || savingDefaultModel}
          saving={savingDefaultModel}
        />
        <Button
          data-testid="knowledge-digest-button"
          variant="default"
          size="sm"
          disabled={busy}
          aria-disabled={nothingToDigest || undefined}
          onClick={() => {
            if (!nothingToDigest && !busy) void onDigest();
          }}
          className={`w-full min-h-9 ${nothingToDigest && !busy ? 'cursor-not-allowed' : ''}`}
        >
          {digestLabel}
        </Button>
        {digestBlockedReason && !busy && (
          <p
            className="text-center text-supporting text-text-muted"
            // Only where it explains the line being shown — hung on an
            // unrelated reason it is a tooltip about someone else's problem.
            title={(kbUnavailable && basesError) || undefined}
          >
            {digestBlockedReason}
            {/* The one state the user can act on from here: re-read the list.
                Offered for a missing base too — a base that came back, or one
                the prune has since cleared, both settle this line. */}
            {kbUnavailable && (
              <button
                data-testid="knowledge-ingest-retry"
                type="button"
                onClick={() => void refresh()}
                className="ml-1 underline underline-offset-2 transition-colors duration-[var(--motion-fast)] hover:text-text-default"
              >
                Retry
              </button>
            )}
          </p>
        )}
      </div>
    </div>
  );
}
