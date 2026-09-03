import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

/**
 * `shell.openPath` hands a path to the operating system, which decides what to
 * run. It sat one click from the artifact panel with no allowlist, no type
 * check and no confirmation, while the `open-external` handler beside it
 * validated its target AND named it in a native dialog, and `read-artifact-file`
 * refused paths `openPath` opened happily — same panel, same file, two answers.
 *
 * Asserted against `main.ts`'s SOURCE, as `utils/embeddedBrowser.test.ts` does
 * for the sibling `open-external` handler: nothing can import `main.ts` under
 * vitest (it is 6k lines of Electron main-process module-level side effects), so
 * the text of the guard is what there is to pin. The containment decision it
 * delegates to is behaviourally covered by `utils/pathContainment.test.ts`.
 */
const main = readFileSync(join(__dirname, 'main.ts'), 'utf8');

/** The body of one `ipcMain` registration, up to the next one. */
function ipcHandlerSource(channel: string): string {
  const start = main.indexOf(`ipcMain.handle('${channel}'`);
  expect(start, `no handler registered for '${channel}'`).toBeGreaterThan(-1);
  const next = main.indexOf('ipcMain.', start + 1);
  return main.slice(start, next === -1 ? undefined : next);
}

/** The body of a top-level `async function`, up to the next top-level `}`. */
function functionSource(name: string): string {
  const start = main.indexOf(`async function ${name}(`);
  expect(start, `no function named ${name}`).toBeGreaterThan(-1);
  const end = main.indexOf('\n}\n', start);
  return main.slice(start, end === -1 ? undefined : end);
}

describe('open-directory-in-explorer refuses what it cannot vouch for', () => {
  const handler = ipcHandlerSource('open-directory-in-explorer');

  it('applies the preview reader’s containment before the OS ever sees the path', () => {
    expect(handler).toContain('isAllowedFilePath(expanded, workingDirForSender(event))');
    expect(
      handler.indexOf('isAllowedFilePath'),
      'the containment check must precede shell.openPath, not follow it'
    ).toBeLessThan(handler.indexOf('shell.openPath'));
    // The refusal has to be a refusal: a check whose failing branch falls
    // through to the open is not a check.
    const refusal = handler.slice(handler.indexOf('isAllowedFilePath'));
    expect(refusal.slice(0, refusal.indexOf('shell.openPath'))).toContain('return false');
  });

  it('reads the working directory from the window, never from the IPC argument', () => {
    // `workingDirForSender` is the main process's own record. The handler must
    // therefore take `event` — the old signature discarded it as `_event`, and
    // a renderer-supplied root would void the boundary entirely.
    expect(handler).toContain("ipcMain.handle('open-directory-in-explorer', async (event,");
    expect(handler).not.toContain('async (_event,');
  });

  it('confirms with the user before handing anything but a plain folder to the OS', () => {
    expect(handler).toContain('await isPlainDirectory(expanded)');
    expect(handler).toContain('await confirmSystemHandlerOpen(event, expanded)');
    expect(
      handler.indexOf('confirmSystemHandlerOpen'),
      'the confirmation must precede shell.openPath'
    ).toBeLessThan(handler.indexOf('shell.openPath'));
    const gate = handler.slice(handler.indexOf('confirmSystemHandlerOpen'));
    expect(gate.slice(0, gate.indexOf('shell.openPath'))).toContain('return false');
  });

  it('uses the same containment call as the artifact preview reader', () => {
    // The finding was an asymmetry, so the parity is what the test asserts.
    expect(ipcHandlerSource('read-artifact-file')).toContain(
      'isAllowedFilePath(resolvedPath, workingDirForSender(event))'
    );
  });
});

describe('the two helpers the handler leans on', () => {
  it('does not mistake a macOS package bundle for a folder', () => {
    const source = functionSource('isPlainDirectory');
    // `/Applications/Anything.app` stats as a directory and `open` LAUNCHES it,
    // so `isDirectory()` alone is not the test.
    expect(source).toContain('stats.isDirectory()');
    expect(source).toContain("path.extname(resolvedPath) === ''");
  });

  it('names the target and defaults to Cancel, as open-external does', () => {
    const source = functionSource('confirmSystemHandlerOpen');
    expect(source).toContain('dialog.showMessageBox');
    expect(source).toContain('${resolvedPath}');
    expect(source).toContain('defaultId: 0');
    expect(source).toContain('cancelId: 0');
    // Button index 1 is "Open"; anything else — including the dialog being
    // dismissed — must read as a refusal.
    expect(source).toContain('return result.response === 1');
  });
});
