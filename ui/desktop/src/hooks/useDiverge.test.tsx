import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useDiverge } from './useDiverge';

const mockDivergeSession = vi.fn();
vi.mock('../api', () => ({
  divergeSession: (...args: unknown[]) => mockDivergeSession(...args),
}));
const mockToastError = vi.fn();
vi.mock('../toasts', () => ({ toastError: (...args: unknown[]) => mockToastError(...args) }));
const mockNotifySessionListChanged = vi.fn();
vi.mock('../utils/sessionListCache', () => ({
  notifySessionListChanged: () => mockNotifySessionListChanged(),
}));

const mockCreateChatWindow = vi.fn();
const mockCreateDivergedChatWindow = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
  // @ts-expect-error test shim
  window.electron = {
    createChatWindow: mockCreateChatWindow,
    createDivergedChatWindow: mockCreateDivergedChatWindow,
  };
  // The backend resolves and returns the canonical, locked branch name.
  mockDivergeSession.mockResolvedValue({
    data: { sessionId: 'branch_1', workingDir: '/wd', name: 'Orig (branch 1)' },
  });
});

describe('useDiverge', () => {
  it('opens the branch in a NEW diverged Electron window, leaving the current one alone', async () => {
    const { result } = renderHook(() => useDiverge());

    let returned: string | null = null;
    await act(async () => {
      returned = await result.current.diverge('orig');
    });

    expect(returned).toBe('branch_1');
    // The canonical branch name is threaded into the new window so its tab is
    // born correct instead of flashing "New Session".
    expect(mockCreateDivergedChatWindow).toHaveBeenCalledWith('/wd', 'branch_1', 'Orig (branch 1)');
    // The branch enters every window's Recents / See-all / Home immediately —
    // it fires no session-created event, so this is the only announcement.
    expect(mockNotifySessionListChanged).toHaveBeenCalledTimes(1);
    // A diverge must never open a plain new chat window — that would start an
    // empty session instead of resuming the branch.
    expect(mockCreateChatWindow).not.toHaveBeenCalled();
  });

  it('forwards the branch point to the API when a specific message was clicked', async () => {
    const { result } = renderHook(() => useDiverge());

    await act(async () => {
      await result.current.diverge('orig', 1234, 'msg_7');
    });

    expect(mockDivergeSession).toHaveBeenCalledWith(
      expect.objectContaining({
        path: { session_id: 'orig' },
        body: { truncateAfter: 1234, truncateAfterId: 'msg_7' },
      })
    );
  });

  it('refuses to diverge without a session and opens no window', async () => {
    const { result } = renderHook(() => useDiverge());

    let returned: string | null = 'unset';
    await act(async () => {
      returned = await result.current.diverge('');
    });

    expect(returned).toBeNull();
    expect(mockDivergeSession).not.toHaveBeenCalled();
    expect(mockCreateDivergedChatWindow).not.toHaveBeenCalled();
    expect(mockToastError).toHaveBeenCalled();
  });

  it('surfaces a failure as a toast and opens no window', async () => {
    mockDivergeSession.mockRejectedValue(new Error('boom'));
    const { result } = renderHook(() => useDiverge());

    let returned: string | null = 'unset';
    await act(async () => {
      returned = await result.current.diverge('orig');
    });

    expect(returned).toBeNull();
    expect(mockCreateDivergedChatWindow).not.toHaveBeenCalled();
    expect(mockToastError).toHaveBeenCalled();
  });
});
