// ui/desktop/src/components/knowledge/graph/KnowledgeGraphPanel.tsx
import { useEffect, useState } from 'react';
import { History, RefreshCw } from 'lucide-react';
import type { GraphNode } from '../../../api/types.gen';
import { Button } from '../../ui/button';
import { useKnowledge } from '../KnowledgeContext';
import { useKnowledgeGraph } from '../hooks/useKnowledgeGraph';
import { ForceGraphCanvas } from './ForceGraphCanvas';
import { NodePreview } from './NodePreview';

interface Props {
  onOpenChangeLog: () => void;
  /// When set, the panel is in read-only "preview at SHA" mode. The banner
  /// shows the SHA; the graph still renders the current data (ghosting of
  /// future-state nodes is a Plan-6 polish item).
  previewSha: string | null;
  onClearPreview: () => void;
}

export function KnowledgeGraphPanel({ onOpenChangeLog, previewSha, onClearPreview }: Props) {
  const { activeKbId, activeKb, registerGraphRefresh } = useKnowledge();
  const { graph, loading, error, refresh } = useKnowledgeGraph(activeKbId);
  const [hoveredId, setHoveredId] = useState<string | null>(null);
  const [selected, setSelected] = useState<GraphNode | null>(null);

  // Expose refresh() to KnowledgeContext so IngestPanel can call it after each ingest.
  useEffect(() => {
    registerGraphRefresh(refresh);
    return () => registerGraphRefresh(null);
  }, [refresh, registerGraphRefresh]);

  return (
    <div className="flex flex-col h-full relative">
      <div className="flex items-center justify-between px-6 py-3 border-b border-border-subtle">
        <div className="flex items-center gap-2 text-xs text-text-muted">
          <span className="font-medium text-text-default">
            {activeKb?.name ?? 'No knowledge base'}
          </span>
          {graph && (
            <span>
              · {graph.nodes.length} {graph.nodes.length === 1 ? 'page' : 'pages'}
              {' · '}
              {graph.edges.length} {graph.edges.length === 1 ? 'link' : 'links'}
            </span>
          )}
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => void refresh()}
            disabled={!activeKbId || loading}
            title="Refresh graph"
          >
            <RefreshCw className={`h-4 w-4 ${loading ? 'animate-spin' : ''}`} />
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={onOpenChangeLog}
            disabled={!activeKbId}
            title="Open change log"
          >
            <History className="h-4 w-4 mr-1" />
            Change log
          </Button>
        </div>
      </div>

      {previewSha && (
        <div className="px-6 py-2 bg-yellow-900/30 border-b border-yellow-700/50 text-xs text-yellow-200 flex items-center justify-between">
          <span>Previewing commit {previewSha.slice(0, 7)} — read-only</span>
          <button onClick={onClearPreview} className="underline">
            Exit preview
          </button>
        </div>
      )}

      <div className="flex-1 relative min-h-0">
        {!activeKbId && (
          <div className="absolute inset-0 flex items-center justify-center text-sm text-text-muted">
            Select a knowledge base to see its graph.
          </div>
        )}
        {activeKbId && error && (
          <div className="absolute inset-0 flex items-center justify-center text-sm text-red-400">
            {error}
          </div>
        )}
        {activeKbId && !error && graph && graph.nodes.length === 0 && (
          <div className="absolute inset-0 flex items-center justify-center text-sm text-text-muted">
            No pages yet. Ingest a source to populate the graph.
          </div>
        )}
        {activeKbId && !error && graph && graph.nodes.length > 0 && (
          <ForceGraphCanvas
            graph={graph}
            selectedId={selected?.id ?? null}
            hoveredId={hoveredId}
            onHover={setHoveredId}
            onNodeClick={(n) => setSelected(n)}
            visibleSet={null}
          />
        )}
        {selected && activeKbId && (
          <NodePreview kbId={activeKbId} node={selected} onClose={() => setSelected(null)} />
        )}
      </div>
    </div>
  );
}
