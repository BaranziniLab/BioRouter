import React, { createContext, useContext, ReactNode } from 'react';
import { ChatType } from '../types/chat';
import { Workflow } from '../workflow';

// TODO(Douwe): We should not need this anymore
export const DEFAULT_CHAT_TITLE = 'New Chat';

interface ChatContextType {
  chat: ChatType;
  setChat: (chat: ChatType) => void;
  resetChat: () => void;
  hasActiveSession: boolean;
  setWorkflow: (workflow: Workflow | null) => void;
  clearWorkflow: () => void;
  // Context identification
  contextKey: string; // 'hub' or 'pair-{sessionId}'
}

const ChatContext = createContext<ChatContextType | undefined>(undefined);

interface ChatProviderProps {
  children: ReactNode;
  chat: ChatType;
  setChat: (chat: ChatType) => void;
  contextKey?: string; // Optional context key, defaults to 'hub'
}

export const ChatProvider: React.FC<ChatProviderProps> = ({
  children,
  chat,
  setChat,
  contextKey = 'hub',
}) => {
  const resetChat = () => {
    setChat({
      sessionId: '',
      name: DEFAULT_CHAT_TITLE,
      messages: [],
      workflow: null,
      workflowParameterValues: null,
    });
  };

  const setWorkflow = (workflow: Workflow | null) => {
    setChat({
      ...chat,
      workflow: workflow,
      workflowParameterValues: null,
    });
  };

  const clearWorkflow = () => {
    setChat({
      ...chat,
      workflow: null,
    });
  };

  const hasActiveSession = chat.messages.length > 0;

  const value: ChatContextType = {
    chat,
    setChat,
    resetChat,
    hasActiveSession,
    setWorkflow,
    clearWorkflow,
    contextKey,
  };

  return <ChatContext.Provider value={value}>{children}</ChatContext.Provider>;
};

export const useChatContext = (): ChatContextType | null => {
  const context = useContext(ChatContext);
  return context || null;
};
