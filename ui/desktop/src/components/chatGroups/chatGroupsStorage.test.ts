import { describe, it, expect, beforeEach } from 'vitest';
import {
  saveChatGroups,
  loadChatGroups,
  loadChatGroupsOrInitial,
  chatGroupsStorageKey,
  getWindowId,
} from './chatGroupsStorage';
import { createInitialChatGroupsState, chatGroupsReducer } from './chatGroupsReducer';
import { ChatGroupsState } from './chatGroupsTypes';

function seeded(): ChatGroupsState {
  return chatGroupsReducer(createInitialChatGroupsState(), {
    type: 'openTab',
    payload: {
      sessionId: 's1',
      title: 'Cohort',
      pendingInitialMessage: 'do not re-send me',
      pendingInitialAttachments: [],
    },
  });
}

beforeEach(() => {
  localStorage.clear();
  sessionStorage.clear();
});

describe('chatGroupsStorage', () => {
  it('round-trips a state', () => {
    const state = seeded();
    saveChatGroups(state, 'w1');
    const loaded = loadChatGroups('w1');
    expect(loaded?.groups['grp-1'].tabs[0].sessionId).toBe('s1');
    expect(loaded?.groups['grp-1'].tabs[0].title).toBe('Cohort');
    expect(loaded?.activeGroupId).toBe('grp-1');
  });

  it('NEVER persists pendingInitialMessage / pendingInitialAttachments', () => {
    saveChatGroups(seeded(), 'w1');

    // Assert on the raw serialized bytes, not just the parsed object: a queued
    // message that survives a reload re-sends, which is a data bug.
    const raw = localStorage.getItem(chatGroupsStorageKey('w1'))!;
    expect(raw).not.toContain('do not re-send me');
    expect(raw).not.toContain('pendingInitialMessage');
    expect(raw).not.toContain('pendingInitialAttachments');

    const loaded = loadChatGroups('w1')!;
    expect(loaded.groups['grp-1'].tabs[0].pendingInitialMessage).toBeUndefined();
    expect(loaded.groups['grp-1'].tabs[0].pendingInitialAttachments).toBeUndefined();
  });

  it("prunes sessionId:'' tabs on load and re-points activeTabId", () => {
    const state = chatGroupsReducer(seeded(), { type: 'openTab', payload: { sessionId: '' } });
    expect(state.groups['grp-1'].activeTabId).toBe(state.groups['grp-1'].tabs[1].tabId);
    saveChatGroups(state, 'w1');

    const loaded = loadChatGroups('w1')!;
    expect(loaded.groups['grp-1'].tabs).toHaveLength(1);
    expect(loaded.groups['grp-1'].tabs[0].sessionId).toBe('s1');
    // The pruned tab was active — focus must fall to a tab that still exists.
    expect(loaded.groups['grp-1'].activeTabId).toBe(loaded.groups['grp-1'].tabs[0].tabId);
  });

  it('returns null on garbage, wrong version, and a dangling activeGroupId', () => {
    localStorage.setItem(chatGroupsStorageKey('w1'), 'not json {{{');
    expect(loadChatGroups('w1')).toBeNull();

    localStorage.setItem(chatGroupsStorageKey('w1'), JSON.stringify({ ...seeded(), version: 99 }));
    expect(loadChatGroups('w1')).toBeNull();

    localStorage.setItem(
      chatGroupsStorageKey('w1'),
      JSON.stringify({ ...seeded(), activeGroupId: 'ghost' })
    );
    expect(loadChatGroups('w1')).toBeNull();

    expect(loadChatGroups('never-written')).toBeNull();
  });

  it('falls back to a cold-boot state rather than throwing', () => {
    localStorage.setItem(chatGroupsStorageKey('w1'), '{]');
    const state = loadChatGroupsOrInitial('w1');
    expect(state.groups['grp-1'].tabs).toEqual([]);
    expect(state.activeGroupId).toBe('grp-1');
  });

  it('R3: two windowIds produce two DISJOINT keys and never clobber', () => {
    const a = seeded();
    const b = chatGroupsReducer(createInitialChatGroupsState(), {
      type: 'openTab',
      payload: { sessionId: 'other-window-session' },
    });

    saveChatGroups(a, 'w1');
    saveChatGroups(b, 'w2');

    expect(chatGroupsStorageKey('w1')).not.toBe(chatGroupsStorageKey('w2'));
    expect(loadChatGroups('w1')!.groups['grp-1'].tabs[0].sessionId).toBe('s1');
    expect(loadChatGroups('w2')!.groups['grp-1'].tabs[0].sessionId).toBe('other-window-session');
  });

  it('getWindowId is stable within a window and survives reload', () => {
    const first = getWindowId();
    expect(getWindowId()).toBe(first);
    // A reload keeps sessionStorage, so the window keeps its layout.
    expect(getWindowId()).toBe(first);
    // A different window = a fresh sessionStorage = a different id.
    sessionStorage.clear();
    expect(getWindowId()).not.toBe(first);
  });
});
