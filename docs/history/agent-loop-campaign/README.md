# Agent-loop fix campaign

> **What this is.** The plan of record for the agent-loop fix campaign: branch and
> worktree conventions, the wave-to-cluster-to-proposal mapping, the regression-gate
> rule, and a dated log of every gate and merge decision.
> **Status:** Historical record — the campaign is finished and its work is merged to
> `main` (86 `BR-`prefixed commits landed; the modules it created —
> `crates/biorouter/src/agents/{budget,mistakes,stall,effort,mcp_pool}.rs`,
> `crates/biorouter/src/checkpoint/`, `crates/biorouter/src/security/policy/`, the
> `biorouter-sandbox` crate and `.github/workflows/rust.yml` — are all present in the
> tree). Read it as the record of *how the work was sequenced*, not as an active plan.
> **Audience:** maintainers.

`BR-NN` identifiers (`BR-1` … `BR-70`) are proposal numbers from the agentic-loop
review. `BR-1`…`BR-67` are defined in
[the improvement proposals register](../agent-loop-review/improvement-proposals.md);
`BR-68`, `BR-69` and `BR-70` were added mid-campaign by the cross-platform audit and
are defined in [the platform parity audit](../../agent-loop/cross-platform/platform-parity-audit.md).
`GAP-1`…`GAP-3` are the three cross-platform gaps that same audit found; they are also
defined there. Commit messages carry the `BR-NN:` prefix, so these identifiers are
cited across git history and cannot be renumbered.

The campaign implemented the 67 proposals raised by the agentic-loop review. The
strategy was clustered worktrees in dependency-ordered waves off a single integration
branch, with every wave regression-gated against a fixed baseline before merging. The
outcome — what actually landed, the final test counts, and the caveats — is recorded
separately in [the outcome report](outcome-report.md).

## Documents in this folder

| Document | What it holds |
|---|---|
| `README.md` (this file) | The plan of record and the dated campaign log. |
| [Outcome report](outcome-report.md) | The closing record: what landed, test progression across all four gates, highest-value fixes, caveats. |
| [Mid-flight review index](mid-flight-review-index.md) | The hand-off index written for the human reviewer at Gate 1, frozen mid-campaign. |
| [Commit log](commit-log.md) | One line per commit on the campaign branch, mapping each commit to the `BR-NN` proposal it implements. |
| [Wave reports](wave-reports/) | One verification report per cluster: per-proposal commits, files, exact test-result lines, regressions found and fixed. |

## Conventions

> **Note.** The branches and worktrees named below (`agent-loop-integration`,
> `.worktrees/integration`, `agent-loop-wave0`, `agent-loop-loopdet` and the other
> cluster branches) were deleted after the campaign landed. The cache paths are
> machine-local to the host the campaign ran on. They are kept here because the log
> entries and commit messages refer to them.

- **Integration branch:** `agent-loop-integration` (this branch, worktree
  `.worktrees/integration`). Clusters branch off it; nothing merges to `main`
  without an explicit human decision at the end.
- **Cluster branches/worktrees:** `agent-loop-<name>` at `.worktrees/<name>`.
- **Build isolation:** every worktree uses its own
  `CARGO_TARGET_DIR=~/.cache/br-targets/<name>` (shared target dirs lock and
  serialize parallel builds).
- **Commits:** one commit per proposal, message starts `BR-NN:`.
- **Docs:** each wave writes `docs/agent-loop-fixes/wave<N>.md` (what changed,
  test evidence, regressions found/resolved). Architectural items get a design
  doc in `docs/agent-loop-fixes/designs/BR-NN-design.md` before implementation.
  Those files now live in [`wave-reports/`](wave-reports/) and
  [`docs/agent-loop/designs/`](../../agent-loop/designs/) respectively.
- **Regression gate:** full `cargo test --workspace --no-fail-fast` compared
  against the baseline log (`~/.cache/br-baseline/workspace-test.log`, taken at
  integration commit a409e7d7). A wave merges only with **zero new failures**.
  Frontend `npm run test:run` gates any wave that touches `ui/desktop`.

## Waves

