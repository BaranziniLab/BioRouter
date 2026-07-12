/**
 * Deterministic harness for the agent-driven UI runtime.
 *
 * Serves a built Agent Drafter app and stands in for `biorouterd`'s per-app
 * agent socket, speaking the real wire protocol. That lets us exercise every
 * `ui` command (and the `ui_ask` round-trip) against the real `sdk.ts` in a real
 * browser, with none of an LLM's nondeterminism.
 *
 *   node scripts/agent-drafter/ui-control-harness.mjs --app <dir> [--port 8899]
 *
 * The page drives it with `window.__harness.send({cmd:"panel", ...})`, which
 * POSTs to /__emit; the harness relays it down the socket as a `ui` frame.
 * Anything the browser sends back (`ui_reply`, `ui_surface`, `widget_action`) is
 * recorded and readable at GET /__frames.
 *
 * Zero dependencies: the WebSocket server is ~60 lines of RFC 6455.
 */
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { extname, resolve, normalize, join, dirname } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const argOf = (name, dflt) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : dflt;
};
const APP = resolve(argOf("--app", "."));
const PORT = Number(argOf("--port", 8899));
// With no `--app`, the harness runs its own SDK v2 scenarios (below): it bundles
// the real sdk.ts, mounts it in jsdom against this mock daemon, drives the state
// channel + bindings, and asserts. With `--app`, it stays a plain server a real
// browser drives via /__emit (the original behavior).
const SELFTEST = args.indexOf("--app") < 0;
/** The upgrade request URL of the most recent socket (for the wsToken check). */
let lastUpgradeUrl = null;

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json",
};

/** Frames the browser sent us, in order. */
const received = [];
/** The live client socket, if any. */
let client = null;

// ── RFC 6455: just enough to send/receive unfragmented text frames ──────────

const GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

function encodeFrame(text) {
  const payload = Buffer.from(text, "utf8");
  const len = payload.length;
  let header;
  if (len < 126) {
    header = Buffer.alloc(2);
    header[1] = len;
  } else if (len < 65536) {
    header = Buffer.alloc(4);
    header[1] = 126;
    header.writeUInt16BE(len, 2);
  } else {
    header = Buffer.alloc(10);
    header[1] = 127;
    header.writeBigUInt64BE(BigInt(len), 2);
  }
  header[0] = 0x81; // FIN + text
  return Buffer.concat([header, payload]);
}

/** Pull every complete text frame out of `buf`; returns [texts, rest]. */
function decodeFrames(buf) {
  const out = [];
  let i = 0;
  while (i + 2 <= buf.length) {
    const opcode = buf[i] & 0x0f;
    const masked = (buf[i + 1] & 0x80) !== 0;
    let len = buf[i + 1] & 0x7f;
    let off = i + 2;
    if (len === 126) {
      if (off + 2 > buf.length) break;
      len = buf.readUInt16BE(off);
      off += 2;
    } else if (len === 127) {
      if (off + 8 > buf.length) break;
      len = Number(buf.readBigUInt64BE(off));
      off += 8;
    }
    let mask = null;
    if (masked) {
      if (off + 4 > buf.length) break;
      mask = buf.subarray(off, off + 4);
      off += 4;
    }
    if (off + len > buf.length) break;
    const payload = Buffer.from(buf.subarray(off, off + len));
    if (mask) for (let k = 0; k < payload.length; k++) payload[k] ^= mask[k % 4];
    if (opcode === 0x1) out.push(payload.toString("utf8"));
    if (opcode === 0x8) out.push(null); // close
    i = off + len;
  }
  return [out, buf.subarray(i)];
}

const send = (obj) => client && client.write(encodeFrame(JSON.stringify(obj)));

// ── HTTP: static files + the harness control plane ──────────────────────────

