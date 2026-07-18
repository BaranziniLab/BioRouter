# jcode → BioRouter: Performance & Efficiency Comparison

**Date:** 2026-06-24
**Source studied:** [`1jehuang/jcode`](https://github.com/1jehuang/jcode) (cloned for analysis), a Rust coding-agent
harness explicitly engineered for RAM/boot efficiency.
**Target:** BioRouter (this repo) — Rust workspace + Electron/React GUI.
**Method:** 11 parallel sub-agents, each owning one efficiency theme and comparing both codebases at
`file:line` granularity. This document synthesizes their findings into a prioritized roadmap.

> jcode is an unrelated third-party project. Nothing here is copied code — these are *techniques and
> architectural patterns* to borrow, each with evidence from both sides so the gap is verifiable.

---

## Executive summary

jcode and BioRouter are both Rust agent harnesses, but jcode treats performance as a measured product
feature. Its published numbers (Linux, 10 PTY launches):

| Metric | jcode | Claude Code | jcode advantage |
|---|---:|---:|---:|
| Baseline RAM (1 session, embeddings off) | **27.8 MB** | 386.6 MB | 13.9× |
| RAM per added session | **~9.9 MB** | ~212.7 MB | 21.5× |
| 10 active sessions | **117 MB** | 2300 MB | 19.7× |
| Time to first frame | **14.0 ms** | 3436.9 ms | 245× |
| Time to first input | **48.7 ms** | 3512.8 ms | 72× |

The advantage comes from a small number of architectural decisions, not micro-optimization. The five
highest-leverage, lowest-risk borrows for BioRouter are:

1. **Add a tuned allocator (jemalloc) to `biorouterd`/CLI** — BioRouter ships *no* custom allocator;
   jcode's decay-tuned jemalloc reclaimed ~1.4 GB RSS in their own testing. *(S effort, High impact.)*
2. **Add Cargo profiles + feature-gate heavy deps** — BioRouter has **no `[profile.*]` section at all**
   and compiles **988 crates unconditionally** (23 AWS, 15 tree-sitter, 7 boa-JS-engine, all PDF/DOCX).
   *(S–M effort, High impact on compile time + binary size.)*
3. **Stop blocking first frame on full backend + MCP boot** — both CLI and GUI render only *after* every
   enabled MCP extension subprocess has spawned and handshaked. *(M effort, High impact on perceived
   latency.)*
4. **Share MCP servers + HTTP clients across sessions** (and one `biorouterd` across windows) — today
   BioRouter spawns MCP child processes *per agent* and HTTP clients *per provider instance*, and the GUI
   spawns a *whole daemon per window*. This is the dominant RAM multiplier. *(L effort, Very High impact.)*
5. **Move auto-compaction off the critical path + lock the tool list** — compaction is a synchronous LLM
   round-trip inside the user's turn, and the tool list is rebuilt every turn (silently busting the
   provider prompt cache). *(M effort, High impact on latency + token cost.)*

BioRouter is **not** behind everywhere — it already offloads conversation to SQLite (lighter than jcode at
steady state), dispatches tools concurrently, gates token-counting, uses a clean `FramedRead<LinesCodec>`
SSE decoder, and recently landed gzip compression, streaming-render coalescing, and non-blocking job
persistence. The gaps are concentrated in **resource sharing, startup ordering, build configuration, and
interrupt/cache discipline**.

---

## The big structural findings (cross-cutting)

These themes each surfaced in *multiple* independent sub-agent reports, so they're the
highest-confidence conclusions.

### A. Resource multiplication — the #1 RAM story

jcode runs **one process that owns all sessions** and shares every expensive resource by `Arc`:

- One MCP pool: `Arc<OnceCell<Arc<SharedMcpPool>>>` — *"Instead of each session spawning its own set of
  MCP servers (N sessions × M servers = N×M processes), sessions share a single pool (M processes
  total)."* (`jcode-base/src/mcp/pool.rs:1-10`)
- One provider template cheaply `fork()`ed per session (`jcode-app-core/src/server.rs:393`,
  `client_lifecycle.rs:418`).
