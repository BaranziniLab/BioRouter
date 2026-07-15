# Wave-0 Verification Report — Agent-Loop Fix Campaign

Verifier run against branch `ui-hardening-a11y-tests` worktree at
`/Users/wanjun/Desktop/biorouter/.worktrees/wave0`, comparing
`agent-loop-integration..HEAD`.

**Merge gate: GREEN.** Zero new (unexplained) test failures across all seven
crates. The single test failure observed (`test_anthropic_provider`) is
pre-existing in the baseline (a live-API test with no credentials) and is not a
regression. Clippy and rustfmt are clean for wave-0 code after two small fixes
committed here.

---

## 1. What landed (BR → status / commit / files / tests)

Implemented BRs (committed to this worktree ahead of `agent-loop-integration`):

| BR | Status | Commit | Primary files | Tests exercising it |
|----|--------|--------|---------------|---------------------|
| BR-4  | done | `c9faa523` | `agents/prompt_manager.rs`, `prompts/system.md`, `todo_extension.rs`, `code_execution_extension.rs` (+3 prompt snapshots) | biorouter lib (`prompt_manager` snapshots) |
| BR-20 | done | `f9f15b59` | `security/patterns.rs` (new, 354 L), `security/mod.rs`, `security/security_inspector.rs`, `agents/agent.rs` | biorouter lib (`security::`) |
| BR-25 | done | `53088bc8` | `permission/permission_store.rs` | biorouter lib (`permission::`) |
| BR-26 | done | `52867b37` | `hooks/outcome.rs` (cap + untrusted frame), `agents/agent.rs` | biorouter lib (`hooks::outcome`), `tests/hooks_agent_loop_tests.rs` |
| BR-33 | done | `53160a6e` | `biorouter-server/src/routes/reply.rs`, `state.rs` | biorouter-server lib + route tests |
| BR-34 | done | `fa5a0d0c` | `agents/agent.rs`, `agents/types.rs`, `tests/agent.rs` (+7 call-site threading changes) | `biorouter/tests/agent.rs` (167 new lines) |
| BR-36 | done | `58535da2` | `tool_monitor.rs`, `agents/retry.rs`, `tests/repetition_inspector_tests.rs` | `tests/repetition_inspector_tests.rs` (3 tests) |
| BR-38 | done | `0393b122` | `scheduler.rs` | biorouter lib (`scheduler`) |
| BR-39 | done | `0d07221b` | `biorouter-mcp/src/developer/background.rs`, `rmcp_developer.rs` | biorouter-mcp lib (`developer::`) |
| BR-46 | done | `be6f087c` | `providers/formats/anthropic.rs` | biorouter lib (`providers::formats::anthropic`) |

Design-only (architectural design docs written pre-implementation, not code —
commit `703717dc`), under `docs/agent-loop-fixes/designs/`:

| BR | Artifact |
|----|----------|
| BR-17 | `designs/BR-17-design.md` |
| BR-21 | `designs/BR-21-design.md` |
| BR-43 | `designs/BR-43-design.md` |
| BR-45 | `designs/BR-45-design.md` |
| BR-54 | `designs/BR-54-design.md` |
| BR-65 | `designs/BR-65-design.md` |

Supporting commits:
- `2de2d500` — seam refactor of `agent.rs` (no behavior change; see §2).
- `e03c7516` — `cargo fmt --all` drift in untouched files (no behavior change).
- `f89ec104` — clippy fixes committed by this verifier (see §4).

### Working-tree state at start
`git status --porcelain` was **clean** — no orphaned/uncommitted implementer
work to rescue or revert.

---

## 2. Seam refactor summary (`2de2d500`)

`refactor(agent): extract seam methods in agent.rs (no behavior change)`
(+174 / −78, single file). The monolithic reply/tool-dispatch loop in
`agents/agent.rs` was decomposed into four named, individually-testable seam
methods, giving each wave-0 BR a clean insertion point instead of editing one
giant function:

- `assemble_turn_context(...)` — builds the per-turn context/messages.
- `inspect_and_gate_tool_requests(...)` — runs tool inspection + permission
  gating before dispatch (the seam BR-20 / BR-25 / BR-34 hook into).
- `integrate_tool_result(...)` — validates one completed tool result, records
  it for PostToolUse hooks, writes it into the response slot (the seam BR-26
  hooks into).
- `record_turn_usage(...)` — token/usage accounting for the turn.

Behavior is unchanged (pure extraction); the full biorouter suite passing at the
same counts as baseline plus the newly-added BR tests confirms this.

---

## 3. Regression findings and resolutions

**No regressions found.** Per-crate suites were compared line-for-line against
the baseline (`/Users/wanjun/.cache/br-baseline/summary.txt` +
`workspace-test.log`, `DONE` marker present — baseline complete).

- The only FAILED test anywhere is `test_anthropic_provider`
  (`crates/biorouter/tests/providers.rs`). It is present and failing in the
  baseline log (baseline line 1731: `test test_anthropic_provider ... FAILED`,
  suite result `FAILED. 14 passed; 1 failed`). It is a live Anthropic API test
  that requires network + credentials; it fails identically here
  (`14 passed; 1 failed`). **Pre-existing, not a regression.** No fix required.
