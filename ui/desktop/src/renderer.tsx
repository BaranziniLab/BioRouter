// Browser compatibility: polyfill Node globals that Electron provides
if (typeof (globalThis as Record<string, unknown>).process === 'undefined') {
  (globalThis as Record<string, unknown>).process = { env: {}, platform: 'darwin', versions: {} };
}

import React, { Suspense, lazy } from 'react';
import ReactDOM from 'react-dom/client';
import { ConfigProvider } from './components/ConfigContext';
import { ErrorBoundary } from './components/ErrorBoundary';
import { client } from './api/client.gen';
import { wrapArtifactForBrowser } from './utils/artifactSecurity';

const App = lazy(() => import('./App'));

type HeadlessConfig = {
  apiBaseUrl: string;
  headlessBaseUrl: string;
  secretKey: string;
  appConfig: Record<string, unknown>;
};

declare global {
  interface Window {
    __BIOROUTER_HEADLESS_CONFIG__?: Partial<HeadlessConfig>;
    /** Boot splash controller, defined by the inline script in index.html. */
    __brBootSplash?: { dismiss: () => void; isShown: () => boolean };
  }
}

/**
 * Tears down the pre-React boot splash once the app has actually mounted.
 *
 * It sits inside the <Suspense> boundary with no props of its own, so React
 * commits it in the same pass as <App> — i.e. exactly when the lazy chunk has
 * resolved and there is a real UI to hand off to. Dismissing any earlier would
 * uncover a blank page.
 */
function DismissBootSplash() {
  React.useEffect(() => {
    window.__brBootSplash?.dismiss();
  }, []);
  return null;
}

function getHeadlessConfig(): HeadlessConfig {
  const params = new URLSearchParams(window.location.search);
  const globalConfig = window.__BIOROUTER_HEADLESS_CONFIG__ ?? {};
  const querySecret = params.get('secret') ?? '';
  const storedSecret = window.sessionStorage?.getItem('biorouter-headless-secret') ?? '';
  const secretKey = globalConfig.secretKey || querySecret || storedSecret;
  const isLocalhost = ['localhost', '127.0.0.1'].includes(window.location.hostname);
  const apiBaseUrl =
    globalConfig.apiBaseUrl ||
    params.get('api') ||
    (isLocalhost ? 'http://127.0.0.1:3000' : `${window.location.origin}/api`);
  const headlessBaseUrl =
    globalConfig.headlessBaseUrl || params.get('headless') || `${window.location.origin}/headless`;

  if (querySecret) {
    window.sessionStorage?.setItem('biorouter-headless-secret', querySecret);
    params.delete('secret');
    const query = params.toString();
    window.history.replaceState(
      {},
      '',
      `${window.location.pathname}${query ? `?${query}` : ''}${window.location.hash}`
    );
  }

  return {
    apiBaseUrl,
    headlessBaseUrl,
    secretKey: secretKey || 'devkey',
    appConfig: {
      BIOROUTER_API_HOST: apiBaseUrl,
      BIOROUTER_BASE_URL_SHARE: window.location.origin,
      BIOROUTER_PREDEFINED_MODELS: '',
      BIOROUTER_WORKING_DIR: '',
      BIOROUTER_VERSION: 'Headless',
      ...(globalConfig.appConfig ?? {}),
    },
  };
}

const needsHeadlessElectron =
  typeof window.electron === 'undefined' ||
  typeof window.electron.getBiorouterdHostPort !== 'function' ||
  typeof window.electron.getSecretKey !== 'function';

function promptForRemotePath(message: string, defaultValue = '/home/ubuntu'): string | null {
  const selected = window.prompt(message, defaultValue)?.trim();
  return selected || null;
}

async function headlessJson<T>(
  headlessBaseUrl: string,
  path: string,
  init?: RequestInit
): Promise<T | null> {
  try {
    const response = await fetch(`${headlessBaseUrl}${path}`, init);
    if (!response.ok) return null;
    return (await response.json()) as T;
  } catch {
    return null;
  }
}

