import { describe, it, expect, vi, beforeEach } from 'vitest';

// Capture the toast without rendering it.
const mockToastError = vi.fn();
vi.mock('../toasts', () => ({
  toastError: (...args: unknown[]) => mockToastError(...args),
  toastSuccess: vi.fn(),
}));

import { handleCreateSessionError } from './BaseChat';

const restoreEventFrom = (dispatch: ReturnType<typeof vi.spyOn>): CustomEvent | undefined =>
  dispatch.mock.calls
    .map((c: unknown[]) => c[0] as CustomEvent)
    .find((e: CustomEvent) => e?.type === 'restore-chat-input');

describe('handleCreateSessionError', () => {
  beforeEach(() => vi.clearAllMocks());

  it('preserves the typed message and shows a disconnected toast on a connection error', () => {
    const dispatch = vi.spyOn(window, 'dispatchEvent');

    handleCreateSessionError(new TypeError('Failed to fetch'), {
      textValue: 'analyze my cohort',
      attachments: [],
      sessionId: null,
    });

    // (1) the text the composer already cleared is restored, not lost
    const restore = restoreEventFrom(dispatch);
    expect(restore).toBeTruthy();
    expect(restore!.detail).toMatchObject({ value: 'analyze my cohort', sessionId: null });

    // (2) a visible, connection-specific toast surfaces (no silent swallow)
    expect(mockToastError).toHaveBeenCalledTimes(1);
    expect(mockToastError).toHaveBeenCalledWith(
      expect.objectContaining({ title: 'Backend disconnected' })
    );

    dispatch.mockRestore();
  });

  it('still preserves text but shows a generic toast on a non-connection error', () => {
    const dispatch = vi.spyOn(window, 'dispatchEvent');

    handleCreateSessionError(new Error('HTTP 500 Internal Server Error'), {
      textValue: 'keep me',
      attachments: [],
      sessionId: 'sess-1',
    });

    const restore = restoreEventFrom(dispatch);
    expect(restore!.detail).toMatchObject({ value: 'keep me', sessionId: 'sess-1' });
    expect(mockToastError).toHaveBeenCalledWith(
      expect.objectContaining({ title: 'Failed to start session' })
    );

    dispatch.mockRestore();
  });
});
