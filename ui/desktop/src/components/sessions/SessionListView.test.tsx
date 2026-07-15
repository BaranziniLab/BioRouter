import { act, render, screen, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import SessionListView from './SessionListView';
import { clearSessionListCache } from '../../utils/sessionListCache';

const mocks = vi.hoisted(() => ({
  listSessions: vi.fn(),
}));

vi.mock('../../api', () => ({
  listSessions: mocks.listSessions,
  deleteSession: vi.fn(),
  exportSession: vi.fn(),
  importSession: vi.fn(),
  updateSessionName: vi.fn(),
}));

vi.mock('../../contexts/DashboardContext', () => ({
  useDashboard: () => ({ spawnWindow: vi.fn() }),
}));

vi.mock('../conversation/SearchView', () => ({
  SearchView: ({ children }: { children: ReactNode }) => <>{children}</>,
}));

vi.mock('../ui/scroll-area', () => ({
  ScrollArea: ({ children }: { children: ReactNode }) => <div>{children}</div>,
}));

beforeEach(() => {
  vi.clearAllMocks();
  clearSessionListCache();
  mocks.listSessions.mockResolvedValue({ data: { sessions: [] } });
});

describe('SessionListView loading and cache', () => {
  it('shows a heatmap-inspired row animation while the first history request is pending', async () => {
    let finishRequest: ((value: { data: { sessions: never[] } }) => void) | undefined;
    mocks.listSessions.mockReturnValue(
      new Promise((resolve) => {
        finishRequest = resolve;
      })
    );

    render(
      <MemoryRouter>
        <SessionListView onSelectSession={vi.fn()} />
      </MemoryRouter>
    );

    expect(screen.getByRole('status', { name: 'Loading chat history' })).toBeInTheDocument();
    expect(screen.getAllByTestId('history-loading-row')).toHaveLength(9);
    expect(
      screen
        .getAllByTestId('history-loading-row')[0]
        .querySelector('.biorouter-history-loading-cell')
    ).toBeInTheDocument();

    await act(async () => {
      finishRequest?.({ data: { sessions: [] } });
    });
    await waitFor(() =>
      expect(screen.queryByRole('status', { name: 'Loading chat history' })).not.toBeInTheDocument()
    );
  });

  it('renders cached history immediately while a return visit revalidates in the background', async () => {
    const session = {
      id: 'session-1',
      name: 'Cached conversation',
      created_at: '2026-07-14T12:00:00Z',
      updated_at: '2026-07-14T12:00:00Z',
      extension_data: {},
      message_count: 3,
      working_dir: '/Users/wgu/Desktop',
    };
    mocks.listSessions.mockResolvedValueOnce({ data: { sessions: [session] } });

    const firstVisit = render(
      <MemoryRouter>
        <SessionListView onSelectSession={vi.fn()} />
      </MemoryRouter>
    );
    await screen.findByText('Cached conversation');
    firstVisit.unmount();

    let finishRefresh: ((value: { data: { sessions: Array<typeof session> } }) => void) | undefined;
    mocks.listSessions.mockReturnValueOnce(
      new Promise((resolve) => {
        finishRefresh = resolve;
      })
    );

    render(
      <MemoryRouter>
        <SessionListView onSelectSession={vi.fn()} />
      </MemoryRouter>
    );

    expect(screen.getByText('Cached conversation')).toBeInTheDocument();
    expect(screen.queryByRole('status', { name: 'Loading chat history' })).not.toBeInTheDocument();
    expect(mocks.listSessions).toHaveBeenCalledTimes(2);

    await act(async () => {
      finishRefresh?.({ data: { sessions: [session] } });
    });
  });

  it('limits the first history render to sixteen sessions', async () => {
    const sessions = Array.from({ length: 17 }, (_, index) => ({
      id: `session-${index}`,
      name: `Conversation ${index + 1}`,
      created_at: new Date(Date.UTC(2026, 6, 14 - index, 12)).toISOString(),
      updated_at: new Date(Date.UTC(2026, 6, 14 - index, 12)).toISOString(),
      extension_data: {},
      message_count: 3,
      working_dir: '/Users/wgu/Desktop',
    }));
    mocks.listSessions.mockResolvedValue({ data: { sessions } });

    render(
      <MemoryRouter>
        <SessionListView onSelectSession={vi.fn()} />
      </MemoryRouter>
    );

    await screen.findByText('Conversation 1');
    expect(screen.getByText('Conversation 16')).toBeInTheDocument();
    expect(screen.queryByText('Conversation 17')).not.toBeInTheDocument();
    expect(screen.getByText('Loading more sessions...')).toBeInTheDocument();
  });
});

describe('SessionListView empty state', () => {
  it('explains where conversations appear and offers useful next steps', async () => {
    render(
      <MemoryRouter>
        <SessionListView onSelectSession={vi.fn()} />
      </MemoryRouter>
    );

    const title = await screen.findByRole('heading', { name: 'No conversations yet' });
    const emptyState = title.closest('section');

    expect(emptyState).toHaveAccessibleDescription(
      'Past conversations will appear here after you start chatting. You can also import an existing session.'
    );
    expect(screen.getByRole('button', { name: 'Start a chat' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Import session' })).toBeInTheDocument();
  });
});

describe('SessionListView row actions', () => {
  it('uses the standard outlined action buttons and destructive delete button', async () => {
    mocks.listSessions.mockResolvedValue({
      data: {
        sessions: [
          {
            id: 'session-1',
            name: 'Example session',
            created_at: '2026-07-14T12:00:00Z',
            updated_at: '2026-07-14T12:00:00Z',
            extension_data: {},
            message_count: 3,
            working_dir: '/Users/wgu/Desktop',
          },
        ],
      },
    });

    render(
      <MemoryRouter>
        <SessionListView onSelectSession={vi.fn()} />
      </MemoryRouter>
    );

    const outlinedActions = await Promise.all([
      screen.findByRole('button', { name: 'Launch options for Example session' }),
      screen.findByRole('button', { name: 'Edit Example session' }),
      screen.findByRole('button', { name: 'Export Example session' }),
    ]);

    for (const action of outlinedActions) {
      expect(action).toHaveClass('h-7', 'w-7', 'border');
      expect(action).not.toHaveAttribute('title');
    }

    const deleteAction = screen.getByRole('button', { name: 'Delete Example session' });
    expect(deleteAction).toHaveAttribute('data-slot', 'tooltip-trigger');
    expect(deleteAction).toHaveClass('h-8', 'w-8', 'text-text-danger');
    expect(deleteAction).not.toHaveAttribute('title');
  });
});
