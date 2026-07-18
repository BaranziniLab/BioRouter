# BioRouter merge execution plan and record

> **What this is.** The execution record of the July 2026 branch and pull-request merge campaign for `BaranziniLab/biorouter` — the decisions taken, the conflicts resolved, the qualification evidence gathered, and the commit-level inventory of everything that landed.
> **Status:** Historical record. The campaign completed on 2026-07-13, with execution evidence captured through 2026-07-14T15:51:26-07:00. All nine examined pull requests are merged; nothing here is an open plan.
> **Audience:** Maintainers and release owners auditing what merged and why, and agents reconstructing the state of the repository at the end of the campaign.

Nine pull requests were examined and all nine merged. Three carried live feature work — #12 (Apps SDK v2), #13 (usage reporting), and #11 (the agent-loop overhaul) — and were integrated in that order so the high-overlap branch landed last, onto a baseline that already contained the other two. Six older PRs were already on `main` at the audit snapshot and are recorded here as the historical baseline. Decisions are identified as `D1`–`D10` and indexed in the decision table in the action brief; agent-loop proposals carry `BR-<n>` identifiers from the 70-item campaign that produced PR #11. Section numbers below are cited by the document's own cross-references and are retained.

## Report metadata

| Field | Value |
|---|---|
| Repository | BaranziniLab/biorouter |
| Execution evidence through | 2026-07-14T15:51:26-07:00 (America/Los_Angeles) |
| Historical audit baseline | main@758856e02260 |
| Final PR #11 source | 822c7c3daf49 |
| Feature-merge baseline | 0e576948a843 · all 9 PRs merged · same-head CI passed |
| GitHub branches | main only · merged-branch auto-delete enabled |
| UI authority | `design.md` v1.87.2 (signed off; the repo-root UI specification of the time, no longer present in the tree) |
| PRs examined | 9 |
| Initial audited commits | 216 |
| Final portfolio commits | 240 under the selected counting convention |
| D9 | owner-reviewed and approved · zero formal GitHub review events |

The technical integration choices are settled and all three feature pull requests are merged. PR #11's exact final source head passed the full same-head GitHub matrix and landed as a merge commit; root `main` now equals `origin/main`, and only the root checkout remains registered. GitHub now exposes only the `main` branch, and automatic deletion of merged pull-request branches is enabled. The first section isolates the four remaining post-merge decisions, gives the recommendation, and explains why. D9 owner review and approval is separately recorded as complete. The rest is the completed execution record, with the original audit and merge forecasts retained only where they remain useful as historical evidence.

## Your action brief

No architecture, conflict-resolution, CI-gate, merge-method, or D9 owner-approval choice remains open. The exact source head `822c7c3daf49499d7c39d92d303d24e80d042b07` passed its required workflows and merged without rewriting history. The four choices below govern explicitly owned post-merge work.

> **Leading remaining recommendation.** Assign and prioritize the process-tree hardening work before the next security-sensitive release. The shipped Windows fix now awaits bounded `taskkill /T` and confirms terminal leader state, but it does not prove that every descendant is gone on every platform; Windows Job Objects, stronger POSIX process-group guarantees, and the over-12-second identity-classification edge still need an accountable owner and milestone. Then assign owners to D6 coverage, tag provenance, and issue #14. Do not reopen the applied RepetitionInspector/TurnToolGuard authority, migration numbering, nullable pricing, or `design.md` resolutions without new evidence.

> **D9 complete — owner reviewed and approved this merge.** Merged with the owner's direct authorization. The owner explicitly declared that instructing the coding agent to merge constituted their review, acceptance, and approval of these changes for this merge. This is owner approval, not a formal GitHub review submission; GitHub still records zero formal submitted review events. Durable evidence is the [owner-authored PR comment](https://github.com/BaranziniLab/biorouter/pull/11#issuecomment-4974237126) and Git note `refs/notes/owner-authorization` on merge `0e576948a84309f31a16fa26c0629c45496e3bfc`. Local repository configuration `notes.displayRef` points to that note ref.

### Four decisions still needing your disposition

None requires reopening the completed feature integration or its owner approval. Each remaining item should receive a named owner and a milestone.

1. **Process-tree hardening (medium) — when should timeout handling kill an entire spawned process tree?**
   **Recommendation:** open and prioritize a bounded hardening item for Windows job objects and POSIX process groups before the next security-sensitive release, while retaining the new bounded, awaited `taskkill /T` path and supervisor-confirmed leader termination.
   **Why:** the final fix prevents a successful kill result before the supervisor sees a terminal leader state, but it does not prove that every descendant is gone. Identity lookup that outlives the 12-second confirmation window also remains an error-classification edge case.
2. **D6 coverage — who owns full app reauthoring and browser coverage, and by when?**
   **Recommendation:** assign a named owner and the next product milestone for the full 30-app reauthoring and browser-level scenario matrix. Keep the passing app-smoke contract as the current merge gate.
   **Why:** the typed runtime is qualified, while presentation-level breadth is valuable follow-up work that should not disappear into an unowned promise.
3. **Release provenance — which divergent tag lineage is authoritative, and when may audit refs expire?**
   **Recommendation:** preserve both lineages, all feature branches, and the 61 audit refs through an observation window; decide tag authority only after comparing published artifacts and release records.
   **Why:** 11 tag names resolve to different local and remote objects, and feature integration does not establish which object was actually released.
4. **Issue #14 — should the stale coordination issue remain open?**
   **Recommendation:** replace its obsolete 104-commit/placeholder-branch description with the final PR #11 head, CI links, merge outcome, and this report; then close it after the coordination owner confirms the final record.
   **Why:** leaving stale counts open creates a second, conflicting source of truth; deleting it would remove useful campaign history.

### Execution status at a glance

| Measure | Value | Detail |
|---|---|---|
| Ordered merge execution | #12 ✓ · #13 ✓ · #11 ✓ | All three landed with merge commits in the approved order. |
| Final #11 source | 822c7c3daf49 | 150 commits · 317 files · +74,383/−5,456. |
| Stop-decision authority | One authority | RepetitionInspector decides; TurnToolGuard structurally enforces. |
| Final same-head CI | Passed | Rust 29351008946 · Apps 29351008266 · exact source head |

> **SDK v2 worktree inventory confirmation.** `/Users/wanjun/Desktop/biorouter-sdk-v2-wt` was explicitly audited as the clean `feat/apps-sdk-v2` checkout. Its final source head was `b719aa9f024e`, represented by PR #12 and merged as `5cec0ae3`; it was never a fourth merge target. Its `design.md` was byte-identical to the signed-off root `design.md`. The clean auxiliary checkout was removed during final consolidation; its branch, tags, and audit refs were retained.

### Decision register

| ID / execution status | Decision | Approved recommendation | What happened and why it worked | Remaining guardrail |
|---|---|---|---|---|
| D1 — Applied · Complete | **Merge order** — #12 → #13 → #11. | **Use the low-overlap PRs as the baseline and integrate #11 last.** | #12 merged at `5cec0ae3`, then #13 at `9c3f70c6`. Their three-file overlap merged cleanly; #11 was then integrated onto that combined baseline, localizing the high-overlap work to one branch. | PR #11 passed same-head CI and merged as `0e576948a843`; preserve the qualified source and merge anchors. |
| D2 — Applied · Architecture | **Loop-safety authority** — One policy, two layers. | **RepetitionInspector decides; TurnToolGuard enforces.** | The combined implementation kept #11's staged, failure-aware evidence model and #12's structural masking/typed abort. This avoids duplicate counters and user notices while preserving a hard enforcement boundary. | Keep exact-signature blocking distinct from whole-tool disablement and reset evidence at genuine user turns. |
| D3 — Applied · Data | **Migration ownership** — #13 v11–v12; #11 v13–v16. | **Keep one monotonic schema sequence and a union fresh schema.** | #13 retained model/provider and cache-token migrations at v11/v12; #11's checkpoint, message-ID, FTS, and blob work moved to v13–v16. The collision was resolved in source instead of skipped conditionally. | Retain fresh, v10, and v12 upgrade coverage as the schema evolves. |
| D4 — Applied · History | **Commit strategy** — Merge commits, not squash/rebase. | **Preserve proposal, phase, repair, and merge boundaries.** | #12 and #13 landed with merge commits and their source heads remain anchored. #11's 150-commit source line preserves bisectable feature and remediation boundaries. | #11 also landed with a merge commit; the qualified head was not rewritten. |
| D5 — Satisfied · Same-head pass | **#11 release gate** — No merge on red or incomplete CI. | **Require all configured platform jobs from the same source head.** | The earlier Linux, Windows GNU, and Windows native failures produced concrete portability fixes instead of being dismissed as environmental. Rust run [29351008946](https://github.com/BaranziniLab/biorouter/actions/runs/29351008946) and Apps run [29351008266](https://github.com/BaranziniLab/biorouter/actions/runs/29351008266) then passed from the exact final head. | Guards, both cross-checks, Ubuntu, macOS, and Windows passed; the schedule-only nightly job was correctly skipped. |
| D6 — Merge threshold met · Follow-up decision | **#12 completeness** — Mandatory smoke now; full breadth later. | **Do not block on all 30 reauthorings; do run real app smoke.** | The contract, corpus, Chromium app smoke, self-test application flow, and real Electron/Avatar Lab interaction all passed. That validates runtime integration without pretending every app scenario is already browser-authored. | Name the owner and milestone for full reauthoring/browser coverage. |
| D7 — Applied · Verified in UI | **Unknown pricing** — Nullable, never fabricated zero. | **Retain token counts and display unavailable/incomplete cost evidence.** | The implementation preserves nullable prices and lower-bound totals. The Electron usage panel displayed "—", "≥", and incomplete/unpriced explanations rather than false `$0.00` claims. | Keep end-to-end null-cost tests whenever price catalogs change. |
| D8 — Applied · Locally verified | **UI cleanup timing** — Resolve new drift in the integration. | **Use shared controls, semantic colors, both themes, and 40px rows.** | Known raw-control, literal-color, and density findings were resolved on the combined branch. Typecheck, format, lint—including 128 contrast checks—and 821 Vitest tests passed. | Continue review against `design.md` for future UI additions. |
| D9 — Satisfied · Owner approved | **Owner review and approval** — Direct authorization for this merge. | **Record owner approval honestly and distinguish it from a formal GitHub review event.** | The owner explicitly declared that instructing the coding agent to merge constituted their review, acceptance, and approval. Merged with the owner's direct authorization. GitHub still records zero formal submitted review events, so the record preserves both facts. | Durable evidence: [owner comment](https://github.com/BaranziniLab/biorouter/pull/11#issuecomment-4974237126) plus `refs/notes/owner-authorization` on merge `0e576948a843`. |
| D10 — Preserved · Cleanup complete | **History cleanup** — Preserve branches, tags, and audit refs. | **Remove only auxiliary checkouts after verification.** | All nine auxiliary checkouts were removed after clean-state and containment verification. Source branches, 11 divergent local-tag objects, remote-tag anchors, PR refs, and recovered objects remain available. | Only root `main` is registered; decide tag authority separately without deleting retained provenance. |

### Execution record against the approved seven-step plan

