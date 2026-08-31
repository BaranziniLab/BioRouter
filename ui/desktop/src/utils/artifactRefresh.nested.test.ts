import { describe, expect, it } from 'vitest';
import type { Message } from '../api';
import { artifactRefreshEvents, refreshEventMatches } from './artifactRefresh';

const executed = (
  tool = 'agent_drafter__build_app',
  args: unknown = JSON.stringify({ id: 'qa' }),
  status = 'ok'
) => ({ tool, args, status, result_bytes: 80 });
function wrapper(
  records: unknown,
  resultExtras: Record<string, unknown> = {},
  id = 'wrapper-1'
): Message[] {
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
            value: {
              name: 'code_execution__execute_code',
              arguments: {
                code: 'await agent_drafter.build_app({id:"qa"})',
                tool_graph: [
                  {
                    tool: 'agent_drafter/build_app',
                    description: 'Build the planned-only app',
                    depends_on: [],
                  },
                ],
              },
            },
          },
        },
        {
          type: 'toolResponse',
          id,
          toolResult: {
            status: 'success',
            value: { content: [], _meta: { 'biorouter/tool-calls': records }, ...resultExtras },
          },
        },
      ],
    },
  ] as Message[];
}
const appEvents = (messages: Message[]) =>
  artifactRefreshEvents(messages, 'a').filter((event) => event.appId);

describe('executed nested app refresh telemetry', () => {
  it.each([
    ['agent_drafter__build_app', { id: 'qa' }],
    ['agent_drafter__configure_app', { id: 'qa', greeting: 'Updated greeting' }],
    ['agent_drafter__update_app', { id: 'qa', content: 'New entry' }],
    ['agent_drafter__update_app', { id: 'qa', path: 'manifest.json', content: '{}' }],
    ['agent_drafter__update_app', { id: 'qa', path: 'assets/icon.svg', content: '<svg/>' }],
  ])('refreshes only the app named by successful executed %s telemetry', (tool, args) => {
    const events = appEvents(wrapper([executed(tool, JSON.stringify(args))]));
    expect(events).toHaveLength(1);
    expect(refreshEventMatches(events[0], 'app:qa')).toBe(true);
    expect(refreshEventMatches(events[0], 'app:other')).toBe(false);
    expect(refreshEventMatches(events[0], 'file:/tmp/result.txt')).toBe(false);
  });

  it('retains ordinary file-only opaque execution behavior alongside actual app hints', () => {
    const events = artifactRefreshEvents(wrapper([executed()]), 'a');
    expect(
      events.filter((event) => refreshEventMatches(event, 'file:/tmp/result.txt'))
    ).toHaveLength(1);
    expect(events.filter((event) => refreshEventMatches(event, 'app:qa'))).toHaveLength(1);
    expect(events.every((event) => event.paths.length === 0)).toBe(true);
  });

  it('never guesses an app from code, planned tool_graph, response text, or app-path metadata', () => {
    const messages = wrapper(undefined, {
      content: [{ type: 'text', text: 'Built app qa; agent_drafter__build_app({"id":"qa"})' }],
      _meta: { 'biorouter/app-paths': ['/apps/qa/'] },
    });
    const events = artifactRefreshEvents(messages, 'a');
    expect(events).toEqual([{ id: 'wrapper-1', paths: [], checkActiveFile: true }]);
  });

  it.each([
    executed(undefined, undefined, 'error'),
    executed(undefined, undefined, 'success'),
    executed(undefined, '{"id":"qa"'),
    executed(undefined, { id: 'qa' }),
    executed(undefined, 'null'),
    executed(undefined, '[]'),
    executed(undefined, '"qa"'),
    executed(undefined, '{"id":123}'),
    executed(undefined, '{}'),
    executed('other__build_app'),
    executed('agent_drafter__preview_app'),
    executed('agent_drafter__create_app'),
    executed('agent_drafter__update_app', '{"id":"qa","path":"src/main.ts"}'),
  ])('ignores unsuccessful, malformed, unknown, or unbundled nested records: %j', (record) => {
    expect(appEvents(wrapper([record]))).toEqual([]);
  });

  it.each([null, {}, '[]', [null, 1, 'build_app']])(
    'ignores malformed metadata arrays: %j',
    (records) => {
      expect(appEvents(wrapper(records))).toEqual([]);
    }
  );

  it.each([{ isError: true }, { is_error: true }])(
    'preserves the successful-parent boundary: %j',
    (failure) => {
      expect(artifactRefreshEvents(wrapper([executed()], failure), 'a')).toEqual([]);
    }
  );

  it('does not process nested success when the parent response envelope failed', () => {
    const messages = wrapper([executed()]);
    (messages[0].content[1] as { toolResult: unknown }).toolResult = {
      status: 'error',
      value: { _meta: { 'biorouter/tool-calls': [executed()] } },
    };
    expect(artifactRefreshEvents(messages, 'a')).toEqual([]);
  });

  it('requires local provenance for both the wrapper request and its result', () => {
    const combined = wrapper([executed()])[0];
    for (const foreignIndex of [0, 1]) {
      const messages = combined.content.map((content, index) => ({
        ...combined,
        content: [content],
        metadata: {
          ...combined.metadata,
          ...(index === foreignIndex ? { provenance: { fromSessionId: 'other' } } : {}),
        },
      })) as Message[];
      expect(artifactRefreshEvents(messages, 'a')).toEqual([]);
    }
  });

  it('deduplicates replayed telemetry per outer call and app, without collapsing later executions', () => {
    const first = wrapper([
      executed(),
      executed(),
      executed('agent_drafter__update_app', '{"id":"qa"}'),
    ]);
    const replayed = appEvents([...first, ...first]);
    expect(replayed).toHaveLength(1);
    const later = appEvents([...first, ...wrapper([executed()], {}, 'wrapper-2')]);
    expect(later).toHaveLength(2);
    expect(new Set(later.map((event) => event.id)).size).toBe(2);
  });

  it('keeps separate matching hints for two actually updated apps in one wrapper', () => {
    const events = appEvents(wrapper([executed(), executed(undefined, '{"id":"second"}')]));
    expect(events.map((event) => event.appId).sort()).toEqual(['qa', 'second']);
    expect(events.some((event) => refreshEventMatches(event, 'app:planned-only'))).toBe(false);
  });
});
