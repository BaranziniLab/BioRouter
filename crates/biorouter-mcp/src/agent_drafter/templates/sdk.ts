/**
 * BioRouter App SDK
 * =================
 * The runtime every Agent-Drafter-built app uses to talk to a *real* BioRouter
 * agent. Unlike the old "bridge" mode (which only forwarded a prompt into the
 * BioRouter chat box), this SDK opens a per-app WebSocket to the BioRouter
 * backend, which runs the full agent loop — the app's own model, extensions,
 * skills and knowledge base — and streams the answer (text / markdown / tool
 * activity) straight back into the app.
 *
 * The app is authored in TypeScript and bundled with esbuild; this module is
 * `import`ed by the app's `main.ts`. It has no external dependencies (markdown
 * rendering is built in) so a built app is fully self-contained and works when
 * served by `biorouterd` or opened as a standalone export.
 */

export interface AppConfig {
  /** Stable app id (matches the on-disk project + the backend route). */
  appId: string;
  /**
   * Explicit agent WebSocket endpoint. When omitted, it is derived from the
   * page location: `ws[s]://<host>/apps/<appId>/agent`.
   */
  endpoint?: string;
  /**
   * Candidate endpoints, tried in order until one connects. Standalone exports
   * set this so the app works whether it is served by the daemon itself, by the
   * bundled `serve.mjs` proxy, or opened straight off disk while a daemon runs
   * on some other port.
   */
  endpoints?: string[];
  /** Greeting shown when the default chat panel mounts. */
  greeting?: string;
  /** Auto-mount a chat panel into `[data-br-chat]` if the app has no custom UI. */
  autoChat?: boolean;
  /** Mount the agent-driven UI runtime (panels, charts, highlights). Default on. */
  ui?: boolean;
}

export interface ImageInput {
  /** MIME type, e.g. "image/png". */
  mimeType: string;
  /** Base64-encoded image bytes (no data-URL prefix). */
  data: string;
}

export interface PromptOptions {
  images?: ImageInput[];
}

export interface TimelineOptions {
  maxItems?: number;
}

export interface TimelineSummary {
  label: string;
  detail: string;
  state: string;
}

/** Events emitted while the agent answers a prompt. */
export type AgentEvent =
  | { type: "ready"; protocol?: number; capabilities?: string[]; sessionId?: string; resumed?: boolean; messageCount?: number }
  | { type: "message"; delta: string }
  | { type: "thought"; delta: string }
  | { type: "tool"; name: string; status: string; id?: string }
  | { type: "done" }
  | { type: "error"; message: string }
  // ── BRSDK protocol v2 (additive; gated by the ready frame's capabilities) ──
  | { type: "output"; schema?: unknown; value: unknown }
  | { type: "usage"; inputTokens?: number; outputTokens?: number; totalTokens?: number; model?: string }
  | { type: "guardrail"; stage?: string; name?: string; blocked?: boolean; reason?: string }
  | { type: "approval"; requestId: string; tool: string; args?: unknown; prompt?: string | null }
  | { type: "tool_call"; id: string; name: string; args?: unknown }
  | { type: "handoff"; from?: string; to?: string }
  | { type: "compaction"; phase: string; trigger?: string; before?: number; after?: number }
  | { type: "trace"; span?: unknown; snapshot?: unknown }
  | { type: "context"; used?: number; limit?: number; ratio?: number }
  | { type: "history"; messages?: Array<{ role: string; text: string }> }
  | { type: "model"; ok: boolean; provider?: string; model?: string }
  | { type: "widget"; id: string; tree: unknown }
  // ── Agent-driven UI control (BRSDK v3) ──
  // The agent's `ui_*` tools push these; the UI runtime below applies them.
  | ({ type: "ui" } & UiCommand);

type EventKind = AgentEvent["type"];
type Listener = (ev: AgentEvent) => void;
// Named aliases so the no-esbuild fallback type-stripper can remove these
// annotations (it keys off an uppercase/primitive leading type token, which a
// bare `(() => void)` annotation lacks).
type ResolveFn = () => void;
type RejectFn = (e: Error) => void;

declare global {
  interface Window {
    BIOROUTER_APP_CONFIG?: AppConfig;
    BioRouter?: BioRouterClient;
    /** Not in older lib.dom typings; used by `cssEscape`'s feature check. */
    CSS?: { escape?: (value: string) => string };
  }
}

/** Stable per-app client id (persisted) so sessions resume across reloads. */
function getClientId(appId: string): string {
  const key = "br.client." + appId;
  try {
    let id = window.localStorage.getItem(key);
    if (!id) {
      id = "c-" + Math.random().toString(36).slice(2) + Date.now().toString(36);
      window.localStorage.setItem(key, id);
    }
    return id;
  } catch {
    // localStorage unavailable (e.g. private mode) → ephemeral id (no resume).
    return "c-" + Math.random().toString(36).slice(2);
  }
}

/**
 * The endpoint the page's own origin implies. `null` under `file://`, where
 * there is no host to derive one from.
 */
function sameOriginEndpoint(appId: string): string | null {
  const loc = window.location;
  if (loc.protocol !== "http:" && loc.protocol !== "https:") return null;
  const proto = loc.protocol === "https:" ? "wss:" : "ws:";
  return `${proto}//${loc.host}/apps/${appId}/agent`;
}

/**
 * Candidate agent endpoints, most-likely first. Exports used to bake a single
 * absolute `ws://127.0.0.1:3000/...`, which fails whenever the daemon isn't on
 * that port (the desktop app starts it on an ephemeral one) — so we now try the
 * page's own origin first, then any configured fallbacks.
 */
function resolveEndpoints(cfg: AppConfig): string[] {
  const out: string[] = [];
  const add = (e: string | null) => {
    if (e && out.indexOf(e) < 0) out.push(e);
  };
  // An explicit single endpoint always wins (a remote daemon, a test harness).
  if (cfg.endpoint) add(cfg.endpoint);
  else add(sameOriginEndpoint(cfg.appId));
  for (const e of cfg.endpoints || []) add(e);
  if (!cfg.endpoint) add(sameOriginEndpoint(cfg.appId));
  return out;
}

function withClientId(base: string, appId: string): string {
  const cid = encodeURIComponent(getClientId(appId));
  const sep = base.indexOf("?") >= 0 ? "&" : "?";
  return `${base}${sep}client_id=${cid}`;
}

/** Open one WebSocket, resolving on `open` and rejecting on the first error. */
function openSocket(url: string): Promise<WebSocket> {
  return new Promise((resolve, reject) => {
    let ws: WebSocket;
    try {
      ws = new WebSocket(url);
    } catch (e) {
      reject(e as Error);
      return;
    }
    const onOpen = () => {
      ws.removeEventListener("error", onErr);
      resolve(ws);
    };
    const onErr = () => {
      ws.removeEventListener("open", onOpen);
      reject(new Error(`could not connect to ${url}`));
    };
    ws.addEventListener("open", onOpen, { once: true });
    ws.addEventListener("error", onErr, { once: true });
  });
}

export class BioRouterClient {
  readonly config: AppConfig;
  private ws: WebSocket | null = null;
  private readyPromise: Promise<void> | null = null;
  // Lazily-populated so new/unknown event kinds (BRSDK v2, future frames) work
  // without enumerating every key — `on()` seeds buckets on demand.
  private listeners: Record<string, Listener[]> = {};
  // Capabilities advertised by the server in the `ready` frame (deny-by-default).
  private capabilities: string[] = [];
  // Durable-session info latched from the `ready` frame.
  sessionId: string | null = null;
  resumed = false;
  private activeResolve: ResolveFn | null = null;
  private activeReject: RejectFn | null = null;
  private tokensWaiters: Array<(u: { used: number; limit: number; ratio: number }) => void> = [];
  private historyWaiters: Array<(m: Array<{ role: string; text: string }>) => void> = [];
  // Last widget tree the server sent for each widget id.
  private widgetStore: Map<string, WidgetNode> = new Map();
  /** The agent-driven UI runtime: applies `ui` frames to the page. */
  readonly ui: UiRuntime;
  /** The endpoint that actually connected (useful in diagnostics). */
  activeEndpoint: string | null = null;

  constructor(config: AppConfig) {
    this.config = config;
    this.ui = new UiRuntime(this);
  }

  /** Register a listener for an agent event. Returns `this` for chaining. */
  on(kind: EventKind, fn: Listener): this {
    (this.listeners[kind] ||= []).push(fn);
    return this;
  }

  private emit(ev: AgentEvent): void {
    for (const fn of this.listeners[ev.type] || []) {
      try {
        fn(ev);
      } catch {
        /* listener errors are non-fatal */
      }
    }
  }

  /**
   * Open (or reuse) the WebSocket to the BioRouter agent backend, trying each
   * candidate endpoint in turn. Rejects with an actionable message listing what
   * was tried, rather than a bare "could not reach the backend".
   */
  connect(): Promise<void> {
    if (this.readyPromise) return this.readyPromise;
    this.readyPromise = this.dial().catch((e: Error) => {
      // Let a later call retry (e.g. after the user starts biorouterd).
      this.readyPromise = null;
      throw e;
    });
    return this.readyPromise;
  }

