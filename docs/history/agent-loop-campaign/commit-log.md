# Agent-loop campaign commit log

> **What this is.** One line per commit on the `agent-loop-integration` branch for the
> 67-proposal agent-loop improvement campaign, mapping each commit to the proposal
> (`BR-NN`) it implements. The catalogue was generated at Gate 1: its last entry is the
> commit that launched Wave 2, so it covers Waves 0 and 1 only, not the whole campaign.
> **Status:** Historical record — the campaign concluded and its work merged to `main`;
> this catalogue is the branch as it stood when it was generated, part-way through.
> [The outcome report](outcome-report.md) carries the campaign's final commit total.
> **Audience:** maintainers auditing what shipped in the campaign.

`BR-NN` identifiers are the proposal numbers from [the improvement proposals list](../agent-loop-review/improvement-proposals.md). Commits with no `BR-NN` are campaign bookkeeping — plans, wave reports, and baseline regenerations rather than proposal implementations.

Review any single proposal in isolation with `git show <sha>`, or the whole branch with
`git diff main...agent-loop-integration`.

> **Warning.** The `agent-loop-integration` branch was deleted after the campaign
> merged, so the second command no longer resolves as written. The commits it names are
> on `main`, where `git show <sha>` still works.

69 commits, 43 of them carrying a proposal id.

