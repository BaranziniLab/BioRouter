import Electron from 'electron';
import fs from 'node:fs';
import { spawn, ChildProcess } from 'child_process';
import { createServer } from 'net';
import os from 'node:os';
import path from 'node:path';
import log from './utils/logger';
import { App } from 'electron';
import { Buffer } from 'node:buffer';

import { status } from './api';
import { Client } from './api/client';
import { ExternalBiorouterdConfig } from './utils/settings';

export const findAvailablePort = (): Promise<number> => {
  return new Promise((resolve, _reject) => {
    const server = createServer();

    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address() as { port: number };
      server.close(() => {
        log.info(`Found available port: ${port}`);
        resolve(port);
      });
    });
  });
};

// Check if biorouterd server is ready by polling the status endpoint
export const checkServerStatus = async (client: Client, errorLog: string[]): Promise<boolean> => {
  const interval = 100; // ms
  const maxAttempts = 100; // 10s

  const fatal = (line: string) => {
    const trimmed = line.trim().toLowerCase();
    return trimmed.startsWith("thread 'main' panicked at") || trimmed.startsWith('error:');
  };

  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    if (errorLog.some(fatal)) {
      log.error('Detected fatal error in server logs');
      return false;
    }
    try {
      await status({ client, throwOnError: true });
      return true;
    } catch {
      if (attempt === maxAttempts) {
        log.error(`Server failed to respond after ${(interval * maxAttempts) / 1000} seconds`);
      }
    }
    await new Promise((resolve) => setTimeout(resolve, interval));
  }
  return false;
};

export type DaemonLogLevel = 'error' | 'warn' | 'info' | 'debug';

// eslint-disable-next-line no-control-regex
const ANSI_ESCAPE_PATTERN = /\u001b\[[0-9;]*m/g;

// The daemon writes tracing's pretty format to stderr:
//   `  2026-07-26T18:40:14.289898Z  WARN some::target: message`
// possibly with continuation lines (`    at src/foo.rs:12`). Match the level
// only at the head of the line (after an optional timestamp) so a level word
// appearing inside a message body cannot re-classify the line.
const DAEMON_LEVEL_PATTERN =
  /^\s*(?:\[?\d{4}-\d{2}-\d{2}[T ][0-9:.]+(?:Z|[+-]\d{2}:?\d{2})?\]?\s+)?\[?(TRACE|DEBUG|INFO|WARN|WARNING|ERROR|FATAL)\]?\b/;

// Not everything on the daemon's stderr comes from tracing. Rust's default
// panic hook writes `thread '<name>' panicked at <loc>:` directly, and
// `biorouterd`'s `async fn main() -> anyhow::Result<()>` makes the standard
// `Termination` impl print `Error: <chain>` when startup fails. Neither line
// carries a level word, so the tracing parser cannot see them — and these are
// precisely the lines that must not be filed under `info`. Anchored at the
// head of the (trimmed) line so the same words inside a message body cannot
// re-classify it.
const RUST_PANIC_PATTERN = /^thread\s+'[^']*'\s+panicked\s+at\b/;
const FATAL_PREFIX_PATTERN = /^(?:error|fatal)\s*:/i;

/**
 * Map a line of `biorouterd` stderr onto the electron-log level that matches
 * the daemon's own severity. A line whose level cannot be parsed defaults to
 * `info`: it is a line that could not be parsed, not an error. Logging all
 * daemon stderr at `error` destroys severity at the process boundary and makes
 * main.log unfilterable (see issue #49).
 *
 * The daemon's console layer is configured with `.pretty().with_ansi(false)`
 * (crates/biorouter-server/src/logging.rs), so there is no JSON tracing format
 * to parse here — only the pretty format, plus the two non-tracing shapes
 * above.
 */
export const daemonStderrLogLevel = (line: string): DaemonLogLevel => {
  const plain = line.replace(ANSI_ESCAPE_PATTERN, '');
  const match = DAEMON_LEVEL_PATTERN.exec(plain);
  switch (match?.[1]) {
    case 'ERROR':
    case 'FATAL':
      return 'error';
    case 'WARN':
    case 'WARNING':
      return 'warn';
    case 'DEBUG':
    case 'TRACE':
      return 'debug';
    case undefined: {
      const head = plain.trimStart();
      return RUST_PANIC_PATTERN.test(head) || FATAL_PREFIX_PATTERN.test(head) ? 'error' : 'info';
    }
    default:
      return 'info';
  }
};

export interface BiorouterdResult {
  baseUrl: string;
  workingDir: string;
  process: ChildProcess;
  errorLog: string[];
}

