import { describe, it, expect } from 'vitest';
import {
  chatGroupsReducer,
  createInitialChatGroupsState,
  ChatGroupsAction,
} from './chatGroupsReducer';
import { ChatGroupsState, GroupLayout, firstLeaf, leafGroupIds } from './chatGroupsTypes';
import { MAX_GROUPS, normalizeSizes, removeLeaf, splitLeaf, setSizesAtPath } from './chatGroupsLayout';

function run(state: ChatGroupsState, ...actions: ChatGroupsAction[]): ChatGroupsState {
  return actions.reduce(chatGroupsReducer, state);
}

const open = (sessionId: string): ChatGroupsAction => ({
  type: 'openTab',
  payload: { sessionId, title: sessionId },
});

/** Two tabs in one group — the starting point for every split below. */
function twoTabs(): ChatGroupsState {
  return run(createInitialChatGroupsState(), open('s1'), open('s2'));
}

const branchOf = (state: ChatGroupsState) => state.layout as Extract<GroupLayout, { kind: 'branch' }>;

describe('splitLeaf / removeLeaf / normalizeSizes', () => {
  it('splitLeaf replaces the leaf with a branch and puts the new group on the named side', () => {
    const leaf: GroupLayout = { kind: 'leaf', groupId: 'a' };

    const right = splitLeaf(leaf, 'a', 'b', 'right');
    expect(right).toEqual({
      kind: 'branch',
      dir: 'row',
      children: [{ kind: 'leaf', groupId: 'a' }, { kind: 'leaf', groupId: 'b' }],
      sizes: [0.5, 0.5],
    });

    // `left` must put the NEW group first — otherwise "split left" silently
    // splits right and the layout is the mirror of what the user aimed at.
    const left = splitLeaf(leaf, 'a', 'b', 'left');
    expect(leafGroupIds(left)).toEqual(['b', 'a']);

    expect(splitLeaf(leaf, 'a', 'b', 'top')).toMatchObject({ dir: 'col' });
    expect(leafGroupIds(splitLeaf(leaf, 'a', 'b', 'top'))).toEqual(['b', 'a']);
    expect(splitLeaf(leaf, 'a', 'b', 'bottom')).toMatchObject({ dir: 'col' });
    expect(leafGroupIds(splitLeaf(leaf, 'a', 'b', 'bottom'))).toEqual(['a', 'b']);
  });

  it('splitLeaf finds a leaf nested deep in the tree', () => {
    const tree: GroupLayout = {
      kind: 'branch',
      dir: 'row',
      children: [
        { kind: 'leaf', groupId: 'a' },
        {
          kind: 'branch',
          dir: 'col',
          children: [{ kind: 'leaf', groupId: 'b' }, { kind: 'leaf', groupId: 'c' }],
          sizes: [0.5, 0.5],
        },
      ],
      sizes: [0.5, 0.5],
    };
    expect(leafGroupIds(splitLeaf(tree, 'c', 'd', 'right'))).toEqual(['a', 'b', 'c', 'd']);
  });

  it('removeLeaf COLLAPSES a branch left with one child into that child', () => {
    const tree = splitLeaf({ kind: 'leaf', groupId: 'a' }, 'a', 'b', 'right');
    // Without the collapse this would be branch[leaf a] — a splitter with
    // nothing on one side, and a level of tree that never goes away.
    expect(removeLeaf(tree, 'b')).toEqual({ kind: 'leaf', groupId: 'a' });
    expect(removeLeaf(tree, 'a')).toEqual({ kind: 'leaf', groupId: 'b' });
  });

  it('removeLeaf renormalizes the surviving siblings to sum to 1', () => {
    const tree: GroupLayout = {
      kind: 'branch',
      dir: 'row',
      children: [
        { kind: 'leaf', groupId: 'a' },
        { kind: 'leaf', groupId: 'b' },
        { kind: 'leaf', groupId: 'c' },
      ],
      sizes: [0.2, 0.3, 0.5],
    };
    const next = removeLeaf(tree, 'b') as Extract<GroupLayout, { kind: 'branch' }>;
    expect(leafGroupIds(next)).toEqual(['a', 'c']);
    // 0.2 : 0.5 preserved as a RATIO, renormalized to sum to 1. Leaving the raw
    // [0.2, 0.5] would leave 30% of the branch unclaimed.
    expect(next.sizes[0] + next.sizes[1]).toBeCloseTo(1);
    expect(next.sizes[0]).toBeCloseTo(0.2 / 0.7);
  });

  it('removeLeaf returns null when the last leaf goes', () => {
    expect(removeLeaf({ kind: 'leaf', groupId: 'a' }, 'a')).toBeNull();
  });

  it('normalizeSizes falls back to an even split on unusable input', () => {
    expect(normalizeSizes([1, 3], 2)).toEqual([0.25, 0.75]);
    // Wrong length, zeroes, negatives and NaN all mean "this array is not a
    // description of this branch" — an even split is the only safe reading.
    expect(normalizeSizes([1], 2)).toEqual([0.5, 0.5]);
    expect(normalizeSizes([0, 0], 2)).toEqual([0.5, 0.5]);
    expect(normalizeSizes([-1, 2], 2)).toEqual([0.5, 0.5]);
    expect(normalizeSizes([NaN, 1], 2)).toEqual([0.5, 0.5]);
  });

  it('setSizesAtPath addresses a branch by PATH, not by a flat index', () => {
    const tree: GroupLayout = {
      kind: 'branch',
      dir: 'row',
      children: [
        { kind: 'leaf', groupId: 'a' },
        {
          kind: 'branch',
          dir: 'col',
          children: [{ kind: 'leaf', groupId: 'b' }, { kind: 'leaf', groupId: 'c' }],
          sizes: [0.5, 0.5],
        },
      ],
      sizes: [0.5, 0.5],
    };
    const next = setSizesAtPath(tree, [1], [0.8, 0.2]) as Extract<GroupLayout, { kind: 'branch' }>;
    // The NESTED branch resized; the root untouched.
    expect((next.children[1] as Extract<GroupLayout, { kind: 'branch' }>).sizes).toEqual([0.8, 0.2]);
    expect(next.sizes).toEqual([0.5, 0.5]);
  });
});

