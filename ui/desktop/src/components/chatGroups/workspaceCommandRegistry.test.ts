import { describe, expect, it, beforeEach } from 'vitest';
import {
  registerWorkspaceCommands,
  applyWorkspaceCommand,
  drainPendingWorkspaceCommands,
  hasPendingWorkspaceCommands,
  resetWorkspaceCommandRegistry,
  type WorkspaceCommand,
  type WorkspaceCommandResult,
} from './workspaceCommandRegistry';

// Every handler in this file is synchronous, which is what these cases are
// about. `applyWorkspaceCommand` now returns a union because a *capture* cannot
// be sync; narrowing here keeps each assertion testing what it always tested.
const applySync = (cmd: Parameters<typeof applyWorkspaceCommand>[0]) =>
  applyWorkspaceCommand(cmd) as WorkspaceCommandResult;

const openTab: WorkspaceCommand = {
  type: 'workspace',
  cmd: 'open_tab',
  session_id: 's1',
  placement: 'tab',
  focus: false,
};

const activateTab: WorkspaceCommand = {
  type: 'workspace',
  cmd: 'activate_tab',
  session_id: 's2',
};

const notify: WorkspaceCommand = {
  type: 'workspace',
  cmd: 'notify',
  level: 'info',
  message: 'done',
};

describe('workspaceCommandRegistry — the daemon→tabs hand-off', () => {
  beforeEach(() => resetWorkspaceCommandRegistry());

  it('dispatches to a live handler and reports its result', () => {
    const seen: WorkspaceCommand[] = [];
    registerWorkspaceCommands((cmd) => {
      seen.push(cmd);
      return { ok: true, detail: 'opened' };
    });
    const result = applyWorkspaceCommand(openTab);
    expect(result).toEqual({ ok: true, detail: 'opened' });
    expect(seen).toHaveLength(1);
  });

  it('queues commands with no provider mounted, for the mounting provider to drain', () => {
    // Both edges of the peek, not just the true one: a hasPending() stuck at
    // true is the dangerous mutation, because the obvious Task 26+ move is to
    // gate the empty-/pair redirect on it beside hasPendingNewTab() — and a
    // permanently-true peek would suppress the issue #38 redirect for good,
    // with a green suite.
    expect(hasPendingWorkspaceCommands()).toBe(false);
    const result = applySync(openTab);
    expect(result.ok).toBe(false);
    expect(hasPendingWorkspaceCommands()).toBe(true);
    const drained = drainPendingWorkspaceCommands();
    expect(drained).toEqual([openTab]);
    // Consume-once: StrictMode double-mount must not double-apply (same
    // rationale as newTabRegistry.consumePendingNewTab).
    expect(drainPendingWorkspaceCommands()).toEqual([]);
    expect(hasPendingWorkspaceCommands()).toBe(false);
  });

  it('drains a multi-command queue in arrival order', () => {
    // Replay order decides which tab ends up focused, so the queue is a FIFO
    // and not a bag: an unshift- or Set-backed drain would activate s2 before
    // the open_tab that creates it.
    applyWorkspaceCommand(openTab);
    applyWorkspaceCommand(activateTab);
    applyWorkspaceCommand(notify);
    expect(drainPendingWorkspaceCommands()).toEqual([openTab, activateTab, notify]);
  });

  it('reset clears queued commands, not just the handler', () => {
    // The reset exists to stop the singleton leaking across cases; if it only
    // dropped the handler, a queued frame from one case would be drained by
    // the next one's provider (Tasks 26/28/30 all mount against this).
    applyWorkspaceCommand(openTab);
    resetWorkspaceCommandRegistry();
    expect(hasPendingWorkspaceCommands()).toBe(false);
    expect(drainPendingWorkspaceCommands()).toEqual([]);
  });

  it('disposer only clears its own handler (mount-B-then-dispose-A order)', () => {
    const disposeA = registerWorkspaceCommands(() => ({ ok: true }));
    registerWorkspaceCommands(() => ({ ok: true, detail: 'B' }));
    disposeA();
    expect(applySync(openTab).detail).toBe('B');
  });
});
