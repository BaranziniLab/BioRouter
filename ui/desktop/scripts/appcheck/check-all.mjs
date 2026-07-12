// check-all: live health check of every BioRouter app in the local store.
// Store-driven + repo-relative — enumerates app ids from the store dir directly
// (no external round.sh scripts). For each app it verifies the daemon serves the
// index + bundle, that the served page carries the injected biorouter-theme, and
// that the per-app agent socket produces a real reply.
//
// Store dir: $BIOROUTER_APPS_DIR (default ~/.config/biorouter/agent_drafter).
// Base URL:  argv[2] (default http://127.0.0.1:3000).
//
// Empty/missing store       -> clean exit 0 with a "no apps installed" note.
// Daemon unreachable          -> exit 0 with a note (can't run live checks; CI-safe).
// Daemon reachable + failures -> exit 1 (usable as a gate).
// Unexpected crash            -> exit 1.
import WebSocket from 'ws';
import { readdirSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { homedir } from 'node:os';

const base = process.argv[2] || 'http://127.0.0.1:3000';
const STORE =
  process.env.BIOROUTER_APPS_DIR ||
  join(homedir(), '.config', 'biorouter', 'agent_drafter');

function listApps(dir) {
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return null;
  }
  return entries
    .filter((e) => e.isDirectory() && !e.name.startsWith('.'))
    .map((e) => e.name)
    .filter((name) => existsSync(join(dir, name, 'manifest.json')))
    .sort();
}

async function reachable(url) {
  try {
    // Any HTTP response (even 404) means the daemon is up.
    await fetch(url, { method: 'GET' });
    return true;
  } catch {
    return false;
  }
}

function check(id) {
  return new Promise(async (resolve) => {
    const r = { id, ok: false, err: '' };
    try {
      const idx = await fetch(`${base}/apps/${id}/`);
      const html = await idx.text();
      const b = await fetch(`${base}/apps/${id}/dist/app.js`);
      const bundle = await b.text();
      if (idx.status !== 200 || b.status !== 200 || bundle.length < 500 || !html.includes('biorouter-theme')) {
        throw new Error(`http idx=${idx.status} bundle=${b.status}`);
      }
    } catch (e) {
      r.err = 'http: ' + e.message;
      return resolve(r);
    }
    const ws = new WebSocket(base.replace(/^http/, 'ws') + `/apps/${id}/agent`);
    let reply = '';
    const t = setTimeout(() => {
      r.err = 'timeout';
      try {
        ws.close();
      } catch {}
      resolve(r);
    }, 60000);
    ws.on('message', (d) => {
      let m;
      try {
        m = JSON.parse(d);
      } catch {
        return;
      }
      if (m.type === 'ready') ws.send(JSON.stringify({ type: 'prompt', text: 'In one sentence, what do you do?' }));
      else if (m.type === 'message') reply += m.delta;
      else if (m.type === 'error') {
        clearTimeout(t);
        r.err = m.message;
        ws.close();
        resolve(r);
      } else if (m.type === 'done') {
        clearTimeout(t);
        r.ok = reply.trim().length > 10;
        if (!r.ok) r.err = 'empty reply';
        ws.close();
        resolve(r);
      }
    });
    ws.on('error', (e) => {
      clearTimeout(t);
      r.err = 'ws: ' + e.message;
      resolve(r);
    });
  });
}

async function main() {
  const ids = listApps(STORE);
  if (ids === null || ids.length === 0) {
    const note =
      ids === null
        ? `no apps installed: store dir not found (${STORE})`
        : `no apps installed: store dir is empty (${STORE})`;
    console.log(`CHECKLIST: ${note}`);
    process.exit(0);
  }

  if (!(await reachable(base))) {
    console.log(`CHECKLIST: daemon not reachable at ${base}; skipping live checks of ${ids.length} app(s).`);
    console.log('  Start biorouterd (e.g. `just run-server`) and re-run to health-check them.');
    process.exit(0);
  }

  let pass = 0;
  const fails = [];
  for (const id of ids) {
    const r = await check(id);
    if (r.ok) pass++;
    else fails.push(r);
  }
  console.log(`CHECKLIST: ${pass}/${ids.length} ok`);
  for (const f of fails) console.log(`  FAIL ${f.id}: ${f.err}`);
  process.exit(fails.length ? 1 : 0);
}

main().catch((e) => {
  console.error('check-all crashed:', e && e.stack ? e.stack : e);
  process.exit(1);
});
