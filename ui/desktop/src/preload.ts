import Electron, { contextBridge, ipcRenderer, webUtils } from 'electron';
import { Workflow } from './workflow';
import { BioRouterApp } from './api';

// One-time warning for callers still using the legacy `off()` API. Each
// channel only warns once to avoid log spam under React StrictMode.
const offDeprecationWarned = new Set<string>();
function warnOffDeprecated(channel: string): void {
  if (offDeprecationWarned.has(channel)) return;
  offDeprecationWarned.add(channel);
  console.warn(
    `[preload] window.electron.off('${channel}', ...) is a no-op. ` +
      'Use the disposer returned by on(): `const dispose = electron.on(...); return dispose;`.'
  );
}

interface NotificationData {
  title: string;
  body: string;
}

interface MessageBoxOptions {
  type?: 'none' | 'info' | 'error' | 'question' | 'warning';
  buttons?: string[];
  defaultId?: number;
  title?: string;
  message: string;
  detail?: string;
}

interface MessageBoxResponse {
  response: number;
  checkboxChecked?: boolean;
}

interface SaveDialogOptions {
  title?: string;
  defaultPath?: string;
  buttonLabel?: string;
  filters?: Array<{ name: string; extensions: string[] }>;
  message?: string;
  nameFieldLabel?: string;
  showsTagField?: boolean;
}

interface SaveDialogResponse {
  canceled: boolean;
  filePath?: string;
}

interface FileResponse {
  file: string;
  filePath: string;
  error: string | null;
  found: boolean;
}

interface SaveDataUrlResponse {
  id: string;
  filePath?: string;
  error?: string;
}

type ArtifactFileEntry = {
  name: string;
  path: string;
  isDirectory: boolean;
  size?: number;
};

type ArtifactFilePreview =
  | {
      kind: 'text' | 'html';
      title: string;
      path: string;
      mimeType: string;
      text: string;
      size: number;
      found: true;
    }
  | {
      kind: 'image';
      title: string;
      path: string;
      mimeType: string;
      dataUrl: string;
      size: number;
      found: true;
    }
  | {
      kind: 'directory';
      title: string;
      path: string;
      entries: ArtifactFileEntry[];
      found: true;
    }
  | {
      kind: 'binary';
      title: string;
      path: string;
      mimeType: string;
      size: number;
      found: true;
    }
  | {
      kind: 'error';
      title: string;
      path: string;
      error: string;
      found: false;
    };

const config = JSON.parse(process.argv.find((arg) => arg.startsWith('{')) || '{}');

interface UpdaterEvent {
  event: string;
  data?: unknown;
}

