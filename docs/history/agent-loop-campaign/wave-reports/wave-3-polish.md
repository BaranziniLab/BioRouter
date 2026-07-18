# Wave 3 — polish cluster verification report

> **What this is.** Gate evidence for the campaign's final cluster, named "polish": BR-40's
> async subagent handle, BR-62b wiring the desktop Stop button to reliable cancel, and the
> non-proposal commit that greened the pre-existing frontend ESLint and vitest failures.
> **Status:** Historical record — verified 2026-07-13, cleared the gate, and merged into the
> campaign's `agent-loop-integration` branch as the last Wave 3 cluster.
> `agents/subagent_handle.rs` exists in the tree today and the frontend gate-greening landed.
> **Audience:** maintainers auditing what the campaign shipped and on what evidence.

The agent-loop fix campaign implemented 67 numbered proposals (`BR-1` … `BR-67`) from
[the master improvement-proposals list](../../agent-loop-review/improvement-proposals.md).
Related proposals were grouped into **clusters**, each built in its own git worktree, and
clusters shipped in dependency-ordered **waves**. Every wave had to clear a **gate**: a full
test run admitting zero new failures against a recorded baseline. This file is the gate
evidence for the "polish" cluster — the campaign's clean-up wave, which is why its three
commits have little in common beyond closing out loose ends. Campaign conventions and the wave
table are in [the campaign overview](../README.md).

> **Note.** Paths beginning `~/` or `.worktrees/` are on the verifier's machine as it was
> configured during the campaign; they are not repository paths.

Worktree `.worktrees/polish`. Verified 2026-07-13.
**Verdict: GREEN** — gate met, zero new failures.

## Cluster commits (`agent-loop-integration..HEAD`)

The third commit carries no proposal id: it is infrastructure work, not a numbered proposal.

| Commit | BR | Summary | Files | Tests |
|--------|----|---------|-------|-------|
| `9f47941f` | BR-40 | async subagent handle | `crates/biorouter/src/agents/{agent.rs, mod.rs, reply_parts.rs, subagent_handle.rs (new, 478 lines), subagent_result.rs, subagent_tool.rs}` | covered by `-p biorouter --lib` (agents/subagent) — 1182 passed |
| `3225ce5b` | BR-62b | wire desktop GUI to reliable-cancel + idempotent `/reply` | `ui/desktop/src/hooks/{chatStreamStore.tsx, chatStreamStore.test.ts}` | 3 new chatStreamStore unit tests; vitest 90 files / 731 pass |
| `65fd7227` | — | frontend gate-greening; green up pre-existing eslint + vitest reds | `ui/desktop/**` (eslint.config.js, App.tsx, ChatInput.tsx, ContextWindowIndicator.tsx, MessageCopyLink.tsx, icons/app-icons.tsx, ForceGraphCanvas.tsx, ExtensionModal.test.tsx, LocalModelInventory.tsx, WorkflowExpandableInfo.tsx, biorouterd.test.ts, test/setup.ts, react-force-graph-2d.d.ts, dependencyChecker.ts, extensionUpdater.ts, githubUpdater.ts, ollamaDetection.test.ts, package.json/lock) | `npm run test:run` 731 pass; `npm run lint:check` exit 0 (128 contrast assertions) |

BR-62b closes the follow-up that
[the Wave 2 server and cancel report](wave-2-server-cancellation.md) left explicitly undone.

Working tree clean (`git status --porcelain` empty). No orphaned work.

## Decisions

- **`65fd7227` is not a `BR-NN` commit and is intentionally left as-is.** It is not one of the
  numbered proposals; it is the prerequisite that greens the frontend gate — roughly 40
  pre-existing ESLint `no-undef` and `exhaustive-deps` errors plus 2 failing vitest files that
  were red on `main` before the campaign. The root cause was the flat ESLint config's
  hand-maintained `globals` list omitting browser and DOM globals. The message is descriptive
  and the work is coherent, so no split was needed.
- **No `just generate-openapi` needed.** The cluster touches only
  `crates/biorouter/src/agents/**` and `ui/desktop/**`; no `biorouter-server` route changed, so
  the OpenAPI spec and TS client are untouched.

> **Note.** A robustness proposal-lens document shows as deleted in
> `git diff agent-loop-integration..HEAD`, and that is **not** polish-cluster junk.
> `agent-loop-integration` (tip `29c1e1d4`) is **not** an ancestor of the polish HEAD
> (`65fd7227`); their merge-base is `a54c4d79`. The integration branch advanced by 2 commits
> (`ca835e33` docs and `29c1e1d4` merge) that added that file **after** the polish branch was
> cut, so a two-dot diff shows it as deleted on the polish side. It is work on the integration
> branch, not something the polish cluster added or dropped. No action. The file is
> [the robustness proposal lens](../../agent-loop-review/proposal-lenses/robustness.md).

