import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ChatState } from '../types/chatState';
import { ChatStreamRegistry } from './chatStreamStore';
import type { Message, MessageEvent, Session, TokenState } from '../api';
import { cancelTurn, editMessage, getSession, interrupt, reply, resumeAgent } from '../api';

vi.mock('../api', async () => {
  return {
    cancelTurn: vi.fn(async () => ({ data: { cancelled: true } })),
    editMessage: vi.fn(),
    getSession: vi.fn(async () => ({ data: null })),
    interrupt: vi.fn(),
    listApps: vi.fn(async () => ({ data: { apps: [] } })),
    listSessions: vi.fn(async () => ({ data: { sessions: [] } })),
    reply: vi.fn(),
    resumeAgent: vi.fn(),
    updateFromSession: vi.fn(async () => ({ data: {} })),
    updateSessionUserWorkflowValues: vi.fn(async () => ({ data: {} })),
  };
});

const tokenState: TokenState = {
  accumulatedInputTokens: 0,
  accumulatedOutputTokens: 0,
  accumulatedTotalTokens: 0,
  inputTokens: 0,
  outputTokens: 0,
  totalTokens: 0,
};

function session(id: string): Session {
  return {
    id,
    name: `Session ${id}`,
    working_dir: '/tmp',
    conversation: [],
    message_count: 0,
    total_tokens: 0,
    created_at: '',
    updated_at: '',
    extension_data: {},
    user_set_name: false,
  } as Session;
}

function assistantMessage(id: string, text: string): Message {
  return {
    id,
    role: 'assistant',
    created: 1,
    content: [{ type: 'text', text }],
    metadata: { userVisible: true, agentVisible: true },
  };
}

function createControlledStream() {
  const events: MessageEvent[] = [];
  let resolveNext: (() => void) | null = null;
  let closed = false;

  async function* stream() {
    while (!closed || events.length > 0) {
      if (events.length === 0) {
        await new Promise<void>((resolve) => {
          resolveNext = resolve;
        });
      }
      const event = events.shift();
      if (event) {
        yield event;
      }
    }
  }

  return {
    stream: stream(),
    push(event: MessageEvent) {
      events.push(event);
      resolveNext?.();
      resolveNext = null;
    },
    close() {
      closed = true;
      resolveNext?.();
      resolveNext = null;
    },
  };
}

async function flush() {
  await Promise.resolve();
  await Promise.resolve();
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.mocked(resumeAgent).mockReset();
  vi.mocked(reply).mockReset();
  vi.mocked(getSession).mockReset();
  vi.mocked(interrupt).mockReset();
  vi.mocked(cancelTurn).mockReset();
  vi.mocked(editMessage).mockReset();
  vi.mocked(cancelTurn).mockResolvedValue({ data: { cancelled: true } } as never);
  vi.mocked(getSession).mockResolvedValue({ data: null } as never);
  Object.assign(window, {
    electron: {
      openExternal: vi.fn(async () => undefined),
      readTempImageAsBase64: vi.fn(async () => ({ data: 'B64-image', mimeType: 'image/png' })),
      showNotification: vi.fn(),
      logInfo: vi.fn(),
    },
  });
});

