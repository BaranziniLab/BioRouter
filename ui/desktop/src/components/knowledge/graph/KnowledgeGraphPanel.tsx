// ui/desktop/src/components/knowledge/graph/KnowledgeGraphPanel.tsx
import { useEffect, useState } from 'react';
import { Download, FolderOpen, History, RefreshCw } from '../../icons/app-icons';
import { getLocation } from '../../../api';
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
  const { primaryKbId, primaryKb, registerGraphRefresh } = useKnowledge();
  const { graph, loading, error, refresh } = useKnowledgeGraph(primaryKbId);
  const { exportArchive } = useKnowledgeBases();
  const [hoveredId, setHoveredId] = useState<string | null>(null);
  const [selected, setSelected] = useState<GraphNode | null>(null);

  // Open the active KB's folder in the OS file explorer so the user can inspect
  // the raw markdown sources and on-disk knowledge graph directly.
  const openKbFolder = async () => {
    if (!primaryKbId) return;
    try {
      const res = await getLocation({ path: { id: primaryKbId }, throwOnError: true });
      const path = res.data?.path;
      if (path) await window.electron.openDirectoryInExplorer(path);
    } catch (err) {
      window.electron.logInfo(`Failed to open knowledge base folder: ${String(err)}`);
    }
  };

  // Expose refresh() to KnowledgeContext so IngestPanel can call it after each ingest.
  useEffect(() => {
    registerGraphRefresh(refresh);
    return () => registerGraphRefresh(null);
  }, [refresh, registerGraphRefresh]);

  return (
    <div className="relative flex h-full w-full min-w-0 flex-1 flex-col overflow-hidden">
      <div className="mb-0 flex flex-wrap items-center justify-between gap-3 px-4 py-3">
        <div className="flex min-w-[220px] flex-1 items-baseline gap-2 text-supporting text-text-muted">
          <span className="truncate text-label font-semibold text-text-default">
            {primaryKb?.name ?? 'No primary knowledge base'}
          </span>
          {graph && (
            <span className="shrink-0 text-supporting">
              <span data-testid="knowledge-graph-summary">
                · {graph.nodes.length} {graph.nodes.length === 1 ? 'page' : 'pages'}
                {' · '}
                {graph.edges.length} {graph.edges.length === 1 ? 'link' : 'links'}
              </span>
            </span>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button
            variant="ghost"
            size="sm"
            shape="round"
            onClick={() => void refresh()}
            disabled={!primaryKbId || loading}
            title="Refresh graph"
          >
            <RefreshCw className={`h-4 w-4 ${loading ? 'animate-spin' : ''}`} />
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => primaryKb && void exportArchive(primaryKb.id, primaryKb.name)}
            disabled={!primaryKb}
            title="Export current knowledge base as .brkb"
          >
            <Download className="mr-1 h-4 w-4" />
            Export as .brkb
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={onOpenChangeLog}
            disabled={!primaryKbId}
            title="Open change log"
          >
            <History className="h-4 w-4 mr-1" />
            Change log
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => void openKbFolder()}
            disabled={!primaryKbId}
            title="Open the knowledge base folder (raw sources + markdown) in your file explorer"
          >
            <FolderOpen className="h-4 w-4 mr-1" />
            Open folder
          </Button>
        </div>
      </div>

      {previewSha && (
        <div className="mx-4 mt-2 flex items-center justify-between rounded-container border border-border-warning/40 bg-background-warning/10 px-4 py-2 text-supporting text-text-warning">
          <span>Previewing commit {previewSha.slice(0, 7)}: read-only</span>
          <button onClick={onClearPreview} className="underline">
            Exit preview
          </button>
        </div>
      )}

      <div className="relative min-h-0 flex-1 overflow-hidden border-t border-border-subtle bg-background-muted">
        {!primaryKbId && (
          <div className="absolute inset-0 flex items-center justify-center text-body text-text-muted">
            Make a knowledge base primary to see its graph.
          </div>
        )}
        {primaryKbId && error && (
          <div className="absolute inset-0 flex items-center justify-center text-body text-text-danger">
            {error}
          </div>
        )}
        {primaryKbId && !error && graph && graph.nodes.length === 0 && (
          <div className="absolute inset-0 flex items-center justify-center text-body text-text-muted">
            No pages yet. Ingest a source to populate the graph.
          </div>
        )}
        {primaryKbId && !error && graph && graph.nodes.length > 0 && (
          <ForceGraphCanvas
            graph={graph}
            selectedId={selected?.id ?? null}
            hoveredId={hoveredId}
            onHover={setHoveredId}
            onNodeClick={(n) => setSelected(n)}
            visibleSet={null}
          />
        )}
        {selected && primaryKbId && (
          <NodePreview
            kbId={primaryKbId}
            node={selected}
            previewSha={previewSha}
            onClose={() => setSelected(null)}
          />
        )}
        {primaryKbId && graph && graph.nodes.length > 0 && (
          <div className="absolute bottom-4 left-4 rounded-container border border-border-subtle bg-background-default px-3 py-2.5">
            <div className="grid grid-cols-2 gap-x-4 gap-y-2 text-supporting text-text-default">
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
