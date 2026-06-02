import { useEffect, useState } from 'react';
import type { ModelRef } from '../../../api/types.gen';
import { checkModel } from '../../../api/sdk.gen';
import { useModelAndProvider } from '../../ModelAndProviderContext';
import { Button } from '../../ui/button';
import { DispatchProgress } from '../DispatchProgress';
import { useKnowledge } from '../KnowledgeContext';
import { useIngestStream } from '../hooks/useIngestStream';
import { useKnowledgeBases } from '../hooks/useKnowledgeBases';
import { useStagedSources } from '../hooks/useStagedSources';
import { Dropzone } from './Dropzone';
import { IngestModelPicker } from './IngestModelPicker';
import { PasteTextBox } from './PasteTextBox';
import { StagedList } from './StagedList';

// TODO Plan 6: read default from config via useConfig once model picker is real
const FALLBACK_MODEL: ModelRef = { provider: 'anthropic', model: 'claude-sonnet-4-6' };

function genId(): string {
  return Date.now().toString(36) + Math.random().toString(36).substring(2, 9);
}

export function IngestPanel() {
  const { activeKbId, triggerGraphRefresh } = useKnowledge();
  const { currentModel, currentProvider } = useModelAndProvider();
  const { items, add, remove, update, clear } = useStagedSources();
  const { importArchive } = useKnowledgeBases();
  const stream = useIngestStream();
  const [showPasteBox, setShowPasteBox] = useState(false);
  const [digesting, setDigesting] = useState(false);

  // Seed the model from the existing config-backed context; fall back to hardcoded default
  const [model, setModel] = useState<ModelRef>(FALLBACK_MODEL);
  useEffect(() => {
    if (currentModel && currentProvider) {
      setModel({ model: currentModel, provider: currentProvider });
    }
  }, [currentModel, currentProvider]);

  function onFiles(files: File[]) {
    for (const file of files) {
      add({ kind: 'file', id: genId(), file, status: 'pending' });
    }
  }

  async function onDigest() {
    if (!activeKbId || digesting) return;

    // Pre-flight: confirm the model is reachable before iterating staged items.
    try {
      const res = await checkModel({ body: { model } });
      const data = res.data;
      if (!data?.ok) {
        window.alert(
          `Model unreachable: ${data?.error ?? 'unknown'}\n\nPlease switch to a different model.`,
        );
        return;
      }
    } catch (err) {
      window.alert(
        `Model check failed: ${err instanceof Error ? err.message : String(err)}\n\nPlease verify your provider's credentials and try a different model.`,
      );
      return;
    }

    setDigesting(true);
    try {
      for (const item of items) {
        if (item.status === 'done') continue;

        if (item.kind === 'file') {
          update(item.id, { status: 'ingesting', error: undefined });
          try {
            if (item.file.name.toLowerCase().endsWith('.brkb')) {
              await importArchive(item.file);
              update(item.id, { status: 'done' });
              triggerGraphRefresh();
              continue;
            }

            const formData = new FormData();
            formData.append('file', item.file);
            formData.append('provider', model.provider);
            formData.append('model', model.model);

            const result = await stream.startMultipart(
              `/knowledge/bases/${activeKbId}/ingest`,
              formData
            );

            if (result === 'error') {
              update(item.id, {
                status: 'error',
                error: stream.error ?? 'ingest stream error',
              });
            } else {
              update(item.id, { status: 'done' });
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
            item.kind === 'url'
              ? { url: item.url }
              : { text: item.text, title: item.title };

          // POST /knowledge/bases/:id/ingest — SSE streamed digestion.
          // The macro materialises the raw source and then runs the sub-agent.
          const result = await stream.start(`/knowledge/bases/${activeKbId}/ingest`, {
            source: sourceBody,
            model,
          });

          if (result === 'error') {
            // stream.error is populated by useIngestStream from the SSE error frame.
            update(item.id, {
              status: 'error',
              error: stream.error ?? 'ingest stream error',
            });
          } else {
            update(item.id, { status: 'done' });
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
      setDigesting(false);
      // Auto-clear successfully ingested items; keep errors visible for user action.
      for (const item of items) {
        if (item.status === 'done') remove(item.id);
      }
    }
  }

  const canDigest = items.length > 0 && !!activeKbId && !digesting;

  return (
    <div className="flex flex-col gap-4 p-4">
      <Dropzone onFiles={onFiles} onPasteTextRequested={() => setShowPasteBox(true)} />

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

      <DispatchProgress state={stream} onAbort={() => stream.abort()} />

      <div className="flex flex-col gap-2 pt-1">
        <Button
          variant="default"
          size="sm"
          disabled={!canDigest}
          onClick={() => void onDigest()}
          className="w-full min-h-9"
        >
          {digesting ? 'Digesting…' : 'Digest Staged Sources'}
        </Button>
        <IngestModelPicker value={model} onChange={setModel} />
      </div>

      {!activeKbId && (
        <p className="text-[10px] text-text-muted text-center">
          Select or create a knowledge base above to enable digestion.
        </p>
      )}
    </div>
  );
}