describe('ChatStreamRegistry', () => {
  it('does not auto-open Agent Drafter launch metadata', async () => {
    const registry = new ChatStreamRegistry();
    vi.mocked(resumeAgent).mockResolvedValue({ data: { session: session('s1') } } as never);
    vi.mocked(reply).mockResolvedValue({
      stream: (async function* () {
        yield {
          type: 'Message',
          message: {
            id: 'launch-result',
            role: 'tool',
            created: 2,
            content: [
              {
                type: 'toolResponse',
                id: 'launch-tool',
                toolResult: {
                  status: 'success',
                  value: {
                    is_error: false,
                    content: [{ type: 'text', text: 'App ready' }],
                    _meta: { 'biorouter/app-path': '/apps/click-me/' },
                  },
                },
              },
            ],
            metadata: { userVisible: false, agentVisible: true },
          },
          token_state: tokenState,
        } as MessageEvent;
        yield { type: 'Finish', reason: 'done', token_state: tokenState } as MessageEvent;
      })(),
    } as never);

    await registry.getController('s1').handleSubmit('build an app');

    expect(window.electron.openExternal).not.toHaveBeenCalled();
  });

  it('synchronizes a generated title and refreshes history after a tool-first turn', async () => {
    const registry = new ChatStreamRegistry();
    const initialSession = { ...session('20260716_27'), name: 'New Session' };
    const generatedSession = { ...initialSession, name: 'Apple Watch News' };
    vi.mocked(resumeAgent).mockResolvedValue({ data: { session: initialSession } } as never);
    vi.mocked(getSession).mockResolvedValue({ data: generatedSession } as never);
    vi.mocked(reply).mockResolvedValue({
      stream: (async function* () {
        yield {
          type: 'Message',
          message: {
            id: 'tool-first',
            role: 'assistant',
            created: 2,
            content: [
              {
                type: 'toolRequest',
                id: 'tool-1',
                toolCall: {
                  status: 'success',
                  value: { name: 'developer__text_editor', arguments: { command: 'view' } },
                },
              },
            ],
            metadata: { userVisible: true, agentVisible: true },
          },
          token_state: tokenState,
        } as MessageEvent;
        yield { type: 'Finish', reason: 'done', token_state: tokenState } as MessageEvent;
      })(),
    } as never);

    const onFinished = vi.fn();
    window.addEventListener('message-stream-finished', onFinished);
    const controller = registry.getController('20260716_27');
    await controller.handleSubmit('summarize the latest Apple Watch news');

    expect(onFinished).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(800);
    await flush();

    expect(getSession).toHaveBeenCalledWith({
      path: { session_id: '20260716_27' },
      throwOnError: true,
    });
    expect(controller.getSnapshot().session?.name).toBe('Apple Watch News');
    window.removeEventListener('message-stream-finished', onFinished);
  });

  it('routes structured provider failures inline without failing the session', async () => {
    const registry = new ChatStreamRegistry();
    vi.mocked(resumeAgent).mockResolvedValue({ data: { session: session('s1') } } as never);
    vi.mocked(reply).mockResolvedValue({
      stream: (async function* () {
        yield {
          type: 'Error',
          error: 'You exceeded your current quota.',
          code: 'provider_failure',
          scope: 'provider',
          retryable: false,
          provider_kind: 'quota',
        } as MessageEvent;
      })(),
    } as never);

    const controller = registry.getController('s1');
    await controller.handleSubmit('hello');

    expect(controller.getSnapshot()).toMatchObject({
      chatState: ChatState.Idle,
      turnError: {
        message: 'You exceeded your current quota.',
        code: 'provider_failure',
        scope: 'provider',
        retryable: false,
        providerKind: 'quota',
      },
    });
    expect(controller.getSnapshot().sessionLoadError).toBeUndefined();
    expect(controller.getSnapshot().messages[0]).toMatchObject({ role: 'user' });
  });

  it('clears the previous inline turn error when a new model turn starts', async () => {
    const registry = new ChatStreamRegistry();
    vi.mocked(resumeAgent).mockResolvedValue({ data: { session: session('s1') } } as never);
    vi.mocked(reply)
      .mockResolvedValueOnce({
        stream: (async function* () {
          yield {
            type: 'Error',
            error: 'provider unavailable',
            code: 'provider_failure',
            scope: 'provider',
            retryable: true,
          } as MessageEvent;
        })(),
      } as never)
      .mockResolvedValueOnce({
        stream: (async function* () {
          yield { type: 'Finish', reason: 'done', token_state: tokenState } as MessageEvent;
        })(),
      } as never);

    const controller = registry.getController('s1');
    await controller.handleSubmit('first try');
    expect(controller.getSnapshot().turnError).toMatchObject({
      message: 'provider unavailable',
      code: 'provider_failure',
    });

    await controller.handleSubmit('second try');
    expect(controller.getSnapshot().turnError).toBeUndefined();
    expect(controller.getSnapshot().sessionLoadError).toBeUndefined();
  });

  it('uses the generic inline path for an unknown future provider error', async () => {
    const registry = new ChatStreamRegistry();
    vi.mocked(resumeAgent).mockResolvedValue({ data: { session: session('s1') } } as never);
    vi.mocked(reply).mockResolvedValue({
      stream: (async function* () {
        yield {
          type: 'Error',
          error: 'Provider returned a new rejection type',
          code: 'provider_failure',
          scope: 'provider',
          retryable: false,
          provider_kind: 'unknown',
        } as MessageEvent;
      })(),
    } as never);

    const controller = registry.getController('s1');
    await controller.handleSubmit('hello');

    expect(controller.getSnapshot().turnError).toMatchObject({
      message: 'Provider returned a new rejection type',
      providerKind: 'unknown',
    });
    expect(controller.getSnapshot().sessionLoadError).toBeUndefined();
  });

  it('treats a stream ending without a terminal event as an inline interruption', async () => {
    const registry = new ChatStreamRegistry();
    vi.mocked(resumeAgent).mockResolvedValue({ data: { session: session('s1') } } as never);
    vi.mocked(reply).mockResolvedValue({
      stream: (async function* () {
        yield {
          type: 'Message',
          message: assistantMessage('a1', 'partial response'),
          token_state: tokenState,
        } as MessageEvent;
      })(),
    } as never);

    const controller = registry.getController('s1');
    await controller.handleSubmit('hello');

    expect(controller.getSnapshot()).toMatchObject({
      chatState: ChatState.Idle,
      turnError: {
        code: 'stream_interrupted',
        scope: 'transport',
        retryable: true,
      },
    });
  });

  it('catches non-Error stream failures instead of leaving the chat running', async () => {
    const registry = new ChatStreamRegistry();
    vi.mocked(resumeAgent).mockResolvedValue({ data: { session: session('s1') } } as never);
    vi.mocked(reply).mockResolvedValue({
      stream: {
        [Symbol.asyncIterator]() {
          return {
            next: async () => {
              throw 'decoder rejected an unknown frame';
            },
          };
        },
      },
    } as never);

    const controller = registry.getController('s1');
    await controller.handleSubmit('hello');

    expect(controller.getSnapshot()).toMatchObject({
      chatState: ChatState.Idle,
      turnError: {
        message: 'decoder rejected an unknown frame',
        code: 'stream_error',
      },
    });
  });

  it('keeps a provider initialization failure inline after loading the session', async () => {
    const registry = new ChatStreamRegistry();
    const sessionId = 'provider-init-failure-session';
    vi.mocked(resumeAgent).mockResolvedValue({
      data: {
        session: session(sessionId),
        initialization_error: {
          code: 'provider_restore_failed',
          message: 'Configured model no longer exists',
          retryable: false,
        },
      },
    } as never);

    const controller = registry.getController(sessionId);
    await controller.loadSession();

    expect(controller.getSnapshot()).toMatchObject({
      session: { id: sessionId },
      turnError: {
        code: 'provider_restore_failed',
        scope: 'session',
      },
    });
    expect(controller.getSnapshot().sessionLoadError).toBeUndefined();
  });

  it('reserves the full-session error state for an actual resume failure', async () => {
    const registry = new ChatStreamRegistry();
    vi.mocked(resumeAgent).mockRejectedValue(new Error('session database unavailable'));

    const controller = registry.getController('actual-session-load-failure');
    await controller.loadSession();

    expect(controller.getSnapshot().sessionLoadError).toBe('session database unavailable');
    expect(controller.getSnapshot().turnError).toBeUndefined();
  });

  // ---- SEND-HARDENING: a system-level blip on the send/load path must degrade
  // to an inline turn error, never the transcript-nuking "Failed to Load
  // Session" card. See docs/qa/2026-07-19-qa-round2.md (SEND-HARDENING).

  it('degrades a send into an unreachable backend to an inline error, never the fatal card (gate a)', async () => {
    // The user reproduction: biorouterd is briefly down (a QA restart), the
    // controller is fresh (post-reload), and the user hits Send. The send runs
    // through loadSession's cold path, whose fetch rejects with the browser's
    // `TypeError: Failed to fetch`.
    const registry = new ChatStreamRegistry();
    vi.mocked(resumeAgent).mockRejectedValue(new TypeError('Failed to fetch'));

    const controller = registry.getController('send-into-dead-backend');
    await controller.handleSubmit('analyze my cohort');

    const snap = controller.getSnapshot();
    // The full-pane "Failed to Load Session / Go home" card is NOT triggered.
    expect(snap.sessionLoadError).toBeUndefined();
    // A retryable, transport-scoped inline turn error surfaces instead.
    expect(snap.turnError).toMatchObject({
      code: 'session_load_unreachable',
      scope: 'transport',
      retryable: true,
    });
    expect(snap.chatState).toBe(ChatState.Idle);
  });

  it('keeps a transient cold-load connection failure inline instead of the fatal card', async () => {
    const registry = new ChatStreamRegistry();
    vi.mocked(resumeAgent).mockRejectedValue(new TypeError('Failed to fetch'));

    const controller = registry.getController('cold-load-blip');
    await controller.loadSession();

    const snap = controller.getSnapshot();
    expect(snap.sessionLoadError).toBeUndefined();
    expect(snap.turnError).toMatchObject({
      code: 'session_load_unreachable',
      scope: 'transport',
      retryable: true,
    });
  });

  it('still shows the fatal card for a genuine (non-connection) resume failure (gate c)', async () => {
    // A real HTTP error (bad id / corrupt data) is not a connection blip: it
    // arrives as a plain Error, not a TypeError, and must stay fatal.
    const registry = new ChatStreamRegistry();
    vi.mocked(resumeAgent).mockRejectedValue(new Error('HTTP 404: session not found'));

    const controller = registry.getController('genuinely-unloadable');
    await controller.loadSession();

    const snap = controller.getSnapshot();
    expect(snap.sessionLoadError).toBe('HTTP 404: session not found');
    expect(snap.turnError).toBeUndefined();
  });

  it('retryTurn re-runs the failed send exactly once without duplicating the user message (gate b)', async () => {
    // Unique session id: the transcript cache is a module-level LRU keyed by id,
    // so reusing a shared id ('s1') would load another test's cached messages
    // and break the exact-length assertions below.
    const sessionId = 'retry-gate-b';
    const registry = new ChatStreamRegistry();
    vi.mocked(resumeAgent).mockResolvedValue({ data: { session: session(sessionId) } } as never);
    // First send: the backend blips mid-POST. Retry: the backend is back.
    vi.mocked(reply).mockRejectedValueOnce(new TypeError('Failed to fetch'));
    vi.mocked(reply).mockResolvedValueOnce({
      stream: (async function* () {
        yield { type: 'Finish', reason: 'done', token_state: tokenState } as MessageEvent;
      })(),
    } as never);

    const controller = registry.getController(sessionId);
    await controller.handleSubmit('hello world');

    // The failed send parked the user's message in the transcript (once) and
    // surfaced a retryable inline error — NOT the fatal card.
    expect(controller.getSnapshot().sessionLoadError).toBeUndefined();
    expect(controller.getSnapshot().turnError).toMatchObject({
      code: 'submit_error',
      scope: 'transport',
      retryable: true,
    });
    expect(controller.getSnapshot().messages.filter((m) => m.role === 'user')).toHaveLength(1);

    await controller.retryTurn();

    // reply fired once for the original send and once for the retry — no more.
    expect(reply).toHaveBeenCalledTimes(2);
    // The user's message still appears exactly once (retry reused the trailing
    // turn rather than appending a duplicate).
    expect(controller.getSnapshot().messages.filter((m) => m.role === 'user')).toHaveLength(1);
    expect(controller.getSnapshot().turnError).toBeUndefined();
    expect(controller.getSnapshot().sessionLoadError).toBeUndefined();
  });

  it('retryTurn does not fire a second concurrent turn while one is live', async () => {
    const sessionId = 'retry-concurrent';
    const registry = new ChatStreamRegistry();
    vi.mocked(resumeAgent).mockResolvedValue({ data: { session: session(sessionId) } } as never);
    const controlled = createControlledStream();
    vi.mocked(reply).mockResolvedValue({ stream: controlled.stream } as never);

    const controller = registry.getController(sessionId);
    // Preload the session + agent so the turn reaches reply() and enters
    // Streaming deterministically (no cold-load latency to flush past).
    await controller.loadSession();
    await flush();

    const inFlight = controller.handleSubmit('hello world');
    // Drive the turn up to the (mocked) reply() call, where it parks on the
    // controlled stream's first event with a live abortController.
    for (let i = 0; i < 12 && vi.mocked(reply).mock.calls.length === 0; i += 1) {
      await flush();
    }
    expect(reply).toHaveBeenCalledTimes(1);
    expect(controller.getSnapshot().chatState).toBe(ChatState.Streaming);

    // A retry while the turn is still streaming must be a no-op.
    await controller.retryTurn();
    expect(reply).toHaveBeenCalledTimes(1);

    controlled.push({ type: 'Finish', reason: 'done', token_state: tokenState } as MessageEvent);
    controlled.close();
    await inFlight;
  });

  it('submits attachment-only messages as new user messages', async () => {
    const registry = new ChatStreamRegistry();
    vi.mocked(resumeAgent).mockResolvedValue({ data: { session: session('s1') } } as never);
    vi.mocked(reply).mockResolvedValue({
      stream: (async function* () {
        yield { type: 'Finish', reason: 'done', token_state: tokenState } as MessageEvent;
      })(),
    } as never);

    await registry
      .getController('s1')
      .handleSubmit('', [{ path: '/tmp/biorouter-pasted-images/scan.png', kind: 'image' }]);

    expect(reply).toHaveBeenCalledTimes(1);
    expect(vi.mocked(reply).mock.calls[0][0].body.user_message.content).toEqual([
      { type: 'image', data: 'B64-image', mimeType: 'image/png' },
    ]);
  });

  it('submits hidden system messages as agent-visible repair context', async () => {
    const registry = new ChatStreamRegistry();
    vi.mocked(resumeAgent).mockResolvedValue({ data: { session: session('s1') } } as never);
    vi.mocked(reply).mockResolvedValue({
      stream: (async function* () {
        yield { type: 'Finish', reason: 'done', token_state: tokenState } as MessageEvent;
      })(),
    } as never);

    const hiddenMessage: Message = {
      id: 'hidden-repair',
      role: 'user',
      created: 1,
      content: [{ type: 'text', text: 'repair this artifact' }],
      metadata: { userVisible: false, agentVisible: true },
    };

    const controller = registry.getController('s1');
    await controller.submitSystemMessage(hiddenMessage);

    expect(reply).toHaveBeenCalledTimes(1);
    expect(vi.mocked(reply).mock.calls[0][0].body.user_message).toEqual(hiddenMessage);
    expect(controller.getSnapshot().messages).toContainEqual(hiddenMessage);
  });

  it('keeps a submitted stream running after the last view unsubscribes', async () => {
    const registry = new ChatStreamRegistry();
    const controlled = createControlledStream();
    vi.mocked(resumeAgent).mockResolvedValue({ data: { session: session('s1') } } as never);
    vi.mocked(reply).mockResolvedValue({ stream: controlled.stream } as never);

    const controller = registry.getController('s1');
    const unsubscribe = controller.subscribe(() => undefined);
    const submit = controller.handleSubmit('analyze this');
    await flush();
    unsubscribe();

    controlled.push({
      type: 'Message',
      message: assistantMessage('a1', 'working'),
      token_state: tokenState,
    });
    await flush();

    expect(registry.getRunningSnapshot()).toMatchObject([
      {
        sessionId: 's1',
        chatState: ChatState.Streaming,
      },
    ]);

    controlled.push({ type: 'Finish', reason: 'done', token_state: tokenState });
    controlled.close();
    await submit;

    expect(registry.getRunningSnapshot()[0]).toMatchObject({
      sessionId: 's1',
      chatState: ChatState.Idle,
    });

    await vi.advanceTimersByTimeAsync(1600);
    expect(registry.getRunningSnapshot()).toEqual([]);
  });

  it('tracks two session streams independently', async () => {
    const registry = new ChatStreamRegistry();
    const first = createControlledStream();
    const second = createControlledStream();

    vi.mocked(resumeAgent)
      .mockResolvedValueOnce({ data: { session: session('s1') } } as never)
      .mockResolvedValueOnce({ data: { session: session('s2') } } as never);
    vi.mocked(reply)
      .mockResolvedValueOnce({ stream: first.stream } as never)
      .mockResolvedValueOnce({ stream: second.stream } as never);

    const firstSubmit = registry.getController('s1').handleSubmit('first');
    const secondSubmit = registry.getController('s2').handleSubmit('second');
    await flush();

    first.push({
      type: 'Message',
      message: assistantMessage('a1', 'first running'),
      token_state: tokenState,
    });
    second.push({
      type: 'Message',
      message: assistantMessage('a2', 'second running'),
      token_state: tokenState,
    });
    await flush();

    expect(
      registry
        .getRunningSnapshot()
        .map((entry) => entry.sessionId)
        .sort()
    ).toEqual(['s1', 's2']);

    first.push({ type: 'Finish', reason: 'done', token_state: tokenState });
    second.push({ type: 'Finish', reason: 'done', token_state: tokenState });
    first.close();
    second.close();
    await Promise.all([firstSubmit, secondSubmit]);
  });
  it('steers a running turn through the soft-interrupt route', async () => {
    const registry = new ChatStreamRegistry();
    const controlled = createControlledStream();
    vi.mocked(resumeAgent).mockResolvedValue({ data: { session: session('s1') } } as never);
    vi.mocked(reply).mockResolvedValue({ stream: controlled.stream } as never);
    vi.mocked(interrupt).mockResolvedValue({ data: undefined } as never);

    const controller = registry.getController('s1');
    const submit = controller.handleSubmit('plot the data');
    await flush();

    await expect(controller.steer('actually, use R')).resolves.toBe(true);
    expect(vi.mocked(interrupt).mock.calls[0][0].body).toEqual({
      session_id: 's1',
      text: 'actually, use R',
    });
    // The steer is not appended locally — the agent streams it back once consumed.
    const steerAppended = controller
      .getSnapshot()
      .messages.some((m) =>
        m.content.some((c) => c.type === 'text' && c.text === 'actually, use R')
      );
    expect(steerAppended).toBe(false);

    controlled.push({ type: 'Finish', reason: 'done', token_state: tokenState });
    controlled.close();
    await submit;
  });

  it('refuses to steer when no turn is running', async () => {
    const registry = new ChatStreamRegistry();
    vi.mocked(resumeAgent).mockResolvedValue({ data: { session: session('s1') } } as never);
    const controller = registry.getController('s1');
    await controller.loadSession();

    await expect(controller.steer('too late')).resolves.toBe(false);
    expect(interrupt).not.toHaveBeenCalled();
  });

  it('dispatches the canonical divergence event after an edited-message divergence', async () => {
    const registry = new ChatStreamRegistry();
    const sourceSessionId = 'edit-diverge-source';
    const userMessage: Message = {
      id: 'u1',
      role: 'user',
      created: 10,
      content: [{ type: 'text', text: 'original prompt' }],
      metadata: { userVisible: true, agentVisible: true },
    };
    vi.mocked(resumeAgent).mockResolvedValue({
      data: { session: { ...session(sourceSessionId), conversation: [userMessage] } },
    } as never);
    vi.mocked(editMessage).mockResolvedValue({ data: { sessionId: 's2' } } as never);
    const onDiverged = vi.fn();
    window.addEventListener('session-diverged', onDiverged);

    const controller = registry.getController(sourceSessionId);
    await controller.loadSession();
    await controller.onMessageUpdate('u1', 'updated prompt', 'diverge');

    expect(editMessage).toHaveBeenCalledWith({
      path: { session_id: sourceSessionId },
      body: { timestamp: 10, editType: 'diverge' },
      throwOnError: true,
    });
    expect(onDiverged).toHaveBeenCalledTimes(1);
    expect((onDiverged.mock.calls[0][0] as CustomEvent).detail).toEqual({
      // The ORIGIN session. 'session-diverged' is a window broadcast heard by
      // every mounted chat, and newSessionId names a session that doesn't exist
      // in the UI yet — so listeners identify the single chat that should
      // navigate by the session the user diverged FROM.
      sessionId: sourceSessionId,
      newSessionId: 's2',
      shouldStartAgent: true,
      editedMessage: 'updated prompt',
    });

    window.removeEventListener('session-diverged', onDiverged);
  });

  it('reports a rejected soft interrupt so the caller can fall back', async () => {
    const registry = new ChatStreamRegistry();
    const controlled = createControlledStream();
    vi.mocked(resumeAgent).mockResolvedValue({ data: { session: session('s1') } } as never);
    vi.mocked(reply).mockResolvedValue({ stream: controlled.stream } as never);
    // The turn ended between the click and the POST: the server answers 409.
    vi.mocked(interrupt).mockRejectedValue(new Error('conflict'));

    const controller = registry.getController('s1');
    const submit = controller.handleSubmit('plot the data');
    await flush();

    await expect(controller.steer('actually, use R')).resolves.toBe(false);

    controlled.push({ type: 'Finish', reason: 'done', token_state: tokenState });
    controlled.close();
    await submit;
  });

  it('sends a client-generated turn_id so an SSE re-POST dedupes (BR-62b)', async () => {
    const registry = new ChatStreamRegistry();
    vi.mocked(resumeAgent).mockResolvedValue({ data: { session: session('s1') } } as never);
    vi.mocked(reply).mockResolvedValue({
      stream: (async function* () {
        yield { type: 'Finish', reason: 'done', token_state: tokenState } as MessageEvent;
      })(),
    } as never);

    await registry.getController('s1').handleSubmit('hello');

    const turnId = vi.mocked(reply).mock.calls[0][0].body.turn_id;
    expect(typeof turnId).toBe('string');
    expect((turnId as string).length).toBeGreaterThan(0);
  });

  it('uses a fresh turn_id for each turn (BR-62b)', async () => {
    const registry = new ChatStreamRegistry();
    vi.mocked(resumeAgent).mockResolvedValue({ data: { session: session('s1') } } as never);
    vi.mocked(reply).mockResolvedValue({
      stream: (async function* () {
        yield { type: 'Finish', reason: 'done', token_state: tokenState } as MessageEvent;
      })(),
    } as never);

    const controller = registry.getController('s1');
    await controller.handleSubmit('first');
    await controller.handleSubmit('second');

    const firstId = vi.mocked(reply).mock.calls[0][0].body.turn_id;
    const secondId = vi.mocked(reply).mock.calls[1][0].body.turn_id;
    expect(firstId).not.toEqual(secondId);
  });

  it('reliably cancels the running turn on Stop via /agent/cancel (BR-62b)', async () => {
    const registry = new ChatStreamRegistry();
    const controlled = createControlledStream();
    vi.mocked(resumeAgent).mockResolvedValue({ data: { session: session('s1') } } as never);
    vi.mocked(reply).mockResolvedValue({ stream: controlled.stream } as never);

    const controller = registry.getController('s1');
    const submit = controller.handleSubmit('long task');
    await flush();

    controller.stopStreaming();

    expect(cancelTurn).toHaveBeenCalledTimes(1);
    expect(vi.mocked(cancelTurn).mock.calls[0][0].body).toEqual({ session_id: 's1' });
    expect(controller.getSnapshot().chatState).toBe(ChatState.Idle);

    controlled.close();
    await submit;
  });
});

