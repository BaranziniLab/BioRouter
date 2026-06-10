import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ChatState } from '../types/chatState';

import {
  getSession,
  listSessions,
  Message,
  MessageEvent,
  reply,
  resumeAgent,
  Session,
  TokenState,
  updateFromSession,
  updateSessionUserWorkflowValues,
  listApps,
} from '../api';
import {
  announceSessionName,
  cacheGet,
  cacheSet,
  isDefaultSessionName,
  renameSession,
  subscribeSessionNameChanges,
} from '../utils/sessionNameSync';

import {
  createUserMessage,
  createElicitationResponseMessage,
  getCompactingMessage,
  getThinkingMessage,
  NotificationEvent,
  UserAttachment,
} from '../types/message';
import { errorMessage } from '../utils/conversionUtils';
import { showExtensionLoadResults } from '../utils/extensionErrorUtils';

// Cache lives in utils/sessionNameSync.ts so that renames from any
// component update every reader's view consistently.

// Coalesces concurrent listSessions calls within a short window. The
// dashboard mounts N ChatWindows in parallel; each polls listSessions up to
// 4 times after the user's first ~3 replies — without coalescing that's 4N
// round-trips against biorouterd in a few seconds. Sharing the in-flight
// Promise (and caching its result briefly) collapses those to one request
// per burst.
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
    // Drop the cached promise on failure so the next caller retries.
    sessionListInflight = null;
  });
  return sessionListInflight;
}

/** If `proposed` collides with any *other* session's name in history, return
 * the first available "<proposed> N" variant (N ≥ 2). Otherwise return
 * `proposed` unchanged. The current session is excluded from the collision
 * set so a session that already had the name doesn't get renamed away.
 *
 * Failure mode: if the sessions list can't be fetched, return the proposed
 * name as-is. Better to occasionally allow a duplicate than to drop the
 * LLM-assigned name on the floor. */
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
  submitElicitationResponse: (
    elicitationId: string,
    userData: Record<string, unknown>
  ) => Promise<void>;
  setWorkflowUserParams: (values: Record<string, string>) => Promise<void>;
  stopStreaming: () => void;
  sessionLoadError?: string;
  tokenState: TokenState;
  notifications: Map<string, NotificationEvent[]>;
  onMessageUpdate: (
    messageId: string,
    newContent: string,
    editType?: 'fork' | 'edit'
  ) => Promise<void>;
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
      const existingContent = new Set(updatedLastMsg.content.map((content) => JSON.stringify(content)));
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

async function streamFromResponse(
  stream: AsyncIterable<MessageEvent>,
  initialMessages: Message[],
  updateMessages: (messages: Message[]) => void,
  updateTokenState: (tokenState: TokenState) => void,
  updateChatState: (state: ChatState) => void,
  updateNotifications: (notification: NotificationEvent) => void,
  onFinish: (error?: string) => void
): Promise<void> {
  let currentMessages = initialMessages;

  try {
    for await (const event of stream) {
      switch (event.type) {
        case 'Message': {
          const msg = event.message;
          currentMessages = pushMessage(currentMessages, msg);

          const hasToolConfirmation = msg.content.some(
            (content) => content.type === 'toolConfirmationRequest'
          );

          const hasElicitation = msg.content.some(
            (content) =>
              content.type === 'actionRequired' && content.data.actionType === 'elicitation'
          );

          if (hasToolConfirmation || hasElicitation) {
            updateChatState(ChatState.WaitingForUserInput);
          } else if (getCompactingMessage(msg)) {
            updateChatState(ChatState.Compacting);
          } else if (getThinkingMessage(msg)) {
            updateChatState(ChatState.Thinking);
          } else {
            updateChatState(ChatState.Streaming);
          }

          updateTokenState(event.token_state);
          updateMessages(currentMessages);
          break;
        }
        case 'Error': {
          onFinish('Stream error: ' + event.error);
          return;
        }
        case 'Finish': {
          // The server attaches the authoritative end-of-stream token_state.
          // Without applying it, manual `/compact` (whose only post-compaction
          // event is HistoryReplaced -> UpdateConversation -> Finish) leaves
          // the UI gauge stuck at the pre-compact value.
          updateTokenState(event.token_state);
          onFinish();
          return;
        }
        case 'ModelChange': {
          break;
        }
        case 'UpdateConversation': {
          // WARNING: Since Message handler uses this local variable, we need to update it here to avoid the client clobbering it.
          // Longterm fix is to only send the agent the new messages, not the entire conversation.
          currentMessages = event.conversation;
          updateMessages(event.conversation);
          updateTokenState(event.token_state);
          break;
        }
        case 'Notification': {
          updateNotifications(event as NotificationEvent);
          break;
        }
        case 'Ping':
          break;
      }
    }

    onFinish();
  } catch (error) {
    if (error instanceof Error && error.name !== 'AbortError') {
      onFinish('Stream error: ' + errorMessage(error));
    }
  }
}

