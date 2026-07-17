import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  showExtensionLoadResults,
  setFocusedChatSession,
  resetExtensionToastState,
} from './extensionErrorUtils';
import { toastService } from '../toasts';

vi.mock('../toasts', () => ({
  toastService: {
    extensionLoading: vi.fn(),
    error: vi.fn(),
    isExtensionToastActive: vi.fn(() => false),
  },
}));

const ok = (name: string) => ({ name, success: true, error: null });
const bad = (name: string, error = 'spawn ENOENT') => ({ name, success: false, error });

describe('showExtensionLoadResults — multi-chat toast rules', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetExtensionToastState();
    vi.mocked(toastService.isExtensionToastActive).mockReturnValue(false);
  });

  it('toasts a clean load for the chat the user is focused on', () => {
    setFocusedChatSession('chat-a');
    showExtensionLoadResults([ok('developer'), ok('memory')], 'chat-a');

    expect(toastService.extensionLoading).toHaveBeenCalledTimes(1);
  });

  // The regression this whole rule exists for: four splits opening at once must
  // not stack four "all loaded" toasts over the transcript you are reading.
  it('stays silent for a BACKGROUND chat that loads cleanly', () => {
    setFocusedChatSession('chat-a');
    showExtensionLoadResults([ok('developer')], 'chat-b');

    expect(toastService.extensionLoading).not.toHaveBeenCalled();
  });

  it('four chats opening at once produce exactly one toast — the focused one', () => {
    setFocusedChatSession('chat-2');
    for (const id of ['chat-1', 'chat-2', 'chat-3', 'chat-4']) {
      showExtensionLoadResults([ok('developer'), ok('memory')], id);
    }

    expect(toastService.extensionLoading).toHaveBeenCalledTimes(1);
  });

  // A silent failure resurfaces later as a broken tool call. Never swallow it,
  // even from a pane the user is not currently looking at.
  it('toasts a FAILURE from a background chat anyway', () => {
    setFocusedChatSession('chat-a');
    showExtensionLoadResults([ok('developer'), bad('memory')], 'chat-b');

    expect(toastService.extensionLoading).toHaveBeenCalledTimes(1);
    const [statuses] = vi.mocked(toastService.extensionLoading).mock.calls[0];
    expect(statuses.find((s) => s.name === 'memory')?.status).toBe('error');
  });

  it('does not let a later success overwrite a failure still on screen', () => {
    setFocusedChatSession('chat-a');
    showExtensionLoadResults([bad('memory')], 'chat-b');
    vi.mocked(toastService.extensionLoading).mockClear();

    // That failure toast is still up...
    vi.mocked(toastService.isExtensionToastActive).mockReturnValue(true);
    showExtensionLoadResults([ok('developer')], 'chat-a');

    expect(toastService.extensionLoading).not.toHaveBeenCalled();
  });

  it('lets a success through once the failure toast has gone', () => {
    setFocusedChatSession('chat-a');
    showExtensionLoadResults([bad('memory')], 'chat-b');
    vi.mocked(toastService.extensionLoading).mockClear();

    vi.mocked(toastService.isExtensionToastActive).mockReturnValue(false);
    showExtensionLoadResults([ok('developer')], 'chat-a');

    expect(toastService.extensionLoading).toHaveBeenCalledTimes(1);
  });

  it('toasts when no host has claimed focus (single-chat hosts degrade permissively)', () => {
    showExtensionLoadResults([ok('developer')], 'chat-a');

    expect(toastService.extensionLoading).toHaveBeenCalledTimes(1);
  });

  it('toasts for non-chat callers, which pass no session id', () => {
    setFocusedChatSession('chat-a');
    showExtensionLoadResults([ok('developer'), ok('memory')]);

    expect(toastService.extensionLoading).toHaveBeenCalledTimes(1);
  });

  it('says nothing when there is nothing to report', () => {
    setFocusedChatSession('chat-a');
    showExtensionLoadResults([], 'chat-a');
    showExtensionLoadResults(null, 'chat-a');
    showExtensionLoadResults(undefined, 'chat-a');

    expect(toastService.extensionLoading).not.toHaveBeenCalled();
    expect(toastService.error).not.toHaveBeenCalled();
  });

  it('a lone failed extension still gets the dedicated error toast', () => {
    setFocusedChatSession('chat-a');
    showExtensionLoadResults([bad('memory', 'connection refused')], 'chat-a');

    expect(toastService.error).toHaveBeenCalledTimes(1);
    expect(vi.mocked(toastService.error).mock.calls[0][0]).toMatchObject({
      title: 'memory',
      msg: 'connection refused',
    });
  });
});
