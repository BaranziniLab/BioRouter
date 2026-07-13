import React, { createContext, useContext, useSyncExternalStore } from 'react';
import { ChatState } from '../types/chatState';
import {
  getSession,
  interrupt,
  listApps,
  listSessions,
  Message,
  MessageEvent,
  reply,
  resumeAgent,
  Session,
  TokenState,
  updateFromSession,
  updateSessionUserWorkflowValues,
} from '../api';
import { client } from '../api/client.gen';
import {
  announceSessionName,
  cacheGet,
  cacheSet,
  isDefaultSessionName,
  renameSession,
  subscribeSessionNameChanges,
} from '../utils/sessionNameSync';
import {
  createElicitationResponseMessage,
  createUserMessage,
  getCompactingMessage,
  getThinkingMessage,
  NotificationEvent,
  UserAttachment,
} from '../types/message';
import { getToolResponses } from '../types/message';
import { errorMessage } from '../utils/conversionUtils';
import { showExtensionLoadResults } from '../utils/extensionErrorUtils';

const openedAppUrls = new Set<string>();

function autoOpenLaunchedApps(msg: Message): void {
  for (const tr of getToolResponses(msg)) {
    const result = tr.toolResult as {
      status?: string;
      value?: { _meta?: Record<string, unknown> };
    };
    if (result?.status !== 'success') continue;
    const path = result.value?._meta?.['biorouter/app-path'];
    if (typeof path !== 'string' || !path.startsWith('/apps/')) continue;
    const base = ((client.getConfig().baseUrl as string) || '').replace(/\/+$/, '');
    const url = base + path;
    if (!base || openedAppUrls.has(url)) continue;
    openedAppUrls.add(url);
    window.electron?.openExternal(url).catch((e) => console.error('auto-open app failed:', e));
  }
}

const SESSION_LIST_CACHE_TTL_MS = 5000;
let sessionListInflight: Promise<{ id: string; name?: string | null }[]> | null = null;
let sessionListInflightAt = 0;

async function fetchAllSessions(): Promise<{ id: string; name?: string | null }[]> {
  const now = Date.now();
  if (sessionListInflight && now - sessionListInflightAt < SESSION_LIST_CACHE_TTL_MS) {
    return sessionListInflight;
  }
  sessionListInflightAt = now;
  sessionListInflight = (async () => {
    const response = await listSessions({ throwOnError: true });
    return (response.data?.sessions ?? []) as { id: string; name?: string | null }[];
  })();
  sessionListInflight.catch(() => {
    sessionListInflight = null;
  });
  return sessionListInflight;
}

async function disambiguateSessionName(
  proposed: string,
  currentSessionId: string
): Promise<string> {
  let existingNames: Set<string>;
  try {
    const sessions = await fetchAllSessions();
    existingNames = new Set(
      sessions
        .filter((s) => s.id !== currentSessionId)
        .map((s) => s.name)
        .filter((n): n is string => typeof n === 'string')
    );
  } catch (e) {
    console.warn('disambiguateSessionName: failed to list sessions:', e);
    return proposed;
  }
  if (!existingNames.has(proposed)) return proposed;
  for (let n = 2; n < 1000; n++) {
    const candidate = `${proposed} ${n}`;
    if (!existingNames.has(candidate)) return candidate;
  }
  return proposed;
}

function sameContent(a: Message, b: Message): boolean {
  return a.role === b.role && JSON.stringify(a.content) === JSON.stringify(b.content);
}

function pushMessage(currentMessages: Message[], incomingMsg: Message): Message[] {
  const lastMsg = currentMessages[currentMessages.length - 1];

  if (lastMsg?.id && lastMsg.id === incomingMsg.id) {
    const updatedLastMsg = {
      ...lastMsg,
      content: [...lastMsg.content],
    };
    const lastContent = lastMsg.content[lastMsg.content.length - 1];
    const newContent = incomingMsg.content[incomingMsg.content.length - 1];

    if (
      lastContent?.type === 'text' &&
      newContent?.type === 'text' &&
      incomingMsg.content.length === 1
    ) {
      const updatedLastContent = { ...lastContent };
      if (newContent.text.startsWith(updatedLastContent.text)) {
        updatedLastContent.text = newContent.text;
      } else if (!updatedLastContent.text.endsWith(newContent.text)) {
        updatedLastContent.text += newContent.text;
      }
      updatedLastMsg.content[updatedLastMsg.content.length - 1] = updatedLastContent;
    } else {
      const existingContent = new Set(
        updatedLastMsg.content.map((content) => JSON.stringify(content))
      );
      updatedLastMsg.content.push(
        ...incomingMsg.content.filter((content) => !existingContent.has(JSON.stringify(content)))
      );
    }
    return [...currentMessages.slice(0, -1), updatedLastMsg];
  }

  if (lastMsg && sameContent(lastMsg, incomingMsg)) {
    return currentMessages;
  }

  return [...currentMessages, incomingMsg];
}

