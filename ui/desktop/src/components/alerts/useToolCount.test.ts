import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getCallableToolCount } from '../../api';
import { CATALOG_CHANGED_EVENT } from '../../utils/catalogSubscription';
import { useToolCount } from './useToolCount';

vi.mock('../../api', () => ({ getCallableToolCount: vi.fn() }));

const tools = (count: number) => ({ data: { count } });

describe('useToolCount', () => {
  beforeEach(() => vi.resetAllMocks());

  it('refreshes the actual tool count after hot attach and detach', async () => {
    vi.mocked(getCallableToolCount).mockResolvedValueOnce(tools(4) as never);
    const { result } = renderHook(() => useToolCount('chat', true));
    await waitFor(() => expect(result.current).toBe(4));

    vi.mocked(getCallableToolCount).mockResolvedValueOnce(tools(9) as never);
    act(() => window.dispatchEvent(new Event(CATALOG_CHANGED_EVENT)));
    await waitFor(() => expect(result.current).toBe(9));

    vi.mocked(getCallableToolCount).mockResolvedValueOnce(tools(4) as never);
    act(() => window.dispatchEvent(new Event(CATALOG_CHANGED_EVENT)));
    await waitFor(() => expect(result.current).toBe(4));
  });

  it('does not let a stale response overwrite the newer catalog', async () => {
    let finishOld!: (value: never) => void;
    vi.mocked(getCallableToolCount).mockImplementationOnce(
      () =>
        new Promise<never>((resolve) => {
          finishOld = resolve;
        })
    );
    const { result } = renderHook(() => useToolCount('chat', true));
    vi.mocked(getCallableToolCount).mockResolvedValueOnce(tools(8) as never);
    act(() => window.dispatchEvent(new Event(CATALOG_CHANGED_EVENT)));
    await waitFor(() => expect(result.current).toBe(8));
    await act(async () => finishOld(tools(2) as never));
    expect(result.current).toBe(8);
  });

  it('clears the old count until the selected chat is ready', async () => {
    vi.mocked(getCallableToolCount).mockResolvedValueOnce(tools(7) as never);
    const { result, rerender } = renderHook(({ session, ready }) => useToolCount(session, ready), {
      initialProps: { session: 'first', ready: true },
    });
    await waitFor(() => expect(result.current).toBe(7));
    rerender({ session: 'second', ready: false });
    expect(result.current).toBeNull();
    act(() => window.dispatchEvent(new Event(CATALOG_CHANGED_EVENT)));
    expect(getCallableToolCount).toHaveBeenCalledTimes(1);
    vi.mocked(getCallableToolCount).mockResolvedValueOnce(tools(3) as never);
    rerender({ session: 'second', ready: true });
    await waitFor(() => expect(result.current).toBe(3));
  });

  it('removes its catalog listener when unmounted', async () => {
    vi.mocked(getCallableToolCount).mockResolvedValueOnce(tools(1) as never);
    const { result, unmount } = renderHook(() => useToolCount('chat', true));
    await waitFor(() => expect(result.current).toBe(1));
    unmount();
    act(() => window.dispatchEvent(new Event(CATALOG_CHANGED_EVENT)));
    expect(getCallableToolCount).toHaveBeenCalledTimes(1);
  });

  it('refreshes after a completed agent turn can change this chat tool set', async () => {
    vi.mocked(getCallableToolCount)
      .mockResolvedValueOnce(tools(4) as never)
      .mockResolvedValueOnce(tools(7) as never);
    const { result } = renderHook(() => useToolCount('chat', true));
    await waitFor(() => expect(result.current).toBe(4));

    act(() => window.dispatchEvent(new Event('message-stream-finished')));

    await waitFor(() => expect(result.current).toBe(7));
  });

  it('refreshes only for a session-tools change addressed to this chat', async () => {
    vi.mocked(getCallableToolCount)
      .mockResolvedValueOnce(tools(2) as never)
      .mockResolvedValueOnce(tools(5) as never);
    const { result } = renderHook(() => useToolCount('chat', true));
    await waitFor(() => expect(result.current).toBe(2));

    act(() =>
      window.dispatchEvent(
        new CustomEvent('session-tools:changed', { detail: { sessionId: 'another-chat' } })
      )
    );
    expect(getCallableToolCount).toHaveBeenCalledTimes(1);

    act(() =>
      window.dispatchEvent(
        new CustomEvent('session-tools:changed', { detail: { sessionId: 'chat' } })
      )
    );
    await waitFor(() => expect(result.current).toBe(5));
  });

  it('keeps the last known count when the SDK returns an error response', async () => {
    vi.mocked(getCallableToolCount)
      .mockResolvedValueOnce(tools(7) as never)
      .mockResolvedValueOnce({ error: { message: 'unavailable' } } as never);
    const { result } = renderHook(() => useToolCount('chat', true));
    await waitFor(() => expect(result.current).toBe(7));

    await act(async () => {
      window.dispatchEvent(new Event(CATALOG_CHANGED_EVENT));
    });

    expect(getCallableToolCount).toHaveBeenCalledTimes(2);
    expect(result.current).toBe(7);
  });

  it('keeps the last known count when the SDK throws', async () => {
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    vi.mocked(getCallableToolCount)
      .mockResolvedValueOnce(tools(6) as never)
      .mockRejectedValueOnce(new Error('daemon unavailable'));
    const { result } = renderHook(() => useToolCount('chat', true));
    await waitFor(() => expect(result.current).toBe(6));

    await act(async () => {
      window.dispatchEvent(new Event(CATALOG_CHANGED_EVENT));
    });

    expect(consoleError).toHaveBeenCalled();
    expect(result.current).toBe(6);
    consoleError.mockRestore();
  });
});
