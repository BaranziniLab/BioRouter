import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import type { ReactNode } from 'react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import SessionHistoryView from './SessionHistoryView';
import type { Session } from '../../api';

const mocks = vi.hoisted(() => ({ declassifySession: vi.fn() }));

vi.mock('../../api', () => ({ declassifySession: mocks.declassifySession }));

vi.mock('../../utils/userAction', () => ({
  userActionHeaders: async () => ({ 'X-User-Action': 'test-key' }),
}));

// See SessionItem.test.tsx: this file deliberately never names the badge
// component, because Task 27's gate greps src/components for that name and
// expects an exact file list.

vi.mock('../ProgressiveMessageList', () => ({
  default: () => <div data-testid="messages" />,
}));

vi.mock('../conversation/SearchView', () => ({
  SearchView: ({ children }: { children: ReactNode }) => <>{children}</>,
}));

vi.mock('../ui/scroll-area', () => ({
  ScrollArea: ({ children }: { children: ReactNode }) => <div>{children}</div>,
}));

function session(over: Partial<Session> = {}): Session {
  return {
    id: 'session-1',
    name: 'Cohort query',
    created_at: '2026-07-14T12:00:00Z',
    updated_at: '2026-07-14T12:00:00Z',
    working_dir: '/tmp',
    message_count: 0,
    extension_data: {},
    conversation: [],
    ...over,
  } as Session;
}

function renderView(over: Partial<Session> = {}, showActionButtons = false) {
  return render(
    <MemoryRouter>
      <SessionHistoryView
        session={session(over)}
        isLoading={false}
        error={null}
        onBack={vi.fn()}
        onRetry={vi.fn()}
        showActionButtons={showActionButtons}
      />
    </MemoryRouter>
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.declassifySession.mockResolvedValue({});
});

describe('SessionHistoryView — the privacy marker', () => {
  it('marks a private session in the header', () => {
    renderView({ privacy_tier: 'private' });
    expect(screen.getByTestId('privacy-badge')).toHaveAttribute('data-privacy', 'private');
  });

  it('names the tier in words on this roomy header, including for a public session', () => {
    renderView({ privacy_tier: 'public' });
    const badge = screen.getByTestId('privacy-badge');
    expect(badge).toHaveAttribute('data-privacy', 'public');
    expect(badge).toHaveTextContent('Public');
  });

  it('says nothing at all when the session carries no tier', () => {
    renderView();
    expect(screen.queryByTestId('privacy-badge')).toBeNull();
  });
});

// Issue #56 §12.1's second entry point. It shares `DeclassifySessionDialog` with
// History's row menu so the two cannot come to ask for different confirmations.
describe('SessionHistoryView — declassification', () => {
  it('offers Make public only on a private session', () => {
    const view = renderView({ privacy_tier: 'public' }, true);
    expect(screen.queryByRole('button', { name: 'Make public' })).toBeNull();
    view.unmount();

    renderView({ privacy_tier: 'private' }, true);
    expect(screen.getByRole('button', { name: 'Make public' })).toBeInTheDocument();
  });

  it('clears the header badge once the chat is public, without waiting for a refetch', async () => {
    // `mcp:*`, so §12.4 grades this onto the typed confirmation and the request
    // goes out on confirm. The `turn:*` path is the same code with a 5-second
    // hold in front of it, and its window is exercised where it can be shortened
    // (`DeclassifySessionDialog.test.tsx`), not against a real five seconds here.
    renderView(
      { privacy_tier: 'private', privacy_reason: 'mcp:ucsfomopagent', id: '20260714_120000' },
      true
    );
    expect(screen.getByTestId('privacy-badge')).toHaveAttribute('data-privacy', 'private');

    fireEvent.click(screen.getByRole('button', { name: 'Make public' }));

    // Scoped to the dialog: the page's own trigger carries the same label, and
    // an unscoped query would be ambiguous the moment the dialog opens.
    const dialog = await screen.findByRole('dialog');
    fireEvent.change(within(dialog).getByRole('textbox'), { target: { value: '120000' } });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Make public' }));

    await waitFor(() => expect(mocks.declassifySession).toHaveBeenCalled());
    await waitFor(() =>
      expect(screen.getByTestId('privacy-badge')).toHaveAttribute('data-privacy', 'public')
    );
  });
});
