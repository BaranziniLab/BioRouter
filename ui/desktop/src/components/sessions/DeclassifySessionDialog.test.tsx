import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DeclassifySessionDialog } from './DeclassifySessionDialog';
import type { Session } from '../../api';

const mocks = vi.hoisted(() => ({
  declassifySession: vi.fn(),
  toastError: vi.fn(),
}));

vi.mock('../../api', () => ({
  declassifySession: mocks.declassifySession,
}));

vi.mock('../../toasts', () => ({
  toastError: mocks.toastError,
  toastSuccess: vi.fn(),
}));

vi.mock('../../utils/userAction', () => ({
  userActionHeaders: async () => ({ 'X-User-Action': 'test-key' }),
}));

const s = {
  id: 'abc123def456',
  name: 'Cohort of 4,102 patients',
  working_dir: '/tmp',
  created_at: '2026-07-14T12:00:00Z',
  updated_at: '2026-07-14T12:00:00Z',
  extension_data: {},
  message_count: 12,
  privacy_tier: 'private',
  privacy_reason: 'mcp:ucsfomopagent',
} as unknown as Session;

beforeEach(() => {
  vi.clearAllMocks();
  mocks.declassifySession.mockResolvedValue({});
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('DeclassifySessionDialog', () => {
  it('the phrase gate is real, not decorative', async () => {
    const user = userEvent.setup();
    render(<DeclassifySessionDialog session={{ ...s, id: 'abc123def456' }} onClose={vi.fn()} />);

    const confirm = screen.getByRole('button', { name: /Make public/ });
    const field = () => screen.getByLabelText(/last 6 characters/i);

    await user.type(field(), 'ef45');
    expect(confirm).toBeDisabled();

    await user.clear(field());
    // The NAME, not the id. `is_default_session_name` shows "New Session",
    // "CLI Session" and "Session <N>" are live placeholders shared by dozens of
    // rows, so a name-typed phrase would confirm nothing about WHICH chat.
    await user.type(field(), s.name);
    expect(confirm).toBeDisabled();

    await user.clear(field());
    await user.type(field(), 'def456');
    expect(confirm).toBeEnabled();
  });

  it('a turn-only session gets the single-click path, an mcp session does not', () => {
    const { rerender } = render(
      <DeclassifySessionDialog
        session={{ ...s, privacy_reason: 'turn:versa_azure' }}
        onClose={vi.fn()}
      />
    );
    expect(screen.queryByRole('textbox')).toBeNull();
    expect(screen.getByText(/undo/i)).toBeInTheDocument();

    rerender(
      <DeclassifySessionDialog
        session={{ ...s, privacy_reason: 'mcp:ucsfomopagent' }}
        onClose={vi.fn()}
      />
    );
    expect(screen.getByRole('textbox')).toBeInTheDocument();
  });

  it('grades anything that is not a turn onto the typed confirmation', () => {
    // §12.4 asks for the strong control on a chat that reached a private data
    // source "or inherited from an `mcp:*` ancestor". A branch of an OMOP chat
    // carries `diverged:<parent>` and none of the `mcp:` spelling, so a
    // match-on-`mcp:` rule would hand the single-click control to a copy of
    // exactly the conversation the rule exists to protect. Mirrors
    // `requires_typed_confirmation` in `privacy/declassify.rs`.
    for (const reason of ['diverged:20260101_120000', 'inherited:x', 'imported', undefined]) {
      const view = render(
        <DeclassifySessionDialog session={{ ...s, privacy_reason: reason }} onClose={vi.fn()} />
      );
      expect(screen.getByRole('textbox')).toBeInTheDocument();
      view.unmount();
    }
  });

  it('sends the typed phrase and the proof-of-user, and reports the new tier', async () => {
    const user = userEvent.setup();
    const onDeclassified = vi.fn();
    render(
      <DeclassifySessionDialog session={s} onClose={vi.fn()} onDeclassified={onDeclassified} />
    );

    await user.type(screen.getByLabelText(/last 6 characters/i), 'def456');
    await user.click(screen.getByRole('button', { name: /Make public/ }));

    await waitFor(() =>
      expect(mocks.declassifySession).toHaveBeenCalledWith({
        path: { session_id: 'abc123def456' },
        body: { confirmation: 'def456' },
        headers: { 'X-User-Action': 'test-key' },
        throwOnError: true,
      })
    );
    await waitFor(() => expect(onDeclassified).toHaveBeenCalledWith('abc123def456'));
  });

  it('the single-click path holds the request open for the undo window', async () => {
    const user = userEvent.setup();
    const onDeclassified = vi.fn();
    render(
      <DeclassifySessionDialog
        session={{ ...s, privacy_reason: 'turn:versa_azure' }}
        onClose={vi.fn()}
        onDeclassified={onDeclassified}
        undoMs={60_000}
      />
    );

    await user.click(screen.getByRole('button', { name: /Make public/ }));

    // The undo is only real if NOTHING has been sent yet: declassification is
    // one-way at the daemon (a re-raise would write a second ledger row under a
    // different provenance), so an "undo" that fires after the request would be
    // a lie in the audit trail.
    const undo = await screen.findByRole('button', { name: /Undo/ });
    expect(mocks.declassifySession).not.toHaveBeenCalled();

    await user.click(undo);
    expect(mocks.declassifySession).not.toHaveBeenCalled();
    expect(onDeclassified).not.toHaveBeenCalled();
  });

  it('the single-click path sends once the undo window closes', async () => {
    const user = userEvent.setup();
    const onDeclassified = vi.fn();
    render(
      <DeclassifySessionDialog
        session={{ ...s, privacy_reason: 'turn:versa_azure' }}
        onClose={vi.fn()}
        onDeclassified={onDeclassified}
        undoMs={10}
      />
    );

    await user.click(screen.getByRole('button', { name: /Make public/ }));

    await waitFor(() =>
      expect(mocks.declassifySession).toHaveBeenCalledWith({
        path: { session_id: 'abc123def456' },
        body: { confirmation: null },
        headers: { 'X-User-Action': 'test-key' },
        throwOnError: true,
      })
    );
    await waitFor(() => expect(onDeclassified).toHaveBeenCalledWith('abc123def456'));
  });

  it('the undo window is a deadline, not a countdown a re-render restarts', async () => {
    // Both entry points hand this component INLINE arrows —
    // `SessionListView`: `onClose={() => setDeclassifyTarget(null)}`,
    // `SessionHistoryView`: both props — so it is given fresh callback
    // identities on every parent render, and the session list re-renders on its
    // own change subscription. A `send` that closes over them directly changes
    // identity each time; an effect keyed on it then clears the pending timeout
    // and arms a fresh FULL-LENGTH one. One re-render silently doubles the
    // advertised window, and any re-render source faster than it means the
    // request is never sent at all — the user sits in front of "becomes public
    // in 5 seconds" forever. That is precisely the "an action that reads as an
    // action that did nothing" failure this dialog exists to avoid.
    vi.useFakeTimers();
    const view = render(
      <DeclassifySessionDialog
        session={{ ...s, privacy_reason: 'turn:versa_azure' }}
        onClose={() => {}}
        onDeclassified={() => {}}
        undoMs={5000}
      />
    );
    fireEvent.click(screen.getByRole('button', { name: /Make public/ }));
    expect(screen.getByRole('button', { name: /Undo/ })).toBeInTheDocument();

    for (let elapsed = 0; elapsed < 5000; elapsed += 1000) {
      // A new pair of arrows each time, exactly as a parent re-render supplies.
      view.rerender(
        <DeclassifySessionDialog
          session={{ ...s, privacy_reason: 'turn:versa_azure' }}
          onClose={() => {}}
          onDeclassified={() => {}}
          undoMs={5000}
        />
      );
      await act(async () => {
        await vi.advanceTimersByTimeAsync(1000);
      });
    }

    expect(mocks.declassifySession).toHaveBeenCalledTimes(1);
  });

  it('abandoning the undo window sends nothing at all', () => {
    // Pins the direction this fails in, which is the safe one: the request has
    // not been made while the window is open, so unmounting — navigating away,
    // closing History — leaves the chat private. The alternative, firing an
    // irreversible action after the user has left the surface that announced
    // it, would be worse.
    vi.useFakeTimers();
    const { unmount } = render(
      <DeclassifySessionDialog
        session={{ ...s, privacy_reason: 'turn:versa_azure' }}
        onClose={vi.fn()}
        undoMs={5000}
      />
    );
    fireEvent.click(screen.getByRole('button', { name: /Make public/ }));
    unmount();
    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    expect(mocks.declassifySession).not.toHaveBeenCalled();
  });

  it('a refused single-click escalates to the typed control instead of looping', async () => {
    // The row this dialog is handed comes from the session list's cache. If it
    // still says `turn:*` but the daemon has since recorded `mcp:*`, the
    // single-click control is the wrong one: the request goes out with no
    // confirmation and the daemon refuses it over a phrase the user was never
    // shown. Dropping back to the SAME control re-renders it from the SAME
    // stale prop, so clicking again fails identically — an unrecoverable
    // dialog. The strong control is always an acceptable answer to a refusal,
    // and it is the only one that can recover from a stale grade.
    mocks.declassifySession.mockRejectedValue(
      new Error("The confirmation did not match the last six characters of this chat's id.")
    );

    render(
      <DeclassifySessionDialog
        session={{ ...s, privacy_reason: 'turn:versa_azure' }}
        onClose={vi.fn()}
        undoMs={5}
      />
    );
    fireEvent.click(screen.getByRole('button', { name: /Make public/ }));

    await waitFor(() => expect(mocks.toastError).toHaveBeenCalled());
    expect(screen.getByLabelText(/last 6 characters/i)).toBeInTheDocument();
    // And it says why the field appeared, rather than borrowing the copy for a
    // chat that genuinely reached a private data source — which would be a
    // claim about this conversation that nothing here established.
    expect(screen.queryByText(/reached a private data source/i)).toBeNull();
  });

  it('surfaces a refusal instead of claiming the chat is now public', async () => {
    const user = userEvent.setup();
    const onDeclassified = vi.fn();
    mocks.declassifySession.mockRejectedValue('Nothing was changed.');

    render(
      <DeclassifySessionDialog session={s} onClose={vi.fn()} onDeclassified={onDeclassified} />
    );
    await user.type(screen.getByLabelText(/last 6 characters/i), 'def456');
    await user.click(screen.getByRole('button', { name: /Make public/ }));

    await waitFor(() => expect(mocks.toastError).toHaveBeenCalled());
    expect(onDeclassified).not.toHaveBeenCalled();
  });
});
