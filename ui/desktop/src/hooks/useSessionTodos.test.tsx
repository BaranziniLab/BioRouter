import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Message, Session } from '../api';
import { useSessionTodos } from './useSessionTodos';

const mocks = vi.hoisted(() => ({ getSession: vi.fn(), headers: vi.fn() }));
vi.mock('../api', () => ({ getSession: mocks.getSession }));
vi.mock('../utils/userAction', () => ({ userActionHeaders: mocks.headers }));

const task = (text: string, status = 'pending') => ({ id: '1', text, status });
const session = (id: string, items: ReturnType<typeof task>[] = []) =>
  ({ id, extension_data: { 'todo.v1': { items } } }) as unknown as Session;
function exchange(id: string): Message[] {
  return [
    {
      role: 'assistant',
      created: 0,
      metadata: { agentVisible: true, userVisible: true },
      content: [
        {
          type: 'toolRequest',
          id,
          toolCall: { status: 'success', value: { name: 'todo__todo_update' } },
        },
        { type: 'toolResponse', id, toolResult: { status: 'success', value: { content: [] } } },
      ],
    },
  ] as Message[];
}

beforeEach(() => {
  mocks.getSession.mockReset().mockResolvedValue({ data: session('a', [task('Current')]) });
  mocks.headers.mockReset().mockResolvedValue({ 'X-User-Action': 'synthetic-proof' });
});

describe('live summary checklist', () => {
  it('fetches metadata with user proof only while the summary is open', async () => {
    const { result, rerender } = renderHook(
      ({ open }) => useSessionTodos('a', undefined, [], open),
      { initialProps: { open: false } }
    );
    expect(mocks.getSession).not.toHaveBeenCalled();
    rerender({ open: true });
    await waitFor(() => expect(result.current.items[0]?.text).toBe('Current'));
    expect(mocks.getSession).toHaveBeenCalledWith(
      expect.objectContaining({
        path: { session_id: 'a' },
        query: { metadata_only: true },
        headers: { 'X-User-Action': 'synthetic-proof' },
      })
    );
  });
  it('refreshes after successful updates and reflects reopened or cleared work', async () => {
    const { result, rerender } = renderHook(
      ({ messages }) => useSessionTodos('a', undefined, messages, true),
      { initialProps: { messages: [] as Message[] } }
    );
    await waitFor(() => expect(result.current.items[0]?.text).toBe('Current'));
    mocks.getSession.mockResolvedValueOnce({ data: session('a', [task('Renamed', 'completed')]) });
    rerender({ messages: exchange('first') });
    await waitFor(() => expect(result.current.items[0]?.status).toBe('completed'));
    mocks.getSession.mockResolvedValueOnce({
      data: session('a', [task('Reopened', 'in_progress')]),
    });
    rerender({ messages: exchange('second') });
    await waitFor(() => expect(result.current.items[0]?.text).toBe('Reopened'));
    mocks.getSession.mockResolvedValueOnce({ data: session('a') });
    rerender({ messages: exchange('third') });
    await waitFor(() => expect(result.current.items).toEqual([]));
  });
  it('does not refetch for prose or a replayed tool response', async () => {
    const { result, rerender } = renderHook(
      ({ messages }) => useSessionTodos('a', undefined, messages, true),
      { initialProps: { messages: exchange('same') } }
    );
    await waitFor(() => expect(result.current.loading).toBe(false));
    rerender({
      messages: [
        ...exchange('same'),
        {
          role: 'assistant',
          created: 0,
          content: [{ type: 'text', text: 'More prose' }],
        } as Message,
      ],
    });
    await act(() => new Promise((resolve) => setTimeout(resolve, 110)));
    expect(mocks.getSession).toHaveBeenCalledTimes(1);
  });
  it('clears the old session immediately and ignores a late response', async () => {
    let finish!: (value: unknown) => void;
    mocks.getSession.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        })
    );
    const initial = session('a', [task('Old session')]);
    const { result, rerender } = renderHook(
      ({ id, loaded }) => useSessionTodos(id, loaded, [], true),
      { initialProps: { id: 'a', loaded: initial as Session | undefined } }
    );
    await waitFor(() => expect(mocks.getSession).toHaveBeenCalledTimes(1));
    mocks.getSession.mockResolvedValueOnce({ data: session('b') });
    rerender({ id: 'b', loaded: undefined });
    expect(result.current.items).toEqual([]);
    await waitFor(() => expect(mocks.getSession).toHaveBeenCalledTimes(2));
    await act(async () => finish({ data: session('a', [task('Late old task')]) }));
    expect(result.current.items).toEqual([]);
  });
  it('ignores superseded same-session responses even if the client does not honor abort', async () => {
    let finish!: (value: unknown) => void;
    mocks.getSession.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        })
    );
    const { result, rerender } = renderHook(
      ({ messages }) => useSessionTodos('a', undefined, messages, true),
      { initialProps: { messages: [] as Message[] } }
    );
    await waitFor(() => expect(mocks.getSession).toHaveBeenCalledTimes(1));
    mocks.getSession.mockResolvedValueOnce({ data: session('a', [task('Newest', 'completed')]) });
    rerender({ messages: exchange('new') });
    await waitFor(() => expect(result.current.items[0]?.text).toBe('Newest'));
    await act(async () => finish({ data: session('a', [task('Stale')]) }));
    expect(result.current.items[0].text).toBe('Newest');
  });
  it('keeps last known progress with an explicit error and supports retry', async () => {
    mocks.getSession.mockRejectedValueOnce(new Error('offline'));
    const loaded = session('a', [task('Last known')]);
    const { result } = renderHook(() => useSessionTodos('a', loaded, [], true));
    await waitFor(() => expect(result.current.error).toBe(true));
    expect(result.current.items[0].text).toBe('Last known');
    act(() => result.current.refresh());
    await waitFor(() => expect(result.current.items[0].text).toBe('Current'));
    expect(result.current.error).toBe(false);
  });
  it('cancels an in-flight refresh on close', async () => {
    mocks.getSession.mockImplementationOnce(() => new Promise(() => {}));
    const { rerender } = renderHook(({ open }) => useSessionTodos('a', undefined, [], open), {
      initialProps: { open: true },
    });
    await waitFor(() => expect(mocks.getSession).toHaveBeenCalledTimes(1));
    const signal = mocks.getSession.mock.calls[0][0].signal;
    rerender({ open: false });
    expect(signal.aborted).toBe(true);
  });
});
