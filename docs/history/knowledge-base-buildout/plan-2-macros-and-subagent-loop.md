# Plan 2 — knowledge macros and the sub-agent loop

> **What this is.** Plan 2 of the six-plan Knowledge buildout: the `kb_ingest_source` / `kb_query` / `kb_lint` macros running over a bounded sub-agent loop, plus the primitives Plan 1 deferred (`kb_search`, `kb_set_active` / `kb_get_active`, `kb_append_log`, the MCP-exposed transaction tools) and the real agentic credibility fallback.
> **Status:** Historical record — executed and shipped. The sub-agent loop lives at `crates/biorouter-mcp/src/knowledge/subagent/loop_.rs`; `CLAUDE.md` documents the three macros, the per-KB concurrency mutex, and BM25 search as shipped behaviour. The unticked `- [ ]` checkboxes below are the plan as written, not outstanding work.
> **Audience:** developers working on the Knowledge subsystem, and agents tracing how the macro engine came to be shaped this way.
>
> **Plan numbering.** "Plan *N* of 6" refers to the six sibling documents in this
> folder, `plan-1-…` through `plan-6-…`, executed in order against the design in
> [`founding-design.md`](founding-design.md).

Plan 1 built the storage, git, conversion, credibility and graph layers behind a shared `KnowledgeService`. This plan puts an agent on top of them: a bounded LLM loop whose only tools are the KB primitives, wrapped by three macros that each commit as a single logical change.

> **Note — this plan was written against unverified APIs.** Its "Open risks"
> section, at the end, records that the `SubAgent` loop was written against
> assumed shapes for `Message::user()`, `Message::tool_result()` and
> `Message.tool_calls()`, and that the BM25 snippet in Task 1 targets
> `bm25 = "2.3"` while [Plan 1](plan-1-storage-git-and-graph.md) pins `bm25 = "2.2"`.
> Treat every code snippet here as the intent, not as verified-compiling source,
> and read those risks before following a task literally.

> **Note — worktree paths and expected test counts are point-in-time.** Commands
> below `cd` into `/Users/wgu/Desktop/biorouter-knowledge`, the isolated git
> worktree the Knowledge branch was developed in; read it as your own checkout
> root. The baseline gates ("expect: 81 passed (Plan 1's baseline)", "≈ 94+")
> record the suite as it stood when this plan was written. The Knowledge library
> suite is roughly 122 tests today, so a higher number is expected, not a failure.
> Source line anchors such as `factory.rs#L125`, `base.rs#L360` and
> `session_manager.rs#L102` have long since moved — use the symbol names, not the
> line numbers.

## Scope and approach

**Goal:** Layer high-level macro tools (`kb_ingest_source`, `kb_query`, `kb_lint`) and an in-process sub-agent loop on top of Plan 1's primitives, plus fill in the missing primitive surface (`kb_search`, `kb_set_active` / `kb_get_active`, `kb_append_log`, MCP-exposed transaction tools), and replace the Plan-1 agentic-credibility stub with the real bounded-agent implementation.

**Explicitly out of scope:** streaming progress. MCP `CallToolResult` is request/response only in rmcp 0.14, so macros return a single final result. SSE-streamed progress lands in [Plan 3](plan-3-http-routes-and-export.md) when the HTTP routes wrap these macros. Everything else deferred is listed under "What this plan does NOT cover" near the end.

**Architecture:**
- A `SubAgent` runs an LLM in a bounded loop (max steps, max wall time, max tokens) with the KB primitives as its tool surface and the per-KB `schema.md` + a macro-specific operating procedure as its system prompt. It uses `biorouter::providers::factory::create()` to instantiate the user-chosen provider.
- Macros (`ingest`, `query`, `lint`) wrap the sub-agent: they open a git transaction, run the sub-agent against a tailored procedure, then commit the txn on success or abort on failure.
- Session-scoped "active KB" lives in `Session::extension_data["knowledge"]["v0"]` (see `crates/biorouter/src/session/session_manager.rs`) and is read/written by `kb_set_active` / `kb_get_active`. When a tool omits `kb_id`, the active KB is the default.
- Per-KB concurrency is guarded by a `DashMap<String, Arc<tokio::Mutex<()>>>` on `KnowledgeService` so two concurrent macros against the same KB serialize cleanly.

**Tech stack:** Rust 1.92, tokio, dashmap, `biorouter::providers::Provider`, `bm25` crate (already added in Plan 1 deps), rmcp 0.14, wiremock + recorded LLM cassettes for tests.

**Source spec:** [`founding-design.md`](founding-design.md).

**Series position:** Plan 2 of 6. Plan 1 (backend foundation) is complete. Plan 3 adds HTTP routes + SSE streaming. Plans 4-6 add the frontend (sidebar route, KB selector, graph view, change log drawer, chat-side KB chip).

**TDD note:** Same convention as Plan 1 — most tasks combine "write tests" + "write impl" into single steps for brevity. Read the test code first, mentally verify it would fail against an empty impl, then proceed. Verification steps gate each task.

**Execution convention:** the plan was written for an agentic worker driving it task-by-task with the `superpowers:subagent-driven-development` or `superpowers:executing-plans` skill. Steps use checkbox (`- [ ]`) syntax for tracking.

---

## Before starting

- [ ] **Pre-step A: confirm branch.** Execution should continue on `feature/knowledge` (the same branch as Plan 1).

```bash
cd /Users/wgu/Desktop/biorouter-knowledge
source bin/activate-hermit
git rev-parse --abbrev-ref HEAD   # expect: feature/knowledge
cargo test -p biorouter-mcp --lib knowledge 2>&1 | tail -3
# expect: 81 passed (Plan 1's baseline)
```

- [ ] **Pre-step B: skim the integration points** so the file paths below make sense:
  - LLM provider factory `biorouter::providers::factory::create(name, model)` in `crates/biorouter/src/providers/factory.rs`, returning `Arc<dyn Provider>`.
  - Provider trait `biorouter::providers::base::Provider` in `crates/biorouter/src/providers/base.rs`, with `complete()` + `stream()`.
  - Session state API `SessionManager::update().extension_data(...).apply()` in `crates/biorouter/src/session/session_manager.rs`.
  - Knowledge module at `crates/biorouter-mcp/src/knowledge/` (Plan 1 lives here; everything new in Plan 2 lands here too).

---

## File structure (decomposition map)

```text
crates/biorouter-mcp/src/knowledge/
├── service.rs              — extended: per-KB Mutex, set_active/get_active hooks
├── store.rs                — extended: kb_search (BM25 over knowledge/ + raw/source.md)
├── log.rs                  — NEW: kb_append_log helper (writes to log.md + commits)
├── credibility/
│   └── agentic.rs          — REWRITE: real sub-agent fallback (replaces Plan 1 stub)
├── subagent/
│   ├── mod.rs              — NEW: re-exports
│   ├── loop_.rs            — NEW: SubAgent struct, bounded execution, tool dispatch
│   ├── procedures.rs       — NEW: INGEST / QUERY / LINT operating-procedure templates
│   └── events.rs           — NEW: SubAgentEvent enum (used now for tests; Plan 3 streams these)
├── macros/
│   ├── mod.rs              — NEW: re-exports
│   ├── ingest.rs           — NEW: kb_ingest_source macro
│   ├── query.rs            — NEW: kb_query macro
│   └── lint.rs             — NEW: kb_lint macro
└── server.rs               — extended: new MCP tools (search/append_log/active/txn/ingest/query/lint)

crates/biorouter/src/session/  — touch one file for the active-KB session helper:
└── session_manager.rs     — extended: small helper for reading "knowledge" extension state (or use existing API directly from server.rs)

crates/biorouter/tests/
└── knowledge_macros.rs     — NEW: integration tests for macros (with mocked Provider)
```

Test code lives next to each module via `#[cfg(test)] mod tests { … }` plus the new integration file.

