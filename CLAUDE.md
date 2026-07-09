# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**BioRouter** (v1.87.2) is an AI-powered integrated research environment for biomedical discovery built by UCSF's Baranzini Lab. It unifies multiple LLM providers, AI agents, MCP-based extensions, and customizable workflows into a single extensible tool. The architecture has three layers: Interface (Electron GUI or CLI) → Agent (reasoning loop with session state) → Extensions (pluggable MCP servers providing tools).

## Tech Stack

- **Backend:** Rust workspace (`crates/`) with Rust 1.92 (see `rust-toolchain.toml`)
- **Frontend:** Electron + React 19 + TypeScript, built with Vite and packaged via Electron Forge
- **Task runner:** `just` (see `Justfile` for all available tasks)
- **Node requirement:** Node 24+

## Key Commands

### Development

```bash
source bin/activate-hermit      # Activate hermit environment (run first)
cargo build                     # Debug build of Rust backend
just install-deps               # Install npm/Yarn deps (run once)
just run-ui                     # Build backend + frontend and launch GUI
just run-dev                    # Build debug (not release) backend + launch GUI
just run-server                 # Run REST API server only (biorouterd)
just run-ui-only                # Run frontend without rebuilding backend
just debug-ui                   # Run frontend against external backend
just debug-server               # Run server with secret=test (pairs with debug-ui)
just debug-ui-main-process      # Run UI with Chrome DevTools on localhost:9229
```

### Testing

```bash
cargo test                                      # Run all Rust tests
cargo test -p biorouter-mcp                     # Run tests for a single crate
cargo test --test mcp_integration_test          # Run MCP integration tests
BIOROUTER_RECORD_MCP=1 just record-mcp-tests    # Re-record MCP test cassettes
cd ui/desktop && npm run test:run               # Run frontend unit tests (Vitest)
cd ui/desktop && npm run test-e2e               # Run Playwright E2E tests
```

### Code Quality

```bash
cargo fmt                           # Format Rust code
./scripts/clippy-lint.sh            # Run clippy linter
cd ui/desktop && npm run lint:check # ESLint + Prettier check
just check-everything               # Run all style/lint checks
```

### Build & Release

```bash
just release-binary     # macOS ARM64 release build
just release-intel      # macOS Intel release build
just make-ui            # Package Electron app (macOS ARM64)
just make-ui-linux      # Package for Linux (requires Docker)
just make-ui-windows    # Package for Windows (requires Docker)
just generate-openapi   # Regenerate OpenAPI spec from server routes
```

**Cargo build profiles** (defined in the root `Cargo.toml`):
- `release` — the default for `cargo build --release`, the Justfile, and the whole
  `scripts/release.sh` pipeline. Now sets `strip = true`, which removes the symbol
  table from shipped binaries (~13% smaller: biorouterd 124→108 MB, biorouter
  138→120 MB) with no runtime change. Debug info was already off. Every shipped
  artifact benefits automatically — no pipeline change needed.
- `release-dist` — `cargo build --profile release-dist`. Inherits `release` (so
  stripped) and adds thin LTO + 16 codegen-units for the smallest/fastest binary.
  Slower to compile, so it is opt-in. Intended as the final distribution profile;
  wiring `just release-binary`/`scripts/release.sh` to it is a safe follow-up once
  the macOS notarized packaging path has had a smoke test.
- `quick` — `cargo build --profile quick`. opt-level 1 + max codegen-units +
  incremental, for fast iteration on optimized builds. Everyday dev still uses the
  debug `cargo build` / `cargo check`.

### Releasing (cross-platform)

**Automated path (preferred): `scripts/release.sh` and the `release` workflow.**
The entire pipeline — version bump → compile all 4 backends → sign + **notarize**
both macOS dmgs → package Windows zip + Linux deb/rpm → verify → publish to GitHub —
is encoded in [`scripts/release.sh`](scripts/release.sh). It bakes in every
hard-won invariant below (Node-24 dmg maker, winpthread + `LZMA_API_STATIC`
cross-compile fixes, one-platform-at-a-time staging, Linux-last node_modules
order, auto-installing the `appdmg` dmg dep, notarization creds read from
`notarization/APPLE_DEVELOPER_NOTES.md`).

