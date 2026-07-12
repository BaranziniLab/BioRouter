import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { ApplicationItem } from './ApplicationsView';

const app = {
  id: 'cohort-explorer',
  title: 'Cohort Explorer',
  description: 'Explore a paired cohort.',
  kind: 'agentic' as const,
  session_id: 'session-1',
};

describe('ApplicationItem', () => {
  it('keeps every action keyboard-accessible when hover controls are visually collapsed', async () => {
    const user = userEvent.setup();
    const onLaunch = vi.fn();

    render(
      <ApplicationItem
        app={app}
        onLaunch={onLaunch}
        onOpenConversation={vi.fn()}
        onExport={vi.fn()}
        onDelete={vi.fn()}
      />
    );

    const launch = screen.getByRole('button', { name: 'Launch Cohort Explorer in browser' });
    expect(
      screen.getByRole('button', {
        name: 'Open the conversation where Cohort Explorer was built',
      })
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Export Cohort Explorer to a folder' })
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Delete Cohort Explorer' })).toBeInTheDocument();
    expect(launch.parentElement).toHaveClass('sm:group-focus-within:opacity-100');

    await user.tab();
    expect(launch).toHaveFocus();
    await user.keyboard('{Enter}');
    expect(onLaunch).toHaveBeenCalledTimes(1);
  });

  it('disables duplicate launch and export actions while they are in flight', () => {
    const { container } = render(
      <ApplicationItem
        app={app}
        onLaunch={vi.fn()}
        onOpenConversation={vi.fn()}
        onExport={vi.fn()}
        onDelete={vi.fn()}
        isLaunching
        isExporting
      />
    );

    expect(
      screen.getByRole('button', { name: 'Launch Cohort Explorer in browser' })
    ).toBeDisabled();
    expect(
      screen.getByRole('button', { name: 'Export Cohort Explorer to a folder' })
    ).toBeDisabled();
    expect(container.firstElementChild).toHaveAttribute('aria-busy', 'true');
  });
});
