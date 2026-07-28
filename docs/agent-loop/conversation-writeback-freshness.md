# Conversation writeback freshness

> **What this is.** Why every whole-history rewrite of a session goes through a freshness
> guard, how that guard works, and — importantly — the two writeback paths it does **not**
> yet cover, so "the root cause is fixed" is not read more broadly than it should be.
> **Status:** Current for the three compaction sites. Two known-unguarded writebacks remain,
> listed below with the reason each needs a different answer.
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
`sessions.db`. If the GUI is mid-turn on that session, the logged command is destroyed. The
same shape is what a `workspace_send_prompt { mode: "note" }` tool would hit.

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

`ConversationRevision` is `(COUNT(*), MAX(id))` over a session's `messages` rows. This is a
revision counter SQLite already maintains: `messages.id` is `INTEGER PRIMARY KEY
AUTOINCREMENT`, so `sqlite_sequence` never rewinds and a rowid is never reused. An append
raises `max_rowid`; a whole-history rewrite raises it too, **even when the content is
byte-identical**; a delete lowers `count` and burns rowids. The pair cannot ABA. The read is
index-only through `idx_messages_session`, so it costs less than the `COUNT(*)` that
`get_session` already runs.

No schema change was needed, and none is wanted: a `conversation_rev` column would mean a
migration plus three write paths that must each bump it in their own transaction — miss one
and the token silently under-reports, which is exactly the bug `sessions.updated_at` had.

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

## Deliberately unguarded, and why

`replace_conversation` keeps its exact signature and is now the **named exception** for
callers that genuinely own the whole history: `/clear` on both the agent and CLI sides, and
`import_session` / `copy_session` / `diverge_session` / `import_legacy_session`, which all
write into a session they just created. A `/clear` that preserved a tail would not be a
clear.

Two paths can still destroy a concurrent append. Both are real, both are out of scope for
the freshness guard, and each needs a different answer.

- **`conversation_so_far` on `POST /reply`** (`crates/biorouter-server/src/routes/reply.rs`).
  The snapshot here is not a database read at all — it is the HTTP client's own copy of the
  history, with unbounded staleness. No DB-freshness precondition applies. The field exists
  so a client can deliberately overwrite server history (message edits), so guarding it
  needs a client-supplied expected-revision, i.e. a public API change plus a product
  decision about who is authoritative. A failure there is currently only `warn!`-logged
  while the turn continues against the stored history. The desktop app does not send the
  field; any other API client can.
- **`truncate_conversation`** (`crates/biorouter/src/session/session_manager.rs`), behind
  checkpoint restore, `POST /sessions/{id}/edit_message`, and `biorouter term run`. It
  cannot be fixed by a freshness precondition **at all**: the DELETE is ranged on
  `created_timestamp >= ?`, so a concurrently appended message has a `created` of "now" and
  is inside the range by construction, even with a perfectly fresh snapshot. It needs an
  explicit rowid upper bound (which can reuse this design's watermark). It also currently
  leaks `messages_fts` rows and `message_blobs`, and skips `updated_at`.

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
