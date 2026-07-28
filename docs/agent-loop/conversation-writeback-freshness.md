# Conversation writeback freshness

> **What this is.** Why every whole-history rewrite of a session goes through a freshness
> guard, how that guard works, exactly what it does and does not promise, and a scoreboard
> of which writeback paths are closed and which are deliberately not — so "the root cause is
> fixed" is not read more broadly than it should be.
> **Status:** Current. The rewrite paths — the three compaction sites and the client
> write-back on `POST /reply` — are closed. Of the three *deletion* paths, one is now
> bounded and two are deliberately left alone, for two different reasons. The scoreboard is
> [What is closed and what is not](#what-is-closed-and-what-is-not) — read it before
> concluding anything about "the truncation race".
> **Audience:** developers touching the session store, the compaction paths, or any code
> that appends to a session another turn might be running on.

## The hazard

`SessionManager::replace_conversation` DELETEs a session's entire message set and
re-INSERTs the supplied one. It is the only way a message's content can change — there is
no in-place `UPDATE` on the `messages` table — so compaction, `/clear`, imports, copies and
branch divergence all route through it.

That makes it correct for a caller that owns the whole history, and destructive for a
caller that computed its new conversation from a *snapshot*. The pattern is always the
same:

1. read the conversation,
2. spend a multi-second summarization round-trip,
3. write the result back.

Anything another writer appended between steps 1 and 3 is deleted — silently, and *after*
that writer was told its append succeeded. There is no error, no log line, and no way for
the losing writer to find out.

This is not hypothetical and does not depend on any unshipped feature.
`biorouter term log` (`crates/biorouter-cli/src/commands/term.rs`) appends to
`$BIOROUTER_SESSION_ID` from the user's shell hook, in a **separate process**, into the same
`sessions.db`. If the GUI was mid-turn on that session, the logged command was destroyed.
The same shape is what a `workspace_send_prompt { mode: "note" }` tool would hit.

A **deletion** has the same shape and a worse property. `truncate_conversation` DELETEs
every row at or after a timestamp, and a concurrent append's `created` of "now" is inside
that open-ended range *by construction* — so unlike the rewrite, a perfectly fresh snapshot
does not help it. The two hazards therefore need two mechanisms, and the deletion paths do
not all want the same one; see [What is closed and what is not](#what-is-closed-and-what-is-not).

## The discipline

BR-12 recognised this for the *background* eager-compaction path and added
`eager_swap_is_safe(snapshot_len, current_len)` — a message-count compare between the
snapshot and a re-read just before the swap. It was never extended to the in-turn sites,
and it had two residual holes of its own: it is blind to an equal-length change (an
`edit_message` or an `/undo` that drops one message, plus the next turn's user message,
nets to zero), and the check was three separate round-trips with nothing holding them
together, so an append landing between the re-read and the write was still destroyed.

It is replaced by a compare-and-swap inside the rewrite's own transaction.

### The revision token

`ConversationRevision` is `(COUNT(*), MAX(id))` over a session's `messages` rows, plus the
identity of the session ROW they belong to. The `(count, max_rowid)` pair is a revision
counter SQLite already maintains: `messages.id` is `INTEGER PRIMARY KEY AUTOINCREMENT`, so
`sqlite_sequence` never rewinds and a rowid is never reused. An append raises `max_rowid`; a
whole-history rewrite raises it too, **even when the content is byte-identical**; a delete
lowers `count` and burns rowids. The pair cannot ABA *within one session row*. The read is
index-only through `idx_messages_session`, so it costs less than the `COUNT(*)` that
`get_session` already runs.

No message-side schema change was needed, and none is wanted: a `conversation_rev` column
would mean a migration plus three write paths that must each bump it in their own
transaction — miss one and the token silently under-reports, which is exactly the bug
`sessions.updated_at` had.

The one column this did add is `sessions.incarnation` (#51 W3), because a session **id is
reusable**: `create_session` allocates `YYYYMMDD_N` as `MAX(N) + 1` over the `sessions`
table, so once that table is emptied the ids restart at 1, and a one-message session at
`(1, 1)` is reproducible by an entirely different conversation. `incarnation` is minted per
session row from `random()` and never reused, so a basis taken before a wipe can never match
after one, whatever the rowids do. It is backfilled in place, and a legacy `0` compares equal
to a legacy `0` — degrading to the rowid guard alone on such a database rather than refusing
every rewrite on it.

### The guard

`replace_conversation_preserving_tail(session_id, replacement, basis, known)` takes the
revision the caller's view was based on plus the view itself. Inside the rewrite's
transaction it:

1. **verifies the basis prefix is intact** — `COUNT(id <= basis.max_rowid) == basis.count`.
   A concurrent truncate lowers it; a concurrent wholesale rewrite renumbers everything and
   drives it to 0. Either way there is no sound prefix to merge onto, so the rewrite
   refuses and writes nothing (`ReplaceOutcome::Stale`);
2. **reads the rows above the watermark whose ids the caller never saw** and carries them
   onto the tail of the replacement (`ReplaceOutcome::ReplacedPreservingTail`).

Both conditions are necessary, and each has a test that fails without the other. The
watermark alone would resurrect messages the writer itself appended and then deliberately
compacted away. The id set alone would resurrect messages the snapshot saw and the
compaction dropped.

Only a genuine basis mismatch is `Stale`. `SQLITE_BUSY`, I/O errors and a full disk
propagate as `Err`: reporting a busy database as stale would look like data loss, and
reporting it as written would *be* data loss.

### Three invariants a reviewer must check

These are the parts where a plausible-looking simplification is silently wrong.

- **The transaction's first statement must be a write.** sqlx emits a bare (DEFERRED)
  `BEGIN`. Under WAL, a deferred transaction that reads before it writes pins a read
  snapshot, and the later upgrade to a writer returns `SQLITE_BUSY_SNAPSHOT`
  **immediately** — measured at 0.0000 s, i.e. the 5 s `busy_timeout` is bypassed entirely,
  because a busy handler is not consulted for a snapshot upgrade. The
  `UPDATE sessions SET updated_at = …` at the top of `replace_conversation_inner` takes the
  per-file write lock up front so every SELECT below reads true latest-committed state.
  (It also fixes a real pre-existing gap: the rewrite never bumped `updated_at`, so a
  compaction was invisible to the `ORDER BY updated_at DESC` session list.) Removing it as
  a gratuitous write reintroduces intermittent "database is locked" turn failures that look
  like flaky infrastructure. `concurrent_appends_survive_racing_rewrites` asserts that no
  *rewrite* reports that error. (A starved *append* still can, under enough pressure — see
  [What it costs](#what-it-costs); that is ordinary lock contention, not the snapshot bug.)
- **The merged list is built before the insert loop.** A recovered message's blob handle
  must be in `live_blob_uids` when `sweep_orphan_blobs` runs. Append the recovered rows in a
  second pass and the sweep deletes the payload of a message that survives, leaving a
  dangling stub — silent, and only visible on the next read. The recovery scan also
  deliberately does not hydrate blobs: re-inserting the stub verbatim keeps one blob row
  instead of minting a duplicate.
- **`snapshot_for_rewrite` reads the revision before the conversation.** A message landing
  between the two reads then appears in the caller's view instead of being both unseen and
  below the watermark, which is the one ordering under which it could be lost.

## What is guaranteed

The claim to hold this work to — and the one to check a change against — is:

> **A message may only be missing from the verbatim transcript if some compaction was
> actually shown it.**

It is deliberately **not** "a concurrent append can no longer be destroyed". That stronger
sentence is falsified by *correct* code, in two ordinary ways, and writing it down invites
the next reviewer to check the wrong thing and then declare the area closed:

- **A multi-compaction turn legitimately summarizes an append.** Compaction A snapshots, a
  `term log` lands, compaction B snapshots. B's view contains that message, so B folds it
  into a summary and the guard has nothing to preserve — there is nothing foreign above the
  watermark. The message is gone from the verbatim transcript and nothing went wrong: it was
  *shown* to a compaction, and summarizing what it is shown is what compaction is.
- **`snapshot_for_rewrite` reads the revision before the conversation, on purpose.** A
  message landing between those two reads is inside the caller's view, so it is summarized
  rather than carried verbatim onto the tail. The other ordering would leave it both unseen
  *and* below the watermark — actually lost — so revision-first is the correct ordering, and
  the price of it is that the weaker-sounding claim is the true one.

The guarantee is therefore about **unseen** destruction, and unlike the stronger claim it is
checkable: for every message not in the verbatim transcript, there is a compaction whose
*input* contained it.

`concurrent_appends_survive_racing_rewrites` is that check in mechanical form. Its rewriter
carries its entire view forward verbatim and only appends, so it never intentionally drops
anything — which makes "missing from the store" and "swept without being seen" the same
event, and the assertion `lost.is_empty()` a direct test of the guarantee. Only
**acknowledged** appends enter the set it checks.

Two things this claim does not say, both covered elsewhere on this page:

- it says nothing about a message that reaches a compaction and is summarized away for good
  — that is the [preservation marker](compaction-preservation-marker.md)'s job, and a marked
  message *is* carried verbatim through a compaction that sees it;
- it is scoped to the *rewrite* paths. Deletion has its own mechanism and its own scoreboard
  — [What is closed and what is not](#what-is-closed-and-what-is-not).

## What each caller does with a declined swap

The three compaction sites want different failure modes, and getting this wrong trades a
data-loss bug for a liveness bug.

| Site | On `Stale` |
|---|---|
| Background eager compaction (`context_mgmt/run_eager_compaction`) | Abort. The next turn's synchronous fallback re-derives everything, so nothing is lost. |
| In-turn auto-compaction (`agents/agent.rs`, top of `reply()`) | Do **not** fail the turn. Re-read the fresh history, say so inline, continue. The overflow ladder is the backstop. |
| Overflow recovery (`agents/agent.rs`, the `ContextLengthExceeded` arm) | Recompute against the fresh history and retry **once** — each attempt re-spends a billed summarization call. Twice declined: keep the compaction in memory, leave the store untouched, and do not emit `HistoryReplaced`. This is the last rung before "context limit still exceeded". |
| `/compact` (`agents/execute_commands.rs`) | Tell the user. It is user-initiated and trivially re-runnable. |

Two ordering rules in the reply loop, each with a dedicated test:

- the overflow-recovery basis is captured in `reply_internal`, **after** the auto-compaction
  at the top of `reply()` has run — that compaction renumbers every row, so a basis taken
  before it would fail the prefix check on every recovery that follows one, silently
  disabling durable recovery compaction on the largest sessions;
- it is re-seeded after each successful swap, for the same reason.

The basis is deliberately **not** a length compare against the in-memory conversation: the
normalizer merges and drops messages at turn start, and the retry manager pushes messages
that are never persisted, so the two are routinely unequal with zero concurrency.

Every summarization round-trip is billed, including one whose result was discarded — the
provider charged for it either way. Only the attempt that actually landed is marked
`is_compaction_usage`, because that flag means "this usage replaced the context"; marking a
discarded one would reset the live gauge to the summary's size over a history that never
shrank.

## What is closed and what is not

`replace_conversation` keeps its exact signature and is now the **named exception** for
callers that genuinely own the whole history: `/clear` on both the agent and CLI sides, and
`import_session` / `copy_session` / `diverge_session` / `import_legacy_session`, which all
write into a session they just created. A `/clear` that preserved a tail would not be a
clear.

| Path | State |
|---|---|
| The compaction rewrites (`run_eager_compaction`, `reply()`'s auto-compaction, the `ContextLengthExceeded` arm, `/compact`) | **Closed**, and structurally so — `replace_conversation_preserving_tail` is the only rewrite they can reach, and its check runs inside the rewrite's own transaction. |
| `truncate_conversation` from `biorouter term run` | **Closed** — bounded by the caller's own basis (below). |
| `truncate_conversation` from checkpoint restore | **Open, deliberately.** The mechanism exists and is not wired up, because bounding it makes a rewind incomplete. Needs an operator decision (below). |
| `truncate_conversation` from `POST /sessions/{id}/edit_message` | **Open, and not fixable server-side.** The decision is the client's; the server never saw the view it was made from (below). |
| `conversation_so_far` on `POST /reply` | **Closed by refusal**, not by merging — a client copy that does not name every stored message gets a 409 and nothing is written (below). |

### Why truncation needed its own mechanism

`truncate_conversation` cannot be fixed by a freshness *precondition*. The DELETE is ranged
on `created_timestamp >= ?`, so a concurrently appended message has a `created` of "now" and
is inside the range **by construction**, even when the caller's snapshot was perfectly fresh
a microsecond earlier. Checking freshness would pass and the message would still die.

`truncate_conversation_bounded` is the answer: it adds an explicit `id <= basis.max_rowid`
upper bound, reusing this design's watermark, so the cut reaches only the rows the caller's
view covered. It is atomic with the FTS mirror delete, the orphan-blob sweep and the
`updated_at` bump — three things the unbounded path used to skip, which is why it is a
transaction rather than a statement. It refuses a basis from a previous incarnation of the
session id outright (`TruncateOutcome::Stale`), for the same reason the rewrite does.

Note what its bounded form does when the tail *has* moved: it **silently keeps** the extra
rows and reports `Truncated`. That failure mode suits one of the three callers and actively
does not suit another, which is the whole reason they are not interchangeable.

### `biorouter term run` — fixed

`crates/biorouter-cli/src/commands/term.rs`. This is the caller-view bug the bounded variant
was built for. `term run` reads the trailing run of non-assistant messages — what
`term log` recorded from the shell hook since the last reply — folds them into its prompt as
`<shell_history>`, and deletes them from the store. Anything another process appended in
between (a `term log` from a second pane, a GUI turn on the same session) was destroyed: it
was inside the open-ended range, and it was not in the prompt either.

It now reads its conversation through `snapshot_for_rewrite` and passes that revision to
`truncate_conversation_bounded`. "Keep the extra rows" is exactly right here — an append
this process never saw is not part of the shell history it is folding in, and the resumed
session shows it to the model on its own.

The remaining window inside `snapshot_for_rewrite` is closed in the safe direction: the
recorded commands are taken from the *prefix the revision describes*
(`basis.message_count()`), so a message landing between the revision read and the
conversation read is neither folded into the prompt nor dropped from the store. Fold it and
you would show the model a duplicate; drop it without folding it and you would be back to
the original bug.

### Checkpoint restore — open, and it is a semantics question

`crates/biorouter/src/checkpoint/manager.rs`, the `axis.touches_conversation()` arm of
`restore`. This has the **widest** window of the three: between the decision to restore and
the DELETE sit a `pre_restore` shadow-git snapshot and, on a `Both` restore, a full
work-tree checkout — two `spawn_blocking` git operations that can run for seconds on a large
tree. It is therefore the call site most likely to actually eat an append. It is also the
one that must not be changed without a decision, because the available mechanism has the
wrong failure mode for it.

**What a rewind promises.** "Restore to turn N" means the session — files *and* transcript —
is as it was at turn N. The two axes are one promise: `RestoreAxis::Both` checks out the
checkpoint's tree and truncates the conversation at the same `anchor_ts`, so what the model
reads and what is on disk describe the same moment.

**What bounding it would break.** A bounded cut keeps whatever landed after the basis. The
result is a session that is neither turn N nor the present: a transcript that ends at the
anchor and then has a tail of messages referring to files that have just been rewound out
from under them. The next turn resumes into that inconsistency. The redo baseline makes it
worse — the `pre_restore` checkpoint was captured *before* those messages existed, so
"undo the undo" does not restore them either.

**And bounding it would be silent.** `truncate_conversation_bounded` reports `Truncated`
whether or not it kept anything, so the user would be told the restore succeeded while
getting an incomplete one. For this caller the honest options are the two the current
mechanism does not offer:

- **refuse** — treat "the conversation moved since the restore was requested" as a hard stop
  and tell the user, which needs the *full* prefix check the rewrite guard does
  (`COUNT(id <= basis.max_rowid) == basis.count`, plus an unmoved `max_rowid`), not the
  incarnation-only check the bounded truncate does; or
- **rewind and say so** — keep today's complete, authoritative cut, but capture the basis
  anyway and surface "N messages that arrived during the restore were discarded" in
  `RestoreOutcome`, so the loss stops being silent.

Both are product decisions about what "restore" means, not one-line changes, and either one
changes `RestoreOutcome` and the GUI that renders it. Until one is made, the site keeps the
unbounded call **on purpose**: a complete rewind that can destroy a racing append is a
defensible reading of "restore"; an incomplete rewind reported as a complete one is not.

### `POST /sessions/{id}/edit_message` — open, and the server cannot fix it alone

`crates/biorouter-server/src/routes/session.rs`, the `EditType::Edit` arm. Threading a
server-side basis through it would be theatre. **The decision is made in the browser**: the
user scrolls to a rendered message, opens the edit box, and types — and the request that
arrives carries only a `timestamp`. The view that decision was based on is the client's
rendered transcript, of unbounded age; the server never saw it. Any revision the handler
reads is taken *after* the concurrent append has already committed, so it would bound the
delete to a watermark that includes exactly the row the bound exists to protect. The guard
would pass every time and protect nothing — a strictly worse outcome than today's honest
lack of a guard, because it would look fixed.

Bounding it correctly means the client sends what it saw — an expected revision, or the
`msg_uid` of the last message it had rendered. That is a public API change
(`EditMessageRequest` gains a field, plus an OpenAPI and TypeScript-client regeneration) and
a product decision about the refusal path: what should the GUI do when an edit is rejected
because the transcript moved underneath it? That is the same shape as `conversation_so_far`
below, and it likely wants the same answer — refuse loudly rather than merge — because an
edit, like a rewind, is a statement about a transcript the user was looking at.

The sibling `EditType::Diverge` arm has none of this: it truncates a session it just
created, which is the owned-history exception.

### `conversation_so_far` on `POST /reply` — closed by refusal

`crates/biorouter-server/src/routes/reply.rs`. The snapshot here is not a database read at
all — it is the HTTP client's own copy of the history, with unbounded staleness — so no
DB-freshness precondition applies to *it*. The answer was to stop treating the field as
authoritative. `apply_client_writeback` reads the stored history, and if the client's copy
does not name every message the server holds (message ids are durable and server-assigned,
so naming one is proof the client saw it) it answers **409 `conversation_out_of_date`** and
writes nothing; the client re-reads and retries. What it does store goes through
`replace_conversation_preserving_tail`, so a row landing between the check and the write is
preserved rather than deleted, and a `Stale` verdict is answered the same way as a stale
client copy.

This is a deliberate deprecation of the field's overwrite semantics. Nothing in this
repository sends it — the desktop client posts `session_id` + `user_message` only — so the
compatibility cost falls entirely on out-of-tree API clients, which now get a loud 409
instead of silently deleting a user's messages. Making the field *usable* again against a
moving session (rather than merely safe) still needs a client-supplied expected-revision and
a product decision about who is authoritative.

## What this does not fix

**A guarded write only gets the message stored; it does not keep it reaching the model.**
Compaction keeps the last `keep_last_turns` turns verbatim and summarizes the rest, so a
message this guard just saved is dissolved into a summary a few turns later — the same
broken promise, arriving more slowly. The answer is the per-message preservation marker,
documented separately in
[The compaction preservation marker](compaction-preservation-marker.md). The two halves are
one guarantee and neither is useful alone.

**This makes concurrent writes safe, not rare.** Two turns can still run on one session:
the apps agent socket never takes the per-session turn guard and two browser tabs of one
app share a session through the `localStorage` client id; and `AppState::active_turns` is
process-local, so the CLI and the daemon on the same `sessions.db` are not ordered against
each other at all. Those turns will now burn duplicate summarization calls where one used
to silently delete the other's work. Preventing the overlap is a separate problem.

One adjacent pre-existing defect is also untouched and deliberately not folded in here: an
**aborted eager compaction is never billed** (`run_eager_compaction` returns before
`apply_session_metrics`, a systematic under-count that touches budgets and wants its own
review).

**`PreCompact` can fire without a matching `PostCompact`, and that is by design.** The
pre-existing instance is the eager abort path; the declined-swap arms this work added to
in-turn auto-compaction and to `/compact` are two more, and the summarizer-error arms at
both sites are two older ones. It is not a bracket to be balanced. `PreCompact` is fired
*speculatively*, before a summarization whose outcome is unknown — which is exactly what
gives a hook its chance to capture the transcript before it is replaced, and what makes it
"pre". `PostCompact` means a compaction landed. Firing it on a skip would tell every
consumer the transcript was replaced when it was not, so a hook that re-indexes the
history, invalidates a cache or reports "compacted to N tokens" would act on a history that
never changed — a worse failure than the asymmetry. The contract is written down for hook
authors under [Known limitations](hooks/hooks-reference.md#known-limitations).

## What it costs

A whole-history rewrite holds SQLite's **single write lock for an O(history) transaction** —
measured at ~310 ms for 300 messages in a debug build. The freshness guard does not add to
that (312.5 ms vs 312.6 ms p50 over 30 rewrites of the same history: the guard's two extra
SELECTs are index-only and run inside a transaction that was going to rewrite every row
anyway), but it does not remove it either, and it is worth knowing before assuming a
guarded rewrite is free.

Under a synthetic 6-appender storm with a rewriter looping on a 2 ms gap, the rewriter
re-takes the lock faster than a starved appender's 5 s `busy_timeout` can expire, and
roughly **1 append in 300 fails with `SQLITE_BUSY`** ("database is locked"). Three things
make this a cost rather than a regression:

- it is **pre-existing** — it reproduces identically on the unguarded `replace_conversation`
  path, so it is a property of whole-history rewriting, not of the guard;
- it is **loud** — `add_message` returns `Err` and the caller is told the write failed,
  which is the opposite of the silent destruction this work exists to fix;
- it **disappears at realistic compaction gaps** — 0 failures in 300 appends at both 25 ms
  and 50 ms. Compactions in production are minutes apart, not milliseconds.

`conversation_writeback_stress.rs` therefore tolerates and counts a busy *append* (see the
doc comment on `is_busy`) while treating a busy *rewrite* as a hard failure — a rewrite that
reports `database is locked` means its transaction read before it wrote, which is the
`SQLITE_BUSY_SNAPSHOT` bug the write-first `UPDATE sessions` exists to prevent.

## Related documentation

- [The compaction preservation marker](compaction-preservation-marker.md) — the other
  half of the same guarantee: keeping a stored message out of the summary.
- [The agent loop](README.md) — the loop that runs the three compaction sites.
- [Agent lifecycle hooks](hooks/README.md) — the `PreCompact`/`PostCompact` pair discussed above.
- [Session branching design](designs/session-branching.md) — the other consumer of stable `msg_uid`s across a rewrite.
- [Shadow-git checkpoints design](designs/shadow-git-checkpoints.md) — checkpoint restore, one of the `truncate_conversation` callers named above.
