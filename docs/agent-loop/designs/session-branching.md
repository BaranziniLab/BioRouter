# BR-45 — Session branching UX (fork/tree) with stable message ids

**Status:** design (implementation not started)
**Lens:** UX (ux P-15); best-in-class branching = Pi (`/tree`, `/fork`, `/clone`), rewind = Claude Code.
**Depends on / unblocks:** stable ids are a prerequisite for BR-53's message-level SSE patch protocol.

---

## Problem (grounded in code, with file:line)

BioRouter's message identity is **positional and re-derived on every load**, and every branch/edit operation anchors on a **whole-second timestamp**. Both are fragile foundations for stable UI anchors, an edit/patch protocol, and a real branching tree.

1. **Synthetic ids are positional.** `get_conversation` selects only
   `role, content_json, created_timestamp, metadata_json` (no id) and then stamps
   each message with `msg_{session_id}_{idx}` from its *enumerate index*
   (`crates/biorouter/src/session/session_manager.rs:1812-1817,1836`). The id is
   a function of ordering, not of the message. This is exactly
   `state-awareness.md` gap #10: "any history rewrite renumbers messages — fragile
   for anything that wants stable per-message references (UI anchors, edit
   provenance)."

2. **The durable rowid is thrown away, and even it is not stable.** The
   `messages` table has `id INTEGER PRIMARY KEY AUTOINCREMENT`
   (`session_manager.rs:1105`) but `get_conversation` never reads it. More
   importantly, `replace_conversation_inner` does `DELETE FROM messages ... ` then
   re-`INSERT`s the whole list (`session_manager.rs:1872-1904`) — so rowids are
   reassigned on *every* compaction/edit/diverge. The single operation we most
   need stability across (history rewrite) is precisely the one that renumbers
   both the synthetic id and the rowid.

3. **Branch / edit / fork anchor on a seconds-granularity timestamp.**
   - `Message::user()` / `Message::assistant()` set `created: Utc::now().timestamp()`
     — **whole seconds** (`crates/biorouter/src/conversation/message.rs:614,625`).
   - `EditMessageRequest { timestamp: i64 }` (`routes/session.rs:65-66`) and
     `truncate_conversation` delete `WHERE created_timestamp >= timestamp`
     (`session_manager.rs:2302-2311`).
   - `DivergeSessionRequest { truncate_after: Option<i64> }`
     (`routes/session.rs:83-96`, doc-comment even mislabels it "(ms)") flows to
     `trim_to_last_complete_answer`, which keeps `m.created <= ts`
     (`session_manager.rs:909-928`).
   Because `created` is seconds, **two messages produced in the same second share
   an anchor**: an edit/diverge at one of them silently truncates the other
   (`>=` / `<=` are inclusive). The frontend passes `message.created` as the
   anchor (`ui/desktop/src/components/BioRouterMessage.tsx:187`
   `truncateAfterMs={message.created}`), so this is a live user-facing path, not
   theoretical.

4. **Branching is a one-level lineage pointer, no tree.** `diverged_from TEXT`
   (schema `session_manager.rs:1094`, migration 8 `:1435`) records a single parent
   session id (`diverge_session`, `:2208-2261`); the only "tree" affordance is
   sibling name numbering (`compute_branch_name`, `:2267-2286`). There is no API
   to fetch a branch forest, no fork-point annotation, and no GUI/CLI tree view —
   `state-awareness.md` gap #10 and the Pi comparison call this out as the missing
   first-class branching UX.

5. **The frontend already keys off the unstable id.** `BioRouterMessage.tsx:98`
   (`messages.findIndex((msg) => msg.id === message.id)`) and `:200`
   (`key={toolRequest.id}`) rely on ids that change across reloads/rewrites, so a
   patch protocol (BR-53) built on today's ids would mis-target updates.

`Message.id` is already a serialized, camelCase, `Option<String>` field on the
wire (`message.rs:583-593`), so making it *stable* is a value change, not a shape
change — the client already receives it.

