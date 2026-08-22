# Coding-agent vendor frame fixtures

> **What this is.** A committed corpus of **real** frames emitted by the Claude Code and Codex
> CLIs, one raw vendor frame per line (NDJSON), for the streaming and tool-call-parity work on the
> `claude_code` and `codex` providers. Every later unit test replays these files instead of
> spawning a vendor CLI.
> **Status:** Current. Phase 0 of
> [`docs/providers/coding-agents/streaming-and-tool-call-parity.md`](../../../../../docs/providers/coding-agents/streaming-and-tool-call-parity.md).
> **Audience:** developers working on the coding-agent providers.

## The one rule

**These files are recordings, not specifications.** They are deliberately *not* idealised: field
names, key order, types, `null`s, absent keys and duplicated content are exactly as the vendor
emitted them. When a fixture contradicts what you expected the protocol to be, the fixture wins —
that is the whole point of capturing it. Do not "tidy" a cell to make a decoder pass; fix the
decoder, or add a new cell.

Corollary: a shape that is **not** in this corpus has not been observed. Section
[What this corpus does not contain](#what-this-corpus-does-not-contain) lists those explicitly,
because the tempting mistake is to write a fixture from a schema reading and then treat the
resulting green test as evidence the real CLI behaves that way.

## Format

One JSON object per line, terminated by `\n`, no trailing blank line, no CRLF.

- **Claude cells** hold stdout lines of
  `claude -p --output-format stream-json --include-partial-messages --verbose` — objects with a
  `"type"` of `system` / `assistant` / `user` / `result` / `stream_event` / `rate_limit_event`.
  The `result` frame is the terminal one and, confusingly, carries `"type":"result"` as a *late*
  key rather than a leading one.
- **Codex cells** hold raw JSON-RPC frames read from `codex app-server` stdout: notifications
  (`{"method":…,"params":…,"emittedAtMs":…}`), responses (`{"id":N,"result":…}`) and error
  responses (`{"id":N,"error":…}`) interleaved on one stream.

Some cells come from bb's recordings, whose on-disk format wraps each frame as
`{"ts","seq","dir","line"}` with the vendor frame **as a JSON string** in `line`. That wrapper is
unwrapped here: our files hold the vendor frame directly, so a test can `serde_json::from_str`
each line with no pre-step.

## Cells

### `claude/`

| Cell | Source | Vendor version | Frames |
|------|--------|----------------|--------|
| `turn-text.ndjson` | local capture | Claude Code 2.1.235 | 14 |
| `turn-thinking.ndjson` | local capture | Claude Code 2.1.235 | 21 |
| `turn-tools.ndjson` | bb `claude-code/turn-tools` | Claude Code 2.1.238 (Agent SDK 0.3.197) | 37 |
| `turn-tool-error.ndjson` | bb `claude-code/approval-deny` | Claude Code 2.1.238 (Agent SDK 0.3.197) | 27 |
| `auth-failure.ndjson` | bb `claude-code/auth-failure` | Claude Code 2.1.238 (Agent SDK 0.3.197) | 4 |

**`claude/turn-text.ndjson` — a plain text answer, streamed.**
Captured locally on 2026-08-21 from a scratch cwd with:

```bash
MAX_THINKING_TOKENS=0 claude -p "Write a four-line poem about a river. No preamble." \
  --model claude-haiku-4-5 --output-format stream-json --include-partial-messages --verbose \
  --tools "" --strict-mcp-config --setting-sources "" --max-turns 1
```

Exercises the ordinary text path: one `message_start` (id `msg_011CeHCCK4VkTMTMekDVrx77`, the
**same id on every chunk of the turn** — the stability the persistence layer needs), a
`content_block_start{text}`, three `text_delta`s that concatenate to exactly the `result` frame's
`result` string, `content_block_stop`, a `message_delta` carrying the turn's `usage`, and the
terminal `result`. `MAX_THINKING_TOKENS=0` is what keeps this cell thinking-free; the same prompt
without it produces a thinking block (see the next cell).

Frame types: `system/init` 1, `system/status` 1, `stream_event/message_start` 1,
`stream_event/content_block_start{text}` 1, `stream_event/content_block_delta{text_delta}` 3,
`assistant` 1, `stream_event/content_block_stop` 1, `stream_event/message_delta` 1,
`stream_event/message_stop` 1, `rate_limit_event` 1, `system/post_turn_summary` 1, `result` 1.

**`claude/turn-thinking.ndjson` — thinking deltas and a signature.**
Same command, without `MAX_THINKING_TOKENS`, prompt `"Reply with exactly the word OK"`. The turn
opens a `thinking` content block at index 0 and a `text` block at index 1, so it exercises
multi-block index handling as well as thinking. Note `signature_delta` arrives as a *content block
delta*, not as a field on the block, and the `assistant` frame then repeats the whole thinking
block with its assembled signature — the value phase 1 blanks before persisting.

Frame types: `system/init` 1, `system/status` 1, `stream_event/message_start` 1,
`stream_event/content_block_start{thinking}` 1, `system/thinking_tokens` 3,
`stream_event/content_block_delta{thinking_delta}` 2,
`stream_event/content_block_delta{signature_delta}` 1, `assistant` 2,
`stream_event/content_block_stop` 2, `stream_event/content_block_start{text}` 1,
`stream_event/content_block_delta{text_delta}` 1, `stream_event/message_delta` 1,
`stream_event/message_stop` 1, `rate_limit_event` 1, `system/post_turn_summary` 1, `result` 1.

**`claude/turn-tools.ndjson` — a `tool_use` → `tool_result` pair, twice.**
From bb's `claude-code/turn-tools` cell ("One turn: read a file, edit it, run a shell command.").
Two complete tool cycles, each: `content_block_start{tool_use}` with an **empty** `input: {}` (the
name and id are known here, the arguments are not — this is the moment a skeleton card can be
shown), a run of `input_json_delta` fragments whose `partial_json` concatenates to the arguments,
`content_block_stop`, then the `assistant` frame **repeating the same block** with `input` fully
materialised, then a `user` frame carrying the `tool_result` under the same `tool_use_id`. Ids
`toolu_01DELyv79QQJP4jHzu7fG1HJ` and `toolu_01KvxcNXVnvZgWfD6RafpxbR`. Both results are
`"is_error": false`. The tools are the CLI's own built-in `Bash` — see the gap section below.

⚠ **The streamed deltas are not always the arguments that ran.** For
`toolu_01KvxcNXVnvZgWfD6RafpxbR` the concatenated `input_json_delta`s decode to a `command` of
`"cd /tmp/fixture-ws && cat >> math.js …"` while the `assistant` frame's `input` for the same block
id is `"cat >> math.js …"` — the CLI rewrote the model's command (stripping the `cd` into the
working directory) between the two. The other two tool blocks in this corpus round-trip
identically, which is exactly why this is worth a fixture: assemble a *preview* from the deltas if
you like, but take the **`assistant` frame as authoritative** for what was actually invoked, or a
tool card will show a command the child never ran.

