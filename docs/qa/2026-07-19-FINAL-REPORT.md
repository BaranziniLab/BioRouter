# Final report — streaming latency track + QA campaign (2026-07-18/19)

Branch `feat/streaming-tool-call-ui`, in sync with origin. Companion docs:
`docs/qa/2026-07-19-session-trace.md` (every instruction→fix→sha),
`docs/qa/2026-07-19-qa-round{1,2,3}.md`, `-tool-errors-audit.md`,
`docs/tool-routing.md`, `docs/investigations/2026-07-18-tool-call-ui-latency.md`,
`docs/perf/2026-07-18-implementation-status.md`.

## Final gate status (Round-3 regression, all green)
- `cargo test -p biorouter --lib` — 1476 passed, 0 failed
- `cargo test -p biorouter-mcp --lib` — 809 passed, 2 ignored, 0 failed
- `cargo test --test mcp_integration_test` — 4 passed
- `cargo fmt --check` — pass · `./scripts/clippy-lint.sh` — pass
- `npm run test:run` ×2 — 1406 passed (161 files), both runs
- `tsc --noEmit` — pass · `lint:check` — pass

## Problems found and fixed, per round

### Pre-QA (streaming latency track — already merged to main once at 78471bdc, then extended)
- 14 providers (versa_azure/bedrock/azure + CLI shims) never streamed → streaming
  implemented; **root cause of the original "tool card appears already finished"**.
  `032c938c`, `81294947`/`7385be3c`, `bcdbe53d` (thinking), `a43ba578` (instrumentation).
- MCP client mutex serialized concurrent tool calls; removal found+fixed a real
  `todo_extension` read-modify-write race. `63b7e493`.
- Trailing thinking indicator + honest tool-card status. `1c681c8e`.
- Pre-existing bugs fixed: update-depth crash `d789c6ab`; tab-close duplicate
  submission `f1f1d6b6`; Home silent message loss `6c261665`. Ours: Bedrock
  per-token bubbles `d65f7103`. "B4 activation resubmit" bisected NOT-a-bug, guard `fb47f544`.
- Deferred perf items (all kill-switched): pending tool events `77a7564d`,
  batching `8e20f6cc`, per-tool emission `ae740027`, SecretGuard cache `e06a3b43`,
  CI repairs `478925c2`.

### Round 1
- **R1-01** Recents active-highlight + URL desynced on empty tabs (this WAS the
  "Recent Chats misbehaving" report — open/dedupe was correct). Fixed `185578cc`.
- **R1-02 (major)** rmcp serializes `isError` camelCase but the UI read snake_case,
  so **failed tool calls rendered as green successes**. Fixed `dfa6dc32`.
- 12 areas passed clean. Cancel-card now reads "Stopped", not "Finished".

### Directives (between R1 and R2)
- Preview `/tmp` + symlink containment `dc66324b`; preview auto-open pure fn `84296bb9`;
  **Fully-Automatic policy: broad file access + approval only for sensitive ops,
  preview parity** `1079f909`; tool-routing tiers in prompts + `docs/tool-routing.md` `b925d72a`;
  markdown preview images/links/GFM `3db5d420`.

### Round 2
- **R2-01 (BLOCKER)** Auto mode **silently wrote to `~/.ssh/config` with no approval** —
  the sensitive-op gate only saw file-editor path args, but file ops hide inside
  `execute_code`/shell. Fixed `1e8fea2e` (scan shell lines + execute_code bodies).
- Preview couldn't see files written inside `execute_code` (collector only matched
  top-level tool names) `f8f1505f`; working-dir jail relaxed in Auto `90bc2acf`;
  structured per-tool-result logging + error audit `eb0eadb0`.
- **Send-path hardening**: a daemon blip on send fell through to the fatal
  "Failed to Load Session" card; now an inline retryable error, transcript intact `87a5744d`.
- BioOKF build: 100% of assigned slice (45/45 files, exact path fidelity);
  self-found+fixed a planted schema inconsistency.

### BioOKF repeat-until-clean loop (3 iterations)
- Made the developer working-dir jail mode-aware (was unconditional; contradicted
  Auto policy) `90bc2acf`.
