# Agent-loop fix campaign outcome report

> **What this is.** The campaign's closing record: what landed, the test-count
> progression across all four gates, the highest-value fixes, what the gated merges
> caught, and the honest caveat list.
> **Status:** Historical record — the campaign ran 2026-07-12 to 2026-07-13 and this
> report was written at its close, with all 70 items implemented and Gate 3 GREEN. Its
> framing claims are no longer true: the report says "Nothing is merged to `main`" and
> recommends a PR from `agent-loop-integration` as the next step, but the work has
> since landed on `main` and that branch has been deleted. Read the technical content
> as accurate and the landing guidance as a record of what was recommended at the time.
> **Audience:** maintainers.

`BR-NN` identifiers are proposal numbers from the agentic-loop review.
[The improvement proposals register](../agent-loop-review/improvement-proposals.md)
defines BR-1…BR-67 with Problem / Proposal / Affected code / Impact / Effort / Risk;
BR-68, BR-69 and BR-70 were added mid-campaign and are defined in
[the platform parity audit](cross-platform/platform-parity-audit.md),
along with the `GAP-N` cross-platform gaps. Commit messages carry the `BR-NN:` prefix.

This is the wrap-up of the campaign that implemented every proposal from the
agentic-loop review. [The mid-flight review index](mid-flight-review-index.md) is the
reviewer's index taken at Gate 1; [the campaign README](README.md) holds the plan and
the dated log. This file is the outcome record.

**Branch:** `agent-loop-integration` · **Nothing is merged to `main`.**
**Base:** `main` (current — upstream merged in mid-campaign).

---

## What landed

| Measure | Result |
|---|---|
| Proposals implemented | **all 67 from the review** (BR-16 folded into BR-33 by design) **+ 3 new cross-platform** (BR-68/69/70) **+ GAP-2** (a bug the campaign itself introduced and then fixed) |
| Commits vs `main` | **138** (93 `BR-`/`GAP-` tagged — one per proposal + regression/merge fixes) |
| Code change | **200 files, +46,399 / −2,112** |
| Design docs | 10 (`designs/BR-{17,21,43,45,54,64,65,68,69,70}-design.md`) — architectural items designed before coding; the big ones shipped as a **first mergeable slice** |
| New crate | `biorouter-sandbox` (OS-level tool sandboxing) |

The design docs are split by whether they still describe live guidance. Seven remain in
[`docs/agent-loop/designs/`](../../agent-loop/designs/README.md), including the cross-platform
BR-69 sandboxing design. The rest — the cross-platform records and the superseded
Seatbelt design — were archived alongside this report in
[`cross-platform/`](cross-platform/README.md) when the documentation tree was
reorganized on 2026-07-18.

### Test progression, full workspace, each gate

| Gate | Tests passing | Δ | Notes |
|---|---|---|---|
| Baseline | 1,786 | — | 1 pre-existing failure (`test_anthropic_provider`, live API) |
| Gate 1 | 2,024 | +238 | Wave 1 (5 clusters) |
| Gate 2 | 2,332 | +308 | Wave 2 (3 clusters) |
| **Gate 3** | **2,552** | **+220** | Wave 3 (4 clusters) |
| **Total** | | **+766** | **Zero new failures at any gate.** Only red throughout is the pre-existing live-API test, which fails on `main` too. |

Clippy is clean across the workspace. `cargo fmt` clean.

## The highest-value fixes

These are what the review most wanted.

- **BR-46** — Anthropic streaming never set `finish_reason`; length-truncated answers ended **silently mid-sentence** on the default provider. One-line map.
- **BR-18** — `SmartApprove` was byte-identical to `Approve`; the LLM permission judge + read-only auto-approve were **dead code**. Now live.
- **BR-19** — Hooks went from veto-only to a **policy engine** (PreToolUse input rewrite, PostToolUse block, hook context reaching the model).
- **BR-43** — **Shadow-git checkpoints** + `/rewind`. The recovery net every current-gen agent had and BioRouter did not.
- **BR-1** — A **repo map**. The model previously got one line ("Working directory: …").
- **BR-33** — **Single-turn-per-session server lock** (subsumes BR-16). Raced `/reply` no longer corrupts shared state.
- **BR-29/30/31/32/66** — Loop detection went from byte-exact duplicates only to **staged stops, semantic/oscillation, failure-streak, stall, and mistake-streak** detection — and stopped lying to the model that *the user* declined.
- **BR-52** — Killed two SQLite reads **per streamed token** (one growing with history).
- **BR-54** — **SharedMcpPool**: MCP servers shared across sessions (the biggest memory win), flag-gated.
- **BR-62** — **Reliable cancel** (addressable endpoint, request-scoped confirmations, idempotent `/reply`), now wired into the desktop Stop button (BR-62b).

## What the merges caught

This was the real value of the gated approach. Each cluster was green in isolation, but
**merging them surfaced defects no single cluster could see**:

- **Gate 2 — silent feature loss.** Resolving a hook conflict for BR-19 (correctly) *deleted* loopdet's BR-31/BR-66 wiring. It compiled and all tests passed; the only evidence was two dead-code warnings. Traced and re-inserted. **Two of seven loop fixes would have shipped as dead code.**
- **Gate 2 — cross-cluster fields.** `SessionConfig` gained `budget` (BR-35) + `reasoning_effort` (BR-63) from different clusters; `ChatRequest` gained `turn_id` (BR-62) + `reasoning_effort`. Test literals authored by one cluster didn't know the other's field → found only by compiling **all** workspace targets.
- **Gate 2 — latent test flakiness** exposed by feature-unification: a JSON-key-order assertion (broke once `biorouter-server` enabled serde `preserve_order` workspace-wide) and env-var races in the undo-history tests. Both were fragile *tests*, not product bugs — diagnosed as such rather than "fixed" by touching product code.
- **Gate 3 — a decision violation.** BR-47 shipped **default-enabled**, contradicting the signed-off "flags stay default-off" decision. Flipped to off; tests updated.