// Define the API types in a single place
type ElectronAPI = {
  platform: string;
  reactReady: () => void;
  getConfig: () => Record<string, unknown>;
  hideWindow: () => void;
  dashboardEnter: () => Promise<void>;
  dashboardExit: () => Promise<void>;
  directoryChooser: () => Promise<Electron.OpenDialogReturnValue>;
  createChatWindow: (
    query?: string,
    dir?: string,
    version?: string,
    resumeSessionId?: string,
    viewType?: string,
    workflowId?: string
  ) => void;
  createDivergedChatWindow: (dir: string | undefined, resumeSessionId: string) => void;
  logInfo: (txt: string) => void;
  showNotification: (data: NotificationData) => void;
  showMessageBox: (options: MessageBoxOptions) => Promise<MessageBoxResponse>;
  showSaveDialog: (options: SaveDialogOptions) => Promise<SaveDialogResponse>;
  openInChrome: (url: string) => void;
  fetchMetadata: (url: string) => Promise<string>;
  reloadApp: () => void;
  checkForOllama: () => Promise<boolean>;
  selectFileOrDirectory: (defaultPath?: string) => Promise<string | null>;
  getBinaryPath: (binaryName: string) => Promise<string>;
  importSessionFile: () => Promise<string | null>;
  readFile: (directory: string) => Promise<FileResponse>;
  readArtifactFile: (filePath: string) => Promise<ArtifactFilePreview>;
  writeFile: (directory: string, content: string) => Promise<boolean>;
  ensureDirectory: (dirPath: string) => Promise<boolean>;
  listFiles: (dirPath: string, extension?: string) => Promise<string[]>;
  listSkillDirs: (dirPath: string) => Promise<string[]>;
  deleteFile: (filePath: string) => Promise<boolean>;
  deleteDirectory: (dirPath: string) => Promise<boolean>;
  getAllowedExtensions: () => Promise<string[]>;
  getPathForFile: (file: File) => string;
  setMenuBarIcon: (show: boolean) => Promise<boolean>;
  getMenuBarIconState: () => Promise<boolean>;
  setDockIcon: (show: boolean) => Promise<boolean>;
  getDockIconState: () => Promise<boolean>;
  getSettings: () => Promise<unknown | null>;
  saveSettings: (settings: unknown) => Promise<boolean>;
  getSecretKey: () => Promise<string>;
  getBiorouterdHostPort: () => Promise<string | null>;
  setWakelock: (enable: boolean) => Promise<boolean>;
  getWakelockState: () => Promise<boolean>;
  setSpellcheck: (enable: boolean) => Promise<boolean>;
  getSpellcheckState: () => Promise<boolean>;
  openNotificationsSettings: () => Promise<boolean>;
  onMouseBackButtonClicked: (callback: () => void) => () => void;
  offMouseBackButtonClicked: (callback: () => void) => void;
  /** Subscribe to a main-process IPC event. Returns a disposer; call it to
   * remove the listener. Do not use `off()` — contextBridge proxies the
   * callback differently on each crossing, so `off(channel, sameCallback)`
   * can't find the registered wrapper and silently leaks the listener. */
  on: (
    channel: string,
    callback: (event: Electron.IpcRendererEvent, ...args: unknown[]) => void
  ) => () => void;
  /** Deprecated. Use the disposer returned by `on()` instead. Calling this
   * is a no-op (kept for source compatibility); the listener will leak. */
  off: (
    channel: string,
    callback: (event: Electron.IpcRendererEvent, ...args: unknown[]) => void
  ) => void;
  emit: (channel: string, ...args: unknown[]) => void;
  broadcastThemeChange: (themeData: {
    mode: string;
    useSystemTheme: boolean;
    theme: string;
  }) => void;
  // Functions for image pasting
  saveDataUrlToTemp: (dataUrl: string, uniqueId: string) => Promise<SaveDataUrlResponse>;
  deleteTempFile: (filePath: string) => void;
  // Function for opening external URLs securely
  openExternal: (url: string) => Promise<void>;
  // Function to serve temp images
  getTempImage: (filePath: string) => Promise<string | null>;
  // Function to read temp image as raw base64 + mimeType for API use
  readTempImageAsBase64: (filePath: string) => Promise<{ data: string; mimeType: string }>;
  // Update-related functions
  getVersion: () => string;
  checkForUpdates: () => Promise<{ updateInfo: unknown; error: string | null }>;
  downloadUpdate: () => Promise<{ success: boolean; error: string | null }>;
  installUpdate: () => void;
  restartApp: () => void;
  /** Subscribe to main-process updater events. Returns a disposer that removes
   * the listener; call it on unmount to avoid duplicate registrations. */
  onUpdaterEvent: (callback: (event: UpdaterEvent) => void) => () => void;
  getUpdateState: () => Promise<{
    updateAvailable: boolean;
    latestVersion?: string;
    status?: 'checking' | 'available' | 'downloaded' | 'up-to-date' | 'error';
    percent?: number;
    usingFallback?: boolean;
    error?: string;
  } | null>;
  isUsingGitHubFallback: () => Promise<boolean>;
  // Workflow warning functions
  closeWindow: () => void;
  hasAcceptedWorkflowBefore: (workflow: Workflow) => Promise<boolean>;
  recordWorkflowHash: (workflow: Workflow) => Promise<boolean>;
  openDirectoryInExplorer: (directoryPath: string) => Promise<boolean>;
  launchApp: (app: BioRouterApp) => Promise<void>;
  openArtifactWindow: (payload: {
    html: string;
    title?: string;
    width?: number;
    height?: number;
    theme?: 'light' | 'dark';
  }) => Promise<{ ok: boolean }>;
  prepareArtifactHtml: (payload: { html: string }) => Promise<{ html: string }>;
  addRecentDir: (dir: string) => Promise<boolean>;
  openBrxtFilePicker: () => Promise<string | null>;
  validateBrxtBundle: (filePath: string) => Promise<
    | {
        manifest: import('./types/brxt').BrxtManifest;
        skillsPreview: Array<{ slug: string; name: string; description: string }>;
      }
    | { error: string }
  >;
  uninstallBrxtExtension: (extensionName: string) => Promise<{ success: true } | { error: string }>;
  extractSkillZip: (filePath: string) => Promise<
    | {
        isBundle: false;
        files: [string, string][];
        name: string;
        description: string;
        slug: string;
      }
    | {
        isBundle: true;
        bundleName: string;
        bundleSkills: Array<{ name: string; description: string }>;
        files: [string, string][];
        slug: string;
        name: string;
        description: string;
      }
    | { error: string }
  >;
  installBrxtBundle: (
    filePath: string,
    extensionName: string
  ) => Promise<{ success: true; installDir: string } | { error: string }>;
  // BAAM registry (Browse Skills / Browse Extensions)
  fetchRegistry: () => Promise<
    { registry: import('./components/baam/registry').BaamRegistry } | { error: string }
  >;
  downloadRegistryAsset: (url: string) => Promise<{ path: string } | { error: string }>;
  // Dependency checker
  checkDependencies: () => Promise<import('./utils/dependencyChecker').DependencyInfo[]>;
  installDependency: (dep: string) => Promise<{ started: boolean } | { error: string }>;
  // Biorouter CLI install (delegates to the bundled `biorouter setup-path`)
  cliStatus: () => Promise<{
    bundled: string | null;
    onPath: boolean;
    pathLocation: string | null;
    bundledVersion: string | null;
    pathVersion: string | null;
    needsUpdate: boolean;
    brokenOnPath: boolean;
  }>;
  installCli: () => Promise<{ success: true; output: string } | { success: false; error: string }>;
  launchCli: (
    workingDir?: string
  ) => Promise<{ success: true } | { success: false; error: string }>;
  // Extension updater (events pushed via 'extension-update-event' channel)
  onExtensionUpdateEvent: (
    callback: (event: import('./utils/extensionUpdater').ExtensionUpdateEvent) => void
  ) => void;
};

