# Wave 0 — foundation cluster verification report

> **What this is.** The merge-gate verification record for Wave 0 of the agent-loop fix
> campaign: the ten proposals that landed, the `agent.rs` seam refactor that made room for
> them, the clippy fixes the verifier had to commit, and the per-crate test counts proving
> zero regressions against the campaign baseline.
> **Status:** Historical record — Wave 0 passed this gate and merged into the campaign's
> `agent-loop-integration` branch in July 2026. The four seam methods it introduces
> (`assemble_turn_context`, `inspect_and_gate_tool_requests`, `integrate_tool_result`,
> `record_turn_usage`) still exist in `crates/biorouter/src/agents/agent.rs`, but the value
> of this document is the evidence, not the architecture description.
> **Audience:** maintainers auditing what the campaign shipped and on what evidence.

The agent-loop fix campaign implemented 67 numbered proposals (`BR-1` … `BR-67`) from
[the master improvement-proposals list](../../agent-loop-review/improvement-proposals.md).
Work was split into **clusters** — groups of related proposals, each developed in its own
git worktree on its own branch — and clusters were released in dependency-ordered **waves**.
Each wave had to clear a **gate**: a full test run compared line-for-line against a recorded
baseline, admitting **zero new failures**. This file is Wave 0's gate evidence. The campaign
conventions, wave table and merge status live in
[the campaign overview](../README.md).

> **Note.** Paths beginning `~/` or `.worktrees/` are on the verifier's machine as it was
> configured during the campaign; they are not repository paths.

Wave 0 was run against a worktree at `.worktrees/wave0`, comparing
`agent-loop-integration..HEAD`.

> **Note.** This report records the worktree's branch as `ui-hardening-a11y-tests`, while
> every sibling wave report names a branch of the form `agent-loop-<cluster>`. The
> discrepancy is preserved as written; it was never explained in the original record.

**Merge gate: GREEN.** Zero new (unexplained) test failures across all seven crates. The
single test failure observed (`test_anthropic_provider`) is pre-existing in the baseline (a
live-API test with no credentials) and is not a regression. Clippy and rustfmt are clean for
Wave 0 code after two small fixes committed here.

## What landed

Implemented proposals, committed to this worktree ahead of `agent-loop-integration`.
Proposal titles are from
[the improvement-proposals list](../../agent-loop-review/improvement-proposals.md).

| BR | Proposal | Status | Commit | Primary files | Tests exercising it |
|----|----------|--------|--------|---------------|---------------------|
| BR-4  | Move core disciplines into `system.md` | done | `c9faa523` | `agents/prompt_manager.rs`, `prompts/system.md`, `todo_extension.rs`, `code_execution_extension.rs` (+3 prompt snapshots) | biorouter lib (`prompt_manager` snapshots) |
| BR-20 | Always-on non-bypassable catastrophic-command denylist | done | `f9f15b59` | `security/patterns.rs` (new, 354 L), `security/mod.rs`, `security/security_inspector.rs`, `agents/agent.rs` | biorouter lib (`security::`) |
| BR-25 | Fix `unwrap()` panics in the permission store | done | `53088bc8` | `permission/permission_store.rs` | biorouter lib (`permission::`) |
| BR-26 | Output-size limits + untrusted framing on injected hook stdout | done | `52867b37` | `hooks/outcome.rs` (cap + untrusted frame), `agents/agent.rs` | biorouter lib (`hooks::outcome`), `tests/hooks_agent_loop_tests.rs` |
| BR-33 | Server-enforced single-turn-per-session lock | done | `53160a6e` | `biorouter-server/src/routes/reply.rs`, `state.rs` | biorouter-server lib + route tests |
| BR-34 | Absolute per-tool call ceiling + tool-call (not just iteration) turn cap | done | `fa5a0d0c` | `agents/agent.rs`, `agents/types.rs`, `tests/agent.rs` (+7 call-site threading changes) | `biorouter/tests/agent.rs` (167 new lines) |
| BR-36 | Consolidate the two `RepetitionInspector` implementations | done | `58535da2` | `tool_monitor.rs`, `agents/retry.rs`, `tests/repetition_inspector_tests.rs` | `tests/repetition_inspector_tests.rs` (3 tests) |
| BR-38 | Reconcile `currently_running` on scheduler load | done | `0393b122` | `scheduler.rs` | biorouter lib (`scheduler`) |
| BR-39 | `shell_list` tool for background jobs | done | `0d07221b` | `biorouter-mcp/src/developer/background.rs`, `rmcp_developer.rs` | biorouter-mcp lib (`developer::`) |
| BR-46 | Fix Anthropic `finish_reason` so length-truncation continuation works | done | `be6f087c` | `providers/formats/anthropic.rs` | biorouter lib (`providers::formats::anthropic`) |

