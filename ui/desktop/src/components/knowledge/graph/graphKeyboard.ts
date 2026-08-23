// ui/desktop/src/components/knowledge/graph/graphKeyboard.ts

/**
 * The pure half of §5.12's keyboard model.
 *
 * ⚠ **THE CANVAS WAS POINTER-ONLY, WHICH IS WCAG 2.1.1 AT LEVEL A.** §3.5 calls
 * the canvas "the reason the view exists" and the one pane that never yields,
 * and it had no tab stop, no focus model and no traversal — so the primary
 * content of the section was unreachable without a mouse. §5.7 then made edges
 * selectable and defined that purely in pointer terms, widening the gap.
 *
 * ⚠ **This is also where the redundant channel now lives.** The node shape
 * channel was deleted (every node is a circle), so colour is the only visual
 * encoding left and it does not survive dichromacy — cross-family distance
 * bottoms out near ΔE00 0.00. `announce()` below is the replacement, and it is a
 * strictly better one: a spoken `IL6, Gene, Genomic` serves blind users as well
 * as colour-blind ones, where a silhouette only ever served the second group.
 *
 * Everything here is free of React and the DOM on purpose. `ui-spec.md` §10
 * names "the arrow-key candidate selection in §5.12" as jsdom-testable and the
 * rest of the canvas as browser-only, and the reason `utils/messageClamp.ts`
 * records applies: a threshold you can only exercise by rendering a component is
 * one nobody re-tests — and this component cannot be rendered in jsdom at all,
 * because force-graph calls `canvas.getContext('2d')`.
 */

export type Direction = 'up' | 'down' | 'left' | 'right';

/** The minimum a caller must supply for traversal: an id and a position. */
export interface KeyboardNode {
  id: string;
  x: number;
  y: number;
}

/**
 * Screen-space direction vectors.
 *
 * ⚠ **`up` is `-y`.** Canvas y grows downward, so the arithmetic below is in
 * screen space rather than mathematical space; getting this backwards inverts
 * the whole traversal and still passes any test that only checks "some node was
 * chosen".
 */
const VECTORS: Record<Direction, { dx: number; dy: number }> = {
  up: { dx: 0, dy: -1 },
  down: { dx: 0, dy: 1 },
  left: { dx: -1, dy: 0 },
  right: { dx: 1, dy: 0 },
};

/** §5.12's cone half-angle. cos(60°) = 0.5. */
const CONE_COS = 0.5;

/**
 * The node an arrow key moves to, or `null` when nothing lies that way.
 *
 * §5.12: "the nearest candidate within a ±60° cone, falling back to the nearest
 * node in that half-plane". Both passes are needed and the order matters — the
 * cone keeps a press feeling directional in a dense field, and the half-plane
 * stops a press being a no-op in a sparse one, which would read as a broken key.
 *
 * `candidates` is the CURRENT FILTER SET, not what is on screen. Traversing to a
 * node outside the viewport is intended; the caller centres it afterwards.
 */
export function nextNodeInDirection<T extends KeyboardNode>(
  from: KeyboardNode,
  candidates: readonly T[],
  direction: Direction
): T | null {
  const v = VECTORS[direction];
  let coneBest: T | null = null;
  let coneDist = Infinity;
  let planeBest: T | null = null;
  let planeDist = Infinity;

  for (const n of candidates) {
    if (n.id === from.id) continue;
    const dx = n.x - from.x;
    const dy = n.y - from.y;
    const dist = Math.hypot(dx, dy);
    // A node exactly on top of the focused one has no direction; skipping it
    // avoids a division by zero that would otherwise produce NaN and silently
    // lose the candidate.
    if (dist === 0) continue;

    const projection = (dx * v.dx + dy * v.dy) / dist;
    if (projection <= 0) continue; // behind, or exactly perpendicular

    if (projection >= CONE_COS) {
      if (dist < coneDist) {
        coneDist = dist;
        coneBest = n;
      }
    } else if (dist < planeDist) {
      planeDist = dist;
      planeBest = n;
    }
  }

  return coneBest ?? planeBest;
}

/**
 * The visible set in the order Tab walks it: descending degree, hubs first.
 *
 * §5.12 ties this to the label ladder's priority deliberately — the node a
 * reader's eye goes to first should also be the one a keyboard reaches first.
 * Ties break on `id` so the order is stable across renders; without that, two
 * equal-degree nodes could swap places between presses and Tab would appear to
 * jump backwards.
 */
export function tabOrder<T extends { id: string }>(
  nodes: readonly T[],
  degreeOf: (id: string) => number
): T[] {
  return [...nodes].sort((a, b) => {
    const d = degreeOf(b.id) - degreeOf(a.id);
    return d !== 0 ? d : a.id.localeCompare(b.id);
  });
}

/**
 * Step `delta` places through `order` from `currentId`, wrapping.
 *
 * Wrapping rather than stopping at the ends: the canvas is one tab stop, so a
 * press that did nothing at the last node would read as a dead key rather than
 * as an edge.
 */
export function stepThrough<T extends { id: string }>(
  order: readonly T[],
  currentId: string | null,
  delta: 1 | -1
): T | null {
  if (order.length === 0) return null;
  if (currentId == null) return delta === 1 ? order[0] : order[order.length - 1];
  const i = order.findIndex((n) => n.id === currentId);
  if (i === -1) return order[0];
  return order[(i + delta + order.length) % order.length];
}

/**
 * The `aria-live` announcement for a focused node: `<identifier>, <type>, <family>`.
 *
 * ⚠ **Both optional parts really are optional, and the spec's example hides
 * that.** `identifier` is nullable on the API type, so the display label is the
 * fallback; and a legacy or plain-OKF base has NO families at all, so the third
 * part is dropped rather than announced as "undefined". Announcing a wrong or
 * empty field is worse than announcing a shorter one — this is the only channel
 * a screen-reader user has for the type now that shape is gone.
 */
export function announce(parts: {
  identifier?: string | null;
  label: string;
  nodeType?: string | null;
  family?: string | null;
}): string {
  const name = parts.identifier?.trim() || parts.label;
  const out = [name];
  if (parts.nodeType) out.push(parts.nodeType);
  if (parts.family) out.push(parts.family);
  return out.join(', ');
}

/** The node `Home` focuses: highest degree, ties broken on `id` for stability. */
export function highestDegree<T extends { id: string }>(
  nodes: readonly T[],
  degreeOf: (id: string) => number
): T | null {
  return tabOrder(nodes, degreeOf)[0] ?? null;
}
