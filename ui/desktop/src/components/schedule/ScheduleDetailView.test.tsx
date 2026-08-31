import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { MemoryRouter } from 'react-router-dom';
import type { ReactNode } from 'react';
import ScheduleDetailView from './ScheduleDetailView';

const mocks = vi.hoisted(() => ({
  getScheduleSessions: vi.fn(),
  listSchedules: vi.fn(),
  runScheduleNow: vi.fn(),
  pauseSchedule: vi.fn(),
  toastError: vi.fn(),
  toastSuccess: vi.fn(),
}));

vi.mock('../../schedule', () => ({
  ...mocks,
  unpauseSchedule: vi.fn(),
  updateSchedule: vi.fn(),
  killRunningJob: vi.fn(),
  inspectRunningJob: vi.fn(),
}));
vi.mock('../../toasts', () => mocks);
vi.mock('../../api', () => ({ getSession: vi.fn() }));
vi.mock('../sessions/SessionHistoryView', () => ({ default: () => null }));
vi.mock('./ScheduleModal', () => ({ ScheduleModal: () => null }));
vi.mock('../ui/scroll-area', () => ({
  ScrollArea: ({ children }: { children: ReactNode }) => <div>{children}</div>,
}));

const schedule = {
  id: 'daily-meditation',
  source: '/tmp/daily-meditation.yaml',
  cron: '0 0 3 * * *',
  last_run: null,
  currently_running: false,
  paused: false,
};

function renderDetails() {
  return render(
    <MemoryRouter>
      <ScheduleDetailView scheduleId={schedule.id} onNavigateBack={vi.fn()} />
    </MemoryRouter>
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.listSchedules.mockResolvedValue([schedule]);
  mocks.getScheduleSessions.mockResolvedValue([]);
  mocks.pauseSchedule.mockResolvedValue(undefined);
});

describe('manual schedule run feedback', () => {
  it('announces a pending run without claiming success or submitting twice', async () => {
    let finish!: (id: string) => void;
    mocks.runScheduleNow.mockReturnValue(new Promise<string>((resolve) => (finish = resolve)));
    renderDetails();
    const run = await screen.findByRole('button', { name: 'Run Schedule Now' });
    fireEvent.click(run);
    fireEvent.click(run);

    expect(mocks.runScheduleNow).toHaveBeenCalledTimes(1);
    expect(run).toBeDisabled();
    expect(screen.getByRole('status')).toHaveTextContent('Waiting for the scheduled run to finish');
    expect(mocks.toastSuccess).not.toHaveBeenCalled();

    await act(async () => finish('finished-session'));
    await waitFor(() => expect(run).not.toBeDisabled());
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
    expect(mocks.getScheduleSessions).toHaveBeenCalledTimes(2);
  });

  it('clears pending feedback and allows a retry after a failed run', async () => {
    let fail!: (reason: Error) => void;
    mocks.runScheduleNow.mockReturnValue(new Promise<string>((_, reject) => (fail = reject)));
    renderDetails();
    const run = await screen.findByRole('button', { name: 'Run Schedule Now' });
    fireEvent.click(run);
    expect(screen.getByRole('status')).toBeInTheDocument();

    await act(async () => fail(new Error('Provider unavailable')));
    await waitFor(() => expect(run).not.toBeDisabled());
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
    expect(mocks.toastError).toHaveBeenCalledWith(
      expect.objectContaining({ msg: 'Provider unavailable' })
    );
    expect(mocks.toastSuccess).not.toHaveBeenCalled();
  });

  it('does not describe another pending action as a schedule run', async () => {
    let finish!: () => void;
    mocks.pauseSchedule.mockReturnValue(new Promise<void>((resolve) => (finish = resolve)));
    renderDetails();
    fireEvent.click(await screen.findByRole('button', { name: 'Pause Schedule' }));
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
    expect(mocks.runScheduleNow).not.toHaveBeenCalled();
    await act(async () => finish());
  });
});
