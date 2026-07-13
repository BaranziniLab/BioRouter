#!/usr/bin/env node
/**
 * app-smoke.mjs — the executing check. Lint that RUNS the app.
 * ============================================================
 *
 * Every remaining finding from the 100-app test drive shares one property:
 * **no string analysis can catch it.** The code is correct-looking and the failure
 * is a runtime state.
 *
 *   * a control that fires and delivers no turn (the run queue was wedged, or the
 *     target lookup threw before any paint) — the click handler completed, nothing
 *     was sent, the console was clean, and the session held no record of it;
 *   * a bound KPI that renders blank until a *paid* agent turn writes to the shared
 *     document;
 *   * a slider that no arrow key can move;
 *   * a drag surface that only a human mouse can drive;
 *   * a progress stream that displaces the science it was meant to show.
 *
 * The audit could not see any of these statically, and neither could `lint_app`.
 * The only evidence that matters is **a frame on the wire** and **a pixel on the
 * page**, so this harness produces both: it boots the built app in a real browser
 * against a real (mock) daemon, drives it, and asserts.
 *
 * Usage:
 *   node scripts/agent-drafter/app-smoke.mjs <app-dir> [--json] [--port N]
 *
 * Exit codes: 0 = pass, 1 = findings, 2 = could not run (no browser, bad app dir).
 * `BIOROUTER_APP_SMOKE=off` makes it a no-op success, for the escape hatch.
 */

import { createServer } from "node:http";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { extname, join, normalize, resolve, dirname } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));

// ── args ────────────────────────────────────────────────────────────────────
const args = process.argv.slice(2);
const APP = resolve(args.find((a) => !a.startsWith("--")) || ".");
const AS_JSON = args.includes("--json");
const PORT = Number((args.find((a) => a.startsWith("--port=")) || "").split("=")[1] || 0);

if (String(process.env.BIOROUTER_APP_SMOKE || "").toLowerCase() === "off") {
  if (AS_JSON) console.log(JSON.stringify({ skipped: "BIOROUTER_APP_SMOKE=off", findings: [] }));
  else console.log("app-smoke: skipped (BIOROUTER_APP_SMOKE=off)");
  process.exit(0);
}

if (!existsSync(join(APP, "index.html"))) {
  fail(2, `not an app directory (no index.html): ${APP}`);
}

// ── findings ────────────────────────────────────────────────────────────────
/** @type {{level:'error'|'warn', check:string, msg:string}[]} */
const findings = [];
const error = (check, msg) => findings.push({ level: "error", check, msg });
const warn = (check, msg) => findings.push({ level: "warn", check, msg });

function fail(code, msg) {
  if (AS_JSON) console.log(JSON.stringify({ error: msg, findings }));
  else console.error(`app-smoke: ${msg}`);
  process.exit(code);
}

// ── the mock daemon: records every frame the app sends ───────────────────────
// This is the whole point. A control "works" only if a frame reaches the wire.
const GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json",
  ".svg": "image/svg+xml",
  ".png": "image/png",
};

/** Frames the app sent us. The ONLY evidence a control did anything. */
const received = [];
let client = null;
/** The app's id — the SDK needs it to build the socket URL. */
const APP_ID = APP.split("/").filter(Boolean).pop();
/** Filled in once the server is listening. */
let PORT_ACTUAL = 0;
/** The design system the daemon inlines into every served app. */
const THEME_CSS = await readFile(
  join(HERE, "../../crates/biorouter-mcp/src/agent_drafter/templates/theme.css"),
  "utf8"
).catch(() => "");

function encodeFrame(text) {
  const payload = Buffer.from(text, "utf8");
  const len = payload.length;
  let head;
  if (len < 126) head = Buffer.from([0x81, len]);
  else if (len < 65536) {
    head = Buffer.alloc(4);
    head[0] = 0x81;
    head[1] = 126;
    head.writeUInt16BE(len, 2);
  } else {
    head = Buffer.alloc(10);
    head[0] = 0x81;
    head[1] = 127;
    head.writeBigUInt64BE(BigInt(len), 2);
  }
  return Buffer.concat([head, payload]);
}