```bash
scripts/release.sh all 1.80.1          # whole release end-to-end
# or one phase at a time (resumable):
scripts/release.sh bump 1.80.1
scripts/release.sh backends 1.80.1     # mac arm64/x64 + windows + linux (docker)
scripts/release.sh linux-backend 1.80.1 # just the linux x86_64 backend (re-runnable)
scripts/release.sh mac-arm64 1.80.1    # sign + notarize
scripts/release.sh mac-intel 1.80.1
scripts/release.sh windows 1.80.1
scripts/release.sh linux 1.80.1        # GUI deb + rpm; run LAST (corrupts node_modules)
scripts/release.sh cli-linux 1.80.1    # headless CLI-only deb + rpm (biorouter + biorouterd)
scripts/release.sh verify 1.80.1
scripts/release.sh publish 1.80.1
```

For an agent-orchestrated run (each phase as a verified subagent that stops on
the first failure), use the **`release` workflow** in
[`.claude/workflows/release.js`](.claude/workflows/release.js):
`Workflow({ name: 'release', args: { version: '1.80.1' } })`. After a release,
restore a mac-native node_modules: `cd ui/desktop && rm -rf node_modules && npm install`.

The detailed manual steps and the reasoning behind each invariant follow.

- **Version bump**: edit 5 files — `Cargo.toml`, `ui/desktop/package.json`, `ui/desktop/package-lock.json` (2 occurrences), `ui/desktop/openapi.json`. Then `cargo check` to refresh `Cargo.lock`. (`scripts/release.sh bump <ver>` does this.)
- **One version, three binaries (CLI = daemon = GUI)**: the version lives in exactly one source of truth — `[workspace.package].version` in `Cargo.toml`. The CLI (`biorouter`), the daemon (`biorouterd`), and the core library all use `version.workspace = true`, so the three Rust binaries can **never** disagree at build time — they are compiled from the same workspace version (surfaced via `env!("CARGO_PKG_VERSION")`). The desktop GUI keeps its own copy in `ui/desktop/package.json` (+ two in `package-lock.json`, one in `openapi.json`); `release.sh bump` rewrites all of them in lockstep. `scripts/check-version-consistency.sh` (run by `just check-everything` / `just check-versions`) is the guard that fails CI if any desktop JSON drifts from the Cargo version, or if a crate ever hardcodes its own `version` instead of inheriting the workspace one. **Do not hand-edit a single version file** — always use `scripts/release.sh bump <ver>` so all five stay in sync.
- **Runtime CLI-vs-app drift** (the "Biorouter CLI Update 1.20.0 → 1.85.1" prompt): the GUI bundles a `biorouterd` that always matches the app, but the user's terminal `biorouter` is a *separately installed* binary that can lag. This is an install-state mismatch, not a source-version bug — the in-app "Biorouter CLI Update" card re-installs/symlinks the matching CLI (in a dev tree it points `~/.local/bin/biorouter` at `target/debug/biorouter`). The source versions are already linked; only the on-PATH install can be stale.
- **macOS dmg maker needs Node 24**: the `macos-alias` / `appdmg` native modules only build under hermit's Node (v24), not a newer Homebrew Node — run all packaging under `source bin/activate-hermit`. If the dmg maker dies with `Cannot find module 'appdmg'` or a `NODE_MODULE_VERSION` mismatch, `(cd ui/desktop && npm install && npm rebuild macos-alias ds-store)`.
- **Cross-compile link fixes** (windows-gnu / linux-gnu, in the Justfile + `release.sh`): `aws-lc-sys` needs winpthread appended *after* the rlibs on the mingw link line (linker wrapper); `lzma-sys` (via `xz2`, the `.brkb` path) needs `LZMA_API_STATIC=1` so it statically builds bundled liblzma instead of the host one. Run the docker cross builds with the system docker (hermit does **not** shadow it).
- **macOS sign + notarize**: set `APPLE_ID` and `APPLE_APP_SPECIFIC_PASSWORD` on the `npm run bundle:default` / `bundle:intel` invocation. Signing identity is the UCSF Developer ID Application (team `F3YYBXAFJ8`).
- **Intel macOS requires `just release-intel` first**. `bundle:intel` does NOT cross-compile the Rust backend — it repackages whatever is in `ui/desktop/src/bin/`. Without `target/x86_64-apple-darwin/release/{biorouter,biorouterd}`, `prepare-platform-binaries.js` falls through to the arm64 build and ships an Intel dmg that crashes on Intel Macs with "bad CPU type." Always run `just release-intel` (or have a recent `target/x86_64-apple-darwin/release/` build) immediately before `npm run bundle:intel`. Verify with `file ui/desktop/out/BioRouter-darwin-x64/BioRouter.app/Contents/Resources/bin/biorouter` — must say `x86_64`, not `arm64`. Same rule applies symmetrically: `bundle:default` needs `target/release/` to be the arm64 build (`just release-binary` or `just copy-binary`).
- **Build platforms one at a time** — every bundle writes to `ui/desktop/src/bin/` and clobbers the others. After any non-mac build, run `just release-binary` (or `just copy-binary`) to restore the local arm64 binary.
- **After Linux/Windows Docker builds**, the on-disk `ui/desktop/node_modules` is Linux-flavored — macOS bundle then fails with `@rollup/rollup-darwin-arm64` missing. Fix: `cd ui/desktop && rm -rf node_modules && npm install`.
- **`macos-alias` `NODE_MODULE_VERSION` mismatch** during forge `make`: `cd ui/desktop && npm rebuild macos-alias`.
- **Unmount any stale `/Volumes/BioRouter*` mounts before the dmg step** — leftover mounts cause `cp: Operation not permitted` and abort `electron-forge maker-dmg`.
- **Do not hand-roll the dmg via `hdiutil create`** — it skips the `Applications` symlink and the background-image layout that `electron-forge maker-dmg` adds. If `bundle:default` fails at the dmg step, fix the underlying cause (usually a stale `/Volumes` mount) and re-run, don't `hdiutil` over it.
- **Release assets — exactly 10**: the 5 GUI artifacts `BioRouter-{ver}-arm64.dmg`, `BioRouter-{ver}-x64.dmg`, `biorouter_{ver}_amd64.deb`, `BioRouter-{ver}-1.x86_64.rpm`, `BioRouter-win32-x64-{ver}.zip`; the 2 **headless CLI-only** Linux packages `biorouter-cli_{ver}_amd64.deb` and `biorouter-cli-{ver}-1.x86_64.rpm` (just `biorouter` + `biorouterd`, built by `scripts/build-cli-linux-packages.sh` → `dist/cli/`, smoke-tested in clean Debian/Rocky containers); and the 3 **macOS auto-update** artifacts `BioRouter-darwin-arm64-{ver}.zip`, `BioRouter-darwin-x64-{ver}.zip`, and `latest-mac.yml`. The two darwin zips are the signed+notarized `maker-zip` app archives (Squirrel.Mac format, `BioRouter.app` at root); `latest-mac.yml` (generated by `ui/desktop/scripts/generate-update-manifests.js`, run from `scripts/release.sh publish` / the `mac-manifest` phase) lists both with base64 SHA-512 + size so `electron-updater` can do the in-app one-click "Restart & Update" on macOS — **without the zips + yml, clients 404 and drop to the assisted GitHub-download fallback**. electron-updater 6.x picks the arch by the `arm64`/`x64` token in the zip filename, so both clients share one manifest. Don't also upload the unversioned `BioRouter.zip` / `BioRouter_intel_mac.zip` from `out/<platform>/` — they're build intermediates, not release artifacts. Windows (plain zip) and Linux (deb/rpm) have no electron-updater in-place installer and keep using the assisted-download fallback. See `docs/AUTO_UPDATE_TESTING_CHECKLIST.md`.
- **Linux glibc baseline**: the linux backend is cross-compiled on `rust:1.92-bullseye` (glibc 2.31), NOT rolling `rust:latest` (now trixie, glibc 2.39, which yields binaries that fail to start on Debian 12 / Ubuntu 22.04 / RHEL-Rocky 9). The pin lives in `LINUX_RUST_IMG` in `scripts/release.sh`; `cli-linux`'s smoke test is what catches a regression here. `linux-backend` `rm -rf`s the target dir first to force a from-scratch compile against the pinned glibc (cached objects keep stale symbol versions).
- **Publish**: `gh release create v{ver} --notes-file docs/release-notes/v{ver}.md <5 assets>`. Verify macOS with `xcrun stapler validate <app>` and `codesign -dv <app>` before publishing.
- **Release notes** live at `docs/release-notes/v{ver}.md`, one file per version.

