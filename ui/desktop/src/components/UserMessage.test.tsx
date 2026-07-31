import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import UserMessage from './UserMessage';
import type { Message } from '../api';

const message: Message = {
  id: 'message-1',
  role: 'user',
  created: 1,
  content: [{ type: 'text', text: 'original prompt' }],
  metadata: { userVisible: true, agentVisible: true },
};

beforeEach(() => {
  Object.assign(window, {
    electron: {
      logInfo: vi.fn(),
    },
  });
});

describe('UserMessage edit actions', () => {
  it('uses Diverge Session copy and emits the diverge action', () => {
    const onMessageUpdate = vi.fn();
    render(<UserMessage message={message} onMessageUpdate={onMessageUpdate} />);

    fireEvent.click(screen.getByRole('button', { name: /edit message:/i }));

    expect(screen.getAllByText('Diverge Session')).toHaveLength(2);
    expect(screen.queryByText('Fork Session')).not.toBeInTheDocument();

    fireEvent.change(screen.getByRole('textbox', { name: 'Edit message content' }), {
      target: { value: 'updated prompt' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Diverge session with edited message' }));

    expect(onMessageUpdate).toHaveBeenCalledWith('message-1', 'updated prompt', 'diverge');
  });

  it('labels a message another agent injected (BR-71 §5)', () => {
    render(
      <UserMessage
        message={{
          ...message,
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
        }}
        onMessageUpdate={vi.fn()}
      />
    );
    expect(screen.getByText(/injected by Planning chat/i)).toBeTruthy();
  });

  it('shows no chip on an ordinary same-session message', () => {
    render(<UserMessage message={message} onMessageUpdate={vi.fn()} />);
    expect(document.querySelector('[data-provenance-kind]')).toBeNull();
  });
});