const connectToExternalBackend = (workingDir: string, url: string): BiorouterdResult => {
  log.info(`Using external biorouterd backend at ${url}`);

  const mockProcess = {
    pid: undefined,
    kill: () => {
      log.info(`Not killing external process that is managed externally`);
    },
  } as ChildProcess;

  return { baseUrl: url, workingDir, process: mockProcess, errorLog: [] };
};

interface BiorouterProcessEnv {
  [key: string]: string | undefined;

  HOME: string;
  USERPROFILE: string;
  APPDATA: string;
  LOCALAPPDATA: string;
  PATH: string;
  BIOROUTER_PORT: string;
  BIOROUTER_SERVER__SECRET_KEY?: string;
  BIOROUTER_DISABLE_KEYRING?: string;
}

export interface StartBiorouterdOptions {
  app: App;
  serverSecret: string;
  dir: string;
  env?: Partial<BiorouterProcessEnv>;
  externalBiorouterd?: ExternalBiorouterdConfig;
}

export const startBiorouterd = async (
  options: StartBiorouterdOptions
): Promise<BiorouterdResult> => {
  const { app, serverSecret, dir: inputDir, env = {}, externalBiorouterd } = options;
  const isWindows = process.platform === 'win32';
  const homeDir = os.homedir();
  const dir = path.resolve(path.normalize(inputDir));

  if (externalBiorouterd?.enabled && externalBiorouterd.url) {
    return connectToExternalBackend(dir, externalBiorouterd.url);
  }

  if (process.env.BIOROUTER_EXTERNAL_BACKEND) {
    return connectToExternalBackend(dir, 'http://127.0.0.1:3000');
  }

  let biorouterdPath = getBiorouterdBinaryPath(app);

  const resolvedBiorouterdPath = path.resolve(biorouterdPath);

  const port = await findAvailablePort();
  // Bounded ring of the most recent stderr lines. Without a cap this array
  // grows for the lifetime of the Electron main process — a long-running
  // chatty biorouterd can retain hundreds of MB of strings and trip a fatal
  // V8 CHECK on the main thread during optimizing compile / GC compaction.
  const STDERR_RING_MAX = 500;
  const stderrLines: string[] = [];

  log.info(`Starting biorouterd from: ${resolvedBiorouterdPath} on port ${port} in dir ${dir}`);

  const additionalEnv: BiorouterProcessEnv = {
    HOME: homeDir,
    USERPROFILE: homeDir,
    APPDATA: process.env.APPDATA || path.join(homeDir, 'AppData', 'Roaming'),
    LOCALAPPDATA: process.env.LOCALAPPDATA || path.join(homeDir, 'AppData', 'Local'),
    PATH: `${path.dirname(resolvedBiorouterdPath)}${path.delimiter}${process.env.PATH || ''}`,
    BIOROUTER_PORT: String(port),
    BIOROUTER_SERVER__SECRET_KEY: serverSecret,
    // Dev Electron rebuilds should not trigger macOS Keychain prompts; packaged
    // builds keep the normal OS credential-store behavior.
    BIOROUTER_DISABLE_KEYRING:
      process.env.BIOROUTER_DISABLE_KEYRING ?? (!app.isPackaged ? 'true' : undefined),
    // Default Auto Visualiser to CDN-referenced assets so each figure's persisted
    // HTML blob is a few KB instead of megabytes of inlined D3/Chart.js/Leaflet/
    // Mermaid — keeps figure-heavy sessions light in the renderer heap and SQLite.
    // Respects an explicit user override; set BIOROUTER_AUTOVIS_CDN=0 for fully
    // offline/self-contained figures (no network needed at render time).
    BIOROUTER_AUTOVIS_CDN: process.env.BIOROUTER_AUTOVIS_CDN ?? '1',
    ...env,
  } as BiorouterProcessEnv;

  const processEnv: BiorouterProcessEnv = {
    ...process.env,
    ...additionalEnv,
  } as BiorouterProcessEnv;

  if (isWindows && !resolvedBiorouterdPath.toLowerCase().endsWith('.exe')) {
    biorouterdPath = resolvedBiorouterdPath + '.exe';
  } else {
    biorouterdPath = resolvedBiorouterdPath;
  }
  log.info(`Binary path resolved to: ${biorouterdPath}`);

  const spawnOptions = {
    cwd: dir,
    env: processEnv,
    stdio: ['ignore', 'pipe', 'pipe'] as ['ignore', 'pipe', 'pipe'],
    windowsHide: true,
    detached: isWindows,
    shell: false,
  };

  const safeArgs = ['agent'];

  const biorouterdProcess: ChildProcess = spawn(biorouterdPath, safeArgs, spawnOptions);

  if (isWindows && biorouterdProcess.unref) {
    biorouterdProcess.unref();
  }

  biorouterdProcess.stdout?.on('data', (data: Buffer) => {
    log.info(`biorouterd stdout for port ${port} and dir ${dir}: ${data.toString()}`);
  });

  biorouterdProcess.stderr?.on('data', (data: Buffer) => {
    const lines = data
      .toString()
      .split('\n')
      .filter((l) => l.trim());
    lines.forEach((line) => {
      // Route each line at the daemon's own severity. Logging the whole stream
      // at `error` made main.log a wall of apparent failures with no way to
      // find the real one.
      log[daemonStderrLogLevel(line)](`biorouterd stderr for port ${port} and dir ${dir}: ${line}`);
      stderrLines.push(line);
      if (stderrLines.length > STDERR_RING_MAX) {
        stderrLines.splice(0, stderrLines.length - STDERR_RING_MAX);
      }
    });
  });

  biorouterdProcess.on('close', (code: number | null) => {
    log.info(`biorouterd process exited with code ${code} for port ${port} and dir ${dir}`);
  });

  biorouterdProcess.on('error', (err: Error) => {
    // Do not `throw` here — this callback runs inside the EventEmitter, and
    // a synchronous throw becomes an uncaught exception in the Node event
    // loop, fatally aborting the Electron main process with no usable
    // diagnostic. Record the failure so checkServerStatus can surface it
    // through the normal startup error path instead.
    log.error(`Failed to start biorouterd on port ${port} and dir ${dir}`, err);
    // "error:" prefix matches checkServerStatus's fatal() predicate so the
    // startup probe short-circuits with a useful error rather than waiting
    // out the 10s status-poll timeout.
    stderrLines.push(`error: failed to spawn biorouterd: ${err.message}`);
  });

  const try_kill_biorouter = () => {
    try {
      if (isWindows) {
        const pid = biorouterdProcess.pid?.toString() || '0';
        spawn('taskkill', ['/pid', pid, '/T', '/F'], { shell: false });
      } else {
        biorouterdProcess.kill?.();
      }
    } catch (error) {
      log.error('Error while terminating biorouterd process:', error);
    }
  };

  app.on('will-quit', () => {
    log.info('App quitting, terminating biorouterd server');
    try_kill_biorouter();
  });

  log.info(`Biorouterd server successfully started on port ${port}`);
  return {
    baseUrl: `http://127.0.0.1:${port}`,
    workingDir: dir,
    process: biorouterdProcess,
    errorLog: stderrLines,
  };
};

