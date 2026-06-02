// Browser compatibility: polyfill Node globals that Electron provides
if (typeof (globalThis as Record<string, unknown>).process === 'undefined') {
  (globalThis as Record<string, unknown>).process = { env: {}, platform: 'darwin', versions: {} };
}

import React, { Suspense, lazy } from 'react';
import ReactDOM from 'react-dom/client';
import { ConfigProvider } from './components/ConfigContext';
import { ErrorBoundary } from './components/ErrorBoundary';
import SuspenseLoader from './suspense-loader';
import { client } from './api/client.gen';
import { setTelemetryEnabled } from './utils/analytics';
import { readConfig } from './api';

const App = lazy(() => import('./App'));

const TELEMETRY_CONFIG_KEY = 'BIOROUTER_TELEMETRY_ENABLED';

// Browser dev mode: inject a no-op electron mock so the app renders without Electron
if (typeof window.electron === 'undefined') {
  (window as unknown as Record<string, unknown>).electron = {
    getBiorouterdHostPort: async () => 'http://127.0.0.1:3000',
    getSecretKey: async () => 'devkey',
    logInfo: () => {},
    logError: () => {},
    reloadApp: () => window.location.reload(),
    reactReady: () => {},
    openExternal: (url: string) => window.open(url, '_blank'),
    on: () => () => {},
    off: () => {},
    once: () => {},
    emit: () => {},
    removeListener: () => {},
    removeAllListeners: () => {},
    platform: 'darwin',
    createChatWindow: () => {},
    directoryChooser: async () => null,
    showSaveDialog: async () => null,
    showOpenDialog: async () => null,
    getVersion: async () => '0.0.0-browser',
    startPowerSaveBlocker: () => {},
    stopPowerSaveBlocker: () => {},
    getBiorouterDir: async () => '/',
    checkForUpdates: async () => null,
    installUpdate: () => {},
    getPathForFile: (file: File) => file.name,
  };
}

(async () => {
  // Check if we're in the launcher view (doesn't need biorouterd connection)
  const isLauncher = window.location.hash === '#/launcher';

  if (!isLauncher) {
    console.log('window created, getting biorouterd connection info');
    const biorouterApiHost = await window.electron.getBiorouterdHostPort();
    if (biorouterApiHost === null) {
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

    try {
      const telemetryResponse = await readConfig({
        body: { key: TELEMETRY_CONFIG_KEY, is_secret: false },
      });
      const isTelemetryEnabled = telemetryResponse.data !== false;
      setTelemetryEnabled(isTelemetryEnabled);
    } catch (error) {
      console.warn('[Analytics] Failed to initialize analytics:', error);
    }
  }

  ReactDOM.createRoot(document.getElementById('root')!).render(
    <React.StrictMode>
      <Suspense fallback={SuspenseLoader()}>
        <ConfigProvider>
          <ErrorBoundary>
            <App />
          </ErrorBoundary>
        </ConfigProvider>
      </Suspense>
    </React.StrictMode>
  );
})();