/**
 * Progressive conversation loading.
 *
 * A resume used to take ~5.1s on a real 355-message session, of which ~4.6s was
 * starting 9 extensions that contribute 359 bytes to what the user reads. The
 * transcript now paints first and the agent follows. These tests pin the two
 * things that makes fragile: the ORDER, and the fact that a submit landing in
 * the gap must not be eaten.
 */
describe('ChatStreamRegistry — progressive loading', () => {
  /** A resume mock whose model+extension phase we can hold open on demand. */
  function stagedResume(sessionId = 's1') {
    let releaseAgent: (() => void) | null = null;
    const agentGate = new Promise<void>((resolve) => {
      releaseAgent = resolve;
    });

    vi.mocked(resumeAgent).mockImplementation(async (opts: unknown) => {
      const body = (opts as { body: { load_model_and_extensions: boolean } }).body;
      if (!body.load_model_and_extensions) {
        // Phase 1: the transcript. Fast.
        return { data: { session: session(sessionId) } } as never;
      }
      // Phase 2: the slow half.
      await agentGate;
      return {
        data: {
          session: session(sessionId),
          extension_results: [{ name: 'developer', success: true, error: null }],
        },
      } as never;
    });

    return { releaseAgent: () => releaseAgent!() };
  }

  it('paints the transcript without waiting for the model and extensions', async () => {
    const registry = new ChatStreamRegistry();
    const { releaseAgent } = stagedResume('prog-paint');

    const controller = registry.getController('prog-paint');
    await controller.loadSession();

    // The conversation is up and interactive while the agent is still loading.
    expect(controller.getSnapshot().session).toMatchObject({ id: 'prog-paint' });
    expect(controller.getSnapshot().chatState).toBe(ChatState.Idle);
    expect(controller.getSnapshot().agentReady).toBe(false);

    releaseAgent();
    await flush();
    expect(controller.getSnapshot().agentReady).toBe(true);
  });

  it('fetches the transcript alone first, then the agent — in that order', async () => {
    const registry = new ChatStreamRegistry();
    const { releaseAgent } = stagedResume('prog-order');

    const controller = registry.getController('prog-order');
    await controller.loadSession();

    const flags = vi
      .mocked(resumeAgent)
      .mock.calls.map(
        (c) => (c[0] as { body: { load_model_and_extensions: boolean } }).body
          .load_model_and_extensions
      );
    expect(flags[0]).toBe(false);
    expect(flags).toContain(true);

    releaseAgent();
    await flush();
  });

  // THE one that matters. A snappy UI that eats your first message is a
  // downgrade, not an upgrade.
  it('HOLDS a submit made before the agent is ready — never drops it', async () => {
    const registry = new ChatStreamRegistry();
    const { releaseAgent } = stagedResume('prog-hold');
    vi.mocked(reply).mockResolvedValue({
      stream: (async function* () {
        yield { type: 'Finish', reason: 'stop' } as MessageEvent;
      })(),
    } as never);

    const controller = registry.getController('prog-hold');
    await controller.loadSession();
    expect(controller.getSnapshot().agentReady).toBe(false);

    // The user types into a transcript whose agent is still starting.
    const submit = controller.handleSubmit('the first thing I typed');
    await flush();

    // Not dropped, and not invisible: the message is already in the transcript
    // and the chat reads as working, so the user has feedback.
    const painted = controller.getSnapshot().messages;
    expect(painted[painted.length - 1]).toMatchObject({ role: 'user' });
    expect(controller.getSnapshot().chatState).not.toBe(ChatState.Idle);
    // ...but it has NOT been sent to an agent that cannot serve it yet.
    expect(reply).not.toHaveBeenCalled();

    releaseAgent();
    await submit;

    // It went out the moment the agent landed, intact.
    expect(reply).toHaveBeenCalledTimes(1);
    const body = vi.mocked(reply).mock.calls[0][0].body as { user_message: Message };
    expect(body.user_message.content).toMatchObject([
      { type: 'text', text: 'the first thing I typed' },
    ]);
  });

  it('lets the user Stop a turn parked on a slow agent load, without sending it', async () => {
    const registry = new ChatStreamRegistry();
    const { releaseAgent } = stagedResume('prog-stop');

    const controller = registry.getController('prog-stop');
    await controller.loadSession();

    const submit = controller.handleSubmit('never mind');
    await flush();
    controller.stopStreaming();

    releaseAgent();
    await submit;

    // Cancelled while parked: the request must never reach the model.
    expect(reply).not.toHaveBeenCalled();
  });

  it('loads the agent even when the transcript came from cache', async () => {
    const registry = new ChatStreamRegistry();
    const { releaseAgent } = stagedResume('cached-1');

    // Prime the module-level LRU by loading once...
    const first = registry.getController('cached-1');
    await first.loadSession();
    releaseAgent();
    await flush();
    vi.mocked(resumeAgent).mockClear();

    // ...then reach the session through a brand new controller, which takes the
    // cache fast-path. It must STILL load the agent: this path previously could
    // reach a submit having never started a single extension.
    const { releaseAgent: release2 } = stagedResume('cached-1');
    const fresh = new ChatStreamRegistry().getController('cached-1');
    await fresh.loadSession();

    const agentCalls = vi
      .mocked(resumeAgent)
      .mock.calls.filter(
        (c) => (c[0] as { body: { load_model_and_extensions: boolean } }).body
          .load_model_and_extensions
      );
    expect(agentCalls.length).toBeGreaterThan(0);
    release2();
    await flush();
  });

  it('only loads the agent once, however many callers ask', async () => {
    const registry = new ChatStreamRegistry();
    const { releaseAgent } = stagedResume('prog-once');

    const controller = registry.getController('prog-once');
    await Promise.all([
      controller.loadSession(),
      controller.loadSession(),
      controller.loadSession(),
    ]);
    releaseAgent();
    await flush();
    await controller.loadSession();

    const agentCalls = vi
      .mocked(resumeAgent)
      .mock.calls.filter(
        (c) => (c[0] as { body: { load_model_and_extensions: boolean } }).body
          .load_model_and_extensions
      );
    expect(agentCalls).toHaveLength(1);
  });

  // A failed agent load must not park submits forever — readiness means
  // "settled", not "succeeded".
  it('becomes ready, with an inline error, when the agent fails to load', async () => {
    const registry = new ChatStreamRegistry();
    vi.mocked(resumeAgent).mockImplementation(async (opts: unknown) => {
      const body = (opts as { body: { load_model_and_extensions: boolean } }).body;
      if (!body.load_model_and_extensions) {
        return { data: { session: session('prog-agentfail') } } as never;
      }
      throw new Error('extension manager unavailable');
    });

    const controller = registry.getController('prog-agentfail');
    await controller.loadSession();
    await flush();

    // The transcript survived a failed agent load...
    expect(controller.getSnapshot().session).toMatchObject({ id: 'prog-agentfail' });
    expect(controller.getSnapshot().sessionLoadError).toBeUndefined();
    // ...and the failure is visible rather than silent.
    expect(controller.getSnapshot().agentReady).toBe(true);
    expect(controller.getSnapshot().turnError).toMatchObject({ code: 'agent_load_failed' });
  });

  it('still reports a full resume failure as a session-level error', async () => {
    const registry = new ChatStreamRegistry();
    vi.mocked(resumeAgent).mockRejectedValue(new Error('session database unavailable'));

    const controller = registry.getController('prog-resumefail');
    await controller.loadSession();

    expect(controller.getSnapshot().sessionLoadError).toContain('session database unavailable');
    expect(controller.getSnapshot().chatState).toBe(ChatState.Idle);
  });
});

