import { fireEvent, render, screen } from '@testing-library/react';
import type { ComponentProps } from 'react';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import type { Session } from '../../api';
import { SidebarProvider } from '../ui/sidebar';
import RecentChats, {
  formatSessionDateLabel,
  formatTimeSinceLastWorked,
  getRecentChats,
  groupRecentChatsByDate,
} from './RecentChats';

const now = Date.parse('2026-07-15T12:00:00.000Z');

beforeAll(() => {
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    value: vi.fn().mockImplementation((query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })),
  });
});

function makeSession(index: number): Session {
  const updatedAt = new Date(now - index * 60_000).toISOString();
  return {
    id: `session-${index}`,
    name: `Chat ${index}`,
    created_at: updatedAt,
    updated_at: updatedAt,
    working_dir: `/workspace/project-${index}`,
    message_count: index,
    extension_data: {},
  };
}

function renderRecentChats(props: Partial<ComponentProps<typeof RecentChats>> = {}) {
  return render(
    <SidebarProvider>
      <RecentChats
        sessions={[makeSession(2), makeSession(0), makeSession(1)]}
        runningSessionIds={new Set(['session-1'])}
        onOpen={vi.fn()}
        onViewAll={vi.fn()}
        {...props}
      />
    </SidebarProvider>
  );
}

describe('getRecentChats', () => {
  it('orders chats by most recent activity and caps the list at ten', () => {
    const sessions = Array.from({ length: 12 }, (_, index) => makeSession(index));

    expect(getRecentChats(sessions).map((session) => session.id)).toEqual(
      Array.from({ length: 10 }, (_, index) => `session-${index}`)
    );
  });

  it('groups the sorted result under human-readable activity dates', () => {
    const today = makeSession(0);
    const yesterday = {
      ...makeSession(1),
      updated_at: '2026-07-14T12:00:00.000Z',
    };
    const earlier = {
      ...makeSession(2),
      updated_at: '2026-07-10T12:00:00.000Z',
    };

    expect(groupRecentChatsByDate([earlier, yesterday, today], now)).toEqual([
      { label: 'Today', sessions: [today] },
      { label: 'Yesterday', sessions: [yesterday] },
      { label: 'Jul 10', sessions: [earlier] },
    ]);
    expect(formatSessionDateLabel(today.updated_at, now)).toBe('Today');
  });
});

describe('formatTimeSinceLastWorked', () => {
  it('renders concise elapsed time suitable for the sidebar summary', () => {
    expect(formatTimeSinceLastWorked(new Date(now - 36 * 60_000).toISOString(), now)).toBe(
      '36m ago'
    );
    expect(formatTimeSinceLastWorked(new Date(now - 3 * 24 * 60 * 60_000).toISOString(), now)).toBe(
      '3d ago'
    );
  });
});

describe('RecentChats', () => {
  it('opens individual chats, marks an ongoing chat, and exposes a compact summary on focus', async () => {
    const onOpen = vi.fn();
    renderRecentChats({ onOpen });

    const currentChat = screen.getByTestId('recent-chat-session-0');
    const ongoingChat = screen.getByTestId('recent-chat-session-1');
    expect(ongoingChat).toHaveAccessibleName('Open ongoing chat: Chat 1');
    expect(ongoingChat).toHaveClass('w-full', 'h-8');
    expect(screen.getByTestId('running-chat-indicator-session-1')).toBeInTheDocument();
    expect(ongoingChat).toHaveTextContent('Chat 1');
    expect(ongoingChat).not.toHaveTextContent('1 message');
    expect(currentChat.querySelector('svg')).toBeNull();
    expect(screen.getByText('Recents')).toBeInTheDocument();
    expect(screen.queryByText('3')).not.toBeInTheDocument();
    expect(screen.queryByTestId('recent-actions-divider')).not.toBeInTheDocument();

    fireEvent.click(currentChat);
    expect(onOpen).toHaveBeenCalledWith('session-0');

    fireEvent.focus(currentChat);
    const [summary] = await screen.findAllByTestId('recent-chat-summary-session-0');
    expect(summary).toHaveTextContent('Chat 0');
    expect(summary).toHaveTextContent('/workspace/project-0');
    expect(summary).toHaveTextContent('Last worked');
    expect(summary).toHaveTextContent('0 messages');
    expect(summary).toHaveTextContent('Jul 15');
  });

  it('keeps a dedicated View all chat history action beneath the recent rows', () => {
    const onViewAll = vi.fn();
    renderRecentChats({ onViewAll });

    const viewAllButton = screen.getByTestId('view-all-chat-history');
    expect(viewAllButton).toHaveClass('h-8', 'px-3', 'py-2', 'text-sm');
    expect(viewAllButton).not.toHaveClass('text-text-muted');

    fireEvent.click(viewAllButton);
    expect(onViewAll).toHaveBeenCalledOnce();
  });
});