## Architecture

### Rust Workspace (`crates/`)

| Crate | Binary | Purpose |
|-------|--------|---------|
| `biorouter` | — | Core agent library: main agent loop, LLM providers, MCP extension manager, session/conversation state, workflow execution, scheduling |
| `biorouter-server` | `biorouterd` | Axum REST API + WebSocket server; routes in `src/routes/`; OpenAPI spec generated via utoipa |
| `biorouter-cli` | `biorouter` | Interactive CLI; subcommands in `src/commands/` |
| `biorouter-mcp` | — | Built-in MCP servers (Developer, Computer Controller, Memory, Auto Visualiser, Tutorial, Knowledge) |
| `biorouter-acp` | — | Agent Communication Protocol for multi-agent orchestration |
| `biorouter-bench` | — | Benchmarking harness |
| `biorouter-test` | — | Integration tests |

### Core Agent Library (`crates/biorouter/src/`)

- **`agents/agent.rs`** (~77KB) — Main agent loop: LLM interaction, tool dispatch, context management
- **`agents/extension_manager.rs`** (~71KB) — MCP extension lifecycle and tool registration
- **`providers/`** — 43+ provider modules (Anthropic, OpenAI, Azure, AWS Bedrock, Databricks, Ollama, etc.); `factory.rs` creates providers, `base.rs` defines the abstract interface
- **`session/`** — Session persistence (SQLite via sqlx)
- **`workflow/`** — Workflow definition (YAML/JSON), Jinja-style templating (minijinja), and execution
- **`context_mgmt/`** — Token counting (tiktoken-rs) and context window pruning
- **`security/`** — Permission modes, `.biorouterignore` handling
- **`scheduler.rs`** — Cron-based job scheduling (tokio-cron-scheduler)
- **`knowledge/`** — Personal knowledge base: storage, git history, file
  conversion (HTML/PDF/DOCX/CSV), credibility classification
  (Crossref/OpenAlex), graph derivation, **macros (ingest / query / lint)
  backed by a bounded sub-agent loop**, BM25 search, per-KB concurrency
  mutex, and an active-KB state for session-scoped tool defaulting. The
  shared service backs the `knowledge` MCP extension and HTTP routes under
  `/knowledge/*` via `biorouter-server` (with SSE-streamed macros and
  `.brkb` export/import). (Module lives in `biorouter-mcp`; re-exported as
  `biorouter::knowledge`.)

  Note: the macros (`ingest`, `query`, `lint`) and the agentic credibility
  fallback take a `Box<dyn Completer>` argument rather than a `Provider`
  directly, to avoid a circular dependency on `biorouter`. The HTTP routes
  wrap a real `biorouter::providers::Provider` in a `ProviderCompleter`
  adapter (`crates/biorouter/src/knowledge/provider_completer.rs`).