export interface ChatStreamSnapshot {
  session?: Session;
  messages: Message[];
  chatState: ChatState;
  sessionLoadError?: string;
  tokenState: TokenState;
  notifications: NotificationEvent[];
}

export interface RunningChatEntry {
  sessionId: string;
  title: string;
  chatState: ChatState;
  startedAt: number;
  completedAt?: number;
}

const EMPTY_TOKEN_STATE: TokenState = {
  inputTokens: 0,
  outputTokens: 0,
  totalTokens: 0,
  accumulatedInputTokens: 0,
  accumulatedOutputTokens: 0,
  accumulatedTotalTokens: 0,
};

function isRunningState(chatState: ChatState): boolean {
  return chatState !== ChatState.Idle && chatState !== ChatState.LoadingConversation;
}

class ChatStreamController {
  private snapshot: ChatStreamSnapshot = {
    messages: [],
    chatState: ChatState.Idle,
    tokenState: EMPTY_TOKEN_STATE,
    notifications: [],
  };
  private listeners = new Set<() => void>();
  private finishListeners = new Set<() => void>();
  private messagesRef: Message[] = [];
  private abortController: AbortController | null = null;
  private activeStreamId = 0;
  private lastInteractionTime = Date.now();
  private loadPromise: Promise<void> | null = null;
  private lastSubmittedTitle: string | null = null;

  constructor(
    readonly sessionId: string,
    private readonly onActivityChange: (controller: ChatStreamController) => void
  ) {
    subscribeSessionNameChanges((change) => {
      if (change.sessionId !== sessionId) return;
      this.updateSnapshot((prev) => {
        if (!prev.session) return prev;
        if (
          prev.session.name === change.name &&
          prev.session.user_set_name === change.userSetName
        ) {
          return prev;
        }
        return {
          ...prev,
          session: { ...prev.session, name: change.name, user_set_name: change.userSetName },
        };
      });
    });
  }

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  subscribeFinish(listener: () => void): () => void {
    this.finishListeners.add(listener);
    return () => {
      this.finishListeners.delete(listener);
    };
  }

  getSnapshot = (): ChatStreamSnapshot => this.snapshot;

  isRunning(): boolean {
    return isRunningState(this.snapshot.chatState);
  }

  getRunningEntry(): RunningChatEntry {
    return {
      sessionId: this.sessionId,
      title: this.snapshot.session?.name || this.lastSubmittedTitle || 'New Session',
      chatState: this.snapshot.chatState,
      startedAt: this.lastInteractionTime,
    };
  }

  setChatState = (chatState: ChatState): void => {
    this.updateSnapshot((prev) => ({ ...prev, chatState }));
  };

  private notify(): void {
    for (const listener of this.listeners) listener();
    this.onActivityChange(this);
  }

  private updateSnapshot(updater: (prev: ChatStreamSnapshot) => ChatStreamSnapshot): void {
    this.snapshot = updater(this.snapshot);
    if (this.snapshot.session) {
      cacheSet(this.sessionId, {
        session: this.snapshot.session,
        messages: this.snapshot.messages,
      });
    }
    this.notify();
  }

  private updateMessages = (messages: Message[]): void => {
    this.messagesRef = messages;
    this.updateSnapshot((prev) => ({ ...prev, messages }));
  };

  private updateTokenState = (tokenState: TokenState): void => {
    this.updateSnapshot((prev) => ({ ...prev, tokenState }));
  };

  private updateNotifications = (notification: NotificationEvent): void => {
    this.updateSnapshot((prev) => ({
      ...prev,
      notifications: [...prev.notifications, notification],
    }));
  };

