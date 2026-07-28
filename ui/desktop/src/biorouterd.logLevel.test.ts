import type { App } from 'electron';
import type { PathLike, Stats } from 'node:fs';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  logInfo: vi.fn(),
  logError: vi.fn(),
  logWarn: vi.fn(),
  logDebug: vi.fn(),
  spawn: vi.fn(),
}));

// Same rationale as biorouterd.test.ts: the binary-path probe hits the real
// filesystem, so stub `node:fs` for the biorouterd candidates only.
const isBiorouterdPath = (p: PathLike): boolean => {
  const s = p.toString();
  return s.endsWith('biorouterd') || s.endsWith('biorouterd.exe');
};
vi.mock('node:fs', async (importOriginal) => {
  const actual = await importOriginal<typeof import('node:fs')>();
  const existsSync = (p: PathLike): boolean => (isBiorouterdPath(p) ? true : actual.existsSync(p));
  const statSync = (p: PathLike): Stats =>
    isBiorouterdPath(p) ? ({ isFile: () => true } as unknown as Stats) : actual.statSync(p);
  const mocked = { ...actual, existsSync, statSync };
  return { ...mocked, default: mocked };
});

vi.mock('./utils/logger', () => ({
  default: {
    info: mocks.logInfo,
    error: mocks.logError,
    warn: mocks.logWarn,
    debug: mocks.logDebug,
  },
}));

vi.mock('child_process', async (importOriginal) => {
  const actual = await importOriginal<typeof import('child_process')>();
  const mocked = { ...actual, spawn: mocks.spawn };
  return { ...mocked, default: mocked };
});

import { daemonStderrLogLevel, startBiorouterd } from './biorouterd';

describe('daemonStderrLogLevel', () => {
  it('maps the daemon tracing level onto the matching electron-log level', () => {
    expect(
      daemonStderrLogLevel(
        '  2026-07-26T18:40:14.289898Z ERROR biorouter::agents::agent: provider call failed'
      )
    ).toBe('error');
    expect(
      daemonStderrLogLevel(
        '  2026-07-26T18:40:14.289898Z  WARN biorouter::slash_commands: nothing configured'
      )
    ).toBe('warn');
    expect(
      daemonStderrLogLevel(
        '  2026-07-26T18:40:14.289898Z  INFO biorouter_server::routes: listening on 127.0.0.1'
      )
    ).toBe('info');
    expect(
      daemonStderrLogLevel(
        '  2026-07-26T18:40:14.289898Z DEBUG biorouter::config::base: loaded config'
      )
    ).toBe('debug');
    expect(
      daemonStderrLogLevel('  2026-07-26T18:40:14.289898Z TRACE mcp_client::transport: frame')
    ).toBe('debug');
  });

  it('reads the level when the line carries no timestamp', () => {
    expect(daemonStderrLogLevel('WARN biorouter::slash_commands: missing key')).toBe('warn');
    expect(daemonStderrLogLevel(' INFO biorouter: started')).toBe('info');
  });

  it('defaults an unparseable line to info, not error', () => {
    // Pretty-format continuation lines, and anything a child process prints
    // without a level, carry no severity — that is not the same as a failure.
    expect(daemonStderrLogLevel('    at crates/biorouter/src/workflow/mod.rs:42 on main')).toBe(
      'info'
    );
    expect(daemonStderrLogLevel('some library banner text')).toBe('info');
  });

  it('does not take a level word from the middle of a message', () => {
    expect(
      daemonStderrLogLevel(
        '  2026-07-26T18:40:14.289898Z  INFO biorouter::agents: tool returned ERROR to the model'
      )
    ).toBe('info');
  });

  // The Rust panic hook and anyhow's `Termination` impl write straight to
  // stderr, bypassing tracing entirely, so these lines carry no level word.
  // They are exactly the lines that must not be swallowed at `info`.
  it('classifies a raw Rust panic as error', () => {
    expect(
      daemonStderrLogLevel("thread 'main' panicked at crates/biorouter-server/src/main.rs:44:9:")
    ).toBe('error');
    // A panic on a tokio worker is just as much a failure as one on main.
    expect(
      daemonStderrLogLevel(
        "thread 'tokio-runtime-worker' panicked at crates/biorouter/src/x.rs:1:1:"
      )
    ).toBe('error');
    expect(daemonStderrLogLevel("  thread 'main' panicked at src/main.rs:1:1:")).toBe('error');
  });

  it('classifies anyhow/fatal startup output as error', () => {
    // `async fn main() -> anyhow::Result<()>` prints `Error: <chain>` on Err.
    expect(daemonStderrLogLevel('Error: failed to bind 127.0.0.1:0')).toBe('error');
    expect(daemonStderrLogLevel('error: failed to spawn biorouterd: ENOENT')).toBe('error');
    expect(daemonStderrLogLevel('Fatal: unrecoverable')).toBe('error');
  });

  it('leaves panic follow-up and mid-message mentions at their own level', () => {
    expect(daemonStderrLogLevel('note: run with `RUST_BACKTRACE=1` to display a backtrace')).toBe(
      'info'
    );
    expect(
      daemonStderrLogLevel(
        "  2026-07-26T18:40:14.289898Z  INFO biorouter::agents: thread 'main' panicked at was in the tool output"
      )
    ).toBe('info');
  });
});