function decodeFrames(buf, onText) {
  let i = 0;
  while (i + 2 <= buf.length) {
    const b1 = buf[i + 1];
    const masked = (b1 & 0x80) !== 0;
    let len = b1 & 0x7f;
    let off = i + 2;
    if (len === 126) {
      if (off + 2 > buf.length) return buf.slice(i);
      len = buf.readUInt16BE(off);
      off += 2;
    } else if (len === 127) {
      if (off + 8 > buf.length) return buf.slice(i);
      len = Number(buf.readBigUInt64BE(off));
      off += 8;
    }
    let mask = null;
    if (masked) {
      if (off + 4 > buf.length) return buf.slice(i);
      mask = buf.slice(off, off + 4);
      off += 4;
    }
    if (off + len > buf.length) return buf.slice(i);
    const data = buf.slice(off, off + len);
    if (mask) for (let k = 0; k < data.length; k++) data[k] ^= mask[k % 4];
    const opcode = buf[i] & 0x0f;
    if (opcode === 1) onText(data.toString("utf8"));
    i = off + len;
  }
  return buf.slice(i);
}

const send = (obj) => client && client.write(encodeFrame(JSON.stringify(obj)));

const server = createServer(async (req, res) => {
  const url = (req.url || "/").split("?")[0];
  if (url.endsWith("/models")) {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ providers: [] }));
    return;
  }
  let path = decodeURIComponent(url);
  if (path === "/" || path.endsWith("/")) path = "/index.html";
  const file = resolve(APP, "." + normalize(path));
  if (file !== APP && !file.startsWith(APP + "/")) {
    res.writeHead(403).end("Forbidden");
    return;
  }
  try {
    let body = await readFile(file);

    // Serve index.html the way the DAEMON does.
    //
    // `render.rs` injects the `#biorouter-app-config` island at serve time — the
    // app's on-disk index.html has no such island. Without it the SDK has no appId,
    // never opens the socket, and every control looks dead. A smoke runner that
    // served the raw file would therefore fail every app for the wrong reason,
    // which is worse than not running at all: a check that cries wolf gets muted.
    if (file.endsWith("index.html")) {
      let html = body.toString("utf8");
      const inject = [];
      if (!html.includes("biorouter-app-config")) {
        const cfg = JSON.stringify({ appId: APP_ID, endpoint: `ws://127.0.0.1:${PORT_ACTUAL}` })
          .replace(/</g, "\\u003c");
        inject.push(
          `<script type="application/json" id="biorouter-app-config">${cfg}</script>`
        );
      }
      // The daemon also injects the bundle tag and inlines the theme — the app's
      // on-disk index.html carries NEITHER. Serving it raw yields a page with no
      // SDK at all, so every control is dead for a reason that has nothing to do
      // with the app.
      if (!/<script[^>]+dist\/app\.js/.test(html)) {
        inject.push(`<script src="dist/app.js"></script>`);
      }
      if (THEME_CSS && !html.includes("--br-accent")) {
        inject.push(`<style>${THEME_CSS}</style>`);
      }
      if (inject.length) {
        const blob = inject.join("\n");
        html = html.includes("</body>")
          ? html.replace("</body>", `${blob}\n</body>`)
          : html + blob;
      }
      body = Buffer.from(html, "utf8");
    }

    res.writeHead(200, { "Content-Type": MIME[extname(file)] || "application/octet-stream" });
    res.end(body);
  } catch {
    res.writeHead(404).end("Not found");
  }
});

server.on("upgrade", (req, socket) => {
  const key = req.headers["sec-websocket-key"];
  const accept = createHash("sha1").update(key + GUID).digest("base64");
  socket.write(
    "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n" +
      `Sec-WebSocket-Accept: ${accept}\r\n\r\n`
  );
  client = socket;
  let buf = Buffer.alloc(0);
  socket.on("data", (chunk) => {
    buf = decodeFrames(Buffer.concat([buf, chunk]), (text) => {
      let frame;
      try {
        frame = JSON.parse(text);
      } catch {
        return;
      }
      received.push(frame);
      // A prompt/call starts a turn. Answer it so the app's run queue drains —
      // an unanswered turn would wedge `runChain`, which is the very bug the
      // watchdog now covers, and we do not want to test the watchdog by accident.
      if (frame.type === "prompt" || frame.type === "call") {
        send({ type: "message", delta: "ok", agent: frame.agent });
        send({ type: "done", agent: frame.agent });
      }
    });
  });
  socket.on("error", () => {});

  // The SDK waits for `ready` before it will send anything.
  send({
    type: "ready",
    protocol: 2,
    capabilities: ["ui"],
    sessionId: "smoke",
    stateVersion: 0,
    profiles: [],
    surface: { signals: [], actions: [] },
  });
});

