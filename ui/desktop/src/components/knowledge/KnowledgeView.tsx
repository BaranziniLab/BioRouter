import { useEffect, useState } from 'react';
import { MainPanelLayout } from '../Layout/MainPanelLayout';
import { KnowledgeProvider } from './KnowledgeContext';
import { KBSelectorTrigger } from './KBSelector/KBSelectorTrigger';
import { IngestPanel } from './IngestPanel/IngestPanel';
import { RightSidePlaceholder } from './RightSidePlaceholder';

export default function KnowledgeView() {
  return (
    <KnowledgeProvider>
      <KnowledgeViewInner />
    </KnowledgeProvider>
  );
}

function KnowledgeViewInner() {
  // Lift palette open-state here so Cmd-K (global shortcut) can open it while
  // KnowledgeView is mounted without requiring a custom event bus.
  const [paletteOpen, setPaletteOpen] = useState(false);

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
      <div className="flex flex-col min-w-0 flex-1 overflow-y-auto relative" data-search-scroll-area>
        <div className="px-8 pt-12 pb-6 flex-shrink-0 border-b border-border-subtle">
          <div className="flex flex-col page-transition">
            <h1 className="text-2xl font-semibold tracking-tight mb-1">Knowledge</h1>
            <p className="text-sm text-text-muted mb-0">
              Personal, LLM-maintained knowledge bases.
            </p>
          </div>
        </div>
        <div className="flex-1 grid grid-cols-1 lg:grid-cols-[360px_1fr] min-h-0">
          <div className="border-r border-border-subtle overflow-y-auto">
            <div className="p-6">
              <KBSelectorTrigger open={paletteOpen} onOpenChange={setPaletteOpen} />
            </div>
            <IngestPanel />
          </div>
          <div className="min-h-0">
            <RightSidePlaceholder />
          </div>
        </div>
      </div>
    </MainPanelLayout>
  );
}
