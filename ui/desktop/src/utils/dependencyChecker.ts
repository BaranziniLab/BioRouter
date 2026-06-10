/**
 * dependencyChecker.ts
 *
 * Checks for required system dependencies (git, python, uv, npm) at startup.
 * Runs in the Electron main process. Sends check results and install progress
 * to the renderer via IPC push events on channel 'dependency-event'.
 *
 * Why custom PATH augmentation: Electron inherits a minimal PATH from launchd/
 * services, not the user's interactive shell. Tools installed via Homebrew,
 * cargo, or pyenv won't be on that PATH, so we probe known install locations.
 */

import { app, ipcMain, BrowserWindow } from 'electron';
import { spawnSync, spawn } from 'child_process';
import * as os from 'os';
import * as path from 'path';
import * as fs from 'fs';
import log from './logger';
import { getBiorouterCliBinaryPath } from '../biorouterd';

// ─── Types ────────────────────────────────────────────────────────────────────

export interface DependencyInfo {
  name: string;
  displayName: string;
  version: string | null;
  installed: boolean;
  installCmd: string;
  requiresSudo: boolean;
  downloadUrl: string;
  required?: boolean;
}

export interface DependencyEvent {
  type:
    | 'check-results'
    | 'install-start'
    | 'install-output'
    | 'install-done'
    | 'install-error'
    | 'recheck-results';
  dep?: string;
  deps?: DependencyInfo[];
  output?: string;
  error?: string;
  installed?: boolean;
  version?: string | null;
}

// ─── PATH augmentation ────────────────────────────────────────────────────────

function buildAugmentedPath(): string {
  const home = os.homedir();
  const extra: string[] = [];

  if (process.platform === 'darwin') {
    extra.push(
      '/usr/local/bin',
      '/opt/homebrew/bin',
      '/opt/homebrew/sbin',
      '/usr/bin',
      '/bin',
      path.join(home, '.cargo', 'bin'),
      path.join(home, '.local', 'bin'),
      path.join(home, 'Library', 'Python', '3.12', 'bin'),
      path.join(home, 'Library', 'Python', '3.11', 'bin'),
      path.join(home, 'Library', 'Python', '3.10', 'bin'),
    );
  } else if (process.platform === 'win32') {
    const localAppData = process.env.LOCALAPPDATA || '';
    const programFiles = process.env.ProgramFiles || 'C:\\Program Files';
    const programFilesX86 = process.env['ProgramFiles(x86)'] || 'C:\\Program Files (x86)';
    extra.push(
      // BioRouter's own bundled shims (uv.exe, uvx.exe, npx.cmd, git shim if added)
      // — must be first so bundled tools take priority over stale system installs
      path.join(localAppData, 'BioRouter', 'bin'),
      path.join(programFiles, 'Git', 'bin'),
      path.join(programFilesX86, 'Git', 'bin'),
      path.join(localAppData, 'Programs', 'Python', 'Python312'),
      path.join(localAppData, 'Programs', 'Python', 'Python311'),
      path.join(localAppData, 'Programs', 'Python', 'Python310'),
      path.join(programFiles, 'nodejs'),
      path.join(localAppData, 'Programs', 'nodejs'),
      path.join(home, '.cargo', 'bin'),
      path.join(localAppData, 'uv', 'bin'),
      // Bundled git fallback — appended last so system git always takes priority
      path.join(localAppData, 'BioRouter', 'git', 'cmd'),
    );
  } else {
    // Linux
    extra.push(
      '/usr/bin',
      '/usr/local/bin',
      '/bin',
      path.join(home, '.cargo', 'bin'),
      path.join(home, '.local', 'bin'),
    );
  }

  const current = process.env.PATH || '';
  const sep = process.platform === 'win32' ? ';' : ':';
  const unique = [...new Set([...extra, ...current.split(sep).filter(Boolean)])];
  return unique.join(sep);
}

const AUGMENTED_PATH = buildAugmentedPath();
export const SPAWN_ENV = { ...process.env, PATH: AUGMENTED_PATH };

// ─── Linux distro detection ───────────────────────────────────────────────────

type LinuxDistro = 'deb' | 'rpm' | 'unknown';

