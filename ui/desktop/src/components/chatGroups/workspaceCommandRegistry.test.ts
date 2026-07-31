import { describe, expect, it, beforeEach } from 'vitest';
import {
  registerWorkspaceCommands,
  applyWorkspaceCommand,
  drainPendingWorkspaceCommands,
  hasPendingWorkspaceCommands,
  resetWorkspaceCommandRegistry,
  type WorkspaceCommand,
} from './workspaceCommandRegistry';

const openTab: WorkspaceCommand = {
  type: 'workspace',
  cmd: 'open_tab',
  session_id: 's1',
  placement: 'tab',
  focus: false,
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
    const result = applyWorkspaceCommand(openTab);
    expect(result.ok).toBe(false);
    expect(hasPendingWorkspaceCommands()).toBe(true);
    const drained = drainPendingWorkspaceCommands();
    expect(drained).toEqual([openTab]);
    // Consume-once: StrictMode double-mount must not double-apply (same
    // rationale as newTabRegistry.consumePendingNewTab).
    expect(drainPendingWorkspaceCommands()).toEqual([]);
  });

  it('disposer only clears its own handler (mount-B-then-dispose-A order)', () => {
    const disposeA = registerWorkspaceCommands(() => ({ ok: true }));
    registerWorkspaceCommands(() => ({ ok: true, detail: 'B' }));
    disposeA();
    expect(applyWorkspaceCommand(openTab).detail).toBe('B');
  });
});
