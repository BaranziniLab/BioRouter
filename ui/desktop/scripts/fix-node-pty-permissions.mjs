#!/usr/bin/env node
/**
 * Restore the executable bit on node-pty's `spawn-helper`.
 *
 * WHY THIS EXISTS
 * ---------------
 * On macOS, node-pty does not fork/exec the shell directly. `PtyFork` in
 * `src/unix/pty.cc` builds an argv whose argv[0] is a small helper binary,
 * `spawn-helper`, and `posix_spawn()`s *that*; the helper then sets the
 * controlling TTY and execs the real shell. If the helper cannot be executed,
 * `posix_spawn` returns EACCES and node-pty throws the opaque native error:
 *
 *     posix_spawnp failed.
 *
 * The npm tarball for node-pty ships `prebuilds/<platform>-<arch>/spawn-helper`
 * with mode 0644 — no executable bit. Verify with:
 *
 *     npm pack node-pty@1.1.0 && tar -tvf node-pty-1.1.0.tgz | grep spawn-helper
 *     -rw-r--r-- ... package/prebuilds/darwin-arm64/spawn-helper
 *
 * Nothing in the install ever repairs it: node-pty's own `install` script is
 * `node scripts/prebuild.js || node-gyp rebuild`, and `prebuild.js` exits 0 as
 * soon as a prebuild directory exists — so on macOS, where prebuilds always
 * ship, node-gyp (which would compile *and* chmod the helper) never runs.
 *
 * The result is that every fresh `npm install` produces a node-pty whose every
 * spawn fails, and the failure surfaces to the user as "Could not start
 * terminal: posix_spawnp failed." This bites often here because the release
 * pipeline ends with `rm -rf node_modules && npm install` to restore a
 * mac-native tree after the Linux/Windows Docker builds.
 *
 * This runs on `postinstall` (repairs the dev tree) and again from `package` /
 * `make` (so a build cannot ship a broken helper even when node_modules was
 * installed with --ignore-scripts or restored from an archive). It is
 * idempotent and never fails the install: a missing node-pty just means the
 * terminal falls back to pipes, which is not worth aborting an install over.
 */

import { chmodSync, existsSync, readdirSync, statSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const packageRoot = dirname(dirname(fileURLToPath(import.meta.url)));
const nodePtyRoot = join(packageRoot, 'node_modules', 'node-pty');

/** Every directory node-pty's `loadNativeModule` will look in, in its order. */
function helperCandidates(root) {
  const dirs = [join(root, 'build', 'Release'), join(root, 'build', 'Debug')];
  const prebuilds = join(root, 'prebuilds');
  if (existsSync(prebuilds)) {
    for (const entry of readdirSync(prebuilds)) {
      dirs.push(join(prebuilds, entry));
    }
  }
  // Windows uses ConPTY/winpty and has no spawn-helper, so those prebuild
  // directories simply contribute no candidate.
  return dirs.map((dir) => join(dir, 'spawn-helper')).filter((file) => existsSync(file));
}

function main() {
  if (!existsSync(nodePtyRoot)) {
    // node-pty is optional at runtime — main.ts falls back to pipes.
    return;
  }

  const repaired = [];
  for (const helper of helperCandidates(nodePtyRoot)) {
    const mode = statSync(helper).mode;
    // 0o111 — executable by anyone. node-pty spawns the helper as the current
    // user, but repairing all three bits matches what node-gyp would produce.
    if ((mode & 0o111) === 0o111) continue;
    chmodSync(helper, (mode & 0o7777) | 0o755);
    repaired.push(helper.slice(packageRoot.length + 1));
  }

  if (repaired.length > 0) {
    console.log(
      `[node-pty] restored the executable bit on ${repaired.length} spawn-helper binar${
        repaired.length === 1 ? 'y' : 'ies'
      }:`
    );
    for (const file of repaired) console.log(`  ${file}`);
  }
}

try {
  main();
} catch (error) {
  // Never break an install or a build over this. A helper that stays 0644
  // degrades the terminal to the pipe fallback; it does not stop the app.
  console.warn('[node-pty] could not fix spawn-helper permissions:', error?.message ?? error);
}
