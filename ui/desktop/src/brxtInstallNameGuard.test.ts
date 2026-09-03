import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

/**
 * F-13: the desktop is the THIRD installer of a `.brxt` bundle, and the name it
 * is handed is the bundle's own `manifest.name` — so the archive names the
 * directory it is written into.
 *
 * A bundle declaring `"name": "../../evil"` therefore escapes the extensions
 * root unless every installer refuses it. The Rust transaction refuses it
 * (`extension_install::brxt::validate_extension_name`) and so does
 * `routes::shell`; this asserts the desktop's copy has not drifted or been
 * deleted.
 *
 * ⚠ Asserted against `main.ts` as SOURCE TEXT, following
 * `workspaceChannelCsp.test.ts`. The handler is an Electron main-process IPC
 * registration: importing it would start the app, and there is no seam to call
 * the check through. A source assertion is weaker than an executed one and is
 * chosen deliberately over no assertion at all — a validation that is silently
 * removed is exactly the change this is here to notice.
 */
const MAIN_TS = readFileSync(join(__dirname, 'main.ts'), 'utf8');

/** The body of one `ipcMain.handle('<channel>', …)` registration. */
function handlerBody(channel: string): string {
  const start = MAIN_TS.indexOf(`'${channel}',`);
  expect(start, `${channel} handler not found — did the channel get renamed?`).toBeGreaterThan(-1);
  // Enough of the body to cover the guard, without depending on brace matching.
  return MAIN_TS.slice(start, start + 4000);
}

describe('brxt:install rejects a bundle that names its own directory unsafely', () => {
  const body = handlerBody('brxt:install');

  it('refuses an empty name, a traversal, and a separator', () => {
    // Catches deleting the guard outright — the shape that lets
    // `"name": "../../evil"` out of the extensions root.
    expect(body).toMatch(/!extensionName/);
    expect(body).toMatch(/\/\[\/\\\\\]\/\.test\(extensionName\)/);
    expect(body).toMatch(/extensionName === '\.\.'/);
    expect(body).toMatch(/extensionName === '\.'/);
  });

  it('checks the name before it is joined onto a path', () => {
    // Catches a guard that is present but runs too late: validating after the
    // join has already resolved the traversal is validation of the wrong value.
    const guard = body.search(/extensionName === '\.\.'/);
    const join = body.search(/path\.join\(/);
    expect(guard).toBeGreaterThan(-1);
    if (join > -1) {
      expect(guard).toBeLessThan(join);
    }
  });

  it('also confines the resolved directory to the extensions root', () => {
    // Defence in depth: the name check is a denylist of shapes, and this is the
    // property that actually matters. Catches removing the containment check on
    // the grounds that the name check "already covers it".
    expect(body).toMatch(/startsWith\(/);
  });

  it('is mirrored by the uninstall handler on the same string', () => {
    // Catches fixing one direction only: a name that cannot be installed but
    // can be uninstalled is a deletion primitive pointed anywhere on disk.
    const uninstall = handlerBody('brxt:uninstall');
    expect(uninstall).toMatch(/extensionName === '\.\.'/);
    expect(uninstall).toMatch(/\/\[\/\\\\\]\/\.test\(extensionName\)/);
  });
});
