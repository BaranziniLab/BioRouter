// Verify one Biorouter app end-to-end: HTTP serve + esbuild bundle + a real
// streamed agent reply. Usage: node check-app.mjs <base> <id> "<prompt>"
import WebSocket from 'ws';
const base = process.argv[2], id = process.argv[3], prompt = process.argv[4] || 'Briefly, what can you do?';
const res = { id, httpIndex: 0, httpBundle: 0, bundleBytes: 0, theme: false, wsReply: '', wsError: '', tools: [], ok: false };
try {
  const idx = await fetch(`${base}/apps/${id}/`); res.httpIndex = idx.status;
  res.theme = (await idx.text()).includes('biorouter-theme');
  const b = await fetch(`${base}/apps/${id}/dist/app.js`); res.httpBundle = b.status;
  res.bundleBytes = (await b.text()).length;
} catch (e) { res.wsError = 'http: ' + e.message; }
const wsUrl = base.replace(/^http/, 'ws') + `/apps/${id}/agent`;
await new Promise((resolve) => {
  let done = false; const ws = new WebSocket(wsUrl);
  const t = setTimeout(() => { if (!done) { res.wsError ||= 'timeout'; ws.close(); resolve(); } }, 90000);
  ws.on('message', (d) => { let m; try { m = JSON.parse(d); } catch { return; }
    if (m.type === 'ready') ws.send(JSON.stringify({ type: 'prompt', text: prompt }));
    else if (m.type === 'message') res.wsReply += m.delta;
    else if (m.type === 'tool') res.tools.push(`${m.name}:${m.status}`);
    else if (m.type === 'error') { res.wsError = m.message; done = true; clearTimeout(t); ws.close(); resolve(); }
    else if (m.type === 'done') { done = true; clearTimeout(t); ws.close(); resolve(); } });
  ws.on('error', (e) => { res.wsError ||= e.message; done = true; clearTimeout(t); resolve(); });
});
res.ok = res.httpIndex === 200 && res.httpBundle === 200 && res.bundleBytes > 500 && res.theme && !res.wsError && res.wsReply.trim().length > 10;
console.log(JSON.stringify(res));