- One lazy, idle-unloaded embedder (`jcode-base/src/embedding.rs:23,228-292`).
- Adding a session allocates only the `Agent` shell + its transcript ⇒ ~10 MB/session.

BioRouter multiplies resources on **three axes**:

1. **Daemon per Electron window.** `startBiorouterd` picks a fresh port and `spawn()`s a new `biorouterd`
   for *every* window (`ui/desktop/src/biorouterd.ts:115,172`; `ui/desktop/src/main.ts:612,683,919-920`).
   N windows = N tokio runtimes + N `AgentManager`s + N SQLite pools + N×(MCP trees). The server is
   *already* session-keyed and singleton (`crates/biorouter/src/execution/manager.rs:19-24`) — only the
   Electron spawn path assumes per-window.
2. **MCP child processes per agent.** Each `Agent` builds its own `ExtensionManager`
   (`crates/biorouter/src/agents/agent.rs:236`) and `add_extension` → `TokioChildProcess…spawn()` per
   agent (`crates/biorouter/src/agents/extension_manager.rs:236,250-252,548-559`). Up to 100 live agents ×
   M stdio/uvx MCP servers (each 40–150 MB). A `uvx` Python or Node `@playwright/mcp` server is a real OS
   process. **No `SharedMcpPool` exists** (grep returns nothing).
3. **HTTP client per provider instance.** Every provider builds its own `reqwest::Client`
   (`crates/biorouter/src/providers/api_client.rs:208-227`); a fresh provider is created on each session
   restore (`agent.rs:1978`) and each sub-agent spawn (`subagent_tool.rs:414`). No shared/static client
   exists (~10 ms TLS+pool init each, jcode's own measurement at
   `jcode-provider-core/src/lib.rs:455-458`).

> Net: a worst case of `N_windows × up-to-100_agents × M_servers` MCP processes, plus N cold HTTP pools.
> jcode's equivalent is `M` MCP processes and `1` HTTP pool for the whole machine.

### B. Don't block the first frame / first turn

jcode's documented doctrine: *"we deliberately do NOT block the first turn on MCP connection, so the user
can talk to the agent immediately"* (`jcode-app-core/src/agent.rs:217-223`); *"time to window visible:
should not wait for WGPU"* (`docs/DESKTOP_STABLE_HOST_RELOAD_STARTUP.md:224`). It renders the first frame
and accepts input *while* the server connection and MCP registration happen in the background, and the MCP
pool is lazy (`OnceCell::get_or_init` on first tool use).

BioRouter serializes the opposite way:

- **CLI:** `build_session()` drains the *entire* extension `JoinSet` before the TUI is entered — first
  frame waits on the **slowest** MCP handshake (`crates/biorouter-cli/src/session/builder.rs:578,285`;
  `extension_manager.rs:250-268`). `uvx` Python extensions on a cold cache are seconds.
- **GUI:** `mainWindow.loadURL()` runs only *after* `checkServerStatus` polls `/status` to ready
  (`ui/desktop/src/main.ts:665→837`) — a blank framed window the whole time.
- **Server `/status`** is gated by `AppState::new()` → `Scheduler::new()` + `load_jobs_from_storage()` +
  `soul::install()`, all `await`ed before `TcpListener::bind` (`crates/biorouter-server/src/commands/agent.rs:44,68`;
  `crates/biorouter/src/scheduler.rs:190-207`).

BioRouter already does some of this right: SQLite is `connect_lazy_with` + `OnceCell`
(`session_manager.rs:666`), keyring is cached after one read (`config/base.rs:86-88`), OpenAPI is not
generated at boot, and MCP servers are not started at *server* boot (only per-session).

### C. jemalloc — the cheapest big win

jcode wires `tikv-jemallocator` as `#[global_allocator]` with
`dirty_decay_ms:1000,muzzy_decay_ms:1000,narenas:4`, with the comment: *"The defaults … caused 1.4 GB RSS
in previous testing."* (`src/main.rs:1-26`). On non-jemalloc Linux it caps glibc arenas via
`mallopt(M_ARENA_MAX,4)`. In jcode the allocator is opt-in behind a `jemalloc` feature
(`Cargo.toml:117-119,261-268`) — *"reduces fragmentation for long-running server."*