describe('ChatStreamRegistry turn timestamps', () => {
  it('stamps lastMessageAt per live Message event and advances it on the next one', async () => {
    const registry = new ChatStreamRegistry();
    vi.mocked(resumeAgent).mockResolvedValue({ data: { session: session('ts1') } } as never);

    const controlled = createControlledStream();
    vi.mocked(reply).mockResolvedValue({ stream: controlled.stream } as never);

    const controller = registry.getController('ts1');
    const submitted = controller.handleSubmit('go');
    await vi.advanceTimersByTimeAsync(0);
    await flush();

    // Submit stamps a turn origin so the indicator has a fallback before any
    // message lands.
    const afterSubmit = controller.getSnapshot();
    expect(afterSubmit.turnStartedAt).toBeTypeOf('number');
    expect(afterSubmit.lastMessageAt).toBeUndefined();

    controlled.push({
      type: 'Message',
      message: assistantMessage('m1', 'first'),
      token_state: tokenState,
    } as MessageEvent);
    await flush();
    const first = controller.getSnapshot().lastMessageAt;
    expect(first).toBeTypeOf('number');

    vi.advanceTimersByTime(5000);
    controlled.push({
      type: 'Message',
      message: assistantMessage('m2', 'second'),
      token_state: tokenState,
    } as MessageEvent);
    await flush();
    const second = controller.getSnapshot().lastMessageAt;
    expect(second).toBeGreaterThan(first as number);

    controlled.push({ type: 'Finish', reason: 'done', token_state: tokenState } as MessageEvent);
    controlled.close();
    await submitted;
  });

  it('clears both timestamps when the turn finishes', async () => {
    const registry = new ChatStreamRegistry();
    vi.mocked(resumeAgent).mockResolvedValue({ data: { session: session('ts2') } } as never);
    vi.mocked(reply).mockResolvedValue({
      stream: (async function* () {
        yield {
          type: 'Message',
          message: assistantMessage('m1', 'hello'),
          token_state: tokenState,
        } as MessageEvent;
        yield { type: 'Finish', reason: 'done', token_state: tokenState } as MessageEvent;
      })(),
    } as never);

    const controller = registry.getController('ts2');
    await controller.handleSubmit('go');
    await flush();

    const snapshot = controller.getSnapshot();
    expect(snapshot.chatState).toBe(ChatState.Idle);
    expect(snapshot.turnStartedAt).toBeUndefined();
    expect(snapshot.lastMessageAt).toBeUndefined();
  });

  it('clears both timestamps when the turn errors', async () => {
    const registry = new ChatStreamRegistry();
    vi.mocked(resumeAgent).mockResolvedValue({ data: { session: session('ts3') } } as never);
    vi.mocked(reply).mockResolvedValue({
      stream: (async function* () {
        yield {
          type: 'Message',
          message: assistantMessage('m1', 'partial'),
          token_state: tokenState,
        } as MessageEvent;
        yield {
          type: 'Error',
          error: 'provider exploded',
          code: 'inference_failed',
          token_state: tokenState,
        } as unknown as MessageEvent;
      })(),
    } as never);

    const controller = registry.getController('ts3');
    await controller.handleSubmit('go');
    await flush();

    const snapshot = controller.getSnapshot();
    expect(snapshot.turnStartedAt).toBeUndefined();
    expect(snapshot.lastMessageAt).toBeUndefined();
  });

  it('clears both timestamps when the user stops the turn', async () => {
    const registry = new ChatStreamRegistry();
    vi.mocked(resumeAgent).mockResolvedValue({ data: { session: session('ts4') } } as never);

    const controlled = createControlledStream();
    vi.mocked(reply).mockResolvedValue({ stream: controlled.stream } as never);

    const controller = registry.getController('ts4');
    void controller.handleSubmit('go');
    await vi.advanceTimersByTimeAsync(0);
    await flush();

    controlled.push({
      type: 'Message',
      message: assistantMessage('m1', 'working'),
      token_state: tokenState,
    } as MessageEvent);
    await flush();
    expect(controller.getSnapshot().lastMessageAt).toBeTypeOf('number');

    controller.stopStreaming();
    await flush();

    const snapshot = controller.getSnapshot();
    expect(snapshot.chatState).toBe(ChatState.Idle);
    expect(snapshot.turnStartedAt).toBeUndefined();
    expect(snapshot.lastMessageAt).toBeUndefined();

    controlled.close();
  });

  it('does not stamp lastMessageAt when replaying a saved session', async () => {
    // The historical-session guarantee at the store level: loading a transcript
    // must never look like a live event, or a replay would inherit a clock.
    const registry = new ChatStreamRegistry();
    vi.mocked(resumeAgent).mockResolvedValue({
      data: {
        session: {
          ...session('ts5'),
          conversation: [assistantMessage('old-1', 'from last week')],
          message_count: 1,
        },
      },
    } as never);

    const controller = registry.getController('ts5');
    await controller.loadSession();
    await flush();

    const snapshot = controller.getSnapshot();
    expect(snapshot.messages.length).toBeGreaterThan(0);
    expect(snapshot.lastMessageAt).toBeUndefined();
    expect(snapshot.turnStartedAt).toBeUndefined();
  });
});