---

## Task 1: `kb_search` primitive — BM25 over knowledge + raw

**Files:**
- Modify: `crates/biorouter-mcp/src/knowledge/store.rs`

- [ ] **Step 1: Add the test + impl**

Append to `store.rs`:

```rust
use bm25::{Embedder, EmbedderBuilder, Language, ScoredDocument, Scorer, SearchEngineBuilder};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    pub path: String,
    pub score: f32,
    pub snippet: String,
}

pub fn search(kb_root: &Path, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
    let mut docs: Vec<(String, String)> = Vec::new(); // (logical_path, body)
    let knowledge_dir = kb_root.join("knowledge");
    if knowledge_dir.exists() {
        collect_docs_under(&knowledge_dir, &knowledge_dir, "knowledge", &mut docs)?;
    }
    let raw_dir = kb_root.join("raw");
    if raw_dir.exists() {
        for entry in std::fs::read_dir(&raw_dir)? {
            let entry = entry?;
            if !entry.path().is_dir() { continue; }
            let id = entry.file_name().to_string_lossy().to_string();
            let source_md = entry.path().join("source.md");
            if source_md.exists() {
                let body = std::fs::read_to_string(&source_md)?;
                docs.push((format!("raw/{id}/source.md"), body));
            }
        }
    }
    if docs.is_empty() { return Ok(Vec::new()); }

    let corpus: Vec<&str> = docs.iter().map(|(_, b)| b.as_str()).collect();
    let engine = SearchEngineBuilder::<usize>::with_documents(Language::English, corpus.iter().copied().collect::<Vec<_>>())
        .build();
    let results = engine.search(query, limit);
    let hits: Vec<SearchHit> = results
        .into_iter()
        .map(|sd: ScoredDocument<usize>| {
            let (path, body) = &docs[sd.id];
            SearchHit { path: path.clone(), score: sd.score, snippet: snippet_of(body, query, 200) }
        })
        .collect();
    Ok(hits)
}

fn collect_docs_under(base: &Path, dir: &Path, prefix: &str, out: &mut Vec<(String, String)>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir() {
            collect_docs_under(base, &p, prefix, out)?;
        } else if p.extension().and_then(|e| e.to_str()) == Some("md") {
            let rel = p.strip_prefix(base).unwrap().to_string_lossy().to_string();
            let logical = format!("{prefix}/{rel}");
            let body = std::fs::read_to_string(&p)?;
            out.push((logical, body));
        }
    }
    Ok(())
}

fn snippet_of(body: &str, query: &str, max_len: usize) -> String {
    let needle = query.to_ascii_lowercase();
    let hay = body.to_ascii_lowercase();
    if let Some(pos) = hay.find(&needle) {
        let start = pos.saturating_sub(60);
        let end = (pos + needle.len() + 140).min(body.len());
        let mut snippet = body[start..end].replace('\n', " ");
        if snippet.len() > max_len { snippet.truncate(max_len); }
        snippet
    } else {
        body.chars().take(max_len).collect::<String>().replace('\n', " ")
    }
}
```

Append to the existing `tests` module:

```rust
    #[test]
    fn search_returns_relevant_hits() {
        let (_dir, kb) = fresh();
        write_page(&kb, "knowledge/entities/hrv.md",
            "---\ntitle: HRV\nkind: entity\n---\n\nHeart rate variability is a key marker.", "a", None).unwrap();
        write_page(&kb, "knowledge/concepts/sleep.md",
            "---\ntitle: Sleep\nkind: concept\n---\n\nSleep quality affects HRV directly.", "b", None).unwrap();
        let hits = search(&kb, "heart rate variability", 5).unwrap();
        assert!(!hits.is_empty());
        assert!(hits.iter().any(|h| h.path.ends_with("hrv.md")));
    }

    #[test]
    fn search_returns_empty_when_no_match() {
        let (_dir, kb) = fresh();
        write_page(&kb, "knowledge/entities/x.md", "---\ntitle: X\n---\nbody", "a", None).unwrap();
        let hits = search(&kb, "zzznonexistent", 5).unwrap();
        assert!(hits.is_empty());
    }
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p biorouter-mcp --lib knowledge::store::tests
# Expected: 4 prior + 2 new = 6 passed
```

If the `bm25` crate's API differs from the snippet above (the listing here was written against `bm25 = "2.3"` at time of plan authoring), adapt minimally — the contract is: given a corpus + query + limit, return scored hits. Document any deviation in your final report.

- [ ] **Step 3: Commit**

```bash
git add crates/biorouter-mcp/src/knowledge/store.rs
git commit -m "feat(knowledge): kb_search via BM25 over knowledge/ + raw/source.md"
```

---

## Task 2: `kb_append_log` primitive

**Files:**
- Create: `crates/biorouter-mcp/src/knowledge/log.rs`
- Modify: `crates/biorouter-mcp/src/knowledge/mod.rs` (add `pub mod log;`)

- [ ] **Step 1: Write impl + tests**

```rust
// log.rs
use crate::knowledge::{git::GitRepo, types::ChangeKind};
use anyhow::Result;
use chrono::Utc;
use std::path::Path;

pub fn append(
    kb_root: &Path,
    kind: ChangeKind,
    summary: &str,
    delta: Option<&str>,
    txn_branch: Option<&str>,
) -> Result<Option<String>> {
    let log_path = kb_root.join("log.md");
    let kind_str = match kind {
        ChangeKind::Ingest => "ingest",
        ChangeKind::Link => "link",
        ChangeKind::Flag => "flag",
        ChangeKind::Query => "query",
        ChangeKind::Lint => "lint",
        ChangeKind::Restore => "restore",
        ChangeKind::Manual => "manual",
    };
    let now = Utc::now().format("%Y-%m-%d");
    let line = match delta {
        Some(d) => format!("## [{now}] {kind_str} | {summary}\n\n{d}\n\n"),
        None => format!("## [{now}] {kind_str} | {summary}\n\n"),
    };
    let mut existing = if log_path.exists() {
        std::fs::read_to_string(&log_path)?
    } else {
        String::from("# Log\n\n")
    };
    existing.push_str(&line);
    let tmp = log_path.with_extension("md.tmp");
    std::fs::write(&tmp, existing)?;
    std::fs::rename(tmp, &log_path)?;

    let repo = GitRepo::open(kb_root)?;
    let sha = if let Some(_branch) = txn_branch {
        repo.commit_on_txn_in_progress(&format!("log: {kind_str} | {summary}"))?
    } else {
        repo.commit_all(kind, summary, delta)?
    };
    Ok(Some(sha))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::service::KnowledgeService;

    fn fresh() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("k", "K", None).unwrap();
        let kb = dir.path().join("k");
        (dir, kb)
    }

    #[test]
    fn append_writes_to_log_md() {
        let (_d, kb) = fresh();
        append(&kb, ChangeKind::Ingest, "first source", Some("+1 source"), None).unwrap();
        let body = std::fs::read_to_string(kb.join("log.md")).unwrap();
        assert!(body.contains("ingest | first source"));
        assert!(body.contains("+1 source"));
    }

    #[test]
    fn append_commits_to_git() {
        let (_d, kb) = fresh();
        let sha = append(&kb, ChangeKind::Manual, "test", None, None).unwrap().unwrap();
        let repo = crate::knowledge::git::GitRepo::open(&kb).unwrap();
        let log = repo.log(5).unwrap();
        assert_eq!(log[0].commit_sha, sha);
    }
}
```

- [ ] **Step 2: Register the module**

In `crates/biorouter-mcp/src/knowledge/mod.rs`, add `pub mod log;` alongside the existing `pub mod` declarations (alphabetical order: between `graph` and `manifest`).

- [ ] **Step 3: Run tests**

```bash
cargo test -p biorouter-mcp --lib knowledge::log::tests
# Expected: 2 passed
```

- [ ] **Step 4: Commit**

