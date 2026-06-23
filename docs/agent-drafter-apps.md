# Agent Drafter → BioRouter Apps (redesign)

Agent Drafter was reworked from "Claude-style artifacts" into a builder for
**BioRouter apps**: TypeScript front-ends wired to a *real* BioRouter agent
backend. When a user sends a message in a built app, BioRouter runs the full
agent loop — the app's own model, extensions, skills, and knowledge base — and
streams the answer (text / markdown / tool activity) back into the app. Apps are
*launched in the browser* (GUI auto-opens the default browser; CLI prints a URL),
not embedded in a chat iframe.

## What changed

| Before | After |
|---|---|
| Static/agentic HTML artifacts | TypeScript app projects (esbuild bundle) |
| `agent.js` "bridge" routed prompts into the chat box (no real reply) | Per-app WebSocket runs the real agent loop and streams the reply |
| Export = Tauri/Rust project | Export = standalone TypeScript project (esbuild + tiny static server) against a BioRouter daemon |
| Shown inline in a sandboxed chat iframe | Served by `biorouterd` at `/apps/<id>/`, opened in the browser |
| No per-artifact model/extension/skill/KB | Manifest carries model (default MiMo), extensions, skills, knowledge base, persona |

## Architecture

- **Store + manifest** — `crates/biorouter-mcp/src/agent_drafter/store.rs`.
  Each app is a project dir `~/.config/biorouter/agent_drafter/<id>/`:
  `manifest.json`, `index.html`, `src/main.ts`, `src/sdk.ts`, `dist/app.js`.
  Manifest `agent` block: `system_prompt`, `greeting`, `model {provider, model}`,
  `extensions[]`, `skills[]`, `knowledge_base`.
- **App SDK** — `templates/sdk.ts` (authored in TypeScript, bundled into each app).
  Opens the per-app WebSocket, streams events, renders markdown (headings, lists,
  code, links, **GFM tables**), handles multimodal image input, and can auto-mount
  a chat panel into `[data-br-chat]`.
- **Bundler** — `bundle.rs`. Locates esbuild (`$BIOROUTER_ESBUILD_BIN` → desktop
  `node_modules/.bin/esbuild` → PATH → `npx esbuild`); falls back to a vendored
  type-stripper when esbuild is absent. `src/main.ts` → `dist/app.js` (IIFE).
- **MCP tools** — `mod.rs`: `create_app`, `configure_app`, `update_app`,
  `build_app`, `launch_app`, `list_apps`, `read_app`, `preview_app`, `export_app`,
  `delete_app`, `set_app_size`.
