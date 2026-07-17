import { describe, it, expect } from 'vitest';
import {
  chatGroupsReducer,
  createInitialChatGroupsState,
  ChatGroupsAction,
  mergeAllGroups,
  restoreGroupLayout,
  snapshotGroupLayout,
} from './chatGroupsReducer';
import { ChatGroupsState, leafGroupIds } from './chatGroupsTypes';

/**
 * Rung 4 of the yield ladder (D-32) — the state half.
 *
 * "a split merges back to one group rather than render two useless slivers."
 *
 * WHEN it fires is a width rule and lives in Layout/yieldLadder.ts (crossing-
 * gated, so it can never fight a split the user made by hand). WHAT it does to
 * the tabs is a state transition, and that is what these prove. The merge is the
 * easy half; the RESTORE is where the bugs are, because by the time it runs the
 * user has kept working and the tab set has moved on underneath it.
 */

function run(state: ChatGroupsState, ...actions: ChatGroupsAction[]): ChatGroupsState {
  return actions.reduce(chatGroupsReducer, state);
}

const open = (sessionId: string): ChatGroupsAction => ({
  type: 'openTab',
  payload: { sessionId, title: sessionId },
});

// Tabs and groups share one `seq`, so the ids are not 1,2,3 — they are whatever
// the reducer minted. Named here rather than hardcoded at every assertion.
const LEFT = 'grp-1'; // holds s1
const RIGHT = 'grp-4'; // the group the split created; holds s2, and has focus
const TAB_S1 = 'tab-2';
const TAB_S2 = 'tab-3';

/** LEFT: [s1] | RIGHT: [s2] — a 2-up row split, focus on RIGHT. */
function twoUp(): ChatGroupsState {
  const state = run(createInitialChatGroupsState(), open('s1'), open('s2'));
  return run(state, {
    type: 'moveTabToGroup',
    tabId: TAB_S2,
    targetGroupId: LEFT,
    zone: 'right',
  });
}

describe('mergeAllGroups', () => {
  it('collapses the tree to one leaf and gathers every tab', () => {
    const merged = mergeAllGroups(twoUp());
    expect(merged.layout).toEqual({ kind: 'leaf', groupId: RIGHT });
    expect(leafGroupIds(merged.layout)).toEqual([RIGHT]);
    expect(Object.keys(merged.groups)).toEqual([RIGHT]);
    expect(merged.groups[RIGHT].tabs.map((t) => t.sessionId)).toEqual(['s1', 's2']);
  });

  it('keeps you on the chat you were reading — the window narrowed, it did not change the subject', () => {
    const before = twoUp();
    expect(before.activeGroupId).toBe(RIGHT);
    const merged = mergeAllGroups(before);
    expect(merged.activeGroupId).toBe(RIGHT);
    expect(merged.groups[RIGHT].activeTabId).toBe(TAB_S2);
  });

  it('orders the merged strip the way the layout READ, left to right', () => {
    // RIGHT is FOCUSED but sits on the right. Its tab must land second, or the
    // strip would silently reorder itself around whichever pane you happened to
    // click last.
    const merged = mergeAllGroups(twoUp());
    expect(merged.groups[RIGHT].tabs.map((t) => t.tabId)).toEqual([TAB_S1, TAB_S2]);
  });

  it('walks the TREE for that order, not the groups object', () => {
    // The above passes even if you iterate Object.values(state.groups), because
    // in a `right` split the insertion order and the leaf order agree — a
    // mutation test proved that test was decorative. A `left` split is where
    // they disagree: the new group is inserted into the object LAST but renders
    // FIRST. Iterating the object would put the left-hand pane's chat second and
    // silently mirror the user's strip.
    const state = run(createInitialChatGroupsState(), open('s1'), open('s2'));
    const leftSplit = run(state, {
      type: 'moveTabToGroup',
      tabId: TAB_S2,
      targetGroupId: LEFT,
      zone: 'left',
    });
    expect(leafGroupIds(leftSplit.layout)).toEqual([RIGHT, LEFT]); // s2's group renders FIRST
    expect(Object.keys(leftSplit.groups)).toEqual([LEFT, RIGHT]); // …but was inserted LAST

    const merged = mergeAllGroups(leftSplit);
    expect(merged.groups[merged.activeGroupId].tabs.map((t) => t.sessionId)).toEqual(['s2', 's1']);
  });

  it('is a no-op on a single group — there is nothing to merge', () => {
    const state = run(createInitialChatGroupsState(), open('s1'));
    expect(mergeAllGroups(state)).toBe(state);
  });

  it('flattens a 4-way split in one step', () => {
    let state = twoUp();
    state = run(
      state,
      open('s3'),
      { type: 'moveTabToGroup', tabId: 'tab-5', targetGroupId: RIGHT, zone: 'bottom' },
      open('s4'),
      { type: 'moveTabToGroup', tabId: 'tab-7', targetGroupId: LEFT, zone: 'bottom' }
    );
    expect(leafGroupIds(state.layout)).toHaveLength(4);
    const merged = mergeAllGroups(state);
    expect(leafGroupIds(merged.layout)).toHaveLength(1);
    expect(merged.groups[merged.activeGroupId].tabs).toHaveLength(4);
  });

  it('runs through the reducer, not only as a bare function', () => {
    const merged = run(twoUp(), { type: 'mergeAllGroups' });
    expect(leafGroupIds(merged.layout)).toEqual([RIGHT]);
  });
});

