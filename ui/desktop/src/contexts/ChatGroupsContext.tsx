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
import {
  registerNewTab,
  consumePendingNewTab,
  acknowledgeNewTabCommit,
} from '../components/chatGroups/newTabRegistry';
import {
  isTabCycleEvent,
  tabCycleOffset,
  nextTabIndex,
  isWithinArtifactPanel,
} from '../utils/tabCycle';
import {
  ChatGroupsState,
  ChatGroup,
  ChatTab,
  leafGroupIds,
} from '../components/chatGroups/chatGroupsTypes';
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

  // Cmd+T / Ctrl+T. Arrives as IPC from the Go menu's "New Chat" item, for the
  // same reason Cmd+W does — the menu owns the accelerator and the renderer
  // never sees the keydown.
  //
  // A new tab is a tab with no session: sessionId '' is what BaseChat renders
  // as its centered empty composer. The reducer's dedupe is gated on a truthy
  // sessionId, so an empty tab never collapses onto an existing one — Cmd+T
  // twice gives two blank tabs, exactly as a browser does. `title` is left
  // undefined so DEFAULT_TAB_TITLE applies; a Cmd+T tab has no name yet and
  // inventing a placeholder here would put the naming rule in two places.
  const openNewTab = useCallback(() => {
    dispatch({ type: 'openTab', payload: { sessionId: '' } });
  }, []);

  useEffect(() => registerNewTab(openNewTab), [openNewTab]);

  // A Cmd+T that arrived while we were on another route (Settings, the Hub) was
  // remembered rather than dropped; App.tsx navigated here for us and this is
  // where the request is cashed in. consumePendingNewTab is consume-once, so
  // StrictMode's double mount opens one tab, not two.
  useEffect(() => {
    if (consumePendingNewTab()) openNewTab();
  }, [openNewTab]);

  // A Cmd+T dispatched to THIS mounted provider stays observable in the
  // registry (hasPendingNewTab) until the tab it asked for has actually
  // COMMITTED — otherwise the empty-pair redirect's stale zero-tab effect
  // (issue #38) could bounce the window Home in the gap between the dispatch
  // and the commit, discarding the tab with the unmounting provider. Keyed on
  // `state` so it runs on every commit; child effects (the redirect) run
  // first, then this acknowledgement retires the request once tabs exist.
  useEffect(() => {
    acknowledgeNewTabCommit(
      leafGroupIds(state.layout).reduce((count, id) => count + state.groups[id].tabs.length, 0)
    );
  }, [state]);

  // Ctrl+Tab / Ctrl+Shift+Tab — cycle the focused group's strip, left to right,
  // wrapping. Unlike Cmd+T and Cmd+W this really is a DOM keydown: a dump of
  // the built application menu shows nothing claiming Ctrl+Tab, so the key
  // reaches the renderer and a listener is the honest mechanism.
  //
  // The preview panel cycles its OWN stack on the same key, so focus arbitrates
  // — see isWithinArtifactPanel, which both sides consult so their answers
  // cannot disagree. We return early on the panel's keystrokes unconditionally,
  // rather than only when we have nothing to cycle: both listeners are on
  // window in the capture phase, and "whoever runs first wins" would make the
  // behaviour depend on mount order.
  const stateRef = useRef(state);
  stateRef.current = state;

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!isTabCycleEvent(event)) return;
      if (isWithinArtifactPanel(event.target)) return;

      const current = stateRef.current;
      const group = activeGroupOf(current);
      if (!group) return;

      const activeIndex = group.tabs.findIndex((t) => t.tabId === group.activeTabId);
      const next = nextTabIndex(group.tabs.length, activeIndex, tabCycleOffset(event));
      // Null for 0 or 1 tabs. Do NOT preventDefault in that case — with nothing
      // to cycle, swallowing the key would only rob Tab of its focus behaviour.
      if (next === null) return;

      event.preventDefault();
      event.stopPropagation();
      dispatch({ type: 'activateTab', tabId: group.tabs[next].tabId });
    };

    window.addEventListener('keydown', onKeyDown, true);
    return () => window.removeEventListener('keydown', onKeyDown, true);
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
