# Wave 1 — compaction and memory cluster verification report

> **What this is.** Gate evidence for the Wave 1 compaction cluster — BR-10 through BR-15 plus
> BR-17 FTS5 chat recall — including the FTS write-path hardening the verifier added and the
> cross-cluster SQLite schema-version collision it diagnosed.
> **Status:** Historical record — this cluster cleared the gate and merged into the campaign's
> `agent-loop-integration` branch at Wave 1. Its one live instruction, "renumber BR-17's FTS
> migration past 11", **was carried out at Gate 1**: the migration was renumbered 11→13 with
> `CURRENT_SCHEMA_VERSION=13`, as [the campaign overview](../README.md) records. Nothing here
> is still outstanding. The verification run itself is undated in the original record.
> **Audience:** maintainers auditing what the campaign shipped and on what evidence.

The agent-loop fix campaign implemented 67 numbered proposals (`BR-1` … `BR-67`) from
[the master improvement-proposals list](../../agent-loop-review/improvement-proposals.md).
Related proposals were grouped into **clusters**, each built in its own git worktree, and
clusters shipped in dependency-ordered **waves**. Every wave had to clear a **gate**: a full
per-crate test run admitting zero new failures against a recorded baseline. This file is the
compaction cluster's gate evidence. Campaign conventions and the wave table are in
[the campaign overview](../README.md).

> **Note.** Paths beginning `~/` or `.worktrees/` are on the verifier's machine as it was
> configured during the campaign; they are not repository paths.

Worktree `.worktrees/compaction`, branch `agent-loop-compaction` (base
`agent-loop-integration`), target dir `~/.cache/br-targets/compaction`.

## Verdict

**GATE GREEN.** Every proposal is its own commit, `cargo fmt` is clean, the one genuine clippy
error introduced by the cluster is fixed, and all five crates pass with **zero new test
failures** versus the baseline. Two agent-loop tests that regressed were fixed inside the
cluster (see the BR-17 regression below) and now pass against the real, polluted session
database.

## Integration action, since completed

This was the single most important output of this gate, and it has been carried out:

> **Note.** BR-17's migration number (11) will collide with whatever other cluster claimed
> 11/12. On merge, renumber BR-17's FTS migration to the next free version (≥13) and confirm
> `CURRENT_SCHEMA_VERSION` is bumped past all merged migrations, so a real database actually
> applies the FTS migration. The write-path guard means the app degrades gracefully (recall
> falls back to `LIKE`) even if that renumber is missed, rather than failing every save.

The campaign carried this out at Gate 1: the FTS migration was renumbered 11→13 and
`CURRENT_SCHEMA_VERSION` set to 13. See [the campaign overview](../README.md). The diagnosis
that produced this instruction is in the regression findings below.

## Proposals shipped

| BR | Title | Status | Commit | Key files | Tests |
|----|-------|--------|--------|-----------|-------|
| BR-10 | Keep recent-turn verbatim window at compaction | verified | `ae74f29b` | `context_mgmt/mod.rs`, `agents/agent.rs` | biorouter lib (green) |
| BR-11 | Head/tail-truncate an over-window message instead of dead-ending | verified | `38fe53f3` (+clippy fix `1c866383`) | `context_mgmt/mod.rs` | `test_truncate_middle_out_keeps_head_and_tail` |
| BR-12 | Eager background compaction between turns w/ synchronous fallback | verified | `ed573eac` | `agents/agent.rs`, `context_mgmt/mod.rs` | biorouter lib (green) |
| BR-13 | Progressive context-overflow fallback (vs 2-attempt cliff) | verified | `9c1503ab` | `context_mgmt/mod.rs`, `agents/agent.rs` | biorouter lib (green) |
| BR-14 | Validate + retry compaction summary; summarize with session model | verified | `b097ee68` | `context_mgmt/mod.rs` (`do_compact`) | biorouter lib (green) |
| BR-15 | Include system/tools in cold-path token estimate + per-provider calibration | verified | `7bb223ad` | `token_counter.rs`, `context_mgmt/mod.rs` | biorouter lib (green) |
| BR-17 | FTS5 relevance-ranked chat recall (memory Phase 1) | verified + hardened | `9066b19d` (+regression fix `68cdcb93`) | `session/chat_fts.rs`, `session/chat_history_search.rs`, `session/session_manager.rs` | agent + lib (green) |

