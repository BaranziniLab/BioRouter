# Performance, limits and known gaps

> **What this is.** What these two providers cost in latency and prompt tokens, measured rather than
> estimated; why conversation history is flattened into one prompt each turn; why there is no
> streaming yet; and the failure modes worth recognising before diagnosing them as something else.
> **Status:** Current. The figures below are single-machine measurements taken during
> implementation, quoted so a later change can be compared against something; re-measure before
> treating any of them as a budget.
> **Audience:** developers working on the coding-agent providers, and users wondering why the first
> token takes longer than with an API provider.

A turn on these providers is a process launch, not an HTTP request. That is the whole explanation for
most of what follows, and it is a fixed cost that no amount of prompt tuning removes.

## Latency: most of it is not the model

| Measurement | Value |
| --- | --- |
| A `claude -p` turn, wall clock | ~5.3 s |
| …of which was actually API time | ~1.8 s |
| A `codex exec` turn, wall clock | ~3.1 s |
| A cold `claude` start, warm dev machine | ~3.5 s |
| Probe ceiling, per CLI | 20 s |
| Turn ceiling, both providers | 30 minutes |

So roughly 3.5 of the 5.3 seconds is process start, not inference. Two consequences worth
internalising:

- **Short turns feel disproportionately slow**, and long turns barely notice the overhead. These
  providers suit substantial questions, not rapid-fire exchanges.
- **The 20-second probe ceiling is generous on purpose.** A first run after a CLI update can be
  slower than a cold start, and reporting "not installed" for a slow-but-working binary is worse than
  waiting.

The turn ceiling exists because none of the earlier CLI-agent providers had one, and a wedged child
held a session open indefinitely with no way to stop it. It is generous because a real coding-agent
turn can legitimately run for minutes, and finite because "forever" is not a state a session should
be able to reach.

## Prompt overhead: replacing the vendor prompt is the single biggest win

| Prompt | Tokens per call |
| --- | --- |
| Claude Code's default system prompt | 25,022 |
| BioRouter's system prompt, replacing it | 1,527 |
| Codex's own default preamble | ~15,000 |

A 16x reduction on Claude Agent, from one flag. This is why `--system-prompt` **replaces** rather
than `--append-system-prompt` adding to the default, and why Codex's `baseInstructions` is set at
`thread/start` — see
[the flags that are not optional](child-agent-isolation.md#the-system-prompt-is-replaced-not-appended).

## Tool-schema cost, and why it is cheaper than it looks

A 60-tool surface measured about **5.2k prompt tokens on Claude — and they were cache reads**, not
fresh input. The tool list is a stable prefix, so both vendors' prompt caching absorbs most of its
cost after the first turn.

That is the same reason re-sending the conversation each turn is affordable; see below. It is also
why the usage figures are reported with the cache buckets kept separate: the live context gauge counts
the cached prefix as occupancy, while the billed total does not double-count it. The bucket
arithmetic differs per vendor and is described in
[usage accounting](how-it-works.md#usage-accounting-for-a-turn-that-billed-no-tokens).

## Why history is flattened rather than replayed

The `Provider` trait hands every call the entire conversation; both CLIs take a prompt. BioRouter
flattens the conversation into one prompt each turn. Two better-looking alternatives were tried and
rejected on evidence:

**Replay over `--input-format stream-json`.** That channel is an *interactive* one, not history
replay: each user message it receives starts its own complete turn, and an injected assistant message
is ignored outright. Measured — a three-message replay produced **two separate answers** rather than
one continued conversation.

**`--session-id` then `--resume`.** This does work across processes and is a legitimate later
optimisation. It is not the default because it moves the authoritative history **into the child**,
where BioRouter's compaction, message editing and `.biorouterignore`-driven redaction cannot reach
it. The two copies would silently diverge exactly when a long session gets compacted — the moment
you can least afford it. Flattening keeps BioRouter's transcript authoritative, which is the property
the `Provider` contract already assumes.

The cost is re-sending the conversation every turn, and prompt caching of the stable prefix puts the
marginal cost far below the naive reading.

Two details of the flattening are worth knowing when output looks odd:

- A lone user message is passed through **verbatim**. Wrapping a single question in scaffolding
  measurably degrades the answer.
- With history, earlier turns go inside a `<conversation_history>` block and the live instruction
  follows *outside* it with no role label. Without that separation both models tend to summarise the
  transcript instead of answering it. Tool traffic is included as text, capped at 4,000 characters
  per result, so one huge SQL or shell result cannot evict the actual conversation.

## There is no streaming yet

Both providers are non-streaming: the answer appears when the turn completes. `supports_streaming()`
returns false, so the agent takes the blocking branch.

This is unfinished work rather than a structural obstacle, and an earlier version of this page
overstated the difficulty by blaming the bridge. It does not: `Agent::reply` binds the bridge lease
before the task-local scope and it lives to the end of that loop iteration, which outlasts stream
consumption, and a `stream()` implementation runs *inside* the scope — so it can read the URL and
spawn the child exactly as `complete()` does. The one real rule is that the URL must be captured at
construction time, never read from inside a poll.

What is genuinely left is the parsing. For Claude Agent most of it already exists: the CLI's
`--output-format stream-json` emits raw Anthropic frames inside a `stream_event` envelope, so
unwrapping the envelope and feeding `providers::formats::anthropic::response_to_streaming_message`
reuses the decoder the Anthropic provider already uses — and the argument builder already takes the
output format as its only varying axis. For Codex the equivalent is the `item/agentMessage/delta`
and `item/reasoning/textDelta` notifications the app server already sends and the provider currently
ignores. Cancellation would want wiring at the same time.

## Failure modes worth recognising

| What you see | What it usually is |
| --- | --- |
| "not installed" on a machine where the terminal finds the binary | The desktop app's truncated `PATH`. Pin the path — [when BioRouter cannot find the binary](installing-and-signing-in.md#when-biorouter-cannot-find-the-binary). |
| An authentication error naming `apiKeyHelper` after a CLI upgrade | `assert_subscription_auth` fired: the run would have been billed to something other than the subscription. Most likely an `apiKeyHelper` in a Claude Code settings file, or `-p` having defaulted to `--bare`. |
| A card stuck on "indeterminate" | The probe ran but could not be understood — malformed `claude auth status` output, an `auth.json` with no `auth_mode`, or an expired refresh token behind a well-formed file. The reason is on the card. |
| The child says it cannot do something and asks you to approve it | A bridged tool call routed to `needs_approval`, which is [refused rather than parked](tool-bridge.md#a-call-needing-approval-is-refused-not-parked). |
| The child has no tools at all | No bridge was established — typically a CLI process with no HTTP server. The turn still answers from the conversation; this is deliberate degradation, not an error. |
| An empty response with nothing in the logs | For Codex, the app server exited before a terminal frame; the error carries a bounded tail of the child's stderr. For Claude Agent, stderr is drained concurrently and included in the failure. |
| A turn that stops at 30 minutes | The turn ceiling. The child is killed rather than left holding the session. |

## Related documentation

- [How the coding-agent providers work](how-it-works.md) — the mechanism these costs come from,
  including the flattening and the usage arithmetic.
- [The tool bridge](tool-bridge.md) — the grant lifetime that currently prevents streaming, and what
  a large tool surface buys.
- [What the child agent may not do](child-agent-isolation.md) — the flags behind the prompt saving.
- [Installing and signing in](installing-and-signing-in.md) — the setup-side failure modes in the
  table above.
- [Common problems and fixes](../../troubleshooting/common-problems-and-fixes.md) — general app and
  daemon troubleshooting.
