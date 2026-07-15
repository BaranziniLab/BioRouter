import { render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Session } from '../../api';
import { SessionInsights } from './SessionsInsights';

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
  UsageHeatmap: () => <div>Usage heatmap</div>,
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

beforeEach(() => {
  vi.clearAllMocks();
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
});