describe('chatGroupsReducer — moveTabToGroup: split', () => {
  it('an EDGE drop creates a branch, a new group, and normalized sizes', () => {
    const before = twoTabs();
    const tabId = before.groups['grp-1'].tabs[0].tabId;

    const after = chatGroupsReducer(before, {
      type: 'moveTabToGroup',
      tabId,
      targetGroupId: 'grp-1',
      zone: 'right',
    });

    expect(after.layout.kind).toBe('branch');
    const branch = branchOf(after);
    expect(branch.dir).toBe('row');
    expect(branch.sizes).toEqual([0.5, 0.5]);
    expect(branch.sizes.reduce((a, b) => a + b, 0)).toBeCloseTo(1);

    const [left, right] = leafGroupIds(after.layout);
    expect(left).toBe('grp-1');
    // The moved tab is alone in the new group, active, and GONE from the source.
    expect(after.groups[right].tabs.map((t) => t.tabId)).toEqual([tabId]);
    expect(after.groups[right].activeTabId).toBe(tabId);
    expect(after.groups['grp-1'].tabs.map((t) => t.tabId)).not.toContain(tabId);
    // You are looking at the group you dropped into.
    expect(after.activeGroupId).toBe(right);
  });

  it('a `col` split builds a column branch', () => {
    const before = twoTabs();
    const tabId = before.groups['grp-1'].tabs[0].tabId;
    const after = chatGroupsReducer(before, {
      type: 'moveTabToGroup',
      tabId,
      targetGroupId: 'grp-1',
      zone: 'bottom',
    });
    expect(branchOf(after).dir).toBe('col');
  });

  it('refuses to split a group off its own ONLY tab', () => {
    // Otherwise: create a group, move the tab, and collapse the now-empty source
    // — a lot of motion to arrive exactly where you started.
    const before = run(createInitialChatGroupsState(), open('s1'));
    const tabId = before.groups['grp-1'].tabs[0].tabId;
    const after = chatGroupsReducer(before, {
      type: 'moveTabToGroup',
      tabId,
      targetGroupId: 'grp-1',
      zone: 'right',
    });
    expect(after).toBe(before);
  });

  it('a centre drop on the tab own group is a no-op (reorder owns that gesture)', () => {
    const before = twoTabs();
    const tabId = before.groups['grp-1'].tabs[0].tabId;
    expect(
      chatGroupsReducer(before, {
        type: 'moveTabToGroup',
        tabId,
        targetGroupId: 'grp-1',
        zone: 'center',
      })
    ).toBe(before);
  });

  it('caps the split at MAX_GROUPS', () => {
    let state = run(createInitialChatGroupsState(), open('s1'), open('s2'), open('s3'), open('s4'), open('s5'));
    // Split until the cap.
    for (let i = 0; i < MAX_GROUPS - 1; i++) {
      const target = state.activeGroupId;
      const source = Object.values(state.groups).find((g) => g.tabs.length > 1);
      expect(source).toBeDefined();
      state = chatGroupsReducer(state, {
        type: 'moveTabToGroup',
        tabId: source!.tabs[0].tabId,
        targetGroupId: source!.groupId,
        zone: 'right',
      });
      expect(target).toBeDefined();
    }
    expect(leafGroupIds(state.layout)).toHaveLength(MAX_GROUPS);

    const crowded = Object.values(state.groups).find((g) => g.tabs.length > 1);
    expect(crowded).toBeDefined();
    const refused = chatGroupsReducer(state, {
      type: 'moveTabToGroup',
      tabId: crowded!.tabs[0].tabId,
      targetGroupId: crowded!.groupId,
      zone: 'right',
    });
    expect(refused).toBe(state);
    expect(leafGroupIds(refused.layout)).toHaveLength(MAX_GROUPS);
  });
});