  async loadSession(onSessionLoaded?: () => void): Promise<void> {
    if (!this.sessionId) return;

    if (this.snapshot.session) {
      onSessionLoaded?.();
      return;
    }

    const cached = cacheGet(this.sessionId);
    if (cached) {
      this.messagesRef = cached.messages;
      this.updateSnapshot((prev) => ({
        ...prev,
        session: cached.session,
        messages: cached.messages,
        tokenState: {
          inputTokens: cached.session?.input_tokens ?? 0,
          outputTokens: cached.session?.output_tokens ?? 0,
          totalTokens: cached.session?.total_tokens ?? 0,
          accumulatedInputTokens: cached.session?.accumulated_input_tokens ?? 0,
          accumulatedOutputTokens: cached.session?.accumulated_output_tokens ?? 0,
          accumulatedTotalTokens: cached.session?.accumulated_total_tokens ?? 0,
        },
        chatState: this.isRunning() ? prev.chatState : ChatState.Idle,
      }));
      onSessionLoaded?.();
      return;
    }

    if (!this.loadPromise) {
      this.updateSnapshot((prev) => ({
        ...prev,
        messages: [],
        session: undefined,
        sessionLoadError: undefined,
        chatState: ChatState.LoadingConversation,
      }));

      this.loadPromise = (async () => {
        try {
          const response = await resumeAgent({
            body: {
              session_id: this.sessionId,
              load_model_and_extensions: true,
            },
            throwOnError: true,
          });
          const resumeData = response.data;
          const loadedSession = resumeData?.session;
          const extensionResults = resumeData?.extension_results;

          showExtensionLoadResults(extensionResults);
          this.messagesRef = loadedSession?.conversation || [];
          this.updateSnapshot((prev) => ({
            ...prev,
            session: loadedSession,
            messages: this.messagesRef,
            tokenState: {
              inputTokens: loadedSession?.input_tokens ?? 0,
              outputTokens: loadedSession?.output_tokens ?? 0,
              totalTokens: loadedSession?.total_tokens ?? 0,
              accumulatedInputTokens: loadedSession?.accumulated_input_tokens ?? 0,
              accumulatedOutputTokens: loadedSession?.accumulated_output_tokens ?? 0,
              accumulatedTotalTokens: loadedSession?.accumulated_total_tokens ?? 0,
            },
            chatState: this.abortController ? prev.chatState : ChatState.Idle,
          }));

          listApps({
            throwOnError: true,
            query: { session_id: this.sessionId },
          }).catch((err) => {
            console.warn('Failed to populate apps cache:', err);
          });

          if (loadedSession) {
            updateFromSession({
              body: {
                session_id: loadedSession.id,
              },
              throwOnError: true,
            }).catch((err) => console.warn('Failed to update agent from session:', err));
          }
        } catch (error) {
          this.updateSnapshot((prev) => ({
            ...prev,
            sessionLoadError: errorMessage(error),
            chatState: ChatState.Idle,
          }));
        } finally {
          this.loadPromise = null;
        }
      })();
    }

    await this.loadPromise;
    onSessionLoaded?.();
  }

  private finishCurrentStream = async (error?: string): Promise<void> => {
    if (error) {
      this.updateSnapshot((prev) => ({ ...prev, sessionLoadError: error }));
    }
    this.abortController = null;

    const timeSinceLastInteraction = Date.now() - this.lastInteractionTime;
    if (!error && timeSinceLastInteraction > 60000) {
      window.electron?.showNotification({
        title: 'biorouter finished the task.',
        body: 'Click here to expand.',
      });
    }

    const isNewSession = this.sessionId && this.sessionId.match(/^\d{8}_\d{6}$/);
    if (isNewSession) {
      window.dispatchEvent(new CustomEvent('message-stream-finished'));
    }

    if (!error && this.sessionId) {
      const userMessageCount = this.messagesRef.filter(
        (m) => m.role === 'user' && m.metadata?.userVisible !== false
      ).length;
      if (userMessageCount <= 3) {
        const pollDelays = [800, 1200, 2000, 3000, 4000, 6000, 8000, 10000];
        void (async () => {
          for (const delay of pollDelays) {
            await new Promise((r) => setTimeout(r, delay));
            try {
              const response = await getSession({
                path: { session_id: this.sessionId },
                throwOnError: true,
              });
              const data = response.data;
              if (!data) continue;
              const proposedName = data.name;
              if (data.user_set_name) break;
              if (proposedName && !isDefaultSessionName(proposedName)) {
                const uniqueName = await disambiguateSessionName(proposedName, this.sessionId);
                if (uniqueName !== proposedName) {
                  try {
                    await renameSession(this.sessionId, uniqueName, 'llm');
                  } catch (renameError) {
                    console.warn('Failed to persist disambiguated session name:', renameError);
                  }
                } else {
                  announceSessionName({
                    sessionId: this.sessionId,
                    name: uniqueName,
                    userSetName: false,
                    origin: 'llm',
                  });
                }
                this.updateSnapshot((prev) =>
                  prev.session && prev.session.name !== uniqueName
                    ? { ...prev, session: { ...prev.session, name: uniqueName } }
                    : prev
                );
                break;
              }
            } catch (refreshError) {
              console.warn('Failed to refresh session name:', refreshError);
            }
          }
        })();
      }
    }

    this.updateSnapshot((prev) => ({ ...prev, chatState: ChatState.Idle }));
    for (const listener of this.finishListeners) listener();
  };

