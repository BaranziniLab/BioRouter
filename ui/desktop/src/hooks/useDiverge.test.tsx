import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useDiverge, PRIVATE_COPY_TOAST_TITLE, PRIVATE_COPY_TOAST_MSG } from './useDiverge';
import { COPY_OF_PRIVATE_REFUSAL_MARKER, isPrivateCopyRefusal } from '../utils/userAction';

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

  // Issue #56 DR-19. The daemon's 403 body is the ONLY explanation the user can
  // get here, and it was being thrown away: under `throwOnError` the generated
  // client throws the PARSED BODY — a plain string — so `err instanceof Error`
  // is false and the generic fallback answered instead. On a backend started
  // outside the app (open question 23) that made the Diverge button on a private
  // chat fail with no reason given.
  const REFUSAL_BODY =
    'This chat is private, and branching it creates a new chat that inherits its private ' +
    `model — so ${COPY_OF_PRIVATE_REFUSAL_MARKER}, and this request carried no proof it ` +
    'came from them.';

  it('names the private-copy refusal instead of falling back to the generic message', async () => {
    mockDivergeSession.mockRejectedValue(REFUSAL_BODY);
    const { result } = renderHook(() => useDiverge());

    let returned: string | null = 'unset';
    await act(async () => {
      returned = await result.current.diverge('orig');
    });

    expect(returned).toBeNull();
    expect(mockCreateDivergedChatWindow).not.toHaveBeenCalled();
    expect(mockToastError).toHaveBeenCalledWith(
      expect.objectContaining({
        title: PRIVATE_COPY_TOAST_TITLE,
        msg: PRIVATE_COPY_TOAST_MSG,
      })
    );
  });

  it('leaves every other failure on the generic message', () => {
    // The negative control. A 500 from the same route carries plain text too,
    // and reporting one as a privacy refusal would be a confident lie.
    expect(isPrivateCopyRefusal(REFUSAL_BODY)).toBe(true);
    expect(isPrivateCopyRefusal('internal server error')).toBe(false);
    expect(isPrivateCopyRefusal(new Error(REFUSAL_BODY))).toBe(false);
    expect(isPrivateCopyRefusal(undefined)).toBe(false);
  });
});
