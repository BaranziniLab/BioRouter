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
}

export interface StartBiorouterdOptions {
  app: App;
  serverSecret: string;
  dir: string;
  env?: Partial<BiorouterProcessEnv>;
  externalBiorouterd?: ExternalBiorouterdConfig;
}

export const startBiorouterd = async (options: StartBiorouterdOptions): Promise<BiorouterdResult> => {
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
    ...env,
  } as BiorouterProcessEnv;

  const processEnv: BiorouterProcessEnv = { ...process.env, ...additionalEnv } as BiorouterProcessEnv;

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

  const safeSpawnOptions = {
    ...spawnOptions,
    env: Object.keys(spawnOptions.env || {}).reduce(
      (acc, key) => {
        if (key.includes('SECRET') || key.includes('PASSWORD') || key.includes('TOKEN')) {
          acc[key] = '[REDACTED]';
        } else {
          acc[key] = spawnOptions.env![key] || '';
        }
        return acc;
      },
      {} as Record<string, string>
    ),
  };
  log.info('Spawn options:', JSON.stringify(safeSpawnOptions, null, 2));

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
      log.error(`biorouterd stderr for port ${port} and dir ${dir}: ${line}`);
      stderrLines.push(line);
    });
  });

  biorouterdProcess.on('close', (code: number | null) => {
    log.info(`biorouterd process exited with code ${code} for port ${port} and dir ${dir}`);
  });

  biorouterdProcess.on('error', (err: Error) => {
    log.error(`Failed to start biorouterd on port ${port} and dir ${dir}`, err);
    throw err;
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

const getBiorouterdBinaryPath = (app: Electron.App): string => {
  let executableName = process.platform === 'win32' ? 'biorouterd.exe' : 'biorouterd';

  let possiblePaths: string[];
  if (!app.isPackaged) {
    possiblePaths = [
      path.join(process.cwd(), 'src', 'bin', executableName),
      path.join(process.cwd(), 'bin', executableName),
      path.join(process.cwd(), '..', '..', 'target', 'debug', executableName),
      path.join(process.cwd(), '..', '..', 'target', 'release', executableName),
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
