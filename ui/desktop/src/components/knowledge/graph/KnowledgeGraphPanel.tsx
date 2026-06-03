// ui/desktop/src/components/knowledge/graph/KnowledgeGraphPanel.tsx
import { useEffect, useState } from 'react';
import { Download, History, RefreshCw } from 'lucide-react';
import type { GraphNode } from '../../../api/types.gen';
import { Button } from '../../ui/button';
import { useKnowledge } from '../KnowledgeContext';
import { useKnowledgeGraph } from '../hooks/useKnowledgeGraph';
import { useKnowledgeBases } from '../hooks/useKnowledgeBases';
import { credColor, kindColor } from './credColors';
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
  const { exportArchive } = useKnowledgeBases();
  const [hoveredId, setHoveredId] = useState<string | null>(null);
  const [selected, setSelected] = useState<GraphNode | null>(null);

  // Expose refresh() to KnowledgeContext so IngestPanel can call it after each ingest.
  useEffect(() => {
    registerGraphRefresh(refresh);
    return () => registerGraphRefresh(null);
  }, [refresh, registerGraphRefresh]);

  return (
    <div className="relative flex h-full min-w-0 flex-col overflow-hidden">
      <div className="flex flex-col gap-3 border-b border-border-subtle px-6 py-3 lg:flex-row lg:items-center lg:justify-between">
        <div className="flex min-w-0 items-center gap-2 text-xs text-text-muted">
          <span className="truncate font-medium text-text-default">
            {activeKb?.name ?? 'No knowledge base in focus'}
          </span>
          {graph && (
            <span>
              <span data-testid="knowledge-graph-summary">
                · {graph.nodes.length} {graph.nodes.length === 1 ? 'page' : 'pages'}
                {' · '}
                {graph.edges.length} {graph.edges.length === 1 ? 'link' : 'links'}
              </span>
            </span>
          )}
        </div>
        <div className="flex w-full flex-wrap items-center justify-end gap-2 lg:w-auto lg:flex-nowrap">
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
            onClick={() => activeKb && void exportArchive(activeKb.id, activeKb.name)}
            disabled={!activeKb}
            title="Export current knowledge base as .brkb"
          >
            <Download className="mr-1 h-4 w-4" />
            Export as .brkb
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
            Focus a knowledge base to see its graph.
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
          <NodePreview
            kbId={activeKbId}
            node={selected}
            previewSha={previewSha}
            onClose={() => setSelected(null)}
          />
        )}
        {activeKbId && graph && graph.nodes.length > 0 && (
          <div className="absolute bottom-4 left-4 rounded-2xl border border-border-subtle bg-background-default/85 px-4 py-3 shadow-lg backdrop-blur-sm">
            <div className="grid grid-cols-2 gap-x-4 gap-y-2 text-xs text-text-default">
              <div className="flex items-center gap-2">
                <span
                  className="h-2.5 w-2.5 rounded-full"
                  style={{ background: kindColor.entity }}
                />
                Entity
              </div>
              <div className="flex items-center gap-2">
                <span
                  className="h-2.5 w-2.5 rounded-full"
                  style={{ background: kindColor.concept }}
                />
                Concept
              </div>
              <div className="flex items-center gap-2">
                <span className="h-2.5 w-2.5 rounded-full" style={{ background: kindColor.hub }} />
                Hub
              </div>
              <div className="flex items-center gap-2">
                <span
                  className="h-2.5 w-2.5 rounded-full"
                  style={{ background: credColor.peer_reviewed }}
                />
                Peer reviewed
              </div>
              <div className="flex items-center gap-2">
                <span className="h-2.5 w-2.5 rounded-full" style={{ background: credColor.web }} />
                Web source
              </div>
              <div className="flex items-center gap-2">
                <span
                  className="h-2.5 w-2.5 rounded-full"
                  style={{ background: credColor.personal }}
                />
                Personal source
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
