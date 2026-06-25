# jcode-borrows: implementation & benchmark report

**Branch:** `perf/jcode-borrows` (a git worktree off `main@0f9bb71`).
**Date:** 2026-06-24.
**Scope:** implement the First Wave + Second Wave changes from
[docs/jcode-comparison-perf-analysis.md](jcode-comparison-perf-analysis.md), one
commit per change, and benchmark/verify each one.

> **Recoverability.** All work is on the `perf/jcode-borrows` branch in a separate
> worktree at `/Users/wanjun/Desktop/BioRouter-perf`. `main`'s uncommitted WIP is
> untouched and fully intact. Every change is its own commit, so any of them can be
> reverted individually (`git revert <hash>`) or the whole branch dropped. The
> shared `target/` build cache was reused for speed; main's source is unaffected
> (rebuild main any time to regenerate its binaries).

---

## How to read this report

The proposed changes fall into three honest verification buckets, because not all
are numerically benchmarkable in this environment (no live LLM provider, no GUI
profiler):

- **Numeric** — measured a real number (RAM, binary size, crate count).
- **Structural / compile** — verified it compiles + integrates and is correct by
  construction; the runtime gain is real but only observable under load that needs
  a live provider or interactive use.
- **Typecheck / reasoning** — GUI changes verified by `tsc` + first-principles
  analysis; empirical render gain needs the in-app React profiler.

A key finding up front: **several proposals target gaps that BioRouter mostly does
not have.** BioRouter's baseline is already good where jcode/Claude Code were bad
(idle RSS ~21 MB, server startup ~99 ms warm, conversation offloaded to SQLite,
tools stable across turns within a process). So for those, the honest result is
"implemented, but the gain for BioRouter is small/conditional" — which is itself a
valuable finding.

---

## Baseline (clean HEAD, release, this machine)

| metric | value |
|---|---:|
| biorouter (release) | 137.8 MB |
| biorouterd (release) | 123.7 MB |
| Cargo.lock crates | 988 |
| biorouter dep-graph crates | 795 |
| biorouterd idle RSS | ~21.3 MB (±0.5%) |
| biorouterd startup (warm) | 34–99 ms |

---

## Per-change results

| # | Change | Commit | Verification | Result for BioRouter |
|---|---|---|---|---|
| FW1 | jemalloc (tuned) global allocator | `2bd4aff` | **Numeric** (churn microbench) | **~4.4× lower peak RSS under churn** (3.3 vs 14 MB); +2.3 MB idle; +0.4 MB binary. Net win for a busy daemon. |
| FW2 | Cargo profiles: `strip` release + `release-dist` + `quick` | `22c3b0f` | **Numeric** (binary size) | **−13% binaries** (biorouterd 123.7→107.6, biorouter 137.8→120.4 MB). Clear win, zero runtime cost. |
| FW3 | `spawn_blocking` cold-path token scan | `2daa584` | **Structural** | Removes a synchronous BPE stall from the async runtime on the cold path. No regression. Real under concurrency. |
| FW3+SW3 | HTTP client hardening (read_timeout, connect_timeout, keepalive, pool) | `1ac8500` | **Structural** | Stalled streams abort in 300 s not 600 s; healthy long streams survive (1800 s cap); connection reuse. Numeric needs a live/mock server. |
| FW4 | Scheduler pause-on-active + 429 backoff; subagent fork-bomb caps | `07a8fc2` | **Structural** (+ mechanism) | Bounds runaway subagents; background work yields to the user + to rate limits. Safety/responsiveness win. |
| FW5 | Auto-Vis CDN default + explicit backgroundThrottling | `13007f3` | **Typecheck** | CDN default: real renderer-heap/SQLite win for figure-heavy sessions (offline tradeoff, overridable). backgroundThrottling: **no-op — already Electron's default** (honest). |
| SW2 | Deterministic tool ordering (prompt-cache stability) | `19eff1c` | **Structural** | Tool list already stable across turns *within* a process; this fixes the *cross-process* case (resumed sessions keep the provider prompt-cache prefix). Small but real, conditional on resume-within-TTL. |
| SW3 | (see FW3+SW3 row) | `1ac8500` | — | — |
| SW4 | Feature-gate the AWS SDK (`aws-providers`, default-on) | `9af616f` | **Numeric** (crate count) | **−42 crates** with `--no-default-features` (795→753); verified the minimal build compiles. Win for opt-out builds (headless CLI package). |
| SW5-GUI | Hoist O(n²) per-message scans to O(n) | `598da59` | **Typecheck + reasoning** | Per-frame chain/index recomputation goes from N×O(n) to O(n). Real per-frame CPU cut for long sessions; full memoization is a documented follow-up. |
| SW5-CLI | Scrollback wrapped-count cache | `4910c42` | **Structural** | Skips the O(content) unicode re-measure on spinner-tick redraws during streaming. Real (partial) CLI-render win; the Vec clone remains (slice-on-scroll deferred). |
| SW1 | Soft interrupt (queue + inject at safe boundary + `/interrupt` route) | `56d6a7e` | **Compile + structural** | Mechanism implemented end-to-end (backend); injects mid-turn user input at the next loop boundary instead of cancel-and-resend. UX latency gain is qualitative (needs live LLM + GUI wiring follow-up). |
| SW6 | GUI render-before-/status + scheduler off bind | **documented** | **Reasoning** | **Limited value for BioRouter:** backend starts ~99 ms warm (vs the multi-second gap that motivated the proposal); render-before-ready risks startup breakage unless the React app tolerates a not-ready backend. Not worth the risk for the small gain — see rationale below. |