describe('restoreGroupLayout', () => {
  it('gives back exactly the layout the user built', () => {
    const before = twoUp();
    const snapshot = snapshotGroupLayout(before);
    const restored = restoreGroupLayout(mergeAllGroups(before), snapshot);

    expect(restored.layout).toEqual(before.layout);
    expect(leafGroupIds(restored.layout)).toEqual([LEFT, RIGHT]);
    expect(restored.groups[LEFT].tabs.map((t) => t.tabId)).toEqual([TAB_S1]);
    expect(restored.groups[RIGHT].tabs.map((t) => t.tabId)).toEqual([TAB_S2]);
    expect(restored.activeGroupId).toBe(RIGHT);
    expect(restored.groups[RIGHT].activeTabId).toBe(TAB_S2);
  });

  it('round-trips through the reducer', () => {
    const before = twoUp();
    const snapshot = snapshotGroupLayout(before);
    const restored = run(before, { type: 'mergeAllGroups' }, { type: 'restoreLayout', snapshot });
    expect(restored.layout).toEqual(before.layout);
  });

  it('drops a tab the user closed while merged instead of resurrecting it', () => {
    // The snapshot names s1's tab. The user closes it. Growing back must
    // not bring the chat back from the dead — which is exactly what storing whole
    // ChatTab objects in the snapshot would have done.
    const before = twoUp();
    const snapshot = snapshotGroupLayout(before);
    const merged = run(before, { type: 'mergeAllGroups' }, { type: 'closeTab', tabId: TAB_S1 });

    const restored = restoreGroupLayout(merged, snapshot);
    const allTabs = Object.values(restored.groups).flatMap((g) => g.tabs);
    expect(allTabs.map((t) => t.tabId)).toEqual([TAB_S2]);
  });

  it('collapses a leaf that lost every tab rather than restoring an empty pane', () => {
    const before = twoUp();
    const snapshot = snapshotGroupLayout(before);
    const merged = run(before, { type: 'mergeAllGroups' }, { type: 'closeTab', tabId: TAB_S1 });

    const restored = restoreGroupLayout(merged, snapshot);
    // LEFT held only s1's tab, so the split has nothing left to be: one group.
    expect(restored.layout).toEqual({ kind: 'leaf', groupId: RIGHT });
    expect(restored.groups[LEFT]).toBeUndefined();
  });

  it('keeps a chat opened WHILE merged in the group the user is actually in', () => {
    // A new chat must not be teleported into a background pane the moment the
    // window widens. The user opened it where they were; that is where it stays.
    const before = twoUp();
    const snapshot = snapshotGroupLayout(before);
    const merged = run(before, { type: 'mergeAllGroups' }, open('s3'));

    const restored = restoreGroupLayout(merged, snapshot);
    expect(restored.groups[RIGHT].tabs.map((t) => t.sessionId)).toEqual(['s2', 's3']);
    expect(restored.groups[LEFT].tabs.map((t) => t.sessionId)).toEqual(['s1']);
  });

  it('follows the user: focus lands wherever the tab they are on ended up', () => {
    // While merged the user switched to the chat that came from the OTHER pane.
    // Restoring must move focus to that pane, not dump them back where the
    // snapshot's focus happened to be.
    const before = twoUp();
    const snapshot = snapshotGroupLayout(before);
    const merged = run(before, { type: 'mergeAllGroups' }, { type: 'activateTab', tabId: TAB_S1 });

    const restored = restoreGroupLayout(merged, snapshot);
    expect(restored.activeGroupId).toBe(LEFT);
    expect(restored.groups[LEFT].activeTabId).toBe(TAB_S1);
  });

  it('every tab that existed before the restore still exists after it', () => {
    // The invariant that matters most: rung 4 is a LAYOUT rule and must never be
    // able to lose a chat.
    const before = twoUp();
    const snapshot = snapshotGroupLayout(before);
    const merged = run(before, { type: 'mergeAllGroups' }, open('s3'), open('s4'));

    const restored = restoreGroupLayout(merged, snapshot);
    const ids = (s: ChatGroupsState) =>
      Object.values(s.groups)
        .flatMap((g) => g.tabs.map((t) => t.tabId))
        .sort();
    expect(ids(restored)).toEqual(ids(merged));
  });

  it('leaves activeGroupId naming a live leaf, whatever happened while merged', () => {
    const before = twoUp();
    const snapshot = snapshotGroupLayout(before);
    const merged = run(before, { type: 'mergeAllGroups' }, { type: 'closeTab', tabId: TAB_S1 });
    const restored = restoreGroupLayout(merged, snapshot);
    expect(leafGroupIds(restored.layout)).toContain(restored.activeGroupId);
    expect(restored.groups[restored.activeGroupId]).toBeDefined();
  });

  it('refuses to restore into nothing rather than produce a null tree', () => {
    const before = twoUp();
    const snapshot = snapshotGroupLayout(before);
    let merged = run(before, { type: 'mergeAllGroups' });
    merged = run(merged, { type: 'closeTab', tabId: TAB_S1 }, { type: 'closeTab', tabId: TAB_S2 });
    // Every tab is gone; the last group is the deliberate empty-state exception.
    const restored = restoreGroupLayout(merged, snapshot);
    expect(leafGroupIds(restored.layout).length).toBeGreaterThanOrEqual(1);
    expect(restored.groups[restored.activeGroupId]).toBeDefined();
  });
});
