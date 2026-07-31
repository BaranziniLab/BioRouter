import { beforeEach, describe, expect, it, vi, afterEach } from 'vitest';
import type { Message, MessageEvent, Session, TokenState } from '../api';

const mocks = vi.hoisted(() => ({
  observeSessionEvents: vi.fn(),
  // The rest of the surface a driving controller touches. Mocked so the
  // ownership tests below can put a REAL /reply turn in flight without any of
  // it reaching the generated client (and so nothing in this file depends on
  // `fetch` or on how fast the machine is).
  reply: vi.fn(),
  resumeAgent: vi.fn(),
  cancelTurn: vi.fn(async () => ({ data: { cancelled: true } })),
  getSession: vi.fn(async () => ({ data: null })),
  listApps: vi.fn(async () => ({ data: { apps: [] } })),
  listSessions: vi.fn(async () => ({ data: { sessions: [] } })),
  // Part of agent readiness, which a submit AWAITS: left real, it reaches the
  // generated client and the turn never launches.
  updateFromSession: vi.fn(async () => ({ data: {} })),
  updateSessionUserWorkflowValues: vi.fn(async () => ({ data: {} })),
}));

vi.mock('../api', async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>;
  return { ...actual, ...mocks };
});

import { ChatStreamRegistry, defaultChatStreamRegistry } from './chatStreamStore';

/** The `{ stream }` shape the generated .sse.get returns: an AsyncIterable of
 * parsed MessageEvent frames. */
async function* frames() {
  yield { type: 'UpdateConversation', conversation: [], token_state: {} };
  yield {
    type: 'Message',
    token_state: {},
    message: {
      role: 'assistant',
      created: 1,
      content: [{ type: 'text', text: 'from the observed turn' }],
      metadata: { userVisible: true, agentVisible: true },
    },
  };
  yield { type: 'Finish', reason: 'stop', token_state: {} };
}

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

/** A stream whose frames and end are driven by the test — the same helper the
 * sibling store suite uses, so an observer connection can be held open, dropped
 * mid-turn, or closed cleanly on demand. */
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

/** Puts a real user-driven `/reply` turn in flight on a fresh controller and
 * hands back the pieces the ownership tests assert on. */
async function drivingController(sessionId: string) {
  const registry = new ChatStreamRegistry();
  const driving = createControlledStream();
  mocks.resumeAgent.mockResolvedValue({ data: { session: session(sessionId) } });
  mocks.reply.mockResolvedValue({ stream: driving.stream });

  const controller = registry.getController(sessionId);
  const submit = controller.handleSubmit('drive this myself');
  await vi.advanceTimersByTimeAsync(0);
  expect(mocks.reply).toHaveBeenCalledTimes(1);
  const signal = mocks.reply.mock.calls[0][0].signal as AbortSignal;
  expect(signal.aborted).toBe(false);

  return { controller, driving, submit, signal };
}

beforeEach(() => {
  // Each test configures these itself; a leftover `…Once` queue from the
  // previous one must not decide what this one sees. `mockReset` restores the
  // implementation passed to `vi.fn(impl)`, so the always-succeed stubs above
  // survive it.
  mocks.observeSessionEvents.mockReset();
  mocks.reply.mockReset();
  mocks.resumeAgent.mockReset();
});

afterEach(() => {
  // Belt for the suspenders in each test's `finally`. A test that TIMES OUT
  // never reaches its own `finally` — its await simply never settles — so fake
  // timers would stay installed and the NEXT test would hang on a real-timer
  // wait that can no longer fire. That is one failure masquerading as two, and
  // it is what an `afterEach` (which does run) is for.
  vi.useRealTimers();
});

