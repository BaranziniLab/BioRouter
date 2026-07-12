# BioRouter Apps SDK v2 — Implementation Plan

**Design:** [`docs/superpowers/specs/2026-07-12-apps-sdk-v2-design.md`](../specs/2026-07-12-apps-sdk-v2-design.md)
**Date:** 2026-07-12 (revised after adversarial review)
**Shape:** six phases, each independently shippable and mergeable; every phase ends green (`cargo test -p biorouter-mcp --lib agent_drafter::`, `cargo test -p biorouter-server --lib routes::apps`, `node scripts/agent-drafter/ui-control-harness.mjs`, `tsc` clean) and keeps all v1 apps working. **Export parity is a per-phase gate (design §3.9):** each phase's acceptance includes the export smoke — the demo app exported via `GET /apps/{id}/export` and launched by `run.sh` against a headless daemon must exercise that phase's new features (state restore, socket token, `ui_patch`, signals, profiles, theme packs) identically to the panel-served app.

Key files, recurring in every phase:
- `crates/biorouter-mcp/src/agent_drafter/control.rs` — `ui_*` tools, `UiBridge`, frame emission, validation
- `crates/biorouter-mcp/src/agent_drafter/templates/sdk.ts` — client runtime
- `crates/biorouter-mcp/src/agent_drafter/manifest.rs` — capabilities + (new) surface declarations
- `crates/biorouter-mcp/src/agent_drafter/bundle.rs` — build + `lint_app`
- `crates/biorouter-mcp/src/agent_drafter/mod.rs` — MCP tools + authoring instructions
- `crates/biorouter-server/src/routes/apps.rs` — socket loop, session, extension injection
- `scripts/agent-drafter/ui-control-harness.mjs`, `ui/desktop/scripts/appcheck/*` — harnesses/evals

---

## Phase 1 — Protocol core: shared state, versioning, socket auth, typed surface (~1–2 weeks)

The foundation every other pillar builds on. Additive; no behavior change for v1 apps.