Frame types: `system/init` 1, `system/status` 3, `stream_event/message_start` 3,
`stream_event/content_block_start{tool_use}` 2, `stream_event/content_block_delta{input_json_delta}` 10,
`assistant` 3, `stream_event/content_block_stop` 3, `user` 2,
`stream_event/content_block_start{text}` 1, `stream_event/content_block_delta{text_delta}` 1,
`stream_event/message_delta` 3, `stream_event/message_stop` 3, `rate_limit_event` 1, `result` 1.

**`claude/turn-tool-error.ndjson` — a `tool_result` with `"is_error": true`.**
From bb's `claude-code/approval-deny` cell ("accept-edits mode; an out-of-sandbox command
denied."). `toolu_01RYGrMixrvN6YTVzWQ6EgWM` (`Bash`, `touch ~/bb-recording-outside.txt`) comes back
as `{"tool_use_id":…,"type":"tool_result","content":"Permission request denied","is_error":true}`.

Two details worth keeping in view, both of which a hand-written fixture would have got wrong:

- **A failed tool does not fail the turn.** The terminal `result` here is `"is_error": false`,
  `"subtype": "success"`, `"terminal_reason": "completed"`. Turn status and tool status are
  independent axes.
- The denial is *also* reported out-of-band in the `result` frame's `permission_denials` array,
  with `tool_name`, `tool_use_id` and the full `tool_input`.