BioRouter uses the **system allocator everywhere** (grep for jemalloc/mimalloc/global_allocator returns
nothing). Combined with its per-turn full-transcript reload + double-clone
(`reply.rs:274,298`; `agent.rs:369,381`), freed pages are retained by glibc per-thread arenas and never
returned — RSS creep on a long-running `biorouterd`.

### D. Soft interrupt — queue-and-inject instead of cancel-and-restart

jcode never cancels a turn to inject a user message. It queues into
`Arc<std::sync::Mutex<Vec<SoftInterruptMessage>>>` (pushable *without* the async agent lock,
`jcode-agent-runtime/src/lib.rs:19-20`) and drains at API-safe points — after a no-tool turn (Point B),
urgent-between-tools after writing stub tool-results (Point C), or after all tools (Point D, the default).
Documented benefit: *"No cancellation, no lost work. No delay. Efficient API calls."*
(`docs/SOFT_INTERRUPT.md:28-62`). The same queue carries swarm messages and ambient notifications.

BioRouter has **no soft interrupt** (grep finds nothing). The only way to redirect a running turn is to
fire the `CancellationToken` — discarding the in-flight provider response + running tools, then paying a
full context re-send next call. This is exactly the "Hard Interrupt" jcode replaced.

### E. Prompt-cache stability — tool-list locking + cache tracker

jcode locks the tool snapshot after the first turn so asynchronously-registering MCP tools can't keep
invalidating the provider prompt cache; exactly one rebuild is allowed when MCP tools first appear, gated
by a one-shot `mcp_late_register_resolved` flag (`jcode-app-core/src/agent/turn_execution.rs:294-352`). A
cheap `CacheTracker` hashes the message prefix each request and logs append-only violations
(`jcode-base/src/cache_tracker.rs`); the ephemeral memory suffix is excluded so it doesn't false-positive.

BioRouter applies Anthropic `cache_control: ephemeral` correctly
(`crates/biorouter/src/providers/formats/anthropic.rs:151-209`) but **re-derives the tool list every
turn** and rebuilds it mid-turn whenever an extension is enabled (`agent.rs:1712-1715`), with **no cache
tracker and no tool-list locking**. Async MCP registration silently busts the entire tool-prefix cache —
a recurring token/$ cost on cache-capable providers.

### F. Background work instead of blocking the turn

jcode pushes everything off the user-visible path: compaction is `tokio::spawn`'d and swapped in a later
turn (only the rare ≥95% hard-drop is synchronous, and it does no LLM call —
`jcode-base/src/compaction.rs:860-925,932-1027`); memory recall is computed in a background sidecar and
injected next turn (`jcode-app-core/src/agent/prompting.rs:31-47`); telemetry is fire-and-forget.

BioRouter compacts **synchronously inside `reply()`** — the user waits for a summarization LLM round-trip,
and `do_compact` can retry up to 5× with progressive tool-response removal, all blocking
(`crates/biorouter/src/context_mgmt/mod.rs:222-330`; `agent.rs:1129-1207,1679-1690`). It *has* started
down this road: commit `8d78ddd` made `persist_jobs` writes non-blocking, and `230544b` coalesced
streaming re-renders — the right instinct, more surface to cover.

---

## Theme-by-theme comparison

### 1. Startup / cold boot
- **jcode:** thin client renders before the server is ready; lazy MCP pool; background log-prune/update-
  check/perf-detection; jemalloc+rustls tuned at `main.rs`; `startup_profile.rs` instruments every
  milestone as a %-of-total bar so regressions are visible.
- **BioRouter:** CLI draws after `build_session().await`; GUI `loadURL` after `/status`; scheduler init
  blocks listener bind; `update_project_tracker` runs inline (`cli.rs:1874`).
- **Borrow:** render-before-ready (CLI + GUI), lazy/background MCP load, move scheduler off the bind path,
  a `startup_profile` instrument to measure before/after.

