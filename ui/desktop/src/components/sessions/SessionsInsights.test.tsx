import { act, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Session } from '../../api';
import { SessionInsights } from './SessionsInsights';
import { clearSessionListCache, updateCachedSessionList } from '../../utils/sessionListCache';
import {
  cacheHomeActivity,
  cacheHomeRecentSessions,
  clearHomeInsightsCache,
} from '../../utils/homeInsightsCache';

const mocks = vi.hoisted(() => ({
  getSessionActivity: vi.fn(),
  listSessions: vi.fn(),
}));

vi.mock('../../api', () => ({
  getSessionActivity: mocks.getSessionActivity,
  listSessions: mocks.listSessions,
}));

vi.mock('../../hooks/useNavigation', () => ({
  useNavigation: () => vi.fn(),
}));

vi.mock('../../sessions', () => ({
  resumeSession: vi.fn(),
}));

vi.mock('../common/Greeting', () => ({
  Greeting: () => <h1>Greeting</h1>,
}));

vi.mock('./UsageHeatmap', () => ({
  UsageHeatmap: ({ window }: { window: { currentStreak: number } }) => (
    <div>Usage heatmap {window.currentStreak}</div>
  ),
  UsageHeatmapLoading: () => <div>Loading usage</div>,
}));

function session(index: number): Session {
  return {
    id: `session-${index}`,
    name: `Recent chat ${index}`,
    created_at: '2026-07-14T12:00:00Z',
    updated_at: '2026-07-14T12:00:00Z',
    extension_data: {},
    message_count: index,
    working_dir: '/Users/wgu/Desktop',
  };
}

function subagent(index: number): Session {
  return { ...session(index), session_type: 'sub_agent', parent_session_id: 'session-1' };
}

beforeEach(() => {
  vi.clearAllMocks();
  clearSessionListCache();
  clearHomeInsightsCache();
  mocks.getSessionActivity.mockReturnValue(new Promise(() => {}));
});

describe('SessionInsights', () => {
  it('shows no more than three recent chats on the home page', async () => {
    mocks.listSessions.mockResolvedValue({
      data: { sessions: [session(1), session(2), session(3), session(4), session(5)] },
    });

    render(
      <MemoryRouter>
        <SessionInsights />
      </MemoryRouter>
    );

    await waitFor(() => expect(screen.getByText('Recent chat 3')).toBeInTheDocument());
    expect(screen.getByText('Recent chat 1')).toBeInTheDocument();
    expect(screen.getByText('Recent chat 2')).toBeInTheDocument();
    expect(screen.queryByText('Recent chat 4')).not.toBeInTheDocument();
    expect(screen.queryByText('Recent chat 5')).not.toBeInTheDocument();
  });

  it('renders persisted activity and recent chats immediately while refreshing', () => {
    cacheHomeActivity({
      start: '2026-02-11',
      end: '2026-07-15',
      maxSessions: 4,
      maxTokens: 1000,
      tokensComplete: true,
      currentStreak: 7,
      longestStreak: 12,
      days: [],
    });
    cacheHomeRecentSessions([session(1), session(2)]);
    mocks.listSessions.mockReturnValue(new Promise(() => {}));

    render(
      <MemoryRouter>
        <SessionInsights />
      </MemoryRouter>
    );

    expect(screen.getByText('Usage heatmap 7')).toBeInTheDocument();
    expect(screen.queryByText('Loading usage')).not.toBeInTheDocument();
    expect(screen.getByText('Recent chat 1')).toBeInTheDocument();
    expect(screen.queryAllByRole('progressbar')).toHaveLength(0);
    expect(mocks.getSessionActivity).toHaveBeenCalledTimes(1);
    expect(mocks.listSessions).toHaveBeenCalledTimes(1);
  });

  // BR-71: History and Home read ONE shared session cache. When History's
  // "Show subagent runs" toggle is on, that cache holds sub_agent rows — and
  // Home's recents are conversations the *user* started, so every one of the
  // three places Home reads the cache has to drop them. Filtering only the
  // fetch path leaves the other two leaking.
  it('drops subagent runs from the recents the loader fetches', async () => {
    mocks.listSessions.mockResolvedValue({
      data: { sessions: [session(1), subagent(2), session(3), session(4)] },
    });

    render(
      <MemoryRouter>
        <SessionInsights />
      </MemoryRouter>
    );

    await waitFor(() => expect(screen.getByText('Recent chat 1')).toBeInTheDocument());
    expect(screen.queryByText('Recent chat 2')).not.toBeInTheDocument();
    // Filtering must precede the slice, or dropping a subagent silently shortens
    // Home's recents instead of promoting the next real conversation.
    expect(screen.getByText('Recent chat 4')).toBeInTheDocument();
  });

  it('does not paint subagent runs from a warm shared cache on mount', () => {
    // History left the cache holding subagent rows; Home then mounts.
    updateCachedSessionList([session(1), subagent(2), session(3), session(4)]);
    mocks.listSessions.mockReturnValue(new Promise(() => {}));

    render(
      <MemoryRouter>
        <SessionInsights />
      </MemoryRouter>
    );

    expect(screen.getByText('Recent chat 1')).toBeInTheDocument();
    expect(screen.queryByText('Recent chat 2')).not.toBeInTheDocument();
    expect(screen.getByText('Recent chat 4')).toBeInTheDocument();
  });

  it('does not adopt subagent runs when the shared cache changes under it', async () => {
    mocks.listSessions.mockResolvedValue({ data: { sessions: [session(1)] } });

    render(
      <MemoryRouter>
        <SessionInsights />
      </MemoryRouter>
    );

    await waitFor(() => expect(screen.getByText('Recent chat 1')).toBeInTheDocument());

    // Any create / diverge / delete / rename — or History's own refetch with the
    // toggle on — republishes the shared list through this subscription.
    act(() => {
      updateCachedSessionList([session(1), subagent(2)]);
    });

    expect(screen.getByText('Recent chat 1')).toBeInTheDocument();
    expect(screen.queryByText('Recent chat 2')).not.toBeInTheDocument();
  });
});