describe('the titlebar reserve follows the TREE, not the groups record (plan R6 / card Pi)', () => {
  /**
   * The one shape where tree order and record order DISAGREE.
   *
   * A `left` or `top` split puts the NEW group first in the tree, but the groups
   * record still has grp-1 first — records are keyed by insertion. So
   * `Object.keys(state.groups)[0]` says grp-1 and firstLeaf() says grp-2, and
   * only firstLeaf is right: after a left split the new group is the one against
   * the traffic lights.
   *
   * This is the assertion with teeth. A `right` split has both answers agreeing,
   * so it proves nothing — and neither does any test built on one, which is
   * exactly how this reserve failed silently once already.
   */
  it('a LEFT split: firstLeaf names the NEW group, which is LAST in the groups record', () => {
    const before = twoTabs();
    const after = chatGroupsReducer(before, {
      type: 'moveTabToGroup',
      tabId: before.groups['grp-1'].tabs[0].tabId,
      targetGroupId: 'grp-1',
      zone: 'left',
    });

    const newGroupId = leafGroupIds(after.layout)[0];
    expect(newGroupId).not.toBe('grp-1');
    expect(firstLeaf(after.layout)).toBe(newGroupId);
    // Record order disagrees — the trap.
    expect(Object.keys(after.groups)[0]).toBe('grp-1');
    expect(firstLeaf(after.layout)).not.toBe(Object.keys(after.groups)[0]);
  });

  it('a TOP split: same disagreement, and the reserve must follow the tree', () => {
    // The column case the plan calls out by name: both groups sit at x=0, only
    // the TOP one collides with the traffic lights.
    const before = twoTabs();
    const after = chatGroupsReducer(before, {
      type: 'moveTabToGroup',
      tabId: before.groups['grp-1'].tabs[0].tabId,
      targetGroupId: 'grp-1',
      zone: 'top',
    });
    expect(branchOf(after).dir).toBe('col');
    const topGroupId = leafGroupIds(after.layout)[0];
    expect(firstLeaf(after.layout)).toBe(topGroupId);
    expect(topGroupId).not.toBe(Object.keys(after.groups)[0]);
  });
});