  private async dial(): Promise<void> {
    const candidates = resolveEndpoints(this.config);
    if (!candidates.length) {
      throw new Error(
        "No BioRouter endpoint to connect to. This page was opened from a file:// URL " +
          "with no fallback configured — serve it with `npm start` instead."
      );
    }
    const tried: string[] = [];
    for (const base of candidates) {
      const url = withClientId(base, this.config.appId);
      try {
        const ws = await openSocket(url);
        this.ws = ws;
        this.activeEndpoint = base;
        ws.onerror = () => {
          this.emit({ type: "error", message: "connection error" });
          this.settleActive(new Error("connection error"));
        };
        ws.onclose = () => {
          this.readyPromise = null;
          this.ws = null;
          this.settleActive(new Error("connection closed"));
        };
        ws.onmessage = (ev) => this.handleFrame(ev.data);
        return;
      } catch {
        tried.push(base);
      }
    }
    throw new Error(
      "Could not reach the BioRouter backend. Tried: " +
        tried.join(", ") +
        ". Start it with `biorouterd agent`, or run this app's `run.sh` / `npm start`, " +
        "which starts one for you."
    );
  }

  private settleActive(err?: Error): void {
    if (err && this.activeReject) {
      const rej = this.activeReject;
      this.activeResolve = null;
      this.activeReject = null;
      rej(err);
    } else if (!err && this.activeResolve) {
      const res = this.activeResolve;
      this.activeResolve = null;
      this.activeReject = null;
      res();
    }
  }

  private handleFrame(raw: string): void {
    let msg: AgentEvent;
    try {
      msg = JSON.parse(raw);
    } catch {
      return;
    }
    if (msg == null || typeof msg.type !== "string") return;
    // Latch advertised capabilities + durable-session info from `ready`.
    if (msg.type === "ready") {
      if (Array.isArray(msg.capabilities)) this.capabilities = msg.capabilities;
      if (typeof msg.sessionId === "string") this.sessionId = msg.sessionId;
      this.resumed = msg.resumed === true;
      // Tell the agent what this page offers it (regions, ids) so `ui_describe`
      // returns something real and `ui_render` can target the author's markup.
      if (this.has("ui")) this.ui.reportSurface();
    }
    // Agent-driven UI: apply the command to the page.
    if (msg.type === "ui") {
      this.ui.apply(msg);
    }
    // Resolve any pending br.context.tokens() callers.
    if (msg.type === "context") {
      const waiters = this.tokensWaiters;
      this.tokensWaiters = [];
      const u = { used: msg.used || 0, limit: msg.limit || 0, ratio: msg.ratio || 0 };
      for (const w of waiters) {
        try {
          w(u);
        } catch {
          /* ignore */
        }
      }
    }
    // Cache server-emitted widget trees so apps can re-render / look them up.
    if (msg.type === "widget" && typeof msg.id === "string") {
      this.widgetStore.set(msg.id, msg.tree as WidgetNode);
    }
    // Resolve any pending history() callers.
    if (msg.type === "history") {
      const waiters = this.historyWaiters;
      this.historyWaiters = [];
      const m = Array.isArray(msg.messages) ? msg.messages : [];
      for (const w of waiters) {
        try {
          w(m);
        } catch {
          /* ignore */
        }
      }
    }
    this.emit(msg);
    if (msg.type === "done") this.settleActive();
    else if (msg.type === "error") this.settleActive(new Error(msg.message));
  }

  /**
   * Fetch the user-visible message backlog for the current (resumed) session so
   * a reloaded app can repaint its chat history. Returns `[]` when there is no
   * session yet or the request fails.
   */
  history(): Promise<Array<{ role: string; text: string }>> {
    return new Promise((resolve) => {
      this.historyWaiters.push(resolve);
      if (!this.send({ type: "history" })) {
        this.historyWaiters = this.historyWaiters.filter((w) => w !== resolve);
        resolve([]);
      }
    });
  }

  /** Current context-window usage (BRSDK context API). */
  tokens(): Promise<{ used: number; limit: number; ratio: number }> {
    return new Promise((resolve) => {
      this.tokensWaiters.push(resolve);
      if (!this.send({ type: "tokens" })) {
        // Socket not open — resolve with zeros rather than hang.
        this.tokensWaiters = this.tokensWaiters.filter((w) => w !== resolve);
        resolve({ used: 0, limit: 0, ratio: 0 });
      }
    });
  }

  /** Namespaced context API: `br.context.tokens()` / `br.context.history()`. */
  get context() {
    return {
      tokens: () => this.tokens(),
      history: () => this.history(),
    };
  }

  /**
   * The HTTP origin of the daemon we are actually talking to. Derived from the
   * connected WebSocket endpoint, NOT from `window.location` — an exported app
   * can be served from one origin while its agent lives on another, and using
   * the page origin silently 404s every REST call.
   */
  private httpBase(): string {
    const ep = this.activeEndpoint;
    const appPath = `/apps/${encodeURIComponent(this.config.appId)}`;
    if (ep) {
      const u = new URL(ep);
      const proto = u.protocol === "wss:" ? "https:" : "http:";
      return `${proto}//${u.host}${appPath}`;
    }
    const loc = window.location;
    return `${loc.protocol}//${loc.host}${appPath}`;
  }

  /** Provider/model catalog the user has available (the provider-agnostic
   *  headline). Returns `[]` on failure. */
  async listModels(): Promise<unknown[]> {
    try {
      await this.connect();
      const res = await fetch(`${this.httpBase()}/models`);
      if (!res.ok) return [];
      const data = await res.json();
      return Array.isArray(data.providers) ? data.providers : [];
    } catch {
      return [];
    }
  }

  /** Live-switch the session's provider/model. */
  selectModel(provider: string, model: string): void {
    this.send({ type: "modelselect", provider, model });
  }

  /** Namespaced model surface: `br.model.list()` / `br.model.select(p, m)`. */
  get model() {
    return {
      list: () => this.listModels(),
      select: (provider: string, model: string) => this.selectModel(provider, model),
    };
  }

  /** HITL: approve a pending tool surfaced via an `approval` event.
   *  `action` is "allow_once" (default) or "always_allow". */
  approve(requestId: string, action: string = "allow_once"): void {
    this.send({ type: "approve", request: requestId, action });
  }

  /** HITL: reject a pending tool, with an optional human-readable reason. */
  reject(requestId: string, reason?: string): void {
    this.send({ type: "reject", request: requestId, reason });
  }

  /** Render a widget tree into `target` and wire its actions back to the agent
   *  (a submit button sends a `widget_action` frame scoped to this widget id). */
  private renderWidgetInto(
    id: string,
    tree: WidgetNode,
    target: HTMLElement | string
  ): HTMLElement {
    const host =
      typeof target === "string"
        ? (document.querySelector(target) as HTMLElement | null)
        : target;
    const ctx: WidgetContext = {
      fields: new Map(),
      onAction: (action, payload) =>
        this.send({ type: "widget_action", widgetId: id, action, payload }),
    };
    const dom = renderWidget(tree, ctx);
    if (host) {
      host.innerHTML = "";
      host.appendChild(dom);
    }
    return dom;
  }

  /** Interactive widgets API: render an agent-emitted tree, fire an action, or
   *  look up the last tree the server sent for an id. */
  get widgets() {
    return {
      render: (id: string, tree: WidgetNode, target: HTMLElement | string) =>
        this.renderWidgetInto(id, tree, target),
      action: (widgetId: string, action: string, payload?: unknown) =>
        this.send({ type: "widget_action", widgetId, action, payload }),
      // alias kept for symmetry with the agent-side naming
      submit: (widgetId: string, action: string, payload?: unknown) =>
        this.send({ type: "widget_action", widgetId, action, payload }),
      get: (id: string) => this.widgetStore.get(id),
    };
  }

  /** Whether the server advertised a given BRSDK capability in `ready`. */
  has(capability: string): boolean {
    return this.capabilities.indexOf(capability) >= 0;
  }

  /** Send a raw client frame. Public so the UI runtime can answer `ui_ask`. */
  sendRaw(frame: unknown): boolean {
    return this.send(frame);
  }

  /** Send a raw client frame if the socket is open; returns whether it went. */
  private send(frame: unknown): boolean {
    if (this.ws && this.ws.readyState === 1 /* WebSocket.OPEN */) {
      this.ws.send(JSON.stringify(frame));
      return true;
    }
    return false;
  }

