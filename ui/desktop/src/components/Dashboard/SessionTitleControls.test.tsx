import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { SessionNamePill } from './SessionNamePill';
import { WindowTitleBar } from './WindowTitleBar';

describe('session title controls', () => {
  it('turns the main chat title into an input when the displayed title is clicked', async () => {
    const user = userEvent.setup();

    render(<SessionNamePill name="Weather summary website" onRename={vi.fn()} />);

    await user.click(screen.getByRole('button', { name: 'Weather summary website' }));

    expect(screen.getByRole('textbox')).toHaveValue('Weather summary website');
  });

  it('turns the main chat title into an input on pointer down', () => {
    render(<SessionNamePill name="Weather summary website" onRename={vi.fn()} />);

    fireEvent.pointerDown(screen.getByRole('button', { name: 'Weather summary website' }));

    expect(screen.getByRole('textbox')).toHaveValue('Weather summary website');
  });

  it('turns the dashboard window title into an input on a single click', async () => {
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

    await user.click(screen.getByRole('button', { name: 'Weather summary website' }));

    expect(screen.getByRole('textbox')).toHaveValue('Weather summary website');
  });

  it('keeps dashboard title presses out of the drag handler', () => {
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

    fireEvent.pointerDown(screen.getByRole('button', { name: 'Weather summary website' }));

    expect(screen.getByRole('textbox')).toHaveValue('Weather summary website');
    expect(onPointerDownDrag).not.toHaveBeenCalled();
  });
});
