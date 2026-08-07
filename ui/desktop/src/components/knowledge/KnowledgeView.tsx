// ui/desktop/src/components/knowledge/KnowledgeView.tsx
import { useEffect, useState } from 'react';
import { MainPanelLayout } from '../Layout/MainPanelLayout';
import { KBSelectorTrigger } from './KBSelector/KBSelectorTrigger';
import { IngestPanel } from './IngestPanel/IngestPanel';
import { KnowledgeGraphPanel } from './graph/KnowledgeGraphPanel';
import { ChangeLogDrawer } from './changelog/ChangeLogDrawer';
import { useKnowledge } from './KnowledgeContext';
import { KbTierPanel } from './KbTierControl';
import { ReadableContent } from '../Layout/ReadableContent';

export default function KnowledgeView() {
  return <KnowledgeViewInner />;
}

function KnowledgeViewInner() {
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [changeLogOpen, setChangeLogOpen] = useState(false);
  const [previewSha, setPreviewSha] = useState<string | null>(null);
  const [compactView, setCompactView] = useState<'digest' | 'graph'>('graph');
  const { refresh, primaryKb } = useKnowledge();

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
        className="relative flex min-w-0 flex-1 flex-col overflow-hidden"
        data-search-scroll-area
      >
        {/* §4.2 — the hairline is FULL-BLEED, not capped to the reading column. */}
        <div className="flex-shrink-0 border-b border-border-subtle">
          <ReadableContent size="text" className="px-4 pb-4 pt-8 sm:px-6 lg:px-8 lg:pb-6 lg:pt-12">
            <div className="page-transition">
              <h1 className="text-title">Knowledge</h1>
              <p className="mt-1 text-secondary text-text-muted">
                Personal, LLM-maintained knowledge bases.
              </p>
            </div>
          </ReadableContent>
        </div>
        <ReadableContent
          size="text"
          className="mb-4 mt-2 grid min-h-0 flex-1 grid-cols-1 grid-rows-[auto_minmax(0,1fr)] items-stretch gap-3 overflow-hidden px-4 sm:px-6 lg:mb-6 lg:grid-cols-[minmax(280px,360px)_minmax(0,1fr)] lg:grid-rows-1 lg:gap-4 lg:px-8"
        >
          <div className="flex min-w-0 items-center gap-3 rounded-container border border-border-subtle bg-background-default p-3 lg:hidden">
            <div className="min-w-0 flex-1">
              <KBSelectorTrigger open={paletteOpen} onOpenChange={setPaletteOpen} />
            </div>
            <div
              role="tablist"
              aria-label="Knowledge workspace"
              className="flex shrink-0 items-center gap-1"
            >
              {(['digest', 'graph'] as const).map((view) => (
                <button
                  key={view}
                  type="button"
                  role="tab"
                  aria-selected={compactView === view}
                  aria-controls={`knowledge-${view}-panel`}
                  onClick={() => setCompactView(view)}
                  className={`h-control-md rounded-element px-3 text-label transition-colors ${
                    compactView === view
                      ? 'tint-selected tint-interactive text-text-default'
                      : 'tint-interactive text-text-muted'
                  }`}
                >
                  {view === 'digest' ? 'Digest' : 'Graph'}
                </button>
              ))}
            </div>
          </div>

          <div
            id="knowledge-digest-panel"
            data-testid="knowledge-digest-panel"
            role="tabpanel"
            className={`${compactView === 'digest' ? 'flex' : 'hidden'} min-h-0 flex-col overflow-hidden rounded-container border border-border-subtle bg-background-default lg:flex lg:h-full`}
          >
            <div className="hidden flex-col gap-3 p-4 lg:flex">
              <KBSelectorTrigger open={paletteOpen} onOpenChange={setPaletteOpen} />
              {/* Issue #56 DR-18. Beside the base it acts on, in the KB header —
                  not in a settings page, where a user reading a private base
                  would never meet it. */}
              {primaryKb && (
                <KbTierPanel
                  kb={{ id: primaryKb.id, name: primaryKb.name, tier: primaryKb.tier }}
                />
              )}
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto lg:border-t lg:border-border-subtle">
              <IngestPanel />
            </div>
          </div>

          <div
            id="knowledge-graph-panel"
            data-testid="knowledge-graph-panel"
            role="tabpanel"
            className={`${compactView === 'graph' ? 'flex' : 'hidden'} min-h-0 min-w-0 overflow-hidden rounded-container border border-border-subtle bg-background-default lg:flex lg:h-full`}
          >
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