  private async streamFromResponse(
    stream: AsyncIterable<MessageEvent>,
    initialMessages: Message[],
    streamId: number
  ): Promise<void> {
    let currentMessages = initialMessages;

    try {
      for await (const event of stream) {
        if (this.activeStreamId !== streamId) return;
        switch (event.type) {
          case 'Message': {
            const msg = event.message;
            currentMessages = pushMessage(currentMessages, msg);
            autoOpenLaunchedApps(msg);

            const hasToolConfirmation = msg.content.some(
              (content) => content.type === 'toolConfirmationRequest'
            );
            const hasElicitation = msg.content.some(
              (content) =>
                content.type === 'actionRequired' && content.data.actionType === 'elicitation'
            );

            if (hasToolConfirmation || hasElicitation) {
              this.setChatState(ChatState.WaitingForUserInput);
            } else if (getCompactingMessage(msg)) {
              this.setChatState(ChatState.Compacting);
            } else if (getThinkingMessage(msg)) {
              this.setChatState(ChatState.Thinking);
            } else {
              this.setChatState(ChatState.Streaming);
            }

            this.updateTokenState(event.token_state);
            this.updateMessages(currentMessages);
            break;
          }
          case 'Error':
            await this.finishCurrentStream('Stream error: ' + event.error);
            return;
          case 'Finish':
            this.updateTokenState(event.token_state);
            await this.finishCurrentStream();
            return;
          case 'ModelChange':
          case 'Ping':
            break;
          case 'UpdateConversation':
            currentMessages = event.conversation;
            this.updateMessages(event.conversation);
            this.updateTokenState(event.token_state);
            break;
          case 'Notification':
            this.updateNotifications(event as NotificationEvent);
            break;
        }
      }

      await this.finishCurrentStream();
    } catch (error) {
      if (error instanceof Error && error.name !== 'AbortError') {
        await this.finishCurrentStream('Stream error: ' + errorMessage(error));
      }
    }
  }

  private submitPreparedMessage = async (
    newMessage: Message,
    currentMessages: Message[],
    updateMessageList: boolean
  ): Promise<void> => {
    if (updateMessageList) {
      this.updateMessages(currentMessages);
    }

    this.updateSnapshot((prev) => ({
      ...prev,
      chatState: ChatState.Streaming,
      notifications: [],
    }));
    this.abortController = new AbortController();
    const streamId = this.activeStreamId + 1;
    this.activeStreamId = streamId;

    try {
      const { stream } = await reply({
        body: {
          session_id: this.sessionId,
          user_message: newMessage,
        },
        throwOnError: true,
        signal: this.abortController.signal,
        sseMaxRetryAttempts: 1,
      });

      await this.streamFromResponse(stream, currentMessages, streamId);
    } catch (error) {
      if (error instanceof Error && error.name === 'AbortError') {
        return;
      }
      await this.finishCurrentStream('Submit error: ' + errorMessage(error));
    } finally {
      if (this.activeStreamId === streamId && this.abortController?.signal.aborted) {
        this.abortController = null;
      }
    }
  };

  private canSubmitMessage(): boolean {
    return (
      !!this.snapshot.session &&
      this.snapshot.chatState !== ChatState.LoadingConversation &&
      !(this.abortController && !this.abortController.signal.aborted)
    );
  }

