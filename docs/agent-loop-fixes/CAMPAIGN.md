# Agent-Loop Fix Campaign

Implementation campaign for the 67 proposals in
[`../agent-loop-review/PROPOSALS.md`](../agent-loop-review/PROPOSALS.md)
(BR-1…BR-67). Strategy: clustered worktrees in dependency-ordered waves off
this integration branch. Every wave is regression-gated against the baseline
before merging.

## Conventions

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
- **Regression gate:** full `cargo test --workspace --no-fail-fast` compared
  against the baseline log (`~/.cache/br-baseline/workspace-test.log`, taken at
  integration commit a409e7d7). A wave merges only with **zero new failures**.
  Frontend `npm run test:run` gates any wave that touches `ui/desktop`.

## Waves

| Wave | Cluster | Branch | Proposals | Status |
|---|---|---|---|---|
| 0 | Foundation + seams + designs | `agent-loop-wave0` | BR-46, 25, 36, 38, 20, 39, 34, 26, 4, 33 (+BR-16 subsumed); agent.rs seam refactor; design docs BR-43/54/21/17/45/65 | **merged 129589ba — gate GREEN** |
| 1 | Compaction & memory | `agent-loop-compaction` | BR-15, 10, 11, 13, 14, 12, 17 | **merged — gate GREEN** |
| 1 | Security & guardrails | `agent-loop-security` | BR-22, 23, 21, 65, 64 | **merged — gate GREEN** |
| 1 | Checkpoints & VCS | `agent-loop-checkpoints` | BR-43, 44, 45 | **merged — gate GREEN** |
| 1 | Long-running & processes | `agent-loop-processes` | BR-37, 40, 41, 42 | **merged — gate GREEN** |
| 1 | Context & prompts | `agent-loop-context` | BR-5, 2, 9, 8, 3, 60, 1 | **merged — gate GREEN** |
| 2 | Loop, stuck & budgets | `agent-loop-loopdet` | BR-29, 30, 31, 32, 35, 66, 67 | **in progress** |
| 2 | Hooks & permissions | `agent-loop-hooks` | BR-18, 19, 24, 27, 28, 63 | **in progress** |
| 2 | Server & cancel | `agent-loop-server` | BR-6, 7, 52, 61, 62 | **in progress** |
| 3 | Verification & done-ness | `agent-loop-verify` | BR-47, 48, 49, 50, 51 (needs BR-19) | pending |
| 3 | Runtime & perf tail | `agent-loop-perf` | BR-53, 54, 55, 56, 57, 58, 59 | pending |

Dependency notes: cluster *verify* needs BR-19 (hooks) merged; cluster *server*
builds on BR-33 (wave 0); BR-16 is subsumed by BR-33; BR-47 builds on BR-19.

## Log

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
- **2026-07-12** — Campaign started. Integration branch cut from `main`
  (24cdc3a2) + review corpus merged (a409e7d7). Baseline full-workspace test
  run started. Wave 0 workflow launched.
- **2026-07-12** — Wave 0 MERGED (129589ba): 10 BRs + seams + 6 designs, gate GREEN (zero new failures; lib 782 vs 755, mcp 584 vs 582, server 50/49 vs 47/46). Evidence: wave0.md. Wave 1 launched: 5 cluster worktrees off 129589ba.
- **2026-07-12** — Baseline complete: **53 suites ok, 1 pre-existing failure**
  — `test_anthropic_provider` (`crates/biorouter/tests/providers.rs:251`, a
  *live-API* test asserting oversized input yields `ContextLengthExceeded`;
  the call instead succeeds with `finish_reason: None` — the exact BR-46 bug
  surface). Known-failing at baseline; only failures *beyond* this one count
  as regressions at any gate. Note: `tests/providers.rs` hits live provider
  APIs when keys are present — gate comparisons must tolerate its
  environment-dependence.
