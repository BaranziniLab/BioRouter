// ui/desktop/src/components/knowledge/KnowledgeView.tsx
import { useEffect, useState } from 'react';
import { MainPanelLayout } from '../Layout/MainPanelLayout';
import { KBSelectorTrigger } from './KBSelector/KBSelectorTrigger';
import { IngestPanel } from './IngestPanel/IngestPanel';
import { KnowledgeGraphPanel } from './graph/KnowledgeGraphPanel';
import { ChangeLogDrawer } from './changelog/ChangeLogDrawer';
import { useKnowledge } from './KnowledgeContext';

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
        <div className="biorouter-page-header px-8 pt-12 pb-6 flex-shrink-0 border-b border-border-subtle">
          <div className="flex flex-col page-transition">
            <h1 className="text-2xl font-semibold tracking-tight mb-1">Knowledge</h1>
            <p className="text-sm text-text-muted mb-0">
              Personal, LLM-maintained knowledge bases.
            </p>
          </div>
        </div>
        <div className="grid min-h-0 flex-1 grid-cols-1 gap-4 px-6 pb-6 pt-4 lg:grid-cols-[360px_minmax(0,1fr)]">
          <div className="biorouter-page-block min-h-0 overflow-y-auto">
            <div className="p-4 pb-0">
              <KBSelectorTrigger open={paletteOpen} onOpenChange={setPaletteOpen} />
            </div>
            <IngestPanel />
          </div>
          <div className="biorouter-page-block min-h-0 min-w-0 overflow-hidden">
            <KnowledgeGraphPanel
              onOpenChangeLog={() => setChangeLogOpen(true)}
              previewSha={previewSha}
              onClearPreview={() => setPreviewSha(null)}
            />
          </div>
        </div>
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