```bash
git add crates/biorouter-mcp/src/knowledge/log.rs crates/biorouter-mcp/src/knowledge/mod.rs
git commit -m "feat(knowledge): kb_append_log primitive"
```

---

## Task 3: Per-KB concurrency mutex

**Files:**
- Modify: `crates/biorouter-mcp/src/knowledge/service.rs`
- Modify: `crates/biorouter-mcp/Cargo.toml` (add `dashmap` if absent)

- [ ] **Step 1: Add `dashmap` dep**

Check `crates/biorouter-mcp/Cargo.toml`. If `dashmap` isn't there, add:

```toml
dashmap = "6"
```

- [ ] **Step 2: Implement per-KB mutex on the service**

Modify `KnowledgeService` to hold a `DashMap<String, Arc<tokio::Mutex<()>>>`. Add a method that returns a guard for a given `kb_id`.

```rust
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, OwnedMutexGuard};

#[derive(Clone)]
pub struct KnowledgeService {
    root: PathBuf,
    locks: Arc<DashMap<String, Arc<Mutex<()>>>>,
}

impl KnowledgeService {
    pub fn new(root: PathBuf) -> Self {
        Self { root, locks: Arc::new(DashMap::new()) }
    }

    /// Acquire an exclusive lock for `kb_id`. Held until the returned guard is dropped.
    /// Used by macros to serialize concurrent writers against the same KB.
    pub async fn lock_kb(&self, kb_id: &str) -> OwnedMutexGuard<()> {
        let m = self.locks
            .entry(kb_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        m.lock_owned().await
    }
}
```

- [ ] **Step 3: Write the failing test**

Append to the existing `tests` module in `service.rs`:

```rust
    #[tokio::test]
    async fn lock_kb_serializes_writers() {
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();
        let svc1 = svc.clone();
        let svc2 = svc.clone();
        let h1 = tokio::spawn(async move {
            let _g = svc1.lock_kb("k").await;
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            std::time::Instant::now()
        });
        // Brief delay so h1 acquires the lock first.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let h2 = tokio::spawn(async move {
            let _g = svc2.lock_kb("k").await;
            std::time::Instant::now()
        });
        let t1 = h1.await.unwrap();
        let t2 = h2.await.unwrap();
        assert!(t2 >= t1, "h2 must observe lock acquisition after h1 released");
    }
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p biorouter-mcp --lib knowledge::service
# Expected: prior 7 + 1 new = 8 passed
```

- [ ] **Step 5: Commit**

```bash
git add crates/biorouter-mcp/Cargo.toml crates/biorouter-mcp/src/knowledge/service.rs
git commit -m "feat(knowledge): per-KB tokio Mutex for concurrent-write safety"
```

---

## Task 4: Expose transaction primitives via MCP

**Files:**
- Modify: `crates/biorouter-mcp/src/knowledge/server.rs`

- [ ] **Step 1: Add three tool methods**