function detectLinuxDistro(): LinuxDistro {
  try {
    const content = fs.readFileSync('/etc/os-release', 'utf8').toLowerCase();
    if (content.includes('ubuntu') || content.includes('debian')) return 'deb';
    if (
      content.includes('fedora') ||
      content.includes('rhel') ||
      content.includes('centos') ||
      content.includes('rocky') ||
      content.includes('alma')
    )
      return 'rpm';
  } catch {}
  try {
    if (fs.existsSync('/etc/debian_version')) return 'deb';
    if (fs.existsSync('/etc/redhat-release')) return 'rpm';
  } catch {}
  return 'unknown';
}

// ─── Version probing ──────────────────────────────────────────────────────────

function probeVersion(cmd: string, args: string[]): string | null {
  try {
    const result = spawnSync(cmd, args, {
      encoding: 'utf8',
      timeout: 8000,
      env: SPAWN_ENV,
      // On Windows, use shell so we pick up .cmd/.bat wrappers (npm.cmd, etc.)
      shell: process.platform === 'win32',
    });
    if (result.status === 0 && result.stdout) {
      return result.stdout.trim().split('\n')[0].trim();
    }
  } catch {}
  return null;
}

// ─── Install command builders ─────────────────────────────────────────────────

function buildInstallInfo(
  dep: 'git' | 'python' | 'uv' | 'npm' | 'aws',
  distro: LinuxDistro,
): { cmd: string; requiresSudo: boolean; downloadUrl: string } {
  const platform = process.platform;

  if (platform === 'darwin') {
    switch (dep) {
      case 'git':
        return {
          cmd: 'xcode-select --install',
          requiresSudo: false,
          downloadUrl: 'https://git-scm.com/download/mac',
        };
      case 'python':
        return {
          cmd: 'brew install python3',
          requiresSudo: false,
          downloadUrl: 'https://www.python.org/downloads/',
        };
      case 'uv':
        return {
          cmd: 'curl -LsSf https://astral.sh/uv/install.sh | sh',
          requiresSudo: false,
          downloadUrl: 'https://docs.astral.sh/uv/getting-started/installation/',
        };
      case 'npm':
        return {
          cmd: 'brew install node',
          requiresSudo: false,
          downloadUrl: 'https://nodejs.org/en/download',
        };
      case 'aws':
        return {
          cmd: 'brew install awscli',
          requiresSudo: false,
          downloadUrl: 'http://biorouter.ucsf.edu/docs',
        };
    }
  }

  if (platform === 'win32') {
    switch (dep) {
      case 'git':
        return {
          cmd: 'winget install --id Git.Git -e --source winget',
          requiresSudo: false,
          downloadUrl: 'https://git-scm.com/download/win',
        };
      case 'python':
        return {
          cmd: 'winget install --id Python.Python.3 -e --source winget',
          requiresSudo: false,
          downloadUrl: 'https://www.python.org/downloads/',
        };
      case 'uv':
        return {
          cmd: 'powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/install.ps1 | iex"',
          requiresSudo: false,
          downloadUrl: 'https://docs.astral.sh/uv/getting-started/installation/',
        };
      case 'npm':
        return {
          cmd: 'winget install --id OpenJS.NodeJS -e --source winget',
          requiresSudo: false,
          downloadUrl: 'https://nodejs.org/en/download',
        };
      case 'aws':
        return {
          cmd: 'winget install Amazon.AWSCLI',
          requiresSudo: false,
          downloadUrl: 'http://biorouter.ucsf.edu/docs',
        };
    }
  }

  // Linux
  if (dep === 'uv') {
    return {
      cmd: 'curl -LsSf https://astral.sh/uv/install.sh | sh',
      requiresSudo: false,
      downloadUrl: 'https://docs.astral.sh/uv/getting-started/installation/',
    };
  }

  if (dep === 'aws') {
    if (distro === 'deb') {
      return { cmd: 'sudo apt-get install -y awscli', requiresSudo: true, downloadUrl: 'http://biorouter.ucsf.edu/docs' };
    }
    if (distro === 'rpm') {
      return { cmd: 'sudo dnf install -y awscli', requiresSudo: true, downloadUrl: 'http://biorouter.ucsf.edu/docs' };
    }
    return { cmd: 'pip install awscli', requiresSudo: false, downloadUrl: 'http://biorouter.ucsf.edu/docs' };
  }

  if (distro === 'deb') {
    const pkgs: Record<string, string> = { git: 'git', python: 'python3', npm: 'nodejs npm' };
    return {
      cmd: `sudo apt-get install -y ${pkgs[dep] ?? dep}`,
      requiresSudo: true,
      downloadUrl: '',
    };
  }

  if (distro === 'rpm') {
    const pkgs: Record<string, string> = { git: 'git', python: 'python3', npm: 'nodejs npm' };
    return {
      cmd: `sudo dnf install -y ${pkgs[dep] ?? dep}`,
      requiresSudo: true,
      downloadUrl: '',
    };
  }

  return {
    cmd: `# Install ${dep} via your system package manager`,
    requiresSudo: false,
    downloadUrl: '',
  };
}

