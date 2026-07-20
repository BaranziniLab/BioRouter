# Master checklist — everything requested this session

> **What this is.** The single, exhaustive checklist of every instruction, bug report
> and test directive the user gave across the 2026-07-18/20 session, in the order it
> was raised, with a checkbox and the evidence for each. It supersedes nothing — the
> [work register](user-requested-work-register.md) stays as the narrative table; this
> is the tick-list you work down.
> **Status:** Current — updated as items land.
> **Audience:** the user checking progress, and whoever picks this up next.

Work order is **top to bottom**. Every fix is followed by a stress/regression pass
before the next item starts. `[x]` means fixed *and* verified; `[~]` means an agent
is working it now; `[ ]` means not started; `[?]` means it needs a user decision.

## A · Original latency investigation and both tracks

- [x] **A1** Investigate why tool calls appear only when nearly finished, and why there is a dead gap between them — programmatic vs intrinsic
- [x] **A2** Fix what is programmatic (streaming for 14 non-streaming providers)
- [x] **A3** Design and build the trailing "thinking" indicator with an increasing elapsed timer
- [x] **A4** Check Versa Bedrock for all Anthropic models — `81294947` ConverseStream
- [x] **A5** Launch the dev app for credentialed live testing
- [x] **A6** Live-test both providers for regressions (streaming, tool calls)

## B · Bugs found and fixed along the way

- [x] **B1** "Maximum update depth exceeded" crash — `d789c6ab` (pre-existing)
- [x] **B2** Bedrock rendering one bubble per token — `d65f7103` shared message id
- [x] **B3** Duplicate submission when closing tabs — `f1f1d6b6` `consumePending`
- [x] **B4** Home-surface submit silently losing the message — `6c261665`
- [x] **B5** "Failed to Load Session" fatal card on a transient send failure — `87a5744d`
- [x] **B6** Preview could not open files the task wrote (`/tmp`) — `dc66324b`, `f8f1505f`
- [x] **B7** Markdown preview: images, hyperlinks, full GFM — `3db5d420`
- [x] **B8** "outside the working directory" tool error — `90bc2acf`
- [x] **B9** Recurring `code_execution` "Module could not be found" — `101c7166`

## C · Deferred performance items (approved)

- [x] **C1** §6.1b stream pending tool-call events — `77a7564d`
- [x] **C2** §6.2b batch `tool_use` blocks — `8e20f6cc`
- [x] **C3** §6.2c per-tool response emission — `ae740027`
- [x] **C4** §6.2d SecretGuard cache — `e06a3b43`

## D · Policy and architecture directives

- [x] **D1** Preview panel appears only where it should — `84296bb9`
- [x] **D2** Fully-Automatic mode: broad file access, approval only for sensitive ops — `1079f909`
- [x] **D3** Close the sensitive-op hole (writes hidden in shell/`execute_code`) — `1e8fea2e`
- [x] **D4** Fix the resulting false positive on angle-bracket prose — `7bca4b5e`
- [x] **D5** Tool-routing taxonomy: when `developer` vs `code_execution` — `b925d72a`
- [x] **D6** Document routing in the system and extension prompts — `b925d72a`
- [?] **D7** Deprecation proposal for overlapping tools — **awaiting your decision**
- [?] **D8** Human security review: R2-01 dynamic-path residual + MCP mutex — **awaiting your review**

## E · The BioOKF build exercise

- [x] **E1** Recreate the repo by driving BioRouter with sets of instructions
- [x] **E2** Multiple parallel sessions, switching tabs between them
- [x] **E3** Observe compaction behaviour — triggered in round 3, nothing degraded
- [x] **E4** Audit tool-call logs; add logging where missing — `eb0eadb0`
- [x] **E5** Classify every tool error intended vs defect
- [x] **E6** Repeat until the build completes with zero unintended failures — 3 iterations, clean

## F · Merges, documentation and traceability

- [x] **F1** Merge everything back to main and push
- [x] **F2** List all branches; identify what is not superseded by main
- [x] **F3** Merge both remaining branches, resolving conflicts
- [x] **F4** Produce documentation structure/formulation instructions
- [x] **F5** Conform all campaign docs to that structure and nomenclature
- [x] **F6** Commit and document continuously so everything is revertible
- [x] **F7** Write the full session trace (instructions → tests → findings → fixes)
- [x] **F8** Clean loose files out of the docs root — 27 files audited, none lost

## G · QA campaign (three rounds, as directed)

- [x] **G1** Close all dev instances and relaunch a fresh build
- [x] **G2** Drive every button and UI element
- [x] **G3** Fix Recent Chats misbehaviour — `185578cc`
- [x] **G4** Varied chat tasks: web search, write a program, file operations, multi-step
- [x] **G5** Tabs: conversations across many tabs, open/close stress
- [x] **G6** Generate visualizations and view them in the preview panel
- [x] **G7** Creative and edge-case probes
- [x] **G8** Document abnormalities, fix, retest — three rounds
- [x] **G9** Failed tool calls rendering as green successes — `dfa6dc32`
- [x] **G10** Per-round final report

## H · Current wave

- [x] **H1** Remove the recents count badge — `211668be`
- [x] **H2** Alma Mater and Roche Limit: white canvas, grey sidebar; **Alma dark unchanged**, Parchment unchanged — `4d308c49` (new `--background-canvas` token; all six combos sampled live)
- [~] **H3** Terminal: open every type, split every way, per-session terminals
- [~] **H4** Terminal working directory always matches its session's working directory
- [~] **H5** Terminal stress: busy terminals while launching more, open/close churn, many at once under narrow splits
- [~] **H6** Terminal cap of 8 is too low **and fires when few are open** (suspected slot leak)
- [x] **H7** Boot animation: logo must not shift between the two states — `30d615f3`, vision-verified
- [x] **H8** Soft interrupt must acknowledge immediately — `aaf24a22`
- [x] **H9** Stop-and-Send needs a smooth confirming animation — `aaf24a22`
- [~] **H10** Sending while scrolled up must smoothly return to the bottom
- [~] **H11** GitHub issue #19 — verify, harden, close if fixed

## J · Surfaced by the fixes — need your decision

- [?] **J1** Real contrast failures the old gate was hiding, now measured against the true canvas: Parchment light `text-accent` **4.35:1** (below AA 4.5); Parchment light `border-subtle` 1.23:1; Parchment dark `border-subtle` **1.00:1** (invisible); Alma dark `border-subtle` 1.24:1. All **pre-existing**, none introduced. Fixing them means changing Parchment and Alma dark, which you pinned as must-not-change — so this is your call.

## I · Remaining, in work order

- [ ] **I1** Parallel execution: agent running several tools/code at once — stress and edge cases
- [ ] **I2** Subagents: edge cases, stress, and traceability of each subagent in the chat
- [ ] **I3** Full regression + stress pass over everything above
- [ ] **I4** Fast-forward `main`, push, and deliver the final summary

## Related documentation

- [User-requested work register](user-requested-work-register.md) — the narrative table with evidence per item.
- [Campaign final report](campaign-final-report.md) — per-round findings and fixes.
- [Session trace](session-trace.md) — every instruction mapped to its commit.
