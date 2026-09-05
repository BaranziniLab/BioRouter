import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  PENDING_RUN_TTL_MS,
  onTerminalRunRequest,
  resetTerminalRunChannelForTests,
  runInTerminal,
} from './terminalRunChannel';

beforeEach(() => {
  resetTerminalRunChannelForTests();
});

afterEach(() => {
  vi.useRealTimers();
  resetTerminalRunChannelForTests();
});

describe('terminalRunChannel', () => {
  it('delivers to a listening pane', () => {
    const handler = vi.fn();
    onTerminalRunRequest('tab-1', handler);

    runInTerminal('tab-1', 'ls -la');

    expect(handler).toHaveBeenCalledExactlyOnceWith('ls -la');
  });

  it('queues when no pane is listening, and drains on subscribe', () => {
    // The ordinary case: the chat has no terminal at all, so the click opens
    // the dock and the pane subscribes several commits later.
    runInTerminal('tab-1', 'ls -la');

    const handler = vi.fn();
    onTerminalRunRequest('tab-1', handler);

    expect(handler).toHaveBeenCalledExactlyOnceWith('ls -la');
  });

  it('drains a queue in the order it was filled', () => {
    runInTerminal('tab-1', 'first');
    runInTerminal('tab-1', 'second');

    const handler = vi.fn();
    onTerminalRunRequest('tab-1', handler);

    expect(handler.mock.calls).toEqual([['first'], ['second']]);
  });

  it('drains once — a second subscriber gets nothing', () => {
    runInTerminal('tab-1', 'ls');
    const first = vi.fn();
    onTerminalRunRequest('tab-1', first);
    const second = vi.fn();
    onTerminalRunRequest('tab-1', second);

    expect(first).toHaveBeenCalledOnce();
    expect(second).not.toHaveBeenCalled();
  });

  it('keys by DOCK, so one chat never runs a command in another chat', () => {
    // The property focus-based routing (terminalFocus.ts) cannot give: Run is
    // clicked in a specific chat even when another pane holds the cursor.
    const one = vi.fn();
    const two = vi.fn();
    onTerminalRunRequest('tab-1', one);
    onTerminalRunRequest('tab-2', two);

    runInTerminal('tab-2', 'echo two');

    expect(one).not.toHaveBeenCalled();
    expect(two).toHaveBeenCalledExactlyOnceWith('echo two');
  });

  it('routes to the newest listener when panes are mid-switch', () => {
    // React registers the arriving pane before the departing pane's cleanup
    // runs, and the arriving one is the pane the user can see.
    const leaving = vi.fn();
    const arriving = vi.fn();
    onTerminalRunRequest('tab-1', leaving);
    onTerminalRunRequest('tab-1', arriving);

    runInTerminal('tab-1', 'ls');

    expect(leaving).not.toHaveBeenCalled();
    expect(arriving).toHaveBeenCalledExactlyOnceWith('ls');
  });

  it('stops delivering after the pane unsubscribes', () => {
    const handler = vi.fn();
    const dispose = onTerminalRunRequest('tab-1', handler);
    dispose();

    runInTerminal('tab-1', 'ls');

    expect(handler).not.toHaveBeenCalled();
  });

  it('falls back to the queue once the last pane has gone', () => {
    const handler = vi.fn();
    onTerminalRunRequest('tab-1', handler)();
    runInTerminal('tab-1', 'ls');

    const next = vi.fn();
    onTerminalRunRequest('tab-1', next);
    expect(next).toHaveBeenCalledExactlyOnceWith('ls');
  });

  it('disposing one of two listeners leaves the other delivering', () => {
    const first = vi.fn();
    const second = vi.fn();
    const disposeFirst = onTerminalRunRequest('tab-1', first);
    onTerminalRunRequest('tab-1', second);
    disposeFirst();

    runInTerminal('tab-1', 'ls');

    expect(second).toHaveBeenCalledExactlyOnceWith('ls');
  });

  it('EXPIRES a queued command rather than ambushing a later terminal', () => {
    vi.useFakeTimers();
    runInTerminal('tab-1', 'rm -rf build');

    vi.advanceTimersByTime(PENDING_RUN_TTL_MS + 1);
    const handler = vi.fn();
    onTerminalRunRequest('tab-1', handler);

    expect(handler).not.toHaveBeenCalled();
  });

  it('still delivers a command queued just inside the window', () => {
    vi.useFakeTimers();
    runInTerminal('tab-1', 'ls');

    vi.advanceTimersByTime(PENDING_RUN_TTL_MS - 1);
    const handler = vi.fn();
    onTerminalRunRequest('tab-1', handler);

    expect(handler).toHaveBeenCalledExactlyOnceWith('ls');
  });

  it('bounds the queue so a dock that never appears cannot grow it', () => {
    for (let i = 0; i < 50; i += 1) runInTerminal('tab-1', `cmd-${i}`);

    const handler = vi.fn();
    onTerminalRunRequest('tab-1', handler);

    expect(handler.mock.calls.length).toBeLessThanOrEqual(4);
    // The most recent clicks are the ones kept.
    expect(handler).toHaveBeenLastCalledWith('cmd-49');
  });

  it('ignores a subscription with no dock key', () => {
    const handler = vi.fn();
    const dispose = onTerminalRunRequest(null, handler);
    runInTerminal('', 'ls');
    expect(handler).not.toHaveBeenCalled();
    expect(() => dispose()).not.toThrow();
  });
});
