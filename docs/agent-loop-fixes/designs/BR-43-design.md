# BR-43 — Shadow-git checkpoints + `/rewind` (three-axis restore: files / conversation / both)

**Lens:** R + U (robustness P-12, ux P-1). "The single starkest deficit vs every
current-gen agent" (`docs/agent-loop-review/PROPOSALS.md:504-512`).
**Inspired by (primary mechanism):** OpenCode's *private git-object DB* snapshot model —
capture the worktree before/after each model step into a separate Git object database in
the app's data dir, **no commits, no branch moves, no touching the user's Git index**
(`docs/agent-loop-review/external/opencode.md:64`, "Ideas worth stealing" #2 at line 74).
Cline supplies the three restore *modes* (files / conversation / both); Claude Code /
Gemini CLI the `/rewind` affordance (`external/claude-code.md:226-229, 288-291`).

---

## Problem (grounded in code, with file:line)

BioRouter has **no checkpointing of the agent's own edits and no session-level undo**.
The internal review states this plainly (`internal/state-awareness.md:133-170`, gap #2 at
`:313-318`): *"No git checkpointing, no shadow git, no session-level undo of edits. This
is a clear absence."*

1. **The only rollback is `text_editor`'s in-memory, per-file, per-process LIFO.**
   `file_history: Arc<Mutex<HashMap<PathBuf, Vec<String>>>>` is created fresh in
   `DeveloperServer::new()` (`crates/biorouter-mcp/src/developer/rmcp_developer.rs:698`).
   `save_file_history` pushes the whole file body onto a per-path stack
   (`crates/biorouter-mcp/src/developer/text_editor.rs:1088-1109`) and `text_editor_undo`
   pops one and writes it back (`text_editor.rs:1052-1085`). Its limits, per the review:
   it dies with the process, is never persisted, covers **only** files touched via
   `text_editor` (shell redirects, `write_file`, and other extensions are invisible), and
   offers no cross-file atomic "revert the whole task" (`state-awareness.md:152-164`).

2. **`git2` is already in-tree but never applied to the workspace.** It is a dependency of
   exactly one crate for exactly one purpose — the Knowledge-base wiki
   (`crates/biorouter-mcp/Cargo.toml:77`, `vendored-libgit2`), used only in
   `crates/biorouter-mcp/src/knowledge/git.rs`. That `GitRepo` already demonstrates the
   exact primitives we need: `init` with a custom initial head and identity
   (`git.rs:11-22`), `commit_all` building an index → tree → commit (`git.rs:29-47`),
   `read_file_at(sha, path)` reading a blob from a commit tree (`git.rs:235-246`), and
   `restore_to(sha)` which commits an old tree and force-checks-out the worktree
   (`git.rs:248-266`). None of it touches the user's project — it versions
   `~/.config/biorouter/knowledge/<kb>/.git`.

3. **The reply loop already has a clean per-turn boundary, with no snapshot hook.**
   `Agent::reply_internal` (`crates/biorouter/src/agents/agent.rs:1520`) runs the turn loop
   at `agent.rs:1556`; each iteration drains soft-interrupts (`:1589-1594`), injects MOIM
   (`:1596-1601`), calls the provider (`:1603-1609`), then dispatches tools. `no_tools_called`
   (`:1611`) already tells us whether a step mutated anything. There is nowhere in this loop
   that snapshots the filesystem.

4. **The conversation-axis machinery already exists and is reusable.** Sessions persist in
   one SQLite DB; messages are append-only, anchored by `created_timestamp`
   (`session_manager.rs:1843-1870`). `truncate_conversation(session_id, timestamp)` is a
   `DELETE FROM messages WHERE created_timestamp >= ?` (`session_manager.rs:2302-2311`), and
   the `edit_message` (`crates/biorouter-server/src/routes/session.rs:490-535`) and
   `diverge_session` (`session.rs:561`) routes already fork/truncate history by timestamp.
   The **conversation axis of `/rewind` is largely this code**; BR-43 adds the *files* axis
   and ties both to a checkpoint id.

5. **Message ids are positional and unstable.** `get_conversation` re-derives synthetic ids
   `msg_<session>_<idx>` on every load (`session_manager.rs:1810-1841`), so any history
   rewrite renumbers them (gap #10, `state-awareness.md:360-362`). A checkpoint must **not**
   anchor on these; it anchors on the message `created_timestamp` (`message.rs:588`, the same
   key `truncate_conversation` uses), which is stable within a session and survives the
   eventual BR-45 UUID migration.

Net: aggressive autonomy on a scientist's working tree is intolerable with only a
per-process, per-file, `text_editor`-only undo. BR-43 supplies the missing safety net.

---

## Design

### Overview

A **shadow git repository per session**, whose Git object DB lives in the app data dir and
whose *work-tree* is the session's `working_dir`. Before/after each mutating model step the
agent commits the worktree into this shadow repo on a private ref — never HEAD of the user's
repo, never the user's `.git`. Each checkpoint is recorded in a new SQLite `checkpoints`
table keyed to the turn's anchor `created_timestamp`. `/rewind` restores along one of three
axes:

- **Files** — check out the checkpoint's tree into the worktree (force; delete files created
  since).
- **Conversation** — `truncate_conversation` at the anchor timestamp (reuses existing code).
- **Both** — do both, atomically.

### Data model

**Shadow repo location** (mirrors the KB layout, respects the `BIOROUTER_PATH_ROOT` test
override in `config/paths.rs:8-13`):

```
<Paths::data_dir()>/checkpoints/<session_id>/
    git/            # GIT_DIR — objects, refs/biorouter/checkpoints, our own index file
    (work-tree = the session's working_dir, set via core.worktree / workdir_path)
```

The user's project directory is **never** written to except during a Files restore
(checkout). We never create a `.git` in the working dir and never read/write the user's
`.git/index` or refs — GIT_DIR points into the data dir, so a user repo in the same worktree
is untouched.

**SQLite `checkpoints` table** (migration 11; see below):

| column            | type     | note                                                        |
|-------------------|----------|-------------------------------------------------------------|
| `id`              | TEXT PK  | uuid                                                         |
| `session_id`      | TEXT FK  | → `sessions.id`                                             |
| `turn_index`      | INTEGER  | monotonically increasing per session (display/order)        |
| `anchor_ts`       | INTEGER  | `created_timestamp` of the user msg that opened the turn     |
| `kind`            | TEXT     | `pre_step` \| `post_step` \| `manual` \| `pre_restore`      |
| `commit_sha`      | TEXT     | commit in the shadow repo                                    |
| `tree_sha`        | TEXT     | for O(1) no-op dedup between consecutive snapshots           |
| `changed_paths_json` | TEXT  | paths whose blobs differ from the previous checkpoint        |
| `created_at`      | TEXT     | `datetime('now')`                                           |

We deliberately **do not** widen `MessageMetadata` (`message.rs:509-514`, a fixed
`{user_visible, agent_visible}` struct) — extending it ripples through serialization and the
generated TS client. The checkpoint↔message link lives entirely in this side table, keyed by
`anchor_ts`.

### Module layout (files to create)

`crates/biorouter/src/checkpoint/` (new; `pub mod checkpoint;` added to
`crates/biorouter/src/lib.rs` alongside the existing modules at `lib.rs:1-34`):

- **`mod.rs`** — public types: `CheckpointId`, `CheckpointRecord`, `RestoreAxis {Files,
  Conversation, Both}`, `RestoreOutcome`, `CheckpointConfig` (caps + on/off), and the
  `CheckpointManager` facade.
- **`store.rs`** — `ShadowRepo`: thin git2 wrapper analogous to
  `knowledge/git.rs`'s `GitRepo`, but with a **detached GIT_DIR + external work-tree**. Key
  methods:
  - `ShadowRepo::open_or_init(git_dir: &Path, worktree: &Path) -> Result<Self>` —
    `git2::Repository::init_opts` with `opts.bare(false).workdir_path(worktree)
    .no_dotgit_dir(true)` (or `Repository::open` when it exists), set
    `user.name/email`, `commit.gpgsign=false` (as in `git.rs:15-19`).
  - `snapshot(&self, ignore: &Gitignore, caps: &Caps) -> Result<(commit_sha, tree_sha)>` —
    build an **in-repo index** (`self.inner.index()`, pointed at our own index file, NOT the
    user's) by walking the worktree with the `ignore` crate's `WalkBuilder` (already used in
    `rmcp_developer.rs:1670-1676` to honor `.biorouterignore` + `.gitignore`), skipping files
    over `caps.max_file_bytes`; `write_tree` → `commit` onto `refs/biorouter/checkpoints`.
  - `restore_files(&self, commit_sha: &str) -> Result<Vec<PathBuf>>` — check out that
    commit's tree into the worktree with a forced `CheckoutBuilder` and
    `remove_untracked(true)` **scoped to tracked paths only**, so files that existed at the
    checkpoint are restored and files created since are removed — mirroring
    `git.rs:248-266`'s `restore_to`, but into the external worktree.
  - `read_file_at(sha, path)` / `diff_paths(a_tree, b_tree)` for `changed_paths_json` and
    previews.
- **`manager.rs`** — `CheckpointManager { data_root: PathBuf, session_manager:
  Arc<SessionManager>, cfg: CheckpointConfig }`:
  - `async fn snapshot(&self, session_id, working_dir, anchor_ts, kind) -> Result<Option<CheckpointRecord>>`
    — no-op returning `None` when checkpoints are disabled, the worktree exceeds
    `cfg.max_tree_bytes`, or `tree_sha` equals the last checkpoint's (dedup). Otherwise
    commits and inserts a row.
  - `async fn list(&self, session_id) -> Result<Vec<CheckpointRecord>>`
  - `async fn restore(&self, session_id, checkpoint_id, axis) -> Result<RestoreOutcome>` —
    first takes a `pre_restore` snapshot (so restore is itself reversible — OpenCode's redo
    baseline, `opencode.md:64`), then:
      - `Files`/`Both`: `store.restore_files(commit_sha)`.
      - `Conversation`/`Both`: `self.session_manager.truncate_conversation(session_id,
        anchor_ts)` (existing, `session_manager.rs:2302`) and signal the caller to emit
        `AgentEvent::HistoryReplaced` (already a variant, `agent.rs:180`) / trigger a session
        reload, the same refresh path `edit_message` uses.
  - `async fn gc(&self, session_id)` — remove the shadow repo dir; called from
    `delete_session`.

`store.rs` reuses `git2` and the `ignore` crate; both are added to
`crates/biorouter/Cargo.toml` (git2 with the same `vendored-libgit2` feature the workspace
already builds for `biorouter-mcp/Cargo.toml:77`; `ignore` is already a transitive dep).

### Wiring into the agent turn boundary (files to change)

`crates/biorouter/src/agents/agent.rs`:

- Add `pub(super) checkpoints: Option<Arc<CheckpointManager>>` to `struct Agent`
  (`agent.rs:142-173`), constructed in `Agent::new` from `Paths::data_dir()` +
  `config.session_manager` (`agent.rs:118-141` for `AgentConfig`). `Option` so the subagent /
  test paths can leave it unset.
- In `reply_internal` (`agent.rs:1520`), capture `anchor_ts` = the `created` of the last user
  message once, before the loop. Inside the loop (`agent.rs:1556`):
  - **pre-step:** after the soft-interrupt drain (`:1594`), if a previous iteration mutated
    the tree (`dirty` flag, see below) but we have not yet snapshotted this boundary, call
    `checkpoints.snapshot(..., kind = pre_step)`. In practice a single pre-turn snapshot per
    user message plus post-step snapshots is enough; the tree-sha dedup makes an extra call
    free.
  - **post-step:** at the end of each iteration, once the assistant message + tool responses
    are persisted, if `!no_tools_called` (`:1611`) and any tool was a *mutating* one, set
    `dirty` and call `checkpoints.snapshot(..., kind = post_step)`. Dedup by `tree_sha` drops
    steps that changed nothing on disk (read-only tool turns cost one cheap tree hash, no row).

"Mutating tool" is a coarse allowlist (any `developer`/shell/`write_file`/`text_editor`
write, plus any extension flagged as filesystem-writing); when in doubt we snapshot — the
dedup absorbs false positives.

### Server routes (files to change)

`crates/biorouter-server/src/routes/session.rs` — add siblings to `diverge`/`edit_message`
(router wiring at `session.rs:650-666`), all utoipa-annotated so `just generate-openapi`
regenerates the TS client:

- `GET  /sessions/{id}/checkpoints` → `Vec<CheckpointRecord>` (list, newest-first).
- `POST /sessions/{id}/checkpoints/{cid}/restore`, body `{ "axis": "files"|"conversation"|"both" }`
  → `RestoreOutcome`.
- `POST /sessions/{id}/checkpoints` (optional) → manual "mark a checkpoint here".

The `CheckpointManager` is reachable from `AppState` (the same handle that owns
`session_manager()`, used throughout `session.rs`).

### `/rewind` slash command + GUI

- **Slash command:** `/rewind` in `crates/biorouter/src/slash_commands/` — lists recent
  turns (from `list`), lets the user pick a checkpoint and an axis. CLI renders a picker;
  bare `/rewind` defaults to the previous turn, `both` axis.
- **GUI:** a per-turn rewind affordance in `ui/desktop/src/components/BaseChat.tsx` (the file
  BR-43 names in PROPOSALS) that calls the restore route and, for conversation/both, reloads
  the transcript via the existing history-refresh path.

### Control flow (one restore)

```
user clicks "Rewind to turn N (files+conversation)"
  → POST /sessions/{id}/checkpoints/{cid}/restore {axis:"both"}
      → CheckpointManager::restore
          1. snapshot(kind=pre_restore)                 # redo baseline
          2. ShadowRepo::restore_files(commit_sha)      # worktree ← tree, delete newer files
          3. session_manager.truncate_conversation(id, anchor_ts)  # existing DELETE-by-ts
      → RestoreOutcome { restored_paths, truncated_from_ts }
  → GUI reloads session (HistoryReplaced path) → transcript + files now at turn N
```

---

## Alternatives considered (and why rejected)

1. **Commit into the user's real repo (Aider's commit-per-edit).** Rejected: pollutes the
   user's history, competes with their own `git` operations, and fails outright when the
   working dir is not a git repo — common for a scientist's data folder. OpenCode explicitly
   avoids this (`opencode.md:74`).
2. **Persist whole-file snapshots in SQLite (extend `text_editor` undo — that is BR-44).**
   Rejected as the *primary* mechanism: no content-addressed dedup, no cross-file atomicity,
   and it bloats the session DB (gap #9, `state-awareness.md:354-358`). BR-44 is the
   incremental cousin (persist per-path undo, cover shell/`write_file`) explicitly framed as
   "a step toward BR-43" (`PROPOSALS.md:517`); shadow-git is the durable, deduped answer.
3. **Tar/copy the worktree per step.** Rejected: no dedup, slow, space-heavy — exactly the
   risk flagged in the proposal (`PROPOSALS.md:512`). Git's content-addressed store gives free
   dedup across steps.
4. **`git stash` on the user's repo.** Rejected: mutates the user's index/refs and only works
   inside a git repo.
5. **Stamp the checkpoint sha into `MessageMetadata`.** Rejected for the first slice: widening
   `MessageMetadata` (`message.rs:509`) ripples through serialization, the schema, and the
   generated TS client. A side table keyed by `anchor_ts` is decoupled and migration-safe.
6. **Snapshot on every provider call.** Rejected: bounded instead by *dirty-gating* (only
   after a mutating tool) plus `tree_sha` dedup, so read-only turns cost one tree hash and no
   commit/row.

---

## Migration & compatibility

- **SQLite:** bump `CURRENT_SCHEMA_VERSION` 10 → 11 (`session_manager.rs:21`) and add a
  migration-11 arm to `apply_migration` (`session_manager.rs:1325`; the existing arms, e.g.
  the metadata-column add at `:1351`, are the template) that `CREATE TABLE IF NOT EXISTS
  checkpoints (…)`. Purely additive — old sessions load unchanged and simply have zero
  checkpoints. Guarded by the existing versioned-migration loop (`session_manager.rs:1272-1288`).
- **On-disk:** shadow repos under `<data_dir>/checkpoints/<session_id>/`, honoring
  `BIOROUTER_PATH_ROOT` (`config/paths.rs:8-13`) so tests are hermetic. Nothing is written
  under the user's project. `delete_session` calls `CheckpointManager::gc` to remove the dir.
- **Config flags:** `BIOROUTER_CHECKPOINTS=on|off` (auto-skip when the worktree exceeds
  `BIOROUTER_CHECKPOINT_MAX_TREE_MB`), `BIOROUTER_CHECKPOINT_MAX_FILE_MB` (default 2, matching
  OpenCode's >2 MiB exclusion, `opencode.md:64`), `BIOROUTER_CHECKPOINT_IGNORE` (extra globs).
  First ship gated behind `ALPHA=true` so the perf/space profile is validated before default-on.
- **Dependencies:** add `git2 = { …, features = ["vendored-libgit2"] }` and `ignore` to
  `crates/biorouter/Cargo.toml` (the workspace already vendors libgit2 for `biorouter-mcp`).
- **Rollout order:** (1) snapshot silently, no restore UI — measure cost; (2) enable
  restore + routes + `/rewind`; (3) GUI affordance + redo. Reversible at each step by the
  config flag.
- **BR-45 interaction:** anchoring on `created_timestamp` (not the positional
  `msg_<session>_<idx>` id) means checkpoints survive the future stable-UUID migration.

## Test plan

- **`checkpoint::store` unit tests** (pattern from `knowledge/git.rs` tests, `git.rs:290-405`):
  - snapshot→`restore_files` roundtrip restores a modified file, re-creates a deleted file,
    and removes a file created after the checkpoint.
  - `.biorouterignore`/`.gitignore` + `max_file_bytes` caps are honored (no huge/ignored blobs
    committed).
  - snapshotting a **non-git** working dir succeeds.
  - **Isolation invariant:** snapshotting a working dir that *is itself* a user git repo leaves
    the user's `HEAD`, index, and refs byte-for-byte unchanged (assert `git status` /
    `rev-parse HEAD` identical before/after) — the load-bearing safety property.
  - identical consecutive worktrees produce the same `tree_sha` (dedup).
- **`checkpoint::manager` unit tests:** Files-only restore leaves the conversation intact;
  Conversation-only leaves files intact (delegates to `truncate_conversation`); Both;
  `pre_restore` baseline makes a restore reversible.
- **Migration test:** fresh DB at v11 has `checkpoints`; a v10→v11 upgrade is additive and
  preserves existing rows (extend the schema tests near `session_manager.rs:2642`).
- **Agent integration** (`crates/biorouter` test): drive `reply_internal` with a stub provider
  that issues one mutating tool call; assert a `post_step` checkpoint row + shadow commit
  appear and that `restore(Both)` returns the worktree and transcript to the prior turn.
- **Server route tests** (`biorouter-server`): mirror the `diverge_tests` block
  (`session.rs:673-880`) — list returns rows; restore with each axis; bad ids → 400/404.
- **Regression:** `cargo test -p biorouter`, `-p biorouter-server`, and
  `cargo test -p biorouter-mcp --lib knowledge::git` (prove the KB git path is untouched);
  the existing `truncate_conversation` and `text_editor` undo tests must stay green.
- **Perf/space smoke:** snapshotting a large tree stays within a time budget and the caps
  trigger a skip (returns `None`, no row).

## Effort & phasing

Overall **L** (per `PROPOSALS.md:511`).

- **Slice 1 (first mergeable):** `checkpoint/` module (`store.rs` + `manager.rs`), git2/ignore
  deps, migration 11, turn-boundary snapshot (dirty-gated + `tree_sha` dedup + caps), and the
  `restore(Files|Conversation|Both)` API with full unit + one agent-integration test. No UI,
  gated behind `ALPHA`/config. This alone delivers a durable, all-writers safety net and
  programmatic three-axis restore — the core of BR-43.
- **Slice 2:** server routes (list/restore/manual) + `just generate-openapi` + TS client, and
  the `/rewind` slash command (CLI).
- **Slice 3:** GUI per-turn rewind affordance in `BaseChat.tsx`, redo, `gc` on session delete,
  docs, and default-on after perf validation.

## Open questions for the human (only genuine product decisions)

1. **Default posture:** ship default-on, or `ALPHA`-only until the space/time cost on a
   scientist's large data dir is measured? (A worktree of BAM/FASTQ files could be huge even
   with the per-file cap.)
2. **Snapshot granularity:** every model step, only turns that ran a mutating tool (proposed),
   or only user-visible turn boundaries? This trades storage/CPU against how finely a user can
   rewind.
3. **Retention/GC:** keep the last N checkpoints or M days per session, or keep all and only
   GC on session delete?
4. **Conversation-axis semantics:** does restore **truncate** (destructive, like
   `truncate_conversation`) or **hide** messages with a redo (OpenCode-style, recoverable)?
5. **Fork vs in-place:** does `/rewind` mutate the current session, or branch a new one (like
   `diverge`)? Cline offers both; picking one keeps the UX simple.
6. **External side effects:** DB writes, network calls, and files written *outside* the
   `working_dir` cannot be reverted. Do we surface a clear "files + conversation only" caveat
   on the rewind control (as OpenCode does)?
