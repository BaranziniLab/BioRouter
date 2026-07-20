# Final report — streaming latency track and QA campaign

> **What this is.** The closing report of the 2026-07-18/19 streaming tool-call campaign: what
> was found and fixed in each round, what was deliberately left unfixed, and what still needs a
> decision.
> **Status:** Historical record (completed 2026-07-19).
> **Audience:** maintainers deciding what to merge, and anyone tracing a fix back to its round.

The campaign ran on branch `feat/streaming-tool-call-ui` in two phases: a streaming latency track
that merged to `main`, then a three-round QA campaign over the result. Findings are numbered by
the round that raised them (`R1-01`, `R2-01`, `R3-05`); each round report defines its own. This
report collects the ones that survived to the end.

Companion documents: the [session trace](session-trace.md) (every instruction, fix and commit
SHA), [rounds 1](qa-round-1-results.md), [2](qa-round-2-results.md) and
[3](qa-round-3-results.md), the [tool-errors audit](tool-errors-audit.md),
[tool routing](../../agent-loop/tool-routing.md), the
[latency investigation](tool-call-ui-latency-investigation.md), and the
[implementation status](streaming-implementation-status.md).

## Final gate status (round-3 regression, all green)
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
  preview parity** `1079f909`; tool-routing tiers in prompts + [tool routing](../../agent-loop/tool-routing.md) `b925d72a`;
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

## Deferred and documented — not fixed, awaiting a decision
- **R3-02** "Provider not set" card has no Retry after an *out-of-band* daemon
  restart (harness artifact; in-app own-daemon path already recovers). Follow-up feature.
- **R3-03** double summary block only under pathological compaction threshold; not
  reachable at defaults. Tuning, not a bug.
- **R3-04** preview of a nonexistent path leaks raw `ENOENT`. Cosmetic; map to a
  friendly empty-state in a contained follow-up.
- **R3-05** "New Session" opens a tab per click (no blank-tab reuse). UX policy.
- **R2-06** routing guidance says "call developer/shell directly" but code_execution
  mode strips developer tools from the set — guidance is aspirational there.

## Deprecation proposal — awaiting approval, nothing removed

Per [tool routing](../../agent-loop/tool-routing.md):
1. `computercontroller/automation_script` shell-mode — near-total overlap with `developer/shell`.
2. Three URL-fetch surfaces (`web_scrape` / shell curl / execute_code fetch) — designate `web_scrape` canonical.
3. `files_server`/`compute_server` — rename/redescribe so they aren't picked for the user's own workspace.

## Security review checklist

Per `HOWTOAI.md`, MCP, concurrency and security logic all need human review.

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

## What was not driven

Backend-kill-of-shared-daemon (destructive to concurrent runs), 3 simultaneous
paid streaming turns, sub-800px widths (Electron clamp), full 262-file BioOKF
transcription (scoped to scaffold+core), model-switch via composer chip (switching
is via Settings), 2k-word single-line paste (driver readline limit).

## Process honesty — what went wrong in the running of the campaign
Three vacuous tests were written and **caught by revert-proof discipline** before
shipping (ProgressiveMessageList depth test, two reducer tests) — recorded, not hidden.
Shared-worktree collisions, cargo target contention, and DOM-lies-across-tabs were
recurring environment hazards, worked around and documented. Transient Anthropic
529/500 errors killed Round 3's first attempt; re-launched cleanly (no partial state).

## Branch state at close

`feat/streaming-tool-call-ui` fully committed + pushed. `main` has the streaming
track (78471bdc) but NOT the QA-campaign fixes — those await your merge decision.

## Post-merge verification — documentation-reorganization integration (2026-07-19)

After this campaign closed, its branch was merged with a large documentation
reorganization on `integrate/docs-cleanup` and re-verified before fast-forwarding
`main`. The merge resolved three source-file conflicts in the theme engine and
repointed inbound documentation links in Rust and TypeScript comments, so the
theme system was exercised in the running app rather than by tests alone.

### What the merge actually changed in source

The three conflicted files are byte-identical to `main` except for two comment
lines in `ui/desktop/src/styles/codeTheme.ts`, which repoint moved design docs.
`ui/desktop/src/styles/main.css` and `ui/desktop/src/components/InAppTerminalDock.tsx`
carry no net diff against `main` at all.

```bash
git diff main...HEAD -- ui/desktop/src/styles/main.css \
  ui/desktop/src/components/InAppTerminalDock.tsx   # empty
```

All 11 distinct documentation paths introduced into source comments resolve to
files that exist, as do every `docs/*.md` reference in `.github/`, `scripts/`,
`Justfile`, `CLAUDE.md` and `.claude/`.

### Gate results