---

## Design

Two coupled changes: (A) a **durable per-message id** that survives history
rewrites, and (B) a **branch tree** anchored on that id instead of on timestamps.

### Data model

**`messages.msg_uid TEXT`** (new column, migration 11).
- Written at insert time from `Message.id` (mint a UUID if `None`), and
  **preserved verbatim** by `replace_conversation_inner` so kept messages keep
  their id across compaction/edit/diverge.
- `UNIQUE(session_id, msg_uid)` index. Uniqueness is per-session because
  `copy_session`/`diverge` intentionally carry ids into a child (see below).
- Backfill existing rows deterministically from the durable rowid:
  `UPDATE messages SET msg_uid = 'm' || id WHERE msg_uid IS NULL`. This is stable
  because the backfill runs once and every subsequent rewrite preserves it.
- UUID scheme: **UUIDv7** (time-ordered, so ids sort by creation and aid
  debugging). `uuid` is already a dep at `crates/biorouter/Cargo.toml:48`
  (`features = ["v4"]`); add `"v7"`.

**`sessions.branch_point_msg_uid TEXT`** (new column, migration 11).
- Records the exact parent message a branch was cut at (replaces the fuzzy
  timestamp). Combined with the existing `diverged_from`, sessions form a
  navigable forest: `diverged_from` = parent session, `branch_point_msg_uid` =
  the edge label (the fork message).

`Message.id: Option<String>` stays the carrier (no struct-shape change). Add a
small helper:

```rust
// crates/biorouter/src/conversation/message.rs
pub fn new_message_id() -> String { uuid::Uuid::now_v7().to_string() }
impl Message { pub fn ensure_id(&mut self) { if self.id.is_none() { self.id = Some(new_message_id()); } } }
```

### Module layout — files to change

- `crates/biorouter/src/conversation/message.rs` — `new_message_id()` / `ensure_id()`.
- `crates/biorouter/src/session/session_manager.rs` — schema col, migration 11,
  `get_conversation`, `add_message`, `replace_conversation_inner`,
  `diverge_session`, new `truncate_conversation_after_uid`, `Session.branch_point_msg_uid`
  field + `FromRow`, `session_tree()` builder.
- `crates/biorouter-server/src/routes/session.rs` — new optional request fields,
  new `GET /sessions/{id}/tree` route; then `just generate-openapi`.
- `ui/desktop/src/components/{BioRouterMessage,MessageDivergeLink}.tsx` — pass
  message id as anchor; new `SessionTree` view (later phase). Regenerate API client.
- `crates/biorouter-cli/src/tui/` — `/fork` `/tree` `/clone` commands (later phase).

### Key APIs / signatures

```rust
// session_manager.rs — persistence
async fn get_conversation(&self, session_id: &str) -> Result<Conversation>;
//   SELECT role, content_json, created_timestamp, metadata_json, msg_uid, id ...
//   message.id = Some(msg_uid.unwrap_or_else(|| format!("msg_{session_id}_{idx}"))) // dual-read fallback

async fn add_message(&self, session_id: &str, message: &Message) -> Result<()>;
//   INSERT ... msg_uid = message.id.clone().unwrap_or_else(new_message_id)

fn replace_conversation_inner(pool, session_id, conv) -> Result<()>;
//   INSERT ... msg_uid = m.id.clone().unwrap_or_else(new_message_id)  // PRESERVE existing ids

// New: anchor by durable id instead of timestamp
async fn truncate_conversation_after_uid(&self, session_id: &str, msg_uid: &str) -> Result<()>;
//   delete rows whose ordinal position (ORDER BY timestamp, id) is AFTER the anchor uid's row
async fn diverge_session(&self, sm: &SessionManager, session_id: &str,
                         name: Option<String>, anchor_uid: Option<String>) -> Result<Session>;
//   trims at anchor_uid; sets child.branch_point_msg_uid + diverged_from

// New: tree read model
pub struct SessionTreeNode { pub session_id: String, pub name: String,
    pub diverged_from: Option<String>, pub branch_point_msg_uid: Option<String>,
    pub children: Vec<SessionTreeNode> }
pub async fn session_tree(&self, session_id: &str) -> Result<SessionTreeNode>; // walk to root, build forest
```