const server = createServer(async (req, res) => {
  const url = (req.url || "/").split("?")[0];

  if (url === "/__frames") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify(received));
    return;
  }
  if (url === "/__emit" && req.method === "POST") {
    let body = "";
    for await (const chunk of req) body += chunk;
    const cmd = JSON.parse(body);
    send({ type: "ui", ...cmd });
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ ok: !!client }));
    return;
  }
  if (url === "/__reset" && req.method === "POST") {
    received.length = 0;
    res.writeHead(200);
    res.end("ok");
    return;
  }
  // The SDK's model surface; keeps `listModels()` from erroring in the console.
  if (url.endsWith("/models")) {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ providers: [] }));
    return;
  }

  let path = decodeURIComponent(url);
  if (path === "/") path = "/index.html";
  const file = resolve(APP, "." + normalize(path));
  if (file !== APP && !file.startsWith(APP + "/")) {
    res.writeHead(403);
    res.end("Forbidden");
    return;
  }
  try {
    const body = await readFile(file);
    res.writeHead(200, { "Content-Type": MIME[extname(file)] || "application/octet-stream" });
    res.end(body);
  } catch {
    res.writeHead(404);
    res.end("Not found");
  }
});

// ── The per-app agent socket ────────────────────────────────────────────────

server.on("upgrade", (req, socket) => {
  lastUpgradeUrl = req.url || null;
  const key = req.headers["sec-websocket-key"];
  const accept = createHash("sha1").update(key + GUID).digest("base64");
  socket.write(
    "HTTP/1.1 101 Switching Protocols\r\n" +
      "Upgrade: websocket\r\nConnection: Upgrade\r\n" +
      `Sec-WebSocket-Accept: ${accept}\r\n\r\n`
  );
  client = socket;

  let buf = Buffer.alloc(0);
  socket.on("data", (chunk) => {
    buf = Buffer.concat([buf, chunk]);
    const [frames, rest] = decodeFrames(buf);
    buf = rest;
    for (const text of frames) {
      if (text === null) {
        socket.destroy();
        return;
      }
      try {
        received.push(JSON.parse(text));
      } catch {
        received.push({ raw: text });
      }
    }
  });
  socket.on("close", () => {
    if (client === socket) client = null;
  });
  socket.on("error", () => {});

  // Advertise `ui` so `sdk.ts` mounts the runtime and reports its surface.
  send({
    type: "ready",
    protocol: 2,
    capabilities: ["ui"],
    sessionId: "harness",
    resumed: false,
    messageCount: 0,
  });
});

server.listen(PORT, "127.0.0.1", () => {
  if (SELFTEST) {
    runSelftest()
      .then((code) => process.exit(code))
      .catch((e) => {
        console.error("[ui-harness] ERROR", e && e.stack ? e.stack : e);
        process.exit(2);
      });
    return;
  }
  console.log(`ui-control harness: app=${APP} on http://127.0.0.1:${PORT}`);
});

// ── SDK v2 self-test: real sdk.ts in jsdom against this mock daemon ──────────
// Bundles the shipped sdk.ts (via the real app entry `main.ts`, exactly like
// `build_app`), mounts it in jsdom with Node's global WebSocket pointed at this
// server, then drives the shared-state channel, declarative bindings, frame
// tolerance, and wsToken — asserting on the DOM and on the frames the SDK sends
// back. Exits non-zero on any failure, matching the sibling test kits.

const log = (...a) => console.log("[ui-harness]", ...a);
let failures = 0;
function check(name, cond, detail) {
  if (cond) log(`PASS — ${name}`);
  else {
    failures++;
    console.error(`[ui-harness] FAIL — ${name}${detail ? ": " + detail : ""}`);
  }
}
const delay = (ms) => new Promise((r) => setTimeout(r, ms));
async function waitFor(pred, ms = 4000, step = 15) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    let ok = false;
    try {
      ok = pred();
    } catch {
      ok = false;
    }
    if (ok) return true;
    await delay(step);
  }
  return false;
}

function firstExisting(paths) {
  for (const p of paths) if (p && existsSync(p)) return p;
  return null;
}

/** Emit a server→client `ui` frame down the socket (as `control.rs::emit` does,
 *  now stamped with `"v": 1`). */
function emitUi(frame) {
  send({ type: "ui", v: 1, ...frame });
}

