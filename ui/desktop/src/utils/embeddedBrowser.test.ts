import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { isNavigableEmbeddedUrl } from './permissionPolicy';

/**
 * The embedded browser is the one place in this app that renders arbitrary
 * remote content, so its guards are asserted here rather than left to review.
 *
 * Most of these read the module's real source. That is deliberate: the failures
 * being guarded against are *omissions* — a handler that stops being installed,
 * a partition that reverts to the app's — and there is no value to import that
 * would reveal them. Electron is not available under vitest, so behaviour that
 * needs a live session is verified in a real Electron run instead.
 */

const source = readFileSync(join(__dirname, 'embeddedBrowser.ts'), 'utf8');
const proxySource = readFileSync(join(__dirname, 'embeddedBrowserProxy.ts'), 'utf8');

describe('isNavigableEmbeddedUrl', () => {
  it.each(['https://www.ucsf.edu/', 'http://example.test/path?q=1', 'https://pubmed.gov'])(
    'admits %s',
    (url) => expect(isNavigableEmbeddedUrl(url)).toBe(true)
  );

  it.each([
    'file:///etc/passwd',
    'data:text/html,<script>alert(1)</script>',
    'blob:https://example.test/id',
    'javascript:alert(1)',
    'about:blank',
    'not a url',
    '',
  ])('refuses %s', (url) => expect(isNavigableEmbeddedUrl(url)).toBe(false));

  // Credentials in a URL are a phishing primitive: the visible host is not the
  // host that gets contacted.
  it('refuses a URL carrying embedded credentials', () => {
    expect(isNavigableEmbeddedUrl('https://user:pass@evil.test/')).toBe(false);
    expect(isNavigableEmbeddedUrl('https://user@evil.test/')).toBe(false);
  });
});

describe('the embedded session is hardened, not inherited', () => {
  // The finding this whole file exists for. A handler on `defaultSession` does
  // NOT cover a partitioned session, and an unhandled session GRANTS BY
  // DEFAULT — measured with two views side by side, where the unhandled one
  // returned `granted` for notifications and allowed geolocation while the
  // app's handler was never called. Without these four lines the browser would
  // silently auto-grant notifications, geolocation, clipboard-read, media and
  // display-capture to every site the user visits.
  it.each([
    'setPermissionRequestHandler',
    'setPermissionCheckHandler',
    'setDevicePermissionHandler',
    'setDisplayMediaRequestHandler',
  ])('installs %s on its own session', (handler) => {
    expect(source).toContain(`embedded.${handler}(`);
  });

  it('uses its own partition, never the app’s', () => {
    expect(source).toContain("'persist:biorouter-embedded-browser'");
    expect(source).not.toContain("'persist:biorouter'");
    expect(source).toContain('session.fromPartition(EMBEDDED_PARTITION)');
  });

  it('never gives the remote page a preload', () => {
    // The structural guarantee: the app's IPC bridge is attached per
    // `webPreferences`, so a view constructed without one has nothing to reach.
    expect(source).not.toMatch(/preload:\s*(?!undefined)/);
    expect(source).toContain('sandbox: true');
    expect(source).toContain('contextIsolation: true');
    expect(source).toContain('nodeIntegration: false');
    // An embedded page must not be able to spawn its own guest views.
    expect(source).toContain('webviewTag: false');
  });

  it('blocks the daemon and the filesystem from the embedded partition', () => {
    // The daemon listens on loopback under a per-launch secret; a page that
    // could reach it would own the agent's tools.
    expect(source).toContain('onBeforeRequest');
    expect(source).toContain("urls: ['<all_urls>']");
    expect(source).toContain('isAllowedEmbeddedRequestUrl(details.url)');
    expect(source).toContain("proxyBypassRules: '<-loopback>'");
    expect(source).toContain("setWebRTCIPHandlingPolicy('disable_non_proxied_udp')");
    expect(proxySource).toContain('host: target.address');
    expect(proxySource).not.toContain('host: candidate');
  });

  it('blocks page-created windows and auth navigation without opening native dialogs', () => {
    expect(source).toContain('setWindowOpenHandler');
    expect(source).toContain("action: 'deny'");
    expect(source).toContain("contents.on('will-navigate', guardNavigation)");
    expect(source).toContain("contents.on('will-redirect', guardNavigation)");
    const remoteNavigationHandlers = source.slice(
      source.indexOf('contents.setWindowOpenHandler'),
      source.indexOf("contents.on('did-start-loading'")
    );
    expect(remoteNavigationHandlers).not.toContain('requestAuthenticationConfirmation');
    expect(remoteNavigationHandlers).not.toContain('dialog.');
    expect(remoteNavigationHandlers).not.toContain('shell.openExternal');
  });

  it('requires exact-host native confirmation and fresh validation for external navigation', () => {
    const confirmation = source.slice(
      source.indexOf('async function confirmPublicExternalNavigation'),
      source.indexOf('async function confirmExternalAuthenticationNavigation')
    );
    expect(confirmation).toContain('dialog.showMessageBox');
    expect(confirmation.match(/validateExternalBrowserTarget/g)).toHaveLength(2);
    expect(confirmation).toContain('result.response !== 1');
    expect(confirmation).toContain('Destination hostname: ${validated.hostname}');
    expect(confirmation).toContain('revalidated.hostname !== validated.hostname');
    const guard = source.slice(
      source.indexOf('const guardNavigation'),
      source.indexOf("contents.on('will-navigate'")
    );
    expect(guard).not.toContain('shell.openExternal');
  });

  it('never loads initial or address-bar authentication URLs without native confirmation', () => {
    const initialLoad = source.slice(
      source.indexOf('void prepareEmbeddedNetwork(embedded).then'),
      source.indexOf('return readState(entry)')
    );
    expect(initialLoad).toContain('isAuthenticationNavigation(initialUrl)');
    expect(initialLoad).toContain('entry.requestAuthenticationConfirmation(initialUrl)');

    const addressBarLoad = source.slice(
      source.indexOf('export function navigateEmbeddedBrowser'),
      source.indexOf('export function controlEmbeddedBrowser')
    );
    expect(addressBarLoad).toContain('isAuthenticationNavigation(url)');
    expect(addressBarLoad).toContain('entry.requestAuthenticationConfirmation(url)');
  });

  it('routes every renderer-controlled system-browser open through the confirmation policy', () => {
    const main = readFileSync(join(__dirname, '..', 'main.ts'), 'utf8');
    const genericOpen = main.slice(
      main.indexOf("ipcMain.handle('open-external'"),
      main.indexOf("ipcMain.handle('directory-chooser'")
    );
    expect(genericOpen).toContain('openExternalBrowserNavigation(window, url)');
    expect(genericOpen).not.toContain('shell.openExternal');

    const legacyOpen = main.slice(
      main.indexOf("ipcMain.on('open-in-chrome'"),
      main.indexOf("ipcMain.on('restart-app'")
    );
    expect(legacyOpen).toContain('openExternalBrowserNavigation(window, url)');
    expect(legacyOpen).not.toContain('shell.openExternal');
  });

  it('intercepts downloads rather than letting a page write to disk', () => {
    expect(source).toContain("embedded.on('will-download'");
    expect(source).toContain('event.preventDefault()');
  });
});