async function headlessPost<T>(
  headlessBaseUrl: string,
  path: string,
  body: unknown
): Promise<T | null> {
  return headlessJson<T>(headlessBaseUrl, path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
}

type HeadlessHealthResponse = { home_dir?: string };
type HeadlessDirsResponse = { dirs?: string[] };
type HeadlessFilesResponse = { files?: string[] };
type HeadlessOkResponse = { ok?: boolean };
type HeadlessSettingsResponse = { settings?: Record<string, unknown> };
type HeadlessReadFileResponse = {
  filePath: string;
  file: string;
  found: boolean;
  error: string | null;
};
type HeadlessArtifactFileResponse =
  | HeadlessReadFileResponse
  | {
      kind: string;
      title: string;
      path: string;
      found: boolean;
      text?: string;
      mimeType?: string;
      dataUrl?: string;
      size?: number;
      entries?: Array<{ name: string; path: string; isDirectory: boolean; size?: number }>;
      error?: string;
    };

// Browser/headless mode: inject browser-safe preload bridges so the app renders without Electron.
if (needsHeadlessElectron || typeof window.appConfig === 'undefined') {
  const headlessConfig = getHeadlessConfig();
  document.documentElement.dataset.biorouterSurface = 'headless';
  document.body.classList.add('biorouter-headless-browser');
  if (typeof window.appConfig === 'undefined') {
    (window as unknown as Record<string, unknown>).appConfig = {
      get: (key: string) => headlessConfig.appConfig[key],
      getAll: () => headlessConfig.appConfig,
    };
  }
  if (needsHeadlessElectron) {
    (window as unknown as Record<string, unknown>).electron = {
      getConfig: () => headlessConfig.appConfig,
      hideWindow: () => {},
      getBiorouterdHostPort: async () => headlessConfig.apiBaseUrl,
      getSecretKey: async () => headlessConfig.secretKey,
      // Issue #56 DR-16: a headless/browser surface was not started by the
      // Biorouter app, so it holds no proof of user and every request that
      // would raise a chat's privacy capability fails closed. Chats already on
      // a private model keep working, and switching to a public model still
      // does.
      getUserActionKey: async () => '',
      logInfo: console.info,
      logError: console.error,
      showNotification: () => {},
      showMessageBox: async () => ({ response: 0 }),
      reloadApp: () => window.location.reload(),
      reactReady: () => {},
      openExternal: async (url: string) => {
        window.open(url, '_blank', 'noopener,noreferrer');
      },
      openInChrome: (url: string) => window.open(url, '_blank', 'noopener,noreferrer'),
      fetchMetadata: async (url: string) => {
        const response = await fetch(url);
        return response.text();
      },
      on: () => () => {},
      off: () => {},
      once: () => {},
      emit: () => {},
      removeListener: () => {},
      removeAllListeners: () => {},
      platform: 'linux',
      createChatWindow: () => {},
      createDivergedChatWindow: () => {},
      closeWindow: () => {},
      directoryChooser: async () => {
        const selected = promptForRemotePath('Enter a folder path on the Ubuntu server');
        return selected
          ? { canceled: false, filePaths: [selected] }
          : { canceled: true, filePaths: [] };
      },
      showSaveDialog: async (options?: { defaultPath?: string }) => {
        const selected = promptForRemotePath(
          'Enter a save path on the Ubuntu server',
          options?.defaultPath || '/home/ubuntu'
        );
        return selected ? { canceled: false, filePath: selected } : { canceled: true };
      },
      saveDiagnosticsBundle: async (sessionId: string, archive: ArrayBuffer) => {
        const filename = `diagnostics_${sessionId}.zip`;
        const url = window.URL.createObjectURL(new Blob([archive], { type: 'application/zip' }));
        const anchor = document.createElement('a');
        anchor.href = url;
        anchor.download = filename;
        document.body.appendChild(anchor);
        anchor.click();
        anchor.remove();
        window.URL.revokeObjectURL(url);
        return { canceled: false, filePath: filename };
      },
      showOpenDialog: async () => {
        const selected = promptForRemotePath('Enter a file or folder path on the Ubuntu server');
        return selected
          ? { canceled: false, filePaths: [selected] }
          : { canceled: true, filePaths: [] };
      },
      selectFileOrDirectory: async (defaultPath?: string) =>
        promptForRemotePath(
          'Enter a file or folder path on the Ubuntu server',
          defaultPath || '/home/ubuntu'
        ),
      getVersion: async () => '0.0.0-browser',
      startPowerSaveBlocker: () => {},
      stopPowerSaveBlocker: () => {},
      setWakelock: async () => false,
      getWakelockState: async () => false,
      setSpellcheck: async () => false,
      getSpellcheckState: async () => false,
      setMenuBarIcon: async () => false,
      getMenuBarIconState: async () => false,
      setDockIcon: async () => false,
      getDockIconState: async () => false,
      openNotificationsSettings: async () => false,
      getSettings: async () => {
        const result = await headlessJson<HeadlessSettingsResponse>(
          headlessConfig.headlessBaseUrl,
          '/settings'
        );
        if (result?.settings && typeof result.settings === 'object') return result.settings;
        return JSON.parse(window.localStorage.getItem('biorouter-browser-settings') ?? '{}');
      },
      saveSettings: async (settings: unknown) => {
        const result = await headlessPost<HeadlessOkResponse>(
          headlessConfig.headlessBaseUrl,
          '/settings',
          settings
        );
        if (result?.ok === true) return true;
        window.localStorage.setItem('biorouter-browser-settings', JSON.stringify(settings));
        return false;
      },
      getBiorouterDir: async () => {
        const health = await headlessJson<HeadlessHealthResponse>(
          headlessConfig.headlessBaseUrl,
          '/health'
        );
        return `${health?.home_dir || '/home/ubuntu'}/.config/biorouter`;
      },
      getBinaryPath: async (binaryName: string) => binaryName,
      getAllowedExtensions: async () => [],
      ensureDirectory: async (dirPath: string) => {
        const result = await headlessPost<HeadlessOkResponse>(
          headlessConfig.headlessBaseUrl,
          '/fs/ensure-dir',
          { path: dirPath }
        );
        return result?.ok === true;
      },
      listFiles: async (dir: string, extension?: string) => {
        const query = new URLSearchParams({ path: dir });
        if (extension) query.set('extension', extension);
        const result = await headlessJson<HeadlessFilesResponse>(
          headlessConfig.headlessBaseUrl,
          `/fs/list-files?${query.toString()}`
        );
        return Array.isArray(result?.files) ? result.files : [];
      },
      listSkillDirs: async (dir: string) => {
        const result = await headlessJson<HeadlessDirsResponse>(
          headlessConfig.headlessBaseUrl,
          `/fs/list-dirs?path=${encodeURIComponent(dir)}`
        );
        return Array.isArray(result?.dirs) ? result.dirs : [];
      },
      readFile: async (filePath: string) => {
        const result = await headlessJson<HeadlessReadFileResponse>(
          headlessConfig.headlessBaseUrl,
          `/fs/read?path=${encodeURIComponent(filePath)}`
        );
        if (result) return result;
        return {
          file: window.localStorage.getItem(`biorouter-browser-file:${filePath}`) ?? '',
          filePath,
          error: null,
          found: window.localStorage.getItem(`biorouter-browser-file:${filePath}`) !== null,
        };
      },
      readArtifactFile: async (filePath: string) => {
        const result = await headlessJson<HeadlessArtifactFileResponse>(
          headlessConfig.headlessBaseUrl,
          `/fs/artifact?path=${encodeURIComponent(filePath)}`
        );
        if (result && 'kind' in result) return result;
        const stored = window.localStorage.getItem(`biorouter-browser-file:${filePath}`);
        if (stored !== null) {
          return {
            kind: filePath.endsWith('.html') ? 'html' : 'text',
            title: filePath.split('/').filter(Boolean).pop() || filePath,
            path: filePath,
            mimeType: filePath.endsWith('.html') ? 'text/html' : 'text/plain',
            text: stored,
            size: stored.length,
            found: true,
          };
        }
        return {
          kind: 'error',
          title: filePath.split('/').filter(Boolean).pop() || filePath,
          path: filePath,
          error: 'Could not read artifact in browser mode',
          found: false,
        };
      },
      writeFile: async (filePath: string, content: string) => {
        const result = await headlessPost<HeadlessOkResponse>(
          headlessConfig.headlessBaseUrl,
          '/fs/write',
          { path: filePath, content }
        );
        if (result?.ok === true) return true;
        window.localStorage.setItem(`biorouter-browser-file:${filePath}`, content);
        return false;
      },
      deleteFile: async (filePath: string) => {
        window.localStorage.removeItem(`biorouter-browser-file:${filePath}`);
        const result = await headlessPost<HeadlessOkResponse>(
          headlessConfig.headlessBaseUrl,
          '/fs/delete-file',
          { path: filePath }
        );
        return result?.ok === true;
      },
      deleteDirectory: async (dirPath: string) => {
        const result = await headlessPost<HeadlessOkResponse>(
          headlessConfig.headlessBaseUrl,
          '/fs/delete-dir',
          { path: dirPath }
        );
        return result?.ok === true;
      },
      saveDataUrlToTemp: async (dataUrl: string, id: string) => ({ id, filePath: dataUrl }),
      deleteTempFile: () => {},
      getTempImage: async (filePath: string) => filePath,
      readTempImageAsBase64: async (filePath: string) => ({
        data: filePath.split(',')[1] ?? '',
        mimeType: filePath.match(/^data:([^;]+)/)?.[1] ?? 'application/octet-stream',
      }),
      checkForUpdates: async () => null,
      downloadUpdate: async () => ({ success: false, error: null }),
      installUpdate: () => {},
      restartApp: () => {},
      onUpdaterEvent: () => () => {},
      getPathForFile: (file: File) => file.name,
      ensureWindowContentWidth: async (width: number) => ({
        expanded: false,
        width,
        height: window.innerHeight,
      }),
      checkForOllama: async () => false,
      cliStatus: async () => ({ installed: false }),
      installCli: async () => ({ success: false }),
      launchCli: async () => ({ success: false }),
      createTerminalSession: async () => ({
        success: false,
        error: 'In-app terminal is only available in Electron.',
      }),
      writeTerminalSession: async () => ({ success: false, error: 'No terminal session.' }),
      resizeTerminalSession: async () => ({ success: false, error: 'No terminal session.' }),
      disposeTerminalSession: async () => ({ success: true }),
      onTerminalData: () => () => {},
      onTerminalExit: () => () => {},
      addRecentDir: async () => {},
      openDirectoryInExplorer: async () => {},
      openArtifactWindow: async (payload: { html: string } | string) => {
        const html = typeof payload === 'string' ? payload : payload.html;
        const blob = new Blob([wrapArtifactForBrowser(html)], { type: 'text/html' });
        window.open(URL.createObjectURL(blob), '_blank', 'noopener,noreferrer');
      },
      openArtifactInBrowser: async (payload: { html: string } | string) => {
        const html = typeof payload === 'string' ? payload : payload.html;
        const blob = new Blob([wrapArtifactForBrowser(html)], { type: 'text/html' });
        window.open(URL.createObjectURL(blob), '_blank', 'noopener,noreferrer');
      },
      prepareArtifactHtml: async ({ html }: { html: string }) => ({ html }),
      recordWorkflowHash: async () => {},
      hasAcceptedWorkflowBefore: async () => false,
      openBrxtFilePicker: async () =>
        promptForRemotePath('Enter a .brxt bundle path on the Ubuntu server', '/home/ubuntu'),
      validateBrxtBundle: async (filePath: string) =>
        (await headlessPost(headlessConfig.headlessBaseUrl, '/brxt/validate', {
          filePath,
        })) ?? { error: 'Could not validate bundle on the headless server' },
      installBrxtBundle: async (filePath: string, extensionName: string) =>
        (await headlessPost(headlessConfig.headlessBaseUrl, '/brxt/install', {
          filePath,
          extensionName,
        })) ?? { success: false, error: 'Could not install bundle on the headless server' },
      uninstallBrxtExtension: async (extensionName: string) =>
        (await headlessPost(headlessConfig.headlessBaseUrl, '/brxt/uninstall', {
          extensionName,
        })) ?? {
          success: false,
          error: 'Could not uninstall extension on the headless server',
        },
      extractSkillZip: async (filePath: string) =>
        (await headlessPost(headlessConfig.headlessBaseUrl, '/skills/extract-zip', {
          filePath,
        })) ?? { success: false, error: 'Could not extract ZIP on the headless server' },
      downloadRegistryAsset: async (url: string) =>
        (await headlessPost(headlessConfig.headlessBaseUrl, '/registry/download', {
          url,
        })) ?? {
          success: false,
          error: 'Could not download registry asset on the headless server',
        },
      fetchRegistry: async (url: string) => fetch(url).then((response) => response.json()),
      installDependency: async () => ({ success: false, error: 'Not available in browser mode' }),
      launchApp: async (url: string) => window.open(url, '_blank', 'noopener,noreferrer'),
    };
  }
}

(async () => {
  // Check if we're in the launcher view (doesn't need biorouterd connection)
  const isLauncher = window.location.hash === '#/launcher';

  if (!isLauncher) {
    console.log('window created, getting biorouterd connection info');
    const biorouterApiHost = await window.electron.getBiorouterdHostPort();
    if (biorouterApiHost === null) {
      // React never renders on this path, so nothing else would ever take the
      // splash down — the user would be left staring at it behind the alert.
      window.__brBootSplash?.dismiss();
      window.alert('failed to start biorouter backend process');
      return;
    }
    console.log('connecting at', biorouterApiHost);
    client.setConfig({
      baseUrl: biorouterApiHost,
      headers: {
        'Content-Type': 'application/json',
        'X-Secret-Key': await window.electron.getSecretKey(),
      },
    });
  }

  ReactDOM.createRoot(document.getElementById('root')!).render(
    <React.StrictMode>
      {/* No fallback element: the boot splash in index.html is already on
          screen and covers this window, so rendering one here would only
          flash a second loader underneath it. */}
      <Suspense fallback={null}>
        <ConfigProvider>
          <ErrorBoundary>
            <App />
          </ErrorBoundary>
        </ConfigProvider>
        <DismissBootSplash />
      </Suspense>
    </React.StrictMode>
  );
})();