Every gate matches the round-3 baseline recorded above, with no count drift.

- `cargo check --workspace` — 0 warnings, 0 errors
- `cargo test -p biorouter --lib` — `test result: ok. 1476 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`
- `cargo test -p biorouter-mcp --lib` — `test result: ok. 809 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out`
- `cargo test --test mcp_integration_test` — `test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`
- `cargo fmt --check` — pass (no output) · `./scripts/clippy-lint.sh` — pass, no banned TLS crates
- `npm run test:run` ×2 — 1406 passed (161 files), both runs, exit 0 both times
- `tsc --noEmit` — pass · `npm run lint:check` — pass

The theme gates inside `lint:check` are the load-bearing ones for this merge:
`generate-themes.mjs --check` reports generated artifacts current for 3 themes,
and `check-contrast.mjs` reports `OK — all 228 contrast assertions pass`.

None of the known pre-existing frontend flakes (`App.test.tsx`,
`ConfirmationModal`, `ResetPanel`, `InstructionsEditor`, `ProgressiveMessageList`)
appeared in either run.

### Theme sweep in the running app

A debug build was staged with `just copy-binary debug` and driven through the
Playwright GUI driver. All six family/mode combinations were applied through
Settings → App → Theme and screenshotted. Each produces distinct, family-correct
tokens with no unstyled or black-on-black surface:

| Family | Mode | Body background | Body foreground |
|--------|------|-----------------|-----------------|
| Parchment | light | `#ffffff` | `#2a2520` |
| Parchment | dark | `#0d0a06` | `#f4f0e6` |
| Alma Mater | light | `#ffffff` | `#052049` |
| Alma Mater | dark | `#04142e` | `#f2f3f4` |
| Roche Limit | light | `#ffffff` | `#1f1e1c` |
| Roche Limit | dark | `#131312` | `#ededea` |

The Roche Limit dark pair matches the value `check-contrast.mjs` audits at
15.85:1, so the stylesheet the audit reads and the stylesheet the app renders
are the same one.

Syntax palettes were read off a live chat code block and are per family and per
mode, as `codeThemesByFamily` intends — Parchment light resolves to rust/green
(`#a94f2a`, `#22784f`), Alma Mater dark to blue/green (`#7fb3e6`, `#6fc084`),
Roche Limit light to the Pygments hues its design doc specifies (`#0a7a32`,
`#7024b0`, `#b02121`).

The terminal dock was opened under several families and re-grounds per family:
Parchment dark paints the warm code ground, Alma Mater light paints the muted
ground, and ANSI output from `ls -G` and `git status --short --branch` stays
legible on both.

> **Note.** An early attempt to switch themes by writing `localStorage` and
> toggling `data-theme` from the console produced identical syntax tokens across
> families and looked like a regression. It was not. The palette is selected in
> React via `codeThemesByFamily[useThemeFamily()][useResolvedTheme()]`, so a
> DOM-only change desyncs React state from the DOM and proves nothing. Theme
> switching must be driven through the Settings UI.

### Other flows exercised

- A live turn (`run pwd using your shell tool`) streamed a tool card to
  completion and returned `/Users/wanjun/Desktop`.
- The artifact side panel auto-opened on an agent-created `demo.md`, rendered the
  heading and link, and inlined the neighbouring local PNG — the data-URI image
  path still works.
- A `console.error` hook installed in the renderer recorded **0 errors** across a
  four-way theme sweep, chat navigation, terminal use and the preview panel.

**Verdict: GO.** Nothing regressed; the merge is safe to fast-forward onto `main`.

## Final regression and stress sweep — I3 (2026-07-20)

The last gate before `main` is fast-forwarded. Every suite was re-run verbatim,
every standing guarantee this campaign landed was re-driven in the running app
where an LLM was not required, and the two guarantees that turned out to be
drivable only with a live provider were driven against a local Ollama model.

### Environment: the provider blocker persists, but is no longer total

The UCSF gateway is still IP-blocked from this machine, in the GUI and the CLI
alike:

```
Ran into this error: Authentication error: Authentication failed.
Status: 403 Forbidden. Response: {"error":"The IP Address is invalid: 104.52.5.246"}
```

Unlike round I2, a local provider was available: Ollama serving `llama3.1`. It is
competent at prose but **cannot drive the tool loop** — given the real extension
set it hallucinates tool syntax as prose, and even against a minimal
`developer`-only config it emitted `import { shell } from "developer";` as text
rather than a tool call. So it buys real streaming turns — and with them the
turn-scoped UI states — but not real tool calls.

Everything driven through Ollama used an isolated `BIOROUTER_PATH_ROOT`
(`/tmp/br-p4-root`), so the user's own config, secrets and session history were
never mutated.

