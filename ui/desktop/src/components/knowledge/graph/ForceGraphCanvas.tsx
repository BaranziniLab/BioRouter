// ui/desktop/src/components/knowledge/graph/ForceGraphCanvas.tsx
import { useEffect, useMemo, useRef, useState } from 'react';
import ForceGraph2D, { ForceGraphMethods } from 'react-force-graph-2d';
import type { Graph, GraphNode } from '../../../api/types.gen';
import { nodeFill, retractedColor } from './credColors';
import {
  DIMMED_OPACITY,
  edgeStyle,
  HUB_RADIUS,
  HUB_TOP_N,
  LABEL_FONT_PX,
  LABEL_FONT_PX_HUB,
  NODE_BASE_RADIUS,
} from './graphStyle';

interface Props {
  graph: Graph;
  selectedId: string | null;
  hoveredId: string | null;
  onHover: (id: string | null) => void;
  onNodeClick: (node: GraphNode) => void;
  /// Optional: if set, nodes whose id is NOT in this set are dimmed and dashed
  /// (used in "preview at SHA" mode to ghost future-state additions).
  visibleSet: Set<string> | null;
}

interface Sized {
  width: number;
  height: number;
}

function useSize(): [React.RefObject<HTMLDivElement | null>, Sized] {
  const ref = useRef<HTMLDivElement | null>(null);
  const [size, setSize] = useState<Sized>({ width: 600, height: 400 });
  useEffect(() => {
    if (!ref.current) return;
    const el = ref.current;
    const ro = new ResizeObserver(() => {
      const r = el.getBoundingClientRect();
      setSize({
        width: Math.max(1, Math.floor(r.width)),
        height: Math.max(1, Math.floor(r.height)),
      });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return [ref, size];
}

export function ForceGraphCanvas({
  graph,
  selectedId,
  hoveredId,
  onHover,
  onNodeClick,
  visibleSet,
}: Props) {
  const fgRef = useRef<ForceGraphMethods | undefined>(undefined);
  const [containerRef, size] = useSize();

  // Convert API graph → force-graph data. react-force-graph mutates nodes
  // (adds x/y) and links — so we keep our own stable copies.
  const data = useMemo(() => {
    const nodes = graph.nodes.map((n) => ({ ...n }));
    const links = graph.edges.map((e) => ({ source: e.from, target: e.to, relation: e.relation }));
    return { nodes, links };
  }, [graph]);

  // Degree centrality for hub treatment.
  const hubIds = useMemo(() => {
    const deg = new Map<string, number>();
    for (const e of graph.edges) {
      deg.set(e.from, (deg.get(e.from) ?? 0) + 1);
      deg.set(e.to, (deg.get(e.to) ?? 0) + 1);
    }
    return new Set(
      [...deg.entries()]
        .sort((a, b) => b[1] - a[1])
        .slice(0, HUB_TOP_N)
        .map(([id]) => id)
    );
  }, [graph]);

  // Neighbour map for hover dimming.
  const neighbours = useMemo(() => {
    const m = new Map<string, Set<string>>();
    const touch = (a: string, b: string) => {
      if (!m.has(a)) m.set(a, new Set());
      m.get(a)!.add(b);
    };
    for (const e of graph.edges) {
      touch(e.from, e.to);
      touch(e.to, e.from);
    }
    return m;
  }, [graph]);

  const focusId = selectedId ?? hoveredId;

  return (
    <div ref={containerRef} className="w-full h-full overflow-hidden">
      <ForceGraph2D
        ref={fgRef as unknown as React.MutableRefObject<ForceGraphMethods>}
        graphData={data}
        width={size.width}
        height={size.height}
        cooldownTicks={120}
        d3VelocityDecay={0.3}
        nodeRelSize={NODE_BASE_RADIUS}
        backgroundColor="transparent"
        onNodeHover={(n) => onHover((n as GraphNode | null)?.id ?? null)}
        onNodeClick={(n) => onNodeClick(n as GraphNode)}
        nodeCanvasObject={(rawNode, ctx, globalScale) => {
          const n = rawNode as GraphNode & { x: number; y: number };
          const isHub = hubIds.has(n.id);
          const r = isHub ? HUB_RADIUS : NODE_BASE_RADIUS;
          const dim =
            (focusId && focusId !== n.id && !neighbours.get(focusId)?.has(n.id)) ||
            (visibleSet && !visibleSet.has(n.id));
          ctx.globalAlpha = dim ? DIMMED_OPACITY : 1.0;
          ctx.beginPath();
          ctx.arc(n.x, n.y, r, 0, Math.PI * 2);
          ctx.fillStyle = nodeFill(n);
          ctx.fill();
          if (isHub) {
            ctx.lineWidth = 1.5;
            ctx.strokeStyle = '#1f1f1f';
            ctx.stroke();
          }
          if ((n as { retracted?: boolean }).retracted) {
            // Small red "!" badge top-right of the node.
            const bx = n.x + r * 0.7;
            const by = n.y - r * 0.7;
            const br = Math.max(3, r * 0.45);
            ctx.beginPath();
            ctx.arc(bx, by, br, 0, Math.PI * 2);
            ctx.fillStyle = retractedColor;
            ctx.fill();
            ctx.fillStyle = '#fff';
            ctx.font = `700 ${br * 1.2}px ui-sans-serif`;
            ctx.textAlign = 'center';
            ctx.textBaseline = 'middle';
            ctx.fillText('!', bx, by + 0.5);
          }
          // Label
          const fs = (isHub ? LABEL_FONT_PX_HUB : LABEL_FONT_PX) / globalScale;
          ctx.font = `${isHub ? '600' : '400'} ${fs}px ui-sans-serif, system-ui, -apple-system`;
          ctx.fillStyle = '#cfd2dc';
          ctx.textAlign = 'left';
          ctx.textBaseline = 'middle';
          ctx.fillText(' ' + n.label, n.x + r + 1, n.y);
          ctx.globalAlpha = 1.0;
        }}
        linkCanvasObject={(rawLink, ctx) => {
          const l = rawLink as {
            source: GraphNode & { x: number; y: number };
            target: GraphNode & { x: number; y: number };
          };
          const tier =
            (l.source.kind === 'source' ? l.source.credibility_tier : null) ??
            (l.target.kind === 'source' ? l.target.credibility_tier : null);
          const style = edgeStyle(tier);
          const dim = focusId && l.source.id !== focusId && l.target.id !== focusId;
          ctx.globalAlpha = dim ? DIMMED_OPACITY : 0.9;
          ctx.strokeStyle =
            focusId && (l.source.id === focusId || l.target.id === focusId)
              ? '#7aa57c' // --t-green
              : '#5b6072';
          ctx.lineWidth = style.width;
          if (style.dash) ctx.setLineDash(style.dash);
          else ctx.setLineDash([]);
          ctx.beginPath();
          ctx.moveTo(l.source.x, l.source.y);
          ctx.lineTo(l.target.x, l.target.y);
          ctx.stroke();
          ctx.setLineDash([]);
          ctx.globalAlpha = 1.0;
        }}
        linkCanvasObjectMode={() => 'replace'}
      />
    </div>
  );
}