In `server.rs`, alongside the existing `#[tool]` methods on `KnowledgeServer`, add:

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct BeginTxnParams {
    pub kb_id: String,
    pub label: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CommitTxnParams {
    pub kb_id: String,
    pub txn: String,           // branch name returned by begin
    pub summary: String,
    pub kind: crate::knowledge::types::ChangeKind,
    #[serde(default)]
    pub delta: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AbortTxnParams {
    pub kb_id: String,
    pub txn: String,
}

#[tool(name = "kb_begin_txn", description = "Open a transactional working branch on a knowledge base. Returns the txn handle (branch name) for use with subsequent mutating primitives.")]
pub async fn kb_begin_txn(&self, p: Parameters<BeginTxnParams>) -> Result<CallToolResult, ErrorData> {
    let p = p.0;
    let kb_root = biorouter::knowledge::paths::kb_root(self.service.root(), &p.kb_id);
    let repo = biorouter::knowledge::git::GitRepo::open(&kb_root).map_err(into_err)?;
    let txn = repo.begin_txn(&p.label).map_err(into_err)?;
    ok_json(&serde_json::json!({ "txn": txn.branch }))
}

#[tool(name = "kb_commit_txn", description = "Squash-merge a transaction branch onto the main history with the given kind/summary/delta.")]
pub async fn kb_commit_txn(&self, p: Parameters<CommitTxnParams>) -> Result<CallToolResult, ErrorData> {
    let p = p.0;
    let kb_root = biorouter::knowledge::paths::kb_root(self.service.root(), &p.kb_id);
    let repo = biorouter::knowledge::git::GitRepo::open(&kb_root).map_err(into_err)?;
    let txn = biorouter::knowledge::git::Txn { branch: p.txn };
    let sha = repo.commit_txn(&txn, p.kind, &p.summary, p.delta.as_deref()).map_err(into_err)?;
    ok_json(&serde_json::json!({ "commit_sha": sha }))
}

#[tool(name = "kb_abort_txn", description = "Discard a transaction branch and restore the working tree to main.")]
pub async fn kb_abort_txn(&self, p: Parameters<AbortTxnParams>) -> Result<CallToolResult, ErrorData> {
    let p = p.0;
    let kb_root = biorouter::knowledge::paths::kb_root(self.service.root(), &p.kb_id);
    let repo = biorouter::knowledge::git::GitRepo::open(&kb_root).map_err(into_err)?;
    let txn = biorouter::knowledge::git::Txn { branch: p.txn };
    repo.abort_txn(&txn).map_err(into_err)?;
    ok_json(&serde_json::json!({ "ok": true }))
}
```

Also expose `kb_search` and `kb_append_log` here, following the same `#[tool]` pattern. Their parameter structs are: `{kb_id, query, limit?}` and `{kb_id, kind, summary, delta?}`.

- [ ] **Step 2: Build + smoke**

```bash
cargo build -p biorouter-mcp
# Expected: clean
```

No new lib tests for this task — the MCP wrapper is a thin pass-through to already-tested service functions. An integration test is in Task 14.

- [ ] **Step 3: Commit**

```bash
git add crates/biorouter-mcp/src/knowledge/server.rs
git commit -m "feat(knowledge): expose kb_begin_txn / commit / abort + kb_search + kb_append_log as MCP tools"
```

---

## Task 5: `kb_set_active` / `kb_get_active` via session state

**Files:**
- Modify: `crates/biorouter-mcp/src/knowledge/server.rs`
- (No biorouter changes — the MCP server reads session state via the existing extension-data API)

- [ ] **Step 1: Implement** (see context note below)

Background: `Session::extension_data` is a `HashMap<String, Value>` keyed by `"<extension>.<version>"`. The Knowledge extension uses key `"knowledge"` + version `"v0"`. The value is a JSON object `{ "active_kb": "<kb-id>" }`.

In the MCP world, the running extension does NOT see the parent agent's `Session` directly — the rmcp protocol is stateless per tool call. We therefore store the active KB in a **process-local map keyed by client-id / session-id derived from `rmcp`'s `Peer` context**. If `Peer` doesn't expose a stable client id, fall back to a single `Arc<Mutex<Option<String>>>` (global active KB) and document the limitation; subsequent plans can refine it.

Add to `KnowledgeServer`:

```rust
#[derive(Clone, Default)]
pub struct ActiveKbState {
    inner: Arc<tokio::sync::Mutex<Option<String>>>,
}

impl ActiveKbState {
    pub async fn set(&self, kb_id: &str) { *self.inner.lock().await = Some(kb_id.to_string()); }
    pub async fn get(&self) -> Option<String> { self.inner.lock().await.clone() }
    pub async fn clear(&self) { *self.inner.lock().await = None; }
}
```

Add a field `active: ActiveKbState` to `KnowledgeServer`, initialize in `new()`.

Tools:

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetActiveParams { pub kb_id: String }

#[tool(name = "kb_set_active", description = "Set the active knowledge base for this session. Subsequent kb_* tool calls that omit kb_id will default to this one.")]
pub async fn kb_set_active(&self, p: Parameters<SetActiveParams>) -> Result<CallToolResult, ErrorData> {
    biorouter::knowledge::paths::validate_kb_id(&p.0.kb_id).map_err(|e| into_err(e.into()))?;
    self.active.set(&p.0.kb_id).await;
    ok_json(&serde_json::json!({ "ok": true, "active_kb": p.0.kb_id }))
}

#[tool(name = "kb_get_active", description = "Return the currently active knowledge base id (if any).")]
pub async fn kb_get_active(&self) -> Result<CallToolResult, ErrorData> {
    let v = self.active.get().await;
    ok_json(&serde_json::json!({ "active_kb": v }))
}
```

- [ ] **Step 2: Add a `kb_id_or_active` helper** so every primitive tool can accept an optional `kb_id`:

```rust
async fn kb_id_or_active(&self, supplied: Option<String>) -> Result<String, ErrorData> {
    if let Some(id) = supplied { return Ok(id); }
    self.active.get().await.ok_or_else(|| ErrorData::invalid_params(
        "kb_id not supplied and no active knowledge base is set. Call kb_set_active first.", None,
    ))
}
```

- [ ] **Step 3: Convert primitive tool params to `Option<String>`** for `kb_id` on the read-only tools (list_pages, read_page, get_graph, list_history, search). For mutating tools, keep `kb_id` required for safety. Each tool's body resolves via the helper:

```rust
let kb_id = self.kb_id_or_active(p.0.kb_id).await?;
```

- [ ] **Step 4: Smoke test**

```bash
cargo build -p biorouter-mcp
cargo test -p biorouter-mcp --lib knowledge 2>&1 | tail -3
# Expected: prior count unchanged
```

- [ ] **Step 5: Commit**

```bash
git add crates/biorouter-mcp/src/knowledge/server.rs
git commit -m "feat(knowledge): kb_set_active / kb_get_active + optional kb_id default"
```

---

## Task 6: SubAgent loop infrastructure

**Files:**
- Create: `crates/biorouter-mcp/src/knowledge/subagent/mod.rs`
- Create: `crates/biorouter-mcp/src/knowledge/subagent/loop_.rs`
- Create: `crates/biorouter-mcp/src/knowledge/subagent/events.rs`
- Modify: `crates/biorouter-mcp/src/knowledge/mod.rs` (`pub mod subagent;`)

This is the heaviest task in Plan 2. A `SubAgent` is a bounded LLM agent that:
1. Receives a `system_prompt: String`, a `user_message: String`, a `tools: Vec<Tool>`, an `Arc<dyn Provider>`, and bounds `(max_steps, max_wall, max_tokens)`.
2. Calls `provider.complete()` in a loop. Each iteration: parse tool calls from the assistant's reply, dispatch each via a `ToolDispatch` callback, push results back as new messages, then loop until the model emits a `complete()` sentinel tool or no tool calls remain.
3. Returns a `SubAgentResult { final_message: String, events: Vec<SubAgentEvent>, steps_used: usize, tokens_used: u64 }`.
4. Aborts cleanly on time budget exceeded, step budget exceeded, or external cancellation.

- [ ] **Step 1: Define `SubAgentEvent`**

`events.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SubAgentEvent {
    Step { index: usize, assistant_text: String },
    ToolCall { name: String, args: serde_json::Value },
    ToolResult { name: String, ok: bool, summary: String },
    Done { reason: DoneReason, final_text: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DoneReason {
    CompleteSentinel,
    NoMoreToolCalls,
    StepBudgetReached,
    TimeBudgetReached,
    Cancelled,
    Error,
}
```

- [ ] **Step 2: Define `SubAgent`**

`loop_.rs`:

```rust
use crate::knowledge::subagent::events::{DoneReason, SubAgentEvent};
use anyhow::{Context, Result};
use biorouter::providers::base::Provider;
use rmcp::model::Tool;
use std::{sync::Arc, time::{Duration, Instant}};
use tokio::sync::mpsc;

pub struct SubAgentBounds {
    pub max_steps: usize,
    pub max_wall: Duration,
    pub max_tokens: u64,
}
impl Default for SubAgentBounds {
    fn default() -> Self { Self { max_steps: 30, max_wall: Duration::from_secs(300), max_tokens: 200_000 } }
}

pub struct SubAgentResult {
    pub final_text: String,
    pub events: Vec<SubAgentEvent>,
    pub reason: DoneReason,
    pub steps_used: usize,
}

/// Trait the macro implements: given a tool name + args, run it and return the result string.
#[async_trait::async_trait]
pub trait ToolDispatch: Send + Sync {
    async fn call(&self, name: &str, args: serde_json::Value) -> Result<String>;
}

pub struct SubAgent {
    pub provider: Arc<dyn Provider>,
    pub tools: Vec<Tool>,
    pub system_prompt: String,
    pub bounds: SubAgentBounds,
}

impl SubAgent {
    pub async fn run(
        &self,
        user_message: &str,
        dispatch: &dyn ToolDispatch,
        cancel: Option<&tokio::sync::Notify>,
    ) -> Result<SubAgentResult> {
        let mut events = Vec::new();
        let mut messages: Vec<biorouter::message::Message> =
            vec![biorouter::message::Message::user(user_message)];
        let started = Instant::now();
        let mut steps = 0;

        loop {
            if steps >= self.bounds.max_steps {
                return Ok(done(events, DoneReason::StepBudgetReached, "step budget reached", steps));
            }
            if started.elapsed() > self.bounds.max_wall {
                return Ok(done(events, DoneReason::TimeBudgetReached, "time budget reached", steps));
            }
            if let Some(c) = cancel {
                if c.notified().now_or_never().is_some() {
                    return Ok(done(events, DoneReason::Cancelled, "cancelled", steps));
                }
            }

            let (assistant, _usage) = self.provider
                .complete(&self.system_prompt, &messages, &self.tools)
                .await
                .context("provider.complete failed")?;
            events.push(SubAgentEvent::Step { index: steps, assistant_text: assistant.text().to_string() });

            let tool_calls = assistant.tool_calls();
            if tool_calls.is_empty() {
                return Ok(done(events, DoneReason::NoMoreToolCalls, assistant.text(), steps));
            }
            if tool_calls.iter().any(|t| t.name == "complete") {
                return Ok(done(events, DoneReason::CompleteSentinel, assistant.text(), steps));
            }
            for call in &tool_calls {
                events.push(SubAgentEvent::ToolCall { name: call.name.clone(), args: call.args.clone() });
                let result = match dispatch.call(&call.name, call.args.clone()).await {
                    Ok(s) => { events.push(SubAgentEvent::ToolResult { name: call.name.clone(), ok: true, summary: s.chars().take(120).collect() }); s }
                    Err(e) => { events.push(SubAgentEvent::ToolResult { name: call.name.clone(), ok: false, summary: e.to_string() }); format!("error: {e}") }
                };
                messages.push(biorouter::message::Message::tool_result(&call.name, &result));
            }
            steps += 1;
        }
    }
}

fn done(events: Vec<SubAgentEvent>, reason: DoneReason, text: &str, steps: usize) -> SubAgentResult {
    SubAgentResult { final_text: text.to_string(), events, reason, steps_used: steps }
}
```

NOTE: the exact `biorouter::message::Message` API names may differ. Inspect `crates/biorouter/src/conversation/message.rs` (or wherever the agent's existing `Message` lives — `assistant.tool_calls()`, `Message::user()`, `Message::tool_result()`) and adapt the wrapping accordingly. The contract is: build a user message, append assistant replies + tool results, then loop. Use whatever API the Plan-1 codebase already uses.

- [ ] **Step 3: Tests**

Write a test that uses a `MockProvider` (lives next to the SubAgent module) which returns canned assistant messages. The test verifies:

- A canned reply with one tool call followed by a canned reply with no tool calls completes in 2 steps.
- A canned reply that always returns the same tool call hits the step budget after `max_steps` iterations.
- Cancellation via `Notify::notify_one` returns `DoneReason::Cancelled`.

Mock provider:

```rust
pub struct MockProvider {
    pub replies: tokio::sync::Mutex<Vec<biorouter::message::Message>>,
}
#[async_trait::async_trait]
impl biorouter::providers::base::Provider for MockProvider {
    async fn complete(&self, _: &str, _: &[biorouter::message::Message], _: &[Tool])
        -> Result<(biorouter::message::Message, biorouter::providers::base::ProviderUsage), anyhow::Error>
    {
        let mut q = self.replies.lock().await;
        let m = q.remove(0);
        Ok((m, Default::default()))
    }
    // ... other Provider trait methods can be unimplemented!() for tests
}
```

Three tests on this trait.

- [ ] **Step 4: Run tests + commit**

```bash
cargo test -p biorouter-mcp --lib knowledge::subagent
# Expected: 3 passed
git add crates/biorouter-mcp/src/knowledge/subagent crates/biorouter-mcp/src/knowledge/mod.rs
git commit -m "feat(knowledge): bounded SubAgent loop with tool dispatch + step/time bounds + cancel"
```

---

## Task 7: KB-primitive tool registry for the sub-agent

The sub-agent uses a fixed set of tools: `kb_read_page`, `kb_write_page`, `kb_list_pages`, `kb_search`, `kb_add_raw_source`, `kb_append_log`, `kb_classify_source`, and a `complete()` sentinel. Build a single `ToolDispatch` implementation that maps tool-name strings to the `KnowledgeService` methods.

**Files:**
- Create: `crates/biorouter-mcp/src/knowledge/subagent/kb_tools.rs`
- Modify: `subagent/mod.rs` (export the module)

- [ ] **Step 1: Implement `KbToolDispatch`**

```rust
use crate::knowledge::{
    convert::SourceInput,
    service::KnowledgeService,
    store,
    log as kb_log,
    types::ChangeKind,
};
use crate::knowledge::subagent::loop_::ToolDispatch;
use anyhow::Result;
use serde_json::Value;

pub struct KbToolDispatch {
    pub svc: KnowledgeService,
    pub kb_id: String,
    pub txn_branch: String,
}

#[async_trait::async_trait]
impl ToolDispatch for KbToolDispatch {
    async fn call(&self, name: &str, args: Value) -> Result<String> {
        let kb_root = crate::knowledge::paths::kb_root(self.svc.root(), &self.kb_id);
        match name {
            "kb_list_pages" => {
                let prefix = args.get("path_prefix").and_then(|v| v.as_str());
                let pages = store::list_pages(&kb_root, prefix)?;
                Ok(serde_json::to_string(&pages)?)
            }
            "kb_read_page" => {
                let path = args["path"].as_str().ok_or_else(|| anyhow::anyhow!("missing path"))?;
                let p = store::read_page(&kb_root, path)?;
                Ok(serde_json::to_string(&p)?)
            }
            "kb_write_page" => {
                let path = args["path"].as_str().ok_or_else(|| anyhow::anyhow!("missing path"))?;
                let content = args["content"].as_str().ok_or_else(|| anyhow::anyhow!("missing content"))?;
                let msg = args.get("commit_message").and_then(|v| v.as_str()).unwrap_or("subagent write");
                let sha = store::write_page(&kb_root, path, content, msg, Some(&self.txn_branch))?;
                Ok(serde_json::json!({ "commit_sha": sha }).to_string())
            }
            "kb_search" => {
                let q = args["query"].as_str().ok_or_else(|| anyhow::anyhow!("missing query"))?;
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
                let hits = store::search(&kb_root, q, limit)?;
                Ok(serde_json::to_string(&hits)?)
            }
            "kb_append_log" => {
                let summary = args["summary"].as_str().ok_or_else(|| anyhow::anyhow!("missing summary"))?;
                let kind_str = args["kind"].as_str().unwrap_or("manual");
                let kind = parse_change_kind(kind_str)?;
                let delta = args.get("delta").and_then(|v| v.as_str());
                kb_log::append(&kb_root, kind, summary, delta, Some(&self.txn_branch))?;
                Ok(serde_json::json!({ "ok": true }).to_string())
            }
            "kb_add_raw_source" => {
                let input: SourceInput = serde_json::from_value(args["source"].clone())?;
                let res = self.svc.add_raw_source(&self.kb_id, input, Some(&self.txn_branch)).await?;
                Ok(serde_json::to_string(&res)?)
            }
            "kb_classify_source" => {
                let source_id = args["source_id"].as_str().ok_or_else(|| anyhow::anyhow!("missing source_id"))?;
                let meta = crate::knowledge::raw::read_meta(&kb_root, source_id)?;
                Ok(serde_json::to_string(&meta.credibility)?)
            }
            other => anyhow::bail!("unknown tool: {other}"),
        }
    }
}

fn parse_change_kind(s: &str) -> Result<ChangeKind> {
    Ok(match s {
        "ingest" => ChangeKind::Ingest,
        "link" => ChangeKind::Link,
        "flag" => ChangeKind::Flag,
        "query" => ChangeKind::Query,
        "lint" => ChangeKind::Lint,
        "manual" => ChangeKind::Manual,
        other => anyhow::bail!("invalid kind: {other}"),
    })
}
```

- [ ] **Step 2: Add a `tool_specs()` function** that returns the `Vec<Tool>` to give to the sub-agent (these match the `name` arms above, with their schemas in `rmcp::model::Tool`). Keep schemas minimal — name + description + argument JSON schema.

- [ ] **Step 3: Test**

Single test: instantiate a `KnowledgeService` on a tempdir, create a KB, build a `KbToolDispatch`, call `dispatch.call("kb_write_page", ...)`, then `dispatch.call("kb_read_page", ...)`, and assert round-trip.

- [ ] **Step 4: Commit**

```bash
git add crates/biorouter-mcp/src/knowledge/subagent
git commit -m "feat(knowledge): KbToolDispatch for sub-agent (read/write/search/log/add_raw/classify)"
```

---

## Task 8: Operating-procedure templates

**Files:**
- Create: `crates/biorouter-mcp/src/knowledge/subagent/procedures.rs`

Each macro hands the sub-agent a different system prompt composed of the per-KB `schema.md` plus a macro-specific procedure.

- [ ] **Step 1: Define three procedure templates as `pub const` strings**

```rust
pub const INGEST_PROCEDURE: &str = r#"
You are integrating a new source into a personal knowledge base. You have already
been told the source-id and where to read it (raw/<id>/source.md and raw/<id>/meta.yaml).
Your job is to:

1. Read the source markdown and its meta.yaml.
2. Identify the biomedical entities and concepts the source touches.
3. For each: read existing knowledge/entities/ or knowledge/concepts/ pages if they exist.
4. Write or update knowledge/sources/<source-id>.md with: 2-3 sentence summary, key claims as bullets,
   methods if applicable, limitations, and outbound [[knowledge-link]] references.
5. For each entity/concept mentioned, create or update its page with a backlink to the source.
6. If a claim contradicts an existing page, set `contradiction: true` in frontmatter and
   add an "## Open contradictions" section listing positions + sources.
7. Update index.md with any new pages.
8. Append a one-line entry to log.md via kb_append_log with kind=ingest and a one-sentence summary.
9. Call complete() when done.

Respect the schema.md voice and conventions above. Prefer concise, evidence-led language.
Hedge claims sourced only from web or personal materials.
"#;

pub const QUERY_PROCEDURE: &str = r#"
You are answering a question against a personal knowledge base.

1. Use kb_search to find relevant pages.
2. Use kb_read_page on the top hits.
3. Compose an answer that cites pages with [[knowledge-link]] references.
4. If the user asked you to file the answer (file_as_page=true), write it to
   knowledge/notes/<slug>.md and append a log entry via kb_append_log with kind=query.
5. Call complete() with your final answer as the assistant message.

Be precise. Do not invent facts not present in the KB.
"#;

pub const LINT_PROCEDURE: &str = r#"
You are auditing a personal knowledge base for hygiene issues.

Find:
1. Pages with no inbound links (orphans).
2. Pages with frontmatter contradiction: true that have not been resolved.
3. Concepts mentioned in source pages but lacking a dedicated knowledge/concepts/ page.
4. Sources >90 days old not referenced from any other page.

If autofix=true:
- Add missing cross-references where unambiguous.
- Create stub pages for orphaned concepts (frontmatter + a "## TODO: expand" section).
- Append a kb_append_log entry with kind=lint summarizing what you fixed.

Otherwise, return a structured report (do not modify the KB). Call complete() when done.
"#;
```

- [ ] **Step 2: Commit**

No tests needed for static strings.

```bash
git add crates/biorouter-mcp/src/knowledge/subagent/procedures.rs
git commit -m "feat(knowledge): ingest/query/lint operating procedure templates"
```

---

## Task 9: Real agentic credibility fallback

**Files:**
- Modify: `crates/biorouter-mcp/src/knowledge/credibility/agentic.rs`

Replace the Plan-1 stub with a real SubAgent run that:
1. Has tools: `fetch_url(url)`, `crossref_search(query)`, `openalex_search(query)`.
2. Receives the source's URL / title / first 500 chars as the user message.
3. Returns a JSON `Credibility` struct.
4. Caps at 5 steps and 30s wall time.
5. Uses a cheap model (configurable; default = caller's chosen model or a fallback like `claude-haiku-4-5`).

- [ ] **Step 1: Rewrite `classify`**

Sketch:

```rust
pub async fn classify(input: &SourceInput, provider: Arc<dyn Provider>) -> Result<Credibility> {
    let user_msg = build_user_message(input);
    let tools = vec![
        // fetch_url(url) -> markdown
        // crossref_search(query) -> top-5 publishers
        // openalex_search(query) -> top-5 publishers
    ];
    let dispatch = AgenticDispatch::new();
    let agent = SubAgent {
        provider,
        tools,
        system_prompt: AGENTIC_PROCEDURE.into(),
        bounds: SubAgentBounds { max_steps: 5, max_wall: Duration::from_secs(30), max_tokens: 20_000 },
    };
    let result = agent.run(&user_msg, &dispatch, None).await?;
    parse_credibility_from(&result.final_text)
        .unwrap_or_else(|_| Credibility {
            tier: CredibilityTier::Web,
            confidence: 0.3,
            reasoning: format!("Agentic fallback could not parse result; final_text: {}", result.final_text),
            publisher: None, venue: None, doi: None, retracted: false,
            classifier_version: 2,
        })
}
```

The `AgenticDispatch` lives in this file. The system prompt `AGENTIC_PROCEDURE` instructs the agent: "Investigate the source. Use the tools. Conclude with a JSON object matching this schema: `{tier, confidence, publisher?, venue?, doi?, retracted, reasoning}`. Call `complete()` with ONLY that JSON."

- [ ] **Step 2: Adapt the existing `classify()` call sites**

`credibility/mod.rs`'s `classify()` currently calls `agentic::classify(input)` with one argument. It now needs `(input, provider)`. Plumb the provider through `KnowledgeService` or pass it in via a new arg. Simplest: change `credibility::classify` to accept `Option<Arc<dyn Provider>>` and skip the agentic step when `None` is passed.

- [ ] **Step 3: Tests**

Test the parse path with a canned mock provider that returns a valid JSON `Credibility` blob as its final message. Verify the function returns it correctly.

- [ ] **Step 4: Commit**

```bash
git add crates/biorouter-mcp/src/knowledge/credibility
git commit -m "feat(knowledge): real agentic credibility fallback (replaces Plan 1 stub)"
```

---

## Task 10: `kb_ingest_source` macro

**Files:**
- Create: `crates/biorouter-mcp/src/knowledge/macros/mod.rs`
- Create: `crates/biorouter-mcp/src/knowledge/macros/ingest.rs`
- Modify: `crates/biorouter-mcp/src/knowledge/mod.rs` (`pub mod macros;`)

- [ ] **Step 1: Implement `ingest`**

```rust
use crate::knowledge::{
    convert::SourceInput,
    git::GitRepo,
    log as kb_log,
    paths,
    service::KnowledgeService,
    subagent::{
        kb_tools::{tool_specs, KbToolDispatch},
        loop_::{SubAgent, SubAgentBounds},
        procedures::INGEST_PROCEDURE,
    },
    types::ChangeKind,
};
use anyhow::{Context, Result};
use biorouter::providers::base::Provider;
use std::sync::Arc;
use std::time::Duration;

pub struct IngestArgs {
    pub kb_id: String,
    pub source: SourceInput,
    pub provider: Arc<dyn Provider>,
    pub focus: Option<String>,
    pub bounds: SubAgentBounds,
}

pub async fn ingest(svc: &KnowledgeService, args: IngestArgs) -> Result<IngestResult> {
    let _lock = svc.lock_kb(&args.kb_id).await;
    let kb_root = paths::kb_root(svc.root(), &args.kb_id);

    // Materialize raw + classify (this commits, so do it BEFORE the txn so the raw is durable).
    let raw = svc.add_raw_source(&args.kb_id, args.source, None).await?;

    // Open a txn for the wiki integration.
    let repo = GitRepo::open(&kb_root)?;
    let txn = repo.begin_txn(&format!("ingest-{}", raw.source_id))?;

    // Build the per-KB system prompt: schema.md + INGEST_PROCEDURE.
    let schema = std::fs::read_to_string(kb_root.join("schema.md")).context("read schema.md")?;
    let system = format!("{schema}\n\n---\n{INGEST_PROCEDURE}");

    let dispatch = KbToolDispatch {
        svc: svc.clone(),
        kb_id: args.kb_id.clone(),
        txn_branch: txn.branch.clone(),
    };
    let agent = SubAgent {
        provider: args.provider,
        tools: tool_specs(),
        system_prompt: system,
        bounds: args.bounds,
    };
    let focus_line = args.focus.as_deref().unwrap_or("");
    let user = format!("New source to integrate: source-id={}. Focus hints: {focus_line}", raw.source_id);

    let agent_result = agent.run(&user, &dispatch, None).await;

    match agent_result {
        Ok(r) if matches!(r.reason, crate::knowledge::subagent::events::DoneReason::CompleteSentinel
                                  | crate::knowledge::subagent::events::DoneReason::NoMoreToolCalls) => {
            let sha = repo.commit_txn(&txn, ChangeKind::Ingest, &format!("ingest {}", raw.source_id),
                Some(&format!("+1 source · {} steps", r.steps_used)))?;
            Ok(IngestResult { source_id: raw.source_id, commit_sha: sha, steps: r.steps_used, events: r.events })
        }
        Ok(r) => {
            repo.abort_txn(&txn)?;
            anyhow::bail!("ingest sub-agent aborted: reason={:?}, final={}", r.reason, r.final_text)
        }
        Err(e) => {
            let _ = repo.abort_txn(&txn);
            Err(e)
        }
    }
}

pub struct IngestResult {
    pub source_id: String,
    pub commit_sha: String,
    pub steps: usize,
    pub events: Vec<crate::knowledge::subagent::events::SubAgentEvent>,
}
```

- [ ] **Step 2: Add `kb_ingest_source` MCP tool in `server.rs`**

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct IngestSourceParams {
    pub kb_id: String,
    pub source: RawSourceInput,        // already defined in Plan 1
    pub model: ModelRef,               // already defined in Plan 1 types
    #[serde(default)]
    pub focus: Option<String>,
}

#[tool(name = "kb_ingest_source", description = "Ingest a source into a knowledge base. Runs a bounded sub-agent that summarizes the source, integrates it into knowledge/, and commits as one logical change.")]
pub async fn kb_ingest_source(&self, p: Parameters<IngestSourceParams>) -> Result<CallToolResult, ErrorData> {
    let p = p.0;
    let provider = biorouter::providers::factory::create(
        &p.model.provider,
        biorouter::providers::base::ModelConfig::default_with(p.model.model.clone()),
    ).await.map_err(into_err)?;
    let args = crate::knowledge::macros::ingest::IngestArgs {
        kb_id: p.kb_id,
        source: p.source.into(),
        provider,
        focus: p.focus,
        bounds: Default::default(),
    };
    let result = crate::knowledge::macros::ingest::ingest(&self.service, args).await.map_err(into_err)?;
    ok_json(&serde_json::json!({
        "source_id": result.source_id,
        "commit_sha": result.commit_sha,
        "steps": result.steps,
    }))
}
```

The exact `ModelConfig` constructor may differ — inspect `crates/biorouter/src/providers/base.rs` for the right call.

- [ ] **Step 3: Integration test with mock provider**

`crates/biorouter/tests/knowledge_macros.rs`:

```rust
use biorouter_mcp::knowledge::{convert::SourceInput, service::KnowledgeService};
// + your mock provider import path
// + macros::ingest::ingest

#[tokio::test]
async fn ingest_writes_source_page_and_commits_one_change() {
    let dir = tempfile::tempdir().unwrap();
    let svc = KnowledgeService::new(dir.path().to_path_buf());
    svc.create_base("k", "K", None).unwrap();

    // Build a mock provider that returns:
    //   reply 1: tool_call kb_write_page knowledge/sources/<src-id>.md "stub"
    //   reply 2: complete() with "done"
    let provider = mock_two_step_provider(/* ... */);

    let result = biorouter_mcp::knowledge::macros::ingest::ingest(&svc, IngestArgs {
        kb_id: "k".into(),
        source: SourceInput::Text { text: "Note about HRV.".into(), title: Some("note".into()) },
        provider,
        focus: None,
        bounds: Default::default(),
    }).await.unwrap();

    let kb = dir.path().join("k");
    let log = svc.list_history("k", 10).unwrap();
    // Expect: create + add_raw + ingest commit (squashed) = 3 entries total
    assert_eq!(log.len(), 3);
    assert_eq!(log[0].kind, biorouter::knowledge::types::ChangeKind::Ingest);
    assert!(kb.join(format!("knowledge/sources/{}.md", result.source_id)).exists());
}
```

(The mock-provider impl + matching `Message`/`tool_calls()` types may need slight adjustment after you read the actual provider trait.)

- [ ] **Step 4: Commit**

```bash
git add crates/biorouter-mcp/src/knowledge/macros crates/biorouter-mcp/src/knowledge/mod.rs \
        crates/biorouter-mcp/src/knowledge/server.rs crates/biorouter/tests/knowledge_macros.rs
git commit -m "feat(knowledge): kb_ingest_source macro (sub-agent + txn-atomic commit)"
```

---

## Task 11: `kb_query` macro

**Files:**
- Create: `crates/biorouter-mcp/src/knowledge/macros/query.rs`
- Modify: `crates/biorouter-mcp/src/knowledge/server.rs`

Pattern mirrors Task 10. Differences:
- The system prompt uses `QUERY_PROCEDURE`.
- The user message is the user's question.
- The txn is optional: only if `file_as_page=true` does the macro need to commit a new note page. If `file_as_page=false`, the macro is read-only and skips the txn entirely.
- Returns `{ answer: String, cited_pages: Vec<String>, commit_sha: Option<String> }`.

- [ ] **Step 1: Implement `query`**

```rust
pub struct QueryArgs {
    pub kb_id: String,
    pub question: String,
    pub provider: Arc<dyn Provider>,
    pub file_as_page: bool,
    pub bounds: SubAgentBounds,
}

pub async fn query(svc: &KnowledgeService, args: QueryArgs) -> Result<QueryResult> {
    let _lock = svc.lock_kb(&args.kb_id).await;
    let kb_root = paths::kb_root(svc.root(), &args.kb_id);

    let txn_branch = if args.file_as_page {
        let repo = GitRepo::open(&kb_root)?;
        Some(repo.begin_txn("query")?.branch)
    } else {
        None
    };

    let schema = std::fs::read_to_string(kb_root.join("schema.md"))?;
    let mut system = format!("{schema}\n\n---\n{QUERY_PROCEDURE}");
    if !args.file_as_page {
        system.push_str("\n\nIMPORTANT: file_as_page is FALSE for this call. Do NOT write any pages. Read-only.");
    }

    let dispatch = KbToolDispatch {
        svc: svc.clone(),
        kb_id: args.kb_id.clone(),
        txn_branch: txn_branch.clone().unwrap_or_default(),
    };
    let agent = SubAgent {
        provider: args.provider,
        tools: tool_specs(),
        system_prompt: system,
        bounds: args.bounds,
    };
    let r = agent.run(&args.question, &dispatch, None).await?;

    let commit_sha = if let Some(branch) = txn_branch {
        let repo = GitRepo::open(&kb_root)?;
        let txn = crate::knowledge::git::Txn { branch };
        Some(repo.commit_txn(&txn, ChangeKind::Query, "query filed", Some(&format!("+1 note · {} steps", r.steps_used)))?)
    } else { None };

    Ok(QueryResult {
        answer: r.final_text,
        cited_pages: extract_wiki_links(&r.events),
        commit_sha,
    })
}

fn extract_wiki_links(events: &[crate::knowledge::subagent::events::SubAgentEvent]) -> Vec<String> {
    let re = regex::Regex::new(r"\[\[([^\]]+)\]\]").unwrap();
    let mut out = Vec::new();
    for e in events {
        if let crate::knowledge::subagent::events::SubAgentEvent::Step { assistant_text, .. } = e {
            for cap in re.captures_iter(assistant_text) {
                let s = cap.get(1).unwrap().as_str().to_string();
                if !out.contains(&s) { out.push(s); }
            }
        }
    }
    out
}
```

- [ ] **Step 2: MCP tool wiring + test**

Mirror Task 10's `kb_ingest_source` tool. Add a `knowledge_macros.rs` test for query that:
- Builds a fixture KB with one page about HRV.
- Mock provider replies with one tool_call `kb_search("HRV")` and one assistant final answer citing `[[HRV]]`.
- Asserts `cited_pages` contains `"HRV"`.

- [ ] **Step 3: Commit**

```bash
git add crates/biorouter-mcp/src/knowledge/macros/query.rs crates/biorouter-mcp/src/knowledge/server.rs crates/biorouter/tests/knowledge_macros.rs
git commit -m "feat(knowledge): kb_query macro (search + synthesize, optional file_as_page)"
```

---

## Task 12: `kb_lint` macro

**Files:**
- Create: `crates/biorouter-mcp/src/knowledge/macros/lint.rs`
- Modify: `crates/biorouter-mcp/src/knowledge/server.rs`

Same pattern, with these specifics:
- The macro itself does the **deterministic** report-generation first (orphan detection, contradiction scan, stale-source scan) so the sub-agent receives the structured report as part of its user message. The sub-agent then either summarizes (when `autofix=false`) or applies fixes (when `autofix=true`).
- If `autofix=true`, txn opened; otherwise no txn.

- [ ] **Step 1: Deterministic lint report**

In `lint.rs`:

```rust
pub struct LintReport {
    pub orphans: Vec<String>,
    pub contradictions: Vec<String>,
    pub stale_sources: Vec<String>,
    pub missing_concept_pages: Vec<String>,
}

pub fn scan(kb_root: &Path) -> Result<LintReport> {
    // … walks knowledge/, parses frontmatter, finds:
    //   - pages with zero inbound [[links]]
    //   - pages with frontmatter contradiction: true
    //   - sources/<id> whose meta.yaml.ingested_at < 90 days ago AND no inbound links
    //   - capitalized tokens in source.md bodies that don't have a knowledge/entities/<token>.md page
}
```

Tests against fixture data.

- [ ] **Step 2: Sub-agent path for autofix**

```rust
pub struct LintArgs {
    pub kb_id: String,
    pub provider: Arc<dyn Provider>,
    pub autofix: bool,
    pub bounds: SubAgentBounds,
}

pub async fn lint(svc: &KnowledgeService, args: LintArgs) -> Result<LintResult> {
    let _lock = svc.lock_kb(&args.kb_id).await;
    let kb_root = paths::kb_root(svc.root(), &args.kb_id);
    let report = scan(&kb_root)?;
    if !args.autofix {
        return Ok(LintResult { report: report.clone(), commit_sha: None, fixes_applied: 0 });
    }
    let repo = GitRepo::open(&kb_root)?;
    let txn = repo.begin_txn("lint")?;
    // … sub-agent run analogous to ingest …
    let sha = repo.commit_txn(&txn, ChangeKind::Lint, "lint autofix", None)?;
    Ok(LintResult { report, commit_sha: Some(sha), fixes_applied: r.events.iter().filter(|e| matches!(e, SubAgentEvent::ToolResult { name, ok: true, .. } if name == "kb_write_page")).count() })
}

pub struct LintResult {
    pub report: LintReport,
    pub commit_sha: Option<String>,
    pub fixes_applied: usize,
}
```

- [ ] **Step 3: MCP tool + tests**

Tests:
- Fixture KB with one orphan page → scan returns it in `orphans`.
- Fixture KB with `contradiction: true` page → scan returns it.
- Autofix path: mock provider creates a missing entity page; assert commit happens and report's `missing_concept_pages` shrinks.

- [ ] **Step 4: Commit**

```bash
git add crates/biorouter-mcp/src/knowledge/macros/lint.rs crates/biorouter-mcp/src/knowledge/server.rs crates/biorouter/tests/knowledge_macros.rs
git commit -m "feat(knowledge): kb_lint macro (deterministic scan + optional sub-agent autofix)"
```

---

## Task 13: End-to-end integration test

**Files:**
- Modify: `crates/biorouter/tests/knowledge_e2e.rs` (or create `knowledge_macros_e2e.rs`)

- [ ] **Step 1: Add an e2e flow that exercises ingest + query + lint**

```rust
#[tokio::test]
async fn macros_e2e_ingest_query_lint() {
    let dir = tempfile::tempdir().unwrap();
    let svc = biorouter::knowledge::service::KnowledgeService::new(dir.path().to_path_buf());
    svc.create_base("e2e", "E2E", None).unwrap();

    // Ingest: mock provider that writes source + one entity page.
    let provider = mock_ingest_provider();
    let r1 = biorouter_mcp::knowledge::macros::ingest::ingest(&svc, IngestArgs {
        kb_id: "e2e".into(),
        source: SourceInput::Text { text: "HRV improves after zone-2.".into(), title: Some("HRV note".into()) },
        provider,
        focus: None,
        bounds: Default::default(),
    }).await.unwrap();

    // Query: mock provider returns "[[HRV]] improves..."
    let provider = mock_query_provider();
    let r2 = biorouter_mcp::knowledge::macros::query::query(&svc, QueryArgs {
        kb_id: "e2e".into(),
        question: "Does zone-2 affect HRV?".into(),
        provider,
        file_as_page: false,
        bounds: Default::default(),
    }).await.unwrap();
    assert!(r2.cited_pages.contains(&"HRV".to_string()));

    // Lint: deterministic scan only (no autofix).
    let report = biorouter_mcp::knowledge::macros::lint::scan(&dir.path().join("e2e")).unwrap();
    // Should be clean after a single ingest.
    assert!(report.orphans.is_empty() || report.orphans.len() < 3);
}
```

- [ ] **Step 2: Run + commit**

```bash
cargo test -p biorouter --test knowledge_e2e
git add crates/biorouter/tests/knowledge_e2e.rs
git commit -m "test(knowledge): e2e macros (ingest + query + lint) with mock provider"
```

---

## Task 14: Update CLAUDE.md and final verification

- [ ] **Step 1: CLAUDE.md edit**

In the Core Agent Library section, extend the `knowledge/` bullet:

```markdown
- **`knowledge/`** — Personal knowledge base: storage, git history, file
  conversion, credibility classification, graph derivation, **macros
  (ingest / query / lint) backed by a bounded sub-agent loop**, and
  the per-KB concurrency mutex. The shared service backs both the
  `knowledge` MCP extension and (in Plan 3) HTTP routes.
```

- [ ] **Step 2: Final verification**

```bash
cd /Users/wgu/Desktop/biorouter-knowledge
source bin/activate-hermit
cargo fmt -p biorouter -p biorouter-mcp -- --check    # knowledge files only
cargo clippy -p biorouter -p biorouter-mcp --no-deps -- -D warnings
cargo test -p biorouter-mcp --lib knowledge
cargo test -p biorouter --test knowledge_e2e
cargo test -p biorouter --test knowledge_macros 2>&1 | tail -5
cargo test -p biorouter-mcp --test knowledge_registered
```

All clean. Test count should be:
- `biorouter-mcp --lib knowledge`: 81 (Plan 1 baseline) + 2 (search) + 2 (log) + 1 (lock) + 3 (subagent) + 1 (kb_tools) + 1 (agentic) + ~3 (macros internal) ≈ 94+
- `biorouter --test knowledge_e2e`: 1 (Plan 1) + 1 (Plan 2 macros e2e) = 2

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "docs(claude): document Plan 2 macros + sub-agent loop"
```

---

## What this plan does NOT cover (handled by Plan 3+)

- HTTP routes (REST + SSE streaming of sub-agent events) → Plan 3.
- The frontend route, KB selector, ingest panel, graph view, change-log drawer → Plans 4 & 5.
- Chat-side KB chip + slash commands → Plan 6.
- `.brkb` export/import → Plan 3.
- Session-state binding for `kb_set_active` via the chat session (current Plan 2 implementation uses a process-local mutex; a true session-aware version requires deeper changes to extension-manager and rmcp's session/peer model).

## Open risks

- **Provider message API drift**: The `SubAgent` loop assumes specific shapes for `Message::user()`, `Message::tool_result()`, and `Message.tool_calls()`. If those don't exist verbatim, Tasks 6, 9, 10, 11, 12 all need minor adjustments to match the actual conversation/message types. Inspect `crates/biorouter/src/conversation/` early in Task 6.
- **`provider.complete()` tool-call schema**: rmcp `Tool` is the schema rmcp speaks; biorouter's `Provider::complete()` might want a different `Tool` type. Adapt as needed.
- **Mock provider in tests**: writing a `Provider` impl for tests requires implementing every method on the trait. If the trait is large, use `unimplemented!()` for unused methods or extract a smaller `Completer` trait. Note any such refactor in the executor's report.
- **bm25 crate API**: the snippet in Task 1 was written against `bm25 = "2.3"`. If the actual installed version's API differs, adapt.

## Related documentation

- [Knowledge founding design](founding-design.md) — the macro and sub-agent-loop design this plan implements, including the step and time budgets.
- [Plan 1 — storage, git and graph](plan-1-storage-git-and-graph.md) — the primitives and git transactions this plan builds on, and the source of the "81 passed" baseline quoted above.
- [Plan 3 — HTTP routes and export/import](plan-3-http-routes-and-export.md) — takes up the SSE streaming this plan explicitly deferred.
- [Plan 6 — chat integration and closeout](plan-6-chat-integration-and-closeout.md) — completes the session-scoped active-KB binding this plan implements with a process-local mutex.
