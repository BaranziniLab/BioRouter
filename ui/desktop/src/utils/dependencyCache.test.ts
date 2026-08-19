/**
 * The dependency check memoises, and concurrent callers share one probe. Both
 * are there because `biorouter doctor` costs ~3.5 s and startup, the modal and
 * the post-install re-check all ask for it at once.
 *
 * The interleavings below are what the generation counter exists for: a `force`
 * cannot cancel a probe already running, so without it a superseded probe lands
 * afterwards and reinstates the snapshot the force asked to discard — with a
 * FRESH timestamp, so it is served for another full TTL.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('electron', () => ({
  app: { getVersion: () => '1.0.0', getPath: () => '/tmp' },
  ipcMain: { handle: vi.fn() },
  BrowserWindow: { getAllWindows: () => [] },
}));
vi.mock('./logger', () => ({ default: { info: vi.fn(), warn: vi.fn(), error: vi.fn() } }));

// One controllable probe stands in for `biorouter doctor`. Declared with `var`
// because `vi.mock` factories are hoisted above every `let`/`const`, and the
// factory below closes over these.
var pending: Array<() => void> = [];
var probeCount = 0;
vi.mock('../biorouterd', () => ({ getBiorouterCliBinaryPath: () => '/nonexistent/biorouter' }));

vi.mock('child_process', async () => {
  const execFile = (
    _cmd: string,
    _args: string[],
    _opts: unknown,
    cb: (e: Error | null, stdout: string, stderr: string) => void
  ) => {
    probeCount += 1;
    const n = probeCount;
    pending.push(() =>
      cb(null, JSON.stringify({ dependencies: [{ name: `probe-${n}`, installed: true }] }), '')
    );
    return {};
  };
  // `dependencyChecker` uses `promisify(execFile)`, and promisify's GENERIC
  // wrapper resolves with only the first callback value — i.e. `stdout` as a bare
  // string, so destructuring `{ stdout, stderr }` yields undefined and every probe
  // silently "fails". Node's real execFile avoids that by defining
  // `promisify.custom`; a mock has to do the same or the module under test falls
  // through to its native-probe path and hangs.
  const promisifyCustom = (await import('util')).promisify.custom;
  (execFile as unknown as Record<symbol, unknown>)[promisifyCustom] = (
    cmd: string,
    args: string[],
    opts: unknown
  ) =>
    new Promise((resolve, reject) => {
      execFile(cmd, args, opts, (e, stdout, stderr) =>
        e ? reject(e) : resolve({ stdout, stderr })
      );
    });

  const spawn = () => ({ stdout: null, stderr: null, on: () => {} });
  return { execFile, spawn, default: { execFile, spawn } };
});

import { checkAllDependencies, invalidateDependencyCache } from './dependencyChecker';

function settleAll() {
  const q = pending;
  pending = [];
  q.forEach((fn) => fn());
}

beforeEach(() => {
  pending = [];
  probeCount = 0;
  invalidateDependencyCache();
});

describe('dependency check caching', () => {
  it('shares one probe between concurrent callers', async () => {
    const a = checkAllDependencies();
    const b = checkAllDependencies();
    expect(probeCount).toBe(1);

    settleAll();
    expect((await a)[0].name).toBe('probe-1');
    expect((await b)[0].name).toBe('probe-1');
  });

  it('serves the memoised result without re-probing', async () => {
    const first = checkAllDependencies();
    settleAll();
    await first;

    const second = await checkAllDependencies();
    expect(probeCount).toBe(1);
    expect(second[0].name).toBe('probe-1');
  });

  it('force re-probes even with a warm cache', async () => {
    const first = checkAllDependencies();
    settleAll();
    await first;

    const forced = checkAllDependencies({ force: true });
    expect(probeCount).toBe(2);
    settleAll();
    expect((await forced)[0].name).toBe('probe-2');
  });

  it('a probe superseded by force does not reinstate its stale snapshot', async () => {
    // A starts, then a force supersedes it, then A finishes LAST.
    const a = checkAllDependencies();
    const b = checkAllDependencies({ force: true });
    expect(probeCount).toBe(2);

    const [settleA, settleB] = pending;
    pending = [];
    settleB();
    await b;
    settleA();
    await a;

    // The next reader must see the forced result, not the one it superseded.
    const next = await checkAllDependencies();
    expect(next[0].name).toBe('probe-2');
    expect(probeCount).toBe(2);
  });

  it('a superseded probe finishing does not deregister the live one', async () => {
    const a = checkAllDependencies();
    const b = checkAllDependencies({ force: true });

    const [settleA] = pending;
    settleA();
    await a;

    // B is still running: a new caller must JOIN it, not start a third probe.
    const c = checkAllDependencies();
    expect(probeCount).toBe(2);

    pending.forEach((fn) => fn());
    expect((await c)[0].name).toBe('probe-2');
    expect((await b)[0].name).toBe('probe-2');
  });

  it('invalidate makes the next caller re-probe', async () => {
    const first = checkAllDependencies();
    settleAll();
    await first;

    invalidateDependencyCache();
    const second = checkAllDependencies();
    expect(probeCount).toBe(2);
    settleAll();
    expect((await second)[0].name).toBe('probe-2');
  });
});
