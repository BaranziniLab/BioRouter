import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import SessionListView from './SessionListView';
import { clearSessionListCache } from '../../utils/sessionListCache';
import type { Session } from '../../api';

/**
 * Issue #56 §12.1 — declassification is reached from the History ROW.
 *
 * ⚠ **Its own file, deliberately.** These cases belong to `SessionListView` and
 * would naturally live in `SessionListView.test.tsx`, but two cases in that
 * file's `row actions` block are a PRE-EXISTING failure: they time out after 5
 * seconds each (verified against the unmodified file at HEAD), and they leave
 * `listSessions` promises resolving into whatever test runs next — through the
 * module-global session-list cache, which no `cleanup()` can reset. Any Radix
 * menu interaction that follows them is then racing a component that replaces
 * its own DOM nodes, and a trigger queried a tick early is detached, so
 * `pointerDown` is silently a no-op.
 *
 * Vitest isolates test FILES, so this is the one boundary available that does
 * not require either fixing that flake (a refactor of `sessionListCache` and of
 * `SessionItem`'s placement, neither of which belongs to this task) or softening
 * an assertion. The assertions here are exactly what they would be in the other
 * file.
 *
 * The paired absence assertion — that this action is NOT in the chat title menu,
 * which is the obvious slot and the one §12.1 forbids — lives in
 * `SessionNamePill.test.tsx`.
 */

const mocks = vi.hoisted(() => ({
  listSessions: vi.fn(),
  declassifySession: vi.fn(),
}));

vi.mock('../../api', () => ({
  listSessions: mocks.listSessions,
  deleteSession: vi.fn(),
  exportSession: vi.fn(),
  importSession: vi.fn(),
  updateSessionName: vi.fn(),
  declassifySession: mocks.declassifySession,
}));

vi.mock('../../toasts', () => ({
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
}));

vi.mock('../conversation/SearchView', () => ({
  SearchView: ({ children }: { children: ReactNode }) => <>{children}</>,
}));

vi.mock('../ui/scroll-area', () => ({
  ScrollArea: ({ children }: { children: ReactNode }) => <div>{children}</div>,
}));

vi.mock('../../utils/userAction', () => ({
  userActionHeaders: async () => ({ 'X-User-Action': 'test-key' }),
}));

