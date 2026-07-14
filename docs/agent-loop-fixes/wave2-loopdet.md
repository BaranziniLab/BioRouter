# Wave 2 — "Loop & budgets" cluster (`loopdet`)

Verification report for branch `agent-loop-loopdet`, cut from `agent-loop-integration`.

**Verdict: GREEN.** 7 proposals, 7 commits, no new test failures against GATE-1.

---

## Proposals shipped

| BR | Commit | What it adds | New surface |
|----|--------|--------------|-------------|
| BR-29 | `7b2d8a78` | Staged soft-then-hard repetition stop, and an *honest* repetition reason (the turn no longer claims completion when it was actually stopped for repeating). | `BIOROUTER_REPETITION_SOFT_WARN`, `BIOROUTER_REPETITION_HARD_STOP` |
| BR-30 | `560649e5` | Semantic **near-duplicate** detection (args that differ only cosmetically) plus **A/B/A/B oscillation** detection — the two loop shapes exact-match hashing cannot see. | `BIOROUTER_LOOP_SEMANTIC_DETECTION`, `BIOROUTER_LOOP_ARG_SIMILARITY`, `BIOROUTER_LOOP_NEAR_DUP_{SOFT_WARN,HARD_STOP}`, `BIOROUTER_LOOP_OSCILLATION_{SOFT_WARN,HARD_STOP}` |
| BR-31 | `b21dc522` | Repeated-**failing**-result detector: same tool + same error signature, N times in a row. A tool that keeps failing identically is a loop even when the args change. | `BIOROUTER_FAILURE_LOOP_DETECTION`, `BIOROUTER_FAILURE_ERROR_SIMILARITY`, `BIOROUTER_FAILURE_LOOP_{SOFT_WARN,ESCALATE,HARD_STOP}` |
| BR-32 | `1efb1541` | Periodic **no-progress (stall) check** for long agentic turns — asks, every N iterations after a threshold, whether the turn is still making progress toward the goal. New `agents/stall.rs` (+ `agents/goal.rs` changes). | `BIOROUTER_STALL_CHECK{,_AFTER,_EVERY}`, `BIOROUTER_STALL_LIMIT`, `BIOROUTER_STALL_MAX_FLAGS` |
| BR-35 | `9948c5d5` | Per-reply **wall-clock / token / dollar budget** (`agents/budget.rs`). See "Orphaned work" below. | `BIOROUTER_REPLY_BUDGET_{SECONDS,TOKENS,USD}`, `SessionConfig::budget` |
| BR-66 | `b24ef521` | **Mistake-streak / recoverable-failure** handling (`agents/mistakes.rs`): counts consecutive failed tool calls of *any* kind, nudges at 3, escalates at 6; recoverable provider errors earn one more attempt instead of ending the turn. | `BIOROUTER_MISTAKE_STREAK_{DETECTION,NUDGE,ESCALATE}`, `BIOROUTER_PROVIDER_ERROR_RETRIES` |
| BR-67 | `924a071d` | Runtime **observability** for every loop-safety event (`observability/loop_safety.rs`): one typed event stream for all of the above. | `BIOROUTER_LOOP_SAFETY_TRACE` |

New modules: `agents/budget.rs`, `agents/mistakes.rs`, `agents/stall.rs`,
`observability/loop_safety.rs`, `tests/loop_safety_observability_tests.rs`.

**Every behaviour above is config-gated and off (or set to the pre-existing
behaviour) by default.** A stock BioRouter runs exactly as it did before this
cluster; this is the property that makes the wave safe to land.

---

## Decisions taken during verification

### 1. Orphaned BR-35 work was coherent — committed, not discarded

The worktree arrived with 18 modified files plus an **untracked
`crates/biorouter/src/agents/budget.rs`** (562 lines). This was not junk:

- The already-committed BR-67 (`observability/loop_safety.rs`) *already ships*
  the `BudgetWarn` / `BudgetExceeded` / `BudgetStop` event variants and a
  `maybe_axis()` builder — i.e. the budget work was a planned member of this
  cluster whose implementation simply never got committed.
- The change was complete and internally consistent: the module, its wiring into
  the reply loop, `SessionConfig::budget`, the one-line `budget: None` additions
  at every `SessionConfig` construction site (acp / cli / server / examples), 13
  unit tests in `agents::budget`, and 2 integration tests in `tests/agent.rs`.
- It builds clean and its tests pass.

It was therefore committed as its own proposal commit, `9948c5d5`, preserving the
one-commit-per-proposal rule.

BR-35 also **de-duplicates cost estimation**: the private `estimate_cost_usd` in
the CLI's cost line moved to `providers::pricing::estimate_cost_usd`, so the
budget, the CLI cost line and `/config/pricing` can no longer disagree about what
a turn cost.

### 2. No OpenAPI regeneration needed

`biorouter-server` *was* touched (`routes/apps.rs`, `routes/reply.rs`), which
normally triggers `just generate-openapi`. It is **not** needed here:

- Both diffs are a single line each — adding `budget: None` to an existing
  `SessionConfig` struct literal. No route signature, request or response type
  changed.
- `SessionConfig` has **no `ToSchema` derive** and does not appear anywhere in
  `ui/desktop/openapi.json` (grep count: 0), so the new `budget` field is not
  part of the generated spec.

`ui/desktop` is untouched by the cluster, so the frontend test/lint step is also
out of scope.

### 3. No `too_many_lines` baseline update needed

The baseline was regenerated at the merge base, so any red would be real. There
is none: the 13 violations found are **exactly** the 13 in
`clippy-baselines/too_many_lines.txt`. Notably the reply loop in `agents/agent.rs`
— which this cluster grew by several hundred lines across BR-29/30/31/32/35 —
does **not** trip the lint, because the loop body lives inside an `async_stream`
macro that clippy attributes to the macro rather than the function.

