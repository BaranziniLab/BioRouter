// ui/desktop/src/components/knowledge/graph/KnowledgeGraphPanel.tsx
import { useState } from 'react';
import { AlertCircle, LoaderCircle, Sparkles } from '../../icons/app-icons';
import type { Graph, GraphNode } from '../../../api/types.gen';
import { Button } from '../../ui/button';
import { EmptyState } from '../../ui/empty-state';
import { credColor, kindColor } from './credColors';
import { ForceGraphCanvas } from './ForceGraphCanvas';
import { NodePreview } from './NodePreview';

interface Props {
  kbId: string | null;
  graph: Graph | null;
  loading: boolean;
  error: string | null;
  onRefresh: () => void;
  /// When set, the panel is in read-only "preview at SHA" mode. The banner
  /// shows the SHA; the graph still renders the current data (ghosting of
  /// future-state nodes is a Plan-6 polish item).
  previewSha: string | null;
  onClearPreview: () => void;
}

/**
 * The graph pane (ui-spec §4.5).
 *
 * The header row this used to draw is gone: the base's name, its counts, the
 * Refresh button and the four overflow actions all belong to the section's
 * SUBJECT BAND, where they name the base the whole view is about rather than
 * decorating one of its three panes.
 *
 * The canvas is the content — no card, no shadow, no inner padding — and the
 * pane's own border is the only edge.
 *
 * ⚠ **Two loading behaviours, and conflating them is the bug this fixes.** The
 * FIRST load of a base's graph cross-fades a `role="status"` block against the
 * canvas. A REFRESH of a graph already on screen does **not** blank it: the
 * Refresh button's icon spins and nothing else changes. Blanking a graph the
 * user is reading, in order to redraw the same graph, is a regression disguised
 * as feedback.
 */
export function KnowledgeGraphPanel({
  kbId,
  graph,
  loading,
  error,
  onRefresh,
  previewSha,
  onClearPreview,
}: Props) {
  const [hoveredId, setHoveredId] = useState<string | null>(null);
  const [selected, setSelected] = useState<GraphNode | null>(null);

  const firstLoad = loading && !graph;
  const hasNodes = !!graph && graph.nodes.length > 0;

  return (
    <div className="relative flex h-full w-full min-w-0 flex-1 flex-col overflow-hidden rounded-container border border-border-subtle bg-background-default">
      {previewSha && (
        <div className="flex flex-none items-center justify-between gap-2 border-b border-border-subtle bg-wash-warning px-3 py-2 text-supporting text-text-warning">
          <span>Previewing commit {previewSha.slice(0, 7)}: read-only</span>
          <Button variant="ghost" size="sm" onClick={onClearPreview}>
            Exit preview
          </Button>
        </div>
      )}

      <div className="relative min-h-0 flex-1 overflow-hidden bg-background-muted">
        {/* Two absolutely-stacked opacity layers, never a swap: loading out at
            `--dur-fast`, content in at `--dur-med`. */}
        <div
          role="status"
          className={`br-knowledge-fade-out absolute inset-0 flex items-center justify-center gap-2 ${
            firstLoad ? 'opacity-100' : 'pointer-events-none opacity-0'
          }`}
        >
          {firstLoad && (
            <>
              <LoaderCircle
                aria-hidden="true"
                className="h-icon-row w-icon-row animate-spin text-text-muted"
              />
              <span className="text-secondary text-text-muted">Loading graph</span>
            </>
          )}
        </div>

        {/* `flex` so an `EmptyState` (which centres itself only within its own
            box) is centred in the CANVAS, and so the canvas child still gets the
            full height it measures against. */}
        <div
          className={`br-knowledge-fade-in absolute inset-0 flex ${
            firstLoad ? 'opacity-0' : 'opacity-100'
          }`}
        >
          {error ? (
            <EmptyState
              className="m-auto"
              icon={AlertCircle}
              title="Could not load the graph"
              description={error}
              actions={
                <Button variant="secondary" onClick={onRefresh}>
                  Try again
                </Button>
              }
            />
          ) : graph && !hasNodes ? (
            <EmptyState
              className="m-auto"
              icon={Sparkles}
              title="Nothing digested yet"
              description="Stage a source in the Sources rail and press Digest."
            />
          ) : hasNodes ? (
            <ForceGraphCanvas
              graph={graph}
              selectedId={selected?.id ?? null}
              hoveredId={hoveredId}
              onHover={setHoveredId}
              onNodeClick={(n) => setSelected(n)}
              visibleSet={null}
            />
          ) : null}
        </div>

        {selected && kbId && (
          <NodePreview
            kbId={kbId}
            node={selected}
            previewSha={previewSha}
            onClose={() => setSelected(null)}
          />
        )}

        {/* ⚠ SLICE B. This legend still reads `credColors.ts` — the kind/tier
            palette the OKF migration replaces with a generated, contrast- and
            colour-vision-audited 28-type palette (§4.7, §5.2–§5.3). It is left
            exactly as it was rather than half-rewritten: the typed palette has
            no data source until Stage 6 puts `node_type` on `GraphNode`. */}
        {hasNodes && (
          <div className="absolute bottom-4 left-4 rounded-container border border-border-subtle bg-background-default px-3 py-2.5">
            <div className="grid grid-cols-2 gap-x-4 gap-y-2 text-supporting text-text-default">
              <div className="flex items-center gap-2">
                <span
                  className="br-swatch-ring h-2 w-2 rounded-full"
                  style={{ background: kindColor.entity }}
                  aria-hidden="true"
                />
                Entity
              </div>
              <div className="flex items-center gap-2">
                <span
                  className="br-swatch-ring h-2 w-2 rounded-full"
                  style={{ background: kindColor.concept }}
                  aria-hidden="true"
                />
                Concept
              </div>
              <div className="flex items-center gap-2">
                <span
                  className="br-swatch-ring h-2 w-2 rounded-full"
                  style={{ background: kindColor.hub }}
                  aria-hidden="true"
                />
                Hub
              </div>
              <div className="flex items-center gap-2">
                <span
                  className="br-swatch-ring h-2 w-2 rounded-full"
                  style={{ background: credColor.peer_reviewed }}
                  aria-hidden="true"
                />
                Peer reviewed
              </div>
              <div className="flex items-center gap-2">
                <span
                  className="br-swatch-ring h-2 w-2 rounded-full"
                  style={{ background: credColor.web }}
                  aria-hidden="true"
                />
                Web source
              </div>
              <div className="flex items-center gap-2">
                <span
                  className="br-swatch-ring h-2 w-2 rounded-full"
                  style={{ background: credColor.personal }}
                  aria-hidden="true"
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