// ── run ─────────────────────────────────────────────────────────────────────
const chromium = await loadChromium();
if (!chromium) {
  // Degrading to "cannot run" rather than "passed" is deliberate: a check that
  // silently reports success when it did not execute is the exact failure mode
  // this harness exists to eliminate.
  fail(2, "playwright/chromium not available — cannot execute the app. Install it, or set BIOROUTER_APP_SMOKE=off to skip.");
}

await new Promise((r) => server.listen(PORT, "127.0.0.1", r));
const { port } = server.address();
PORT_ACTUAL = port;
const base = `http://127.0.0.1:${port}/`;

// Prefer a bundled headless shell; fall back to system Chrome. A partially
// downloaded playwright cache is common on dev machines and must not silently
// turn this check into a no-op.
let browser = null;
for (const opts of [
  { args: ["--no-sandbox"] },
  { channel: "chrome", args: ["--no-sandbox"] },
  { channel: "chromium", args: ["--no-sandbox"] },
]) {
  try {
    browser = await chromium.launch(opts);
    break;
  } catch {
    /* try the next */
  }
}
if (!browser) {
  server.close();
  fail(
    2,
    "could not launch a browser (playwright's chromium is not installed and no system Chrome " +
      "was found). Run `npx playwright install chromium`, or set BIOROUTER_APP_SMOKE=off to skip. " +
      "This is deliberately NOT a pass: a check that reports success without executing is the " +
      "exact failure this harness exists to eliminate."
  );
}

const page = await browser.newPage();
const consoleErrors = [];
page.on("console", (m) => m.type() === "error" && consoleErrors.push(m.text()));
page.on("pageerror", (e) => consoleErrors.push(String(e)));

try {
  await page.goto(base, { waitUntil: "load" });
  await page.waitForTimeout(600); // let the SDK connect + paint

  await checkBindingsPaintOnFirstLoad(page);
  await checkControlsDeliverATurn(page);
  await checkKeyboardReachesState(page);
  await checkDragIsReachable(page);
  await checkProgressDoesNotDisplaceResults(page);

  for (const e of consoleErrors) {
    // The favicon 401 is a known-benign artefact of the served-app path.
    if (/favicon/i.test(e)) continue;
    warn("console", `console error: ${e.slice(0, 160)}`);
  }
} catch (e) {
  // A harness that crashes must not look like a harness that passed.
  await browser.close().catch(() => {});
  server.close();
  fail(2, `the app could not be executed: ${e && e.message ? e.message : e}`);
} finally {
  await browser.close().catch(() => {});
  server.close();
}

report();

// ── checks ──────────────────────────────────────────────────────────────────

/**
 * ZERO agent turns have run. Every `data-br-bind` element must already show its
 * value.
 *
 * The shared state document used to start empty, so bindings rendered blank until
 * a *paid* turn wrote to it — which is precisely why authors kept a private local
 * `state` object, which then silently diverged from the document the agent reads.
 * The blank render is the root; the divergence is the symptom.
 */
async function checkBindingsPaintOnFirstLoad(page) {
  const r = await page.evaluate(() => {
    const els = Array.from(document.querySelectorAll("[data-br-bind]"));
    return {
      total: els.length,
      blank: els
        .filter((el) => !(el.textContent || "").trim())
        .map((el) => el.getAttribute("data-br-bind"))
        .slice(0, 8),
    };
  });
  if (!r.total) return;
  if (r.blank.length) {
    error(
      "bindings-first-load",
      `${r.blank.length} of ${r.total} bound elements render BLANK before any agent turn ` +
        `(${r.blank.join(", ")}). Declare surface.state_initial — until you do, every KPI is ` +
        `empty until a paid turn writes to the shared document, and the local-state workaround ` +
        `that follows is what makes the app and the agent read different numbers.`
    );
  }
}

