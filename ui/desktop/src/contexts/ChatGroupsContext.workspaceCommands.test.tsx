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
  info: vi.fn(),
  warning: vi.fn(),
  error: vi.fn(),
  success: vi.fn(),
  createChatWindow: vi.fn(),
  /**
   * ONE array, for the life of the file — never `() => []`.
   *
   * The real `useRunningChats` is a `useSyncExternalStore` snapshot
   * (`chatStreamStore.tsx`: `getRunningSnapshot = () => this.lastRunningSnapshot`),
   * so its identity is stable across renders and the provider's context `useMemo`
   * only recomputes when one of its declared deps actually changes. A mock that
   * returns a fresh `[]` every call makes `runningSessionIds` change on EVERY
   * render, which recomputes the memo unconditionally and renders its dependency
   * array inert — including the `tabAnnotations` entry the badge case below
   * exists to protect. Measured: with `() => []`, deleting `tabAnnotations` from
   * the deps left all five cases green.
   */
  runningChats: [] as { sessionId: string; completedAt?: number }[],
}));

// The provider imports `useRunningChats` from this module and the executor calls
// `defaultChatStreamRegistry.getController(...).observeSession()`. Mocking the
// module replaces BOTH, so the mock must supply both — `tabs.test.tsx` supplies
// only `useRunningChats`, which is safe there only because it never dispatches a
// workspace command.
vi.mock('../hooks/chatStreamStore', () => ({
  useRunningChats: () => mocks.runningChats,
  defaultChatStreamRegistry: {
    getController: () => ({ observeSession: mocks.observeSession }),
  },
}));
vi.mock('../utils/sessionNameSync', () => ({ subscribeSessionNameChanges: () => () => undefined }));
vi.mock('../toasts', () => ({
  toastInfo: mocks.info,
  toastWarning: mocks.warning,
  toastError: mocks.error,
  toastService: { success: mocks.success, error: vi.fn() },
}));

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
      {/* Panes, not just tabs: `sessions` flattens the layout tree, so it reads
          the same whether two sessions share one pane or sit in two. Anything
          about splitting has to assert on THIS. */}
      <span data-testid="panes">
        {ctx
          ? leafGroupIds(ctx.state.layout)
              .map((id) => ctx.state.groups[id].tabs.map((t) => t.sessionId).join('+'))
              .join(' | ')
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

  it('splits a new session into its own pane, from a frame delivered as the socket delivers one', async () => {
    // NOT wrapped in `act()`, and that is the entire point. `ws.onmessage` hands
    // the executor a frame from a MACROTASK; React then commits on the
    // Scheduler's own macrotask, so anything the executor defers to
    // `queueMicrotask` runs BEFORE the commit it is waiting for. The first
    // implementation deferred the split's follow-up move exactly that way and
    // re-read `stateRef`, which React had not written yet: the session lookup
    // missed, the move was silently dropped, and the daemon was answered
    // `ok: true, detail: 'opened in split'` for a window showing ONE pane.
    //
    // `act()` masks it — it flushes the reducer synchronously before the
    // microtask drains — so the test harness has to stop being kinder than
    // production for this one case.
    mount();
    act(() => {
      applyWorkspaceCommand(openTab('s-a'));
    });
    await waitFor(() => expect(screen.getByTestId('panes').textContent).toBe('s-a'));

    let result: WorkspaceCommandResult | undefined;
    await act(async () => {
      await new Promise<void>((resolve) =>
        setTimeout(() => {
          result = applyWorkspaceCommand({
            type: 'workspace',
            cmd: 'open_tab',
            session_id: 's-split',
            placement: 'split',
            focus: true,
          });
          resolve();
        }, 0)
      );
    });

    await waitFor(() => expect(screen.getByTestId('panes').textContent).toBe('s-a | s-split'));
    // The answer the daemon gets has to describe the window the user is looking
    // at — a `detail` of 'opened in split' over a single pane is worse than a
    // refusal, because nothing downstream can tell it is wrong.
    expect(result).toEqual({ ok: true, detail: 'opened in split' });

    // And it settles there: a re-introduced post-commit follow-up move would
    // land here, on state that already split, and split again.
    await act(async () => {
      await new Promise<void>((resolve) => setTimeout(resolve, 0));
    });
    expect(screen.getByTestId('panes').textContent).toBe('s-a | s-split');
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
    //
    // It only catches it because `useRunningChats` is mocked with a STABLE array
    // (see `mocks.runningChats`). `annotate_tab` dispatches no reducer action, so
    // `state` and `activeSessionId` are unchanged here and the memo's only reason
    // to recompute is `tabAnnotations` — unless a fresh-identity mock gives it
    // another one, at which point this assertion silently stops meaning anything.
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
    await waitFor(() =>
      expect(mocks.info).toHaveBeenCalledWith(
        expect.objectContaining({ msg: 'An agent wants to show you something' })
      )
    );
    expect(screen.getByTestId('sessions').textContent).toBe(before);
  });

  it('routes a notify by its level, so a failure is not reported as a success', async () => {
    // The daemon stamps every notify with a level (`workspace_extension.rs`
    // sends "info" today; §5's autonomous-mode visibility is the reason the
    // frame exists at all). Collapsing them onto one channel is not cosmetic —
    // a failure rendered with a green check mark is a lie the user acts on, and
    // an informational cross-session notice dressed as a confirmation reads as
    // "your thing worked" when nothing of the user's did.
    mount();
    const channels = [mocks.info, mocks.warning, mocks.error];
    for (const [level, expected] of [
      ['info', mocks.info],
      // No level at all is a notice, not a success.
      [undefined, mocks.info],
      ['warn', mocks.warning],
      ['warning', mocks.warning],
      ['error', mocks.error],
    ] as const) {
      vi.clearAllMocks();
      act(() => {
        applyWorkspaceCommand({
          type: 'workspace',
          cmd: 'notify',
          level,
          message: `at ${String(level)}`,
        });
      });
      await waitFor(() =>
        expect(expected).toHaveBeenCalledWith(
          expect.objectContaining({ title: 'Workspace', msg: `at ${String(level)}` })
        )
      );
      for (const other of channels) {
        if (other !== expected) expect(other).not.toHaveBeenCalled();
      }
      // And never the success channel: nothing about a workspace notice is a
      // confirmation of something the user asked for.
      expect(mocks.success).not.toHaveBeenCalled();
    }
  });
});