  /**
   * Send a prompt to the agent. Resolves when the agent finishes its turn
   * (`done`); reject on error. Streamed output arrives via `on("message", …)`.
   */
  async prompt(text: string, opts: PromptOptions = {}): Promise<void> {
    await this.connect();
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      throw new Error("Not connected to the BioRouter backend.");
    }
    return new Promise<void>((resolve, reject) => {
      this.activeResolve = resolve;
      this.activeReject = reject;
      this.ws!.send(
        JSON.stringify({ type: "prompt", text, images: opts.images || [] })
      );
    });
  }

  /** Ask the agent to stop the current turn. */
  cancel(): void {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ type: "cancel" }));
    }
  }

  /** Remove a previously-registered listener. */
  off(kind: EventKind, fn: Listener): this {
    const arr = this.listeners[kind];
    if (arr) {
      const i = arr.indexOf(fn);
      if (i >= 0) arr.splice(i, 1);
    }
    return this;
  }

  /** Convenience: collect the full reply for one prompt as a single string. */
  async ask(text: string, opts: PromptOptions = {}): Promise<string> {
    let buf = "";
    const onMsg: Listener = (ev) => {
      if (ev.type === "message") buf += ev.delta;
    };
    this.on("message", onMsg);
    try {
      await this.prompt(text, opts);
    } finally {
      this.off("message", onMsg);
    }
    return buf;
  }

  /**
   * The primary helper for custom UIs: send `text` and stream the agent's
   * markdown (and ```chart blocks / tables) into `target` (an element or CSS
   * selector), showing a spinner and tool activity. Returns the full reply.
   * Wire any control to this — e.g. slider/select `change`, button `click`.
   *
   * Runs are serialized: if a call is still streaming when another starts, the
   * new one waits for it, so rapid control changes never overlap or garble the
   * output (and no request is orphaned).
   */
  run(
    text: string,
    target: HTMLElement | string,
    opts: PromptOptions = {}
  ): Promise<string> {
    const next = this.runChain.then(() => this.doRun(text, target, opts));
    this.runChain = next.then(
      () => undefined,
      () => undefined
    );
    return next;
  }

  private runChain: Promise<void> = Promise.resolve();

  private async doRun(
    text: string,
    target: HTMLElement | string,
    opts: PromptOptions = {}
  ): Promise<string> {
    const el =
      typeof target === "string"
        ? document.querySelector<HTMLElement>(target)
        : target;
    if (!el) throw new Error("run(): target element not found");
    let buf = "";
    el.innerHTML =
      '<div class="br-run-status"></div><div class="br-run-answer"><span class="br-spinner"></span> Starting agent run…</div>';
    const statusEl = el.querySelector<HTMLElement>(".br-run-status")!;
    const answerEl = el.querySelector<HTMLElement>(".br-run-answer")!;
    const stopTimeline = mountTimeline(this, statusEl, { maxItems: 18 });
    const onMsg: Listener = (ev) => {
      if (ev.type === "message") {
        buf += ev.delta;
        answerEl.innerHTML = this.renderMarkdown(buf);
      }
    };
    const onTool: Listener = (ev) => {
      if (ev.type === "tool" && !buf) {
        answerEl.innerHTML = `<span class="br-spinner"></span> <span class="br-msg--tool">${ev.name}…</span>`;
      }
    };
    this.on("message", onMsg);
    this.on("tool", onTool);
    try {
      await this.prompt(text, opts);
      if (!buf) answerEl.innerHTML = "";
    } catch (e) {
      // Hoisted out of the template literal so the no-esbuild fallback stripper
      // (which treats `${…}` as part of the string) can strip the `as` cast.
      const emsg = (e as Error).message || "request failed";
      answerEl.innerHTML = `<div class="br-msg br-msg--agent">Failed: ${emsg}</div>`;
      throw e;
    } finally {
      this.off("message", onMsg);
      this.off("tool", onTool);
      stopTimeline();
    }
    return buf;
  }

  /** Minimal, dependency-free markdown → HTML (safe-escaped). */
  renderMarkdown(md: string): string {
    return renderMarkdown(md);
  }
}

// ---------------------------------------------------------------------------
// Markdown rendering (compact, no external deps, HTML-escaped first).
// ---------------------------------------------------------------------------

function escapeHtml(s: string): string {
  // Quotes are escaped too: the model's markdown is untrusted (prompt injection),
  // and escaped text is interpolated into attributes downstream (notably a link
  // href). Without escaping `"`, `[t](https://x" onmouseover="alert(1))` breaks
  // out of the href and injects an event handler — XSS in the app's own origin,
  // which is not a sandboxed iframe.
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function inline(s: string): string {
  // links, bold, italic, inline code — operating on already-escaped text.
  return s
    .replace(/`([^`]+)`/g, (_m, c) => `<code>${c}</code>`)
    .replace(
      /\[([^\]]+)\]\((https?:[^)\s]+)\)/g,
      // Belt-and-suspenders with escapeHtml: the URL is `http(s)` only, and any
      // residual quote/angle/space is stripped so it cannot escape the attribute.
      (_m, t, u) =>
        `<a href="${String(u).replace(/["'<>`\s]/g, "")}" target="_blank" rel="noopener">${t}</a>`
    )
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/(^|[^*])\*([^*]+)\*/g, "$1<em>$2</em>");
}

interface ChartPoint {
  label: string;
  value: number;
}
interface ChartSpec {
  type?: "bar" | "line" | "pie";
  title?: string;
  data: ChartPoint[];
}

interface GraphNode {
  id: string;
  label?: string;
  group?: string;
}
interface GraphEdge {
  source: string;
  target: string;
  label?: string;
}
interface GraphSpec {
  title?: string;
  nodes?: Array<GraphNode | string>;
  edges?: Array<GraphEdge | string>;
}
interface ParsedGraph {
  nodes: GraphNode[];
  edges: GraphEdge[];
}
interface GraphPoint {
  x: number;
  y: number;
}

/**
 * Render an AI-generated chart from a ```chart JSON block into dependency-free
 * SVG, themed with BioRouter tokens. Supports "bar" (default) and "line".
 * Returns a quiet placeholder on malformed/partial JSON (streaming-safe).
 */
export function renderChart(json: string): string {
  let spec: ChartSpec;
  try {
    spec = JSON.parse(json);
  } catch {
    return '<div class="br-chart br-chart--pending"></div>';
  }
  const data = Array.isArray(spec?.data) ? spec.data.filter((d) => d && isFinite(d.value)) : [];
  if (!data.length) return '<div class="br-chart br-chart--pending"></div>';
  const W = 520;
  const H = 240;
  const padL = 44;
  const padB = 48;
  const padT = spec.title ? 28 : 12;
  const max = Math.max(...data.map((d) => d.value), 0);
  const min = Math.min(...data.map((d) => d.value), 0);
  const span = max - min || 1;
  const plotW = W - padL - 12;
  const plotH = H - padT - padB;
  const x = (i: number) => padL + (plotW * (i + 0.5)) / data.length;
  const y = (v: number) => padT + plotH * (1 - (v - min) / span);
  const esc = (s: string) => escapeHtml(String(s));
  const palette = [
    "var(--br-coral)",
    "var(--br-accent)",
    "var(--br-n400)",
    "var(--br-green)",
    "var(--br-n500)",
    "var(--br-n300)",
  ];
  const title = spec.title
    ? `<text x="${W / 2}" y="18" font-size="12" font-weight="600" text-anchor="middle" fill="var(--br-text)">${esc(
        spec.title
      )}</text>`
    : "";
  const svg = (inner: string) =>
    `<div class="br-chart"><svg viewBox="0 0 ${W} ${H}" width="100%" preserveAspectRatio="xMidYMid meet" role="img">${title}${inner}</svg></div>`;

  // Pie chart: slices + legend, no axes.
  if (spec.type === "pie") {
    const total = data.reduce((s, d) => s + Math.max(d.value, 0), 0) || 1;
    const cx = 130;
    const cy = padT + (H - padT - 12) / 2;
    const r = Math.min(cy - padT, 84);
    let a0 = -Math.PI / 2;
    let slices = "";
    data.forEach((d, i) => {
      const frac = Math.max(d.value, 0) / total;
      const a1 = a0 + frac * 2 * Math.PI;
      const large = frac > 0.5 ? 1 : 0;
      const x0 = cx + r * Math.cos(a0);
      const y0 = cy + r * Math.sin(a0);
      const x1 = cx + r * Math.cos(a1);
      const y1 = cy + r * Math.sin(a1);
      slices +=
        frac >= 0.999
          ? `<circle cx="${cx}" cy="${cy}" r="${r}" fill="${palette[i % palette.length]}"/>`
          : `<path d="M${cx},${cy} L${x0.toFixed(1)},${y0.toFixed(1)} A${r},${r} 0 ${large} 1 ${x1.toFixed(
              1
            )},${y1.toFixed(1)} Z" fill="${palette[i % palette.length]}" stroke="var(--br-surface)" stroke-width="1"/>`;
      a0 = a1;
    });
    const legend = data
      .map((d, i) => {
        const pct = Math.round((Math.max(d.value, 0) / total) * 100);
        return `<g transform="translate(${cx + r + 28},${padT + 6 + i * 18})"><rect width="11" height="11" rx="2" fill="${
          palette[i % palette.length]
        }"/><text x="16" y="10" font-size="11" fill="var(--br-text)">${esc(d.label).slice(
          0,
          20
        )} (${pct}%)</text></g>`;
      })
      .join("");
    return svg(slices + legend);
  }

  let body = "";
  if (spec.type === "line") {
    const pts = data.map((d, i) => `${x(i).toFixed(1)},${y(d.value).toFixed(1)}`).join(" ");
    body += `<polyline fill="none" stroke="var(--br-coral)" stroke-width="2.5" points="${pts}"/>`;
    body += data
      .map((d, i) => `<circle cx="${x(i).toFixed(1)}" cy="${y(d.value).toFixed(1)}" r="3" fill="var(--br-accent)"/>`)
      .join("");
  } else {
    const bw = Math.min((plotW / data.length) * 0.62, 56);
    body += data
      .map((d, i) => {
        const bx = x(i) - bw / 2;
        const by = y(Math.max(d.value, 0));
        const bh = Math.abs(y(d.value) - y(0));
        return `<rect x="${bx.toFixed(1)}" y="${by.toFixed(1)}" width="${bw.toFixed(
          1
        )}" height="${bh.toFixed(1)}" rx="3" fill="${palette[i % palette.length]}"/>`;
      })
      .join("");
  }
  // x labels (truncated, centered) + baseline + y-max tick.
  const labels = data
    .map(
      (d, i) =>
        `<text x="${x(i).toFixed(1)}" y="${H - padB + 16}" font-size="10" text-anchor="middle" fill="var(--br-text-muted)">${esc(
          d.label
        ).slice(0, 10)}</text>`
    )
    .join("");
  const axis = `<line x1="${padL}" y1="${y(0).toFixed(1)}" x2="${W - 12}" y2="${y(0).toFixed(
    1
  )}" stroke="var(--br-border)"/>`;
  const yMax = `<text x="${padL - 6}" y="${(y(max) + 4).toFixed(
    1
  )}" font-size="10" text-anchor="end" fill="var(--br-text-muted)">${esc(String(max))}</text>`;
  return svg(axis + body + labels + yMax);
}

