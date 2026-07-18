# BioRouter Apps SDK v2 — Design

**Date:** 2026-07-12
**Status:** Draft for review (adversarially reviewed: feasibility vs code, security, completeness — findings incorporated)
**Scope:** Evolve Agent Drafter from "apps that are mostly chatbots with a run button" into a true application SDK: apps that expose the *whole* BioRouter platform (any provider, knowledge bases, MCP extensions, skills) through diverse, agent-driven, aesthetically distinct, secure user interfaces — including BioOKF-Studio-class experiences (live network viewers, avatar/scene control, workbenches) that today cannot be expressed at all.

---

## 1. Vision

BioRouter's strength is integration: 43+ providers (institutional, local, commercial), personal knowledge bases with git history and graph derivation, pluggable MCP extensions, and ~85 skills — all behind one agent loop. Agent Drafter already wires a *real* per-app agent to a generated front-end. But the front-ends it produces are almost all the same shape: controls → `buildPrompt()` → one English string → markdown streamed into one `<div>`.

The exemplar of what we want instead is **BioOKF Studio**: a network-viewer application where the agent *drives* the GUI through a small semantic API (`selectNode`, `search`, `narrate`, `getState`), the user sees an ambient "AI agent is doing X" presence, selection stays synchronized across canvas/panels/citations, and the picture encodes domain meaning (negated edges are red and struck-through; uncurated entities look tentative). Nothing about that app is a chatbot — and nothing about it is expressible in today's Agent Drafter SDK.

**SDK v2's contract:** an app is a typed, three-party contract between the *author-agent* (writes the app once), the *runtime-agent* (drives it live, turn by turn), and the *user* (owns attention and data) — with the BioRouter platform (models, knowledge, extensions, skills) available as first-class, capability-gated APIs on both sides.

## 2. What the research established

Full reports live in the session archive; the load-bearing conclusions:

### 2.1 The architectural families for LLM→UI (industry survey)

| Family | Examples | Who makes the UI | Strength | Weakness |
|---|---|---|---|---|
| **A. Model writes code, host sandboxes it** | Claude Artifacts, Websim, tldraw make-real | Model, per prompt | Unlimited novelty | Fragile, weak bidirectionality, weak models fail |
| **B. Pre-authored components; model fills slots** | OpenAI Apps SDK, MCP-UI/MCP Apps, Vercel AI SDK | Human author; model emits tool calls | Most robust for weak models; standardized postMessage/intent bridge | Model can't invent UI |
| **C. Model emits declarative UI DSL from a catalog** | Google A2UI, Flutter GenUI, Thesys C1 | Model, per turn, constrained to catalog | Safe by construction; novel compositions | Bounded by catalog |
| **D. App owns UI; agent streamed in via events + shared state** | AG-UI/CopilotKit | App author; agent drives | Strongest bidirectionality (JSON-Patch shared state, HITL) | App must be pre-built for it |

**The "author once, drive live" requirement is a hybrid: A at author time, C+D at runtime, over B's transport discipline.** Agent Drafter v1 already has this skeleton (authored TS bundle + `ui_*` frames + blocking `ui_ask` + rebindable bridge) — the survey validates the shape and tells us exactly which pieces are missing: a real component **catalog** with **flat ID-keyed nodes**, **shared reactive state** (snapshot + RFC-6902 patches + data binding), **typed bidirectional RPC**, **version stamping with unknown-component fallback** (Airbnb's hardest-won lesson), and **morphing instead of innerHTML-replace** (Phoenix LiveView/Idiomorph).

### 2.2 What BioOKF Studio teaches (the exemplar)

1. **A tiny semantic verb surface beats a generic bridge.** The whole agent contract is ~6 domain verbs plus `getState()`/`getGraph()`. The agent speaks the *app's* language, never the DOM's.
2. **Structured state reads beat screenshots.** Tool descriptions actively steer the model to `getState()`; screenshots are demoted to "visual inspection only."
3. **Ambient narration + observe-don't-hijack.** Agent actions flash a banner; user actions don't. The agent's KB focus shows as a pulsing marker without stealing the user's view.
4. **Selection synchronized across representations** (canvas ↔ detail panel ↔ citations ↔ raw source) is what makes it feel like an application.
5. **Domain-honest visual encoding** (epistemic status rendered, not just topology).
6. Its weaknesses are equally instructive: an `execute_js` eval bridge, a hand-maintained state contract, a world-readable fixed socket, polling, one-way communication, and a fully bespoke renderer that generalizes to nothing. **v2 must deliver these UX patterns through declared, typed, capability-gated primitives instead.**

### 2.3 Why today's generated apps stay chatbot-shaped (code-level diagnosis)

From the deep-read of `agent_drafter/` and ~110 real generated apps:

1. **The zero-effort path is a chat box** — default `main.ts` is `createApp()`; default `index.html` ships a `data-br-chat` card.
2. **Single-prompt, string-only ingress** — authored UI reaches the agent only as one English string (`br.run(buildPrompt(), "#out")`); every app hand-flattens sliders/tabs/chips into prose.
3. **Markdown-first egress** — the agent's answer is markdown streamed into one div; "rich output" = fenced ```chart/```graph blocks re-parsed into capped SVG (3 chart types; 18-node graphs).
4. **Closed 16-widget enum**, form-shaped (card/table/stat/input/button…), no custom components, no free (sanitized) HTML.
5. **No shared reactive state** — `ui_state` is a flat, agent-owned bag; author code can't write it; nothing binds to it.
6. **Replace-only rendering** — `ui_render`/`ui_panel` do `innerHTML = ""`; no patching, no morphing, focus/scroll/input state destroyed.
7. **No canvas / animation / scene / frame loop** — literally no `canvas`, `requestAnimationFrame`, or WebGL path in the SDK.
8. **No generic event subscription** — the agent cannot observe a node click, slider drag, or selection; only `widget_action` (which arrives as *synthetic prose*: `"[widget:id] The user submitted…"`, and is itself the turn trigger) and author-initiated prompts.
9. **The `output{schema,value}` frame is declared in `sdk.ts` but has no server producer** — structured results can't reach author code; everything is re-parsed from markdown.
10. **Serialized `br.run` chain** — a dragged slider queues N full agent turns; no debounce/supersede.
11. **Theme rigidity** — forced light, one accent, one token set; 5 fixed layout presets.

