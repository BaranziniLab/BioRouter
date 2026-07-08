import { describe, expect, it } from 'vitest';
import type { Message } from '../api';
import { collectArtifactsFromMessages } from './BaseChat';

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
});
