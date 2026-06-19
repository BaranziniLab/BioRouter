# BioRouter Improvements Applied During QA

One concrete improvement per 5-app checkpoint, motivated by `ISSUES/` findings.
Implemented on branches in the BioRouter repo (`/Users/wanjun/Desktop/BioRouter`),
then the CLI binary is rebuilt so subsequent app builds use the improved agent.

## Round 1 (after apps 1–5) — fix F3: opaque `-32602 missing field 'path'`

**Finding:** Xiaomi MiMo intermittently emits the `text_editor` parameter as
`file_path` instead of `path`. Because `TextEditorParams.path` was a required
field, serde rejected the call *before* the handler with an opaque
`-32602: failed to deserialize parameters: missing field 'path'`, costing the
agent a recovery turn.

**Change:** add `#[serde(alias = "file_path")]` to `TextEditorParams.path` in
`crates/biorouter-mcp/src/developer/rmcp_developer.rs`, so the tool accepts either
key. Added a unit test
(`test_text_editor_params_accepts_file_path_alias`) covering both the alias and
the canonical key.

**Why it makes the agent better:** removes a whole class of wasted turns / failed
edits for MiMo (and any model that uses the common `file_path` convention) with a
one-line, zero-risk, backward-compatible change.

**Status:** implemented + unit-tested on branch
`improve/text-editor-path-alias`; CLI binary rebuilt for later batches.

## Round 2 (after apps 6–10) — fix G1: rate-limit aborts the run (retry too shallow)

**Finding:** transient MiMo 429s truncated builds (apps 6, 8). Root cause is
code-level: 429 → `RateLimitExceeded` IS retried, but `DEFAULT_MAX_RETRIES = 3`
(1s→2s→4s ≈ 7s) is exhausted by sustained throttling, after which
`agents/agent.rs:1672` surfaces a turn-ending error.

**Change:** `crates/biorouter/src/providers/retry.rs` — add `RATE_LIMIT_MAX_RETRIES
= 8` and an `effective_max_retries(error, config)` helper that gives *only*
`RateLimitExceeded` the deeper budget (max of configured + 8), applied in both
`retry_operation` and `with_retry`. With the 30s-capped backoff this spans ~2 min
instead of ~7s. Generic errors keep the conservative 3. Two unit tests added.

**Why better:** transient throttling no longer kills a run/turn; the agent waits
it out automatically (the exact failure that truncated apps 6 & 8).

**Status:** implemented + unit-tested; branch `improve/ratelimit-retry-budget`;
CLI rebuild pending.

## Round 3 (final batch) — git A+B + all FINAL_REPORT §4 items

Branch `improve/git-and-report-followups` (stacks on rounds 1–2). All authored by
Claude Code; all in shared backend so they reach the **CLI and GUI**.

| Item | Change | Where |
|---|---|---|
| **Git Plan A** | Inject git branch/dirty status + commit policy (commit logical units; .gitignore artifacts; never rewrite history without asking) into the developer extension instructions when cwd is a repo | `rmcp_developer.rs::git_context_block` |
| **Git Plan B + F1 + G2** | `verify-and-checkpoint.sh` Stop hook: blocks finishing until tree is committed (reproducible) and (opt-in) build/tests are green for cargo/cmake/pytest/npm — incl. running `*test*` binaries when CMake forgot `add_test()` (the exact app-3/6/9 failure). Failure-open, block-cap bounded | `scripts/hooks/` + `docs/hooks/` |
| **F4** | `--resume` on a missing/typo'd/`--no-session` name now warns + starts fresh instead of `rc=1` dead-end | `cli.rs::get_or_create_session_id` |
| **C1** | Tool-call paths keep the in-project tail in full (`~/…/project/src/mod/file.rs`) instead of one-letter-per-dir | `output.rs::shorten_path` (+test) |
| **C2** | Action-limit stop now states the cap, clarifies "stopped on budget, not necessarily done", points at `max_turns`, logs N/max progress | `agent.rs` |

Verified: `cargo check` clean across the 3 crates; `shorten_path` 5/5; the Stop
hook tested against real app repos (green+committed → allow; dirty → block; red
Rust/Python/C++ → block, incl. the unregistered-ctest C++ case). CLI rebuilt.

## Still queued (deliberately deferred)
- A first-class, permission-gated `git` tool in the developer extension (Plan C) —
  only if A+B prove insufficient.
- A live turn/budget HUD (C2 quantifies the *stop*, not a running indicator) —
  needs agent→renderer plumbing.
