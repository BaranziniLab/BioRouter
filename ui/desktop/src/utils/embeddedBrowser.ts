import { BrowserWindow, WebContentsView, session, shell, type Session } from 'electron';
import log from 'electron-log';
import { normalizeExternalHttpUrl } from './externalUrl';
import { isNavigableEmbeddedUrl } from './permissionPolicy';

/**
 * A real, interactive browser inside the artifact panel.
 *
 * **Why a `WebContentsView` and not an `<iframe>`.** Half of the sites this
 * audience actually opens refuse to be framed — `google.com`, PubMed, NCBI
 * Gene, ClinicalTrials.gov and Nature all send `X-Frame-Options` or
 * `frame-ancestors`, while `ucsf.edu` does not. An iframe implementation
 * therefore demos perfectly and fails on the destinations that matter. It is
 * also unfixable by stripping those headers: a cross-site frame is a
 * third-party context where `SameSite=Lax` cookies are not sent, so the page
 * would render logged-out and half-broken, and clickjacking protection would be
 * gone for nothing. A `WebContentsView` is a top-level browsing context, so
 * those headers do not apply and the page behaves exactly as it does in Chrome
 * — clicking, typing, scrolling, JS, cookies and logins all work.
 *
 * **What it costs.** A native view paints above the DOM. It does not respect
 * stacking, scrolling, border radius or modals, so the renderer must tell us
 * where it goes and hide it whenever something needs to sit on top. That bill
 * is the price of the paragraph above, and it is paid in `setBounds`/`setVisible`.
 */

/**
 * A partition of its own — **never** the app's `persist:biorouter`.
 *
 * Sharing the app's partition would put arbitrary third-party cookies, storage
 * and service workers in the same jar as BioRouter's own origin, and would let
 * an embedded page reach the daemon's session.
 */
const EMBEDDED_PARTITION = 'persist:biorouter-embedded-browser';

export type EmbeddedBrowserBounds = { x: number; y: number; width: number; height: number };

export type EmbeddedBrowserState = {
  url: string;
  title: string;
  canGoBack: boolean;
  canGoForward: boolean;
  isLoading: boolean;
  error: string | null;
};

type Entry = {
  view: WebContentsView;
  window: BrowserWindow;
  visible: boolean;
  bounds: EmbeddedBrowserBounds;
};

const views = new Map<string, Entry>();
let hardenedSession: Session | null = null;

/**
 * Locks down the embedded partition.
 *
 * ⚠ **This is the single most important function in this file, and the reason
 * it exists at all.** A permission handler installed on `session.defaultSession`
 * — which is where the app installs its own — **does not cover a partitioned
 * session, and an unhandled session grants by default.** Measured with two
 * views side by side: the one whose partition had no handler returned
 * `granted` for `Notification.requestPermission()` and allowed geolocation,
 * while the app's handler was never called once.
 *
 * So an embedded browser that simply inherited the app's setup would silently
 * auto-grant notifications, geolocation, clipboard-read, media and
 * display-capture to every site the user visited.
 *
 * Both handlers are needed: `setPermissionRequestHandler` covers asynchronous
 * prompts, `setPermissionCheckHandler` covers the synchronous checks behind
 * `navigator.permissions.query` and device enumeration.
 */
function embeddedSession(): Session {
  if (hardenedSession) return hardenedSession;

  const embedded = session.fromPartition(EMBEDDED_PARTITION);

  embedded.setPermissionRequestHandler((_contents, permission, callback) => {
    log.info('[EmbeddedBrowser] denied permission request:', permission);
    callback(false);
  });
  embedded.setPermissionCheckHandler((_contents, permission) => {
    log.info('[EmbeddedBrowser] denied permission check:', permission);
    return false;
  });
  // WebUSB / Serial / HID.
  embedded.setDevicePermissionHandler(() => false);
  // `getDisplayMedia`. Returning an empty object denies without a prompt.
  embedded.setDisplayMediaRequestHandler(() => {});

  // The embedded page must never reach the daemon. It listens on loopback under
  // a per-launch secret, and a site that could talk to it would own the agent's
  // tools. `file:` is blocked for the same class of reason.
  embedded.webRequest.onBeforeRequest({ urls: ['*://*/*', 'file://*/*'] }, (details, callback) => {
    let hostname = '';
    let protocol = '';
    try {
      const parsed = new URL(details.url);
      hostname = parsed.hostname;
      protocol = parsed.protocol;
    } catch {
      callback({ cancel: true });
      return;
    }
    const isLoopback =
      hostname === 'localhost' ||
      hostname === '127.0.0.1' ||
      hostname === '0.0.0.0' ||
      hostname === '::1' ||
      hostname === '[::1]';
    callback({ cancel: protocol === 'file:' || isLoopback });
  });

  // Downloads are the user's call, not the page's.
  embedded.on('will-download', (event, item) => {
    event.preventDefault();
    log.info('[EmbeddedBrowser] blocked download:', item.getURL());
  });

  hardenedSession = embedded;
  return embedded;
}

