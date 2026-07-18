# Agent-loop campaign mid-flight review index

> **What this is.** The hand-off index written for the human reviewer at Gate 1 of the
> agent-loop fix campaign: where the branch stood, which wave report covered which
> proposals, links to the design docs, and the five open decisions put to the user.
> **Status:** Superseded — every count and status below is a snapshot frozen at
> Gate 1 (2026-07-13), roughly halfway through the campaign. The campaign then ran to
> completion and landed on `main`: all 70 items shipped, not the "36 of 67" this file
> reports. [The outcome report](outcome-report.md) supersedes every number here and is
> the file to trust. This one is kept because it records the five decisions as they
> were originally put to the user, and the reviewer's-eye framing of the work at its
> midpoint.
> **Audience:** maintainers.

`BR-NN` identifiers are proposal numbers from the agentic-loop review;
[the improvement proposals register](../agent-loop-review/improvement-proposals.md)
defines BR-1…BR-67, and [the platform parity audit](../../agent-loop/cross-platform/platform-parity-audit.md)
defines the later BR-68/69/70. The campaign's plan of record and dated log live in
[the campaign README](README.md).

> **Warning.** The branch and worktree this file navigates — `agent-loop-integration`
> and `.worktrees/integration` — were deleted after the campaign merged. The shell
> commands below no longer run as written; the work they pointed at is on `main`.

The snapshot as written at the time follows, with the two claims that later changed
flagged inline.

**Branch:** `agent-loop-integration` · **Worktree:** `.worktrees/integration` ·
**Nothing is merged to `main`.** This branch is the whole campaign; `main` is untouched.

---

## Where it stood at Gate 1

| Measure | Value at Gate 1 |
|---|---|
| Proposals implemented & merged | **36 of 67** (43 BR-tagged commits incl. fixes/regen) |
| Proposals in flight (Wave 2) | 13 committed, 5 running |
| Proposals queued (Wave 3) | ~13 |
| Code change vs `main` | **100 files, +14,672 / −850** |
| Last full regression (Gate 1) | **2,024 tests passing, 55 suites — up +238 from the 1,786 baseline, zero new failures** |
| Only failing test | `test_anthropic_provider` — **pre-existing**, fails identically on `main` (live-API test, needs network + key) |

Wave 0 and Wave 1 are merged and regression-gated. Wave 2 (loop detection, hooks/permissions,
server/cancel) is still implementing in its own worktrees and is **not** on this branch yet.

---

## Start here

- **[The campaign README](README.md)** — the plan, the wave table, conventions, and a dated log of every
  gate (including the conflict resolutions and the schema-version collision that had to be fixed by hand).
- **[The agentic-loop review](../agent-loop-review/README.md)** — the executive report for the original
  28-document review, and the index to its internal, external and comparison chapters.
  *This is the "why" behind every change.*
- **[The improvement proposals register](../agent-loop-review/improvement-proposals.md)** — all 67 proposals
  (BR-1…BR-67) with Problem / Proposal / Affected code / Impact / Effort / Risk.

## Evidence per wave

Each report lists per-proposal commits, files, the exact test-result lines, and any regression
found *and fixed* during that wave.

The first six rows are the wave reports that existed when this index was written. The
remaining rows cover Wave 2 and Wave 3, which completed after this snapshot; they are
listed here so the index is usable as an index.

