import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, act, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { ChatGroupsProvider, useChatGroups } from './ChatGroupsContext';
import {
  applyWorkspaceCommand,
  resetWorkspaceCommandRegistry,
  type WorkspaceCommand,
  type WorkspaceCommandResult,
} from '../components/chatGroups/workspaceCommandRegistry';
import { leafGroupIds } from '../components/chatGroups/chatGroupsTypes';

const mocks = vi.hoisted(() => ({
  observeSession: vi.fn(),
  success: vi.fn(),
  createChatWindow: vi.fn(),
}));

// The provider imports `useRunningChats` from this module and the executor calls
// `defaultChatStreamRegistry.getController(...).observeSession()`. Mocking the
// module replaces BOTH, so the mock must supply both — `tabs.test.tsx` supplies
// only `useRunningChats`, which is safe there only because it never dispatches a
// workspace command.
vi.mock('../hooks/chatStreamStore', () => ({
  useRunningChats: () => [],
  defaultChatStreamRegistry: {
    getController: () => ({ observeSession: mocks.observeSession }),
  },
}));
vi.mock('../utils/sessionNameSync', () => ({ subscribeSessionNameChanges: () => () => undefined }));
vi.mock('../toasts', () => ({ toastService: { success: mocks.success, error: vi.fn() } }));

function Probe() {
  const ctx = useChatGroups();
  return (
    <div>
      <span data-testid="sessions">
        {ctx
          ? leafGroupIds(ctx.state.layout)
              .flatMap((id) => ctx.state.groups[id].tabs)
              .map((t) => t.sessionId)
              .join(',')
          : ''}
      </span>
      <span data-testid="badges">
        {JSON.stringify(
          Object.fromEntries(
            Object.entries(ctx?.tabAnnotations ?? {}).map(([k, v]) => [k, v.badge])
          )
        )}
      </span>
    </div>
  );
}

const mount = () =>
  render(
    <MemoryRouter initialEntries={['/pair']}>
      <ChatGroupsProvider>
        <Probe />
      </ChatGroupsProvider>
    </MemoryRouter>
  );

const openTab = (session_id: string): WorkspaceCommand => ({
  type: 'workspace',
  cmd: 'open_tab',
  session_id,
  placement: 'tab',
  focus: false,
});

describe('ChatGroupsProvider — the workspace command executor', () => {
  beforeEach(() => {
    localStorage.clear();
    vi.clearAllMocks();
    resetWorkspaceCommandRegistry();
    Object.assign(window, { electron: { createChatWindow: mocks.createChatWindow } });
  });

  it('registers a handler on mount, so a daemon open_tab really opens a tab', async () => {
    mount();
    // Nothing is queued: a live provider claimed the registry.
    let result: WorkspaceCommandResult | undefined;
    act(() => {
      result = applyWorkspaceCommand(openTab('s-daemon'));
    });
    expect(result).toEqual(expect.objectContaining({ ok: true }));
    await waitFor(() => expect(screen.getByTestId('sessions').textContent).toContain('s-daemon'));
    // …and the tab is attached to the observer stream, because this window is
    // not the one driving that session (§4.3).
    expect(mocks.observeSession).toHaveBeenCalled();
  });

  it('drains commands that arrived before any provider was mounted', async () => {
    // The Settings-page case: a frame lands with no chat surface up.
    const queued = applyWorkspaceCommand(openTab('s-early'));
    expect(queued.ok).toBe(false);
    mount();
    await waitFor(() => expect(screen.getByTestId('sessions').textContent).toContain('s-early'));
  });

  it('applies annotate_tab to the context value the strip reads', async () => {
    mount();
    act(() => {
      applyWorkspaceCommand({
        type: 'workspace',
        cmd: 'annotate_tab',
        session_id: 's-child',
        badge: 'subagent',
        parent_session_id: 's-parent',
      });
    });
    // This is the assertion that catches the `useMemo` dependency-array omission:
    // with `tabAnnotations` missing from the deps, the state updates and the
    // context value does not, so the badge never reaches a consumer and every
    // other test in Tasks 26 and 37 stays green.
    await waitFor(() => expect(screen.getByTestId('badges').textContent).toContain('subagent'));
  });

  it('relays open_window to the main process instead of opening a tab', async () => {
    mount();
    act(() => {
      applyWorkspaceCommand({ type: 'workspace', cmd: 'open_window', session_id: 's-win' });
    });
    await waitFor(() => expect(mocks.createChatWindow).toHaveBeenCalled());
    // The session id goes in the resume-session position (4th arg).
    expect(mocks.createChatWindow.mock.calls[0][3]).toBe('s-win');
    expect(screen.getByTestId('sessions').textContent).not.toContain('s-win');
  });

  it('surfaces notify frames as a toast and touches no tabs', async () => {
    mount();
    const before = screen.getByTestId('sessions').textContent;
    act(() => {
      applyWorkspaceCommand({
        type: 'workspace',
        cmd: 'notify',
        session_id: 's-x',
        level: 'info',
        message: 'An agent wants to show you something',
      });
    });
    await waitFor(() => expect(mocks.success).toHaveBeenCalled());
    expect(screen.getByTestId('sessions').textContent).toBe(before);
  });
});