/**
 * THE check. Click every wired control and assert a frame reaches the wire.
 *
 * A control that fires and delivers no turn is invisible to every other kind of
 * test: the handler completes, the console is clean, and the session holds no
 * record. Only the wire knows.
 */
async function checkControlsDeliverATurn(page) {
  const controls = await page.$$(
    "button:not([disabled]), [role=button]:not([aria-disabled=true]), [data-br-action]"
  );
  if (!controls.length) return;

  let probed = 0;
  const dead = [];

  for (const c of controls) {
    const label = (
      (await c.getAttribute("data-br-action")) ||
      (await c.textContent()) ||
      "(control)"
    )
      .trim()
      .slice(0, 40);

    // Skip obvious non-turn affordances so we do not cry wolf.
    if (/dismiss|close|cancel|copy|expand|collapse/i.test(label)) continue;
    if (!(await c.isVisible().catch(() => false))) continue;

    const before = received.length;
    await c.click({ timeout: 1500 }).catch(() => {});
    probed++;
    // The turn frame is sent on click (or after a debounce).
    await page.waitForTimeout(700);
    const sent = received
      .slice(before)
      .some((f) => f.type === "prompt" || f.type === "call" || f.type === "signal");
    if (!sent) dead.push(label);
    if (probed >= 6) break; // bounded: enough to catch a wedged queue
  }

  if (probed && dead.length === probed) {
    error(
      "control-delivers-a-turn",
      `every control probed (${dead.join(", ")}) fired and delivered NOTHING to the agent — ` +
        `no prompt, call, or signal frame reached the wire. The click handler completed and the ` +
        `console was clean, which is exactly how this failure hides. Check br.run's target and ` +
        `that the run queue is not wedged.`
    );
  } else if (dead.length) {
    warn(
      "control-delivers-a-turn",
      `${dead.length}/${probed} controls delivered no frame to the agent: ${dead.join(", ")}. ` +
        `If they are meant to be local-only, ignore; if they are meant to start a turn, they are dead.`
    );
  }
}

/**
 * A bound range/select must be movable by KEYBOARD, and the move must reach the
 * shared state — not just the DOM.
 *
 * The generated code listened for `change` while re-rendering the region from a
 * stale local object, so arrow-key `input` events never reached the document and a
 * slider sat at 0.35 no matter how many times it was pressed. jsdom cannot catch
 * this: only a real browser implements native range key handling.
 */
async function checkKeyboardReachesState(page) {
  const inputs = await page.$$('input[type=range][data-br-model], input[type=range][data-br-bind-attr]');
  if (!inputs.length) return;

  for (const el of inputs.slice(0, 3)) {
    const before = await el.inputValue().catch(() => null);
    if (before === null) continue;
    await el.focus();
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("ArrowRight");
    await page.waitForTimeout(150);
    const after = await el.inputValue().catch(() => before);
    if (after === before) {
      const id = (await el.getAttribute("data-br-model")) || (await el.getAttribute("id")) || "(range)";
      error(
        "keyboard-reaches-state",
        `the range "${id}" does not respond to arrow keys. Bind it with data-br-model="/pointer" ` +
          `so the SDK owns the write path — a hand-rolled 'change' listener that re-renders from a ` +
          `local object swallows the 'input' events arrow keys produce.`
      );
    }
  }
}

/**
 * A drag surface must be usable without a mouse.
 *
 * HTML5 drag-and-drop does not fire `dragstart` for a synthetic pointer move, so a
 * hand-rolled drag is unreachable by keyboard, touch, and every automated or
 * assistive pointer. Spec-009's core interaction could be performed only by a human.
 */