function readState(view: WebContentsView, error: string | null = null): EmbeddedBrowserState {
  const contents = view.webContents;
  return {
    url: contents.getURL(),
    title: contents.getTitle(),
    canGoBack: contents.navigationHistory.canGoBack(),
    canGoForward: contents.navigationHistory.canGoForward(),
    isLoading: contents.isLoading(),
    error,
  };
}

export function createEmbeddedBrowser(
  window: BrowserWindow,
  viewId: string,
  initialUrl: string,
  onState: (state: EmbeddedBrowserState) => void
): EmbeddedBrowserState | null {
  if (!isNavigableEmbeddedUrl(initialUrl)) return null;
  destroyEmbeddedBrowser(viewId);

  const view = new WebContentsView({
    webPreferences: {
      session: embeddedSession(),
      // No preload at all. The bridge this app exposes is attached per
      // `webPreferences`; a view constructed without one has nothing to reach.
      // That is the structural guarantee, and it is why the toolbar is driven
      // from the main process rather than from inside the page.
      sandbox: true,
      contextIsolation: true,
      nodeIntegration: false,
      nodeIntegrationInWorker: false,
      nodeIntegrationInSubFrames: false,
      // An embedded page must not be able to spawn its own guest views.
      webviewTag: false,
      webSecurity: true,
      allowRunningInsecureContent: false,
      experimentalFeatures: false,
      plugins: false,
      safeDialogs: true,
      safeDialogsMessage: 'This page is showing too many dialogs.',
      navigateOnDragDrop: false,
      spellcheck: false,
    },
  });

  const contents = view.webContents;

  // Every `window.open` and `target=_blank` leaves the app. Validate the scheme
  // *before* handing anything to the OS opener, never after.
  contents.setWindowOpenHandler(({ url }) => {
    try {
      shell.openExternal(normalizeExternalHttpUrl(url));
    } catch {
      log.warn('[EmbeddedBrowser] refused to open external url');
    }
    return { action: 'deny' };
  });

  const blockNonHttp = (event: Electron.Event, url: string) => {
    if (isNavigableEmbeddedUrl(url)) return;
    log.warn('[EmbeddedBrowser] blocked navigation to a non-http(s) url');
    event.preventDefault();
  };
  contents.on('will-navigate', blockNonHttp);
  contents.on('will-redirect', blockNonHttp);

  const push = (error: string | null = null) => onState(readState(view, error));
  contents.on('did-start-loading', () => push());
  contents.on('did-stop-loading', () => push());
  contents.on('did-navigate', () => push());
  // Without this the URL bar goes stale on every single-page-app route change.
  contents.on('did-navigate-in-page', () => push());
  contents.on('page-title-updated', () => push());
  contents.on('did-fail-load', (_event, errorCode, errorDescription, _url, isMainFrame) => {
    // Subframe failures are constant background noise, and -3 (ERR_ABORTED) is
    // what an ordinary user-cancelled navigation looks like.
    if (!isMainFrame || errorCode === -3) return;
    push(errorDescription || 'This page could not be loaded.');
  });
  contents.on('render-process-gone', () => push('This page stopped responding and was closed.'));

  window.contentView.addChildView(view);
  // BrowserView was transparent; WebContentsView defaults to white, which flashes
  // against a dark panel before the first paint.
  view.setBackgroundColor('#00000000');
  const bounds = { x: 0, y: 0, width: 0, height: 0 };
  view.setBounds(bounds);
  view.setVisible(false);

  views.set(viewId, { view, window, visible: false, bounds });
  void contents.loadURL(initialUrl);
  return readState(view);
}

