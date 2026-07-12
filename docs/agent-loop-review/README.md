# BioRouter Agentic Loop Review

A comprehensive review of how BioRouter's agent feedback loop works today, how it
compares to nine open-source coding agents, and a merged program of 67
improvement proposals. This README is the executive report; the detailed evidence
lives in the ten internal reviews (`internal/*.md`), the nine external tool
reports (`external/*.md`), the four comparison chapters (`compare/*.md`), the
three proposal-lens files (`proposals/*.md`), and the master list
([`PROPOSALS.md`](PROPOSALS.md)).

---

## How the loop works today

Follow one user message through the pipeline. In the desktop app the message
begins as an SSE `POST /reply` from the GUI's `chatStreamStore`, carrying only
`session_id` and `user_message` with an `AbortController.signal`
(see `internal/server-flow.md`). The route handler (`reply.rs:214-485`, 50 MB
body limit) returns an SSE response immediately and does all work in a spawned
tokio task: it resolves a shared `Arc<Agent>` from the `AgentManager` LRU, loads
the session's `Conversation` from SQLite, appends the user message, and calls
`agent.reply(...)`, which returns a `BoxStream<AgentEvent>`. A `tokio::select!`
loop multiplexes the agent's events, a 500 ms `Ping` heartbeat, and the cancel
token, serializing each event as `data: {json}\n\n`. Notably there is **no
server-side single-turn-per-session lock** — serialization is only client-side
(`internal/server-flow.md` gap #1, "the single most important gap").

`Agent::reply` (`agent.rs:1240`, see `internal/core-loop.md`) first handles
special cases: an elicitation response short-circuits; `SessionStart` (once) and
`UserPromptSubmit` hooks may block or inject context; slash commands may resolve
the whole reply. It persists the user message, runs the **auto-compaction check**
(`agent.rs:1432`), then delegates to `reply_internal`.

**Context injection** happens at three cadences (`internal/context-injection.md`).
Once at agent construction the system-prompt clock is frozen
(`prompt_manager.rs:186`). Per user turn the entire system prompt is rebuilt from
`prompts/system.md` via MiniJinja — re-reading `.biorouterhints`/`AGENTS.md` from
disk (with `@import` transclusion, boundary/depth/cycle/gitignore-guarded) and
each MCP server's `instructions` (Unicode-sanitized, name-sorted for cache
stability). Per agent action inside the turn, `inject_moim` (`agent.rs:1596`)
splices a fresh `<info-msg>` user message carrying the time, working directory,
and each platform extension's `get_moim()` — critically the live todo list, which
lives in `session.extension_data` and so survives compaction. The model is told
**one line** about the project ("Working directory: …") and nothing else — no
repo map (`internal/state-awareness.md` gap #1).

The loop (`agent.rs:1556`) then runs turns. Each turn drains soft-interrupts,
injects MOIM, and starts the LLM call via `stream_response_from_provider`.
Provider SSE deltas become `MessageStream` items; `categorize_tools`
(`agent.rs:1670`) splits frontend/backend tool requests and yields partial text
to the client. When the model emits tool requests they run the **tool gauntlet**
(`internal/guardrails-permissions.md`): four inspectors in fixed order —
`security` (a regex scanner, off by default, ask-only), `permission` (the real
gate: `Auto` allows all, `Approve`/`SmartApprove` route unknown tools to human
approval — though `SmartApprove` is inert because its read-only sets are empty and
its LLM judge is dead code), `repetition` (`RepetitionInspector`, denies the 4th
byte-identical consecutive call), and `hooks` (user `PreToolUse` hooks). Verdicts
merge **escalation-only** (raise the bar, never lower it). Approved tools dispatch
in parallel via unbounded `select_all` (`agent.rs:1792`); a `needs_approval`
request yields an `ActionRequired` card and **blocks** on `confirmation_rx.recv()`
until the GUI POSTs `/action-required/tool-confirmation` — with no TTL, so a lost
confirmation blocks forever (`internal/server-flow.md` gap #2).

Each tool result is round-trip validated (`call_tool_result::validate`) before
persistence, then `PostToolUse`/`PostToolUseFailure` hooks fire — **observe-only**,
they cannot block (`internal/hooks.md` #2). The assistant tool-call message and
its tool-result message are appended to `messages_to_add` and yielded over the
same SSE stream. Usage is recorded once per turn (`agent.rs:2035`). If no tool
was called, the loop decides done-ness: auto-continue a length-truncated turn
(dead code on native Anthropic streaming because `finish_reason` is never set —
`internal/core-loop.md` gap #1, the "single most surprising correctness gap"),
enforce a pending `final_output` schema, run workflow retry, or evaluate a Stop
hook (the `/goal` judge can block completion). `messages_to_add` is persisted and
merged into the `Conversation` (`agent.rs:2108`).

**Compaction** (`internal/compaction.md`) fires proactively at 0.8 of the context
limit, reading the provider-reported `session.total_tokens` (tiktoken `o200k_base`
on the cold path). `compact_messages` feeds the *entire* agent-visible history to
`complete_fast` (the weak model) producing one 9-section summary; originals are
flipped `agent_invisible` but stay `user_visible` (non-destructive), and the
summary + a continuation note + a fresh copy of the last user message become the
new context. **Session persistence** is one SQLite DB: a `sessions` row
(working_dir, tokens, `extension_data` JSON) and an append-only `messages` table
(`content_json`, positional synthetic ids). Compaction does a whole-history
`DELETE`+re-`INSERT` (`internal/state-awareness.md`, `internal/compaction.md`
gap #8). The turn ends when the loop breaks; the consumer calls
`maybe_rename_session`.

---

## Answers to the review questions

**When are contexts injected?** At three cadences (`internal/context-injection.md`):
once at agent construction (frozen clock); rebuilt every user turn (system prompt
re-rendered, hints re-read from disk); and per agent action inside a turn (MOIM,
soft-interrupts, truncation nudge). Freshness is best-in-class; the trade is
cache churn and a frozen system-prompt date that contradicts MOIM's clock.

**How do contexts flow through the conversation?** Two independent streams: the
system prompt (assembled by `SystemPromptBuilder::build`, `prompt_manager.rs:104`)
and the message array (persisted `Vec<Message>` plus synthetic per-turn
injections). MOIM re-presents durable state (time, cwd, todos) every provider
call and is inserted after trailing tool_results to preserve tool_call/result
pairing (`internal/context-injection.md`).

**How does the agent call tools and how do results return?** Function-calling over
MCP, name-routed. The model emits tool requests; inspectors gate them; approved
ones dispatch in parallel; each result is validated, appended as a tool-response
message, and streamed back over SSE (`internal/core-loop.md`,
`internal/guardrails-permissions.md`). Errors return as `is_error: true` text the
model must read — there is no structured error taxonomy.

**How does compaction work and how does the conversation continue after it?**
Summarize-everything at 0.8 of the window: the whole agent-visible history →
one `complete_fast` summary; originals go `agent_invisible` (user still sees
them); the model then sees summary + continuation + re-appended last user message
(`internal/compaction.md`). No recent-turn verbatim window — the biggest fidelity
gap vs SOTA.

**How do hooks work — how many types/events, where do they inject/enforce?**
A Claude-Code-compatible model of shell-command or LLM-judge hooks at **13 wired
event variants** (`internal/hooks.md`). Four can block (PreToolUse,
PermissionRequest, UserPromptSubmit, Stop); SessionStart/UserPromptSubmit inject
context; PostToolUse is observe-only and PreToolUse `additionalContext` is
silently dropped. Failure-open by design; file/env config only, no rewrite path.

**How are infinite loops / repetitive tool calls handled?** Five layered
mechanisms (`internal/loop-detection.md`): `RepetitionInspector` (byte-exact
consecutive duplicates, denies the 4th), a 100-*iteration* turn cap, cooperative
cancellation, provider retry/backoff, and `/goal`-only Jaccard stall detection.
Trivially defeated by a one-char arg change or A/B/A/B oscillation, and the true
reason is hidden (the model is told the *user* declined).

**How are long-running tasks handled?** Four independent systems
(`internal/long-running.md`): well-built background shell jobs (`background=true`,
OS-exit-truth, read cursors, race-free `shell_wait`), blocking semaphore-capped
subagents, a durable cron `Scheduler` with overlap/pause-when-active guards, and
elicitation. Background jobs, subagents, and in-flight scheduled runs do **not**
survive a daemon restart.

**How does the agent track processes it started?** Three disjoint in-memory
registries with `kill_on_drop(false)` and **no PID-file/parent-death reaping** —
a crash orphans process groups forever, even though the llama.cpp sidecar already
implements the reaping pattern; `list()` exists but is dead-coded so the agent
can't enumerate its own jobs (`internal/long-running.md` gaps #1/#3).

**How does the agent understand its surroundings / track what it's working on?**
A single `working_dir` line in MOIM (no repo map / file tree / symbol index),
plus a full-overwrite `String` todo blob re-injected via MOIM and an in-memory
`/goal` state (`internal/state-awareness.md` gaps #1/#3/#4).

**Does it version-control its own edits?** No — no shadow git, no session undo.
`git2` is used only for the Knowledge-base wiki; `text_editor` has an in-memory,
per-process, whole-file LIFO that dies with the process and misses shell/other
writes (`internal/state-awareness.md` gap #2 — "the single biggest gap").

**How does it know when it's making a mistake?** Almost entirely tool-result
`is_error` text; no post-edit compile/lint/LSP loop, no structured errors, no
self-critique. The `/goal` judge is the one active "are you done?" signal, and
loop/stall detection lives only in the goal loop (`internal/verification.md`
gaps #3/#7, `internal/state-awareness.md` gap #7).

**How does it decide explore-vs-answer and when to harden/test?** Almost entirely
prompt tone — no effort/thinking budget, no automatic "this is complex → plan"
trigger; plan mode is a one-shot prompt rewrite. Enforced verification exists only
for workflows (`execute_success_checks`, single `Shell` variant, resets all
progress on failure); in interactive chat "done" is whatever the model decides
(`internal/verification.md` gaps #1/#9).

**What infrastructure helps it understand the BioRouter ecosystem to call tools
well?** MOIM injects each platform extension's live state; each MCP server's
`instructions` render verbatim under `## <name>`; the OSV malware check gates
extension install; `.biorouterignore` protects secret reads (Developer-MCP only).
The gap is a repo map and per-model prompt variants (`internal/context-injection.md`).

**How does the system prompt teach tool use?** One fixed `system.md` (MiniJinja),
with tool schemas passed via function-calling and a contract-tested set of
behavioral clauses; the only per-model transform is the toolshim JSON rewrite, so
43+ providers share one prompt regardless of capability (`internal/context-injection.md`
gap #10, `compare/context.md`).

---

## How BioRouter compares

**Context (`compare/context.md`).** *Ahead:* per-turn prompt rebuild with live
hint re-read, MOIM durable-state re-injection, `@import` transclusion with a real
security model, and Unicode-tag injection hardening — best-in-class on freshness
and import safety. *Behind:* no repo map (worst-in-class, alongside Pi — **Aider**
does ranked tree-sitter repo-maps best), no per-model prompt variants (**Codex**),
no total context budget (**OpenHands**/**Codex**), and project files trusted as
system-level instruction (**Claude Code** treats them lower-trust).

**Memory (`compare/memory.md`).** *Ahead:* non-destructive compaction via
visibility flags (the full user transcript survives), provider-truth-first token
accounting, and MOIM keeping task state out of the summarizer. *Behind:* no
recent-turn verbatim window (**Gemini CLI** and **Aider** do this best), no
targeted tool-output offloading (**Claude Code**), an OpenAI-only tokenizer for all
providers (worst-in-class token counting; **Codex** best), and three disjoint,
never-auto-promoted memory stores (**Codex** self-maintained memories best).

**Safety (`compare/safety.md`).** *Ahead:* a genuine superset of Claude Code's
hook events (13 wired), a fail-closed escalation-only inspector chain, and unique
OSV malware + local PII guards. *Behind:* no hook input-rewrite path (**Codex**
`updated_input` best), read-only auto-approve and the LLM judge are dead code (so
SmartApprove ≡ Approve; **OpenHands** `security_risk`+`ConfirmRisky` best), no OS
sandbox (**Codex** Seatbelt/Landlock best), an evadable off-by-default command
scanner (**Codex** `execpolicy`), exact-duplicate-only loop detection (**Gemini
CLI** three-layer best), no mistake-streak handling (**Cline** best), no budget
cap, and no checkpoint/undo.

**Execution (`compare/execution.md`).** *Ahead:* genuinely well-built background
shell jobs (match **Claude Code**/**Codex**), fork-bomb-guarded subagents,
tool-result validation, soft interrupts, and a resource-aware scheduler. *Behind:*
three conspicuous holes — **no checkpoints/undo** (worst-in-class, "the single
starkest deficit"; **Cline**/**OpenCode** best), **no automatic post-edit
verification** (the LSP/`analyze` capability exists but is unwired; **Claude
Code**/**OpenCode**/**Aider** best), and **no git integration of agent edits**
(**Aider** auto-commit-per-edit best) — plus crude oversized-output handling
(worst-in-class), no per-tool timeouts, unbounded parallel dispatch, and lossy
blocking subagents.

---

## Improvement program

The three lens reviews (performance, robustness, ux) were merged and deduplicated
into **67 proposals (BR-1 … BR-67)** in [`PROPOSALS.md`](PROPOSALS.md), every
distinct source proposal preserved (a lens→BR crosswalk verifies coverage).

### Counts by category

| Category | Count |
|---|---|
| Context & prompts | 9 |
| Compaction & memory | 8 |
| Hooks & guardrails | 14 |
| Loop & stuck detection | 9 |
| Long-running & processes | 6 |
| Checkpoints & version control | 3 |
| Verification & done-ness | 6 |
| Performance & tokens | 8 |
| UX & agent ergonomics | 4 |
| **Total** | **67** |

### Top 15 by leverage

| id | title | impact | effort | why now |
|---|---|---|---|---|
| BR-46 | Fix Anthropic `finish_reason` → stop silent mid-sentence truncation | High | S | Correctness bug on the *default* provider; every truncated answer ends silently — one-line map, well-scoped test. |
| BR-43 | Shadow-git checkpoints + `/rewind` | High | L | The single starkest deficit vs every current-gen agent; the recovery net that makes aggressive autonomy tolerable. |
| BR-1 | Repo map / workspace summary in context | High | L (M for a file listing) | Worst-in-class per two comparison chapters; the model rediscovers structure every session. |
| BR-18 | Revive read-only auto-approve + risk grading (SmartApprove ≠ Approve) | High | M | Dead code today makes "smart" mode identical to Approve and over-prompts on every read. |
| BR-33 | Single-turn-per-session server lock | High | M | "The single most important gap" in server-flow; raced/duplicate `/reply` corrupts shared state and doubles spend. |
| BR-10 | Recent-turn verbatim window at compaction | High | M | The biggest fidelity regression vs SOTA; freshest tool output/diffs are lost to a lossy summary. |
| BR-52 | Kill per-token DB reads + carry `TokenState` in the SSE event | High | M | #1 streaming-latency source: two SQLite queries per streamed token, one growing with history. |
| BR-6 | Token-aware large-response handling (preview + in-sandbox handle) | High | M | Worst-in-class oversized-output handling; blinds the model on huge grep/SQL/bio outputs. |
| BR-19 | PreToolUse `updated_input` rewrite + let PostToolUse block | High | M | Turns hooks from a veto into a policy engine (sandbox a path, reject a write that fails lint). |
| BR-29 | Staged soft-then-hard loop stop + honest repetition reason | High | S–M | Cheapest loop-safety upgrade; today the model is falsely told the *user* declined. |
| BR-12 | Background compaction off the critical path | High | L | Compaction is a synchronous multi-second LLM stall inside the user's turn. |
| BR-54 | SharedMcpPool across agents/sessions | Very high | L | The dominant RAM multiplier: N sessions × M MCP processes with no sharing. |
| BR-47 | Auto post-edit diagnostics (LSP/`analyze`) feedback loop | High | M | The capability exists but is never wired; turns "run tests" from a suggestion into a signal. |
| BR-20 | Always-on catastrophic-command denylist | High | S–M | `Auto` mode = zero command screening today; a tiny hard-block list closes the hole. |
| BR-30 | Semantic / oscillation / repeated-failing-result loop detection | High | M | Catches the loop classes (arg-tweak, A/B/A/B, repeated errors) that actually occur. |

---

## Index of all reports

**Master proposal list**
- [PROPOSALS.md](PROPOSALS.md) — the merged 67-proposal improvement program

**Proposal lenses** (`proposals/`)
- [performance](proposals/performance.md)
- [robustness](proposals/robustness.md)
- [ux](proposals/ux.md)

**Comparison chapters** (`compare/`)
- [context](compare/context.md)
- [memory](compare/memory.md)
- [safety](compare/safety.md)
- [execution](compare/execution.md)

**Internal reviews — BioRouter ground truth** (`internal/`)
- [core-loop](internal/core-loop.md)
- [context-injection](internal/context-injection.md)
- [compaction](internal/compaction.md)
- [hooks](internal/hooks.md)
- [guardrails-permissions](internal/guardrails-permissions.md)
- [loop-detection](internal/loop-detection.md)
- [long-running](internal/long-running.md)
- [state-awareness](internal/state-awareness.md)
- [server-flow](internal/server-flow.md)
- [verification](internal/verification.md)

**External tool reports** (`external/`)
- [goose](external/goose.md)
- [cline](external/cline.md)
- [opencode](external/opencode.md)
- [pi](external/pi.md)
- [aider](external/aider.md)
- [openhands](external/openhands.md)
- [codex-cli](external/codex-cli.md)
- [gemini-cli](external/gemini-cli.md)
- [claude-code](external/claude-code.md)