function parseGraphEdges(text: string): ParsedGraph {
  const nodeMap = new Map<string, GraphNode>();
  const edges: GraphEdge[] = [];
  const ensure = (id: string) => {
    const clean = id.trim().replace(/^["'`[]+|["'`\]]+$/g, "");
    if (!clean) return "";
    if (!nodeMap.has(clean)) nodeMap.set(clean, { id: clean, label: clean });
    return clean;
  };
  for (const raw of text.split("\n")) {
    const line = raw
      .trim()
      .replace(/;$/, "")
      .replace(/^\s*(graph|flowchart)\s+(TD|TB|BT|LR|RL)\s*/i, "");
    if (!line || line.startsWith("#")) continue;
    const match = line.match(/^(.+?)\s*(?:-->|---|--|->)\s*(.+?)(?:\s*[:|]\s*(.+))?$/);
    if (!match) continue;
    const source = ensure(match[1].replace(/\[.*?\]|\(.*?\)/g, ""));
    const target = ensure(match[2].replace(/\[.*?\]|\(.*?\)/g, ""));
    if (source && target) edges.push({ source, target, label: match[3]?.trim() });
  }
  return { nodes: Array.from(nodeMap.values()), edges };
}

function normalizeGraph(input: string): GraphSpec | null {
  try {
    const spec = JSON.parse(input) as GraphSpec;
    const nodes = Array.isArray(spec.nodes) ? spec.nodes : [];
    const edges = Array.isArray(spec.edges) ? spec.edges : [];
    if (nodes.length || edges.length) return spec;
  } catch {
    // Edge-list and Mermaid-ish blocks are parsed below.
  }
  const parsed = parseGraphEdges(input);
  return parsed.nodes.length || parsed.edges.length ? parsed : null;
}

/**
 * Render a graph/diagram block as dependency-free SVG. Accepts either:
 * ```graph JSON ({ nodes, edges, title }) or a simple edge list:
 * A -> B : relationship
 */
export function renderGraph(input: string): string {
  const spec = normalizeGraph(input);
  if (!spec) return '<div class="br-visual br-visual--pending"></div>';
  const rawNodes = Array.isArray(spec.nodes) ? spec.nodes : [];
  const rawEdges = Array.isArray(spec.edges) ? spec.edges : [];
  const nodes = new Map<string, GraphNode>();
  const ensureNode = (id: string, label?: string) => {
    const clean = String(id || "").trim();
    if (!clean) return;
    if (!nodes.has(clean)) nodes.set(clean, { id: clean, label: label || clean });
  };
  for (const n of rawNodes) {
    if (typeof n === "string") ensureNode(n);
    else ensureNode(n.id, n.label);
  }
  const edges: GraphEdge[] = [];
  for (const e of rawEdges) {
    if (typeof e === "string") {
      const parsed = parseGraphEdges(e);
      for (const n of parsed.nodes) ensureNode(n.id, n.label);
      edges.push(...parsed.edges);
    } else if (e?.source && e?.target) {
      ensureNode(e.source);
      ensureNode(e.target);
      edges.push({ source: e.source, target: e.target, label: e.label });
    }
  }
  if (!nodes.size) {
    for (const e of edges) {
      ensureNode(e.source);
      ensureNode(e.target);
    }
  }
  const list = Array.from(nodes.values()).slice(0, 18);
  const visible = new Set(list.map((n) => n.id));
  const visibleEdges = edges.filter((e) => visible.has(e.source) && visible.has(e.target)).slice(0, 28);
  if (!list.length) return '<div class="br-visual br-visual--pending"></div>';

  const W = 640;
  const H = 360;
  const cx = W / 2;
  const cy = H / 2 + 8;
  const radius = Math.min(132, 52 + list.length * 9);
  const esc = (s: string) => escapeHtml(String(s));
  const pos = new Map<string, GraphPoint>();
  list.forEach((n, i) => {
    const angle = -Math.PI / 2 + (2 * Math.PI * i) / Math.max(list.length, 1);
    pos.set(n.id, {
      x: cx + Math.cos(angle) * radius,
      y: cy + Math.sin(angle) * radius,
    });
  });
  const title = spec.title
    ? `<text x="${cx}" y="24" font-size="13" font-weight="650" text-anchor="middle" fill="var(--br-text)">${esc(spec.title)}</text>`
    : "";
  const edgeEls = visibleEdges
    .map((e) => {
      const a = pos.get(e.source);
      const b = pos.get(e.target);
      if (!a || !b) return "";
      const mx = (a.x + b.x) / 2;
      const my = (a.y + b.y) / 2;
      const label = e.label
        ? `<text x="${mx.toFixed(1)}" y="${(my - 5).toFixed(1)}" font-size="10" text-anchor="middle" fill="var(--br-text-muted)">${esc(
            e.label
          ).slice(0, 26)}</text>`
        : "";
      return `<line x1="${a.x.toFixed(1)}" y1="${a.y.toFixed(1)}" x2="${b.x.toFixed(1)}" y2="${b.y.toFixed(
        1
      )}" stroke="var(--br-visual-line)" stroke-width="1.4"/>${label}`;
    })
    .join("");
  const nodeEls = list
    .map((n, i) => {
      const p = pos.get(n.id)!;
      const fill = i % 3 === 0 ? "var(--br-coral)" : i % 3 === 1 ? "var(--br-accent)" : "var(--br-n500)";
      return `<g><circle cx="${p.x.toFixed(1)}" cy="${p.y.toFixed(1)}" r="18" fill="${fill}"/><text x="${p.x.toFixed(
        1
      )}" y="${(p.y + 4).toFixed(1)}" font-size="10" text-anchor="middle" fill="var(--br-on-accent)">${esc(
        n.label || n.id
      ).slice(0, 10)}</text></g>`;
    })
    .join("");
  return `<div class="br-visual br-graph"><svg viewBox="0 0 ${W} ${H}" width="100%" preserveAspectRatio="xMidYMid meet" role="img">${title}${edgeEls}${nodeEls}</svg></div>`;
}

export function renderMarkdown(md: string): string {
  const src = escapeHtml(md || "");
  const lines = src.split("\n");
  const out: string[] = [];
  let i = 0;
  let inList = false;
  let inOrdered = false;
  const closeList = () => {
    if (inList) {
      out.push(inOrdered ? "</ol>" : "</ul>");
      inList = false;
      inOrdered = false;
    }
  };
  while (i < lines.length) {
    const line = lines[i];
    // fenced code block
    const fence = line.match(/^```([A-Za-z0-9_-]*)(?:\s+.*)?\s*$/);
    if (fence) {
      closeList();
      const code: string[] = [];
      i++;
      while (i < lines.length && !/^```\s*$/.test(lines[i])) {
        code.push(lines[i]);
        i++;
      }
      i++; // skip closing fence
      const fenceKind = (fence[1] || "").toLowerCase();
      // AI-generated visualization blocks render as themed SVG.
      if (fenceKind === "chart") {
        out.push(renderChart(code.join("\n")));
      } else if (["graph", "diagram", "network", "map", "mermaid"].includes(fenceKind)) {
        out.push(renderGraph(code.join("\n")));
      } else {
        out.push(`<pre><code>${code.join("\n")}</code></pre>`);
      }
      continue;
    }
    const heading = line.match(/^(#{1,4})\s+(.*)$/);
    if (heading) {
      closeList();
      const level = heading[1].length;
      out.push(`<h${level}>${inline(heading[2])}</h${level}>`);
      i++;
      continue;
    }
    // GFM table: a header row, a separator row (---|:--:), then body rows.
    if (
      /^\s*\|.*\|\s*$/.test(line) &&
      i + 1 < lines.length &&
      /^\s*\|?[\s:|-]+\|[\s:|-]*$/.test(lines[i + 1]) &&
      lines[i + 1].includes("-")
    ) {
      closeList();
      const cells = (row: string) =>
        row
          .trim()
          .replace(/^\||\|$/g, "")
          .split("|")
          .map((c) => c.trim());
      const header = cells(line);
      i += 2; // skip header + separator
      const body: string[][] = [];
      while (i < lines.length && /^\s*\|.*\|\s*$/.test(lines[i])) {
        body.push(cells(lines[i]));
        i++;
      }
      let t = "<table><thead><tr>";
      t += header.map((h) => `<th>${inline(h)}</th>`).join("");
      t += "</tr></thead><tbody>";
      for (const r of body) {
        t += "<tr>" + r.map((c) => `<td>${inline(c)}</td>`).join("") + "</tr>";
      }
      t += "</tbody></table>";
      out.push(t);
      continue;
    }
    const ul = line.match(/^\s*[-*]\s+(.*)$/);
    const ol = line.match(/^\s*\d+\.\s+(.*)$/);
    if (ul || ol) {
      const ordered = !!ol;
      if (!inList || inOrdered !== ordered) {
        closeList();
        out.push(ordered ? "<ol>" : "<ul>");
        inList = true;
        inOrdered = ordered;
      }
      // Hoisted out of the template literal so the fallback stripper can drop
      // the non-null assertion (`!`) — it skips `${…}` interpolations as string.
      const liText = (ul || ol)![1];
      out.push(`<li>${inline(liText)}</li>`);
      i++;
      continue;
    }
    if (line.trim() === "") {
      closeList();
      i++;
      continue;
    }
    closeList();
    // accumulate a paragraph
    const para: string[] = [line];
    i++;
    while (
      i < lines.length &&
      lines[i].trim() !== "" &&
      !/^(#{1,4})\s|^\s*[-*]\s|^\s*\d+\.\s|^```|^\s*\|.*\|\s*$/.test(lines[i])
    ) {
      para.push(lines[i]);
      i++;
    }
    out.push(`<p>${inline(para.join(" "))}</p>`);
  }
  closeList();
  return out.join("\n");
}

// ---------------------------------------------------------------------------
// Default chat panel (used when the app doesn't build its own UI).
// ---------------------------------------------------------------------------

/** File → base64 ImageInput (strips the data-URL prefix). */
export function fileToImageInput(file: File): Promise<ImageInput> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = String(reader.result || "");
      const comma = result.indexOf(",");
      resolve({ mimeType: file.type || "image/png", data: result.slice(comma + 1) });
    };
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(file);
  });
}