export function setEmbeddedBrowserBounds(viewId: string, bounds: EmbeddedBrowserBounds): void {
  const entry = views.get(viewId);
  if (!entry) return;
  entry.bounds = bounds;
  entry.view.setBounds({
    x: Math.round(bounds.x),
    y: Math.round(bounds.y),
    width: Math.max(0, Math.round(bounds.width)),
    height: Math.max(0, Math.round(bounds.height)),
  });
}

/**
 * Show or hide the view.
 *
 * The renderer calls this whenever something has to sit on top — a modal, a
 * dropdown, a toast, the resize shield — because a native view has no shared
 * z-index with the DOM and would otherwise paint straight over them.
 */
export function setEmbeddedBrowserVisible(viewId: string, visible: boolean): void {
  const entry = views.get(viewId);
  if (!entry) return;
  entry.visible = visible;
  entry.view.setVisible(visible);
}

export function navigateEmbeddedBrowser(viewId: string, url: string): boolean {
  const entry = views.get(viewId);
  if (!entry || !isNavigableEmbeddedUrl(url)) return false;
  void entry.view.webContents.loadURL(url);
  return true;
}

export function controlEmbeddedBrowser(
  viewId: string,
  action: 'back' | 'forward' | 'reload' | 'stop'
): boolean {
  const entry = views.get(viewId);
  if (!entry) return false;
  const contents = entry.view.webContents;
  if (action === 'back' && contents.navigationHistory.canGoBack()) {
    contents.navigationHistory.goBack();
  } else if (action === 'forward' && contents.navigationHistory.canGoForward()) {
    contents.navigationHistory.goForward();
  } else if (action === 'reload') {
    contents.reload();
  } else if (action === 'stop') {
    contents.stop();
  }
  return true;
}

export function embeddedBrowserState(viewId: string): EmbeddedBrowserState | null {
  const entry = views.get(viewId);
  return entry ? readState(entry.view) : null;
}

/**
 * The visible text of the embedded page, for the agent.
 *
 * Runs in the page's own world via `executeJavaScript`, which is the only way
 * in: the view shares no origin with the renderer and has no preload. The
 * expression is a fixed literal — nothing from the caller is interpolated into
 * it — and the result is a string the caller must still treat as untrusted page
 * content.
 */
export async function readEmbeddedBrowserText(
  viewId: string,
  maxChars: number
): Promise<{ url: string; title: string; text: string } | null> {
  const entry = views.get(viewId);
  if (!entry) return null;
  const contents = entry.view.webContents;
  const text = (await contents.executeJavaScript(
    '(() => (document.body && document.body.innerText) || "")()',
    true
  )) as string;
  return {
    url: contents.getURL(),
    title: contents.getTitle(),
    text: typeof text === 'string' ? text.slice(0, maxChars) : '',
  };
}

/**
 * A PNG of the embedded page.
 *
 * `capturePage` grabs the compositor's current surface rather than forcing a
 * render, so it works while the view is occluded or off-screen, but returns the
 * *last painted frame* for a hidden window and an **empty image** if the view
 * was hidden and then navigated. Callers must check for the empty case — the
 * failure mode is a zero-byte buffer, not a rejection.
 */
export async function captureEmbeddedBrowser(viewId: string): Promise<Buffer | null> {
  const entry = views.get(viewId);
  if (!entry) return null;
  const image = await entry.view.webContents.capturePage();
  return image.isEmpty() ? null : image.toPNG();
}

export function destroyEmbeddedBrowser(viewId: string): void {
  const entry = views.get(viewId);
  if (!entry) return;
  views.delete(viewId);
  try {
    entry.window.contentView.removeChildView(entry.view);
  } catch {
    // The window may already be gone; the view goes with it.
  }
  entry.view.webContents.close();
}

/** Tears down every view belonging to a window that is closing. */
export function destroyEmbeddedBrowsersForWindow(window: BrowserWindow): void {
  for (const [viewId, entry] of [...views.entries()]) {
    if (entry.window === window) destroyEmbeddedBrowser(viewId);
  }
}