| Report | Covers |
|---|---|
| [wave-0-foundation.md](wave-reports/wave-0-foundation.md) | BR-4, 20, 25, 26, 33, 34, 36, 38, 39, 46 + the `agent.rs` seam refactor |
| [wave-1-compaction.md](wave-reports/wave-1-compaction.md) | BR-10, 11, 12, 13, 14, 15, 17 |
| [wave-1-security.md](wave-reports/wave-1-security.md) | BR-21, 22, 23, 64, 65 |
| [wave-1-checkpoints.md](wave-reports/wave-1-checkpoints.md) | BR-43, 44, 45 |
| [wave-1-processes.md](wave-reports/wave-1-processes.md) | BR-37, 40, 41, 42 |
| [wave-1-context-and-prompts.md](wave-reports/wave-1-context-and-prompts.md) | BR-1, 2, 3, 5, 8, 9, 60 |
| [wave-2-loop-detection.md](wave-reports/wave-2-loop-detection.md) | BR-29, 30, 31, 32, 35, 66, 67 |
| [wave-2-hooks-and-permissions.md](wave-reports/wave-2-hooks-and-permissions.md) | BR-18, 19, 24, 27, 28, 63 |
| [wave-2-server-cancellation.md](wave-reports/wave-2-server-cancellation.md) | BR-6, 7, 52, 61, 62 |
| [wave-3-polish.md](wave-reports/wave-3-polish.md) | BR-40 (async subagent handle), BR-62b (desktop cancel wiring), and the frontend gate-greening commit |
| [parity-verification-report.md](../../agent-loop/cross-platform/parity-verification-report.md) | BR-68, 69, 70 (cross-platform cluster) |

## Design docs — the architectural items

These were designed before coding; each records the options considered and the choices made.
**Only the first mergeable slice of each was implemented** — the rest is deliberately left for your call.

- [BR-43 — shadow-git checkpoints + `/rewind`](../../agent-loop/designs/shadow-git-checkpoints.md)
- [BR-54 — SharedMcpPool (share MCP servers across sessions)](../../agent-loop/designs/shared-mcp-server-pool.md) — *designed, not implemented*
- [BR-21 — auditable command policy engine](../../agent-loop/designs/command-policy-engine.md)
- [BR-17 — cross-session memory](../../agent-loop/designs/cross-session-memory.md)
- [BR-45 — session branching / fork](../../agent-loop/designs/session-branching.md)
- [BR-65 — managed/enterprise policy tier](../../agent-loop/designs/managed-policy-tier.md)
- [BR-64 — OS-level sandbox](../../agent-loop/designs/macos-seatbelt-sandbox.md)

> **Note.** The "*designed, not implemented*" label on BR-54 was true only at this
> snapshot. SharedMcpPool was built later in the Wave-3 perf tail (decision 2 below)
> and now lives at `crates/biorouter/src/agents/mcp_pool.rs`.

## The changes themselves

```bash
# everything, vs main
git diff main...agent-loop-integration

# just the Rust
git diff main...agent-loop-integration -- crates

# commit-by-commit (one commit per proposal, prefixed BR-NN)
git log --oneline main..agent-loop-integration

# review one proposal in isolation
git show <sha>
```

### The headline fixes

| Proposal | What it fixes |
|---|---|
| **BR-46** | Anthropic streaming never set `finish_reason` → length-truncated answers ended **silently mid-sentence** on the default provider |
| **BR-18** *(Wave 2)* | `SmartApprove` was byte-identical to `Approve` — the LLM permission judge and read-only auto-approve were **dead code** |
| **BR-19** *(Wave 2)* | Hooks were a veto only; now a policy engine (input rewrite, PostToolUse block, hook context reaches the model) |
| **BR-43** | **No checkpoints/undo existed.** Shadow-git capture at turn boundaries + three-axis restore |
| **BR-1** | The model got **one line** about the project. Now a gitignore-aware, cached repo map |
| **BR-33** | No single-turn-per-session lock — raced `/reply` corrupted shared state and doubled token spend |
| **BR-10/11/13/14** | Compaction was summarize-everything with the *weak* model, with a 2-attempt "start over" cliff |
| **BR-29/30/31/32/66** *(Wave 2)* | Loop detection was byte-exact duplicates only, and told the model *the user declined* when it tripped |
| **BR-52** *(Wave 2)* | Two SQLite reads **per streamed token**, one growing with history |

---

## Decisions — answered 2026-07-13 ("proceed with all defaults")

