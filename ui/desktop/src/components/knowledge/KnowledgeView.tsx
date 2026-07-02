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
        <ReadableContent size="graph" className="px-8 pt-12 pb-4 flex-shrink-0">
          <div className="flex flex-col page-transition">
            <h1 className="text-2xl font-semibold tracking-tight mb-1">Knowledge</h1>
            <p className="text-sm text-text-muted mb-0">
              Personal, LLM-maintained knowledge bases.
            </p>
          </div>
        </ReadableContent>
        <ReadableContent
          size="graph"
          className="mb-6 mt-2 grid min-h-[640px] flex-1 grid-cols-1 items-stretch gap-4 overflow-y-auto px-8 lg:min-h-0 lg:grid-cols-[340px_minmax(0,1fr)] lg:overflow-hidden"
        >
          <div className="flex min-h-0 flex-col overflow-visible px-4 pb-4 pt-0 lg:h-full lg:overflow-y-auto">
            <div className="pb-1">
              <KBSelectorTrigger open={paletteOpen} onOpenChange={setPaletteOpen} />
            </div>
            <IngestPanel />
          </div>
          <div className="flex min-h-[520px] min-w-0 overflow-hidden rounded-2xl bg-[color-mix(in_srgb,var(--background-muted)_72%,var(--background-default))] shadow-[inset_0_1px_0_rgba(255,255,255,0.58),0_18px_46px_-36px_rgba(32,25,15,0.34)] lg:h-full lg:min-h-0">
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
