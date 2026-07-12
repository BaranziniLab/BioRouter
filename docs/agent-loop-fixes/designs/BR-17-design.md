# BR-17 — Unified, auto-promoted cross-session memory

**Status:** design (implementation not started)
**Lens:** U (ux P-28); `state-awareness.md` gaps #5/#6; `compaction.md` context.
**Inspired by:** Codex `~/.codex/memories/` (distilled, ranked, self-maintained), Claude Code auto-memory (`MEMORY.md` first 200 lines auto-loaded every session), Gemini CLI Auto Memory (idle-session mining + a `/memory inbox` review queue).

---

## Problem (grounded in code, with file:line)

Cross-session memory today is **three disjoint stores with no shared index and no auto-promotion**. The user must know which one to ask for, and durable facts never accumulate on their own.

1. **Chat Recall is substring `LIKE`, ranked only by recency.**
   `ChatHistorySearch::parse_keywords` wraps every whitespace token as `%word%`
   (`crates/biorouter/src/session/chat_history_search.rs:117-122`), and
   `build_sql` OR-joins `LOWER(json_extract(value,'$.text')) LIKE ?` clauses with a
   final `ORDER BY m.timestamp DESC LIMIT ?`
   (`chat_history_search.rs:124-172`). There is no relevance ranking, no
   stemming, no phrase match — a paraphrase misses, and on a large history the
   "best" hit is simply the most recent one. It is invoked through
   `SessionManager::search_chat_history`
   (`crates/biorouter/src/session/session_manager.rs:777-788`) from the
   `chatrecall` platform tool
   (`crates/biorouter/src/agents/chatrecall_extension.rs:190-245`).
   `chatrecall` is also **off by default** (`default_enabled: false`,
   `crates/biorouter/src/agents/extension.rs:57-68`).

2. **Knowledge bases / Soul are opt-in per query, never auto-injected.** The
   git-backed markdown wiki is the closest thing to durable memory, and a
   built-in **Soul** KB (`kb_id="soul"`) holds durable user facts
   (`crates/biorouter-mcp/src/knowledge/instructions.md:31-34`). But the system
   prompt only *asks* the model to "consult the relevant knowledge base
   (including Soul) first" (`crates/biorouter/src/prompts/system.md:31-36`) — it
   is a suggestion, gated on tool availability
   (`prompt_manager.rs:420-438` test), and nothing puts Soul facts into context
   unless the model chooses to call `kb_search`.

3. **Conversation-ingest is fully manual.** `platform__ingest_conversation`
   (`crates/biorouter/src/agents/knowledge_tool.rs:23-99`) only runs when the
   user says "remember this chat"; it resolves a target KB and runs
   `knowledge::conversation_ingest::ingest_conversation`. Nothing triggers it
   automatically at session end.

The three never share an index (`state-awareness.md:200-203`) and there is no
"promote useful facts automatically" step (`state-awareness.md:331-340`). Net
effect: strong-but-siloed stores, poor recall, no accumulation of lab know-how.