function eventSummary(ev: AgentEvent): TimelineSummary | null {
  switch (ev.type) {
    case "ready":
      return {
        label: ev.resumed ? "Session resumed" : "Session ready",
        detail: ev.capabilities?.length ? ev.capabilities.join(", ") : "Agent connection open",
        state: "done",
      };
    case "tool":
      return {
        label: ev.status === "completed" ? "Tool completed" : ev.status === "failed" ? "Tool failed" : "Tool running",
        detail: ev.name || "tool",
        state: ev.status === "completed" ? "done" : ev.status === "failed" ? "error" : "active",
      };
    case "tool_call":
      return { label: "Tool call queued", detail: ev.name, state: "active" };
    case "guardrail":
      return {
        label: ev.blocked ? "Guardrail blocked" : "Guardrail checked",
        detail: [ev.name, ev.stage, ev.reason].filter(Boolean).join(" · "),
        state: ev.blocked ? "error" : "done",
      };
    case "approval":
      return { label: "Approval needed", detail: ev.tool, state: "active" };
    case "handoff":
      return {
        label: "Agent handoff",
        detail: [ev.from, ev.to].filter(Boolean).join(" → "),
        state: "active",
      };
    case "compaction":
      return {
        label: "Context compaction",
        detail: [ev.phase, ev.trigger].filter(Boolean).join(" · "),
        state: ev.phase === "done" ? "done" : "active",
      };
    case "context":
      return {
        label: "Context checked",
        detail: ev.limit ? `${ev.used || 0}/${ev.limit} tokens (${Math.round((ev.ratio || 0) * 100)}%)` : "Token usage unavailable",
        state: "done",
      };
    case "model":
      return {
        label: ev.ok ? "Model selected" : "Model switch failed",
        detail: [ev.provider, ev.model].filter(Boolean).join(" / "),
        state: ev.ok ? "done" : "error",
      };
    case "widget":
      return { label: "Interface updated", detail: ev.id, state: "done" };
    case "done":
      return { label: "Run complete", detail: "", state: "done" };
    case "error":
      return { label: "Run error", detail: ev.message, state: "error" };
    default:
      return null;
  }
}

/** Mount a visible execution timeline for long-running agent work. Generated
 * apps should include this when they call `prompt()` / `ask()` directly; `run()`
 * and the default chat mount it automatically. */
export function mountTimeline(
  client: BioRouterClient,
  target: HTMLElement | string,
  options: TimelineOptions = {}
) {
  const host =
    typeof target === "string"
      ? (document.querySelector(target) as HTMLElement | null)
      : target;
  if (!host) return () => undefined;
  host.classList.add("br-run-status");
  const maxItems = options.maxItems || 16;
  const entries: HTMLElement[] = [];
  const add = (ev: AgentEvent) => {
    const s = eventSummary(ev);
    if (!s) return;
    const row = document.createElement("div");
    row.className = "br-run-step br-run-step--" + s.state;
    const label = document.createElement("span");
    label.className = "br-run-step__label";
    label.textContent = s.label;
    row.appendChild(label);
    if (s.detail) {
      const detail = document.createElement("span");
      detail.className = "br-run-step__detail";
      detail.textContent = s.detail;
      row.appendChild(detail);
    }
    host.appendChild(row);
    entries.push(row);
    while (entries.length > maxItems) {
      const old = entries.shift();
      if (old) old.remove();
    }
  };
  const kinds: EventKind[] = [
    "ready",
    "tool",
    "tool_call",
    "guardrail",
    "approval",
    "handoff",
    "compaction",
    "context",
    "model",
    "widget",
    "done",
    "error",
  ];
  for (const kind of kinds) client.on(kind, add);
  return () => {
    for (const kind of kinds) client.off(kind, add);
  };
}

// ── Interactive widgets ─────────────────────────────────────────────────────
// Agent-emitted UI (cards/forms/tables/charts) that can call back into the loop:
// the agent emits a `widget` frame, the app renders the tree, and a Button with
// `submit` collects the named form fields and sends a `widget_action` frame the
// server feeds back into the agent as the next turn. Dependency-free DOM,
// built only from the BioRouter theme classes so it stays on-brand and passes
// the build lint.

export type WidgetNode =
  | { t: "card"; title?: string; children: WidgetNode[] }
  | { t: "row"; children: WidgetNode[] }
  | { t: "col"; children: WidgetNode[] }
  | { t: "text"; value: string; markdown?: boolean; muted?: boolean }
  | { t: "badge"; value: string }
  | { t: "table"; columns: string[]; rows: Array<Array<string | number>> }
  | { t: "chart"; spec: unknown }
  | { t: "graph"; spec: unknown }
  | { t: "stat"; label?: string; value: string | number; unit?: string; delta?: string }
  | { t: "divider" }
  | { t: "progress"; value: number; label?: string }
  | { t: "input"; name: string; label?: string; value?: string; placeholder?: string; inputType?: string }
  | { t: "select"; name: string; label?: string; value?: string; options: Array<string | { value: string; label?: string }> }
  | { t: "checkbox"; name: string; label?: string; checked?: boolean }
  | { t: "button"; label: string; action: string; variant?: string; submit?: boolean }
  | { t: "form"; children: WidgetNode[] };

export interface WidgetContext {
  // name → live value getter, registered by inputs/selects/checkboxes.
  fields: Map<string, () => string | boolean>;
  // dispatched by a button (carrying collected form fields on submit).
  onAction: (action: string, payload: unknown) => void;
}

function wEl(tag: string, cls?: string): HTMLElement {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  return e;
}

/** Build a detached DOM subtree from a widget node. Recursive; unknown node
 *  types render a muted placeholder rather than throwing. */