### 2. RAM footprint
- **jcode:** jemalloc decay tuning; shared `Arc` resources; dual transcript repr with a non-persisted,
  lazily-rebuilt provider-message cache (`jcode-base/src/session.rs:99,174-184`); hard-capped caches with
  a `MEMORY_BUDGET.md` CI ratchet; lazy+idle-unload embedder; feature-compiled-out "off" path.
- **BioRouter:** *better* at steady state (transcripts in SQLite, not RAM; `Agent` holds no conversation),
  but no allocator, per-agent MCP processes, per-turn transcript reload+double-clone, and the Electron
  multi-window renderer (150–400 MB each) dominates total.
- **Borrow:** jemalloc (P1); share MCP processes (theme A); avoid the per-turn double-clone
  (`agent.rs:369,381`); a `MEMORY_BUDGET.md`-style cap ratchet; Electron: reuse one window, set
  `backgroundThrottling`, code-split force-graph/visualiser, default Auto Visualiser to CDN assets.

### 3. Agent loop / tool dispatch
- **jcode:** `InterruptSignal` (AtomicBool+Notify) wired into every `select!`; per-tool `tokio::spawn` with
  "move to background" detach (Alt+B adopts the `JoinHandle`); soft interrupt; 30 s keepalive `Pong`;
  incremental scan helpers (avoids O(n²) over a streamed answer); batched turn-end `session.save()`.
- **BioRouter:** *already concurrent* (lazy boxed tool futures + `stream::select_all`, `agent.rs:1473`);
  cancellation propagates into MCP calls (`mcp_client.rs:373`); but coarse cancellation (only between
  events/items), no soft interrupt, no tool backgrounding, no stream keepalive, per-message SQLite writes
  at turn end (`agent.rs:1781-1783`).
- **Borrow:** soft interrupt (P, theme D); `select!` cancellation + keepalive on the provider stream;
  tool backgrounding; batch turn-end persistence.

### 4. Rendering (CLI TUI + React GUI)
- **jcode:** render rate decoupled from event rate via `needs_redraw |=` + `select!`; frame coalescing
  (drain ≤32 events → 1 frame); single-cell spinner damage path; five-layer content cache (syntax
  highlight, per-message, body, full-frame) with width-keyed wrapped lines sliced on scroll (no re-wrap);
  `IncrementalMarkdownRenderer` for the streaming tail; adaptive perf tiers (SSH→Minimal 12 fps);
  DECSET 2026 synchronized output (no flicker); time-paced `StreamBuffer` token reveal.
  *(The README's ">1000 fps" / "1800× Mermaid" are marketing — the enforced ceiling is 120 fps; the real
  win is the cheap idle/diff paths.)*
- **BioRouter CLI:** immediate-mode, no dirty flag; full `app.scrollback.clone()` + full re-wrap **every
  frame** (`tui/mod.rs:765-766`); whole-message markdown re-parse **per token** (`app.rs:172-176`); no fps
  cap, no SSH awareness, no synchronized output.
- **BioRouter GUI:** rAF stream coalescing already landed (`useChatStream.ts:329-347`) but it caps
  *frequency*, not *scope* — per-message components are **not** `React.memo`'d, props are identity-
  unstable, and there are O(n²) `findIndex`/`identifyConsecutiveToolCalls` scans per render; **no
  virtualization** (every message stays mounted); markdown double-renders; full synchronous Prism;
  multi-MB un-memoized Auto-Vis iframes.
- **Borrow (CLI):** cache wrapped scrollback + slice on scroll; `needs_redraw` gate; throttle streaming
  markdown; SSH fps cap; synchronized output. **(GUI):** memoize messages + stabilize props (prerequisite
  that makes the existing rAF coalescing pay off), then virtualize the list; fix the markdown double-render;
  `PrismAsyncLight`; memoize the MCP-UI renderer + CDN-default Auto-Vis.

### 5. Context management / compaction
- **jcode:** background compaction at 80%, synchronous no-LLM hard-drop only at 95%; approximate
  `chars/4` token estimate with an incremental active-char sum (never rescans history); flat
  `IMAGE_TOKEN_COST`; trusts provider-observed tokens; tool-list locking + cache tracker.
