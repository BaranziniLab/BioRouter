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
import { createHash } from "node:crypto";
import { extname, resolve, normalize, join } from "node:path";

const args = process.argv.slice(2);
const argOf = (name, dflt) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : dflt;
};
const APP = resolve(argOf("--app", "."));
const PORT = Number(argOf("--port", 8899));

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
  console.log(`ui-control harness: app=${APP} on http://127.0.0.1:${PORT}`);
});
