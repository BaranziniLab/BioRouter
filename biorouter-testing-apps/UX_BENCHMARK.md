# BioRouter CLI — UX / Aesthetics / Clarity Benchmark

Beyond "did it work," every interactive build is scored on the *experience* of
using the BioRouter CLI. Scores are 1–5 (5 = excellent). Notes capture concrete
observations (good and bad) that feed the issue reports in `ISSUES/`.

## Dimensions

1. **Request handling** — Does the agent correctly interpret the instruction and
   a follow-up's intent? Does it stay on task, scope appropriately, avoid
   re-doing work, and respect "don't ask questions" vs. genuinely-needed clarity?
2. **Tool-call behavior** — Are tool calls (shell, text_editor, todo) sensible,
   minimal, and well-sequenced? Any thrash, redundant reads, oversized shell
   output, failed calls, or wrong-path edits?
3. **Output clarity / presentation** — Is the streamed output readable? Are tool
   calls, diffs, results, and the final summary clearly presented? Is it obvious
   what changed and whether it succeeded?
4. **Aesthetics / polish** — Banner, spacing, color, alignment, progress
   indication, truncation behavior, final-summary quality.
5. **Iteration fidelity** — On `--resume`, does it retain context, build on prior
   work instead of restarting, and produce coherent incremental commits?
6. **Reliability** — Crashes, hangs, timeouts, session/resume failures, git
   mistakes, broken builds left behind.

## Scorecard (per app)

| # | App | Req | Tools | Clarity | Aesthetics | Iter | Reliab | Headline note |
|---|-----|-----|-------|---------|-----------|------|--------|---------------|
| 1 | algo-pathfinding-rs | 5 | 4 | 4 | 3 | 4 | 4 | One-shot built working 6-algo lib, 54 tests pass; 1× -32602 tool malformation; path abbrev hurts clarity |
| 2 | algo-sorting-visualizer-py | 5 | 5 | 4 | 3 | – | 5 | Clean 9-sort project, self-ran pytest 98×, 156 tests pass; diligent Python verification |
| 3 | algo-bst-avl-redblack-cpp | 4 | 3 | 4 | 3 | 5 | 2 | Initial build left BROKEN+unverified (reliab=2); but interactive fix turn fully recovered it (iter=5) |

| 4 | algo-graph-toolkit-rs | 5 | 4 | 4 | 3 | – | 2 | 13-module real binary crate, but shipped 3 RED edge-case tests + only 1 commit (reliab=2); fix turn running |
| 5 | algo-string-matching-py | 5 | 4 | 4 | 3 | 5 | 3 | 11 algorithms, 199 good tests, but clean-checkout pytest broke (src-layout); fix turn added pythonpath → out-of-box green (iter=5) |

**Pattern:** MiMo self-verifies rigorously for Rust/Python (runs cargo test / pytest
repeatedly) but skipped compilation entirely for C++/cmake — declaring "done" on a
non-building repo. Verification discipline appears language-dependent.
**Pattern 2:** "works in my session, broken on clean checkout" recurs — missing
commits, src-layout import path, tolerated red tests. The agent optimizes for its
own transient environment, not a reproducible repo. The interactive fix turns
reliably recover all of these, which is the strongest positive signal: **BioRouter
is highly effective at iterative repair when given a precise failure.**

## Cross-cutting observations
(appended as patterns emerge — these become ISSUES/ entries)
