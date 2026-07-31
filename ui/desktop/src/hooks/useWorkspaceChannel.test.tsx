import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useWorkspaceChannel, buildEchoFrame } from './useWorkspaceChannel';
import {
  registerWorkspaceCommands,
  resetWorkspaceCommandRegistry,
} from '../components/chatGroups/workspaceCommandRegistry';
import {
  chatGroupsReducer,
  createInitialChatGroupsState,
} from '../components/chatGroups/chatGroupsReducer';
import { leafGroupIds } from '../components/chatGroups/chatGroupsTypes';

class FakeWebSocket {
  static instances: FakeWebSocket[] = [];
  static OPEN = 1;
  readyState = FakeWebSocket.OPEN;
  sent: string[] = [];
  onmessage: ((ev: { data: string }) => void) | null = null;
  onclose: (() => void) | null = null;
  onopen: (() => void) | null = null;
  constructor(public url: string) {
    FakeWebSocket.instances.push(this);
    queueMicrotask(() => this.onopen?.());
  }
  send(data: string) {
    this.sent.push(data);
  }
  close() {
    this.onclose?.();
  }
}

describe('useWorkspaceChannel', () => {
  beforeEach(() => {
    resetWorkspaceCommandRegistry();
    vi.stubGlobal('WebSocket', FakeWebSocket as unknown as typeof WebSocket);
    // getApiUrl reads window.appConfig, which the preload supplies in the real
    // app and jsdom does not. Without it the hook's first connect throws before
    // it can build a socket — a fixture gap, not a behaviour under test.
    (window as unknown as { appConfig: { get: (key: string) => unknown } }).appConfig = {
      get: () => 'http://localhost:9999',
    };
    FakeWebSocket.instances = [];
  });
  afterEach(() => vi.unstubAllGlobals());

  it('applies inbound frames through the registry and answers request_ids', async () => {
    const applied: string[] = [];
    registerWorkspaceCommands((cmd) => {
      applied.push(cmd.cmd);
      return { ok: true, detail: 'done' };
    });
    renderHook(() => useWorkspaceChannel({ secret: 's', windowId: 'w1', enabled: true }));
    await act(async () => {});
    const ws = FakeWebSocket.instances[0];
    act(() => {
      ws.onmessage?.({
        data: JSON.stringify({
          type: 'workspace',
          cmd: 'open_tab',
          session_id: 's1',
          request_id: 'wsreq-1',
        }),
      });
    });
    expect(applied).toEqual(['open_tab']);
    const reply = ws.sent.map((s) => JSON.parse(s)).find((f) => f.type === 'workspace_result');
    expect(reply).toMatchObject({ request_id: 'wsreq-1', ok: true, detail: 'done' });
  });

  it('buildEchoFrame reports the layout tree in LEAF order, with session bindings', () => {
    // Real state, built by the real reducer — not a hand-written literal. Two
    // reasons, both learned the hard way: `ChatTab.userSetName` is required, so
    // a `{ tabId, sessionId, title }` literal does not typecheck against
    // `ChatGroupsState`; and a fixture that agrees with `Object.keys(groups)`
    // cannot catch the one bug this function has.
    //
    // The split is deliberately LEFT. `splitLeaf` puts the new group BEFORE the
    // one it split (`chatGroupsLayout.ts` BEFORE.left === true) while the
    // `groups` record is keyed by insertion order — so after this the leaf walk
    // says [grp-new, grp-1] and `Object.keys(state.groups)` says
    // [grp-1, grp-new]. An implementation that walks `Object.values(state.groups)`
    // passes a single-group or right-split fixture and reports the two panes to
    // the daemon in the WRONG ORDER, silently. This fixture is the only thing
    // standing between that and `workspace_list`'s `gui` block.
    let state = createInitialChatGroupsState();
    state = chatGroupsReducer(state, {
      type: 'openTab',
      payload: { sessionId: 's-focused', title: 'A' },
    });
    // sessionId '' is a real, reachable state: a tab opened and not yet bound.
    state = chatGroupsReducer(state, {
      type: 'openTab',
      payload: { sessionId: '', title: 'blank' },
    });
    const moved = state.groups[state.activeGroupId].tabs[0];
    state = chatGroupsReducer(state, {
      type: 'moveTabToGroup',
      tabId: moved.tabId,
      targetGroupId: state.activeGroupId,
      zone: 'left',
    });

    const leaves = leafGroupIds(state.layout);
    // The fixture is only meaningful if the two orders disagree. Assert that,
    // so a reducer change that quietly makes them agree fails HERE rather than
    // turning the assertions below into a tautology.
    expect(leaves).not.toEqual(Object.keys(state.groups));

    const echo = buildEchoFrame('w1', 's-focused', state);
    expect(echo.type).toBe('workspace_echo');
    expect(echo.window_id).toBe('w1');
    expect(echo.focused_session).toBe('s-focused');
    expect(echo.layout.map((g) => g.group_id)).toEqual(leaves);
    const first = echo.layout[0];
    expect(first.tabs[0].session_id).toBe('s-focused');
    expect(first.active_tab).toBe(state.groups[leaves[0]].activeTabId);
    // The UNBOUND tab is reported, not filtered: merged_layout() has to know a
    // blank tab exists before it can decide to reuse it.
    expect(echo.layout.flatMap((g) => g.tabs)).toHaveLength(2);
  });
});