### A · Suites, verbatim

```
cargo check --workspace                     Finished `dev` profile ... in 31.05s   (exit 0, 0 warnings)
cargo test -p biorouter --lib               test result: ok. 1487 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 26.42s
cargo test -p biorouter-mcp --lib           test result: ok. 809 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 21.43s
cargo test --test mcp_integration_test      test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.83s
cargo fmt --check                           pass (no output)
./scripts/clippy-lint.sh                    ✅ All baseline clippy checks passed! · ✓ No banned TLS crates found
npm run test:run (run 1)                    Test Files  164 passed (164) · Tests  1457 passed (1457)
npm run test:run (run 2)                    Test Files  164 passed (164) · Tests  1457 passed (1457)
npx tsc --noEmit                            pass (exit 0)
npm run lint:check                          pass (exit 0)
  └ check:themes                            OK — generated artifacts are current (3 themes)
  └ check:contrast                          OK — all 252 contrast assertions pass
```

**Test-count deltas, fully accounted.** Both counts moved since the
docs-reorganization baseline (`300d2cdb`), and neither is unexplained:

| Suite | Baseline | Now | Delta | Source |
|-------|----------|-----|-------|--------|
| `biorouter --lib` | 1476 | 1487 | +11 | `101c7166` — 7 `#[test]` + one `#[test_case]` with 4 cases, all in the code-execution sandbox self-correcting-errors work |
| frontend | 1406 (161 files) | 1457 (164 files) | +51 (+3 files) | 3 new files (`keyboardResubmitGuard` 6, `useStopAcknowledgement` 5, `terminalSessionRegistry` 12 = 23) + 28 added to 7 existing files |

The contrast assertion count rose 228 → 252 with the `--background-canvas` work
recorded under H2; the audit discovers families from the stylesheet, so the
growth is the audit widening, not a threshold being relaxed.

**Suites the task list omits, run anyway.** The gates that rounds I1 and I2
actually landed live in `crates/biorouter/tests/`, which `--lib` does not
compile. All were re-run:

```
13 parallel/subagent/streaming/interrupt/abort/code-execution binaries   54 passed; 0 failed
6 permission, policy, path-resolution and apps-routing targets          115 passed; 0 failed
```

None of the known pre-existing frontend flakes (`App.test.tsx`,
`ConfirmationModal`, `ResetPanel`, `InstructionsEditor`,
`ProgressiveMessageList`) failed; each was observed passing in **both** runs.

### B · Standing guarantees, re-driven

Verdicts below are from the running app. The **detector is the session
database**, not the DOM — the DOM double-counts a message across the sidebar
title, the tab header and the bubble, which is exactly the trap the earlier
rounds warned about.

| Guarantee | Verdict | Evidence |
|-----------|---------|----------|
| Home submit lands exactly 1 message | **PASS** | DOM reported 3 occurrences; `messages` table holds exactly one row |
| 3 tab open/close cycles do not duplicate | **PASS** | 3 × ⌘T/⌘W → back to 1 tab, 0 console errors; the submit that followed persisted exactly once |
| Terminal cwd matches its session | **PASS** | dock opened at `/Users/wanjun/Desktop` matching the composer chip; a second session's dock showed `tmp/br-term-a/` |
| >8 terminals open fine | **PASS** | 14 panes / 15 tabs concurrently, 0 console errors |
| No terminal leak on reload | **PASS** | 14 panes → reload → 0 panes; `/bin/zsh -l` children of the Electron main process fell to exactly 2 (the 2 reopened), proving PTYs were reaped, not orphaned |
| `/tmp` file previews | **PASS** | `readFile('/tmp/br-preview/demo.md')` → `found:true`; panel rendered it |
| `~/.ssh` preview denied | **PASS** | `readFile('/Users/wanjun/.ssh/config')` → `{file:"", found:false, error:{}}` — and the file exists (619 bytes), so this is a denial, not a miss |
| Markdown images + links render | **PASS** | preview panel on `demo.md` yielded 1 `h1`, 1 `a[href]`, 1 `img` — the neighbouring local PNG inlined |
| Steer chip appears | **PASS** | typing during a live turn produced `● Next  also mention CRISPR` with `Add now` / `Stop & send` |
| Steer chip retracts | **PASS** | chip gone within ~1s of the stop; composer returned to its idle Send face |
| Stop acknowledges | **PASS** | activity indicator cleared ~1s after the press, transcript truncated mid-sentence as expected of a mid-stream cancel |
| Sending while scrolled up returns to the bottom | **PASS** | scrolled to `scrollTop=0`, submitted; settled at `scrollTop 1859` of `sh 2591 − ch 685 = 1906`, the 48 px being trailing padding — newest message fully visible |
| All 3 families × light/dark; canvas WHITE in Alma/Roche light; Parchment and Alma dark UNCHANGED | **PASS** | six-way sweep below |