export function useChatStream({
  sessionId,
  onStreamFinish,
  onSessionLoaded,
}: UseChatStreamProps): UseChatStreamReturn {
  const [messages, setMessages] = useState<Message[]>([]);
  const messagesRef = useRef<Message[]>([]);
  const [session, setSession] = useState<Session>();
  const [sessionLoadError, setSessionLoadError] = useState<string>();
  const [chatState, setChatState] = useState<ChatState>(ChatState.Idle);
  const [tokenState, setTokenState] = useState<TokenState>({
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    accumulatedInputTokens: 0,
    accumulatedOutputTokens: 0,
    accumulatedTotalTokens: 0,
  });
  const [notifications, setNotifications] = useState<NotificationEvent[]>([]);
  const abortControllerRef = useRef<AbortController | null>(null);
  const activeStreamIdRef = useRef(0);
  const lastInteractionTimeRef = useRef<number>(Date.now());

  useEffect(() => {
    if (session) {
      cacheSet(sessionId, { session, messages });
    }
  }, [sessionId, session, messages]);

  // Pick up rename events from sibling windows / routes (BaseChat pill,
  // dashboard window titlebar, LLM auto-rename). Without this, e.g. a
  // dashboard window renames the session and the history list, still
  // mounted in another route, keeps showing the stale name until refresh.
  useEffect(() => {
    if (!sessionId) return;
    return subscribeSessionNameChanges((change) => {
      if (change.sessionId !== sessionId) return;
      setSession((prev) => {
        if (!prev) return prev;
        if (prev.name === change.name && prev.user_set_name === change.userSetName) {
          return prev;
        }
        return { ...prev, name: change.name, user_set_name: change.userSetName };
      });
    });
  }, [sessionId]);

  const updateMessages = useCallback((newMessages: Message[]) => {
    setMessages(newMessages);
    messagesRef.current = newMessages;
  }, []);

  const updateNotifications = useCallback((notification: NotificationEvent) => {
    setNotifications((prev) => [...prev, notification]);
  }, []);

  const onFinish = useCallback(
    async (error?: string): Promise<void> => {
      if (error) {
        setSessionLoadError(error);
      }
      abortControllerRef.current = null;

      const timeSinceLastInteraction = Date.now() - lastInteractionTimeRef.current;
      if (!error && timeSinceLastInteraction > 60000) {
        window.electron.showNotification({
          title: 'biorouter finished the task.',
          body: 'Click here to expand.',
        });
      }

      const isNewSession = sessionId && sessionId.match(/^\d{8}_\d{6}$/);
      if (isNewSession) {
        console.log(
          'useChatStream: Message stream finished for new session, emitting message-stream-finished event'
        );
        window.dispatchEvent(new CustomEvent('message-stream-finished'));
      }

      // Refresh session name after each reply for the first 3 user messages.
      // The backend regenerates the name (in a fire-and-forget tokio::spawn —
      // see agents/agent.rs:maybe_update_name) and we don't know exactly when
      // it finishes, so poll a few times with backoff. Two early-exits:
      // (1) backend tells us the user already renamed (`user_set_name=true`)
      //     → stop polling, don't disturb the user's name.
      // (2) name is non-default → persist any disambiguation and announce.
      if (!error && sessionId) {
        const userMessageCount = messagesRef.current.filter(
          (m) => m.role === 'user'
        ).length;
        if (userMessageCount <= 3) {
          const pollDelays = [800, 1200, 2000, 3000];
          void (async () => {
            for (const delay of pollDelays) {
              await new Promise((r) => setTimeout(r, delay));
              try {
                const response = await getSession({
                  path: { session_id: sessionId },
                  throwOnError: true,
                });
                const data = response.data;
                if (!data) continue;
                const proposedName = data.name;
                if (data.user_set_name) {
                  // User already renamed via the UI — stop racing them.
                  break;
                }
                if (proposedName && !isDefaultSessionName(proposedName)) {
                  // Backend's LLM-generated name. Disambiguate against
                  // history, persist if changed, broadcast to siblings.
                  const uniqueName = await disambiguateSessionName(
                    proposedName,
                    sessionId
                  );
                  if (uniqueName !== proposedName) {
                    try {
                      await renameSession(sessionId, uniqueName, 'llm');
                    } catch (renameError) {
                      console.warn(
                        'Failed to persist disambiguated session name:',
                        renameError
                      );
                      // Fall through with the un-disambiguated name; better
                      // than dropping the LLM rename altogether.
                    }
                  } else {
                    announceSessionName({
                      sessionId,
                      name: uniqueName,
                      userSetName: false,
                      origin: 'llm',
                    });
                  }
                  setSession((prev) =>
                    prev && prev.name !== uniqueName
                      ? { ...prev, name: uniqueName }
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

      setChatState(ChatState.Idle);
      onStreamFinish();
    },
    [onStreamFinish, sessionId]
  );

  // Load session on mount or sessionId change
  useEffect(() => {
    if (!sessionId) return;

    const cached = cacheGet(sessionId);
    if (cached) {
      setSession(cached.session);
      updateMessages(cached.messages);
      setTokenState({
        inputTokens: cached.session?.input_tokens ?? 0,
        outputTokens: cached.session?.output_tokens ?? 0,
        totalTokens: cached.session?.total_tokens ?? 0,
        accumulatedInputTokens: cached.session?.accumulated_input_tokens ?? 0,
        accumulatedOutputTokens: cached.session?.accumulated_output_tokens ?? 0,
        accumulatedTotalTokens: cached.session?.accumulated_total_tokens ?? 0,
      });
      setChatState(ChatState.Idle);
      onSessionLoaded?.();
      return;
    }

    // Reset state when sessionId changes
    updateMessages([]);
    setSession(undefined);
    setSessionLoadError(undefined);
    setChatState(ChatState.LoadingConversation);

    let cancelled = false;

    (async () => {
      try {
        const response = await resumeAgent({
          body: {
            session_id: sessionId,
            load_model_and_extensions: true,
          },
          throwOnError: true,
        });

        if (cancelled) {
          return;
        }

        const resumeData = response.data;
        const loadedSession = resumeData?.session;
        const extensionResults = resumeData?.extension_results;

        showExtensionLoadResults(extensionResults);
        setSession(loadedSession);
        updateMessages(loadedSession?.conversation || []);
        setTokenState({
          inputTokens: loadedSession?.input_tokens ?? 0,
          outputTokens: loadedSession?.output_tokens ?? 0,
          totalTokens: loadedSession?.total_tokens ?? 0,
          accumulatedInputTokens: loadedSession?.accumulated_input_tokens ?? 0,
          accumulatedOutputTokens: loadedSession?.accumulated_output_tokens ?? 0,
          accumulatedTotalTokens: loadedSession?.accumulated_total_tokens ?? 0,
        });
        setChatState(ChatState.Idle);

        listApps({
          throwOnError: true,
          query: { session_id: sessionId },
        }).catch((err) => {
          console.warn('Failed to populate apps cache:', err);
        });

        onSessionLoaded?.();
      } catch (error) {
        if (cancelled) return;

        setSessionLoadError(errorMessage(error));
        setChatState(ChatState.Idle);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [sessionId, updateMessages, onSessionLoaded]);

  const handleSubmit = useCallback(
    async (userMessage: string, attachments: UserAttachment[] = []) => {
      // Guard: Don't submit if session hasn't been loaded yet
      if (!session || chatState === ChatState.LoadingConversation) {
        return;
      }

      if (abortControllerRef.current && !abortControllerRef.current.signal.aborted) {
        return;
      }

      const hasExistingMessages = messagesRef.current.length > 0;
      const hasNewMessage = userMessage.trim().length > 0;

      // Don't submit if there's no message and no conversation to continue
      if (!hasNewMessage && !hasExistingMessages) {
        return;
      }

      lastInteractionTimeRef.current = Date.now();

      // Emit session-created event for first message in a new session
      if (!hasExistingMessages && hasNewMessage) {
        window.dispatchEvent(new CustomEvent('session-created'));
      }

      let newMessage;
      if (hasNewMessage) {
        try {
          newMessage = await createUserMessage(userMessage, attachments);
        } catch (error) {
          // createUserMessage rejects when an attached image can't be read
          // (e.g. the temp file vanished). Surface the failure instead of
          // letting the rejection silently disappear.
          onFinish('Submit error: ' + errorMessage(error));
          return;
        }
      } else {
        newMessage = messagesRef.current[messagesRef.current.length - 1];
      }
      const currentMessages = hasNewMessage
        ? [...messagesRef.current, newMessage]
        : [...messagesRef.current];

      if (hasNewMessage) {
        updateMessages(currentMessages);
      }

      setChatState(ChatState.Streaming);
      setNotifications([]);
      abortControllerRef.current = new AbortController();
      const streamId = activeStreamIdRef.current + 1;
      activeStreamIdRef.current = streamId;
      const finishCurrentStream = (error?: string) => {
        if (activeStreamIdRef.current !== streamId) {
          return;
        }
        onFinish(error);
      };

      try {
        const { stream } = await reply({
          body: {
            session_id: sessionId,
            user_message: newMessage,
          },
          throwOnError: true,
          signal: abortControllerRef.current.signal,
          sseMaxRetryAttempts: 1,
        });

        await streamFromResponse(
          stream,
          currentMessages,
          updateMessages,
          setTokenState,
          setChatState,
          updateNotifications,
          finishCurrentStream
        );
      } catch (error) {
        // AbortError is expected when user stops streaming
        if (error instanceof Error && error.name === 'AbortError') {
          // Silently handle abort
        } else {
          // Unexpected error during fetch setup (streamFromResponse handles its own errors)
          finishCurrentStream('Submit error: ' + errorMessage(error));
        }
      } finally {
        if (activeStreamIdRef.current === streamId && abortControllerRef.current?.signal.aborted) {
          abortControllerRef.current = null;
        }
      }
    },
    [sessionId, session, chatState, updateMessages, updateNotifications, onFinish]
  );

  const submitElicitationResponse = useCallback(
    async (elicitationId: string, userData: Record<string, unknown>) => {
      if (!session || chatState === ChatState.LoadingConversation) {
        return;
      }

      if (abortControllerRef.current && !abortControllerRef.current.signal.aborted) {
        return;
      }

      lastInteractionTimeRef.current = Date.now();

      const responseMessage = createElicitationResponseMessage(elicitationId, userData);
      const currentMessages = [...messagesRef.current, responseMessage];

      updateMessages(currentMessages);
      setChatState(ChatState.Streaming);
      setNotifications([]);
      abortControllerRef.current = new AbortController();
      const streamId = activeStreamIdRef.current + 1;
      activeStreamIdRef.current = streamId;
      const finishCurrentStream = (error?: string) => {
        if (activeStreamIdRef.current !== streamId) {
          return;
        }
        onFinish(error);
      };

      try {
        const { stream } = await reply({
          body: {
            session_id: sessionId,
            user_message: responseMessage,
          },
          throwOnError: true,
          signal: abortControllerRef.current.signal,
          sseMaxRetryAttempts: 1,
        });

        await streamFromResponse(
          stream,
          currentMessages,
          updateMessages,
          setTokenState,
          setChatState,
          updateNotifications,
          finishCurrentStream
        );
      } catch (error) {
        if (error instanceof Error && error.name === 'AbortError') {
          // Silently handle abort
        } else {
          finishCurrentStream('Submit error: ' + errorMessage(error));
        }
      } finally {
        if (activeStreamIdRef.current === streamId && abortControllerRef.current?.signal.aborted) {
          abortControllerRef.current = null;
        }
      }
    },
    [sessionId, session, chatState, updateMessages, updateNotifications, onFinish]
  );

  const setWorkflowUserParams = useCallback(
    async (user_workflow_values: Record<string, string>) => {
      if (session) {
        await updateSessionUserWorkflowValues({
          path: {
            session_id: sessionId,
          },
          body: {
            userWorkflowValues: user_workflow_values,
          },
          throwOnError: true,
        });
        // TODO(Douwe): get this from the server instead of emulating it here
        setSession({
          ...session,
          user_workflow_values,
        });
      } else {
        setSessionLoadError("can't call setWorkflowParams without a session");
      }
    },
    [sessionId, session, setSessionLoadError]
  );

  useEffect(() => {
    // This should happen on the server when the session is loaded or changed
    // use session.id to support changing of sessions rather than depending on the
    // stable sessionId.
    if (session) {
      updateFromSession({
        body: {
          session_id: session.id,
        },
        throwOnError: true,
      });
    }
  }, [session]);

  const stopStreaming = useCallback(() => {
    activeStreamIdRef.current += 1;
    abortControllerRef.current?.abort();
    setChatState(ChatState.Idle);
    lastInteractionTimeRef.current = Date.now();
  }, []);

  const onMessageUpdate = useCallback(
    async (messageId: string, newContent: string, editType: 'fork' | 'edit' = 'fork') => {
      try {
        const { editMessage } = await import('../api');
        const message = messagesRef.current.find((m) => m.id === messageId);

        if (!message) {
          throw new Error(`Message with id ${messageId} not found in current messages`);
        }

        const response = await editMessage({
          path: {
            session_id: sessionId,
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
          window.electron.logInfo(`Dispatched session-forked event for session ${targetSessionId}`);
        } else {
          const { getSession } = await import('../api');
          const sessionResponse = await getSession({
            path: { session_id: targetSessionId },
            throwOnError: true,
          });

          if (sessionResponse.data?.conversation) {
            updateMessages(sessionResponse.data.conversation);
          }
          await handleSubmit(newContent);
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
    },
    [sessionId, handleSubmit, updateMessages]
  );

  const cached = cacheGet(sessionId);
  const maybe_cached_messages = session ? messages : cached?.messages || [];
  const maybe_cached_session = session ?? cached?.session;

  const notificationsMap = useMemo(() => {
    return notifications.reduce((map, notification) => {
      const key = notification.request_id;
      if (!map.has(key)) {
        map.set(key, []);
      }
      map.get(key)!.push(notification);
      return map;
    }, new Map<string, NotificationEvent[]>());
  }, [notifications]);

  return {
    sessionLoadError,
    messages: maybe_cached_messages,
    session: maybe_cached_session,
    chatState,
    setChatState,
    handleSubmit,
    submitElicitationResponse,
    stopStreaming,
    setWorkflowUserParams,
    tokenState,
    notifications: notificationsMap,
    onMessageUpdate,
  };
}