0. **Reproducible regression gate (prerequisite).** Vendor/pin the app corpus + `round*` authoring scripts into this repo (today `appcheck/check-all.mjs` references an external worktree path), and record the exact command + pass count that constitutes the v1 baseline for `check-all`/`benchmark`.
1. **WS authority hardening (moved up from "hardening" — the socket gains real authority in later phases).**
   - Exact-origin pinning on `/apps/{id}/agent` upgrade (scheme+host+port of the app's served origin), replacing the any-localhost `is_local_origin` acceptance (`apps.rs:206-216`).
   - Per-app socket token: minted server-side into the served page's config (same-origin readable only), required as a query/header on upgrade. Export path (`serve.mjs`) proxies it.
   - Tests: route tests — foreign-localhost origin rejected; missing/wrong token rejected; existing same-origin flow unaffected.
2. **Frame versioning + fallbacks.**
   - Add `catalog_version` to the `ready` frame and every `ui` frame (`control.rs::emit`).
   - SDK: unknown `cmd` → ignore (verify existing), unknown widget kind → labeled placeholder div instead of "unsupported widget" error (`renderWidget` default arm).
   - Test: old-SDK fixture receives v2 frames without breakage (harness).
3. **Shared state document.**
   - `BridgeInner.state` becomes `{doc: serde_json::Value, version: u64}`. `ui_state` (compat) merges into the doc; new tool `ui_patch_state { patch }` applies RFC-6902 (**new workspace dep: `json-patch` crate** — vet license/maintenance; the client-side binding index needs only JSON Pointer resolution, not the full crate), bumps version, rebroadcasts `{cmd:"state", mode:"patch", patch, version}`.
   - New client→server frame `state_write { patch | set:{path,value}, base_version }`; server applies (last-writer-wins with version check → on conflict send fresh snapshot), rebroadcasts, and exposes the doc to the agent via `ui_describe`/tool results.
   - `attach()` replay sends `{cmd:"state", mode:"snapshot", doc, version}` (extend the existing replay).
   - Caps: 256 KB doc, 64 ops/patch, patch-rate limit; schema validation when `surface.state_schema` present; **default structural caps when absent** (max depth 8, max 2,000 keys, string-length caps).
   - Tests: patch/version/conflict/rebroadcast unit tests in `control.rs`; snapshot-on-reconnect in harness.
4. **State persistence.**
   - Persist doc+version per durable session (`app:<id>:<client_id>`) via the **`ExtensionData` `store_into`/`load` pattern** (as `RunState` does — no new sessions column, no migration; if a column ever becomes necessary, that's a human-reviewed schema change per `HOWTOAI.md`). Enforce the 256 KB cap before serialization; restore seeds the `UiBridge` doc *before* `attach()` replays.
   - Test: route test — reconnect restores doc.
5. **Declarative bindings (client-only, safe sinks).**
   - SDK: index `[data-br-bind]`/`data-br-bind-attr`/`data-br-bind-show` on mount + after any render; on state snapshot/patch, resolve JSON Pointers and update only affected nodes. `br.state.get/set/update/subscribe(path, fn)`.
   - **Safety contract:** `data-br-bind` writes `textContent` only; `data-br-bind-attr` enforces an attribute allowlist (no `on*`) + URL-scheme validation for `href`/`src`.
   - Tests: harness — bound span updates on agent patch; author write round-trips; `javascript:` URL and `onclick` bind attempts are refused.
6. **Manifest `surface` block + `ui_describe` v2.**
   - `manifest.rs`: add `Surface { state_schema, actions, signals, components }` (all optional, serde-defaulted). **Named `signals`, not `events` — `Capabilities.events` already exists with opposite-direction semantics (agent-lifecycle stream to the app via `br.on()`, advertised as `event:<name>`); document the distinction where both are defined.**
   - `ui_describe` merges declarations + runtime registrations + browser-reported surface + state version into one typed report. **Constructor change:** `AppControlServer::new` gains the manifest surface (today it receives only `UiBridge` + `UiCapability`); thread it through `configure_agent` in `apps.rs`.
7. **Docs:** protocol section in `docs/agent-drafter-apps.md`.

**Acceptance:** a demo app with two bound spans and a button whose author code writes state; the agent patches state and both views update without any `innerHTML` replacement; reload restores everything; a foreign localhost page cannot open the socket.

## Phase 2 — Catalog v2: flat IDs, `ui_patch`, morphing, science pack (~2–3 weeks)

1. **Flat ID-keyed instances.** Normalize `ui_render`/`ui_panel` trees into an instance map (`id → {kind, props, parent, order}`) held per-connection in the SDK; assign stable auto-IDs where the agent omits them.
2. **`ui_patch` tool** — ops `add | replace | remove | set_props` by id; server-validates each node like `validate_widget` today; SDK applies against the instance map.
3. **Morphing renderer.** Keyed reconciliation on re-render (match by id/kind, patch attributes/children, preserve focus/scroll/inputs/canvas). Modeled on Idiomorph's id-set matching; implemented in-SDK (dependency-free, ~250 lines).
   - Tests: focus survives a `set_props` on a sibling; input value survives table refresh (harness).
4. **Science pack, in order of leverage:**
   a. `network` — port/generalize the BioOKF canvas engine (Barnes-Hut layout, culling, hover/select/drag/zoom, label placement) with a typed spec + `encoding` map; selection → declarable signal (wired in Phase 3). Constants become props with the Studio values as defaults.
   b. `table` v2 — virtualized scroll, sort, filter, row-select.
   c. `plot` — scatter/area/box/heatmap added to the existing themed-SVG charts; `bind` prop.
   d. `markdown`, `image`, `kpi`, `log`.
   e. `figure` — **prerequisite first:** `autovisualiser::common` is a private module (`autovisualiser/mod.rs:8`) and `render_fragment`/`ASSET_SINK` are not re-exported; add a `pub(crate) fn render_named_figure(tool: &str, args: Value)` API on the autovisualiser side (dispatching to the named `render_*` tool inside `render_fragment`'s asset-capture), then the `ui_figure { tool, args, target }` tool emits the fragment into a sandboxed `srcdoc` iframe node. Asset splicing/dedup is real work — single-figure-per-node (each fragment carries its own assets) is acceptable for this phase. Same crate, so no circular-dependency risk.
   f. `html` — **server-side, fail-closed sanitization in `control.rs`** with a pinned config (no script/style/form, no `on*`, no `javascript:`/`data:` URLs, SVG/MathML mXSS guards) + a known-bypass regression corpus; `ui.allow_html` gate (default off); lint rule.
5. **Custom components.** `app.components.register(name, {props, mount, update})`; build-time extraction (`bundle.rs`) into the manifest — **fail closed:** an unparseable/dynamic registration is a build error, never accept-any; server-side prop validation vs declared schema (mismatch test); `ui_patch` can emit them like built-ins. Document + lint: props are agent-controlled/untrusted (flag `innerHTML`/URL sinks fed from props).
6. **Fence lowering:** ```chart/```graph in markdown lower to catalog instances internally (one renderer).
7. **Lint v2 (part 1):** custom component registered-but-undeclared (Error), declared-but-unregistered (Error), `html` used without capability (Error), prop-fed sink (Warn).

**Acceptance:** agent builds a network+inspector UI via `ui_patch`, updates node props incrementally, selection state survives every update; a custom `pathway_map` component round-trips validation and a schema mismatch is rejected; autovis Kaplan-Meier renders inside an app panel; sanitizer corpus green.

## Phase 3 — Typed RPC + signals: `app_call`, `br.call`, subscriptions, run-supersede (~2 weeks)

1. **`app_call`.** Tool generated per declared action (schema in tool description); frame `{type:"app_call", callId, action, args}`; SDK dispatches to registered handler; result frame `app_result { callId, result | error }` resolves the parked tool call (reuse the `pending` oneshot machinery + timeout + `cancel_all` discipline from `ui_ask`). **`app_result` values are size-capped (64 KB, truncation marker) and delivered in the untrusted-data envelope.**
2. **`br.call(name, args, {route?, output_schema?})` via the `emit_result` tool convention.** There is **no** structured-output/response-format channel in the `Provider` trait (`providers/base.rs` — `complete*` take only system/messages/tools), so v2 does *not* build one: when `output_schema` is supplied, the server injects a synthetic `emit_result` tool whose input schema *is* the `output_schema`, instructs the model to finish by calling it, validates the call (the tool path already validates schemas), and emits the **new** `output {schema, value}` frame — the type exists in `sdk.ts:74` but has never had a server producer; this step adds it. Prose fallback when the model never calls the tool. Provider-level structured output remains a possible later swap behind the same app-facing contract.
3. **`widget_action` becomes typed without losing turn-start semantics.** Today the WidgetAction arm (`apps.rs:1099-1111`) synthesizes the *user `Message` that starts the turn* — it is the only ingress for author-declared buttons, not mere formatting. Keep the turn trigger; change the payload: a minimal user-text envelope carrying the structured JSON block. Verify the store's button-driven apps against this before removing the pure-prose form (compat flag for one release).
4. **Signals.** `app.signals.emit(name, payload)` → `signal` frame (validated vs declaration, coalesced per `coalesce_ms` client-side, rate-capped server-side); `ui_subscribe {signals}` tool; queued through the existing between-turn/mid-turn frame paths (`MAX_QUEUED_FRAMES` discipline); delivered to the model as size-capped structured JSON inside the untrusted-data envelope. **Queue-only in this phase:** signals are context for the next turn and never start one. (`autorun` is deferred to Phase 6 behind its own capability + budgets.)
5. **Debounce/supersede runs.** `br.run`/`br.call` options `{debounce_ms, supersede}`; supersede sends `cancel` for the in-flight superseded turn (cancel path already works mid-turn).
6. **Lint v2 (part 2):** action declared but never registered; signal emitted but never declared; `buildPrompt`-style string-concat detected while actions exist (Warn: prefer `br.call`).

**Acceptance:** the avatar demo — declared `move_avatar` action, agent drives it from NL, state animates the canvas, `collision` signal reaches the agent, slider-driven `br.call` supersedes stale turns, `output_schema` results arrive as data via `emit_result`.

## Phase 4 — Platform APIs: `br.kb`, model routes, tool-resource bridge (~2 weeks; the read-only `br.kb` slice has no Phase-3 dependency and can start right after Phase 2 so the KB-explorer flagship lands early)

1. **`br.kb`** — server-side handlers on the app socket, gated by `data.sources[kind:knowledge]` **scoped to enumerated KB ids (default: none — never "all bases")**: `search`, `page`, `graph`, `history`; `ingest` requires the separately-consented `write:true` (cross-session KB-poisoning risk documented in the capability prompt) and streams progress frames (reuse the knowledge SSE machinery). Client namespace with typed results.
2. **Model routes.** Manifest `agent.routes: {name: {provider?, model?}}`; `br.call({route})` and `ui_*`-initiated turns can select; validation: routes must resolve against the user's configured providers at session start (fallback chain as today). **Provider-class constraint:** apps holding sensitive data sources (`omop`, `cdw`, confidential KBs) are restricted to local/institutional provider classes unless the user explicitly consents per app. `br.model.status()` surfaces llamacpp state via the existing `/llamacpp/status` plumbing.
3. **Tool `ui://` resources → `figure` components.** When an extension tool result carries a `ui://` embedded resource, the socket loop emits it as a catalog `figure` instance targeted at the app's declared results region (author-overridable), instead of dropping it. (Depends on Phase 2's figure node, not on autovis internals — the resource is already-rendered HTML.)
4. **Skills scoping enforcement** for app sessions (per-app skill list actually filters, not advisory).
5. **Docs + capability matrix** in `docs/agent-drafter-apps.md`.

**Phase 4b — multi-agent profiles (design §3.8; ~1–2 weeks, after Phase 3's `br.call` lands):**

6. **Named profiles.** Wire the dormant `orchestration.agents` manifest map (`manifest.rs` — the struct exists, `apps.rs` never reads it): the `agent` block becomes the `main` profile; validate at session start that each profile's capabilities/extensions/KB are a **subset** of the app's grants, and that routes obey the provider-class constraint per profile.
7. **Session-per-profile + frame multiplexing.** Sessions keyed `app:<id>:<client_id>:<profile>`; frames gain an optional `agent` field (omitted = `main`, so every v1 frame is unchanged). `handle_agent_socket` grows a task-per-active-profile with an mpsc merge into the single socket writer — this is the real work: today the loop drives exactly one agent's turn; profile turns must run concurrently (per-app cap, default 3) while same-profile turns stay serialized. Cancel targets a profile.
8. **SDK facade.** `br.agent(name)` → scoped `call/run/prompt/on`; `main` remains the default for all existing APIs. Presence chips + timeline items attribute the acting profile ("Critic · …"); `handoff` frames (already rendered, `sdk.ts:1156`) get the profile name.
9. **`consult` tool.** `main` (or any profile granted it) can invoke a named profile mid-turn as a tool — parked/resolved like `app_call`, result size-capped + enveloped. Keep the shipped sub-agents-as-tools path (`materialize_subagent_recipe`, `apps.rs:708-737`) untouched as the low-cost delegation default; document when to use which.
10. **UI authority.** Only `main` gets `appcontrol` unless a profile declares `ui:true`; worker-profile panels are namespaced so two profiles can't fight over one panel id.
11. Tests: route tests for subset-validation and parallel-profile turns; harness — two profiles answer one fan-out concurrently and render into separate panels; unit — consult parking/timeout/cancel; per-app concurrency cap enforced.

**Acceptance:** the KB-explorer demo app (design §4A) works end-to-end against a real knowledge base with `network` + inspector + agent narration, and cannot read a KB outside its grant. The adversarial-review demo (design §4D) runs both author-orchestrated (`br.agent(...)` fan-out) and agent-orchestrated (`consult`) with visible per-agent attribution.

## Phase 5 — Aesthetics: theme packs, layout grammar, archetype starters (~1–2 weeks)

1. **Theme packs.** Token-set structure in the manifest; 6 curated presets in `theme.css` (CSS custom-property layers, incl. dark variants); `render.rs` injects the selected pack; `ui_theme` pack-switching behind `allow_theme`. Contrast lint generalizes to the active pack.
2. **Layout grammar.** `ui_layout { areas, sizes }` → validated grid template (bounded vocabulary, no raw CSS); **the 5 existing presets remain as aliases** so v1 `ui_layout{preset}` calls keep working.
3. **Archetype starters.** Six starter template sets (`explorer`, `dashboard`, `workbench`, `wizard`, `canvas`, `chat`) under `templates/starters/`; `create_app` gains `archetype` (default: inferred from description, `chat` only when asked); each starter ships a bound state path, one action, one signal, one catalog component — working, lint-clean.
4. **Authoring instructions rewrite** (`mod.rs`): archetype-first structure, one curated exemplar per archetype, the DynaVis rule ("after an NL-driven parameter change, emit a persistent bound control"), presence/narration guidance, `frontend-design` skill pointer.
5. **Applications tab audit** (`ui/desktop/src/components/applications/ApplicationsView.tsx`): archetype badge, theme-pack swatch, launch affordance appropriate for non-chat apps (or a documented decision that no change is needed).

**Acceptance:** ten vague-prompt apps authored by a mid-tier model: ≥8 select a non-chat archetype and lint clean.

## Phase 6 — Hardening, autorun, error loop, evals, docs (~1–2 weeks + ongoing)

1. **CSP (strict) on served apps** in `routes/apps.rs`: `script-src 'self'` (no `unsafe-inline` — the injected `BIOROUTER_APP_CONFIG` becomes a non-executable `<script type="application/json">` the SDK parses; `render.rs` change), `connect-src 'self'`, `img-src 'self' data:`, `form-action 'none'`, `base-uri 'self'`, `frame-ancestors 'self'`. Verify export/`serve.mjs` parity. (Mirrors the app-proxy's existing `script-src 'self'` at `mcp_app_proxy.rs:65`.)
2. **`autorun` (last, smallest).** `ui.allow_autorun` capability — default off, user-granted only (never agent-self-granted); per-minute + per-session + daily turn budgets; presence-layer indicator with one-click stop. Signals otherwise stay queue-only.
3. **`ui_error` feedback loop.** Catalog render/handler errors post structured `ui_error` frames; agent sees them only under the live-turn grace discipline (port `shouldAutoRepairArtifact` semantics); harness test.
4. **Mass SDK reroll + regression.** Batch rebuild of the whole store on the new SDK (plus the existing lazy `serve_index` drift rebuild); acceptance: all stored apps rebuilt, `check-all` holds the pinned v1 pass count.
5. **Benchmark v2.** Extend `appcheck/benchmark.mjs`: score bound-state paths, declared actions, catalog components, archetype distribution on raw store markup (same honesty rule as v1); wire into the vendored `round2/round3` scripts; record vs the pinned v1 baseline.
6. **Docs.** (a) rewrite `docs/agent-drafter-apps.md` v2 sections; (b) **human-facing SDK reference** (distinct from author-agent prompts): `br.*` API, `manifest.surface` JSON Schema, frame/protocol reference, capability matrix, custom-component guide, three worked examples as annotated source; (c) CLAUDE.md pointer update; (d) example apps under `scripts/agent-drafter-apps/examples/` (KB explorer, avatar, cohort dashboard).
7. **CLI parity:** `biorouter apps list|open|serve` (launch daemon, print/open URL). In-terminal rendering stays out of scope.
8. **Standalone export v2 (design §3.9).**
   - **Fat export:** `export_app { bundle_daemon: true }` + a checkbox on the panel's existing Export button — stage the platform-matching `biorouterd` into the folder; `biorouter-launch.sh` prefers the bundled binary over PATH; export README documents per-platform choice + macOS quarantine (right-click-Open on first run).
   - **Socket-token + CSP parity in `serve.mjs`** (with Phase 1's WS auth): the proxy mints/forwards the per-app token and serves the same headers.
   - **Graceful degradation on foreign machines:** no-provider first run shows the backend banner with a guided setup pointer; missing KB grants feature-detect via `has()` (harness fixture).
   - **Re-import path:** an edited export re-installs through lint + build; `sdk_hash` drift rebuilds. Foreign-machine capability re-consent per §3.7.
   - Smoke: Linux fat export boots in a clean container (mirrors the `cli-linux` release smoke); macOS fat export manual checklist.
9. **Full-matrix verification:** unit + route + harness + `check-ui-app.mjs` live + export runs standalone (thin *and* fat) + Electron Applications tab smoke (`just agent-browser-ui` / debug-app skill).

---

## Sequencing & risk notes

- **Phases 1→2→3 are the dependency spine** (state → catalog → RPC/signals). The **read-only `br.kb` slice of Phase 4 can run concurrently after Phase 2** so the flagship KB-explorer (the closest analog to the BioOKF exemplar) demos as early as possible. Phase 5 can start once 2 lands; Phase 6 is continuous with a closing milestone.
- **Compat gates every phase:** the existing `agent_drafter::` unit tests + `agent_drafter_registered` + the vendored `check-all.mjs` corpus must stay at the pinned v1 pass count; any v1 app that breaks is a phase-blocking bug.
- **Deferred to v2.1 (explicitly, per design §7):** `.brapp` distribution/BAAM listing (gated on import re-consent), workflows/scheduled refresh, provider-level structured output, collaborative multi-user apps, voice input. **Multi-agent profiles are NOT deferred** — they land as Phase 4b (design §3.8); only same-profile parallel turns and cross-process (ACP) orchestration stay out.
- **Biggest technical risks:** (a) morphing renderer correctness — mitigate with the harness's focus/scroll tests before porting widgets onto it; (b) the `emit_result` convention's reliability across weak local models — mitigate with the lint/instruction pairing and prose fallback, and measure it in benchmark v2; (c) autovis `figure` wiring — the private-module refactor is a prerequisite task, not incidental; (d) WS token/origin changes must not break the Electron Applications tab or exported apps — route tests + export smoke cover both.
- **Estimate:** ~11–15 weeks of focused work end-to-end (Phase 4b adds ~1–2), but Phase 1+2 (≈4 weeks) already dissolve the chatbot ceiling, and with the early `br.kb` read slice the KB-explorer class of apps lands by ~week 6. Multi-agent apps (4b) land by ~week 8–9.
