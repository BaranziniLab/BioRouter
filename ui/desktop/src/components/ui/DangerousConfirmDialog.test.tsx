import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { DangerousConfirmDialog } from './DangerousConfirmDialog';

afterEach(cleanup);

describe('DangerousConfirmDialog', () => {
  it('Cancel holds initial focus and Enter does not fire the destructive action', async () => {
    const user = userEvent.setup();
    const onConfirm = vi.fn();

    render(
      <DangerousConfirmDialog
        open
        title="Make this chat public?"
        phrase="def456"
        fieldLabel="Type the last 6 characters of this chat's ID"
        confirmLabel="Make public"
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />
    );

    // Radix's own default focuses the FIRST focusable node in the content —
    // which `DialogContent` renders as the close button, one Tab away from the
    // destructive confirm. This dialog overrides it.
    expect(document.activeElement).toBe(screen.getByRole('button', { name: /Cancel/ }));

    await user.type(screen.getByRole('textbox'), 'def4{Enter}');
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it('does not fire the destructive action on Enter even once the phrase is complete', async () => {
    const user = userEvent.setup();
    const onConfirm = vi.fn();

    render(
      <DangerousConfirmDialog
        open
        title="Make this chat public?"
        phrase="def456"
        fieldLabel="Type the last 6 characters of this chat's ID"
        confirmLabel="Make public"
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />
    );

    // The interesting case: the field is satisfied, so the confirm is enabled,
    // and a stray Return in the text field is exactly the accident a typed
    // confirmation exists to prevent.
    await user.type(screen.getByRole('textbox'), 'def456{Enter}');
    expect(screen.getByRole('button', { name: /Make public/ })).toBeEnabled();
    expect(onConfirm).not.toHaveBeenCalled();

    await user.click(screen.getByRole('button', { name: /Make public/ }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it('gates the confirm on the exact phrase, ignoring case and surrounding space', async () => {
    const user = userEvent.setup();
    render(
      <DangerousConfirmDialog
        open
        title="Disable protection?"
        phrase="DISABLE PROTECTION"
        fieldLabel="Type DISABLE PROTECTION to continue"
        confirmLabel="Disable"
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />
    );

    const confirm = screen.getByRole('button', { name: /Disable/ });
    const field = screen.getByRole('textbox');
    expect(confirm).toBeDisabled();

    await user.type(field, 'DISABLE');
    expect(confirm).toBeDisabled();

    await user.clear(field);
    await user.type(field, ' disable protection ');
    expect(confirm).toBeEnabled();
  });

  it('needs no phrase when none is required, and still never confirms on Enter', async () => {
    const user = userEvent.setup();
    const onConfirm = vi.fn();
    render(
      <DangerousConfirmDialog
        open
        title="Make this chat public?"
        confirmLabel="Make public"
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />
    );

    expect(screen.queryByRole('textbox')).toBeNull();
    expect(screen.getByRole('button', { name: /Make public/ })).toBeEnabled();
    expect(document.activeElement).toBe(screen.getByRole('button', { name: /Cancel/ }));

    await user.keyboard('{Enter}');
    expect(onConfirm).not.toHaveBeenCalled();
  });
});
