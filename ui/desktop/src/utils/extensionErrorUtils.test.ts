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
    showExtensionLoadResults([ok('weather-mcp'), ok('notion-mcp')], 'chat-a');

    expect(toastService.extensionLoading).toHaveBeenCalledTimes(1);
  });

  // The regression this whole rule exists for: four splits opening at once must
  // not stack four "all loaded" toasts over the transcript you are reading.
  it('stays silent for a BACKGROUND chat that loads cleanly', () => {
    setFocusedChatSession('chat-a');
    showExtensionLoadResults([ok('weather-mcp')], 'chat-b');

    expect(toastService.extensionLoading).not.toHaveBeenCalled();
  });

  it('four chats opening at once produce exactly one toast — the focused one', () => {
    setFocusedChatSession('chat-2');
    for (const id of ['chat-1', 'chat-2', 'chat-3', 'chat-4']) {
      showExtensionLoadResults([ok('weather-mcp'), ok('notion-mcp')], id);
    }

    expect(toastService.extensionLoading).toHaveBeenCalledTimes(1);
  });

  // A silent failure resurfaces later as a broken tool call. Never swallow it,
  // even from a pane the user is not currently looking at.
  it('toasts a FAILURE from a background chat anyway', () => {
    setFocusedChatSession('chat-a');
    showExtensionLoadResults([ok('weather-mcp'), bad('notion-mcp')], 'chat-b');

    expect(toastService.extensionLoading).toHaveBeenCalledTimes(1);
    const [statuses] = vi.mocked(toastService.extensionLoading).mock.calls[0];
    expect(statuses.find((s) => s.name === 'notion-mcp')?.status).toBe('error');
  });

  it('does not let a later success overwrite a failure still on screen', () => {
    setFocusedChatSession('chat-a');
    showExtensionLoadResults([bad('notion-mcp')], 'chat-b');
    vi.mocked(toastService.extensionLoading).mockClear();

    // That failure toast is still up...
    vi.mocked(toastService.isExtensionToastActive).mockReturnValue(true);
    showExtensionLoadResults([ok('weather-mcp')], 'chat-a');

    expect(toastService.extensionLoading).not.toHaveBeenCalled();
  });

  it('lets a success through once the failure toast has gone', () => {
    setFocusedChatSession('chat-a');
    showExtensionLoadResults([bad('notion-mcp')], 'chat-b');
    vi.mocked(toastService.extensionLoading).mockClear();

    vi.mocked(toastService.isExtensionToastActive).mockReturnValue(false);
    showExtensionLoadResults([ok('weather-mcp')], 'chat-a');

    expect(toastService.extensionLoading).toHaveBeenCalledTimes(1);
  });

  it('toasts when no host has claimed focus (single-chat hosts degrade permissively)', () => {
    showExtensionLoadResults([ok('weather-mcp')], 'chat-a');

    expect(toastService.extensionLoading).toHaveBeenCalledTimes(1);
  });

  it('toasts for non-chat callers, which pass no session id', () => {
    setFocusedChatSession('chat-a');
    showExtensionLoadResults([ok('weather-mcp'), ok('notion-mcp')]);

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
    showExtensionLoadResults([bad('notion-mcp', 'connection refused')], 'chat-a');

    expect(toastService.error).toHaveBeenCalledTimes(1);
    expect(vi.mocked(toastService.error).mock.calls[0][0]).toMatchObject({
      title: 'notion-mcp',
      msg: 'connection refused',
    });
  });

  it('counts only the user extensions — built-ins that loaded are not in the tally', () => {
    setFocusedChatSession('chat-a');
    // 5 user extensions (1 failed) + the built-ins that always come up.
    showExtensionLoadResults(
      [
        ok('developer'),
        ok('memory'),
        ok('autovisualiser'),
        ok('knowledge'), // built-ins: excluded
        ok('weather-mcp'),
        ok('notion-mcp'),
        ok('slack-mcp'),
        ok('github-mcp'),
        bad('failing-mcp'), // user: 5 total, 1 failed
      ],
      'chat-a'
    );

    // "4 of 5", not "8 of 9": the total passed to the toast excludes the loaded
    // built-ins, and the statuses list carries only the user extensions.
    expect(toastService.extensionLoading).toHaveBeenCalledTimes(1);
    const [statuses, total] = vi.mocked(toastService.extensionLoading).mock.calls[0];
    expect(total).toBe(5);
    expect(statuses.map((s) => s.name)).toEqual([
      'weather-mcp',
      'notion-mcp',
      'slack-mcp',
      'github-mcp',
      'failing-mcp',
    ]);
  });

  it('a built-in that FAILED is still surfaced', () => {
    setFocusedChatSession('chat-a');
    showExtensionLoadResults([bad('knowledge', 'index corrupt')], 'chat-a');
    // Only a failed built-in remains -> the lone-failure error toast fires.
    expect(toastService.error).toHaveBeenCalledTimes(1);
    expect(vi.mocked(toastService.error).mock.calls[0][0]).toMatchObject({ title: 'knowledge' });
  });

  it('stays silent when only built-ins loaded', () => {
    setFocusedChatSession('chat-a');
    showExtensionLoadResults([ok('developer'), ok('memory'), ok('knowledge')], 'chat-a');
    expect(toastService.extensionLoading).not.toHaveBeenCalled();
    expect(toastService.error).not.toHaveBeenCalled();
  });
});
