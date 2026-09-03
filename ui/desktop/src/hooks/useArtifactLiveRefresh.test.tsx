import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Message } from '../api';
import type { ArtifactSource } from '../components/artifacts/artifactTypes';
import { useArtifactLiveRefresh } from './useArtifactLiveRefresh';
import * as refreshCollector from '../utils/artifactRefresh';

function exchange(id: string, path = '/tmp/result.txt'): Message[] {
  return [
    {
      role: 'assistant',
      created: 0,
      metadata: { agentVisible: true, userVisible: true },
      content: [
        {
          type: 'toolRequest',
          id,
          toolCall: {
            status: 'success',
            value: { name: 'developer__write_file', arguments: { path } },
          },
        },
        { type: 'toolResponse', id, toolResult: { status: 'success', value: { content: [] } } },
      ],
    },
  ] as Message[];
}
const file: ArtifactSource = { kind: 'file', path: '/tmp/result.txt', title: 'Result' };
const advance = () =>
  act(() => {
    vi.advanceTimersByTime(300);
  });
beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe('coalesced session-local artifact refresh', () => {
  it('refreshes a matching app after actual nested build telemetry, once per execution', () => {
    const app: ArtifactSource = {
      kind: 'externalUrl',
      title: 'QA app',
      url: 'http://127.0.0.1:64005/apps/qa/',
    };
    const build = (id: string, appId = 'qa'): Message[] =>
      [
        {
          role: 'assistant',
          created: 0,
          metadata: { agentVisible: true, userVisible: true },
          content: [
            {
              type: 'toolRequest',
              id,
              toolCall: {
                status: 'success',
                value: {
                  name: 'code_execution__execute_code',
                  arguments: { code: 'await agent_drafter.build_app(...)' },
                },
              },
            },
            {
              type: 'toolResponse',
              id,
              toolResult: {
                status: 'success',
                value: {
                  content: [],
                  _meta: {
                    'biorouter/tool-calls': [
                      {
                        tool: 'agent_drafter__build_app',
                        args: JSON.stringify({ id: appId }),
                        status: 'ok',
                        result_bytes: 80,
                      },
                    ],
                  },
                },
              },
            },
          ],
        },
      ] as Message[];
    const { result, rerender } = renderHook(
      ({ messages, sessionId }) => useArtifactLiveRefresh(sessionId, messages, app, '/tmp', true),
      { initialProps: { messages: [] as Message[], sessionId: 'a' } }
    );
    rerender({ messages: build('first'), sessionId: 'a' });
    advance();
    expect(result.current).toBe(1);
    rerender({ messages: [...build('first'), ...build('unrelated', 'other')], sessionId: 'a' });
    advance();
    expect(result.current).toBe(1);
    rerender({ messages: [...build('first'), ...build('second')], sessionId: 'a' });
    advance();
    expect(result.current).toBe(2);
    rerender({ messages: [...build('first'), ...build('second')], sessionId: 'b' });
    advance();
    expect(result.current).toBe(0);
  });
  it('does not scan history without a target or before session hydration', () => {
    const collect = vi.spyOn(refreshCollector, 'artifactRefreshEvents');
    try {
      const { result, rerender } = renderHook(
        ({ artifact, ready, messages }) =>
          useArtifactLiveRefresh('a', messages, artifact, '/tmp', ready),
        {
          initialProps: {
            artifact: file as ArtifactSource | null,
            ready: false,
            messages: exchange('restored'),
          },
        }
      );
      expect(collect).not.toHaveBeenCalled();
      rerender({ artifact: null, ready: true, messages: exchange('restored') });
      expect(collect).not.toHaveBeenCalled();
      rerender({ artifact: file, ready: true, messages: exchange('restored') });
      expect(collect).toHaveBeenCalledOnce();
      advance();
      expect(result.current).toBe(0);
    } finally {
      collect.mockRestore();
    }
  });
  it('baselines restored history; each new completion counts once and prose cannot cancel a queued refresh', () => {
    const { result, rerender } = renderHook(
      ({ messages }) => useArtifactLiveRefresh('a', messages, file, '/tmp', true),
      { initialProps: { messages: exchange('old') } }
    );
    advance();
    expect(result.current).toBe(0);
    rerender({ messages: [...exchange('old'), ...exchange('new')] });
    rerender({
      messages: [
        ...exchange('old'),
        ...exchange('new'),
        {
          role: 'assistant',
          metadata: { userVisible: true },
          content: [{ type: 'text', text: 'streaming' }],
        } as Message,
      ],
    });
    advance();
    expect(result.current).toBe(1);
    rerender({ messages: [...exchange('old'), ...exchange('new')] });
    advance();
    expect(result.current).toBe(1);
  });
  it('coalesces a burst of three successful writes and ignores unrelated output', () => {
    const { result, rerender } = renderHook(
      ({ messages }) => useArtifactLiveRefresh('a', messages, file, '/tmp', true),
      { initialProps: { messages: [] as Message[] } }
    );
    rerender({ messages: exchange('one') });
    act(() => {
      vi.advanceTimersByTime(80);
    });
    rerender({ messages: [...exchange('one'), ...exchange('two')] });
    rerender({ messages: [...exchange('one'), ...exchange('two'), ...exchange('three')] });
    advance();
    expect(result.current).toBe(1);
    rerender({
      messages: [
        ...exchange('one'),
        ...exchange('two'),
        ...exchange('three'),
        ...exchange('other', '/tmp/other.txt'),
      ],
    });
    advance();
    expect(result.current).toBe(1);
  });
  it('cancels pending work on session or active file switches', () => {
    const { result, rerender } = renderHook(
      ({ id, messages, artifact }) => useArtifactLiveRefresh(id, messages, artifact, '/tmp', true),
      { initialProps: { id: 'a', messages: [] as Message[], artifact: file } }
    );
    rerender({ id: 'a', messages: exchange('queued'), artifact: file });
    rerender({ id: 'b', messages: exchange('queued'), artifact: file });
    advance();
    expect(result.current).toBe(0);
    rerender({ id: 'b', messages: [...exchange('queued'), ...exchange('new')], artifact: file });
    rerender({
      id: 'b',
      messages: [...exchange('queued'), ...exchange('new')],
      artifact: { ...file, path: '/tmp/other.txt' },
    });
    advance();
    expect(result.current).toBe(0);
  });
  it('baselines delayed session hydration rather than replaying writes', () => {
    const { result, rerender } = renderHook(
      ({ messages, ready }) => useArtifactLiveRefresh('a', messages, file, '/tmp', ready),
      { initialProps: { messages: [] as Message[], ready: false } }
    );
    rerender({ messages: exchange('restored'), ready: true });
    advance();
    expect(result.current).toBe(0);
  });
  it('cancels timers on unmount', () => {
    const { rerender, unmount } = renderHook(
      ({ messages }) => useArtifactLiveRefresh('a', messages, file, '/tmp', true),
      { initialProps: { messages: [] as Message[] } }
    );
    rerender({ messages: exchange('queued') });
    expect(vi.getTimerCount()).toBe(1);
    unmount();
    expect(vi.getTimerCount()).toBe(0);
  });
});