```rust
// routes/session.rs — additive, back-compatible
struct EditMessageRequest   { timestamp: i64, message_id: Option<String>, edit_type: EditType } // id wins when present
struct DivergeSessionRequest{ name: Option<String>, truncate_after: Option<i64>, truncate_after_id: Option<String> }
// GET /sessions/{id}/tree -> SessionTreeNode
```

### Control flow

1. **Turn** → `add_message` stamps a UUIDv7 `msg_uid`; the id round-trips to the
   client already (`Message.id`).
2. **Load** → `get_conversation` returns each message's durable `msg_uid` (or the
   positional fallback for un-backfilled NULLs).
3. **Compaction / edit** → `replace_conversation_inner` re-writes the message list
   but preserves each kept message's `msg_uid`; only newly-minted (e.g. summary)
   messages get fresh ids. Ids are now stable across the exact op that used to
   renumber them → BR-53 can address messages by id.
4. **Fork / diverge** → cut at `truncate_after_id` (uid), unambiguous even when
   two messages share a second; child records `diverged_from` + `branch_point_msg_uid`.
5. **Tree** → `GET /sessions/{id}/tree` reconstructs the forest from
   `diverged_from` edges; GUI renders a rewind/branch tree; CLI exposes `/tree`.

---

## Alternatives considered (and why rejected)

- **Reuse the DB rowid as the stable id.** Rejected: `replace_conversation_inner`
  DELETEs + re-INSERTs on every compaction/edit (`session_manager.rs:1872-1904`),
  reassigning rowids — so rowid is *not* stable across the very operations that
  need stability. A carried, preserved `msg_uid` is required.
- **Keep timestamp anchoring but widen to milliseconds.** Rejected: shrinks but
  does not eliminate collisions (parallel tool messages, imports, same-ms turns),
  and still gives no durable handle for UI anchors or the patch protocol. It's a
  patch on a positional scheme, not a fix.
- **Content-hash ids.** Rejected: identical messages collide; editing a message
  changes its id (defeating the point); hashing large tool blobs is wasteful.
- **Adopt Pi's single-JSONL `parentId` session tree wholesale**
  (`external/pi.md` §"State tracking & checkpoints"). Rejected: BioRouter persists
  to SQLite (one `sessions` row + append-only `messages`), and re-architecting to
  a per-entry parent-tree file is far beyond this item. We instead port Pi's
  *semantics* (a `parentId`/fork-point tree) onto the existing schema via
  `diverged_from` + `branch_point_msg_uid`.
- **Dedicated `branch_edges` table (multi-parent / merges).** Rejected for v1: a
  self-referential column + fork-point covers a strict tree; revisit only if we
  ever need merge/clone-with-multiple-parents.

---

## Migration & compatibility (config, persisted state, rollout)

- **Migration 11** (`apply_migration`, bump `CURRENT_SCHEMA_VERSION` 10 → 11 at
  `session_manager.rs:21`, forward-only per existing policy):
  1. `ALTER TABLE messages ADD COLUMN msg_uid TEXT`
  2. `UPDATE messages SET msg_uid = 'm' || id WHERE msg_uid IS NULL` (deterministic
     rowid backfill; stable thereafter because rewrites preserve it)
  3. `CREATE UNIQUE INDEX idx_messages_uid ON messages(session_id, msg_uid)`
  4. `ALTER TABLE sessions ADD COLUMN branch_point_msg_uid TEXT`
- **Dual-read:** `get_conversation` uses `msg_uid` when present and falls back to
  the legacy `msg_{session}_{idx}` only when it is NULL — covers any row an
  in-flight upgrade hasn't backfilled. New inserts always carry a uid.
