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
   * page location: `ws[s]://<host>/apps/<appId>/agent`. Standalone exports set
   * this to the daemon they bundle/launch.
   */
  endpoint?: string;
  /** Greeting shown when the default chat panel mounts. */
  greeting?: string;
  /** Auto-mount a chat panel into `[data-br-chat]` if the app has no custom UI. */
  autoChat?: boolean;
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

/** Events emitted while the agent answers a prompt. */
export type AgentEvent =
  | { type: "ready"; protocol?: number; capabilities?: string[]; sessionId?: string }
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
  | { type: "widget"; id: string; tree: unknown };

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

function resolveEndpoint(cfg: AppConfig): string {
  const cid = encodeURIComponent(getClientId(cfg.appId));
  let base: string;
  if (cfg.endpoint) {
    base = cfg.endpoint;
  } else {
    const loc = window.location;
    const proto = loc.protocol === "https:" ? "wss:" : "ws:";
    // Served by biorouterd at /apps/<id>/ — the agent socket is a sibling.
    base = `${proto}//${loc.host}/apps/${cfg.appId}/agent`;
  }
  const sep = base.indexOf("?") >= 0 ? "&" : "?";
  return `${base}${sep}client_id=${cid}`;
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

  constructor(config: AppConfig) {
    this.config = config;
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

  /** Open (or reuse) the WebSocket to the BioRouter agent backend. */
  connect(): Promise<void> {
    if (this.readyPromise) return this.readyPromise;
    this.readyPromise = new Promise((resolve, reject) => {
      let opened = false;
      try {
        this.ws = new WebSocket(resolveEndpoint(this.config));
      } catch (e) {
        reject(e as Error);
        return;
      }
      this.ws.onopen = () => {
        opened = true;
        resolve();
      };
      this.ws.onerror = (e) => {
        if (!opened) reject(new Error("Could not reach the BioRouter backend."));
        this.emit({ type: "error", message: "connection error" });
        this.settleActive(new Error("connection error"));
      };
      this.ws.onclose = () => {
        this.readyPromise = null;
        this.ws = null;
        this.settleActive(new Error("connection closed"));
      };
      this.ws.onmessage = (ev) => this.handleFrame(ev.data);
    });
    return this.readyPromise;
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

  /** Provider/model catalog the user has available (the provider-agnostic
   *  headline). Returns `[]` on failure. */
  async listModels(): Promise<unknown[]> {
    try {
      const loc = window.location;
      const base = `${loc.protocol}//${loc.host}/apps/${encodeURIComponent(this.config.appId)}`;
      const res = await fetch(`${base}/models`);
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

  /** Send a raw client frame if the socket is open; returns whether it went. */
  private send(frame: unknown): boolean {
    if (this.ws && this.ws.readyState === 1 /* WebSocket.OPEN */) {
      this.ws.send(JSON.stringify(frame));
      return true;
    }
    return false;
  }

  /** Approve a pending human-in-the-loop tool request (BRSDK Phase 5). */
  approve(id: string): void {
    this.send({ type: "approve", id });
  }

  /** Reject a pending human-in-the-loop tool request, with an optional reason. */
  reject(id: string, reason?: string): void {
    this.send({ type: "reject", id, reason });
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
    el.innerHTML = '<span class="br-spinner"></span>';
    const onMsg: Listener = (ev) => {
      if (ev.type === "message") {
        buf += ev.delta;
        el.innerHTML = this.renderMarkdown(buf);
      }
    };
    const onTool: Listener = (ev) => {
      if (ev.type === "tool" && !buf) {
        el.innerHTML = `<span class="br-spinner"></span> <span class="br-msg--tool">⚙ ${ev.name}…</span>`;
      }
    };
    this.on("message", onMsg);
    this.on("tool", onTool);
    try {
      await this.prompt(text, opts);
      if (!buf) el.innerHTML = "";
    } catch (e) {
      // Hoisted out of the template literal so the no-esbuild fallback stripper
      // (which treats `${…}` as part of the string) can strip the `as` cast.
      const emsg = (e as Error).message || "request failed";
      el.innerHTML = `<div class="br-msg br-msg--agent">⚠ ${emsg}</div>`;
      throw e;
    } finally {
      this.off("message", onMsg);
      this.off("tool", onTool);
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
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

function inline(s: string): string {
  // links, bold, italic, inline code — operating on already-escaped text.
  return s
    .replace(/`([^`]+)`/g, (_m, c) => `<code>${c}</code>`)
    .replace(
      /\[([^\]]+)\]\((https?:[^)]+)\)/g,
      (_m, t, u) => `<a href="${u}" target="_blank" rel="noopener">${t}</a>`
    )
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/(^|[^*])\*([^*]+)\*/g, "$1<em>$2</em>");
}

interface ChartPoint {
  label: string;
  value: number;
}
interface ChartSpec {
  type?: "bar" | "line";
  title?: string;
  data: ChartPoint[];
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
    const fence = line.match(/^```(\w*)\s*$/);
    if (fence) {
      closeList();
      const code: string[] = [];
      i++;
      while (i < lines.length && !/^```\s*$/.test(lines[i])) {
        code.push(lines[i]);
        i++;
      }
      i++; // skip closing fence
      // A ```chart block is an AI-generated visualization: render it as SVG.
      if (fence[1] === "chart") {
        out.push(renderChart(code.join("\n")));
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
  | { t: "input"; name: string; label?: string; value?: string; placeholder?: string; inputType?: string }
  | { t: "select"; name: string; label?: string; value?: string; options: Array<{ value: string; label: string }> }
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
        o.value = opt.value;
        o.textContent = opt.label;
        if (node.value === opt.value) o.selected = true;
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

export function mountChat(client: BioRouterClient, host: HTMLElement): void {
  host.classList.add("br-chat");
  const log = document.createElement("div");
  log.className = "br-chat__log";
  const form = document.createElement("div");
  form.className = "br-chat__form";
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
  host.appendChild(form);

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
    if (ev.type === "tool") add("tool", false, "⚙ " + ev.name + " (" + ev.status + ")");
  });
  client.on("error", () => {
    clearTyping();
    add("agent", false, "⚠ Could not reach the agent.");
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
    } catch {
      clearTyping();
      add("agent", false, "⚠ Failed to get a response.");
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
    ...(window.BIOROUTER_APP_CONFIG || {}),
    ...overrides,
  };
  const client = new BioRouterClient(cfg);
  window.BioRouter = client;
  if (cfg.autoChat) {
    const mount = () => {
      const host = document.querySelector<HTMLElement>("[data-br-chat]");
      if (host && !host.dataset.brMounted) {
        host.dataset.brMounted = "1";
        mountChat(client, host);
      }
    };
    if (document.readyState === "loading")
      document.addEventListener("DOMContentLoaded", mount);
    else mount();
  }
  return client;
}