### Frontend (`ui/desktop/src/`)

- **`main.ts`** — Electron main process: spawns `biorouterd`, manages windows, IPC
- **`preload.ts`** — IPC bridge between renderer and main process
- **`api/`** — TypeScript API client auto-generated from OpenAPI spec (do not hand-edit)
- **`components/`** — 64+ modular React UI components
- **`contexts/`** — React Context for global state
- **`workflow/`** — Workflow builder UI
- **`components/knowledge/`** — Top-level Knowledge route in the sidebar
  (between Skills and Settings). Provides KB selector with cmd-K-style
  palette, ingest panel (dropzone / paste text with URL extraction / staged
  list), and live SSE-streamed digestion progress via `useIngestStream`.
  Graph view + change-log drawer come in Plan 5.

### Knowledge feature

The Knowledge feature (built across Plans 1-6 in `docs/superpowers/plans/2026-05-30..2026-06-01-knowledge-*`) provides personal, LLM-maintained knowledge bases backed by markdown trees + git history.

- **Backend module:** `crates/biorouter-mcp/src/knowledge/` (types, store, git, graph, credibility, convert/, macros/, subagent/loop_, MCP server).
- **HTTP routes:** `crates/biorouter-server/src/routes/knowledge.rs` covers `/knowledge/bases`, `/ingest` (SSE), `/graph`, `/history`, `/preview`, `/restore`, `/page`, `/active`, `/export`, `/import`.
- **Frontend:** `ui/desktop/src/components/knowledge/` (view shell, KB selector, ingest panel, force-graph + change-log drawer). The chat-side KB chip lives at `ui/desktop/src/components/bottom_menu/BottomMenuKnowledgeSelection.tsx`.
- **Storage layout:** `~/.config/biorouter/knowledge/<kb-id>/` with `raw/`, `knowledge/`, `index.md`, `log.md`, `schema.md`, and a hidden `.git/`. The active-KB id is persisted at `~/.config/biorouter/knowledge/.active-kb`.
- **Sub-agent loop:** `crates/biorouter-mcp/src/knowledge/subagent/loop_.rs` drives ingest / query / lint macros. Mutating tools accept an optional `txn` so a macro's tool calls commit as one logical change.