**Theme sweep.** Driven through Settings → App → Theme (never by writing
`data-theme`, which desyncs React from the DOM and proves nothing):

| Family | Mode | `--background-canvas` | Body bg | Body fg |
|--------|------|----------------------|---------|---------|
| Parchment | light | `#faf8f3` | `#ffffff` | `#2a2520` |
| Parchment | dark | `#282217` | `#0d0a06` | `#f4f0e6` |
| Alma Mater | light | **`#ffffff`** | `#ffffff` | `#052049` |
| Alma Mater | dark | `#0d2a50` | `#04142e` | `#f2f3f4` |
| Roche Limit | light | **`#ffffff`** | `#ffffff` | `#1f1e1c` |
| Roche Limit | dark | `#131312` | `#131312` | `#ededea` |

Every body pair matches the round-3 table verbatim, so Parchment and Alma dark
are provably unchanged; the canvas is white in Alma and Roche light and stays
warm (`#faf8f3`) in Parchment, which is exactly H2. 0 console errors across all
six.

### The one defect found — and why it is not a regression

**Stop & send drops the queued message.** With a turn running and a message
queued in the steer chip, pressing **Stop & send** stops the turn but never
sends the queued message: it does not appear as a user message, does not start a
new turn, and never reaches the `messages` table. Its own `aria-label` promises
otherwise — *"Stop the current turn and send this message as a new turn"*.
Reproduced twice, with two different markers.

It is isolated to that one control: **Add now** on the same chip delivers
correctly (marker present, turn continues).

This branch *does* touch that function — `aaf24a22` swapped `if (onStop)
onStop();` for `stopAck.trigger();`. So causality had to be settled rather than
argued. It was settled by experiment: the line was temporarily reverted to
main's original call in the running tree, the scenario re-driven, and **the
message dropped identically**. The probe was then reverted and the tree
confirmed clean.

```
handleStopAndSend with stopAck.trigger()     → queued message dropped
handleStopAndSend with if (onStop) onStop()  → queued message dropped
```

`MessageQueue.tsx` carries no diff against `main` at all, and
`useStopAcknowledgement.trigger` calls `onStop()` synchronously, so it cannot
have reordered the stop against the submit. **The defect is pre-existing on
`main` and is not introduced by this branch** — fast-forwarding neither creates
nor worsens it. It is logged here rather than fixed because fixing it changes
queue-flush semantics and deserves its own review, in the same spirit as J2.

### What could not be verified, and why

These four need a model that will actually emit tool calls, which no reachable
provider would do. Each is named with the gate that does cover it, so the gap is
bounded rather than waved away:

| Not driven live | Covered by |
|-----------------|-----------|
| A multi-tool turn shows pending cards early with complete args | `streaming_pending_tool_calls` (2), `streaming_tool_response_ordering` (1); and the **persisted** card from an earlier round still renders — "Ran Coordinating 2 tool steps · 1 result ready" |
| Cancel mid-tool leaves no orphan | `parallel_tool_batch_cancellation` (1), `subagent_cancellation` (1), `turn_abort_tests` (4) — the PAR-04 backfill |
| `execute_code`-written files preview | `code_execution_integration` (27); the sibling `/tmp` preview path was driven live and passes |
| A sensitive write prompts for approval | `smart_approve_tests` (10), `scoped_permission_tests` (6); the persisted round-3 session still shows *"The user has declined to run this tool"* |

Also not re-driven: **the boot-splash logo shift (H7)**. A renderer reload
rehydrates faster than a screenshot can be taken, and the driver's `launch`
blocks until the window is ready, so neither path can catch the two splash
states. It stands on its 22 passing `boot-splash.test.ts` assertions, the
generated-splash CSS check inside `lint:check`, and the vision verification
already recorded at `30d615f3`.

### Verdict

**GO.** Every suite is green and every count delta is explained. Thirteen
standing guarantees were re-driven in the running app and all thirteen hold. The
single defect found is demonstrated by experiment to pre-date this branch. The
branch is safe to fast-forward onto `main`.

## Related documentation

- [Streaming tool-call UI campaign](README.md) — the campaign index, and the shape of the whole effort.
- [Session trace](session-trace.md) — the same work ordered by user instruction rather than by round.
- [Tool-errors audit](tool-errors-audit.md) — the log sweep behind the `INTENDED` against `DEFECT` classification cited above.
- [Tool routing](../../agent-loop/tool-routing.md) — the living document the deprecation proposal above belongs to.
