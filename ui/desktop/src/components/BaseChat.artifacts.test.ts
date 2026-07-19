import { describe, expect, it } from 'vitest';
import type { Message } from '../api';
import {
  collectArtifactsFromMessages,
  getDefaultArtifactPanelWidth,
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

  it('collects external resource links as click-only preview artifacts', () => {
    const request = visibleMessage([
      {
        type: 'toolRequest',
        id: 'tool-link',
        toolCall: {
          status: 'success',
          value: { name: 'reports__publish', arguments: {} },
        },
      },
    ]);
    const response: Message = {
      id: crypto.randomUUID(),
      role: 'tool',
      created: 2,
      metadata: { userVisible: false, agentVisible: true },
      content: [
        {
          type: 'toolResponse',
          id: 'tool-link',
          toolResult: {
            status: 'success',
            value: {
              is_error: false,
              content: [
                {
                  uri: 'https://example.test/report.html',
                  name: 'report',
                  title: 'Study report',
                },
              ],
            },
          },
        },
      ],
    };

    expect(collectArtifactsFromMessages([request, response])).toEqual([
      {
        kind: 'externalUrl',
        title: 'Study report',
        url: 'https://example.test/report.html',
      },
    ]);
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

  /**
   * R1-02 — camelCase is the wire spelling: rmcp serialises CallToolResult with
   * `rename_all = "camelCase"`, so a failed call arrives as `isError: true`.
   * Only successful tool responses may contribute artifacts.
   */
  it('ignores files named by a tool call that failed with the camelCase isError flag', () => {
    const failedResponse: Message = {
      id: crypto.randomUUID(),
      role: 'tool',
      created: 2,
      metadata: { userVisible: false, agentVisible: true },
      content: [
        {
          type: 'toolResponse',
          id: 't-camel-err',
          toolResult: {
            status: 'success',
            value: {
              isError: true,
              content: [{ type: 'text', text: 'could not write /work/report.md' }],
            },
          },
        },
      ],
    };
    const messages: Message[] = [
      writeRequest('t-camel-err', 'developer__text_editor', {
        command: 'write',
        path: '/work/report.md',
      }),
      failedResponse,
    ];

    expect(collectArtifactsFromMessages(messages, '/work')).toEqual([]);
  });

  it('does not auto-preview protocol examples or hidden repository metadata', () => {
    const messages = [
      visibleMessage([
        {
          type: 'text',
          text: 'These links require `file://` support. The folder also contains a hidden `.git` directory.',
        },
      ]),
    ];

    expect(collectArtifactsFromMessages(messages, '/Users/ada/Desktop')).toEqual([]);
  });

  it('still discovers a concrete file URI without trailing prose punctuation', () => {
    const messages = [
      visibleMessage([
        { type: 'text', text: 'Open file:///Users/ada/Desktop/results/report.pdf).' },
      ]),
    ];

    expect(collectArtifactsFromMessages(messages)).toEqual([
      {
        kind: 'file',
        title: 'report.pdf',
        path: '/Users/ada/Desktop/results/report.pdf',
      },
    ]);
  });

  it('does not extract web URLs or prefixes of non-artifact paths', () => {
    const messages = [
      visibleMessage([
        {
          type: 'text',
          text: 'Ignore https://example.com/report.pdf, /tmp/report.pdf.bak, and /tmp/report.pdf/notes.',
        },
      ]),
    ];

    expect(collectArtifactsFromMessages(messages)).toEqual([]);
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

  it('does not crash on a file whose name contains a literal percent sign', () => {
    // Regression: the agent writes a file like "results 100%.csv". basenameFromPath
    // called decodeURIComponent on it, which throws URIError: "URI malformed" on the
    // stray `%`. Because collectArtifactsFromMessages runs during chat render, that
    // throw crashed the whole app into the "Honk!" error boundary mid-task.
    const messages: Message[] = [
      writeRequest('t1', 'developer__text_editor', {
        command: 'write',
        path: '/work/results 100%.csv',
      }),
      textToolResponse('t1', 'ok'),
    ];

    expect(() => collectArtifactsFromMessages(messages, '/work')).not.toThrow();
    expect(collectArtifactsFromMessages(messages, '/work')).toEqual([
      { kind: 'file', title: 'results 100%.csv', path: '/work/results 100%.csv' },
    ]);
  });

  it('does not crash on a ui:// resource whose URI contains a stray percent sign', () => {
    const messages: Message[] = [
      writeRequest('t1', 'autovisualiser__show_chart', {}),
      hiddenToolResponse('t1', '<html><body>Chart</body></html>'),
    ];
    // Force the resource URI to carry an invalid percent-escape.
    (
      messages[1].content[0] as unknown as {
        toolResult: { value: { content: Array<{ resource: { uri: string } }> } };
      }
    ).toolResult.value.content[0].resource.uri = 'ui://charts/effect 50% panel.html';

    expect(() => collectArtifactsFromMessages(messages)).not.toThrow();
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

  it('resolves relative files named in assistant text from the session working directory', () => {
    const messages = visibleMessage([
      { type: 'text', text: 'Open the generated page at ./dist/index.html' },
    ]);

    expect(collectArtifactsFromMessages([messages], '/work/site')).toEqual([
      { kind: 'file', title: 'index.html', path: '/work/site/dist/index.html' },
    ]);
  });

  const dashboardRequest = (id: string): Message =>
    visibleMessage([
      {
        type: 'toolRequest',
        id,
        toolCall: {
          status: 'success',
          value: { name: 'autovisualiser__render_dashboard', arguments: {} },
        },
      },
    ]);

  const dashboardResponse = (id: string, uri: string, html: string): Message => ({
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
            content: [{ resource: { uri, mimeType: 'text/html', text: html } }],
          },
        },
      },
    ],
  });

  const userTurn = (text: string): Message => ({
    id: crypto.randomUUID(),
    role: 'user',
    created: 3,
    metadata: { userVisible: true, agentVisible: true },
    content: [{ type: 'text', text }],
  });

  it('collapses a report re-rendered within one turn down to its final version', () => {
    // The model called render_dashboard twice in one turn (a refine): same
    // ui://dashboard/<slug> URI, different bytes. Only the last should surface.
    const messages: Message[] = [
      dashboardRequest('d1'),
      dashboardResponse('d1', 'ui://dashboard/orbit-report', '<html><body>DRAFT</body></html>'),
      dashboardRequest('d2'),
      dashboardResponse(
        'd2',
        'ui://dashboard/orbit-report',
        '<html><body>FINAL — longer, refined report body</body></html>'
      ),
    ];

    const artifacts = collectArtifactsFromMessages(messages);

    expect(artifacts).toHaveLength(1);
    expect(artifacts[0]).toMatchObject({
      kind: 'html',
      html: '<html><body>FINAL — longer, refined report body</body></html>',
    });
  });

  it('keeps a dashboard the user refines in a LATER turn as its own entry', () => {
    const messages: Message[] = [
      dashboardRequest('d1'),
      dashboardResponse(
        'd1',
        'ui://dashboard/orbit-report',
        '<html><body>version one</body></html>'
      ),
      userTurn('make the title bigger'),
      dashboardRequest('d2'),
      dashboardResponse(
        'd2',
        'ui://dashboard/orbit-report',
        '<html><body>version two</body></html>'
      ),
    ];

    expect(collectArtifactsFromMessages(messages)).toHaveLength(2);
  });

  it('keeps two genuinely different dashboards produced in the same turn', () => {
    const messages: Message[] = [
      dashboardRequest('d1'),
      dashboardResponse('d1', 'ui://dashboard/orbit-report', '<html><body>orbit</body></html>'),
      dashboardRequest('d2'),
      dashboardResponse('d2', 'ui://dashboard/thermal-report', '<html><body>thermal</body></html>'),
    ];

    expect(collectArtifactsFromMessages(messages)).toHaveLength(2);
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

describe('getDefaultArtifactPanelWidth', () => {
  it('uses 48% of the available width for a squarer initial preview', () => {
    expect(getDefaultArtifactPanelWidth(1600)).toBe(768);
  });

  it('preserves the panel bounds and minimum chat width', () => {
    expect(getDefaultArtifactPanelWidth(900)).toBe(360);
    expect(getDefaultArtifactPanelWidth(3000)).toBe(920);
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
