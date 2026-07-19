import { useEffect, useMemo, useSyncExternalStore } from 'react';
import { ChatState } from '../types/chatState';
import { Message, Session, TokenState } from '../api';
import { NotificationEvent, UserAttachment } from '../types/message';
import { useChatStreamController } from './chatStreamStore';
import type { ChatTurnErrorData } from '../types/turnError';

interface UseChatStreamProps {
  sessionId: string;
  onStreamFinish: () => void;
  onSessionLoaded?: () => void;
}

interface UseChatStreamReturn {
  session?: Session;
  messages: Message[];
  chatState: ChatState;
  setChatState: (state: ChatState) => void;
  handleSubmit: (userMessage: string, attachments?: UserAttachment[]) => Promise<void>;
  submitSystemMessage: (message: Message) => Promise<void>;
  submitElicitationResponse: (
    elicitationId: string,
    userData: Record<string, unknown>
  ) => Promise<void>;
  setWorkflowUserParams: (values: Record<string, string>) => Promise<void>;
  stopStreaming: () => void;
  /** BR-61: inject a message into the running turn without cancelling it. */
  steer: (text: string) => Promise<boolean>;
  sessionLoadError?: string;
  turnError?: ChatTurnErrorData;
  tokenState: TokenState;
  /** Client clock (ms) when the current turn was submitted; undefined while idle. */
  turnStartedAt?: number;
  /** Client clock (ms) when the most recent live Message event was applied. */
  lastMessageAt?: number;
  /**
   * Whether the session's model + extensions have finished loading. The
   * transcript is up well before this — anything reading AGENT state must gate
   * on it rather than on `session`.
   */
  agentReady: boolean;
  notifications: Map<string, NotificationEvent[]>;
  onMessageUpdate: (
    messageId: string,
    newContent: string,
    editType?: 'diverge' | 'edit'
  ) => Promise<void>;
}

export function useChatStream({
  sessionId,
  onStreamFinish,
  onSessionLoaded,
}: UseChatStreamProps): UseChatStreamReturn {
  const controller = useChatStreamController(sessionId);
  const snapshot = useSyncExternalStore(
    controller.subscribe,
    controller.getSnapshot,
    controller.getSnapshot
  );

  useEffect(() => {
    return controller.subscribeFinish(onStreamFinish);
  }, [controller, onStreamFinish]);

  useEffect(() => {
    void controller.loadSession(onSessionLoaded);
  }, [controller, onSessionLoaded]);

  const notificationsMap = useMemo(() => {
    return snapshot.notifications.reduce((map, notification) => {
      const key = notification.request_id;
      if (!map.has(key)) {
        map.set(key, []);
      }
      map.get(key)!.push(notification);
      return map;
    }, new Map<string, NotificationEvent[]>());
  }, [snapshot.notifications]);

  return {
    sessionLoadError: snapshot.sessionLoadError,
    turnError: snapshot.turnError,
    messages: snapshot.messages,
    session: snapshot.session,
    chatState: snapshot.chatState,
    setChatState: controller.setChatState,
    handleSubmit: controller.handleSubmit,
    submitSystemMessage: controller.submitSystemMessage,
    submitElicitationResponse: controller.submitElicitationResponse,
    stopStreaming: controller.stopStreaming,
    steer: controller.steer,
    setWorkflowUserParams: controller.setWorkflowUserParams,
    tokenState: snapshot.tokenState,
    turnStartedAt: snapshot.turnStartedAt,
    lastMessageAt: snapshot.lastMessageAt,
    agentReady: snapshot.agentReady,
    notifications: notificationsMap,
    onMessageUpdate: controller.onMessageUpdate,
  };
}
