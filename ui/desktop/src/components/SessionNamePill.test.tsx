import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { SessionNamePill } from './SessionNamePill';

describe('session title controls', () => {
  it('keeps the main chat title read-only until Rename is selected from the title menu', async () => {
    const user = userEvent.setup();

    render(<SessionNamePill name="Weather summary website" onRename={vi.fn()} />);

    await user.click(screen.getByText('Weather summary website'));
    expect(screen.queryByRole('textbox')).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /conversation title actions/i }));
    await user.click(await screen.findByRole('menuitem', { name: 'Rename' }));
    expect(screen.getByRole('textbox')).toHaveValue('Weather summary website');
  });

  it('does not rename the main chat title from pointer down on the title text', () => {
    render(<SessionNamePill name="Weather summary website" onRename={vi.fn()} />);

    fireEvent.pointerDown(screen.getByText('Weather summary website'));

    expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
  });

  it('offers Diverge from the main chat title menu when a response exists', async () => {
    const user = userEvent.setup();
    const onDiverge = vi.fn();

    render(
      <SessionNamePill
        name="Weather summary website"
        onRename={vi.fn()}
        onDiverge={onDiverge}
        canDiverge
      />
    );

    await user.click(screen.getByRole('button', { name: /conversation title actions/i }));
    await user.click(await screen.findByRole('menuitem', { name: 'Diverge' }));

    expect(onDiverge).toHaveBeenCalledTimes(1);
  });

  // Issue #56 §12.1, the paired half of
  // `SessionListView declassification entry point`. This menu is the obvious
  // slot for the action and the one the design forbids: it is rename/diverge,
  // one careless click from the chat title, and lowering a chat's tier is a
  // decision that owes its own confirmed surface. The pill shows the tier as a
  // LABEL and offers no control over it.
  it('never offers declassification from the chat title menu', async () => {
    const user = userEvent.setup();

    render(<SessionNamePill name="x" privacyTier="private" onRename={vi.fn()} />);

    // Asserted with the menu OPEN: a closed Radix menu renders no items, so the
    // same expectation on an unopened menu would pass against an implementation
    // that does put the action here.
    await user.click(screen.getByRole('button', { name: /conversation title actions/i }));
    await screen.findByRole('menuitem', { name: 'Rename' });
    expect(screen.queryByText(/Make this chat public/)).toBeNull();
  });
});