- **BioRouter:** synchronous compaction in `reply()`; real tiktoken `o200k_base` BPE run on the async
  runtime with **no `spawn_blocking`** over the whole history when `session.total_tokens` is `None`
  (`context_mgmt/mod.rs:184-199`) — gated, so happy-path is cheap, but the first turn / usage-less
  providers pay a synchronous full-history encode; no cache tracker; single 0.8 threshold.
- **Borrow:** background compaction + 0.95 hard-drop floor; `spawn_blocking` (or char estimate) for the
  cold-path scan; tool-list locking (theme E); a cheap `CacheTracker` regression guard.

### 6. Memory / self-adapting recall
- **jcode:** *passive* recall injected into the system prompt every turn with **zero tool calls**,
  computed in a background sidecar (turn N → available N+1, `prompting.rs:31-47`); hybrid dense+BM25+RRF
  retrieval; a cadence-gated listwise-LLM reranker (measured recall@5 0.0 → 0.53 → **0.75**,
  `docs/MEMORY_GRAPH_PLAN.md:204-219`); graph as reranker/rescue not first-stage; category-specific
  confidence decay (Corrections 365 d … Inferred 7 d); ambient consolidation gated on rate-limit headroom.
- **BioRouter:** Knowledge feature is **tool-driven** (`kb_search` must be called, costs a turn + tool
  result) over BM25; Chat Recall is SQL `LIKE '%word%'` (`chat_history_search.rs:117-138`); the `query`
  macro runs a ≤30-step / 200k-token LLM sub-agent; no passive injection, no embeddings in the recall
  path, no decay. The daily 3 AM "Meditation" cron is a real background *writer* but is a full agentic
  workflow with no resource-awareness, and its output is still only readable via tool calls.
- **Borrow:** passive KB recall injected into the (non-cached) system prompt, precomputed in a background
  task (BM25-only first — low risk); cadence-gated listwise rerank (the measured precision lever);
  confidence/staleness frontmatter + decay; graph-as-reranker over the existing `[[wiki-links]]`; make
  Meditation resource-adaptive. Optional embeddings *last* (jcode found the pool, not recall, was the
  bottleneck once a reranker exists).

### 7. Crate / build architecture
- **jcode:** `release` tuned for *compile speed* (`opt-level=1, codegen-units=256, incremental=true`) +
  separate `release-lto` (`lto="thin", codegen-units=16`) for distribution; per-package `opt-level=3`
  overrides on hot low-churn leaf deps (anim math, `cosmic-text`/`rustybuzz` text shaping, `image`/`png`)
  so debug builds feel like release at a one-time compile cost; heavy deps (ONNX, PDF, Azure, AWS, email)
  isolated in **feature-gated leaf crates** with a `JCODE_DEV_FEATURE_PROFILE=minimal` escape hatch (cut
  the dep graph ~3.4×); ~30 tiny `*-types` contract crates + a boundary-guard script. Hard-won lesson:
  *"the lever is crate content/weight, not crate count"* — rmeta pipelining made splitting *intermediate*
  crates a no-win (`docs/COMPILE_PERFORMANCE_PLAN.md:1000-1004`).
- **BioRouter:** **no `[profile.*]` section at all** (everyday release = the slowest config: opt-3 /
  cu-16 / no-incremental), **no `.cargo/config.toml`**, **no allocator**, the `release-lto` profile
  referenced in `CLAUDE.md` *doesn't exist*; **988 crates compiled unconditionally** — 23 `aws-*`, 15
  `tree-sitter*`, 7 `boa-*` (a full JS engine), all PDF/DOCX/XLSX, `git2` vendored; only two trivial
  features in the whole workspace; duplicate dep versions (reqwest ×2, image ×2, base64 ×2, zip ×4).
- **Borrow:** add the `release` (fast) + `release-dist` (LTO+strip) profile pair; feature-gate the
  tree-sitter / AWS / doc-conversion / boa stacks (default-on) with a minimal escape hatch; jemalloc
  feature; `debug="line-tables-only"` for dev; unify duplicate deps; extract `message.rs` (1528 lines of
  DTOs) into a `*-types` crate *only* where it shrinks the biggest unit or decouples ≥2 crates.

