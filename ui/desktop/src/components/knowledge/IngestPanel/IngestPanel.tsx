import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { Clipboard } from '../../icons/app-icons';
import { Progress } from '../../ui/progress';
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
import { ingestModelBlockedByProvider, resolveIngestModel } from './resolveIngestModel';

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
  // The queue's own progress, so the section's LONGEST operation finally has a
  // `role="progressbar"` (ui-spec §4.4 state 3). Determinate on the QUEUE — the
  // denominator is known — while `indeterminate` is reserved for the pre-flight
  // model check, where there genuinely is none.
  const [digestProgress, setDigestProgress] = useState<{
    completed: number;
    total: number;
  } | null>(null);
  const stopRequestedRef = useRef(false);
  // The summoned box and the strip that would otherwise cover it.
  const pasteBoxRef = useRef<HTMLDivElement>(null);
  const footerRef = useRef<HTMLDivElement>(null);

  /**
   * Bring the paste box into view when it opens, clear of the pinned footer.
   *
   * ⚠ **Without this, "Paste text" read as a dead button.** The box mounts at
   * the END of the rail's one scroll container, below the fold at ordinary
   * window heights (measured: y=790 in an 887px window), and the footer is
   * `sticky bottom-0` — so it paints over exactly the region the box lands in.
   * The user clicked, nothing appeared to happen, and the textarea *and* its
   * Stage button were reachable only by scrolling the rail to its end by hand.
   *
   * `scroll-margin-bottom` is what keeps the footer off it, and it is measured
   * rather than guessed: the footer holds the model picker, the primary button
   * and a helper line that comes and goes, so its height is a runtime fact.
   * Written straight onto the node in a layout effect, before the scroll — a
   * state round-trip would scroll against the previous frame's inset.
   *
   * `behavior` honours `prefers-reduced-motion`, and `scrollIntoView` is
   * feature-detected because jsdom does not implement it.
   */
  /**
   * The digest log belongs to the base it ran against.
   *
   * Keyed on `primaryKbId` rather than on `dispatchKbId`, so the log clears the
   * moment the user switches, not when the new base's manifest happens to
   * arrive. `reset` also aborts anything in flight — a stream started against
   * the base we just navigated away from has no surface left to report into.
   */
  const resetStream = stream.reset;
  useEffect(() => {
    resetStream();
  }, [primaryKbId, resetStream]);

  useLayoutEffect(() => {
    if (!showPasteBox) return;
    const box = pasteBoxRef.current;
    if (!box) return;
    const footer = footerRef.current?.getBoundingClientRect().height ?? 0;
    box.style.setProperty('--br-ingest-footer-inset', `${Math.ceil(footer) + 12}px`);
    if (typeof box.scrollIntoView !== 'function') return;
    const reduced = window.matchMedia?.('(prefers-reduced-motion: reduce)')?.matches ?? false;
    box.scrollIntoView({ block: 'end', behavior: reduced ? 'auto' : 'smooth' });
  }, [showPasteBox]);

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
  // A fourth reason `model` can be null, and the only one that is not a gap in
  // the user's setup: everything resolved, but the only candidate was a
  // coding-agent provider a knowledge macro cannot drive. Telling that user
  // "No model configured" sends them to Settings to fix a configuration that is
  // already correct, so the picker needs to be able to tell the two apart.
  const modelBlockedByProvider =
    !primaryKbUnresolved &&
    ingestModelBlockedByProvider(primaryKb?.default_model, currentProvider, currentModel);
  const modelValueState = model
    ? 'resolved'
    : kbUnavailable
      ? 'unavailable'
      : modelPending
        ? 'loading'
        : modelBlockedByProvider
          ? 'unsupported'
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
    // Clear the finished run BEFORE the pre-flight model check, not when the
    // first stream opens: the check is a network round-trip, and until it
    // returns the panel would otherwise still be showing the previous run's
    // "Digest complete" under a progress bar that has already started.
    stream.reset();
    setDigestState('checking');
    const queue = [...items];
    const succeededIds: string[] = [];
    setDigestProgress({ completed: 0, total: queue.filter((i) => i.status !== 'done').length });

    // Pre-flight: confirm the model is reachable before iterating staged items.
    try {
      const res = await checkModel({ body: { model } });
      const data = res.data;
      if (!data?.ok) {
        setDigestState('idle');
        setDigestProgress(null);
        toastError({
          title: 'Model unreachable',
          msg: `${data?.error ?? 'Unknown model error'}. Switch to a different model.`,
        });
        return;
      }
    } catch (err) {
      setDigestState('idle');
      setDigestProgress(null);
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
        // Counted when the item is TAKEN, not when it succeeds: a failure is
        // still one of the N the bar is measuring, and a bar that stalls on an
        // error reads as a hang.
        const advance = () =>
          setDigestProgress((p) => (p ? { ...p, completed: p.completed + 1 } : p));

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
          advance();
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
          advance();
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
        advance();
      }
    } finally {
      setDigestState('idle');
      setDigestProgress(null);
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
          : 'Digest staged sources';

  const failed = items.filter((item) => item.status === 'error');

  return (
    <div className="flex flex-col">
      <div className="flex flex-col gap-4 p-4">
        <Dropzone onFiles={onFiles} onPathPickRequested={() => void onPathPickRequested()} />
        <Button
          data-testid="knowledge-ingest-paste-text"
          type="button"
          variant="secondary"
          onClick={() => setShowPasteBox(true)}
          className="w-full"
        >
          <Clipboard aria-hidden="true" />
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
          // `br-ingest-summoned` carries the scroll margin the effect above
          // fills in. AUTHORED CSS, never an arbitrary utility: a freshly
          // written class can silently fail to generate under
          // `BIOROUTER_NO_HMR`, and this one is the whole fix.
          <div ref={pasteBoxRef} className="br-ingest-summoned">
            <PasteTextBox
              onCancel={() => setShowPasteBox(false)}
              onStage={(text, title, urls) => {
                add({ kind: 'text', id: genId(), text, title, status: 'pending' });
                for (const url of urls) add({ kind: 'url', id: genId(), url, status: 'pending' });
                setShowPasteBox(false);
              }}
            />
          </div>
        )}

        <StagedList items={items} onRemove={remove} onClear={clear} />

        {/* State 3. Determinate on the queue — `value`/`max` are the two numbers
            the loop already knows — with `indeterminate` reserved for the
            pre-flight model check, where there is no denominator to report. */}
        {busy && (
          <Progress
            data-testid="knowledge-digest-progress"
            label="Digesting staged sources"
            indeterminate={digestState === 'checking' || digestProgress === null}
            value={digestProgress?.completed ?? 0}
            max={Math.max(1, digestProgress?.total ?? 1)}
          />
        )}

        <DispatchProgress state={stream} />

        {/* State 5. Successful rows auto-clear; errored ones stay, and this is
            the one summary that lets the user act on all of them at once. */}
        {!busy && failed.length > 0 && (
          <div className="flex flex-wrap items-center gap-2 rounded-element bg-wash-danger px-3 py-2">
            <span className="min-w-0 flex-1 text-supporting text-text-danger">
              {failed.length} {failed.length === 1 ? 'source' : 'sources'} failed
            </span>
            <Button
              type="button"
              variant="secondary"
              size="sm"
              onClick={() => {
                if (!nothingToDigest) void onDigest();
              }}
            >
              Retry failed
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() => {
                for (const item of failed) remove(item.id);
              }}
            >
              Clear failed
            </Button>
          </div>
        )}
      </div>

      {/* Pinned. `sticky` rather than a sibling of the scroll box because the
          rail owns one scroll container and the footer belongs to this panel;
          it is the last child, so it paints above the body without a z-index —
          and an off-scale z-index is exactly the value class that soft-locked
          this app once already. */}
      <div
        ref={footerRef}
        className="sticky bottom-0 flex flex-col gap-2 border-t border-border-subtle bg-background-default p-4"
      >
        <IngestModelPicker
          value={model}
          valueState={modelValueState}
          onChange={(next) => void onDefaultModelChange(next)}
          disabled={!dispatchKbId || savingDefaultModel}
          saving={savingDefaultModel}
        />
        {/* K-04 preserved verbatim: the one primary action stays full-opacity
            with nothing staged, guarded by a cursor and a helper line, so it
            never trains the eye to ignore a permanently half-lit button.
            `size="lg"` and NO className height — `size="sm" className="min-h-9"`
            was a contradiction that forced a 28px rung to render at 36px while
            keeping `sm`'s own 6px icon gap. */}
        <Button
          data-testid="knowledge-digest-button"
          variant={busy ? 'secondary' : 'default'}
          size="lg"
          disabled={digestState === 'stopping'}
          aria-disabled={(!busy && nothingToDigest) || undefined}
          onClick={() => {
            if (busy) {
              onAbort();
              return;
            }
            if (!nothingToDigest) void onDigest();
          }}
          className={`w-full ${!busy && nothingToDigest ? 'cursor-not-allowed' : ''}`}
        >
          {busy ? (digestState === 'stopping' ? 'Stopping…' : 'Stop') : digestLabel}
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
              <Button
                data-testid="knowledge-ingest-retry"
                type="button"
                variant="ghost"
                size="sm"
                className="ml-1 align-baseline"
                onClick={() => void refresh()}
              >
                Retry
              </Button>
            )}
          </p>
        )}
      </div>
    </div>
  );
}
