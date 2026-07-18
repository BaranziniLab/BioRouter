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
});
