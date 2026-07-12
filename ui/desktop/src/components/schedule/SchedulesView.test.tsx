import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { MemoryRouter } from 'react-router-dom';
import type { ReactNode } from 'react';
import SchedulesView from './SchedulesView';

const mocks = vi.hoisted(() => ({
  listSchedules: vi.fn(),
  createSchedule: vi.fn(),
  deleteSchedule: vi.fn(),
  pauseSchedule: vi.fn(),
  unpauseSchedule: vi.fn(),
  updateSchedule: vi.fn(),
  killRunningJob: vi.fn(),
  inspectRunningJob: vi.fn(),
}));

vi.mock('../../schedule', () => ({
  listSchedules: mocks.listSchedules,
  createSchedule: mocks.createSchedule,
  deleteSchedule: mocks.deleteSchedule,
  pauseSchedule: mocks.pauseSchedule,
  unpauseSchedule: mocks.unpauseSchedule,
  updateSchedule: mocks.updateSchedule,
  killRunningJob: mocks.killRunningJob,
  inspectRunningJob: mocks.inspectRunningJob,
}));

vi.mock('./ScheduleDetailView', () => ({
  default: ({ scheduleId }: { scheduleId: string }) => (
    <div aria-label="Schedule detail">Schedule detail for {scheduleId}</div>
  ),
}));

vi.mock('./ScheduleModal', () => ({
  ScheduleModal: ({ isOpen }: { isOpen: boolean }) =>
    isOpen ? <div role="dialog" aria-label="Create schedule form" /> : null,
}));

vi.mock('../ui/scroll-area', () => ({
  ScrollArea: ({ children }: { children: ReactNode }) => <div>{children}</div>,
}));

vi.mock('../../toasts', () => ({
  toastError: vi.fn(),
  toastSuccess: vi.fn(),
}));

const schedule = {
  id: 'nightly-cohort',
  source: '/tmp/nightly-cohort.yaml',
  cron: '0 0 1 * * *',
  last_run: null,
  currently_running: false,
  paused: false,
};

function renderSchedules() {
  return render(
    <MemoryRouter>
      <SchedulesView />
    </MemoryRouter>
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.listSchedules.mockResolvedValue([schedule]);
  mocks.createSchedule.mockResolvedValue(undefined);
  mocks.deleteSchedule.mockResolvedValue(undefined);
  mocks.pauseSchedule.mockResolvedValue(undefined);
  mocks.unpauseSchedule.mockResolvedValue(undefined);
  mocks.updateSchedule.mockResolvedValue(undefined);
  mocks.killRunningJob.mockResolvedValue({ message: 'Killed' });
  mocks.inspectRunningJob.mockResolvedValue({});
});

describe('SchedulesView interactions', () => {
  it('exposes schedule navigation as a keyboard-activatable button without nesting actions', async () => {
    const user = userEvent.setup();
    renderSchedules();

    const openSchedule = await screen.findByRole('button', {
      name: 'View schedule nightly-cohort',
    });
    expect(openSchedule.tagName).toBe('BUTTON');
    expect(openSchedule.querySelector('button')).toBeNull();

    openSchedule.focus();
    await user.keyboard('{Enter}');

    expect(await screen.findByText('Schedule detail for nightly-cohort')).toBeInTheDocument();
  });

  it('asks for deletion confirmation and dismisses it when clicking outside', async () => {
    const user = userEvent.setup();
    renderSchedules();

    await user.click(await screen.findByRole('button', { name: 'Delete nightly-cohort' }));
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Delete "nightly-cohort"?' })).toBeInTheDocument();

    fireEvent.pointerDown(document.body);

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(mocks.deleteSchedule).not.toHaveBeenCalled();
  });

  it('suppresses duplicate rapid actions while the first request is pending', async () => {
    let finishPause: (() => void) | undefined;
    mocks.pauseSchedule.mockReturnValueOnce(
      new Promise<void>((resolve) => {
        finishPause = resolve;
      })
    );
    renderSchedules();

    const pause = await screen.findByRole('button', { name: 'Pause nightly-cohort' });
    fireEvent.click(pause);
    fireEvent.click(pause);

    expect(mocks.pauseSchedule).toHaveBeenCalledTimes(1);
    expect(pause).toBeDisabled();

    await act(async () => finishPause?.());
    await waitFor(() => expect(pause).not.toBeDisabled());
  });

  it('presents an accessible empty state that opens schedule creation', async () => {
    const user = userEvent.setup();
    mocks.listSchedules.mockResolvedValueOnce([]);
    renderSchedules();

    const title = await screen.findByRole('heading', { name: 'No schedules yet' });
    const emptyState = title.closest('section');
    expect(emptyState).toHaveAccessibleDescription(
      'Create a schedule to run a saved workflow automatically at the time you choose.'
    );

    await user.click(screen.getByRole('button', { name: 'Create schedule' }));
    expect(screen.getByRole('dialog', { name: 'Create schedule form' })).toBeInTheDocument();
  });
});