describe('chatGroupsReducer — moveTabToGroup: move + collapse', () => {
  /** grp-1[s1] | grp-2[s2] — a real 2-group split. */
  function split(): ChatGroupsState {
    const before = twoTabs();
    return chatGroupsReducer(before, {
      type: 'moveTabToGroup',
      tabId: before.groups['grp-1'].tabs[1].tabId,
      targetGroupId: 'grp-1',
      zone: 'right',
    });
  }

  it('a CENTRE drop on another group moves the tab and collapses the emptied source', () => {
    const state = split();
    const [sourceId, targetId] = leafGroupIds(state.layout);
    const tabId = state.groups[sourceId].tabs[0].tabId;

    const after = chatGroupsReducer(state, {
      type: 'moveTabToGroup',
      tabId,
      targetGroupId: targetId,
      zone: 'center',
    });

    // The source lost its last tab, so the split is gone and the tree collapsed
    // back to a single leaf. An empty half of a split is a dead pane.
    expect(after.layout).toEqual({ kind: 'leaf', groupId: targetId });
    expect(after.groups[sourceId]).toBeUndefined();
    expect(after.groups[targetId].tabs).toHaveLength(2);
    expect(after.activeGroupId).toBe(targetId);
  });

  it('closing the last tab of a non-last group collapses that group out of the tree', () => {
    const state = split();
    const [sourceId, targetId] = leafGroupIds(state.layout);
    const tabId = state.groups[sourceId].tabs[0].tabId;

    const after = chatGroupsReducer(state, { type: 'closeTab', tabId });

    expect(after.layout).toEqual({ kind: 'leaf', groupId: targetId });
    expect(after.groups[sourceId]).toBeUndefined();
    expect(after.activeGroupId).toBe(targetId);
  });

  it('closing the last tab of the LAST group still leaves an empty group', () => {
    // The Stage-2 invariant, unchanged by the split: it renders BaseChat's empty
    // state, does not navigate away, and does not delete the session.
    const before = run(createInitialChatGroupsState(), open('s1'));
    const after = chatGroupsReducer(before, {
      type: 'closeTab',
      tabId: before.groups['grp-1'].tabs[0].tabId,
    });
    expect(after.layout).toEqual({ kind: 'leaf', groupId: 'grp-1' });
    expect(after.groups['grp-1'].tabs).toEqual([]);
    expect(after.groups['grp-1'].activeTabId).toBeNull();
    expect(after.activeGroupId).toBe('grp-1');
  });

  it('activeGroupId always names a live leaf after a collapse', () => {
    const state = split();
    const [sourceId, targetId] = leafGroupIds(state.layout);
    // Focus the group that is about to die.
    const focused = chatGroupsReducer(state, { type: 'setActiveGroup', groupId: sourceId });
    expect(focused.activeGroupId).toBe(sourceId);

    const after = chatGroupsReducer(focused, {
      type: 'closeTab',
      tabId: focused.groups[sourceId].tabs[0].tabId,
    });
    expect(after.activeGroupId).toBe(targetId);
    expect(leafGroupIds(after.layout)).toContain(after.activeGroupId);
    expect(after.groups[after.activeGroupId]).toBeDefined();
    expect(firstLeaf(after.layout)).toBe(targetId);
  });

  it('moving a tab OUT of a multi-tab group leaves the group alive with a successor', () => {
    const state = run(twoTabs(), open('s3'));
    const moved = state.groups['grp-1'].tabs[2].tabId;
    const split1 = chatGroupsReducer(state, {
      type: 'moveTabToGroup',
      tabId: state.groups['grp-1'].tabs[0].tabId,
      targetGroupId: 'grp-1',
      zone: 'right',
    });
    const [sourceId, targetId] = leafGroupIds(split1.layout);

    const after = chatGroupsReducer(split1, {
      type: 'moveTabToGroup',
      tabId: moved,
      targetGroupId: targetId,
      zone: 'center',
    });

    // Source keeps its remaining tab => no collapse.
    expect(after.groups[sourceId].tabs).toHaveLength(1);
    expect(after.layout.kind).toBe('branch');
    expect(after.groups[targetId].tabs.map((t) => t.tabId)).toContain(moved);
  });

  it('resizeBranch normalizes and addresses the root by an empty path', () => {
    const state = split();
    const after = chatGroupsReducer(state, { type: 'resizeBranch', path: [], sizes: [3, 1] });
    expect(branchOf(after).sizes).toEqual([0.75, 0.25]);
  });

  it('resizeBranch ignores a size array that disagrees with the child count', () => {
    const state = split();
    expect(chatGroupsReducer(state, { type: 'resizeBranch', path: [], sizes: [1] })).toBe(state);
  });
});
