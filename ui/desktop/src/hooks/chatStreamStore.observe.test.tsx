import { describe, expect, it, vi, afterEach } from 'vitest';

const mocks = vi.hoisted(() => ({
  observeSessionEvents: vi.fn(),
}));

vi.mock('../api', async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>;
  return { ...actual, observeSessionEvents: mocks.observeSessionEvents };
});

import { defaultChatStreamRegistry } from './chatStreamStore';

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
