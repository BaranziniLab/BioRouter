import { UserAttachment } from '../../types/message';
import {
  ChatGroupsState,
  ChatGroup,
  ChatTab,
  ChatTabId,
  ChatGroupId,
  firstLeaf,
} from './chatGroupsTypes';
import {
  MAX_GROUPS,
  removeLeaf,
  setSizesAtPath,
  splitLeaf,
  groupCountOf,
} from './chatGroupsLayout';
import { DropZone } from './dropZones';

export interface OpenTabPayload {
  sessionId: string;
  title?: string;
  userSetName?: boolean;
  pendingInitialMessage?: string;
  pendingInitialAttachments?: UserAttachment[];
  workflowId?: string;
  cwd?: string;
  /** Target group; defaults to activeGroupId. */
  groupId?: ChatGroupId;
}

export type ChatGroupsAction =
  | { type: 'openTab'; payload: OpenTabPayload }
  | { type: 'activateTab'; tabId: ChatTabId }
  | { type: 'closeTab'; tabId: ChatTabId }
  | { type: 'reorderTab'; draggedTabId: ChatTabId; targetTabId: ChatTabId }
  | { type: 'renameTab'; sessionId: string; title: string; userSetName?: boolean }
  | { type: 'bindSession'; tabId: ChatTabId; sessionId: string }
  | { type: 'consumePending'; tabId: ChatTabId }
  | { type: 'setActiveGroup'; groupId: ChatGroupId }
  /**
   * The drop. `zone: 'center'` MOVES the tab into targetGroupId; an edge SPLITS
   * targetGroupId and moves the tab into the new half. One action for both,
   * because they are one gesture — the user aims and lets go, and the zone under
   * the cursor at that moment is the whole difference.
   */
  | { type: 'moveTabToGroup'; tabId: ChatTabId; targetGroupId: ChatGroupId; zone: DropZone }
  | { type: 'resizeBranch'; path: readonly number[]; sizes: readonly number[] };

export const DEFAULT_TAB_TITLE = 'New Session';

export function createInitialChatGroupsState(): ChatGroupsState {
  const groupId = 'grp-1';
  return {
    version: 1,
    layout: { kind: 'leaf', groupId },
    groups: { [groupId]: { groupId, tabs: [], activeTabId: null } },
    activeGroupId: groupId,
    seq: 1,
  };
}

function findTabGroup(
  state: ChatGroupsState,
  predicate: (tab: ChatTab) => boolean
): { group: ChatGroup; tab: ChatTab } | null {
  for (const group of Object.values(state.groups)) {
    const tab = group.tabs.find(predicate);
    if (tab) return { group, tab };
  }
  return null;
}

function withGroup(state: ChatGroupsState, groupId: ChatGroupId, next: ChatGroup): ChatGroupsState {
  return { ...state, groups: { ...state.groups, [groupId]: next } };
}

/**
 * Open a chat as a tab.
 *
 * Every open is a REAL tab. There is no preview/italic slot and nothing is ever
 * recycled out from under the user: this reducer used to implement VS Code's
 * enablePreview (single click = italic tab, reused in place), and the user
 * rejected it after living with it — clicking a chat in Recents must leave the
 * chat you were reading exactly where it was. Two rules, and only two:
 *
 *   DEDUPE  — a sessionId already open ANYWHERE activates that tab (and focuses
 *             its group) instead of opening a second one. "New chats that are
 *             not already launched as a tab will always launch as a tab."
 *   ADOPT   — the pre-session submit path, and it alone, fills the empty tab the
 *             user is already looking at. See below.
 */
