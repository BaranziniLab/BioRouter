import React, { createContext, useContext, useSyncExternalStore } from 'react';
import { ChatState } from '../types/chatState';
import {
  cancelTurn,
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
import { errorMessage, isConnectionError } from '../utils/conversionUtils';
import { showExtensionLoadResults } from '../utils/extensionErrorUtils';
import { reasoningEffortForRequest } from '../store/reasoningEffort';
import type { ChatTurnErrorData, TurnErrorScope } from '../types/turnError';
import type { PendingSteer } from '../utils/trailingActivity';

/**
 * BR-62b — a client-generated idempotency key naming a single `/reply` turn. If
 * the SSE transport reconnects and re-POSTs the same body (a flaky network, a
 * resumed fetch), it resends this key, so the server recognises the retry as a
 * duplicate of the turn already in flight (409 `duplicate:true`) instead of
 * starting a second turn. A fresh key is minted per turn, so a genuine next
 * turn is never mistaken for a retry of the previous one.
 */
function newTurnId(): string {
  const c = globalThis.crypto;
  if (c && typeof c.randomUUID === 'function') {
    return c.randomUUID();
  }
  return `turn-${Date.now()}-${Math.random().toString(36).slice(2)}`;
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

/**
 * BR-61 — has the agent consumed the soft interrupt we are optimistically
 * showing? The agent echoes a steer back onto the live stream as an ordinary
 * user message once it injects it, and that echo is the ONLY reliable signal
 * that it landed.
 *
 * `afterCount` is the transcript length at the moment the steer was issued, so
 * only messages that arrived AFTER the press can satisfy it. Matching on text
 * alone would let a user who steers with the same words as an earlier prompt
 * clear the indicator against their own history, the instant it appeared.
 */
export function steerWasEchoed(
  pending: { text: string } | undefined,
  messages: Message[],
  afterCount: number
): boolean {
  if (!pending) return false;
  const wanted = pending.text.trim();
  if (!wanted) return false;
  return messages
    .slice(afterCount)
    .some(
      (m) =>
        m.role === 'user' &&
        m.content.some((c) => c.type === 'text' && c.text.trim() === wanted)
    );
}

export interface ChatStreamSnapshot {
  session?: Session;
  messages: Message[];
  chatState: ChatState;
  sessionLoadError?: string;
  turnError?: ChatTurnErrorData;
  tokenState: TokenState;
  notifications: NotificationEvent[];
  /**
   * Client clock (ms) when the current turn was submitted; undefined while
   * idle. Fallback origin for the trailing activity timer when no Message
   * event has landed yet.
   */
  turnStartedAt?: number;
  /**
   * Client clock (ms) when the most recent Message event was APPLIED. This is
   * deliberately a client timestamp, not `message.created` (which is SECONDS —
   * see utils/timeUtils.ts — and carries server-clock skew). It is the origin
   * every live elapsed display counts from, and living in the store is what
   * makes it survive the message list's per-event re-render churn.
   */
  lastMessageAt?: number;
  /**
   * BR-61 — the soft interrupt this client has issued and the agent has not yet
   * consumed. Set OPTIMISTICALLY, before the POST resolves, because the whole
   * point is to fill the dead air between the user's press and the agent's next
   * output; cleared on rejection, on echo, and on any turn boundary, so it can
   * never outlive the thing it describes.
   */
  pendingSteer?: PendingSteer;
  /**
   * Whether this session's agent — model provider + extensions — has finished
   * loading on the backend. The transcript paints before this flips (see
   * `loadSession`), so anything that reads AGENT state rather than SESSION
   * state (the tool count, for one) must wait for it or it will read an empty
   * world and cache the emptiness. `false` means "not yet", never "failed";
   * a failed load still ends at `true` with `turnError` set.
   */
  agentReady: boolean;
  /**
   * §6.1b — tool calls the model has begun emitting whose arguments are still
   * generating, keyed by tool-call id. Populated from `ToolCallPending` stream
   * events so the UI can draw a skeleton card the moment a tool's NAME is known
   * — seconds before its arguments finish — and removed the instant the
   * authoritative `ToolRequest` (same id) lands in a `Message`.
   *
   * These are DELIBERATELY held OUT of `messages`. A pending tool call is
   * advisory display state, never a real request: it must never be dispatched,
   * persisted, or fed back to the model, and keeping it off the message array
   * also sidesteps the content-dedup landmine (`pushMessage` dedupes content by
   * JSON equality, so a partial and its completed form would BOTH survive).
   */
  pendingToolCalls: PendingToolCallView[];
}

/** A tool call announced before its arguments finished streaming (§6.1b). */
export interface PendingToolCallView {
  id: string;
  name: string;
  /** Arguments accumulated so far; almost never valid JSON. Display only. */
  partialArgs?: string;
}

function clientTurnError(
  error: unknown,
  code: string,
  defaultScope: TurnErrorScope
): ChatTurnErrorData {
  const message = errorMessage(error);
  return {
    message,
    technicalDetails: message,
    code,
    scope: isConnectionError(error) ? 'transport' : defaultScope,
    retryable: true,
  };
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
    agentReady: false,
    pendingToolCalls: [],
  };
  private listeners = new Set<() => void>();
  private finishListeners = new Set<() => void>();
  private messagesRef: Message[] = [];
  private abortController: AbortController | null = null;
  private activeStreamId = 0;
  private lastInteractionTime = Date.now();
  private loadPromise: Promise<void> | null = null;
  /**
   * R3-01 — synchronous re-entrancy latch for the submit prep window. The
   * `abortController` guard in `canSubmitMessage` only trips once a turn has
   * been launched, but `handleSubmit` awaits `loadSession` + `createUserMessage`
   * *before* assigning `abortController`. A rapid double-click hits that gap:
   * both calls pass the guard and both append the user turn (phantom duplicate
   * bubble). This latch is set synchronously at the top of `handleSubmit`, so
   * the second click bails before its first await.
   */
  private submitInFlight = false;
  /**
   * The in-flight (or settled) model+extension load. Unlike `loadPromise` this
   * is never nulled on completion: it is the memo that makes `ensureAgentLoaded`
   * idempotent across the several call sites that can reach it (cold load,
   * cached load, controller reuse, a submit that got there first).
   */
  private agentLoadPromise: Promise<void> | null = null;
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

  // `receivedAt` is stamped ONLY by the live stream path. The other callers
  // (session load, diverge, edit) deliberately omit it: replaying a saved
  // transcript must never look like a live event, or a historical session
  // would inherit a running clock.
  private updateMessages = (messages: Message[], receivedAt?: number): void => {
    this.messagesRef = messages;
    this.updateSnapshot((prev) => {
      // BR-61: the echo of our own steer is what retires the optimistic chip.
      const steerLanded = steerWasEchoed(prev.pendingSteer, messages, this.steerAfterCount);
      return {
        ...prev,
        messages,
        lastMessageAt: receivedAt ?? prev.lastMessageAt,
        pendingSteer: steerLanded ? undefined : prev.pendingSteer,
      };
    });
  };

  /** Transcript length when the in-flight steer was issued. See steerWasEchoed. */
  private steerAfterCount = 0;

  private clearPendingSteer = (): void => {
    if (!this.snapshot.pendingSteer) return;
    this.updateSnapshot((prev) => ({ ...prev, pendingSteer: undefined }));
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

  /** Upsert a pending tool-call skeleton by id (§6.1b). */
  private upsertPendingToolCall = (pending: PendingToolCallView): void => {
    this.updateSnapshot((prev) => {
      const idx = prev.pendingToolCalls.findIndex((p) => p.id === pending.id);
      if (idx === -1) {
        return { ...prev, pendingToolCalls: [...prev.pendingToolCalls, pending] };
      }
      // Merge in a later (longer) partial-args preview without reordering.
      const next = prev.pendingToolCalls.slice();
      next[idx] = { ...next[idx], ...pending };
      return { ...prev, pendingToolCalls: next };
    });
  };

  /**
   * Remove any pending skeletons whose id matches a real tool request in the
   * message just applied — the authoritative card now renders in its place.
   */
  private clearPendingToolCallsFromMessage = (msg: Message): void => {
    const landedIds = new Set(
      msg.content
        .filter((c) => c.type === 'toolRequest' || c.type === 'frontendToolRequest')
        .map((c) => (c as { id?: string }).id)
        .filter((id): id is string => typeof id === 'string')
    );
    if (landedIds.size === 0) return;
    this.updateSnapshot((prev) => {
      const remaining = prev.pendingToolCalls.filter((p) => !landedIds.has(p.id));
      if (remaining.length === prev.pendingToolCalls.length) return prev;
      return { ...prev, pendingToolCalls: remaining };
    });
  };

  private clearPendingToolCalls = (): void => {
    this.updateSnapshot((prev) =>
      prev.pendingToolCalls.length === 0 ? prev : { ...prev, pendingToolCalls: [] }
    );
  };

  /**
   * Load the agent — model provider + extensions — for this session, once.
   *
   * This is the slow half of resuming a chat: on a real session it is ~4.6s of
   * extension startup against ~0.5s to fetch the transcript, and it is
   * per-session, so every tab re-pays it. `loadSession` therefore paints the
   * transcript first and leaves this running in the background; anything that
   * genuinely needs the agent awaits `whenAgentReady()` instead of blocking the
   * paint.
   *
   * Never rejects: a failed agent load is reported through `turnError` and the
   * toast, and still resolves, so a submit parked on it can proceed and produce
   * a real error rather than hanging forever.
   */
  private ensureAgentLoaded(): Promise<void> {
    if (!this.sessionId) return Promise.resolve();
    if (this.agentLoadPromise) return this.agentLoadPromise;

    this.agentLoadPromise = (async () => {
      try {
        const response = await resumeAgent({
          body: {
            session_id: this.sessionId,
            load_model_and_extensions: true,
          },
          throwOnError: true,
        });
        const resumeData = response.data;
        const initializationError = resumeData?.initialization_error;

        showExtensionLoadResults(resumeData?.extension_results, this.sessionId);

        if (initializationError) {
          this.updateSnapshot((prev) => ({
            ...prev,
            turnError: {
              message: initializationError.message,
              technicalDetails: initializationError.message,
              code: initializationError.code,
              scope: 'session',
              retryable: initializationError.retryable,
            },
          }));
        }

        // Binds the session's model/provider onto the agent. Previously
        // fire-and-forget on the assumption the agent was already up; now it is
        // part of readiness, so a submit that waits for the agent also waits for
        // its model to be bound instead of racing it.
        if (resumeData?.session) {
          try {
            await updateFromSession({
              body: { session_id: resumeData.session.id },
              throwOnError: true,
            });
          } catch (err) {
            console.warn('Failed to update agent from session:', err);
          }
        }
      } catch (error) {
        console.warn('Failed to load model and extensions:', error);
        this.updateSnapshot((prev) => ({
          ...prev,
          // Do not clobber a turn error the user is already looking at.
          turnError: prev.turnError ?? clientTurnError(error, 'agent_load_failed', 'session'),
        }));
      } finally {
        this.updateSnapshot((prev) => ({ ...prev, agentReady: true }));
      }
    })();

    return this.agentLoadPromise;
  }

  /**
   * Resolves once the agent is loaded (or has failed to load). Kicks the load
   * off if nothing has yet — the cached-transcript path reaches submit without
   * ever having gone through `loadSession`'s cold path.
   */
  whenAgentReady(): Promise<void> {
    if (this.snapshot.agentReady) return Promise.resolve();
    return this.ensureAgentLoaded();
  }

  async loadSession(onSessionLoaded?: () => void): Promise<void> {
    if (!this.sessionId) return;

    if (this.snapshot.session) {
      // Session already painted, but the agent may still be missing entirely on
      // the controller-reuse path. Idempotent.
      void this.ensureAgentLoaded();
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
      // The cache is a process-lifetime LRU with no TTL, so this path could
      // previously reach a submit having NEVER loaded the agent for this
      // session — the transcript looked live while the backend had no
      // extensions. Kick the load off here; `whenAgentReady` is what makes the
      // submit safe.
      void this.ensureAgentLoaded();
      onSessionLoaded?.();
      return;
    }

    if (!this.loadPromise) {
      this.updateSnapshot((prev) => ({
        ...prev,
        messages: [],
        session: undefined,
        sessionLoadError: undefined,
        turnError: undefined,
        chatState: ChatState.LoadingConversation,
      }));

      this.loadPromise = (async () => {
        try {
          // PHASE 1 — the transcript, and nothing else. `load_model_and_extensions:
          // false` skips agent construction, provider restore and extension
          // startup, which is ~4.6s of the ~5.1s a resume used to take while
          // contributing a few hundred bytes to what the user reads. The user
          // came here to read the conversation; give them the conversation.
          const response = await resumeAgent({
            body: {
              session_id: this.sessionId,
              load_model_and_extensions: false,
            },
            throwOnError: true,
          });
          const resumeData = response.data;
          const loadedSession = resumeData?.session;

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
            sessionLoadError: undefined,
            turnError: undefined,
          }));

          // PHASE 2 — model + extensions, off the paint path. Deliberately not
          // awaited: `loadSession` resolves as soon as the transcript is up.
          void this.ensureAgentLoaded();

          listApps({
            throwOnError: true,
            query: { session_id: this.sessionId },
          }).catch((err) => {
            console.warn('Failed to populate apps cache:', err);
          });
        } catch (error) {
          if (isConnectionError(error)) {
            // The backend (biorouterd) was transiently unreachable — it is
            // restarting, or the network blipped. This is NOT an unloadable
            // session, so it must NOT escalate to the full-pane "Failed to Load
            // Session" card that nukes the transcript and offers only "Go home".
            // Surface it as a retryable inline turn error instead: the chat UI
            // and composer stay mounted, and `retryTurn` (or the daemon
            // self-recovering) re-runs this load and repaints the transcript.
            // The genuine-load-failure card below is reserved for real errors
            // (bad id / corrupt data / an HTTP response), which are not
            // TypeErrors and never look like a connection failure.
            this.updateSnapshot((prev) => ({
              ...prev,
              turnError:
                prev.turnError ?? clientTurnError(error, 'session_load_unreachable', 'transport'),
              chatState: ChatState.Idle,
            }));
          } else {
            this.updateSnapshot((prev) => ({
              ...prev,
              sessionLoadError: errorMessage(error),
              chatState: ChatState.Idle,
            }));
          }
        } finally {
          this.loadPromise = null;
        }
      })();
    }

    await this.loadPromise;
    onSessionLoaded?.();
  }

  private finishCurrentStream = async (error?: ChatTurnErrorData): Promise<void> => {
    if (error) {
      this.updateSnapshot((prev) => ({ ...prev, turnError: error }));
    }
    // The turn is over: any skeleton whose authoritative request never arrived
    // (cancel, provider abort, a dropped block) must not linger.
    this.clearPendingToolCalls();
    this.abortController = null;

    const timeSinceLastInteraction = Date.now() - this.lastInteractionTime;
    if (!error && timeSinceLastInteraction > 60000) {
      window.electron?.showNotification({
        title: 'biorouter finished the task.',
        body: 'Click here to expand.',
      });
    }

    // Every completed turn can change the session's recency or generated name.
    // Session ids use a variable-width daily counter (for example
    // `20260716_1` and `20260716_27`), so gating this refresh behind a fixed
    // id shape leaves History and Recents stale for real sessions.
    if (this.sessionId) {
      window.dispatchEvent(new CustomEvent('message-stream-finished'));
    }

    if (
      this.sessionId &&
      this.snapshot.session &&
      !this.snapshot.session.user_set_name &&
      isDefaultSessionName(this.snapshot.session.name)
    ) {
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

    this.updateSnapshot((prev) => ({
      ...prev,
      chatState: ChatState.Idle,
      turnStartedAt: undefined,
      lastMessageAt: undefined,
      // The turn it was aimed at is over; whether or not we saw the echo, there
      // is nothing left to steer.
      pendingSteer: undefined,
    }));
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
          case 'ToolCallPending': {
            // Advisory skeleton for a tool whose args are still streaming. Upsert
            // by id; NEVER routed into `messages` (see `pendingToolCalls`).
            this.upsertPendingToolCall({
              id: event.id,
              name: event.name,
              partialArgs: event.partial_args ?? undefined,
            });
            // A tool block is generating: reflect that as active streaming.
            this.setChatState(ChatState.Streaming);
            break;
          }
          case 'Message': {
            const msg = event.message;
            currentMessages = pushMessage(currentMessages, msg);
            // The authoritative request(s) landed: drop any matching skeletons so
            // the real tool card replaces the placeholder with no flicker or ghost.
            this.clearPendingToolCallsFromMessage(msg);
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
            this.updateMessages(currentMessages, Date.now());
            break;
          }
          case 'Error':
            await this.finishCurrentStream({
              message: event.error,
              technicalDetails: event.error,
              code: event.code || 'unknown',
              scope: event.scope || 'inference',
              retryable: event.retryable ?? false,
              providerKind: event.provider_kind ?? undefined,
            });
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
          default:
            break;
        }
      }

      if (this.activeStreamId === streamId && !this.abortController?.signal.aborted) {
        await this.finishCurrentStream({
          message: 'The connection closed before Biorouter received a completion status.',
          code: 'stream_interrupted',
          scope: 'transport',
          retryable: true,
        });
      }
    } catch (error) {
      if (this.activeStreamId !== streamId) return;
      if (error instanceof Error && error.name === 'AbortError') return;
      await this.finishCurrentStream(clientTurnError(error, 'stream_error', 'transport'));
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
      pendingToolCalls: [],
      turnError: undefined,
      turnStartedAt: Date.now(),
      lastMessageAt: undefined,
      pendingSteer: undefined,
    }));
    this.abortController = new AbortController();
    const streamId = this.activeStreamId + 1;
    this.activeStreamId = streamId;
    // BR-62b: one idempotency key per turn, sent in the body so an SSE
    // reconnect re-POST carries the same key and the server dedupes it.
    const turnId = newTurnId();

    try {
      // The transcript paints before the agent's model + extensions are up, so
      // the user can submit into a session whose backend agent is not ready.
      // HOLD the turn here rather than dropping or blocking it:
      //  - not dropped: the message is already appended to the transcript above
      //    and goes out the instant the agent lands, so the composer never eats
      //    the first thing you type.
      //  - not blocked: we are already in a Streaming state with a live
      //    abortController, so the user sees their message and a working
      //    indicator, and Stop works throughout.
      // Resolves immediately (a microtask) once the agent is ready, which is the
      // overwhelmingly common case.
      await this.whenAgentReady();
      if (this.abortController?.signal.aborted || this.activeStreamId !== streamId) {
        return;
      }

      const { stream } = await reply({
        body: {
          session_id: this.sessionId,
          user_message: newMessage,
          // BR-62b: idempotency key for this turn — see `newTurnId`.
          turn_id: turnId,
          // BR-63: the composer's per-turn reasoning effort. Omitted on the
          // default ('normal'), so a session-level `/effort` still applies.
          reasoning_effort: reasoningEffortForRequest(),
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
      await this.finishCurrentStream(clientTurnError(error, 'submit_error', 'inference'));
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
    // R3-01: bail synchronously on a re-entrant submit (double-click) so the
    // second call never reaches the async prep that appends a duplicate user
    // turn. Held across the whole submit; `canSubmitMessage`'s abortController
    // guard takes over the moment the turn is actually launched.
    if (this.submitInFlight) {
      return;
    }
    this.submitInFlight = true;
    try {
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
          await this.finishCurrentStream(
            clientTurnError(error, 'message_preparation_failed', 'inference')
          );
          return;
        }
      } else {
        newMessage = this.messagesRef[this.messagesRef.length - 1];
      }

      const currentMessages = hasNewMessage
        ? [...this.messagesRef, newMessage]
        : [...this.messagesRef];
      await this.submitPreparedMessage(newMessage, currentMessages, hasNewMessage);
    } finally {
      this.submitInFlight = false;
    }
  };

  /**
   * Re-run the last turn after a RETRYABLE failure — a backend/provider blip on
   * send, a dropped stream, or a transient cold-load failure while biorouterd
   * was restarting. Safe to call repeatedly and safe to double-fire:
   *
   *  - Bails if a turn is already live (an in-flight, un-aborted controller), so
   *    a stray double-click can never launch a second concurrent turn.
   *  - Clears the inline error being retried past.
   *  - Re-attempts the session load. This is a no-op once the session is loaded,
   *    and re-runs a cold load that failed while the daemon was briefly down —
   *    which is what repaints a transcript the fatal-card bug used to discard.
   *  - Re-submits the TRAILING user message EXACTLY ONCE, reusing the message
   *    already at the tail of the transcript (`updateMessageList: false`), so no
   *    duplicate user turn is ever appended to the store. If there is no trailing
   *    user turn (nothing was ever sent on this controller, e.g. a pure mount
   *    load failure) the reload alone is the retry.
   */
  retryTurn = async (): Promise<void> => {
    if (this.abortController && !this.abortController.signal.aborted) return;
    this.updateSnapshot((prev) => ({ ...prev, turnError: undefined }));

    await this.loadSession();
    if (!this.canSubmitMessage()) return;

    const last = this.messagesRef[this.messagesRef.length - 1];
    if (!last || last.role !== 'user') return;

    this.lastInteractionTime = Date.now();
    await this.submitPreparedMessage(last, [...this.messagesRef], false);
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
    // Show it BEFORE the round-trip, not after. The POST itself can take a
    // moment and the agent only consumes the steer at its next loop boundary —
    // which may be a whole tool call away — so waiting for either would leave
    // the user staring at a composer that just emptied itself for no visible
    // reason. If the server refuses, the catch below retracts this.
    this.steerAfterCount = this.messagesRef.length;
    this.updateSnapshot((prev) => ({
      ...prev,
      pendingSteer: { text: trimmed, since: Date.now() },
    }));
    try {
      await interrupt({
        body: { session_id: this.sessionId, text: trimmed },
        throwOnError: true,
      });
      this.lastInteractionTime = Date.now();
      return true;
    } catch (error) {
      // 409 = the turn ended between the click and the POST; the caller queues
      // or sends it instead. Retract the optimistic chip in the same breath —
      // leaving "Steering…" up while the text is actually taking the ordinary
      // send path would be the UI telling the user something untrue.
      this.clearPendingSteer();
      console.warn('Soft interrupt rejected, falling back to a normal send:', error);
      return false;
    }
  };

  stopStreaming = (): void => {
    this.activeStreamId += 1;
    this.abortController?.abort();
    this.updateSnapshot((prev) => ({
      ...prev,
      chatState: ChatState.Idle,
      turnStartedAt: undefined,
      lastMessageAt: undefined,
      pendingSteer: undefined,
    }));
    this.lastInteractionTime = Date.now();

    // BR-62b: aborting the SSE socket only tears down *this* client's view of
    // the turn. The server's reply task keeps running on its own `Arc<Agent>`,
    // burning tokens into a socket nobody reads — and if the turn is parked on
    // a tool-permission prompt, closing the socket does not release it. Trip
    // the running turn's cancellation token by session id so it actually stops.
    // `/agent/cancel` is deliberately idempotent: a cancel with no turn in
    // flight is a 200 `cancelled:false`, not an error, so this is safe to fire
    // even when the turn already finished between the click and the POST.
    cancelTurn({
      body: { session_id: this.sessionId },
      throwOnError: true,
    }).catch((error) => {
      console.warn('Failed to cancel running turn on stop:', error);
    });
  };

  onMessageUpdate = async (
    messageId: string,
    newContent: string,
    editType: 'diverge' | 'edit' = 'diverge'
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

      if (editType === 'diverge') {
        const event = new CustomEvent('session-diverged', {
          detail: {
            // The session diverged FROM. 'session-diverged' is a window
            // broadcast, and newSessionId names a session that doesn't exist in
            // the UI yet — so listeners identify the one chat that should
            // navigate by the ORIGIN session, which is this controller's own.
            sessionId: this.sessionId,
            newSessionId: targetSessionId,
            shouldStartAgent: true,
            editedMessage: newContent,
          },
        });
        window.dispatchEvent(event);
        window.electron?.logInfo(
          `Dispatched session-diverged event for session ${targetSessionId}`
        );
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
