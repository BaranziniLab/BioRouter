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
  // would keep pulsing in the strip AND would refuse to give up its preview tab
  // (pin-on-run) for that window — and the strip would disagree with the sidebar
  // about which chats are live.
  const runningSessionIds = useMemo(
    () => running.filter((entry) => !entry.completedAt).map((entry) => entry.sessionId),
    [running]
  );
  const runningRef = useRef(runningSessionIds);
  runningRef.current = runningSessionIds;

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

  // "Submitting a message pins it." A preview tab whose session starts a turn is
  // committed-to, exactly as double-clicking commits to it. useRunningChats is
  // already the authority on which sessions have a live turn, so this needs no
  // new signal from the composer and no backend work.
  useEffect(() => {
    for (const group of Object.values(state.groups)) {
      for (const tab of group.tabs) {
        if (tab.preview && tab.sessionId && runningSessionIds.includes(tab.sessionId)) {
          dispatch({ type: 'pinTab', tabId: tab.tabId });
        }
      }
    }
  }, [runningSessionIds, state.groups]);

  const handleUrlOpen = useCallback((request: UrlOpenRequest) => {
    dispatch({
      type: 'openTab',
      payload: {
        sessionId: request.sessionId,
        preview: request.preview,
        pendingInitialMessage: request.initialMessage,
        pendingInitialAttachments: request.initialAttachments,
      },
      // Pin-on-run: never recycle a preview tab mid-turn.
      runningSessionIds: runningRef.current,
    });
  }, []);

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
