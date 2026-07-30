import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { SessionMessages } from './SessionViewComponents';
import type { Message } from '../../api';
import { refTag } from '../../utils/resourceRefs';

// Issue #65 — the shared/read-only session view is the one surface that renders
// a user message through `MarkdownContent`, and react-markdown is configured
// without `rehype-raw`, so it DROPS unknown HTML rather than showing it. A
// reference therefore vanished here without a trace: worse than raw markup,
// because the reader has no way to tell a skill was ever attached.

vi.mock('../MarkdownContent', () => ({
  default: ({ content }: { content: string }) => <div data-testid="markdown">{content}</div>,
}));

const userMessage = (text: string): Message => ({
  id: 'message-1',
  role: 'user',
  created: 1,
  content: [{ type: 'text', text }],
  metadata: { userVisible: true, agentVisible: true },
});

const renderMessages = (messages: Message[]) =>
  render(<SessionMessages messages={messages} isLoading={false} error={null} onRetry={vi.fn()} />);

describe('a session in the history view keeps its references visible', () => {
  it('shows a chip for a reference the markdown renderer would have swallowed', () => {
    renderMessages([userMessage(`please run ${refTag('skill', 'my skill')}`)]);

    expect(screen.getByTestId('resource-ref-chip-name')).toHaveTextContent('my skill');
    expect(screen.getByTestId('markdown')).toHaveTextContent('please run');
    expect(screen.getByTestId('markdown').textContent).not.toContain('biorouter-ref');
  });

  it('leaves a message with no reference untouched', () => {
    renderMessages([userMessage('just a message')]);

    expect(screen.getByTestId('markdown')).toHaveTextContent('just a message');
    expect(screen.queryByTestId('resource-ref-chip')).not.toBeInTheDocument();
  });

  // Read-only history: the reference is a record of what was sent, not
  // something to edit here.
  it('offers no remove control', () => {
    renderMessages([userMessage(refTag('skill', 'my skill'))]);

    expect(screen.queryByRole('button', { name: /^remove/i })).not.toBeInTheDocument();
  });
});