beforeEach(() => {
  vi.clearAllMocks();
  clearSessionListCache();
  mocks.listSessions.mockResolvedValue({ data: { sessions: [] } });
  mocks.declassifySession.mockResolvedValue({});
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

function renderList() {
  return render(
    <MemoryRouter>
      <SessionListView onSelectSession={vi.fn()} />
    </MemoryRouter>
  );
}

/**
 * Open a row's overflow menu, and return once its one item is on screen.
 *
 * `fireEvent.pointerDown`, not `userEvent.click`: Radix's menu opens on
 * pointerdown, and `userEvent` yields to the event loop between pointerdown and
 * pointerup — long enough for a re-render to replace the row and detach the node
 * mid-click. Every passing interaction in the sibling file uses `fireEvent` for
 * the same reason.
 *
 * Retrying the open is the other half, and it is a harness concession rather
 * than a softened assertion — the expectation is exactly the one a single click
 * would make. `SessionItem` is declared inside `SessionListView`'s BODY, so
 * every re-render is a fresh component type and React replaces the row's whole
 * DOM node; a trigger queried in the window before the list settles is already
 * detached, and `pointerDown` on a detached node is silently a no-op. The
 * sibling file documents the same hazard on its subagent-nesting cases.
 *
 * The early return keeps it idempotent: a second pointerdown on an OPEN Radix
 * menu closes it, so a naive retry would flip it shut on the attempt after the
 * one that worked.
 */
async function openRowMenu(trigger: RegExp | string) {
  await waitFor(() => {
    if (screen.queryByText(/Make this chat public/)) return;
    fireEvent.pointerDown(screen.getByLabelText(trigger), { button: 0, ctrlKey: false });
    expect(screen.getByText(/Make this chat public/)).toBeInTheDocument();
  });
}

describe('SessionListView declassification entry point', () => {
  it('offers "Make this chat public" from the row overflow menu of a private chat', async () => {
    mocks.listSessions.mockResolvedValue({
      data: {
        sessions: [
          row({
            id: 'session-1',
            name: 'Patient cohort',
            privacy_tier: 'private',
            privacy_reason: 'mcp:ucsfomopagent',
          }),
        ],
      },
    });

    renderList();

    await screen.findByText('Patient cohort');
    await openRowMenu(/More actions for/);
    expect(screen.getByText(/Make this chat public/)).toBeInTheDocument();
  });

  it('opens the shared dialog on the right row, showing that row and no other', async () => {
    mocks.listSessions.mockResolvedValue({
      data: {
        sessions: [
          row({
            id: '20260714_120000',
            name: 'Patient cohort',
            privacy_tier: 'private',
            privacy_reason: 'mcp:ucsfomopagent',
          }),
          row({
            id: '20260714_130000',
            name: 'Another private chat',
            privacy_tier: 'private',
            privacy_reason: 'mcp:cdwagent',
          }),
        ],
      },
    });

    renderList();

    await screen.findByText('Patient cohort');
    await openRowMenu('More actions for Another private chat');
    fireEvent.click(screen.getByText(/Make this chat public/));

    // The dialog previews the row it will act on — the ResetPanel precedent —
    // and the phrase is that row's, not the first row's. Opening the menu on the
    // wrong row is the mistake this preview exists to catch, so a dialog that
    // showed the first row would defeat the whole confirmation.
    const dialog = await screen.findByRole('dialog');
    expect(dialog).toHaveTextContent('Another private chat');
    expect(dialog).toHaveTextContent('130000');
    expect(dialog).not.toHaveTextContent('Patient cohort');
  });

  it('drops the private marker from the row once the chat is public', async () => {
    mocks.listSessions.mockResolvedValue({
      data: {
        sessions: [
          row({
            id: '20260714_120000',
            name: 'Patient cohort',
            privacy_tier: 'private',
            // `mcp:*`, so §12.4 grades this onto the typed confirmation and the
            // request goes out on confirm. The `turn:*` path is the same wiring
            // behind a five-second hold, and its window is exercised where it can
            // be shortened (`DeclassifySessionDialog.test.tsx`) rather than by
            // making this suite wait five real seconds.
            privacy_reason: 'mcp:ucsfomopagent',
          }),
        ],
      },
    });

    renderList();
    await screen.findByText('Patient cohort');
    expect(screen.getByTestId('privacy-badge')).toHaveAttribute('data-privacy', 'private');

    await openRowMenu(/More actions for/);
    fireEvent.click(screen.getByText(/Make this chat public/));
    fireEvent.change(await screen.findByLabelText(/last 6 characters/i), {
      target: { value: '120000' },
    });
    fireEvent.click(screen.getByRole('button', { name: /Make public/ }));

    // The row stops claiming to be private without waiting for a refetch — an
    // action whose only visible effect arrives on the next page load reads as one
    // that did nothing. The control goes with it: a public row has nothing left
    // to declassify.
    await waitFor(() => expect(mocks.declassifySession).toHaveBeenCalled());
    await waitFor(() => expect(screen.queryByTestId('privacy-badge')).toBeNull());
    expect(screen.queryByLabelText(/More actions for/)).toBeNull();
  });

  it('leaves a public chat without the control at all', async () => {
    mocks.listSessions.mockResolvedValue({
      data: { sessions: [row({ id: 'session-2', name: 'Public notes' })] },
    });

    renderList();

    await screen.findByText('Public notes');
    expect(screen.queryByLabelText(/More actions for/)).toBeNull();
  });
});
