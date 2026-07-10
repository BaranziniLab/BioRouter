import { describe, expect, it } from 'vitest';
import type { Message } from '../api';
import {
  collectArtifactsFromMessages,
  getArtifactPanelExpansionContentWidth,
  shouldAutoRepairArtifact,
} from './BaseChat';
import { ChatState } from '../types/chatState';

const visibleMessage = (content: Message['content']): Message => ({
  id: crypto.randomUUID(),
  role: 'assistant',
  created: 1,
  metadata: { userVisible: true, agentVisible: true },
  content,
});

const hiddenToolResponse = (id: string, html: string): Message => ({
  id: crypto.randomUUID(),
  role: 'tool',
  created: 2,
  metadata: { userVisible: false, agentVisible: true },
  content: [
    {
      type: 'toolResponse',
      id,
      toolResult: {
        status: 'success',
        value: {
          is_error: false,
          content: [
            {
              resource: {
                uri: 'ui://chart.html',
                mimeType: 'text/html',
                text: html,
              },
            },
          ],
        },
      },
    },
  ],
});

describe('collectArtifactsFromMessages', () => {
  it('collects artifacts from tool responses paired with visible assistant tool requests', () => {
    const messages: Message[] = [
      visibleMessage([
        {
          type: 'toolRequest',
          id: 'tool-1',
          toolCall: {
            status: 'success',
            value: {
              name: 'autovisualiser__show_chart',
              arguments: {},
            },
          },
        },
      ]),
      hiddenToolResponse('tool-1', '<html><body>Chart</body></html>'),
    ];

    const artifacts = collectArtifactsFromMessages(messages);

    expect(artifacts).toHaveLength(1);
    expect(artifacts[0]).toMatchObject({
      kind: 'html',
      title: 'chart.html',
      html: '<html><body>Chart</body></html>',
    });
  });

  it('ignores orphaned hidden tool responses without a visible request', () => {
    expect(collectArtifactsFromMessages([hiddenToolResponse('tool-1', '<p>Hidden</p>')])).toEqual(
      []
    );
  });

  const writeRequest = (id: string, name: string, args: Record<string, unknown>): Message =>
    visibleMessage([
      {
        type: 'toolRequest',
        id,
        toolCall: { status: 'success', value: { name, arguments: args } },
      },
    ]);

  const textToolResponse = (id: string, text: string, isError = false): Message => ({
    id: crypto.randomUUID(),
    role: 'tool',
    created: 2,
    metadata: { userVisible: false, agentVisible: true },
    content: [
      {
        type: 'toolResponse',
        id,
        toolResult: {
          status: 'success',
          value: { is_error: isError, content: [{ type: 'text', text }] },
        },
      },
    ],
  });

  it('previews a markdown file the agent wrote', () => {
    const messages: Message[] = [
      writeRequest('t1', 'developer__text_editor', {
        command: 'write',
        path: '/work/report.md',
      }),
      textToolResponse('t1', 'wrote /work/report.md'),
    ];

    expect(collectArtifactsFromMessages(messages, '/work')).toEqual([
      { kind: 'file', title: 'report.md', path: '/work/report.md' },
    ]);
  });

  it('previews an R script and the image its shell run produced', () => {
    const messages: Message[] = [
      writeRequest('t1', 'developer__text_editor', { command: 'write', path: 'analysis.R' }),
      textToolResponse('t1', 'ok'),
      writeRequest('t2', 'developer__shell', { command: 'Rscript analysis.R -o volcano.png' }),
      textToolResponse('t2', 'done'),
    ];

    expect(collectArtifactsFromMessages(messages, '/work')).toEqual([
      { kind: 'file', title: 'analysis.R', path: '/work/analysis.R' },
      { kind: 'file', title: 'volcano.png', path: '/work/volcano.png' },
    ]);
  });

  it('does not preview a file whose write failed', () => {
    const messages: Message[] = [
      writeRequest('t1', 'developer__text_editor', { command: 'write', path: '/work/report.md' }),
      textToolResponse('t1', 'permission denied', true),
    ];

    expect(collectArtifactsFromMessages(messages, '/work')).toEqual([]);
  });

  it('does not preview a file the agent only viewed', () => {
    const messages: Message[] = [
      writeRequest('t1', 'developer__text_editor', { command: 'view', path: '/work/report.md' }),
      textToolResponse('t1', '# Report'),
    ];

    expect(collectArtifactsFromMessages(messages, '/work')).toEqual([]);
  });

  it('does not duplicate a file already named in the assistant text', () => {
    const messages: Message[] = [
      visibleMessage([
        { type: 'text', text: 'I saved it to /work/report.md' },
        {
          type: 'toolRequest',
          id: 't1',
          toolCall: {
            status: 'success',
            value: {
              name: 'developer__text_editor',
              arguments: { command: 'write', path: '/work/report.md' },
            },
          },
        },
      ]),
      textToolResponse('t1', 'ok'),
    ];

    expect(collectArtifactsFromMessages(messages, '/work')).toHaveLength(1);
  });
});

describe('getArtifactPanelExpansionContentWidth', () => {
  it('requests enough extra window width for a narrow split pane', () => {
    expect(getArtifactPanelExpansionContentWidth(1000, 980)).toBe(1092);
  });

  it('does not request expansion when the split pane already fits the artifact panel', () => {
    expect(getArtifactPanelExpansionContentWidth(1200, 1100)).toBeNull();
  });
});

describe('shouldAutoRepairArtifact', () => {
  const now = 1_000_000;

  it('auto-fixes while the agent is actively working, regardless of timing', () => {
    for (const state of [
      ChatState.Thinking,
      ChatState.Streaming,
      ChatState.WaitingForUserInput,
      ChatState.Compacting,
      ChatState.RestartingAgent,
    ]) {
      // lastActive is stale, but the live state alone is enough.
      expect(shouldAutoRepairArtifact(state, 0, now)).toBe(true);
    }
  });

  it('auto-fixes an artifact that fails just after a turn finishes (grace window)', () => {
    expect(shouldAutoRepairArtifact(ChatState.Idle, now - 2_000, now)).toBe(true);
  });

  it('does NOT resume a conversation that has been idle past the grace window', () => {
    // The failure surfaced long after the agent last worked — user housekeeping.
    expect(shouldAutoRepairArtifact(ChatState.Idle, now - 60_000, now)).toBe(false);
  });

  it('does NOT resume when reopening a saved conversation (never active this session)', () => {
    // lastAgentActiveAt is 0 (default) — an old artifact re-rendering on load
    // must not resume the finished chat.
    expect(shouldAutoRepairArtifact(ChatState.LoadingConversation, 0, now)).toBe(false);
    expect(shouldAutoRepairArtifact(ChatState.Idle, 0, now)).toBe(false);
  });
});
