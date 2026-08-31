import { randomUUID } from 'node:crypto';
import { session, type BrowserWindow, type Session } from 'electron';
import log from 'electron-log';
import {
  isManagedAppRequest,
  MANAGED_APP_PREVIEW_CSP,
  type ManagedAppPreviewBackend,
  type ManagedAppPreviewScope,
} from './managedAppPreviewPolicy';
import { startManagedAppPreviewProxy } from './managedAppPreviewProxy';

export type ManagedAppPreviewSession = {
  session: Session;
  scope: ManagedAppPreviewScope;
  ready: Promise<void>;
  isActive: () => boolean;
  onRevoke: (callback: () => void) => () => void;
};

const owners = new WeakMap<
  BrowserWindow,
  Map<ManagedAppPreviewBackend, Map<string, ManagedAppPreviewSession>>
>();

export function managedAppSession(
  owner: BrowserWindow,
  scope: ManagedAppPreviewScope
): ManagedAppPreviewSession {
  let backends = owners.get(owner);
  if (!backends) {
    backends = new Map();
    owners.set(owner, backends);
  }
  let apps = backends.get(scope.backend);
  if (!apps) {
    apps = new Map();
    backends.set(scope.backend, apps);
  }
  const existing = apps.get(scope.appId);
  if (existing) return existing;
  const appSessions = apps;
  const backendSessions = backends;

  // No persist: prefix: no stale service workers/cookies survive an app launch.
  const isolated = session.fromPartition(`biorouter-managed-app-${randomUUID()}`, { cache: false });
  const transport = new AbortController();
  const callbacks = new Set<() => void>();
  let active = !scope.backend.signal.aborted && !owner.isDestroyed();
  let disposed = false;
  const dispose = () => {
    if (disposed) return;
    disposed = true;
    active = false;
    transport.abort();
    scope.backend.signal.removeEventListener('abort', dispose);
    owner.removeListener('closed', dispose);
    appSessions.delete(scope.appId);
    if (appSessions.size === 0) backendSessions.delete(scope.backend);
    for (const callback of callbacks) callback();
    callbacks.clear();
    void Promise.all([isolated.closeAllConnections(), isolated.clearStorageData()]).catch(() =>
      log.warn('[ManagedAppPreview] session cleanup failed')
    );
  };
  isolated.setPermissionRequestHandler((_contents, _permission, callback) => callback(false));
  isolated.setPermissionCheckHandler(() => false);
  isolated.setDevicePermissionHandler(() => false);
  isolated.setDisplayMediaRequestHandler((_request, callback) => callback({}));
  isolated.on('will-download', (event) => event.preventDefault());
  isolated.webRequest.onBeforeRequest({ urls: ['<all_urls>'] }, (details, callback) => {
    callback({ cancel: !active || !isManagedAppRequest(scope, details) });
  });
  isolated.webRequest.onBeforeSendHeaders({ urls: ['<all_urls>'] }, (details, callback) => {
    const worker = Object.entries(details.requestHeaders).some(
      ([key, value]) =>
        key.toLowerCase() === 'service-worker' ||
        (key.toLowerCase() === 'sec-fetch-dest' && /worker/i.test(String(value)))
    );
    callback({ cancel: !active || worker });
  });
  isolated.webRequest.onHeadersReceived({ urls: ['<all_urls>'] }, (details, callback) => {
    const headers = { ...details.responseHeaders };
    const existingPolicy = Object.keys(headers).find(
      (key) => key.toLowerCase() === 'content-security-policy'
    );
    const key = existingPolicy ?? 'Content-Security-Policy';
    headers[key] = [...(headers[key] ?? []), MANAGED_APP_PREVIEW_CSP];
    callback({ cancel: !active, responseHeaders: headers });
  });
  owner.once('closed', dispose);
  scope.backend.signal.addEventListener('abort', dispose, { once: true });
  if (!active) dispose();

  const ready = startManagedAppPreviewProxy(scope.port, transport.signal)
    .then(async (proxy) => {
      if (!active) {
        await proxy.close();
        throw new Error('Managed app backend is unavailable.');
      }
      await isolated.setProxy({
        proxyRules: `socks5://127.0.0.1:${proxy.port}`,
        proxyBypassRules: '<-loopback>',
      });
      if (!active) throw new Error('Managed app backend is unavailable.');
    })
    .catch((error) => {
      dispose();
      throw error;
    });
  const managed: ManagedAppPreviewSession = {
    session: isolated,
    scope,
    ready,
    isActive: () => active,
    onRevoke: (callback) => {
      if (active) callbacks.add(callback);
      else callback();
      return () => callbacks.delete(callback);
    },
  };
  if (active) appSessions.set(scope.appId, managed);
  return managed;
}
