# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**BioRouter** (v1.60.0) is an AI-powered integrated research environment for biomedical discovery built by UCSF's Baranzini Lab. It unifies multiple LLM providers, AI agents, MCP-based extensions, and customizable workflows into a single extensible tool. The architecture has three layers: Interface (Electron GUI or CLI) → Agent (reasoning loop with session state) → Extensions (pluggable MCP servers providing tools).

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
scripts/release.sh mac-arm64 1.80.1    # sign + notarize
scripts/release.sh mac-intel 1.80.1
scripts/release.sh windows 1.80.1
scripts/release.sh linux 1.80.1        # deb + rpm; run LAST (corrupts node_modules)
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
- **macOS dmg maker needs Node 24**: the `macos-alias` / `appdmg` native modules only build under hermit's Node (v24), not a newer Homebrew Node — run all packaging under `source bin/activate-hermit`. If the dmg maker dies with `Cannot find module 'appdmg'` or a `NODE_MODULE_VERSION` mismatch, `(cd ui/desktop && npm install && npm rebuild macos-alias ds-store)`.
- **Cross-compile link fixes** (windows-gnu / linux-gnu, in the Justfile + `release.sh`): `aws-lc-sys` needs winpthread appended *after* the rlibs on the mingw link line (linker wrapper); `lzma-sys` (via `xz2`, the `.brkb` path) needs `LZMA_API_STATIC=1` so it statically builds bundled liblzma instead of the host one. Run the docker cross builds with the system docker (hermit does **not** shadow it).
- **macOS sign + notarize**: set `APPLE_ID` and `APPLE_APP_SPECIFIC_PASSWORD` on the `npm run bundle:default` / `bundle:intel` invocation. Signing identity is the UCSF Developer ID Application (team `F3YYBXAFJ8`).
- **Intel macOS requires `just release-intel` first**. `bundle:intel` does NOT cross-compile the Rust backend — it repackages whatever is in `ui/desktop/src/bin/`. Without `target/x86_64-apple-darwin/release/{biorouter,biorouterd}`, `prepare-platform-binaries.js` falls through to the arm64 build and ships an Intel dmg that crashes on Intel Macs with "bad CPU type." Always run `just release-intel` (or have a recent `target/x86_64-apple-darwin/release/` build) immediately before `npm run bundle:intel`. Verify with `file ui/desktop/out/BioRouter-darwin-x64/BioRouter.app/Contents/Resources/bin/biorouter` — must say `x86_64`, not `arm64`. Same rule applies symmetrically: `bundle:default` needs `target/release/` to be the arm64 build (`just release-binary` or `just copy-binary`).
- **Build platforms one at a time** — every bundle writes to `ui/desktop/src/bin/` and clobbers the others. After any non-mac build, run `just release-binary` (or `just copy-binary`) to restore the local arm64 binary.
- **After Linux/Windows Docker builds**, the on-disk `ui/desktop/node_modules` is Linux-flavored — macOS bundle then fails with `@rollup/rollup-darwin-arm64` missing. Fix: `cd ui/desktop && rm -rf node_modules && npm install`.
- **`macos-alias` `NODE_MODULE_VERSION` mismatch** during forge `make`: `cd ui/desktop && npm rebuild macos-alias`.
- **Unmount any stale `/Volumes/BioRouter*` mounts before the dmg step** — leftover mounts cause `cp: Operation not permitted` and abort `electron-forge maker-dmg`.
- **Do not hand-roll the dmg via `hdiutil create`** — it skips the `Applications` symlink and the background-image layout that `electron-forge maker-dmg` adds. If `bundle:default` fails at the dmg step, fix the underlying cause (usually a stale `/Volumes` mount) and re-run, don't `hdiutil` over it.
- **Release assets — exactly 5**: `BioRouter-{ver}-arm64.dmg`, `BioRouter-{ver}-x64.dmg`, `biorouter_{ver}_amd64.deb`, `BioRouter-{ver}-1.x86_64.rpm`, `BioRouter-win32-x64-{ver}.zip`. Don't also upload the unversioned `BioRouter.zip` / `BioRouter_intel_mac.zip` from `out/<platform>/` — they're build intermediates, not release artifacts.
- **Publish**: `gh release create v{ver} --notes-file docs/release-notes/v{ver}.md <5 assets>`. Verify macOS with `xcrun stapler validate <app>` and `codesign -dv <app>` before publishing.
- **Release notes** live at `docs/release-notes/v{ver}.md`, one file per version.

## Architecture

### Rust Workspace (`crates/`)

| Crate | Binary | Purpose |
|-------|--------|---------|
| `biorouter` | — | Core agent library: main agent loop, LLM providers, MCP extension manager, session/conversation state, recipe execution, scheduling |
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
- Recipes/skills: `~/.config/biorouter/recipes/` and `~/.config/biorouter/skills/`
- Secrets: OS credential store (macOS Keychain / Windows Credential Manager / Linux Secret Service) via the `keyring` crate, read once per process and cached in memory so macOS shows at most one Keychain authorization prompt per run (tell users to click "Always Allow"). `BIOROUTER_DISABLE_KEYRING=true` switches to plaintext `secrets.yaml`; headless Linux falls back to it automatically. On Windows the secrets blob is chunked across credentials (2560-byte cap each). `just copy-binary` re-signs dev binaries with the Developer ID (when present) so Keychain grants survive rebuilds. Logic in `crates/biorouter/src/config/base.rs`; see `docs/guides/secret-storage.md`.

Key environment variables:
- `ALPHA=true` — Enable alpha features
- `BIOROUTER_EXTERNAL_BACKEND=true` — Use external backend (for UI dev)
- `BIOROUTER_EXTERNAL_PORT` — Backend port (default 3000)
- `BIOROUTER_SERVER__SECRET_KEY` — Server auth key (default `test` in debug-server mode); uses `__` for nested config keys
- `BIOROUTER_RECORD_MCP=1` — Re-record MCP integration test cassettes (VCR-style)

## Code Review Standards

From `.github/copilot-instructions.md`: Reviews focus on **security, correctness, and architecture patterns** — not style (handled by CI) or refactoring suggestions. Flag issues only with >80% confidence. Security-sensitive code (auth, permissions, credential handling) requires human review regardless of AI assistance. Note: this file previously referenced old crate names but has been updated to use `biorouter`, `biorouter-cli`, `biorouter-server`, `biorouter-mcp`.

From `HOWTOAI.md`: Avoid using AI-generated code for security logic, complex business rules, or schema migrations without thorough human review. Always get human review for MCP protocol implementations and async/concurrency logic.

Use `.biorouterhints` to guide BioRouter's coding style (patterns, error handling, tests) and `.biorouterignore` to protect sensitive files from being read by the agent.
