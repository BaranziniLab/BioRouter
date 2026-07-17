import { ChatGroupsState, ChatGroup } from './chatGroupsTypes';
import { createInitialChatGroupsState } from './chatGroupsReducer';

export const CHAT_GROUPS_KEY_PREFIX = 'biorouter.chatgroups.v1';
const WINDOW_ID_KEY = 'biorouter.chatgroups.windowId';

/**
 * R3: two Electron windows share an origin and would clobber ONE localStorage
 * key. createChatWindow (main.ts:~894) spawns a real second renderer, so this
 * is not hypothetical. The Dashboard has exactly this bug today with
 * `biorouter.dashboard.v2`, unnoticed only because nobody opens two dashboards.
 * Tabs will be used daily. Do not copy the Dashboard here.
 *
 * The id is minted into sessionStorage rather than plumbed through appConfig:
 * sessionStorage is per-BrowserWindow by construction (each renderer gets its
 * own) and survives a reload of that window, which is exactly the scope the key
 * needs — and it costs zero main-process/IPC surface. Accepted residue, as the
 * plan allows: one stale key per crashed window.
 */
export function getWindowId(): string {
  try {
    const existing = sessionStorage.getItem(WINDOW_ID_KEY);
    if (existing) return existing;
    const minted = `w${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`;
    sessionStorage.setItem(WINDOW_ID_KEY, minted);
    return minted;
  } catch {
    // Storage unavailable (jsdom edge, privacy mode): fall back to a per-load
    // id. Persistence is lost, correctness is not.
    return 'w-ephemeral';
  }
}

export function chatGroupsStorageKey(windowId: string): string {
  return `${CHAT_GROUPS_KEY_PREFIX}:${windowId}`;
}

/** Route-state cargo must NEVER round-trip. A queued message that re-sends
 *  after a reload is a data bug, not a UX wrinkle. */
function stripTransient(group: ChatGroup): ChatGroup {
  return {
    ...group,
    tabs: group.tabs.map((tab) => {
      const {
        pendingInitialMessage: _m,
        pendingInitialAttachments: _a,
        ...persisted
      } = tab;
      void _m;
      void _a;
      return persisted;
    }),
  };
}

export function saveChatGroups(state: ChatGroupsState, windowId: string): void {
  try {
    const groups: Record<string, ChatGroup> = {};
    for (const [id, group] of Object.entries(state.groups)) {
      groups[id] = stripTransient(group);
    }
    localStorage.setItem(chatGroupsStorageKey(windowId), JSON.stringify({ ...state, groups }));
  } catch {
    // A full or unavailable quota must never take the chat down.
  }
}

function isValid(value: unknown): value is ChatGroupsState {
  if (!value || typeof value !== 'object') return false;
  const state = value as Partial<ChatGroupsState>;
  if (state.version !== 1) return false;
  if (!state.layout || typeof state.layout !== 'object') return false;
  if (!state.groups || typeof state.groups !== 'object') return false;
  if (typeof state.activeGroupId !== 'string') return false;
  if (typeof state.seq !== 'number') return false;
  if (!state.groups[state.activeGroupId]) return false;
  return Object.values(state.groups).every((g) => Array.isArray(g?.tabs));
}

/** Returns null on ANY shape mismatch -> the caller takes the cold-boot path. */
export function loadChatGroups(windowId: string): ChatGroupsState | null {
  try {
    const raw = localStorage.getItem(chatGroupsStorageKey(windowId));
    if (!raw) return null;
    const parsed: unknown = JSON.parse(raw);
    if (!isValid(parsed)) return null;

    const groups: Record<string, ChatGroup> = {};
    for (const [id, group] of Object.entries(parsed.groups)) {
      // A tab with sessionId '' never resolved its createSession. It cannot be
      // restored into anything meaningful, so it is dropped rather than
      // rendered as a permanently blank tab.
      const tabs = group.tabs.filter((tab) => typeof tab?.sessionId === 'string' && tab.sessionId);
      const activeTabId = tabs.some((t) => t.tabId === group.activeTabId)
        ? group.activeTabId
        : (tabs[0]?.tabId ?? null);
      groups[id] = { ...group, tabs, activeTabId };
    }
    if (!groups[parsed.activeGroupId]) return null;
    return { ...parsed, groups };
  } catch {
    return null;
  }
}

export function loadChatGroupsOrInitial(windowId: string): ChatGroupsState {
  return loadChatGroups(windowId) ?? createInitialChatGroupsState();
}

/** Prune keys belonging to windows that are gone. Best-effort; called on
 *  beforeunload for this window's own key only when it holds nothing worth
 *  keeping. */
export function clearChatGroups(windowId: string): void {
  try {
    localStorage.removeItem(chatGroupsStorageKey(windowId));
  } catch {
    // ignore
  }
}