> **Note.** The Status column originally froze mid-campaign, with the Wave 2 rows
> reading *in progress* and every Wave 3 row reading *pending*. It has been
> reconciled against [the outcome report](outcome-report.md), which records Gate 2
> (Wave 2, 3 clusters) and Gate 3 (Wave 3, 4 clusters) both closing GREEN with zero
> new failures. All waves shipped.

| Wave | Cluster | Branch | Proposals | Status |
|---|---|---|---|---|
| 0 | Foundation + seams + designs | `agent-loop-wave0` | BR-46, 25, 36, 38, 20, 39, 34, 26, 4, 33 (+BR-16 subsumed); agent.rs seam refactor; design docs BR-43/54/21/17/45/65 | **merged 129589ba — gate GREEN** |
| 1 | Compaction & memory | `agent-loop-compaction` | BR-15, 10, 11, 13, 14, 12, 17 | **merged — gate GREEN** |
| 1 | Security & guardrails | `agent-loop-security` | BR-22, 23, 21, 65, 64 | **merged — gate GREEN** |
| 1 | Checkpoints & VCS | `agent-loop-checkpoints` | BR-43, 44, 45 | **merged — gate GREEN** |
| 1 | Long-running & processes | `agent-loop-processes` | BR-37, 40, 41, 42 | **merged — gate GREEN** |
| 1 | Context & prompts | `agent-loop-context` | BR-5, 2, 9, 8, 3, 60, 1 | **merged — gate GREEN** |
| 2 | Loop, stuck & budgets | `agent-loop-loopdet` | BR-29, 30, 31, 32, 35, 66, 67 | **merged — Gate 2 GREEN** |
| 2 | Hooks & permissions | `agent-loop-hooks` | BR-18, 19, 24, 27, 28, 63 | **merged — Gate 2 GREEN** |
| 2 | Server & cancel | `agent-loop-server` | BR-6, 7, 52, 61, 62 | **merged — Gate 2 GREEN** |
| 3 | Verification & done-ness | `agent-loop-verify` | BR-47, 48, 49, 50, 51 (needs BR-19) | **merged — Gate 3 GREEN** |
| 3 | Runtime & perf tail | `agent-loop-perf` | BR-53, 54, 55, 56, 57, 58, 59 | **merged — Gate 3 GREEN** |
| 3 | **Cross-platform** | `agent-loop-xplat` | **BR-68** (windows/linux command safety + shlex tokenizer fix), **BR-69** (linux Landlock + windows sandbox tier), **BR-70** (cross-compile CI gate), GAP-2 PID-reuse fix | **merged — Gate 3 GREEN** |
| 3 | Frontend cleanup | `agent-loop-frontend` | pre-existing lint (~40) + vitest reds (biorouterd.test.ts, ExtensionModal.test.tsx) | **merged — Gate 3 GREEN** |

Dependency notes: cluster *verify* needs BR-19 (hooks) merged; cluster *server*
builds on BR-33 (wave 0); BR-16 is subsumed by BR-33; BR-47 builds on BR-19.

## Campaign log

Newest first. Entries within a single day carry no clock time in the source; where a
day holds several entries they are ordered by the sequence their content implies.

- **2026-07-13** — **Cross-platform audit**
  ([platform parity audit](../../agent-loop/cross-platform/platform-parity-audit.md)) +
  design specs BR-68/69/70 committed (bd18d8ab). Verdict: **zero compile BREAKs** —
  the branch builds on macOS/Windows/Linux (every `#[cfg]` has a complementary arm;
  `libc` is correctly `[target.'cfg(unix)'.dependencies]`). But the campaign's
  *safety* work is POSIX-only in substance. Three gaps now scheduled for Wave 3:
  **GAP-1** on Windows the catastrophic denylist covers ~0 real threats yet loads,
  self-tests pass and reports as ON (users told they are protected when they are not);
  **GAP-2** BR-37's orphan reaper has no PID-reuse guard on Windows
  (`is_group_leader()` → `true`), so a stale pidfile + recycled PID can
  `taskkill /F /T` an innocent process tree — a bug this campaign introduced;
  **GAP-3** BR-21 tokenizes argv with POSIX `shlex`, mangling every absolute Windows
  path, so Windows rules will silently not match until fixed (GAP-1 + GAP-3 must ship
  together). Docker cross-checks (linux rust:1.92-bullseye, windows mingw) running.
