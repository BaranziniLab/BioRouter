declare module 'react-force-graph-2d' {
  import type { ForwardRefExoticComponent, PropsWithoutRef, RefAttributes } from 'react';

  export interface ForceGraphMethods {}

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
    nodeCanvasObject?: (
      node: unknown,
      ctx: CanvasRenderingContext2D,
      globalScale: number
    ) => void;
    linkCanvasObject?: (link: unknown, ctx: CanvasRenderingContext2D) => void;
    linkCanvasObjectMode?: () => 'replace' | 'before' | 'after';
  }

  const ForceGraph2D: ForwardRefExoticComponent<
    PropsWithoutRef<ForceGraph2DProps> & RefAttributes<ForceGraphMethods>
  >;

  export default ForceGraph2D;
}
