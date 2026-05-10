import React, { useState } from 'react';
import BaseChat from '../BaseChat';
import { ChatProvider, DEFAULT_CHAT_TITLE } from '../../contexts/ChatContext';
import { ChatType } from '../../types/chat';
import { DashboardWindow } from '../../contexts/DashboardContext';
import { useDashboard } from '../../contexts/DashboardContext';
import { updateSessionName } from '../../api';

interface Props {
  win: DashboardWindow;
}

/**
 * Mounts a `BaseChat` for a tucked window so its `useChatStream` keeps running
 * (the AI agent should not halt just because the user tucked the window away).
 * The element is wrapped by the parent in a `display: none` container, which
 * keeps React state & effects alive but skips visual paint.
 */
export const HiddenChatHolder: React.FC<Props> = ({ win }) => {
  const dashboard = useDashboard();
  const [chat, setChat] = useState<ChatType>({
    sessionId: win.sessionId,
    name: win.name || DEFAULT_CHAT_TITLE,
    messages: [],
    workflow: null,
    workflowParameterValues: null,
  });

  return (
    <ChatProvider chat={chat} setChat={setChat} contextKey={`dashboard-tucked-${win.sessionId}`}>
      <BaseChat
        setChat={setChat}
        sessionId={win.sessionId}
        suppressEmptyState
        coherent
        hideSessionNamePill
        accentColor={win.accentColor}
        onRenameSession={(newName) => {
          dashboard.renameWindow(win.windowId, newName);
          void updateSessionName({
            path: { session_id: win.sessionId },
            body: { name: newName },
          });
        }}
        onSessionUpdate={(s) => {
          if (s?.name) {
            dashboard.syncSessionName(win.windowId, s.name);
            dashboard.markActivity(win.windowId);
          }
        }}
      />
    </ChatProvider>
  );
};
