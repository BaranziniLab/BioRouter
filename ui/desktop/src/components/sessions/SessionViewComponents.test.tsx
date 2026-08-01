import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { SessionMessages } from './SessionViewComponents';
import type { Message } from '../../api';

const userMessage: Message = {
  id: 'message-1',
  role: 'user',
  created: 1,
  content: [{ type: 'text', text: 'run the analysis' }],
  metadata: { userVisible: true, agentVisible: true },
};

const injected: Message = {
  ...userMessage,
  id: 'message-2',
  metadata: {
    userVisible: true,
    agentVisible: true,
    provenance: {
      kind: 'agent_injection',
      fromSessionId: 's-parent',
      fromSessionName: 'Planning chat',
    },
  },
};

describe('SessionMessages provenance (BR-71 §5)', () => {
  it('labels a message another agent injected instead of attributing it to "You"', () => {
    // This is the transcript a share link renders, i.e. the one that leaves the
    // machine — the highest-stakes place to pass another agent's text off as
    // the human's. §5 makes the label structural, not per-view.
    render(
      <SessionMessages messages={[injected]} isLoading={false} error={null} onRetry={vi.fn()} />
    );

    expect(screen.getByText(/injected by Planning chat/i)).toBeTruthy();
  });

  it('shows no chip on an ordinary same-session message', () => {
    render(
      <SessionMessages messages={[userMessage]} isLoading={false} error={null} onRetry={vi.fn()} />
    );

    expect(screen.getByText('You')).toBeTruthy();
    expect(document.querySelector('[data-provenance-kind]')).toBeNull();
  });
});
