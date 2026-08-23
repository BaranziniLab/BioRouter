declare module 'react-force-graph-2d' {
  import type { ForwardRefExoticComponent, PropsWithoutRef, RefAttributes } from 'react';

  // Opaque ref-handle marker. Call sites intersect this with the concrete
  // imperative methods they use (e.g. `ForceGraphMethods & { zoomToFit(): void }`),
  // so the base only needs to denote "some object" — `object` is the type
  // `@typescript-eslint/no-empty-object-type` recommends over an empty interface.
  export type ForceGraphMethods = {
    /** Pan so `(x, y)` is centred, over `ms`. §5.12 uses it to follow keyboard focus. */
    centerAt?: (x?: number, y?: number, ms?: number) => void;
    /** Fit the whole graph, over `ms`, leaving `padding` screen px on every side. */
    zoomToFit?: (ms?: number, padding?: number) => void;
  };

  /**
   * The subset of force-graph's surface this app uses.
   *
   * ⚠ **This shim SHADOWS the package's own `.d.ts`** (`src/types` is on
   * `typeRoots`), so a prop absent here is a type error at the call site even
   * though the library accepts it — and, worse, a prop declared here with the
   * wrong arity silently narrows a callback the library really does call with
   * more arguments. `linkCanvasObject` was declared `(link, ctx)` while
   * force-graph calls it `(link, ctx, globalScale)`; every screen-space constant
   * in the edge painter is divided by that third argument, so the shim was the
   * only thing standing between the painter and the scale it needs.
   *
   * Extend it when you reach for a new prop. Do not delete it in favour of the
   * vendored types without checking what their `any`-typed accessors let past.
   */
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
    onLinkHover?: (link: unknown | null) => void;
    onLinkClick?: (link: unknown) => void;
    /** `lane * (bend / globalScale) / length`, so the SHADOW canvas hit-tests the drawn curve. */
    linkCurvature?: (link: unknown) => number;
    /**
     * Viewport culling. Both accessors are in force-graph's `bindBoth` list, so
     * they apply to the hit-test canvas as well as the visible one.
     */
    nodeVisibility?: (node: unknown) => boolean;
    linkVisibility?: (link: unknown) => boolean;
    /** Runs BEFORE `tickFrame()` → `paintLinks()` → `paintNodes()`. */
    onRenderFramePre?: (ctx: CanvasRenderingContext2D, globalScale: number) => void;
    /** Runs after every node and edge is down — where the label pass has to live. */
    onRenderFramePost?: (ctx: CanvasRenderingContext2D, globalScale: number) => void;
    nodeCanvasObject?: (node: unknown, ctx: CanvasRenderingContext2D, globalScale: number) => void;
    linkCanvasObject?: (link: unknown, ctx: CanvasRenderingContext2D, globalScale: number) => void;
    linkCanvasObjectMode?: () => 'replace' | 'before' | 'after';
  }

  const ForceGraph2D: ForwardRefExoticComponent<
    PropsWithoutRef<ForceGraph2DProps> & RefAttributes<ForceGraphMethods>
  >;

  export default ForceGraph2D;
}
