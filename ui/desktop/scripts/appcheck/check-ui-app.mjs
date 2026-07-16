// Verify one Biorouter app's **agent-driven UI**: does the agent actually change
// the page, not just talk into it?
//
//   node check-ui-app.mjs <base> <id> "<prompt>" [--expect panel,chart,highlight]
//
// Speaks the app WebSocket protocol directly (no browser), so it can assert on
// the `ui` command frames the agent's `ui_*` tools emit. Complements
// check-app.mjs, which only checks that a reply streams back.
import WebSocket from 'ws';

const [, , base, id, prompt = 'Summarize this app and show me a chart.'] = process.argv;
const expectArg = process.argv.find((a) => a.startsWith('--expect='));
const expected = expectArg ? expectArg.slice('--expect='.length).split(',').filter(Boolean) : [];
const TIMEOUT_MS = Number(process.env.UI_CHECK_TIMEOUT_MS || 180000);

if (!base || !id) {
  console.error('usage: node check-ui-app.mjs <base> <id> "<prompt>" [--expect=panel,chart]');
  process.exit(2);
}

const res = {
  id,
  httpIndex: 0,
  httpBundle: 0,
  bundleBytes: 0,
  uiCapability: false,
  declaresRegions: [],
  uiCommands: [],
  uiCmdKinds: [],
  tools: [],
  reply: '',
  error: '',
  missing: [],
  ok: false,
};

try {
  const idx = await fetch(`${base}/apps/${id}/`);
  res.httpIndex = idx.status;
  const html = await idx.text();
  // The regions the author exposed for `ui_render(target="@region:…")`.
  res.declaresRegions = [...html.matchAll(/data-br-region=["']([^"']+)["']/g)].map((m) => m[1]);
  const b = await fetch(`${base}/apps/${id}/dist/app.js`);
  res.httpBundle = b.status;
  res.bundleBytes = (await b.text()).length;
} catch (e) {
  res.error = 'http: ' + e.message;
}

const wsUrl = base.replace(/^http/, 'ws') + `/apps/${id}/agent`;
await new Promise((resolve) => {
  let settled = false;
  const ws = new WebSocket(wsUrl);
  const finish = (why) => {
    if (settled) return;
    settled = true;
    if (why) res.error ||= why;
    clearTimeout(timer);
    try { ws.close(); } catch { /* already closing */ }
    resolve();
  };
  const timer = setTimeout(() => finish('timeout'), TIMEOUT_MS);

  ws.on('message', (data) => {
    let m;
    try { m = JSON.parse(data); } catch { return; }
    switch (m.type) {
      case 'ready':
        res.uiCapability = Array.isArray(m.capabilities) && m.capabilities.includes('ui');
        // Tell the agent what this page offers, exactly as the SDK would, so
        // `ui_describe` returns real regions and `@region:…` targets resolve.
        ws.send(JSON.stringify({
          type: 'ui_surface',
          surface: { title: id, regions: res.declaresRegions, ids: [], hasChat: true, panels: [] },
        }));
        ws.send(JSON.stringify({ type: 'prompt', text: prompt }));
        break;
      case 'ui':
        res.uiCommands.push(m);
        res.uiCmdKinds.push(m.cmd);
        // A `ui_ask` parks the agent's tool call until we answer. Auto-answer
        // with the field defaults so the turn can finish.
        if (m.cmd === 'ask') {
          const payload = {};
          for (const f of m.fields || []) {
            payload[f.name] = f.value ?? (f.type === 'checkbox' ? false : (f.options?.[0] ?? 'auto'));
          }
          ws.send(JSON.stringify({ type: 'ui_reply', requestId: m.requestId, payload }));
        }
        break;
      case 'message': res.reply += m.delta; break;
      case 'tool': res.tools.push(`${m.name}:${m.status}`); break;
      case 'error': finish(m.message); break;
      case 'done': finish(); break;
      default: break;
    }
  });
  ws.on('error', (e) => finish(e.message));
  ws.on('close', () => finish('closed before done'));
});

// `chart`/`graph` arrive as a `panel` whose body holds that node (or a `render`).
const bodyKinds = new Set();
for (const c of res.uiCommands) {
  for (const node of c.body || []) if (node && node.t) bodyKinds.add(node.t);
}
const satisfied = new Set([...res.uiCmdKinds, ...bodyKinds]);
res.missing = expected.filter((e) => !satisfied.has(e));

res.ok =
  res.httpIndex === 200 &&
  res.httpBundle === 200 &&
  res.bundleBytes > 500 &&
  res.uiCapability &&
  !res.error &&
  res.uiCommands.length > 0 &&
  res.missing.length === 0;

const brief = {
  ...res,
  uiCommands: res.uiCommands.length,
  reply: res.reply.slice(0, 160),
};
console.log(JSON.stringify(brief, null, 2));
process.exit(res.ok ? 0 : 1);