| Step | Status | Item | Outcome |
|---|---|---|---|
| PLAN 01 | Complete | Freeze evidence | Created 61 audit refs, recorded heads, inventoried every branch/worktree—including the SDK v2 worktree—and preserved divergent tags and recovered commits. |
| PLAN 02 | Complete | Repair #11 | Fixed source integration, CI portability, platform-aware tests, and serialized only the fallback `npx` bundle path while keeping direct esbuild parallel. |
| PLAN 03 | Complete | Merge #12 | Apps SDK v2 source `b719aa9f` merged at `5cec0ae3`; typed contracts, corpus, and mandatory runtime smoke were retained. |
| PLAN 04 | Complete | Merge #13 | Usage source `0e76b382` merged at `9c3f70c6`; v11/v12 ownership and nullable pricing were preserved. |
| PLAN 05 | Complete | Integrate #11 last | Resolved textual and semantic conflicts, unified loop authority, unioned migrations and APIs, and qualified final source `822c7c3daf49`. |
| PLAN 06 | Complete | Qualify the combined system | Local workspace, release, clippy, self-test, UI, Chromium, and Electron evidence passed; exact-head Apps and Rust workflows also completed successfully. |
| PLAN 07 | Merge/cleanup complete | Merge, sync, and clean checkouts | #11 merged as `0e576948a843`, root `main` was synchronized without altering user-owned dirty items, retained refs were verified, and all nine auxiliary checkouts were removed. Issue #14 remains an OPEN external coordination follow-up. |

> **What needs a response.** Assign owners and milestones for process-tree hardening, D6 coverage, tag governance, and issue #14. D9 owner review and approval is complete; no response is needed for D9 or the CI rule.

## 1. Execution summary

| Measure | Value | Detail |
|---|---|---|
| Final PR state | 9 / 9 | All examined pull requests merged |
| Final PR #11 source | 150 | commits · 317 files · +74,383/−5,456 |
| Historical conflict forecast | 16 | textual conflict files, all resolved on the integration branch |
| Audit retention refs | 61 | 12 PR refs · 20 remote tags · 29 recovered commits |

### What merged with minimal conflict

- **#12 and #13 followed the low-risk path predicted by the audit.** Their three shared files produced no simulated text conflicts. #12 landed first as merge `5cec0ae3`; #13 followed as `9c3f70c6`.
- **#12 established the formatted Apps baseline before #13.** This removed #13's inherited repository-wide formatting failure without disguising it as a usage-feature defect.
- **Only the agent-loop integration branch was integrated.** The 14 campaign branches were already contained by #11, so merging them individually would have added history without code.

### What required deliberate integration—and was resolved

