// Static verification of built stress-test apps against a running sandbox daemon.
//   node verify.mjs <base> <id1> [id2 ...]        (or --all to read the store)
// Checks: serves 200, bundle has the UI runtime, manifest is agentic + ui-enabled,
// system prompt directs the agent to ui_* tools, regions declared, agent socket
// advertises `ui`. Prints one JSON line per app.
import { readFile, readdir } from "node:fs/promises";
import { join } from "node:path";
import { homedir } from "node:os";

const base = (process.argv[2] || "http://127.0.0.1:3900").replace(/\/$/, "");
const STORE = join(process.env.XDG_CONFIG_HOME || join(homedir(), ".config"), "biorouter", "agent_drafter");
let ids = process.argv.slice(3);
if (ids[0] === "--all") {
  ids = (await readdir(STORE, { withFileTypes: true })).filter((d) => d.isDirectory()).map((d) => d.name).sort();
}

const UI_TOOLS = ["ui_panel","ui_render","ui_chart","ui_graph","ui_highlight","ui_theme","ui_layout","ui_notify","ui_state","ui_ask"];

function ready(id, ms = 8000) {
  return new Promise((res) => {
    const url = base.replace(/^http/, "ws") + `/apps/${id}/agent?client_id=verify`;
    let ws;
    try { ws = new WebSocket(url); } catch (e) { return res({ error: String(e.message || e) }); }
    const t = setTimeout(() => { try { ws.close(); } catch {} res({ error: "ws timeout" }); }, ms);
    ws.onerror = () => { clearTimeout(t); res({ error: "ws error" }); };
    ws.onmessage = (ev) => {
      let m; try { m = JSON.parse(ev.data); } catch { return; }
      if (m.type !== "ready") return;
      clearTimeout(t); try { ws.close(); } catch {}
      res({ ready: m });
    };
  });
}

for (const id of ids) {
  const fails = [];
  let manifest = {};
  try { manifest = JSON.parse(await readFile(join(STORE, id, "manifest.json"), "utf8")); }
  catch (e) { console.log(JSON.stringify({ id, ok: false, fails: ["no manifest"] })); continue; }

  const sp = (manifest.agent?.system_prompt || "").toLowerCase();
  const named = UI_TOOLS.filter((t) => sp.includes(t));
  if (manifest.kind !== "agentic") fails.push("not agentic");
  if (manifest.agent?.capabilities?.ui?.enabled === false) fails.push("ui disabled");
  if (!named.length) fails.push("prompt names no ui_* tool");

  const idx = await fetch(`${base}/apps/${id}/`).catch(() => ({ ok: false, status: 0 }));
  if (!idx.ok) fails.push(`index ${idx.status}`);
  const html = idx.ok ? await idx.text() : "";
  if (idx.ok && !html.includes('id="biorouter-theme"')) fails.push("no theme");
  const regions = [...html.matchAll(/data-br-region="([^"]+)"/g)].map((m) => m[1]);

  const js = await fetch(`${base}/apps/${id}/dist/app.js`).then((r) => r.ok ? r.text() : "").catch(() => "");
  if (js.length < 500) fails.push(`bundle ${js.length}b`);
  if (!(js.includes("br-dock") && js.includes("ui_reply"))) fails.push("bundle missing UI runtime");

  const { ready: rf, error } = await ready(id);
  if (error) fails.push(`socket ${error}`);
  else if (!(rf.capabilities || []).includes("ui")) fails.push("no ui capability advertised");

  console.log(JSON.stringify({ id, ok: fails.length === 0, fails, regions, promptTools: named,
    bundleBytes: js.length, model: manifest.agent?.model || null }));
}