Design-only items — architectural design docs written pre-implementation, not code, all in
commit `703717dc`. They were filed under `docs/agent-loop-fixes/designs/` at the time and
have since moved:

| BR | Design document |
|----|-----------------|
| BR-17 | [Cross-session memory](../../../agent-loop/designs/cross-session-memory.md) |
| BR-21 | [Command policy engine](../../../agent-loop/designs/command-policy-engine.md) |
| BR-43 | [Shadow-git checkpoints](../../../agent-loop/designs/shadow-git-checkpoints.md) |
| BR-45 | [Session branching](../../../agent-loop/designs/session-branching.md) |
| BR-54 | [Shared MCP server pool](../../../agent-loop/designs/shared-mcp-server-pool.md) |
| BR-65 | [Managed policy tier](../../../agent-loop/designs/managed-policy-tier.md) |

Supporting commits:

- `2de2d500` — seam refactor of `agent.rs` (no behaviour change; see the seam-refactor
  section below).
- `e03c7516` — `cargo fmt --all` drift in untouched files (no behaviour change).
- `f89ec104` — clippy fixes committed by this verifier (see the clippy section below).

### Working-tree state at start

`git status --porcelain` was **clean** — no orphaned or uncommitted implementer work to
rescue or revert.

## Seam refactor (`2de2d500`)

`refactor(agent): extract seam methods in agent.rs (no behavior change)` (+174 / −78, single
file). The monolithic reply/tool-dispatch loop in `agents/agent.rs` was decomposed into four
named, individually-testable seam methods, giving each Wave 0 proposal a clean insertion
point instead of editing one giant function:

- `assemble_turn_context(...)` — builds the per-turn context/messages.
- `inspect_and_gate_tool_requests(...)` — runs tool inspection and permission gating before
  dispatch (the seam BR-20 / BR-25 / BR-34 hook into).
- `integrate_tool_result(...)` — validates one completed tool result, records it for
  PostToolUse hooks, writes it into the response slot (the seam BR-26 hooks into).
- `record_turn_usage(...)` — token and usage accounting for the turn.

Behaviour is unchanged (pure extraction); the full biorouter suite passing at the same counts
as baseline plus the newly-added proposal tests confirms this.

## Regression findings and resolutions

**No regressions found.** Per-crate suites were compared line-for-line against the baseline
(`~/.cache/br-baseline/summary.txt` plus `workspace-test.log`, `DONE` marker present —
baseline complete).

- The only FAILED test anywhere is `test_anthropic_provider`
  (`crates/biorouter/tests/providers.rs`). It is present and failing in the baseline log
  (baseline line 1731: `test test_anthropic_provider ... FAILED`, suite result
  `FAILED. 14 passed; 1 failed`). It is a live Anthropic API test that requires network and
  credentials; it fails identically here (`14 passed; 1 failed`). **Pre-existing, not a
  regression.** No fix required.
- All other crates: green, with pass counts equal to or above baseline. Wave 0 adds tests —
  biorouter lib 755→782, biorouter-mcp lib 582→584, biorouter-server lib 47/46→50/49.

No `BR-NN: fix regression` commits were needed.

## Clippy and rustfmt

**rustfmt:** `cargo fmt --all -- --check` is clean (exit 0) at every point, including after
the verifier's edits.

**Clippy** (`./scripts/clippy-lint.sh`, which runs `cargo clippy --all-targets -- -D warnings`
plus a baseline-rules pass): the initial run failed with **3 clippy `-D warnings` errors, all
in Wave-0-introduced code**, now fixed and committed (`f89ec104`,
`fix(clippy): resolve wave-0 clippy warnings`):

1. `clippy::too_many_arguments` on `integrate_tool_result` (8/7 args) — the seam method from
   `2de2d500`. Fixed with `#[allow(clippy::too_many_arguments)]` on the extracted seam (its
   arg list mirrors the loop-local state it replaced).
2. and 3. `clippy::string_slice` ×2 in `hooks/outcome.rs` (BR-26, `cap_hook_context`) — the
   workspace enforces `string_slice = "warn"` plus `-D warnings`. The slices were already
   char-boundary-safe (computed via `floor_char_boundary`); rewrote `&s[..head_end]` /
   `&s[tail_start..]` as `s.get(..head_end)` / `s.get(tail_start..)` to satisfy the lint
   without changing behaviour.

