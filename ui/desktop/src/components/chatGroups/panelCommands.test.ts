import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  registerPanelAccess,
  resetPanelAccessRegistry,
  describePanel,
  type PanelAccessor,
} from '../artifacts/panelAccessRegistry';
import { runPanelCommand } from './panelCommands';
import type { WorkspaceCommand } from './workspaceCommandRegistry';

const read = (session_id?: string, max_chars?: number): WorkspaceCommand => ({
  type: 'workspace',
  cmd: 'read_panel',
  session_id,
  max_chars,
});
const capture = (session_id?: string): WorkspaceCommand => ({
  type: 'workspace',
  cmd: 'capture_panel',
  session_id,
});

function accessor(overrides: Partial<PanelAccessor> = {}): PanelAccessor {
  return {
    describe: () => ({ open: true, kind: 'file', title: 'notes.md', locator: '/w/notes.md' }),
    readText: async (max) => ({
      kind: 'text',
      title: 'notes.md',
      locator: '/w/notes.md',
      text: 'hello world'.slice(0, max),
      truncated: false,
    }),
    capture: async () => ({ path: '/tmp/capture-panel-abc.png', width: 800, height: 600 }),
    ...overrides,
  };
}

beforeEach(() => resetPanelAccessRegistry());

describe('reading the panel', () => {
  it('returns the content, and a descriptor of what produced it', async () => {
    registerPanelAccess('s1', accessor());
    const result = await runPanelCommand(read('s1'));
    expect(result.ok).toBe(true);
    expect(result.data).toMatchObject({
      content: 'hello world',
      content_kind: 'text',
      locator: '/w/notes.md',
      truncated: false,
      panel: { open: true, kind: 'file', title: 'notes.md' },
    });
  });

  it('clamps the requested size, in both directions', async () => {
    const readText = vi.fn(async (max: number) => ({
      kind: 'text',
      title: 't',
      text: 'x'.repeat(max),
      truncated: true,
    }));
    registerPanelAccess('s1', accessor({ readText }));

    // An unbounded read would hand a whole document to the model on a tool call
    // the model itself chose the size of.
    await runPanelCommand(read('s1', 10_000_000));
    expect(readText).toHaveBeenLastCalledWith(40_000);

    await runPanelCommand(read('s1', 0));
    expect(readText).toHaveBeenLastCalledWith(20_000);

    await runPanelCommand(read('s1', 500));
    expect(readText).toHaveBeenLastCalledWith(500);
  });

  // "There is nothing to read" and "there is nothing there" are different
  // answers, and an agent picks a different next step from each.
  it('says to capture instead when the content has no text', async () => {
    registerPanelAccess(
      's1',
      accessor({
        describe: () => ({ open: true, kind: 'file', title: 'plot.png' }),
        readText: async () => null,
      })
    );
    const result = await runPanelCommand(read('s1'));
    expect(result.ok).toBe(false);
    expect(result.detail).toContain('capture_panel');
  });

  it('distinguishes a closed panel from a chat that is not on screen here', async () => {
    registerPanelAccess('s-closed', accessor({ describe: () => ({ open: false }) }));

    expect((await runPanelCommand(read('s-closed'))).detail).toContain('not open');
    expect((await runPanelCommand(read('s-elsewhere'))).detail).toContain('in this window');
  });

  it('refuses a call with no session', async () => {
    expect((await runPanelCommand(read(undefined))).ok).toBe(false);
  });
});

describe('capturing the panel', () => {
  it('returns a path, never the bytes', async () => {
    registerPanelAccess('s1', accessor());
    const result = await runPanelCommand(capture('s1'));

    expect(result.ok).toBe(true);
    expect(result.data?.screenshot_path).toBe('/tmp/capture-panel-abc.png');
    // The workspace channel caps an inbound frame at 128 KiB and hands stored
    // echoes to the model verbatim, so a PNG must never travel through it.
    expect(JSON.stringify(result)).not.toMatch(/base64|data:image/);
    expect(JSON.stringify(result).length).toBeLessThan(1000);
  });

  it('reports an empty capture as a refusal, not a broken path', async () => {
    // `capturePage` returns an empty image rather than rejecting when the view
    // was hidden and then navigated, so this is a real outcome.
    registerPanelAccess('s1', accessor({ capture: async () => null }));
    const result = await runPanelCommand(capture('s1'));
    expect(result.ok).toBe(false);
    expect(result.data?.screenshot_path).toBeUndefined();
  });
});

describe('the registry itself', () => {
  it('reports a closed panel for an unknown session rather than throwing', () => {
    // `describePanel` runs while building the workspace echo, on every commit.
    // An exception there would take down the channel carrying every other
    // workspace command.
    expect(describePanel('nobody')).toEqual({ open: false });
    expect(describePanel(null)).toEqual({ open: false });
  });

  it('survives an accessor that throws', () => {
    registerPanelAccess('s1', {
      describe: () => {
        throw new Error('boom');
      },
      readText: async () => null,
      capture: async () => null,
    });
    expect(describePanel('s1')).toEqual({ open: false });
  });

  it('does not let a stale unregister clear a live re-registration', () => {
    // A remount registers the new accessor before the old effect's cleanup
    // runs; an unconditional delete there would leave the panel unreachable.
    const first = accessor();
    const disposeFirst = registerPanelAccess('s1', first);
    const second = accessor({ describe: () => ({ open: true, title: 'second' }) });
    registerPanelAccess('s1', second);

    disposeFirst();

    expect(describePanel('s1')).toMatchObject({ title: 'second' });
  });
});
