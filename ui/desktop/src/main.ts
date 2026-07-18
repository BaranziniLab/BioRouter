import type {
  MenuItemConstructorOptions,
  OpenDialogOptions,
  OpenDialogReturnValue,
  Rectangle,
} from 'electron';
import {
  app,
  App,
  BrowserWindow,
  dialog,
  globalShortcut,
  ipcMain,
  Menu,
  MenuItem,
  Notification,
  powerSaveBlocker,
  screen,
  session,
  shell,
  Tray,
} from 'electron';
import { pathToFileURL, format as formatUrl, URLSearchParams } from 'node:url';
import { Buffer } from 'node:buffer';
import { isIP } from 'node:net';
import dns from 'node:dns/promises';
import fs from 'node:fs/promises';
import fsSync from 'node:fs';
import started from 'electron-squirrel-startup';
import path from 'node:path';
import os from 'node:os';
import { spawn, spawnSync, type ChildProcess } from 'child_process';
import AdmZip from 'adm-zip';
import { safeExtractZip, safeZipEntryTarget } from './utils/safeZip';
import 'dotenv/config';
import { checkServerStatus, startBiorouterd, getBiorouterCliBinaryPath } from './biorouterd';
import { getSharedBackend, isSharedDaemonEnabled, resetSharedBackend } from './biorouterdSingleton';
import { expandTilde } from './utils/pathUtils';
import log from './utils/logger';
import { ensureWinShims } from './utils/winShims';
import { addRecentDir, loadRecentDirs } from './utils/recentDirs';
import {
  EnvToggles,
  loadSettings,
  saveSettings,
  updateEnvironmentVariables,
} from './utils/settings';
import * as crypto from 'crypto';
// import electron from "electron";
import * as yaml from 'yaml';
import windowStateKeeper from 'electron-window-state';
import {
  getUpdateAvailable,
  openUpdateSettings,
  popUpTrayMenu,
  registerUpdateIpcHandlers,
  setTrayRef,
  setupAutoUpdater,
  updateTrayMenu,
} from './utils/autoUpdater';
import { UPDATES_ENABLED } from './updates';
import './utils/workflowHash';
import { parseWorkflowDeeplink, type WorkflowDeeplinkData } from './utils/workflowDeeplink';
import {
  registerDependencyIpcHandlers,
  setupDependencyChecker,
  triggerDependencyCheck,
  SPAWN_ENV,
} from './utils/dependencyChecker';
import { runExtensionUpdateCheck, scheduleExtensionUpdateCheck } from './utils/extensionUpdater';
import {
  isAllowedArtifactFrameNavigation,
  isAllowedRendererPermission,
  isAppOrigin,
  shouldOpenExternalNavigation,
} from './utils/permissionPolicy';
import {
  ARTIFACT_WRAPPER_CSP,
  injectArtifactHostTheme,
  wrapArtifactForBrowser,
} from './utils/artifactSecurity';
import { readGitArtifactTree } from './utils/artifactGit';
import { readArtifactDirectoryTree } from './utils/artifactDirectory';
import {
  diagnosticsArchiveBytes,
  diagnosticsArchiveFilename,
  type DiagnosticsArchivePayload,
} from './utils/diagnosticsExport';
import { Client, createClient, createConfig } from './api/client';
import { BioRouterApp } from './api';
import installExtension, { REACT_DEVELOPER_TOOLS } from 'electron-devtools-installer';

// Updater functions (moved here to keep updates.ts minimal for release replacement)
function shouldSetupUpdater(): boolean {
  // Setup updater if either the flag is enabled OR dev updates are enabled
  return UPDATES_ENABLED || process.env.ENABLE_DEV_UPDATES === 'true';
}

// Define temp directory for pasted images
const biorouterTempDir = path.join(app.getPath('temp'), 'biorouter-pasted-images');

function resolveImagePath(filename: string): string | undefined {
  return [
    path.join(process.resourcesPath, 'images', filename),
    path.join(process.cwd(), 'src', 'images', filename),
    path.join(__dirname, '..', 'images', filename),
    path.join(__dirname, 'images', filename),
    path.join(process.cwd(), 'images', filename),
  ].find((candidate) => fsSync.existsSync(candidate));
}

function expandBiorouterPath(filePath: string): string {
  const expandedPath = expandTilde(filePath);
  const pathRoot = process.env.BIOROUTER_PATH_ROOT;
  if (!pathRoot) return expandedPath;

  const defaultConfigDir = path.join(os.homedir(), '.config', 'biorouter');
  if (expandedPath === defaultConfigDir || expandedPath.startsWith(defaultConfigDir + path.sep)) {
    return path.join(pathRoot, 'config', path.relative(defaultConfigDir, expandedPath));
  }
  return expandedPath;
}

function allowedFileRoots(): string[] {
  return [
    os.homedir(),
    app.getPath('userData'),
    app.getPath('temp'),
    ...(process.env.BIOROUTER_PATH_ROOT ? [process.env.BIOROUTER_PATH_ROOT] : []),
  ];
}

function isAllowedFilePath(resolvedPath: string): boolean {
  const allowedRoots = allowedFileRoots();
  return allowedRoots.some(
    (root) => resolvedPath.startsWith(root + path.sep) || resolvedPath === root
  );
}

/**
 * Reject addresses that only exist inside the user's machine or LAN: the
 * biorouterd loopback API, cloud metadata at 169.254.169.254, printers, routers.
 */
export function isPrivateAddress(address: string): boolean {
  const family = isIP(address);
  if (family === 4) {
    const [a, b] = address.split('.').map(Number);
    if (a === 0 || a === 127 || a === 10) return true;
    if (a === 172 && b >= 16 && b <= 31) return true;
    if (a === 192 && b === 168) return true;
    if (a === 169 && b === 254) return true; // link-local, incl. cloud metadata
    if (a === 100 && b >= 64 && b <= 127) return true; // CGNAT
    if (a >= 224) return true; // multicast / reserved / broadcast
    return false;
  }
  if (family === 6) {
    const addr = address.toLowerCase();
    if (addr === '::' || addr === '::1') return true;
    if (addr.startsWith('fe8') || addr.startsWith('fe9')) return true;
    if (addr.startsWith('fea') || addr.startsWith('feb')) return true; // fe80::/10
    if (addr.startsWith('fc') || addr.startsWith('fd')) return true; // fc00::/7 ULA
    const mapped = addr.match(/^::ffff:(\d+\.\d+\.\d+\.\d+)$/);
    if (mapped) return isPrivateAddress(mapped[1]);
    return false;
  }
  return false;
}

/** Throws unless `candidate` is an http(s) URL whose host resolves off-machine. */
async function assertPublicHttpUrl(candidate: string): Promise<URL> {
  const parsed = new URL(candidate);
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
    throw new Error('Invalid URL protocol. Only HTTP and HTTPS are allowed.');
  }
  const host = parsed.hostname.replace(/^\[|\]$/g, '');
  if (isIP(host)) {
    if (isPrivateAddress(host)) throw new Error(`Blocked non-public address: ${host}`);
    return parsed;
  }
  const resolved = await dns.lookup(host, { all: true });
  if (resolved.some((entry) => isPrivateAddress(entry.address))) {
    throw new Error(`Blocked non-public address for host: ${host}`);
  }
  return parsed;
}

/** Largest artifact the previewer will read into memory. */
const ARTIFACT_PREVIEW_MAX_BYTES = 16 * 1024 * 1024;

/** Only http(s) may be handed to the OS opener. */
export function isExternallyOpenableUrl(candidate: string): boolean {
  if (candidate.length > 8 * 1024) return false;
  try {
    const url = new URL(candidate);
    return (
      (url.protocol === 'http:' || url.protocol === 'https:') &&
      url.username === '' &&
      url.password === ''
    );
  } catch {
    return false;
  }
}

function rendererEntryUrl(): URL {
  return MAIN_WINDOW_VITE_DEV_SERVER_URL
    ? new URL(MAIN_WINDOW_VITE_DEV_SERVER_URL)
    : pathToFileURL(path.join(__dirname, `../renderer/${MAIN_WINDOW_VITE_NAME}/index.html`));
}

function mimeTypeForArtifactPath(filePath: string): string {
  const ext = path.extname(filePath).toLowerCase();
  const mimeTypes: Record<string, string> = {
    '.css': 'text/css',
    '.csv': 'text/csv',
    '.gif': 'image/gif',
    '.htm': 'text/html',
    '.html': 'text/html',
    '.jpeg': 'image/jpeg',
    '.jpg': 'image/jpeg',
    '.js': 'text/javascript',
    '.json': 'application/json',
    '.ipynb': 'application/x-ipynb+json',
    '.md': 'text/markdown',
    '.pdf': 'application/pdf',
    '.png': 'image/png',
    '.docx': 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
    '.pptx': 'application/vnd.openxmlformats-officedocument.presentationml.presentation',
    '.xlsx': 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
    '.py': 'text/x-python',
    '.r': 'text/x-r',
    '.rs': 'text/rust',
    '.sh': 'text/x-shellscript',
    '.sql': 'application/sql',
    '.svg': 'image/svg+xml',
    '.toml': 'text/toml',
    '.ts': 'text/typescript',
    '.tsx': 'text/typescript',
    '.txt': 'text/plain',
    '.webp': 'image/webp',
    '.xml': 'application/xml',
    '.yaml': 'application/yaml',
    '.yml': 'application/yaml',
  };
  return mimeTypes[ext] || 'application/octet-stream';
}

function documentFormatForArtifactPath(filePath: string) {
  const formats = {
    '.pdf': 'pdf',
    '.docx': 'docx',
    '.xlsx': 'xlsx',
    '.pptx': 'pptx',
  } as const;
  return formats[path.extname(filePath).toLowerCase() as keyof typeof formats] ?? null;
}

function isTextArtifact(mimeType: string, buffer: Buffer): boolean {
  if (
    mimeType.startsWith('text/') ||
    mimeType.includes('json') ||
    mimeType.includes('xml') ||
    mimeType.includes('yaml') ||
    mimeType.includes('sql')
  ) {
    return true;
  }
  return !buffer.subarray(0, Math.min(buffer.length, 512)).includes(0);
}

// Function to ensure the temporary directory exists
async function ensureTempDirExists(): Promise<string> {
  try {
    // Check if the path already exists
    try {
      const stats = await fs.stat(biorouterTempDir);

      // If it exists but is not a directory, remove it and recreate
      if (!stats.isDirectory()) {
        await fs.unlink(biorouterTempDir);
        await fs.mkdir(biorouterTempDir, { recursive: true });
      }

      // Startup cleanup: remove old files and any symlinks
      const files = await fs.readdir(biorouterTempDir);
      const now = Date.now();
      const MAX_AGE = 24 * 60 * 60 * 1000; // 24 hours in milliseconds

      for (const file of files) {
        const filePath = path.join(biorouterTempDir, file);
        try {
          const fileStats = await fs.lstat(filePath);

          // Always remove symlinks
          if (fileStats.isSymbolicLink()) {
            console.warn(
              `[Main] Found symlink in temp directory during startup: ${filePath}. Removing it.`
            );
            await fs.unlink(filePath);
            continue;
          }

          // Remove old files (older than 24 hours)
          if (fileStats.isFile()) {
            const fileAge = now - fileStats.mtime.getTime();
            if (fileAge > MAX_AGE) {
              console.log(
                `[Main] Removing old temp file during startup: ${filePath} (age: ${Math.round(fileAge / (60 * 60 * 1000))} hours)`
              );
              await fs.unlink(filePath);
            }
          }
        } catch (fileError) {
          // If we can't stat the file, try to remove it anyway
          console.warn(`[Main] Could not stat file ${filePath}, attempting to remove:`, fileError);
          try {
            await fs.unlink(filePath);
          } catch (unlinkError) {
            console.error(`[Main] Failed to remove problematic file ${filePath}:`, unlinkError);
          }
        }
      }
    } catch (error) {
      if (error && typeof error === 'object' && 'code' in error && error.code === 'ENOENT') {
        // Directory doesn't exist, create it
        await fs.mkdir(biorouterTempDir, { recursive: true });
      } else {
        throw error;
      }
    }

    // Set proper permissions on the directory (0755 = rwxr-xr-x)
    await fs.chmod(biorouterTempDir, 0o755);

    console.log('[Main] Temporary directory for pasted images ensured:', biorouterTempDir);
  } catch (error) {
    console.error('[Main] Failed to create temp directory:', biorouterTempDir, error);
    throw error; // Propagate error
  }
  return biorouterTempDir;
}

async function configureProxy() {
  const httpsProxy = process.env.HTTPS_PROXY || process.env.https_proxy;
  const httpProxy = process.env.HTTP_PROXY || process.env.http_proxy;
  const noProxy = process.env.NO_PROXY || process.env.no_proxy || '';

  const proxyUrl = httpsProxy || httpProxy;

  if (proxyUrl) {
    console.log('[Main] Configuring proxy');
    await session.defaultSession.setProxy({
      proxyRules: proxyUrl,
      proxyBypassRules: noProxy,
    });
    console.log('[Main] Proxy configured successfully');
  }
}

if (started) app.quit();

// Global safety net: turn uncaught exceptions / unhandled rejections in the
// main process into logged diagnostics instead of bare brk #0 aborts. The
// default Node behavior tears the process down with no readable cause line
// in the crash report, which is what every recent Biorouter crash report
// has looked like. Logging here doesn't *prevent* the crash, but it
// guarantees the next .ips will have actionable context.
process.on('uncaughtException', (err, origin) => {
  try {
    log.error(`[Main] uncaughtException (${origin}):`, err);
  } catch {
    /* logger itself may be torn down — swallow */
  }
});
process.on('unhandledRejection', (reason) => {
  try {
    log.error('[Main] unhandledRejection:', reason);
  } catch {
    /* logger itself may be torn down — swallow */
  }
});

if (process.env.ENABLE_PLAYWRIGHT) {
  const cdpPort = process.env.PLAYWRIGHT_CDP_PORT ?? '9222';
  console.log(`[Main] Enabling Playwright remote debugging on port ${cdpPort}`);
  app.commandLine.appendSwitch('remote-debugging-port', cdpPort);
}

// Register as the handler for biorouter:// deep links.
//
// On macOS this maps the scheme to the *running bundle's* identifier. In a dev
// tree that bundle is `node_modules/electron/dist/Electron.app`
// (`com.github.Electron`) — a bare Electron shell with no app to run. Claiming
// the scheme from there permanently steals `biorouter://` from the installed
// app, and every subsequent link launches the shell, which exits immediately.
// So on macOS we only register from a packaged build.
//
// Windows/Linux resolve the handler by executable path rather than bundle id,
// and Electron's documented dev form (execPath + the app entry point) launches
// the real app, so registering there is both safe and useful.
if (process.platform === 'darwin') {
  if (app.isPackaged) {
    app.setAsDefaultProtocolClient('biorouter');
  } else {
    log.info(
      '[Main] Dev build on macOS: skipping biorouter:// registration so the installed app keeps the scheme'
    );
  }
} else if (app.isPackaged || !process.argv[1]) {
  app.setAsDefaultProtocolClient('biorouter');
} else {
  app.setAsDefaultProtocolClient('biorouter', process.execPath, [path.resolve(process.argv[1])]);
}

// Set as soon as we know a deep link is driving this launch, so appMain() does
// not also open an empty window. Declared here because the Windows/Linux argv
// path below claims the launch synchronously.
let openUrlHandledLaunch = false;

/** Deep links that open their own window rather than reusing an existing one. */
const WINDOW_OWNING_DEEPLINK_HOSTS = ['bot', 'workflow', 'diverge'];

// Apply single instance lock on Windows and Linux where it's needed for deep links
// macOS uses the 'open-url' event instead
let gotTheLock = true;
if (process.platform !== 'darwin') {
  gotTheLock = app.requestSingleInstanceLock();

  if (!gotTheLock) {
    app.quit();
  } else {
    app.on('second-instance', (_event, commandLine) => {
      const protocolUrl = commandLine.find((arg) => arg.startsWith('biorouter://'));
      if (protocolUrl) {
        let parsedUrl: URL;
        try {
          parsedUrl = new URL(protocolUrl);
        } catch (error) {
          log.error('[Main] Ignoring malformed deep link:', protocolUrl, error);
          return;
        }
        // Diverge: always open the branch in a fresh, focused window.
        if (parsedUrl.hostname === 'diverge') {
          app.whenReady().then(() => openDivergedWindow(parsedUrl));
          return;
        }
        // If it's a bot/workflow URL, handle it directly by creating a new window
        if (parsedUrl.hostname === 'bot' || parsedUrl.hostname === 'workflow') {
          app.whenReady().then(async () => {
            const recentDirs = loadRecentDirs();
            const openDir = recentDirs.length > 0 ? recentDirs[0] : null;

            const deeplinkData = parseWorkflowDeeplink(protocolUrl);
            const scheduledJobId = parsedUrl.searchParams.get('scheduledJob');

            createChat(
              app,
              undefined,
              openDir || undefined,
              undefined,
              undefined,
              undefined,
              deeplinkData?.config,
              scheduledJobId || undefined,
              undefined,
              deeplinkData?.parameters
            );
          });
          return; // Skip the rest of the handler
        }

        // For non-bot URLs, continue with normal handling
        handleProtocolUrl(protocolUrl);
      }

      // Only focus existing windows for non-bot/workflow URLs
      const existingWindows = BrowserWindow.getAllWindows();
      if (existingWindows.length > 0) {
        const mainWindow = existingWindows[0];
        if (mainWindow.isMinimized()) {
          mainWindow.restore();
        }
        mainWindow.focus();
      }
    });
  }

  // Handle protocol URLs on Windows and Linux startup
  const protocolUrl = process.argv.find((arg) => arg.startsWith('biorouter://'));
  if (protocolUrl) {
    try {
      if (WINDOW_OWNING_DEEPLINK_HOSTS.includes(new URL(protocolUrl).hostname)) {
        openUrlHandledLaunch = true;
      }
    } catch (error) {
      log.error('[Main] Ignoring malformed deep link argument:', protocolUrl, error);
    }
    app.whenReady().then(() => {
      handleProtocolUrl(protocolUrl);
    });
  }

  // Check if launched with a .brxt file argument (Windows/Linux double-click)
  const brxtArg = process.argv.slice(1).find((arg) => arg.endsWith('.brxt'));
  if (brxtArg) {
    app.whenReady().then(() => handleBrxtFileOpen(brxtArg));
  }
}

let firstOpenWindow: BrowserWindow;
let pendingDeepLink: string | null = null;
let pendingBrxtFilePath: string | null = null;

/**
 * A window-owning deep link claims the launch, so appMain() will not open its
 * own window. If the link then fails to produce one — a malformed URL, a
 * backend that won't start — the app would sit running with nothing on screen,
 * which is indistinguishable from "clicking the link quit Biorouter". Always
 * leave the user with a window.
 */
async function ensureWindowAfterDeepLink(openDir?: string | null) {
  if (BrowserWindow.getAllWindows().length > 0) return;
  log.warn('[Main] Deep link produced no window; opening a plain one instead');
  await createNewWindow(app, openDir || undefined);
}