### 8. Process / server model
- **jcode:** one detached `setsid` daemon, many thin reconnecting clients; `Arc<RwLock<HashMap>>` session
  map with `try_lock` to skip contended agents; shared MCP pool / provider / embedder; 5-min idle-timeout
  self-exit; hot-reload via `exec()` on the same socket; reconnect backoff 1 s→30 s. *jcode's own plan
  says NOT to split into OS processes — keep it one process, modular inside* (`SERVER_SERVICE_SPLIT_PLAN.md:596-598`).
- **BioRouter:** daemon-per-window; sound concurrency primitives (short RwLock guards, LRU-100 agent cache,
  4-conn SQLite pool); minor blocking-in-async spots (`config_management.rs:672` `std::fs`,
  `tunnel/mod.rs:25-45`). The problem is *topology*, not locks.
- **Borrow:** share one `biorouterd` across windows (ref-counted, idle-timeout exit, detached spawn +
  client reconnect/backoff); shared MCP pool within a daemon (theme A) — the highest-leverage change that
  needs *no* Electron re-arch; provider `fork()`; fix the blocking-in-async spots.

### 9. Swarm / ambient autonomy
- **jcode:** `AdaptiveScheduler` learns rate-limit headroom from `x-ratelimit-*` headers, reserves a 20%
  user buffer, projects user consumption, spreads cycles, backs off ×2 (cap 64) on 429 and resets on
  success, and **pauses entirely when a user session is active** (`jcode-app-core/src/ambient/scheduler.rs:155-276`,
  `runner.rs:585-602`); lock-free copy-on-read status snapshots; optimistic file-touch conflict detection;
  cheap in-process headless spawn with depth cap (≤5); a single `communicate` tool with a 3-tier read.
- **BioRouter:** subagents are in-process (cheap) but have **no concurrency cap, no depth cap** (fork-bomb
  risk), turn-cap only; the cron scheduler has an overlap guard + `max_runs` but **no rate-limit
  awareness, no pause-when-active, no backoff**; ACP is single-agent interop, not coordination; no
  agent-to-agent messaging.
- **Borrow:** pause-scheduler-when-active (S, high value); rate-limit-aware backoff (start with 429
  backoff); subagent concurrency + depth caps; (later, if a use case appears) soft-interrupt messaging and
  file-touch coordination.

### 10. Provider / network / streaming
- **jcode:** one process-wide `reqwest::Client` (`OnceLock`, cheap clone) with `connect_timeout(15s)`,
  `tcp_keepalive`, `http2_keep_alive_*`, `pool_idle_timeout(90s)` — *"Creating a reqwest::Client is
  expensive (~10ms)"* (`jcode-provider-core/src/lib.rs:455-496`); a fresh **unpooled** client only on
  transport-retry (avoids reusing a poisoned connection); 180 s per-chunk SSE idle timeout; backoff +
  jitter; mid-stream rollback; sliding two-marker prompt-cache breakpoints.
- **BioRouter:** *per-provider-instance* clients with **only** `.timeout(600s)` — no `connect_timeout`,
  no keepalive, no pool tuning (`api_client.rs:208-227`); the 600 s whole-request timeout also caps
  streams (kills healthy long generations; never catches a stalled stream early); retry can reuse a
  poisoned pooled connection; client rebuilt 2–4× during construction. *Good:* clean `FramedRead<LinesCodec>`
  SSE decode (cleaner than jcode), reqwest features (gzip/h2/zstd) already enabled, honors provider
  `retry_delay`, deeper 429 budget.
- **Borrow:** shared client with pool + h2 keepalive (gated to non-mTLS); `connect_timeout`; per-chunk SSE
  idle timeout + stop using whole-request timeout for streams; fresh client on retry; build client once;
  sliding cache breakpoints.

---

## What BioRouter already does well (don't regress these)

- **Conversation lives in SQLite, not RAM** — the `Agent` struct holds no transcript; steady-state
  per-session daemon RAM is arguably *tighter* than jcode's (`agent.rs:138-161`; `manager.rs`).
