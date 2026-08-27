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
import { MAX_GROUPS } from '../components/chatGroups/chatGroupsLayout';

const mocks = vi.hoisted(() => ({
  observeSession: vi.fn(),
  releaseOwnership: vi.fn(),
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

// The provider imports `useRunningChats` from this module, the executor calls
// `defaultChatStreamRegistry.getController(...).observeSession()`, and closing a
// tab calls `peekController(...)?.releaseOwnership()`. Mocking the module replaces
// all three, so the mock must supply all three.
vi.mock('../hooks/chatStreamStore', () => ({
  useRunningChats: () => mocks.runningChats,
  defaultChatStreamRegistry: {
    getController: () => ({ observeSession: mocks.observeSession }),
    // `peek`, not `get`: the detach path must never CREATE a controller for a
    // session this window has no tab for — that is the leak `getController` is
    // guarded against on the attach side too.
    peekController: () => ({ releaseOwnership: mocks.releaseOwnership }),
  },
}));
vi.mock('../utils/sessionNameSync', () => ({ subscribeSessionNameChanges: () => () => undefined }));
vi.mock('../toasts', () => ({
  toastInfo: mocks.info,
  toastWarning: mocks.warning,
  toastError: mocks.error,
  toastService: { success: mocks.success, error: vi.fn() },
}));

/**
 * The mounted provider's context value, so a case can drive the reducer the way
 * a user would (drag a tab into a new pane) instead of only through workspace
 * frames. The MAX_GROUPS fixture below needs it: nothing the daemon can send
 * builds six panes, and a refusal that cannot be reached is a refusal that
 * cannot be tested.
 */
let captured: ReturnType<typeof useChatGroups> = null;

function Probe() {
  const ctx = useChatGroups();
  captured = ctx;
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
      result = applyWorkspaceCommand(openTab('s-daemon')) as WorkspaceCommandResult;
    });
    expect(result).toEqual(expect.objectContaining({ ok: true }));
    await waitFor(() => expect(screen.getByTestId('sessions').textContent).toContain('s-daemon'));
    // …and the tab is attached to the observer stream, because this window is
    // not the one driving that session (§4.3).
    expect(mocks.observeSession).toHaveBeenCalled();
  });

  // BR-71 §3c: the whole reason the `observe` frame exists. A tab the USER
  // opened has no observer stream — nothing attaches one, because an ordinary
  // tab is driven by its own `/reply` — so a conversation written into from
  // elsewhere sat stale until reload. The daemon sends this frame after the row
  // is durable; the observer's first frame is a full snapshot from the store,
  // so the injected message renders whether or not the bus publish beat it.
  it('an observe frame attaches the live feed to a tab this window already has', async () => {
    mount();
    act(() => {
      applyWorkspaceCommand(openTab('s-target'));
    });
    await waitFor(() => expect(screen.getByTestId('sessions').textContent).toContain('s-target'));
    mocks.observeSession.mockClear();

    let result: WorkspaceCommandResult | undefined;
    act(() => {
      result = applyWorkspaceCommand({
        type: 'workspace',
        cmd: 'observe',
        session_id: 's-target',
      }) as WorkspaceCommandResult;
    });
    expect(result).toEqual(expect.objectContaining({ ok: true }));
    expect(mocks.observeSession).toHaveBeenCalled();
  });

  // The control, and the reason the executor re-checks `findTabBySession`
  // itself rather than trusting `plan.result.ok`: `getController` is a
  // create-AND-RETAIN, so calling it for a session with no tab both starts a
  // stream for a chat that is nowhere on screen and leaks a controller, once
  // per frame, on input the daemon fully controls.
  it('an observe frame for a session with no tab here attaches nothing', async () => {
    mount();
    act(() => {
      applyWorkspaceCommand(openTab('s-mine'));
    });
    await waitFor(() => expect(screen.getByTestId('sessions').textContent).toContain('s-mine'));
    mocks.observeSession.mockClear();

    act(() => {
      applyWorkspaceCommand({
        type: 'workspace',
        cmd: 'observe',
        session_id: 's-in-another-window',
      });
    });
    expect(mocks.observeSession).not.toHaveBeenCalled();
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
      applyWorkspaceCommand(openTab('s-a')) as WorkspaceCommandResult;
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
          }) as WorkspaceCommandResult;
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
    const queued = applyWorkspaceCommand(openTab('s-early')) as WorkspaceCommandResult;
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
      }) as WorkspaceCommandResult;
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

  it('attaches the observer stream only when the frame really produced a tab', async () => {
    // `getController` is not a lookup — it CREATES a ChatStreamController and
    // retains it in a map keyed by session id, for the life of the renderer.
    // Calling it for a session this frame did not open therefore both starts a
    // stream for a chat that is nowhere on screen and leaks the controller, once
    // per frame, on input the daemon fully controls.
    mount();

    // (a) annotate_tab for a session with no tab: a badge for a tab that does
    // not exist observes nothing.
    act(() => {
      applyWorkspaceCommand({
        type: 'workspace',
        cmd: 'annotate_tab',
        session_id: 's-nowhere',
        badge: 'subagent',
      }) as WorkspaceCommandResult;
    });
    await waitFor(() => expect(screen.getByTestId('badges').textContent).toContain('subagent'));
    expect(mocks.observeSession).not.toHaveBeenCalled();

    // (b) a refused open_tab. Six panes is the ceiling, and only a real drag can
    // build them — each move splits one tab off the first pane into a new one.
    act(() => {
      for (let i = 0; i < MAX_GROUPS; i++)
        applyWorkspaceCommand(openTab(`s-${i}`)) as WorkspaceCommandResult;
    });
    await waitFor(() => expect(screen.getByTestId('sessions').textContent).toContain('s-5'));
    for (let i = 1; i < MAX_GROUPS; i++) {
      act(() => {
        const state = captured!.state;
        const leaves = leafGroupIds(state.layout);
        captured!.dispatch({
          type: 'moveTabToGroup',
          tabId: state.groups[leaves[0]].tabs[0].tabId,
          targetGroupId: leaves[leaves.length - 1],
          zone: 'right',
        });
      });
    }
    await waitFor(() => expect(leafGroupIds(captured!.state.layout)).toHaveLength(MAX_GROUPS));

    mocks.observeSession.mockClear();
    let refused: WorkspaceCommandResult | undefined;
    act(() => {
      refused = applyWorkspaceCommand({
        type: 'workspace',
        cmd: 'open_tab',
        session_id: 's-refused',
        placement: 'split',
        focus: true,
      }) as WorkspaceCommandResult;
    });
    expect(refused?.ok).toBe(false);
    expect(screen.getByTestId('sessions').textContent).not.toContain('s-refused');
    expect(mocks.observeSession).not.toHaveBeenCalled();
  });

  it('forgets a closed tab annotation, but keeps one whose tab has not arrived yet', async () => {
    // `tabAnnotations` is keyed by session id and written from daemon frames, so
    // with no prune it only ever grows for the life of the window. Closing the
    // tab is the moment the entry stops being able to mean anything.
    mount();
    act(() => {
      applyWorkspaceCommand(openTab('s-badged')) as WorkspaceCommandResult;
    });
    await waitFor(() => expect(screen.getByTestId('sessions').textContent).toContain('s-badged'));
    act(() => {
      applyWorkspaceCommand({
        type: 'workspace',
        cmd: 'annotate_tab',
        session_id: 's-badged',
        badge: 'subagent',
      }) as WorkspaceCommandResult;
    });
    await waitFor(() => expect(screen.getByTestId('badges').textContent).toContain('s-badged'));

    // The other half, and the reason the prune is scoped to sessions that HAD a
    // tab rather than to "any session with no tab right now": nothing orders the
    // daemon's frames, so an annotation can legitimately land before its tab and
    // must survive every commit in between.
    act(() => {
      applyWorkspaceCommand({
        type: 'workspace',
        cmd: 'annotate_tab',
        session_id: 's-pending',
        badge: 'subagent',
      }) as WorkspaceCommandResult;
    });
    await waitFor(() => expect(screen.getByTestId('badges').textContent).toContain('s-pending'));

    act(() => {
      applyWorkspaceCommand({
        type: 'workspace',
        cmd: 'close_tab',
        session_id: 's-badged',
      }) as WorkspaceCommandResult;
    });
    await waitFor(() => expect(screen.getByTestId('badges').textContent).not.toContain('s-badged'));
    // Unrelated commits keep happening; the pending annotation is still there.
    act(() => {
      applyWorkspaceCommand(openTab('s-unrelated')) as WorkspaceCommandResult;
    });
    await waitFor(() =>
      expect(screen.getByTestId('sessions').textContent).toContain('s-unrelated')
    );
    expect(screen.getByTestId('badges').textContent).toContain('s-pending');
  });

  it('detaches the observer stream when the tab it was attached for closes', async () => {
    // The other end of the attach above. `observeSession` owns a reconnect loop
    // that runs until something detaches it, and `getController` retains its
    // controller for the life of the renderer — so with nothing calling
    // `releaseOwnership`, closing a daemon-opened tab leaves an SSE subscription
    // reconnecting forever for a chat that is nowhere on screen, one per tab the
    // daemon ever opened.
    mount();
    act(() => {
      applyWorkspaceCommand(openTab('s-observed')) as WorkspaceCommandResult;
    });
    await waitFor(() => expect(screen.getByTestId('sessions').textContent).toContain('s-observed'));
    expect(mocks.observeSession).toHaveBeenCalled();
    expect(mocks.releaseOwnership).not.toHaveBeenCalled();

    act(() => {
      applyWorkspaceCommand({
        type: 'workspace',
        cmd: 'close_tab',
        session_id: 's-observed',
      }) as WorkspaceCommandResult;
    });
    await waitFor(() =>
      expect(screen.getByTestId('sessions').textContent).not.toContain('s-observed')
    );
    expect(mocks.releaseOwnership).toHaveBeenCalledTimes(1);
  });

  it('relays open_window to the main process instead of opening a tab', async () => {
    mount();
    act(() => {
      applyWorkspaceCommand({
        type: 'workspace',
        cmd: 'open_window',
        session_id: 's-win',
      }) as WorkspaceCommandResult;
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
      }) as WorkspaceCommandResult;
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
        }) as WorkspaceCommandResult;
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
