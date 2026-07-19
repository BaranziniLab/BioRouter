import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { canonicalizeForContainment, isPathContained } from './pathContainment';

/**
 * Gate for the live false-denial: a session tool call wrote /tmp/qa-r1b/hi.txt
 * and the preview panel refused it with "Access denied: path '/tmp/qa-r1b/hi.txt'
 * is outside allowed directories" — because on macOS /tmp is a symlink to
 * /private/tmp and the old check was a plain string-prefix with no symlink
 * resolution. These tests exercise real symlinks in a real tempdir.
 */
describe('isPathContained', () => {
  let real: string; // a real dir, canonical
  let alias: string; // a symlink pointing at it

  beforeAll(() => {
    real = fs.mkdtempSync(path.join(fs.realpathSync(os.tmpdir()), 'pc-real-'));
    alias = path.join(fs.realpathSync(os.tmpdir()), `pc-alias-${process.pid}`);
    fs.symlinkSync(real, alias);
    fs.writeFileSync(path.join(real, 'hi.txt'), 'hello');
  });

  afterAll(() => {
    fs.unlinkSync(alias);
    fs.rmSync(real, { recursive: true, force: true });
  });

  it('admits a file addressed through a symlinked alias of an allowed root (the /tmp case)', () => {
    // Root registered in canonical form, file addressed via the alias.
    expect(isPathContained(path.join(alias, 'hi.txt'), [real])).toBe(true);
  });

  it('admits a file addressed canonically when the ROOT is registered via its alias', () => {
    // Root registered as the alias (like allowing '/tmp'), file canonical.
    expect(isPathContained(path.join(real, 'hi.txt'), [alias])).toBe(true);
  });

  it('admits a not-yet-existing write target under an allowed root', () => {
    expect(isPathContained(path.join(alias, 'new-dir', 'out.csv'), [real])).toBe(true);
  });

  it('denies a path outside every root', () => {
    expect(isPathContained('/etc/passwd', [real])).toBe(false);
  });

  it('denies lexical traversal escaping a root', () => {
    // canonicalize collapses the ../.. of the deepest existing ancestor.
    expect(isPathContained(path.join(real, '..', '..', 'etc', 'passwd'), [real])).toBe(false);
  });

  it('denies a sibling whose name merely extends the root string', () => {
    // /tmp/pc-real-XXXXevil must not match root /tmp/pc-real-XXXX.
    expect(isPathContained(`${real}evil/secret.txt`, [real])).toBe(false);
  });

  it('follows a symlink INSIDE a root that points outside it (false-admit guard)', () => {
    const outside = fs.mkdtempSync(path.join(fs.realpathSync(os.tmpdir()), 'pc-outside-'));
    const inner = path.join(real, 'escape');
    fs.symlinkSync(outside, inner);
    try {
      expect(isPathContained(path.join(inner, 'x.txt'), [real])).toBe(false);
    } finally {
      fs.unlinkSync(inner);
      fs.rmSync(outside, { recursive: true, force: true });
    }
  });

  it('canonicalizeForContainment resolves an existing symlink and preserves a missing tail', () => {
    expect(canonicalizeForContainment(alias)).toBe(real);
    expect(canonicalizeForContainment(path.join(alias, 'nope', 'deep.txt'))).toBe(
      path.join(real, 'nope', 'deep.txt')
    );
  });
});
