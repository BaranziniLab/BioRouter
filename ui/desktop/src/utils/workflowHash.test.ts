import { describe, it, expect, vi, beforeEach } from 'vitest';

/**
 * WHAT THIS GUARDS, AND WHAT IT CANNOT.
 *
 * It cannot see window lifecycle — jsdom has no windows and no Electron. It
 * cannot tell you that a tab merge leaves the right window alive; the only
 * honest gate for that is a real CGEventPost tear-off + merge against a running
 * app (`scripts/` has the drag driver; see the P-02 notes on the fix commit).
 *
 * What it CAN pin is the thing that actually went wrong: `ipcMain.on` is
 * ADDITIVE, so a second module registering a channel does not replace the
 * owner's handler — both run. `utils/workflowHash.ts` registered a second
 * `close-window` listener that closed `BrowserWindow.getFocusedWindow()` while
 * the real one in main.ts closes `event.sender`. Those are the same window
 * almost always, which is exactly why it hid: the only caller where they differ
 * is the tab merge, which focuses the TARGET immediately before the SOURCE asks
 * to close itself. One message then closed both windows — and the app could be
 * left with none.
 *
 * So the invariant is about OWNERSHIP, not behaviour: this module owns workflow
 * hashes and must never register window-lifecycle IPC. That is checkable here,
 * and a re-added handler fails it on the first run.
 */

const ipcMainOn = vi.fn();
const ipcMainHandle = vi.fn();
const getFocusedWindow = vi.fn();

vi.mock('electron', () => ({
  ipcMain: {
    on: (...args: unknown[]) => ipcMainOn(...args),
    handle: (...args: unknown[]) => ipcMainHandle(...args),
  },
  app: { getPath: () => '/tmp/p02-test-userdata' },
  BrowserWindow: { getFocusedWindow },
}));

/** Channels that decide whether a window lives or dies. */
const WINDOW_LIFECYCLE_CHANNELS = ['close-window', 'close-active-tab', 'reload-app'];

describe('utils/workflowHash — channel ownership', () => {
  beforeEach(async () => {
    ipcMainOn.mockClear();
    ipcMainHandle.mockClear();
    vi.resetModules();
    await import('./workflowHash');
  });

  it('registers its two workflow-hash channels', () => {
    const handled = ipcMainHandle.mock.calls.map((c) => c[0]);
    expect(handled).toContain('has-accepted-workflow-before');
    expect(handled).toContain('record-workflow-hash');
  });

  it('registers NO window-lifecycle channel — main.ts is their sole owner', () => {
    const registered = [
      ...ipcMainOn.mock.calls.map((c) => c[0]),
      ...ipcMainHandle.mock.calls.map((c) => c[0]),
    ];
    for (const channel of WINDOW_LIFECYCLE_CHANNELS) {
      expect(registered).not.toContain(channel);
    }
  });

  it('never reaches for the focused window', () => {
    // The specific defect: acting on `getFocusedWindow()` rather than the
    // window that sent the message. Importing must not even consult it.
    expect(getFocusedWindow).not.toHaveBeenCalled();
  });

  it('registers exactly the channels it owns, and no others', () => {
    // A total assertion rather than a denylist: a NEW stray channel added here
    // fails too, without anyone remembering to extend the list above.
    const registered = [
      ...ipcMainOn.mock.calls.map((c) => c[0]),
      ...ipcMainHandle.mock.calls.map((c) => c[0]),
    ].sort();
    expect(registered).toEqual(['has-accepted-workflow-before', 'record-workflow-hash']);
  });
});