Frame types: `system/init` 1, `system/status` 2, `stream_event/message_start` 2,
`stream_event/content_block_start{tool_use}` 1, `stream_event/content_block_delta{input_json_delta}` 7,
`assistant` 2, `stream_event/content_block_stop` 2, `user` 1,
`stream_event/content_block_start{text}` 1, `stream_event/content_block_delta{text_delta}` 2,
`stream_event/message_delta` 2, `stream_event/message_stop` 2, `rate_limit_event` 1, `result` 1.

**`claude/auth-failure.ndjson` — not signed in.**
From bb's `claude-code/auth-failure` cell (recorded against an empty `CLAUDE_CONFIG_DIR`). Four
frames, and every one of them matters:

- `system/init` still arrives, and reports `"apiKeySource": "none"` — a successful init is not
  evidence of a usable session.
- the `assistant` frame is **synthetic**: `"model": "<synthetic>"`, text
  `"Not logged in · Please run /login"`, plus the out-of-band keys `"error":
  "authentication_failed"` and `"is_api_error_message": true`.
- the `result` frame is the trap the plan calls out: **`"is_error": true` while
  `"subtype": "success"`**. Classification must read `is_error` / `terminal_reason`
  (`"api_error"` here) and never `subtype`. `api_error_status` is `null` even though this *is* an
  API error.

Frame types: `system/init` 1, `system/status` 1, `assistant` 1, `result` 1.

### `codex/`

| Cell | Source | Vendor version | Frames |
|------|--------|----------------|--------|
| `turn-text.ndjson` | bb `codex/plan-mode` | codex-cli 0.149.0 | 61 |
| `turn-tools.ndjson` | bb `codex/turn-tools` | codex-cli 0.149.0 | 59 |
| `turn-usage-two-requests.ndjson` | bb `codex/turn-tools` (projection) | codex-cli 0.149.0 | 6 |
| `turn-failed.ndjson` | bb `codex/auth-failure` | codex-cli 0.149.0 | 25 |

**`codex/turn-text.ndjson` — agent-message deltas.**
From bb's `codex/plan-mode` cell. It is sourced from the plan-mode lane for one reason: across
every bb codex recording this is the only lane whose turns are **pure text** and carry more than a
couple of deltas (26 across two turns). Nothing about the frames is plan-specific except the turn
settings inside `turn/started`; the agent-message lifecycle is the ordinary one. Two turns, so it
also exercises a second turn on an existing thread.

Per turn the shape is `item/started{agentMessage}` (with `text: ""` and a `phase`, here
`final_answer`) → N × `item/agentMessage/delta` (`params.itemId`, `params.delta`) →
`item/completed{agentMessage}` whose `item.text` is the **whole** message. The accumulated deltas
equal that final text in both turns here — but the decoder should prefer the completed frame's
text, since a dropped delta is otherwise silent.

Frame types: `<jsonrpc response>` 6, `remoteControl/status/changed` 1, `skills/changed` 1,
`thread/started` 1, `mcpServer/startupStatus/updated` 2, `thread/status/changed` 4,
`turn/started` 2, `item/started{userMessage}` 2, `item/completed{userMessage}` 2,
`item/started{reasoning}` 1, `item/completed{reasoning}` 1, `item/started{agentMessage}` 2,
`item/agentMessage/delta` 26, `item/completed{agentMessage}` 2, `rawResponse/completed` 2,
`thread/tokenUsage/updated` 2, `account/rateLimits/updated` 2, `turn/completed` 2.

**`codex/turn-tools.ndjson` — a `commandExecution` lifecycle.**
From bb's `codex/turn-tools` cell ("One turn: read a file, edit it, run a shell command."). Two
`commandExecution` items and one `fileChange` item, each as an `item/started` → `item/completed`
pair under a stable `item.id` (`exec-…`). On `item/started` the item is
`"status": "inProgress"` with `"exitCode": null` and `"aggregatedOutput": null`; on
`item/completed` it is `"status": "completed"`, `"exitCode": 0`, and `aggregatedOutput` holds the
captured stdout. These are Codex's **sandboxed built-ins** — they never pass through the tool
bridge, which is exactly why phase 3 renders them as child-executed.

