# Issue sweep 2026-07 — plan of record

> **What this is.** The execution plan for closing every open GitHub issue (#18–#43,
> 24 issues) in one campaign: validated root causes, batch composition, ordering,
> worktree strategy, test gates, and review gates. Companion documents in this folder
> record the results as the campaign runs.
> **Status:** Current — campaign in execution (started 2026-07-26).
> **Audience:** anyone auditing how this sweep was planned and executed.

## 1. Scope and ground rules

- **In scope:** all 24 open issues. 23 get implemented; **#30 (Workspace Control MCP
  extension, BR-71) is plan-only** — a comprehensive execution plan is produced for
  operator review and nothing is implemented until sign-off.
- **Validation before code.** Every issue was re-validated against main @ `708390d8`
  (v1.88.5) by 10 parallel read-only investigation agents before any fix was written.
  Verdicts: 18 confirmed, 4 partially fixed by commits that landed after the reports
  (`c971c5b8`, `9ed3fd52`, `dcf1070c`, `a827259a`), 1 not-a-bug-as-reported (#42 — the
  reported repro was a UTC-vs-PDT timestamp misreading; hardening is still warranted).
- **Granular commits.** Every batch commits one logical step at a time (fix, then its
  tests, then docs), never one squashed blob, so the whole sweep is revertible and
  reviewable step by step. No AI co-author trailers (repo CI rejects them).
- **Model policy for any live-model run:** UCSF Versa only — `versa_azure`
  `gpt-5.5-2026-04-24` and `versa_bedrock` Claude Opus 4.8. Never the direct Anthropic
  API. Local-model functionality is exercised against the bundled llama.cpp sidecar.
- **Review gates.** Each batch: Claude implements + tests → Codex (GPT-5.4 CLI) reviews
  the diff → Claude addresses every Codex comment → merge. GUI-visible fixes also get a
  vision check in the running dev GUI (CDP screenshots) before the batch closes.

## 2. Validated root causes (one line each; full detail in §5 batch briefs)