  submitSystemMessage = async (message: Message): Promise<void> => {
    await this.loadSession();

    if (!this.canSubmitMessage()) {
      return;
    }

    this.lastInteractionTime = Date.now();
    const currentMessages = [...this.messagesRef, message];
    await this.submitPreparedMessage(message, currentMessages, true);
  };

  handleSubmit = async (userMessage: string, attachments: UserAttachment[] = []): Promise<void> => {
    await this.loadSession();

    if (!this.canSubmitMessage()) {
      return;
    }

    const hasExistingMessages = this.messagesRef.length > 0;
    const hasNewMessage = userMessage.trim().length > 0 || attachments.length > 0;
    if (!hasNewMessage && !hasExistingMessages) {
      return;
    }

    this.lastInteractionTime = Date.now();
    if (userMessage.trim().length > 0) {
      this.lastSubmittedTitle = userMessage.trim().slice(0, 80);
    } else if (attachments.length > 0) {
      this.lastSubmittedTitle = `${attachments.length} attachment${attachments.length === 1 ? '' : 's'}`;
    }

    if (!hasExistingMessages && hasNewMessage) {
      window.dispatchEvent(new CustomEvent('session-created'));
    }

    let newMessage: Message;
    if (hasNewMessage) {
      try {
        newMessage = await createUserMessage(userMessage, attachments);
      } catch (error) {
        await this.finishCurrentStream('Submit error: ' + errorMessage(error));
        return;
      }
    } else {
      newMessage = this.messagesRef[this.messagesRef.length - 1];
    }

    const currentMessages = hasNewMessage
      ? [...this.messagesRef, newMessage]
      : [...this.messagesRef];
    await this.submitPreparedMessage(newMessage, currentMessages, hasNewMessage);
  };

  submitElicitationResponse = async (
    elicitationId: string,
    userData: Record<string, unknown>
  ): Promise<void> => {
    await this.loadSession();

    if (!this.canSubmitMessage()) {
      return;
    }

    this.lastInteractionTime = Date.now();
    const responseMessage = createElicitationResponseMessage(elicitationId, userData);
    const currentMessages = [...this.messagesRef, responseMessage];

    await this.submitPreparedMessage(responseMessage, currentMessages, true);
  };

  setWorkflowUserParams = async (user_workflow_values: Record<string, string>): Promise<void> => {
    if (this.snapshot.session) {
      await updateSessionUserWorkflowValues({
        path: {
          session_id: this.sessionId,
        },
        body: {
          userWorkflowValues: user_workflow_values,
        },
        throwOnError: true,
      });
      this.updateSnapshot((prev) =>
        prev.session
          ? {
              ...prev,
              session: {
                ...prev.session,
                user_workflow_values,
              },
            }
          : prev
      );
    } else {
      this.updateSnapshot((prev) => ({
        ...prev,
        sessionLoadError: "can't call setWorkflowParams without a session",
      }));
    }
  };

  /**
   * BR-61 — soft interrupt ("steer"). Injects `text` into the turn that is
   * *already running*, at the agent's next loop boundary, instead of cancelling
   * it and re-sending the whole context: in-flight tool work is kept and the
   * model simply sees the new instruction on its next step.
   *
   * Resolves `false` when there is nothing to steer (no turn in flight, empty
   * text) or the server rejected the interrupt — callers must then fall back to
   * sending the text as an ordinary message, so it is never silently dropped.
   *
   * The injected message is NOT pushed locally: the agent streams it back as a
   * normal user message once it is consumed, which is also the only reliable
   * signal that it landed.
   */
  steer = async (text: string): Promise<boolean> => {
    const trimmed = text.trim();
    if (!trimmed || !this.isRunning()) {
      return false;
    }
    try {
      await interrupt({
        body: { session_id: this.sessionId, text: trimmed },
        throwOnError: true,
      });
      this.lastInteractionTime = Date.now();
      return true;
    } catch (error) {
      // 409 = the turn ended between the click and the POST; the caller queues
      // or sends it instead.
      console.warn('Soft interrupt rejected, falling back to a normal send:', error);
      return false;
    }
  };

  stopStreaming = (): void => {
    this.activeStreamId += 1;
    this.abortController?.abort();
    this.updateSnapshot((prev) => ({ ...prev, chatState: ChatState.Idle }));
    this.lastInteractionTime = Date.now();
  };