## Gate results

| Check | Command | Result |
|-------|---------|--------|
| Disk | `df -h /` | 58Gi avail — OK (>8G) |
| fmt | `cargo fmt --all -- --check` | exit 0, clean |
| clippy | `./scripts/clippy-lint.sh` | exit 0 |
| clippy hand-check | too_many_lines vs baseline | see below |
| compile all targets | `cargo test --workspace --no-run` | exit 0 |
| Rust regression | `cargo test --workspace --no-fail-fast` | 2370 passed, 59 suites ok, 1 allowed red |
| frontend vitest | `npm run test:run` | 90 files / 731 tests pass |
| frontend lint | `npm run lint:check` | exit 0 |

### Clippy `too_many_lines` hand cross-check

This cross-check exists because the baseline parser in `scripts/clippy-lint.sh` is buggy and
cannot be trusted on its own.

Baseline (`clippy-baselines/too_many_lines.txt`) = 13 functions. The current run emitted **14**
`too many lines` warnings. All 13 baseline functions are present. The one extra:

- `crates/biorouter-bench/src/eval_suites/core/developer/simple_repo_clone_test.rs:22`
  (`SimpleRepoCloneTest::run`, 168/100).

This function exists **unchanged at `agent-loop-integration`** and the polish cluster does not
touch `biorouter-bench` at all (see the file list above). It is a pre-existing violation that
the buggy jq baseline parser silently drops — **not a polish regression.** No fix in scope for
this cluster.

> **Note.** Three sibling reports write up the same tooling defect independently, each from its
> own run: [Wave 2 — hooks and permissions](wave-2-hooks-and-permissions.md),
> [Wave 2 — loop detection](wave-2-loop-detection.md) and
> [the cross-platform parity verification report](../../../agent-loop/cross-platform/parity-verification-report.md).
> It was never fixed inside the campaign.

## Regression evidence

- **Gate baseline** = `~/.cache/br-baseline/gate2-test.log` (2332 passed, 59 suites ok). That is
  a machine-local log file from the campaign run; it is the comparison point this gate used and
  it does not live in the repository.
- **This run:** **2370 passed** — higher, because polish added BR-40 subagent and BR-62b
  `chatStreamStore` tests — **59 suites `ok`**, **1 suite `FAILED`**.
- The single failing suite is `-p biorouter --test providers`:
  `test result: FAILED. 14 passed; 1 failed` — the one failure is `test_anthropic_provider`, a
  **known-allowed** red against the live Anthropic API.
- `tunnel::lapstone_test`, the other known-flaky red, **passed** this run.
- No other new failures. The gate's zero-new-failures bar is met.

Representative `test result:` lines below — the largest of the 59 `ok` suites, not the full
list; the trailing `...` marks the elision:

```text
test result: ok. 1182 passed; 0 failed; 0 ignored   (biorouter --lib)
test result: ok. 623 passed; 0 failed; 2 ignored     (biorouter-mcp --lib)
test result: ok. 173 passed; 0 failed; 0 ignored
test result: ok. 65 passed; 0 failed; 0 ignored
test result: ok. 64 passed; 0 failed; 0 ignored
test result: ok. 31 passed; 0 failed; 0 ignored
test result: ok. 24 passed; 0 failed; 0 ignored
test result: ok. 22 passed; 0 failed; 0 ignored
test result: FAILED. 14 passed; 1 failed; 0 ignored  (providers — test_anthropic_provider, ALLOWED live-API red)
...
Frontend: Test Files 90 passed (90); Tests 731 passed (731)
Frontend lint: OK — all 128 contrast assertions pass, exit 0
```

## Summary for whoever lands this

Restating the three conclusions established above:

- No real regressions were found; no fixes were required.
- The only Rust red is the allowed live-API `test_anthropic_provider`.
- The 14th `too_many_lines` warning (`simple_repo_clone_test.rs`) is pre-existing on the
  integration base — do not attribute it to this cluster.

## Related documentation

- [Agent-loop fix campaign overview](../README.md) — the wave table, cluster conventions and
  merge status this report is evidence for.
- [Wave 2 — server and cancel cluster](wave-2-server-cancellation.md) — the report whose open
  follow-up BR-62b closed here.
- [Master improvement proposals](../../agent-loop-review/improvement-proposals.md) — the
  definition of BR-40 and BR-62.
- [Campaign outcome report](../outcome-report.md) — the end-of-campaign totals across all three
  gates, of which this is the last.
- [Cross-platform parity verification report](../../../agent-loop/cross-platform/parity-verification-report.md)
  — the other Wave 3 gate, covering the cross-platform proposals and GAP-2.
