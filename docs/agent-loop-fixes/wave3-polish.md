# Wave 3 — "polish" cluster verification report

Worktree: `/Users/wanjun/Desktop/BioRouter/.worktrees/polish`
Verified: 2026-07-13
Verdict: **GREEN** — gate met, zero new failures.

## Cluster commits (`agent-loop-integration..HEAD`)

| Commit | BR | Summary | Files | Tests |
|--------|----|---------|-------|-------|
| `9f47941f` | BR-40 | async subagent handle | `crates/biorouter/src/agents/{agent.rs, mod.rs, reply_parts.rs, subagent_handle.rs (new, 478 lines), subagent_result.rs, subagent_tool.rs}` | covered by `-p biorouter --lib` (agents/subagent) — 1182 passed |
| `3225ce5b` | BR-62b | wire desktop GUI to reliable-cancel + idempotent `/reply` | `ui/desktop/src/hooks/{chatStreamStore.tsx, chatStreamStore.test.ts}` | 3 new chatStreamStore unit tests; vitest 90 files / 731 pass |
| `65fd7227` | (frontend gate-greening; not a numbered proposal) | green up pre-existing eslint + vitest reds | `ui/desktop/**` (eslint.config.js, App.tsx, ChatInput.tsx, ContextWindowIndicator.tsx, MessageCopyLink.tsx, icons/app-icons.tsx, ForceGraphCanvas.tsx, ExtensionModal.test.tsx, LocalModelInventory.tsx, WorkflowExpandableInfo.tsx, biorouterd.test.ts, test/setup.ts, react-force-graph-2d.d.ts, dependencyChecker.ts, extensionUpdater.ts, githubUpdater.ts, ollamaDetection.test.ts, package.json/lock) | `npm run test:run` 731 pass; `npm run lint:check` exit 0 (128 contrast assertions) |

Working tree clean (`git status --porcelain` empty). No orphaned work.

## Decisions

- **`65fd7227` is not a BR-NN commit and is intentionally left as-is.** It is not
  one of the numbered proposals; it is the prerequisite that greens the frontend
  gate (~40 pre-existing ESLint `no-undef`/`exhaustive-deps` errors + 2 failing
  vitest files that were red on `main` before the campaign). Root cause was the
  flat ESLint config's hand-maintained `globals` list omitting browser/DOM
  globals. Message is descriptive and the work is coherent — no split needed.

- **`proposals/robustness.md` in `git diff agent-loop-integration..HEAD` is NOT
  polish-cluster junk.** `agent-loop-integration` (tip `29c1e1d4`) is **not** an
  ancestor of the polish HEAD (`65fd7227`); their merge-base is `a54c4d79`. The
  integration branch advanced by 2 commits (`ca835e33` docs + `29c1e1d4` merge)
  that added `proposals/robustness.md` **after** the polish branch was cut, so a
  two-dot diff shows that file as "deleted" on the polish side. It is work on the
  integration branch, not something the polish cluster added or dropped. No action.

- **No `just generate-openapi` needed.** The cluster touches only
  `crates/biorouter/src/agents/**` and `ui/desktop/**`; no `biorouter-server`
  route changed, so the OpenAPI spec / TS client are untouched.

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

### Clippy too_many_lines hand cross-check (baseline-parser bug workaround)

Baseline (`clippy-baselines/too_many_lines.txt`) = 13 functions. Current run
emitted **14** `too many lines` warnings. All 13 baseline functions present.
The one extra:

- `crates/biorouter-bench/src/eval_suites/core/developer/simple_repo_clone_test.rs:22`
  (`SimpleRepoCloneTest::run`, 168/100).

This function exists **unchanged at `agent-loop-integration`** and the polish
cluster does not touch `biorouter-bench` at all (see file list above). It is a
pre-existing violation the buggy jq baseline parser silently drops — **not a
polish regression.** No fix in scope for this cluster.

## Regression evidence

- **GATE BASELINE** = `/Users/wanjun/.cache/br-baseline/gate2-test.log` (2332 passed, 59 suites ok).
- **This run:** **2370 passed** (higher — polish added BR-40 subagent + BR-62b
  chatStreamStore tests), **59 suites `ok`**, **1 suite `FAILED`**.
- The single failing suite is `-p biorouter --test providers`:
  `test result: FAILED. 14 passed; 1 failed` — the one failure is
  `test_anthropic_provider`, a **KNOWN-ALLOWED** red (live Anthropic API).
- `tunnel::lapstone_test` (the other known-flaky red) **passed** this run.
- No other new failures. Gate (zero new failures) met.

Representative `test result:` lines (59 ok suites; largest first):

```
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

## Must-knows

- No real regressions found; no fixes were required.
- The only Rust red is the allowed live-API `test_anthropic_provider`.
- The 14th too_many_lines warning (`simple_repo_clone_test.rs`) is pre-existing
  on the integration base — do not attribute it to this cluster.