  onMessageUpdate = async (
    messageId: string,
    newContent: string,
    editType: 'fork' | 'edit' = 'fork'
  ): Promise<void> => {
    try {
      const { editMessage } = await import('../api');
      const message = this.messagesRef.find((m) => m.id === messageId);

      if (!message) {
        throw new Error(`Message with id ${messageId} not found in current messages`);
      }

      const response = await editMessage({
        path: {
          session_id: this.sessionId,
        },
        body: {
          timestamp: message.created,
          editType,
        },
        throwOnError: true,
      });

      const targetSessionId = response.data?.sessionId;
      if (!targetSessionId) {
        throw new Error('No session ID returned from edit_message');
      }

      if (editType === 'fork') {
        const event = new CustomEvent('session-forked', {
          detail: {
            newSessionId: targetSessionId,
            shouldStartAgent: true,
            editedMessage: newContent,
          },
        });
        window.dispatchEvent(event);
        window.electron?.logInfo(`Dispatched session-forked event for session ${targetSessionId}`);
      } else {
        const sessionResponse = await getSession({
          path: { session_id: targetSessionId },
          throwOnError: true,
        });

        if (sessionResponse.data?.conversation) {
          this.updateMessages(sessionResponse.data.conversation);
        }
        await this.handleSubmit(newContent);
      }
    } catch (error) {
      const errorMsg = errorMessage(error);
      console.error('Failed to edit message:', error);
      const { toastError } = await import('../toasts');
      toastError({
        title: 'Failed to edit message',
        msg: errorMsg,
      });
    }
  };
}

export class ChatStreamRegistry {
  private controllers = new Map<string, ChatStreamController>();
  private runningListeners = new Set<() => void>();
  private running = new Map<string, RunningChatEntry>();
  private lastRunningSnapshot: RunningChatEntry[] = [];

  getController(sessionId: string): ChatStreamController {
    let controller = this.controllers.get(sessionId);
    if (!controller) {
      controller = new ChatStreamController(sessionId, this.handleControllerActivity);
      this.controllers.set(sessionId, controller);
    }
    return controller;
  }

  isSessionRunning(sessionId: string): boolean {
    return this.controllers.get(sessionId)?.isRunning() ?? false;
  }

  subscribeRunning = (listener: () => void): (() => void) => {
    this.runningListeners.add(listener);
    return () => {
      this.runningListeners.delete(listener);
    };
  };

  getRunningSnapshot = (): RunningChatEntry[] => this.lastRunningSnapshot;

  resetForTests(): void {
    this.controllers.clear();
    this.running.clear();
    this.lastRunningSnapshot = [];
  }

  private handleControllerActivity = (controller: ChatStreamController): void => {
    const current = this.running.get(controller.sessionId);
    if (controller.isRunning()) {
      this.running.set(controller.sessionId, controller.getRunningEntry());
    } else if (current && !current.completedAt) {
      this.running.set(controller.sessionId, {
        ...current,
        chatState: ChatState.Idle,
        completedAt: Date.now(),
      });
      window.setTimeout(() => {
        const entry = this.running.get(controller.sessionId);
        if (entry?.completedAt && !controller.isRunning()) {
          this.running.delete(controller.sessionId);
          this.emitRunning();
        }
      }, 1600);
    }
    this.emitRunning();
  };

  private emitRunning(): void {
    this.lastRunningSnapshot = Array.from(this.running.values()).sort(
      (a, b) => b.startedAt - a.startedAt
    );
    for (const listener of this.runningListeners) listener();
  }
}

export const defaultChatStreamRegistry = new ChatStreamRegistry();

const ChatStreamRegistryContext = createContext<ChatStreamRegistry>(defaultChatStreamRegistry);

export function ChatStreamProvider({ children }: { children: React.ReactNode }) {
  return (
    <ChatStreamRegistryContext.Provider value={defaultChatStreamRegistry}>
      {children}
    </ChatStreamRegistryContext.Provider>
  );
}

export function useChatStreamRegistry(): ChatStreamRegistry {
  return useContext(ChatStreamRegistryContext);
}

export function useRunningChats(): RunningChatEntry[] {
  const registry = useChatStreamRegistry();
  return useSyncExternalStore(
    registry.subscribeRunning,
    registry.getRunningSnapshot,
    registry.getRunningSnapshot
  );
}

export function useChatStreamController(sessionId: string): ChatStreamController {
  const registry = useChatStreamRegistry();
  return registry.getController(sessionId);
}