### 2.4 Interaction-design principles (research frontier)

- **NL bootstraps; a synthesized persistent control refines** (DynaVis — the single most user-validated GenUI pattern).
- **Externalize agent output as navigable structure** (graph/space/tree), not transcript (Graphologue/Sensecape).
- **Small, independently-owned, reactive units** with visible dependencies keep users oriented when the AI mutates the UI (Marimo; Ink & Switch "tools, not apps").
- **Semantic action channel over pixels** when you own the app (computer-use lesson; MCP-UI intents).
- **Skill library over per-turn improvisation** (Voyager): named, reusable capabilities compound.
- **Townie's engineering lessons:** shape APIs the way LLMs expect; ~dozens of curated examples beat maximalist prompts; error-feedback loops need real infrastructure; **evals before features**.

## 3. Architecture

### 3.0 One diagram

```
                        ┌────────────────────────────────────────────┐
                        │  BioRouter platform                        │
                        │  providers · knowledge · extensions ·      │
                        │  skills · autovisualiser · workflows       │
                        └───────────────┬────────────────────────────┘
                                        │ per-app agent (session, capabilities)
              ┌─────────────────────────┴──────────────────────────┐
              │ biorouterd  /apps/<id>/agent  (split WS)           │
              │  · agent events (message/thought/tool/output)      │
              │  · ui frames (catalog ops, state patches, ask)     │
              │  · app frames (signals, actions, state writes,     │
              │                ui_reply, typed calls)              │
              └─────────────────────────┬──────────────────────────┘
                                        │ versioned, typed frames
   ┌────────────────────────────────────┴─────────────────────────────────┐
   │ App (authored once by the author-agent, served at /apps/<id>/)      │
   │  index.html + main.ts  ←  SDK v2 runtime (sdk.ts)                    │
   │  · declared: regions, actions, signals, state schema, components    │
   │  · shared state doc (snapshot + JSON-Patch, data-br-bind)           │
   │  · catalog renderer (built-ins + science pack + custom components,  │
   │    morphing, ID-keyed)                                              │
   │  · presence layer (narration chip, highlights, ask modals)          │
   └──────────────────────────────────────────────────────────────────────┘
```

Nine pillars, each independently shippable, ordered by leverage.

### 3.1 Pillar 1 — The App Contract: typed surface, both directions

**The manifest grows a declared surface** (all optional; absent = v1 behavior). Naming note: the existing `Capabilities.events` field (`manifest.rs:66-69`) is the *agent-lifecycle* stream flowing **to** the app via `br.on()` (`tool`, `handoff`, `compaction`, …, advertised as `event:<name>` tokens). The new app→agent notifications are therefore named **signals** to avoid colliding with it; the two channels stay independent.

```jsonc
// manifest.json (additions)
{
  "surface": {
    "state_schema": { /* JSON Schema for the shared state document */ },
    "actions": [        // app-defined verbs the AGENT may call
      { "name": "move_avatar", "description": "Move the avatar",
        "params": { "type":"object", "properties": { "direction": {"enum":["up","down","left","right"]}, "steps": {"type":"integer","minimum":1,"maximum":20} } } },
      { "name": "focus_node", "description": "Center and select a graph node", "params": { /* … */ } }
    ],
    "signals": [        // app notifications the agent may SUBSCRIBE to
      { "name": "node_selected", "payload": { /* JSON Schema */ }, "coalesce_ms": 250 }
    ],
    "components": [     // custom catalog components (see Pillar 3)
      { "name": "pathway_map", "props": { /* JSON Schema */ } }
    ]
  }
}
```

Author code registers implementations; the SDK enforces that registrations match declarations at build/lint time:

```ts
// main.ts
const app = createApp({ autoChat: false });
app.actions.register("move_avatar", async ({ direction, steps }) => {
  world.move(direction, steps);            // author's own logic — any JS they wrote
  return { position: world.position };     // typed result returned INTO the tool call
});
app.signals.emit("node_selected", { id, type });   // → agent, if subscribed
```

**Agent side:** `AppControlServer` exposes two new tools generated from the manifest (which means `AppControlServer` must now be constructed with the manifest surface — today it receives only `UiBridge` + `UiCapability`):

- `app_call { action, args }` — validated against the declared schema, forwarded as an `{type:"app_call", callId, action, args}` frame; the author's handler result resolves the tool call (same oneshot parking as `ui_ask`, same timeout discipline). *This is the avatar-control primitive:* "move the avatar up three squares" becomes `app_call{action:"move_avatar", args:{direction:"up", steps:3}}` — no prompt-parsing, no DOM.
- `ui_describe` v2 returns the **full typed surface**: regions, panels, actions (with schemas), signals, components, state schema, state version — merging manifest declarations + runtime registrations with the browser-reported surface it returns today, replacing BioOKF's hand-maintained `getState()` with a generated one.

