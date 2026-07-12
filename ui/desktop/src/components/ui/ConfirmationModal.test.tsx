import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ConfirmationModal } from './ConfirmationModal';

afterEach(cleanup);

describe('ConfirmationModal', () => {
  it('blocks every dismissal path while submitting', async () => {
    const user = userEvent.setup();
    const onCancel = vi.fn();

    render(
      <ConfirmationModal
        isOpen
        title="Delete item"
        message="This cannot be undone."
        onConfirm={() => {}}
        onCancel={onCancel}
        isSubmitting
      />
    );

    expect(screen.queryByRole('button', { name: 'Close' })).not.toBeInTheDocument();
    await user.keyboard('{Escape}');

    const overlay = document.querySelector<HTMLElement>('[data-slot="dialog-overlay"]');
    expect(overlay).not.toBeNull();
    fireEvent.pointerDown(overlay!);

    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(onCancel).not.toHaveBeenCalled();
  });
});
