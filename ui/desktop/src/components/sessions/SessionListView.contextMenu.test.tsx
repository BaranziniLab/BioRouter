import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import SessionListView from './SessionListView';
import { clearSessionListCache } from '../../utils/sessionListCache';
import type { Session } from '../../api';

const mocks = vi.hoisted(() => ({
  listSessions: vi.fn(),
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
}));

vi.mock('../../api', () => ({
  listSessions: mocks.listSessions,
  deleteSession: vi.fn(),
  exportSession: vi.fn(),
  importSession: vi.fn(),
  updateSessionName: vi.fn(),
  declassifySession: vi.fn(),
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

const session = {
  id: '20260823_2',
  name: 'Excel research',
  working_dir: '/Users/x/project',
  created_at: '2026-08-23T12:00:00Z',
  updated_at: '2026-08-23T12:00:00Z',
  extension_data: {},
  message_count: 4,
} as Session;

beforeEach(() => {
  vi.clearAllMocks();
  clearSessionListCache();
  mocks.listSessions.mockResolvedValue({ data: { sessions: [session] } });
  Object.assign(navigator, { clipboard: { writeText: vi.fn(() => Promise.resolve()) } });
  Object.assign(window, { electron: { createChatWindow: vi.fn() } });
});

async function renderHistory(onSelectSession = vi.fn()) {
  render(
    <MemoryRouter>
      <SessionListView onSelectSession={onSelectSession} />
    </MemoryRouter>
  );
  await screen.findByText('Excel research');
  return { onSelectSession };
}

describe('History row menus', () => {
  it('offers the three actions on a right-click', async () => {
    await renderHistory();
    fireEvent.contextMenu(screen.getByText('Excel research'));

    const items = await screen.findAllByRole('menuitem');
    expect(items.map((item) => item.textContent)).toEqual([
      'Open in new tab',
      'Open in new window',
      'Copy conversation ID',
    ]);
  });

  /**
   * The `⋯` overflow is the keyboard path — Tab to it, Enter — so it must carry
   * the SAME list. It had the first two items before #114; asserting all three
   * against the same expectation as the right-click menu is what stops the two
   * menus on one row from drifting apart.
   */
  it('offers the identical list from the keyboard-reachable overflow', async () => {
    await renderHistory();
    // Radix opens a dropdown on pointerdown, not click — the same door the
    // declassify suite next door uses.
    fireEvent.pointerDown(screen.getByLabelText('More actions for Excel research'), {
      button: 0,
      ctrlKey: false,
    });

    const items = await screen.findAllByRole('menuitem');
    expect(items.slice(0, 3).map((item) => item.textContent)).toEqual([
      'Open in new tab',
      'Open in new window',
      'Copy conversation ID',
    ]);
  });

  it('copies the raw conversation id from the right-click menu', async () => {
    await renderHistory();
    fireEvent.contextMenu(screen.getByText('Excel research'));
    fireEvent.click(await screen.findByText('Copy conversation ID'));

    await waitFor(() => expect(navigator.clipboard.writeText).toHaveBeenCalledWith('20260823_2'));
  });

  it('opens a tab through the same handler the row click uses', async () => {
    const { onSelectSession } = await renderHistory();
    fireEvent.contextMenu(screen.getByText('Excel research'));
    fireEvent.click(await screen.findByText('Open in new tab'));

    await waitFor(() => expect(onSelectSession).toHaveBeenCalledWith('20260823_2'));
  });

  /**
   * The window path was History's before it was shared; this pins that sharing
   * it did not change the five arguments the window is opened with.
   */
  it('opens a window with the arguments History always used', async () => {
    await renderHistory();
    fireEvent.contextMenu(screen.getByText('Excel research'));
    fireEvent.click(await screen.findByText('Open in new window'));

    await waitFor(() =>
      expect(window.electron.createChatWindow).toHaveBeenCalledWith(
        undefined,
        '/Users/x/project',
        undefined,
        '20260823_2',
        'pair'
      )
    );
  });

  /**
   * "Works for user, scheduled, diverged, and subagent rows." The menu is not
   * gated on kind and must not become so — a subagent run is exactly the row
   * whose id an agent is most likely to be asked about, and it is the one that
   * only appears once "Show subagent runs" is on, so it is the one a
   * kind-dependent regression would hide.
   *
   * `chatRowActions` takes no kind at all, which is why this can be asserted
   * once across all four rather than four times.
   */
  it('offers the menu on every kind of row, copying each row’s own id', async () => {
    mocks.listSessions.mockResolvedValue({
      data: {
        sessions: [
          session,
          { ...session, id: '20260823_3', name: 'Nightly QC', session_type: 'scheduled' },
          { ...session, id: '20260823_4', name: 'Branch of Excel', diverged_from: '20260823_2' },
          {
            ...session,
            id: '20260823_5',
            name: 'Subagent: Word research',
            session_type: 'sub_agent',
            parent_session_id: '20260823_2',
          },
        ],
      },
    });
    await renderHistory();
    // The subagent row is only fetched and shown once the toggle is on.
    fireEvent.click(await screen.findByLabelText(/show subagent runs/i));
    await screen.findByText('Subagent: Word research');

    for (const [label, id] of [
      ['Excel research', '20260823_2'],
      ['Nightly QC', '20260823_3'],
      ['Branch of Excel', '20260823_4'],
      ['Subagent: Word research', '20260823_5'],
    ] as const) {
      fireEvent.contextMenu(screen.getByText(label));
      const items = await screen.findAllByRole('menuitem');
      expect(items.map((item) => item.textContent)).toEqual([
        'Open in new tab',
        'Open in new window',
        'Copy conversation ID',
      ]);
      fireEvent.click(screen.getByText('Copy conversation ID'));
      await waitFor(() => expect(navigator.clipboard.writeText).toHaveBeenLastCalledWith(id));
    }
  });

  /**
   * The trigger is `asChild` on the row itself. A wrapper element here would sit
   * between `.biorouter-list-shell` and its rows and take the list separators
   * with it — invisible to jsdom, so the structural assertion is the guard.
   */
  it('keeps the menu trigger on the row, adding no wrapper', async () => {
    await renderHistory();
    const row = document.querySelector('.session-item');
    expect(row).not.toBeNull();
    expect(row?.parentElement?.className).toContain('session-grid');
  });
});
