import { useCallback, useMemo, useState, useEffect, ReactElement } from 'react';
import BaseChat from '../BaseChat';
import { ChatType } from '../../types/chat';
import { useChatGroups } from '../../contexts/ChatGroupsContext';
import { ChatTabStrip } from './ChatTabStrip';
import { firstLeaf, GroupLayout, ChatGroupId } from './chatGroupsTypes';
import { useIsMobile } from '../../hooks/use-mobile';
import { useSidebar } from '../ui/sidebar';
import { SIDEBAR_COMPACT_TITLE_WIDTH } from '../Layout/TitlebarControls';

interface ChatGroupsShellProps {
  /** Mirrors the focused chat up to App's hubChat, so AppSidebar's recents
   *  highlight and document.title keep working with ZERO sidebar edits. */
  onChatChange: (chat: ChatType) => void;
}

/**
 * Renders the layout tree. TODAY IT RENDERS ONLY `leaf`.
 *
 * The tree type is wider than this renderer on purpose: it ships now so the
 * split is not a state migration later. A `branch` node cannot be produced by
 * any action the reducer exposes today, so reaching this throw means someone
 * wired splitting without wiring its renderer — which must be loud, not a blank
 * pane.
 */
function renderLayout(layout: GroupLayout, renderGroup: (groupId: ChatGroupId) => ReactElement) {
  if (layout.kind === 'leaf') return renderGroup(layout.groupId);
  if (import.meta.env.DEV) {
    throw new Error(
      'ChatGroupsShell: branch layouts are not rendered yet. The split is Stage 3; ' +
        'the layout tree exists in the state model but only `leaf` renders today.'
    );
  }
  // Production degrades to the first leaf rather than white-screening.
  return renderGroup(firstLeaf(layout));
}

export function ChatGroupsShell({ onChatChange }: ChatGroupsShellProps) {
  const groups = useChatGroups();

  const isMobile = useIsMobile();
  const { state: sidebarState } = useSidebar();
  const [isSidebarCompact, setIsSidebarCompact] = useState(
    () => typeof window !== 'undefined' && window.innerWidth < SIDEBAR_COMPACT_TITLE_WIDTH
  );
  useEffect(() => {
    const update = () => setIsSidebarCompact(window.innerWidth < SIDEBAR_COMPACT_TITLE_WIDTH);
    update();
    window.addEventListener('resize', update);
    return () => window.removeEventListener('resize', update);
  }, []);

  const isMacOS = (window?.electron?.platform || 'darwin') === 'darwin';
  const isCompactSidebarOverlayOpen = isSidebarCompact && !isMobile && sidebarState !== 'collapsed';
  const reserveTitlebarControls =
    isMacOS && (isMobile || isSidebarCompact || sidebarState === 'collapsed');

  // firstLeaf is a TREE WALK, never an array index. In a root `col` split both
  // strips sit at x=0 but only the TOP one collides with the traffic lights, so
  // an index would reserve the gap for the wrong strip — silently.
  const reservedGroupId = useMemo(
    () => (groups ? firstLeaf(groups.state.layout) : null),
    [groups]
  );

  const dispatch = groups?.dispatch;
  const handleSelect = useCallback(
    (tabId: string) => dispatch?.({ type: 'activateTab', tabId }),
    [dispatch]
  );
  const handlePin = useCallback((tabId: string) => dispatch?.({ type: 'pinTab', tabId }), [dispatch]);
  const handleClose = useCallback(
    (tabId: string) => dispatch?.({ type: 'closeTab', tabId }),
    [dispatch]
  );
  const handleReorder = useCallback(
    (draggedTabId: string, targetTabId: string) =>
      dispatch?.({ type: 'reorderTab', draggedTabId, targetTabId }),
    [dispatch]
  );
  /**
   * Mirror the loaded session's real name onto its tab.
   *
   * The rename mirror in ChatGroupsContext only listens to
   * `announceSessionName`, and BaseChat only announces on an explicit RENAME —
   * nothing announces when a session merely LOADS. So a tab opened from the
   * sidebar or a deep link kept its creation-time placeholder and sat there
   * reading "New Session" while document.title showed the real name. Caught by
   * driving the app; jsdom could not have shown it, because it needs a session
   * to actually load.
   */
  const handleSessionLoaded = useCallback(
    (session: { id: string; name: string; userSetName: boolean } | null) => {
      if (!session?.id || !session.name) return;
      dispatch?.({
        type: 'renameTab',
        sessionId: session.id,
        title: session.name,
        userSetName: session.userSetName,
      });
    },
    [dispatch]
  );

  if (!groups) return null;

  const renderGroup = (groupId: ChatGroupId) => {
    const group = groups.state.groups[groupId];
    if (!group) return <div key={groupId} />;
    const activeTab = group.tabs.find((t) => t.tabId === group.activeTabId);

    const strip = (
      <ChatTabStrip
        tabs={group.tabs}
        activeTabId={group.activeTabId}
        runningSessionIds={groups.runningSessionIds}
        onSelect={handleSelect}
        onPin={handlePin}
        onClose={handleClose}
        onReorder={handleReorder}
        reserveTitlebar={reserveTitlebarControls && groupId === reservedGroupId}
        isCompactSidebarOverlayOpen={isCompactSidebarOverlayOpen}
      />
    );

    return (
      <ChatTabHost
        key={groupId}
        // The tab is the chat's identity: keying on tabId (not sessionId) keeps
        // a tab's mount stable across a session bind.
        tabKey={activeTab?.tabId ?? `${groupId}-empty`}
        sessionId={activeTab?.sessionId ?? ''}
        initialMessage={activeTab?.pendingInitialMessage}
        initialAttachments={activeTab?.pendingInitialAttachments}
        renderSessionTitle={() => strip}
        onChatChange={onChatChange}
        onSessionLoaded={handleSessionLoaded}
      />
    );
  };

  return renderLayout(groups.state.layout, renderGroup);
}

interface ChatTabHostProps {
  tabKey: string;
  sessionId: string;
  initialMessage?: string;
  initialAttachments?: import('../../types/message').UserAttachment[];
  renderSessionTitle: () => ReactElement;
  onChatChange: (chat: ChatType) => void;
  onSessionLoaded: (session: { id: string; name: string; userSetName: boolean } | null) => void;
}

/**
 * Owns the chat state for one group's active tab (copied from ChatWindow's
 * pattern), so App's singleton stops being /pair's identity.
 */
function ChatTabHost({
  tabKey,
  sessionId,
  initialMessage,
  initialAttachments,
  renderSessionTitle,
  onChatChange,
  onSessionLoaded,
}: ChatTabHostProps) {
  return (
    <BaseChat
      key={tabKey}
      setChat={onChatChange}
      sessionId={sessionId}
      initialMessage={initialMessage}
      initialAttachments={initialAttachments}
      suppressEmptyState={false}
      renderSessionTitle={renderSessionTitle}
      onSessionUpdate={onSessionLoaded}
    />
  );
}

export default ChatGroupsShell;
