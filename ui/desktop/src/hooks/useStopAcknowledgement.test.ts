import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { STOP_ACK_MS, useStopAcknowledgement } from './useStopAcknowledgement';

/**
 * Issue 2 — "Stop and Send" / the Stop button is the most intrusive control in
 * the composer and gave the least feedback: the click ran `onStop()` and the
 * button did not change. Teardown is asynchronous (SSE abort + /agent/cancel),
 * so the press looked identical to a missed click for a beat.
 */
describe('useStopAcknowledgement', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('acknowledges on the press itself, not on the stop completing', () => {
    // A stop whose teardown never resolves — the acknowledgement must not wait
    // on it, which is the entire bug.
    const onStop = vi.fn();
    const { result } = renderHook(() => useStopAcknowledgement(onStop));

    expect(result.current.acknowledged).toBe(false);

    act(() => {
      void result.current.trigger();
    });

    expect(result.current.acknowledged).toBe(true);
    expect(onStop).toHaveBeenCalledTimes(1);
    expect(onStop).toHaveBeenCalledWith(false);
  });

  it('keeps the immediate acknowledgement while exposing asynchronous completion', async () => {
    let finishStop: (value: boolean) => void = () => {};
    const onStop = vi.fn(
      () =>
        new Promise<boolean>((resolve) => {
          finishStop = resolve;
        })
    );
    const { result } = renderHook(() => useStopAcknowledgement(onStop));

    let completion!: Promise<boolean>;
    act(() => {
      completion = result.current.trigger();
    });

    expect(result.current.acknowledged).toBe(true);
    expect(onStop).toHaveBeenCalledTimes(1);

    act(() => finishStop(true));
    await expect(completion).resolves.toBe(true);
  });

  it('reports a rejected stop as false without an unhandled rejection', async () => {
    const { result } = renderHook(() =>
      useStopAcknowledgement(async () => {
        throw new Error('cancel unavailable');
      })
    );

    let completion!: Promise<boolean>;
    act(() => {
      completion = result.current.trigger();
    });

    await expect(completion).resolves.toBe(false);
    expect(result.current.acknowledged).toBe(true);
  });

  it('forwards whether the caller will send a continuation', async () => {
    const onStop = vi.fn(async () => true);
    const { result } = renderHook(() => useStopAcknowledgement(onStop));

    await act(async () => {
      await result.current.trigger(true);
    });

    expect(onStop).toHaveBeenCalledWith(true);
  });

  it('retires itself, so it cannot outlive the turn it confirmed', () => {
    const { result } = renderHook(() => useStopAcknowledgement(vi.fn()));

    act(() => {
      void result.current.trigger();
    });
    act(() => void vi.advanceTimersByTime(STOP_ACK_MS - 1));
    expect(result.current.acknowledged).toBe(true);

    act(() => void vi.advanceTimersByTime(1));
    expect(result.current.acknowledged).toBe(false);
  });

  it('gives a second press a full-length confirmation, not the remainder of the first', () => {
    const onStop = vi.fn();
    const { result } = renderHook(() => useStopAcknowledgement(onStop));

    act(() => {
      void result.current.trigger();
    });
    act(() => void vi.advanceTimersByTime(STOP_ACK_MS - 10));
    act(() => {
      void result.current.trigger();
    });

    // The first window would have expired here; the restart keeps it up.
    act(() => void vi.advanceTimersByTime(20));
    expect(result.current.acknowledged).toBe(true);

    act(() => void vi.advanceTimersByTime(STOP_ACK_MS));
    expect(result.current.acknowledged).toBe(false);
    expect(onStop).toHaveBeenCalledTimes(2);
  });

  it('cancels its timer on unmount rather than setState-ing into a dead composer', () => {
    const { result, unmount } = renderHook(() => useStopAcknowledgement(vi.fn()));

    act(() => {
      void result.current.trigger();
    });
    unmount();

    expect(() => vi.advanceTimersByTime(STOP_ACK_MS * 2)).not.toThrow();
    expect(vi.getTimerCount()).toBe(0);
  });

  it('tolerates a missing onStop (a composer with no live turn)', async () => {
    const { result } = renderHook(() => useStopAcknowledgement(undefined));

    let completion!: Promise<boolean>;
    act(() => {
      completion = result.current.trigger();
    });
    await expect(completion).resolves.toBe(true);
    expect(result.current.acknowledged).toBe(true);
  });
});