Frame types: `<jsonrpc response>` 5, `remoteControl/status/changed` 1, `skills/changed` 1,
`mcpServer/startupStatus/updated` 2, `thread/started` 1, `thread/status/changed` 2,
`turn/started` 1, `item/started{userMessage}` 1, `item/completed{userMessage}` 1,
`item/started{agentMessage}` 2, `item/agentMessage/delta` 17, `item/completed{agentMessage}` 2,
`rawResponse/completed` 4, `item/started{commandExecution}` 2, `item/completed{commandExecution}` 2,
`item/started{fileChange}` 1, `item/completed{fileChange}` 1, `thread/tokenUsage/updated` 4,
`account/rateLimits/updated` 4, `turn/diff/updated` 4, `turn/completed` 1.

**`codex/turn-usage-two-requests.ndjson` — the total-vs-last trap.**
A **projection** of the same `codex/turn-tools` recording: the same lines, verbatim and in order,
filtered to `turn/started`, every `thread/tokenUsage/updated`, and `turn/completed`. It exists as
its own cell so the arithmetic bug has a fixture that is impossible to misread.

One turn issues four model requests, and each one emits a `thread/tokenUsage/updated` whose
`params.tokenUsage` has **both** a `total` and a `last`:

| frame | `total.totalTokens` | `last.totalTokens` |
|------:|--------------------:|-------------------:|
| 1 | 19767 | 19767 |
| 2 | 39660 | 19893 |
| 3 | 59653 | 19993 |
| 4 | 79676 | 20023 |

`total` is **cumulative over the thread**, `last` is the most recent request. A decoder that adds
up the `total`s reports 198 756 tokens for a turn that used about 20 000; one that takes the last
`total` reports 79 676. The first frame is the one that hides the bug — there `total == last`, so a
single-request fixture cannot distinguish the two readings. `tokenUsage` also carries
`modelContextWindow`.

Frame types: `turn/started` 1, `thread/tokenUsage/updated` 4, `turn/completed` 1.

**`codex/turn-failed.ndjson` — a turn that fails.**
From bb's `codex/auth-failure` cell (empty `CODEX_HOME`; an HTTP 401 from the Responses API). It
covers the failure surface end to end:

- a JSON-RPC **error response** to an ordinary request — an id-bearing error, not a notification,
  so a pump keyed only on `method` will drop it. Note the key order, which is the recorded one:
  `{"error":{"code":-32600,"message":"codex account authentication required to read rate
  limits"},"id":2}` — `error` *precedes* `id` here while the success responses in the same file put
  `id` first. Match on keys, never on position.
- ten `error` **notifications**, nine of them retry chatter (`"Reconnecting... 2/5"` …) carrying
  `codexErrorInfo.responseStreamDisconnected.httpStatusCode: 401`. Surfacing each of these to the
  user would be nine spurious errors for one failure.
- one `warning` notification (WebSocket → HTTPS transport fallback).
- the authoritative terminal frame: `turn/completed` with `turn.status: "failed"` and a populated
  `turn.error` (`message`, `codexErrorInfo: "other"`, `additionalDetails: null`). **`turn/completed`
  is emitted for a failed turn too** — its presence is not success.

Frame types: `<jsonrpc response>` 4, `<jsonrpc error>` 1, `remoteControl/status/changed` 1,
`skills/changed` 1, `thread/started` 1, `thread/status/changed` 2, `turn/started` 1,
`item/started{userMessage}` 1, `item/completed{userMessage}` 1, `error` 10, `warning` 1,
`turn/completed` 1.

## Provenance

**bb recordings.** `get-bb/bb`, checkout of 2026-08-21, at
`packages/provider-bridge-protocol/recordings/<provider>/<cell>/provider→bridge.ndjson`. That
directory also holds a `manifest.json` (CLI version, description) and the other three lanes
(`bridge→provider`, `runtime→bridge`, `bridge→runtime`), which are bb-internal and not mirrored
here. All bb cells above were recorded 2026-08-21 in a Linux container.

**Local captures.** Run 2026-08-21 on macOS against `claude` 2.1.235 from a scratch directory
outside the repo, on the user's own subscription. Exact commands are given per cell above.

## What was changed from the source recordings

Three transformations, all mechanical, applied by a build script that re-parses every output line
to guarantee it is still valid JSON:

**1. Unwrapping.** bb's `{"ts","seq","dir","line"}` envelope is removed and the `line` string
becomes the output line. Nothing inside a frame is reformatted, reordered or re-serialised — the
bytes of the vendor frame are carried through as they were recorded.

**2. Dropped frame types.** Only whole lines are dropped, never fields:

- *Claude cells* drop `control_request`, `control_response`, `command_lifecycle` and
  `system/hook_started` / `system/hook_response`. These belong to the Agent SDK's **bidirectional**
  control protocol, which requires `--input-format stream-json` and a hooks configuration. Our
  invocation has neither (it passes `--setting-sources ""`), so these frames cannot reach our
  parser — verified against the local captures, which contain none of them. Recover them from bb's
  recording if the provider ever adopts the bidirectional mode.
- *Codex cells* drop `rawResponseItem/completed`. Those frames carry whole prompt and message
  bodies — including bb's own system prompt — and correspond to nothing the provider decodes.
  `rawResponse/completed`, which carries the per-request usage, is kept.

**3. Redaction.** Applied as text substitution so cross-references survive:

| From | To |
|------|-----|
| the recording workspace (`/tmp/bb-recording-ws`, the local capture cwd) | `/tmp/fixture-ws` |
| home directories (`/home/user`, `/Users/…`) | `/tmp/fixture-home` |
| Claude `session_id` | `c1a0deNN-0000-4000-8000-000000000001` (one stable value per cell) |
| Codex thread / session / turn / installation ids | `c0de000N-0000-4000-8000-0000000000NN` |
| `messaging_socket_path` (contains a pid) | `/tmp/fixture-home/cc-socks/0.sock` |
| `rateLimits.credits.balance` | `"0.0000000000"` |

Ids are remapped **per cell** and every occurrence of a given id maps to the same replacement, so
`turnId` ↔ `turn.id`, `threadId` ↔ `thread.id` and `session_id` cross-references still resolve.
Message ids (`msg_…`), tool ids (`toolu_…`, `call_…`), item ids (`exec-…`, `rs_…`) and per-frame
`uuid`s are **not** remapped — they are not identifying, and rewriting them would break the very
correlations these fixtures exist to test. No API keys, tokens or email addresses were found in any
source line (scanned before and after the build).

## What this corpus does not contain

Say so out loud, because a missing shape is invisible once the tests are green:

- **No bridged BioRouter tool call.** No cell contains an `mcp__biorouter__*` `tool_use` /
  `tool_result` pair (Claude) or an `item/started|completed{mcpToolCall}` pair (Codex). bb's
  recordings have none — a grep across every bb codex lane finds zero `mcpToolCall` frames — and
  capturing one requires a live `biorouterd` serving the tool bridge, which phase 0 could not run.
  The plan makes the Codex `mcpToolCall` capture an explicit **entry gate for phase 3's Codex
  half**; it is still open. The only in-tree description of those shapes remains a schema reading
  (Codex 0.147.0 schema; `codex.rs`'s hand-labelled unit fixture carries just
  `id`/`server`/`tool`/`status`, with no `arguments` and no `result`).
- **No `item/reasoning/*` deltas.** `codex/turn-text.ndjson` contains a `reasoning` *item* pair but
  no streamed reasoning deltas; whether `summary:"auto"` ever produces them on a subscription model
  is still an open question in the plan.
- **No cancel cell.** The plan's phase-0 list mentions a `cancel` cell; it is not here. bb's
  `stop-interrupt` lanes are driven over the SDK control protocol our invocation does not speak, so
  the recording would not represent what our provider sees when it kills the child.
- **No compaction, steer, resume, subagent, approval or web-search cells.** bb has all of these for
  both vendors at the path given above; add one as its own cell when a phase needs it, rather than
  broadening an existing cell.

## Related documentation

- [Streaming and tool-call parity for the coding-agent providers](../../../../../docs/providers/coding-agents/streaming-and-tool-call-parity.md) — the plan these fixtures are phase 0 of.
- [How the coding-agent providers work](../../../../../docs/providers/coding-agents/how-it-works.md) — the blocking mechanism the streaming work replaces.
- [The tool bridge](../../../../../docs/providers/coding-agents/tool-bridge.md) — the relay whose calls no cell here yet records.