describe('the panel keeps the native view honest', () => {
  // A native view paints above the DOM with no shared stacking context, so it
  // has to be told to hide. Losing this makes the view cover modals and menus.
  it('can be hidden independently of its bounds', () => {
    expect(source).toContain('export function setEmbeddedBrowserVisible');
    expect(source).toContain('entry.view.setVisible(visible)');
  });

  it('reports an empty capture rather than an empty PNG', () => {
    // capturePage returns a zero-byte image for a hidden-then-navigated view
    // and does not reject, so the empty case has to be checked explicitly.
    expect(source).toContain('image.isEmpty()');
    expect(source).toContain('revision !== sourceRevision(entry)');
  });

  it('revisions committed navigation and never returns a mixed-document text snapshot', () => {
    expect(source).toContain("contents.on('did-navigate'");
    expect(source).toContain("contents.on('did-navigate-in-page'");
    expect(source).toContain('entry.revision += 1');
    expect(source).toContain('if (contents.isLoadingMainFrame()) return null');
    expect(source).toContain('revision !== entry.revision || url !== contents.getURL()');
    expect(source).toContain('sourceRevision: sourceRevision(entry)');
    expect(source).not.toMatch(/executeJavaScript\([\s\S]{0,500},\s*true\s*\)/);
  });

  it('tears views down with the window that owns them', () => {
    // The view is a child of the window's contentView, not of the React tree,
    // so nothing in the renderer unmounts it.
    expect(source).toContain('export function destroyEmbeddedBrowsersForWindow');
    const main = readFileSync(join(__dirname, '..', 'main.ts'), 'utf8');
    expect(main).toContain('destroyEmbeddedBrowsersForWindow(mainWindow)');
  });

  it('resolves the owning window from the event sender, not from the renderer', () => {
    // Otherwise one window could drive another window's view.
    const main = readFileSync(join(__dirname, '..', 'main.ts'), 'utf8');
    const embeddedHandlers = main.slice(main.indexOf("'embedded-browser:create'"));
    expect(
      embeddedHandlers.match(/BrowserWindow\.fromWebContents\(event\.sender\)/g)?.length
    ).toBeGreaterThanOrEqual(9);
    expect(source).toContain('views.get(viewKey(window, viewId))');
  });

  it('answers display-capture denial instead of leaving the page pending', () => {
    expect(source).toContain('setDisplayMediaRequestHandler((_request, callback) => callback({}))');
  });
});
