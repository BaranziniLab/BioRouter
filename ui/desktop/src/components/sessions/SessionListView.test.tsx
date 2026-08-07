import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ReactNode } from 'react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import SessionListView from './SessionListView';
import { clearSessionListCache, updateCachedSessionList } from '../../utils/sessionListCache';
import type { Session } from '../../api';

const mocks = vi.hoisted(() => ({
  listSessions: vi.fn(),
  deleteSession: vi.fn(),
  updateSessionName: vi.fn(),
  declassifySession: vi.fn(),
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
}));

vi.mock('../../api', () => ({
  listSessions: mocks.listSessions,
  deleteSession: mocks.deleteSession,
  exportSession: vi.fn(),
  importSession: vi.fn(),
  updateSessionName: mocks.updateSessionName,
  declassifySession: mocks.declassifySession,
}));

vi.mock('../../toasts', () => ({
  toastSuccess: mocks.toastSuccess,
  toastError: mocks.toastError,
}));

vi.mock('../conversation/SearchView', () => ({
  SearchView: ({ children }: { children: ReactNode }) => <>{children}</>,
}));

vi.mock('../ui/scroll-area', () => ({
  ScrollArea: ({ children }: { children: ReactNode }) => <div>{children}</div>,
}));

vi.mock('../ui/ConfirmationModal', () => ({
  ConfirmationModal: ({ isOpen, onConfirm }: { isOpen: boolean; onConfirm: () => void }) =>
    isOpen ? <button onClick={onConfirm}>Confirm deletion</button> : null,
}));

beforeEach(() => {
  vi.clearAllMocks();
  clearSessionListCache();
  mocks.listSessions.mockResolvedValue({ data: { sessions: [] } });
});

