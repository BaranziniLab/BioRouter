// Verify a batch of custom-UI apps: serve + bundle + CUSTOM UI present + real
// WS reply. Resolves each app's id as the spec id (if served) else slug(title).
// Usage: node batch-verify.mjs <base> <batchNumber 1..5> [round2|round3]
import WebSocket from 'ws';
import { readFileSync } from 'node:fs';
const base = process.argv[2] || 'http://127.0.0.1:3000';
const batch = parseInt(process.argv[3] || '1', 10);
const round = process.argv[4] || 'round2';
const rs = readFileSync(`/Users/wanjun/Desktop/biorouter-apps-wt/scripts/agent-drafter-apps/${round}.sh`, 'utf8');
const slug = (t) => { let o = '', d = false; for (const ch of t.trim()) { if (/[a-z0-9]/i.test(ch)) { o += ch.toLowerCase(); d = false; } else if (!d && o) { o += '-'; d = true; } } return o.replace(/^-+|-+$/g, '') || 'artifact'; };
const specs = [...rs.matchAll(/^"([a-z0-9-]+)\|([^|]+)\|/gm)].map((m) => ({ spec: m[1], title: m[2] }));
const batchSpecs = specs.slice((batch - 1) * 10, batch * 10);
const CONTROL = /(br-select|br-slider|type="range"|br-switch|br-check|br-chip|br-tab|br-dropzone|br-draglist|br-mapgrid|br-region|br-grid)/;

async function served(id) {
  try { const r = await fetch(`${base}/apps/${id}/`); return r.status === 200 ? await r.text() : null; } catch { return null; }
}
function wsReply(id) {
  return new Promise((resolve) => {
    const ws = new WebSocket(base.replace(/^http/, 'ws') + `/apps/${id}/agent`); let buf = '';
    const t = setTimeout(() => { try { ws.close(); } catch {} resolve({ ok: false, err: 'timeout' }); }, 70000);
    ws.on('message', (d) => { let m; try { m = JSON.parse(d); } catch { return; }
      if (m.type === 'ready') ws.send(JSON.stringify({ type: 'prompt', text: 'Give a one-sentence demo answer.' }));
      else if (m.type === 'message') buf += m.delta;
      else if (m.type === 'error') { clearTimeout(t); ws.close(); resolve({ ok: false, err: m.message }); }
      else if (m.type === 'done') { clearTimeout(t); ws.close(); resolve({ ok: buf.trim().length > 8, err: buf ? '' : 'empty' }); } });
    ws.on('error', (e) => { clearTimeout(t); resolve({ ok: false, err: 'ws ' + e.message }); });
  });
}
let pass = 0; const rows = [];
for (const { spec, title } of batchSpecs) {
  let id = spec, html = await served(spec);
  if (!html) { id = slug(title); html = await served(id); }
  const r = { id, serve: !!html, custom: false, bundle: false, reply: false, err: '' };
  if (html) {
    r.serve = html.includes('biorouter-theme');
    const body = html.replace(/<style id="biorouter-theme">[\s\S]*?<\/style>/,'');
    r.custom = CONTROL.test(body);
    try { const b = await fetch(`${base}/apps/${id}/dist/app.js`); r.bundle = b.status === 200 && (await b.text()).length > 500; } catch (e) { r.err = 'bundle ' + e.message; }
  } else { r.err = 'not served (' + spec + '/' + slug(title) + ')'; }
  if (r.serve && r.bundle) { const w = await wsReply(id); r.reply = w.ok; if (!w.ok) r.err = 'reply: ' + w.err; }
  const ok = r.serve && r.bundle && r.custom && r.reply;
  if (ok) pass++;
  rows.push({ ok, ...r });
}
console.log(`${round} BATCH ${batch}: ${pass}/${batchSpecs.length} ok`);
for (const r of rows) if (!r.ok) console.log(`  FAIL ${r.id}: serve=${r.serve} bundle=${r.bundle} custom=${r.custom} reply=${r.reply} ${r.err}`);
console.log('PASS:', rows.filter((r) => r.ok).map((r) => r.id).join(' '));
