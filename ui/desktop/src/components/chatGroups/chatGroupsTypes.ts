import { UserAttachment } from '../../types/message';

export type ChatTabId = string; // 'tab-7'
export type ChatGroupId = string; // 'grp-1'

export interface ChatTab {
  /** Stable across sessionId changes — this is why tabId !== sessionId. A tab
   *  opened empty and later bound to a created session keeps its identity, so
   *  React keys and strip order survive the bind. */
  tabId: ChatTabId;
  /** '' only transiently, before createSession resolves. Pruned on load. */
  sessionId: string;
  /** Mirror for the strip; kept fresh by utils/sessionNameSync.ts. */
  title: string;
  userSetName: boolean;
  /** VS Code enablePreview. Italic label (.br-tab--preview, main.css — its first
   *  consumer). The next preview-open in this group REUSES this tab in place,
   *  keeping tabId. At most one per group; reducer invariant. */
  preview: boolean;
  /** Route-state cargo, consumed exactly once by BaseChat on mount. NEVER
   *  persisted — a queued message that re-sends after a reload is a data bug.
   *  chatGroupsStorage strips both fields on write; chatGroupsStorage.test.ts
   *  asserts their absence. */
  pendingInitialMessage?: string;
  pendingInitialAttachments?: UserAttachment[];
  workflowId?: string;
  cwd?: string;
}

export interface ChatGroup {
  groupId: ChatGroupId;
  /** Array order IS strip order. */
  tabs: ChatTab[];
  /** null only when tabs === []. */
  activeTabId: ChatTabId | null;
}

/**
 * The tree ships today with depth 0. It exists now so the split is not a state
 * migration later. ChatGroupsShell renders `leaf` only and throws in dev on a
 * branch — see its comment for why the type is wider than the renderer.
 */
export type GroupLayout =
  | { kind: 'leaf'; groupId: ChatGroupId }
  | { kind: 'branch'; dir: 'row' | 'col'; children: GroupLayout[]; sizes: number[] };

export interface ChatGroupsState {
  version: 1;
  layout: GroupLayout;
  groups: Record<ChatGroupId, ChatGroup>;
  /** ALWAYS names a live leaf. Reducer invariant. */
  activeGroupId: ChatGroupId;
  /** Deterministic ids -> testable reducer. */
  seq: number;
}

/**
 * The FIRST leaf of the layout tree, by a recursive children[0] walk.
 *
 * This must never become an array index. Only the first leaf's strip collides
 * with the macOS traffic lights, and in a root `col` split BOTH children sit at
 * x=0 — an index-based predicate would reserve the titlebar gap for the wrong
 * strip (or for both), and the failure is SILENT: the tabs simply slide under
 * the traffic lights with no error. Spec card Pi exists because this reserve
 * failed silently once already.
 *
 * It ships as a tree walk today at depth 0 (where it is a tautology) precisely
 * so the split cannot introduce it later as a bug. Tested against a synthetic
 * depth-1 `col` tree that the renderer cannot yet produce.
 */
export function firstLeaf(layout: GroupLayout): ChatGroupId {
  let node = layout;
  while (node.kind === 'branch') {
    if (node.children.length === 0) {
      throw new Error('firstLeaf: branch node has no children');
    }
    node = node.children[0];
  }
  return node.groupId;
}

/** Every leaf groupId in the tree, in strip order. */
export function leafGroupIds(layout: GroupLayout): ChatGroupId[] {
  if (layout.kind === 'leaf') return [layout.groupId];
  return layout.children.flatMap(leafGroupIds);
}
