# The compaction preservation marker

> **What this is.** The per-message marker that says "carry this verbatim through every
> compaction", the exact promise it makes, the budget that bounds it, and what happens when
> that budget is exceeded. Read it with
> [Conversation writeback freshness](conversation-writeback-freshness.md): the two are the
> two halves of one guarantee, and each is useless alone.
> **Status:** Current. The mechanism ships; nothing sets the marker yet — BR-71's
> `workspace_send_prompt { mode: "note" }` is the first consumer and is unbuilt.
> **Audience:** developers touching compaction, the session store, or any feature that
> promises a message will reach the model.

## The problem it solves

A message appended to a live session had two independent ways of never reaching the model,
and fixing only one of them makes the failure slower rather than rarer.

The first is a write: a whole-history rewrite computed from a stale snapshot deletes it.
That is [conversation writeback freshness](conversation-writeback-freshness.md). The rewrite
paths are fixed — a write-back can no longer destroy an append it was never shown — and that
page's scoreboard says which of the *deletion* paths are and are not, because they are not
all the same question.

The second is compaction. Compaction keeps only the last `keep_last_turns` turns verbatim
(default 4, `DEFAULT_COMPACT_KEEP_LAST_TURNS`) and summarizes the older prefix, flipping it
agent-invisible. There was no way to say "keep this one" — no pin, no sticky flag, nothing
`recent_window_split` consulted. So a note that survived the write-back was dissolved into a
summary a few turns later. The caller had already been told its append succeeded, and
nothing ever corrected that.

## The marker

`MessageMetadata.pinned` (`crates/biorouter/src/conversation/message.rs`), set with
`Message::pinned()`, read with `Message::is_pinned()`.

It rides the metadata that is already persisted as `messages.metadata_json`, so it
round-trips through every path that already round-trips `user_visible` / `agent_visible`
with no new plumbing: `add_message`, the DELETE + re-INSERT rewrite, the guarded rewrite's
foreign-tail recovery, export/import, copy, and both diverge paths. `pin_persistence_tests`
in `session_manager.rs` asserts each of those separately, because a marker that compaction
honours but a fork erases would be worse than no marker — callers would trust it.

`#[serde(default)]` on the field is load-bearing rather than tidy. The read path decodes
with `from_str(..).ok().unwrap_or_default()`; a required field would make every row written
before the marker existed fail to decode and silently reset to fully default metadata,
which would resurrect every compacted-away message into the agent's context.

## The promise, stated exactly

> A preserved message survives **summarization** unconditionally. It is still subject to the
> overall context **budget**, and when the budget bites the **oldest** preserved messages are
> the first to lose their exemption — visibly.

So the guarantee is "never dropped by summarization", **not** "never dropped". Losing the
exemption is a demotion, not a deletion: the message stays in the user's transcript and is
summarized like any other older message.

## Where it is honoured

At exactly one place — `compact_messages_with_window` in `context_mgmt/mod.rs` — because
that is the funnel every compaction site reaches:

| Site | Reaches it via |
|---|---|
| Background eager swap (`run_eager_compaction`) | `compact_messages` |
| In-turn auto-compaction (`agent.rs`) | `compact_messages` |
| `/compact` and `/summarize` (`execute_commands.rs`) | `compact_messages` |
| Overflow recovery, all four rungs (`agent.rs`) | `compact_messages_with_recovery` |

A marker honoured by three of five paths would be worse than none, because callers would
trust it. Honouring it at the funnel is what makes "every path" a structural property rather
than a checklist.

Three distinct changes were needed, because compaction erases a message three different
ways:

- **The windowed path** flips the older prefix agent-invisible. An honoured pin is left
  exactly as it is — visibility *and* marker untouched, so the next compaction honours it
  too. (`a_pin_survives_repeated_compactions` runs three consecutive passes.)
- **The legacy / summarize-all path** (`keep_last_turns == 0`, the `SummarizeAll` rung)
  flips *everything* agent-invisible. Same exception; and its "preserve the most recent user
  message" copy now skips a pin, because re-appending it would both duplicate it and produce
  an unmarked copy that the next compaction would eat.
- **`drop_oldest_agent_visible_turns`**, the bottom recovery rung, runs *before* the
  compaction that honours pins and works by flipping messages agent-invisible. Without a
  clause there it would quietly make every old pin ineligible on the way past, and the
  compaction that followed would find nothing to honour.

The summarizer never sees an honoured pin. Its text survives verbatim a few messages later,
so summarizing it as well would spend the window on the same content twice — a cost that
recurs on every compaction for the life of the session.

### What cannot be pinned

