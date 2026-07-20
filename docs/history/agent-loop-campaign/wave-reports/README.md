# Agent-loop campaign wave reports

This folder holds the ten per-cluster verification reports produced by the agent-loop fix
campaign, which ran 2026-07-12 to 2026-07-13. The campaign implemented 67 numbered
proposals (`BR-1` … `BR-67`) from the agentic-loop review, grouping related proposals into
**clusters** built in separate git worktrees and releasing them in dependency-ordered
**waves**. Every wave had to clear a **gate**: a full per-crate test run compared
line-for-line against a recorded baseline, admitting zero new failures. Each file here is
one cluster's gate evidence — the proposals that landed, their commits and files, the exact
test-result lines, and the regressions the verifier found and fixed. This work is finished
and merged to `main`; all ten reports record a GREEN verdict. The reports are kept as the
audit trail for what shipped and on what evidence, not as guidance for how the code works
today. The cluster branches and worktrees they name were deleted after the campaign landed.

Come here when you need to answer "which proposal introduced this, in which commit, and
what proved it worked?" for a specific `BR-NN` item. Go elsewhere for anything else: the
campaign's sequencing conventions, wave table and dated gate log are in
[the campaign overview](../README.md); the summary of what actually landed across all four
gates is in [the outcome report](../outcome-report.md); the proposals themselves are defined
in [the improvement proposals register](../../agent-loop-review/improvement-proposals.md);
and for how the agent loop behaves *now* — rather than what changed in July 2026 — read
[the agent-loop documentation](../../../agent-loop/README.md), which is the current truth
for every subsystem these reports touch.

## Documents in this folder

| Document | What it covers |
|---|---|
| [Wave 0 — foundation](wave-0-foundation.md) | The merge-gate record for Wave 0: the ten proposals that landed, the `agent.rs` seam refactor that made room for them, the clippy fixes the verifier committed, and the per-crate test counts proving zero regressions. |
| [Wave 1 — checkpoints and VCS](wave-1-checkpoints.md) | BR-43 shadow-git checkpoints, BR-44 persisted `text_editor` undo history and BR-45 stable message ids plus branch fork point, with the OpenAPI regeneration the verifier added and the disk and tunnel flakes it ruled out. |
| [Wave 1 — compaction and memory](wave-1-compaction.md) | BR-10 through BR-15 plus BR-17 FTS5 chat recall, including the FTS write-path hardening the verifier added and the cross-cluster SQLite schema-version collision it diagnosed. |
| [Wave 1 — context and prompts](wave-1-context-and-prompts.md) | BR-1 repo map, BR-2 context budget, BR-3 per-model prompt variants, BR-5, BR-8, BR-9 and BR-60 structured todos, including the insta-snapshot regression BR-60 introduced and the fix for it. |
| [Wave 1 — long-running and processes](wave-1-processes.md) | BR-37 orphan reaping, BR-40 subagent result envelope, BR-41 persisted goals and BR-42 active-work registry plus route, including the pinned-generator finding for OpenAPI regeneration. |
| [Wave 1 — security and guardrails](wave-1-security.md) | BR-21 policy engine slice 1, BR-22 tool-output guardrails, BR-23 central secret redaction, BR-64 macOS Seatbelt slice 1 and BR-65 managed policy tier, with the design decisions observed in each slice. |
| [Wave 2 — hooks and permissions](wave-2-hooks-and-permissions.md) | BR-27 content matchers, BR-28 hook aggregates, BR-19 hooks on the tool path, BR-18 SmartApprove revival, BR-24 scoped permissions and BR-63 reasoning effort, plus the fail-closed reasoning behind the two security-critical diffs. |
| [Wave 2 — loop detection and budgets](wave-2-loop-detection.md) | BR-29 staged repetition stop, BR-30 semantic and oscillation detection, BR-31 failure streaks, BR-32 stall checks, BR-35 budgets, BR-66 mistake streaks and BR-67 loop-safety observability, plus the record of adopting the orphaned BR-35 work. |
| [Wave 2 — server and cancel](wave-2-server-cancellation.md) | BR-6 large-response handling, BR-7 externalized blobs, BR-52 token state in the event stream, BR-61 interrupt wiring and BR-62 reliable cancel, plus the design decisions behind request-scoped confirmations and the follow-ups it deliberately left undone. |
| [Wave 3 — polish](wave-3-polish.md) | The campaign's final cluster: BR-40's async subagent handle, BR-62b wiring the desktop Stop button to reliable cancel, and the non-proposal commit that greened the pre-existing frontend ESLint and vitest failures. |

> **Note.** Only the Wave 2 hooks and Wave 3 polish reports carry a verification date
> (both 2026-07-13). The remaining runs are undated in the original record. The seven
> Wave 1 and Wave 2 reports say so in their own context headers; Wave 0's header records
> only that it merged in July 2026.

> **Warning.** Two reports carry caveats a reader should not skip. The security and the
> hooks clusters cover permission gating, command policy and sandboxing, where a green
> gate is explicitly **not** sufficient sign-off. The server and cancel report is thinner
> than its siblings and carries no per-crate evidence table, because both of that
> cluster's verifier agents were cut off mid-run and the orchestrator verified the
> proposals directly.

## Related documentation

- [Agent-loop fix campaign overview](../README.md) — the conventions these reports assume: branch and worktree naming, the wave-to-cluster-to-proposal mapping, and the regression-gate rule.
- [Campaign outcome report](../outcome-report.md) — what landed across all four gates, the test-count progression and the caveat list, once every cluster below had merged.
- [Campaign commit log](../commit-log.md) — one line per commit on the campaign branch, to look up a `BR-NN` proposal by its commit rather than by its cluster.
- [The agent loop](../../../agent-loop/README.md) — the current documentation for the subsystems these reports changed, including the per-proposal design documents and the cross-platform parity work.
