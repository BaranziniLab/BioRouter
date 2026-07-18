import { GroupLayout, ChatGroupId, leafGroupIds } from './chatGroupsTypes';
import { DropZone } from './dropZones';

/**
 * The split cap.
 *
 * R4 was MEASURED (plan banner, 2026-07-16): N mounted BaseChats cost ~74 DOM
 * nodes and ~1 MB each, and heap was flat from 3 to 6 chats. So this is not a
 * memory cliff and 4 is not a fear — it is the honest edge of the evidence. The
 * measurement used small dashboard windows and freshly-spawned sessions with
 * short transcripts; a full-height group with a long transcript and tiktoken
 * counting will cost more than 1 MB. The measurement retires R4 as a blocker; it
 * does not license an unbounded split. Re-measure before raising this.
 */
export const MAX_GROUPS = 4;

/** Sizes that sum to 1. Equal split when the input is unusable. */
export function normalizeSizes(sizes: readonly number[], count: number): number[] {
  if (count <= 0) return [];
  const usable = sizes.length === count && sizes.every((s) => Number.isFinite(s) && s > 0);
  if (!usable) return Array.from({ length: count }, () => 1 / count);
  const total = sizes.reduce((a, b) => a + b, 0);
  if (!(total > 0)) return Array.from({ length: count }, () => 1 / count);
  return sizes.map((s) => s / total);
}

const DIR_OF: Record<Exclude<DropZone, 'center'>, 'row' | 'col'> = {
  left: 'row',
  right: 'row',
  top: 'col',
  bottom: 'col',
};

/** Does the new group go BEFORE the one that was dropped on? */
const BEFORE: Record<Exclude<DropZone, 'center'>, boolean> = {
  left: true,
  right: false,
  top: true,
  bottom: false,
};

export function splitDirection(zone: Exclude<DropZone, 'center'>): 'row' | 'col' {
  return DIR_OF[zone];
}

/**
 * Replace the `targetGroupId` leaf with a branch holding it and `newGroupId`.
 *
 * Deliberately always NESTS rather than merging into a same-direction parent.
 * VS Code merges (split-right twice gives three even columns; this gives
 * 50/25/25), but the merge has to reason about the parent's sizes array while
 * rewriting it, and an even-columns nicety is not worth that on the first pass.
 * The result is correct, just not evenly weighted. Named here so the next reader
 * knows it is a choice and not an oversight.
 */
export function splitLeaf(
  layout: GroupLayout,
  targetGroupId: ChatGroupId,
  newGroupId: ChatGroupId,
  zone: Exclude<DropZone, 'center'>
): GroupLayout {
  const dir = DIR_OF[zone];
  const before = BEFORE[zone];

  const walk = (node: GroupLayout): GroupLayout => {
    if (node.kind === 'leaf') {
      if (node.groupId !== targetGroupId) return node;
      const fresh: GroupLayout = { kind: 'leaf', groupId: newGroupId };
      return {
        kind: 'branch',
        dir,
        children: before ? [fresh, node] : [node, fresh],
        sizes: [0.5, 0.5],
      };
    }
    return { ...node, children: node.children.map(walk) };
  };

  return walk(layout);
}

/**
 * Remove a leaf, collapsing any branch left with a single child INTO that child.
 *
 * Without the collapse, closing one half of a split leaves `branch[leaf]` — a
 * splitter with nothing on one side, and a tree that grows a useless level every
 * time you split and unsplit. Returns null if the last leaf was removed, which
 * callers must refuse (there is always at least one group).
 */
export function removeLeaf(layout: GroupLayout, groupId: ChatGroupId): GroupLayout | null {
  if (layout.kind === 'leaf') return layout.groupId === groupId ? null : layout;

  const kept: GroupLayout[] = [];
  const keptSizes: number[] = [];
  layout.children.forEach((child, index) => {
    const next = removeLeaf(child, groupId);
    if (!next) return;
    kept.push(next);
    keptSizes.push(layout.sizes[index] ?? 1 / layout.children.length);
  });

  if (kept.length === 0) return null;
  // Collapse: a branch with one child IS that child.
  if (kept.length === 1) return kept[0];
  return { ...layout, children: kept, sizes: normalizeSizes(keptSizes, kept.length) };
}

/**
 * A branch is addressed by its PATH from the root ([] is the root, [1,0] is the
 * first child of the second child). An index into a flat list of branches would
 * be re-derived on every render and would silently point at a different branch
 * the moment a split or a close reshaped the tree.
 */
export function setSizesAtPath(
  layout: GroupLayout,
  path: readonly number[],
  sizes: readonly number[]
): GroupLayout {
  if (path.length === 0) {
    if (layout.kind !== 'branch') return layout;
    if (sizes.length !== layout.children.length) return layout;
    return { ...layout, sizes: normalizeSizes(sizes, layout.children.length) };
  }
  if (layout.kind !== 'branch') return layout;
  const [head, ...rest] = path;
  const child = layout.children[head];
  if (!child) return layout;
  const nextChild = setSizesAtPath(child, rest, sizes);
  if (nextChild === child) return layout;
  const children = [...layout.children];
  children[head] = nextChild;
  return { ...layout, children };
}

export function groupCountOf(layout: GroupLayout): number {
  return leafGroupIds(layout).length;
}