- **2026-07-13** — **Decisions signed off** (user: "proceed with all of the default
  options"): flags stay default-off (checkpoints / sandbox / budgets remain opt-in,
  so merging changes no runtime behaviour); BR-54 SharedMcpPool **will be built** in
  the Wave-3 perf tail; shipped first slices are kept as-is (no extra scope);
  landing = finish Waves 2-3 and hand over one verified branch, **nothing merges to
  `main` without explicit sign-off**; pre-existing frontend lint/vitest reds folded
  into a Wave-3 cleanup cluster. Wave 2: loopdet complete 7/7 (BR-35 orphan adopted
  by its verifier as designed).
- **2026-07-12** — Gate 1 GREEN: full-workspace suite on merged integration —
  55 suites ok, 2024 tests passed (baseline 1786; +238 added by waves 0-1),
  sole failure = pre-existing live-API test_anthropic_provider. Wave 2
  launched: loopdet / hooks / server clusters off integration @ 70ce551e.
- **2026-07-12** — Gate 1: all five Wave-1 clusters merged into integration
  (checkpoints c7974c28 → compaction ee43bc0f → security ea6799aa → processes
  c38cf9ba → context 76855c18). Conflict resolutions: session_manager.rs
  3-way rebuild with BR-17 FTS migration renumbered 11→13,
  CURRENT_SCHEMA_VERSION=13; agent.rs field unions (checkpoints +
  eager_compactions + injected_skills; managed-policy HooksManager + checkpoint
  init); rmcp_developer.rs keeps BR-44 FileHistory + BR-23 secret_guard (which
  absorbed ignore_patterns by design). OpenAPI + TS client regenerated (zero
  diff — textual merge was already correct). Clippy too_many_lines baseline
  regenerated (13 entries). Post-merge targeted suites green (session 58,
  context_mgmt 20, checkpoint 14, developer 191, security 35, hooks 54,
  guardrails 29, prompt_manager 18, moim 11, agents 216). Full-workspace
  regression running as the final gate check. BR-40 async-handle remainder
  deferred to Wave 3. Known environment notes: cluster verifiers hit ENOSPC
  during parallel builds (mitigated: prune ~/.cache/br-targets between waves);
  pre-existing frontend lint/vitest failures on base (biorouterd.test.ts,
  ExtensionModal.test.tsx) queued for Wave-3 cleanup.
- **2026-07-12** — Wave 0 MERGED (129589ba): 10 BRs + seams + 6 designs, gate GREEN (zero new failures; lib 782 vs 755, mcp 584 vs 582, server 50/49 vs 47/46). Evidence: [wave-0-foundation.md](wave-reports/wave-0-foundation.md). Wave 1 launched: 5 cluster worktrees off 129589ba.
- **2026-07-12** — Baseline complete: **53 suites ok, 1 pre-existing failure**
  — `test_anthropic_provider` (`crates/biorouter/tests/providers.rs:251`, a
  *live-API* test asserting oversized input yields `ContextLengthExceeded`;
  the call instead succeeds with `finish_reason: None` — the exact BR-46 bug
  surface). Known-failing at baseline; only failures *beyond* this one count
  as regressions at any gate. Note: `tests/providers.rs` hits live provider
  APIs when keys are present — gate comparisons must tolerate its
  environment-dependence.
- **2026-07-12** — Campaign started. Integration branch cut from `main`
  (24cdc3a2) + review corpus merged (a409e7d7). Baseline full-workspace test
  run started. Wave 0 workflow launched.

## Related documentation

- [Outcome report](outcome-report.md) — what the campaign actually landed, with final test counts and the caveat list.
- [Mid-flight review index](mid-flight-review-index.md) — the Gate-1 snapshot handed to the reviewer; superseded by the outcome report but records the five open decisions as they were put.
- [Improvement proposals register](../agent-loop-review/improvement-proposals.md) — the definition of BR-1…BR-67, the source of every ticket in the wave table.
- [Platform parity audit](../../agent-loop/cross-platform/platform-parity-audit.md) — defines GAP-1/2/3 and the BR-68/69/70 cross-platform work added in Wave 3.
- [Wave reports](wave-reports/) — per-cluster verification evidence behind each "gate GREEN" in the table above.