| # | Verdict | Real root cause (validated, may differ from the report) |
|---|---|---|
| 18 | confirmed | `validate_base_path` (biorouter-headless/main.rs:475) allows only `[A-Za-z0-9._~/-]`; RFC 3986 pchar (`@`, `:`, sub-delims, `%XX`) rejected. |
| 21 | partially fixed | Typing bug was TERM-02/03 PTY-slot leak, fixed in v1.88.5 (`a827259a`). Remaining: Cmd+W closes the window because App.tsx close-active-tab never checks terminal focus. |
| 22 | confirmed | 3 synchronous store notifications per streamed token (chatStreamStore) + whole-snapshot subscription re-renders BaseChat+ChatInput+sidebar per token; no rAF batching anywhere. |
| 23 | confirmed | Report's premise wrong: `String.raw` does NOT make `${…}` literal — the generated JS was invalid. Product bug: tool description recommends the trap; boa parse errors carry no self-correction hint. |
| 24 | confirmed | developer shell spawns `$SHELL -c` with biorouterd's verbatim env; Finder-launched apps have launchd's minimal PATH; external MCP servers get an augmented PATH but the builtin shell doesn't. |
| 25 | partially fixed | Discovery + JSON-footgun fixed (`dcf1070c`, `9ed3fd52`). Remaining: web_scrape UA is `biorouter/1.0` (bot-blocked → 403s), no timeout, no retry, contradictory server instruction prefers browser automation. |
| 26 | confirmed | `search_modules` no-match returns `tool_failure retryable=false`; knowledge raw/ rejection explains why but not what to do; kb_write_page description omits the path contract; text_editor `file_text` validation is actually fine. |
| 27 | confirmed | Content IS reachable via expandables; real defect is `summarizeToolCall` lacks keys for `module_path`/`terms`/skill `name`, so labels degrade to bare "Read Module". |
| 28 | confirmed | Plan list + full code view already exist; genuinely missing: per-sub-call args/status/result telemetry (backend never emits it) and errors naming the failing tool. |
| 29 | confirmed | `VERSA_BEDROCK_KNOWN_MODELS` still omits Opus 4.8; the `-v1` id also needs its own exact `MODEL_CONTEXT_WINDOWS` entry (the report's "already handled" claim covers only the bare id). |
| 31 | partially fixed | Cross-process-collision theory wrong (id allocation is atomic; WAL+busy_timeout already on). Crash = #41. Real residue: `--no-session` still writes the shared DB; raw sqlite error + invalid JSON on failure. |
| 32 | confirmed | Backend + GUI enable/disable fully exist (`skills-config.json`); CLI has only install/list/remove. Identifier mismatch (frontmatter name vs dir slug) must be bridged. |
| 33 | confirmed | No direct Moonshot provider; declarative-JSON path (like deepseek.json) is fully wired and is the right vehicle; bare kimi ids lack exact context-window entries. |
| 34 | confirmed | Poller + progress state are component-local in both llamacpp surfaces; unmount cleanup kills polling while the detached sidecar download continues. |
| 35 | confirmed | ≥64 GiB default is the 35B-A3B MoE (24 GB download); KV/ctx ruled out (matches report's benchmark); no speed metadata exists. Download size is already surfaced pre-download. |
| 36 | confirmed | `read-artifact-file` catch forwards raw Node `ENOENT…` message; ArtifactViewer renders it verbatim; no structured error code. |
| 37 | confirmed | Active tab is a filled pill (deliberate D-07 note); no accent strip; no spring easing token; reorder is an instant array move with no FLIP. |
| 38 | confirmed | `closeTab` deliberately leaves an empty group ("does NOT navigate away"); nothing routes back to `/` when the whole layout has zero tabs. |
| 39 | confirmed | Pre-session dir choice dies inside ChatInput (BaseChat never passes `onWorkingDirChange`); first submit calls `createSession(getInitialWorkingDir())` unconditionally. Hub path is fine; backend innocent. |
| 40 | partially fixed | /dev/null trigger fixed (`c971c5b8`). Remaining: `prompt_tool_confirmation` is called unconditionally headless → interactive cliclack prompt on stdout (corrupts JSON), blocks, dies as `not connected`. |
| 41 | confirmed | NOT msg_uid atomicity: BedrockStreamDecoder stamps one shared message_id on every yielded message and never batches tool_use blocks (unlike Anthropic §6.2b) → two assistant messages share a msg_uid in one persist batch → UNIQUE(2067). Explains versa_bedrock correlation. |
| 42 | not a bug (as reported) | `enabled:false` IS honored for fresh `biorouter run` sessions (verified against the reporter's own sessions.db; UTC/PDT confusion). Hardening: agent can silently re-enable a config-disabled extension via `manage_extensions`; disabled entries unlabeled in search. |
| 43 | confirmed | `claude-opus-5` nowhere in the codebase; typed-in fallback gives wrong window (200k vs 1M) and wrong price (legacy $15/$75 vs real $5/$25). Model id verified: `claude-opus-5`, 1M ctx, no date suffix. |

Latent bug found during validation (not among the 24, fix alongside B1): `formats/anthropic.rs`
sends `thinking:{type:"enabled",budget_tokens}` on deep-effort turns — rejected (400) by the
modern Claude models incl. Opus 4.8/Opus 5; needs a model-gated adaptive-thinking switch, and
Opus 5 also rejects explicit `temperature` when thinking is off. Verify against provider docs
before changing.

## 3. Batches, worktrees, and ordering

Batches group issues that touch the same files (one worktree, sequential commits inside);
batches in the same wave touch disjoint files and run as parallel worktrees. Merges to main
are sequential, cheapest-first, with the full targeted test suite after each merge.

### Wave 1 — 7 parallel worktrees

| Batch | Issues | Branch | Primary files |
|---|---|---|---|
| B1 providers | #43, #29, #33 (+latent thinking bug) | `sweep/b1-providers` | providers/anthropic.rs, versa_bedrock.rs, declarative/moonshot.json, model.rs, pricing.rs, canonical_models.json, formats/anthropic.rs |
| B3 session/headless CLI | #41, #31, #40 | `sweep/b3-session-headless` | formats/bedrock.rs, agents/agent.rs, session/session_manager.rs, biorouter-cli/session/{mod,builder,output}.rs, cli.rs |
| B4 mcp-tooling | #23, #26, #25, #24 | `sweep/b4-mcp-tooling` | agents/code_execution_extension.rs, computercontroller/mod.rs, knowledge/{store,server}.rs, developer/shell.rs |
| B5 headless server | #18 | `sweep/b5-public-url` | biorouter-headless/src/main.rs |
| B6 tabs+terminal UI | #21 → #38 → #37 (this order) | `sweep/b6-tabs-terminal` | App.tsx, chatGroups/*, InAppTerminalDock.tsx, terminalFocus.ts, styles/main.css |
| B7 chat perf UI | #22, #39 | `sweep/b7-chat-perf` | hooks/chatStreamStore.tsx, BaseChat.tsx, ChatInput.tsx |
| B9 llamacpp | #34 → #35 (store refactor first) | `sweep/b9-llamacpp` | llamaServerStore.ts (new), LlamaServerInlineCard.tsx, LocalModelInventory.tsx, providers/llamacpp.rs, routes/llamacpp.rs (+OpenAPI regen) |

### Wave 2 — 2 worktrees, branched after their conflicting Wave-1 batch merges

| Batch | Issues | After | Why sequenced |
|---|---|---|---|
| B2 config/CLI | #32, #42-hardening | B3 merges | shares `cli.rs` + `session/builder.rs` with B3 |
| B8 tool transparency + artifact | #27, #28, #36 | B4 merges | #28's backend telemetry edits `code_execution_extension.rs`, same file as B4 |

### Wave 3 — verification and closure

1. Full regression: `cargo test --workspace`, frontend vitest, `npm run themes -- --check`,
   clippy + eslint on changed areas. Baselines recorded pre-sweep: vitest 164/164 files
   green; cargo green except `test_anthropic_provider` (live-API credit failure, machine
   environment, pre-existing).
2. GUI vision pass in the dev app (CDP screenshots) for #21, #22, #27, #28, #34, #35,
   #36, #37, #38, #39; local-model checks against the real llama.cpp sidecar.
3. **Parallel stress test** (documented in `stress-test.md`): concurrent headless
   `biorouter run` fleets on versa_azure gpt-5.5 + versa_bedrock Opus 4.8 with the desktop
   GUI open, deliberately maximizing sessions-DB read/write concurrency — the live proof
   for the #31/#41/#40 fixes; any failures found are fixed in this campaign.
4. Per-issue GitHub follow-up: comment with root cause + fix commit; close when merged.

### Wave 4 — #30 (BR-71) execution plan only

Comprehensive plan filed at `docs/agent-loop/designs/` building on the existing BR-71
design doc; presented for operator review. **No implementation in this campaign.**

## 4. Test gates (every batch)

1. Targeted suites named in the batch brief (cargo -p / vitest paths) — must pass.
2. New regression tests for each fix (fail-before/pass-after where feasible).
3. `cargo fmt` / prettier+eslint clean on touched files.
4. Codex review of the branch diff; every comment addressed (fix or explicit rebuttal
   recorded in the batch record).
5. UI batches: vision check in the dev GUI before close (BIOROUTER_NO_HMR=1, sandboxed
   XDG_CONFIG_HOME, CDP screenshot — never full-screen capture).

## 5. Batch briefs

The full validated fix plans (root-cause evidence, file:line anchors, step-by-step fixes,
test plans, UI verification scripts, risks) live in the per-cluster validation reports
produced by the investigation workflow; each batch's worktree brief is generated from
those. Summary of the intended fix per issue:

- **#18** — widen `validate_base_path` to the splice-safe pchar subset (add `@ : ! $ * + , ; =`
  and `%XX` validation); keep rejecting `' ( ) &`-class breakers; never percent-decode.
- **#21** — add close-pane registry to terminalFocus.ts; register in InAppTerminalDock; App.tsx
  close-active-tab checks `isTerminalFocused() && requestCloseTerminalPane()` first.
- **#22** — coalesce per-event triple notify into one snapshot swap; no-op guard in
  updateSnapshot; rAF-batched scheduleNotify (document.hidden timeout fallback; sync flush at
  turn boundaries); skip registry re-emit when entry unchanged; pass messagesLength to ChatInput.
- **#23** — `annotate_parse_error` beside `annotate_opaque_js_error`: detect template-literal
  or bash-`${…}` parse failures, teach the escape (`${"$"}{VAR}`) and the plain-string/file
  alternatives; correct the String.raw guidance in the execute_code description.
- **#24** — `augmented_path()` in developer/shell.rs appending `~/.local/bin`, `/usr/local/bin`
  (+ `/opt/homebrew/bin`, `/opt/local/bin` on macOS), deduped, mirroring SearchPaths; wired into
  `configure_shell_command` (covers background jobs too).
- **#25** — browser-compatible UA + 30 s timeout + one retry (connect/429/5xx only) for
  web_scrape; status-preserving error text with per-status hints; fix the contradictory
  instruction so web_scrape is the preferred URL fetcher.
- **#26** — search_modules no-match → Ok with guidance (flip the unit test); raw/ rejection
  message gains the recovery path; kb_write_page description states the path contract.
- **#27** — summarizeToolCall special cases for read_module/search_modules/loadSkill using
  their real arg names (module_path/terms/name).
- **#28** — backend: sub-call errors prefixed `Tool error from {tool}`; per-call telemetry
  (tool, truncated args, status, error/result size) attached as `biorouter/tool-calls` meta;
  frontend: executed-calls section with per-call status + args + real error; syntax-highlighted
  code view; error fallback prefers assistant-audience text over the generic sentence.
- **#29** — add `us.anthropic.claude-opus-4-8-v1` to VERSA_BEDROCK_KNOWN_MODELS (+ model.rs
  exact entry for the -v1 form); rewrite the stale AccessDenied comment; keep 4.6 default until
  a live Versa smoke passes.
- **#31** — `--no-session` builds a private per-run SessionManager (temp dir), never the shared
  DB; friendly store-error message; JSON output remains valid on failure (shared with #41 work).
- **#32** — `biorouter skill enable|disable <name>` editing skills-config.json (preserve unknown
  fields; idempotent); `skill list` shows slug, frontmatter name, enabled state; accepts
  name/bundle/slug with mapping notes.
- **#33** — declarative `moonshot.json` (MOONSHOT_API_KEY, api.moonshot.ai/v1, kimi-k2.x models
  with real context windows + prices); exact model.rs entries; canonical name mapping.
