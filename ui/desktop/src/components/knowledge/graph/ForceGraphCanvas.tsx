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

  useEffect(() => {
    const fg = fgRef.current as
      | (ForceGraphMethods & {
          d3Force?: (name: string) => {
            strength?: (value: number) => unknown;
            distance?: (value: number) => unknown;
            distanceMax?: (value: number) => unknown;
          };
          zoomToFit?: (durationMs?: number, paddingPx?: number) => void;
        })
      | undefined;

    if (!fg) {
      return;
    }

    const nodeCount = graph.nodes.length;
    const spread = nodeCount > 80 ? 155 : nodeCount > 35 ? 135 : 115;

    fg.d3Force?.('charge')?.strength?.(-spread);
    fg.d3Force?.('charge')?.distanceMax?.(360);
    fg.d3Force?.('link')?.distance?.(nodeCount > 80 ? 92 : 108);
    fg.d3Force?.('link')?.strength?.(0.22);

    const timeout = window.setTimeout(() => {
      fg.zoomToFit?.(500, 84);
    }, 900);

    return () => window.clearTimeout(timeout);
  }, [graph, size.height, size.width]);

  return (
    <div
      ref={containerRef}
      className="h-full w-full overflow-hidden"
      style={{
        background:
          'radial-gradient(circle at top left, rgba(214, 176, 106, 0.08), transparent 32%), radial-gradient(circle at top right, rgba(73, 101, 154, 0.08), transparent 28%), linear-gradient(180deg, rgba(255, 255, 255, 0.02), rgba(0, 0, 0, 0.04))',
      }}
    >
      <ForceGraph2D
        ref={fgRef as unknown as React.MutableRefObject<ForceGraphMethods>}
        graphData={data}
        width={size.width}
        height={size.height}
        cooldownTicks={120}
        d3VelocityDecay={0.3}
        nodeRelSize={NODE_BASE_RADIUS}
        backgroundColor="transparent"
        onNodeHover={(n: unknown) => onHover((n as GraphNode | null)?.id ?? null)}
        onNodeClick={(n: unknown) => onNodeClick(n as GraphNode)}
        nodeCanvasObject={(rawNode: unknown, ctx, globalScale) => {
          const n = rawNode as GraphNode & { x: number; y: number };
          const isHub = hubIds.has(n.id);
          const r = isHub ? HUB_RADIUS : NODE_BASE_RADIUS;
          const isFocused = focusId === n.id;
          const isNeighbour = !!(focusId && neighbours.get(focusId)?.has(n.id));
          const dim =
            (focusId && !isFocused && !isNeighbour) ||
            (visibleSet && !visibleSet.has(n.id));
          ctx.globalAlpha = dim ? DIMMED_OPACITY : 1.0;
          ctx.beginPath();
          ctx.arc(n.x, n.y, r, 0, Math.PI * 2);
          ctx.fillStyle = nodeFill(n);
          ctx.fill();
          ctx.lineWidth = isHub ? 1.05 : 0.85;
          ctx.strokeStyle = 'rgba(31, 36, 44, 0.5)';
          ctx.stroke();
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
          const shouldShowLabel = isFocused || isNeighbour || isHub || globalScale >= 1.75;
          if (shouldShowLabel) {
            const fs = (isHub ? LABEL_FONT_PX_HUB : LABEL_FONT_PX) / globalScale;
            ctx.font = `${isHub || isFocused ? '600' : '400'} ${fs}px ui-sans-serif, system-ui, -apple-system`;
            ctx.fillStyle = '#1f242c';
            ctx.textAlign = 'left';
            ctx.textBaseline = 'middle';
            ctx.fillText(' ' + n.label, n.x + r + 1, n.y);
          }
          ctx.globalAlpha = 1.0;
        }}
        linkCanvasObject={(rawLink: unknown, ctx) => {
          const l = rawLink as {
            source: GraphNode & { x: number; y: number };
            target: GraphNode & { x: number; y: number };
          };
          const tier =
            (l.source.kind === 'source' ? l.source.credibility_tier : null) ??
            (l.target.kind === 'source' ? l.target.credibility_tier : null);
          const style = edgeStyle(tier);
          const dim = focusId && l.source.id !== focusId && l.target.id !== focusId;
          ctx.globalAlpha = dim ? 0.12 : 0.42;
          ctx.strokeStyle =
            focusId && (l.source.id === focusId || l.target.id === focusId)
              ? 'rgba(99, 141, 104, 0.75)'
              : 'rgba(119, 128, 145, 0.42)';
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