- **API back-compat:** `timestamp` / `truncate_after` fields are retained; the new
  `message_id` / `truncate_after_id` fields are optional and take precedence when
  present. Old desktop clients (and the current timestamp path) keep working; the
  regenerated OpenAPI client picks up the id fields.
- **No config flag required** — the change is additive. The tree *UX* (Phase 3)
  can sit behind `ALPHA=true` until polished. No secrets/config-file changes.
- **Rollout order:** ship backend (Phases 1–2) first so ids are stable and
  populated before any client relies on them; ship GUI/CLI tree (Phase 3) after.

---

## Test plan (unit/integration; what proves no regression)

**Unit — `cargo test -p biorouter` (session_manager, message):**
- *Id stability across rewrite:* add N messages, snapshot ids, run
  `replace_conversation` (compaction sim); assert kept-message ids are unchanged
  and a newly-inserted summary message gets a fresh id.
- *get_conversation* returns `msg_uid` ids; NULL-uid fallback yields the legacy
  positional id.
- *Same-second regression:* two messages with identical `created`;
  `truncate_conversation_after_uid` at the first removes only the strict suffix,
  proving the `>=`-timestamp over-deletion bug (item 3) is gone.
- *diverge by uid* sets `diverged_from` + `branch_point_msg_uid`, leaves the
  original untouched, trims correctly.
- *Migration 11:* seed a v10 DB (rows lacking `msg_uid`), run migrations, assert
  every row has a unique non-null uid and that ids are stable across a reload.
- *`session_tree`* rebuilds a 3-level forest with correct parent/child edges.

**Integration — `cargo test -p biorouter-server` (session routes):**
- `edit_message`/`diverge` by `message_id` / `truncate_after_id`.
- `GET /sessions/{id}/tree` shape.
- **No-regression:** existing route tests `routes/session.rs:758-880` (diverge
  copies history, keeps original, records lineage) and manager tests
  `session_manager.rs:3262-3347` (branch lineage/sibling naming) must pass
  unchanged via the retained timestamp path.

**Frontend — `cd ui/desktop && npm run test:run`:**
- `MessageDivergeLink.test.tsx` updated to pass `messageId`; assert stable
  react-key behavior in `BioRouterMessage` across a simulated reload.

---

## Effort & phasing (first mergeable slice)

- **Phase 1 (first slice, S–M):** migration 11 + mint/persist/read stable
  `msg_uid` in `add_message` / `replace_conversation_inner` / `get_conversation`
  with dual-read fallback. **No API or UX change.** This alone delivers stable ids
  and unblocks BR-53; fully unit-testable in `biorouter`. This is the mergeable
  slice.
- **Phase 2 (M):** switch edit/fork/diverge anchoring to `msg_uid` (new optional
  request fields, keep timestamp), add `truncate_conversation_after_uid` and
  `branch_point_msg_uid`; frontend passes `message.id`. Fixes the same-second
  truncation bug.
- **Phase 3 (L):** first-class tree UX — `GET /sessions/{id}/tree`, a GUI
  branch/rewind view, and `/fork` `/tree` `/clone` CLI commands. Optional
  Pi-style branch-summary-on-abandon as a follow-up.

---

## Open questions for the human (only genuine product decisions)

1. **UUID scheme:** UUIDv7 (time-sortable, debug-friendly) vs plain v4? (Low
   stakes; recommend v7.)
2. **Tree UX surface:** a dedicated left-rail session-tree view (Pi `/tree`), an
   inline "rewind to here" affordance per message (Claude Code), or both?
3. **Fork scope:** conversation-only for v1, or also snapshot workspace files
   (ties into the separate checkpoint/shadow-git gap, `state-awareness.md` #2)?
   Recommend conversation-only now.
4. **Terminology:** keep `diverge`/`diverged_from`, or rename to `fork`/`branch`
   for consistency with Pi and Claude Code UX vocabulary?
5. **Branch summaries:** inject a distilled summary of an abandoned branch on
   switch (Pi's `BranchSummaryEntry`) now, or defer?
