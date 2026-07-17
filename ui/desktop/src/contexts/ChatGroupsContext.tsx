import {
  createContext,
  useContext,
  useReducer,
  useMemo,
  useEffect,
  useRef,
  useCallback,
} from 'react';
import {
  chatGroupsReducer,
  ChatGroupsAction,
  activeSessionIdOf,
  activeGroupOf,
  activeTabOf,
} from '../components/chatGroups/chatGroupsReducer';
import {
  loadChatGroupsOrInitial,
  saveChatGroups,
  getWindowId,
} from '../components/chatGroups/chatGroupsStorage';
import {
  useChatGroupsUrlSync,
  UrlOpenRequest,
} from '../components/chatGroups/useChatGroupsUrlSync';
import { registerCloseActiveTab } from '../components/chatGroups/closeActiveTabRegistry';
import { ChatGroupsState, ChatGroup, ChatTab } from '../components/chatGroups/chatGroupsTypes';
import { useRunningChats } from '../hooks/chatStreamStore';
import { subscribeSessionNameChanges } from '../utils/sessionNameSync';

interface ChatGroupsContextValue {
  state: ChatGroupsState;
  dispatch: (action: ChatGroupsAction) => void;
  activeGroup: ChatGroup | undefined;
  activeTab: ChatTab | undefined;
  activeSessionId: string;
  runningSessionIds: readonly string[];
}

const ChatGroupsContext = createContext<ChatGroupsContextValue | null>(null);

/** Returns null outside a provider (the ChatContext pattern) — never throws. */
export function useChatGroups(): ChatGroupsContextValue | null {
  return useContext(ChatGroupsContext);
}

export function ChatGroupsProvider({ children }: { children: React.ReactNode }) {
  const windowIdRef = useRef<string>('');
  if (!windowIdRef.current) windowIdRef.current = getWindowId();

  const [state, dispatch] = useReducer(
    chatGroupsReducer,
    windowIdRef.current,
    loadChatGroupsOrInitial
  );

  const running = useRunningChats();
  // `completedAt` must be filtered out, exactly as AppSidebar:139 does: the
  // registry keeps finished entries in the snapshot for ~1.6s so the sidebar can
  // play its completion flourish. Without this filter a chat that just finished
  // would keep pulsing in the strip, and the strip would disagree with the
  // sidebar about which chats are live.
  const runningSessionIds = useMemo(
    () => running.filter((entry) => !entry.completedAt).map((entry) => entry.sessionId),
    [running]
  );

  // Persist. The transient route cargo is stripped inside saveChatGroups, not
  // here, so no caller can forget.
  useEffect(() => {
    saveChatGroups(state, windowIdRef.current);
  }, [state]);

  // A session rename anywhere mirrors into the strip.
  useEffect(
    () =>
      subscribeSessionNameChanges((change) => {
        dispatch({
          type: 'renameTab',
          sessionId: change.sessionId,
          title: change.name,
          userSetName: change.userSetName,
        });
      }),
    []
  );

  const handleUrlOpen = useCallback((request: UrlOpenRequest) => {
    dispatch({
      type: 'openTab',
      payload: {
        sessionId: request.sessionId,
        // Born with the right name when the opener knew it (a Recents click),
        // rather than showing the placeholder until BaseChat finishes loading.
        // Undefined for a deep link / fresh chat — the reducer falls back to
        // DEFAULT_TAB_TITLE and the late rename still corrects it.
        title: request.title,
        userSetName: request.userSetName,
        pendingInitialMessage: request.initialMessage,
        pendingInitialAttachments: request.initialAttachments,
      },
    });
  }, []);

  // Cmd+W / Ctrl+W. The keystroke arrives as an IPC message from the File menu's
  // "Close Tab" item, NOT as a renderer keydown — see main.ts. That is deliberate
  // and it is the only safe arrangement: an Electron menu accelerator is consumed
  // by the menu before the web contents ever sees the key, so a renderer keydown
  // listener could not have won the race against the `role: 'close'` item that
  // used to hold Cmd+W. The window would have closed regardless of what we did
  // here, taking every other tab with it.
  //
  // Returning `true` claims the keystroke. Returning false (no tabs left to
  // close) lets App.tsx fall through to closing the window, which is what a
  // macOS user expects from Cmd+W on a tabless window.
  const activeTabId = activeGroupOf(state)?.activeTabId ?? null;
  const activeTabIdRef = useRef(activeTabId);
  activeTabIdRef.current = activeTabId;

  useEffect(
    () =>
      registerCloseActiveTab(() => {
        const tabId = activeTabIdRef.current;
        if (!tabId) return false;
        dispatch({ type: 'closeTab', tabId });
        return true;
      }),
    []
  );

  const activeSessionId = activeSessionIdOf(state);
  useChatGroupsUrlSync({ activeSessionId, onOpen: handleUrlOpen });

  const value = useMemo<ChatGroupsContextValue>(
    () => ({
      state,
      dispatch,
      activeGroup: activeGroupOf(state),
      activeTab: activeTabOf(state),
      activeSessionId,
      runningSessionIds,
    }),
    [state, activeSessionId, runningSessionIds]
  );

  return <ChatGroupsContext.Provider value={value}>{children}</ChatGroupsContext.Provider>;
}
