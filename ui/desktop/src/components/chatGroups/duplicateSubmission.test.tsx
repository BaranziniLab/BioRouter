import { describe, it, expect } from 'vitest';
import {
  chatGroupsReducer,
  createInitialChatGroupsState,
  activeTabOf,
  ChatGroupsAction,
} from './chatGroupsReducer';
import { ChatGroupsState, leafGroupIds } from './chatGroupsTypes';

/**
 * REGRESSION GATE — duplicate submission on tab close.
 *
 * Reproduced live on 2026-07-18: one message plus three open/close tab cycles
 * produced FOUR identical user messages and four real LLM turns (23,584 input
 * tokens) in one session.
 *
 * A tab carries `pendingInitialMessage` — the message that created its session.
 * `ChatGroupsShell` feeds it to `BaseChat` as `initialMessage` and `BaseChat`
 * auto-submits on mount. Only a group's ACTIVE tab is mounted, so closing a tab
 * remounts the successor's `BaseChat`, which re-submitted the same message as a
 * genuine agent turn. `BaseChat`'s only guards were a component-local ref (dies
 * with the unmount) and a `navigate()` that clears router state (not the tab).
 *
 * The reducer always had `consumePending` — nothing dispatched it. These tests
 * pin the contract the fix relies on.
 */
function run(state: ChatGroupsState, ...actions: ChatGroupsAction[]): ChatGroupsState {
  return actions.reduce(chatGroupsReducer, state);
}

const allTabs = (s: ChatGroupsState) => leafGroupIds(s.layout).flatMap((id) => s.groups[id].tabs);
const cargoTabs = (s: ChatGroupsState) =>
  allTabs(s).filter((t) => t.pendingInitialMessage !== undefined);

/** The pre-session submit path: openTab carrying route-state cargo. */
const submit = (sessionId: string, message: string): ChatGroupsAction => ({
  type: 'openTab',
  payload: { sessionId, title: sessionId, pendingInitialMessage: message },
});

describe('tab route cargo is spent exactly once', () => {
  it('openTab stores the message that created the session', () => {
    const s = run(createInitialChatGroupsState(), submit('session-A', 'QQMARKER name one color'));
    expect(cargoTabs(s).map((t) => t.pendingInitialMessage)).toEqual(['QQMARKER name one color']);
  });

  it('consumePending clears the cargo but keeps the tab and its session', () => {
    const seeded = run(createInitialChatGroupsState(), submit('session-A', 'QQMARKER'));
    const tab = cargoTabs(seeded)[0];

    const after = chatGroupsReducer(seeded, { type: 'consumePending', tabId: tab.tabId });

    expect(cargoTabs(after)).toEqual([]);
    const survivor = allTabs(after).find((t) => t.tabId === tab.tabId);
    expect(survivor).toBeDefined();
    expect(survivor?.sessionId).toBe('session-A');
  });

  it('is idempotent — consuming twice is a no-op, not a crash', () => {
    const seeded = run(createInitialChatGroupsState(), submit('session-A', 'QQMARKER'));
    const tab = cargoTabs(seeded)[0];
    const once = chatGroupsReducer(seeded, { type: 'consumePending', tabId: tab.tabId });
    expect(chatGroupsReducer(once, { type: 'consumePending', tabId: tab.tabId })).toBe(once);
  });

  it('the reported sequence leaves nothing to replay: submit, consume, open tab, close tab', () => {
    let s = run(createInitialChatGroupsState(), submit('session-A', 'QQMARKER'));
    const cargoTab = cargoTabs(s)[0];

    // BaseChat mounts and submits -> the Shell dispatches consumePending.
    s = chatGroupsReducer(s, { type: 'consumePending', tabId: cargoTab.tabId });

    // User clicks "+" then immediately closes the blank tab.
    s = chatGroupsReducer(s, { type: 'openTab', payload: { sessionId: '', title: 'New Session' } });
    const blank = allTabs(s).find((t) => t.tabId !== cargoTab.tabId);
    expect(blank).toBeDefined();
    s = chatGroupsReducer(s, { type: 'closeTab', tabId: blank!.tabId });

    // Focus returns to the original tab, and it has no cargo to re-submit.
    expect(activeTabOf(s)?.tabId).toBe(cargoTab.tabId);
    expect(cargoTabs(s)).toEqual([]);
  });

  it('WITHOUT consumePending the cargo survives a tab close — the bug', () => {
    let s = run(createInitialChatGroupsState(), submit('session-A', 'QQMARKER'));
    const cargoTab = cargoTabs(s)[0];

    s = chatGroupsReducer(s, { type: 'openTab', payload: { sessionId: '', title: 'New Session' } });
    const blank = allTabs(s).find((t) => t.tabId !== cargoTab.tabId)!;
    s = chatGroupsReducer(s, { type: 'closeTab', tabId: blank.tabId });

    // This is what shipped: focus back on the tab, cargo intact, so the
    // remounted BaseChat re-submits it. Documents the mechanism.
    expect(activeTabOf(s)?.tabId).toBe(cargoTab.tabId);
    expect(cargoTabs(s).map((t) => t.pendingInitialMessage)).toEqual(['QQMARKER']);
  });
});
