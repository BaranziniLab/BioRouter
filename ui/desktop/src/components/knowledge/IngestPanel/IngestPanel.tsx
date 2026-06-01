import { useEffect, useState } from 'react';
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

  const ingestable = items.filter((s) => s.kind !== 'file');
  const canDigest = ingestable.length > 0 && !!activeKbId && !digesting;

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

      {items.some((s) => s.kind === 'file') && (
        <div className="mb-2 px-3 py-2 rounded-lg border border-amber-300 bg-amber-50 text-xs text-amber-700 dark:border-amber-600 dark:bg-amber-950/30 dark:text-amber-400">
          File uploads not yet supported — only URL and pasted text sources will be digested.
        </div>
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