The user approved the default path on every open question. Locked in:

> **Note.** This same decision record appears in [the campaign README](README.md)'s
> 2026-07-13 log entry and, in its final form, in
> [the outcome report](outcome-report.md). This copy is the one that preserves the
> original question text as it was put to the user.

| # | Decision | Resolution |
|---|---|---|
| 1 | Default-off flags | **Stay default-off.** Checkpoints (`BIOROUTER_CHECKPOINTS`), the macOS Seatbelt sandbox, and per-reply budgets remain opt-in. Merging changes no runtime behaviour until a user enables them. |
| 2 | BR-54 (SharedMcpPool) | **Build it** — it is already scheduled in the Wave-3 runtime-perf tail (BR-53–59). |
| 3 | Slices vs full features | **Keep the shipped first slices**; carry forward only what the wave plan already schedules. No extra scope. |
| 4 | Landing strategy | **Option (a):** finish Waves 2–3, hand over one verified branch. **Nothing merges to `main` without explicit sign-off.** |
| 5 | Pre-existing frontend reds | **Fix them** — folded into the Wave-3 cleanup cluster. |

### Original question text, for the record

1. **Default-off flags.** Several new capabilities ship gated so they change nothing until enabled —
   checkpoints (`BIOROUTER_CHECKPOINTS`), the macOS Seatbelt sandbox, per-reply budgets. Do you want
   any of them **on by default** before this merges?
2. **The unimplemented designs.** BR-54 (SharedMcpPool — the biggest memory win) is designed but not
   built, because it changes the process model. Build it, or defer?
3. **Slices vs. full features.** BR-43/21/17/45/65/64 landed as *first slices* per their design docs.
   Review the designs and tell me which to carry to completion.
4. **Landing strategy.** Options: (a) let me finish Waves 2–3 and hand you one branch; (b) open a PR
   now for the 36 merged proposals and continue in parallel; (c) cherry-pick a subset (e.g. just the
   correctness bugs BR-46/25/33) to `main` immediately and take the rest slowly.
5. **Pre-existing reds worth a separate fix:** the frontend has ~40 lint errors and 2 failing vitest
   files (`biorouterd.test.ts`, `ExtensionModal.test.tsx`) **on `main`, unrelated to this work**.
   Want me to clean those up too?

## Known caveats at Gate 1

- `test_anthropic_provider` still fails — pre-existing, live-API, fails on `main` too. BR-46 fixed the
  underlying `finish_reason` mapping but the test needs network + credentials to go green.
- **BR-35** (per-reply budget) has code in the Wave-2 tree but had not landed a commit at last check;
  its cluster verifier is instructed to adopt orphaned work. It was to be confirmed committed and
  tested before that cluster merged — it would not be silently dropped.

  > **Note.** This was resolved. The 2026-07-13 log entry in
  > [the campaign README](README.md) records loopdet completing 7/7 with the BR-35
  > orphan adopted by its verifier as designed, and the budget code is on `main` at
  > `crates/biorouter/src/agents/budget.rs`.

- Wave-1 cluster verifiers hit **ENOSPC** during parallel builds (the host disk filled). Some "failures"
  in their logs were disk, not code; each was re-run and passed. Mitigated by pruning build caches.
- The `too_many_lines` clippy baseline was stale repo-wide; it was regenerated (13 entries).

## Related documentation

- [Outcome report](outcome-report.md) — the campaign's closing record; supersedes every count in this file.
- [Campaign README](README.md) — the plan of record, wave table, and dated log this index navigates.
- [Improvement proposals register](../agent-loop-review/improvement-proposals.md) — defines BR-1…BR-67.
- [Agentic-loop review](../agent-loop-review/README.md) — the 28-document review that motivated every proposal.
- [Platform parity audit](../../agent-loop/cross-platform/platform-parity-audit.md) — the cross-platform findings that added BR-68/69/70 after this snapshot was taken.