function openTab(state: ChatGroupsState, action: ChatGroupsAction & { type: 'openTab' }) {
  const { payload } = action;

  // Dedupe: a sessionId already open ANYWHERE activates its tab and its group
  // rather than duplicating. Generalizes the artifactSourceKey dedupe in
  // ArtifactViewer, so the two surfaces cannot drift.
  if (payload.sessionId) {
    const hit = findTabGroup(state, (tab) => tab.sessionId === payload.sessionId);
    if (hit) {
      return {
        ...withGroup(state, hit.group.groupId, {
          ...hit.group,
          activeTabId: hit.tab.tabId,
        }),
        activeGroupId: hit.group.groupId,
      };
    }
  }

  const groupId = payload.groupId ?? state.activeGroupId;
  const group = state.groups[groupId];
  if (!group) return state;

  const nextTab = (tabId: ChatTabId): ChatTab => ({
    tabId,
    sessionId: payload.sessionId,
    title: payload.title ?? DEFAULT_TAB_TITLE,
    userSetName: payload.userSetName ?? false,
    pendingInitialMessage: payload.pendingInitialMessage,
    pendingInitialAttachments: payload.pendingInitialAttachments,
    workflowId: payload.workflowId,
    cwd: payload.cwd,
  });

  // ADOPT — the ONLY path that may fill an existing tab, and it is not an "open"
  // in the user's sense at all.
  //
  // An empty tab (sessionId '') is a tab the user opened and has not yet bound to
  // a session. When BaseChat's pre-session submit finally creates one and
  // navigates, that arrives here as an openTab — and it must fill the empty tab
  // in place, keeping its tabId, rather than opening a second tab beside it and
  // orphaning the blank one the user is staring at.
  //
  // Gated on the route-state cargo (`pendingInitial*`) because that cargo is what
  // makes this the submit path: only a submit carries the message that created
  // the session. A Recents click carries none, so it can never land here and can
  // never consume a blank tab — it always opens its own. That gate is the whole
  // reason this branch is safe to keep; without it "open in a new tab" would
  // silently become "replace the blank tab" and we would be back to the
  // behaviour the user rejected.
  const isPreSessionSubmit =
    payload.pendingInitialMessage !== undefined || payload.pendingInitialAttachments !== undefined;
  if (payload.sessionId && isPreSessionSubmit) {
    // The ACTIVE blank tab first, and only then any blank tab.
    //
    // "The blank tab" and "the leftmost blank tab" were the same tab back when
    // reaching two blanks took real effort. Cmd+T makes two blanks a keystroke
    // away, and then the leftmost is the wrong answer: the submit came from the
    // tab the user is typing in, so their first message would appear in a tab
    // they were not looking at while the one they typed in stayed empty.
    //
    // The fallback survives because not every pre-session submit originates in
    // the active tab — a launcher deep link carries its message in from outside
    // the strip entirely, and filling a waiting blank is the right home for it.
    const active = group.tabs.find((t) => t.tabId === group.activeTabId && !t.sessionId);
    const empty = active ?? group.tabs.find((t) => !t.sessionId);
    if (empty) {
      return {
        ...withGroup(state, groupId, {
          ...group,
          tabs: group.tabs.map((t) => (t.tabId === empty.tabId ? nextTab(empty.tabId) : t)),
          activeTabId: empty.tabId,
        }),
        activeGroupId: groupId,
      };
    }
  }

  const tabId = `tab-${state.seq + 1}`;
  return {
    ...withGroup(state, groupId, {
      ...group,
      tabs: [...group.tabs, nextTab(tabId)],
      activeTabId: tabId,
    }),
    activeGroupId: groupId,
    seq: state.seq + 1,
  };
}

/**
 * Drop a group that has just lost its last tab — but ONLY when it is not the
 * last group.
 *
 * The single-group case is a deliberate exception: closing the last tab of the
 * last group leaves an EMPTY group that renders BaseChat's own empty state. It
 * does not navigate away and it does not delete the session. That is the Stage-2
 * invariant and it must survive the split unchanged, which is why this is gated
 * on the group COUNT rather than on "is there a branch".
 */
function collapseEmptyGroup(state: ChatGroupsState, groupId: ChatGroupId): ChatGroupsState {
  const group = state.groups[groupId];
  if (!group || group.tabs.length > 0) return state;
  if (groupCountOf(state.layout) <= 1) return state;

  const layout = removeLeaf(state.layout, groupId);
  // removeLeaf returning null means we just removed the only leaf, which the
  // count guard above already excluded. Refuse rather than produce a null tree.
  if (!layout) return state;

  const groups = { ...state.groups };
  delete groups[groupId];

  // activeGroupId must ALWAYS name a live leaf. If the group that just died was
  // the active one, focus falls to the first surviving leaf.
  const activeGroupId = groups[state.activeGroupId] ? state.activeGroupId : firstLeaf(layout);
  return { ...state, layout, groups, activeGroupId };
}