describe('ChatStreamController.observeSession', () => {
  afterEach(() => vi.clearAllMocks());

  it('renders observer frames without owning a /reply stream', async () => {
    // ⚠ The plan wrote this as a bare `await controller.observeSession()`. That
    // can only ever time out, and the third test below is why: the observer
    // loop is DELIBERATELY non-terminating — "the observer stream never
    // completes from the client's point of view", so a stream that ends must be
    // re-subscribed. An `observeSession()` that resolved after one drain would
    // fail that test. So drive the first connect + drain on fake timers, assert
    // the two things this test is actually about (the call shape, and observed
    // frames landing in the store the chat renders from), then stop the loop and
    // let it unwind. Both assertions are the plan's, verbatim.
    vi.useFakeTimers();
    try {
      mocks.observeSessionEvents.mockResolvedValue({ stream: frames() });

      const controller = defaultChatStreamRegistry.getController('obs-1');
      const running = controller.observeSession();
      await vi.advanceTimersByTimeAsync(0); // first connect + drain

      expect(mocks.observeSessionEvents).toHaveBeenCalledWith(
        expect.objectContaining({ path: { session_id: 'obs-1' } })
      );
      const text = JSON.stringify(controller.getSnapshot().messages);
      expect(text).toContain('from the observed turn');

      controller.stopObserving();
      await vi.advanceTimersByTimeAsync(1100); // release the parked backoff
      await running; // the loop must actually exit, not leak
    } finally {
      vi.useRealTimers();
    }
  });

  it('stops reconnecting once aborted', async () => {
    // First connect yields a stream that ends; observeSession must not spin a
    // reconnect after stopObserving() aborts it.
    mocks.observeSessionEvents.mockResolvedValue({ stream: frames() });
    const controller = defaultChatStreamRegistry.getController('obs-2');
    const done = controller.observeSession();
    controller.stopObserving();
    await done;
    // EXACTLY one: the initial subscribe, and no reconnect after the abort.
    //
    // ⚠ This assertion used to be `toBeLessThanOrEqual(2)`, which is satisfied by
    // ZERO — an `observeSession()` that returned immediately without ever calling
    // the API passed it, and so did one that never existed as anything but a stub.
    // A ceiling is only half a bound; assert the floor too. Do not relax it back:
    // the count IS deterministic, because `observeSession`'s synchronous prefix
    // issues the first request before its first `await`, and the loop's staleness
    // check (`if (!this.observing || …) return;`) runs before the backoff sleep.
    expect(mocks.observeSessionEvents).toHaveBeenCalledTimes(1);
  });

  it('re-subscribes when the observed stream ends and it was NOT stopped', async () => {
    // The other half of the same behaviour, and the one the ≤2 ceiling was
    // gesturing at: "the observer stream never completes from the client's point
    // of view", so a stream that ends must be re-opened. Without this, a
    // subagent tab goes silent the first time the daemon's broadcast receiver is
    // replaced, and looks like a dead child.
    vi.useFakeTimers();
    try {
      mocks.observeSessionEvents.mockResolvedValue({ stream: frames() });
      const controller = defaultChatStreamRegistry.getController('obs-3');
      void controller.observeSession();
      await vi.advanceTimersByTimeAsync(0); // first connect + drain
      expect(mocks.observeSessionEvents).toHaveBeenCalledTimes(1);
      await vi.advanceTimersByTimeAsync(1100); // past the 1 s first backoff
      expect(mocks.observeSessionEvents.mock.calls.length).toBeGreaterThanOrEqual(2);
      controller.stopObserving();
    } finally {
      vi.useRealTimers();
    }
  });
});

