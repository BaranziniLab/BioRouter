import { describe, it, expect } from 'vitest';
import {
  chatGroupsReducer,
  createInitialChatGroupsState,
  activeTabOf,
  activeSessionIdOf,
  ChatGroupsAction,
} from './chatGroupsReducer';
import { ChatGroupsState, firstLeaf, leafGroupIds } from './chatGroupsTypes';

function run(state: ChatGroupsState, ...actions: ChatGroupsAction[]): ChatGroupsState {
  return actions.reduce(chatGroupsReducer, state);
}

const open = (sessionId: string, preview = false): ChatGroupsAction => ({
  type: 'openTab',
  payload: { sessionId, preview, title: sessionId },
});

describe('chatGroupsReducer — preview tabs (VS Code enablePreview)', () => {
  it('reuses the group preview tab IN PLACE, keeping the same tabId', () => {
    const a = run(createInitialChatGroupsState(), open('s1', true));
    const firstTabId = a.groups['grp-1'].tabs[0].tabId;

    const b = chatGroupsReducer(a, open('s2', true));

    // The load-bearing invariant: one tab, same identity, new session.
    expect(b.groups['grp-1'].tabs).toHaveLength(1);
    expect(b.groups['grp-1'].tabs[0].tabId).toBe(firstTabId);
    expect(b.groups['grp-1'].tabs[0].sessionId).toBe('s2');
    expect(b.groups['grp-1'].activeTabId).toBe(firstTabId);
  });

  it('leaves twelve Recents clicks as exactly one tab', () => {
    let state = createInitialChatGroupsState();
    for (let i = 0; i < 12; i++) state = chatGroupsReducer(state, open(`s${i}`, true));
    expect(state.groups['grp-1'].tabs).toHaveLength(1);
    expect(state.groups['grp-1'].tabs[0].sessionId).toBe('s11');
  });

  it('a pinned tab is never recycled by the next preview open', () => {
    const state = run(
      createInitialChatGroupsState(),
      open('s1', false), // pinned
      open('s2', true) // preview
    );
    expect(state.groups['grp-1'].tabs.map((t) => t.sessionId)).toEqual(['s1', 's2']);
  });

  it('pinTab makes the tab survive the next preview open', () => {
    const a = run(createInitialChatGroupsState(), open('s1', true));
    const tabId = a.groups['grp-1'].tabs[0].tabId;
    const b = run(a, { type: 'pinTab', tabId }, open('s2', true));

    expect(b.groups['grp-1'].tabs).toHaveLength(2);
    expect(b.groups['grp-1'].tabs[0].preview).toBe(false);
  });

  it('PIN-ON-RUN: a preview tab whose session is running is pinned, not replaced', () => {
    const a = run(createInitialChatGroupsState(), open('s1', true));
    const b = chatGroupsReducer(a, {
      type: 'openTab',
      payload: { sessionId: 's2', preview: true },
      runningSessionIds: ['s1'],
    });

    // A live turn is committed-to: s1 survives, pinned, and s2 gets a new tab.
    expect(b.groups['grp-1'].tabs).toHaveLength(2);
    expect(b.groups['grp-1'].tabs[0].sessionId).toBe('s1');
    expect(b.groups['grp-1'].tabs[0].preview).toBe(false);
    expect(b.groups['grp-1'].tabs[1].sessionId).toBe('s2');
  });

  it('re-opening a preview tab non-preview pins it in place (the submit path)', () => {
    const a = run(createInitialChatGroupsState(), open('s1', true));
    const tabId = a.groups['grp-1'].tabs[0].tabId;
    const b = chatGroupsReducer(a, open('s1', false));

    expect(b.groups['grp-1'].tabs).toHaveLength(1);
    expect(b.groups['grp-1'].tabs[0].tabId).toBe(tabId);
    expect(b.groups['grp-1'].tabs[0].preview).toBe(false);
  });
});

