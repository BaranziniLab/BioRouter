# Streaming tool-call UI, July 2026

This folder is the complete record of one campaign that ran on 2026-07-18 and 2026-07-19 on the
branch `feat/streaming-tool-call-ui`. It began as an investigation into two user-reported
symptoms — a tool card that appeared late and already looked finished, and a two-to-five second
dead gap between consecutive tool calls — and ended as a three-round QA campaign over the fixes
that investigation produced. The work shipped: the streaming track merged to `main` at
`78471bdc`, and the QA-campaign fixes were held for a separate merge decision.

The campaign is worth reading for two things beyond the fixes themselves. The root cause of the
headline symptom was not a rendering bug but the fact that **fourteen providers never streamed at
all**, so the "tool call appears already finished" complaint was literally accurate. And the QA
rounds turned up a security blocker — Fully-Automatic mode writing to `~/.ssh/config` with no
approval, because the sensitive-operation gate only inspected file-editor path arguments while
file operations hide inside `execute_code` and shell bodies.

Read it to trace why a provider streams the way it does, to find the commit behind a fix, or to
decode an `R1-NN` / `R2-NN` / `R3-NN` finding identifier cited in a commit message. Do not read it
for current behaviour: the living account of which tool the agent should reach for is
[tool routing](../../agent-loop/tool-routing.md), which was written during this campaign and
stays maintained.

## Finding identifiers

Findings are numbered by the round that discovered them — `R1-01`, `R2-01`, `R3-05` — and the
`BIOOKF-I<n>-DEFECT-<n>` scheme covers the defects found by the BioOKF repeat-until-clean loop
inside round 2. Every identifier is defined at the point it is raised in the round report that
owns it; the [campaign final report](campaign-final-report.md) collects the ones that survived
to the end, including the five that were deliberately left unfixed.

## Documents

| Document | What it covers |
|---|---|
| [Campaign final report](campaign-final-report.md) | The closing summary: per-round problems found and fixed, the final gate status, the five deferred items that were documented rather than fixed, a tool deprecation proposal awaiting approval, and the security review checklist. Start here. |
| [Session trace](session-trace.md) | Every user instruction in the order it was given, mapped to the tests run and the commit that answered it. The traceability spine for the whole campaign. |
| [Tool-call UI latency investigation](tool-call-ui-latency-investigation.md) | The founding investigation: how a tool call flows end to end, why it appears late, why there is a gap between calls, what was ruled out, and the two tracks of work proposed — performance fixes and a trailing thinking indicator. Carries the critical addendum that the reporting user's provider did not stream at all. |
| [Latency measurement register](latency-measurement-register.md) | The before/after register the investigation required of itself: every fix had to carry a number or count as not landed. Several entries honestly record `NOT MEASURED` because no live endpoint was available. |
| [Streaming implementation status](streaming-implementation-status.md) | What actually landed against the investigation, split into verified-by-test and asserted-but-unmeasured, plus the live smoke-test results against real UCSF credentials and two bugs that only live GUI testing caught. |
| [QA round 1 results](qa-round-1-results.md) | The first sweep over the merged tracks: the named "Recent Chats" bug, twelve areas that passed clean, and the major finding that failed tool calls rendered as green successes because `isError` was read in the wrong case. |
| [QA round 2 results](qa-round-2-results.md) | Directive verification, the BioOKF parallel-build orchestration, and the security blocker `R2-01`. Also holds the send-path hardening work and the three iterations of the repeat-until-clean build loop. |
| [QA round 3 results](qa-round-3-results.md) | The closing round: compaction finally triggered and nothing degraded, concurrent cross-tab send proved correct, `R2-01` re-verified live through the `execute_code` wrapper, and the full regression sweep. |
| [Tool-errors audit](tool-errors-audit.md) | Every tool error surfaced during rounds 1 and 2, swept from the daemon logs and classified `INTENDED` against `DEFECT`. Six of the eleven turned out to be one defect: a mode-blind path jail. |

## Related documentation

- [Tool routing](../../agent-loop/tool-routing.md) — the living guidance this campaign wrote, and the document the deprecation proposal in the final report belongs to.
- [The agent loop](../../agent-loop/README.md) — the live documentation for the loop whose tool dispatch and streaming decoders this campaign changed.
- [Performance, June 2026](../performance-2026-06/review-findings.md) — the earlier whole-app latency review, which this campaign's investigation builds on rather than repeats.
- [GUI QA, June 2026](../gui-qa-2026-06/debug-session-issue-tracker.md) — the previous QA campaign against the desktop app, for comparison of method.
- [Historical records](../README.md) — the archive index this campaign belongs to.