- **Caught the R2-01 fix's own over-correction** — a false-positive reading `<type>`
  prose as a shell redirect — `7bca4b5e`.
- Iteration 3: full slice built with **zero unintended tool failures**. Remaining
  errors all correctly INTENDED (model typos, real sensitive-op gating).

### Round 3 (honest gaps + regression sweep)
- **Compaction TRIGGERED** (never fired in R1/R2 — needs ~840K tokens at default
  0.8; forced via low threshold). **Nothing degraded**: transcript intact, all 8
  tool cards survived, facts recalled from summary, no repeat/contradiction, 0 errors.
- **Concurrent cross-tab send: correct** — R2-02 was a harness artifact, not a bug.
- **R2-01 re-verified live through the execute_code wrapper**: approval fires,
  Deny blocks the write, ordinary writes don't prompt, angle-prose doesn't false-prompt.
- **Send hardening verified live**: daemon kill → inline card + Retry recovers.
- **R3-01 (fixed `7b26f977`)** rapid double-click Send left a phantom duplicate
  bubble (renderer-only; backend already deduped). Synchronous in-flight latch.
- Pre-existing `fmt` violation from R2-01 work fixed `1a4b5058`.

## Deferred / documented — NOT fixed (need your call)
- **R3-02** "Provider not set" card has no Retry after an *out-of-band* daemon
  restart (harness artifact; in-app own-daemon path already recovers). Follow-up feature.
- **R3-03** double summary block only under pathological compaction threshold; not
  reachable at defaults. Tuning, not a bug.
- **R3-04** preview of a nonexistent path leaks raw `ENOENT`. Cosmetic; map to a
  friendly empty-state in a contained follow-up.
- **R3-05** "New Session" opens a tab per click (no blank-tab reuse). UX policy.
- **R2-06** routing guidance says "call developer/shell directly" but code_execution
  mode strips developer tools from the set — guidance is aspirational there.

## Deprecation proposal — AWAITING YOUR APPROVAL (nothing removed)
Per `docs/tool-routing.md`:
1. `computercontroller/automation_script` shell-mode — near-total overlap with `developer/shell`.
2. Three URL-fetch surfaces (`web_scrape` / shell curl / execute_code fetch) — designate `web_scrape` canonical.
3. `files_server`/`compute_server` — rename/redescribe so they aren't picked for the user's own workspace.

## Security review checklist (HOWTOAI.md: MCP/concurrency/security need human review)
- **R2-01 residual**: a *dynamically-constructed* sensitive path inside a script
  (`` `${dir}/config` ``) is not statically resolvable by the shell/execute_code
  scanner. Deeper fix = gate the code-execution extension's INNER dispatch boundary
  (that layer can't surface an interactive ask, so it would deny). Documented in
  `sensitive_ops.rs` module docs.
- Sensitive-path list completeness (`SENSITIVE_HOME_SUBPATHS`, `SENSITIVE_ABSOLUTE_PREFIXES`):
  `~/.netrc`, `~/.pgpass`, shell rc files deliberately excluded to avoid friction — confirm.
- MCP mutex removal `63b7e493`: the 4 other in-process trait impls + progress-token
  routing under concurrency; `BIOROUTER_TOOL_MAX_CONCURRENT=1` is the rollback.
- Backend path classification is lexical (no realpath); preview is realpath-canonical — asymmetry documented.

## What was NOT driven (honest)
Backend-kill-of-shared-daemon (destructive to concurrent runs), 3 simultaneous
paid streaming turns, sub-800px widths (Electron clamp), full 262-file BioOKF
transcription (scoped to scaffold+core), model-switch via composer chip (switching
is via Settings), 2k-word single-line paste (driver readline limit).

## Process honesty
Three vacuous tests were written and **caught by revert-proof discipline** before
shipping (ProgressiveMessageList depth test, two reducer tests) — recorded, not hidden.
Shared-worktree collisions, cargo target contention, and DOM-lies-across-tabs were
recurring environment hazards, worked around and documented. Transient Anthropic
529/500 errors killed Round 3's first attempt; re-launched cleanly (no partial state).

## State
`feat/streaming-tool-call-ui` fully committed + pushed. `main` has the streaming
track (78471bdc) but NOT the QA-campaign fixes — those await your merge decision.
