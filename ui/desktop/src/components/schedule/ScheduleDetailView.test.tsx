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

describe('one session id, one typeface', () => {
  /**
   * A session id was rendered three times on this screen in two faces.
   *
   * In a single run card, an unnamed session's heading printed
   * `Session ID: <id>` in the body font while the row 26 lines below printed
   * `ID: <id>` in `font-mono` — the SAME string, in one card, visible without
   * scrolling. The running-schedule line above printed a third copy in the body
   * font again. The working directory beside them was body here and mono in the
   * sidebar, the history view and the shared-session view.
   *
   * The rule is `main.css`'s own D-31 — "mono for data, sans for chrome" — and
   * an id and a path are both data. What makes this a bug rather than a
   * preference is that the two renderings are on screen at the same time.
   *
   * ⚠ jsdom never runs Tailwind, so `getComputedStyle(...).fontFamily` reports
   * the same thing whatever the class says. The assertion has to be on the
   * class, and it walks up from the text node because the class sits on a
   * wrapping span rather than on the element holding the text.
   */
  const monoAncestor = (element: HTMLElement | null): boolean => {
    for (let node = element; node; node = node.parentElement) {
      if (node.classList?.contains('font-mono')) return true;
    }
    return false;
  };

  it('sets an unnamed session id and its working directory in the data face', async () => {
    mocks.getScheduleSessions.mockResolvedValue([
      { id: 'sess-20260902-7f3', name: null, workingDir: '/tmp/work', messageCount: 2 },
    ]);
    renderDetails();

    // The heading's fallback and the ID row are the same string; both must be
    // mono, which is what the ID row was already doing alone.
    const rendered = await screen.findAllByText('sess-20260902-7f3');
    expect(rendered.length).toBeGreaterThan(1);
    for (const node of rendered) {
      expect(monoAncestor(node as HTMLElement)).toBe(true);
    }
    expect(monoAncestor(screen.getByText('/tmp/work') as HTMLElement)).toBe(true);
  });

  it('sets a running schedule’s current session id in the same face', async () => {
    mocks.listSchedules.mockResolvedValue([
      { ...schedule, currently_running: true, current_session_id: 'sess-running-1' },
    ]);
    renderDetails();
    expect(monoAncestor((await screen.findByText('sess-running-1')) as HTMLElement)).toBe(true);
  });
});
