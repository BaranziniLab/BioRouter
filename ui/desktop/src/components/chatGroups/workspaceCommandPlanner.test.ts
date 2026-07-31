import { describe, expect, it } from 'vitest';
import { planWorkspaceCommand } from './workspaceCommandPlanner';
import { chatGroupsReducer, createInitialChatGroupsState, activeTabOf } from './chatGroupsReducer';
// Types come from chatGroupsTypes; the reducer exports functions only. This is
// the convention every existing consumer follows (chatGroupsStorage.ts:1-2,
// chatGroupsReducer.test.ts:9) — importing `ChatGroupsState` from
// './chatGroupsReducer' is a compile error, not a style preference.
import { leafGroupIds, type ChatGroupsState } from './chatGroupsTypes';
import { MAX_GROUPS, groupCountOf } from './chatGroupsLayout';

/** Real state with N session-bound tabs, built through the real reducer. */
function stateWithSessions(ids: string[]): ChatGroupsState {
  let state = createInitialChatGroupsState();
  for (const id of ids) {
    state = chatGroupsReducer(state, { type: 'openTab', payload: { sessionId: id } });
  }
  return state;
}

describe('planWorkspaceCommand', () => {
  it('open_tab focus:false opens then restores the previously active tab', () => {
    const state = stateWithSessions(['s-mine']);
    const prevActive = activeTabOf(state)?.tabId;
    const plan = planWorkspaceCommand(
      { type: 'workspace', cmd: 'open_tab', session_id: 's-new', placement: 'tab', focus: false },
      state
    );
    expect(plan.result.ok).toBe(true);
    expect(plan.actions[0]).toEqual({ type: 'openTab', payload: { sessionId: 's-new' } });
    // Focus etiquette (§4.1): the LAST action re-activates the user's tab.
    expect(plan.actions[plan.actions.length - 1]).toEqual({
      type: 'activateTab',
      tabId: prevActive,
    });
  });

  it('open_tab focus:true does not restore', () => {
    const plan = planWorkspaceCommand(
      { type: 'workspace', cmd: 'open_tab', session_id: 's-new', placement: 'tab', focus: true },
      stateWithSessions(['s-mine'])
    );
    expect(plan.actions).toHaveLength(1);
  });

  it('refuses a split at MAX_GROUPS with a clear detail', () => {
    // MAX_GROUPS tabs in one group, then MAX_GROUPS-1 right-edge splits: each
    // split moves one tab out of the first leaf into a NEW group, so the count
    // goes 1 → MAX_GROUPS and the first leaf keeps exactly one tab (which is
    // what stops `collapseEmptyGroup` from undoing the last move). Verified
    // against the real reducer: leaves end as
    // [grp-1, grp-8, grp-9, grp-10, grp-11, grp-12] — the ids are seq-derived
    // and shared with tab ids, which is why nothing here hardcodes one.
    let state = stateWithSessions(Array.from({ length: MAX_GROUPS }, (_, i) => `s-${i}`));
    for (let i = 1; i < MAX_GROUPS; i++) {
      const leaves = leafGroupIds(state.layout);
      // No `if (!tab) break` — a missing tab means the fixture broke, and the
      // TypeError that follows is a louder, better failure than a silent skip.
      const tab = state.groups[leaves[0]].tabs[0];
      state = chatGroupsReducer(state, {
        type: 'moveTabToGroup',
        tabId: tab.tabId,
        targetGroupId: leaves[leaves.length - 1],
        zone: 'right',
      });
    }

    // UNCONDITIONAL, and before the act. The pre-amendment revision wrapped all
    // three assertions below in `if (state.order.length >= MAX_GROUPS)`, so a
    // fixture that fell short made the test pass having asserted nothing — the
    // exact failure this plan's own gate-quality rule exists to prevent.
    expect(groupCountOf(state.layout)).toBe(MAX_GROUPS);

    const plan = planWorkspaceCommand(
      { type: 'workspace', cmd: 'open_tab', session_id: 's-x', placement: 'split', focus: false },
      state
    );
    expect(plan.result.ok).toBe(false);
    expect(plan.result.detail).toContain('split refused');
    expect(plan.actions).toHaveLength(0);
  });

  it('activate/close resolve tabs by session id; misses are reported', () => {
    const state = stateWithSessions(['s-1']);
    const hit = planWorkspaceCommand(
      { type: 'workspace', cmd: 'close_tab', session_id: 's-1' },
      state
    );
    expect(hit.result.ok).toBe(true);
    expect(hit.actions[0].type).toBe('closeTab');
    const miss = planWorkspaceCommand(
      { type: 'workspace', cmd: 'activate_tab', session_id: 's-none' },
      state
    );
    expect(miss.result.ok).toBe(false);
    expect(miss.result.detail).toBe('session has no tab');
  });

  it('open_window and notify and annotate_tab become side-effect plans', () => {
    const state = stateWithSessions([]);
    const win = planWorkspaceCommand(
      { type: 'workspace', cmd: 'open_window', session_id: 's-w' },
      state
    );
    expect(win.openWindowSessionId).toBe('s-w');
    const note = planWorkspaceCommand(
      { type: 'workspace', cmd: 'notify', session_id: 's-1', message: 'tools changed' },
      state
    );
    expect(note.notify).toEqual({ message: 'tools changed', level: undefined });
    const badge = planWorkspaceCommand(
      {
        type: 'workspace',
        cmd: 'annotate_tab',
        session_id: 'c-1',
        badge: 'subagent',
        parent_session_id: 'p-1',
      },
      state
    );
    expect(badge.annotate).toEqual({
      sessionId: 'c-1',
      annotation: { badge: 'subagent', parentSessionId: 'p-1' },
    });
  });
});