function row(overrides: Partial<Session> & { id: string; name: string }): Session {
  return {
    working_dir: '/tmp',
    created_at: '2026-07-14T12:00:00Z',
    updated_at: '2026-07-14T12:00:00Z',
    extension_data: {},
    message_count: 2,
    ...overrides,
  } as Session;
}

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
  it('shows three 32px row actions and keeps the destructive one in the overflow', async () => {
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

    // §3.10 — every visible row action is one 32px icon button. This assertion
    // is what ended the 28-vs-32px fork between the outlined trio and delete.
    const visibleActions = await Promise.all([
      screen.findByRole('button', { name: 'Edit Example session' }),
      screen.findByRole('button', { name: 'Export Example session' }),
      screen.findByRole('button', { name: 'More actions for Example session' }),
    ]);

    for (const action of visibleActions) {
      expect(action).toHaveClass('h-8', 'w-8', 'border');
      expect(action).not.toHaveAttribute('title');
    }

    // Destructive actions live only in the overflow, so Delete must NOT be one
    // of the buttons a stray click can reach.
    expect(screen.queryByRole('button', { name: 'Delete Example session' })).toBeNull();
  });

  it('offers Open in new tab above Open in new window, on the row-click path', async () => {
    const user = userEvent.setup();
    const onSelectSession = vi.fn();
    // A name of its own: this file leaks `listSessions` promises through a
    // module-global cache no `cleanup()` resets, so a query that could also
    // match the PREVIOUS test's row resolves against a detached tree.
    mocks.listSessions.mockResolvedValue({
      data: { sessions: [row({ id: 'session-1', name: 'A launchable session' })] },
    });

    render(
      <MemoryRouter>
        <SessionListView onSelectSession={onSelectSession} />
      </MemoryRouter>
    );

    // Let the list settle before touching it. A leaked promise resolving between
    // the query and the click detaches the button, and the click then lands on
    // nothing — which reads as "the menu never opened".
    await screen.findByRole('button', { name: 'More actions for A launchable session' });
    await act(async () => {
      await Promise.resolve();
    });

    await user.click(screen.getByRole('button', { name: 'More actions for A launchable session' }));

    const items = (await screen.findAllByRole('menuitem')).map((el) => el.textContent);
    expect(items.indexOf('Open in new tab')).toBeGreaterThanOrEqual(0);
    expect(items.indexOf('Open in new tab')).toBeLessThan(items.indexOf('Open in new window'));

    await user.click(screen.getByRole('menuitem', { name: 'Open in new tab' }));
    expect(onSelectSession).toHaveBeenCalledWith('session-1');
  });

  it('uses the shared notification surface after deleting a session', async () => {
    const user = userEvent.setup();
    const session = {
      id: 'session-1',
      name: 'A session name long enough to exercise notification wrapping',
      created_at: '2026-07-14T12:00:00Z',
      updated_at: '2026-07-14T12:00:00Z',
      extension_data: {},
      message_count: 3,
      working_dir: '/Users/wgu/Desktop',
    };
    mocks.listSessions.mockResolvedValue({ data: { sessions: [session] } });

    render(
      <MemoryRouter>
        <SessionListView onSelectSession={vi.fn()} />
      </MemoryRouter>
    );

    await user.click(
      await screen.findByRole('button', { name: `More actions for ${session.name}` })
    );
    await user.click(await screen.findByRole('menuitem', { name: `Delete ${session.name}` }));
    fireEvent.click(await screen.findByRole('button', { name: 'Confirm deletion' }));

    await waitFor(() =>
      expect(mocks.toastSuccess).toHaveBeenCalledWith({
        title: 'Session deleted',
        msg: `"${session.name}" was removed from chat history.`,
      })
    );
    expect(mocks.deleteSession).toHaveBeenCalledWith({
      path: { session_id: session.id },
      throwOnError: true,
    });
  });

  it('the Show-subagent-runs toggle refetches with include_subagents and nests children', async () => {
    mocks.listSessions.mockResolvedValue({
      data: {
        sessions: [
          {
            id: 'p1',
            session_type: 'user',
            name: 'Parent',
            working_dir: '/tmp',
            created_at: '2026-07-14T12:00:00Z',
            updated_at: '2026-07-14T12:00:00Z',
            extension_data: {},
            message_count: 3,
          },
          {
            id: 'c1',
            session_type: 'sub_agent',
            parent_session_id: 'p1',
            name: 'Subagent task',
            working_dir: '/tmp',
            created_at: '2026-07-14T12:00:00Z',
            updated_at: '2026-07-14T12:00:00Z',
            extension_data: {},
            message_count: 2,
          },
        ],
      },
    });
    // The component calls `useNavigate()`, so it MUST be inside a router, and
    // `onSelectSession` is a required prop. This is the exact shape every other
    // case in this file uses.
    render(
      <MemoryRouter>
        <SessionListView onSelectSession={vi.fn()} />
      </MemoryRouter>
    );
    const toggle = await screen.findByLabelText(/show subagent runs/i);
    fireEvent.click(toggle);
    await waitFor(() =>
      expect(mocks.listSessions).toHaveBeenLastCalledWith(
        expect.objectContaining({ query: { include_subagents: true } })
      )
    );
    // Nested, not merely present: the child sits inside the indented wrapper
    // and carries the badge, so the row reads as belonging to 'Parent'.
    // Re-query on every attempt — `SessionItem` is declared inside
    // `SessionListView`'s body, so each re-render is a fresh component type and
    // React replaces the row's DOM node. A node captured once goes stale (it is
    // detached, and `closest` then walks nothing), which is a flake, not a bug
    // in the nesting.
    await waitFor(() => {
      const childRow = screen.getByText('Subagent task').closest('.ml-6');
      expect(childRow).not.toBeNull();
      // The badge belongs to the row, not above it: a bare inline span placed
      // before a block-level row inside a flex column renders on its own line.
      expect(
        screen
          .getByText('Subagent task')
          .closest('.biorouter-list-row')
          ?.querySelector('[data-testid="subagent-badge"]')
      ).not.toBeNull();
    });
  });

  // BR-71: `groupSessionsByDate` buckets on `updated_at`, and a parent's
  // `updated_at` advances every time the conversation is resumed. Grouping by
  // parent INSIDE each date bucket therefore drops any subagent that ran on an
  // earlier day back to top level — the confusing artifact the feature exists
  // to remove. Parent grouping has to run first.
  it('nests a subagent run under its parent across date buckets', async () => {
    mocks.listSessions.mockResolvedValue({
      data: {
        sessions: [
          row({
            id: 'p1',
            name: 'Parent',
            session_type: 'user',
            updated_at: '2026-07-14T12:00:00Z',
          }),
          row({
            id: 'c1',
            name: 'Subagent task',
            session_type: 'sub_agent',
            parent_session_id: 'p1',
            updated_at: '2026-07-10T12:00:00Z',
          }),
        ],
      },
    });

    render(
      <MemoryRouter>
        <SessionListView onSelectSession={vi.fn()} />
      </MemoryRouter>
    );
    fireEvent.click(await screen.findByLabelText(/show subagent runs/i));
    await waitFor(() =>
      expect(mocks.listSessions).toHaveBeenLastCalledWith(
        expect.objectContaining({ query: { include_subagents: true } })
      )
    );

    await waitFor(() => expect(screen.getByText('Subagent task').closest('.ml-6')).not.toBeNull());
    // The child rides in its parent's bucket, so its own date never opens one.
    expect(screen.queryByText(/July 10/)).not.toBeInTheDocument();
  });

  it('badges a subagent run whose parent is not in the list', async () => {
    mocks.listSessions.mockResolvedValue({
      data: {
        sessions: [
          row({
            id: 'c9',
            name: 'Orphan run',
            session_type: 'sub_agent',
            parent_session_id: 'deleted-parent',
          }),
        ],
      },
    });

    render(
      <MemoryRouter>
        <SessionListView onSelectSession={vi.fn()} />
      </MemoryRouter>
    );
    fireEvent.click(await screen.findByLabelText(/show subagent runs/i));

    // An orphan stays top-level so it is still reachable — but unbadged it is
    // an unexplained bare row, which is the same confusion in another form.
    await waitFor(() =>
      expect(
        screen
          .getByText('Orphan run')
          .closest('.biorouter-list-row')
          ?.querySelector('[data-testid="subagent-badge"]')
      ).not.toBeNull()
    );
  });

  // BR-71: `showSubagents` is per-component but the session cache it reads is
  // module-global, so a second History pane (or Home) can publish subagent rows
  // into a pane whose own toggle is off. The toggle governs what is FETCHED;
  // each pane still has to say what it will SHOW.
  it('never paints subagent runs from a warm shared cache while the toggle is off', () => {
    mocks.listSessions.mockReturnValue(new Promise(() => {}));
    updateCachedSessionList([
      row({ id: 'p1', name: 'Parent', session_type: 'user' }),
      row({ id: 'c1', name: 'Subagent task', session_type: 'sub_agent', parent_session_id: 'p1' }),
    ]);

    render(
      <MemoryRouter>
        <SessionListView onSelectSession={vi.fn()} />
      </MemoryRouter>
    );

    expect(screen.getByText('Parent')).toBeInTheDocument();
    expect(screen.queryByText('Subagent task')).not.toBeInTheDocument();
  });

  it('does not adopt subagent runs another pane pushed into the shared cache', async () => {
    mocks.listSessions.mockResolvedValue({
      data: { sessions: [row({ id: 'p1', name: 'Parent', session_type: 'user' })] },
    });

    render(
      <MemoryRouter>
        <SessionListView onSelectSession={vi.fn()} />
      </MemoryRouter>
    );
    await screen.findByText('Parent');

    // A sibling pane with its own toggle ON refetched and republished the list.
    act(() => {
      updateCachedSessionList([
        row({ id: 'p1', name: 'Parent', session_type: 'user' }),
        row({
          id: 'c1',
          name: 'Subagent task',
          session_type: 'sub_agent',
          parent_session_id: 'p1',
        }),
        row({ id: 'p2', name: 'Another chat', session_type: 'user' }),
      ]);
    });

    // Waiting on the sibling row proves the push landed, so the negative
    // assertion below cannot pass merely because the update had not flushed.
    await screen.findByText('Another chat');
    expect(screen.queryByText('Subagent task')).not.toBeInTheDocument();
  });

  it('uses the shared notification surface after editing a session', async () => {
    const session = {
      id: 'session-1',
      name: 'Original session name',
      created_at: '2026-07-14T12:00:00Z',
      updated_at: '2026-07-14T12:00:00Z',
      extension_data: {},
      message_count: 3,
      working_dir: '/Users/wgu/Desktop',
    };
    mocks.listSessions.mockResolvedValue({ data: { sessions: [session] } });

    render(
      <MemoryRouter>
        <SessionListView onSelectSession={vi.fn()} />
      </MemoryRouter>
    );

    fireEvent.click(await screen.findByRole('button', { name: `Edit ${session.name}` }));
    fireEvent.change(await screen.findByPlaceholderText('Enter session description'), {
      target: { value: 'Updated session name' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() =>
      expect(mocks.toastSuccess).toHaveBeenCalledWith({
        title: 'Session updated',
        msg: 'The session description was saved successfully.',
      })
    );
    expect(mocks.updateSessionName).toHaveBeenCalledWith({
      path: { session_id: session.id },
      body: { name: 'Updated session name' },
      throwOnError: true,
    });
  });
});