export function renderWidget(node: WidgetNode, ctx: WidgetContext): HTMLElement {
  switch (node.t) {
    case "card": {
      const c = wEl("div", "br-card");
      if (node.title) {
        const h = wEl("div", "br-card__title");
        h.textContent = node.title;
        c.appendChild(h);
      }
      for (const ch of node.children) c.appendChild(renderWidget(ch, ctx));
      return c;
    }
    case "row":
    case "col": {
      const r = wEl("div", node.t === "row" ? "br-row" : "br-col");
      for (const ch of node.children) r.appendChild(renderWidget(ch, ctx));
      return r;
    }
    case "text": {
      const t = wEl("div", node.muted ? "br-text br-text--muted" : "br-text");
      if (node.markdown) t.innerHTML = renderMarkdown(node.value);
      else t.textContent = node.value;
      return t;
    }
    case "badge": {
      const b = wEl("span", "br-badge");
      b.textContent = node.value;
      return b;
    }
    case "table": {
      const tbl = wEl("table", "br-table");
      const thead = wEl("thead");
      const htr = wEl("tr");
      for (const col of node.columns) {
        const th = wEl("th");
        th.textContent = col;
        htr.appendChild(th);
      }
      thead.appendChild(htr);
      tbl.appendChild(thead);
      const tbody = wEl("tbody");
      for (const row of node.rows) {
        const tr = wEl("tr");
        for (const cell of row) {
          const td = wEl("td");
          td.textContent = String(cell);
          tr.appendChild(td);
        }
        tbody.appendChild(tr);
      }
      tbl.appendChild(tbody);
      return tbl;
    }
    case "chart": {
      const w = wEl("div", "br-chart");
      w.innerHTML = renderChart(JSON.stringify(node.spec));
      return w;
    }
    case "graph": {
      const w = wEl("div", "br-visual");
      w.innerHTML = renderGraph(JSON.stringify(node.spec));
      return w;
    }
    case "stat": {
      const s = wEl("div", "br-stat");
      if (node.label) {
        const l = wEl("div", "br-stat__label");
        l.textContent = node.label;
        s.appendChild(l);
      }
      const v = wEl("div", "br-stat__value");
      v.textContent = String(node.value);
      if (node.unit) {
        const u = wEl("span", "br-stat__unit");
        u.textContent = " " + node.unit;
        v.appendChild(u);
      }
      s.appendChild(v);
      if (node.delta) {
        // A leading "-" reads as a decrease; everything else is neutral/up.
        const down = node.delta.trim().indexOf("-") === 0;
        const d = wEl("div", down ? "br-stat__delta br-stat__delta--down" : "br-stat__delta");
        d.textContent = node.delta;
        s.appendChild(d);
      }
      return s;
    }
    case "divider":
      return wEl("hr", "br-divider");
    case "progress": {
      const p = wEl("div", "br-progress");
      if (node.label) {
        const l = wEl("div", "br-progress__label");
        l.textContent = node.label;
        p.appendChild(l);
      }
      const track = wEl("div", "br-progress__track");
      const bar = wEl("div", "br-progress__bar");
      const pct = Math.max(0, Math.min(1, Number(node.value) || 0)) * 100;
      bar.style.width = pct.toFixed(1) + "%";
      track.appendChild(bar);
      p.appendChild(track);
      return p;
    }
    case "input": {
      const wrap = wEl("label", "br-field");
      if (node.label) {
        const l = wEl("span", "br-field__label");
        l.textContent = node.label;
        wrap.appendChild(l);
      }
      const i = document.createElement("input");
      i.className = "br-input";
      i.type = node.inputType || "text";
      if (node.value) i.value = node.value;
      if (node.placeholder) i.placeholder = node.placeholder;
      ctx.fields.set(node.name, () => i.value);
      wrap.appendChild(i);
      return wrap;
    }
    case "select": {
      const wrap = wEl("label", "br-field");
      if (node.label) {
        const l = wEl("span", "br-field__label");
        l.textContent = node.label;
        wrap.appendChild(l);
      }
      const s = document.createElement("select");
      s.className = "br-select";
      for (const opt of node.options) {
        const o = document.createElement("option");
        // Accept both {value,label} objects and bare strings. The server-side
        // validator (and `ui_ask`) allow plain-string options, so a
        // string-options select must render its choices, not blank entries.
        if (typeof opt === "string") {
          o.value = opt;
          o.textContent = opt;
        } else {
          o.value = opt.value;
          o.textContent = opt.label ?? opt.value;
        }
        if (node.value === o.value) o.selected = true;
        s.appendChild(o);
      }
      ctx.fields.set(node.name, () => s.value);
      wrap.appendChild(s);
      return wrap;
    }
    case "checkbox": {
      const wrap = wEl("label", "br-check");
      const c = document.createElement("input");
      c.type = "checkbox";
      c.checked = node.checked === true;
      ctx.fields.set(node.name, () => c.checked);
      wrap.appendChild(c);
      if (node.label) {
        const l = wEl("span");
        l.textContent = node.label;
        wrap.appendChild(l);
      }
      return wrap;
    }
    case "button": {
      const b = document.createElement("button");
      b.className =
        node.variant === "secondary"
          ? "br-btn br-btn--secondary"
          : node.variant === "ghost"
            ? "br-btn br-btn--ghost"
            : "br-btn";
      b.textContent = node.label;
      const action = node.action;
      const submit = node.submit === true;
      b.addEventListener("click", () => {
        let payload: unknown;
        if (submit) {
          const collected: Record<string, string | boolean> = {};
          ctx.fields.forEach((get, name) => {
            collected[name] = get();
          });
          payload = collected;
        }
        ctx.onAction(action, payload);
      });
      return b;
    }
    case "form": {
      const f = wEl("div", "br-form");
      for (const ch of node.children) f.appendChild(renderWidget(ch, ctx));
      return f;
    }
    default: {
      const d = wEl("div", "br-msg br-msg--tool");
      d.textContent = "unsupported widget";
      return d;
    }
  }
}

// ---------------------------------------------------------------------------
// Agent-driven UI runtime
// ---------------------------------------------------------------------------
// The agent's `ui_*` tools emit `{type:"ui", cmd:…}` frames; this applies them to
// the page. Everything it creates is confined to elements it owns (`.br-dock`,
// `.br-panel`, `.br-toasts`, `.br-modal-host`) plus the CSS custom properties on
// `:root` — an app's own markup is only ever *targeted*, never rewritten, unless
// the agent explicitly names it.

export interface UiCommand {
  type?: string;
  cmd: string;
  id?: string;
  title?: string | null;
  place?: string;
  body?: WidgetNode[];
  collapsible?: boolean;
  remove?: boolean;
  target?: string;
  mode?: string;
  note?: string | null;
  scroll?: boolean;
  accent?: string | null;
  density?: string | null;
  preset?: string;
  sidebarWidth?: number | null;
  message?: string;
  level?: string;
  timeoutMs?: number;
  state?: Record<string, unknown>;
  requestId?: string;
  prompt?: string;
  submitLabel?: string;
  fields?: AskFieldSpec[];
}

export interface AskFieldSpec {
  name: string;
  label?: string;
  type?: string;
  options?: string[];
  value?: string;
  placeholder?: string;
}

// Named aliases (see the note near `Listener`): the no-esbuild fallback stripper
// keys off an uppercase/primitive leading type token, so a bare
// `(cmd: UiCommand) => void` annotation would survive into the emitted JS.
type StateListener = (state: Record<string, unknown>) => void;
type CommandListener = (cmd: UiCommand) => void;
type FieldGetters = Map<string, () => string | boolean>;

/** Where a `place` maps in the DOM. `dock` is the always-available drawer. */
const DOCK_PLACES: Record<string, string> = {
  dock: "br-dock--right",
  right: "br-dock--right",
  left: "br-dock--left",
  bottom: "br-dock--bottom",
};

export class UiRuntime {
  private client: BioRouterClient;
  /** The agent's shared state bag (mirrors `ui_state` on the server). */
  state: Record<string, unknown> = {};
  private stateListeners: StateListener[] = [];
  private commandListeners: CommandListener[] = [];
  private docks: Map<string, HTMLElement> = new Map();
  private panels: Map<string, HTMLElement> = new Map();
  private toastHost: HTMLElement | null = null;
  private modalHost: HTMLElement | null = null;
  private openAsk: string | null = null;

  constructor(client: BioRouterClient) {
    this.client = client;
  }

  /** Subscribe to state changes the agent makes (`ui_state`). */
  onState(fn: StateListener): this {
    this.stateListeners.push(fn);
    return this;
  }

  /** Observe every UI command (for logging, or to override handling). */
  onCommand(fn: CommandListener): this {
    this.commandListeners.push(fn);
    return this;
  }

  /** The author-declared regions the agent may render into. */
  regions(): string[] {
    const out: string[] = [];
    const nodes = document.querySelectorAll("[data-br-region]");
    for (let i = 0; i < nodes.length; i++) {
      const name = (nodes[i] as HTMLElement).dataset.brRegion;
      if (name) out.push(name);
    }
    return out;
  }

  /**
   * Tell the backend what this page offers, so `ui_describe` returns real
   * targets instead of the agent guessing selectors.
   */
  reportSurface(): void {
    const ids: string[] = [];
    const withId = document.querySelectorAll("[id]");
    for (let i = 0; i < withId.length && ids.length < 200; i++) {
      const id = withId[i].id;
      // Skip our own scaffolding (including the injected theme <style>) — the
      // agent addresses those via @panel:/@region:, not by id.
      if (!id || id.indexOf("br-") === 0 || id === "biorouter-theme") continue;
      ids.push(id);
    }
    this.client.sendRaw({
      type: "ui_surface",
      surface: {
        title: document.title,
        regions: this.regions(),
        ids,
        hasChat: !!document.querySelector("[data-br-chat]"),
        panels: Array.from(this.panels.keys()),
      },
    });
  }

  /** Apply one agent command. Unknown commands are ignored, not fatal. */
  apply(cmd: UiCommand): void {
    for (const fn of this.commandListeners) {
      try {
        fn(cmd);
      } catch {
        /* listener errors are non-fatal */
      }
    }
    try {
      switch (cmd.cmd) {
        case "panel":
          this.applyPanel(cmd);
          break;
        case "render":
          this.applyRender(cmd);
          break;
        case "highlight":
          this.applyHighlight(cmd);
          break;
        case "theme":
          this.applyTheme(cmd);
          break;
        case "layout":
          this.applyLayout(cmd);
          break;
        case "notify":
          this.applyNotify(cmd);
          break;
        case "state":
          this.applyState(cmd);
          break;
        case "ask":
          this.applyAsk(cmd);
          break;
        case "ask_close":
          this.closeAsk(cmd.requestId || "", false);
          break;
        default:
          break;
      }
    } catch (e) {
      // A malformed command must never take the app down.
      this.applyNotify({
        cmd: "notify",
        message: "The agent sent a UI update this app could not apply.",
        level: "warn",
      });
    }
  }

  // ── targets ──────────────────────────────────────────────────────────────

  /**
   * Resolve a tool's `target` string. `@region:x` / `@panel:x` / `@chat` /
   * `@main` are aliases; anything else is a CSS selector.
   */
  resolveTarget(target: string): HTMLElement | null {
    const t = (target || "").trim();
    if (t.indexOf("@region:") === 0) {
      const name = t.slice("@region:".length);
      return document.querySelector<HTMLElement>(
        `[data-br-region="${cssEscape(name)}"]`
      );
    }
    if (t.indexOf("@panel:") === 0) {
      const panel = this.panels.get(t.slice("@panel:".length));
      return panel ? panel.querySelector<HTMLElement>(".br-panel__body") : null;
    }
    if (t === "@chat") return document.querySelector<HTMLElement>("[data-br-chat]");
    if (t === "@main") return this.mainHost();
    return document.querySelector<HTMLElement>(t);
  }