describe('ChatStreamController.observeSession — who owns the socket', () => {
  afterEach(() => vi.clearAllMocks());

  it('can re-attach after Stop tore the observer loop down mid-stream', async () => {
    vi.useFakeTimers();
    try {
      const registry = new ChatStreamRegistry();
      const first = createControlledStream();
      const second = createControlledStream();
      mocks.observeSessionEvents
        .mockResolvedValueOnce({ stream: first.stream })
        .mockResolvedValueOnce({ stream: second.stream });

      const controller = registry.getController('obs-stop-midstream');
      void controller.observeSession();
      await vi.advanceTimersByTimeAsync(0);
      expect(mocks.observeSessionEvents).toHaveBeenCalledTimes(1);

      // Stop, pressed while the observed agent is mid-turn — which is exactly
      // when a user presses it. It bumps `activeStreamId` and aborts, so the
      // observer loop unwinds through its staleness check and is gone.
      controller.stopStreaming();
      first.close();
      await vi.advanceTimersByTimeAsync(0);

      // The FLAG has to unwind with the loop. If `observing` outlives it, every
      // later attach short-circuits on the idempotence guard and the tab is dead
      // until the window reloads — `getController` retains this controller for
      // the life of the renderer, and the daemon re-annotating the tab is the
      // ordinary way an attach is retried.
      void controller.observeSession();
      await vi.advanceTimersByTimeAsync(0);
      expect(mocks.observeSessionEvents).toHaveBeenCalledTimes(2);

      controller.stopObserving();
      second.close();
      await vi.advanceTimersByTimeAsync(0);
    } finally {
      vi.useRealTimers();
    }
  });

  it('does not resurrect the observer stream when Stop lands during backoff', async () => {
    vi.useFakeTimers();
    try {
      const registry = new ChatStreamRegistry();
      mocks.observeSessionEvents.mockResolvedValue({ stream: frames() });

      const controller = registry.getController('obs-stop-backoff');
      void controller.observeSession();
      await vi.advanceTimersByTimeAsync(0);
      expect(mocks.observeSessionEvents).toHaveBeenCalledTimes(1);

      // Parked in the 1 s backoff now — the mirror of the case above, and wrong
      // in the opposite direction. Stop has to mean stop: it already fired
      // `cancelTurn` at a session another agent drives, so a loop that wakes up
      // and re-subscribes anyway leaves Stop doing nothing except harm.
      controller.stopStreaming();
      await vi.advanceTimersByTimeAsync(5000);
      expect(mocks.observeSessionEvents).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it('does not tear down a live user turn when a daemon frame re-attaches the tab', async () => {
    vi.useFakeTimers();
    try {
      const idle = createControlledStream();
      mocks.observeSessionEvents.mockResolvedValue({ stream: idle.stream });
      const { controller, driving, submit, signal } = await drivingController('obs-driver-attach');

      // `ChatGroupsContext` calls `observeSession()` on every qualifying
      // workspace frame — `annotate_tab` for a tab that already exists included,
      // which is input the daemon fully controls. If the user has taken this tab
      // over, that frame must not abort the socket their live turn streams on.
      void controller.observeSession();
      await vi.advanceTimersByTimeAsync(0);
      expect(mocks.observeSessionEvents).not.toHaveBeenCalled();
      expect(signal.aborted).toBe(false);

      driving.push({
        type: 'Message',
        message: assistantMessage('a1', 'still driving'),
        token_state: tokenState,
      });
      driving.push({ type: 'Finish', reason: 'done', token_state: tokenState });
      driving.close();
      await submit;
      expect(JSON.stringify(controller.getSnapshot().messages)).toContain('still driving');
    } finally {
      vi.useRealTimers();
    }
  });

  it('lets the user take an observed tab over, and re-attach once their turn ends', async () => {
    vi.useFakeTimers();
    try {
      const registry = new ChatStreamRegistry();
      const observed = createControlledStream();
      const driving = createControlledStream();
      const reattached = createControlledStream();
      mocks.observeSessionEvents
        .mockResolvedValueOnce({ stream: observed.stream })
        .mockResolvedValueOnce({ stream: reattached.stream });
      mocks.resumeAgent.mockResolvedValue({ data: { session: session('obs-takeover') } });
      mocks.reply.mockResolvedValue({ stream: driving.stream });

      const controller = registry.getController('obs-takeover');
      void controller.observeSession();
      await vi.advanceTimersByTimeAsync(0);
      expect(mocks.observeSessionEvents).toHaveBeenCalledTimes(1);
      const observerSignal = mocks.observeSessionEvents.mock.calls[0][0].signal as AbortSignal;

      // Typing in a subagent tab takes it over — the case `submitPreparedMessage`
      // clears the flag for, and the case §4.3 names ("until the tab detaches or
      // the user takes the session over"). The observer holds a live
      // `abortController` and its subscription leaves the load parked in
      // `LoadingConversation`; both are read as "a turn is already running", so
      // the submit is dropped on the floor before it ever reaches the line that
      // does the converting.
      const submit = controller.handleSubmit('actually, do it this way');
      await vi.advanceTimersByTimeAsync(0);
      expect(mocks.reply).toHaveBeenCalledTimes(1);
      // Taking over closes the feed it took the tab from: two live sockets
      // writing into one transcript is the thing the streamId dance exists to
      // avoid, and the observer's would otherwise stay open until it happened to
      // notice it was stale.
      expect(observerSignal.aborted).toBe(true);

      driving.push({
        type: 'Message',
        message: assistantMessage('a1', 'driving it myself now'),
        token_state: tokenState,
      });
      driving.push({ type: 'Finish', reason: 'done', token_state: tokenState });
      driving.close();
      await submit;
      expect(JSON.stringify(controller.getSnapshot().messages)).toContain('driving it myself now');

      // The other half of clearing the flag: the tab can go back to observing
      // once the user's own turn is over, instead of short-circuiting forever on
      // the idempotence guard.
      void controller.observeSession();
      await vi.advanceTimersByTimeAsync(0);
      expect(mocks.observeSessionEvents).toHaveBeenCalledTimes(2);

      controller.stopObserving();
      observed.close();
      reattached.close();
      await vi.advanceTimersByTimeAsync(0);
    } finally {
      vi.useRealTimers();
    }
  });

  it('does not paint a transport error when the observed connection drops', async () => {
    vi.useFakeTimers();
    try {
      const registry = new ChatStreamRegistry();
      const dropped = createControlledStream();
      const reconnected = createControlledStream();
      mocks.observeSessionEvents
        .mockResolvedValueOnce({ stream: dropped.stream })
        .mockResolvedValueOnce({ stream: reconnected.stream });

      const controller = registry.getController('obs-dropped-feed');
      void controller.observeSession();
      await vi.advanceTimersByTimeAsync(0);
      dropped.push({
        type: 'Message',
        message: assistantMessage('a1', 'mid turn'),
        token_state: tokenState,
      });
      await vi.advanceTimersByTimeAsync(0);

      // The feed drops mid-turn: the stream ends with no `Finish`. For a driver
      // that means the turn died and the red "connection closed before Biorouter
      // received a completion status" card is right (pinned in the sibling suite).
      // For an observer it is the ordinary reconnect trigger this whole loop is
      // built on — the session is untouched and the next subscribe re-snapshots
      // it — so painting a turn failure the user cannot act on, and then silently
      // repairing it behind the card, is telling them something untrue.
      dropped.close();
      await vi.advanceTimersByTimeAsync(0);
      expect(controller.getSnapshot().turnError).toBeUndefined();

      await vi.advanceTimersByTimeAsync(1100);
      expect(mocks.observeSessionEvents).toHaveBeenCalledTimes(2);
      expect(controller.getSnapshot().turnError).toBeUndefined();

      controller.stopObserving();
      reconnected.close();
      await vi.advanceTimersByTimeAsync(0);
    } finally {
      vi.useRealTimers();
    }
  });

  it('does not abort a live user turn when stopObserving is called on a driver', async () => {
    vi.useFakeTimers();
    try {
      const { controller, driving, submit, signal } = await drivingController('obs-driver-detach');

      // The documented caller is "tab closed", and a closed tab does not cancel
      // the turn it was showing (BR-62b: the server keeps running either way).
      // On a controller that never observed anything, detaching is a no-op.
      controller.stopObserving();
      expect(signal.aborted).toBe(false);

      driving.push({
        type: 'Message',
        message: assistantMessage('a1', 'survived the detach'),
        token_state: tokenState,
      });
      driving.push({ type: 'Finish', reason: 'done', token_state: tokenState });
      driving.close();
      await submit;
      expect(JSON.stringify(controller.getSnapshot().messages)).toContain('survived the detach');
    } finally {
      vi.useRealTimers();
    }
  });
});