// ─── Dependency check ─────────────────────────────────────────────────────────

let _distro: LinuxDistro | null = null;
function getLinuxDistro(): LinuxDistro {
  if (_distro === null) _distro = process.platform === 'linux' ? detectLinuxDistro() : 'unknown';
  return _distro;
}

/**
 * Single source of truth: ask the bundled `biorouter` CLI (which reads the Rust
 * `biorouter::system` spec) for the dependency status. Falls back to the native
 * probes below if the CLI isn't available (e.g. dev build) or errors, so the
 * desktop never loses its dependency check.
 */
export function checkAllDependencies(): DependencyInfo[] {
  return checkViaBundledCli() ?? checkNativeDependencies();
}

function checkViaBundledCli(): DependencyInfo[] | null {
  try {
    const cli = getBiorouterCliBinaryPath(app);
    const res = spawnSync(cli, ['doctor', '--format', 'json', '--no-update'], {
      encoding: 'utf8',
      timeout: 15000,
      env: SPAWN_ENV,
    });
    if (res.status !== 0 || !res.stdout) return null;
    const parsed = JSON.parse(res.stdout) as {
      dependencies?: Array<{
        name: string;
        display_name?: string;
        version?: string | null;
        installed?: boolean;
        install_command?: string | null;
        requires_sudo?: boolean;
        download_url?: string | null;
        required?: boolean;
      }>;
    };
    const deps = parsed?.dependencies;
    if (!Array.isArray(deps) || deps.length === 0) return null;
    return deps.map((d) => ({
      name: String(d.name),
      displayName: String(d.display_name ?? d.name),
      version: d.version ?? null,
      installed: !!d.installed,
      installCmd: d.install_command ?? '',
      requiresSudo: !!d.requires_sudo,
      downloadUrl: d.download_url ?? '',
      required: d.required ?? true,
    }));
  } catch (err) {
    log.warn('[DependencyChecker] bundled CLI check unavailable, using native probes:', err);
    return null;
  }
}

function checkNativeDependencies(): DependencyInfo[] {
  const distro = getLinuxDistro();

  const checks: Array<{
    name: 'git' | 'python' | 'uv' | 'npm' | 'aws';
    displayName: string;
    probes: Array<[string, string[]]>;
  }> = [
    {
      name: 'git',
      displayName: 'Git',
      probes: [['git', ['--version']]],
    },
    {
      name: 'python',
      displayName: 'Python',
      // Probe python3 first, then python (Windows often uses python)
      probes: [
        ['python3', ['--version']],
        ['python', ['--version']],
      ],
    },
    {
      name: 'uv',
      displayName: 'uv (Python package manager)',
      probes: [['uv', ['--version']]],
    },
    {
      name: 'npm',
      displayName: 'npm (Node.js)',
      probes: [['npm', ['--version']]],
    },
    {
      name: 'aws' as const,
      displayName: 'AWS CLI (optional)',
      probes: [['aws', ['--version']]],
    },
  ];

  // On Windows, uv manages its own Python runtime via uvx — system Python is not
  // required for extensions. If uv is present, mark Python as satisfied.
  const uvVersion = process.platform === 'win32' ? probeVersion('uv', ['--version']) : null;

  return checks.map(({ name, displayName, probes }) => {
    let version: string | null = null;

    if (name === 'python' && uvVersion !== null) {
      // uv bundles Python internally; no separate system Python needed on Windows
      version = `managed by uv ${uvVersion}`;
    } else {
      for (const [cmd, args] of probes) {
        version = probeVersion(cmd, args);
        if (version) break;
      }
    }

    const { cmd, requiresSudo, downloadUrl } = buildInstallInfo(name, distro);
    return { name, displayName, version, installed: version !== null, installCmd: cmd, requiresSudo, downloadUrl };
  });
}

