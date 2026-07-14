declare module 'react-force-graph-2d' {
  import type { ForwardRefExoticComponent, PropsWithoutRef, RefAttributes } from 'react';

  // Opaque ref-handle marker. Call sites intersect this with the concrete
  // imperative methods they use (e.g. `ForceGraphMethods & { zoomToFit(): void }`),
  // so the base only needs to denote "some object" — `object` is the type
  // `@typescript-eslint/no-empty-object-type` recommends over an empty interface.
  export type ForceGraphMethods = object;

  export interface ForceGraph2DProps {
    graphData: {
      nodes: unknown[];
      links: unknown[];
    };
    width?: number;
    height?: number;
    cooldownTicks?: number;
    d3VelocityDecay?: number;
    nodeRelSize?: number;
    backgroundColor?: string;
    onNodeHover?: (node: unknown | null) => void;
    onNodeClick?: (node: unknown) => void;
    nodeCanvasObject?: (node: unknown, ctx: CanvasRenderingContext2D, globalScale: number) => void;
    linkCanvasObject?: (link: unknown, ctx: CanvasRenderingContext2D) => void;
    linkCanvasObjectMode?: () => 'replace' | 'before' | 'after';
  }

  const ForceGraph2D: ForwardRefExoticComponent<
    PropsWithoutRef<ForceGraph2DProps> & RefAttributes<ForceGraphMethods>
  >;

  export default ForceGraph2D;
}
