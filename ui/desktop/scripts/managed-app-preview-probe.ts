import { app, BrowserWindow, session, type WebContentsView } from 'electron';
import assert from 'node:assert/strict';
import http from 'node:http';
import net, { type Socket } from 'node:net';
import path from 'node:path';
import { readFile, writeFile } from 'node:fs/promises';
import {
  createEmbeddedBrowser,
  controlEmbeddedBrowser,
  navigateEmbeddedBrowser,
  destroyEmbeddedBrowsersForWindow,
  setEmbeddedBrowserBounds,
  setEmbeddedBrowserVisible,
} from '../src/utils/embeddedBrowser';

const output = process.env.BIOROUTER_MANAGED_PREVIEW_PROBE_DIR;
if (!output) throw new Error('Run this probe through check-managed-app-preview.mjs');
app.setPath('userData', path.join(output, 'electron-profile'));
const observations: Array<{ check: string; pass: boolean; detail?: unknown }> = [];
const sockets = new Set<Socket>();
const windows: BrowserWindow[] = [];
const lifetime = new AbortController();
const requests: string[] = [];
const sentinelEvents: Array<{ bytes: number; turn: boolean; tls: boolean }> = [];
let bundleVersion = 1;

const daemon = http.createServer((request, response) => {
  requests.push(request.url ?? '');
  assert.equal(
    request.headers['x-secret-key'],
    undefined,
    'Preview must never receive daemon credentials'
  );
  response.setHeader(
    'Content-Security-Policy',
    "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self'; worker-src 'self'"
  );
  if (request.url === '/apps/qa/dist/app.js') {
    response.setHeader('Content-Type', 'text/javascript');
    response.end(
      `document.getElementById('total').textContent = String([30,45,20].reduce((a,b)=>a+b,0)); document.body.dataset.bundleVersion = '${bundleVersion}';`
    );
  } else if (request.url === '/apps/qa/assets/worker.js') {
    response.setHeader('Content-Type', 'text/javascript');
    response.end('self.postMessage("worker-ran")');
  } else if (request.url === '/apps/qa/') {
    response.setHeader('Content-Type', 'text/html');
    response.end(
      '<!doctype html><title>Synthetic queue</title><p id="total"></p><input id="draft" aria-label="Synthetic draft"><script src="dist/app.js"></script>'
    );
  } else {
    response.statusCode = 404;
    response.end('Synthetic denied route');
  }
});
const sentinel = net.createServer((socket) => {
  const event = { bytes: 0, turn: false, tls: false };
  sentinelEvents.push(event);
  socket.on('data', (chunk: Buffer) => {
    event.bytes += chunk.length;
    event.turn ||= chunk.length >= 8 && chunk.readUInt32BE(4) === 0x2112a442;
    event.tls ||= chunk[0] === 0x16 && chunk[1] === 0x03;
  });
});
for (const server of [daemon, sentinel]) {
  server.on('connection', (socket: Socket) => {
    sockets.add(socket);
    socket.on('error', () => socket.destroy());
    socket.on('close', () => sockets.delete(socket));
  });
}