describe('chatGroupsReducer — dedupe', () => {
  it('opening an already-open sessionId activates rather than duplicates', () => {
    const a = run(createInitialChatGroupsState(), open('s1'), open('s2'));
    const s1Tab = a.groups['grp-1'].tabs[0].tabId;

    const b = chatGroupsReducer(a, open('s1'));
    expect(b.groups['grp-1'].tabs).toHaveLength(2);
    expect(b.groups['grp-1'].activeTabId).toBe(s1Tab);
  });
});

describe('chatGroupsReducer — close with successor', () => {
  it('successor is Math.min(closingIndex, remaining.length - 1)', () => {
    const a = run(createInitialChatGroupsState(), open('a'), open('b'), open('c'));
    const [, tabB] = a.groups['grp-1'].tabs;

    // Close the MIDDLE active tab -> index 1 of the remaining [a, c] is 'c'.
    const b = run(a, { type: 'activateTab', tabId: tabB.tabId }, { type: 'closeTab', tabId: tabB.tabId });
    expect(activeSessionIdOf(b)).toBe('c');
  });

  it('closing the LAST tab in the strip falls back to its left neighbour', () => {
    const a = run(createInitialChatGroupsState(), open('a'), open('b'), open('c'));
    const tabC = a.groups['grp-1'].tabs[2];
    const b = chatGroupsReducer(a, { type: 'closeTab', tabId: tabC.tabId });
    expect(activeSessionIdOf(b)).toBe('b');
  });

  it('closing an INACTIVE tab does not move focus', () => {
    const a = run(createInitialChatGroupsState(), open('a'), open('b'));
    const tabA = a.groups['grp-1'].tabs[0];
    const b = chatGroupsReducer(a, { type: 'closeTab', tabId: tabA.tabId });
    expect(activeSessionIdOf(b)).toBe('b');
  });

  it('closing the last tab of the last group leaves an EMPTY group, not a navigation', () => {
    const a = run(createInitialChatGroupsState(), open('a'));
    const b = chatGroupsReducer(a, { type: 'closeTab', tabId: a.groups['grp-1'].tabs[0].tabId });

    expect(b.groups['grp-1'].tabs).toEqual([]);
    expect(b.groups['grp-1'].activeTabId).toBeNull();
    expect(b.activeGroupId).toBe('grp-1');
    expect(activeTabOf(b)).toBeUndefined();
    expect(activeSessionIdOf(b)).toBe('');
  });
});

describe('chatGroupsReducer — reorder', () => {
  it('splices, it does not swap', () => {
    const a = run(createInitialChatGroupsState(), open('a'), open('b'), open('c'));
    const [tabA, , tabC] = a.groups['grp-1'].tabs;
    const b = chatGroupsReducer(a, {
      type: 'reorderTab',
      draggedTabId: tabA.tabId,
      targetTabId: tabC.tabId,
    });
    // Splice-move gives [b, c, a]. A swap would give [c, b, a].
    expect(b.groups['grp-1'].tabs.map((t) => t.sessionId)).toEqual(['b', 'c', 'a']);
  });

  it('same-index and unknown ids are no-ops (identity preserved)', () => {
    const a = run(createInitialChatGroupsState(), open('a'), open('b'));
    const [tabA] = a.groups['grp-1'].tabs;
    expect(
      chatGroupsReducer(a, { type: 'reorderTab', draggedTabId: tabA.tabId, targetTabId: tabA.tabId })
    ).toBe(a);
    expect(
      chatGroupsReducer(a, { type: 'reorderTab', draggedTabId: 'nope', targetTabId: tabA.tabId })
    ).toBe(a);
  });
});

describe('chatGroupsReducer — rename mirroring', () => {
  it('mirrors a session rename into the tab title', () => {
    const a = run(createInitialChatGroupsState(), open('s1'), open('s2'));
    const b = chatGroupsReducer(a, {
      type: 'renameTab',
      sessionId: 's2',
      title: 'Cohort query',
      userSetName: true,
    });
    expect(b.groups['grp-1'].tabs[1].title).toBe('Cohort query');
    expect(b.groups['grp-1'].tabs[1].userSetName).toBe(true);
    expect(b.groups['grp-1'].tabs[0].title).toBe('s1');
  });

  it('an unchanged title returns the SAME state object (no render churn)', () => {
    const a = run(createInitialChatGroupsState(), open('s1'));
    expect(chatGroupsReducer(a, { type: 'renameTab', sessionId: 's1', title: 's1' })).toBe(a);
    expect(chatGroupsReducer(a, { type: 'renameTab', sessionId: 'ghost', title: 'x' })).toBe(a);
  });
});