**Where injection can happen.** The system prompt is assembled once per turn by
`SystemPromptBuilder::build` (`prompt_manager.rs:104-176`) at
`reply_parts.rs:148-156`; the MOIM `<info-msg>` (working dir + time + each
platform extension's `get_moim`) is re-spliced **every** provider call by
`inject_moim` (`crates/biorouter/src/agents/moim.rs:12-58`,
`extension_manager.rs:1482-1521`). Turns end at the Stop-hook `Proceed` branch
(`agent.rs:2141-2160`). These are the three hook points a memory feature must
plug into.

---

## Design

Three cooperating pieces behind one config flag. Nothing is destructive; every
piece degrades to today's behavior when disabled.

### Piece 1 — FTS5 index for chat recall (read path, quality)

**Data model.** Add SQLite FTS5 (already compiled into the bundled sqlite; used
by the KB's BM25 search, `crates/biorouter-mcp/src/knowledge/store.rs:172-219`)
as a contentless external-content table mirroring `messages`:

```sql
-- migration 11 (crates/biorouter/src/session/session_manager.rs apply_migration)
CREATE VIRTUAL TABLE messages_fts USING fts5(
    text,                       -- extracted plain text of content_json
    session_id UNINDEXED,
    message_id UNINDEXED,
    role UNINDEXED,
    ts UNINDEXED,
    content='',                 -- external/contentless: we own the row lifecycle
    tokenize='porter unicode61'
);
```

We do **not** use SQLite content-sync triggers, because the searchable text is
not a raw column — it is the flattened text of `content_json` (produced today by
`ChatHistorySearch::extract_text_content`,
`chat_history_search.rs:207-220`). Instead the index is maintained in Rust at
the two write sites that already own message lifecycle:

- `SessionStorage::add_message` (`session_manager.rs:1843-1870`) — after the
  `INSERT INTO messages`, extract text and `INSERT INTO messages_fts`.
- `SessionStorage::replace_conversation_inner`
  (`session_manager.rs:1872-1913`, the compaction/edit DELETE+reinsert path) —
  `DELETE FROM messages_fts WHERE session_id=?` then re-insert the surviving
  **user-visible** messages, so a compacted session stays searchable but does not
  double-count. (Note: compaction flips messages `agent_invisible` but keeps them
  `user_visible`, `compaction.md:83-84` — recall should search what the *user*
  saw, so index on `user_visible`.)

**Backfill.** Migration 11 populates `messages_fts` from existing `messages` in a
single pass inside the migration transaction (extract text row-by-row, skip empty).
For very large DBs this is O(n) once; acceptable as a one-time upgrade cost and
consistent with the existing token_events backfill philosophy (`migration 10`).

**Query.** New module `crates/biorouter/src/session/chat_fts.rs`:

```rust
pub fn sanitize_fts_query(user_query: &str) -> String; // tokens -> `"tok"* OR "tok"*`, strips FTS operators
pub fn extract_searchable_text(content: &[MessageContent]) -> String; // reuse today's logic
```

`ChatHistorySearch::build_sql` (`chat_history_search.rs:124-172`) is rewritten to:

```sql
SELECT s.id, s.description, s.working_dir, s.created_at, f.role, f.text, f.ts
FROM messages_fts f
JOIN sessions s ON f.session_id = s.id
WHERE messages_fts MATCH ?              -- sanitized query
  AND s.id != ?                          -- exclude current
  AND f.ts >= ? AND f.ts <= ?            -- optional date filters
ORDER BY bm25(messages_fts) ASC          -- relevance, best first
LIMIT ?;
```

The public struct/method surface (`ChatHistorySearch::new/execute`,
`ChatRecallResults`) is **unchanged**, so `chatrecall_extension.rs` and
`SessionManager::search_chat_history` callers need no edits. Ranking flips from
recency to relevance; the `chatrecall` extension is switched to
`default_enabled: true` (`extension.rs:63`) now that it is worth always having.

### Piece 2 — Auto-distillation into a unified memory store (promotion path)

**Store = the Knowledge subsystem (no new store).** Unification means one durable
home: distilled facts land in the **Soul** KB (`kb_id="soul"`), the same store
`kb_search`/`ingest_conversation` already read/write
(`biorouter-mcp/src/knowledge/service.rs`). This reuses git history, provenance,
and the existing UI instead of inventing a fourth silo. A dedicated hidden
`auto-memory` KB is a config-selectable alternative (see Open Questions).

**Metadata for ranking** (Codex-style `usage_count`/`last_usage`,
`codex-cli.md:30`). Distilled memory pages carry YAML frontmatter fields the KB
already tolerates:

```yaml
source_session_id: <id>
generated_at: <rfc3339>
last_used_at: <rfc3339>
usage_count: <int>
auto_promoted: true
confidence: low|medium|high
```

**Trigger — idle-session scan** (Gemini model: idle ≥ N hours, ≥ M user
messages, `gemini-cli.md:94-98`). New module
`crates/biorouter/src/memory/promote.rs`:

```rust
pub struct DistillConfig { idle_secs: i64, min_user_msgs: usize, enabled: bool, target_kb: String }
pub async fn scan_and_promote(sm: &SessionManager, completer: Box<dyn Completer>, cfg: &DistillConfig) -> Result<Vec<PromotionOutcome>>;
async fn distill_session(session: &Session, completer: &dyn Completer, svc: &KnowledgeService, cfg: &DistillConfig) -> Result<PromotionOutcome>;
```

`scan_and_promote` selects `SessionType::User` sessions whose `updated_at` is
older than `idle_secs`, that have ≥ `min_user_msgs` user messages, and that are
not yet marked distilled (a `memory.v0 { distilled_at, source_updated_at }` key
in `sessions.extension_data`, reusing the versioned-key scheme at
`extension_data.rs:28-37` — same mechanism todos use). For each, it runs a
**bounded** distillation.

**Distillation is a single fast-model call, not a sub-agent** (cheap, matches
Codex "distill into raw_memory + summary"). New prompt
`crates/biorouter/src/prompts/distill_memory.md`: input = the session transcript
(reusing `format_message_for_compacting`-style rendering,
`context_mgmt/mod.rs:351-428`); output = a short list of durable, user-scoped
facts (preferences, tools, working style, project constants) or the sentinel
`NO_DURABLE_FACTS`. Called via `provider.complete_fast(...)`
(`providers/base.rs:460-489`) — the same fast model compaction uses. The result
is written to Soul via `KnowledgeService` (dedup against existing pages by
BM25 search before appending, so re-distilling a resumed session does not
duplicate). Reusing `knowledge::conversation_ingest::ingest_conversation`
(`knowledge_tool.rs:63-79`) with a memory-focused focus string is the heavier
alternative for a later phase; the lightweight `complete_fast` path is the first
slice.

**Where it runs.** Two entry points, both cheap and off the hot path:

- **Session-start scan** (primary, matches Gemini): when a new user session's
  first turn begins, spawn a detached `tokio::task` that runs `scan_and_promote`
  for *other* idle sessions. Hook at the top of `Agent::reply`
  (`agent.rs:1240`), gated by the flag and a process-wide debounce so concurrent
  sessions don't double-scan.
- **Scheduler tick** (optional, for long-lived daemons): register a low-frequency
  job in `crates/biorouter/src/scheduler.rs` (tokio-cron-scheduler already
  present) that calls the same function.

Distillation never blocks a user turn and never mutates the source session's
messages — it only writes memory pages and stamps `extension_data`.

### Piece 3 — Always-on, bounded memory digest (auto-injection)

**Read-back into every session** (Claude "first 200 lines of MEMORY.md load into
every session", `claude-code.md:107-112`; Codex injects memories as developer
instructions, `codex-cli.md:30`). New builder step on `SystemPromptBuilder`:

```rust
// prompt_manager.rs
pub fn with_memory_digest(mut self, digest: Option<String>) -> Self;
```

rendered into the existing "# Additional Instructions" tail
(`prompt_manager.rs:167-175`) under a clearly framed, untrusted-data header
(`## What I remember about this user (auto-collected; verify before relying)`),
capped at `BIOROUTER_MEMORY_DIGEST_MAX_CHARS` (default ~2 KB) and top-N entries.
The digest is produced by `memory::digest::build_digest(svc, top_n, max_chars)`
which reads the highest-ranked Soul pages (rank = recency-decayed
`usage_count`). It is computed in `reply_parts.rs:148-156` and passed to
`.with_memory_digest(...)`.

**Cache safety.** The digest must be *stable within a session* so it does not
bust the prompt cache every turn (the same reason MOIM uses hour/minute
granularity, `moim.rs`, `prompt_manager.rs:184-186`). Compute it once per session
(memoize on the `Agent`, keyed by session id, invalidated only when a promotion
writes) rather than re-ranking per turn. This keeps injection query-independent
and cache-friendly; query-aware injection (ranking against the live user message
via MOIM's `get_moim`) is a deliberate later phase, since `get_moim` currently
receives only `session_id` (`mcp_client.rs:113`, `todo_extension.rs:197`).

**Usage feedback loop.** When the model then calls `kb_search` on Soul and a
memory page is a hit, bump its `usage_count`/`last_used_at`. Simplest first slice:
bump on inclusion in the digest (a memory that keeps getting injected is proven
useful); precise per-hit accounting is a follow-up.

### Module layout & files

Create:
- `crates/biorouter/src/session/chat_fts.rs` — query sanitization + text extraction helpers.
- `crates/biorouter/src/memory/mod.rs`, `promote.rs`, `digest.rs` — promotion + digest.
- `crates/biorouter/src/prompts/distill_memory.md` — distillation prompt.

Change:
- `session_manager.rs` — `CURRENT_SCHEMA_VERSION` 10→11 (`:21`); `apply_migration` arm 11 (`:1325`) creating + backfilling `messages_fts`; index-maintenance in `add_message` (`:1843`) and `replace_conversation_inner` (`:1872`).
- `chat_history_search.rs` — FTS5 `MATCH` + `bm25()` in `build_sql`/`fetch_rows` (`:93-172`); keep public API.
- `extension.rs:63` — flip `chatrecall` `default_enabled` to `true`.
- `prompt_manager.rs` — `with_memory_digest` builder + render (`:104-176`, `:209-219`).
- `reply_parts.rs:148-156` — compute + pass the digest.
- `agent.rs:1240` — spawn the detached idle-scan at `reply` start; memoize digest.
- `crates/biorouter/src/config` / env — new params (below).

### Control flow (one distillation + one recall)

```
new user turn (agent.rs:1240)
  └─ spawn detached: memory::promote::scan_and_promote
        └─ for each idle User session not yet distilled:
              render transcript → complete_fast(distill_memory.md)
              → facts | NO_DURABLE_FACTS
              → dedup vs Soul (BM25) → KnowledgeService.write_page (Soul, provenance+meta)
              → stamp sessions.extension_data[memory.v0]
  └─ build system prompt (reply_parts.rs:148)
        └─ memory::digest::build_digest(Soul, top_n, max_chars)  [memoized per session]
        └─ .with_memory_digest(digest)  → "# Additional Instructions" tail
  ... later, user asks "what did we decide about X" ...
  └─ model calls chatrecall → ChatHistorySearch (FTS5 MATCH + bm25) → ranked hits
```

---

## Alternatives considered (and why rejected)

- **Embeddings / vector index for recall instead of FTS5.** Higher recall on
  paraphrase, but needs an embedding provider, a vector store, and background
  re-embedding — heavy, and offline/local-first hostile. FTS5 is already linked,
  zero-dependency, and BM25 is a large win over `LIKE` for near-zero cost. Vectors
  stay a future option layered on the same store.
- **A brand-new fourth memory store (`~/.config/biorouter/memory/MEMORY.md`).**
  Literal Codex/Claude mirror, but it *adds* a silo — the opposite of "unified."
  Reusing Soul/Knowledge gives git history, provenance, dedup, export, and an
  existing UI for free.
- **Content-sync FTS5 triggers.** Cleaner in pure-SQL schemas, but the indexed
  text is a *derived* flattening of `content_json`, not a column; triggers can't
  run the Rust extraction. App-side maintenance at the two existing write sites is
  simpler and keeps extraction logic in one place.
- **Auto-inject the full Soul KB every turn.** Blows the token budget and busts
  the prompt cache. A bounded, memoized, top-N digest is the Claude-Code
  "first 200 lines" discipline.
- **Distill on every turn-end (Stop `Proceed`, agent.rs:2141).** Too eager,
  expensive, and distills half-finished work. Idle-session scan (Gemini) distills
  a *settled* session once.
- **Full sub-agent distillation for the first slice.** Reusing the KB sub-agent
  loop (`ingest_conversation`) is powerful but slow/costly per session. A single
  `complete_fast` call is the mergeable first cut; the sub-agent path is a later
  quality upgrade.
- **Silent auto-injection with no review.** Risk called out in the proposal
  (stale/irrelevant facts). Provenance + a review surface (Gemini inbox) is
  required before facts are trusted; see phasing.

---

## Migration & compatibility

- **Schema.** One additive migration (11): `CREATE VIRTUAL TABLE messages_fts` +
  one-time backfill. Additive and idempotent (`CREATE ... IF NOT EXISTS`
  pattern, `apply_migration` style at `:1325`). Downgrade tolerance: an older
  binary opening a v11 DB simply ignores `messages_fts` (it still `LIKE`-queries
  `messages`); the guard is that `chat_history_search` only issues `MATCH` when it
  detects the table, else it falls back to the current `LIKE` SQL — so a partially
  migrated or trigger-failed DB never errors, it degrades.
- **Persisted state.** Distillation bookkeeping lives in
  `sessions.extension_data[memory.v0]` (forward-compatible versioned key,
  `extension_data.rs`), not a new column. Memory pages are ordinary Soul KB
  markdown, readable by every existing KB tool/UI.
- **Config / rollout (all off unless enabled):**
  - `BIOROUTER_MEMORY_AUTO_PROMOTE` (bool, default **false** for first release) —
    master switch for Pieces 2+3.
  - `BIOROUTER_MEMORY_IDLE_SECS` (default 10800 = 3 h, Gemini's value).
  - `BIOROUTER_MEMORY_MIN_USER_MSGS` (default 10, Gemini's value).
  - `BIOROUTER_MEMORY_DIGEST_TOP_N` (default 8), `BIOROUTER_MEMORY_DIGEST_MAX_CHARS` (default 2000).
  - `BIOROUTER_MEMORY_TARGET_KB` (default `soul`).
  Piece 1 (FTS5 recall) ships **on** — it is a pure quality improvement with the
  `LIKE` fallback as a safety net. Pieces 2+3 ship behind the flag (opt-in), then
  flip to default-on once the review surface lands.

---

## Test plan

**Unit (Rust, no provider):**
- `chat_fts::sanitize_fts_query` — tokens, quotes, FTS operator escaping
  (`AND`/`OR`/`*`/`"`/`NEAR`), empty query → empty results (mirrors
  `execute`'s empty-keyword short-circuit, `chat_history_search.rs:78-83`).
- `chat_fts::extract_searchable_text` parity with today's
  `extract_text_content` (`chat_history_search.rs:207-220`).
- FTS recall ranking: seed a temp DB (reuse the existing test harness at
  `session_manager.rs:2313`+/`:2642`), assert an exact-phrase and a paraphrase
  both rank the relevant session above a merely-recent one — the case the current
  recency `LIKE` fails.
- Migration 11: build a v10 DB with messages, run migrations, assert
  `messages_fts` populated and counts match `user_visible` messages; assert
  `add_message` and `replace_conversation_inner` keep the index in sync (insert,
  then compact, then search).
- Fallback: with `messages_fts` absent, `ChatHistorySearch` still returns via
  `LIKE` (no panic, no error).
- `memory::promote`: idle/eligibility predicate (idle_secs, min_user_msgs,
  already-distilled skip); `NO_DURABLE_FACTS` → no page written; dedup → no
  duplicate page on re-distill; `extension_data[memory.v0]` stamped.
- `memory::digest`: cap enforcement (top_n, max_chars), stable output for stable
  input (cache-safety), untrusted-data framing present.
- `prompt_manager`: `with_memory_digest(Some(...))` renders under
  "# Additional Instructions"; `None` renders identically to today (snapshot
  tests at `prompt_manager.rs:316-360` must be unchanged when the flag is off).

**Integration:**
- `chatrecall` extension end-to-end against an FTS-indexed DB (extend existing
  chatrecall tests) — same `ChatRecallResults` shape, better ranking.
- Distillation with a stubbed `Completer` (like `knowledge` sub-agent tests):
  transcript in → facts written to a temp Soul KB with provenance metadata.
- Live smoke (ignored, real fast model): one real session → distill → assert a
  plausible non-empty memory page.

**No-regression proof:**
- All `prompt_manager` snapshots and `moim` tests pass unchanged with the flag
  off (default), proving the injection is inert until enabled.
- `cargo test -p biorouter session::` and the existing chatrecall tests are green;
  MOIM insertion invariants (`moim.rs:60-198`) untouched.
- `cargo test -p biorouter-mcp --lib knowledge::` green (Soul writes go through
  the existing service).

---

## Effort & phasing

Proposal effort **L**; slice it:

- **Phase 1 (first mergeable slice, ~M) — FTS5 recall only.** Migration 11 +
  backfill, `chat_fts.rs`, rewrite `build_sql` to `MATCH`/`bm25()` with `LIKE`
  fallback, keep public API, flip `chatrecall` to default-enabled. Pure quality
  win, no auto-injection, no LLM cost, low risk. Ships on.
- **Phase 2 — promotion pipeline (behind `BIOROUTER_MEMORY_AUTO_PROMOTE`).**
  `memory::promote` + `distill_memory.md` + `complete_fast`, write to Soul with
  provenance, `extension_data[memory.v0]` bookkeeping, session-start detached scan.
  No injection yet — validate distillation quality against real sessions first.
- **Phase 3 — bounded digest injection.** `with_memory_digest`, memoized
  `memory::digest`, framing + caps. Turn on for opt-in users.
- **Phase 4 — review surface + usage feedback.** A Gemini-style review inbox in
  the Knowledge UI (accept/prune auto-promoted pages), precise `usage_count`
  accounting on `kb_search` hits, then flip the flag default-on.

---

## Open questions for the human (only genuine product decisions)

1. **Soul vs. a dedicated `auto-memory` KB.** Write auto-promoted facts straight
   into Soul (unified, but mixes user-curated and machine-generated facts), or a
   separate hidden `auto-memory` KB that Soul/`kb_search` can still read? Cleaner
   provenance vs. more silos.
2. **Default posture.** Should auto-promotion + injection ever be default-on, or
   forever opt-in (privacy: the agent silently persists inferences about the
   user)? Recommendation: opt-in until the review inbox exists, then default-on.
3. **Review before load, or load-then-prune?** Gemini parks candidates in an
   inbox for approval *before* they load; Claude/Codex load immediately. Which fits
   a UCSF/clinical trust posture?
4. **PHI/PII in distilled memory.** Sessions may contain clinical text. Do we gate
   distillation behind the (proposed) BR-22 tool-output PII stage, or exclude
   sessions whose working dir is under a clinical/OMOP path, or never
   auto-distill and require explicit `ingest_conversation`?
5. **Scope of a memory: global vs. per-project.** Claude auto-memory is per-repo;
   Codex is global. Should the digest be filtered by the current `working_dir`
   (project-scoped) or always global (user-scoped, like Soul today)?