`scripts/clippy-lint.sh` exits 0 and reports `✅ clippy::too_many_lines: ok`.

---

## Regression findings

### Finding A — `biorouter-server`: 2 tunnel tests fail. **External, not ours.**

```
tunnel::lapstone_test::test_tunnel_end_to_end   -> Response status: 503 Service Unavailable
tunnel::lapstone_test::test_tunnel_post_request
```

These tests exercise a **live third-party Cloudflare Worker**
(`cloudflare-tunnel-proxy.michael-neale.workers.dev`). Probed directly during
verification, independently of any build:

```
GET /tunnel/<id>  -> 503     (relay endpoint down)
GET /             -> 200     (worker itself up)
```

The cluster does not touch `crates/biorouter-server/src/tunnel/` at all. The pass
counts line up exactly with GATE-1 (`51 passed + 2 failed = 53`, and
`51 passed + 1 failed = 52`, versus GATE-1's `53 ok` / `52 ok`), so **no
non-tunnel test regressed**. This is an environment failure of the same class as
the known-allowed `test_anthropic_provider`.

### Finding B — `biorouter-acp` first run died on a full disk. **Infrastructure.**

The first `biorouter-acp` run exited 101 without running a single test:

```
rustc-LLVM ERROR: IO failure on output stream: No space left on device
error: could not compile `biorouter-mcp` (lib)
```

The volume was at 100% (146 MiB free). Reclaimed 27 GB by deleting **this
cluster's own** `debug/incremental` directory (`br-targets/loopdet`) — sibling
clusters' target dirs were deliberately left alone, as concurrent verifiers may
still be using them — and re-ran. Green on the re-run.

**No code regressions were found, and no fix commits were needed.**

---

## Per-crate evidence

Run with `CARGO_TARGET_DIR=/Users/wanjun/.cache/br-targets/loopdet cargo test -p <crate> --no-fail-fast`.

| Crate | Result |
|-------|--------|
| `biorouter` | 1003 + 24 + 22 + 12 + 8 + 5 + 4×4 + 3×2 + 2 + 1×4 passed; **1 failed** (`test_anthropic_provider`, known-allowed live API) |
| `biorouter-mcp` | exit 0 — all green |
| `biorouter-server` | 51 + 51 + 31 + 6 + 1 passed; **2 + 1 failed** (tunnel only — external 503, see Finding A) |
| `biorouter-cli` | `test result: ok. 173 passed; 0 failed` |
| `biorouter-acp` | 16 + 11 + 1 passed; 0 failed (after the disk-space re-run) |

Headline lines:

```
biorouter      lib : test result: ok. 1003 passed; 0 failed; 0 ignored
biorouter      providers: test result: FAILED. 14 passed; 1 failed   <- test_anthropic_provider (allowed)
biorouter-cli      : test result: ok. 173 passed; 0 failed; 0 ignored
biorouter-server   : test result: FAILED. 51 passed; 2 failed       <- tunnel 503 (external)
biorouter-server   : test result: ok. 31 passed; 0 failed
biorouter-acp      : test result: ok. 16 passed; 0 failed; 0 ignored
biorouter-acp      : test result: ok. 11 passed; 0 failed; 0 ignored
```

### Against GATE-1 (`~/.cache/br-baseline/gate1-summary.txt`, 2024 tests / 55 suites ok)

- GATE-1's only failure is the same `FAILED. 14 passed; 1 failed`
  (`test_anthropic_provider`). Reproduced here identically.
- `biorouter` lib grew **935 → 1003 passing** (+68) — the cluster's new unit tests
  (budget, mistakes, stall, loop-safety) all passing.
- **Zero new failures.** The two deltas versus GATE-1 (tunnel, acp) are both
  proven environmental: a third-party 503 and a full disk.

## Gate status

| Step | Result |
|------|--------|
| One commit per proposal | ✅ 7/7, tree clean |
| `cargo fmt --all -- --check` | ✅ clean |
| `scripts/clippy-lint.sh` | ✅ exit 0, `too_many_lines: ok`, no baseline change |
| OpenAPI regeneration | ✅ n/a (justified above) |
| Per-crate regression | ✅ zero new failures vs GATE-1 |
| `ui/desktop` | ✅ n/a (untouched) |

**GREEN.**

---

## Must-knows for whoever lands this

1. **`scripts/clippy-lint.sh`'s baseline check is silently fragile.** It prints
   `jq: error (at <stdin>:N): split input and separator must be strings` — its
   `function_name` parser does `split("fn ")[1]`, which yields `null` on a span
   whose text has no `fn ` (here: a `#[tokio::test]` body in
   `biorouter-bench`). jq then aborts, so **violations after the erroring record
   are dropped from the comparison set**, which can only make the check *more*
   permissive — it can mask a genuinely new violation. This run was verified by
   hand against the baseline (13 = 13) rather than trusting the script. Worth
   fixing separately; it is pre-existing and not this cluster's doing.
2. **The tunnel tests are a live-network dependency.** `tunnel::lapstone_test`
   hits a third-party Cloudflare Worker and will fail whenever that relay is down,
   regardless of the diff under test. Treat like `test_anthropic_provider`.
3. **Disk pressure is real on this machine.** The three `br-targets` cluster dirs
   were 77 GB + 63 GB + 49 GB. Budget for it before running another wave.
4. **All 7 proposals are default-off.** Landing this cluster changes no default
   behaviour; every guard is behind a `BIOROUTER_*` key (and BR-66 restores its
   exact pre-existing behaviour at `BIOROUTER_PROVIDER_ERROR_RETRIES=0`).
