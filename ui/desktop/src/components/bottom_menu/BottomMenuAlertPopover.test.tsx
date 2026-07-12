import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { AlertType } from '../alerts';
import BottomMenuAlertPopover from './BottomMenuAlertPopover';

vi.mock('../alerts/AlertBox', () => ({
  AlertBox: ({ alert }: { alert: { message: string } }) => <div>{alert.message}</div>,
}));

const alerts = [{ type: AlertType.Info, message: 'Context is nearly full.' }];

describe('BottomMenuAlertPopover', () => {
  it('toggles from its trigger and dismisses with Escape', async () => {
    const user = userEvent.setup();
    render(<BottomMenuAlertPopover alerts={alerts} />);

    const trigger = screen.getByRole('button', { name: 'Show notifications' });
    await user.click(trigger);
    expect(screen.getByRole('dialog', { name: 'Notifications' })).toBeInTheDocument();
    expect(trigger).toHaveAttribute('aria-expanded', 'true');

    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByRole('dialog', { name: 'Notifications' })).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });

  it('closes when any outside control is clicked without blocking that control', async () => {
    const user = userEvent.setup();
    let outsideClicks = 0;
    render(
      <>
        <BottomMenuAlertPopover alerts={alerts} />
        <button type="button" onClick={() => (outsideClicks += 1)}>
          Outside action
        </button>
      </>
    );

    await user.click(screen.getByRole('button', { name: 'Show notifications' }));
    await user.click(screen.getByRole('button', { name: 'Outside action' }));

    expect(outsideClicks).toBe(1);
    expect(screen.queryByRole('dialog', { name: 'Notifications' })).not.toBeInTheDocument();
  });
});