function closeTab(state: ChatGroupsState, tabId: ChatTabId): ChatGroupsState {
  const hit = findTabGroup(state, (t) => t.tabId === tabId);
  if (!hit) return state;
  const { group } = hit;

  const closingIndex = group.tabs.findIndex((t) => t.tabId === tabId);
  const tabs = group.tabs.filter((t) => t.tabId !== tabId);

  // Successor = Math.min(closingIndex, remaining.length - 1) — identical to
  // ArtifactViewer's, so the two tab surfaces cannot drift.
  if (group.activeTabId !== tabId) {
    return collapseEmptyGroup(withGroup(state, group.groupId, { ...group, tabs }), group.groupId);
  }
  const successor = tabs[Math.min(closingIndex, tabs.length - 1)] ?? null;
  // Closing the last tab of the last group leaves an EMPTY group. It renders
  // BaseChat's existing empty state. It does NOT navigate away, and it does NOT
  // delete the session — there is no createdHere here, by construction.
  //
  // In a SPLIT, closing the last tab of a non-last group collapses that group
  // out of the tree instead: an empty half of a split is a dead pane the user
  // has to close twice.
  return collapseEmptyGroup(
    withGroup(state, group.groupId, { ...group, tabs, activeTabId: successor?.tabId ?? null }),
    group.groupId
  );
}

function moveTabToGroup(
  state: ChatGroupsState,
  action: ChatGroupsAction & { type: 'moveTabToGroup' }
): ChatGroupsState {
  const hit = findTabGroup(state, (t) => t.tabId === action.tabId);
  if (!hit) return state;
  const source = hit.group;
  const target = state.groups[action.targetGroupId];
  if (!target) return state;

  // Narrowed once, here, rather than testing `zone !== 'center'` at each use:
  // splitLeaf's parameter EXCLUDES 'center' (there is no such thing as a centre
  // split), and a boolean flag does not carry that proof to the call site.
  const splitZone = action.zone === 'center' ? null : action.zone;
  const isSplit = splitZone !== null;

  // Dropping a tab into the centre of its own group is a no-op, not a move — the
  // reorder gesture owns that case. And splitting a group off a tab that is that
  // group's ONLY tab would create a fresh group and leave an empty one behind,
  // i.e. a lot of motion to arrive back where you started.
  if (source.groupId === action.targetGroupId) {
    if (!isSplit) return state;
    if (source.tabs.length <= 1) return state;
  }

  if (isSplit && groupCountOf(state.layout) >= MAX_GROUPS) return state;

  const remaining = source.tabs.filter((t) => t.tabId !== action.tabId);
  const closingIndex = source.tabs.findIndex((t) => t.tabId === action.tabId);
  const sourceActiveTabId =
    source.activeTabId === action.tabId
      ? (remaining[Math.min(closingIndex, remaining.length - 1)]?.tabId ?? null)
      : source.activeTabId;

  let next: ChatGroupsState = {
    ...state,
    groups: {
      ...state.groups,
      [source.groupId]: { ...source, tabs: remaining, activeTabId: sourceActiveTabId },
    },
  };

  let landingGroupId = action.targetGroupId;

  if (splitZone) {
    landingGroupId = `grp-${state.seq + 1}`;
    next = {
      ...next,
      seq: state.seq + 1,
      layout: splitLeaf(next.layout, action.targetGroupId, landingGroupId, splitZone),
      groups: {
        ...next.groups,
        [landingGroupId]: { groupId: landingGroupId, tabs: [], activeTabId: null },
      },
    };
  }

  // Re-read the landing group from `next`: when the target IS the source (a
  // split off one's own group) the source's tab list has already been rewritten
  // above, and appending to the stale `target` would resurrect the moved tab.
  const landing = next.groups[landingGroupId];
  next = {
    ...next,
    groups: {
      ...next.groups,
      [landingGroupId]: {
        ...landing,
        tabs: [...landing.tabs, hit.tab],
        activeTabId: hit.tab.tabId,
      },
    },
    // The group you dropped into is the one you are now looking at.
    activeGroupId: landingGroupId,
  };

  return collapseEmptyGroup(next, source.groupId);
}

