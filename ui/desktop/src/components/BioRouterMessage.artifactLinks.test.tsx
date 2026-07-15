import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { Message } from '../api';
import BioRouterMessage from './BioRouterMessage';

describe('BioRouterMessage artifact links', () => {
  it('opens the generated inline-code file from an assistant message', async () => {
    const onOpenArtifact = vi.fn();
    const message: Message = {
      id: 'weather-site-message',
      role: 'assistant',
      created: 1,
      metadata: { userVisible: true, agentVisible: true },
      content: [
        {
          type: 'text',
          text: `Created a self-contained weather website at:

\`/Users/wgu/Desktop/weather-website/index.html\``,
        },
      ],
    };

    render(
      <BioRouterMessage
        sessionId="weather-site-session"
        message={message}
        messages={[message]}
        toolCallNotifications={new Map()}
        append={vi.fn()}
        workingDir="/Users/wgu/Desktop"
        onOpenArtifact={onOpenArtifact}
      />
    );

    fireEvent.click(
      await screen.findByRole('button', {
        name: '/Users/wgu/Desktop/weather-website/index.html',
      })
    );

    expect(onOpenArtifact).toHaveBeenCalledWith({
      kind: 'file',
      title: 'index.html',
      path: '/Users/wgu/Desktop/weather-website/index.html',
    });
  });
});
