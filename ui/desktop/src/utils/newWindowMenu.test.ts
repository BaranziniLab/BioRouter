/**
 * File > New Window (Cmd+N) must actually open a window.
 *
 * It did not. The menu item ran `ipcMain.emit('create-chat-window')` with no
 * arguments, and `ipcMain` is a plain EventEmitter, so the listener was invoked
 * with `event === undefined`. Since #78 that listener anchors the new window on
 * `event.sender`, so the bare emit threw a TypeError inside an async listener:
 * an unhandled rejection, no window, and nothing on screen to say why. The dock
 * menu and the titlebar control take a different path and kept working, which
 * is presumably why it went unnoticed.
 *
 * A behavioural test cannot reach this. The failure is in an Electron
 * application menu built in the main process; jsdom has neither `ipcMain` nor a
 * menu, and a mocked EventEmitter would happily accept the bare emit and prove
 * nothing. So the guard is structural, in the style of `startupBlocking.test.ts`
 * next door: it reads the source and pins the two properties that were wrong.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import * as path from 'path';

const MAIN = readFileSync(path.join(__dirname, '..', 'main.ts'), 'utf8');

/**
 * Source with `//` comment lines removed.
 *
 * ⚠ Load-bearing. The fix left a comment naming the very call it removed, so an
 * assertion over raw source would read that prose as the defect and fail on the
 * fix. Only executable lines can answer "what does this menu item DO".
 */
const withoutComments = (src: string) =>
  src
    .split('\n')
    .filter((line) => !line.trim().startsWith('//'))
    .join('\n');

/** The `New Window` application-menu item, from its accelerator to its close. */
function newWindowMenuItem(): string {
  const at = MAIN.indexOf("accelerator: isMac ? 'Cmd+N' : 'Ctrl+N'");
  expect(at, 'the New Window accelerator is gone').toBeGreaterThan(-1);
  const end = MAIN.indexOf('\n        },', at);
  expect(end).toBeGreaterThan(at);
  return MAIN.slice(at, end);
}

/** The body of the `create-chat-window` IPC listener. */
function createChatWindowListener(): string {
  const at = MAIN.indexOf("'create-chat-window',\n    async (event");
  expect(at, 'the create-chat-window listener is gone').toBeGreaterThan(-1);
  return MAIN.slice(at, at + 4000);
}

describe('File > New Window', () => {
  it('calls createNewWindow rather than emitting an IPC message with no event', () => {
    const item = withoutComments(newWindowMenuItem());
    expect(item).toContain('createNewWindow(app)');
    // ⚠ The specific regression. A menu click has no renderer sender to anchor
    // on, so the IPC handler is the wrong door for it whatever the handler does.
    expect(item).not.toContain('ipcMain.emit');
  });

  it('keeps the accelerator, since the accelerator is what the user presses', () => {
    expect(newWindowMenuItem()).toContain("isMac ? 'Cmd+N' : 'Ctrl+N'");
  });

  /**
   * Belt and braces: even with the caller fixed, the handler must not explode
   * if something reaches it without an Electron event. It is a public IPC
   * channel and this already happened once.
   */
  it('anchors on an optional sender, so a caller with no event cannot throw', () => {
    const body = withoutComments(createChatWindowListener());
    // Pin the GUARDED form, not the absence of `event.sender` - the guarded
    // expression necessarily still contains it, so a ban on the substring would
    // reject the fix along with the bug.
    expect(body).toContain('event?.sender ? BrowserWindow.fromWebContents(event.sender) : null');
    // And the unguarded original must be gone: it is the first candidate in a
    // `??` chain, so it ran on every call.
    expect(body).not.toContain('BrowserWindow.fromWebContents(event.sender) ??');
  });
});
