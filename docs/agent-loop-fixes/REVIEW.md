# Agent-Loop Campaign — Review Index

**Branch:** `agent-loop-integration` · **Worktree:** `.worktrees/integration` ·
**Nothing is merged to `main`.** This branch is the whole campaign; `main` is untouched.

---

## 1. Where it stands

| | |
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

## 2. Start here

- **[CAMPAIGN.md](CAMPAIGN.md)** — the plan, the wave table, conventions, and a dated log of every
  gate (including the conflict resolutions and the schema-version collision I had to fix by hand).
- **[../agent-loop-review/review.html](../agent-loop-review/review.html)** — the original 28-document
  review, rendered. *This is the "why" behind every change.*
- **[../agent-loop-review/PROPOSALS.md](../agent-loop-review/PROPOSALS.md)** — all 67 proposals
  (BR-1…BR-67) with Problem / Proposal / Affected code / Impact / Effort / Risk.

## 3. Evidence per wave (what changed + what was tested)

| Report | Covers |
|---|---|
| [wave0.md](wave0.md) | BR-4, 20, 25, 26, 33, 34, 36, 38, 39, 46 + the `agent.rs` seam refactor |
| [wave1-compaction.md](wave1-compaction.md) | BR-10, 11, 12, 13, 14, 15, 17 |
| [wave1-security.md](wave1-security.md) | BR-21, 22, 23, 64, 65 |
| [wave1-checkpoints.md](wave1-checkpoints.md) | BR-43, 44, 45 |
| [wave1-processes.md](wave1-processes.md) | BR-37, 40, 41, 42 |
| [wave1-context.md](wave1-context.md) | BR-1, 2, 3, 5, 8, 9, 60 |

Each report lists per-proposal commits, files, the exact test-result lines, and any regression
found *and fixed* during that wave.

## 4. Design docs — the architectural items

These were designed before coding; each records the options considered and the choices made.
**Only the first mergeable slice of each was implemented** — the rest is deliberately left for your call.

- [BR-43 — shadow-git checkpoints + `/rewind`](designs/BR-43-design.md)
- [BR-54 — SharedMcpPool (share MCP servers across sessions)](designs/BR-54-design.md) — *designed, not implemented*
- [BR-21 — auditable command policy engine](designs/BR-21-design.md)
- [BR-17 — cross-session memory](designs/BR-17-design.md)
- [BR-45 — session branching / fork](designs/BR-45-design.md)
- [BR-65 — managed/enterprise policy tier](designs/BR-65-design.md)
- [BR-64 — OS-level sandbox](designs/BR-64-design.md)

## 5. The changes themselves

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

## 6. Decisions — ANSWERED 2026-07-13 ("proceed with all defaults")

The user approved the default path on every open question. Locked in:

| # | Decision | Resolution |
|---|---|---|
| 1 | Default-off flags | **Stay default-off.** Checkpoints (`BIOROUTER_CHECKPOINTS`), the macOS Seatbelt sandbox, and per-reply budgets remain opt-in. Merging changes no runtime behaviour until a user enables them. |
| 2 | BR-54 (SharedMcpPool) | **Build it** — it is already scheduled in the Wave-3 runtime-perf tail (BR-53–59). |
| 3 | Slices vs full features | **Keep the shipped first slices**; carry forward only what the wave plan already schedules. No extra scope. |
| 4 | Landing strategy | **Option (a):** finish Waves 2–3, hand over one verified branch. **Nothing merges to `main` without explicit sign-off.** |
| 5 | Pre-existing frontend reds | **Fix them** — folded into the Wave-3 cleanup cluster. |

### Original question text (for the record)

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

## 7. Known caveats (honest list)

- `test_anthropic_provider` still fails — pre-existing, live-API, fails on `main` too. BR-46 fixed the
  underlying `finish_reason` mapping but the test needs network + credentials to go green.
- **BR-35** (per-reply budget) has code in the Wave-2 tree but had not landed a commit at last check;
  its cluster verifier is instructed to adopt orphaned work. I will confirm it is committed and tested
  before that cluster merges — it will not be silently dropped.
- Wave-1 cluster verifiers hit **ENOSPC** during parallel builds (the host disk filled). Some "failures"
  in their logs were disk, not code; each was re-run and passed. Mitigated by pruning build caches.
- The `too_many_lines` clippy baseline was stale repo-wide; I regenerated it (13 entries).
