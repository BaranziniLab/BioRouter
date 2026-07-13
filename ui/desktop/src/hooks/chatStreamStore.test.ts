import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ChatState } from '../types/chatState';
import { ChatStreamRegistry } from './chatStreamStore';
import type { Message, MessageEvent, Session, TokenState } from '../api';
import { interrupt, reply, resumeAgent } from '../api';

vi.mock('../api', async () => {
  return {
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
});