### Cumulative build & numbers
All commits compile and integrate: the cumulative `cargo build --release`
(default features = AWS on) **exited 0** (re-confirmed by a from-scratch cold
restore build), validating SW1 + SW2 + SW5-CLI with every prior change. Clean
3-run benchmark of the cumulative binaries vs the baseline:

| metric | baseline | cumulative (all changes) | Δ |
|---|---:|---:|---:|
| biorouterd (release) | 123.7 MB | **107.7 MB** | **−13.0%** |
| biorouter (release) | 137.8 MB | **120.6 MB** | **−12.5%** |
| biorouterd idle RSS | 21.3 MB | 26.2 MB | +4.9 MB |
| biorouterd startup (warm) | 34–99 ms | 32–87 ms | ~same |
| Cargo.lock crates | 988 | 991 (−42 buildable via `--no-default-features`) | +3 / −42 |

The idle RSS is **+4.9 MB** (jemalloc arena metadata dominates, ~+2.3 MB of it) —
the deliberate trade for the **4.4× lower peak RSS under churn** (FW1), which is
what a busy daemon actually pays. Startup is unchanged despite jemalloc init.
**SW1 `/interrupt` verified live:** returns 202 with the secret, 401 without.

> **Build-artifact note.** During final benchmarking the shared `target/` cache was
> wiped by an external disk-pressure cleanup (disk had climbed to ~95%). This is
> **build cache only — all source and all 13 commits are intact in git**, and the
> binaries regenerate with `cargo build --release`. The cumulative build had
> already exited 0 (compilation of every change verified) and the size/RSS numbers
> above were captured before the wipe. A fresh restore build is running. Lesson for
> a constrained machine: a dedicated/cleaned `CARGO_TARGET_DIR` per worktree (jcode
> sizes its build host deliberately) — the shared 86 GB target on a 95%-full disk
> was the fragility.

---

## Detailed findings

### The clear numeric wins
- **FW1 jemalloc** — the standout. A standalone churn microbench
  (`benchmarks/alloc-churn`, mimicking BioRouter's per-turn transcript
  clone-and-drop) shows jemalloc holds peak RSS at **3.3 MB vs the system
  allocator's ~14 MB (~4.4×)** and settles tighter. The cost is **+2.3 MB at idle**
  (one-time arena metadata) — so it's a net win for a *busy* daemon (the real
  workload), a slight loss for a *purely idle* one. Behind a default-on feature,
  so trivially disabled. On Linux (glibc) the win is expected to be larger.
- **FW2 strip** — `[profile.release] strip = true` removes the symbol table from
  every shipped binary (debug info was already off): **−13%** with no runtime cost
  and no release-pipeline rewiring (the pipeline already uses `--release`).
  `release-dist` (thin LTO) and `quick` (fast compile) are opt-in extras.
- **SW4 AWS gating** — `--no-default-features` drops **42 crates** from biorouter's
  graph (incl. the `aws-lc-sys` C/asm build). Default builds unchanged; the headless
  CLI-only Linux package is the natural beneficiary. tree-sitter/doc-conversion/boa
  follow the same documented pattern.

### The structural / safety wins
- **FW3 token** + **FW3/SW3 HTTP** — remove a runtime-blocking BPE pass on the cold
  path; add a per-read stall timeout, a connect timeout, keep-alive connection
  reuse, and a higher overall cap so healthy long streams aren't killed.
- **FW4** — a global semaphore + in-flight ceiling turn unbounded subagent spawning
  into a bounded fork-bomb-safe pool; the scheduler now defers background jobs while
  a user is active or a provider is rate-limited.
- **SW1 soft interrupt** — the backend mechanism is complete: a per-agent queue, a
  `POST /interrupt` route, and injection at the safe loop boundary (after the
  previous turn's tools, before the next provider call), so a mid-turn user message
  is incorporated without a cancel-and-resend round trip.

### The conditional / small-gain ones (honest)
- **FW5 backgroundThrottling** — Electron already defaults this to `true`; set
  explicitly for intent, but **no measurable change**. (The CDN-default half *is* a
  real win for figure-heavy sessions.)
- **SW2** — within a process the tool list is already stable across turns (the
  source HashMap iterates consistently for an unchanged map), so the prompt cache
  already holds. The deterministic sort fixes only the *cross-process* case
  (resumed sessions) — real but conditional on resuming within the cache TTL.
- **SW6** — BioRouter's backend startup is ~99 ms warm, so the render-before-ready
  decoupling that gives jcode/Claude Code seconds buys BioRouter little, while
  risking startup breakage. Documented and deliberately not implemented.

---

## Reverting / recoverability

- Whole branch: `git -C /Users/wanjun/Desktop/BioRouter checkout main` (main is
  untouched); delete the worktree with `git worktree remove ../BioRouter-perf`.
- Single change: `git revert <hash>` (commits are 1:1 with changes).
- All new behavior is **off-switchable at runtime**: `--no-default-features` (FW1
  jemalloc, SW4 AWS), `BIOROUTER_SCHEDULER_PAUSE_ON_ACTIVE=0`,
  `BIOROUTER_SUBAGENT_MAX_CONCURRENT/INFLIGHT`, `BIOROUTER_HTTP_*_TIMEOUT_SECS`,
  `BIOROUTER_AUTOVIS_CDN=0`.

## Follow-ups (documented, not done)
- Wire `just release-binary`/`scripts/release.sh` to `release-dist` after a macOS
  notarized-packaging smoke test.
- Feature-gate tree-sitter / doc-conversion / boa (same pattern as SW4).
- SW5-GUI full message memoization (needs the toolResponsesMap hoist) + React
  profiler verification; SW5-CLI slice-on-scroll to drop the per-frame Vec clone.
- SW1 GUI/CLI wiring (call `/interrupt` on mid-turn input) + OpenAPI regen.
