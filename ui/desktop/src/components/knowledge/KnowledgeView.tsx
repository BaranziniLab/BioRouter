import { MainPanelLayout } from '../Layout/MainPanelLayout';
import { KnowledgeProvider } from './KnowledgeContext';

export default function KnowledgeView() {
  return (
    <KnowledgeProvider>
      <KnowledgeViewInner />
    </KnowledgeProvider>
  );
}

function KnowledgeViewInner() {
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
          <div className="border-r border-border-subtle p-6">
            <p className="text-sm text-text-muted">Ingest panel coming in later tasks.</p>
          </div>
          <div className="p-6">
            <p className="text-sm text-text-muted">Graph view comes in Plan 5.</p>
          </div>
        </div>
      </div>
    </MainPanelLayout>
  );
}
