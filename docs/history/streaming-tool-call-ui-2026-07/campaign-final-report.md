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

## Related documentation

- [Streaming tool-call UI campaign](README.md) — the campaign index, and the shape of the whole effort.
- [Session trace](session-trace.md) — the same work ordered by user instruction rather than by round.
- [Tool-errors audit](tool-errors-audit.md) — the log sweep behind the `INTENDED` against `DEFECT` classification cited above.
- [Tool routing](../../agent-loop/tool-routing.md) — the living document the deprecation proposal above belongs to.