- **#34** — module-level llamaServerStore (useSyncExternalStore) owning the poll loop +
  status + operation descriptor; both surfaces subscribe; polling tied to operation, not mount.
- **#35** — ≥64 GiB default drops to a fast dense model; qwen3.6 stays as opt-in "large" with
  a speed hint; CatalogEntry gains speed metadata surfaced in both UIs (OpenAPI regen).
- **#36** — friendly error mapping (ENOENT/EACCES/EISDIR…) via a pure helper + structured
  `code`; ArtifactViewer renders a proper empty-state.
- **#37** — inset box-shadow accent strip on `data-active` tabs (dim rule already zeroes it);
  `--ease-spring` token; FLIP translate pass on reorder + select/ghost easing; amend the D-07
  design note.
- **#38** — PairRouteContent effect: whole layout has zero tabs and no pending cargo (peek,
  don't consume, newTabRegistry) → `navigate('/', {replace:true})`; covers stale deep links.
- **#39** — BaseChat holds `pendingWorkingDir`; passes `onWorkingDirChange` to ChatInput; uses
  it in the pre-session `createSession`.
- **#40** — headless_auto_decision helper: non-interactive/json/no-TTY → auto-deny with a
  specific stderr message + valid JSON output; the turn continues with the denial as a tool
  error instead of hanging.
