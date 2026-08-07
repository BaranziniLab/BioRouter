// ui/desktop/src/components/knowledge/graph/ForceGraphCanvas.tsx
import { useEffect, useMemo, useRef, useState } from 'react';
import ForceGraph2D, { ForceGraphMethods } from 'react-force-graph-2d';
import { forceX, forceY } from 'd3-force';
import type { Graph, GraphNode } from '../../../api/types.gen';
import { nodeFill, retractedColor } from './credColors';
import { prettyLabel, wrapLabel } from './labelText';
import {
  CANVAS_FONT_FALLBACK,
  DIMMED_OPACITY,
  edgeStyle,
  HUB_RADIUS,
  HUB_TOP_N,
  LABEL_FONT_PX,
  LABEL_FONT_PX_HUB,
  NODE_BASE_RADIUS,
  resolveCanvasFontFamily,
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
    const update = () => {
      const r = el.getBoundingClientRect();
      setSize({
        width: Math.max(1, Math.floor(r.width)),
        height: Math.max(1, Math.floor(r.height)),
      });
    };
    const ro = new ResizeObserver(() => {
      update();
    });
    ro.observe(el);
    const raf = window.requestAnimationFrame(update);
    window.addEventListener('resize', update);
    return () => {
      window.cancelAnimationFrame(raf);
      window.removeEventListener('resize', update);
      ro.disconnect();
    };
  }, []);
  return [ref, size];
}

/// The resolved UI font family, read off the graph container's computed style.
///
/// Read once on mount and again whenever the root's theme signal changes, so a
/// family that re-points the face is picked up without re-reading a computed
/// style on every node of every frame.
function useCanvasFontFamily(ref: React.RefObject<HTMLDivElement | null>): string {
  const [family, setFamily] = useState<string>(CANVAS_FONT_FALLBACK);
  useEffect(() => {
    const read = () => setFamily(resolveCanvasFontFamily(ref.current));
    read();
    if (typeof MutationObserver !== 'function') return;
    const observer = new MutationObserver(read);
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['class', 'data-theme'],
    });
    return () => observer.disconnect();
  }, [ref]);
  return family;
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
  const fontFamily = useCanvasFontFamily(containerRef);

  // Convert API graph → force-graph data. react-force-graph mutates nodes
  // (adds x/y) and links — so we keep our own stable copies.
  const data = useMemo(() => {
    const nodes = graph.nodes.map((n) => ({ ...n }));
    const links = graph.edges.map((e) => ({ source: e.from, target: e.to, relation: e.relation }));
    return { nodes, links };
  }, [graph]);

  // Degree centrality for hub treatment.
  const hubIdList = useMemo(() => {
    const deg = new Map<string, number>();
    for (const e of graph.edges) {
      deg.set(e.from, (deg.get(e.from) ?? 0) + 1);
      deg.set(e.to, (deg.get(e.to) ?? 0) + 1);
    }
    return [...deg.entries()]
      .sort((a, b) => b[1] - a[1])
      .slice(0, HUB_TOP_N)
      .map(([id]) => id);
  }, [graph]);
  const hubIds = useMemo(() => new Set(hubIdList), [hubIdList]);
  const compactCanvas = size.width < 560 || size.height < 430;
  const labeledHubIds = useMemo(
    () => new Set(hubIdList.slice(0, compactCanvas ? 3 : HUB_TOP_N)),
    [compactCanvas, hubIdList]
  );

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
    // Cap the range of charge repulsion so a disconnected node is not flung to
    // the far edge of the canvas by the cumulative push of the whole cluster.
    fg.d3Force?.('charge')?.distanceMax?.(260);
    fg.d3Force?.('link')?.distance?.(nodeCount > 80 ? 92 : 108);
    fg.d3Force?.('link')?.strength?.(0.22);

    // Gentle centering pull toward the origin. Linked nodes are held in place
    // by link tension, so this barely moves the connected hub — but an isolated
    // node (no incoming/outgoing edges) feels only this force and drifts back
    // toward the cluster instead of being pushed off-screen by charge repulsion.
    const fgWithForce = fg as typeof fg & {
      d3Force: (name: string, force?: unknown) => unknown;
    };
    fgWithForce.d3Force('x', forceX(0).strength(0.07));
    fgWithForce.d3Force('y', forceY(0).strength(0.07));

    const timeout = window.setTimeout(() => {
      const fitPadding = compactCanvas
        ? 72
        : Math.max(112, Math.min(size.width, size.height) * 0.16);
      fg.zoomToFit?.(500, fitPadding);
    }, 900);

    return () => window.clearTimeout(timeout);
  }, [graph, size.height, size.width, compactCanvas]);

  return (
    <div
      data-testid="knowledge-graph-canvas"
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
            (focusId && !isFocused && !isNeighbour) || (visibleSet && !visibleSet.has(n.id));
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
            ctx.font = `700 ${br * 1.2}px ${fontFamily}`;
            ctx.textAlign = 'center';
            ctx.textBaseline = 'middle';
            ctx.fillText('!', bx, by + 0.5);
          }
          // Label — prettified (machine ids → readable) and folded across up to
          // three lines so a long title never paints one very long horizontal
          // string off the side of its node.
          const shouldShowLabel =
            isFocused ||
            isNeighbour ||
            labeledHubIds.has(n.id) ||
            (!compactCanvas && globalScale >= 1.75);
          if (shouldShowLabel) {
            const fs = (isHub ? LABEL_FONT_PX_HUB : LABEL_FONT_PX) / globalScale;
            ctx.font = `${isHub || isFocused ? '600' : '400'} ${fs}px ${fontFamily}`;
            ctx.fillStyle = '#1f242c';
            ctx.textAlign = 'left';
            ctx.textBaseline = 'middle';
            const text = prettyLabel(n.label, n.kind);
            const maxWidth = (compactCanvas ? 108 : 132) / globalScale;
            const lines = wrapLabel(
              text,
              maxWidth,
              compactCanvas ? 2 : 3,
              (s) => ctx.measureText(s).width
            );
            const lineHeight = fs * 1.18;
            const startY = n.y - (lineHeight * (lines.length - 1)) / 2;
            for (let i = 0; i < lines.length; i++) {
              ctx.fillText(' ' + lines[i], n.x + r + 1, startY + i * lineHeight);
            }
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
