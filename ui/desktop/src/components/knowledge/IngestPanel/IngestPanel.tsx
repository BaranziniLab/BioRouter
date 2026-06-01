import { useEffect, useState } from 'react';
import { addRawSource } from '../../../api';
import type { ModelRef } from '../../../api/types.gen';
import { useModelAndProvider } from '../../ModelAndProviderContext';
import { DispatchProgress } from '../DispatchProgress';
import { useKnowledge } from '../KnowledgeContext';
import { useIngestStream } from '../hooks/useIngestStream';
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
  const { activeKbId } = useKnowledge();
  const { currentModel, currentProvider } = useModelAndProvider();
  const { items, add, remove, update, clear } = useStagedSources();
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
    setDigesting(true);
    try {
      for (const item of items) {
        if (item.status === 'done') continue;

        if (item.kind === 'file') {
          update(item.id, { status: 'error', error: 'file upload not yet supported' });
          continue;
        }

        update(item.id, { status: 'ingesting' });
        try {
          // Build source body
          const sourceBody =
            item.kind === 'url'
              ? { url: item.url }
              : { text: item.text, title: item.title };

          // POST /knowledge/bases/:id/raw — register the raw source
          await addRawSource({
            throwOnError: true,
            path: { id: activeKbId },
            body: sourceBody,
          });

          // POST /knowledge/bases/:id/ingest — SSE streamed digestion
          const result = await stream.start(`/knowledge/bases/${activeKbId}/ingest`, {
            source: sourceBody,
            model,
          });

          if (result === 'error') {
            update(item.id, { status: 'error', error: 'ingest stream error' });
          } else {
            update(item.id, { status: 'done' });
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

      <DispatchProgress state={stream} />

      <div className="flex items-center justify-between gap-2 pt-1">
        <IngestModelPicker value={model} onChange={setModel} />
        <button
          disabled={!canDigest}
          onClick={() => void onDigest()}
          className="px-4 py-1.5 rounded-lg bg-text-default text-background-surface text-xs font-semibold disabled:opacity-40 hover:opacity-90 transition-opacity"
        >
          {digesting ? 'Digesting…' : 'Digest'}
        </button>
      </div>

      {!activeKbId && (
        <p className="text-[10px] text-text-muted text-center">
          Select or create a knowledge base above to enable digestion.
        </p>
      )}
    </div>
  );
}