BR-17 was designed before implementation; see
[the cross-session memory design](../../../agent-loop/designs/cross-session-memory.md).

Verifier-added commits:

- `1c866383` **BR-11: fix clippy string_slice** — the new
  `test_truncate_middle_out_keeps_head_and_tail` sliced `&text[..100]`, which trips
  `clippy::string_slice` under `-D warnings` (compile-blocking). Rewritten to char-based
  head/tail extraction.
- `6b3303a9` **chore: register long fns in the too_many_lines baseline** — see the
  `too_many_lines` decision below.
- `68cdcb93` **BR-17: fix regression - guard FTS write path** — see the regression findings.

## Design decisions taken during verification

### FTS write path must tolerate a missing `messages_fts` table (BR-17)

BR-17's read path already guards on table existence
(`chat_history_search.rs::fts_available`, and the code's own comment at
`session_manager.rs:3790` promises "A DB lacking messages_fts … must still return"). The
**write** path did not: `index_message_fts` (INSERT) and `replace_conversation_inner` (DELETE)
hit `messages_fts` unconditionally, so any message save on a database that reached its schema
version without the FTS table hard-fails.

Decision: extend the read path's tolerance to writes. Added
`SessionManager::messages_fts_exists(executor)`, resolved **once per operation** — not per
message, to avoid N catalog probes on a bulk rewrite — and threaded into `index_message_fts`
as an explicit `fts_available: bool`; the DELETE is likewise skipped when the table is absent.
This is consistent with the reader and the stated intent; message writes now succeed
regardless of FTS presence.

### `too_many_lines` on cluster functions: baseline registration, not refactor

`context_mgmt/mod.rs` grew 596→1955 lines; `compact_messages_with_window` (112), `do_compact`
(112) and, via BR-17, `session_manager.rs::create_schema` (101) now exceed the 100-line clippy
soft cap. These are 1–12 lines over a cosmetic lint. Refactoring working compaction and schema
logic mid-verification carries more correctness risk than the lint is worth, so the three
functions were added to the repo's sanctioned allowlist
(`clippy-baselines/too_many_lines.txt`) — the exact mechanism the repo uses to acknowledge
long functions.

> **Note.** The `too_many_lines` baseline is stale repo-wide. `clippy-lint.sh` still reports
> pre-existing long functions in **untouched** crates that were never added to the baseline:
> `biorouter-cli/src/cli.rs:1399`, `biorouter-cli/src/commands/doctor.rs:17`,
> `biorouter-cli/src/session/tui/mod.rs:502`,
> `biorouter-mcp/src/agent_drafter/render.rs:299` (`serve_mjs`) and `…/control.rs:280`
> (`validate_widget`). The last two are the known stale-allowlist reds the task flags as
> pre-existing; the CLI, doctor and TUI entries are the same class. The clippy baseline gate is
> therefore inherently red on this repo state independent of the compaction cluster, and should
> be regenerated with `./scripts/clippy-baseline.sh generate clippy::too_many_lines` at
> integration time.

Four sibling wave reports record the same stale allowlist independently — see
[Wave 0 — foundation](wave-0-foundation.md),
[Wave 1 — checkpoints](wave-1-checkpoints.md),
[Wave 1 — processes](wave-1-processes.md) and
[Wave 1 — context and prompts](wave-1-context-and-prompts.md).

## Regression findings

### Fixed: two agent-loop tests failed on the shared session database

`biorouter --test agent` initially failed `tests::max_turns_tests::test_max_turns_limit` and
`tests::max_tool_calls_tests::test_max_tool_calls_limit`, both with
`error returned from database: (code: 1) no such table: messages_fts`.