async function handleProtocolUrl(url: string) {
  if (!url) return;

  pendingDeepLink = url;

  let parsedUrl: URL;
  try {
    parsedUrl = new URL(url);
  } catch (error) {
    log.error('[Main] Ignoring malformed deep link:', url, error);
    pendingDeepLink = null;
    await ensureWindowAfterDeepLink();
    return;
  }
  const recentDirs = loadRecentDirs();
  const openDir = recentDirs.length > 0 ? recentDirs[0] : null;

  // Diverge: always open the branch in a fresh, focused window.
  if (parsedUrl.hostname === 'diverge') {
    pendingDeepLink = null;
    try {
      await openDivergedWindow(parsedUrl);
    } catch (error) {
      log.error('[Main] Failed to open diverge deep link:', error);
    }
    await ensureWindowAfterDeepLink(openDir);
    return;
  }

  if (parsedUrl.hostname === 'bot' || parsedUrl.hostname === 'workflow') {
    // processProtocolUrl always opens its own window for these, so don't create
    // a throwaway one first — that left a stray empty window on cold launches.
    try {
      await processProtocolUrl(parsedUrl, null);
    } catch (error) {
      log.error('[Main] Failed to open workflow deep link:', error);
    }
    await ensureWindowAfterDeepLink(openDir);
  } else {
    // For other URL types, reuse existing window if available
    const existingWindows = BrowserWindow.getAllWindows();
    if (existingWindows.length > 0) {
      firstOpenWindow = existingWindows[0];
      if (firstOpenWindow.isMinimized()) {
        firstOpenWindow.restore();
      }
      firstOpenWindow.focus();
    } else {
      firstOpenWindow = await createChat(app, undefined, openDir || undefined);
    }

    if (firstOpenWindow) {
      const webContents = firstOpenWindow.webContents;
      if (webContents.isLoadingMainFrame()) {
        webContents.once('did-finish-load', async () => {
          await processProtocolUrl(parsedUrl, firstOpenWindow);
        });
      } else {
        await processProtocolUrl(parsedUrl, firstOpenWindow);
      }
    }
  }
}

// `window` is null for bot/workflow URLs, which always open a window of their own.
async function processProtocolUrl(parsedUrl: URL, window: BrowserWindow | null) {
  const recentDirs = loadRecentDirs();
  const openDir = recentDirs.length > 0 ? recentDirs[0] : null;

  if (parsedUrl.hostname === 'extension') {
    window?.webContents.send('add-extension', pendingDeepLink);
  } else if (parsedUrl.hostname === 'sessions') {
    window?.webContents.send('open-shared-session', pendingDeepLink);
  } else if (parsedUrl.hostname === 'bot' || parsedUrl.hostname === 'workflow') {
    const deeplinkData = parseWorkflowDeeplink(pendingDeepLink ?? parsedUrl.toString());
    const scheduledJobId = parsedUrl.searchParams.get('scheduledJob');

    // Opens its own window; the `window` argument is deliberately unused here.
    // Awaited so callers can tell whether a window actually appeared.
    await createChat(
      app,
      undefined,
      openDir || undefined,
      undefined,
      undefined,
      undefined,
      deeplinkData?.config,
      scheduledJobId || undefined,
      undefined,
      deeplinkData?.parameters
    );
    pendingDeepLink = null;
  }
}

let windowDeeplinkURL: string | null = null;

app.on('open-url', async (_event, url) => {
  if (process.platform !== 'win32') {
    let parsedUrl: URL;
    try {
      parsedUrl = new URL(url);
    } catch (error) {
      log.error('[Main] Ignoring malformed deep link:', url, error);
      return;
    }

    log.info('[Main] Received open-url event:', url);

    // On a cold launch macOS emits open-url before `ready`, so this handler and
    // appMain both wait on the same whenReady() promise — and appMain's
    // continuation was queued first. Claim the launch *now*, synchronously,
    // otherwise appMain sees the flag still false and opens a redundant empty
    // window alongside the one this handler is about to create.
    if (!app.isReady() && WINDOW_OWNING_DEEPLINK_HOSTS.includes(parsedUrl.hostname)) {
      openUrlHandledLaunch = true;
    }

    await app.whenReady();

    const recentDirs = loadRecentDirs();
    const openDir = recentDirs.length > 0 ? recentDirs[0] : null;

    // Diverge: always open the branch in a fresh, focused window.
    if (parsedUrl.hostname === 'diverge') {
      log.info('[Main] Detected diverge URL, opening branch in a new window');
      openUrlHandledLaunch = true;
      try {
        await openDivergedWindow(parsedUrl);
      } catch (error) {
        log.error('[Main] Failed to open diverge deep link:', error);
      }
      await ensureWindowAfterDeepLink(openDir);
      return;
    }

    // Handle bot/workflow URLs by directly creating a new window
    if (parsedUrl.hostname === 'bot' || parsedUrl.hostname === 'workflow') {
      log.info('[Main] Detected bot/workflow URL, creating new chat window');
      openUrlHandledLaunch = true;
      const deeplinkData = parseWorkflowDeeplink(url);
      if (deeplinkData) {
        windowDeeplinkURL = url;
      }
      const scheduledJobId = parsedUrl.searchParams.get('scheduledJob');

      try {
        await createChat(
          app,
          undefined,
          openDir || undefined,
          undefined,
          undefined,
          undefined,
          deeplinkData?.config,
          scheduledJobId || undefined,
          undefined,
          deeplinkData?.parameters
        );
      } catch (error) {
        log.error('[Main] Failed to open workflow deep link:', error);
      } finally {
        windowDeeplinkURL = null;
      }
      await ensureWindowAfterDeepLink(openDir);
      return;
    }

    // For extension/session URLs, store the deep link for processing after React is ready
    pendingDeepLink = url;
    log.info('[Main] Stored pending deep link for processing after React ready:', url);

    const existingWindows = BrowserWindow.getAllWindows();
    if (existingWindows.length > 0) {
      firstOpenWindow = existingWindows[0];
      if (firstOpenWindow.isMinimized()) firstOpenWindow.restore();
      firstOpenWindow.focus();
      if (parsedUrl.hostname === 'extension') {
        firstOpenWindow.webContents.send('add-extension', pendingDeepLink);
        pendingDeepLink = null;
      } else if (parsedUrl.hostname === 'sessions') {
        firstOpenWindow.webContents.send('open-shared-session', pendingDeepLink);
        pendingDeepLink = null;
      }
    } else {
      openUrlHandledLaunch = true;
      firstOpenWindow = await createChat(app, undefined, openDir || undefined);
    }
  }
});

// Handle macOS drag-and-drop onto dock icon
app.on('will-finish-launching', () => {
  if (process.platform === 'darwin') {
    app.setAboutPanelOptions({
      applicationName: 'Biorouter',
      applicationVersion: app.getVersion(),
    });
  }
});

// Handle drag-and-drop onto dock icon
app.on('open-file', async (event, filePath) => {
  event.preventDefault();
  if (filePath.endsWith('.brxt')) {
    if (app.isReady()) {
      handleBrxtFileOpen(filePath);
    } else {
      app.whenReady().then(() => handleBrxtFileOpen(filePath));
    }
    return;
  }
  await handleFileOpen(filePath);
});

// Handle multiple files/folders (macOS only)
if (process.platform === 'darwin') {
  // Use type assertion for non-standard Electron event
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  app.on('open-files' as any, async (event: any, filePaths: string[]) => {
    event.preventDefault();
    for (const filePath of filePaths) {
      await handleFileOpen(filePath);
    }
  });
}

async function handleFileOpen(filePath: string) {
  try {
    if (!filePath || typeof filePath !== 'string') {
      return;
    }

    const stats = fsSync.lstatSync(filePath);
    let targetDir = filePath;

    // If it's a file, use its parent directory
    if (stats.isFile()) {
      targetDir = path.dirname(filePath);
    }

    // Add to recent directories
    addRecentDir(targetDir);

    // Create new window for the directory
    const newWindow = await createChat(app, undefined, targetDir);

    // Focus the new window
    if (newWindow) {
      newWindow.show();
      newWindow.focus();
      newWindow.moveTop();
    }
  } catch (error) {
    console.error('Failed to handle file open:', error);

    // Show user-friendly error notification
    new Notification({
      title: 'Biorouter',
      body: `Could not open directory: ${path.basename(filePath)}`,
    }).show();
  }
}

declare var MAIN_WINDOW_VITE_DEV_SERVER_URL: string;
declare var MAIN_WINDOW_VITE_NAME: string;

// State for environment variable toggles
let envToggles: EnvToggles = loadSettings().envToggles;

// Parse command line arguments
const parseArgs = () => {
  let dirPath = null;

  // Remove first two elements in dev mode (electron and script path)
  const args = !dirPath && app.isPackaged ? process.argv : process.argv.slice(2);
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--dir' && i + 1 < args.length) {
      dirPath = args[i + 1];
      break;
    }
  }

  return { dirPath };
};

interface BundledConfig {
  defaultProvider?: string;
  defaultModel?: string;
  predefinedModels?: string;
  baseUrlShare?: string;
  version?: string;
}

const getBundledConfig = (): BundledConfig => {
  //{env-macro-start}//
  //needed when biorouter is bundled for a specific provider
  //{env-macro-end}//
  return {
    defaultProvider: process.env.BIOROUTER_DEFAULT_PROVIDER,
    defaultModel: process.env.BIOROUTER_DEFAULT_MODEL,
    predefinedModels: process.env.BIOROUTER_PREDEFINED_MODELS,
    baseUrlShare: process.env.BIOROUTER_BASE_URL_SHARE,
    version: process.env.BIOROUTER_VERSION,
  };
};

const { defaultProvider, defaultModel, predefinedModels, baseUrlShare, version } =
  getBundledConfig();

const GENERATED_SECRET = crypto.randomBytes(32).toString('hex');

const getServerSecret = (settings: ReturnType<typeof loadSettings>): string => {
  if (settings.externalBiorouterd?.enabled && settings.externalBiorouterd.secret) {
    return settings.externalBiorouterd.secret;
  }
  if (process.env.BIOROUTER_EXTERNAL_BACKEND) {
    return 'test';
  }
  return GENERATED_SECRET;
};

let appConfig = {
  BIOROUTER_DEFAULT_PROVIDER: defaultProvider,
  BIOROUTER_DEFAULT_MODEL: defaultModel,
  BIOROUTER_PREDEFINED_MODELS: predefinedModels,
  BIOROUTER_API_HOST: 'http://127.0.0.1',
  BIOROUTER_WORKING_DIR: '',
  // If BIOROUTER_ALLOWLIST_WARNING env var is not set, defaults to false (strict blocking mode)
  BIOROUTER_ALLOWLIST_WARNING: process.env.BIOROUTER_ALLOWLIST_WARNING === 'true',
};

const windowMap = new Map<number, BrowserWindow>();
const biorouterdClients = new Map<number, Client>();

const trackArtifactPreviewFrames = (contents: Electron.WebContents) => {
  const frameIds = new Set<string>();
  contents.on('frame-created', (_event, { frame }) => {
    if (frame?.name === 'biorouter-artifact-preview') {
      frameIds.add(`${frame.processId}:${frame.routingId}`);
    }
  });
  return (frame: Electron.WebFrameMain | null | undefined) => {
    let current = frame;
    while (current) {
      if (
        current.name === 'biorouter-artifact-preview' ||
        frameIds.has(`${current.processId}:${current.routingId}`)
      ) {
        return true;
      }
      current = current.parent;
    }
    return false;
  };
};

// A chat window and every Agent Drafter app window it launches share ONE
// biorouterd (launch-app reuses the launching window's client). The backend
// must outlive any single dependent window, so it is ref-counted and killed
// only when the LAST window using it closes. Without this, closing the chat
// window tore down the backend an app window it launched was still using, and
// nothing respawned it — the app window then silently failed every call.
// `app.on('will-quit')` in biorouterd.ts still sweeps every backend on quit,
// so nothing leaks when the app exits.
const windowBackends = new Map<number, ChildProcess>(); // windowId -> its backend
const backendRefCounts = new Map<ChildProcess, number>(); // backend -> live windows

const retainBackend = (windowId: number, proc: ChildProcess) => {
  windowBackends.set(windowId, proc);
  backendRefCounts.set(proc, (backendRefCounts.get(proc) ?? 0) + 1);
};

const releaseBackend = (windowId: number) => {
  const proc = windowBackends.get(windowId);
  if (!proc) return;
  windowBackends.delete(windowId);
  const remaining = (backendRefCounts.get(proc) ?? 1) - 1;
  if (remaining > 0) {
    backendRefCounts.set(proc, remaining);
    return; // other windows still depend on this backend
  }
  backendRefCounts.delete(proc);
  if (typeof proc === 'object' && 'kill' in proc) {
    proc.kill(); // last dependent window closed -> safe to terminate
  }
};

// Track power save blockers per window
const windowPowerSaveBlockers = new Map<number, number>(); // windowId -> blockerId
// Track pending initial messages per window
const pendingInitialMessages = new Map<number, string>(); // windowId -> initialMessage

interface ChatWindowOptions {
  initialBounds?: Rectangle;
  show?: boolean;
  manageWindowState?: boolean;
}

const createChat = async (
  app: App,
  initialMessage?: string,
  dir?: string,
  _version?: string,
  resumeSessionId?: string,
  viewType?: string,
  workflowDeeplink?: string, // Raw deeplink decoded on server
  scheduledJobId?: string, // Scheduled job ID if applicable
  workflowId?: string,
  workflowParameters?: Record<string, string>, // Workflow parameter values from deeplink URL
  windowOptions?: ChatWindowOptions
) => {
  updateEnvironmentVariables(envToggles);

  const settings = loadSettings();
  const serverSecret = getServerSecret(settings);

  // BR-54 Slice A: share ONE daemon across all windows (default). The daemon is
  // already a session-keyed singleton, so its spawn cwd is just a fallback —
  // start it at the home dir and let each window carry its own working directory
  // to its session via REQUEST_DIR / BIOROUTER_WORKING_DIR (`windowWorkingDir`
  // below), which is unchanged. Set BIOROUTER_SHARED_DAEMON=0 to revert to the
  // previous per-window daemon.
  const useSharedDaemon = isSharedDaemonEnabled();
  const windowWorkingDir = path.resolve(path.normalize(dir || os.homedir()));

  const biorouterdResult = useSharedDaemon
    ? await getSharedBackend(startBiorouterd, {
        app,
        serverSecret,
        dir: os.homedir(),
        env: { BIOROUTER_PATH_ROOT: process.env.BIOROUTER_PATH_ROOT },
        externalBiorouterd: settings.externalBiorouterd,
      })
    : await startBiorouterd({
        app,
        serverSecret,
        dir: dir || os.homedir(),
        env: { BIOROUTER_PATH_ROOT: process.env.BIOROUTER_PATH_ROOT },
        externalBiorouterd: settings.externalBiorouterd,
      });

  const { baseUrl, process: biorouterdProcess, errorLog } = biorouterdResult;
  // Per-window working dir — NOT the shared daemon's spawn cwd. In the
  // per-window (non-shared) path this equals biorouterdResult.workingDir.
  const workingDir = windowWorkingDir;

  const mainWindowState = windowStateKeeper({
    // First-launch size (windowStateKeeper remembers the user's own size after
    // that). Sized so the Home view opens with the usage heatmap AND the recent
    // chats both visible above the composer, rather than the heatmap alone.
    defaultWidth: 1440,
    defaultHeight: 1000,
  });
  const initialBounds = windowOptions?.initialBounds;

  const mainWindow = new BrowserWindow({
    titleBarStyle: process.platform === 'darwin' ? 'hidden' : 'default',
    trafficLightPosition: process.platform === 'darwin' ? { x: 20, y: 16 } : undefined,
    vibrancy: process.platform === 'darwin' ? 'window' : undefined,
    frame: process.platform !== 'darwin',
    x: initialBounds?.x ?? mainWindowState.x,
    y: initialBounds?.y ?? mainWindowState.y,
    width: initialBounds?.width ?? mainWindowState.width,
    height: initialBounds?.height ?? mainWindowState.height,
    minWidth: 800,
    minHeight: 600,
    resizable: true,
    useContentSize: true,
    show: windowOptions?.show ?? true,
    icon: resolveImagePath(
      process.platform === 'win32'
        ? 'icon.ico'
        : process.platform === 'darwin'
          ? 'icon.icns'
          : 'icon.png'
    ),
    webPreferences: {
      spellcheck: settings.spellcheckEnabled ?? true,
      preload: path.join(__dirname, 'preload.js'),
      webSecurity: true,
      // Throttle timers/rAF/reconciliation in backgrounded windows. This is
      // Electron's default; set explicitly so a future window-pooling change
      // can't silently lose it (each project window is a full renderer process).
      backgroundThrottling: true,
      nodeIntegration: false,
      contextIsolation: true,
      additionalArguments: [
        JSON.stringify({
          ...appConfig,
          BIOROUTER_API_HOST: baseUrl,
          BIOROUTER_WORKING_DIR: workingDir,
          REQUEST_DIR: dir,
          BIOROUTER_BASE_URL_SHARE: baseUrlShare,
          BIOROUTER_VERSION: version,
          workflowId: workflowId,
          workflowDeeplink: workflowDeeplink,
          workflowParameters: workflowParameters,
          scheduledJobId: scheduledJobId,
          SECURITY_ML_MODEL_MAPPING: process.env.SECURITY_ML_MODEL_MAPPING,
        }),
      ],
      partition: 'persist:biorouter',
    },
  });

  if (!app.isPackaged) {
    installExtension(REACT_DEVELOPER_TOOLS, {
      loadExtensionOptions: { allowFileAccess: true },
      session: mainWindow.webContents.session,
    })
      .then(() => log.info('added react dev tools'))
      .catch((err) => log.info('failed to install react dev tools:', err));
  }

  const biorouterdClient = createClient(
    createConfig({
      baseUrl,
      headers: {
        'Content-Type': 'application/json',
        'X-Secret-Key': serverSecret,
      },
    })
  );
  biorouterdClients.set(mainWindow.id, biorouterdClient);
  // With a shared daemon the backend is app-lifetime (killed only in
  // startBiorouterd's own `will-quit` sweep), so windows must NOT ref-count it —
  // closing one window must not tear the daemon out from under the others. The
  // per-window ref-count is only for the (opt-out) per-window daemon path.
  if (!useSharedDaemon) {
    retainBackend(mainWindow.id, biorouterdProcess);
  }

  const serverReady = await checkServerStatus(biorouterdClient, errorLog);
  if (!serverReady) {
    const isUsingExternalBackend = settings.externalBiorouterd?.enabled;

    if (isUsingExternalBackend) {
      const response = dialog.showMessageBoxSync({
        type: 'error',
        title: 'External Backend Unreachable',
        message: `Could not connect to external backend at ${settings.externalBiorouterd?.url}`,
        detail: 'The external biorouterd server may not be running.',
        buttons: ['Disable External Backend & Retry', 'Quit'],
        defaultId: 0,
        cancelId: 1,
      });

      if (response === 0) {
        const updatedSettings = {
          ...settings,
          externalBiorouterd: {
            enabled: false,
            url: settings.externalBiorouterd?.url || '',
            secret: settings.externalBiorouterd?.secret || '',
          },
        };
        saveSettings(updatedSettings);
        // The shared daemon was started against the now-disabled external
        // config; forget it so the retry starts a fresh local daemon.
        resetSharedBackend();
        mainWindow.destroy();
        return createChat(app, initialMessage, dir);
      }
    } else {
      dialog.showMessageBoxSync({
        type: 'error',
        title: 'Biorouter Failed to Start',
        message: 'The backend server failed to start.',
        detail: errorLog.join('\n'),
        buttons: ['OK'],
      });
    }
    app.quit();
  }

  if (windowOptions?.manageWindowState !== false) {
    mainWindowState.manage(mainWindow);
  }

  mainWindow.webContents.session.setSpellCheckerLanguages(['en-US', 'en-GB']);
  mainWindow.webContents.on('context-menu', (_event, params) => {
    const menu = new Menu();
    const hasSpellingSuggestions = params.dictionarySuggestions.length > 0 || params.misspelledWord;

    if (hasSpellingSuggestions) {
      for (const suggestion of params.dictionarySuggestions) {
        menu.append(
          new MenuItem({
            label: suggestion,
            click: () => mainWindow.webContents.replaceMisspelling(suggestion),
          })
        );
      }

      if (params.misspelledWord) {
        menu.append(
          new MenuItem({
            label: 'Add to dictionary',
            click: () =>
              mainWindow.webContents.session.addWordToSpellCheckerDictionary(params.misspelledWord),
          })
        );
      }

      if (params.selectionText) {
        menu.append(new MenuItem({ type: 'separator' }));
      }
    }
    if (params.selectionText) {
      menu.append(
        new MenuItem({
          label: 'Cut',
          accelerator: 'CmdOrCtrl+X',
          role: 'cut',
        })
      );
      menu.append(
        new MenuItem({
          label: 'Copy',
          accelerator: 'CmdOrCtrl+C',
          role: 'copy',
        })
      );
    }

    // Only show paste in editable fields (text inputs)
    if (params.isEditable) {
      menu.append(
        new MenuItem({
          label: 'Paste',
          accelerator: 'CmdOrCtrl+V',
          role: 'paste',
        })
      );
    }

    if (menu.items.length > 0) {
      menu.popup();
    }
  });

  // Handle new window creation for links.
  //
  // Deny by default. An `allow` here would open a BrowserWindow that inherits
  // this window's webPreferences -- including the preload IPC bridge -- and
  // non-http(s) schemes (`data:`, `blob:`, `about:`) receive no CSP, since the
  // CSP is injected by onHeadersReceived. Agent-authored artifact HTML must
  // never reach such a window.
  mainWindow.webContents.setWindowOpenHandler(({ url }) => {
    if (shouldOpenExternalNavigation(url, rendererEntryUrl())) {
      shell.openExternal(url);
    }
    return { action: 'deny' };
  });

  // Handle new-window events (alternative approach for external links)
  // Use type assertion for non-standard Electron event
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  mainWindow.webContents.on('new-window' as any, function (event: any, url: string) {
    event.preventDefault();
    // Unlike setWindowOpenHandler above, this legacy path used to hand any
    // scheme -- including file:// and custom protocols -- to the OS opener.
    if (shouldOpenExternalNavigation(url, rendererEntryUrl())) {
      shell.openExternal(url);
    }
  });

  // Nothing in this app navigates the top frame away from its own origin. A
  // file:// or data: navigation would keep the preload bridge and get no CSP.
  const blockOffOriginNavigation = (event: Electron.Event, url: string) => {
    if (isAppOrigin(url, rendererEntryUrl())) return;
    log.warn('[Main] Blocked off-origin navigation to', url);
    event.preventDefault();
    if (isExternallyOpenableUrl(url)) {
      shell.openExternal(url);
    }
  };
  mainWindow.webContents.on('will-navigate', blockOffOriginNavigation);
  const isArtifactPreviewFrame = trackArtifactPreviewFrames(mainWindow.webContents);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  mainWindow.webContents.on('will-frame-navigate' as any, (event: any) => {
    if (event.isMainFrame) {
      blockOffOriginNavigation(event, event.url);
      return;
    }
    if (
      isArtifactPreviewFrame(event.frame) &&
      !isAllowedArtifactFrameNavigation(event.url, biorouterdClient.getConfig().baseUrl as string)
    ) {
      log.warn('[Main] Blocked artifact frame navigation to', event.url);
      event.preventDefault();
    }
  });

  const windowId = mainWindow.id;
  const url = MAIN_WINDOW_VITE_DEV_SERVER_URL
    ? new URL(MAIN_WINDOW_VITE_DEV_SERVER_URL)
    : pathToFileURL(path.join(__dirname, `../renderer/${MAIN_WINDOW_VITE_NAME}/index.html`));

  let appPath = '/';
  const routeMap: Record<string, string> = {
    chat: '/',
    pair: '/pair',
    settings: '/settings',
    sessions: '/sessions',
    schedules: '/schedules',
    workflows: '/workflows',
    permission: '/permission',
    ConfigureProviders: '/configure-providers',
    sharedSession: '/shared-session',
    welcome: '/welcome',
  };

  if (viewType) {
    appPath = routeMap[viewType] || '/';
  }
  if (
    appPath === '/' &&
    (workflowDeeplink !== undefined || workflowId !== undefined || initialMessage)
  ) {
    appPath = '/pair';
  }

  let searchParams = new URLSearchParams();
  if (resumeSessionId) {
    searchParams.set('resumeSessionId', resumeSessionId);
    if (appPath === '/') {
      appPath = '/pair';
    }
  }
  // Only add workflowId to URL for the non-deeplink case (saved workflows launched from UI)
  // For deeplinks, the workflow object is passed via appConfig, not URL params
  if (workflowId) {
    searchParams.set('workflowId', workflowId);
    if (appPath === '/') {
      appPath = '/pair';
    }
  }

  // Biorouter's react app uses HashRouter, so the path + search params follow a #/
  url.hash = `${appPath}?${searchParams.toString()}`;
  let formattedUrl = formatUrl(url);
  log.info('Opening URL: ', formattedUrl);
  mainWindow.loadURL(formattedUrl);

  // If we have an initial message, store it to send after React is ready
  if (initialMessage) {
    pendingInitialMessages.set(mainWindow.id, initialMessage);
  }

  // Set up local keyboard shortcuts that only work when the window is focused
  mainWindow.webContents.on('before-input-event', (event, input) => {
    if (input.key === 'r' && input.meta) {
      mainWindow.reload();
      event.preventDefault();
    }

    if (input.key === 'i' && input.alt && input.meta) {
      mainWindow.webContents.openDevTools();
      event.preventDefault();
    }
  });

  mainWindow.on('app-command', (e, cmd) => {
    if (cmd === 'browser-backward') {
      mainWindow.webContents.send('mouse-back-button-clicked');
      e.preventDefault();
    }
  });

  // Handle mouse back button (button 3)
  // Use type assertion for non-standard Electron event
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  mainWindow.webContents.on('mouse-up' as any, function (_event: any, mouseButton: number) {
    // MouseButton 3 is the back button.
    if (mouseButton === 3) {
      mainWindow.webContents.send('mouse-back-button-clicked');
    }
  });

  windowMap.set(windowId, mainWindow);

  // Handle window closure
  mainWindow.on('closed', () => {
    windowMap.delete(windowId);

    // Clean up pending initial message
    pendingInitialMessages.delete(windowId);

    if (windowPowerSaveBlockers.has(windowId)) {
      const blockerId = windowPowerSaveBlockers.get(windowId)!;
      try {
        powerSaveBlocker.stop(blockerId);
        console.log(
          `[Main] Stopped power save blocker ${blockerId} for closing window ${windowId}`
        );
      } catch (error) {
        console.error(
          `[Main] Failed to stop power save blocker ${blockerId} for window ${windowId}:`,
          error
        );
      }
      windowPowerSaveBlockers.delete(windowId);
    }

    // Kill this window's backend only if no other window (e.g. an Agent Drafter
    // app window this chat launched) still shares it.
    releaseBackend(windowId);
  });
  return mainWindow;
};