- **#41** — port §6.2b tool_use batching to BedrockStreamDecoder (one batched assistant message
  per response); agent-loop defense: re-mint duplicate assistant ids within a turn; store-level
  UPSERT/retry resilience in add_message.
- **#42** — hardening only + issue comment with the timezone analysis: manage_extensions refuses
  to enable a config-disabled extension (directs the model to ask the user); search results
  label config-disabled entries.
- **#43** — add claude-opus-5 to ANTHROPIC_KNOWN_MODELS (top) + model.rs 1M entries (exact +
  substring before the `claude` catch-all) + $5/$25 pricing branch + canonical_models.json
  entry; default stays claude-opus-4-8 pending a live smoke (Versa-only policy here — the
  direct-API default flip is deferred to the maintainer).

## 6. Risk register

- **B3 touches the agent loop and Bedrock decoder** — highest-risk batch; mitigations:
  decoder unit tests mirroring the Anthropic batching suite, agent-level duplicate-id test,
  stress test in Wave 3 exercises exactly this path on versa_bedrock.
- **B7 touches the streaming state machine** — mitigated by the existing 1365-line
  chatStreamStore suite + new batching tests + a react-render-count probe in the GUI pass.
- **OpenAPI regen (B9)** — regenerated client can collide with other batches touching the
  server; B9 is the only Wave-1 batch regenerating, and Wave-2 batches rebase after merges.
- **Same-file merges (cli.rs, builder.rs, code_execution_extension.rs, App.tsx)** — handled
  by the wave structure; every merge is followed by the targeted suites of ALL previously
  merged batches (cumulative regression).
