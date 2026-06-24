import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import MessageDivergeLink from './MessageDivergeLink';
import { DashboardContext, DashboardApi } from '../contexts/DashboardContext';
import { DashboardCanvasContext } from '../contexts/DashboardCanvasContext';

const mockDivergeSession = vi.fn();
vi.mock('../api', () => ({
  divergeSession: (...args: unknown[]) => mockDivergeSession(...args),
}));

const mockToastError = vi.fn();
vi.mock('../toasts', () => ({
  toastError: (...args: unknown[]) => mockToastError(...args),
}));

// Stub the Electron bridge used to open a new window.
const mockCreateChatWindow = vi.fn();
beforeEach(() => {
  vi.clearAllMocks();
  // @ts-expect-error test shim
  window.electron = { createChatWindow: mockCreateChatWindow };
});

/** Minimal DashboardApi with a spy spawnWindow; the rest are no-ops. */
function makeDashboard(spawnWindow: DashboardApi['spawnWindow']): DashboardApi {
  return new Proxy({ spawnWindow } as Partial<DashboardApi>, {
    get(target, prop) {
      if (prop in target) return target[prop as keyof DashboardApi];
      return () => {};
    },
  }) as DashboardApi;
}

describe('MessageDivergeLink', () => {
  it('opens a new desktop window when NOT inside a dashboard', async () => {
    mockDivergeSession.mockResolvedValue({
      data: { sessionId: '20260622_9', workingDir: '/home/u/proj' },
    });

    render(<MessageDivergeLink sessionId="20260622_1" />);
    fireEvent.click(screen.getByRole('button', { name: /diverge/i }));

    await waitFor(() => {
      expect(mockDivergeSession).toHaveBeenCalledWith({
        path: { session_id: '20260622_1' },
        body: {},
        throwOnError: true,
      });
    });
    // New window opened with the diverged session id, in pair view.
    expect(mockCreateChatWindow).toHaveBeenCalledWith(
      undefined,
      '/home/u/proj',
      undefined,
      '20260622_9',
      'pair'
    );
  });

  it('passes the message timestamp as truncateAfter so the branch ends at this answer', async () => {
    mockDivergeSession.mockResolvedValue({
      data: { sessionId: '20260622_9', workingDir: '/home/u/proj' },
    });

    render(<MessageDivergeLink sessionId="20260622_1" truncateAfterMs={1717171717000} />);
    fireEvent.click(screen.getByRole('button', { name: /diverge/i }));

    await waitFor(() => {
      expect(mockDivergeSession).toHaveBeenCalledWith({
        path: { session_id: '20260622_1' },
        body: { truncateAfter: 1717171717000 },
        throwOnError: true,
      });
    });
  });

  it('spawns an inline chat box when inside a dashboard', async () => {
    mockDivergeSession.mockResolvedValue({
      data: { sessionId: '20260622_9', workingDir: '/home/u/proj' },
    });
    const spawnWindow = vi.fn().mockResolvedValue(undefined);

    render(
      <DashboardContext.Provider value={makeDashboard(spawnWindow)}>
        {/* Inside a dashboard CANVAS window — only then does diverge spawn
            on-canvas instead of opening a new Electron window. */}
        <DashboardCanvasContext.Provider value={true}>
          <MessageDivergeLink sessionId="20260622_1" />
        </DashboardCanvasContext.Provider>
      </DashboardContext.Provider>
    );
    fireEvent.click(screen.getByRole('button', { name: /diverge/i }));

    await waitFor(() => {
      expect(spawnWindow).toHaveBeenCalledWith({
        resumeSessionId: '20260622_9',
        cwd: '/home/u/proj',
      });
    });
    // No new desktop window in dashboard mode.
    expect(mockCreateChatWindow).not.toHaveBeenCalled();
  });

  it('inside the dashboard PROVIDER but NOT on the canvas opens a new window (isolation)', async () => {
    mockDivergeSession.mockResolvedValue({
      data: { sessionId: '20260622_9', workingDir: '/home/u/proj' },
    });
    const spawnWindow = vi.fn().mockResolvedValue(undefined);
    // The DashboardProvider wraps the whole app, so the context is present even
    // in the chat view. A diverge from here must NOT leak into the dashboard.
    render(
      <DashboardContext.Provider value={makeDashboard(spawnWindow)}>
        <MessageDivergeLink sessionId="20260622_1" />
      </DashboardContext.Provider>
    );
    fireEvent.click(screen.getByRole('button', { name: /diverge/i }));

    await waitFor(() => expect(mockCreateChatWindow).toHaveBeenCalled());
    expect(spawnWindow).not.toHaveBeenCalled();
  });

  it('shows an error toast and opens nothing when the backend fails', async () => {
    mockDivergeSession.mockRejectedValue(new Error('boom'));

    render(<MessageDivergeLink sessionId="20260622_1" />);
    fireEvent.click(screen.getByRole('button', { name: /diverge/i }));

    await waitFor(() => expect(mockToastError).toHaveBeenCalled());
    expect(mockCreateChatWindow).not.toHaveBeenCalled();
  });

  it('errors (and opens nothing) if the response lacks a session id', async () => {
    mockDivergeSession.mockResolvedValue({ data: { workingDir: '/x' } });

    render(<MessageDivergeLink sessionId="20260622_1" />);
    fireEvent.click(screen.getByRole('button', { name: /diverge/i }));

    await waitFor(() => expect(mockToastError).toHaveBeenCalled());
    expect(mockCreateChatWindow).not.toHaveBeenCalled();
  });

  it('ignores rapid double-clicks (only one diverge in flight)', async () => {
    let resolve!: (v: unknown) => void;
    mockDivergeSession.mockReturnValue(
      new Promise((r) => {
        resolve = r;
      })
    );

    render(<MessageDivergeLink sessionId="20260622_1" />);
    const btn = screen.getByRole('button', { name: /diverge/i });
    fireEvent.click(btn);
    fireEvent.click(btn);
    fireEvent.click(btn);

    expect(mockDivergeSession).toHaveBeenCalledTimes(1);
    resolve({ data: { sessionId: '20260622_9', workingDir: '/x' } });
    await waitFor(() => expect(mockCreateChatWindow).toHaveBeenCalledTimes(1));
  });
});