Root cause, fully diagnosed: these tests use `Agent::new()` → `SessionManager::instance()`,
which points at the **real shared** user database
(`~/.local/share/biorouter/sessions/sessions.db`, 160 MB). That database is at
**schema_version 12** and has **no `messages_fts` table** — another cluster in the campaign
advanced the shared database to v12 with different migrations. BR-17's migration uses schema
version **11** for the FTS table, so `run_migrations` sees
`current(12) ≥ CURRENT_SCHEMA_VERSION(11)` and never creates the table; the live FTS INSERT
then fails on every message save.

- BR-17's code is correct in isolation: with `BIOROUTER_PATH_ROOT` pointed at a clean
  directory, `test_max_turns_limit` passes — a fresh `create_schema` builds `messages_fts`, and
  a v10→v11 migration also builds it.
- The failure is a **cross-cluster schema-version collision** surfacing through a shared
  on-disk database — an integration-time hazard, not a compaction logic bug.
- The write-path hardening (commit `68cdcb93`, above) fixes the observed failure without wiping
  the user's real database: after it, `--test agent` reports **10 passed / 0 failed** against
  the real v12 database.

This diagnosis is what produced the integration action recorded near the top of this file.

### Known pre-existing failure, not a regression

`biorouter tests/providers.rs::test_anthropic_provider` fails against the live Anthropic API.
It is present in the baseline log (`~/.cache/br-baseline/workspace-test.log`). Not gated.

## Per-crate test evidence

| Crate | Result | Notes |
|-------|--------|-------|
| biorouter | **894 passed / 1 failed** | 810 lib + agent 10/10 (regression fixed) + integration bins; the 1 failure is the known live-API `test_anthropic_provider` |
| biorouter-mcp | **596 passed / 0 failed** | exit 0 |
| biorouter-server | **137 passed / 0 failed** | exit 0 |
| biorouter-cli | **173 passed / 0 failed** (lib) | exit 0 |
| biorouter-acp | **28 passed / 0 failed** | lib 16 + server_test 11 + ws_transport 1 |

Steps skipped, correctly: `generate-openapi` — no `biorouter-server` routes touched; and the
`ui/desktop` step — the frontend is untouched. The cluster diff is confined to
`crates/biorouter/**` and `docs/**`.

## Environment: shared build volume exhaustion

The shared build volume (`/System/Volumes/Data`) hit **100% full** mid-run because several
verifier clusters share it: `br-targets/processes` 45 G, `checkpoints` 28 G, `security` 27 G,
plus this cluster's 16 G. `biorouter-cli` and `biorouter-acp` first failed to *compile* with
`No space left on device` (os error 28) — not a test failure. Freeing the compaction target's
`debug/incremental` (~7 G) unblocked the compile and both crates then passed clean.

> **Note.** Watch shared-disk headroom when running clusters in parallel.

Four sibling reports record the same campaign-wide disk pressure from their own runs, with
differing mitigations; see [Wave 1 — processes](wave-1-processes.md),
[Wave 1 — checkpoints](wave-1-checkpoints.md), [Wave 1 — security](wave-1-security.md) and
[Wave 2 — loop detection](wave-2-loop-detection.md).

## Related documentation

- [Agent-loop fix campaign overview](../README.md) — the wave table and the record that BR-17's
  migration was renumbered 11→13 at Gate 1.
- [Master improvement proposals](../../agent-loop-review/improvement-proposals.md) — the
  definition of BR-10 through BR-17.
- [Cross-session memory design](../../../agent-loop/designs/cross-session-memory.md) — the
  architecture BR-17's FTS5 chat recall implements.
- [Wave 1 — checkpoints and VCS cluster](wave-1-checkpoints.md) — the sibling cluster whose
  `create_schema` changes met this one at integration.
- [Campaign outcome report](../outcome-report.md) — the end-of-campaign totals across all gates.
