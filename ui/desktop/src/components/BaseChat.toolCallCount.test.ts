import { describe, expect, it } from 'vitest';
import type { Message } from '../api';
import { countSessionToolCalls } from './BaseChat';

const message = (content: Message['content']): Message => ({
  id: crypto.randomUUID(),
  role: 'assistant',
  created: 1,
  metadata: { userVisible: true, agentVisible: true },
  content,
});

const toolRequest = (id: string, name: string): Message['content'][number] => ({
  type: 'toolRequest',
  id,
  toolCall: { status: 'success', value: { name, arguments: {} } },
});

const toolResponse = (
  id: string,
  calls: unknown,
  dropped?: unknown
): Message['content'][number] => ({
  type: 'toolResponse',
  id,
  toolResult: {
    status: 'success',
    value: {
      is_error: false,
      content: [],
      _meta: {
        'biorouter/tool-calls': calls,
        'biorouter/tool-calls-dropped': dropped,
      },
    },
  },
});

describe('countSessionToolCalls', () => {
  it('counts direct and recorded nested Code Execution calls', () => {
    const messages = [
      message([
        toolRequest('read', 'developer__text_editor'),
        toolRequest('code', 'code_execution__execute_code'),
      ]),
      message([
        toolResponse('code', [
          { tool: 'developer__shell', status: 'ok' },
          { tool: 'developer__text_editor', status: 'ok' },
        ]),
      ]),
    ];

    expect(countSessionToolCalls(messages)).toBe(4);
  });

  it('includes executed calls omitted from the visible telemetry', () => {
    const messages = [
      message([toolRequest('code', 'multi_tool_use__execute_code')]),
      message([toolResponse('code', [{ tool: 'developer__shell', status: 'ok' }], 3)]),
    ];

    expect(countSessionToolCalls(messages)).toBe(5);
  });

  it('does not grant nested-count semantics to a custom tool with the same suffix', () => {
    const messages = [
      message([toolRequest('code', 'custom__execute_code')]),
      message([toolResponse('code', [{ tool: 'developer__shell' }], 3)]),
    ];

    expect(countSessionToolCalls(messages)).toBe(1);
  });

  it.each([
    {
      condition: 'duplicate requests',
      messages: [
        message([
          toolRequest('code', 'code_execution__execute_code'),
          toolRequest('code', 'code_execution__execute_code'),
        ]),
        message([toolResponse('code', [{ tool: 'developer__shell' }], 3)]),
      ],
      expected: 2,
    },
    {
      condition: 'mixed-name requests sharing an id',
      messages: [
        message([
          toolRequest('code', 'code_execution__execute_code'),
          toolRequest('code', 'records__lookup'),
        ]),
        message([toolResponse('code', [{ tool: 'developer__shell' }], 3)]),
      ],
      expected: 2,
    },
    {
      condition: 'duplicate responses',
      messages: [
        message([toolRequest('code', 'code_execution__execute_code')]),
        message([
          toolResponse('code', [{ tool: 'developer__shell' }], 3),
          toolResponse('code', [{ tool: 'developer__shell' }], 3),
        ]),
      ],
      expected: 1,
    },
    {
      condition: 'a response before its request',
      messages: [
        message([toolResponse('code', [{ tool: 'developer__shell' }], 3)]),
        message([toolRequest('code', 'code_execution__execute_code')]),
      ],
      expected: 1,
    },
    {
      condition: 'a mismatched response id',
      messages: [
        message([toolRequest('code', 'code_execution__execute_code')]),
        message([toolResponse('different', [{ tool: 'developer__shell' }], 3)]),
      ],
      expected: 1,
    },
  ])('rejects nested metadata for $condition', ({ messages, expected }) => {
    expect(countSessionToolCalls(messages)).toBe(expected);
  });

  it('ignores malformed records, invalid dropped counts, and metadata from other tools', () => {
    const messages = [
      message([
        toolRequest('code', 'code_execution__execute_code'),
        toolRequest('lookup', 'records__lookup'),
      ]),
      message([
        toolResponse('code', [null, { tool: '' }, { tool: 'developer__shell' }], -2),
        toolResponse('lookup', [{ tool: 'forged__nested' }], 100),
      ]),
    ];

    expect(countSessionToolCalls(messages)).toBe(3);
  });
});