// ─── Install runner ───────────────────────────────────────────────────────────

type SendFn = (event: DependencyEvent) => void;

function runInstallCommand(dep: string, cmd: string, send: SendFn): void {
  if (!cmd || !cmd.trim()) {
    send({ type: 'install-error', dep, error: 'No automated installer — use the Download link.' });
    return;
  }
  send({ type: 'install-start', dep });

  let child: ReturnType<typeof spawn>;

  if (process.platform === 'win32') {
    // On Windows, run via cmd.exe /c so pipes and .cmd wrappers work
    child = spawn('cmd.exe', ['/c', cmd], {
      env: SPAWN_ENV,
      shell: false,
    });
  } else {
    // macOS/Linux: run via sh -c so pipes (curl | sh) work
    child = spawn('sh', ['-c', cmd], {
      env: SPAWN_ENV,
      shell: false,
    });
  }

  child.stdout?.on('data', (chunk: Buffer) => {
    send({ type: 'install-output', dep, output: chunk.toString() });
  });
  child.stderr?.on('data', (chunk: Buffer) => {
    send({ type: 'install-output', dep, output: chunk.toString() });
  });

  child.on('close', (code) => {
    if (code === 0) {
      // Re-probe to get the version
      const info = checkAllDependencies().find((d) => d.name === dep);
      send({ type: 'install-done', dep, installed: info?.installed ?? false, version: info?.version ?? null });
    } else {
      send({ type: 'install-error', dep, error: `Process exited with code ${code}` });
    }
  });

  child.on('error', (err) => {
    send({ type: 'install-error', dep, error: err.message });
  });
}

// ─── IPC registration ─────────────────────────────────────────────────────────

let _registered = false;

export function registerDependencyIpcHandlers(): void {
  if (_registered) return;
  _registered = true;

  ipcMain.handle('dep:check', async () => {
    try {
      return checkAllDependencies();
    } catch (err) {
      log.error('[DependencyChecker] check error:', err);
      return [];
    }
  });

  ipcMain.handle('dep:install', async (_event, dep: string) => {
    const info = checkAllDependencies().find((d) => d.name === dep);
    if (!info) return { error: `Unknown dependency: ${dep}` };

    const win = BrowserWindow.fromWebContents(_event.sender);
    if (!win) return { error: 'No window found' };

    const send: SendFn = (payload) => {
      if (!win.isDestroyed()) win.webContents.send('dependency-event', payload);
    };

    runInstallCommand(dep, info.installCmd, send);
    return { started: true };
  });
}

// ─── Startup orchestrator ─────────────────────────────────────────────────────

export function setupDependencyChecker(): void {
  // Delay so the window is ready to receive IPC
  setTimeout(() => {
    try {
      const deps = checkAllDependencies();
      const missing = deps.filter((d) => !d.installed);
      if (missing.length === 0) {
        log.info('[DependencyChecker] All dependencies present.');
        return;
      }
      log.warn('[DependencyChecker] Missing deps:', missing.map((d) => d.name));

      // Notify all open windows
      const payload: DependencyEvent = { type: 'check-results', deps };
      BrowserWindow.getAllWindows().forEach((win) => {
        if (!win.isDestroyed()) win.webContents.send('dependency-event', payload);
      });
    } catch (err) {
      log.error('[DependencyChecker] startup check error:', err);
    }
  }, 4000);
}

export function triggerDependencyCheck(): void {
  try {
    const deps = checkAllDependencies();
    const payload: DependencyEvent = { type: 'check-results', deps };
    BrowserWindow.getAllWindows().forEach((win) => {
      if (!win.isDestroyed()) win.webContents.send('dependency-event', payload);
    });
  } catch (err) {
    log.error('[DependencyChecker] manual check error:', err);
  }
}
