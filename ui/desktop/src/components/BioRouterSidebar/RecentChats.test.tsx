import { fireEvent, render, screen } from '@testing-library/react';
import type { ComponentProps } from 'react';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import type { SessionSummary } from '../../api';
import { SidebarProvider } from '../ui/sidebar';
import RecentChats, {
  formatSessionDateLabel,
  formatTimeSinceLastWorked,
  groupRecentChatsByDate,
  sortRecentChats,
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

function makeSession(index: number, referenceTime = now): SessionSummary {
  const updatedAt = new Date(referenceTime - index * 60_000).toISOString();
  return {
    id: `session-${index}`,
    name: `Chat ${index}`,
    created_at: updatedAt,
    updated_at: updatedAt,
    working_dir: `/workspace/project-${index}`,
    message_count: index,
  };
}

function renderRecentChats(props: Partial<ComponentProps<typeof RecentChats>> = {}) {
  const currentTime = Date.now();
  return render(
    <SidebarProvider>
      <RecentChats
        sessions={[
          makeSession(2, currentTime),
          makeSession(0, currentTime),
          makeSession(1, currentTime),
        ]}
        runningSessionIds={new Set(['session-1'])}
        hasMore={false}
        isLoadingMore={false}
        onLoadMore={vi.fn()}
        onOpen={vi.fn()}
        onViewAll={vi.fn()}
        {...props}
      />
    </SidebarProvider>
  );
}

describe('sortRecentChats', () => {
  it('orders every loaded chat by most recent activity without a fixed cap', () => {
    const sessions = Array.from({ length: 12 }, (_, index) => makeSession(index));

    expect(sortRecentChats(sessions).map((session) => session.id)).toEqual(
      Array.from({ length: 12 }, (_, index) => `session-${index}`)
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
    const currentDateLabel = new Intl.DateTimeFormat('en-US', {
      month: 'short',
      day: 'numeric',
    }).format(Date.now());
    renderRecentChats({ onOpen, activeSessionId: 'session-0' });

    const currentChat = screen.getByTestId('recent-chat-session-0');
    const ongoingChat = screen.getByTestId('recent-chat-session-1');
    expect(ongoingChat).toHaveAccessibleName('Open ongoing chat: Chat 1');
    expect(ongoingChat).toHaveClass('w-full', 'h-8', 'px-3', 'text-sm');
    expect(ongoingChat).not.toHaveClass('font-medium');
    expect(currentChat).toHaveClass('font-medium');
    expect(currentChat).toHaveAttribute('aria-current', 'page');
    expect(screen.getByTestId('running-chat-indicator-session-1')).toBeInTheDocument();
    expect(ongoingChat).toHaveTextContent('Chat 1');
    expect(ongoingChat).not.toHaveTextContent('1 message');
    expect(currentChat.querySelector('svg')).toBeNull();
    const recentsLabel = screen.getByText('Recents');
    expect(recentsLabel.parentElement).toHaveClass('h-8', 'px-5');
    expect(screen.getByText('Today')).toHaveClass('text-xs', 'font-normal');
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
    expect(summary).toHaveTextContent(currentDateLabel);
  });

  it('truncates an overlong row title while preserving the full hover summary', async () => {
    const longTitle = 'app:ucsf-versa-gpt55-kb-visualizer-smoke-with-an-even-longer-suffix';
    const longSession = { ...makeSession(0), name: longTitle };
    renderRecentChats({ sessions: [longSession] });

    const row = screen.getByTestId('recent-chat-session-0');
    const title = screen.getByText(longTitle);
    expect(row).toHaveClass('w-full', 'min-w-0', 'max-w-full', 'overflow-hidden');
    expect(title).toHaveClass('min-w-0', 'flex-1', 'truncate');

    fireEvent.focus(row);
    const [summary] = await screen.findAllByTestId('recent-chat-summary-session-0');
    expect(summary).toHaveTextContent(longTitle);
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

  it('keeps View all chat history attached to Recents when the list is empty', () => {
    renderRecentChats({ sessions: [] });

    const scrollContainer = screen.getByTestId('recent-chat-scroll');
    expect(scrollContainer).toHaveClass('shrink', 'overflow-y-auto');
    expect(scrollContainer).not.toHaveClass('flex-1');
    expect(screen.getByText('No recent chats yet')).toBeInTheDocument();
    expect(screen.getByTestId('view-all-chat-history')).toBeInTheDocument();
  });

  it('requests another page when the user scrolls near the end of the loaded chats', () => {
    const onLoadMore = vi.fn();
    renderRecentChats({ hasMore: true, onLoadMore });

    const scrollContainer = screen.getByTestId('recent-chat-scroll');
    Object.defineProperties(scrollContainer, {
      clientHeight: { configurable: true, value: 100 },
      scrollHeight: { configurable: true, value: 500 },
      scrollTop: { configurable: true, value: 350 },
    });

    fireEvent.scroll(scrollContainer);
    expect(onLoadMore).toHaveBeenCalledOnce();
  });
});