async function runSelftest() {
  const WS_TOKEN = "tok-abc123";
  const APP_ID = "harness";

  // 1) Resolve the real esbuild + jsdom (worktrees have no node_modules, so a
  //    full checkout or the BIOROUTER_ESBUILD_BIN / BIOROUTER_JSDOM_DIR env
  //    vars point us at one).
  const esbuild =
    process.env.BIOROUTER_ESBUILD_BIN ||
    firstExisting([join(HERE, "../../ui/desktop/node_modules/.bin/esbuild")]);
  const jsdomDir =
    process.env.BIOROUTER_JSDOM_DIR ||
    firstExisting([join(HERE, "../../ui/desktop/node_modules/jsdom")]);
  if (!esbuild || !jsdomDir) {
    console.error(
      "[ui-harness] need esbuild + jsdom. Set BIOROUTER_ESBUILD_BIN and " +
        "BIOROUTER_JSDOM_DIR, or run from a checkout with ui/desktop/node_modules."
    );
    return 2;
  }

  // 2) Bundle the shipped SDK through the real app entry (main.ts imports ./sdk).
  const mainTs = join(
    HERE,
    "../../crates/biorouter-mcp/src/agent_drafter/templates/main.ts"
  );
  let bundle;
  try {
    bundle = execFileSync(
      esbuild,
      [mainTs, "--bundle", "--format=iife", "--target=es2018", "--loader:.ts=ts", "--log-level=warning"],
      { encoding: "utf8", maxBuffer: 32 * 1024 * 1024 }
    );
  } catch (e) {
    console.error("[ui-harness] esbuild failed:", e.stderr || e.message);
    return 2;
  }

  // 3) Mount the bundle in jsdom with a real WebSocket to this daemon.
  const { JSDOM } = await import(pathToFileURL(join(jsdomDir, "lib/api.js")).href);
  const html =
    "<!doctype html><html><head><title>Harness</title></head><body>" +
    '<section data-br-region="results"></section>' +
    '<span id="cnt" data-br-bind="/cohort/count">-</span>' +
    '<a id="lnk" data-br-bind-attr="href:/link">link</a>' +
    '<a id="jsl" data-br-bind-attr="href:/danger">danger</a>' +
    '<button id="clk" data-br-bind-attr="onclick:/handler">btn</button>' +
    '<div id="box" data-br-bind-show="/visible">boxed</div>' +
    "</body></html>";
  const dom = new JSDOM(html, {
    url: `http://127.0.0.1:${PORT}/`,
    runScripts: "outside-only",
    pretendToBeVisual: true,
  });
  const win = dom.window;
  const warnLog = [];
  win.WebSocket = globalThis.WebSocket;
  win.console = {
    log: () => {},
    info: () => {},
    debug: () => {},
    error: () => {},
    warn: (...a) => warnLog.push(a.map(String).join(" ")),
  };
  win.BIOROUTER_APP_CONFIG = {
    appId: APP_ID,
    autoChat: false,
    ui: true,
    wsToken: WS_TOKEN,
  };
  // jsdom has no real 2D canvas. Give getContext a no-op stub so the `network`
  // engine's draw path actually runs (proving it never crashes) without needing
  // a real context — physics, selection and positions work regardless.
  win.HTMLCanvasElement.prototype.getContext = function () {
    return {
      save() {},
      restore() {},
      beginPath() {},
      moveTo() {},
      lineTo() {},
      arc() {},
      fill() {},
      stroke() {},
      clearRect() {},
      setTransform() {},
      setLineDash() {},
      fillText() {},
      measureText() {
        return { width: 10 };
      },
      createRadialGradient() {
        return { addColorStop() {} };
      },
    };
  };
  win.eval(bundle);

  const doc = win.document;
  const $ = (id) => doc.getElementById(id);

  // Wait until the SDK connected, processed `ready`, and reported its surface
  // (which also indexes the author bindings).
  const up = await waitFor(() => received.some((f) => f.type === "ui_surface"));
  check("SDK connects and reports its surface", up, "no ui_surface frame arrived");
  if (!up) {
    dom.window.close();
    return 1;
  }

  // ── Scenario D (checked early): wsToken rides the WS URL ──────────────────
  check(
    "WS upgrade URL carries token= when config.wsToken is set",
    !!lastUpgradeUrl &&
      lastUpgradeUrl.includes("token=" + WS_TOKEN) &&
      lastUpgradeUrl.includes("client_id="),
    `upgrade url: ${lastUpgradeUrl}`
  );

  // ── Scenario A: state snapshot → patch drives bindings; unsafe sinks refused ─
  emitUi({
    cmd: "state",
    mode: "snapshot",
    version: 3,
    doc: {
      cohort: { count: 7 },
      link: "https://example.org/a",
      danger: "javascript:alert(1)",
      handler: "steal()",
      visible: true,
    },
  });
  await waitFor(() => $("cnt") && $("cnt").textContent === "7");
  check("snapshot: [data-br-bind] span textContent updates", $("cnt").textContent === "7", $("cnt").textContent);
  check(
    "snapshot: [data-br-bind-attr] href updates",
    $("lnk").getAttribute("href") === "https://example.org/a",
    $("lnk").getAttribute("href")
  );
  check(
    "javascript: href is refused (never set)",
    !/javascript:/i.test($("jsl").getAttribute("href") || ""),
    "href=" + $("jsl").getAttribute("href")
  );
  check(
    "unsafe href refusal is logged",
    warnLog.some((w) => /unsafe href/i.test(w)),
    warnLog.join(" | ")
  );
  check(
    "onclick bind-attr is refused (never set)",
    $("clk").getAttribute("onclick") === null,
    "onclick=" + $("clk").getAttribute("onclick")
  );
  check(
    "forbidden on* attr refusal is logged",
    warnLog.some((w) => /forbidden attribute: onclick/i.test(w)),
    warnLog.join(" | ")
  );
  check("bind-show reveals element when truthy", $("box").hidden === false, "hidden=" + $("box").hidden);

  emitUi({
    cmd: "state",
    mode: "patch",
    version: 4,
    patch: [
      { op: "replace", path: "/cohort/count", value: 12 },
      { op: "replace", path: "/link", value: "/rel/path" },
      { op: "replace", path: "/visible", value: false },
    ],
  });
  await waitFor(() => $("cnt").textContent === "12");
  check("patch: bound span updates to new value", $("cnt").textContent === "12", $("cnt").textContent);
  check("patch: bound href updates (relative allowed)", $("lnk").getAttribute("href") === "/rel/path", $("lnk").getAttribute("href"));
  check("patch: bind-show hides element when falsy", $("box").hidden === true, "hidden=" + $("box").hidden);

  // ── Scenario B: br.state.set → state_write(baseVersion); echo patch → subscribe ─
  win.eval(
    "window.__xSub = []; window.__unsub = window.BioRouter.state.subscribe('/x', function (v) { window.__xSub.push(v); });"
  );
  const beforeSet = received.length;
  win.eval("window.BioRouter.state.set('/x', 5);");
  const gotWrite = await waitFor(() =>
    received.slice(beforeSet).some((f) => f.type === "state_write")
  );
  check("state.set emits a state_write frame", gotWrite, "no state_write frame");
  const sw = received.slice(beforeSet).find((f) => f.type === "state_write");
  check(
    "state_write carries set{path,value} and numeric baseVersion",
    !!sw && sw.set && sw.set.path === "/x" && sw.set.value === 5 && typeof sw.baseVersion === "number",
    JSON.stringify(sw)
  );
  check(
    "state_write.baseVersion is the pre-write version (4)",
    !!sw && sw.baseVersion === 4,
    "baseVersion=" + (sw && sw.baseVersion)
  );
  check(
    "optimistic local apply fires the subscriber",
    win.__xSub.length >= 1 && win.__xSub[win.__xSub.length - 1] === 5,
    JSON.stringify(win.__xSub)
  );
  // Server echoes an authoritative patch (correcting /x to 9, bumping version).
  emitUi({ cmd: "state", mode: "patch", version: 5, patch: [{ op: "replace", path: "/x", value: 9 }] });
  await waitFor(() => win.__xSub.some((v) => v === 9));
  check("echoed patch fires subscribe('/x')", win.__xSub.some((v) => v === 9), JSON.stringify(win.__xSub));
  check("state.get('/x') reflects the echoed value", win.eval("window.BioRouter.state.get('/x')") === 9, "get=" + win.eval("window.BioRouter.state.get('/x')"));
  // Unsubscribe stops further notifications.
  const subCountBefore = win.__xSub.length;
  win.eval("window.__unsub();");
  emitUi({ cmd: "state", mode: "patch", version: 6, patch: [{ op: "replace", path: "/x", value: 11 }] });
  await delay(150);
  check("unsubscribe stops notifications", win.__xSub.length === subCountBefore, `before=${subCountBefore} after=${win.__xSub.length}`);

  // ── Scenario C: unknown cmd + unknown widget kind → no crash, placeholder ──
  const countBefore = $("cnt").textContent;
  emitUi({ cmd: "totally_unknown_cmd", foo: 1 });
  await delay(80);
  check("unknown cmd is a silent no-op (client still alive)", !!win.BioRouter && $("cnt").textContent === countBefore, "cnt=" + $("cnt").textContent);

  emitUi({ cmd: "render", target: "@region:results", body: [{ t: "quantum_widget", label: "x" }] });
  await waitFor(() => doc.querySelector("#results-probe, [data-br-region='results'] .br-unknown-widget"));
  const ph = doc.querySelector("[data-br-region='results'] .br-unknown-widget");
  check("unknown widget kind renders a neutral placeholder", !!ph, "no .br-unknown-widget");
  check(
    "placeholder names the unsupported kind",
    !!ph && ph.textContent === "[unsupported: quantum_widget]",
    ph && ph.textContent
  );
  check(
    "unknown widget kind is warned once",
    warnLog.filter((w) => /unsupported widget kind: quantum_widget/i.test(w)).length === 1,
    "warns=" + warnLog.filter((w) => /quantum_widget/i.test(w)).length
  );
  // Rendering the same unknown kind again must NOT warn again (once per kind).
  emitUi({ cmd: "render", target: "@region:results", body: [{ t: "quantum_widget", label: "y" }] });
  await delay(80);
  check(
    "re-rendering the same unknown kind does not re-warn",
    warnLog.filter((w) => /unsupported widget kind: quantum_widget/i.test(w)).length === 1,
    "warns=" + warnLog.filter((w) => /unsupported widget kind: quantum_widget/i.test(w)).length
  );

  // ── Scenario E: legacy `{cmd:"state", state:{…}}` (no mode) → snapshot ─────
  // An old server that predates the mode field must keep working: the whole
  // `state` object replaces the doc and re-paints bindings.
  emitUi({ cmd: "state", state: { cohort: { count: 99 }, link: "https://legacy.example/z", visible: true } });
  await waitFor(() => $("cnt").textContent === "99");
  check("legacy state frame (no mode) is treated as a snapshot", $("cnt").textContent === "99", $("cnt").textContent);
  check("legacy snapshot re-evaluates all bindings", $("lnk").getAttribute("href") === "https://legacy.example/z", $("lnk").getAttribute("href"));

  // ── Scenario F: ui_patch — instance registry, morph, focus preservation ────
  emitUi({
    cmd: "render",
    target: "@region:results",
    body: [
      { t: "kpi", id: "k1", label: "Count", value: 10, delta: "+2", unit: "n" },
      { t: "form", id: "f1", children: [{ t: "input", name: "q", label: "Query" }] },
    ],
  });
  await waitFor(() => doc.querySelector("[data-br-iid='k1']") && doc.querySelector("[data-br-iid='f1']"));
  check(
    "ui_render tags data-br-iid and registers instances",
    !!doc.querySelector("[data-br-iid='k1']") && !!doc.querySelector("[data-br-iid='f1']")
  );
  {
    const d = doc.querySelector("[data-br-iid='k1'] .br-kpi__delta");
    check("kpi renders a delta with an up/down arrow", !!d && /▲|▼/.test(d.textContent || ""), d && d.textContent);
  }

  // Focus a sibling input, patch a different instance; the focus must survive.
  const qInput = doc.querySelector("[data-br-iid='f1'] input");
  if (qInput && typeof qInput.focus === "function") qInput.focus();
  check("sibling input can take focus", doc.activeElement === qInput);
  emitUi({ cmd: "patch", ops: [{ op: "set_props", id: "k1", props: { value: 99, delta: "-5" } }] });
  await waitFor(() => {
    const v = doc.querySelector("[data-br-iid='k1'] .br-kpi__value");
    return v && (v.textContent || "").indexOf("99") >= 0;
  });
  check(
    "set_props shallow-merges + re-renders the targeted instance",
    (doc.querySelector("[data-br-iid='k1'] .br-kpi__value").textContent || "").indexOf("99") >= 0
  );
  check("set_props on a sibling preserves focus", doc.activeElement === qInput);
  check("set_props delta now reads as a decrease (▼)", /▼/.test(doc.querySelector("[data-br-iid='k1'] .br-kpi__delta").textContent || ""));

  // replace re-renders the instance in place (same id).
  emitUi({ cmd: "patch", ops: [{ op: "replace", id: "k1", node: { t: "kpi", label: "Total", value: 7 } }] });
  await waitFor(() => {
    const l = doc.querySelector("[data-br-iid='k1'] .br-kpi__label");
    return l && l.textContent === "Total";
  });
  check("replace re-renders an instance in place", doc.querySelector("[data-br-iid='k1'] .br-kpi__label").textContent === "Total");

  // add + remove round-trip.
  emitUi({ cmd: "patch", ops: [{ op: "add", id: "added1", target: "@region:results", node: { t: "text", value: "hello-added" } }] });
  await waitFor(() => doc.querySelector("[data-br-iid='added1']"));
  check("add inserts a new instance at the target", (doc.querySelector("[data-br-iid='added1']").textContent || "").indexOf("hello-added") >= 0);
  emitUi({ cmd: "patch", ops: [{ op: "remove", id: "added1" }] });
  await waitFor(() => !doc.querySelector("[data-br-iid='added1']"));
  check("remove deletes the instance + registry entry", !doc.querySelector("[data-br-iid='added1']"));

  // ── Scenario G: log append + cap (drop oldest) ─────────────────────────────
  emitUi({
    cmd: "render",
    target: "@region:results",
    body: [{ t: "log", id: "log1", max: 3, lines: [{ text: "l1" }, { text: "l2" }] }],
  });
  await waitFor(() => doc.querySelectorAll("[data-br-iid='log1'] .br-log__line").length === 2);
  check("log renders its initial lines", doc.querySelectorAll("[data-br-iid='log1'] .br-log__line").length === 2);
  emitUi({
    cmd: "patch",
    ops: [{ op: "set_props", id: "log1", props: { append: [{ text: "l3" }, { text: "l4" }, { text: "l5" }] } }],
  });
  await waitFor(() => doc.querySelectorAll("[data-br-iid='log1'] .br-log__line").length === 3);
  {
    const rows = Array.prototype.map.call(doc.querySelectorAll("[data-br-iid='log1'] .br-log__line"), (e) => e.textContent);
    check("log append caps at node.max, dropping the oldest", rows.length === 3 && rows[2] === "l5" && rows.indexOf("l1") < 0, rows.join(","));
  }

  // ── Scenario H: plot scatter + heatmap render SVG ──────────────────────────
  emitUi({
    cmd: "render",
    target: "@region:results",
    body: [
      { t: "plot", id: "sc1", spec: { type: "scatter", data: [{ x: 1, y: 2 }, { x: 3, y: 4 }, { x: 5, y: 1 }] } },
      { t: "plot", id: "hm1", spec: { type: "heatmap", z: [[0, 1], [2, 3]] } },
    ],
  });
  await waitFor(() => doc.querySelector("[data-br-iid='sc1'] svg") && doc.querySelector("[data-br-iid='hm1'] svg"));
  check("plot scatter produces an svg with one circle per point", doc.querySelectorAll("[data-br-iid='sc1'] circle").length === 3);
  check("plot heatmap produces an svg grid of rects", doc.querySelectorAll("[data-br-iid='hm1'] rect").length >= 4);

  // ── Scenario I: network engine mounts, selects, exposes positions ──────────
  emitUi({
    cmd: "render",
    target: "@region:results",
    body: [
      {
        t: "network",
        id: "net1",
        spec: {
          nodes: [{ id: "n1", label: "A" }, { id: "n2", label: "B" }, { id: "n3", label: "C" }],
          edges: [{ source: "n1", target: "n2" }, { source: "n2", target: "n3" }],
        },
      },
    ],
  });
  await waitFor(() => doc.querySelector("[data-br-iid='net1'] canvas"));
  check("network mounts a canvas", !!doc.querySelector("[data-br-iid='net1'] canvas"));
  const netCanvas = doc.querySelector("[data-br-iid='net1'] canvas");
  let netSel = "none";
  netCanvas.addEventListener("br-network-select", (e) => {
    netSel = e && e.detail ? e.detail.id : null;
  });
  win.BioRouter.ui.network("net1").select("n1");
  check("programmatic select fires a br-network-select CustomEvent", netSel === "n1", "detail.id=" + netSel);
  const netPos = win.BioRouter.ui.network("net1").positions();
  check(
    "network exposes a positions object keyed by node id",
    !!netPos && typeof netPos === "object" && !!netPos.n1 && typeof netPos.n1.x === "number",
    JSON.stringify(netPos && netPos.n1)
  );

  // ── Scenario J: author component registry — mount + update ─────────────────
  win.eval(
    "window.__cMount = []; window.__cUpdate = [];" +
      "window.BioRouter.components.register('greeter', {" +
      "  mount: function (el, props) { window.__cMount.push(props && props.name); el.setAttribute('data-greet', (props && props.name) || ''); el.textContent = 'hi ' + ((props && props.name) || ''); }," +
      "  update: function (el, props) { window.__cUpdate.push(props && props.name); el.setAttribute('data-greet', (props && props.name) || ''); el.textContent = 'hi ' + ((props && props.name) || ''); }" +
      "});"
  );
  emitUi({ cmd: "render", target: "@region:results", body: [{ t: "component", id: "c1", name: "greeter", props: { name: "Ada" } }] });
  await waitFor(() => doc.querySelector("[data-br-iid='c1'][data-greet='Ada']"));
  check("component mount runs with the agent-supplied props", win.__cMount.length === 1 && win.__cMount[0] === "Ada", JSON.stringify(win.__cMount));
  emitUi({ cmd: "patch", ops: [{ op: "set_props", id: "c1", props: { name: "Grace" } }] });
  await waitFor(() => doc.querySelector("[data-br-iid='c1'][data-greet='Grace']"));
  check("component update runs with new props (container preserved)", win.__cUpdate.length === 1 && win.__cUpdate[0] === "Grace", JSON.stringify(win.__cUpdate));

  // ── Scenario K: privileged html + figure nodes ─────────────────────────────
  emitUi({
    cmd: "render",
    target: "@region:results",
    body: [
      { t: "html", id: "h1", html: "<b class='br-sanit'>safe-html</b>" },
      { t: "figure", id: "fig1", html: "<!doctype html><body>fig</body>" },
    ],
  });
  await waitFor(() => doc.querySelector("[data-br-iid='h1'] .br-sanit") && doc.querySelector("[data-br-iid='fig1'] iframe"));
  check("html node lands its (server-sanitized) innerHTML", !!doc.querySelector("[data-br-iid='h1'] .br-sanit"));
  {
    const fr = doc.querySelector("[data-br-iid='fig1'] iframe");
    check(
      "figure node creates a sandboxed iframe with srcdoc",
      !!fr && fr.getAttribute("sandbox") === "allow-scripts" && (fr.getAttribute("srcdoc") || "").indexOf("fig") >= 0,
      fr && fr.getAttribute("sandbox")
    );
  }

  dom.window.close();
  log(failures === 0 ? "ALL PASS" : `${failures} FAILURE(S)`);
  return failures === 0 ? 0 : 1;
}