/**
 * Open a diverged (branched) session in a NEW, focused window, leaving every
 * existing window exactly in place. Backs the `biorouter://diverge` deeplink
 * (CLI/TUI `/diverge`) and mirrors the in-app Diverge button. The new window is
 * offset from the focused one so the user can see it's a distinct, second
 * window rather than a silent in-place clone.
 */
async function openDivergedWindow(parsedUrl: URL): Promise<void> {
  const sessionId = parsedUrl.searchParams.get('session_id') || undefined;
  if (!sessionId) {
    log.error('[Main] diverge deeplink missing session_id:', parsedUrl.toString());
    return;
  }
  const recentDirs = loadRecentDirs();
  const dir =
    parsedUrl.searchParams.get('dir') ||
    (recentDirs.length > 0 ? recentDirs[0] : undefined) ||
    undefined;

  await openDivergedChatWindow(sessionId, dir);
}

function branchWindowBounds(anchor?: BrowserWindow | null): Rectangle | undefined {
  if (!anchor || anchor.isDestroyed()) return undefined;

  const anchorBounds = anchor.getBounds();
  const display = screen.getDisplayMatching(anchorBounds);
  const workArea = display.workArea;
  const width = Math.min(anchorBounds.width, workArea.width);
  const height = Math.min(anchorBounds.height, workArea.height);
  let x = anchorBounds.x + 40;
  let y = anchorBounds.y + 40;

  if (x + width > workArea.x + workArea.width) {
    x = Math.max(workArea.x, anchorBounds.x - 40);
  }
  if (y + height > workArea.y + workArea.height) {
    y = Math.max(workArea.y, anchorBounds.y - 40);
  }

  return { x, y, width, height };
}

async function openDivergedChatWindow(
  sessionId: string,
  dir?: string,
  sourceWindow?: BrowserWindow | null
): Promise<void> {
  const anchor = BrowserWindow.getFocusedWindow() ?? BrowserWindow.getAllWindows()[0];
  const bounds = branchWindowBounds(sourceWindow ?? anchor);
  const win = await createChat(
    app,
    undefined,
    dir,
    undefined,
    sessionId,
    'pair',
    undefined,
    undefined,
    undefined,
    undefined,
    bounds ? { initialBounds: bounds, show: false, manageWindowState: false } : undefined
  );
  if (win) {
    win.show();
    win.focus();
    win.moveTop();
  }
}

const createLauncher = () => {
  const launcherWindow = new BrowserWindow({
    width: 600,
    height: 80,
    frame: false,
    transparent: process.platform === 'darwin',
    backgroundColor: process.platform === 'darwin' ? '#00000000' : '#ffffff',
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      nodeIntegration: false,
      contextIsolation: true,
      additionalArguments: [JSON.stringify(appConfig)],
      partition: 'persist:biorouter',
    },
    skipTaskbar: true,
    alwaysOnTop: true,
    resizable: false,
    movable: true,
    minimizable: false,
    maximizable: false,
    fullscreenable: false,
    hasShadow: true,
    vibrancy: process.platform === 'darwin' ? 'window' : undefined,
  });

  // Center on screen
  const primaryDisplay = screen.getPrimaryDisplay();
  const { width, height } = primaryDisplay.workAreaSize;
  const windowBounds = launcherWindow.getBounds();

  launcherWindow.setPosition(
    Math.round(width / 2 - windowBounds.width / 2),
    Math.round(height / 3 - windowBounds.height / 2)
  );

  // Load launcher window content
  const url = MAIN_WINDOW_VITE_DEV_SERVER_URL
    ? new URL(MAIN_WINDOW_VITE_DEV_SERVER_URL)
    : pathToFileURL(path.join(__dirname, `../renderer/${MAIN_WINDOW_VITE_NAME}/index.html`));

  url.hash = '/launcher';
  launcherWindow.loadURL(formatUrl(url));

  // Destroy window when it loses focus
  launcherWindow.on('blur', () => {
    launcherWindow.destroy();
  });

  // Also destroy on escape key
  launcherWindow.webContents.on('before-input-event', (event, input) => {
    if (input.key === 'Escape') {
      launcherWindow.destroy();
      event.preventDefault();
    }
  });

  return launcherWindow;
};

// Track tray instance
let tray: Tray | null = null;

const destroyTray = () => {
  if (tray) {
    tray.destroy();
    tray = null;
  }
};

const disableTray = () => {
  const settings = loadSettings();
  settings.showMenuBarIcon = false;
  saveSettings(settings);
};

const createTray = () => {
  destroyTray();

  const possiblePaths = [
    path.join(process.resourcesPath, 'images', 'iconTemplate.png'),
    path.join(process.cwd(), 'src', 'images', 'iconTemplate.png'),
    path.join(__dirname, '..', 'images', 'iconTemplate.png'),
    path.join(__dirname, 'images', 'iconTemplate.png'),
    path.join(process.cwd(), 'images', 'iconTemplate.png'),
  ];

  const iconPath = possiblePaths.find((p) => fsSync.existsSync(p));

  if (!iconPath) {
    console.warn('[Main] Tray icon not found. App will continue without system tray.');
    disableTray();
    return;
  }

  try {
    tray = new Tray(iconPath);
    setTrayRef(tray);
    updateTrayMenu(getUpdateAvailable());

    if (process.platform === 'win32' || process.platform === 'darwin') {
      tray.on('click', showWindow);
    }
    if (process.platform === 'darwin') {
      tray.on('right-click', () => {
        popUpTrayMenu();
      });
    }
  } catch (error) {
    console.error('[Main] Tray creation failed. App will continue without system tray.', error);
    disableTray();
    tray = null;
  }
};

const showWindow = async () => {
  const windows = BrowserWindow.getAllWindows();

  if (windows.length === 0) {
    log.info('No windows are open, creating a new one...');
    const recentDirs = loadRecentDirs();
    const openDir = recentDirs.length > 0 ? recentDirs[0] : null;
    await createChat(app, undefined, openDir || undefined);
    return;
  }

  const initialOffsetX = 30;
  const initialOffsetY = 30;

  // Iterate over all windows
  windows.forEach((win, index) => {
    const currentBounds = win.getBounds();
    const newX = currentBounds.x + initialOffsetX * index;
    const newY = currentBounds.y + initialOffsetY * index;

    win.setBounds({
      x: newX,
      y: newY,
      width: currentBounds.width,
      height: currentBounds.height,
    });

    if (!win.isVisible()) {
      win.show();
    }

    win.focus();
  });
};

const buildRecentFilesMenu = () => {
  const recentDirs = loadRecentDirs();
  return recentDirs.map((dir) => ({
    label: dir,
    click: () => {
      createChat(app, undefined, dir);
    },
  }));
};

const openDirectoryDialog = async (): Promise<OpenDialogReturnValue> => {
  // Get the current working directory from the focused window
  let defaultPath: string | undefined;
  const currentWindow = BrowserWindow.getFocusedWindow();

  if (currentWindow) {
    try {
      const currentWorkingDir = await currentWindow.webContents.executeJavaScript(
        `window.appConfig ? window.appConfig.get('BIOROUTER_WORKING_DIR') : null`
      );

      if (currentWorkingDir && typeof currentWorkingDir === 'string') {
        // Verify the directory exists before using it as default
        try {
          const stats = fsSync.lstatSync(currentWorkingDir);
          if (stats.isDirectory()) {
            defaultPath = currentWorkingDir;
          }
        } catch (error) {
          if (error && typeof error === 'object' && 'code' in error) {
            const fsError = error as { code?: string; message?: string };
            if (
              fsError.code === 'ENOENT' ||
              fsError.code === 'EACCES' ||
              fsError.code === 'EPERM'
            ) {
              console.warn(
                `Current working directory not accessible (${fsError.code}): ${currentWorkingDir}, falling back to home directory`
              );
              defaultPath = os.homedir();
            } else {
              console.warn(
                `Unexpected filesystem error (${fsError.code}) for directory ${currentWorkingDir}:`,
                fsError.message
              );
              defaultPath = os.homedir();
            }
          } else {
            console.warn(`Unexpected error checking directory ${currentWorkingDir}:`, error);
            defaultPath = os.homedir();
          }
        }
      }
    } catch (error) {
      console.warn('Failed to get current working directory from window:', error);
    }
  }

  if (!defaultPath) {
    defaultPath = os.homedir();
  }

  const result = (await dialog.showOpenDialog({
    properties: ['openFile', 'openDirectory', 'createDirectory'],
    defaultPath: defaultPath,
  })) as unknown as OpenDialogReturnValue;

  if (!result.canceled && result.filePaths.length > 0) {
    const selectedPath = result.filePaths[0];

    // If a file was selected, use its parent directory
    let dirToAdd = selectedPath;
    try {
      const stats = fsSync.lstatSync(selectedPath);

      // Reject symlinks for security
      if (stats.isSymbolicLink()) {
        console.warn(`Selected path is a symlink, using parent directory for security`);
        dirToAdd = path.dirname(selectedPath);
      } else if (stats.isFile()) {
        dirToAdd = path.dirname(selectedPath);
      }
    } catch {
      console.warn(`Could not stat selected path, using parent directory`);
      dirToAdd = path.dirname(selectedPath); // Fallback to parent directory
    }

    addRecentDir(dirToAdd);

    let deeplinkData: WorkflowDeeplinkData | undefined = undefined;
    if (windowDeeplinkURL) {
      deeplinkData = parseWorkflowDeeplink(windowDeeplinkURL);
    }
    // Create a new window with the selected directory
    await createChat(
      app,
      undefined,
      dirToAdd,
      undefined,
      undefined,
      undefined,
      deeplinkData?.config,
      undefined,
      undefined,
      deeplinkData?.parameters
    );
  }
  return result;
};

// Global error handler
const handleFatalError = (error: Error) => {
  const windows = BrowserWindow.getAllWindows();
  windows.forEach((win) => {
    win.webContents.send('fatal-error', error.message || 'An unexpected error occurred');
  });
};

function sanitizeErrorForLogging(err: unknown): string {
  const msg = err instanceof Error ? err.message + '\n' + (err.stack || '') : String(err);
  return msg
    .replace(/sk-[a-zA-Z0-9]{20,}/g, 'sk-***')
    .replace(/[Aa]pi[_-]?[Kk]ey[=:]\s*\S+/g, 'api_key=***')
    .replace(/[Bb]earer\s+\S+/g, 'Bearer ***');
}

process.on('uncaughtException', (error) => {
  console.error('Uncaught Exception:', sanitizeErrorForLogging(error));
  handleFatalError(error);
});

process.on('unhandledRejection', (error) => {
  console.error('Unhandled Rejection:', sanitizeErrorForLogging(error));
  handleFatalError(error instanceof Error ? error : new Error(String(error)));
});

ipcMain.on('react-ready', (event) => {
  log.info('React ready event received');

  // Get the window that sent the react-ready event
  const window = BrowserWindow.fromWebContents(event.sender);
  const windowId = window?.id;

  // Send any pending initial message for this window
  if (windowId && pendingInitialMessages.has(windowId)) {
    const initialMessage = pendingInitialMessages.get(windowId)!;
    log.info('Sending pending initial message to window:', initialMessage);
    window.webContents.send('set-initial-message', initialMessage);
    pendingInitialMessages.delete(windowId);
  }

  if (pendingDeepLink && window) {
    log.info('Processing pending deep link:', pendingDeepLink);
    try {
      const parsedUrl = new URL(pendingDeepLink);
      if (parsedUrl.hostname === 'extension') {
        log.info('Sending add-extension IPC to ready window');
        window.webContents.send('add-extension', pendingDeepLink);
      } else if (parsedUrl.hostname === 'sessions') {
        log.info('Sending open-shared-session IPC to ready window');
        window.webContents.send('open-shared-session', pendingDeepLink);
      }
      pendingDeepLink = null;
    } catch (error) {
      log.error('Error processing pending deep link:', error);
      pendingDeepLink = null;
    }
  } else {
    log.info('No pending deep link to process');
  }

  if (pendingBrxtFilePath && window) {
    const filePath = pendingBrxtFilePath;
    pendingBrxtFilePath = null;
    log.info('Sending pending .brxt file to ready window:', filePath);
    window.webContents.send('open-brxt-file', filePath);
  }

  log.info('React ready - window is prepared for deep links');
});

// Per-window snapshot of the bounds we entered dashboard mode from, so the
// exit handler can restore the user's pre-dashboard window size even if
// `win.isMaximized()` returns false (which happens when the window was
// already at max bounds via manual resize, or was launched maximized).
const preDashboardBounds = new Map<number, Electron.Rectangle>();

ipcMain.handle('dashboard:enter', (event) => {
  const win = BrowserWindow.fromWebContents(event.sender);
  if (!win) return;
  if (!preDashboardBounds.has(win.id)) {
    preDashboardBounds.set(win.id, win.getBounds());
  }
  if (!win.isMaximized()) win.maximize();
});

ipcMain.handle('dashboard:exit', (event) => {
  const win = BrowserWindow.fromWebContents(event.sender);
  if (!win) return;
  // Try the normal path first; if the window is officially maximized this
  // is the cleanest restore. Then *also* set bounds explicitly so we
  // recover when isMaximized() returned false (e.g. the window was already
  // at full bounds before the dashboardEnter call).
  if (win.isMaximized()) win.unmaximize();
  const saved = preDashboardBounds.get(win.id);
  if (saved) {
    win.setBounds(saved, true);
    preDashboardBounds.delete(win.id);
  }
});

