import type { BrowserWindow, Session } from 'electron';
import { afterEach, describe, expect, it, vi } from 'vitest';

const electron = vi.hoisted(() => {
  const events = () => {
    type Listener = (...args: unknown[]) => void;
    const listeners = new Map<string, Listener[]>();
    const on = vi.fn((name: string, callback: Listener) => {
      listeners.set(name, [...(listeners.get(name) ?? []), callback]);
    });
    const removeListener = vi.fn((name: string, callback: Listener) => {
      listeners.set(
        name,
        (listeners.get(name) ?? []).filter((item) => item !== callback)
      );
    });
    return {
      on,
      once: on,
      removeListener,
      emit: (name: string, ...args: unknown[]) => {
        for (const callback of [...(listeners.get(name) ?? [])]) callback(...args);
      },
    };
  };
  const sessions = new Map<string, Record<string, unknown>>();
  const views: Array<{
    options: { webPreferences: { session: Session } };
    webContents: {
      loadURL: ReturnType<typeof vi.fn>;
      reload: ReturnType<typeof vi.fn>;
      executeJavaScript: ReturnType<typeof vi.fn>;
      emit: (name: string, ...args: unknown[]) => void;
    };
  }> = [];
  const fromPartition = vi.fn((partition: string) => {
    if (!sessions.has(partition)) {
      sessions.set(partition, {
        setProxy: vi.fn().mockResolvedValue(undefined),
        setPermissionRequestHandler: vi.fn(),
        setPermissionCheckHandler: vi.fn(),
        setDevicePermissionHandler: vi.fn(),
        setDisplayMediaRequestHandler: vi.fn(),
        webRequest: {
          onBeforeRequest: vi.fn(),
          onBeforeSendHeaders: vi.fn(),
          onHeadersReceived: vi.fn(),
        },
        on: vi.fn(),
        clearStorageData: vi.fn().mockResolvedValue(undefined),
        closeAllConnections: vi.fn().mockResolvedValue(undefined),
      });
    }
    return sessions.get(partition);
  });
  return { sessions, views, fromPartition, events };
});

vi.mock('electron', () => ({
  app: { once: vi.fn() },
  BrowserWindow: class {},
  dialog: { showMessageBox: vi.fn() },
  shell: { openExternal: vi.fn() },
  session: { fromPartition: electron.fromPartition },
  WebContentsView: class {
    url = '';
    closed = false;
    webContents = {
      ...electron.events(),
      id: 71,
      session: null as unknown as Session,
      setWebRTCIPHandlingPolicy: vi.fn(),
      setWindowOpenHandler: vi.fn(),
      getURL: () => {
        if (this.closed) throw new Error('Object has been destroyed');
        return this.url;
      },
      getTitle: () => '',
      isLoading: () => false,
      isDestroyed: () => this.closed,
      loadURL: vi.fn(async (url: string) => {
        this.url = url;
        this.webContents.emit('dom-ready');
      }),
      reload: vi.fn(),
      executeJavaScript: vi.fn(async (source: string) =>
        new Function('document', `return ${source}`)(document)
      ),
      close: vi.fn(() => {
        this.closed = true;
      }),
      navigationHistory: {
        canGoBack: () => false,
        canGoForward: () => false,
      },
    };
    setBackgroundColor = vi.fn();
    setBounds = vi.fn();
    setVisible = vi.fn();
    constructor(public options: { webPreferences: { session: Session } }) {
      this.webContents.session = options.webPreferences.session;
      electron.views.push(this);
    }
  },
}));

vi.mock('electron-log', () => ({ default: { info: vi.fn(), warn: vi.fn() } }));
vi.mock('./embeddedBrowserProxy', () => ({
  startEmbeddedBrowserProxy: vi.fn().mockResolvedValue(41001),
  stopEmbeddedBrowserProxy: vi.fn().mockResolvedValue(undefined),
}));
vi.mock('./managedAppPreviewProxy', () => ({
  startManagedAppPreviewProxy: vi.fn(async () => ({ port: 42001, close: vi.fn() })),
}));

import {
  clearEmbeddedBrowserData,
  controlEmbeddedBrowser,
  createEmbeddedBrowser,
  destroyEmbeddedBrowser,
  destroyEmbeddedBrowsersForWindow,
  navigateEmbeddedBrowser,
  setEmbeddedBrowserVisible,
} from './embeddedBrowser';
import { startManagedAppPreviewProxy } from './managedAppPreviewProxy';
import { PREVIEW_ACTIVITY_INSTALL } from './previewActivity';

const owner = {
  ...electron.events(),
  webContents: { id: 19, isDestroyed: () => false, ...electron.events() },
  contentView: { addChildView: vi.fn(), removeChildView: vi.fn() },
  isDestroyed: () => false,
} as unknown as BrowserWindow;