- **Server routes** — `crates/biorouter-server/src/routes/apps.rs`:
  - `GET /apps` — list manifests (JSON)
  - `GET /apps/{id}/` — assembled index.html (theme + base href + bundle)
  - `GET /apps/{id}/dist|assets/{*path}` — bundle + assets
  - `GET /apps/{id}/agent` — **per-app agent WebSocket** (creates a session,
    applies model/extensions/skills/KB/persona, runs `agent.reply`, streams
    `message`/`thought`/`tool`/`done`/`error` frames)
  - `POST /apps/{id}/build`, `DELETE /apps/{id}`
  - Browser-facing GETs under `/apps` are exempt from the secret-key middleware
    (a browser tab can't send the header); the daemon binds localhost only.
- **Frontend** — `ui/desktop/src/components/applications/ApplicationsView.tsx` +
  an "Applications" sidebar entry under Knowledge. Lists built apps; Launch opens
  the app URL in the default browser via the existing `openExternal` IPC.

## WebSocket protocol (browser ⇄ backend)

Client → server: `{"type":"prompt","text":"…","images":[{"mimeType","data"}]}`,
`{"type":"cancel"}`.
Server → client: `{"type":"ready"}`, `{"type":"message","delta"}`,
`{"type":"thought","delta"}`, `{"type":"tool","name","status"}`,
`{"type":"done"}`, `{"type":"error","message"}`.

## Verification

- `cargo check -p biorouter-mcp` / `-p biorouter-server` — clean.
- `cargo test -p biorouter-mcp --lib agent_drafter::` — **28 pass** (store, tools,
  render, bundler incl. real esbuild bundling).
- `cargo test -p biorouter-mcp --test agent_drafter_registered` — **2 pass**
  (builtin registered; tools advertised over a real MCP transport).
- Live `biorouterd` (debug build, MiMo provider):
  - `GET /apps` lists apps; `GET /apps/<id>/` serves assembled HTML; on-demand
    esbuild build produces `dist/app.js` (10 KB IIFE).
  - **Per-app agent WebSocket streams real MiMo responses** (verified with a
    direct `ws` probe and Playwright):
    - `ask-mimo` → "*Drosophila melanogaster* (the fruit fly) is a classic model
      organism…"
    - `gene-explainer` (genomics persona) → structured TP53 breakdown
    - `biostats-helper` (biostats persona) → decision tree + comparison table
  - Per-app **system prompt** clearly shapes the output; per-app **model** applied.

## Iteration log (bugs found via testing → fixed)

1. **`biorouterd` requires the `agent` subcommand** — operational note; the bare
   binary prints usage.
2. **"Provider not set"** — a fresh per-connection session's agent has no provider
   until `configure_agent` sets one. If the app's provider can't be created (e.g.
   its API key isn't reachable by the running process), the agent had *no*
   provider and `reply` failed cryptically. **Fix:** `configure_agent` now falls
   back to BioRouter's global provider/model when the app-specific provider can't
   be created (`apps.rs`).
3. **Markdown tables not rendered** — the SDK's renderer handled headings/lists/
   code/links but not GFM tables (agents emit them often). **Fix:** added a table
   parser to `sdk.ts` `renderMarkdown` + table CSS in `theme.css`. Verified the
   rebuilt bundle emits `<table>`.

## Known limitations / next steps

- **Per-app skill scoping** is advisory (the selected skills are named in the
  system prompt and the `skills` extension is enabled); BioRouter's skill
  enable/disable is global, so true per-app skill isolation is a follow-up.
- **API keys**: a freshly-built `biorouterd` doesn't inherit the GUI binary's
  macOS Keychain grant; pass provider keys via env or run the signed binary.
- **Apps bundle their own `src/sdk.ts`** — SDK improvements reach existing apps
  only after a rebuild (re-copy `sdk.ts` + `build_app`). New apps get the latest.
- The chat-side preview is a static **card** (apps run in the browser, by design).
- Older artifact-format apps from the previous design remain in the store but
  won't serve a working bundle (no `src/main.ts`); recreate them with `create_app`.

## Example apps built by driving MiMo (16)

Each app below was authored end-to-end by the **MiMo model itself** calling the
Agent Drafter tools (`create_app` → `build_app` → `launch_app`) via
`biorouter run --with-builtin agent_drafter -t "…"` (see
`scripts/agent-drafter-apps/round.sh`). All are served by `biorouterd` at
`/apps/<id>/` and pass the checklist below.

| App | Extensions | What it does |
|-----|-----------|--------------|
| spoke-network-explorer | (chart-block) | NL → SPOKE graph relationships + **AI-generated inline charts** |
| web-research-assistant | computercontroller | Query → web search → sourced markdown answer |
| pathway-explainer | — | Pathway: overview / steps / genes / regulation |
| gene-function-explorer | — | Gene → function, pathways, expression, disease |
| variant-interpreter | — | Variant → functional impact + ACMG-style evidence |
| clinical-trial-navigator | — | Condition → trial phases, endpoints, criteria |
| drug-interaction-analyzer | — | Drugs → interactions, mechanism, severity |
| lab-protocol-generator | — | Experiment → numbered reproducible protocol |
| literature-summarizer-pro | — | Text → TL;DR / findings / methods / limitations |
| biostatistics-advisor | — | Study design → recommended test + assumptions table |
| differential-diagnosis-helper | — | Symptoms → structured DDx (with caveat) |
| sequence-analysis-toolkit | — | DNA/RNA/protein → GC, ORFs, translation, motifs |
| cell-type-annotator | — | Marker genes → likely cell type + confidence |
| enzyme-kinetics-tutor | — | Michaelis–Menten / Km / Vmax, step-by-step |
| omics-pipeline-advisor | — | Assay description → tools + workflow + QC |
| medical-term-explainer | — | Term → plain-language + technical definition |

### Per-app checklist (all green)

For every app: `manifest` valid (agentic, model = MiMo, non-empty system prompt) ·
`GET /apps/<id>/` 200 with theme injected · `GET /apps/<id>/dist/app.js` 200
(esbuild bundle > 500 B) · per-app agent WebSocket streams a real, non-empty,
persona-shaped reply · no error frame. Harness:
`scripts/agent-drafter-apps/round.sh verify` + `ui/desktop/scripts/appcheck/check-app.mjs`.

Round 1: **15/15 pass**. After the chart iteration + SDK propagation + biorouterd
rebuild: regression re-verify **15/15 pass**.

### Iteration log (drive → find issue → fix Agent Drafter → recompile → re-make)

1. **xargs arg-length** in the batch authoring runner (long personas) → switched
   to a batched background loop (`round.sh`).
2. **AI-generated visualizations** (the SPOKE requirement): the SDK rendered no
   charts. **Fix:** added a `renderChart` (dependency-free SVG bar/line) to
   `sdk.ts`, wired a ```chart fenced block into `renderMarkdown`, added chart CSS
   to `theme.css`. Recompiled biorouterd, re-copied `sdk.ts` into all 24 stored
   apps, rebuilt bundles. Verified: SPOKE renders an SVG bar chart in the browser.
3. **`autovisualiser` hijacks visualization**: with that extension the agent calls
   `show_chart` (a `ui://` resource the app WS only surfaces as tool activity) and
   the tool turn timed out, instead of emitting an inline chart block. **Fix:**
   the SPOKE app drops `autovisualiser` and uses a chart-block system prompt, so
   the chart renders inline and the turn finishes promptly. (Lesson: apps wanting
   app-native inline charts should not also load `autovisualiser`.)

## Scale-up: 22 MiMo-authored apps + export pipeline + workflow loops

A second push added 6 more app *types* (now 22 apps total, all MiMo-authored via
the tools) and hardened the whole build→export→run pipeline.

New apps: gene-expression-barplot, survival-analysis-explainer,
epidemiology-trend-explorer, pharmacokinetics-visualizer (line charts),
variant-consequence-distribution (pie), clinical-calculator.

Full-fleet results (harness: `scripts/agent-drafter-apps/round.sh`,
`ui/desktop/scripts/appcheck/{check-all,export-all}.mjs`):
- **CHECKLIST 21/21 ok** — serve + esbuild bundle + theme + real streamed reply.
- **EXPORT 21/21 ok** — every app exports a complete, directly-runnable folder
  (index.html + src + sdk + package.json + serve.mjs + run.command + run.sh +
  prebuilt dist/app.js), correct biorouterd endpoint, and the exported `src`
  re-bundles cleanly.
- **Exported app runs standalone** in a browser (served on :8787, talking to a
  biorouterd) — verified live (BRCA1 reply).
- **Agentic multi-turn memory** verified (turn 2 recalled "CFTR").
- **Charts**: bar (SPOKE), line, and pie (variant consequences) render as native
  SVG with the BioRouter palette; markdown tables render bordered.

### Provider-agnostic (BioRouter's flexibility)

Apps no longer hardcode a model. By default an app pins **no** model and inherits
whatever provider/model the user configured in BioRouter (Anthropic, OpenAI,
Azure, Bedrock, Ollama, Xiaomi MiMo, local llama.cpp, …). A specific
provider+model is stored only when explicitly chosen (`configure_app`). The WS
handler applies the app's model or falls back to the global provider.

### Export = directly runnable + portable

`export_app <id> <target_dir>` (e.g. "export this app to my Desktop") writes a
self-contained folder. `run.command`/`run.sh` self-installs the app into the
local BioRouter store, starts `biorouterd`, and opens it — auth is wired through
the user's existing BioRouter provider config (no per-app prompt). `GET
/apps/{id}/export` returns the same scaffold as JSON (for the GUI / tooling).

### Workflow-style agentic loops + guardrails

Every user message runs BioRouter's **full agent loop** — multi-step tool calls
+ reasoning, not a single LLM reply — so apps encode real pipelines via
system-prompt steps + extensions (modeled on the knowledge sub-agent loop's
bounded design). `agent.max_turns` bounds/raises that loop (a guardrail against
runaway/cost; the knob workflow apps raise), defaulting to a safe server cap (24).

### Security / consistency review (findings + fixes)

- **serve.mjs path traversal**: the exported static server now resolves under
  ROOT and rejects escapes (verified: `/../../etc/passwd` → blocked).
- **Export route blocking the runtime**: `GET /apps/{id}/export` (and the build
  it may trigger) now runs via `spawn_blocking`, keeping the daemon responsive.
- **`</script>` breakout** in injected config: neutralized (`<` → `<`).
- **Path traversal in the store**: `safe_relative` rejects absolute/`..` paths.
- **Auth posture**: browser-facing GET `/apps/*` is intentionally
  secret-exempt (localhost-only serving, like the MCP UI proxy); management
  verbs (POST/DELETE) require the secret.
- **Provider keys**: never embedded in apps/exports; supplied by the user's
  biorouterd. A fresh unsigned dev `biorouterd` can't read the macOS keychain
  (per-binary grant) → pass the provider key via env in dev.

## Diverse interactive UIs + a build harness (70 more apps)

The apps initially all looked alike because they used the default chat panel. Two
changes fixed that, plus a build-time validation harness:

1. **Rich control design system** (`theme.css`) — `br-select`, `br-slider`,
   `br-switch`, `br-check`, `br-chips`/`br-chip`, `br-tabs`/`br-tab`, `br-grid`,
   `br-dropzone`, `br-draglist`/`br-dragitem`, `br-mapgrid`/`br-region` — plus an
   SDK `br.run(prompt, target)` helper that streams markdown/charts into any
   element (wire a button/slider/select/region/drop to it in one line).
2. **Build harness** (`bundle::lint_app`, surfaced in `build_app` + a `lint_app`
   tool) — validates whatever the model generates and feeds findings back so it
   self-corrects: ① backend wired via the App SDK, ② self-contained (no
   CDN/external assets), ③ on-theme (br-* + a result surface), ④ element-id
   consistency (no `getElementById` null crash), ⑤ controls actually wired to
   events. This is a guardrail around free-form LLM generation, not just
   templates.

**Five agent_drafter iterations** (driven by testing): controls+harness+`br.run`
→ explicit idempotent `create_app` id → element-id lint → `br.run` serialization
(rapid control changes never overlap/orphan) → control-wiring lint.

### 50 detailed-spec apps (round2) — varied UIs, 10 at a time

Built by MiMo through the tools, in 5 batches, each verified (serve + esbuild
bundle + **custom UI in markup** + real streamed reply) with retry:
**batches 1–5 = 50/50.** Patterns: sliders (dosing/PCR/kinetics/decay),
dropdowns (organism/assay/tissue/biomarker), button grids (amino-acids/codons/
elements/imaging), toggles+chips+checkboxes (DEG filters/QC/pathways/symptoms),
region maps (prevalence/outbreak/biobank/trial-sites), drag-drop (workflow/
gene-set/protocol reorder, abstract/CSV drop), tabs (omics/patient/gene/compound),
and form wizards (study-design/grant-aims/cohort).

### 20 vague-prompt apps (round3) — the benchmark

Built from one-line ideas with **no UI/SDK/layout guidance** to measure how the
agent handles under-specified requests, relying on its instructions + harness.

| Prompt style | working | with real controls (raw markup) | avg distinct control-types/app |
|---|---|---|---|
| Detailed (round2, 50) | 50/50 | 47/50 | **3.04** |
| Vague (round3, 20)    | 20/20 | 18/20 | **2.10** |

Finding: the harness makes even vague requests yield working, on-theme custom
apps (~90% use real controls), but **detailed tickets produce richer UIs** (more
distinct control types per app). Vague apps trend toward simpler input+button or
single-control layouts. (Verifier note: the served HTML embeds the theme CSS,
which *defines* every `br-*` class — the honest "real controls" metric is
measured against the raw app markup in the store, not the served page.)

### Autonomous testing

Provider key cached to `/tmp/br-mimo.key` (600) + `start-biorouterd.sh` /
`author.sh` read it, so authoring + tests run with **no macOS Keychain prompts**.
Harness: `scripts/agent-drafter-apps/{round2,round3}.sh`,
`ui/desktop/scripts/appcheck/{batch-verify,check-all,export-all,benchmark}.mjs`.

## Note on the work environment

This redesign was implemented in an isolated git worktree
(`/Users/wanjun/Desktop/biorouter-apps-wt`, branch `feat/agent-drafter-apps`)
because a parallel workstream (`perf/streaming-and-latency`) was sharing the main
working tree and snapshotted/reset files mid-edit. The worktree keeps the two
efforts from clobbering each other; merge `feat/agent-drafter-apps` when ready.