When working on the Knowledge feature:
- Run `cargo test -p biorouter-mcp --lib knowledge::` (~122 tests) and `cargo test -p biorouter-server --test knowledge_routes` (~19 tests) for backend changes.
- After touching `routes/knowledge.rs`, regenerate the TS client with `just generate-openapi && cd ui/desktop && npm run generate-api`.
- Graph derivation lives in `graph.rs` and depends on the sub-agent emitting `[[knowledge-link]]` markers in page bodies; the default `schema_default.md` reinforces this. If a graph has nodes but no edges, the underlying pages likely lack `[[…]]` cross-references.

### Llama Server (bundled llama.cpp local models)

The "Llama Server" provider (`llamacpp`) gives zero-setup local models: the
desktop app bundles a pinned llama.cpp `llama-server` binary and manages it as
a sidecar process. It is ranked first among Local Models (before Ollama), and
Local ranks before Institutional/Commercial everywhere (GUI onboarding,
settings provider grid, `biorouter configure`).

- **Provider:** `crates/biorouter/src/providers/llamacpp.rs` — OpenAI-compat
  HTTP to the sidecar; curated `MODEL_CATALOG` (Qwen3.5 + Gemma 4 families,
  Q4_K_M from verified `unsloth/*-GGUF` HF repos; default `qwen3.5-4b`).
  Unlisted models accepted as raw `owner/repo:QUANT` HF specs.
- **Sidecar manager:** `crates/biorouter/src/providers/llamacpp_sidecar.rs` —
  binary discovery (`BIOROUTER_LLAMACPP_BIN` → `<exe dir>/llamacpp/` → dev
  repo path → PATH), spawn with `-hf` (models download from Hugging Face into
  the data dir on first use), `/health` readiness, status snapshots, restart
  on model switch. Defaults: port 11543, q8_0 KV cache, and a **model-native
  context window** (`--ctx-size 0`, so it tracks the loaded model; the live
  window is read back from `/props` and reported for token accounting —
  `LLAMACPP_CONTEXT_SIZE` pins/caps it, e.g. on memory-constrained machines
  where a large native window like qwen3.5-4b's 262k is too heavy; 32k is only
  a fallback when the live value is unreadable). Thinking is **enabled** by
  default via `--reasoning on` (`LLAMACPP_ENABLE_THINKING=false` → `--reasoning
  off`). `LLAMACPP_EXTRA_ARGS` for anything else, `LLAMACPP_EXTERNAL_HOST` to
  use an unmanaged server.
  **Orphan reaping:** statics never drop, so `kill_on_drop` cannot cover
  process exit; spawns are recorded in `<data>/llamacpp/run/<ppid>.pid` and
  the next `ensure()` in any Biorouter process kills children of dead parents.
- **Pinned build:** `LLAMA_SERVER_BUILD` in the sidecar must match
  `LLAMA_BUILD` in `ui/desktop/scripts/fetch-llama-server.js`, which downloads
  per-platform archives at package time into `src/bin/llamacpp/` (mac = Metal
  ~10 MB, win = Vulkan ~37 MB with CPU fallback via ggml dynamic backends,
  linux = CPU ~15 MB; CUDA stays opt-in via `BIOROUTER_LLAMACPP_BIN`). Bump
  the pin deliberately and smoke-test — llama.cpp releases multiple times a
  day with no semver.
- **Linux floor:** llama-server needs glibc ≥ 2.35-ish plus `libssl3` and
  `libgomp1` (declared in the deb/rpm maker configs in `forge.config.ts`) —
  i.e. Debian 12+/Ubuntu 22.04+. Debian 11 runs the app but not local models.
- **HTTP routes:** `/llamacpp/status|ensure|stop` in
  `crates/biorouter-server/src/routes/llamacpp.rs` (status includes the
  catalog; ensure is async — poll status for download progress).
- **Frontend:** onboarding card `LlamaServerInlineCard.tsx` (first card),
  provider ordering in `providerOrdering.ts` + section order in
  `ProviderGrid.tsx`.
- **Tests:** unit tests in both modules; route tests
  `cargo test -p biorouter-server --test llamacpp_routes`; live end-to-end
  (real server + tiny Qwen3.5 0.8B, ~0.5 GB one-time download):
  `BIOROUTER_LLAMACPP_BIN=ui/desktop/src/bin/llamacpp/llama-server cargo test -p biorouter --test llamacpp_integration -- --ignored --test-threads=1`

### Auto Visualiser feature

The Auto Visualiser (`autovisualiser`) built-in MCP server turns structured data
into self-contained interactive HTML figures, returned as `ui://…` resources and
rendered inline in chat (sandboxed iframe via `@mcp-ui` + the `/mcp-ui-proxy`).

