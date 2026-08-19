/**
 * `runProbe` replaced `spawnSync` on every startup path, so its mapping from a
 * child process's outcome onto `{ ok, code, timedOut }` is load-bearing: a
 * mis-read of "failed" vs "timed out" vs "not installed" changes what the
 * dependency check reports and what the user is told to do about it.
 *
 * These spawn real processes — via `process.execPath`, so they run wherever Node
 * does rather than assuming a POSIX shell.
 */
import { describe, it, expect, vi } from 'vitest';

// CI installs npm dependencies without the Electron binary, so evaluating the real
// `electron` module throws "Electron failed to install correctly" at IMPORT time —
// the suite never loads and every test in it is silently skipped rather than
// failing. Anything reaching `electron` (here via `./logger`) must be mocked, and
// a local run cannot catch it: this machine has the binary.
vi.mock('electron', () => ({
  app: { getVersion: () => '0.0.0-test', getPath: () => '/tmp', isPackaged: false },
  ipcMain: { handle: () => {} },
  BrowserWindow: { getAllWindows: () => [] },
}));
vi.mock('./logger', () => ({
  default: { info: () => {}, warn: () => {}, error: () => {} },
}));
vi.mock('../biorouterd', () => ({ getBiorouterCliBinaryPath: () => '/nonexistent/biorouter' }));

import { runProbe } from './dependencyChecker';

const NODE = process.execPath;

describe('runProbe', () => {
  it('reports success and captures stdout', async () => {
    const r = await runProbe(NODE, ['-e', 'process.stdout.write("v1.2.3")']);
    expect(r.ok).toBe(true);
    expect(r.stdout).toBe('v1.2.3');
    expect(r.code).toBe(0);
    expect(r.timedOut).toBe(false);
  });

  it('reports a non-zero exit as a failure, with the code and stderr', async () => {
    const r = await runProbe(NODE, ['-e', 'process.stderr.write("boom"); process.exit(3)']);
    expect(r.ok).toBe(false);
    expect(r.code).toBe(3);
    expect(r.stderr).toContain('boom');
    // Not a timeout — the caller branches on this to choose the message.
    expect(r.timedOut).toBe(false);
  });

  it('reports a missing command as a failure, not a timeout', async () => {
    const r = await runProbe('definitely-not-a-real-command-xyz', ['--version']);
    expect(r.ok).toBe(false);
    expect(r.timedOut).toBe(false);
    // ENOENT is a string code; it must not be reported as an exit status.
    expect(r.code).toBeNull();
  });

  it('distinguishes a timeout from an ordinary failure', async () => {
    const r = await runProbe(NODE, ['-e', 'setTimeout(() => {}, 10000)'], 300);
    expect(r.ok).toBe(false);
    expect(r.timedOut).toBe(true);
  });

  it('honours cwd', async () => {
    const r = await runProbe(NODE, ['-e', 'process.stdout.write(process.cwd())'], 8000, {
      cwd: process.cwd(),
    });
    expect(r.ok).toBe(true);
    expect(r.stdout.length).toBeGreaterThan(0);
  });
});
