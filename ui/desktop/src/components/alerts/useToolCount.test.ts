import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getTools } from '../../api';
import { CATALOG_CHANGED_EVENT } from '../../utils/catalogSubscription';
import { useToolCount } from './useToolCount';

vi.mock('../../api', () => ({ getTools: vi.fn() }));

const tools = (count: number) => ({ data: Array.from({ length: count }, () => ({})) });

describe('useToolCount', () => {
  beforeEach(() => vi.resetAllMocks());

  it('refreshes the actual tool count after hot attach and detach', async () => {
    vi.mocked(getTools).mockResolvedValueOnce(tools(4) as never);
    const { result } = renderHook(() => useToolCount('chat', true));
    await waitFor(() => expect(result.current).toBe(4));

    vi.mocked(getTools).mockResolvedValueOnce(tools(9) as never);
    act(() => window.dispatchEvent(new Event(CATALOG_CHANGED_EVENT)));
    await waitFor(() => expect(result.current).toBe(9));

    vi.mocked(getTools).mockResolvedValueOnce(tools(4) as never);
    act(() => window.dispatchEvent(new Event(CATALOG_CHANGED_EVENT)));
    await waitFor(() => expect(result.current).toBe(4));
  });

  it('does not let a stale response overwrite the newer catalog', async () => {
    let finishOld!: (value: never) => void;
    vi.mocked(getTools).mockImplementationOnce(
      () =>
        new Promise<never>((resolve) => {
          finishOld = resolve;
        })
    );
    const { result } = renderHook(() => useToolCount('chat', true));
    vi.mocked(getTools).mockResolvedValueOnce(tools(8) as never);
    act(() => window.dispatchEvent(new Event(CATALOG_CHANGED_EVENT)));
    await waitFor(() => expect(result.current).toBe(8));
    await act(async () => finishOld(tools(2) as never));
    expect(result.current).toBe(8);
  });

  it('clears the old count until the selected chat is ready', async () => {
    vi.mocked(getTools).mockResolvedValueOnce(tools(7) as never);
    const { result, rerender } = renderHook(({ session, ready }) => useToolCount(session, ready), {
      initialProps: { session: 'first', ready: true },
    });
    await waitFor(() => expect(result.current).toBe(7));
    rerender({ session: 'second', ready: false });
    expect(result.current).toBeNull();
    act(() => window.dispatchEvent(new Event(CATALOG_CHANGED_EVENT)));
    expect(getTools).toHaveBeenCalledTimes(1);
    vi.mocked(getTools).mockResolvedValueOnce(tools(3) as never);
    rerender({ session: 'second', ready: true });
    await waitFor(() => expect(result.current).toBe(3));
  });

  it('removes its catalog listener when unmounted', async () => {
    vi.mocked(getTools).mockResolvedValueOnce(tools(1) as never);
    const { result, unmount } = renderHook(() => useToolCount('chat', true));
    await waitFor(() => expect(result.current).toBe(1));
    unmount();
    act(() => window.dispatchEvent(new Event(CATALOG_CHANGED_EVENT)));
    expect(getTools).toHaveBeenCalledTimes(1);
  });
});