- **Module:** `crates/biorouter-mcp/src/autovisualiser/` — `mod.rs` (router +
  the 8 original tools), `common.rs` (shared infra), `tools_extra.rs` (Mermaid
  wrappers), `tools_charts.rs` (Chart.js), `tools_d3.rs` (D3), `tools_geo.rs`
  (Leaflet), `tests.rs` + `tests_extra.rs`. The `tools_*.rs` files are
  `include!`d into `mod.rs`; each defines a `#[tool_router(router = …)]` impl
  block, combined in `new()` via `ToolRouter` `+`.
- **Shared pipeline (`common.rs`):** validate → JSON-encode safely (`js_data`
  neutralises `</script>` breakout) → `assemble` template with `{{ASSETS}}` +
  `{{COMMON}}` (the shared `templates/_common.js`: theme, palette, auto-resize,
  global error card) → base64 `ui://` blob (`finish`). Every tool also enforces
  size limits + semantic checks and returns a friendly `INVALID_PARAMS` message
  instead of producing a broken figure.
- **Tools (33):** charts (`show_chart`, `render_histogram`, `render_boxplot`,
  `render_bubble`, `render_area`, `render_radar`, `render_donut`, `render_gauge`);
  scientific (`render_volcano`, `render_manhattan`, `render_kaplan_meier`,
  `render_forest`); relationships/hierarchies (`render_network`, `render_sankey`,
  `render_chord`, `render_heatmap`, `render_treemap`, `render_sunburst`,
  `render_dendrogram`, `render_wordcloud`, `render_calendar_heatmap`); diagrams
  (`render_mermaid` + typed wrappers `render_flowchart`/`gantt`/`sequence`/
  `mindmap`/`timeline`/`er_diagram`/`state_diagram`/`class_diagram`); geo
  (`render_map`, `render_choropleth`).
- **Assets:** libraries (D3, Chart.js, Leaflet, Mermaid) are inlined by default
  for offline use. `BIOROUTER_AUTOVIS_CDN=1` switches to pinned CDN tags, which
  shrinks the persisted/reloaded blob from megabytes to a few KB (recommended if
  large Mermaid diagrams fail to re-render on chat reopen).
  `BIOROUTER_AUTOVIS_DEBUG=1` (or debug builds) dumps generated HTML to the app
  cache dir (`<cache>/autovisualiser/<name>-<pid>.html`).
- **Tests:** `cargo test -p biorouter-mcp --lib autovisualiser` (happy paths,
  edge cases, escaping, lenient enum parsing).

### Communication Flow

```
CLI → calls biorouter crate APIs directly
GUI → Electron main spawns biorouterd → React renderer communicates via HTTP/WebSocket
                                       (type-safe client from generated OpenAPI)
```

After changing server routes, always run `just generate-openapi` to regenerate the TypeScript client.

## Configuration

- User config: `~/.config/biorouter/config.yaml` (providers, API keys, extensions)
- Session history: `~/.config/biorouter/sessions/` (SQLite)
- Workflows/skills: `~/.config/biorouter/workflows/` and `~/.config/biorouter/skills/`
- Secrets: OS credential store (macOS Keychain / Windows Credential Manager / Linux Secret Service) via the `keyring` crate, read once per process and cached in memory so macOS shows at most one Keychain authorization prompt per run (tell users to click "Always Allow"). `BIOROUTER_DISABLE_KEYRING=true` switches to plaintext `secrets.yaml`; headless Linux falls back to it automatically. On Windows the secrets blob is chunked across credentials (2560-byte cap each). `just copy-binary` re-signs dev binaries with the Developer ID (when present) so Keychain grants survive rebuilds. Logic in `crates/biorouter/src/config/base.rs`; see `docs/guides/secret-storage.md`.

Key environment variables:
- `ALPHA=true` — Enable alpha features
- `BIOROUTER_EXTERNAL_BACKEND=true` — Use external backend (for UI dev)
- `BIOROUTER_EXTERNAL_PORT` — Backend port (default 3000)
- `BIOROUTER_SERVER__SECRET_KEY` — Server auth key (default `test` in debug-server mode); uses `__` for nested config keys
- `BIOROUTER_RECORD_MCP=1` — Re-record MCP integration test cassettes (VCR-style)
- `BIOROUTER_UPDATE_FEED_URL` — Override the auto-update feed (GitHub by default) with a generic static-file feed (`latest-mac.yml` + per-arch app zips). For self-hosted/enterprise update mirrors and for the one-click-update swap test (`ui/desktop/scripts/notarized-swap-test.sh`). See `docs/AUTO_UPDATE_TESTING_CHECKLIST.md`.
- `BIOROUTER_UPDATE_AUTO_INSTALL=1` — Test-only: auto-trigger `quitAndInstall` on download (gated behind `BIOROUTER_UPDATE_FEED_URL`, so it never fires in production).
- `BIOROUTER_SKIP_NOTARIZE=1` — Build a signed-but-not-notarized macOS app (forge.config.ts), for fast local/test builds. Release builds leave it unset (fully notarized + stapled).