ipcMain.handle('window:ensure-content-width', (event, minWidth: number) => {
  const win = BrowserWindow.fromWebContents(event.sender);
  if (!win || !Number.isFinite(minWidth)) {
    return { expanded: false, width: 0, height: 0 };
  }

  const currentContentBounds = win.getContentBounds();
  if (currentContentBounds.width >= minWidth || win.isMaximized() || win.isFullScreen()) {
    return {
      expanded: false,
      width: currentContentBounds.width,
      height: currentContentBounds.height,
    };
  }

  const windowBounds = win.getBounds();
  const display = screen.getDisplayMatching(windowBounds);
  const maxContentWidth = Math.max(720, display.workArea.width);
  const targetContentWidth = Math.min(Math.ceil(minWidth), maxContentWidth);

  win.setContentSize(targetContentWidth, currentContentBounds.height, true);

  const nextWindowBounds = win.getBounds();
  const maxX = display.workArea.x + display.workArea.width - nextWindowBounds.width;
  const maxY = display.workArea.y + display.workArea.height - nextWindowBounds.height;
  const adjustedX =
    maxX < display.workArea.x
      ? display.workArea.x
      : Math.min(Math.max(nextWindowBounds.x, display.workArea.x), maxX);
  const adjustedY =
    maxY < display.workArea.y
      ? display.workArea.y
      : Math.min(Math.max(nextWindowBounds.y, display.workArea.y), maxY);

  if (adjustedX !== nextWindowBounds.x || adjustedY !== nextWindowBounds.y) {
    win.setBounds({ ...nextWindowBounds, x: adjustedX, y: adjustedY }, true);
  }

  const nextContentBounds = win.getContentBounds();
  return {
    expanded: nextContentBounds.width > currentContentBounds.width,
    width: nextContentBounds.width,
    height: nextContentBounds.height,
  };
});