After the fix the `-D warnings` clippy pass **compiles clean**.

### Pre-existing `too_many_lines` findings, left as-is

The baseline-rules checker still reports two functions over 100 lines that are absent from
the stale allowlist `clippy-baselines/too_many_lines.txt`:

- `crates/biorouter-mcp/src/agent_drafter/render.rs::serve_mjs` (161 L) — file **not touched
  by Wave 0 at all**.
- `crates/biorouter-mcp/src/agent_drafter/control.rs::validate_widget` (102 L) — function body
  **not modified by Wave 0**; the only Wave 0 change to `control.rs` was `cargo fmt` drift in
  `validate_chart` and the `tests` module (hunks at L458 / L1564 / L1715), none inside
  `validate_widget` (spans L280–388).

Both functions are byte-identical to `agent-loop-integration`, so this check was already red
before Wave 0 — the allowlist predates these `agent_drafter` functions crossing 100 lines. It
is outside the Wave 0 mandate ("fix clippy errors in Wave 0 code only"). Four sibling wave
reports record the same stale-allowlist reds independently; see
[Wave 1 — compaction](wave-1-compaction.md),
[Wave 1 — processes](wave-1-processes.md),
[Wave 1 — context and prompts](wave-1-context-and-prompts.md) and
[Wave 1 — security](wave-1-security.md).

## Open item left for maintainers

One optional follow-up was recorded by this gate and deliberately not done inside it:

- Regenerate the stale `too_many_lines` allowlist with
  `./scripts/clippy-baseline.sh generate clippy::too_many_lines`, or refactor `serve_mjs` and
  `validate_widget`, on a separate housekeeping change.

## Test-result evidence, per crate

Command form:

```bash
CARGO_TARGET_DIR=~/.cache/br-targets/wave0 cargo test -p <crate> --no-fail-fast
```

Every crate below is green except the pre-existing live-API provider test.

**biorouter**

```text
lib:        test result: ok. 782 passed; 0 failed; 0 ignored; ...  (baseline 755)
tests/providers.rs: test result: FAILED. 14 passed; 1 failed; ... (test_anthropic_provider — PRE-EXISTING, live API)
all other test binaries (repetition_inspector, session_*, subagent_tool,
  tetrate_streaming, tool_inspection_manager, agent, hooks_agent_loop_tests, ...): ok
doc-tests: test result: ok. 2 passed; 0 failed; ...
```

**biorouter-mcp**

```text
lib:        test result: ok. 584 passed; 0 failed; 2 ignored; ...  (baseline 582)
integration binaries: ok (2, 1, 2, 1, 1, 0/2-ignored, 5, 0 passed respectively)
```

**biorouter-server**

```text
suite 1: test result: ok. 50 passed; 0 failed; ...  (baseline 47)
suite 2: test result: ok. 49 passed; 0 failed; ...  (baseline 46)
route/other suites: ok (31, 1, 6 passed)
```

**biorouter-cli**

```text
test result: ok. 173 passed; 0 failed; 0 ignored; ...  (matches baseline 173)
```

**biorouter-acp**

```text
test result: ok. 16 passed; 0 failed; ...
test result: ok. 11 passed; 0 failed; ...
test result: ok. 1 passed; 0 failed; ...
```

**biorouter-bench**

```text
test result: ok. 0 passed; 0 failed; ...  (no tests; compiles clean)
```

**biorouter-test**

```text
test result: ok. 0 passed; 0 failed; ...  (harness crate; compiles clean)
```

## Gate verdict

**GREEN — safe to merge.** Zero new failures. fmt clean. Wave 0 clippy clean (3 errors found
and fixed in `f89ec104`). One pre-existing baseline test failure (`test_anthropic_provider`,
live API) and one pre-existing stale-allowlist `too_many_lines` finding in untouched
`agent_drafter` code — both documented, neither a Wave 0 regression, neither blocks the gate.

## Related documentation

- [Agent-loop fix campaign overview](../README.md) — the wave table, cluster conventions and
  merge status this report is evidence for.
- [Master improvement proposals](../../agent-loop-review/improvement-proposals.md) — the
  definition of every `BR-NN` identifier used above.
- [Campaign outcome report](../outcome-report.md) — the end-of-campaign totals across all
  gates, including Wave 0's contribution.
- [Campaign commit log](../commit-log.md) — maps each commit SHA cited here to its proposal.
- [Wave 1 — compaction and memory cluster](wave-1-compaction.md) — the next gate, which
  reports the same stale clippy allowlist from the other side.
