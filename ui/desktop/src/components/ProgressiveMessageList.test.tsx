import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import ProgressiveMessageList from './ProgressiveMessageList';
import { ChatState } from '../types/chatState';
import type { Message } from '../api';

const assistantWithToolCall: Message = {
  id: 'assistant-1',
  role: 'assistant',
  created: 1_700_000_000,
  metadata: { userVisible: true, agentVisible: true },
  content: [
    {
      type: 'toolRequest',
      id: 'tool-1',
      toolCall: {
        status: 'success',
        value: { name: 'developer__shell', arguments: { command: 'ls' } },
      },
    },
  ],
} as Message;

const toolResponseOnlyUserMessage: Message = {
  id: 'user-1',
  role: 'user',
  created: 1_700_000_001,
  metadata: { userVisible: true, agentVisible: true },
  content: [
    {
      type: 'toolResponse',
      id: 'tool-1',
      toolResult: { status: 'success', value: { content: [] } },
    },
  ],
} as Message;

const messages = [assistantWithToolCall, toolResponseOnlyUserMessage];

const liveProps = {
  messages,
  chat: { sessionId: 'live-session' },
  toolCallNotifications: new Map(),
  append: vi.fn(),
  isUserMessage: (m: Message) => m.role !== 'assistant',
};

describe('ProgressiveMessageList trailing activity indicator', () => {
  it('never renders the trailing activity indicator on a replayed historical session', () => {
    // Props copied from SessionHistoryView: no isStreamingMessage, no chatState,
    // no timestamps. This is the load-bearing guarantee — a saved transcript
    // must never sprout a running clock.
    render(
      <ProgressiveMessageList
        messages={messages}
        chat={{ sessionId: 'session-preview' }}
        toolCallNotifications={new Map()}
        append={() => {}}
        isUserMessage={(m: Message) => m.role !== 'assistant'}
        batchSize={15}
        batchDelay={30}
        showLoadingThreshold={30}
      />
    );

    expect(screen.queryByTestId('turn-activity-indicator')).toBeNull();
  });

  it('renders the indicator under the tool block while a live turn awaits the model', () => {
    render(
      <ProgressiveMessageList
        {...liveProps}
        isStreamingMessage
        chatState={ChatState.Streaming}
        lastMessageAt={Date.now()}
      />
    );

    expect(screen.getByTestId('turn-activity-indicator')).toBeInTheDocument();
    expect(screen.getAllByText('Working on the result').length).toBeGreaterThan(0);
  });

  it('removes the indicator the moment the turn goes idle', () => {
    const { rerender } = render(
      <ProgressiveMessageList
        {...liveProps}
        isStreamingMessage
        chatState={ChatState.Streaming}
        lastMessageAt={Date.now()}
      />
    );
    expect(screen.getByTestId('turn-activity-indicator')).toBeInTheDocument();

    rerender(
      <ProgressiveMessageList
        {...liveProps}
        isStreamingMessage={false}
        chatState={ChatState.Idle}
      />
    );

    expect(screen.queryByTestId('turn-activity-indicator')).toBeNull();
  });

  it('places the indicator AFTER the last rendered message, not above the list', () => {
    // "Trailing underneath the tool call section" is the whole requirement; a
    // refactor that moved it above the list would otherwise pass silently.
    const { container } = render(
      <ProgressiveMessageList
        {...liveProps}
        isStreamingMessage
        chatState={ChatState.Streaming}
        lastMessageAt={Date.now()}
      />
    );

    const children: Element[] = Array.from(
      container.firstElementChild?.parentElement?.children ?? []
    );
    const lastMessageIndex = children.reduce(
      (last, el, i) => (el.getAttribute('data-testid') === 'message-container' ? i : last),
      -1
    );
    const indicatorIndex = children.findIndex(
      (el: Element) => el.getAttribute('data-testid') === 'trailing-activity-container'
    );

    expect(lastMessageIndex).toBeGreaterThanOrEqual(0);
    expect(indicatorIndex).toBeGreaterThan(lastMessageIndex);
  });

  it('shows no indicator while the assistant is streaming visible prose', () => {
    const prose: Message = {
      id: 'assistant-2',
      role: 'assistant',
      created: 1_700_000_002,
      metadata: { userVisible: true, agentVisible: true },
      content: [{ type: 'text', text: 'Here is what I found' }],
    } as Message;

    render(
      <ProgressiveMessageList
        {...liveProps}
        messages={[assistantWithToolCall, toolResponseOnlyUserMessage, prose]}
        isStreamingMessage
        chatState={ChatState.Streaming}
        lastMessageAt={Date.now()}
      />
    );

    expect(screen.queryByTestId('turn-activity-indicator')).toBeNull();
  });
});