ipcMain.handle('open-external', async (_event, url: string) => {
  try {
    if (typeof url !== 'string' || url.length > 8 * 1024) {
      throw new Error('Blocked: invalid or oversized URL');
    }
    const parsed = new URL(url);
    if (
      (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') ||
      parsed.username !== '' ||
      parsed.password !== ''
    ) {
      throw new Error(`Blocked: unsafe URL protocol '${parsed.protocol}'`);
    }
    // Pass the normalized URL — shell.openExternal and the WHATWG URL parser
    // can disagree on edge-case inputs (embedded auth, backslashes, etc.),
    // and using the parsed.href guarantees shell sees what we validated.
    await shell.openExternal(parsed.href);
  } catch (err) {
    console.error('open-external blocked:', err);
  }
});

ipcMain.handle('directory-chooser', async () => {
  return dialog.showOpenDialog({
    properties: ['openDirectory', 'createDirectory'],
    defaultPath: os.homedir(),
  });
});

ipcMain.handle('add-recent-dir', (_event, dir: string) => {
  if (!dir || typeof dir !== 'string') return;
  const normalized = path.resolve(dir);
  if (!path.isAbsolute(normalized)) return;
  addRecentDir(normalized);
});

// Handle scheduling engine settings
ipcMain.handle('get-settings', () => {
  try {
    return loadSettings();
  } catch (error) {
    console.error('Error getting settings:', error);
    return null;
  }
});

ipcMain.handle('save-settings', (_event, settings) => {
  if (typeof settings !== 'object' || settings === null || Array.isArray(settings)) {
    console.error('save-settings: invalid settings object received');
    return;
  }
  try {
    saveSettings(settings);
    return true;
  } catch (error) {
    console.error('Error saving settings:', error);
    return false;
  }
});

ipcMain.handle('get-secret-key', () => {
  const settings = loadSettings();
  return getServerSecret(settings);
});

ipcMain.handle('get-biorouterd-host-port', async (event) => {
  const windowId = BrowserWindow.fromWebContents(event.sender)?.id;
  if (!windowId) {
    return null;
  }
  const client = biorouterdClients.get(windowId);
  if (!client) {
    return null;
  }
  return client.getConfig().baseUrl || null;
});

// Handle menu bar icon visibility
ipcMain.handle('set-menu-bar-icon', async (_event, show: boolean) => {
  try {
    const settings = loadSettings();
    settings.showMenuBarIcon = show;
    saveSettings(settings);

    if (show) {
      createTray();
    } else {
      destroyTray();
    }
    return true;
  } catch (error) {
    console.error('Error setting menu bar icon:', error);
    return false;
  }
});

ipcMain.handle('get-menu-bar-icon-state', () => {
  try {
    const settings = loadSettings();
    return settings.showMenuBarIcon ?? true;
  } catch (error) {
    console.error('Error getting menu bar icon state:', error);
    return true;
  }
});

// Handle dock icon visibility (macOS only)
ipcMain.handle('set-dock-icon', async (_event, show: boolean) => {
  try {
    if (process.platform !== 'darwin') return false;

    const settings = loadSettings();
    settings.showDockIcon = show;
    saveSettings(settings);

    if (show) {
      app.dock?.show();
    } else {
      // Only hide the dock if we have a menu bar icon to maintain accessibility
      if (settings.showMenuBarIcon) {
        app.dock?.hide();
        setTimeout(() => {
          focusWindow();
        }, 50);
      }
    }
    return true;
  } catch (error) {
    console.error('Error setting dock icon:', error);
    return false;
  }
});

ipcMain.handle('get-dock-icon-state', () => {
  try {
    if (process.platform !== 'darwin') return true;
    const settings = loadSettings();
    return settings.showDockIcon ?? true;
  } catch (error) {
    console.error('Error getting dock icon state:', error);
    return true;
  }
});

// Handle opening system notifications preferences
ipcMain.handle('open-notifications-settings', async () => {
  try {
    if (process.platform === 'darwin') {
      spawn('open', ['x-apple.systempreferences:com.apple.preference.notifications']);
      return true;
    } else if (process.platform === 'win32') {
      // Windows: Open notification settings in Settings app
      spawn('ms-settings:notifications', { shell: true });
      return true;
    } else if (process.platform === 'linux') {
      // Linux: Try different desktop environments
      // GNOME
      try {
        spawn('gnome-control-center', ['notifications']);
        return true;
      } catch {
        console.log('GNOME control center not found, trying other options');
      }

      // KDE Plasma
      try {
        spawn('systemsettings5', ['kcm_notifications']);
        return true;
      } catch {
        console.log('KDE systemsettings5 not found, trying other options');
      }

      // XFCE
      try {
        spawn('xfce4-settings-manager', ['--socket-id=notifications']);
        return true;
      } catch {
        console.log('XFCE settings manager not found, trying other options');
      }

      // Fallback: Try to open general settings
      try {
        spawn('gnome-control-center');
        return true;
      } catch {
        console.warn('Could not find a suitable settings application for Linux');
        return false;
      }
    } else {
      console.warn(
        `Opening notification settings is not supported on platform: ${process.platform}`
      );
      return false;
    }
  } catch (error) {
    console.error('Error opening notification settings:', error);
    return false;
  }
});

// Handle wakelock setting
ipcMain.handle('set-wakelock', async (_event, enable: boolean) => {
  try {
    const settings = loadSettings();
    settings.enableWakelock = enable;
    saveSettings(settings);

    // Stop all existing power save blockers when disabling the setting
    if (!enable) {
      for (const [windowId, blockerId] of windowPowerSaveBlockers.entries()) {
        try {
          powerSaveBlocker.stop(blockerId);
          console.log(
            `[Main] Stopped power save blocker ${blockerId} for window ${windowId} due to wakelock setting disabled`
          );
        } catch (error) {
          console.error(
            `[Main] Failed to stop power save blocker ${blockerId} for window ${windowId}:`,
            error
          );
        }
      }
      windowPowerSaveBlockers.clear();
    }

    return true;
  } catch (error) {
    console.error('Error setting wakelock:', error);
    return false;
  }
});

ipcMain.handle('get-wakelock-state', () => {
  try {
    const settings = loadSettings();
    return settings.enableWakelock ?? false;
  } catch (error) {
    console.error('Error getting wakelock state:', error);
    return false;
  }
});

ipcMain.handle('set-spellcheck', async (_event, enable: boolean) => {
  try {
    const settings = loadSettings();
    settings.spellcheckEnabled = enable;
    saveSettings(settings);
    return true;
  } catch (error) {
    console.error('Error setting spellcheck:', error);
    return false;
  }
});

ipcMain.handle('get-spellcheck-state', () => {
  try {
    const settings = loadSettings();
    return settings.spellcheckEnabled ?? true;
  } catch (error) {
    console.error('Error getting spellcheck state:', error);
    return true;
  }
});

// Add file/directory selection handler
ipcMain.handle('select-file-or-directory', async (_event, defaultPath?: string) => {
  if (process.env.PLAYWRIGHT_SELECT_PATH) {
    return process.env.PLAYWRIGHT_SELECT_PATH;
  }

  const dialogOptions: OpenDialogOptions = {
    properties: process.platform === 'darwin' ? ['openFile', 'openDirectory'] : ['openFile'],
  };

  // Set default path if provided
  if (defaultPath) {
    // Expand tilde to home directory
    const expandedPath = expandTilde(defaultPath);

    // Check if the path exists
    try {
      const stats = await fs.stat(expandedPath);
      if (stats.isDirectory()) {
        dialogOptions.defaultPath = expandedPath;
      } else {
        dialogOptions.defaultPath = path.dirname(expandedPath);
      }
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
    } catch (error) {
      // If path doesn't exist, fall back to home directory and log error
      console.error(`Default path does not exist: ${expandedPath}, falling back to home directory`);
      dialogOptions.defaultPath = os.homedir();
    }
  }

  const result = (await dialog.showOpenDialog(dialogOptions)) as unknown as OpenDialogReturnValue;

  if (!result.canceled && result.filePaths.length > 0) {
    return result.filePaths[0];
  }
  return null;
});

// Import session: open native file dialog, read JSON, return content
ipcMain.handle('import-session-file', async () => {
  const result = (await dialog.showOpenDialog({
    properties: ['openFile'],
    filters: [{ name: 'JSON', extensions: ['json'] }],
  })) as unknown as OpenDialogReturnValue;

  if (result.canceled || result.filePaths.length === 0) return null;
  return fs.readFile(result.filePaths[0], 'utf-8');
});

// IPC handler to save data URL to a temporary file
ipcMain.handle('save-data-url-to-temp', async (_event, dataUrl: string, uniqueId: string) => {
  console.log(`[Main] Received save-data-url-to-temp for ID: ${uniqueId}`);
  try {
    // Input validation for uniqueId - only allow alphanumeric characters and hyphens
    if (!uniqueId || !/^[a-zA-Z0-9-]+$/.test(uniqueId) || uniqueId.length > 50) {
      console.error('[Main] Invalid uniqueId format received.');
      return { id: uniqueId, error: 'Invalid uniqueId format' };
    }

    // Input validation for dataUrl. The 10 MB cap (matching the renderer-side
    // image limit ~5 MB after base64 overhead) caused main-process heap spikes
    // when users rapidly pasted screenshots — every IPC call materializes the
    // entire string in main via structured clone before validation. Drop to
    // 4 MB (≈3 MB image payload), which still covers typical screenshots.
    if (!dataUrl || typeof dataUrl !== 'string' || dataUrl.length > 4 * 1024 * 1024) {
      console.error('[Main] Invalid or too large data URL received.');
      return { id: uniqueId, error: 'Invalid or too large data URL' };
    }

    const tempDir = await ensureTempDirExists();
    const matches = dataUrl.match(/^data:(image\/(png|jpeg|jpg|gif|webp));base64,(.*)$/);

    if (!matches || matches.length < 4) {
      console.error('[Main] Invalid data URL format received.');
      return { id: uniqueId, error: 'Invalid data URL format or unsupported image type' };
    }

    const imageExtension = matches[2]; // e.g., "png", "jpeg"
    const base64Data = matches[3];

    // Validate base64 data
    if (!base64Data || !/^[A-Za-z0-9+/]*={0,2}$/.test(base64Data)) {
      console.error('[Main] Invalid base64 data received.');
      return { id: uniqueId, error: 'Invalid base64 data' };
    }

    const buffer = Buffer.from(base64Data, 'base64');

    // Validate image size (max 5MB)
    if (buffer.length > 3 * 1024 * 1024) {
      console.error('[Main] Image too large.');
      return { id: uniqueId, error: 'Image too large (max 3MB)' };
    }

    const randomString = crypto.randomBytes(8).toString('hex');
    const fileName = `pasted-${uniqueId}-${randomString}.${imageExtension}`;
    const filePath = path.join(tempDir, fileName);

    // Ensure the resolved path is still within the temp directory
    const resolvedPath = path.resolve(filePath);
    const resolvedTempDir = path.resolve(tempDir);
    if (!resolvedPath.startsWith(resolvedTempDir + path.sep)) {
      console.error('[Main] Attempted path traversal detected.');
      return { id: uniqueId, error: 'Invalid file path' };
    }

    await fs.writeFile(filePath, buffer);
    console.log(`[Main] Saved image for ID ${uniqueId} to: ${filePath}`);
    return { id: uniqueId, filePath: filePath };
  } catch (error) {
    console.error(`[Main] Failed to save image to temp for ID ${uniqueId}:`, error);
    return { id: uniqueId, error: error instanceof Error ? error.message : 'Failed to save image' };
  }
});

// IPC handler to serve temporary image files
ipcMain.handle('get-temp-image', async (_event, filePath: string) => {
  console.log(`[Main] Received get-temp-image for path: ${filePath}`);

  // Input validation
  if (!filePath || typeof filePath !== 'string') {
    console.warn('[Main] Invalid file path provided for image serving');
    return null;
  }

  // Ensure the path is within the designated temp directory
  const resolvedPath = path.resolve(filePath);
  const resolvedTempDir = path.resolve(biorouterTempDir);

  if (!resolvedPath.startsWith(resolvedTempDir + path.sep)) {
    console.warn(`[Main] Attempted to access file outside designated temp directory: ${filePath}`);
    return null;
  }

  try {
    // Check if it's a regular file first, before trying realpath
    const stats = await fs.lstat(filePath);
    if (!stats.isFile()) {
      console.warn(`[Main] Not a regular file, refusing to serve: ${filePath}`);
      return null;
    }

    // Get the real paths for both the temp directory and the file to handle symlinks properly
    let realTempDir: string;
    let actualPath = filePath;

    try {
      realTempDir = await fs.realpath(biorouterTempDir);
      const realPath = await fs.realpath(filePath);

      // Double-check that the real path is still within our real temp directory
      if (!realPath.startsWith(realTempDir + path.sep)) {
        console.warn(
          `[Main] Real path is outside designated temp directory: ${realPath} not in ${realTempDir}`
        );
        return null;
      }
      actualPath = realPath;
    } catch (realpathError) {
      // If realpath fails, use the original path validation
      console.log(
        `[Main] realpath failed for ${filePath}, using original path validation:`,
        realpathError instanceof Error ? realpathError.message : String(realpathError)
      );
    }

    // Read the file and return as base64 data URL
    const fileBuffer = await fs.readFile(actualPath);
    const fileExtension = path.extname(actualPath).toLowerCase().substring(1);

    // Validate file extension
    const allowedExtensions = ['png', 'jpg', 'jpeg', 'gif', 'webp'];
    if (!allowedExtensions.includes(fileExtension)) {
      console.warn(`[Main] Unsupported file extension: ${fileExtension}`);
      return null;
    }

    const mimeType = fileExtension === 'jpg' ? 'image/jpeg' : `image/${fileExtension}`;
    const base64Data = fileBuffer.toString('base64');
    const dataUrl = `data:${mimeType};base64,${base64Data}`;

    console.log(`[Main] Served temp image: ${filePath}`);
    return dataUrl;
  } catch (error) {
    console.error(`[Main] Failed to serve temp image: ${filePath}`, error);
    return null;
  }
});
ipcMain.on('delete-temp-file', async (_event, filePath: string) => {
  console.log(`[Main] Received delete-temp-file for path: ${filePath}`);

  // Input validation
  if (!filePath || typeof filePath !== 'string') {
    console.warn('[Main] Invalid file path provided for deletion');
    return;
  }

  // Ensure the path is within the designated temp directory
  const resolvedPath = path.resolve(filePath);
  const resolvedTempDir = path.resolve(biorouterTempDir);

  if (!resolvedPath.startsWith(resolvedTempDir + path.sep)) {
    console.warn(`[Main] Attempted to delete file outside designated temp directory: ${filePath}`);
    return;
  }

  try {
    // Check if it's a regular file first, before trying realpath
    const stats = await fs.lstat(filePath);
    if (!stats.isFile()) {
      console.warn(`[Main] Not a regular file, refusing to delete: ${filePath}`);
      return;
    }

    // Get the real paths for both the temp directory and the file to handle symlinks properly
    let actualPath = filePath;

    try {
      const realTempDir = await fs.realpath(biorouterTempDir);
      const realPath = await fs.realpath(filePath);

      // Double-check that the real path is still within our real temp directory
      if (!realPath.startsWith(realTempDir + path.sep)) {
        console.warn(
          `[Main] Real path is outside designated temp directory: ${realPath} not in ${realTempDir}`
        );
        return;
      }
      actualPath = realPath;
    } catch (realpathError) {
      // If realpath fails, use the original path validation
      console.log(
        `[Main] realpath failed for ${filePath}, using original path validation:`,
        realpathError instanceof Error ? realpathError.message : String(realpathError)
      );
    }

    await fs.unlink(actualPath);
    console.log(`[Main] Deleted temp file: ${filePath}`);
  } catch (error) {
    if (error && typeof error === 'object' && 'code' in error && error.code !== 'ENOENT') {
      // ENOENT means file doesn't exist, which is fine
      console.error(`[Main] Failed to delete temp file: ${filePath}`, error);
    } else {
      console.log(`[Main] Temp file already deleted or not found: ${filePath}`);
    }
  }
});

// IPC handler to read a temporary image file and return raw base64 + mimeType
ipcMain.handle('read-temp-image-as-base64', async (_event, filePath: string) => {
  console.log(`[Main] Received read-temp-image-as-base64 for path: ${filePath}`);

  // Input validation
  if (!filePath || typeof filePath !== 'string') {
    throw new Error('Invalid file path provided');
  }

  // Ensure the path is within the designated temp directory
  const resolvedPath = path.resolve(filePath);
  const resolvedTempDir = path.resolve(biorouterTempDir);

  if (!resolvedPath.startsWith(resolvedTempDir + path.sep)) {
    console.warn(`[Main] Attempted to access file outside designated temp directory: ${filePath}`);
    throw new Error('File path is outside the designated temp directory');
  }

  // Check if it's a regular file first, before trying realpath
  const stats = await fs.lstat(filePath);
  if (!stats.isFile()) {
    console.warn(`[Main] Not a regular file, refusing to read: ${filePath}`);
    throw new Error('Path is not a regular file');
  }

  // Get the real paths for both the temp directory and the file to handle symlinks properly
  let actualPath = filePath;

  try {
    const realTempDir = await fs.realpath(biorouterTempDir);
    const realPath = await fs.realpath(filePath);

    // Double-check that the real path is still within our real temp directory
    if (!realPath.startsWith(realTempDir + path.sep)) {
      console.warn(
        `[Main] Real path is outside designated temp directory: ${realPath} not in ${realTempDir}`
      );
      throw new Error('File path resolves outside the designated temp directory');
    }
    actualPath = realPath;
  } catch (realpathError) {
    // If realpath itself threw our own error, re-throw it
    if (
      realpathError instanceof Error &&
      realpathError.message.startsWith('File path resolves outside')
    ) {
      throw realpathError;
    }
    // Otherwise realpath syscall failed; fall back to original path validation
    console.log(
      `[Main] realpath failed for ${filePath}, using original path validation:`,
      realpathError instanceof Error ? realpathError.message : String(realpathError)
    );
  }

  const fileExtension = path.extname(actualPath).toLowerCase().substring(1);

  // Determine MIME type from extension
  const mimeTypeMap: Record<string, string> = {
    png: 'image/png',
    jpg: 'image/jpeg',
    jpeg: 'image/jpeg',
    gif: 'image/gif',
    webp: 'image/webp',
  };

  const mimeType = mimeTypeMap[fileExtension];
  if (!mimeType) {
    console.warn(`[Main] Unsupported file extension for base64 read: ${fileExtension}`);
    throw new Error(`Unsupported image type: ${fileExtension}`);
  }

  const fileBuffer = await fs.readFile(actualPath);
  const data = fileBuffer.toString('base64');

  console.log(`[Main] Read temp image as base64: ${filePath}`);
  return { data, mimeType };
});

ipcMain.handle('check-ollama', async () => {
  try {
    return new Promise((resolve) => {
      // Run `ps` and filter for "ollama"
      const ps = spawn('ps', ['aux']);
      const grep = spawn('grep', ['-iw', '[o]llama']);

      let output = '';
      let errorOutput = '';

      // Pipe ps output to grep
      ps.stdout.pipe(grep.stdin);

      grep.stdout.on('data', (data) => {
        output += data.toString();
      });

      grep.stderr.on('data', (data) => {
        errorOutput += data.toString();
      });

      grep.on('close', (code) => {
        if (code !== null && code !== 0 && code !== 1) {
          // grep returns 1 when no matches found
          console.error('Error executing grep command:', errorOutput);
          return resolve(false);
        }

        console.log('Raw stdout from ps|grep command:', output);
        const trimmedOutput = output.trim();
        console.log('Trimmed stdout:', trimmedOutput);

        const isRunning = trimmedOutput.length > 0;
        resolve(isRunning);
      });

      ps.on('error', (error) => {
        console.error('Error executing ps command:', error);
        resolve(false);
      });

      grep.on('error', (error) => {
        console.error('Error executing grep command:', error);
        resolve(false);
      });

      // Close ps stdin when done
      ps.stdout.on('end', () => {
        grep.stdin.end();
      });
    });
  } catch (err) {
    console.error('Error checking for Ollama:', err);
    return false;
  }
});

ipcMain.handle('read-file', async (_event, filePath) => {
  const expandedPath = expandBiorouterPath(filePath);
  try {
    const resolvedPath = path.resolve(expandedPath);
    if (!isAllowedFilePath(resolvedPath)) {
      throw new Error(`Access denied: path '${resolvedPath}' is outside allowed directories`);
    }
    // Single fs.readFile path for all platforms. The previous `spawn('cat')`
    // fallback added an extra process per call (FD + PID pressure) for no
    // benefit — fs.readFile is faster and doesn't depend on `cat` being on
    // PATH inside the Electron environment.
    const buffer = await fs.readFile(expandedPath);
    return { file: buffer.toString('utf8'), filePath: expandedPath, error: null, found: true };
  } catch (error) {
    const fileError = error as { code?: string };
    if (fileError.code !== 'ENOENT') {
      console.error('Error reading file:', error);
    }
    return { file: '', filePath: expandedPath, error, found: false };
  }
});

ipcMain.handle('read-artifact-file', async (_event, filePath: string) => {
  const expandedPath = expandBiorouterPath(filePath);
  const resolvedPath = path.resolve(expandedPath);
  const title = path.basename(resolvedPath) || resolvedPath;
  try {
    if (!isAllowedFilePath(resolvedPath)) {
      throw new Error(`Access denied: path '${resolvedPath}' is outside allowed directories`);
    }

    const stats = await fs.stat(resolvedPath);
    if (stats.isDirectory()) {
      const gitTree = await readGitArtifactTree(resolvedPath);
      if (gitTree) {
        return {
          kind: 'gitDirectory',
          title,
          path: resolvedPath,
          branch: gitTree.branch,
          entries: gitTree.entries,
          found: true,
        };
      }
      return {
        kind: 'directory',
        title,
        path: resolvedPath,
        found: true,
        entries: await readArtifactDirectoryTree(resolvedPath),
      };
    }

    if (!stats.isFile()) {
      throw new Error('Path is not a regular file or directory');
    }

    const mimeType = mimeTypeForArtifactPath(resolvedPath);

    // Artifacts are auto-detected from assistant text and opened without a
    // click, so a model that names a huge file must not be able to make the
    // main process buffer it (images additionally grow ~4/3 as base64).
    // Report oversized files as binary: the UI shows metadata, not content.
    if (stats.size > ARTIFACT_PREVIEW_MAX_BYTES) {
      return {
        kind: 'binary',
        title,
        path: resolvedPath,
        mimeType,
        size: stats.size,
        found: true,
      };
    }

    const buffer = await fs.readFile(resolvedPath);
    const documentFormat = documentFormatForArtifactPath(resolvedPath);
    if (documentFormat) {
      return {
        kind: 'document',
        format: documentFormat,
        title,
        path: resolvedPath,
        mimeType,
        data: Uint8Array.from(buffer).buffer,
        size: stats.size,
        found: true,
      };
    }

    if (mimeType.startsWith('image/')) {
      return {
        kind: 'image',
        title,
        path: resolvedPath,
        mimeType,
        dataUrl: `data:${mimeType};base64,${buffer.toString('base64')}`,
        size: stats.size,
        found: true,
      };
    }

    if (isTextArtifact(mimeType, buffer)) {
      return {
        kind: mimeType === 'text/html' ? 'html' : 'text',
        title,
        path: resolvedPath,
        mimeType,
        text: buffer.toString('utf8'),
        size: stats.size,
        found: true,
      };
    }

    return {
      kind: 'binary',
      title,
      path: resolvedPath,
      mimeType,
      size: stats.size,
      found: true,
    };
  } catch (error) {
    const message = error instanceof Error ? error.message : 'Could not read artifact file';
    return {
      kind: 'error',
      title,
      path: resolvedPath,
      error: message,
      found: false,
    };
  }
});

ipcMain.handle('write-file', async (_event, filePath, content) => {
  try {
    const expandedPath = expandBiorouterPath(filePath);
    await fs.mkdir(path.dirname(expandedPath), { recursive: true });
    await fs.writeFile(expandedPath, content, { encoding: 'utf8' });
    return true;
  } catch (error) {
    console.error('Error writing to file:', error);
    return false;
  }
});

// Enhanced file operations
ipcMain.handle('ensure-directory', async (_event, dirPath) => {
  try {
    const expandedPath = expandBiorouterPath(dirPath);

    await fs.mkdir(expandedPath, { recursive: true });
    return true;
  } catch (error) {
    console.error('Error creating directory:', error);
    return false;
  }
});

ipcMain.handle('list-files', async (_event, dirPath, extension) => {
  try {
    const expandedPath = expandBiorouterPath(dirPath);

    const files = await fs.readdir(expandedPath);
    if (extension) {
      return files.filter((file) => file.endsWith(extension));
    }
    return files;
  } catch (error) {
    if (
      typeof error === 'object' &&
      error !== null &&
      'code' in error &&
      (error.code === 'ENOTDIR' || error.code === 'ENOENT')
    ) {
      return [];
    }
    console.error('Error listing files:', error);
    return [];
  }
});

ipcMain.handle('delete-file', async (_event, filePath: string) => {
  try {
    const expandedPath = expandBiorouterPath(filePath);
    const resolvedPath = path.resolve(expandedPath);
    const allowedRoots = allowedFileRoots();
    const isAllowed = allowedRoots.some(
      (root) => resolvedPath.startsWith(root + path.sep) || resolvedPath === root
    );
    if (!isAllowed) {
      throw new Error(`Access denied: path '${resolvedPath}' is outside allowed directories`);
    }
    await fs.unlink(resolvedPath);
    return true;
  } catch (error) {
    console.error('Error deleting file:', error);
    return false;
  }
});

ipcMain.handle('list-skill-dirs', async (_event, dirPath: string) => {
  try {
    const expandedPath = expandBiorouterPath(dirPath);
    const entries = await fs.readdir(expandedPath, { withFileTypes: true });
    return entries.filter((e) => e.isDirectory()).map((e) => e.name);
  } catch {
    return [];
  }
});

ipcMain.handle('delete-directory', async (_event, dirPath: string) => {
  try {
    const expandedPath = expandBiorouterPath(dirPath);
    const resolvedPath = path.resolve(expandedPath);
    const allowedRoots = allowedFileRoots();
    const isAllowed = allowedRoots.some(
      (root) => resolvedPath.startsWith(root + path.sep) || resolvedPath === root
    );
    if (!isAllowed) {
      throw new Error(`Access denied: '${resolvedPath}' is outside allowed directories`);
    }
    await fs.rm(resolvedPath, { recursive: true, force: true });
    return true;
  } catch (error) {
    console.error('Error deleting directory:', error);
    return false;
  }
});

ipcMain.handle('show-message-box', async (_event, options) => {
  return dialog.showMessageBox(options);
});

ipcMain.handle('show-save-dialog', async (_event, options) => {
  return dialog.showSaveDialog(options);
});

ipcMain.handle(
  'save-diagnostics-bundle',
  async (event, sessionId: string, archive: DiagnosticsArchivePayload) => {
    try {
      if (!sessionId || typeof sessionId !== 'string') {
        throw new Error('A session is required to generate diagnostics.');
      }

      const bytes = diagnosticsArchiveBytes(archive);
      const parent = BrowserWindow.fromWebContents(event.sender);
      const options = {
        title: 'Save Diagnostics Bundle',
        defaultPath: path.join(app.getPath('downloads'), diagnosticsArchiveFilename(sessionId)),
        buttonLabel: 'Save',
        filters: [{ name: 'ZIP Archives', extensions: ['zip'] }],
      };
      const result = parent
        ? await dialog.showSaveDialog(parent, options)
        : await dialog.showSaveDialog(options);

      if (result.canceled || !result.filePath) {
        return { canceled: true };
      }

      await fs.writeFile(result.filePath, bytes);
      return { canceled: false, filePath: result.filePath };
    } catch (error) {
      const message =
        error instanceof Error ? error.message : 'Failed to save the diagnostics bundle.';
      log.error('Failed to save diagnostics bundle:', error);
      return { canceled: false, error: message };
    }
  }
);

ipcMain.handle('get-allowed-extensions', async () => {
  return await getAllowList();
});

function parseFrontmatterFromSkillMd(
  content: string
): { name: string; description: string } | null {
  const match = content.match(/^---\r?\n([\s\S]*?)\r?\n---(\r?\n|$)/);
  if (!match) return null;
  const fm = match[1];
  const nameMatch = fm.match(/^name:\s*([^\n]+)$/m);
  const descMatch = fm.match(/^description:\s*([^\n]+)$/m);
  if (!nameMatch?.[1]?.trim() || !descMatch?.[1]?.trim()) return null;
  return { name: nameMatch[1].trim(), description: descMatch[1].trim() };
}

// --- BAAM registry (Browse Skills / Browse Extensions) --------------------
// The marketplace catalog is published at biorouter.ucsf.edu/registry.json
// (generated from baam.html). We fetch it live so the in-app browser stays in
// sync with the website; the renderer ships a bundled snapshot as a fallback.
const REGISTRY_URL = 'https://biorouter.ucsf.edu/registry.json';

// Only these hosts may be fetched/downloaded from for the Browse feature. The
// registry's skill/extension assets all live on github.com or the site itself.
const REGISTRY_DOWNLOAD_HOSTS = new Set([
  'biorouter.ucsf.edu',
  'github.com',
  'objects.githubusercontent.com',
  'raw.githubusercontent.com',
  'codeload.github.com',
]);

function isAllowedRegistryUrl(rawUrl: string): URL | null {
  try {
    const parsed = new URL(rawUrl);
    if (parsed.protocol !== 'https:') return null;
    if (!REGISTRY_DOWNLOAD_HOSTS.has(parsed.hostname)) return null;
    return parsed;
  } catch {
    return null;
  }
}

ipcMain.handle('registry:fetch', async () => {
  try {
    const response = await fetch(REGISTRY_URL, {
      headers: { 'User-Agent': 'Biorouter', Accept: 'application/json' },
    });
    if (!response.ok) return { error: `HTTP ${response.status}` };
    const json = await response.json();
    return { registry: json };
  } catch (err) {
    return { error: (err as Error).message };
  }
});

// Download a registry asset (.zip skill bundle or .brxt extension) to a temp
// file and return its local path, for reuse by the existing install flows.
ipcMain.handle('registry:download', async (_event, { url }: { url: string }) => {
  const parsed = isAllowedRegistryUrl(url);
  if (!parsed) return { error: 'Refusing to download from an untrusted URL.' };

  const ext = parsed.pathname.toLowerCase().endsWith('.brxt') ? '.brxt' : '.zip';
  if (!parsed.pathname.toLowerCase().endsWith('.zip') && ext !== '.brxt') {
    return { error: 'Unsupported asset type.' };
  }

  try {
    const response = await fetch(url, {
      headers: { 'User-Agent': 'Biorouter' },
      redirect: 'follow',
    });
    if (!response.ok) return { error: `Download failed: HTTP ${response.status}` };

    const MAX_SIZE = 200 * 1024 * 1024; // 200MB ceiling
    const buf = Buffer.from(await response.arrayBuffer());
    if (buf.length > MAX_SIZE) return { error: 'Download too large.' };

    const dir = path.join(os.tmpdir(), 'biorouter-registry');
    fsSync.mkdirSync(dir, { recursive: true });
    const safeName = (path.basename(parsed.pathname) || `asset${ext}`).replace(
      /[^a-zA-Z0-9._-]/g,
      '_'
    );
    const dest = path.join(dir, `${crypto.randomBytes(6).toString('hex')}-${safeName}`);
    fsSync.writeFileSync(dest, buf);
    return { path: dest };
  } catch (err) {
    return { error: `Download failed: ${(err as Error).message}` };
  }
});

ipcMain.handle('brxt:open-file-dialog', async (event) => {
  // Allow automated tests to inject a file path without a native dialog
  if (process.env.PLAYWRIGHT_BRXT_FILE) {
    return process.env.PLAYWRIGHT_BRXT_FILE;
  }
  const win = BrowserWindow.fromWebContents(event.sender);
  const result = await dialog.showOpenDialog(win!, {
    title: 'Select Biorouter Extension Bundle',
    filters: [{ name: 'Biorouter Extension Bundle', extensions: ['brxt'] }],
    properties: ['openFile'],
  });
  if (result.canceled || result.filePaths.length === 0) return null;
  return result.filePaths[0];
});

ipcMain.handle('brxt:validate-and-read', async (_event, { filePath }: { filePath: string }) => {
  try {
    const zip = new AdmZip(filePath);
    const entries = zip.getEntries().map((e) => e.entryName);

    if (!entries.some((e) => e === 'manifest.json'))
      return { error: 'Missing manifest.json. This is not a valid .brxt bundle.' };
    if (!entries.some((e) => e.toLowerCase() === 'readme.md'))
      return { error: 'Missing README.md. This is not a valid .brxt bundle.' };
    if (!entries.some((e) => e === 'pyproject.toml'))
      return { error: 'Missing pyproject.toml. This is not a valid .brxt bundle.' };
    if (!entries.some((e) => e.startsWith('src/')))
      return { error: 'Missing src/ directory. This is not a valid .brxt bundle.' };

    const manifestEntry = zip.getEntry('manifest.json');
    if (!manifestEntry) return { error: 'Could not read manifest.json' };

    const manifest = JSON.parse(manifestEntry.getData().toString('utf8'));

    for (const field of [
      'name',
      'display_name',
      'description',
      'version',
      'entry_point',
      'repository',
    ]) {
      if (!manifest[field]) return { error: `manifest.json missing required field: "${field}"` };
    }

    if (!Array.isArray(manifest.env_vars))
      return { error: 'manifest.json "env_vars" must be an array' };

    // Scan for bundled skills in skills/<slug>/SKILL.md
    const skillsPreview: Array<{ slug: string; name: string; description: string }> = [];
    for (const entry of zip.getEntries()) {
      const m = entry.entryName.match(/^skills\/([^/]+)\/SKILL\.md$/);
      if (m) {
        const slug = m[1];
        const parsed = parseFrontmatterFromSkillMd(entry.getData().toString('utf8'));
        if (parsed)
          skillsPreview.push({ slug, name: parsed.name, description: parsed.description });
      }
    }

    return { manifest, skillsPreview };
  } catch (err) {
    return { error: `Failed to read bundle: ${(err as Error).message}` };
  }
});

// Generous cap: when a dependency has no prebuilt wheel, uv compiles it from
// source, which can take several minutes on its own.
const UV_SYNC_TIMEOUT_MS = 600_000;

/** Map well-known `uv sync` failure signatures to an actionable hint appended
 *  below the raw output. Checks run most-specific first. Mirrors
 *  `uv_sync_hint` in crates/biorouter-cli/src/commands/extension.rs. */
function uvSyncHint(detail: string): string | null {
  if (detail.includes('Symbol not found') && detail.includes('librustc_driver')) {
    // Homebrew rust links libLLVM.dylib dynamically and breaks when llvm is
    // upgraded; `brew upgrade rust` does not reliably fix it, so steer to the
    // self-contained rustup toolchain and removing the Homebrew one.
    return (
      'Your Homebrew Rust toolchain is broken. rustc aborts because Homebrew’s llvm was ' +
      'upgraded out from under it (a known Homebrew issue). `brew upgrade rust` usually ' +
      'does NOT fix this. Install the self-contained rustup toolchain and remove the ' +
      'Homebrew one so it takes priority:\n' +
      '    brew uninstall rust\n' +
      "    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh\n" +
      'then fully restart Biorouter and retry.'
    );
  }
  if (detail.includes('cryptography') && cryptographyBuiltFromSource(detail)) {
    // cryptography ≥49 (2026-06-12) dropped x86_64 macOS wheels.
    return (
      '`cryptography` ≥49 no longer ships x86_64 (Intel) macOS wheels, so on an Intel Mac ' +
      'it must be compiled from source, which needs a Rust toolchain. Install rustup ' +
      '(https://rustup.rs) and retry, or ask the extension author to cap `cryptography<49` ' +
      '(the last series with Intel-Mac wheels).'
    );
  }
  if (detail.includes('maturin') || detail.includes('rustc')) {
    return (
      'A dependency has no prebuilt package for your platform, so it was compiled from ' +
      'source, which needs a working Rust toolchain. Install one via https://rustup.rs ' +
      '(or repair your existing install) and retry.'
    );
  }
  if (detail.includes('Failed to build')) {
    return (
      'A dependency has no prebuilt package for your platform, so uv tried to compile it ' +
      'from source. Make sure a compiler toolchain is installed, or ask the extension ' +
      'author to pin versions that ship prebuilt wheels.'
    );
  }
  return null;
}

/** True when stderr indicates `cryptography` was being built from source.
 *  Mirrors `cryptography_built_from_source` in the CLI crate. */
function cryptographyBuiltFromSource(detail: string): boolean {
  return (
    detail.includes('Failed to build `cryptography') ||
    detail.includes('Building cryptography') ||
    (detail.includes('cryptography') && detail.includes('maturin'))
  );
}

ipcMain.handle(
  'brxt:install',
  async (_event, { filePath, extensionName }: { filePath: string; extensionName: string }) => {
    try {
      const installDir = path.join(
        os.homedir(),
        '.config',
        'biorouter',
        'extensions',
        extensionName
      );

      // Create install directory
      fsSync.mkdirSync(installDir, { recursive: true });

      // Extract bundle (zip-slip guarded: rejects entries that escape installDir)
      const zip = new AdmZip(filePath);
      safeExtractZip(zip, installDir);

      // Pre-build the virtual environment
      const uvResult = spawnSync('uv', ['sync'], {
        cwd: installDir,
        encoding: 'utf8',
        timeout: UV_SYNC_TIMEOUT_MS,
        env: SPAWN_ENV,
      });

      if (uvResult.status !== 0) {
        const timedOut =
          (uvResult.error as (Error & { code?: string }) | undefined)?.code === 'ETIMEDOUT';
        if (timedOut) {
          throw new Error(
            `uv sync timed out after ${UV_SYNC_TIMEOUT_MS / 60_000} minutes. ` +
              'A dependency may be compiling from source on a slow connection or machine. ' +
              'Try again, or build manually with `uv sync` in ' +
              installDir
          );
        }
        const detail =
          uvResult.error?.message ||
          uvResult.stderr ||
          uvResult.stdout ||
          `exited with status ${uvResult.status}`;
        const hint = uvSyncHint(detail);
        throw new Error(`uv sync failed: ${detail}${hint ? `\n\nHint: ${hint}` : ''}`);
      }

      return { success: true, installDir };
    } catch (err) {
      return { error: `Installation failed: ${(err as Error).message}` };
    }
  }
);

ipcMain.handle('brxt:uninstall', async (_event, { extensionName }: { extensionName: string }) => {
  try {
    if (
      !extensionName ||
      /[/\\]/.test(extensionName) ||
      extensionName === '..' ||
      extensionName === '.'
    ) {
      return { error: 'Invalid extension name.' };
    }
    const installDir = path.join(os.homedir(), '.config', 'biorouter', 'extensions', extensionName);
    const extensionsBase = path.join(os.homedir(), '.config', 'biorouter', 'extensions');
    if (!installDir.startsWith(extensionsBase + path.sep)) {
      return { error: 'Invalid extension name.' };
    }
    if (fsSync.existsSync(installDir)) {
      fsSync.rmSync(installDir, { recursive: true, force: true });
    }
    return { success: true as const };
  } catch (err) {
    return { error: `Uninstall failed: ${(err as Error).message}` };
  }
});

ipcMain.handle('skills:extract-zip', async (_event, { filePath }: { filePath: string }) => {
  try {
    const zip = new AdmZip(filePath);
    const entries = zip.getEntries();

    const TEXT_EXTENSIONS = ['.md', '.txt', '.yaml', '.yml', '.json', '.py', '.sh'];

    // --- Single skill: root SKILL.md ---
    let skillEntry = entries.find((e) => e.entryName === 'SKILL.md');
    let prefix = '';

    if (!skillEntry) {
      // Single skill inside a folder: <slug>/SKILL.md
      const single = entries.find((e) => /^[^/]+\/SKILL\.md$/.test(e.entryName));
      if (single) {
        skillEntry = single;
        prefix = single.entryName.replace(/\/SKILL\.md$/, '') + '/';
      }
    }

    if (skillEntry) {
      // --- Single skill install ---
      const parsed = parseFrontmatterFromSkillMd(skillEntry.getData().toString('utf8'));
      if (!parsed) {
        return {
          error: 'SKILL.md must have valid frontmatter with "name" and "description".',
        };
      }

      const slug = parsed.name
        .replace(/[^a-z0-9-_]/gi, '-')
        .replace(/-{2,}/g, '-')
        .replace(/^-|-$/g, '')
        .toLowerCase();

      const files: [string, string][] = [];
      for (const entry of entries) {
        if (entry.isDirectory) continue;
        if (prefix && !entry.entryName.startsWith(prefix)) continue;
        const relName = prefix ? entry.entryName.slice(prefix.length) : entry.entryName;
        if (!relName) continue;
        // zip-slip guard: skip entries whose path would escape the skill dir when
        // written downstream (installSkill writes `${destFolder}/${relName}`).
        try {
          safeZipEntryTarget('/__skill__', relName);
        } catch {
          continue;
        }
        const ext = path.extname(relName).toLowerCase();
        if (!TEXT_EXTENSIONS.includes(ext)) continue;
        files.push([relName, entry.getData().toString('utf8')]);
      }

      return {
        isBundle: false as const,
        files,
        name: parsed.name,
        description: parsed.description,
        slug,
      };
    }

    // --- Bundle detection: <bundleName>/<subSlug>/SKILL.md ---
    const bundleSkillEntries = entries.filter((e) => /^[^/]+\/[^/]+\/SKILL\.md$/.test(e.entryName));

    if (bundleSkillEntries.length === 0) {
      return { error: 'No SKILL.md found in the ZIP file.' };
    }

    // Group by bundle folder (first path component)
    const bundleFolder = bundleSkillEntries[0].entryName.split('/')[0];
    const bundlePrefix = bundleFolder + '/';
    const bundleSkills: Array<{ name: string; description: string }> = [];

    for (const entry of bundleSkillEntries) {
      if (!entry.entryName.startsWith(bundlePrefix)) continue;
      const parsed = parseFrontmatterFromSkillMd(entry.getData().toString('utf8'));
      if (parsed) bundleSkills.push(parsed);
    }

    if (bundleSkills.length === 0) {
      return { error: 'No valid SKILL.md files found in bundle.' };
    }

    const bundleFiles: [string, string][] = [];
    for (const entry of entries) {
      if (entry.isDirectory) continue;
      if (!entry.entryName.startsWith(bundlePrefix)) continue;
      const relName = entry.entryName.slice(bundlePrefix.length);
      if (!relName) continue;
      // zip-slip guard: skip entries whose path would escape the skill dir.
      try {
        safeZipEntryTarget('/__skill__', relName);
      } catch {
        continue;
      }
      const ext = path.extname(relName).toLowerCase();
      if (!TEXT_EXTENSIONS.includes(ext)) continue;
      bundleFiles.push([relName, entry.getData().toString('utf8')]);
    }

    const slug = bundleFolder
      .replace(/[^a-z0-9-_]/gi, '-')
      .replace(/-{2,}/g, '-')
      .replace(/^-|-$/g, '')
      .toLowerCase();

    return {
      isBundle: true as const,
      bundleName: bundleFolder,
      bundleSkills,
      files: bundleFiles,
      slug,
      name: bundleFolder,
      description: `Bundle of ${bundleSkills.length} skills`,
    };
  } catch (err) {
    return { error: `Failed to read ZIP: ${(err as Error).message}` };
  }
});

function handleBrxtFileOpen(filePath: string) {
  // Find the main window (or store for when one is ready)
  const win = BrowserWindow.getAllWindows().find((w) => !w.isDestroyed());
  if (win) {
    win.webContents.send('open-brxt-file', filePath);
    win.focus();
  } else {
    // Store for when window is ready
    pendingBrxtFilePath = filePath;
  }
}

/**
 * IPC for the "Install Biorouter CLI" affordance. The actual install logic
 * lives in the bundled CLI (`biorouter setup-path`) so the terminal and the
 * desktop app share one implementation (Rust `biorouter::system::install_cli`).
 */
// Run `<binary> --version` and return the parsed dotted version, or null if it
// can't be determined (missing binary, broken symlink, non-zero exit). The CLI
// prints just the version (e.g. " 1.85.0") thanks to its empty display name.
function cliVersionOf(binary: string): string | null {
  try {
    const res = spawnSync(binary, ['--version'], {
      encoding: 'utf8',
      env: SPAWN_ENV,
      timeout: 10_000,
    });
    if (res.status !== 0) return null;
    const m = (res.stdout || res.stderr || '').match(/(\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.]+)?)/);
    return m ? m[1] : null;
  } catch {
    return null;
  }
}

