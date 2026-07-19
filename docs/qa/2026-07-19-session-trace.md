# Session trace — 2026-07-18/19 latency → streaming → QA campaign

Full traceability of user instructions, tests, findings, and fixes. Detail docs:
`docs/investigations/2026-07-18-tool-call-ui-latency.md`,
`docs/perf/2026-07-18-implementation-status.md` (§9–11),
`docs/qa/2026-07-19-qa-round1.md`, `-round2.md`, `-tool-errors-audit.md`,
`docs/tool-routing.md`. Branch: `feat/streaming-tool-call-ui` (pushed continuously).

## User instructions, in order given
1. Investigate why tool calls appear in the UI nearly-finished and why there is a
   dead gap between tool calls; fix if programmatic, explain if intrinsic; design
   a trailing thinking indicator with an elapsed timer.
2. Implement both tracks; keep the user posted.
3. Also check Versa Bedrock (all Anthropic models); launch dev app for
   credential-based live testing.
4. Live-test both providers for regressions (streaming, tool calls).
5. Investigate + fix "Maximum update depth exceeded" (found pre-existing; fixed).
6. Test the other model (versa_azure) — clean; exposed the Bedrock
   one-bubble-per-token regression (ours; fixed).
7. Leave Bedrock streaming enabled despite gateway buffering (option a).
8. Show the deferred-approval docs; investigate + fix duplicate submission on
   tab close (reproduced, root-caused `consumePending` never dispatched, fixed).
9. Fix the Home-surface silent message loss (root-caused: resumeSessionId never
   in the query string; fixed at the navigation choke point).
10. Approve deferred items §6.1b/6.2b/6.2c/6.2d + human review question →
    implemented all four with kill switches.
11. "Just read — looks good": merge to main, push to GitHub, resolving conflicts.
12. /goal QA campaign: close/recompile, drive all UI, fix Recent Chats, varied
    chat tasks, tab stress, visualizations, creative probes, ≥3 fix-and-retest
    rounds, final per-round report.
13. Preview must show files the task touched (e.g. /tmp) — fixed containment.
14. Preview panel only where it should; Fully-Automatic mode = broad file
    autonomy with approval-gated sensitive ops (incl. preview parity).
15. Tool taxonomy: when developer vs code_execution; document routing in
    prompts; propose deprecations for overlaps.
16. BioOKF rebuild via multi-turn driving, parallel sessions, tab switching,
    compaction stress; hotfix loop.
17. Commit + document continuously; fully revertible.
18. Markdown preview: render images/hyperlinks/all GFM correctly.
19. Tool-call error "outside the working directory" during BioOKF build:
    audit ALL tool logs (add logging if missing), classify intended-vs-defect,
    fix defects, repeat build until zero unintended failures.
20. Preview STILL failing on /tmp/biookf-rebuild files → dedicated follow-up
    (in flight); plus this trace document.

## Tests asked vs run
Every item in 12/16/19 above was driven live except (recorded honestly in the
round logs): backend-kill mid-turn (destructive to shared dev), 3 simultaneous
paid streaming turns (budget), sub-800px widths (Electron clamp), full-content
BioOKF transcription (scoped to scaffold+core-content; disclosed). Unit/suite
coverage after every change: cargo biorouter ~1454, biorouter-mcp ~807,
frontend 1348→1372+, tsc, lint (fully green after dropping the stale disable).

## Findings → fixes (commit-traceable)
- Non-streaming providers (versa_azure/bedrock/azure + 11 more) → streaming
  implemented: 032c938c, 7385be3c/81294947; thinking blocks bcdbe53d;
  instrumentation a43ba578; MCP mutex 63b7e493 (todo_extension race found+fixed).
- Frontend status truth + trailing indicator: 1c681c8e.
- Pre-existing bugs fixed: update-depth loop d789c6ab; tab-close duplicate
  f1f1d6b6; Home message loss 6c261665. Bedrock per-token bubbles (ours)
  d65f7103. B4 "activation resubmit": bisected NOT a bug; guard fb47f544.
- Deferred perf items: 77a7564d (pending tool events, safety-gated),
  8e20f6cc (batching), ae740027 (per-tool emission), e06a3b43 (SecretGuard
  cache), CI repairs 478925c2. Merge to main 3e487752 → pushed 78471bdc.
- QA R1: Recents URL/highlight desync 185578cc; isError camelCase (failed
  calls rendered green) dfa6dc32.
- Preview containment (tmp + symlinks) dc66324b; auto-open pure-function
  84296bb9; full-auto policy + sensitive-ops approval + preview parity
  1079f909; tool routing/prompts b925d72a; markdown fidelity 3db5d420.
- In flight at time of writing: biookf-loop (working-dir gate de-defect +
  logging + repeat-until-clean), preview follow-up, QA round 2 completion,
  round 3, final report.

## Process notes for reproducibility
All fixes carry revert-proven gates unless explicitly recorded otherwise
(three vacuous tests were caught and disclosed during the session). Live-GUI
verification backed every claim that unit tests could not reach. Known
environment hazards recorded: shared-worktree collisions (other sessions),
cargo target-dir contention, DOM-lies-across-tabs, driver focus races.