  private mainHost(): HTMLElement {
    return (
      document.querySelector<HTMLElement>("[data-br-main]") ||
      document.querySelector<HTMLElement>("main") ||
      document.querySelector<HTMLElement>(".br-container") ||
      document.body
    );
  }

  private dock(place: string): HTMLElement {
    const cls = DOCK_PLACES[place] || DOCK_PLACES.dock;
    let el = this.docks.get(cls);
    if (el) return el;
    el = document.createElement("aside");
    el.className = "br-dock " + cls;
    el.setAttribute("data-br-dock", place);
    document.body.appendChild(el);
    this.docks.set(cls, el);
    document.body.classList.add("br-has-" + cls.replace("br-dock--", "dock-"));
    return el;
  }

  private syncDockVisibility(): void {
    this.docks.forEach((el, cls) => {
      const bodyCls = "br-has-" + cls.replace("br-dock--", "dock-");
      document.body.classList.toggle(bodyCls, el.childElementCount > 0);
    });
  }

  // ── commands ─────────────────────────────────────────────────────────────

  private applyPanel(cmd: UiCommand): void {
    const id = cmd.id || "";
    if (!id) return;
    if (cmd.remove) {
      const existing = this.panels.get(id);
      if (existing) existing.remove();
      this.panels.delete(id);
      this.syncDockVisibility();
      return;
    }

    const place = cmd.place || "dock";
    const panel = document.createElement("section");
    panel.className = "br-panel";
    panel.setAttribute("data-br-panel", id);

    if (cmd.title) {
      const head = document.createElement("header");
      head.className = "br-panel__head";
      const h = document.createElement("h3");
      h.className = "br-panel__title";
      h.textContent = cmd.title;
      head.appendChild(h);
      if (cmd.collapsible !== false) {
        const toggle = document.createElement("button");
        toggle.className = "br-panel__toggle";
        toggle.type = "button";
        toggle.setAttribute("aria-label", "Collapse panel");
        toggle.textContent = "–";
        toggle.addEventListener("click", () => {
          const collapsed = panel.classList.toggle("br-panel--collapsed");
          toggle.textContent = collapsed ? "+" : "–";
        });
        head.appendChild(toggle);
      }
      panel.appendChild(head);
    }

    const body = document.createElement("div");
    body.className = "br-panel__body";
    this.renderInto(body, cmd.body || [], id);
    panel.appendChild(body);

    // Replace in place so a refreshed dashboard doesn't jump to the bottom.
    const prev = this.panels.get(id);
    if (prev && prev.parentElement) {
      prev.parentElement.replaceChild(panel, prev);
    } else if (place === "modal") {
      this.modal().appendChild(panel);
    } else if (place === "main") {
      this.mainHost().appendChild(panel);
    } else {
      this.dock(place).appendChild(panel);
    }
    this.panels.set(id, panel);
    this.syncDockVisibility();
  }

  private applyRender(cmd: UiCommand): void {
    const host = this.resolveTarget(cmd.target || "");
    if (!host) {
      // Say so out loud: a silently-dropped render looks like a broken agent.
      this.applyNotify({
        cmd: "notify",
        message: `The agent tried to render into "${cmd.target}", which this app does not have.`,
        level: "warn",
      });
      return;
    }
    if (cmd.mode !== "append") host.innerHTML = "";
    this.renderInto(host, cmd.body || [], cmd.target || "render", cmd.mode === "append");
  }

  /** Render widget nodes, wiring their buttons back into the agent loop. */
  private renderInto(
    host: HTMLElement,
    nodes: WidgetNode[],
    widgetId: string,
    append?: boolean
  ): void {
    const ctx: WidgetContext = {
      fields: new Map(),
      onAction: (action, payload) =>
        this.client.sendRaw({ type: "widget_action", widgetId, action, payload }),
    };
    if (!append) host.innerHTML = "";
    for (const node of nodes) {
      host.appendChild(renderWidget(node, ctx));
    }
  }

  private applyHighlight(cmd: UiCommand): void {
    if (cmd.mode === "clear") {
      this.clearHighlights();
      return;
    }
    this.clearHighlights();
    const el = this.resolveTarget(cmd.target || "");
    if (!el) {
      // The tool reported success, so tell the user something happened rather
      // than leaving them staring at an unchanged page.
      this.applyNotify({
        cmd: "notify",
        message: `The agent tried to highlight "${cmd.target}", which this app does not have.`,
        level: "warn",
      });
      return;
    }
    el.classList.add("br-highlight", "br-highlight--" + (cmd.mode || "outline"));
    // Focus mode dims via a fixed backdrop the target is raised above — NOT via
    // ancestor opacity, which would drag the nested target down with it.
    if (cmd.mode === "focus") this.focusBackdrop();
    if (cmd.note) {
      const note = document.createElement("div");
      note.className = "br-callout";
      note.setAttribute("data-br-callout", "1");
      note.textContent = cmd.note;
      el.insertAdjacentElement("afterend", note);
    }
    if (cmd.scroll !== false && typeof el.scrollIntoView === "function") {
      el.scrollIntoView({ behavior: "smooth", block: "center" });
    }
  }

  private focusBackdrop(): HTMLElement {
    let bd = document.querySelector<HTMLElement>(".br-focus-backdrop");
    if (!bd) {
      bd = document.createElement("div");
      bd.className = "br-focus-backdrop";
      document.body.appendChild(bd);
    }
    return bd;
  }

  private clearHighlights(): void {
    const marked = document.querySelectorAll(".br-highlight");
    for (let i = 0; i < marked.length; i++) {
      marked[i].classList.remove(
        "br-highlight",
        "br-highlight--outline",
        "br-highlight--pulse",
        "br-highlight--focus"
      );
    }
    const notes = document.querySelectorAll("[data-br-callout]");
    for (let i = 0; i < notes.length; i++) notes[i].remove();
    const bd = document.querySelector(".br-focus-backdrop");
    if (bd) bd.remove();
  }

  private applyTheme(cmd: UiCommand): void {
    const root = document.documentElement;
    if (cmd.accent) root.style.setProperty("--br-accent", cmd.accent);
    if (cmd.mode) {
      if (cmd.mode === "auto") root.removeAttribute("data-br-theme");
      else root.setAttribute("data-br-theme", cmd.mode);
    }
    if (cmd.density) root.setAttribute("data-br-density", cmd.density);
  }

  private applyLayout(cmd: UiCommand): void {
    const preset = cmd.preset || "single";
    document.body.setAttribute("data-br-layout", preset);

    // Dock width is a single custom property so an explicit `sidebarWidth`
    // always wins over the preset's default (a CSS rule per preset would beat
    // the inline var and silently ignore the caller).
    const root = document.documentElement;
    const presetWidth: Record<string, string> = {
      dashboard: "min(62vw, 900px)",
      split: "50vw",
    };
    if (cmd.sidebarWidth) {
      root.style.setProperty(
        "--br-dock-w",
        Math.max(200, Math.min(1200, cmd.sidebarWidth)) + "px"
      );
    } else if (presetWidth[preset]) {
      root.style.setProperty("--br-dock-w", presetWidth[preset]);
    } else {
      root.style.removeProperty("--br-dock-w"); // back to the stylesheet default
    }

    // "sidebar-left" has to actually move the panels, or the preset is a lie.
    const side = preset === "sidebar-left" ? "left" : "right";
    const from = this.docks.get(DOCK_PLACES[side === "left" ? "right" : "left"]);
    if (from && from.childElementCount) {
      const to = this.dock(side);
      while (from.firstElementChild) to.appendChild(from.firstElementChild);
    }

    // The dashboard preset lays the dock out as a grid rather than a column.
    for (const [, el] of this.docks) {
      el.classList.toggle("br-dock--grid", preset === "dashboard");
    }
    this.syncDockVisibility();
  }

  private toasts(): HTMLElement {
    if (this.toastHost && this.toastHost.isConnected) return this.toastHost;
    const el = document.createElement("div");
    el.className = "br-toasts";
    document.body.appendChild(el);
    this.toastHost = el;
    return el;
  }

  private applyNotify(cmd: UiCommand): void {
    const t = document.createElement("div");
    t.className = "br-toast br-toast--" + (cmd.level || "info");
    t.setAttribute("role", "status");
    t.textContent = cmd.message || "";
    this.toasts().appendChild(t);
    const ms = cmd.timeoutMs === undefined ? 4000 : cmd.timeoutMs;
    if (ms > 0) window.setTimeout(() => t.remove(), ms);
    else {
      t.classList.add("br-toast--sticky");
      t.addEventListener("click", () => t.remove());
    }
  }

  private applyState(cmd: UiCommand): void {
    this.state = cmd.state || {};
    for (const fn of this.stateListeners) {
      try {
        fn(this.state);
      } catch {
        /* listener errors are non-fatal */
      }
    }
  }

  private modal(): HTMLElement {
    if (this.modalHost && this.modalHost.isConnected) return this.modalHost;
    const el = document.createElement("div");
    el.className = "br-modal-host";
    document.body.appendChild(el);
    this.modalHost = el;
    return el;
  }