// True if dotted version `a` is strictly older than `b` (segment-wise numeric;
// mirrors the Rust `version_newer` used by `biorouter::system`).
function isVersionOlder(a: string, b: string): boolean {
  const parse = (s: string) =>
    s
      .replace(/^v/, '')
      .split(/[.\-+]/)
      .map((p) => parseInt(p, 10) || 0);
  const va = parse(a);
  const vb = parse(b);
  for (let i = 0; i < Math.max(va.length, vb.length); i++) {
    const x = va[i] ?? 0;
    const y = vb[i] ?? 0;
    if (x !== y) return x < y;
  }
  return false;
}

function usableWorkingDir(workingDir?: string): string | undefined {
  if (!workingDir || typeof workingDir !== 'string') return undefined;
  try {
    if (fsSync.statSync(workingDir).isDirectory()) {
      return workingDir;
    }
  } catch {
    return undefined;
  }
  return undefined;
}

function shellQuote(value: string): string {
  return `'${value.replace(/'/g, `'\\''`)}'`;
}

type TerminalBackend = 'pty' | 'process';

type TerminalCreateOptions = {
  workingDir?: string;
  cols?: number;
  rows?: number;
};

type TerminalSession = {
  backend: TerminalBackend;
  cwd: string;
  /**
   * `webContents.id` of the renderer that created this session. A session id is
   * an unguessable UUID, but ids leak (devtools, logs, crash dumps) and a
   * session drives a real shell -- so authorize on the caller, not on knowledge
   * of the id.
   */
  ownerId: number;
  write: (data: string) => void;
  resize: (cols: number, rows: number) => void;
  dispose: () => void;
  removeOwnerDestroyedListener: () => void;
};

type NodePtyModule = typeof import('node-pty');

const terminalSessions = new Map<string, TerminalSession>();
const MAX_TERMINAL_SESSIONS_PER_OWNER = 8;
let nodePtyModule: NodePtyModule | null | undefined;

function terminalSize(value: unknown, fallback: number, min: number, max: number): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) return fallback;
  return Math.min(max, Math.max(min, Math.floor(value)));
}

function terminalShell(): { shellPath: string; ptyArgs: string[]; processArgs: string[] } {
  if (process.platform === 'win32') {
    return {
      shellPath: process.env.ComSpec || 'cmd.exe',
      ptyArgs: [],
      processArgs: [],
    };
  }
  const shellPath = process.env.SHELL || '/bin/zsh';
  return {
    shellPath,
    ptyArgs: shellPath.endsWith('zsh') ? ['-l'] : [],
    processArgs: shellPath.endsWith('zsh') ? ['-i'] : [],
  };
}

function terminalEnv(): Record<string, string | undefined> {
  return {
    ...SPAWN_ENV,
    COLORTERM: 'truecolor',
    FORCE_COLOR: '1',
    TERM: 'xterm-256color',
  };
}

async function loadNodePty(): Promise<NodePtyModule | null> {
  if (nodePtyModule !== undefined) return nodePtyModule;
  try {
    nodePtyModule = await import('node-pty');
  } catch (error) {
    log.warn('[terminal] node-pty unavailable; falling back to process pipes:', error);
    nodePtyModule = null;
  }
  return nodePtyModule;
}

function disposeTerminalSession(sessionId: string) {
  const session = terminalSessions.get(sessionId);
  if (!session) return false;
  terminalSessions.delete(sessionId);
  session.removeOwnerDestroyedListener();
  try {
    session.dispose();
  } catch (error) {
    log.warn('[terminal] failed to dispose session:', error);
  }
  return true;
}

function registerCliInstallHandlers() {
  // Is the `biorouter` command callable from a terminal, and is it current?
  //
  // Reports both the bundled version (what this app ships) and the on-PATH
  // version (what `biorouter` resolves to in a terminal). They can differ:
  //   • macOS/Linux GUI installs symlink into the app bundle, so they usually
  //     auto-upgrade when the app is replaced in place;
  //   • Windows installs *copy* the binary, so they go stale on upgrade;
  //   • a standalone .deb/.rpm CLI is a real binary in /usr/bin that a GUI
  //     upgrade can never touch;
  //   • a symlink can dangle if the old app bundle was moved/removed.
  // `needsUpdate` is what the renderer uses to offer an upgrade in all of these
  // cases — re-running the installer (`cli:install`) overwrites the entry.
  ipcMain.handle('cli:status', async () => {
    let bundled: string | null = null;
    try {
      bundled = getBiorouterCliBinaryPath(app);
    } catch (e) {
      log.warn('[cli:status] bundled CLI not found:', (e as Error).message);
      bundled = null;
    }
    const probe = spawnSync(process.platform === 'win32' ? 'where' : 'which', ['biorouter'], {
      encoding: 'utf8',
      env: SPAWN_ENV,
      shell: process.platform === 'win32',
    });
    const pathLocation =
      probe.status === 0 && (probe.stdout || '').trim().length > 0
        ? (probe.stdout || '').trim().split(/\r?\n/)[0].trim()
        : null;

    const bundledVersion = bundled ? cliVersionOf(bundled) : null;
    // Resolve the on-PATH binary's version. A dangling symlink / broken binary
    // yields null here even though `which` found a name — treat that as "not
    // really installed" so the user is still offered the install.
    const pathVersion = pathLocation ? cliVersionOf('biorouter') : null;
    const onPath = pathVersion !== null;

    const needsUpdate =
      onPath &&
      bundledVersion !== null &&
      pathVersion !== null &&
      isVersionOlder(pathVersion, bundledVersion);

    return {
      bundled,
      onPath,
      pathLocation,
      bundledVersion,
      pathVersion,
      needsUpdate,
      // `which` found a name but it won't run — a broken/dangling install.
      brokenOnPath: pathLocation !== null && pathVersion === null,
    };
  });

  ipcMain.handle('terminal:create', async (event, options?: TerminalCreateOptions) => {
    const cwd = usableWorkingDir(options?.workingDir) || os.homedir();
    const cols = terminalSize(options?.cols, 80, 24, 500);
    const rows = terminalSize(options?.rows, 18, 8, 200);
    const { shellPath, ptyArgs, processArgs } = terminalShell();
    const sessionId = crypto.randomUUID();
    const owner = event.sender;
    let didExit = false;

    const registerSession = (
      session: Omit<TerminalSession, 'removeOwnerDestroyedListener'>
    ): void => {
      const handleOwnerDestroyed = () => disposeTerminalSession(sessionId);
      owner.once('destroyed', handleOwnerDestroyed);
      terminalSessions.set(sessionId, {
        ...session,
        removeOwnerDestroyedListener: () => {
          owner.removeListener('destroyed', handleOwnerDestroyed);
        },
      });
    };

    const sendData = (data: string) => {
      if (!owner.isDestroyed()) {
        owner.send('terminal:data', { sessionId, data });
      }
    };
    const sendExit = (exitCode: number | null, signal?: string | number | null) => {
      if (didExit) return;
      didExit = true;
      const session = terminalSessions.get(sessionId);
      terminalSessions.delete(sessionId);
      session?.removeOwnerDestroyedListener();
      if (!owner.isDestroyed()) {
        owner.send('terminal:exit', {
          sessionId,
          exitCode,
          signal: signal === null || typeof signal === 'undefined' ? null : String(signal),
        });
      }
    };

    try {
      const pty = await loadNodePty();
      const ownerSessionCount = Array.from(terminalSessions.values()).filter(
        (session) => session.ownerId === owner.id
      ).length;
      if (ownerSessionCount >= MAX_TERMINAL_SESSIONS_PER_OWNER) {
        return {
          success: false,
          error: `A window can run at most ${MAX_TERMINAL_SESSIONS_PER_OWNER} terminal sessions.`,
        };
      }
      if (pty) {
        const ptyProcess = pty.spawn(shellPath, ptyArgs, {
          cols,
          cwd,
          env: terminalEnv(),
          name: 'xterm-256color',
          rows,
        });
        const dataDisposer = ptyProcess.onData(sendData);
        const exitDisposer = ptyProcess.onExit(({ exitCode, signal }) => {
          dataDisposer.dispose();
          exitDisposer.dispose();
          sendExit(exitCode, signal);
        });
        registerSession({
          backend: 'pty',
          cwd,
          ownerId: owner.id,
          write: (data) => ptyProcess.write(data),
          resize: (nextCols, nextRows) => {
            ptyProcess.resize(
              terminalSize(nextCols, cols, 24, 500),
              terminalSize(nextRows, rows, 8, 200)
            );
          },
          dispose: () => {
            didExit = true;
            dataDisposer.dispose();
            exitDisposer.dispose();
            ptyProcess.kill();
          },
        });
        return { success: true, sessionId, cwd, backend: 'pty' as const };
      }

      const child = spawn(shellPath, processArgs, {
        cwd,
        env: terminalEnv(),
        stdio: 'pipe',
        windowsHide: true,
      });
      child.stdout.setEncoding('utf8');
      child.stderr.setEncoding('utf8');
      const handleStdout = (data: string) => sendData(data);
      const handleStderr = (data: string) => sendData(data);
      const handleClose = (code: number | null, signal: string | null) => sendExit(code, signal);
      const handleError = (error: Error) => {
        sendData(`\r\n${error.message}\r\n`);
        sendExit(1, null);
      };
      child.stdout.on('data', handleStdout);
      child.stderr.on('data', handleStderr);
      child.on('close', handleClose);
      child.on('error', handleError);
      registerSession({
        backend: 'process',
        cwd,
        ownerId: owner.id,
        write: (data) => {
          child.stdin.write(data);
        },
        resize: () => {},
        dispose: () => {
          didExit = true;
          child.stdout.removeListener('data', handleStdout);
          child.stderr.removeListener('data', handleStderr);
          child.removeListener('close', handleClose);
          child.removeListener('error', handleError);
          child.kill();
        },
      });
      return { success: true, sessionId, cwd, backend: 'process' as const };
    } catch (error) {
      return { success: false, error: (error as Error).message };
    }
  });

  // A session may only be driven by the renderer that created it. Without this,
  // any window holding a session id could write into (or kill) another window's
  // shell.
  const ownedTerminalSession = (event: Electron.IpcMainInvokeEvent, sessionId: string) => {
    const session = terminalSessions.get(sessionId);
    if (!session || session.ownerId !== event.sender.id) return null;
    return session;
  };

  ipcMain.handle('terminal:write', async (event, sessionId: string, data: string) => {
    const session = ownedTerminalSession(event, sessionId);
    if (!session) return { success: false, error: 'Terminal session is no longer running.' };
    session.write(data);
    return { success: true };
  });

  ipcMain.handle(
    'terminal:resize',
    async (event, sessionId: string, cols: number, rows: number) => {
      const session = ownedTerminalSession(event, sessionId);
      if (!session) return { success: false, error: 'Terminal session is no longer running.' };
      session.resize(cols, rows);
      return { success: true };
    }
  );

  ipcMain.handle('terminal:dispose', async (event, sessionId: string) => {
    if (!ownedTerminalSession(event, sessionId)) {
      return { success: false, error: 'Terminal session is no longer running.' };
    }
    disposeTerminalSession(sessionId);
    return { success: true };
  });

  // Launch the installed CLI in the user's terminal app. Assumes `biorouter`
  // is already on PATH (the renderer checks `cli:status` first and offers the
  // install flow otherwise).
  ipcMain.handle('cli:launch', async (_event, workingDir?: string) => {
    // Launch the CLI in `workingDir` when supplied (the chat's working
    // directory) so the terminal opens in the exact folder the user is
    // working in, rather than the terminal's default/home directory. Only
    // honor an existing directory; fall back to no `cd` otherwise.
    const cwd = usableWorkingDir(workingDir);
    try {
      if (process.platform === 'darwin') {
        // Open Terminal.app with `do script`, which runs the literal `biorouter`
        // command in a new window (prefixed with a `cd` into the working
        // directory). This is transparent — the user sees `biorouter` run, not a
        // generated helper script — and relies on the CLI already being on PATH.
        const doScript = cwd ? `cd ${shellQuote(cwd)} && biorouter` : 'biorouter';
        // Escape for the AppleScript string literal: backslashes first, then
        // double quotes.
        const asLiteral = doScript.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
        const res = spawnSync(
          'osascript',
          [
            '-e',
            `tell application "Terminal" to do script "${asLiteral}"`,
            '-e',
            'tell application "Terminal" to activate',
          ],
          { encoding: 'utf8', env: SPAWN_ENV, timeout: 15_000 }
        );
        if (res.status !== 0) {
          return { success: false, error: (res.stderr || 'Failed to open Terminal').trim() };
        }
        return { success: true };
      }

      if (process.platform === 'win32') {
        // `start` opens a new console window that keeps running the CLI.
        // `/d <dir>` sets that window's starting directory.
        const startArgs = ['/c', 'start', 'Biorouter CLI'];
        if (cwd) {
          startArgs.push('/d', cwd);
        }
        startArgs.push('cmd', '/k', 'biorouter');
        const child = spawn('cmd.exe', startArgs, {
          env: SPAWN_ENV,
          detached: true,
          stdio: 'ignore',
        });
        child.unref();
        return { success: true };
      }

      // Linux: walk the common terminal emulators and use the first available.
      const candidates: [string, string[]][] = [
        ['x-terminal-emulator', ['-e', 'biorouter']],
        ['gnome-terminal', ['--', 'biorouter']],
        ['konsole', ['-e', 'biorouter']],
        ['xfce4-terminal', ['-e', 'biorouter']],
        ['kitty', ['biorouter']],
        ['alacritty', ['-e', 'biorouter']],
        ['xterm', ['-e', 'biorouter']],
      ];
      for (const [term, args] of candidates) {
        const found = spawnSync('which', [term], { encoding: 'utf8', env: SPAWN_ENV });
        if (found.status === 0 && (found.stdout || '').trim()) {
          const child = spawn(term, args, {
            env: SPAWN_ENV,
            detached: true,
            stdio: 'ignore',
            ...(cwd ? { cwd } : {}),
          });
          child.unref();
          return { success: true };
        }
      }
      return {
        success: false,
        error: 'No terminal emulator found. Run `biorouter` from your terminal instead.',
      };
    } catch (e) {
      return { success: false, error: (e as Error).message };
    }
  });

  // Install the bundled CLI onto PATH by delegating to `biorouter setup-path`.
  ipcMain.handle('cli:install', async () => {
    let cli: string;
    try {
      cli = getBiorouterCliBinaryPath(app);
    } catch (e) {
      return { success: false, error: `Bundled CLI not found: ${(e as Error).message}` };
    }
    const res = spawnSync(cli, ['setup-path'], {
      encoding: 'utf8',
      env: SPAWN_ENV,
      timeout: 60_000,
    });
    if (res.status === 0) {
      return { success: true, output: (res.stdout || '').trim() };
    }
    return {
      success: false,
      error: (res.stderr || res.stdout || `setup-path exited with ${res.status}`).trim(),
    };
  });
}

const createNewWindow = async (app: App, dir?: string | null) => {
  const recentDirs = loadRecentDirs();
  const openDir = dir || (recentDirs.length > 0 ? recentDirs[0] : undefined);
  return await createChat(app, undefined, openDir);
};

const focusWindow = () => {
  const windows = BrowserWindow.getAllWindows();
  if (windows.length > 0) {
    windows.forEach((win) => {
      win.show();
    });
    windows[windows.length - 1].webContents.send('focus-input');
  } else {
    createNewWindow(app);
  }
};

/**
 * "New Chat" — Cmd+T, the browser's new-tab key.
 *
 * It must be a menu item, not a renderer keydown listener, and this is not a
 * style preference: an Electron menu accelerator is consumed by the menu before
 * the web contents ever sees the key, so a listener in the renderer could never
 * have won. Cmd+T was already claimed here (it sent `set-view ''`, which merely
 * navigated Home), which is exactly the trap `role: 'close'` set for Cmd+W —
 * a key silently owned by the menu, with the renderer helpless.
 *
 * The renderer decides what "new tab" means (chatGroups' newTabRegistry); if it
 * has no tab surface mounted it navigates to /pair and opens one there, so
 * Cmd+T works from Settings exactly as it does in a browser from any page.
 *
 * Prefer the window Electron hands the click over getFocusedWindow(), for the
 * reason the Close Tab item documents: the accelerator fires FOR a window, and
 * getFocusedWindow() returns null often enough (e.g. under an automation
 * driver) that relying on it alone makes the key silently do nothing.
 */
function newChatTabItem(label: string, accelerator?: string): MenuItemConstructorOptions {
  return {
    label,
    ...(accelerator ? { accelerator } : {}),
    click(_item, browserWindow) {
      const target =
        browserWindow instanceof BrowserWindow ? browserWindow : BrowserWindow.getFocusedWindow();
      target?.webContents.send('new-chat-tab');
    },
  };
}

function buildApplicationMenu() {
  const isMac = process.platform === 'darwin';

  // Find submenu — inserted into Edit after Select All (roles don't allow inline custom items)
  const findSubmenu: MenuItemConstructorOptions[] = [
    {
      label: 'Find…',
      accelerator: isMac ? 'Command+F' : 'Control+F',
      click() {
        BrowserWindow.getFocusedWindow()?.webContents.send('find-command');
      },
    },
    {
      label: 'Find Next',
      accelerator: isMac ? 'Command+G' : 'Control+G',
      click() {
        BrowserWindow.getFocusedWindow()?.webContents.send('find-next');
      },
    },
    {
      label: 'Find Previous',
      accelerator: isMac ? 'Shift+Command+G' : 'Shift+Control+G',
      click() {
        BrowserWindow.getFocusedWindow()?.webContents.send('find-previous');
      },
    },
    ...(isMac
      ? [
          {
            label: 'Use Selection for Find',
            accelerator: 'Command+E',
            click() {
              BrowserWindow.getFocusedWindow()?.webContents.send('use-selection-find');
            },
          } as MenuItemConstructorOptions,
        ]
      : []),
  ];

  const template: MenuItemConstructorOptions[] = [
    // ── Biorouter app menu (macOS only) ──────────────────────────────────
    ...(isMac
      ? [
          {
            label: 'Biorouter',
            submenu: [
              { role: 'about' as const },
              { type: 'separator' as const },
              {
                label: 'Settings',
                accelerator: 'CmdOrCtrl+,',
                click() {
                  BrowserWindow.getFocusedWindow()?.webContents.send('set-view', 'settings');
                },
              },
              { type: 'separator' as const },
              {
                label: 'Check for Updates…',
                click: openUpdateSettings,
              },
              {
                label: 'Check for Dependencies…',
                click() {
                  triggerDependencyCheck();
                },
              },
              {
                label: 'Check for Extension Updates',
                click() {
                  runExtensionUpdateCheck();
                },
              },
              { type: 'separator' as const },
              { role: 'quit' as const, label: 'Quit Biorouter' },
            ],
          } as MenuItemConstructorOptions,
        ]
      : []),

    // ── Go ────────────────────────────────────────────────────────────────
    {
      label: 'Go',
      submenu: [
        {
          label: 'Home',
          accelerator: 'CmdOrCtrl+1',
          click() {
            BrowserWindow.getFocusedWindow()?.webContents.send('set-view', '');
          },
        },
        newChatTabItem('New Chat', 'CmdOrCtrl+T'),
        {
          label: 'History',
          accelerator: 'CmdOrCtrl+2',
          click() {
            BrowserWindow.getFocusedWindow()?.webContents.send('set-view', 'sessions');
          },
        },
        { type: 'separator' as const },
        {
          label: 'Workflows',
          accelerator: 'CmdOrCtrl+3',
          click() {
            BrowserWindow.getFocusedWindow()?.webContents.send('set-view', 'workflows');
          },
        },
        {
          label: 'Scheduler',
          accelerator: 'CmdOrCtrl+4',
          click() {
            BrowserWindow.getFocusedWindow()?.webContents.send('set-view', 'schedules');
          },
        },
        { type: 'separator' as const },
        {
          label: 'Extensions',
          accelerator: 'CmdOrCtrl+5',
          click() {
            BrowserWindow.getFocusedWindow()?.webContents.send('set-view', 'extensions');
          },
        },
        {
          label: 'Skills',
          accelerator: 'CmdOrCtrl+6',
          click() {
            BrowserWindow.getFocusedWindow()?.webContents.send('set-view', 'skills');
          },
        },
      ],
    },

    // ── File ─────────────────────────────────────────────────────────────
    {
      label: 'File',
      submenu: [
        // Same item, no accelerator — Go owns the key, File carries the
        // discoverable duplicate. Both must do the same thing or the menu lies.
        newChatTabItem('New Chat'),
        {
          label: 'New Window',
          accelerator: isMac ? 'Cmd+N' : 'Ctrl+N',
          click() {
            ipcMain.emit('create-chat-window');
          },
        },
        { type: 'separator' as const },
        {
          label: 'Open Directory…',
          accelerator: 'CmdOrCtrl+O',
          click: () => openDirectoryDialog(),
        },
        ...(() => {
          const recentFiles = buildRecentFilesMenu();
          return recentFiles.length > 0
            ? [{ label: 'Recent Directories', submenu: recentFiles } as MenuItemConstructorOptions]
            : [];
        })(),
        { type: 'separator' as const },
        // Cmd+W closes the TAB, Shift+Cmd+W closes the window — Safari/Chrome's
        // split, and now ours, because /pair is a tabbed surface.
        //
        // This item must exist. `role: 'close'` silently claims CmdOrCtrl+W as
        // its default accelerator, and a menu accelerator is consumed before the
        // renderer sees the keydown — so no amount of renderer-side key handling
        // could have closed a tab instead. It would have closed the window and
        // every tab in it. The renderer decides what to do (chatGroups'
        // closeActiveTabRegistry); if it has no tab to close it calls
        // 'close-window' itself, so a tabless route still closes on Cmd+W.
        {
          label: 'Close Tab',
          accelerator: 'CmdOrCtrl+W',
          // Prefer the window Electron hands the click over getFocusedWindow():
          // the accelerator fires FOR a window, and that window is the honest
          // target. getFocusedWindow() is only the fallback (and it returns null
          // often enough — e.g. under an automation driver — that relying on it
          // alone makes Cmd+W silently do nothing).
          click(_item, browserWindow) {
            const target =
              browserWindow instanceof BrowserWindow
                ? browserWindow
                : BrowserWindow.getFocusedWindow();
            target?.webContents.send('close-active-tab');
          },
        },
        { role: 'close' as const, label: 'Close Window', accelerator: 'Shift+CmdOrCtrl+W' },
        {
          label: 'Focus Biorouter Window',
          accelerator: 'CmdOrCtrl+Alt+G',
          click() {
            focusWindow();
          },
        },
      ],
    },

    // ── Edit (standard roles + Find inserted after build) ─────────────
    { role: 'editMenu' as const },

    // ── Extensions ───────────────────────────────────────────────────────
    {
      label: 'Extensions',
      submenu: [
        {
          label: 'Install Extension (.brxt)',
          click() {
            BrowserWindow.getFocusedWindow()?.webContents.send('set-view', 'extensions');
          },
        },
        {
          label: 'Browse Extensions',
          click() {
            shell.openExternal('http://biorouter.ucsf.edu/baam');
          },
        },
        {
          label: 'Add Custom Extension…',
          click() {
            BrowserWindow.getFocusedWindow()?.webContents.send('set-view', 'extensions');
          },
        },
        { type: 'separator' as const },
        {
          label: 'Check for Extension Updates',
          click() {
            runExtensionUpdateCheck();
          },
        },
      ],
    },

    // ── Providers ────────────────────────────────────────────────────────
    {
      label: 'Providers',
      submenu: [
        {
          label: 'Configure Providers…',
          click() {
            BrowserWindow.getFocusedWindow()?.webContents.send('set-view', 'configure-providers');
          },
        },
        {
          label: 'Switch Model…',
          click() {
            BrowserWindow.getFocusedWindow()?.webContents.send('set-view', 'settings', 'models');
          },
        },
        {
          label: 'Reset Provider',
          click() {
            BrowserWindow.getFocusedWindow()?.webContents.send('set-view', 'configure-providers');
          },
        },
      ],
    },

    // ── View (theme toggles + standard Electron view roles) ──────────────
    {
      label: 'View',
      submenu: [
        {
          label: 'Light Mode',
          click() {
            BrowserWindow.getAllWindows().forEach((w) =>
              w.webContents.send('theme-changed', { theme: 'light', useSystemTheme: false })
            );
          },
        },
        {
          label: 'Dark Mode',
          click() {
            BrowserWindow.getAllWindows().forEach((w) =>
              w.webContents.send('theme-changed', { theme: 'dark', useSystemTheme: false })
            );
          },
        },
        {
          label: 'System Mode',
          click() {
            // useSystemTheme: true — ThemeContext reads OS preference and ignores the theme field
            BrowserWindow.getAllWindows().forEach((w) =>
              w.webContents.send('theme-changed', { theme: 'light', useSystemTheme: true })
            );
          },
        },
        { type: 'separator' as const },
        { role: 'reload' as const },
        { role: 'forceReload' as const },
        { role: 'toggleDevTools' as const },
        { type: 'separator' as const },
        { role: 'resetZoom' as const },
        { role: 'zoomIn' as const },
        { role: 'zoomOut' as const },
        { type: 'separator' as const },
        { role: 'togglefullscreen' as const },
      ],
    },

    // ── Help ─────────────────────────────────────────────────────────────
    {
      label: 'Help',
      submenu: [
        {
          label: 'Biorouter Documentation',
          click() {
            shell.openExternal('http://biorouter.ucsf.edu/docs');
          },
        },
        { type: 'separator' as const },
        {
          label: 'Report a Bug…',
          click() {
            shell.openExternal(
              'https://github.com/BaranziniLab/biorouter/issues/new?template=bug_report.md'
            );
          },
        },
        {
          label: 'Request a Feature…',
          click() {
            shell.openExternal(
              'https://github.com/BaranziniLab/biorouter/issues/new?template=feature_request.md'
            );
          },
        },
        { type: 'separator' as const },
        { label: `v${version || app.getVersion()}`, enabled: false },
      ],
    },

    // ── Window (standard roles; Always on Top added after build) ─────────
    { role: 'windowMenu' as const },
  ];

  const menu = Menu.buildFromTemplate(template);

  // Insert Find submenu into Edit after Select All
  // (role: 'editMenu' expands to system defaults; custom items can't be inlined)
  const editMenu = menu.items.find((item) => item.label === 'Edit');
  if (editMenu?.submenu) {
    const selectAllIndex = editMenu.submenu.items.findIndex((item) => item.label === 'Select All');
    if (selectAllIndex >= 0) {
      editMenu.submenu.insert(
        selectAllIndex + 1,
        new MenuItem({ label: 'Find', submenu: Menu.buildFromTemplate(findSubmenu) })
      );
    }
  }

  // Add Always on Top to Window menu
  const windowMenu = menu.items.find((item) => item.label === 'Window');
  if (windowMenu?.submenu) {
    windowMenu.submenu.append(
      new MenuItem({
        label: 'Always on Top',
        type: 'checkbox',
        accelerator: isMac ? 'Cmd+Shift+T' : 'Ctrl+Shift+T',
        click(menuItem) {
          const win = BrowserWindow.getFocusedWindow();
          if (!win) return;
          const alwaysOnTop = menuItem.checked;
          if (isMac) {
            win.setAlwaysOnTop(alwaysOnTop, 'floating');
          } else {
            win.setAlwaysOnTop(alwaysOnTop);
          }
        },
      })
    );
  }

  Menu.setApplicationMenu(menu);
}

/**
 * Re-claim `biorouter://` if something else holds it.
 *
 * Older dev builds registered the bare Electron shell as the handler, which
 * silently broke every shared workflow link on that machine — the shell has no
 * app to run, so it launches and exits. Re-asserting on each packaged launch
 * heals those machines without the user having to know any of this.
 */
function ensureDeepLinkHandler() {
  if (!app.isPackaged) return;
  try {
    if (app.isDefaultProtocolClient('biorouter')) return;
    const reclaimed = app.setAsDefaultProtocolClient('biorouter');
    log.info(
      `[Main] biorouter:// was claimed by another app; reclaim ${reclaimed ? 'ok' : 'failed'}`
    );
  } catch (error) {
    log.warn('[Main] Could not verify biorouter:// handler registration:', error);
  }
}

async function appMain() {
  await configureProxy();

  ensureDeepLinkHandler();

  // Ensure Windows shims are available before any MCP processes are spawned
  await ensureWinShims();

  registerUpdateIpcHandlers();
  registerDependencyIpcHandlers();
  registerCliInstallHandlers();

  const appEntryUrl = rendererEntryUrl();
  session.defaultSession.setPermissionCheckHandler(
    (_webContents, permission, requestingOrigin, details) =>
      isAllowedRendererPermission(
        permission,
        details.requestingUrl || requestingOrigin,
        appEntryUrl,
        [details.mediaType ?? 'unknown']
      )
  );
  session.defaultSession.setPermissionRequestHandler(
    (_webContents, permission, callback, details) => {
      const mediaTypes = 'mediaTypes' in details ? (details.mediaTypes ?? []) : [];
      callback(
        isAllowedRendererPermission(permission, details.requestingUrl, appEntryUrl, mediaTypes)
      );
    }
  );

  const buildConnectSrc = (): string => {
    const sources = [
      "'self'",
      'http://127.0.0.1:*',
      'https://api.github.com',
      'https://github.com',
      'https://objects.githubusercontent.com',
    ];

    const settings = loadSettings();
    if (settings.externalBiorouterd?.enabled && settings.externalBiorouterd.url) {
      try {
        const externalUrl = new URL(settings.externalBiorouterd.url);
        sources.push(externalUrl.origin);
      } catch {
        console.warn('Invalid external biorouterd URL in settings, skipping CSP entry');
      }
    }

    return sources.join(' ');
  };

  // Add CSP headers to all sessions
  session.defaultSession.webRequest.onHeadersReceived((details, callback) => {
    // Standalone artifact files contain a sandboxed srcdoc preview. Its inline
    // chart runtime must execute, but the artifact must not fetch remote code,
    // beacon data, or connect to local services.
    const isArtifactWindow =
      details.url.startsWith('file://') && details.url.includes('biorouter-artifacts');

    const csp = isArtifactWindow
      ? ARTIFACT_WRAPPER_CSP
      : "default-src 'self';" +
        "style-src 'self' 'unsafe-inline';" +
        "script-src 'self';" +
        "img-src 'self' data: blob: https:;" +
        `connect-src ${buildConnectSrc()};` +
        "object-src 'none';" +
        "frame-src 'self' blob: https: http:;" +
        "font-src 'self' data: https:;" +
        "media-src 'self' mediastream:;" +
        "form-action 'none';" +
        "base-uri 'self';" +
        "manifest-src 'self';" +
        "worker-src 'self';" +
        'upgrade-insecure-requests;';

    callback({
      responseHeaders: {
        ...details.responseHeaders,
        'Content-Security-Policy': csp,
      },
    });
  });

  try {
    globalShortcut.register('CommandOrControl+Alt+Shift+G', () => {
      createLauncher();
    });
  } catch (e) {
    console.error('Error registering launcher hotkey:', e);
  }

  try {
    globalShortcut.register('CommandOrControl+Alt+G', () => {
      focusWindow();
    });
  } catch (e) {
    console.error('Error registering focus window hotkey:', e);
  }

  session.defaultSession.webRequest.onBeforeSendHeaders((details, callback) => {
    details.requestHeaders['Origin'] = 'http://localhost:5173';
    callback({ cancel: false, requestHeaders: details.requestHeaders });
  });

  // Create tray if enabled in settings
  const settings = loadSettings();
  if (settings.showMenuBarIcon) {
    createTray();
  }

  // Handle dock icon visibility (macOS only)
  if (process.platform === 'darwin' && !settings.showDockIcon && settings.showMenuBarIcon) {
    app.dock?.hide();
  }

  const { dirPath } = parseArgs();

  if (!openUrlHandledLaunch) {
    await createNewWindow(app, dirPath);
  } else {
    log.info('[Main] Skipping window creation in appMain - open-url already handled launch');
  }

  // Setup auto-updater AFTER window is created and displayed (with delay to avoid blocking)
  setTimeout(() => {
    if (shouldSetupUpdater()) {
      log.info('Setting up auto-updater after window creation...');
      try {
        setupAutoUpdater();
      } catch (error) {
        log.error('Error setting up auto-updater:', error);
      }
    }
  }, 2000); // 2 second delay after window is shown

  // Dependency check: runs 4s after window is ready (see dependencyChecker.ts)
  setupDependencyChecker();

  // Extension update check: runs 8s after window is ready (see extensionUpdater.ts)
  scheduleExtensionUpdateCheck();

  // Setup macOS dock menu
  if (process.platform === 'darwin') {
    const dockMenu = Menu.buildFromTemplate([
      {
        label: 'New Window',
        click: () => {
          createNewWindow(app);
        },
      },
    ]);
    app.dock?.setMenu(dockMenu);
  }

  buildApplicationMenu();

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createNewWindow(app);
    }
  });

  ipcMain.on(
    'create-chat-window',
    async (_, query, dir, version, resumeSessionId, viewType, workflowId) => {
      if (!dir?.trim()) {
        const recentDirs = loadRecentDirs();
        dir = recentDirs.length > 0 ? recentDirs[0] : undefined;
      }

      // Offset the new window from the one that triggered it (e.g. the Diverge
      // button) so it's clearly a distinct second window, then bring it to the
      // front — the originating window stays exactly where it is.
      const anchor = BrowserWindow.getFocusedWindow() ?? BrowserWindow.getAllWindows()[0];
      const win = await createChat(
        app,
        query,
        dir,
        version,
        resumeSessionId,
        viewType,
        undefined,
        undefined,
        workflowId
      );
      if (win) {
        if (anchor && anchor !== win && !anchor.isDestroyed()) {
          const b = anchor.getBounds();
          win.setBounds({ x: b.x + 40, y: b.y + 40, width: b.width, height: b.height });
        }
        win.show();
        win.focus();
        win.moveTop();
      }
    }
  );

  ipcMain.on('create-diverged-chat-window', async (event, dir, resumeSessionId) => {
    if (!resumeSessionId) {
      log.error('[Main] create-diverged-chat-window missing session id');
      return;
    }
    if (!dir?.trim()) {
      const recentDirs = loadRecentDirs();
      dir = recentDirs.length > 0 ? recentDirs[0] : undefined;
    }
    const senderWindow = BrowserWindow.fromWebContents(event.sender);
    await openDivergedChatWindow(resumeSessionId, dir, senderWindow);
  });

  ipcMain.on('close-window', (event) => {
    const window = BrowserWindow.fromWebContents(event.sender);
    if (window && !window.isDestroyed()) {
      window.close();
    }
  });

  ipcMain.on('notify', (event, data) => {
    try {
      // Validate notification data
      if (!data || typeof data !== 'object') {
        console.error('Invalid notification data');
        return;
      }

      // Validate title and body
      if (typeof data.title !== 'string' || typeof data.body !== 'string') {
        console.error('Invalid notification title or body');
        return;
      }

      // Limit the length of title and body
      const MAX_LENGTH = 1000;
      if (data.title.length > MAX_LENGTH || data.body.length > MAX_LENGTH) {
        console.error('Notification title or body too long');
        return;
      }

      // Remove any HTML tags for security
      const sanitizeText = (text: string) => text.replace(/<[^>]*>/g, '');

      console.log('NOTIFY', data);
      const notification = new Notification({
        title: sanitizeText(data.title),
        body: sanitizeText(data.body),
      });

      // Add click handler to focus the window
      notification.on('click', () => {
        const window = BrowserWindow.fromWebContents(event.sender);
        if (window) {
          if (window.isMinimized()) {
            window.restore();
          }
          window.show();
          window.focus();
        }
      });

      notification.show();
    } catch (error) {
      console.error('Error showing notification:', error);
    }
  });

  ipcMain.on('logInfo', (_event, info) => {
    try {
      // Validate log info
      if (info === undefined || info === null) {
        console.error('Invalid log info: undefined or null');
        return;
      }

      // Convert to string if not already
      const logMessage = String(info);

      // Limit log message length
      const MAX_LENGTH = 10000; // 10KB limit
      if (logMessage.length > MAX_LENGTH) {
        console.error('Log message too long');
        return;
      }

      // Log the sanitized message
      log.info('from renderer:', logMessage);
    } catch (error) {
      console.error('Error logging info:', error);
    }
  });

  ipcMain.on('broadcast-theme-change', (event, themeData) => {
    const senderWindow = BrowserWindow.fromWebContents(event.sender);
    const allWindows = BrowserWindow.getAllWindows();

    allWindows.forEach((window) => {
      if (window.id !== senderWindow?.id) {
        window.webContents.send('theme-changed', themeData);
      }
    });
  });

  ipcMain.on('reload-app', (event) => {
    // Get the window that sent the event
    const window = BrowserWindow.fromWebContents(event.sender);
    if (window) {
      window.reload();
    }
  });

  // Handle metadata fetching from main process
  ipcMain.handle('fetch-metadata', async (_event, url) => {
    try {
      // Each hop is validated: a public URL that 302s to 127.0.0.1 or
      // 169.254.169.254 would otherwise turn this handler into an SSRF proxy.
      let target = await assertPublicHttpUrl(url);
      let response: Response | undefined;
      for (let hop = 0; hop < 5; hop++) {
        response = await fetch(target.href, {
          redirect: 'manual',
          headers: {
            'User-Agent': 'Mozilla/5.0 (compatible; Biorouter/1.0)',
          },
        });
        const location = response.headers.get('location');
        if (response.status >= 300 && response.status < 400 && location) {
          target = await assertPublicHttpUrl(new URL(location, target).href);
          continue;
        }
        break;
      }
      if (!response) throw new Error('Too many redirects');

      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }

      // Set a reasonable size limit (e.g., 10MB)
      const MAX_SIZE = 10 * 1024 * 1024; // 10MB
      const contentLength = parseInt(response.headers.get('content-length') || '0');
      if (contentLength > MAX_SIZE) {
        throw new Error('Response too large');
      }

      const text = await response.text();
      if (text.length > MAX_SIZE) {
        throw new Error('Response too large');
      }

      return text;
    } catch (error) {
      console.error('Error fetching metadata:', error);
      throw error;
    }
  });

  ipcMain.on('open-in-chrome', (_event, url) => {
    try {
      // Validate URL
      const parsedUrl = new URL(url);

      // Only allow http and https protocols
      if (!['http:', 'https:'].includes(parsedUrl.protocol)) {
        console.error('Invalid URL protocol. Only HTTP and HTTPS are allowed.');
        return;
      }

      // On macOS, use the 'open' command with Chrome
      if (process.platform === 'darwin') {
        spawn('open', ['-a', 'Google Chrome', url]);
      } else if (process.platform === 'win32') {
        // On Windows, start is built-in command of cmd.exe
        spawn('cmd.exe', ['/c', 'start', '', 'chrome', url]);
      } else {
        // On Linux, use xdg-open with chrome
        spawn('xdg-open', [url]);
      }
    } catch (error) {
      console.error('Error opening URL in browser:', error);
    }
  });

  // Handle app restart
  ipcMain.on('restart-app', () => {
    app.relaunch();
    app.exit(0);
  });

  // Handler for getting app version
  ipcMain.on('get-app-version', (event) => {
    event.returnValue = app.getVersion();
  });

  ipcMain.handle('open-directory-in-explorer', async (_event, dirPath: string) => {
    try {
      const expanded = path.resolve(expandBiorouterPath(dirPath));
      const err = await shell.openPath(expanded);
      // shell.openPath returns an empty string on success, error message on failure
      if (err) console.error('Error opening directory in explorer:', err);
      return !err;
    } catch (error) {
      console.error('Error opening directory in explorer:', error);
      return false;
    }
  });

  // Standalone previews are offline. Inline the small, fixed set of libraries
  // emitted by Auto Visualiser before applying its network-denying CSP.
  const artifactCdnAssetCache = new Map<string, Promise<string>>();
  const artifactCdnAssets = [
    'https://cdn.jsdelivr.net/npm/d3@7/dist/d3.min.js',
    'https://cdn.jsdelivr.net/npm/d3-sankey@0.12/dist/d3-sankey.min.js',
    'https://cdn.jsdelivr.net/npm/chart.js@4/dist/chart.umd.min.js',
    'https://cdn.jsdelivr.net/npm/leaflet@1.9.4/dist/leaflet.js',
    'https://cdn.jsdelivr.net/npm/leaflet@1.9.4/dist/leaflet.css',
    'https://cdn.jsdelivr.net/npm/leaflet.markercluster@1.5.3/dist/leaflet.markercluster.js',
  ];
  const escapeRegExp = (value: string): string => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

  const fetchArtifactCdnAsset = (url: string): Promise<string> => {
    let cached = artifactCdnAssetCache.get(url);
    if (!cached) {
      cached = fetch(url).then((response) => {
        if (!response.ok) {
          throw new Error(`HTTP ${response.status}`);
        }
        return response.text();
      });
      artifactCdnAssetCache.set(url, cached);
    }
    return cached;
  };

  const inlineKnownArtifactCdnAssets = async (rawHtml: string): Promise<string> => {
    let html = rawHtml;
    for (const url of artifactCdnAssets) {
      if (!html.includes(url)) continue;
      try {
        const asset = await fetchArtifactCdnAsset(url);
        // Replacement must be a function: minified bundles contain `$&`, `$'`,
        // `$1`, `$$` sequences, which String.replace would expand as match
        // references and corrupt the inlined script.
        if (url.endsWith('.css')) {
          html = html.replace(
            new RegExp(`<link\\b[^>]*href=["']${escapeRegExp(url)}["'][^>]*>`, 'g'),
            () => `<style>${asset}</style>`
          );
        } else {
          html = html.replace(
            new RegExp(`<script\\b[^>]*src=["']${escapeRegExp(url)}["'][^>]*>\\s*</script>`, 'g'),
            () => `<script>${asset}</script>`
          );
        }
      } catch (error) {
        console.warn(`Could not inline artifact CDN asset ${url}:`, error);
      }
    }
    return html;
  };

  type OpenArtifactPayload = {
    html: string;
    title?: string;
    width?: number;
    height?: number;
    theme?: 'light' | 'dark';
  };

  const normalizeArtifactPayload = (payload: unknown): OpenArtifactPayload | null => {
    if (!payload || typeof payload !== 'object') return null;
    const value = payload as Record<string, unknown>;
    if (
      typeof value.html !== 'string' ||
      Buffer.byteLength(value.html, 'utf8') > 16 * 1024 * 1024
    ) {
      return null;
    }
    const title =
      typeof value.title === 'string'
        ? value.title.replace(/[\p{Cc}\p{Cf}]/gu, '').slice(0, 256)
        : undefined;
    const finiteDimension = (dimension: unknown) =>
      typeof dimension === 'number' && Number.isFinite(dimension) ? dimension : undefined;
    return {
      html: value.html,
      title,
      width: finiteDimension(value.width),
      height: finiteDimension(value.height),
      theme: value.theme === 'dark' ? 'dark' : value.theme === 'light' ? 'light' : undefined,
    };
  };

  const prepareArtifactHtml = async (rawHtml: string): Promise<string> => {
    return inlineKnownArtifactCdnAssets(rawHtml);
  };

  ipcMain.handle('prepare-artifact-html', async (_event, payload: unknown) => {
    const normalized = normalizeArtifactPayload(payload);
    if (!normalized) throw new Error('Invalid or oversized artifact preview');
    return { html: await prepareArtifactHtml(normalized.html) };
  });

  let artifactTempDirectoryPromise: Promise<string> | null = null;
  const artifactTempDirectory = (): Promise<string> => {
    artifactTempDirectoryPromise ??= fs
      .mkdtemp(path.join(os.tmpdir(), 'biorouter-artifacts-'))
      .then(async (directory) => {
        await fs.chmod(directory, 0o700);
        app.once('before-quit', () => {
          void fs.rm(directory, { recursive: true, force: true });
        });
        return directory;
      });
    return artifactTempDirectoryPromise;
  };

  const writeArtifactTempFile = async (html: string): Promise<string> => {
    const artifactDir = await artifactTempDirectory();
    const artifactFile = path.join(artifactDir, `artifact-${crypto.randomUUID()}.html`);
    await fs.writeFile(artifactFile, html, { encoding: 'utf-8', mode: 0o600, flag: 'wx' });
    return artifactFile;
  };

  // Open self-contained artifact HTML in a large sandboxed Electron window.
  // Live Agent Drafter apps use their explicit browser link instead.
  const openArtifactInWindow = async (payload: OpenArtifactPayload) => {
    try {
      const isDark = payload.theme === 'dark';
      const html = wrapArtifactForBrowser(
        injectArtifactHostTheme(await prepareArtifactHtml(payload.html), isDark ? 'dark' : 'light')
      );
      const win = new BrowserWindow({
        title: payload.title || 'Biorouter Artifact',
        width: Math.min(Math.max(payload.width || 1000, 480), 1600),
        height: Math.min(Math.max(payload.height || 760, 360), 1200),
        resizable: true,
        // Match the figure's own background so there is no flash before scripts run.
        backgroundColor: isDark ? '#1c1f26' : '#ffffff',
        webPreferences: {
          nodeIntegration: false,
          contextIsolation: true,
          sandbox: true,
          webSecurity: true,
          backgroundThrottling: true,
        },
      });
      // Artifact scripts cannot create windows or trigger the system browser.
      win.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
      const isArtifactPreviewFrame = trackArtifactPreviewFrames(win.webContents);
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      win.webContents.on('will-frame-navigate' as any, (event: any) => {
        if (isArtifactPreviewFrame(event.frame) && !isAllowedArtifactFrameNavigation(event.url)) {
          event.preventDefault();
        }
      });

      // Self-contained artifact HTML can be several megabytes — Auto Visualiser
      // figures inline D3/Chart.js/Leaflet/Mermaid (a Mermaid diagram is ~3.3 MB,
      // ~4.6 MB once percent-encoded). A `data:` URL would exceed Chromium's ~2 MB
      // URL ceiling and silently abort the navigation (net::ERR_ABORTED), leaving a
      // blank window with only the static markup. Write the HTML to a temp file and
      // load it instead: no length limit, and a stable file:// origin so the figure's
      // inline scripts run exactly as they do in the in-chat iframe. The `theme`
      // query mirrors what the in-chat renderer passes so light/dark match.
      const artifactFile = await writeArtifactTempFile(html);
      // Remove the temp file once the window is gone (keeps it available across reloads).
      win.on('closed', () => {
        fs.unlink(artifactFile).catch(() => {});
      });

      await win.loadFile(artifactFile, { query: { theme: isDark ? 'dark' : 'light' } });
      return { ok: true };
    } catch (error) {
      console.error('Error opening artifact window:', error);
      return { ok: false };
    }
  };

  // Open a self-contained, offline artifact preview in the user's default browser.
  // Live Agent Drafter apps are launched through their explicit `/apps/<id>/` link.
  const openArtifactInBrowser = async (payload: OpenArtifactPayload) => {
    try {
      const html = wrapArtifactForBrowser(
        injectArtifactHostTheme(
          await prepareArtifactHtml(payload.html),
          payload.theme === 'dark' ? 'dark' : 'light'
        )
      );
      const artifactFile = await writeArtifactTempFile(html);
      await shell.openExternal(pathToFileURL(artifactFile).href);
      return { ok: true };
    } catch (error) {
      console.error('Error opening artifact in browser:', error);
      return { ok: false };
    }
  };

  ipcMain.handle('open-artifact-window', (_event, payload: unknown) => {
    const normalized = normalizeArtifactPayload(payload);
    return normalized ? openArtifactInWindow(normalized) : { ok: false };
  });
  ipcMain.handle('open-artifact-in-browser', (_event, payload: unknown) => {
    const normalized = normalizeArtifactPayload(payload);
    return normalized ? openArtifactInBrowser(normalized) : { ok: false };
  });

  ipcMain.handle('launch-app', async (event, biorouterApp: BioRouterApp) => {
    try {
      const launchingWindow = BrowserWindow.fromWebContents(event.sender);
      if (!launchingWindow) {
        throw new Error('Could not find launching window');
      }

      const launchingWindowId = launchingWindow.id;
      const launchingClient = biorouterdClients.get(launchingWindowId);
      if (!launchingClient) {
        throw new Error('No client found for launching window');
      }

      const currentUrl = launchingWindow.webContents.getURL();
      const baseUrl = new URL(currentUrl).origin;

      const appWindow = new BrowserWindow({
        title: biorouterApp.name,
        width: biorouterApp.width ?? 800,
        height: biorouterApp.height ?? 600,
        resizable: biorouterApp.resizable ?? true,
        webPreferences: {
          preload: path.join(__dirname, 'preload.js'),
          nodeIntegration: false,
          contextIsolation: true,
          webSecurity: true,
          backgroundThrottling: true,
          partition: 'persist:biorouter',
        },
      });

      biorouterdClients.set(appWindow.id, launchingClient);
      // The app window uses the launcher's backend; retain it so closing the
      // launcher window doesn't kill the backend out from under this app.
      const launcherBackend = windowBackends.get(launchingWindowId);
      if (launcherBackend) retainBackend(appWindow.id, launcherBackend);

      // `closed` (definitive), not `close` (cancelable): a prevented close must
      // not decrement the refcount and tear down a backend still in use.
      appWindow.on('closed', () => {
        biorouterdClients.delete(appWindow.id);
        releaseBackend(appWindow.id);
      });

      const workingDir = app.getPath('home');
      const extensionName = biorouterApp.mcpServer ?? '';
      const standaloneUrl =
        `${baseUrl}/#/standalone-app?` +
        `resourceUri=${encodeURIComponent(biorouterApp.uri)}` +
        `&extensionName=${encodeURIComponent(extensionName)}` +
        `&appName=${encodeURIComponent(biorouterApp.name)}` +
        `&workingDir=${encodeURIComponent(workingDir)}`;

      await appWindow.loadURL(standaloneUrl);
      appWindow.show();
    } catch (error) {
      console.error('Failed to launch app:', error);
      throw error;
    }
  });
}

