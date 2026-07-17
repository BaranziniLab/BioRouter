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

const open = (sessionId: string): ChatGroupsAction => ({
  type: 'openTab',
  payload: { sessionId, title: sessionId },
});

/** The pre-session submit path: an openTab carrying route-state cargo. */
const submit = (sessionId: string, message = 'hello'): ChatGroupsAction => ({
  type: 'openTab',
  payload: { sessionId, title: sessionId, pendingInitialMessage: message },
});

/**
 * Every open is a real tab. This replaces the old preview/enablePreview suite
 * wholesale: that behaviour (single click = italic tab, recycled in place by the
 * next click) shipped, the user used it, and rejected it. Clicking around
 * Recents must never disturb the chat you were reading.
 */
describe('chatGroupsReducer — a click opens a real tab', () => {
  it('opens a NEW tab per chat and never replaces the previous one', () => {
    const a = run(createInitialChatGroupsState(), open('s1'));
    const firstTabId = a.groups['grp-1'].tabs[0].tabId;

    const b = run(a, open('s2'), open('s3'));

    expect(b.groups['grp-1'].tabs.map((t) => t.sessionId)).toEqual(['s1', 's2', 's3']);
    // The tab you were reading keeps its identity and its place.
    expect(b.groups['grp-1'].tabs[0].tabId).toBe(firstTabId);
    // The newest one takes focus.
    expect(activeSessionIdOf(b)).toBe('s3');
  });

  it('leaves twelve DISTINCT Recents clicks as twelve tabs', () => {
    let state = createInitialChatGroupsState();
    for (let i = 0; i < 12; i++) state = chatGroupsReducer(state, open(`s${i}`));
    expect(state.groups['grp-1'].tabs).toHaveLength(12);
    expect(activeSessionIdOf(state)).toBe('s11');
  });

  it('no tab carries a preview marker — the concept is gone, not hidden', () => {
    const state = run(createInitialChatGroupsState(), open('s1'), open('s2'));
    for (const tab of state.groups['grp-1'].tabs) {
      expect(tab).not.toHaveProperty('preview');
    }
  });
});

describe('chatGroupsReducer — empty-tab adoption is the submit path ONLY', () => {
  it('a submit fills the blank tab the user is looking at, keeping its tabId', () => {
    // A blank, unbound tab — what "New Chat" leaves behind.
    const a = run(createInitialChatGroupsState(), { type: 'openTab', payload: { sessionId: '' } });
    const blankTabId = a.groups['grp-1'].tabs[0].tabId;

    const b = chatGroupsReducer(a, submit('s1'));

    expect(b.groups['grp-1'].tabs).toHaveLength(1);
    expect(b.groups['grp-1'].tabs[0].tabId).toBe(blankTabId);
    expect(b.groups['grp-1'].tabs[0].sessionId).toBe('s1');
  });

  it('with TWO blank tabs, the submit fills the ACTIVE one — not the leftmost', () => {
    // Cmd+T twice is now an ordinary thing to do, so two blank tabs is an
    // ordinary state, and "find the first blank tab" is no longer the same
    // question as "find the tab the user is typing in". Before Cmd+T existed
    // you could barely reach two blanks; now the user's first message would
    // land in the OTHER blank tab and the tab they typed in would sit empty.
    const a = run(
      createInitialChatGroupsState(),
      { type: 'openTab', payload: { sessionId: '' } },
      { type: 'openTab', payload: { sessionId: '' } }
    );
    const [first, second] = a.groups['grp-1'].tabs;
    expect(a.groups['grp-1'].activeTabId).toBe(second.tabId);

    const b = chatGroupsReducer(a, submit('s1'));

    expect(b.groups['grp-1'].tabs).toHaveLength(2);
    expect(b.groups['grp-1'].tabs[1].tabId).toBe(second.tabId);
    expect(b.groups['grp-1'].tabs[1].sessionId).toBe('s1');
    // ...and the untouched blank is still blank.
    expect(b.groups['grp-1'].tabs[0].tabId).toBe(first.tabId);
    expect(b.groups['grp-1'].tabs[0].sessionId).toBe('');
    expect(b.groups['grp-1'].activeTabId).toBe(second.tabId);
  });

  it('a Recents click NEVER consumes a blank tab — it opens its own', () => {
    // The regression this gate exists for: without it, "open in a new tab"
    // silently becomes "replace the blank tab".
    const a = run(createInitialChatGroupsState(), { type: 'openTab', payload: { sessionId: '' } });
    const b = chatGroupsReducer(a, open('s1'));

    expect(b.groups['grp-1'].tabs).toHaveLength(2);
    expect(b.groups['grp-1'].tabs[1].sessionId).toBe('s1');
    expect(activeSessionIdOf(b)).toBe('s1');
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

  it('clicking the same chat repeatedly never grows the strip', () => {
    let state = run(createInitialChatGroupsState(), open('s1'));
    for (let i = 0; i < 5; i++) state = chatGroupsReducer(state, open('s1'));
    expect(state.groups['grp-1'].tabs).toHaveLength(1);
  });

  it('dedupe reaches ACROSS groups, and focuses the group that owns the tab', () => {
    // The user's rule — "new chats that are not already launched as a tab will
    // always launch as a tab" — is about the WINDOW, not one strip. A chat open
    // in the other half of a split must not get a second tab over here.
    const a = run(createInitialChatGroupsState(), open('s1'), open('s2'));
    const s1Tab = a.groups['grp-1'].tabs[0].tabId;
    const split = chatGroupsReducer(a, {
      type: 'moveTabToGroup',
      tabId: s1Tab,
      targetGroupId: 'grp-1',
      zone: 'right',
    });
    const otherGroupId = leafGroupIds(split.layout).find((id) => id !== 'grp-1')!;
    expect(split.groups[otherGroupId].tabs.map((t) => t.sessionId)).toEqual(['s1']);

    // Focus the group that does NOT hold s1, then open s1 from Recents.
    const b = run(split, { type: 'setActiveGroup', groupId: 'grp-1' }, open('s1'));

    expect(b.groups['grp-1'].tabs.map((t) => t.sessionId)).toEqual(['s2']);
    expect(b.groups[otherGroupId].tabs.map((t) => t.sessionId)).toEqual(['s1']);
    expect(b.activeGroupId).toBe(otherGroupId);
    expect(activeSessionIdOf(b)).toBe('s1');
  });
});

describe('chatGroupsReducer — close with successor', () => {
  it('successor is Math.min(closingIndex, remaining.length - 1)', () => {
    const a = run(createInitialChatGroupsState(), open('a'), open('b'), open('c'));
    const [, tabB] = a.groups['grp-1'].tabs;

    // Close the MIDDLE active tab -> index 1 of the remaining [a, c] is 'c'.
    const b = run(
      a,
      { type: 'activateTab', tabId: tabB.tabId },
      { type: 'closeTab', tabId: tabB.tabId }
    );
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
      chatGroupsReducer(a, {
        type: 'reorderTab',
        draggedTabId: tabA.tabId,
        targetTabId: tabA.tabId,
      })
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