const lifetimes: AbortController[] = [];
function backend() {
  const lifetime = new AbortController();
  lifetimes.push(lifetime);
  return { baseUrl: 'http://127.0.0.1:64005', signal: lifetime.signal };
}
afterEach(() => {
  new Function('window', `window[Symbol.for('biorouter.preview.activity.v1')]?.dispose()`)(window);
  document.body.replaceChildren();
  for (const lifetime of lifetimes.splice(0)) lifetime.abort();
  destroyEmbeddedBrowsersForWindow(owner);
  electron.views.length = 0;
  vi.clearAllMocks();
});

describe('managed app preview handoff', () => {
  it('reports main-owned managed provenance and refreshes the same view only when idle', async () => {
    const context = backend();
    const state = createEmbeddedBrowser(
      owner,
      'live',
      `${context.baseUrl}/apps/qa/`,
      vi.fn(),
      context
    );
    expect(state?.managedApp).toBe(true);
    setEmbeddedBrowserVisible(owner, 'live', true);
    await vi.waitFor(() => expect(electron.views[0].webContents.loadURL).toHaveBeenCalled());
    expect(await controlEmbeddedBrowser(owner, 'live', 'reload-if-idle')).toBe(true);
    expect(electron.views[0].webContents.reload).toHaveBeenCalledOnce();
    expect(electron.views).toHaveLength(1);
    const remote = createEmbeddedBrowser(owner, 'remote', 'https://example.test', vi.fn(), context);
    expect(remote?.managedApp).toBe(false);
    expect(await controlEmbeddedBrowser(owner, 'remote', 'reload-if-idle')).toBe(false);
    expect(electron.views[1].webContents.executeJavaScript).not.toHaveBeenCalled();
    expect(electron.views[1].webContents.reload).not.toHaveBeenCalled();
  });

  it.each(['input', 'textarea', 'iframe', 'contenteditable', 'shadow-input', 'busy'])(
    'defers managed refresh for %s without reading user values',
    async (kind) => {
      const context = backend();
      createEmbeddedBrowser(owner, 'editing', `${context.baseUrl}/apps/qa/`, vi.fn(), context);
      setEmbeddedBrowserVisible(owner, 'editing', true);
      await vi.waitFor(() => expect(electron.views[0].webContents.loadURL).toHaveBeenCalled());
      const element = document.createElement(
        kind === 'input' || kind === 'textarea' || kind === 'iframe' ? kind : 'div'
      );
      document.body.append(element);
      if (kind === 'contenteditable') {
        element.contentEditable = 'true';
        element.setAttribute('contenteditable', 'true');
        element.tabIndex = 0;
      }
      if (kind === 'busy') element.setAttribute('aria-busy', 'true');
      if (kind === 'shadow-input') {
        const input = document.createElement('input');
        element.attachShadow({ mode: 'open' }).append(input);
        input.focus();
      } else element.focus();
      expect(await controlEmbeddedBrowser(owner, 'editing', 'reload-if-idle')).toBe(false);
      expect(electron.views[0].webContents.reload).not.toHaveBeenCalled();
      const evaluations = electron.views[0].webContents.executeJavaScript.mock.calls;
      expect(evaluations[evaluations.length - 1][0]).not.toMatch(/\.value|textContent|innerHTML/);
    }
  );

  it.each(['navigation', 'replacement', 'abort', 'hidden'])(
    'drops a delayed idle result after %s',
    async (change) => {
      const context = backend();
      const url = `${context.baseUrl}/apps/qa/`;
      createEmbeddedBrowser(owner, 'racing', url, vi.fn(), context);
      setEmbeddedBrowserVisible(owner, 'racing', true);
      await vi.waitFor(() => expect(electron.views[0].webContents.loadURL).toHaveBeenCalled());
      const contents = electron.views[0].webContents;
      let resolve!: (value: boolean) => void;
      contents.executeJavaScript.mockImplementationOnce(
        () =>
          new Promise<boolean>((done) => {
            resolve = done;
          })
      );
      const pending = controlEmbeddedBrowser(owner, 'racing', 'reload-if-idle');
      if (change === 'navigation') contents.emit('did-navigate-in-page');
      else if (change === 'replacement')
        createEmbeddedBrowser(owner, 'racing', url, vi.fn(), context);
      else if (change === 'hidden') setEmbeddedBrowserVisible(owner, 'racing', false);
      else lifetimes[lifetimes.length - 1].abort();
      resolve(true);
      expect(await pending).toBe(false);
      expect(contents.reload).not.toHaveBeenCalled();
    }
  );

  it('does not discard dirty form state after focus has left the field', async () => {
    const context = backend();
    createEmbeddedBrowser(owner, 'dirty', `${context.baseUrl}/apps/qa/`, vi.fn(), context);
    setEmbeddedBrowserVisible(owner, 'dirty', true);
    await vi.waitFor(() => expect(electron.views[0].webContents.loadURL).toHaveBeenCalled());
    expect(electron.views[0].webContents.executeJavaScript).toHaveBeenCalledWith(
      PREVIEW_ACTIVITY_INSTALL
    );
    const input = document.createElement('input');
    document.body.append(input);
    input.focus();
    input.dispatchEvent(new Event('input', { bubbles: true }));
    input.blur();
    expect(await controlEmbeddedBrowser(owner, 'dirty', 'reload-if-idle')).toBe(false);
    expect(electron.views[0].webContents.reload).not.toHaveBeenCalled();
  });

  it('does not send a managed app through the public-only remote browser session', async () => {
    const url = 'http://127.0.0.1:64005/apps/queue-workbench/';
    const context = backend();
    // The extra argument is main-process-owned context, never a renderer flag.
    const state = createEmbeddedBrowser(owner, 'queue-preview', url, vi.fn(), context);
    expect(state).not.toBeNull();
    await vi.waitFor(() => expect(electron.views[0].webContents.loadURL).toHaveBeenCalledWith(url));

    const previewSession = electron.views[0].options.webPreferences.session;
    expect(previewSession).not.toBe(electron.sessions.get('persist:biorouter-embedded-browser'));
    expect(previewSession).not.toBe(electron.sessions.get('persist:biorouter'));
    expect(previewSession.setProxy).toHaveBeenCalledWith({
      proxyRules: expect.stringMatching(/^socks5:\/\/127\.0\.0\.1:\d+$/),
      proxyBypassRules: '<-loopback>',
    });
    expect(previewSession.setProxy).not.toHaveBeenCalledWith(
      expect.objectContaining({ proxyRules: 'socks5://127.0.0.1:41001' })
    );
  });

  it('retains app storage for reopen but isolates apps, owners, and backend generations', async () => {
    const context = backend();
    const url = `${context.baseUrl}/apps/queue-workbench/`;
    createEmbeddedBrowser(owner, 'a', url, vi.fn(), context);
    const first = electron.views[0].options.webPreferences.session;
    destroyEmbeddedBrowser(owner, 'a');
    createEmbeddedBrowser(owner, 'a2', url, vi.fn(), context);
    expect(electron.views[1].options.webPreferences.session).toBe(first);
    createEmbeddedBrowser(owner, 'b', `${context.baseUrl}/apps/another-app/`, vi.fn(), context);
    expect(electron.views[2].options.webPreferences.session).not.toBe(first);
    const otherOwner = {
      ...owner,
      ...electron.events(),
      webContents: { id: 20, isDestroyed: () => false },
    } as unknown as BrowserWindow;
    createEmbeddedBrowser(otherOwner, 'a', url, vi.fn(), context);
    expect(electron.views[3].options.webPreferences.session).not.toBe(first);
    destroyEmbeddedBrowsersForWindow(otherOwner);
    createEmbeddedBrowser(owner, 'next', url, vi.fn(), backend());
    expect(electron.views[4].options.webPreferences.session).not.toBe(first);
    await Promise.resolve();
  });

  it('blocks redirects and address-bar escapes without widening the remote session', async () => {
    const context = backend();
    const url = `${context.baseUrl}/apps/queue-workbench/`;
    createEmbeddedBrowser(owner, 'a', url, vi.fn(), context);
    await vi.waitFor(() => expect(electron.views[0].webContents.loadURL).toHaveBeenCalled());
    expect(navigateEmbeddedBrowser(owner, 'a', `${context.baseUrl}/sessions`)).toBe(false);
    expect(navigateEmbeddedBrowser(owner, 'a', `${context.baseUrl}/apps/other/`)).toBe(false);
    expect(navigateEmbeddedBrowser(owner, 'a', 'https://example.test')).toBe(false);
    const contents = electron.views[0].webContents as {
      emit: (name: string, ...args: unknown[]) => void;
      loadURL: ReturnType<typeof vi.fn>;
    };
    const event = { preventDefault: vi.fn() };
    contents.emit('will-redirect', event, `${context.baseUrl}/config`);
    expect(event.preventDefault).toHaveBeenCalledOnce();
    createEmbeddedBrowser(owner, 'remote', 'https://example.test', vi.fn(), context);
    expect(electron.views[1].options.webPreferences.session).toBe(
      electron.sessions.get('persist:biorouter-embedded-browser')
    );
  });

  it('denies workers and permissions, preserves CSP, and clears only this app session', async () => {
    const context = backend();
    createEmbeddedBrowser(owner, 'a', `${context.baseUrl}/apps/queue-workbench/`, vi.fn(), context);
    await vi.waitFor(() => expect(electron.views[0].webContents.loadURL).toHaveBeenCalled());
    const target = electron.views[0].options.webPreferences.session;
    const headersHook = (target.webRequest.onHeadersReceived as unknown as ReturnType<typeof vi.fn>)
      .mock.calls[0][1];
    const callback = vi.fn();
    headersHook(
      {
        responseHeaders: { 'content-security-policy': ["default-src 'none'"] },
      },
      callback
    );
    expect(callback).toHaveBeenCalledWith({
      cancel: false,
      responseHeaders: {
        'content-security-policy': [
          "default-src 'none'",
          expect.stringContaining("worker-src 'none'"),
        ],
      },
    });
    const workerHook = (
      target.webRequest.onBeforeSendHeaders as unknown as ReturnType<typeof vi.fn>
    ).mock.calls[0][1];
    workerHook(
      {
        requestHeaders: { 'Service-Worker': 'script' },
      },
      callback
    );
    expect(callback).toHaveBeenLastCalledWith({ cancel: true });
    const permission = vi.mocked(target.setPermissionRequestHandler).mock.calls[0][0]!;
    permission(
      {} as Electron.WebContents,
      'notifications',
      callback,
      {} as Electron.PermissionRequest
    );
    expect(callback).toHaveBeenLastCalledWith(false);
    await clearEmbeddedBrowserData(owner, 'a', true);
    expect(target.clearStorageData).toHaveBeenCalledOnce();
    const remote = electron.sessions.get('persist:biorouter-embedded-browser');
    if (remote) expect(remote.clearStorageData).not.toHaveBeenCalled();
  });

  it('revokes the transport, view, and its ephemeral storage immediately on abort', async () => {
    const context = backend();
    createEmbeddedBrowser(owner, 'a', `${context.baseUrl}/apps/queue-workbench/`, vi.fn(), context);
    await vi.waitFor(() => expect(electron.views[0].webContents.loadURL).toHaveBeenCalled());
    const target = electron.views[0].options.webPreferences.session;
    const proxyCalls = vi.mocked(startManagedAppPreviewProxy).mock.calls;
    const proxySignal = proxyCalls[proxyCalls.length - 1][1];
    lifetimes[lifetimes.length - 1].abort();
    expect(proxySignal.aborted).toBe(true);
    expect(target.closeAllConnections).toHaveBeenCalledOnce();
    expect(target.clearStorageData).toHaveBeenCalledOnce();
    expect(navigateEmbeddedBrowser(owner, 'a', `${context.baseUrl}/apps/queue-workbench/`)).toBe(
      false
    );
  });

  it.each(['auth-workbench', 'login', 'oauth-helper'])(
    'loads and navigates the authorized app %s without remote auth diversion',
    async (id) => {
      const context = backend();
      const url = `${context.baseUrl}/apps/${id}/?client_id=synthetic`;
      createEmbeddedBrowser(owner, 'auth-name', url, vi.fn(), context);
      await vi.waitFor(() =>
        expect(electron.views[0].webContents.loadURL).toHaveBeenCalledWith(url)
      );
      expect(navigateEmbeddedBrowser(owner, 'auth-name', url)).toBe(true);
      const contents = electron.views[0].webContents as {
        emit: (name: string, ...args: unknown[]) => void;
        loadURL: ReturnType<typeof vi.fn>;
      };
      const event = { preventDefault: vi.fn() };
      contents.emit('will-redirect', event, url);
      expect(event.preventDefault).not.toHaveBeenCalled();
    }
  );

  it.each(['proxy rejection', 'abort before startup'])(
    'handles %s without reading destroyed webContents',
    async (scenario) => {
      let rejectProxy!: (error: Error) => void;
      vi.mocked(startManagedAppPreviewProxy).mockImplementationOnce(
        () =>
          new Promise((_resolve, reject) => {
            rejectProxy = reject;
          })
      );
      const context = backend();
      const state = vi.fn();
      createEmbeddedBrowser(
        owner,
        'pending',
        `${context.baseUrl}/apps/queue-workbench/`,
        state,
        context
      );
      if (scenario === 'abort before startup') lifetimes[lifetimes.length - 1].abort();
      rejectProxy(new Error('Synthetic proxy startup failure'));
      await vi.waitFor(() =>
        expect(
          electron.views[0].options.webPreferences.session.closeAllConnections
        ).toHaveBeenCalledOnce()
      );
      expect(electron.views[0].webContents.loadURL).not.toHaveBeenCalled();
      expect(
        navigateEmbeddedBrowser(owner, 'pending', `${context.baseUrl}/apps/queue-workbench/`)
      ).toBe(false);
    }
  );
});