async function checkDragIsReachable(page) {
  const r = await page.evaluate(() => {
    const legacy = document.querySelectorAll('[draggable="true"]').length;
    const items = Array.from(document.querySelectorAll("[data-br-item]"));
    const zones = Array.from(document.querySelectorAll("[data-br-zone]"));
    if (!items.length || !zones.length) {
      return { legacy, hasPrimitive: false, keyboardWorks: null };
    }
    const item = items[0];
    const zone = zones[0];
    const key = (k) =>
      item.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true }));
    const before = zone.children.length;
    item.focus();
    key("Enter"); // pick up
    key("Enter"); // drop on the highlighted zone
    return {
      legacy,
      hasPrimitive: true,
      keyboardWorks: zone.children.length > before || !!(item.getAttribute("aria-grabbed") === "false" && !item.classList.contains("is-picked")),
      childrenDelta: zone.children.length - before,
    };
  });

  if (r.legacy && !r.hasPrimitive) {
    error(
      "drag-is-reachable",
      `${r.legacy} element(s) use HTML5 draggable with no br.dnd surface. A synthetic or assistive ` +
        `pointer never fires dragstart, so this interaction is reachable only by a human with a ` +
        `working mouse. Use br.dnd.catalog({source, target, signal, onDrop}).`
    );
    return;
  }
  if (r.hasPrimitive && r.keyboardWorks === false) {
    error(
      "drag-is-reachable",
      "the drag surface did not respond to the keyboard (Enter to pick up, Enter to drop)."
    );
  }
  // A keyboard drop that produced no signal is a drag the agent never hears about.
  if (r.hasPrimitive && r.childrenDelta > 0) {
    const sawSignal = received.some((f) => f.type === "signal");
    if (!sawSignal) {
      warn(
        "drag-emits-a-signal",
        "the drop landed but no signal frame reached the agent. Pass `signal:` to br.dnd.catalog — " +
          "a drag the agent never hears about is decoration."
      );
    }
  }
}

/**
 * Tool-call progress must not render inside the semantic result region.
 *
 * `br.run(prompt, "#synthesis")` used to mount a timeline INSIDE the target by
 * construction, so plumbing displaced the science — and an app that also mounted its
 * own timeline rendered every event twice.
 */
async function checkProgressDoesNotDisplaceResults(page) {
  const r = await page.evaluate(() => {
    const subtrees = new Set();
    for (const step of document.querySelectorAll(".br-run-step")) {
      let n = step.parentElement;
      while (n && !n.classList.contains("br-run-status")) n = n.parentElement;
      if (n) subtrees.add(n);
    }
    const inResultRegion = Array.from(subtrees).some((n) =>
      n.closest('[data-br-region="results"], [data-br-region="result"]')
    );
    return { sinks: subtrees.size, inResultRegion };
  });
  if (r.sinks > 1) {
    warn(
      "progress-isolation",
      `tool-call progress is rendering in ${r.sinks} places at once — the same events, twice.`
    );
  }
  if (r.inResultRegion) {
    warn(
      "progress-isolation",
      "tool-call progress is rendering inside the declared result region, displacing the result. " +
        "Pass `progress:` to br.run, or mount a timeline where progress belongs."
    );
  }
}

// ── plumbing ────────────────────────────────────────────────────────────────

async function loadChromium() {
  const candidates = [
    join(HERE, "../../ui/desktop/node_modules/playwright/index.js"),
    join(HERE, "../../ui/desktop/node_modules/playwright-core/index.js"),
  ];
  for (const c of candidates) {
    if (!existsSync(c)) continue;
    try {
      const pw = await import(pathToFileURL(c).href);
      const chromium = pw.chromium || (pw.default && pw.default.chromium);
      if (chromium) return chromium;
    } catch {
      /* try the next */
    }
  }
  return null;
}

function report() {
  const errors = findings.filter((f) => f.level === "error");
  if (AS_JSON) {
    console.log(JSON.stringify({ app: APP, frames: received.length, findings }, null, 2));
  } else {
    console.log(`\napp-smoke: ${APP}`);
    console.log(`  frames the app sent the agent: ${received.length}`);
    if (!findings.length) console.log("  ✓ no findings — every control delivered, every binding painted");
    for (const f of findings) {
      console.log(`  ${f.level === "error" ? "ERROR" : "warn "} [${f.check}] ${f.msg}`);
    }
  }
  process.exit(errors.length ? 1 : 0);
}