app.whenReady().then(async () => {
  try {
    if (process.platform === 'darwin') {
      const dockIconPath = resolveImagePath('icon.png');
      if (dockIconPath) app.dock?.setIcon(dockIconPath);
    }
    await appMain();
  } catch (error) {
    dialog.showErrorBox('Biorouter Error', `Failed to create main window: ${error}`);
    app.quit();
  }
});

async function getAllowList(): Promise<string[]> {
  if (!process.env.BIOROUTER_ALLOWLIST) {
    return [];
  }

  const response = await fetch(process.env.BIOROUTER_ALLOWLIST);

  if (!response.ok) {
    throw new Error(
      `Failed to fetch allowed extensions: ${response.status} ${response.statusText}`
    );
  }

  // Parse the YAML content
  const yamlContent = await response.text();
  const parsedYaml = yaml.parse(yamlContent);

  // Extract the commands from the extensions array
  if (parsedYaml && parsedYaml.extensions && Array.isArray(parsedYaml.extensions)) {
    const commands = parsedYaml.extensions.map(
      (ext: { id: string; command: string }) => ext.command
    );
    console.log(`Fetched ${commands.length} allowed extension commands`);
    return commands;
  } else {
    console.error('Invalid YAML structure:', parsedYaml);
    return [];
  }
}