/**
 * Resolve the bundled `biorouter` CLI binary (sibling of biorouterd). Used to
 * offer "install the Biorouter CLI onto PATH" and to run `biorouter doctor`
 * from the desktop app, so the dependency/install logic lives in one place
 * (the Rust `biorouter::system` module) shared by the CLI and the GUI.
 */
export const getBiorouterCliBinaryPath = (app: Electron.App): string => {
  const executableName = process.platform === 'win32' ? 'biorouter.exe' : 'biorouter';
  const possiblePaths = app.isPackaged
    ? [path.join(process.resourcesPath, 'bin', executableName)]
    : [
        path.join(process.cwd(), '..', '..', 'target', 'debug', executableName),
        path.join(process.cwd(), '..', '..', 'target', 'release', executableName),
        path.join(process.cwd(), 'src', 'bin', executableName),
        path.join(process.cwd(), 'bin', executableName),
      ];
  for (const binPath of possiblePaths) {
    const resolved = path.resolve(binPath);
    if (fs.existsSync(resolved) && fs.statSync(resolved).isFile()) {
      return resolved;
    }
  }
  throw new Error(`Could not find ${executableName} in: ${possiblePaths.join(', ')}`);
};

const getBiorouterdBinaryPath = (app: Electron.App): string => {
  let executableName = process.platform === 'win32' ? 'biorouterd.exe' : 'biorouterd';

  let possiblePaths: string[];
  if (!app.isPackaged) {
    possiblePaths = [
      path.join(process.cwd(), '..', '..', 'target', 'debug', executableName),
      path.join(process.cwd(), '..', '..', 'target', 'release', executableName),
      path.join(process.cwd(), 'src', 'bin', executableName),
      path.join(process.cwd(), 'bin', executableName),
    ];
  } else {
    possiblePaths = [path.join(process.resourcesPath, 'bin', executableName)];
  }

  for (const binPath of possiblePaths) {
    try {
      const resolvedPath = path.resolve(binPath);

      if (fs.existsSync(resolvedPath)) {
        const stats = fs.statSync(resolvedPath);
        if (stats.isFile()) {
          return resolvedPath;
        } else {
          log.error(`Path exists but is not a regular file: ${resolvedPath}`);
        }
      }
    } catch (error) {
      log.error(`Error checking path ${binPath}:`, error);
    }
  }

  throw new Error(
    `Could not find ${executableName} binary in any of the expected locations: ${possiblePaths.join(
      ', '
    )}`
  );
};