  /**
   * Render a blocking question from `ui_ask`. The tool call on the server is
   * parked until we send `ui_reply` — so *every* exit path from this form must
   * send one, including dismissal.
   */
  private applyAsk(cmd: UiCommand): void {
    const requestId = cmd.requestId || "";
    if (!requestId) return;
    // Only one question at a time; a second supersedes (and cancels) the first.
    if (this.openAsk) this.closeAsk(this.openAsk, true);
    this.openAsk = requestId;

    const host = this.modal();
    const backdrop = document.createElement("div");
    backdrop.className = "br-modal";
    backdrop.setAttribute("data-br-ask", requestId);

    const card = document.createElement("div");
    card.className = "br-modal__card br-card";
    card.setAttribute("role", "dialog");
    card.setAttribute("aria-modal", "true");

    if (cmd.title) {
      const h = document.createElement("h3");
      h.className = "br-card__title";
      h.textContent = cmd.title;
      card.appendChild(h);
    }
    if (cmd.prompt) {
      const p = document.createElement("div");
      p.className = "br-text";
      p.innerHTML = renderMarkdown(cmd.prompt);
      card.appendChild(p);
    }

    const getters: FieldGetters = new Map();
    const form = document.createElement("div");
    form.className = "br-form";
    for (const f of cmd.fields || []) {
      form.appendChild(buildAskField(f, getters));
    }
    card.appendChild(form);

    const actions = document.createElement("div");
    actions.className = "br-modal__actions";
    const cancel = document.createElement("button");
    cancel.className = "br-btn br-btn--ghost";
    cancel.type = "button";
    cancel.textContent = "Skip";
    cancel.addEventListener("click", () => this.closeAsk(requestId, true));
    const submit = document.createElement("button");
    submit.className = "br-btn";
    submit.type = "button";
    submit.textContent = cmd.submitLabel || "Submit";
    submit.addEventListener("click", () => {
      const payload: Record<string, string | boolean> = {};
      getters.forEach((get, name) => {
        payload[name] = get();
      });
      this.client.sendRaw({ type: "ui_reply", requestId, payload });
      this.dismissAsk(requestId);
    });
    actions.appendChild(cancel);
    actions.appendChild(submit);
    card.appendChild(actions);

    backdrop.appendChild(card);
    host.appendChild(backdrop);

    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") this.closeAsk(requestId, true);
    };
    document.addEventListener("keydown", onKey);
    backdrop.addEventListener("br:teardown", () =>
      document.removeEventListener("keydown", onKey)
    );

    const first = card.querySelector<HTMLElement>("input, select, textarea");
    if (first) first.focus();
  }

  /** Remove the form. When `answer` is true, tell the parked tool it was skipped. */
  private closeAsk(requestId: string, answer: boolean): void {
    if (answer) {
      this.client.sendRaw({
        type: "ui_reply",
        requestId,
        payload: { cancelled: true },
      });
    }
    this.dismissAsk(requestId);
  }

  private dismissAsk(requestId: string): void {
    const el = document.querySelector(`[data-br-ask="${cssEscape(requestId)}"]`);
    if (el) {
      el.dispatchEvent(new CustomEvent("br:teardown"));
      el.remove();
    }
    if (this.openAsk === requestId) this.openAsk = null;
  }
}

/** Minimal `CSS.escape` shim for the attribute selectors we build. */
function cssEscape(value: string): string {
  if (window.CSS && typeof window.CSS.escape === "function") {
    return window.CSS.escape(value);
  }
  return String(value).replace(/["\\\]]/g, "\\$&");
}

function buildAskField(f: AskFieldSpec, getters: FieldGetters): HTMLElement {
  const kind = f.type || "text";
  if (kind === "checkbox") {
    const wrap = wEl("label", "br-check");
    const c = document.createElement("input");
    c.type = "checkbox";
    c.checked = f.value === "true";
    getters.set(f.name, () => c.checked);
    wrap.appendChild(c);
    const l = wEl("span");
    l.textContent = f.label || f.name;
    wrap.appendChild(l);
    return wrap;
  }
  const wrap = wEl("label", "br-field");
  const label = wEl("span", "br-field__label");
  label.textContent = f.label || f.name;
  wrap.appendChild(label);
  if (kind === "select") {
    const s = document.createElement("select");
    s.className = "br-select";
    for (const opt of f.options || []) {
      const o = document.createElement("option");
      o.value = opt;
      o.textContent = opt;
      if (f.value === opt) o.selected = true;
      s.appendChild(o);
    }
    getters.set(f.name, () => s.value);
    wrap.appendChild(s);
    return wrap;
  }
  if (kind === "textarea") {
    const ta = document.createElement("textarea");
    ta.className = "br-textarea";
    if (f.value) ta.value = f.value;
    if (f.placeholder) ta.placeholder = f.placeholder;
    getters.set(f.name, () => ta.value);
    wrap.appendChild(ta);
    return wrap;
  }
  const i = document.createElement("input");
  i.className = "br-input";
  i.type = kind === "number" ? "number" : "text";
  if (f.value) i.value = f.value;
  if (f.placeholder) i.placeholder = f.placeholder;
  getters.set(f.name, () => i.value);
  wrap.appendChild(i);
  return wrap;
}

export function mountChat(client: BioRouterClient, host: HTMLElement): void {
  host.classList.add("br-chat");
  const log = document.createElement("div");
  log.className = "br-chat__log";
  const form = document.createElement("div");
  form.className = "br-chat__form";
  const status = document.createElement("div");
  status.className = "br-run-status";
  const input = document.createElement("input");
  input.className = "br-input";
  input.placeholder = "Ask the agent…";
  const send = document.createElement("button");
  send.className = "br-btn";
  send.type = "button";
  send.textContent = "Send";
  form.appendChild(input);
  form.appendChild(send);
  host.appendChild(log);
  host.appendChild(status);
  host.appendChild(form);
  mountTimeline(client, status, { maxItems: 12 });

  const add = (cls: string, html: boolean, content: string): HTMLElement => {
    const el = document.createElement("div");
    el.className = "br-msg br-msg--" + cls;
    if (html) el.innerHTML = content;
    else el.textContent = content;
    log.appendChild(el);
    log.scrollTop = log.scrollHeight;
    return el;
  };
  if (client.config.greeting) add("agent", false, client.config.greeting);

  let current: HTMLElement | null = null;
  let buffer = "";
  let typing: HTMLElement | null = null;
  const clearTyping = () => {
    if (typing) {
      typing.remove();
      typing = null;
    }
  };
  const showTyping = () => {
    clearTyping();
    typing = document.createElement("div");
    typing.className = "br-msg br-msg--agent";
    typing.innerHTML =
      '<span class="br-typing"><span></span><span></span><span></span></span>';
    log.appendChild(typing);
    log.scrollTop = log.scrollHeight;
  };

  client.on("message", (ev) => {
    if (ev.type !== "message") return;
    clearTyping();
    if (!current) {
      current = add("agent", true, "");
      buffer = "";
    }
    buffer += ev.delta;
    current.innerHTML = client.renderMarkdown(buffer);
    log.scrollTop = log.scrollHeight;
  });
  client.on("tool", (ev) => {
    if (ev.type === "tool") add("tool", false, ev.name + " (" + ev.status + ")");
  });
  client.on("error", () => {
    clearTyping();
    add("agent", false, "Could not reach the agent.");
  });

  const submit = async () => {
    const text = input.value.trim();
    if (!text) return;
    add("user", false, text);
    input.value = "";
    current = null;
    showTyping();
    try {
      await client.prompt(text);
    } catch (e) {
      clearTyping();
      // Show why. "Failed to get a response" told the user nothing actionable —
      // the message from `dial()` names the endpoints tried and the fix.
      add("agent", false, (e as Error).message || "Failed to get a response.");
    }
  };
  send.addEventListener("click", submit);
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      submit();
    }
  });
}

/**
 * Create (and globally register) the app client from
 * `window.BIOROUTER_APP_CONFIG`. Auto-mounts a chat panel when requested.
 */
export function createApp(overrides: Partial<AppConfig> = {}): BioRouterClient {
  const cfg: AppConfig = {
    appId: "app",
    autoChat: true,
    ui: true,
    ...(window.BIOROUTER_APP_CONFIG || {}),
    ...overrides,
  };
  const client = new BioRouterClient(cfg);
  window.BioRouter = client;

  // Connect eagerly so agent-driven UI works in apps that never call prompt()
  // (a dashboard the agent populates on load), and so a dead backend surfaces
  // immediately as a banner instead of on the user's first click.
  const start = () => {
    if (cfg.autoChat) {
      const host = document.querySelector<HTMLElement>("[data-br-chat]");
      if (host && !host.dataset.brMounted) {
        host.dataset.brMounted = "1";
        mountChat(client, host);
      }
    }
    client.connect().catch((e: Error) => mountBackendError(e.message));
  };
  if (document.readyState === "loading")
    document.addEventListener("DOMContentLoaded", start);
  else start();
  return client;
}

/**
 * A visible, actionable banner when no BioRouter daemon could be reached. This
 * is the failure an exported app used to show as a bare console error, leaving
 * the user with a UI that silently did nothing.
 */
export function mountBackendError(message: string): void {
  if (document.querySelector("[data-br-backend-error]")) return;
  const bar = document.createElement("div");
  bar.className = "br-backend-error";
  bar.setAttribute("data-br-backend-error", "1");
  bar.setAttribute("role", "alert");
  const title = document.createElement("strong");
  title.textContent = "No BioRouter backend";
  const body = document.createElement("div");
  body.textContent = message;
  bar.appendChild(title);
  bar.appendChild(body);
  document.body.insertBefore(bar, document.body.firstChild);
}