| Commit | Proposal | Subject |
|---|---|---|
| `6f17f669` | — | docs: comprehensive agentic-loop review + 67-proposal improvement program |
| `68b9ae4c` | — | docs: single-file HTML report for the agentic-loop review |
| `a409e7d7` | — | merge: agent-loop review corpus into integration (spec for the fix campaign) |
| `698bd7e9` | — | docs: agent-loop fix campaign plan (waves, gates, conventions) |
| `55385068` | — | docs(campaign): record baseline — 53 suites ok, 1 known live-API failure (test_anthropic_provider) |
| `0393b122` | BR-38 | reconcile stale currently_running flags on scheduler load |
| `53088bc8` | BR-25 | fail-closed on malformed tool_call in permission store |
| `be6f087c` | BR-46 | map Anthropic stop_reason to finish_reason in streaming path |
| `0d07221b` | BR-39 | add shell_list tool for background jobs |
| `f9f15b59` | BR-20 | always-on non-bypassable catastrophic-command denylist |
| `58535da2` | BR-36 | consolidate RepetitionInspector to single production path |
| `703717dc` | — | docs(designs): BR-17/21/43/45/54/65 architectural design docs (pre-implementation) |
| `fa5a0d0c` | BR-34 | per-reply tool-call ceiling with assistant-visible stop |
| `52867b37` | BR-26 | cap + untrusted-frame injected hook stdout |
| `c9faa523` | BR-4 | Base planning/batching/verification disciplines in system.md |
| `53160a6e` | BR-33 | server-enforced single-turn-per-session lock |
| `e03c7516` | — | style: cargo fmt drift in untouched files (fmt --all fallout, no behavior change) |
| `2de2d500` | — | refactor(agent): extract seam methods in agent.rs (no behavior change) |
| `f89ec104` | — | fix(clippy): resolve wave-0 clippy warnings |
| `9b468d0f` | — | docs: wave0 report + architectural design docs |
| `129589ba` | — | merge: Wave 0 foundation — BR-4/20/25/26/33/34/36/38/39/46 + agent.rs seams + 6 design docs |
| `db202388` | — | docs(campaign): Wave 0 merged (gate GREEN); Wave 1 launched across 5 cluster worktrees |
| `2e6c7a9d` | BR-5 | dedup MOIM and refresh the system-prompt clock |
| `7bb223ad` | BR-15 | include system/tools in cold-path token estimate + per-provider calibration |
| `ba9b8596` | BR-22 | scan tool output on the main loop for injection + PII |
| `22518f70` | BR-37 | reap orphaned background shell jobs across restarts |
| `ae74f29b` | BR-10 | keep recent-turn verbatim window at compaction |
| `12f02dcc` | BR-2 | total context budget with ranking/truncation for injected blocks |
| `1e740bc4` | BR-9 | frame project hints/AGENTS.md as lower-trust untrusted context |
| `fc4e5ae6` | BR-23 | central secret-redaction boundary across all extensions |
| `38fe53f3` | BR-11 | head/tail-truncate an over-window message instead of dead-ending compaction |
| `5168cf5e` | BR-40 | structured subagent result envelope (status/tokens/artifacts) |
| `8d946378` | BR-8 | cap and cache eager skill-body inlining |
| `31bbbe6d` | BR-43 | shadow-git checkpoints + three-axis restore (Slice 1) |
| `9c1503ab` | BR-13 | progressive context-overflow fallback instead of the 2-attempt cliff |
| `0717bb5b` | BR-3 | per-model system-prompt variants (strong default + small-local overlay) |
| `46a67474` | BR-41 | persist/restore session goals + surface interrupted elicitations across daemon restart |
| `b097ee68` | BR-14 | validate + retry compaction summary, summarize with the session model |
| `afa11aa8` | BR-21 | auditable command policy engine (Slice 1) atop the BR-20 floor |
| `bfaea95e` | BR-60 | structured per-item todo list + living plan artifact |
| `59228406` | BR-44 | persist and extend text_editor undo history |
| `29943732` | BR-42 | unified active-work registry + /active_work route (jobs, subagents, schedules) |
| `3862995a` | BR-65 | managed/enterprise policy tier (first mergeable slice) |
| `1459b100` | BR-64 | design doc for OS-level tool-execution sandbox |
| `ed573eac` | BR-12 | eager background compaction between turns with synchronous fallback |
| `e4eaa7bd` | BR-45 | stable per-message ids + branch fork point (Phase 1 + diverge route) |
| `b1407965` | BR-64 | macOS Seatbelt sandbox for the developer shell tool (Slice 1) |
| `9bd7e1a9` | BR-42 | regenerate OpenAPI spec + TS client for /active_work route |
| `9066b19d` | BR-17 | FTS5 relevance-ranked chat recall (memory Phase 1) |
| `76dbe752` | BR-45 | regenerate OpenAPI spec + TS client for diverge fork-point fields |
| `1c866383` | BR-11 | fix clippy string_slice lint in truncate_middle_out test |
| `6b3303a9` | — | chore: register compaction cluster long fns in too_many_lines baseline |
| `68cdcb93` | BR-17 | fix regression - guard FTS write path when messages_fts table absent |
| `a65dc489` | — | docs: wave1-security report |
| `f7e080c6` | — | docs: wave1-checkpoints report |
| `fda2d078` | — | docs: wave1-processes report |
| `b37fe886` | — | docs: wave1-compaction report |
| `86d2acd7` | BR-1 | gitignore-aware cached workspace file map in MOIM |
| `6e101107` | BR-60 | fix regression - update prompt_manager snapshots for new todo/plan wording |
| `85c713bf` | — | docs: wave1-context report |
| `950dde2c` | BR-60 | remove stray insta .snap.new pending files |
| `c7974c28` | — | Merge branch 'agent-loop-checkpoints' into agent-loop-integration |
| `ee43bc0f` | — | merge: compaction cluster (BR-10..17) — resolved vs checkpoints |
| `ea6799aa` | — | merge: security cluster (BR-20..23,64,65) — resolved vs checkpoints+compaction |
| `c38cf9ba` | — | Merge branch 'agent-loop-processes' into agent-loop-integration |
| `76855c18` | — | merge: context cluster (BR-1,2,3,5,8,9,60) — union of agent struct fields |
| `d240d7a0` | — | chore: regenerate clippy too_many_lines baseline post-Wave-1 (13 entries; was stale repo-wide per all 5 cluster verifiers) |
| `70ce551e` | — | docs(campaign): Gate 1 — five cluster merges, conflict resolutions, schema renumber |
| `be342632` | — | docs(campaign): Gate 1 GREEN (2024 tests, +238, zero regressions); Wave 2 launched |

## Related documentation

- [Campaign plan and gate log](README.md) — the waves, conventions, and dated gates this branch was built under
- [Campaign outcome report](outcome-report.md) — what the campaign concluded
- [Mid-flight review index](mid-flight-review-index.md) — status, decisions, and caveats recorded part-way through
- [Wave reports](wave-reports/README.md) — per-wave verification evidence