**macOS packaging gotchas (learned building the one-click-update test):** package under **hermit Node 24** (`source bin/activate-hermit`), not Homebrew Node 26 — under Node 26 `electron-forge package` silently no-ops (exits 0 with no `.app`). And ensure `ui/desktop/src/bin/{biorouter,biorouterd}` are the macOS **arm64** Mach-O binaries (`just copy-binary` or restage from `target/release/`), not the Linux ELF binaries a prior Linux release leaves behind — otherwise the bundled backend can't exec and the app quits on launch. Build only `--targets @electron-forge/maker-zip` to skip the DMG maker's `appdmg` native dep when you just need the auto-update zip.

## Code Review Standards

From `.github/copilot-instructions.md`: Reviews focus on **security, correctness, and architecture patterns** — not style (handled by CI) or refactoring suggestions. Flag issues only with >80% confidence. Security-sensitive code (auth, permissions, credential handling) requires human review regardless of AI assistance. Note: this file previously referenced old crate names but has been updated to use `biorouter`, `biorouter-cli`, `biorouter-server`, `biorouter-mcp`.

From `HOWTOAI.md`: Avoid using AI-generated code for security logic, complex business rules, or schema migrations without thorough human review. Always get human review for MCP protocol implementations and async/concurrency logic.

Use `.biorouterhints` to guide BioRouter's coding style (patterns, error handling, tests) and `.biorouterignore` to protect sensitive files from being read by the agent.

## Connected Repositories & Ecosystem

BioRouter is the hub of a small ecosystem of Baranzini Lab / UCSF repositories: the app itself, its public website + marketplace, a shareable-skills repo, and a set of installable MCP "agent" extensions and skill packs. This section maps every related repo/resource so a future session knows how the pieces fit together. All GitHub URLs below were verified to return HTTP 200 on 2026-06-20 unless noted.

### Core repos

| Repo | Purpose | Local path | GitHub |
|------|---------|------------|--------|
| **biorouter** (this repo) | The main app: Rust workspace (CLI `biorouter`, daemon `biorouterd`, core agent lib, MCP servers) + Electron/React desktop GUI. | `/Users/wgu/Desktop/BioRouter` | https://github.com/BaranziniLab/biorouter |
| **landing site** (now in this repo) | Public website + docs, deployed at **http://biorouter.ucsf.edu/** (custom-domain GitHub Pages, `CNAME` = `biorouter.ucsf.edu`). Hosts the BAAM marketplace (`baam.html`), `download.html`, `docs.html`, `skills.html`, and the machine-readable **`registry.json`** (the authoritative marketplace catalog of extensions + skills). **As of the 2026-06-29 consolidation it lives in this repo under [`landing/`](landing/)** and is published by [`.github/workflows/deploy-landing.yml`](.github/workflows/deploy-landing.yml) (Pages source = "GitHub Actions") — so the site ships and versions with the app. The `biorouter.ucsf.edu` custom domain was **cut over** to this repo (released from `biorouter-landing`, claimed on `biorouter`); the live site now serves from `landing/`. The standalone `biorouter-landing` repo was **deleted** (remote + local) on 2026-06-29 after verification; its full history was archived to a git bundle (`~/Desktop/biorouter-landing-archive-2026-06-29.bundle`) kept as insurance. This `landing/` folder is now the single source of truth for the site. See [`landing/MIGRATION.md`](landing/MIGRATION.md). | `/Users/wgu/Desktop/BioRouter/landing` | https://github.com/BaranziniLab/biorouter (was: https://github.com/BaranziniLab/biorouter-landing) |
| **biorouter-skills** | Shareable Biorouter skills. Each skill ships as a GitHub Release asset `skill-<id>` → `<id>.zip`. Local clone has two remotes: `baranzini` → BaranziniLab (canonical) and `origin` → Broccolito (dev fork). | `/Users/wgu/Desktop/biorouter-skills` | https://github.com/BaranziniLab/biorouter-skills (fork: https://github.com/Broccolito/biorouter-skills) |

