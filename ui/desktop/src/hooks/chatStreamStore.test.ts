import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ChatState } from '../types/chatState';
import { ChatStreamRegistry } from './chatStreamStore';
import type { Message, MessageEvent, Session, TokenState } from '../api';
import { cancelTurn, editMessage, interrupt, reply, resumeAgent } from '../api';

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
  vi.mocked(interrupt).mockReset();
  vi.mocked(cancelTurn).mockReset();
  vi.mocked(editMessage).mockReset();
  vi.mocked(cancelTurn).mockResolvedValue({ data: { cancelled: true } } as never);
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