app.on('will-quit', async () => {
  for (const [windowId, blockerId] of windowPowerSaveBlockers.entries()) {
    try {
      powerSaveBlocker.stop(blockerId);
      console.log(
        `[Main] Stopped power save blocker ${blockerId} for window ${windowId} during app quit`
      );
    } catch (error) {
      console.error(
        `[Main] Failed to stop power save blocker ${blockerId} for window ${windowId}:`,
        error
      );
    }
  }
  windowPowerSaveBlockers.clear();

  // Unregister all shortcuts when quitting
  globalShortcut.unregisterAll();

  try {
    await fs.access(biorouterTempDir); // Check if directory exists to avoid error on fs.rm if it doesn't

    // First, check for any symlinks in the directory and refuse to delete them
    let hasSymlinks = false;
    try {
      const files = await fs.readdir(biorouterTempDir);
      for (const file of files) {
        const filePath = path.join(biorouterTempDir, file);
        const stats = await fs.lstat(filePath);
        if (stats.isSymbolicLink()) {
          console.warn(`[Main] Found symlink in temp directory: ${filePath}. Skipping deletion.`);
          hasSymlinks = true;
          // Delete the individual file but leave the symlink
          continue;
        }

        // Delete regular files individually
        if (stats.isFile()) {
          await fs.unlink(filePath);
        }
      }

      // If no symlinks were found, it's safe to remove the directory
      if (!hasSymlinks) {
        await fs.rm(biorouterTempDir, { recursive: true, force: true });
        console.log('[Main] Pasted images temp directory cleaned up successfully.');
      } else {
        console.log(
          '[Main] Cleaned up files in temp directory but left directory intact due to symlinks.'
        );
      }
    } catch (err) {
      console.error('[Main] Error while cleaning up temp directory contents:', err);
    }
  } catch (error) {
    if (error && typeof error === 'object' && 'code' in error && error.code === 'ENOENT') {
      console.log('[Main] Temp directory did not exist during "will-quit", no cleanup needed.');
    } else {
      console.error(
        '[Main] Failed to clean up pasted images temp directory during "will-quit":',
        error
      );
    }
  }
});

app.on('window-all-closed', () => {
  // Only quit if we're not on macOS or don't have a tray icon
  if (process.platform !== 'darwin' || !tray) {
    app.quit();
  }
});