**App side:** `br.call(name, args, {output_schema?})` lets *author code* invoke agent turns with structured arguments and receive structured results. **Mechanism (important — the provider abstraction has no response-schema channel today):** structured results are produced via the *tool path*, which already validates schemas everywhere — when `output_schema` is supplied, the server injects a synthetic `emit_result` tool whose input schema *is* the `output_schema`, instructs the model to finish by calling it, validates the call, and emits it to the app as an `output {schema, value}` frame (the frame type already declared in `sdk.ts:74`; Phase 3 adds the missing server producer). Prose fallback when the model never calls the tool. Provider-level structured output (a `response_format` channel in the `Provider` trait across 43+ providers) is explicitly **not** required for v2; it can replace the synthetic-tool mechanism later without changing the app-facing contract.

`widget_action` frames stop being flattened into synthetic prose — but they remain the **turn trigger** they are today (`routes/apps.rs:1099-1111` feeds them in as the user `Message` that starts the turn). v2 keeps that turn-start semantics and swaps the payload encoding: a minimal user-text envelope carrying the structured JSON block, so existing button-driven apps keep working while the model receives typed data.

`br.run(text, target)` remains — it's the right tool for "explain/summarize into this panel" — but gains `{debounce_ms, supersede:true}` options so slider-driven apps cancel stale turns instead of queueing one model call per pixel (replacing the strict `runChain`).

### 3.2 Pillar 2 — Shared reactive state (snapshot + patch + bindings)

Replace the flat, agent-owned `ui_state` bag with a **single shared JSON state document** per app session:

- **Both sides write.** Agent: `ui_state` (kept, now merge-into-doc) and new `ui_patch_state { patch: [RFC-6902 ops] }` (via the `json-patch` crate — a new, small workspace dependency). Author code: `br.state.set(path, value)` / `br.state.update(fn)`, which sends a `state_write` frame (new client→server frame). Writes carry a **version counter**; the server is the ordering authority and rebroadcasts accepted patches to both consumers.
- **Snapshot on (re)connect.** Extends the existing `UiBridge.attach()` replay: the bridge holds the doc + version; a reconnecting page receives one `state.snapshot`, then deltas.
- **Declarative binding — safe by construction.** Authored HTML: `<span data-br-bind="/cohort/count">` plus `data-br-bind-attr` and `data-br-bind-show`. The runtime keeps a pointer→nodes index and re-renders *only bound nodes* on patch — fine-grained reactivity in ~200 lines, no framework. **Rendering contract:** `data-br-bind` writes via `textContent` only (never `innerHTML`); `data-br-bind-attr` uses a strict attribute allowlist that excludes all `on*` handlers and validates URL schemes (`https:`/relative only) for `href`/`src`. State values are agent-writable and therefore prompt-injectable; the binding layer must be a non-executing sink.
- **Persistence.** The doc is persisted with the durable session (keyed `app:<id>:<client_id>`) using the **`ExtensionData` `store_into`/`load` pattern** (as `RunState` does — there is no general per-session document column, and we don't add one; a schema migration would require human review per `HOWTOAI.md`). Restore seeds the `UiBridge` doc *before* `attach()` replays the snapshot; the 256 KB cap is enforced before serialization.
- **Caps without schema.** `surface.state_schema` is validated server-side when present; when absent, default structural caps still apply (max depth 8, max 2,000 keys, string-value length caps) so an unschema'd app cannot become an unbounded injection/DoS path. Lint requires a `state_schema` when bindings are used.

This one pillar dissolves friction items 5, 6 (partially), and 11's worst effects: `omics-dashboard`'s tabs/chips/sliders become state paths the agent can also read and patch, instead of ad-hoc DOM classes invisible to it.

### 3.3 Pillar 3 — Catalog v2: flat, ID-keyed, morphing, extensible

**Representation change:** agent-driven UI becomes a **flat list of ID-keyed component instances** (A2UI's core lesson — LLMs patch flat lists far more reliably than they regenerate nested trees):

- `ui_render` / `ui_panel` keep working (compat), but internally normalize to catalog instances with stable IDs.
- New `ui_patch { ops: [{op:"add"|"replace"|"remove"|"set_props", id, parent?, node?/props?}] }` — incremental edits to individual components.
- The renderer **morphs** (Idiomorph-style keyed reconciliation) instead of `innerHTML=""` — focus, scroll, input state, and canvas contexts survive agent updates.
- **Version stamping + fallback:** every frame carries `catalog_version`; unknown `cmd`s are ignored (already true) and unknown component kinds render a neutral labeled placeholder instead of "unsupported widget" errors (Airbnb forward-compatibility).

**The science pack** — new built-in kinds that make biomedical apps first-class:

| Kind | What it is |
|---|---|
| `network` | The BioOKF force-graph engine, generalized: typed spec `{nodes:[{id,label,type,size?}], edges:[{source,target,kind,style?}], encoding:{type_colors?, families?, negated_kinds?}, physics?}`; zoom/pan/drag/hover/select built in; selection emits a declarable signal. Canvas-based, Barnes-Hut, viewport culling — proven at KB scale in Studio. |
| `plot` | Real interactive charts beyond bar/line/pie (scatter, area, box, heatmap axes), themed, with a `bind` prop for live data. |
| `figure` | An Auto Visualiser fragment embedded in a sandboxed iframe. **Prerequisite:** `autovisualiser::common` is a *private* module today; this needs a small public API on the autovisualiser side (e.g. `pub(crate) fn render_named_figure(tool, args) -> …` wrapping `render_fragment` + the `ASSET_SINK` task-local), plus the asset-splicing story — real wiring work, same crate so no circular dependency. Once wired, all 34 `render_*` tools (volcano, Manhattan, Kaplan-Meier, Sankey, chord, maps, Mermaid…) become available *inside apps*. |
| `table` (v2) | Virtualized (10k+ rows), sortable, filterable, selectable rows → signal. |
| `canvas` | An author-registered draw surface: the author supplies a render function; the agent supplies *data* via props/state. Gives frame loops, simulations, and avatars a sanctioned home without letting the agent write code at runtime. |
| `markdown`, `image`, `kpi`, `log` | Quality-of-life kinds apps keep hand-rolling. |
| `html` | Sanitized rich HTML, **capability-gated** (`ui.allow_html`, default off). Sanitization is **server-side in `control.rs`, fail-closed** (the frame never leaves the daemon unsanitized), with a pinned config: no `<script>`/`<style>`/`<form>`, no `on*` attributes, no `javascript:`/`data:` URLs, SVG/MathML mXSS guards — and a known-bypass regression corpus as an acceptance criterion. With this node enabled, the sanitizer is a primary XSS barrier and must be treated accordingly (see §3.7 CSP). |

**Custom components — the big unlock.** Authors register their own catalog entries:

```ts
app.components.register("pathway_map", {
  props: PathwayMapSchema,        // must match the manifest declaration
  mount(el, props, ctx) { /* author-written renderer */ },
  update(el, props, prev) { /* optional; else re-mount */ },
});
```

Declared schemas are extracted at build time into the manifest, so `control.rs` **validates agent-emitted instances server-side** exactly like built-ins. Extraction **fails closed**: a registration whose schema can't be statically extracted (dynamic/spread props) is a build error, never an accept-any schema. Authors must treat `props` as **untrusted, agent-controlled input** — lint flags `innerHTML`/URL sinks fed from props. The agent then composes `pathway_map` like any other kind. Catalog = built-ins ∪ app-specific components: Family C's safety with Family A's authorial freedom, which is precisely the hybrid the survey recommends.

### 3.4 Pillar 4 — Platform encapsulation: the whole of BioRouter behind `br.*`

This is the north star: apps as first-class consumers of everything BioRouter integrates. All of it capability-gated (deny-by-default except core `ui`), all resolved server-side so secrets/keys never enter the page.

- **`br.kb` — knowledge bases.** `search(query)` (BM25), `page(path)`, `graph()` (nodes/edges — feeds straight into the `network` component: *a BioOKF-Studio-class KB explorer becomes a ~50-line app*), `ingest(items) → streamed progress`, `history()`. **Scoped grants:** the `data.sources[kind:"knowledge"]` capability enumerates the specific KB id(s) the app may touch — never "all bases" (default: none). `ingest` requires `write:true`, which is a *separately and prominently consented* grant: a poisoned ingest persists in a git-backed KB that other sessions and agents read, so write access is a cross-session integrity decision, not a checkbox.
- **`br.model` — provider routing.** Extends the existing `list()`/`select()` with: model **status** (is llamacpp downloading? context size?), and manifest-level `agent.routes` — named model profiles (`"fast"`, `"deep"`, `"local_only"`) that `br.call`/`br.run` can select per invocation. Routes must resolve to providers the *user* has configured; apps never carry keys. **Provider-class constraint:** an app holding a sensitive data source (`omop`, `cdw`, or a confidential KB) is restricted to an allow-listed provider class (local/institutional) and cannot route that data to an external commercial provider without an explicit, per-app user consent — provider class is a capability, not a post-hoc UI label. Which model answered is additionally surfaced in the UI.
- **Extensions/MCP.** Already injected per-app; v2 adds structured tool-progress: `tool` frames gain `args_summary`, and tool results carrying `ui://` embedded resources are emitted as catalog `figure` instances (targeted at the app's declared results region, author-overridable) instead of being dropped.
- **Skills.** Per-app `skills` list becomes *enforced* scoping (today advisory), and the authoring instructions teach the author-agent to pick skills the way it picks extensions.
- **Workflows & schedules — deferred to v2.1** (see §7): `br.workflow.run(name, args)` + manifest-declared cron refresh ride the existing scheduler once the core is proven.
- **Vault** stays as-is (`{{vault:NAME}}`), already correct.

### 3.5 Pillar 5 — The interaction loop: signals, presence, mixed initiative

- **`ui_subscribe { signals: ["node_selected", …] }`** — the agent opts into declared app signals. Delivery: coalesced/debounced per declaration (`coalesce_ms`), rate-capped server-side, queued through the existing between-turns/mid-turn frame machinery (bounded by the same `MAX_QUEUED_FRAMES` discipline), and presented to the model as structured JSON. **Untrusted-data envelope:** every app→agent payload (signal payloads, `app_result` values, `br.call` outputs, `widget_action` data) is per-field size-capped and delivered inside an explicit envelope the system prompt marks as *data, not instructions*. Apps render untrusted content (KB pages, pasted documents, web results), so these payloads are indirect-prompt-injection carriers by construction; the mitigation is capability minimization (scoped KB grants, deny-by-default writes) plus the envelope — never input trust. Default delivery is **queue-only**: signals are context for the next turn, they do not start turns.
- **`autorun` — off by default, a real capability.** A declared signal may additionally be allowed to *start* a turn only when (a) the app declares it, (b) the **user** grants the `ui.allow_autorun` capability (agent cannot self-grant), and (c) budgets hold: per-minute cap, per-session turn budget, and a daily cap — autonomous turns spend the user's provider quota, which on commercial/institutional providers is real money. Autorun activity renders in the presence layer with a one-click stop.
- **Presence layer (BioOKF's banner, generalized).** The SDK renders an ambient agent-activity chip for every applied `ui_*` frame ("AI · updating cohort table ⋯"), distinguishes agent-driven from user-driven changes, and `ui_highlight` gains a `narrate` note. Observe-don't-hijack: agent updates *mark* rather than steal focus (no auto-scroll unless `scroll:true`).
- **Mixed initiative:** `ui_ask` stays the blocking primitive; new non-blocking `ui_suggest { chips: […] }` renders dismissible suggestion chips (Horvitz: easy to invoke, easy to ignore).
- **The DynaVis rule in the authoring/runtime instructions:** after fulfilling an NL request that changed a parameter, the agent should emit a *persistent bound control* for it (`ui_patch` adding a slider bound to `/plot/km`), so users refine by direct manipulation instead of re-prompting.

### 3.6 Pillar 6 — Aesthetics: themes, layout grammar, archetype starters

- **Theme packs.** Manifest `theme` becomes a token set (palette incl. dark, font stack/scale, radius, density, surface treatment) with ~6 curated presets (`clinical`, `lab-notebook`, `terminal`, `glass`, `journal`, `midnight`) plus custom tokens. Lint keeps enforcing contrast and token usage (the existing rules generalize from "the one palette" to "the active pack"). `ui_theme` can switch packs if allowed. This ends "every app is the BioRouter light theme."
- **Layout grammar.** `ui_layout { areas, sizes }` → validated grid template (bounded vocabulary, no raw CSS from the agent). **The 5 existing presets are retained as aliases** over the grammar so no v1 app that calls `ui_layout{preset}` breaks. Docks and `@region:` targets keep working.
- **Archetype starters.** The single chat-card template is why the median app is a chatbot. Replace with a starter gallery the author-agent (or `create_app{archetype}`) selects: `explorer` (network/canvas + inspector + search), `dashboard` (bound KPI grid + panels), `workbench` (data table + actions + detail), `wizard` (staged form), `canvas` (scene + controls — the avatar archetype), `chat` (today's default, now one option among six). Each starter ships wired state paths, one registered action, one subscribed signal — teaching by example (Townie: curated examples beat prompt maximalism).
- The **frontend-design skill** gets referenced from the authoring instructions for visual originality within the token system.
- **Applications tab:** the desktop `ApplicationsView` gets a light audit to surface the new diversity — archetype badge, theme-pack swatch, and a launch affordance that opens non-chat apps into their real UI.

### 3.7 Pillar 7 — Security model extensions

Every new power maps onto the existing capability lattice (deny-by-default except core `ui`):

| New surface | Gate / cap |
|---|---|
| `html` component (server-side sanitized, fail-closed) | `ui.allow_html` (default **off**); pinned sanitizer config + bypass regression corpus |
| Custom components / `canvas` | `ui.allow_components` (default **on** — author code already runs on the page; registration adds no new *execution* authority; props are validated server-side and documented as untrusted) |
| App signals → agent | `ui.allow_signals` + per-signal declaration; coalescing + server-side rate caps |
| `autorun` (signal-triggered turns) | `ui.allow_autorun` (default **off**, user-granted only) + per-minute/per-session/daily budgets |
| `app_call` actions | declared-schema validation, per-call timeout, `max_concurrent:1` |
| `app_result` / `br.call` output | per-payload size cap (64 KB, truncation marker) — these enter model context |
| `br.kb.*` | `data.sources[kind:knowledge]` **scoped to enumerated KB ids** (default none); `ingest` requires separately-consented `write:true` |
| Model routes | user-configured providers only; **provider-class capability** for sensitive data sources (§3.4) |
| State doc | 256 KB cap, 64 ops/patch, patch-rate limit, schema validation when declared, default structural caps otherwise; bindings render via `textContent` + attribute allowlist only |

**WebSocket authority (Phase-1 requirement, not late hardening).** In v1 the app socket only drove the app's own DOM; in v2 it carries `app_call`, `br.kb`, model routes, and state writes — so the current gate (`is_local_origin`, which accepts *any* `http://localhost:*` page, secret-exempt by design at `apps.rs:206-216`) is no longer sufficient: any local web content could open `/apps/<id>/agent` and drive a capability-bearing agent (CSWSH). v2 requires (a) exact-origin pinning (scheme+host+port of the app's own served origin) and (b) a **per-app socket token** minted into the served page (readable same-origin only) and required on upgrade.

**CSP (corrected).** `'unsafe-inline'` in `script-src` would make CSP inert against exactly the injection classes v2 introduces (`html` node output, binding sinks) — the app-proxy already ships `script-src 'self'` without it (`mcp_app_proxy.rs:65`), and apps load their code externally (`dist/app.js`), so served apps get the strict policy: `script-src 'self'`; the injected `BIOROUTER_APP_CONFIG` inline script becomes a non-executable `<script type="application/json">` block the SDK parses; plus `connect-src 'self'` (blocks exfiltration), `img-src 'self' data:`, `form-action 'none'`, `base-uri 'self'`, `frame-ancestors 'self'`. Lint already forbids external scripts, so app authors are unaffected.

**Trust boundaries stated plainly:** (1) App→agent payloads are untrusted (indirect prompt injection) — enveloped, capped, and never a substitute for capability minimization. (2) Agent→app content is untrusted too (a prompt-injected agent) — hence textContent bindings, server-side sanitization, catalog validation. (3) **An imported/shared app is an untrusted author: its manifest is a capability *request*, not a grant** — the recipient re-consents on first run (deny-by-default), especially for KB access, `write:true`, model routes, and autorun; the same consent screen enumerates the server-side payload the export wants to install (KBs, skills, extensions — §3.9) before anything touches the recipient's store. Exported apps get the same CSP/serve invariants via `serve.mjs` parity. (4) Apps are **single-user, per-client session-scoped** (state keyed by `client_id`); collaborative multi-viewer apps are out of scope for v2.

Unchanged and reaffirmed: vault plaintext never in frames; path-jailed stores; `.vault/` excluded from export; mutating HTTP requires the secret; structured validation of every agent-emitted frame stays server-side in `control.rs` (weak local models get correction messages, not blank panels).

### 3.8 Pillar 8 — Multi-agent apps: named profiles, delegation, collaborative & adversarial patterns

One agent per app is the v1 shape, but multi-agent is already half-present: `orchestration.sub_agents` is **wired today** — declared sub-agents are materialized as engine recipes (`apps.rs:708-737`, `materialize_subagent_recipe`) and exposed to the primary agent as **agents-as-tools** via the core subagent tool (`crates/biorouter/src/agents/subagent_tool.rs`, with its own concurrency cap), and the SDK already renders `handoff{from,to}` frames in the timeline. What's missing is everything *outside* that façade: the app can't address a specific agent, can't run two agents in parallel, can't give a panel its own agent. v2 adds **named agent profiles**:

- **Manifest:** the dormant `orchestration.agents: HashMap<String, AgentConfig>` map (already in `manifest.rs`) becomes the vehicle. The existing `agent` block is the `main` profile; each additional profile carries its own system prompt, model/route, extensions, skills, KB — and a capability set that must be a **subset** of the app's grants.
- **Sessions & transport:** each profile gets its own session (keyed `app:<id>:<client_id>:<profile>`) and turn loop; frames are multiplexed over the *same* WebSocket with an optional `agent` field (omitted = `main`), so reconnect/replay semantics are unchanged.
- **App side:** `br.agent("critic")` returns a scoped facade (`call`/`run`/`prompt`/`on`). Turns on *different* profiles run in parallel (bounded: default max 3 concurrent per app); turns on the same profile stay serialized. This is what lets a dashboard refresh three panels through three worker profiles concurrently, or a "Debate" button fan the same question out to two differently-prompted profiles and render both answers side by side — **author-orchestrated collaboration**, no new protocol concepts.
- **Agent side:** alongside the shipped sub-agents-as-tools path, `main` gets a `consult { agent, prompt }` tool to invoke a named profile mid-turn — **agent-orchestrated collaboration**. The canonical adversarial pattern (generator produces, skeptic refutes, only survivors render) becomes: `main` drafts → `consult{agent:"critic"}` → revise → `ui_patch`.
- **UI authority & presence:** only `main` holds `ui_*`/appcontrol by default; a worker profile gets UI control only if its profile says `ui:true`, and its panels/presence chips are attributed ("Critic · reviewing evidence ⋯") so the user always knows *which* agent is acting. Signals/autorun budgets are per-app, not per-profile (no budget multiplication).
- **Patterns unlocked:** adversarial review (generator + critic), panel-of-judges scoring, pipeline stages (extract → analyze → visualize) each owned by a profile tuned to its task — including **different models per role** (a local model triaging, an institutional model touching PHI, a frontier model writing the synthesis), which is exactly BioRouter's provider-integration strength applied inside one app.
- **Boundaries:** profiles live in-process in `biorouterd` (the `biorouter-acp` protocol remains the layer for *cross-process* agent orchestration, and `br.workflow.run` in v2.1 the layer for declarative DAGs). Two simultaneous turns on the *same* profile remain out of scope.

### 3.9 Pillar 9 — App lifecycle: Applications-panel round-trip + standalone export

The full lifecycle is a product guarantee, not an implementation detail: **create → appears in the Applications panel → reopen and change it anytime → export as standalone software that carries its full server-side payload and runs without opening BioRouter, on any OS.**

**What already ships (v1) and is retained:** the Applications panel lists every app with launch/delete and a working one-click **Export** (`ApplicationsView.tsx:114-142` → `GET /apps/{id}/export`, which rebuilds a stale bundle first via `export_scaffold`). The exported folder is directly runnable: `run.command` (macOS) / `run.sh` source `biorouter-launch.sh`, which locates or installs `biorouterd`, self-installs the app into the recipient's store, starts the daemon **headlessly**, verifies it, and opens the default browser — the BioRouter GUI never opens. A `serve.mjs` loopback proxy (static files + `/apps/**` incl. the WS upgrade) covers the no-shell path. `.vault/` is excluded; the SDK derives its endpoint from the page origin.

**"Standalone" defined honestly:** the app's intelligence *is* the BioRouter platform — providers, KB, extensions, skills live in `biorouterd`. So standalone means *no BioRouter application (GUI) and no visible BioRouter anything*: a double-clickable folder whose scripts run the daemon as an invisible backend. A generated app with no daemon would have no agent; that trade is inherent and stated.

**v2 additions:**

1. **Export parity is a phase-gate invariant.** Every pillar's features must work identically in the exported form — the strict CSP, the per-app socket token (minted by `serve.mjs`/the launch path), durable state restore (the recipient's session store), multi-agent profiles, `figure` fragments, theme packs. The rule: *if it works in the Applications panel, it works exported.* Each plan phase's acceptance includes the export smoke, not just Phase 6.

2. **Full server-side payload travels with the app — or just a launcher: the user chooses.** An exported app is only as good as the platform pieces its agent depends on, so the export can carry them. The panel's Export becomes a small dialog (and `export_app` gains `mode` + `include` params) offering two modes:
   - **Launcher export** (today's thin form, kept as a first-class choice): app + launch scripts only — smallest folder; the app runs against whatever knowledge bases, skills, extensions, and providers already exist on the target machine. Right for self-use, same-machine moves, and lab machines that share a configured BioRouter install.
   - **Full export**: the payload bundling below, with **per-item toggles, pre-checked from what the app's agent config actually references** — the user decides item by item what travels:
   - **Knowledge bases** — each granted KB is staged as a `.brkb` bundle (the existing knowledge export format) inside `payload/knowledge/`; raw sources optionally excluded to control size (the dialog shows a size estimate per item). On the recipient's machine the first-run installer imports it into their store under the same KB id, satisfying the app's scoped grant.
   - **Skills** — the app's skill list is staged under `payload/skills/` (the same zip format the marketplace uses) and installed into the recipient's skills dir on first run.
   - **Extensions** — *builtin* extensions (developer, autovisualiser, knowledge, …) travel with the daemon and need nothing. *External* extensions are staged as `.brxt` bundles under `payload/extensions/` when installed locally, or recorded as **pinned, checksummed registry references** (BAAM) the installer fetches on first run — the dialog says which. Runtime prerequisites an extension declares (e.g. Node for PlaywrightAgent) are recorded and checked at first run with a clear remedy message.
   - Everything is enumerated in a **payload manifest** (`export.json`: items, versions, checksums, required credentials, runtime requirements) so the installer is deterministic and the recipient can audit exactly what a shared app wants to install.

3. **Credentials: never bundled, always onboarded.** The export carries no secrets of any kind (vault excluded, provider keys and extension credentials live in the OS credential store). Instead, `export.json` lists the **credential requirements** — the env keys the app's extensions declare (e.g. `SPOKEAGENT_PASSCODE`) plus "a configured provider" — and on first launch the SDK shows a **setup dialog**: which credentials are missing, a field for each, stored via the daemon into the recipient's OS credential store (existing keyring path), then the agent starts. Re-launches skip whatever is already satisfied. A machine with no provider configured gets the same dialog with a guided provider-setup step rather than a dead app. KB grants referencing a base the user chose *not* to bundle degrade gracefully via `has()`.

4. **OS-agnostic by format.** The app itself is a **web application** — HTML/TS in any modern browser — so the UI is OS-agnostic by construction; what differs per OS is only the backend daemon. The export ships launchers for all three platforms (`run.command` for macOS, `run.sh` for Linux, `run.ps1`/`run.bat` for Windows); in **thin** mode each launcher auto-installs the platform-matching `biorouterd` from the pinned GitHub release; in **fat** mode the dialog chooses `current platform` (default, ~108 MB) or `universal` (all platforms bundled, larger, runs anywhere). One exported folder therefore runs on macOS, Linux, and Windows. macOS quarantine caveat stated in the README (right-click-Open on first run; per-app notarized packaging is v2.1). A *hosted* variant — one daemon serving the app as a plain URL to colleagues' browsers — is the fully-zero-install endgame and lands in v2.1 with the remote-auth work it requires (today's auth model is loopback-only).

5. **First-run experience is one flow.** Launch → daemon up → **single consent screen** combining §3.7 capability re-consent with the payload install list ("this app will install KB *ms-cohort*, skill *ggplot-visualization*, extension *SPOKEAgent*, and needs 1 credential") → payload installs → credential dialog (item 3) → app opens. Same-machine self-export skips consent for grants and payload already present.

6. **Editability round-trip.** The exported folder remains a readable TS project (the human-facing SDK reference in §5 exists precisely for this); re-importing an edited export re-runs lint + build, and `sdk_hash` drift triggers the rebuild path as usual.

Out of scope (consistent with §7): per-app desktop packaging (.dmg/Tauri wrapper per app), the hosted/URL-sharing variant, and BAAM marketplace listing — v2.1, gated on the import-re-consent model.

## 4. Worked examples (what becomes possible)

**A. Knowledge-graph explorer (BioOKF-Studio-class, ~50 lines of authored code).** `explorer` starter + `br.kb.graph()` → `network` component; `node_selected` signal subscribed by the agent → agent `br.kb.page()`s the node and `ui_patch`es the inspector panel with a `markdown` component + a `figure` (Kaplan-Meier from Auto Visualiser) when relevant; user asks "focus on demyelination" → agent calls `app_call{focus_node}`. The presence chip narrates each step. Every piece is a declared, typed, gated primitive — no eval bridge, no bespoke renderer, no polling.

**B. Avatar / scene control ("move the avatar up").** `canvas` starter: author registers a `canvas` component with a `world` model in shared state (`/avatar/position`), plus actions `move_avatar`, `speak`. The user types "walk to the door and greet"; the agent plans and emits `app_call{move_avatar,…}`, `app_call{speak,…}`; state patches animate the canvas; `collision` signals flow back if subscribed. The agent never writes runtime code — it drives declared verbs, exactly like BioOKF's `selectNode` but generated from the manifest. (Voice input is not an SDK primitive in v2; text NL covers the ask.)

**C. Cohort dashboard on institutional data.** `dashboard` starter + `data.sources[omop]` + model route `institutional` (provider-class-constrained, §3.4); KPI grid bound to `/cohort/*` state paths; the agent refreshes via SQL tools and `ui_patch_state`; a dragged age-slider fires a debounced, superseding `br.call("refresh_cohort", {age_range})` with `output_schema`, so results land as data in bound components — zero markdown re-parsing.

**D. Adversarial evidence review (multi-agent).** Two profiles: `reviewer` (institutional model, `br.kb` read on the lab's KB) and `skeptic` (system prompt: "refute every claim; demand provenance"). The user drops in a manuscript claim; author code fans out `br.agent("reviewer").call(...)` and, on its result, `br.agent("skeptic").call(...)`; surviving evidence renders into a `table` with per-row provenance, refuted items into a struck-through `log` panel — the BioOKF "epistemic status is visible" principle, produced by an adversarial pair. Alternatively the whole loop runs agent-side: `main` drafts and `consult{agent:"skeptic"}`s before ever touching the UI.

## 5. Compatibility & migration

- **Old apps keep working untouched.** Every v1 frame/tool/API is preserved; v2 is additive. The `ready` frame advertises `catalog_version` + capability tokens; `has()` feature-detects.
- **`refresh_sdk` + `sdk_hash`** (already built) roll the new runtime to existing apps: lazily on `serve_index` drift detection, plus an explicit `biorouter`-side batch rebuild step for the whole store. Acceptance: all stored apps rebuild on the new SDK and `check-all` holds its v1 pass count. The app corpus + `round*` authoring scripts get vendored/pinned into this repo first (today `check-all.mjs` references an external worktree path), so the regression gate is reproducible.
- **Unknown-frame tolerance** means a stale bundle simply ignores v2 frames until rebuilt.
- **Lint v2** adds rules for the new surface (declared-vs-registered action mismatch, bind paths not in state schema, signal without coalesce, custom component without schema, prop-fed `innerHTML` sinks) so the author-agent self-corrects — the mechanism that made even vague prompts yield working apps in v1.
- The **authoring instructions in `mod.rs` are rewritten around archetypes** with one curated exemplar each, and the "VARY THE INTERFACE" plea becomes structural (starters) rather than rhetorical.
- **Human-facing SDK reference** ships alongside (not just author-agent prompts): the `br.*` API surface, `manifest.surface` JSON Schema, frame/protocol reference, capability matrix, custom-component guide, and the three worked examples as annotated source — `export_app` produces a project humans edit, so the SDK needs human docs.

## 6. Testing & evals (evals before features — Townie)

- **Unit:** state-doc patch/version/rebroadcast in `control.rs`; catalog validation incl. custom schemas (mismatch rejected); morph renderer (focus/scroll survival) in an sdk harness; `app_call` parking/timeout/cancel alongside the existing `ui_ask` tests; sanitizer bypass corpus.
- **Integration:** extend `ui-control-harness.mjs` (mock daemon, real SDK) with state binding, `ui_patch`, signals, `app_call`; extend `check-ui-app.mjs` to assert v2 frames arrive against a real agent.
- **The UI-variety benchmark becomes the eval.** `round2/round3` + `benchmark.mjs` already measure real-controls-per-app on raw store markup. v2 targets: vague prompts ≥ 80% non-chat archetypes; detailed prompts: ≥ 2 bound state paths, ≥ 1 declared action, ≥ 1 non-markdown component per app, measured the same honest way, against a pinned v1 baseline.
- **Error-feedback loop:** runtime errors in catalog rendering post a structured `ui_error` frame the agent sees (bounded by the same live-turn grace discipline as artifact auto-repair), closing the self-correction loop in-app.

## 7. Scope decisions (explicit)

- **Distribution:** standalone export is **first-class in v2** (Pillar 9, §3.9 — panel export dialog with full server-side payload bundling (KBs/skills/extensions), credential onboarding, runnable without opening BioRouter on macOS/Linux/Windows, optional bundled daemon). What stays v2.1: `.brapp` bundle + one-click install mirroring `.brxt`, and BAAM listing — **gated on the import-re-consent model in §3.7** (a shared manifest is a request, not a grant).
- **CLI:** apps are browser-rendered by design; the CLI gets `biorouter apps list|open|serve` parity (launch daemon, print/open the URL — matching how `launch_app` already behaves in CLI contexts). In-terminal (ratatui) rendering of catalog UIs is out of scope.
- **A Tauri/desktop shell for apps** — apps stay browser-served by `biorouterd` (and inside the Electron Applications tab). Studio's vibrancy/PTY/Finder affordances are not portable SDK primitives.
- **CRDTs** — single ordering authority (the server) + JSON Patch suffices; apps are single-user per-client (§3.7). Revisit only if collaborative editing becomes a requirement.
- **Multi-agent apps are IN scope** (Pillar 8, §3.8): named agent profiles with parallel turns *across* profiles, `consult` for agent-orchestrated collaboration, and the already-shipped sub-agents-as-tools path. Still out: two simultaneous turns on the *same* profile, and cross-process orchestration (that's `biorouter-acp`'s layer).
- **Voice input** — not an SDK primitive in v2.
- **Workflows & scheduled refresh** — v2.1 (§3.4).
- **Provider-level structured output** (a `response_format` channel in the `Provider` trait) — not required; the `emit_result` tool mechanism covers v2 (§3.1) and can be swapped later.

## 8. Risks

| Risk | Mitigation |
|---|---|
| Weak local models can't drive the bigger surface | Everything remains small typed frames (Family B/C discipline); server-side validation returns *fixable* errors; archetype starters carry the structure so the runtime-agent mostly fills slots |
| Scope: seven pillars is a platform | Pillars are independently shippable; the plan phases them; Pillars 1–3 alone dissolve the chatbot ceiling; §7 trims v2.1 items explicitly |
| Custom components reintroduce Family-A fragility at runtime | They execute *author* code written once at build time; the agent only supplies schema-validated props (extraction fails closed); authors are linted against prop-fed sinks |
| Indirect prompt injection via app payloads | Untrusted-data envelope + per-field caps + scoped capabilities (§3.5/§3.7); acknowledged as residual risk, mitigated by minimization not trust |
| Signal floods burn tokens | Declaration-level `coalesce_ms`, server-side rate caps, queue-only default, autorun off by default with budgets |
| Multi-agent profiles multiply cost and complexity | Per-app concurrent-turn cap (default 3), per-app (not per-profile) signal/autorun budgets, profile capabilities ⊆ app capabilities, presence attribution per agent; sub-agents-as-tools already ships and stays the low-cost default |
| Full-payload exports get large or stale | Per-item size estimates + toggles in the export dialog (raw KB sources optional); pinned versions + checksums in `export.json`; registry-reference mode fetches instead of bundling; universal daemon bundling is opt-in |
| Two renderers drift (markdown fences vs catalog) | Fenced ```chart/```graph become sugar that lowers to catalog instances internally; one renderer |
