/**
 * Where a dragged tab would land inside a chat group.
 *
 * `center` = move the chat into that group. An edge = split the group and put
 * the chat in the new half. Spec §4, the "Drop zones" box.
 */
export type DropZone = 'center' | 'left' | 'right' | 'top' | 'bottom';

/**
 * How far in from an edge still counts as that edge, as a fraction of the box.
 *
 * 0.25 means the outer quarter on each side splits and the middle half moves.
 * The spec's drop-zone diagram draws exactly this: four edge bands around one
 * central "move here" cell.
 */
export const EDGE_FRACTION = 0.25;

export interface DropTarget {
  groupId: string;
  zone: DropZone;
}

/**
 * The zone a point falls in, purely from geometry. No DOM, no React — this is
 * the one genuinely testable piece of the drag gesture, so it is a free function
 * and it is unit-tested (dropZones.test.ts).
 *
 * A corner is inside TWO edge bands at once, so "is it in the left band?" asked
 * band-by-band is ambiguous and order-dependent. Instead every edge's incursion
 * depth is measured and the SINGLE DEEPEST one wins: at (2%, 10%) of the box you
 * are 2% into the left band and 10% into the top band, so left is the deeper
 * incursion and the answer is `left`. That makes a corner unambiguous and, more
 * importantly, makes it STABLE — dragging along the top edge cannot flicker
 * between `top` and `left` as x wobbles.
 *
 * Exact ties (the true diagonal corner) resolve in declaration order:
 * left > right > top > bottom. Arbitrary but deterministic, and tested as such.
 */
export function zoneFromRect(
  rect: { left: number; top: number; width: number; height: number },
  x: number,
  y: number
): DropZone {
  // A zero-sized box has no edges to speak of; every point is the centre. Guards
  // a division by zero that would otherwise make every zone NaN -> `center`
  // anyway, but silently and by accident rather than on purpose.
  if (rect.width <= 0 || rect.height <= 0) return 'center';

  const fromLeft = (x - rect.left) / rect.width;
  const fromRight = (rect.left + rect.width - x) / rect.width;
  const fromTop = (y - rect.top) / rect.height;
  const fromBottom = (rect.top + rect.height - y) / rect.height;

  const edges: Array<{ zone: DropZone; depth: number }> = [
    { zone: 'left', depth: fromLeft },
    { zone: 'right', depth: fromRight },
    { zone: 'top', depth: fromTop },
    { zone: 'bottom', depth: fromBottom },
  ];

  let best: { zone: DropZone; depth: number } | null = null;
  for (const edge of edges) {
    if (edge.depth >= EDGE_FRACTION) continue;
    if (!best || edge.depth < best.depth) best = edge;
  }
  return best ? best.zone : 'center';
}

/**
 * Hit-test a viewport point against the chat groups on screen.
 *
 * `document.elementFromPoint` rather than per-group pointerenter/leave: the hit
 * test cannot go stale. Panes move while you drag (a split resizes its
 * neighbours the instant it lands), and enter/leave bookkeeping against a moving
 * target is exactly how a zone latches on the group you already left.
 *
 * This is why the drop overlay and the drag ghost MUST be pointer-events:none.
 * elementFromPoint returns the TOPMOST element at the point; an overlay that
 * accepted events would return itself the moment it appeared, `closest()` would
 * still find its own group, and the zone would freeze on whichever half tinted
 * first — you could never aim at the other half. The ghost sits directly under
 * the cursor, so it would shadow every group equally and nothing would hit-test
 * at all.
 */
export function dropTargetAtPoint(x: number, y: number): DropTarget | null {
  const host = document.elementFromPoint(x, y)?.closest<HTMLElement>('[data-chat-group-id]');
  const groupId = host?.dataset.chatGroupId;
  if (!host || !groupId) return null;
  return { groupId, zone: zoneFromRect(host.getBoundingClientRect(), x, y) };
}
