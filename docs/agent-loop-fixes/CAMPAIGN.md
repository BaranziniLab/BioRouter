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
| 0 | Foundation + seams + designs | `agent-loop-wave0` | BR-46, 25, 36, 38, 20, 39, 34, 26, 4, 33 (+BR-16 subsumed); agent.rs seam refactor; design docs BR-43/54/21/17/45/65 | **in progress** |
| 1 | Compaction & memory | `agent-loop-compaction` | BR-10, 11, 12, 13, 14, 15, 17 | pending |
| 1 | Security & guardrails | `agent-loop-security` | BR-21, 22, 23, 64, 65 | pending |
| 1 | Checkpoints & VCS | `agent-loop-checkpoints` | BR-43, 44, 45 | pending |
| 1 | Long-running & processes | `agent-loop-processes` | BR-37, 40, 41, 42 | pending |
| 1 | Context & prompts | `agent-loop-context` | BR-1, 2, 3, 5, 8, 9, 60 | pending |
| 2 | Loop, stuck & budgets | `agent-loop-loopdet` | BR-29, 30, 31, 32, 35, 66, 67 | pending |
| 2 | Hooks & permissions | `agent-loop-hooks` | BR-18, 19, 24, 27, 28, 63 | pending |
| 2 | Server & cancel | `agent-loop-server` | BR-33 follow-ups, 52, 61, 62, 6, 7 | pending |
| 3 | Verification & done-ness | `agent-loop-verify` | BR-47, 48, 49, 50, 51 (needs BR-19) | pending |
| 3 | Runtime & perf tail | `agent-loop-perf` | BR-53, 54, 55, 56, 57, 58, 59 | pending |

Dependency notes: cluster *verify* needs BR-19 (hooks) merged; cluster *server*
builds on BR-33 (wave 0); BR-16 is subsumed by BR-33; BR-47 builds on BR-19.

## Log

- **2026-07-12** — Campaign started. Integration branch cut from `main`
  (24cdc3a2) + review corpus merged (a409e7d7). Baseline full-workspace test
  run started. Wave 0 workflow launched.