type AppConfigAPI = {
  get: (key: string) => unknown;
  getAll: () => Record<string, unknown>;
};

const electronAPI: ElectronAPI = {
  platform: process.platform,
  reactReady: () => ipcRenderer.send('react-ready'),
  getConfig: () => {
    if (!config || Object.keys(config).length === 0) {
      console.warn(
        'No config provided by main process. This may indicate an initialization issue.'
      );
    }
    return config;
  },
  hideWindow: () => ipcRenderer.send('hide-window'),
  dashboardEnter: () => ipcRenderer.invoke('dashboard:enter'),
  dashboardExit: () => ipcRenderer.invoke('dashboard:exit'),
  directoryChooser: () => ipcRenderer.invoke('directory-chooser'),
  createChatWindow: (
    query?: string,
    dir?: string,
    version?: string,
    resumeSessionId?: string,
    viewType?: string,
    workflowId?: string
  ) =>
    ipcRenderer.send(
      'create-chat-window',
      query,
      dir,
      version,
      resumeSessionId,
      viewType,
      workflowId
    ),
  createDivergedChatWindow: (dir: string | undefined, resumeSessionId: string) =>
    ipcRenderer.send('create-diverged-chat-window', dir, resumeSessionId),
  logInfo: (txt: string) => ipcRenderer.send('logInfo', txt),
  showNotification: (data: NotificationData) => ipcRenderer.send('notify', data),
  showMessageBox: (options: MessageBoxOptions) => ipcRenderer.invoke('show-message-box', options),
  showSaveDialog: (options: SaveDialogOptions) => ipcRenderer.invoke('show-save-dialog', options),
  openInChrome: (url: string) => ipcRenderer.send('open-in-chrome', url),
  fetchMetadata: (url: string) => ipcRenderer.invoke('fetch-metadata', url),
  reloadApp: () => ipcRenderer.send('reload-app'),
  checkForOllama: () => ipcRenderer.invoke('check-ollama'),
  selectFileOrDirectory: (defaultPath?: string) =>
    ipcRenderer.invoke('select-file-or-directory', defaultPath),
  getBinaryPath: (binaryName: string) => ipcRenderer.invoke('get-binary-path', binaryName),
  importSessionFile: () => ipcRenderer.invoke('import-session-file'),
  readFile: (filePath: string) => ipcRenderer.invoke('read-file', filePath),
  readArtifactFile: (filePath: string) => ipcRenderer.invoke('read-artifact-file', filePath),
  writeFile: (filePath: string, content: string) =>
    ipcRenderer.invoke('write-file', filePath, content),
  ensureDirectory: (dirPath: string) => ipcRenderer.invoke('ensure-directory', dirPath),
  listFiles: (dirPath: string, extension?: string) =>
    ipcRenderer.invoke('list-files', dirPath, extension),
  listSkillDirs: (dirPath: string) => ipcRenderer.invoke('list-skill-dirs', dirPath),
  deleteFile: (filePath: string) => ipcRenderer.invoke('delete-file', filePath),
  deleteDirectory: (dirPath: string) => ipcRenderer.invoke('delete-directory', dirPath),
  getPathForFile: (file: File) => webUtils.getPathForFile(file),
  getAllowedExtensions: () => ipcRenderer.invoke('get-allowed-extensions'),
  setMenuBarIcon: (show: boolean) => ipcRenderer.invoke('set-menu-bar-icon', show),
  getMenuBarIconState: () => ipcRenderer.invoke('get-menu-bar-icon-state'),
  setDockIcon: (show: boolean) => ipcRenderer.invoke('set-dock-icon', show),
  getDockIconState: () => ipcRenderer.invoke('get-dock-icon-state'),
  getSettings: () => ipcRenderer.invoke('get-settings'),
  saveSettings: (settings: unknown) => ipcRenderer.invoke('save-settings', settings),
  getSecretKey: () => ipcRenderer.invoke('get-secret-key'),
  getBiorouterdHostPort: () => ipcRenderer.invoke('get-biorouterd-host-port'),
  setWakelock: (enable: boolean) => ipcRenderer.invoke('set-wakelock', enable),
  getWakelockState: () => ipcRenderer.invoke('get-wakelock-state'),
  setSpellcheck: (enable: boolean) => ipcRenderer.invoke('set-spellcheck', enable),
  getSpellcheckState: () => ipcRenderer.invoke('get-spellcheck-state'),
  openNotificationsSettings: () => ipcRenderer.invoke('open-notifications-settings'),
  onMouseBackButtonClicked: (callback: () => void) => {
    const wrapper = (_event: Electron.IpcRendererEvent) => callback();
    ipcRenderer.on('mouse-back-button-clicked', wrapper);
    return () => ipcRenderer.removeListener('mouse-back-button-clicked', wrapper);
  },
  offMouseBackButtonClicked: () => {
    warnOffDeprecated('mouse-back-button-clicked');
  },
  on: (
    channel: string,
    callback: (event: Electron.IpcRendererEvent, ...args: unknown[]) => void
  ) => {
    // Wrap in a preload-scope function so removeListener can match by
    // identity. Without this, contextBridge would hand `off()` a different
    // proxy than `on()` registered, and removal would silently no-op —
    // listeners would accumulate on every component remount.
    const wrapper = (event: Electron.IpcRendererEvent, ...args: unknown[]) =>
      callback(event, ...args);
    ipcRenderer.on(channel, wrapper);
    return () => ipcRenderer.removeListener(channel, wrapper);
  },
  off: (channel: string) => {
    warnOffDeprecated(channel);
  },
  emit: (channel: string, ...args: unknown[]) => {
    ipcRenderer.emit(channel, ...args);
  },
  broadcastThemeChange: (themeData: { mode: string; useSystemTheme: boolean; theme: string }) => {
    ipcRenderer.send('broadcast-theme-change', themeData);
  },
  saveDataUrlToTemp: (dataUrl: string, uniqueId: string): Promise<SaveDataUrlResponse> => {
    return ipcRenderer.invoke('save-data-url-to-temp', dataUrl, uniqueId);
  },
  deleteTempFile: (filePath: string): void => {
    ipcRenderer.send('delete-temp-file', filePath);
  },
  openExternal: (url: string): Promise<void> => {
    return ipcRenderer.invoke('open-external', url);
  },
  getTempImage: (filePath: string): Promise<string | null> => {
    return ipcRenderer.invoke('get-temp-image', filePath);
  },
  readTempImageAsBase64: (filePath: string): Promise<{ data: string; mimeType: string }> => {
    return ipcRenderer.invoke('read-temp-image-as-base64', filePath);
  },
  getVersion: (): string => {
    return config.BIOROUTER_VERSION || ipcRenderer.sendSync('get-app-version') || '';
  },
  checkForUpdates: (): Promise<{ updateInfo: unknown; error: string | null }> => {
    return ipcRenderer.invoke('check-for-updates');
  },
  downloadUpdate: (): Promise<{ success: boolean; error: string | null }> => {
    return ipcRenderer.invoke('download-update');
  },
  installUpdate: (): void => {
    ipcRenderer.invoke('install-update');
  },
  restartApp: (): void => {
    ipcRenderer.send('restart-app');
  },
  onUpdaterEvent: (callback: (event: UpdaterEvent) => void): (() => void) => {
    const listener = (_event: Electron.IpcRendererEvent, data: UpdaterEvent) => callback(data);
    ipcRenderer.on('updater-event', listener);
    return () => ipcRenderer.removeListener('updater-event', listener);
  },
  getUpdateState: (): Promise<{ updateAvailable: boolean; latestVersion?: string } | null> => {
    return ipcRenderer.invoke('get-update-state');
  },
  isUsingGitHubFallback: (): Promise<boolean> => {
    return ipcRenderer.invoke('is-using-github-fallback');
  },
  closeWindow: () => ipcRenderer.send('close-window'),
  hasAcceptedWorkflowBefore: (workflow: Workflow) =>
    ipcRenderer.invoke('has-accepted-workflow-before', workflow),
  recordWorkflowHash: (workflow: Workflow) => ipcRenderer.invoke('record-workflow-hash', workflow),
  openDirectoryInExplorer: (directoryPath: string) =>
    ipcRenderer.invoke('open-directory-in-explorer', directoryPath),
  launchApp: (app: BioRouterApp) => ipcRenderer.invoke('launch-app', app),
  openArtifactWindow: (payload: {
    html: string;
    title?: string;
    width?: number;
    height?: number;
    theme?: 'light' | 'dark';
  }) => ipcRenderer.invoke('open-artifact-window', payload),
  prepareArtifactHtml: (payload: { html: string }) =>
    ipcRenderer.invoke('prepare-artifact-html', payload),
  addRecentDir: (dir: string) => ipcRenderer.invoke('add-recent-dir', dir),
  openBrxtFilePicker: () => ipcRenderer.invoke('brxt:open-file-dialog'),
  validateBrxtBundle: (filePath: string) =>
    ipcRenderer.invoke('brxt:validate-and-read', { filePath }),
  installBrxtBundle: (filePath: string, extensionName: string) =>
    ipcRenderer.invoke('brxt:install', { filePath, extensionName }),
  uninstallBrxtExtension: (extensionName: string) =>
    ipcRenderer.invoke('brxt:uninstall', { extensionName }),
  extractSkillZip: (filePath: string) => ipcRenderer.invoke('skills:extract-zip', { filePath }),
  fetchRegistry: () => ipcRenderer.invoke('registry:fetch'),
  downloadRegistryAsset: (url: string) => ipcRenderer.invoke('registry:download', { url }),
  checkDependencies: () => ipcRenderer.invoke('dep:check'),
  installDependency: (dep: string) => ipcRenderer.invoke('dep:install', dep),
  cliStatus: () => ipcRenderer.invoke('cli:status'),
  installCli: () => ipcRenderer.invoke('cli:install'),
  launchCli: (workingDir?: string) => ipcRenderer.invoke('cli:launch', workingDir),
  onExtensionUpdateEvent: (callback) => {
    ipcRenderer.on('extension-update-event', (_event, data) => callback(data));
  },
};

const appConfigAPI: AppConfigAPI = {
  get: (key: string) => config[key],
  getAll: () => config,
};

// Expose the APIs
contextBridge.exposeInMainWorld('electron', electronAPI);
contextBridge.exposeInMainWorld('appConfig', appConfigAPI);

// Type declaration for TypeScript
declare global {
  interface Window {
    electron: ElectronAPI;
    appConfig: AppConfigAPI;
  }
}
