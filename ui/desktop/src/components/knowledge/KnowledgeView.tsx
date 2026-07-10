// ui/desktop/src/components/knowledge/KnowledgeView.tsx
import { useEffect, useState } from 'react';
import { MainPanelLayout } from '../Layout/MainPanelLayout';
import { KBSelectorTrigger } from './KBSelector/KBSelectorTrigger';
import { IngestPanel } from './IngestPanel/IngestPanel';
import { KnowledgeGraphPanel } from './graph/KnowledgeGraphPanel';
import { ChangeLogDrawer } from './changelog/ChangeLogDrawer';
import { useKnowledge } from './KnowledgeContext';
import { ReadableContent } from '../Layout/ReadableContent';

export default function KnowledgeView() {
  return <KnowledgeViewInner />;
}

function KnowledgeViewInner() {
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [changeLogOpen, setChangeLogOpen] = useState(false);
  const [previewSha, setPreviewSha] = useState<string | null>(null);
  const { refresh } = useKnowledge();

  // The KnowledgeProvider only fetches the base list once at app start, so a
  // knowledge base created elsewhere (e.g. via chat / the knowledge MCP tools)
  // would not appear here until a full reload. Re-fetch whenever the Knowledge
  // view mounts so navigating to this tab always reflects the current bases.
  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === 'k' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        setPaletteOpen((v) => !v);
      }
    }
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, []);

  return (
    <MainPanelLayout>
      <div
        className="flex flex-col min-w-0 flex-1 overflow-y-auto relative"
        data-search-scroll-area
      >
        <ReadableContent
          size="text"
          className="px-8 pt-12 pb-6 border-b border-border-subtle flex-shrink-0"
        >
          <div className="page-transition">
            <h1 className="text-2xl font-semibold tracking-tight">Knowledge</h1>
            <p className="mt-1 text-sm text-text-muted">
              Personal, LLM-maintained knowledge bases.
            </p>
          </div>
        </ReadableContent>
        <ReadableContent
          size="text"
          className="mb-6 mt-2 grid min-h-[560px] flex-1 grid-cols-1 items-start gap-4 overflow-y-auto px-8 lg:min-h-0 lg:grid-cols-[360px_minmax(0,1fr)] lg:overflow-hidden"
        >
          {/* Left column — one flat panel; internal hairlines separate the blocks. */}
          <div className="flex min-h-0 flex-col overflow-hidden rounded-xl border border-border-subtle bg-background-default lg:h-full">
            <div className="p-4">
              <KBSelectorTrigger open={paletteOpen} onOpenChange={setPaletteOpen} />
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto border-t border-border-subtle">
              <IngestPanel />
            </div>
          </div>
          {/* Right column — the same flat panel, holding the graph. */}
          <div className="flex min-h-[520px] min-w-0 overflow-hidden rounded-xl border border-border-subtle bg-background-default lg:h-full lg:min-h-0">
            <KnowledgeGraphPanel
              onOpenChangeLog={() => setChangeLogOpen(true)}
              previewSha={previewSha}
              onClearPreview={() => setPreviewSha(null)}
            />
          </div>
        </ReadableContent>
        <ChangeLogDrawer
          open={changeLogOpen}
          onOpenChange={setChangeLogOpen}
          onPreview={(sha) => {
            setPreviewSha(sha);
            setChangeLogOpen(false);
          }}
          onRestored={() => setChangeLogOpen(false)}
        />
      </div>
    </MainPanelLayout>
  );
}