### Marketplace & docs (BAAM)

**BAAM** = the Biorouter Agent/extension/skill marketplace, served from the landing site. The durable, machine-readable source of truth is `registry.json` in the landing repo (`"source": "https://biorouter.ucsf.edu/baam"`), which the app reads to list and one-click-install extensions (`.brxt` bundles) and skills (`.zip`).

- Marketplace page: http://biorouter.ucsf.edu/baam.html
- Registry (catalog JSON): https://biorouter.ucsf.edu/registry.json (source: `landing/registry.json` in this repo, generated by `landing/scripts/build-registry.mjs`)
- Docs: http://biorouter.ucsf.edu/docs.html · Skills gallery: http://biorouter.ucsf.edu/skills.html · Downloads: http://biorouter.ucsf.edu/download.html
- Baranzini Lab: https://baranzinilab.ucsf.edu/ · UCSF: https://www.ucsf.edu

### Extension agents (installable MCP servers, `.brxt` bundles)

These are pluggable MCP extensions distributed via the marketplace; each lives in its own repo and publishes a `.brxt` bundle as a GitHub Release asset.

| Agent | Purpose | GitHub | Notes |
|-------|---------|--------|-------|
| **SPOKEAgent** | Cypher queries on the SPOKE biomedical knowledge graph (diseases, genes, proteins, drugs, pathways); bundles a `spoke-knowledge-graph` skill. | https://github.com/BaranziniLab/SPOKEAgent | Needs `SPOKEAGENT_PASSCODE` (UCSF wiki credentials page). |
| **UCSFOMOPAgent** | Natural-language SQL over the UCSF OMOP de-identified clinical database (OMOP CDM EHR data, read-only). | https://github.com/BaranziniLab/UCSFOMOPAgent | Requires UCSF credentials. |
| **CDWAgent** | Multimodal natural-language access to the UCSF Clinical Data Warehouse (cohorts, labs, imaging, notes/NLP); read-only. | https://github.com/BaranziniLab/CDWAgent | Requires UCSF network credentials (`CAMPUS\username`). |
| **PlaywrightAgent** | Browser automation via Microsoft's `@playwright/mcp` (navigate, extract, fill forms); no vision model needed, needs Node.js. | https://github.com/BaranziniLab/PlaywrightAgent | |
| **CodeGraphAgent** | Pre-indexed code knowledge graph ("who calls X?", "what breaks if I change Z?") across 23 languages incl. R/Julia/MATLAB/Perl; tree-sitter fork of CodeGraph. | https://github.com/Broccolito/CodeGraphAgent | Broccolito org. |
| **BiorOffice** | Create/read/edit Word/Excel/PowerPoint from chat (built on OfficeCLI, no MS Office needed); ships four bundled office skills. | https://github.com/Broccolito/BiorOffice | Broccolito org. |

### Skill packs (`biorouter-skills`, ~85 skills)

All skills are published as releases of **`BaranziniLab/biorouter-skills`** (asset path `releases/download/skill-<id>/<id>.zip`). Grouped by category in `registry.json`:

- **Core** (9): `scientific-research`, `taste-skill` (frontend design), `anti-ai-writing`, `ggplot-visualization`, `r-scripting`, `python-scripting`, `ralph`, `superpowers`, `ucsf-hpc`.
- **Developer** (8): `code-review`, `code-simplifier`, `commit-commands`, `skill-creator`, `hookify`, `code-modernization`, `playground`, `frontend-design`.
- **Biomedical** (~68): genomics/omics + clinical bioinformatics skills, e.g. `single-cell`, `variant-calling`, `differential-expression`, `pathway-analysis`, `spatial-transcriptomics`, `proteomics`, `metabolomics`, `chip-seq`, `atac-seq`, `crispr-screens`, `phylogenetics`, `clinical-biostatistics`, `clinical-databases`, `multi-omics-integration`, `workflows`, etc. See `registry.json` for the full list.

### Workflows

- **biorouter-workflows** — shareable workflow YAML definitions (e.g. `ehr-diabetes-dashboard.yaml`, referenced from `baam.html` via `raw.githubusercontent.com`). GitHub: https://github.com/BaranziniLab/biorouter-workflows (the `Broccolito/biorouter-workflows` URL in `baam.html` 301-redirects to the BaranziniLab repo).

> Maintenance note: the authoritative, always-current catalog of extensions and skills is `landing/registry.json` in this repo (formerly the `biorouter-landing` repo). When agents/skills are added or versions change, that file (not this section) is the source of truth — re-derive this section from it if it drifts.