// This block deliberately never names the badge component: Task 27's gate greps
// src/components for that name and expects an exact file list.
describe('SessionListView privacy markers', () => {
  it('marks a private conversation in the list', async () => {
    mocks.listSessions.mockResolvedValue({
      data: {
        sessions: [row({ id: 'session-1', name: 'Patient cohort', privacy_tier: 'private' })],
      },
    });

    render(
      <MemoryRouter>
        <SessionListView onSelectSession={vi.fn()} />
      </MemoryRouter>
    );

    await screen.findByText('Patient cohort');
    expect(screen.getByTestId('privacy-badge')).toHaveAttribute('data-privacy', 'private');
  });

  it('leaves public conversations unmarked, so the marker keeps meaning something', async () => {
    mocks.listSessions.mockResolvedValue({
      data: {
        sessions: [
          row({ id: 'session-1', name: 'Patient cohort', privacy_tier: 'private' }),
          row({ id: 'session-2', name: 'Public notes', privacy_tier: 'public' }),
          row({ id: 'session-3', name: 'Untiered chat' }),
        ],
      },
    });

    render(
      <MemoryRouter>
        <SessionListView onSelectSession={vi.fn()} />
      </MemoryRouter>
    );

    await screen.findByText('Patient cohort');
    expect(screen.getAllByTestId('privacy-badge')).toHaveLength(1);
  });
});