describe('biorouterd stderr routing', () => {
  beforeEach(() => {
    mocks.logInfo.mockClear();
    mocks.logError.mockClear();
    mocks.logWarn.mockClear();
    mocks.logDebug.mockClear();
    mocks.spawn.mockReset();
  });

  it('logs each daemon stderr line at its own level', async () => {
    let stderrHandler: ((data: Buffer) => void) | undefined;
    mocks.spawn.mockReturnValue({
      stdout: { on: vi.fn() },
      stderr: {
        on: vi.fn((event: string, handler: (data: Buffer) => void) => {
          if (event === 'data') stderrHandler = handler;
        }),
      },
      on: vi.fn(),
      kill: vi.fn(),
      unref: vi.fn(),
    });

    const app = { isPackaged: false, on: vi.fn() } as unknown as App;
    const result = await startBiorouterd({ app, serverSecret: 'secret', dir: process.cwd() });

    expect(stderrHandler).toBeDefined();
    stderrHandler!(
      Buffer.from(
        [
          '  2026-07-26T18:40:14.289898Z  INFO biorouter_server: listening',
          '  2026-07-26T18:40:14.289898Z  WARN biorouter::config: deprecated key',
          '  2026-07-26T18:40:14.289898Z ERROR biorouter::agents::agent: provider call failed',
          '  2026-07-26T18:40:14.289898Z DEBUG biorouter::session: opened db',
          '    at crates/biorouter/src/session/mod.rs:12',
          '',
        ].join('\n')
      )
    );

    const joined = (calls: unknown[][]) => calls.map((c) => String(c[0])).join('\n');
    expect(joined(mocks.logError.mock.calls)).toContain('provider call failed');
    expect(mocks.logError.mock.calls).toHaveLength(1);
    expect(joined(mocks.logWarn.mock.calls)).toContain('deprecated key');
    expect(mocks.logWarn.mock.calls).toHaveLength(1);
    expect(joined(mocks.logDebug.mock.calls)).toContain('opened db');
    expect(mocks.logDebug.mock.calls).toHaveLength(1);
    // startBiorouterd logs its own startup info lines too, so assert on content.
    expect(joined(mocks.logInfo.mock.calls)).toContain('listening');
    expect(joined(mocks.logInfo.mock.calls)).toContain('session/mod.rs:12');

    // Every stderr line still reaches the bounded ring the startup probe reads.
    expect(result.errorLog).toHaveLength(5);
  });
});