async function listen(server: http.Server | net.Server): Promise<number> {
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen({ host: '127.0.0.1', port: 0 }, resolve);
  });
  return (server.address() as net.AddressInfo).port;
}
async function until(predicate: () => boolean, label: string): Promise<void> {
  const start = Date.now();
  while (!predicate()) {
    if (Date.now() - start > 8000) throw new Error(`Timed out: ${label}`);
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
}
async function check(name: string, run: () => Promise<unknown>): Promise<void> {
  try {
    const detail = await run();
    observations.push({ check: name, pass: true, detail });
    console.log(`PASS ${name}`);
  } catch (error) {
    observations.push({ check: name, pass: false, detail: String(error) });
    console.error(`FAIL ${name}: ${String(error)}`);
  }
}

function rtcProbe(url: string): string {
  return `(async () => {
    const errors = [];
    const pc = new RTCPeerConnection({iceServers:[{urls:${JSON.stringify(url)},username:'synthetic-qa',credential:'synthetic-only'}],iceTransportPolicy:'relay'});
    pc.onicecandidateerror = event => errors.push({code:event.errorCode, text:event.errorText});
    pc.createDataChannel('synthetic');
    await pc.setLocalDescription(await pc.createOffer());
    await new Promise(resolve => setTimeout(resolve, 2500));
    const state = pc.iceGatheringState;
    pc.close();
    return {state,errors};
  })()`;
}

async function run(): Promise<void> {
  await app.whenReady();
  const daemonPort = await listen(daemon);
  const sentinelPort = await listen(sentinel);
  const origin = `http://127.0.0.1:${daemonPort}`;
  const url = `${origin}/apps/qa/`;
  const owner = new BrowserWindow({
    show: false,
    webPreferences: { sandbox: true, contextIsolation: true, nodeIntegration: false },
  });
  windows.push(owner);
  const states: Array<{ error: string | null }> = [];
  createEmbeddedBrowser(owner, 'managed-qa', url, (state) => states.push(state), {
    baseUrl: origin,
    signal: lifetime.signal,
  });
  const view = owner.contentView.children[0] as WebContentsView;
  const contents = view.webContents;
  setEmbeddedBrowserBounds(owner, 'managed-qa', { x: 0, y: 0, width: 800, height: 600 });
  setEmbeddedBrowserVisible(owner, 'managed-qa', true);
  async function reloadAndWait(action: 'reload' | 'reload-if-idle'): Promise<void> {
    let finished = false;
    const didFinish = () => {
      finished = true;
    };
    contents.once('did-finish-load', didFinish);
    try {
      assert.equal(await controlEmbeddedBrowser(owner, 'managed-qa', action), true);
      await until(() => finished, `${action} document commit`);
    } finally {
      contents.removeListener('did-finish-load', didFinish);
    }
  }
  await check('actual managed preview loads its app and bundle without credentials', async () => {
    await until(() => contents.getURL() === url && !contents.isLoading(), 'managed app load');
    assert.equal(
      await contents.executeJavaScript("document.getElementById('total').textContent"),
      '95'
    );
    assert.deepEqual(
      states.filter((state) => state.error),
      []
    );
    assert.ok(requests.includes('/apps/qa/dist/app.js'));
  });
  await check(
    'daemon API, cross-app, other listener, and service worker stay unreachable',
    async () => {
      const before = requests.length;
      const denied = await contents.executeJavaScript(`(async()=>{
      const results = [];
      for(const url of ['/sessions','/apps/other/','/apps/qa/export','http://127.0.0.1:${sentinelPort}/']) {
        try { await fetch(url); results.push(false); } catch { results.push(true); }
      }
      try { await navigator.serviceWorker.register('/apps/qa/assets/worker.js'); results.push(false); } catch { results.push(true); }
      return results;
    })()`);
      assert.deepEqual(denied, [true, true, true, true, true]);
      assert.deepEqual(requests.slice(before), []);
      assert.equal(navigateEmbeddedBrowser(owner, 'managed-qa', `${origin}/apps/other/`), false);
      assert.equal(navigateEmbeddedBrowser(owner, 'managed-qa', 'https://example.invalid/'), false);
    }
  );
  await check(
    'idle managed refresh shows changed source in the same view and session',
    async () => {
      const originalId = contents.id;
      const originalSession = contents.session;
      bundleVersion = 2;
      await reloadAndWait('reload-if-idle');
      assert.equal(await contents.executeJavaScript('document.body.dataset.bundleVersion'), '2');
      assert.equal(contents.id, originalId);
      assert.equal(contents.session, originalSession);
      assert.equal(owner.contentView.children[0], view);
    }
  );
  await check(
    'an unsaved typed form stays intact after blur when a newer bundle is ready',
    async () => {
      await contents.executeJavaScript("document.getElementById('draft').focus()");
      await contents.insertText('Synthetic unsaved draft');
      assert.equal(
        await contents.executeJavaScript("document.getElementById('draft').value"),
        'Synthetic unsaved draft'
      );
      await contents.executeJavaScript("document.getElementById('draft').blur()");
      bundleVersion = 3;
      assert.equal(await controlEmbeddedBrowser(owner, 'managed-qa', 'reload-if-idle'), false);
      assert.equal(await contents.executeJavaScript('document.body.dataset.bundleVersion'), '2');
      assert.equal(
        await contents.executeJavaScript("document.getElementById('draft').value"),
        'Synthetic unsaved draft'
      );
    }
  );
  await check('IndexedDB survives a page reload within this isolated app', async () => {
    const saved = await contents.executeJavaScript(`(async()=>{
      const db = await new Promise((resolve,reject)=>{const r=indexedDB.open('synthetic-qa',1);r.onupgradeneeded=()=>r.result.createObjectStore('queue');r.onsuccess=()=>resolve(r.result);r.onerror=()=>reject(r.error)});
      await new Promise((resolve,reject)=>{const t=db.transaction('queue','readwrite');t.objectStore('queue').put(100,'minutes');t.oncomplete=resolve;t.onerror=()=>reject(t.error)});
      db.close();return true;
    })()`);
    assert.equal(saved, true);
    await reloadAndWait('reload');
    assert.equal(await contents.executeJavaScript('document.body.dataset.bundleVersion'), '3');
    const value = await contents.executeJavaScript(`(async()=>{
      const db=await new Promise((resolve,reject)=>{const r=indexedDB.open('synthetic-qa',1);r.onsuccess=()=>resolve(r.result);r.onerror=()=>reject(r.error)});
      const value=await new Promise((resolve,reject)=>{const r=db.transaction('queue').objectStore('queue').get('minutes');r.onsuccess=()=>resolve(r.result);r.onerror=()=>reject(r.error)});db.close();return value;
    })()`);
    assert.equal(value, 100);
  });
  await check(
    'SDK cold connections and pending call/KB maps defer refresh, while an OPEN idle client allows it',
    async () => {
      // The actual renderer receives an SDK-shaped synthetic client. Only activity
      // counts/socket readiness are under test; this does not exercise provider calls.
      await contents.executeJavaScript(
        `window.BioRouter = { pendingCalls: new Map(), pendingKb: new Map(), ws: null }`
      );
      assert.equal(await controlEmbeddedBrowser(owner, 'managed-qa', 'reload-if-idle'), false);
      await contents.executeJavaScript(
        'window.BioRouter.ws = { readyState: WebSocket.CONNECTING }'
      );
      assert.equal(await controlEmbeddedBrowser(owner, 'managed-qa', 'reload-if-idle'), false);
      await contents.executeJavaScript('window.BioRouter.ws = { readyState: WebSocket.OPEN }');
      for (const field of ['pendingCalls', 'pendingKb']) {
        await contents.executeJavaScript(`window.BioRouter.${field}.set('synthetic-request', {})`);
        assert.equal(await controlEmbeddedBrowser(owner, 'managed-qa', 'reload-if-idle'), false);
        await contents.executeJavaScript(`window.BioRouter.${field}.clear()`);
      }
      bundleVersion = 4;
      await reloadAndWait('reload-if-idle');
      assert.equal(await contents.executeJavaScript('document.body.dataset.bundleVersion'), '4');
      assert.equal(owner.contentView.children[0], view);
    }
  );

  // Direct mode exists only in this isolated synthetic positive-control window.
  // It proves the local TURN listener is reachable and this Chromium build attempts TCP.
  const controlSession = session.fromPartition('managed-preview-probe-positive-control');
  await controlSession.setProxy({ mode: 'direct' });
  const control = new BrowserWindow({
    show: false,
    webPreferences: {
      session: controlSession,
      sandbox: true,
      nodeIntegration: false,
      contextIsolation: true,
    },
  });
  windows.push(control);
  await control.loadURL(url);
  control.webContents.setWebRTCIPHandlingPolicy('default');
  for (const turnUrl of [
    `turn:127.0.0.1:${sentinelPort}?transport=tcp`,
    `turn:localhost:${sentinelPort}?transport=tcp`,
    `turns:127.0.0.1:${sentinelPort}?transport=tcp`,
  ]) {
    await check(`WebRTC TCP transport denied: ${turnUrl}`, async () => {
      const beforeControl = sentinelEvents.length;
      await control.webContents.executeJavaScript(rtcProbe(turnUrl));
      const positive = sentinelEvents.slice(beforeControl);
      assert.ok(
        positive.some((event) => (turnUrl.startsWith('turns:') ? event.tls : event.turn)),
        'Positive control must actually deliver TURN/TLS bytes; silence is not a pass'
      );
      for (const socket of sockets) {
        if (socket.localPort === sentinelPort) socket.destroy();
      }
      const beforeProtected = sentinelEvents.length;
      const result = await contents.executeJavaScript(rtcProbe(turnUrl));
      assert.equal(
        sentinelEvents.length,
        beforeProtected,
        'Protected WebRTC must not reach the other TCP listener'
      );
      const routing = await contents.session.resolveProxy(`http://127.0.0.1:${sentinelPort}/`);
      assert.match(routing, /^SOCKS5 127\.0\.0\.1:/);
      return { positiveControl: positive, protected: result, routing };
    });
  }
  await check(
    'classifies TURN and prefetch DNS activity with independent positive controls',
    async () => {
      const netLogPath = path.join(output, 'synthetic-network.json');
      const suffix = Date.now();
      const controlHost = `control-${suffix}.test`;
      const protectedHost = `protected-${suffix}.test`;
      const prefetchHost = `prefetch-${suffix}.test`;
      const controlPrefetchHost = `control-prefetch-${suffix}.test`;
      const prefetch = (host: string) =>
        `(async()=>{const link=document.createElement('link');link.rel='dns-prefetch';link.href='//${host}';document.head.appendChild(link);await new Promise(resolve=>setTimeout(resolve,1000));link.remove()})()`;
      await contents.session.netLog.startLogging(netLogPath);
      try {
        await control.webContents.executeJavaScript(
          rtcProbe(`turn:${controlHost}:${sentinelPort}?transport=tcp`)
        );
        await control.webContents.executeJavaScript(prefetch(controlPrefetchHost));
        await contents.executeJavaScript(
          rtcProbe(`turn:${protectedHost}:${sentinelPort}?transport=tcp`)
        );
        await contents.executeJavaScript(prefetch(prefetchHost));
      } finally {
        await contents.session.netLog.stopLogging();
      }
      const log = JSON.parse(await readFile(netLogPath, 'utf8'));
      const types = Object.fromEntries(
        Object.entries(log.constants.logEventTypes).map(([key, value]) => [String(value), key])
      );
      type Source = { id: number; type: number };
      type NetEvent = {
        type: number;
        phase: number;
        time: string;
        source: Source;
        params?: Record<string, unknown>;
      };
      const events: NetEvent[] = log.events;
      const sourceKey = (source: Source) => `${source.type}:${source.id}`;
      const dependencies = (value: unknown): Source[] => {
        if (!value || typeof value !== 'object') return [];
        return Object.entries(value).flatMap(([key, child]) => {
          if (
            key === 'source_dependency' &&
            child &&
            typeof child === 'object' &&
            'id' in child &&
            'type' in child
          )
            return [child as Source];
          return dependencies(child);
        });
      };
      const edges = events.flatMap((event) =>
        dependencies(event.params).map((dependency) => [
          sourceKey(event.source),
          sourceKey(dependency),
        ])
      );
      const resolverEvents = (host: string) => {
        const hostPattern = new RegExp(
          `(?<![A-Za-z0-9.-])${host.replace(/\./g, '\\.')}(?![A-Za-z0-9.-])`
        );
        const sources = new Set(
          events
            .filter((event) => hostPattern.test(JSON.stringify(event.params ?? {})))
            .map((event) => sourceKey(event.source))
        );
        let expanded = true;
        while (expanded) {
          expanded = false;
          for (const [left, right] of edges) {
            if (sources.has(left) !== sources.has(right)) {
              sources.add(left);
              sources.add(right);
              expanded = true;
            }
          }
        }
        const related = events
          .filter((event) => sources.has(sourceKey(event.source)))
          .map((event) => ({ ...event, typeName: types[String(event.type)] }));
        return {
          // These are distinct evidence levels, not synonyms for a DNS leak.
          resolverRequests: related.filter((event) =>
            /HOST_RESOLVER.*REQUEST/.test(event.typeName)
          ),
          systemTasks: related.filter((event) => /HOST_RESOLVER_SYSTEM_TASK/.test(event.typeName)),
          dnsTransactions: related.filter((event) => /DNS_TRANSACTION/.test(event.typeName)),
          transmissions: related.filter((event) =>
            /(?:UDP|TCP|SOCKET)_BYTES_SENT/.test(event.typeName)
          ),
          related,
        };
      };
      const evidence = {
        control: resolverEvents(controlHost),
        controlPrefetch: resolverEvents(controlPrefetchHost),
        protected: resolverEvents(protectedHost),
        prefetch: resolverEvents(prefetchHost),
      };
      await writeFile(path.join(output, 'dns-evidence.json'), JSON.stringify(evidence, null, 2));
      assert.ok(
        evidence.control.resolverRequests.length > 0,
        'Positive control must prove NetLog observes hostname resolution'
      );
      assert.ok(
        evidence.controlPrefetch.resolverRequests.length > 0,
        'Prefetch characterization unavailable: its separate positive control produced no resolver request'
      );
      // DNS isolation is not an acceptance promise for this preview. The user
      // accepted the existing artifact DNS risk; TCP/app-route checks above
      // remain mandatory. Keep observed DNS activity explicit, never call this
      // a no-external-network sandbox or discard the positive controls.
      const characterization = Object.fromEntries(
        Object.entries(evidence).map(([name, value]) => [
          name,
          {
            resolverRequests: value.resolverRequests.length,
            systemTasks: value.systemTasks.length,
            dnsTransactionEvents: value.dnsTransactions.length,
            transmittedPackets: value.transmissions.length,
            transmittedBytes: value.transmissions.reduce(
              (sum, event) => sum + Number(event.params?.byte_count ?? 0),
              0
            ),
          },
        ])
      );
      const knownLimitation =
        'Chromium TURN hostname resolution and DNS-prefetch can bypass the app proxy. DNS confinement is not provided; this accepted limitation does not relax the TCP destination or app-route policy.';
      console.warn(`KNOWN LIMITATION: ${knownLimitation}`);
      return { knownLimitation, characterization, evidenceFile: 'dns-evidence.json' };
    }
  );
  await check('backend revocation closes the live view and its proxy listener', async () => {
    const route = await contents.session.resolveProxy(url);
    const proxyPort = Number(route.match(/:(\d+)$/)?.[1]);
    assert.ok(proxyPort > 0);
    lifetime.abort();
    await until(() => contents.isDestroyed(), 'revoked view closed');
    await assert.rejects(
      new Promise<void>((resolve, reject) => {
        const socket = net.createConnection({ host: '127.0.0.1', port: proxyPort });
        socket.once('connect', () => {
          socket.destroy();
          resolve();
        });
        socket.once('error', reject);
      })
    );
    assert.equal(navigateEmbeddedBrowser(owner, 'managed-qa', url), false);
  });
}

void run()
  .catch((error) => {
    observations.push({ check: 'probe setup', pass: false, detail: String(error) });
    console.error(error);
  })
  .finally(async () => {
    lifetime.abort();
    for (const window of windows) {
      if (!window.isDestroyed()) {
        destroyEmbeddedBrowsersForWindow(window);
        window.destroy();
      }
    }
    for (const socket of sockets) socket.destroy();
    await Promise.all(
      [daemon, sentinel].map(
        (server) => new Promise<void>((resolve) => server.close(() => resolve()))
      )
    );
    await writeFile(path.join(output, 'results.json'), JSON.stringify(observations, null, 2));
    const passed = observations.filter((item) => item.pass).length;
    const failed = observations.length - passed;
    console.log(`Managed preview probe: ${passed} passed, ${failed} failed`);
    app.exit(failed ? 1 : 0);
  });