A pin is not honoured on a message carrying `tool_request` or `tool_response` content. A
request and its response are one unit to every provider; exempting one half from a
summarization that hides the other hands the provider a dangling call. Pinning is for
standalone messages, which is exactly the shape of the intended consumer.

An already agent-invisible message is not eligible either. A pin means "do not summarize
this away", not "resurrect this" — otherwise a pin would undo `/compact`'s own hiding on the
next pass.

Ineligible is not the same as budget-evicted: an ineligible pin is simply not selected, and
is not reported as an eviction.

## The bound

A session that accumulates pins forever eventually has a conversation that cannot be
compacted at all — it would deadlock on the very overflow the compactor exists to resolve.
Two caps, in `context_mgmt/pins.rs`, both configurable in the same style as
`BIOROUTER_COMPACT_KEEP_LAST_TURNS`:

| Variable | Default | What it caps |
|---|---|---|
| `BIOROUTER_MAX_PINNED_MESSAGES` | `32` | How many marked messages one compaction may honour. `0` disables pinning. |
| `BIOROUTER_MAX_PINNED_CONTEXT_SHARE` | `0.25` | The share of the model's own context window the honoured set may occupy. |

The count cap alone is not a bound — 32 pinned messages can be 32 pasted files. The share
cap is the one that actually keeps a conversation compactable, because it scales with the
window instead of with a message count. A quarter of the window leaves three quarters for
the summary plus the verbatim recent turns, which is the material the model needs to act.

Selection walks the eligible pins newest-first and **stops at the first one that does not
fit**; everything older is evicted too. Stopping — rather than continuing to look for a
smaller older pin that would still fit — is what makes the kept set a contiguous newest
suffix, which is the stated eviction rule.

The budget is measured with the same byte walk the compaction *trigger* uses
(`message_payload_bytes`). Two independent size estimates would let the compaction outcome
depend on which check happened to run.

## What "loud" means when the bound is hit

Three signals, not one, because each reaches an audience the others do not:

1. a structured `warn!` carrying the evicted count, the honoured count and tokens, both
   caps, and the evicted message ids;
2. an **agent-only** text message spliced into the compacted conversation, so the model
   learns that a message it was told to keep now exists only inside the summary;
3. a **user-only** inline `SystemNotification`, so the human who pinned it finds out.

Two separate messages rather than one, because a `SystemNotification` must never reach a
provider — the Bedrock formatter hard-errors on one (`formats/bedrock.rs`) — so the
model-facing half has to be plain text on an agent-visible message, and the user-facing half
has to be agent-invisible.

Nothing is emitted when every pin fits. A notice that appeared on the happy path would train
everyone to ignore it.

## The degenerate case

When every agent-visible message is pinned there is genuinely nothing left to summarize.
Compaction returns the history untouched rather than buying a billed round-trip for a
summary of nothing and telling the model its context was condensed when it was not. This is
reachable only under a very small pinned set (the budget prevents the general case), and it
is an explicit branch rather than an accident.

## Test gates

```bash
cargo test -p biorouter --lib context_mgmt          # selector, budget, every compaction path
cargo test -p biorouter --lib session::             # the marker across every history rewrite
cargo test -p biorouter --test conversation_writeback_freshness   # parts (a) and (b) composed
```

Two properties the suite is built around, and which a change here must keep:

- **Every pin assertion has a control.** An unmarked message in the same summarized prefix
  must be gone from the agent's context. Without it, a test passes whenever compaction
  quietly did nothing.
- **The end-to-end tests were verified non-vacuous** by forcing `pin_is_eligible` to `false`
  and confirming they fail. A test that passes both with and without the mechanism is
  measuring something else; that is how
  `a_pinned_message_is_not_also_summarized` was caught asserting on a mock's canned summary
  body instead of on the summarizer's actual payload.

## The first consumer

Nothing sets the marker, deliberately. BR-71's `workspace_send_prompt { mode: "note" }`
([#30](https://github.com/BaranziniLab/biorouter/issues/30)) is the first: it appends a note
to another session's conversation and returns success, so it needs to be able to promise the
note reaches the model wherever that conversation has got to. It attaches at
`Message::pinned()`, which documents itself as the attachment point.

Shipping the mechanism without a user-facing way to set it is intentional. A marker with no
producer cannot regress anything, and the alternative — inventing a way to pin messages
before the feature that needs it exists — would fix the shape of the API around a guess.

## Related documentation

- [Conversation writeback freshness](conversation-writeback-freshness.md) — the other half of
  the same guarantee: why a write-back can no longer destroy an append it was never shown.
- [The agent loop](README.md) — the loop that runs the compaction sites.
- [Agent lifecycle hooks](hooks/README.md) — `PreCompact` / `PostCompact`, which bracket the
  same compactions.