## Decisions honored, signed off 2026-07-13

1. **All new capabilities default-off** — checkpoints, sandbox, budgets, post-edit diagnostics (BR-47 corrected), SharedMcpPool, self-critique, done-ness gate. **Merging this branch changes no runtime behaviour until a user opts in.**
2. **BR-54 SharedMcpPool built** (flag-gated).
3. First mergeable slices kept for the architectural items; no scope creep.
4. **One verified branch, nothing merged to `main` without sign-off.**
5. Pre-existing frontend reds fixed.

[The mid-flight review index](mid-flight-review-index.md) records the original text of
the five questions as they were put to the user.

## Cross-platform: Windows, Linux, macOS

The app ships on all three. Findings + specs in
[the platform parity audit](cross-platform/platform-parity-audit.md)
and the three cross-platform design docs:
[BR-68 command safety](cross-platform/command-safety.md),
[BR-69 Linux and Windows sandboxing](../../agent-loop/designs/linux-and-windows-sandboxing.md),
[BR-70 CI gate](cross-platform/ci-gate.md).

- **Source audit: zero compile BREAKs.** Every `#[cfg]` has a complementary arm;
  `libc` is `[target.'cfg(unix)'.dependencies]`; `git2`/sqlite/FTS5 are portable.
- **GAP-2 fixed** — the Windows orphan reaper had no PID-reuse guard
  (`is_group_leader() → true`), so a stale pidfile + recycled PID could
  `taskkill /F /T` an innocent process tree. A bug this campaign introduced; now guarded.
- **BR-68** — Windows/Linux destructive-command coverage + the POSIX-tokenizer
  fix that was blocking it (both had to ship together). Alias/abbreviation-aware.
- **BR-69** — sandbox generalized behind one trait; Linux Landlock tier; honest
  Windows story. macOS Seatbelt behaviour unchanged.
- **BR-70** — `just check-cross` gate + CI matrix, sharing `scripts/cross-env.sh`
  with the release (a drift-guard forbids a second recipe copy).
- **Local Docker cross-*compile* verification is environmentally unreliable on
  this host** (repeated ENOSPC; a wedged daemon). The first attempt compiled
  **both** linux and windows targets with zero errors before disk filled. The
  durable answer is **BR-70's CI gate**, which runs the same pinned toolchain on
  every PR in a reliable environment.

> **Note.** The original report ended that last bullet with the editorial placeholder
> "*(This section is updated if the local re-check completes.)*". No updated local
> re-check result was ever recorded in this file.

## Known follow-ups and honest caveats

- **`test_anthropic_provider`** still red — pre-existing, live Anthropic API, fails on `main`. BR-46 fixed the underlying `finish_reason` mapping; the test needs network + a key to go green.
- **First slices, not full features:** BR-43 (checkpoints), BR-21 (policy engine), BR-17 (memory), BR-45 (branching), BR-54 (pool), BR-64/69 (sandbox — Windows tier is the weakest). Each design doc lists the remaining phases.
- **BR-69 Linux/Windows sandbox arms are not compile-tested on this macOS host** (no cross toolchain locally); the guard logic is unit-tested. CI covers the rest.
- **Cancellation is cooperative** — a tool body that ignores the cancel token can't be force-aborted (loop-detection gap #7, out of scope).
- The clippy `too_many_lines` baseline needed a refresh mid-campaign; one bench-only file remains un-baselined (pre-existing, untouched crate).

## How to review and hand off

> **Warning.** This section is a record of the hand-off as it was written, and no
> longer runs. The `agent-loop-integration` branch and the `.worktrees/integration`
> worktree were both deleted once the campaign merged; the commits it names are on
> `main`. For the commit-by-commit view the first two lines produced, see
> [the campaign commit log](commit-log.md).

```bash
cd .worktrees/integration
git log --oneline main..HEAD           # every proposal, one commit each
git diff main...HEAD -- crates ui      # the whole change
git show <sha>                         # one proposal in isolation
open docs/agent-loop-fixes/changes.html  # the rendered dashboard
```

**Security-sensitive code that warrants human review** (per `HOWTOAI.md`), regardless of the green suite: BR-18/19/24/65 (permission + hook gating), BR-20/21/68 (command denylist/policy), BR-33/62 (concurrency + cancel), BR-64/69 (sandbox), BR-7 (schema migration).

**Landing:** the branch is current with `main` and green. Recommended next step is a PR from
`agent-loop-integration` for human review — **not** an auto-merge.

## Related documentation

- [Campaign README](README.md) — the plan of record, wave table, conventions, and the dated log behind every gate in this report.
- [Mid-flight review index](mid-flight-review-index.md) — the Gate-1 snapshot, with the five decisions in their original question form.
- [Platform parity audit](cross-platform/platform-parity-audit.md) — the findings behind the cross-platform section, including GAP-1/2/3.
- [Improvement proposals register](../agent-loop-review/improvement-proposals.md) — defines every BR-NN cited above.
- [Wave reports](wave-reports/README.md) — the per-cluster verification evidence behind the gate test counts.