- **Bounded live-agent LRU (100)** + **4-connection SQLite pool** + lazy connect (`manager.rs:15`;
  `session_manager.rs:664-666`).
- **Concurrent tool dispatch** via lazy boxed futures + `select_all`, with cancellation propagating into
  MCP calls (`agent.rs:1473`; `mcp_client.rs:373`).
- **Token counting gated** by `session.total_tokens` (no per-turn full-history tiktoken on the happy path).
- **Clean SSE decode** (`FramedRead<LinesCodec>`) and reqwest compression/h2 features enabled.
- **Recent perf work** in the right spirit: gzip responses, streaming-render coalescing, non-blocking
  `persist_jobs`, non-blocking computer-controller I/O.

---

## Prioritized roadmap

Effort: S ≤ ~1 day · M ≈ days · L ≈ weeks. Impact/Risk relative to BioRouter.

| # | Proposal | Theme | Effort | Impact | Risk |
|---|---|---|---|---|---|
| 1 | jemalloc (decay-tuned) in `biorouterd` + CLI, behind a default-on feature | RAM | **S** | **High** | Low |
| 2 | Cargo profiles: fast `release` + `release-dist` (LTO+strip); fix `CLAUDE.md` drift | Build | **S** | **High** | Low |
| 3 | Feature-gate tree-sitter / AWS / doc-conversion / boa (default-on) + minimal escape hatch | Build | **M** | **High** | Med |
| 4 | Startup profiler instrument (port `startup_profile.rs`); measure before optimizing | Startup | **S** | Med (enables rest) | Low |
| 5 | GUI: render renderer before `/status`; show "connecting" state | Startup | **M** | **High** | Med |
| 6 | Move scheduler init + `soul::install` off the listener-bind path | Startup | **M** | Med-High | Med |
| 7 | Lazy / non-blocking MCP extension load (don't drain JoinSet before first frame) | Startup | **L** | **High** | Med-High |
| 8 | Soft interrupt (queue + inject at safe points) — replaces cancel-and-resend | Agent loop | **M** | **High** | Med |
| 9 | Tool-list locking + cheap `CacheTracker` (prompt-cache stability) | Context | **M** | **High** ($/latency) | Med |
| 10 | Background compaction + synchronous no-LLM hard-drop at 0.95 | Context | **L** | **High** | Med |
| 11 | `spawn_blocking` (or char estimate) for cold-path token counting | Context | **S** | Med | Low |
| 12 | Shared MCP pool across agents within a daemon | Process/RAM | **L** | **Very High** | Med-High |
| 13 | Shared HTTP client (pool + h2 keepalive + connect_timeout); fresh-on-retry | Network | **M** | **High** | Med |
| 14 | Per-chunk SSE idle timeout; stop whole-request timeout on streams | Network | **S-M** | Med-High | Low-Med |
| 15 | CLI TUI: cache wrapped scrollback + slice on scroll; `needs_redraw` gate; sync output | Render | **M** | **High** (CLI) | Med |
| 16 | GUI: memoize messages + stabilize props; remove O(n²) scans | Render | **M** | **High** (GUI) | Med |
| 17 | GUI: virtualize the message list | Render | **L** | **High** (long sessions) | Med-High |
| 18 | One shared `biorouterd` across windows (ref-counted + idle exit + reconnect) | Process | **L** | **Very High** | Med-High |
| 19 | Passive KB recall injected into system prompt, precomputed in background (BM25) | Memory | **M** | **High** | Low-Med |
| 20 | Cadence-gated listwise-LLM rerank for recall (the measured 0.53→0.75 lever) | Memory | **M** | **High** | Med |
| 21 | Scheduler: pause-when-user-active + 429 backoff | Ambient | **S** | **High** | Low |
| 22 | Subagent concurrency + spawn-depth caps | Ambient | **S** | Med | Low |
| 23 | Electron: `backgroundThrottling`, code-split heavy libs, CDN-default Auto-Vis | RAM (GUI) | **S-M** | Med | Low |
| 24 | `MEMORY_BUDGET.md`-style cache-cap ratchet | RAM | **M** | Med | Low |

**Suggested first wave (low risk, high return):** #1, #2, #4, #11, #14, #21, #22, #23 — all S/quick-M,
mostly self-contained, no architectural change.
**Second wave (the real wins):** #8, #9, #13, #15, #16, #5 + #6.
**Strategic (re-architecture):** #12 (shared MCP pool — biggest within-daemon RAM win, no Electron change)
→ #18 (shared daemon) → #10, #17, #19/#20.

---

## Architectural mismatches — what NOT to blindly borrow

- **jcode's whole-machine single daemon** is its biggest RAM win, but BioRouter's Electron-per-window
  model means adopting it (#18) is a real re-architecture with crash-blast-radius and per-window
  working-dir implications — sequence #12 first (shared MCP pool needs no Electron change and captures
  much of the win).
- **jcode's winit/wgpu desktop host** (stable-host + reloadable-worker, display-list protocol) is
  specific to its native renderer; only the *staged startup* principle transfers to Electron.
- **The ">1000 fps" and "1800× Mermaid" README claims** are unsubstantiated marketing (the enforced TUI
  ceiling is 120 fps; Mermaid is an external feature-gated crate). Borrow the *cheap idle/diff render
  paths*, not the headline number.
- **A local cross-encoder reranker** was *measured and rejected* by jcode (0.325 recall, out-of-
  distribution) — don't add one; use the listwise-LLM rerank instead.
- **Don't chase crate count.** jcode's own finding: rmeta pipelining makes splitting *intermediate* crates
  a no-win. The lever is profiles + feature-gating + the *weight* of the largest compilation unit.

---

## Appendix: key evidence anchors

**jcode (`/tmp/jcode-review` at analysis time):**
`src/main.rs:1-26` (jemalloc) · `Cargo.toml:317-456` (profiles + per-package opt-level) ·
`jcode-base/src/mcp/pool.rs:1-45` (shared MCP pool) · `jcode-app-core/src/server.rs:393,436,1421-1462`
(shared resources + idle exit) · `jcode-app-core/src/agent/{prompting.rs:31-47,turn_execution.rs:294-352,
turn_streaming_mpsc.rs:1245-1451}` (passive memory, tool-lock, tool backgrounding) ·
`jcode-base/src/compaction.rs:860-1027` (background compaction) · `jcode-agent-runtime/src/lib.rs:19-20` +
`docs/SOFT_INTERRUPT.md` (soft interrupt) · `jcode-provider-core/src/lib.rs:455-519` (shared HTTP client) ·
`jcode-app-core/src/ambient/scheduler.rs:155-276` (adaptive scheduler) ·
`docs/{COMPILE_PERFORMANCE_PLAN,MEMORY_BUDGET,MEMORY_GRAPH_PLAN,SERVER_ARCHITECTURE,AMBIENT_MODE}.md`.

**BioRouter:**
`crates/biorouter/src/agents/agent.rs:138-161,236,369,381,1129-1207,1473,1781-1783` ·
`crates/biorouter/src/agents/extension_manager.rs:236,250-252,548-559` ·
`crates/biorouter/src/execution/manager.rs:15,19-24,82-114` ·
`crates/biorouter/src/session/session_manager.rs:664-666,1276-1306` ·
`crates/biorouter/src/context_mgmt/mod.rs:184-330` · `crates/biorouter/src/token_counter.rs:115-157` ·
`crates/biorouter/src/providers/api_client.rs:205-241` · `crates/biorouter/src/scheduler.rs:133-213` ·
`crates/biorouter-cli/src/session/{builder.rs:578,285, tui/mod.rs:391-394,765-766, tui/app.rs:161-179}` ·
`crates/biorouter-server/src/commands/agent.rs:44,68` ·
`ui/desktop/src/main.ts:567,612,665,837,919-920` · `ui/desktop/src/biorouterd.ts:115,172` ·
`ui/desktop/src/hooks/useChatStream.ts:329-347` · `Cargo.toml` (no `[profile.*]`).