- All other crates: green, with pass counts equal to or above baseline (wave-0
  adds tests: biorouter lib 755→782, biorouter-mcp lib 582→584,
  biorouter-server lib 47/46→50/49).

No `BR-NN: fix regression` commits were needed.

---

## 4. Clippy / fmt notes

**rustfmt:** `cargo fmt --all -- --check` is clean (exit 0) at every point,
including after the verifier's edits.

**Clippy** (`./scripts/clippy-lint.sh`, which runs
`cargo clippy --all-targets -- -D warnings` + a baseline-rules pass):

The initial run failed with **3 clippy `-D warnings` errors, all in
wave-0-introduced code**, now fixed and committed (`f89ec104`,
`fix(clippy): resolve wave-0 clippy warnings`):

1. `clippy::too_many_arguments` on `integrate_tool_result` (8/7 args) — the
   seam method from `2de2d500`. Fixed with `#[allow(clippy::too_many_arguments)]`
   on the extracted seam (its arg list mirrors the loop-local state it replaced).
2. + 3. `clippy::string_slice` ×2 in `hooks/outcome.rs` (BR-26,
   `cap_hook_context`) — the workspace enforces `string_slice = "warn"` +
   `-D warnings`. The slices were already char-boundary-safe (computed via
   `floor_char_boundary`); rewrote `&s[..head_end]` / `&s[tail_start..]` as
   `s.get(..head_end)` / `s.get(tail_start..)` to satisfy the lint without
   changing behavior.

After the fix the `-D warnings` clippy pass **compiles clean**.

**Remaining `clippy-baseline.sh` `too_many_lines` finding — pre-existing, NOT
wave-0, left as-is:** the baseline-rules checker still reports two functions over
100 lines that are absent from the stale allowlist
`clippy-baselines/too_many_lines.txt`:
- `crates/biorouter-mcp/src/agent_drafter/render.rs::serve_mjs` (161 L) —
  file **not touched by wave-0 at all**.
- `crates/biorouter-mcp/src/agent_drafter/control.rs::validate_widget` (102 L) —
  function body **not modified by wave-0**; the only wave-0 change to
  `control.rs` was `cargo fmt` drift in `validate_chart` and the `tests` module
  (hunks at L458 / L1564 / L1715), none inside `validate_widget` (spans L280–388).

Both functions are byte-identical to `agent-loop-integration`, so this check was
already red before wave-0 (the allowlist predates these agent_drafter functions
crossing 100 lines). It is outside the wave-0 mandate ("fix clippy errors in
wave-0 code only"). **Human action item (optional):** regenerate the allowlist
with `./scripts/clippy-baseline.sh generate clippy::too_many_lines`, or refactor
`serve_mjs`/`validate_widget`, on a separate housekeeping change.

---

## 5. Exact test-result evidence (per crate)

Command form: `CARGO_TARGET_DIR=/Users/wanjun/.cache/br-targets/wave0 cargo test
-p <crate> --no-fail-fast`.

**biorouter**
```
lib:        test result: ok. 782 passed; 0 failed; 0 ignored; ...  (baseline 755)
tests/providers.rs: test result: FAILED. 14 passed; 1 failed; ... (test_anthropic_provider — PRE-EXISTING, live API)
all other test binaries (repetition_inspector, session_*, subagent_tool,
  tetrate_streaming, tool_inspection_manager, agent, hooks_agent_loop_tests, ...): ok
doc-tests: test result: ok. 2 passed; 0 failed; ...
```

**biorouter-mcp**
```
lib:        test result: ok. 584 passed; 0 failed; 2 ignored; ...  (baseline 582)
integration binaries: ok (2, 1, 2, 1, 1, 0/2-ignored, 5, 0 passed respectively)
```

**biorouter-server**
```
suite 1: test result: ok. 50 passed; 0 failed; ...  (baseline 47)
suite 2: test result: ok. 49 passed; 0 failed; ...  (baseline 46)
route/other suites: ok (31, 1, 6 passed)
```

**biorouter-cli**
```
test result: ok. 173 passed; 0 failed; 0 ignored; ...  (matches baseline 173)
```

**biorouter-acp**
```
test result: ok. 16 passed; 0 failed; ...
test result: ok. 11 passed; 0 failed; ...
test result: ok. 1 passed; 0 failed; ...
```

**biorouter-bench**
```
test result: ok. 0 passed; 0 failed; ...  (no tests; compiles clean)
```

**biorouter-test**
```
test result: ok. 0 passed; 0 failed; ...  (harness crate; compiles clean)
```

---

## 6. Gate verdict

**GREEN — safe to merge.** Zero new failures. fmt clean. Wave-0 clippy clean
(3 errors found + fixed in `f89ec104`). One pre-existing baseline test failure
(`test_anthropic_provider`, live API) and one pre-existing stale-allowlist
`too_many_lines` finding in untouched `agent_drafter` code — both documented,
neither a wave-0 regression, neither blocks the gate.