- **#11 × #12:** the 12 forecast text conflicts across the agent loop, MCP control plane, Apps routes, CLI session loop, and tests were resolved on the combined branch.
- **#11 × #13:** the four forecast conflicts in migrations, reply usage state, and generated API output were resolved from source contracts and regenerated outputs.
- **Loop safety:** RepetitionInspector became the single stop-policy authority; TurnToolGuard retained masking and typed-abort enforcement, eliminating competing stop counters.
- **Database migrations:** usage retained v11/v12 and the agent-loop migrations were renumbered to v13–v16, producing one monotonic union schema.
- **Portability:** the earlier inference, Linux dependency, Windows toolchain, fallback bundling race, platform-assumption, session-workspace, Bash-selection, and background-kill confirmation failures were repaired. Apps run [29351008266](https://github.com/BaranziniLab/biorouter/actions/runs/29351008266) and Rust run [29351008946](https://github.com/BaranziniLab/biorouter/actions/runs/29351008946) passed from exact source `822c7c3daf49`.

> **Warning.** Release-history warning remains: 11 of 20 tag names resolve to different local and remote objects. The integration intentionally preserved both lineages; tag reconciliation remains a separate, user-owned release-governance decision.

## 2. Audit scope and repository reconciliation

The initial audit used Git object and tree comparisons, GitHub pull-request/issue/check metadata, commit lists, path-level diffs, synthetic merge trees, worktree inspection, and targeted test execution. The subsequent authorized execution added real merge commits, combined-source qualification, and Electron interaction evidence. GitHub issue numbers 1, 2, 9, 10, and 14 explain the pull-request numbering gaps; there are no hidden or missing PRs between #3 and #13.

### Remote material made locally durable during the initial audit

| Material | Result | Local retention |
|---|---|---|
| Remote branch heads | Initial heads were fetched and retained for audit; after proving every non-main tip had zero commits outside main, all 17 non-main GitHub branches were deleted. GitHub now has only main. | `refs/biorouter-audit/origin/heads/*` |
| All GitHub PR heads/merge tests | 9 head refs plus 3 open merge refs fetched. | `refs/biorouter-audit/origin/pull/*` |
| Remote tags | All 20 fetched without overwriting divergent local tags. | `refs/biorouter-audit/origin/tags/*` |
| Previously dangling local commits | 29 identified and anchored; subsequent reachability scan found zero unreachable commits. | `refs/biorouter-audit/local/unreachable/*` |

The audit namespace intentionally preserves evidence without moving branches, tags, HEAD, or worktrees. It can be removed later only after release-history owners decide it is no longer needed.

### Historical branch and worktree findings at the audit snapshot

- `main` exactly matched `origin/main` at `758856e02260f83b7b0d297bf7cd2b3d1b18c165` at the audit snapshot.
- No remote-only feature branches and no local-only named branches were found.
- Four local campaign branches (`agent-loop-perf`, `-polish`, `-verify`, `-xplat`) are ahead of their same-name remote-tracking tips, but every extra commit is already reachable through remote `agent-loop-integration`.
- Eight auxiliary worktrees were inspected at the initial snapshot. The SDK v2 and usage branches were checked out separately; the remaining worktrees supported agent-loop review/integration waves.
- No stashes, submodules, or Git notes were found.
- The root worktree initially had an unrelated user-owned deletion of `icon.svg` and user-owned untracked report/video artifacts. The audit and merge did not alter them; the owner later directly authorized committing and publishing the complete remaining local inventory to `main`.
- After final containment verification, all 17 non-main remote branches were deleted. All 19 local named branch tips remain retained and are ancestors of the feature-merge baseline and current `main`; GitHub has one branch, `main`.

### Complete named-branch inventory and execution disposition

| Local branch | Remote after cleanup | Local delta | Integration disposition |
|---|---|---|---|
| `main` | Present | Equal at audit; feature merge `0e576948` | Integration target; contains #12, #13, and #11. A later documentation/assets follow-up advances main without changing those merge anchors. |
| `feat/apps-sdk-v2` | Deleted after containment | Final source `b719aa9f` | PR #12 merged first at `5cec0ae3`; local branch retained and remote branch deleted. |
| `feat/issue1-usage-reporting` | Deleted after containment | Final source `0e76b382` | PR #13 merged second at `9c3f70c6`; local branch retained and remote branch deleted. |
| `agent-loop-integration` | Deleted after containment | Final source `822c7c3d` | PR #11 integrated third and merged at `0e576948`; local branch retained and remote branch deleted. |
| `agent-loop-wave0` | Deleted after containment | Equal at audit | Contained by #11; local branches retained for audit, remote branches deleted, and no separate merge was needed. |
| `agent-loop-context` | Deleted after containment | Equal at audit | Contained by #11; local branches retained for audit, remote branches deleted, and no separate merge was needed. |
| `agent-loop-compaction` | Deleted after containment | Equal at audit | Contained by #11; local branches retained for audit, remote branches deleted, and no separate merge was needed. |
| `agent-loop-security` | Deleted after containment | Equal at audit | Contained by #11; local branches retained for audit, remote branches deleted, and no separate merge was needed. |
| `agent-loop-checkpoints` | Deleted after containment | Equal at audit | Contained by #11; local branches retained for audit, remote branches deleted, and no separate merge was needed. |
| `agent-loop-processes` | Deleted after containment | Equal at audit | Contained by #11; local branches retained for audit, remote branches deleted, and no separate merge was needed. |
| `agent-loop-hooks` | Deleted after containment | Equal at audit | Contained by #11; local branches retained for audit, remote branches deleted, and no separate merge was needed. |
| `agent-loop-loopdet` | Deleted after containment | Equal at audit | Contained by #11; local branches retained for audit, remote branches deleted, and no separate merge was needed. |
| `agent-loop-server` | Deleted after containment | Equal at audit | Contained by #11; local branches retained for audit, remote branches deleted, and no separate merge was needed. |
| `agent-loop-review` | Deleted after containment | Equal at audit | Contained by #11; local branches retained for audit, remote branches deleted, and no separate merge was needed. |
| `agent-loop-perf` | Deleted after containment | Local ahead 11 at audit | The local tip commits were already reachable through #11. Local branches remain for audit; their remote branches were safely deleted. |
| `agent-loop-polish` | Deleted after containment | Local ahead 4 at audit | The local tip commits were already reachable through #11. Local branches remain for audit; their remote branches were safely deleted. |
| `agent-loop-verify` | Deleted after containment | Local ahead 7 at audit | The local tip commits were already reachable through #11. Local branches remain for audit; their remote branches were safely deleted. |
| `agent-loop-xplat` | Deleted after containment | Local ahead 6 at audit | The local tip commits were already reachable through #11. Local branches remain for audit; their remote branches were safely deleted. |

### Worktree inventory

This table preserves the initial audit snapshot so its commit evidence remains reproducible; execution-created or advanced worktree heads were newer. After the merge, the eight auxiliary checkouts represented here plus execution-created `.worktrees/approved-merge` were removed cleanly. Only `/Users/wanjun/Desktop/biorouter` on `main` remains registered. Local source branches, tags, and audit refs were retained; the 17 redundant non-main GitHub branches were subsequently deleted after containment verification.

| Path | Branch | Head at initial audit snapshot |
|---|---|---|
| `/Users/wanjun/Desktop/biorouter` | `main` | `758856e02260` |
| `/Users/wanjun/Desktop/biorouter-sdk-v2-wt` | `feat/apps-sdk-v2` | `370c478a8cc2` |
| `.claude/worktrees/issue1-usage` | `feat/issue1-usage-reporting` | `269db0ee3d5a` |
| `.worktrees/agent-loop-review` | `agent-loop-review` | `ca835e3365d5` |
| `.worktrees/integration` | `agent-loop-integration` | `c4b51e045cb5` |
| `.worktrees/perf` | `agent-loop-perf` | `9c9856e7321f` |
| `.worktrees/polish` | `agent-loop-polish` | `23c8db8cedfd` |
| `.worktrees/verify` | `agent-loop-verify` | `2467c53692ca` |
| `.worktrees/xplat` | `agent-loop-xplat` | `3504525269ec` |

> **Issue #14 disposition.** The coordination issue says #11 contains 104 commits and describes wave-three branches as empty placeholders. That was already stale at the 139-commit initial audit and is now superseded by the 150-commit final source head `822c7c3daf49499d7c39d92d303d24e80d042b07`, successful same-head CI, and merge `0e576948a843`. Issue #14 remains OPEN and stale at this report cutoff; updating it with the final head, CI links, merge result, and this report is pending external coordination. Preserve its history and use the Git graph as merge authority.

## 3. Pull-request portfolio: execution state and audit scope

State and anchors below reflect completed execution through 2026-07-14. Except for the explicitly updated #11 row, size columns preserve the initial audit snapshot so the original conflict forecast remains reproducible; they are historical measurements, not claims about each final merge diff.

| PR | State | Feature / actual scope | Commits | Files | Diff | Disposition |
|---|---|---|---|---|---|---|
| [#3](https://github.com/BaranziniLab/biorouter/pull/3) | Merged | TUI input wrapping, bottom-pinned composer, live streaming, richer rendering. | 1 | 2 | +460/−59 | Baseline; no action. |
| [#4](https://github.com/BaranziniLab/biorouter/pull/4) | Merged | DeepSeek alias retirement plus tree-sitter language expansion and additional z.ai/MiMo provider work. | 4 | 37 | +1,874/−98 | Baseline; title understates scope. |
| [#5](https://github.com/BaranziniLab/biorouter/pull/5) | Merged | Five agent improvements, 36-app QA corpus, Autovis hardening, Agent Drafter work, and release assets. | 16 | 919 | +116,606/−1,363 | Baseline; exceptionally broad. |
| [#6](https://github.com/BaranziniLab/biorouter/pull/6) | Merged | Agent Drafter rebuilt as the Apps v1 platform with TypeScript apps and live agent backend. | 1 | 35 | +4,172/−790 | Foundation for #12. |
| [#7](https://github.com/BaranziniLab/biorouter/pull/7) | Merged | jcode-inspired performance: allocator, strip, AWS gating, interrupt, HTTP/render/scheduler tuning. | 15 | 53 | +1,848/−22 | Baseline; regression reference for #11 performance. |
| [#8](https://github.com/BaranziniLab/biorouter/pull/8) | Merged | Warm two-tone UI theme and chat/sidebar polish. | 14 | 38 | +188/−142 | Visual baseline governed by design.md. |
| [#11](https://github.com/BaranziniLab/biorouter/pull/11) | Merged | Agent-loop overhaul plus completed #12/#13 integration, conflict resolution, CI portability, and qualification fixes. | 150 | 317 | +74,383/−5,456 | Source `822c7c3d`; exact-head CI passed and merge `0e576948` landed third. |
| [#12](https://github.com/BaranziniLab/biorouter/pull/12) | Merged | Apps SDK v2 phases 1–6, typed contracts/platform APIs, 100-app corpus, remediation and smoke tooling. | 15 | 153 | +40,116/−1,864 | Source `b719aa9f`; merged first at `5cec0ae3`. |
| [#13](https://github.com/BaranziniLab/biorouter/pull/13) | Merged | Usage reporting: model-aware billed/cache tokens, price attribution, server/CLI/UI reports. | 11 | 39 | +5,257/−217 | Source `0e76b382`; merged second at `9c3f70c6`. |

The final 240-commit portfolio total uses the selected qualified-source convention: 1 + 4 + 16 + 1 + 15 + 14 + 150 + 18 + 21. The #12 and #13 size cells above intentionally preserve their initial 15- and 11-commit audit snapshots, as stated in the section introduction.

At the initial audit, GitHub permitted merge commits, squash, and rebase; auto-merge and delete-on-merge were disabled, and no `main` ruleset or branch protection was detected. The audited PRs had zero formal review submissions. The owner explicitly declared that their instruction to the coding agent to merge was their review, acceptance, and approval for this merge. Accordingly, D9 is satisfied by direct owner approval, while the report does not mislabel that approval as a formal GitHub review event. The durable records are the [owner-authored PR comment](https://github.com/BaranziniLab/biorouter/pull/11#issuecomment-4974237126) and `refs/notes/owner-authorization` on merge `0e576948a843`.

### Git object anchors for every PR

| PR | Audited head | Main merge commit |
|---|---|---|
| #3 | `c7e8df3b898e933c77b8b3943f2dec880d1fa0b2` | `df1a53e594923fd984bc1914d2d602e211428198` |
| #4 | `53ebf3cb833ca25288a5cecd2e53c4e4bb860431` | `2e7c78422df98d3e54273c427ca965449495ee7c` |
| #5 | `09d61e5edd17b85417dccca06f5103b6d6d56973` | `35d8ce84225ab3cee548bc4e76b1d0e3e1338120` |
| #6 | `d385f3e6a4c9fc1caba9a5a11ecca40dd48644cc` | `07c0cab25902e6d0951fbbf2059b721cd8ffa6b0` |
| #7 | `5bde984a6d5a702decae978ef8b6ba08ac8888ea` | `4132f3d22b2d890d2b6f16f735fc1c1f02040cd8` |
| #8 | `ea1d69f061841a0c3705ea4d478fad56a7b79c77` | `9a5b975abb638f9c69ee3a70f19e3ecc9e111790` |
| #11 | `822c7c3daf49499d7c39d92d303d24e80d042b07` | `0e576948a84309f31a16fa26c0629c45496e3bfc` |
| #12 | `b719aa9f024e` | `5cec0ae3` |
| #13 | `0e76b382` | `9c3f70c6bf98068fb15ff2fd03157beb426359f8` |

Feature-merge and branch-cleanup anchor: `0e576948a84309f31a16fa26c0629c45496e3bfc`. Later documentation/assets commits do not alter the qualified source or merge result.

## 4. Historical merged baseline and how it achieved its goals

This section preserves the six PRs that were already on `main` at the initial audit. PR #12 and #13 are recorded in the execution chapter because they merged during this work.

### #3 — CLI/TUI interaction overhaul

The change concentrated the interaction work in two files: it made the input area wrap instead of clipping, anchored the composer at the bottom, streamed model output as it arrived, and enriched message rendering. It achieved the UX goal by changing the terminal render/layout loop and state updates together, avoiding a split implementation in which rendering and input geometry disagreed.

### #4 — provider future-proofing with a wider payload

The advertised DeepSeek work replaced brittle model-name assumptions with aliases compatible with the provider's `deepseek-chat`/`deepseek-reasoner` direction. Commit inspection also shows tree-sitter support for C, C++, R, Julia, and MATLAB plus z.ai/MiMo provider/tests. Those additions achieved broader language/provider coverage but make the PR title incomplete as a historical description; future release notes should enumerate all three feature families.

### #5 — QA campaign and agent improvements

This was the largest historical PR: 919 files and more than 116k added lines. It combined a 36-app execution corpus, Autovis hardening, Agent Drafter evolution, five agent/reporting improvements, and release outputs. The goal was achieved through end-to-end examples and fixtures as well as runtime code, but the breadth means regressions should be localized with its 16 commit boundaries rather than treating the merge commit as a single atomic feature.

### #6 — Agent Drafter / Apps v1

The one-commit platform rebuild introduced TypeScript-authored apps, a live agent backend, bundling, control routes, UI embedding, and examples as one vertical slice. It is the architectural parent of #12; #12 should therefore be reviewed as a contract-hardening and platform-expansion release, not a new unrelated subsystem.

### #7 — jcode-derived performance work

Performance improvements were distributed across build configuration and runtime hot paths: jemalloc, symbol stripping, optional AWS code, soft interrupt behavior, HTTP/rendering efficiency, and scheduler tuning. The commits preserve individual optimization decisions, which is useful when benchmarking #11's new caches/coalescing against the established performance baseline.

### #8 — warm two-tone UI foundation

Theme tokens and chat/sidebar surfaces were adjusted into the warm, quiet visual language formalized by the repository design specification (`design.md`). This establishes the visual contract for UI added by #11–#13: semantic tokens, flat surfaces, hairlines, shared primitives, both themes, and dense row-oriented layouts.

## 5. Feature execution record

### #12 — Apps SDK v2

This six-phase evolution moved Agent Drafter from an examples-driven v1 into a typed, testable application platform. It merged first from source head `b719aa9f024e` as `5cec0ae3`. That ordering worked because its only pairwise overlap with #13 was clean and its formatted baseline removed the unrelated Agent Drafter format failure inherited by #13.

| Capability | Implementation mechanism | Goal achieved |
|---|---|---|
| Shared runtime and transport | Shared state, authenticated WebSocket channel, generated/typed bindings. | Moves app state and calls from implicit conventions into a consistent runtime contract. |
| Richer UI protocol | ID catalog, UI patch/morph operations, scientific component pack, custom components. | Lets apps evolve UI safely while validating component identity and region targeting. |
| Typed service surface | Typed RPC, signals, themes, starters, and `br.kb`. | Replaces prompt-only APIs with discoverable, verifiable schemas. |
| Platform APIs | Export, CLI, presence, and multi-agent capabilities. | Turns isolated demos into portable, collaborative application workflows. |
| Failure containment | CSP, autorun controls, `ui_error`, `ui_suggest`, fail-closed server checks. | Prevents silent contract drift and gives agents structured recovery feedback. |
| Corpus remediation | 100-app corpus plus build/smoke harness; fixes invented IDs, absent initial state, and inaccessible drag interactions. | Tests the actual authoring surface and converts recurring app mistakes into machine-enforced rules. |

The remediation found 19 invented component IDs, 30 missing initial-state cases, and 10 inaccessible drag-only interactions while retaining 30/30 compatibility coverage. The app-smoke contract was used as a real qualification gate, including a Chromium run and a release Electron/Avatar Lab interaction. D6 remains intentionally narrower than "all scenarios complete": the 30 applications are not all fully reauthored and several scenarios remain unit/build-level rather than browser-level.

### #13 — usage reporting

This change merged second from source head `0e76b382` as `9c3f70c6`. It separates billable token categories instead of presenting one misleading total. It records fresh input, cache-read input, cache-created input, and output independently; attributes each turn to model/provider; applies shared cache-aware pricing; and returns unknown price as null instead of zero. The same data then flows through server reports, a CLI command, Settings usage UI, and a per-model cost popover.

> **Why it is easy to combine with #12.** Only `crates/biorouter-cli/src/cli.rs`, `crates/biorouter-cli/src/commands/mod.rs`, and `crates/biorouter/src/providers/formats/bedrock.rs` overlap; a merge-tree simulation produced no textual conflicts.

Its migration v11 adds per-event model/provider and v12 adds cache buckets. Those identifiers were retained because #13 landed before #11; #11's colliding migrations moved to v13–v16. Unknown pricing remained nullable through the UI, where unpriced data rendered as "—" and aggregate lower bounds used "≥" instead of showing a fabricated "$0.00."

### #11 — agent-loop overhaul

The final source head `822c7c3daf49499d7c39d92d303d24e80d042b07` contains 150 commits, changes 317 files (+74,383/−5,456), contains all 14 supporting campaign branches, and integrates the already-merged #12/#13 baseline. Its original 70-item campaign comprises 67 review proposals plus BR68–BR70 and GAP2. Major feature clusters are:

- **Context:** repository map, trust and source boundaries, prompt variants, focused context selection.
- **Memory/compaction:** recent-message windowing, retries, compaction, full-text searchable memory.
- **Security:** deny lists, redaction, injection scanning, policy tiers, sandbox controls.
- **Recovery:** checkpoints, undo, stable message IDs, branches, and message blobs.
- **Collaboration:** processes, subagents, active-work accounting, and shared MCP connections.
- **Policy:** SmartApprove, hooks, scopes, tool-risk evaluation.
- **Loop safety:** repetition, semantic/oscillation/failure/stall/mistake detection and observability.
- **Server/CLI controls:** session lock, cancellation, idempotent reply, budgets, reasoning effort.
- **Performance/portability:** event coalescing, caches, removal of per-token database reads, and cross-platform fixes.

High-value slices include BR46 finish reasons, BR18 SmartApprove, BR19 hooks, BR43 checkpoints, BR1 repository maps, BR33 session locks, BR29–32/66 loop protection, BR52 removal of token-hot-path database reads, BR54 shared MCP pooling, and BR62 cancellation. New capabilities are generally default-off, but correctness/security changes still alter behavior and need explicit regression testing.

> **Final qualification passed on the exact merged source.** Rust run [29351008946](https://github.com/BaranziniLab/biorouter/actions/runs/29351008946) and Apps run [29351008266](https://github.com/BaranziniLab/biorouter/actions/runs/29351008266) completed successfully from `822c7c3daf49499d7c39d92d303d24e80d042b07`. Rust guards, both cross-checks, Ubuntu, macOS, and Windows passed; the schedule-only nightly job was correctly skipped. The older run 29294328616 and its 139-commit/268-file snapshot are retained below only as historical diagnosis that motivated the repairs.

| Historical failure | Diagnosis | Executed repair |
|---|---|---|
| Windows GNU cross-check · E0283 | `agent.rs:1171–1173` collects `text.text.as_ref()` and joins it; target-specific inference cannot select the borrowed type. | Made the string type explicit and reran focused and workspace qualification before starting the final matrix. |
| Ubuntu native link · `-lxcb` | The native job installs protoc but not XCB development libraries; the Linux cross image does install `libxcb1-dev`. | Aligned the native Linux job's required packages and retained explicit cross-platform coverage. |
| Windows native · aws-lc/cmake | Visual Studio is detected, but `cmake` crate 0.1.53 does not select a usable generator; its C11 compiler probe fails. | Made the workflow/toolchain path explicit and made native tests platform-aware; final Windows native and GNU jobs passed on the exact source head. |
| Ubuntu fallback bundle race | Concurrent Agent Drafter tests could collide in `npx`/npm cache state when direct esbuild was unavailable. | Serialized only the `npx` fallback and isolated its temporary cache per process; direct esbuild remains parallel. A regression test covers the fallback. |
| Cross-platform test assumptions | Hooks, workspace filenames, retry sleeps, permissions, prompts, GCP auth paths, and command-policy fixtures assumed POSIX behavior. | Used native commands and paths, normalized CRLF, separated POSIX-only fixtures, and retained explicit Windows policy coverage. |
| Windows MCP developer fixtures and transport | Text-editor expectations mixed LF with native CRLF, current-directory output differed by shell/path representation, POSIX timing commands did not map to Windows, and an undrained in-memory client could close the test transport. | Made line endings and cwd comparisons platform-native, used PowerShell-native command/timing fixtures, and drained client notifications so the transport remains live through the assertion. |
| MCP session-workspace harness | Some tests created files beside process-global paths instead of the session working directory, so they could accidentally exercise host layout rather than the workspace jail contract. | Made the tests use their session working directories and explicitly assert that outside-workspace access is rejected by the jail. |
| Windows Knowledge IDs and write lock | Native backslashes leaked into logical page IDs, while libgit2 could attempt to stage the exclusively held `.biorouter-knowledge/write.lock`. | Normalized logical Knowledge IDs to stable `/`-separated paths and excluded the runtime write lock from every staging path, with a regression that holds the lock while committing a normal file. |
| Windows Agent Drafter launcher validation | Passing a native `C:\…` launcher path to Bash caused `bash -n` to fail before parsing the script, and the `System32` WSL shim could be discovered even when it was unusable. | Streamed launcher source to `bash -n` through standard input, ran the harness from a controlled working directory, and selected only a successfully probed Git-for-Windows Bash—rejecting the unusable `System32` WSL shim. |
| Windows background kill confirmation | `taskkill` commands were launched without being awaited, so `kill()` could report success while the supervisor still observed the job as running. | Bounded and awaited `taskkill /T`, and made successful `kill()` wait for supervisor-observed terminal leader state. Whole-descendant guarantees and identity lookup beyond the 12-second confirmation window remain explicit hardening work. |

## 6. Compatibility, file overlap, and executed conflict ownership

Counts in this section are the historical pre-integration forecast. They are retained to show why the chosen order mattered and how each known conflict was resolved on the final source branch.

| Pair | Shared files | Text conflicts | Assessment | Merge owner |
|---|---|---|---|---|
| #12 × #13 | 3 | 0 | Easy — forecast and execution both confirmed independent feature surfaces. | Merged in order without manual text conflicts. |
| #11 × #13 | 16 | 4 | Resolved — database schema and response/UI contracts were unioned. | Session persistence + generated API resolution commits. |
| #11 × #12 | 18 | 12 | Resolved — central control-flow and server/MCP surfaces were combined. | Agent-loop + Apps platform resolution commits. |

### #11 × #12 textual conflicts

| File | Executed resolution |
|---|---|
| `crates/biorouter/src/agents/agent.rs` | Used #11's loop architecture as the base and ported #12's typed abort/tool masking without retaining a second stop counter. |
| `crates/biorouter/src/agents/mod.rs` | Unioned exported loop/safety types with Apps typed turn outcomes. |
| `crates/biorouter/src/tool_monitor.rs` | Made RepetitionInspector the decision authority and TurnToolGuard masking an enforcement action of that policy. |
| `crates/biorouter-server/src/routes/apps.rs` | Kept #12's expanded Apps route and ported #11 budgets, reasoning effort, risk cards, and structured-output retry behavior. |
| `crates/biorouter-mcp/src/agent_drafter/control.rs` | Kept #12's functional content, absorbed #11's formatting, and ran the repository formatter. |
| `crates/biorouter-mcp/src/lib.rs` | Unioned module/registration exports and avoided duplicate handler or tool names. |
| `crates/biorouter-cli/src/commands/web.rs` | Retained Apps startup/transport behavior and added #11 controls without duplicate option plumbing. |
| `crates/biorouter-cli/src/main.rs` | Unioned command dispatch and shared initialization while preserving exit-code semantics. |
| `crates/biorouter-cli/src/session/mod.rs` | Combined typed `TurnAborted` outcomes with budgets, reasoning, sandbox, and events. |
| `crates/biorouter-cli/src/session/tui/mod.rs` | Rendered one canonical stop reason while retaining the new controls and avoiding duplicate notices. |
| `crates/biorouter/tests/agent.rs` | Built a combined behavior contract instead of taking either branch's test side wholesale. |
| `crates/biorouter/tests/repetition_inspector_tests.rs` | Covered masking, detection stages, abort mapping, and single-count behavior together. |

The other six shared #11/#12 files auto-merged and were reviewed on the combined branch: `Cargo.lock`, `crates/biorouter-mcp/src/agent_drafter/bundle.rs`, `crates/biorouter-server/src/routes/reply.rs`, `crates/biorouter/src/agents/subagent_handler.rs`, `crates/biorouter/src/agents/tool_execution.rs`, and `crates/biorouter/src/providers/formats/bedrock.rs`. Their syntactically clean results were still included in local build/test qualification because an auto-merge can otherwise hide duplicated policy.

### #11 × #13 textual conflicts

| File | Executed resolution |
|---|---|
| `crates/biorouter/src/session/session_manager.rs` | Preserved #13 v11/v12, renumbered #11's checkpoint/stable-ID/FTS/blob migrations to v13–v16, and unioned the fresh schema. |
| `crates/biorouter/src/agents/reply_parts.rs` | Unioned #11 reply variants/state with #13 model and cache-token usage fields while retaining compatible serialization. |
| `ui/desktop/src/api/index.ts` | Resolved Rust/OpenAPI source first and regenerated the output; no hand-selected conflict side was retained. |
| `ui/desktop/src/api/sdk.gen.ts` | Regenerated from the resolved contract and reviewed as generated output. |

Twelve other #11/#13 overlaps were textually clean but contract-sensitive: `crates/biorouter-server/src/openapi.rs`, `crates/biorouter-server/src/routes/mod.rs`, `crates/biorouter-server/src/routes/session.rs`, `crates/biorouter/src/providers/formats/anthropic.rs`, `crates/biorouter/src/providers/formats/bedrock.rs`, `crates/biorouter/src/providers/formats/openai.rs`, `crates/biorouter/src/providers/pricing.rs`, `crates/biorouter/src/session/mod.rs`, `ui/desktop/openapi.json`, `ui/desktop/src/api/types.gen.ts`, `ui/desktop/src/components/BaseChat.tsx`, and `ui/desktop/src/components/ChatInput.tsx`. Combined source/API/UI qualification checked the resulting contracts for schema loss, double token accounting, and competing stop-reason display.

### Semantic decision: one policy authority, two layers

These components are not true peers: #11's RepetitionInspector detects and classifies loop evidence, while #12's TurnToolGuard masks a tool after the inspector denies it and emits a typed abort if a blocked call continues. The executed integration keeps #11 as the canonical policy engine because it covers exact repetition, near-duplicate arguments, semantic oscillation, and repeated failures with staged thresholds. It keeps #12's masking and `TurnAborted` transport/CLI mapping as enforcement and removes its independent stop threshold. Evidence resets at the last genuine user turn, and blocked signatures are tracked separately from disabled tool names: an exact repeated argument set should not make a corrected invocation of the same tool impossible, while proven tool-wide failure can disable that tool for the rest of the turn. Combined tests exercise one finding, one stop reason, one terminal outcome, and single-count behavior.

### Schema sequence after all three merges

| Version | Owner | Migration |
|---|---|---|
| v10 | Pre-integration main | Historical baseline before the union schema. |
| v11 | #13 | Add model/provider attribution to token events. |
| v12 | #13 | Add cache-read/cache-created token buckets. |
| v13 | #11, renumbered | Checkpoints. |
| v14 | #11, renumbered | Stable message UID and branch point. |
| v15 | #11, renumbered | Full-text-search memory. |
| v16 | #11, renumbered | Message blobs. |

The combined session test suite qualified the monotonic migration implementation. Retain explicit fresh database → v16, v10 → v16, and v12 → v16 coverage, and rehearse rollback/reopen behavior on a copy of a real v10 database before any production migration. Never "solve" a future collision by skipping a version or treating an unrelated schema as equivalent.

## 7. Merge execution record

1. **Complete · freeze audit evidence.** The audit fetched all remote branch/PR/tag material without overwriting divergent local tags, anchored 29 recovered commits, retained PR/remote-tag refs, inspected every local branch and nine worktree paths, and explicitly included `/Users/wanjun/Desktop/biorouter-sdk-v2-wt`. This produced 61 durable audit refs and a reproducible historical baseline.
2. **Complete · refresh and merge #12 Apps SDK v2.** Main was merged into the feature history without rebasing. The final source head `b719aa9f024e` retained the typed Apps contract, corpus remediation, runtime smoke, and signed-off `design.md` treatment. PR #12 then landed first with merge commit `5cec0ae3`.
3. **Complete · refresh and merge #13 usage reporting.** The new main—including #12's formatted Apps baseline—was merged into the usage branch. This kept model/provider attribution at schema v11, cache buckets at v12, and unknown costs nullable through database, server, CLI, and UI. Final source `0e76b382` landed second as merge commit `9c3f70c6bf98068fb15ff2fd03157beb426359f8`.
4. **Complete · integrate and merge #11 onto the combined baseline.** Main containing #12 and #13 was merged into `agent-loop-integration`. The implementation resolved all 16 forecast text conflicts, reviewed the contract-sensitive auto-merges, made RepetitionInspector the sole stop-policy authority, retained TurnToolGuard enforcement, moved #11 migrations to v13–v16, regenerated OpenAPI/TypeScript output from resolved Rust source, and applied the UI design fixes. The final qualified source was `822c7c3daf49499d7c39d92d303d24e80d042b07`; it merged as `0e576948a84309f31a16fa26c0629c45496e3bfc`.
5. **Complete locally · qualify the combined implementation.** Workspace builds, package tests, strict clippy, generated APIs, UI typecheck/format/lint/Vitest, Chromium app smoke, a real-provider self-test workflow, release build, and a launched Electron app were exercised on the combined integration worktree before its clean removal. The detailed evidence and explicitly retained process-tree hardening risks appear in the next section.
6. **Complete · qualify the exact pushed head on GitHub.** Rust run [29351008946](https://github.com/BaranziniLab/biorouter/actions/runs/29351008946) and Apps run [29351008266](https://github.com/BaranziniLab/biorouter/actions/runs/29351008266) passed from the exact final source. D5 was satisfied before merge; Rust guards, both cross-checks, Ubuntu, macOS, and Windows passed, with only the schedule-only nightly job skipped.
7. **Complete · merge #11, sync main, and clean checkouts.** #11 merged with merge commit `0e576948a84309f31a16fa26c0629c45496e3bfc`. Root `main` was fast-forwarded to equal `origin/main` without disturbing the user-owned `icon.svg` deletion or untracked report/video artifacts. Retained refs were verified and all nine auxiliary worktrees were removed without deleting local branches, tag lineages, or audit refs. Issue #14 remains OPEN/stale and pending external coordination.
8. **Complete · reduce GitHub to main and publish the remaining local inventory.** GitHub reported zero open pull requests and zero closed-but-unmerged pull requests. Every exact PR head and all 17 non-main remote branch tips were proven reachable from the feature-merge baseline on `main` with zero unique commits, so the remote branches were deleted with per-ref safety leases. GitHub now has only `main`, and `delete_branch_on_merge` is enabled for future merges. The owner then directly authorized committing the previously preserved root `icon.svg` deletion, this report, and the MP4, WebM, and poster release assets to `main`.

### Why the execution preserved merge history

- Main's recent feature history already uses merge commits.
- #11 intentionally uses one commit per proposal and gate merge commits; squashing removes useful bisect and rollback boundaries.
- #12's phased commits and #13's progressive data→server→CLI/UI commits provide review and incident-localization value.
- Merging main into the branches preserved source SHAs and audit refs; rebasing would have rewritten the evidence used for qualification.

## 8. Combined qualification evidence

The rows below combine evidence executed in the integration worktree before its clean removal with the completed same-head GitHub gate. Linux, macOS, Windows native, both cross-checks, guards, and Apps smoke passed from the exact final source head.

| Qualification area | Observed result | Evidence / interpretation |
|---|---|---|
| Format and source hygiene | Pass | `cargo fmt --all -- --check` passed after the final source fixes; generated files came from the resolved Rust/OpenAPI contract. |
| Workspace build and release | Pass | Locked workspace library/binary build passed. The canonical release build completed, signed binaries, generated OpenAPI/TypeScript output, built Vite assets, and launched Electron. |
| Core `biorouter` | 1,373 unit tests | Unit suite plus integration and documentation tests passed, including agent, loop-safety, session, usage, and migration surfaces. |
| `biorouter-mcp` | 788 passed · 2 ignored | Unit and integration suites passed with two fixture-dependent ignores. Final Windows repairs cover platform-native editor line endings/cwd and PowerShell fixtures with a drained transport, stable `/`-separated Knowledge IDs plus write-lock exclusion, session-directory fixtures that affirm outside-workspace jail rejection, Agent Drafter launcher validation through a successfully probed Git-for-Windows Bash, and supervisor-confirmed background termination. One live Crossref request timed out on a rerun; the exact retry passed, so it is recorded as external-test flakiness rather than hidden. |
| `biorouter-server` | 141 lib + 140 bin | Library, binary, and route integration tests passed across Apps, session, reply, usage, and generated contract surfaces. |
| Hooks and command policy | 91 + 13 focused | Platform-native hook tests and deterministic command-policy suites passed after Windows/POSIX fixture separation and CRLF/path normalization. |
| Strict static quality | Pass | `./scripts/clippy-lint.sh` passed its strict lint, baseline, and banned-TLS checks after the integration repairs. |
| Desktop unit/static suite | 98 files · 821 tests | Vitest passed; UI typecheck, formatter, ESLint, and 128 contrast checks also passed. |
| Chromium application smoke | 1 / 1 | The mandatory built-app smoke completed in 79.45 seconds against the combined application contract. |
| Real-provider self-test | 14 / 14 | `biorouter-self-test.yaml` quick mode passed with a real provider, including browser smoke; the temporary test application was deleted afterward. |
| Actual Electron application | Interactive smoke passed | Release Electron launched with `biorouterd`; 115 Applications loaded. Avatar Lab launched in Chrome, resumed state, and moving Up changed position from x4/y5 to x4/y4. Export Full mode, settings toggles, Max Turns 1000, and Apps SDK controls rendered. |
| Usage UI behavior | Interactive smoke passed | 30-day/7-day controls worked; unknown costs rendered as "—", mixed totals as "≥", and explanatory unpriced/incomplete text was visible. The composer showed a known-cost lower-bound subtotal. |
| Final pushed-head GitHub CI | Pass | Rust [29351008946](https://github.com/BaranziniLab/biorouter/actions/runs/29351008946) and Apps [29351008266](https://github.com/BaranziniLab/biorouter/actions/runs/29351008266) passed on `822c7c3daf49`. Rust guards, both cross-checks, Ubuntu, macOS, and Windows succeeded; the nightly schedule-only job was skipped. PR #11 then merged as `0e576948a843`. |

### Acceptance contract and current evidence

| Scenario | Evidence state |
|---|---|
| Repeated Apps v2 tool use yields one policy finding, one masked enforcement path, one typed abort, and no double count. | Covered locally — combined RepetitionInspector/TurnToolGuard tests and agent suites. |
| Usage v11/v12 and agent-loop v13–v16 form one monotonic schema and preserve reopen/search/checkpoint/report behavior. | Covered locally — session/migration and feature suites; retain real copied-database rehearsal before production rollout. |
| Cancellation, idempotent reply, stable finish reason, and partial usage accounting coexist. | Covered locally — agent/server/reply suites. |
| Unknown price retains tokens and a null cost through database, server, CLI, and UI. | Covered locally + UI observed |
| Apps budgets/reasoning/risk controls coexist with authenticated WebSockets, CSP, export, presence, and typed RPC. | Covered locally + Electron observed |
| Linux, macOS, Windows native, and Windows GNU pass from the exact final pushed head. | Covered by GitHub CI — D5 was satisfied before merge. |

> **Warning.** Residual timeout hardening risk: Windows termination now uses bounded, awaited `taskkill /T`, and a successful `kill()` requires supervisor-observed terminal leader state. That closes the observed false-success failure, but the tests do not prove that every descendant is gone on every platform. A focused follow-up should use Windows job objects and strengthen POSIX process-group guarantees. Identity lookup that exceeds the 12-second caller confirmation window also needs a clearer final classification. These residuals remain explicit decisions rather than unqualified claims of complete process-tree termination.

## 9. Alignment with design.md

The authoritative UI source is `design.md`. The integration applied its principles: flat surfaces and hairlines, rows rather than cards, color as evidence, shared primitives, reachable focus states, a deliberate monospace layer, both themes, and the 40px density rhythm. The following table maps the original audit findings to their executed treatment.

| PR | Historical finding | Executed treatment |
|---|---|---|
| #11 | Three raw `<button>` usages: tool-preview toggle, reasoning-effort trigger, and menu items. | Replaced them with shared Button and Popover/Dropdown/menu primitives so keyboard, focus, and disabled states use the common contract. |
| #12 | `ApplicationsView.tsx` embeds five hex theme swatches in TSX. | Moved theme colors to semantic/typed theme data instead of literal TSX hex values. |
| #12 | `ExportAppDialog.tsx` uses a raw checkbox; enumerable export payloads are rendered as a rounded panel. | Used the shared Checkbox and row-oriented export payload treatment. |
| #13 | Range selection uses raw buttons and error/bar colors use `red-500` literals. | Used shared selection controls and semantic danger/usage tokens. |
| #13 | Per-model table rows use `py-1`, below the specified density rhythm. | Aligned model usage rows to the shared density rhythm and retained tabular number treatment. |

Dynamic inline widths for usage bars remain geometry; their colors use semantic treatment. Local typecheck, format, lint, contrast, unit, and Electron smoke evidence passed. Continue both-theme, keyboard-only, and narrow-width review for future changes against `design.md`.

## 10. Local/remote history and release risks

### Branch containment

Every named branch used by the three feature campaigns was present locally. All 15 named agent-loop branches (integration plus 14 components) are contained by `agent-loop-integration`; #12 and #13 each retained their named source branch after merge. Only the three PR branches were treated as merge targets. The campaign branches remain audit/bisect anchors and were not merged individually.

### Owner authorization provenance

Merged with the owner's direct authorization. The owner confirmed that their coding-agent merge instruction constituted review, acceptance, and approval for this merge. GitHub has zero formal submitted review events, so this is recorded as direct owner approval rather than a formal GitHub review. The durable remote record is the [owner-authored PR comment](https://github.com/BaranziniLab/biorouter/pull/11#issuecomment-4974237126); the durable Git record is `refs/notes/owner-authorization` attached to `0e576948a84309f31a16fa26c0629c45496e3bfc`. Repository configuration `notes.displayRef` points to that note ref so ordinary note display includes the authorization.

### Divergent tags

| Tag | Local object/peeled commit | Remote object/peeled commit |
|---|---|---|
| v1.50.0 | `b7b72013e3dd` | `c30cd549c86f` |
| v1.60.0 | `b0bac520edcc` | `3cbfefbc3977` |
| v1.72.0 | `a4a5cd283eb7` | `c1f3b7ce340d` |
| v1.72.1 | `b75390b2becb` | `14cce8d1c6b7` |
| v1.75.0 | `99a7103d56e5` | `17e06d48cd66` |
| v1.75.1 | `724a4cd5298d` | `d3169d22ed4a` |
| v1.75.2 | `6d05efd968ca` (peeled) | `979332aa5fca` (peeled) |
| v1.76.0 | `b81f5b4bbb07` | `5d66ce0d2c4d` |
| v1.76.1 | `4dff0472aee7` | `8adb5105143e` |
| v1.80.0 | `fad201ced587` | `cceeffb00778` |
| v1.80.1 | `863620992eb2` | `6f7da8d5acd7` |

Nine tags match: v1.20.0, v1.85.0, v1.85.2, v1.85.3, v1.85.4, v1.86.0, v1.86.1, v1.87.0, and v1.87.1. Both divergent lineages were preserved, including archival refs for local tag objects. The remaining user decision is to compare release artifacts/changelogs, identify which lineage was actually published, and set a retention expiry before any authorized correction.

### Recovered local objects

The initial object scan found 29 dangling commits: eight patch-equivalent to reachable commits, five index/snapshot commits with no patch, and 16 unique patch IDs. All are now reachable under the audit namespace. They are not implicitly merge candidates; a maintainer must identify a product requirement before cherry-picking any of the 16 unique patches.

#### Recovered commit inventory (29 objects)

| Commit | Date | Subject |
|---|---|---|
| `d2bf061478f3` | 2026-06-09 | chore(release): automated cross-platform release workflow |
| `824e58d2d3e1` | 2026-06-20 | chore(release): bump to 1.85.4; bundle agent-drafter UI, ACP WebSocket transport, TUI version, testing-apps |
| `6ef113ba7f31` | 2026-06-22 | checkpoint: snapshot pre-existing WIP before perf work |
| `4885ec4a2c12` | 2026-06-23 | build: restore green compile baseline for perf work |
| `c331dc1d1f78` | 2026-06-23 | build: restore green compile baseline for perf work |
| `82820337ac8e` | 2026-06-23 | perf(regex): compile per-call regexes once via Lazy statics |
| `305107915c60` | 2026-06-23 | perf(bundle): import lodash/isEqual directly, not whole lodash |
| `5515fbd54cdb` | 2026-06-23 | perf(electron): cache settings.json instead of re-reading per call |
| `a5de47e59442` | 2026-06-23 | perf(db): faster session hot paths — pragmas, bounded pool, search GROUP BY, token-only read |
| `fc57a65bf9ea` | 2026-06-23 | perf(server): gzip-compress HTTP responses (SSE excluded) |
| `c252507336ec` | 2026-06-23 | perf(ui): coalesce streaming message re-renders to one per frame |
| `dd8d804e6ec7` | 2026-06-23 | perf(ui): coalesce streaming message re-renders to one per frame |
| `645df19e018b` | 2026-06-23 | perf(mcp): stop blocking the async runtime in computer-controller I/O |
| `04c27dde7e4e` | 2026-06-23 | perf(scheduler): non-blocking persist_jobs writes |
| `b69c204a37bc` | 2026-06-23 | docs(perf): performance review report + implementation log |
| `d25b30004f4a` | 2026-06-25 | docs(release): add v1.86.1 release notes |
| `6452bac18152` | 2026-07-09 | docs(release): add v1.86.1 release notes |
| `24b665777261` | 2026-07-11 | index on ui-hardening-a11y-tests: 702966fe Make the whole view-transition overlay click-through for interruptibility |
| `27cfb97b9c51` | 2026-07-11 | WIP on ui-hardening-a11y-tests: 702966fe Make the whole view-transition overlay click-through for interruptibility |
| `51f64e0a561f` | 2026-07-12 | index on feat/apps-sdk-v2: 5d29afdc SDK v2 Phase 1: shared state doc, WS auth, bindings, typed surface |
| `86210fc2f774` | 2026-07-12 | WIP on feat/apps-sdk-v2: 5d29afdc SDK v2 Phase 1: shared state doc, WS auth, bindings, typed surface |
| `2daa9e114cd2` | 2026-07-12 | WIP on feat/apps-sdk-v2: 5d29afdc SDK v2 Phase 1: shared state doc, WS auth, bindings, typed surface |
| `b650e2b87b4f` | 2026-07-12 | index on feat/apps-sdk-v2: 5d29afdc SDK v2 Phase 1: shared state doc, WS auth, bindings, typed surface |
| `17ffe1c3c3a1` | 2026-07-12 | untracked files on agent-loop-server: 104802c8 BR-52: carry the agent-computed TokenState in the event stream (kill per-token DB reads) |
| `1c4e23fb0fd0` | 2026-07-12 | On agent-loop-server: br61-wip |
| `8014109799f1` | 2026-07-12 | index on agent-loop-server: 104802c8 BR-52: carry the agent-computed TokenState in the event stream (kill per-token DB reads) |
| `0fbc525de540` | 2026-07-13 | index on agent-loop-polish: 3225ce5b BR-62b: wire desktop GUI to reliable-cancel + idempotent /reply |
| `a928601d5bc0` | 2026-07-13 | On agent-loop-polish: polish-prettier-check |
| `443aed9df848` | 2026-07-13 | BR-59: cache tree-sitter Query per (language, kind) instead of recompiling per file |

## 11. Residual risk register and rollback strategy

| Risk | Likelihood / impact | Control | Rollback boundary |
|---|---|---|---|
| Dual loop detector causes premature stops or duplicate aborts | Resolved in source / high if regressed | Single RepetitionInspector authority, TurnToolGuard enforcement, combined tests. | Revert the focused loop-policy resolution or #11 merge. |
| Migration collision corrupts/strands sessions | Resolved in source / critical if regressed | v13–v16 renumber, monotonic schema tests, production copied-DB rehearsal still recommended. | Stop rollout before write traffic; restore the DB copy and revert #11. |
| Generated API loses Apps or usage endpoints | Reduced / high | Regenerated from resolved Rust source; local server, UI, OpenAPI, and Electron evidence passed. | Revert the generation/resolution commit; never hand-patch generated output. |
| Cross-platform release failure | Qualified / reduced | Exact-head Rust and Apps workflows passed across guards, both cross-checks, Ubuntu, macOS, and Windows. | For future failures, repair and requalify the changed head before release. |
| Timeout leaves spawned descendants alive | Reduced but unproven / security-medium | Bounded, awaited Windows `taskkill /T` plus supervisor-confirmed terminal leader; retain Job Objects/POSIX group hardening and classify identity lookup beyond 12 seconds. | Disable or constrain affected shell execution until descendants are reliably reaped. |
| Usage cost shown as zero for unknown model | Reduced / financial trust | Nullable price contract, automated tests, and observed Electron lower-bound display. | Disable cost display while retaining token counts. |
| Apps security regression | Reduced / high | Auth/CSP/fail-closed tests, Chromium/Electron smoke, and completed owner approval; an independent focused security review remains optional defense-in-depth, not an unresolved approval requirement. | Disable v2 feature gates or revert #12. |
| Design/accessibility drift | Reduced / medium | Applied `design.md` fixes; typecheck/lint/contrast/unit/Electron evidence passed. | Revert the isolated UI resolution. |
| Release tags overwritten incorrectly | Medium / critical provenance | Separate governed task; never force tags during merge work. | Audit refs retain both object lineages. |
| Approval provenance is misstated | Controlled / audit-medium | Record both facts: owner review/acceptance/approval is evidenced by the PR comment and Git note, while GitHub has zero formal submitted review events. | Correct the PR comment, Git note, release notes, and governance record if any provenance fact changes. |

Conflict resolutions were kept in reviewable commits on #11's integration branch: migration and loop-safety unions, Apps/server integration, generated API, design cleanup, and CI/platform repairs. Those boundaries remain available for targeted rollback now that #11 has landed as a merge commit.

## 12. Historical commit-level inventory and final anchors

The detailed lists below preserve the commit-subject inventory returned by GitHub at the initial audit snapshot. They are intentionally historical: #11 subsequently grew from the listed 139 commits to 150 while integrating #12/#13 and qualification repairs. Use the final anchors below—not the old detail count—as execution authority.

| Execution item | Final source anchor | Final disposition |
|---|---|---|
| PR #12 · Apps SDK v2 | `b719aa9f024e` | Merged at `5cec0ae3`. |
| PR #13 · usage reporting | `0e76b382` | Merged at `9c3f70c6`. |
| PR #11 · combined integration | `822c7c3daf49499d7c39d92d303d24e80d042b07` | 150 commits; exact-head CI passed and merge `0e576948a843` landed. |

### PR #3 — CLI/TUI UX overhaul (1 commit)

| Commit | Date | Subject |
|---|---|---|
| `c7e8df3b898e` | 2026-06-19 | feat(cli/tui): wrapping input, bottom-pinned bar, live streaming, richer rendering |

### PR #4 — Provider future-proofing and language/provider expansion (4 commits)

| Commit | Date | Subject |
|---|---|---|
| `f773a0636a38` | 2026-06-19 | feat(mcp/analyze): bump tree-sitter to 0.26 and add C++, C, R, Julia, MATLAB |
| `f8d0fd10fe82` | 2026-06-19 | feat(providers): add z.ai (GLM) and Xiaomi MiMo as selectable LLM providers |
| `f280c626d480` | 2026-06-19 | test(providers): make the Xiaomi MiMo live provider test actually run and pass |
| `53ebf3cb833c` | 2026-06-19 | feat(providers): future-proof DeepSeek's deepseek-chat/-reasoner retirement |

### PR #5 — QA campaign and agent improvements (16 commits)

| Commit | Date | Subject |
|---|---|---|
| `341976a4f391` | 2026-06-19 | feat(mcp/autovisualiser): harden the pipeline and add 24 new visualizations |
| `03ba5a2d9639` | 2026-06-19 | feat(mcp): add Agent Drafter built-in extension, plus bundled working-tree changes |
| `52bba1149bb8` | 2026-06-19 | fix(mcp/autovisualiser): accept stringified `data` args, fix map sizing, drop experimental note |
| `efa8a4614d01` | 2026-06-19 | fix(developer): accept 'file_path' as alias for text_editor 'path' |
| `4abb47dba444` | 2026-06-19 | fix(providers): deeper retry budget for transient rate-limit (429) errors |
| `a2566d7d2cb6` | 2026-06-19 | feat(developer,hooks): git context in the developer extension + verify/checkpoint Stop hook |
| `ffa433272b21` | 2026-06-19 | fix(cli): graceful --resume fallback + readable tool-call paths |
| `d75abbc8b0a7` | 2026-06-19 | fix(agent): make the per-turn action-limit stop explicit and quantified |
| `2f56a3d9835b` | 2026-06-19 | qa: import biorouter-testing-apps QA suite into the project |
| `46a006de9c60` | 2026-06-19 | qa: repoint build harness ROOT to in-project biorouter-testing-apps (env-overridable) |
| `442893413d00` | 2026-06-19 | qa: snapshot apps 13-15 (phylo, variant-caller, kmer) + round-3 report |
| `4191c5411b97` | 2026-06-20 | qa: snapshot bioinformatics batch apps 16-20 + round-4 report |
| `851b662818f0` | 2026-06-20 | qa: snapshot med batch apps 21-25 + round-5 report |
| `ec105199159f` | 2026-06-20 | qa: complete biomedical batch (apps 26-30) + round-6 report + harness mitigation |
| `5cbb45cdee6a` | 2026-06-20 | chore(release): bump to 1.85.4; bundle agent-drafter UI, ACP WebSocket transport, TUI version, testing-apps |
| `09d61e5edd17` | 2026-06-20 | qa: statistics batch apps 31-37 + round-7 report (loop paused at user request) |

### PR #6 — Agent Drafter Apps v1 (1 commit)

| Commit | Date | Subject |
|---|---|---|
| `d385f3e6a4c9` | 2026-06-23 | feat(agent-drafter): rebuild as a BioRouter Apps platform (TypeScript apps + live agent backend) |

### PR #7 — jcode-derived performance (15 commits)

| Commit | Date | Subject |
|---|---|---|
| `bb750db04b1e` | 2026-06-24 | chore(perf): add jcode comparison analysis + benchmark harness |
| `b42dd6b522e2` | 2026-06-24 | bench(perf): fix harness (BIOROUTER_PORT, awk MB) + capture baseline |
| `2bd4affa0119` | 2026-06-24 | perf(FW1): tuned jemalloc as global allocator (biorouterd + CLI) |
| `13007f3d79fe` | 2026-06-24 | perf(FW5): Auto-Vis CDN-default in GUI + explicit backgroundThrottling |
| `22c3b0fd68b5` | 2026-06-24 | perf(FW2): Cargo profiles — strip release (-13% binaries) + release-dist + quick |
| `598da59b0139` | 2026-06-24 | perf(SW5-GUI): hoist O(n^2) per-message scans to O(n) in the message list |
| `2daa584c89f8` | 2026-06-24 | perf(FW3): spawn_blocking the cold-path token scan |
| `1ac850028788` | 2026-06-24 | perf(FW3+SW3): HTTP client hardening — read_timeout + connect_timeout + keepalive + pool |
| `07a8fc214136` | 2026-06-24 | perf(FW4): resource-aware scheduler + subagent fork-bomb guard |
| `9af616fb0ab7` | 2026-06-24 | perf(SW4): feature-gate the AWS SDK behind default-on aws-providers |
| `19eff1c380a4` | 2026-06-24 | perf(SW2): deterministic tool ordering for prompt-cache stability |
| `4910c42ddf12` | 2026-06-24 | perf(SW5-CLI): cache draw_history's wrapped-line count |
| `56d6a7ee26bd` | 2026-06-24 | perf(SW1): soft interrupt — queue + inject at safe boundary + /interrupt route |
| `573ccebb5d10` | 2026-06-24 | docs(perf): implementation & benchmark report for jcode borrows |
| `5bde984a6d5a` | 2026-06-24 | docs(perf): finalize report with clean cumulative numbers + SW1 live check |

### PR #8 — Warm two-tone UI (14 commits)

| Commit | Date | Subject |
|---|---|---|
| `8c37623c54a7` | 2026-07-02 | ui(theme): give the sidebar its own warm-beige surface (two-tone canvas) |
| `31392293b1b1` | 2026-07-02 | ui(theme): flatten list/settings rows, hairline sidebar edge, define text-subtle |
| `bd60cdbc4ae7` | 2026-07-02 | ui(theme): flatten Knowledge panels to the warm token system |
| `00e3e9e0535e` | 2026-07-02 | ui(theme): Settings flat header + consistent section labels |
| `3b7d8300fabf` | 2026-07-02 | ui(theme): flatten Workflows & Scheduler rows/cards to token borders |
| `65133df5078c` | 2026-07-02 | ui(theme): flat app cards + Home metric scale |
| `e34226d90541` | 2026-07-02 | ui(theme): Skills header inset divider, onboarding select focus, extensions highlight |
| `76a9b0843104` | 2026-07-02 | ui(theme): composer shadow token + harmonize overlays |
| `0c8cf59b8231` | 2026-07-02 | ui(theme): prettier formatting for theme changes |
| `8f848b851a12` | 2026-07-02 | ui(theme): give the chat composer a visible edge + subtle lift |
| `93f62e90dddf` | 2026-07-02 | ui(theme): square off the main panel to sit flush with the straight sidebar |
| `db2926547c99` | 2026-07-02 | ui(theme): warm off-white chat canvas to match the rest of the app |
| `5409eaf6a646` | 2026-07-03 | ui(theme): offset session-name pill when the sidebar is collapsed |
| `ea1d69f06184` | 2026-07-03 | ui(fix): make top controls clickable when the sidebar is collapsed in chat |

### PR #11 — initial audited head only (139 of final 150 commits)

| Commit | Date | Subject |
|---|---|---|
| `6f17f6698bf8` | 2026-07-12 | docs: comprehensive agentic-loop review + 67-proposal improvement program |
| `68b9ae4c9080` | 2026-07-12 | docs: single-file HTML report for the agentic-loop review |
| `a409e7d7e49f` | 2026-07-12 | merge: agent-loop review corpus into integration (spec for the fix campaign) |
| `698bd7e9e349` | 2026-07-12 | docs: agent-loop fix campaign plan (waves, gates, conventions) |
| `553850684258` | 2026-07-12 | docs(campaign): record baseline — 53 suites ok, 1 known live-API failure (test_anthropic_provider) |
| `0393b122851f` | 2026-07-12 | BR-38: reconcile stale currently_running flags on scheduler load |
| `53088bc8e48f` | 2026-07-12 | BR-25: fail-closed on malformed tool_call in permission store |
| `be6f087ca291` | 2026-07-12 | BR-46: map Anthropic stop_reason to finish_reason in streaming path |
| `0d07221b084e` | 2026-07-12 | BR-39: add shell_list tool for background jobs |
| `f9f15b597de7` | 2026-07-12 | BR-20: always-on non-bypassable catastrophic-command denylist |
| `58535da236be` | 2026-07-12 | BR-36: consolidate RepetitionInspector to single production path |
| `703717dc4c14` | 2026-07-12 | docs(designs): BR-17/21/43/45/54/65 architectural design docs (pre-implementation) |
| `fa5a0d0c3b5a` | 2026-07-12 | BR-34: per-reply tool-call ceiling with assistant-visible stop |
| `52867b377da2` | 2026-07-12 | BR-26: cap + untrusted-frame injected hook stdout |
| `c9faa523d5b8` | 2026-07-12 | BR-4: Base planning/batching/verification disciplines in system.md |
| `53160a6e1200` | 2026-07-12 | BR-33: server-enforced single-turn-per-session lock |
| `e03c751612cb` | 2026-07-12 | style: cargo fmt drift in untouched files (fmt --all fallout, no behavior change) |
| `2de2d500a2a3` | 2026-07-12 | refactor(agent): extract seam methods in agent.rs (no behavior change) |
| `f89ec104c265` | 2026-07-12 | fix(clippy): resolve wave-0 clippy warnings |
| `9b468d0fbe86` | 2026-07-12 | docs: wave0 report + architectural design docs |
| `129589ba6177` | 2026-07-12 | merge: Wave 0 foundation — BR-4/20/25/26/33/34/36/38/39/46 + agent.rs seams + 6 design docs |
| `db202388f372` | 2026-07-12 | docs(campaign): Wave 0 merged (gate GREEN); Wave 1 launched across 5 cluster worktrees |
| `2e6c7a9dcb63` | 2026-07-12 | BR-5: dedup MOIM and refresh the system-prompt clock |
| `7bb223ad66f0` | 2026-07-12 | BR-15: include system/tools in cold-path token estimate + per-provider calibration |
| `ba9b85969d1b` | 2026-07-12 | BR-22: scan tool output on the main loop for injection + PII |
| `22518f7065f7` | 2026-07-12 | BR-37: reap orphaned background shell jobs across restarts |
| `ae74f29b1250` | 2026-07-12 | BR-10: keep recent-turn verbatim window at compaction |
| `12f02dccd6b5` | 2026-07-12 | BR-2: total context budget with ranking/truncation for injected blocks |
| `1e740bc438fd` | 2026-07-12 | BR-9: frame project hints/AGENTS.md as lower-trust untrusted context |
| `fc4e5ae61f40` | 2026-07-12 | BR-23: central secret-redaction boundary across all extensions |
| `38fe53f3b137` | 2026-07-12 | BR-11: head/tail-truncate an over-window message instead of dead-ending compaction |
| `5168cf5eecae` | 2026-07-12 | BR-40: structured subagent result envelope (status/tokens/artifacts) |
| `8d946378b144` | 2026-07-12 | BR-8: cap and cache eager skill-body inlining |
| `31bbbe6d1532` | 2026-07-12 | BR-43: shadow-git checkpoints + three-axis restore (Slice 1) |
| `9c1503ab6f45` | 2026-07-12 | BR-13: progressive context-overflow fallback instead of the 2-attempt cliff |
| `0717bb5b6c55` | 2026-07-12 | BR-3: per-model system-prompt variants (strong default + small-local overlay) |
| `46a67474919e` | 2026-07-12 | BR-41: persist/restore session goals + surface interrupted elicitations across daemon restart |
| `b097ee687896` | 2026-07-12 | BR-14: validate + retry compaction summary, summarize with the session model |
| `afa11aa804e3` | 2026-07-12 | BR-21: auditable command policy engine (Slice 1) atop the BR-20 floor |
| `bfaea95e5b48` | 2026-07-12 | BR-60: structured per-item todo list + living plan artifact |
| `5922840616b0` | 2026-07-12 | BR-44: persist and extend text_editor undo history |
| `299437323f4a` | 2026-07-12 | BR-42: unified active-work registry + /active_work route (jobs, subagents, schedules) |
| `3862995a1388` | 2026-07-12 | BR-65: managed/enterprise policy tier (first mergeable slice) |
| `1459b1005afc` | 2026-07-12 | BR-64: design doc for OS-level tool-execution sandbox |
| `ed573eac4edc` | 2026-07-12 | BR-12: eager background compaction between turns with synchronous fallback |
| `e4eaa7bd1f7d` | 2026-07-12 | BR-45: stable per-message ids + branch fork point (Phase 1 + diverge route) |
| `b1407965019a` | 2026-07-12 | BR-64: macOS Seatbelt sandbox for the developer shell tool (Slice 1) |
| `9bd7e1a937f6` | 2026-07-12 | BR-42: regenerate OpenAPI spec + TS client for /active_work route |
| `9066b19d3780` | 2026-07-12 | BR-17: FTS5 relevance-ranked chat recall (memory Phase 1) |
| `76dbe752498e` | 2026-07-12 | BR-45: regenerate OpenAPI spec + TS client for diverge fork-point fields |
| `1c8663830a2d` | 2026-07-12 | BR-11: fix clippy string_slice lint in truncate_middle_out test |
| `6b3303a973e4` | 2026-07-12 | chore: register compaction cluster long fns in too_many_lines baseline |
| `68cdcb936bb4` | 2026-07-12 | BR-17: fix regression - guard FTS write path when messages_fts table absent |
| `a65dc489f6f8` | 2026-07-13 | docs: wave1-security report |
| `f7e080c6a73e` | 2026-07-13 | docs: wave1-checkpoints report |
| `fda2d078b3a3` | 2026-07-13 | docs: wave1-processes report |
| `b37fe886bb4c` | 2026-07-13 | docs: wave1-compaction report |
| `86d2acd79314` | 2026-07-13 | BR-1: gitignore-aware cached workspace file map in MOIM |
| `6e101107f7aa` | 2026-07-13 | BR-60: fix regression - update prompt_manager snapshots for new todo/plan wording |
| `85c713bfc1f7` | 2026-07-13 | docs: wave1-context report |
| `950dde2c6869` | 2026-07-13 | BR-60: remove stray insta .snap.new pending files |
| `c7974c289d53` | 2026-07-13 | Merge branch 'agent-loop-checkpoints' into agent-loop-integration |
| `ee43bc0f147b` | 2026-07-13 | merge: compaction cluster (BR-10..17) — resolved vs checkpoints |
| `ea6799aaa632` | 2026-07-13 | merge: security cluster (BR-20..23,64,65) — resolved vs checkpoints+compaction |
| `c38cf9ba51a4` | 2026-07-13 | Merge branch 'agent-loop-processes' into agent-loop-integration |
| `76855c181620` | 2026-07-13 | merge: context cluster (BR-1,2,3,5,8,9,60) — union of agent struct fields |
| `d240d7a01514` | 2026-07-13 | chore: regenerate clippy too_many_lines baseline post-Wave-1 (13 entries; was stale repo-wide per all 5 cluster verifiers) |
| `70ce551e29bd` | 2026-07-13 | docs(campaign): Gate 1 — five cluster merges, conflict resolutions, schema renumber |
| `be342632d79f` | 2026-07-13 | docs(campaign): Gate 1 GREEN (2024 tests, +238, zero regressions); Wave 2 launched |
| `7b2d8a78868f` | 2026-07-13 | BR-29: staged soft-then-hard repetition stop + honest repetition reason |
| `70b0b73b3445` | 2026-07-13 | BR-27: hook matchers on tool_input content + cached compiled regexes |
| `5e70ecd68846` | 2026-07-13 | BR-6: token-aware large-response handling (head/tail preview + in-workspace handle) |
| `560649e571d7` | 2026-07-13 | BR-30: semantic near-duplicate + A/B/A/B oscillation loop detection |
| `00d90ca91890` | 2026-07-13 | BR-28: return aggregates from fire() hook events |
| `9b820410ac70` | 2026-07-13 | BR-7: externalize large tool results from content_json (message_blobs side table) |
| `b21dc5229328` | 2026-07-13 | BR-31: repeated-failing-result / no-progress detector |
| `018341651288` | 2026-07-13 | BR-19: PreToolUse input rewrite, PostToolUse block, and hook context on the tool path |
| `104802c8c34c` | 2026-07-13 | BR-52: carry the agent-computed TokenState in the event stream (kill per-token DB reads) |
| `1efb15412708` | 2026-07-13 | BR-32: periodic no-progress (stall) check for long agentic turns |
| `031fbc98b6aa` | 2026-07-13 | BR-61: wire the orphaned /interrupt soft-interrupt to the desktop client |
| `01c49a7fc921` | 2026-07-13 | BR-18: revive read-only auto-approve + per-action risk grading (SmartApprove != Approve) |
| `b24ef52150e2` | 2026-07-13 | BR-66: mistake-streak / recoverable-failure handling |
| `924a071d8426` | 2026-07-13 | BR-67: runtime observability for loop-safety events |
| `a48343c63664` | 2026-07-13 | docs: review package — REVIEW.md index + changes.html dashboard |
| `9948c5d5132c` | 2026-07-13 | BR-35: per-reply wall-clock / token / dollar budget |
| `b08f004ed4ed` | 2026-07-13 | docs: decisions signed off — defaults accepted (flags off, build BR-54, keep slices, one-branch handoff, fix frontend reds) |
| `a445269d9530` | 2026-07-13 | BR-63: richer tool-confirmation card (risk grade + call preview) |
| `bd18d8ab80cb` | 2026-07-13 | docs: cross-platform audit + design specs (BR-68 command safety, BR-69 linux/windows sandbox, BR-70 CI gate) |
| `1d8d71bfebdd` | 2026-07-13 | docs(campaign): schedule cross-platform cluster (BR-68/69/70 + GAP-2) and frontend cleanup in Wave 3 |
| `50fbf34e53c8` | 2026-07-13 | docs: wave2-loopdet report |
| `2fcd403b810e` | 2026-07-13 | docs: wave2-hooks report |
| `95464750812d` | 2026-07-13 | BR-62: reliable cancel — addressable /agent/cancel, request-scoped confirmations with TTL, cancellation-aware waits, idempotent /reply |
| `309bafc91783` | 2026-07-13 | BR-24: per-directory / per-command-prefix permission scoping |
| `30f3b1e15f73` | 2026-07-13 | BR-63: per-turn reasoning-effort control (quick / normal / deep) |
| `621274fbe5f1` | 2026-07-13 | docs: wave2-hooks report |
| `7e427992265c` | 2026-07-13 | Merge branch 'agent-loop-loopdet' into agent-loop-integration |
| `64ae8512ce0a` | 2026-07-13 | docs: wave2-server report — gate GREEN (2067 passed, +43 vs baseline, zero new failures) |
| `c2895f4fbd63` | 2026-07-13 | merge: hooks cluster (BR-18/19/24/27/28/63) — resolved vs loopdet |
| `1f54e7424f0c` | 2026-07-13 | merge: server cluster (BR-6/7/52/61/62) — resolved vs loopdet+hooks |
| `2c48eaff531d` | 2026-07-13 | fix(merge): cross-cluster integration in tests/agent.rs |
| `39591d789843` | 2026-07-13 | fix(merge): remaining cross-cluster SessionConfig/ChatRequest literals |
| `8a50d2c34962` | 2026-07-13 | fix(test): two latent isolation bugs exposed by the Wave-2 merge |
| `a54c4d7949ee` | 2026-07-13 | Merge branch 'main' into agent-loop-integration |
| `ca835e3365d5` | 2026-07-13 | docs: robustness & safety improvement proposals from agent-loop review |
| `29c1e1d4d65c` | 2026-07-13 | merge: robustness & safety proposals from agent-loop-review |
| `3d6d3aa90b47` | 2026-07-13 | GAP-2: PID-reuse guard for the Windows orphan reaper + a graceful Windows kill phase |
| `9f47941f5b0e` | 2026-07-13 | BR-40: async subagent handle |
| `3225ce5bd7b9` | 2026-07-13 | BR-62b: wire desktop GUI to reliable-cancel + idempotent /reply |
| `2cb508b8d9a7` | 2026-07-13 | BR-51: Structured tool-error taxonomy (retryable-vs-fatal classification) |
| `c6974111107a` | 2026-07-13 | BR-56: Cut per-turn history work — incremental fix_conversation, Arc-shared transcript, char-estimate compaction trigger |
| `7494a8051d9e` | 2026-07-13 | BR-49: wire structured_output validate/re-prompt loop for app output_type |
| `65fd722785dc` | 2026-07-13 | fix(frontend): green up pre-existing eslint + vitest reds |
| `8422751d8b6b` | 2026-07-13 | BR-57: Move blocking file/git I/O off the async runtime |
| `651acff0c6b4` | 2026-07-13 | BR-68: Cross-platform command safety (Windows + Linux/macOS floor + policy parity) |
| `412c3e84ddee` | 2026-07-13 | BR-58: Bound tool parallelism and add write-side path ordering |
| `0f35be34006c` | 2026-07-13 | BR-47: auto post-edit diagnostics (LSP/analyze) feedback loop |
| `23c8db8cedfd` | 2026-07-13 | docs: wave3-polish report |
| `d45dfe65d03c` | 2026-07-13 | BR-50: config-gated self-critique/reflection pass on ordinary answers |
| `acb7de744841` | 2026-07-13 | BR-59: cache tree-sitter Query per (language, kind) instead of recompiling per file |
| `c6686a7446a1` | 2026-07-13 | BR-59: cache the Knowledge BM25 index instead of rebuilding it per search |
| `2d16ff0afd72` | 2026-07-13 | BR-69: Cross-platform shell sandbox behind one trait (macOS Seatbelt unchanged, Linux Landlock+seccomp, honest Windows) |
| `5d2c317989f6` | 2026-07-13 | BR-55: run first-run skills/Soul install in background, unblocking startup |
| `ab7217802fdd` | 2026-07-13 | BR-70: cross-platform CI verification gate (one cross recipe, check-cross, Rust CI) |
| `1991637e951b` | 2026-07-13 | BR-48: config-gated done-ness gate for interactive chat (SuccessCheck variants + goal-style loop) |
| `b25d63247e6f` | 2026-07-13 | BR-48: regenerate OpenAPI + TS client for new SuccessCheck variants |
| `18ff6011c1b3` | 2026-07-13 | BR-53a: coalesce streamed SSE text deltas into one frame per window |
| `60ace574457a` | 2026-07-13 | BR-53b: skip the whole-conversation tool-response scan for text-only messages |
| `2467c53692ca` | 2026-07-13 | BR-48: fix regression — clippy cloned_ref_to_slice_refs in done_gate test |
| `ba2100b0066a` | 2026-07-13 | BR-68: fix regression — clippy string_slice + dead_code in policy tokenizers |
| `715f5846b317` | 2026-07-13 | BR-53c: defer TUI streaming re-render to a ~60fps frame instead of per-token |
| `9abff2f36f36` | 2026-07-13 | BR-54: one biorouterd daemon per app, shared across windows (Slice A) |
| `3504525269ec` | 2026-07-13 | docs: wave3-xplat report |
| `9c9856e7321f` | 2026-07-13 | BR-54: SharedMcpPool — share MCP processes across sessions with per-dispatch notification isolation (Slice B, flag-gated) |
| `3152b7b89ce3` | 2026-07-13 | Merge branch 'agent-loop-xplat' into agent-loop-integration |
| `307f80803c82` | 2026-07-13 | Merge branch 'agent-loop-polish' into agent-loop-integration |
| `e41a635a0af0` | 2026-07-13 | Merge branch 'agent-loop-verify' into agent-loop-integration |
| `d08def9c881d` | 2026-07-13 | Merge branch 'agent-loop-perf' into agent-loop-integration |
| `a421ac8b41dc` | 2026-07-13 | Gate 3: BR-47 post-edit diagnostics default-OFF + test fix |
| `c4b51e045cb5` | 2026-07-13 | docs: final campaign report — 70 items, 1786->2552 tests, gates + cross-platform status |

### PR #12 — Apps SDK v2 (15 commits)

| Commit | Date | Subject |
|---|---|---|
| `5d29afdc4e5d` | 2026-07-12 | SDK v2 Phase 1: shared state doc, WS auth, bindings, typed surface |
| `e8d347f41478` | 2026-07-12 | SDK v2 Phase 2: ID-keyed catalog, ui_patch + morphing, science pack, custom components |
| `e730eccf4065` | 2026-07-12 | SDK v2 Phase 3 + Phase 5 aesthetics + br.kb client: typed RPC, signals, themes, starters |
| `49b23c665487` | 2026-07-12 | SDK v2 Phase 4 platform APIs + Phase 6 export/CLI/frontend/presence wave |
| `99a1de3245a7` | 2026-07-12 | SDK v2 Phase 4b multi-agent profiles + docs + benchmark v2 + SDK facade |
| `f14a0da6bfed` | 2026-07-12 | SDK v2 final hardening: strict CSP, autorun, ui_error repair loop, ui_suggest |
| `e10fa3b49637` | 2026-07-12 | docs: 100 agentic-app test specs for Agent Drafter (Apps SDK v2) |
| `a443f2c419c9` | 2026-07-12 | docs: guide for a coding agent to test-drive Agent Drafter across the 100 specs |
| `273821590657` | 2026-07-13 | docs: Agent Drafter 100-app test drive — evidence, harness, and remediation plan |
| `ae8987a6eedc` | 2026-07-13 | Agent Drafter remediation, Waves 0-3.1: make the contract enforceable |
| `7527f8485789` | 2026-07-13 | Agent Drafter remediation, Wave 3 + turn guard: enforce the agentic contract |
| `d8cf95cca6e1` | 2026-07-13 | Agent Drafter remediation, Waves 4-5: failure becomes visible; drag becomes reachable |
| `c03ad9e3364f` | 2026-07-13 | Re-lint the real test-drive corpus; record the remediation results |
| `6310341e9acf` | 2026-07-13 | Repair a broken test-drive app with the fixed platform's own agent; fix 2 bugs it found |
| `370c478a8cc2` | 2026-07-13 | Wave 6: app-smoke.mjs — lint that RUNS the app |

### PR #13 — Usage reporting (11 commits)

| Commit | Date | Subject |
|---|---|---|
| `9177a1af7d0e` | 2026-07-13 | feat(usage): make accumulated (billed) tokens the per-conversation headline (#1) |
| `932d30bcd5ad` | 2026-07-13 | fix(usage): don't let store-coalesced zero counters mask the billed total (#1) |
| `48c8fca4a2e9` | 2026-07-13 | feat(usage): per-turn model attribution + per-model usage API (#1) |
| `45ecdd16c068` | 2026-07-13 | feat(usage): real per-model cost breakdown in the cost popover (#1) |
| `0b49b2cd1a22` | 2026-07-13 | feat(usage): server-side usage report + MTD summary with priced buckets (#1) |
| `3cd0ccb953a5` | 2026-07-13 | feat(usage): biorouter usage CLI command (#1) |
| `6a31592097a6` | 2026-07-13 | feat(usage): Usage panel in Settings (per-day bars, per-model table, MTD gauge) (#1) |
| `9cf935091b6c` | 2026-07-13 | feat(usage): capture cache tokens in Usage + provider parsers (#1) |
| `a737708d9b03` | 2026-07-13 | feat(usage): cache-aware pricing for Claude and Claude-on-Bedrock (#1) |
| `7c5ba735a531` | 2026-07-13 | feat(usage): persist and aggregate per-turn cache tokens (#1) |
| `269db0ee3d5a` | 2026-07-13 | feat(usage): surface cache tokens in routes, CLI, panel + diagnostics (#1) |

## Provenance of this report

Prepared from the initial read-only repository/GitHub audit and the subsequent user-authorized integration, qualification, and merge execution. Historical snapshots are labeled as such; final source and merge anchors take precedence. Branches, tag lineages, and audit refs were preserved. UI judgments and executed treatments are grounded in the BioRouter `design.md` specification.

## Related documentation

- [Agent-loop campaign record](../agent-loop-campaign/README.md) — the 70-item BR-numbered campaign whose 150 commits became PR #11, the largest branch merged here.
- [Documentation index](../../README.md) — the top-level map of BioRouter's documentation, including the other historical records.