describe('chatGroupsReducer — invariants', () => {
  it('deterministic ids come from seq', () => {
    const a = run(createInitialChatGroupsState(), open('a'), open('b'));
    expect(a.groups['grp-1'].tabs.map((t) => t.tabId)).toEqual(['tab-2', 'tab-3']);
    expect(a.seq).toBe(3);
  });

  it('activeGroupId always names a live leaf', () => {
    let state = createInitialChatGroupsState();
    state = run(state, open('a'), open('b'), { type: 'setActiveGroup', groupId: 'ghost' });
    expect(leafGroupIds(state.layout)).toContain(state.activeGroupId);
    expect(state.groups[state.activeGroupId]).toBeDefined();
  });

  it('consumePending clears route cargo exactly once', () => {
    const a = chatGroupsReducer(createInitialChatGroupsState(), {
      type: 'openTab',
      payload: { sessionId: 's1', pendingInitialMessage: 'hello' },
    });
    const tabId = a.groups['grp-1'].tabs[0].tabId;
    const b = chatGroupsReducer(a, { type: 'consumePending', tabId });
    expect(b.groups['grp-1'].tabs[0].pendingInitialMessage).toBeUndefined();
    // Second consume is a no-op and must not churn identity.
    expect(chatGroupsReducer(b, { type: 'consumePending', tabId })).toBe(b);
  });

  it('bindSession keeps the tabId stable across the session bind', () => {
    const a = chatGroupsReducer(createInitialChatGroupsState(), {
      type: 'openTab',
      payload: { sessionId: '' },
    });
    const tabId = a.groups['grp-1'].tabs[0].tabId;
    const b = chatGroupsReducer(a, { type: 'bindSession', tabId, sessionId: 'real' });
    expect(b.groups['grp-1'].tabs[0].tabId).toBe(tabId);
    expect(b.groups['grp-1'].tabs[0].sessionId).toBe('real');
  });
});

describe('firstLeaf — the titlebar reserve predicate', () => {
  it('is a tautology at depth 0', () => {
    expect(firstLeaf({ kind: 'leaf', groupId: 'grp-1' })).toBe('grp-1');
  });

  it('walks children[0] on a col split, NOT an array index', () => {
    // The renderer cannot produce this tree today. The test exists so the split
    // cannot introduce a silent regression later: in a COL split both groups sit
    // at x=0, but only the TOP one collides with the traffic lights.
    const tree = {
      kind: 'branch' as const,
      dir: 'col' as const,
      sizes: [0.5, 0.5],
      children: [
        { kind: 'leaf' as const, groupId: 'top' },
        { kind: 'leaf' as const, groupId: 'bottom' },
      ],
    };
    expect(firstLeaf(tree)).toBe('top');
    expect(leafGroupIds(tree)).toEqual(['top', 'bottom']);
  });

  it('recurses to the deepest first leaf, not the first child branch', () => {
    const tree = {
      kind: 'branch' as const,
      dir: 'row' as const,
      sizes: [0.5, 0.5],
      children: [
        {
          kind: 'branch' as const,
          dir: 'col' as const,
          sizes: [0.5, 0.5],
          children: [
            { kind: 'leaf' as const, groupId: 'deep-top' },
            { kind: 'leaf' as const, groupId: 'deep-bottom' },
          ],
        },
        { kind: 'leaf' as const, groupId: 'right' },
      ],
    };
    expect(firstLeaf(tree)).toBe('deep-top');
    expect(leafGroupIds(tree)).toEqual(['deep-top', 'deep-bottom', 'right']);
  });
});