export function chatGroupsReducer(
  state: ChatGroupsState,
  action: ChatGroupsAction
): ChatGroupsState {
  switch (action.type) {
    case 'openTab':
      return openTab(state, action);

    case 'activateTab': {
      const hit = findTabGroup(state, (t) => t.tabId === action.tabId);
      if (!hit) return state;
      if (hit.group.activeTabId === action.tabId && state.activeGroupId === hit.group.groupId) {
        return state;
      }
      return {
        ...withGroup(state, hit.group.groupId, { ...hit.group, activeTabId: action.tabId }),
        activeGroupId: hit.group.groupId,
      };
    }

    case 'closeTab':
      return closeTab(state, action.tabId);

    case 'reorderTab': {
      const hit = findTabGroup(state, (t) => t.tabId === action.draggedTabId);
      if (!hit) return state;
      const { group } = hit;
      const draggedIndex = group.tabs.findIndex((t) => t.tabId === action.draggedTabId);
      const targetIndex = group.tabs.findIndex((t) => t.tabId === action.targetTabId);
      if (draggedIndex < 0 || targetIndex < 0 || draggedIndex === targetIndex) return state;
      const tabs = [...group.tabs];
      const [dragged] = tabs.splice(draggedIndex, 1);
      tabs.splice(targetIndex, 0, dragged);
      return withGroup(state, group.groupId, { ...group, tabs });
    }

    case 'renameTab': {
      // Mirrors a session rename into every tab bound to that session.
      let changed = false;
      const groups: Record<ChatGroupId, ChatGroup> = {};
      for (const [groupId, group] of Object.entries(state.groups)) {
        const tabs = group.tabs.map((t) => {
          if (t.sessionId !== action.sessionId || t.title === action.title) return t;
          changed = true;
          return { ...t, title: action.title, userSetName: action.userSetName ?? t.userSetName };
        });
        groups[groupId] = changed ? { ...group, tabs } : group;
      }
      return changed ? { ...state, groups } : state;
    }

    case 'bindSession': {
      const hit = findTabGroup(state, (t) => t.tabId === action.tabId);
      if (!hit) return state;
      return withGroup(state, hit.group.groupId, {
        ...hit.group,
        tabs: hit.group.tabs.map((t) =>
          t.tabId === action.tabId ? { ...t, sessionId: action.sessionId } : t
        ),
      });
    }

    case 'consumePending': {
      // Route-state cargo is consumed exactly once, by BaseChat on mount.
      const hit = findTabGroup(state, (t) => t.tabId === action.tabId);
      if (!hit) return state;
      if (!hit.tab.pendingInitialMessage && !hit.tab.pendingInitialAttachments) return state;
      return withGroup(state, hit.group.groupId, {
        ...hit.group,
        tabs: hit.group.tabs.map((t) =>
          t.tabId === action.tabId
            ? { ...t, pendingInitialMessage: undefined, pendingInitialAttachments: undefined }
            : t
        ),
      });
    }

    case 'setActiveGroup':
      // activeGroupId must ALWAYS name a live leaf.
      if (!state.groups[action.groupId]) return state;
      if (state.activeGroupId === action.groupId) return state;
      return { ...state, activeGroupId: action.groupId };

    case 'moveTabToGroup':
      return moveTabToGroup(state, action);

    case 'resizeBranch': {
      const layout = setSizesAtPath(state.layout, action.path, action.sizes);
      return layout === state.layout ? state : { ...state, layout };
    }

    default:
      return state;
  }
}

export function activeGroupOf(state: ChatGroupsState): ChatGroup | undefined {
  return state.groups[state.activeGroupId];
}

export function activeTabOf(state: ChatGroupsState): ChatTab | undefined {
  const group = activeGroupOf(state);
  if (!group || !group.activeTabId) return undefined;
  return group.tabs.find((t) => t.tabId === group.activeTabId);
}

/** The focused session id, or '' when the active group is empty. */
export function activeSessionIdOf(state: ChatGroupsState): string {
  return activeTabOf(state)?.sessionId ?? '';
}
