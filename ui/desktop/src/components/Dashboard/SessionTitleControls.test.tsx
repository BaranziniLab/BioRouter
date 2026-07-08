import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { SessionNamePill } from './SessionNamePill';
import { WindowTitleBar } from './WindowTitleBar';

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

  it('turns the dashboard window title into an input from the title menu', async () => {
    const user = userEvent.setup();

    render(
      <WindowTitleBar
        name="Weather summary website"
        accentColor="#eab308"
        onRename={vi.fn()}
        onClose={vi.fn()}
        onShrink={vi.fn()}
        onEnlarge={vi.fn()}
        onFold={vi.fn()}
        onPointerDownDrag={vi.fn()}
      />
    );

    await user.click(screen.getByText('Weather summary website'));
    expect(screen.queryByRole('textbox')).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /conversation title actions/i }));
    await user.click(await screen.findByRole('menuitem', { name: 'Rename' }));
    expect(screen.getByRole('textbox')).toHaveValue('Weather summary website');
  });

  it('keeps dashboard title menu presses out of the drag handler', () => {
    const onPointerDownDrag = vi.fn();

    render(
      <WindowTitleBar
        name="Weather summary website"
        accentColor="#eab308"
        onRename={vi.fn()}
        onClose={vi.fn()}
        onShrink={vi.fn()}
        onEnlarge={vi.fn()}
        onFold={vi.fn()}
        onPointerDownDrag={onPointerDownDrag}
      />
    );

    fireEvent.pointerDown(screen.getByRole('button', { name: /conversation title actions/i }));

    expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
    expect(onPointerDownDrag).not.toHaveBeenCalled();
  });
});
