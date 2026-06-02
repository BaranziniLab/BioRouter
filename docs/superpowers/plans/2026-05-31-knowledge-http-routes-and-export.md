# Knowledge HTTP Routes + Export/Import Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose the Knowledge backend (Plan 1 primitives + Plan 2 macros) over HTTP from `biorouter-server`, add `.brkb` export/import, and refresh the auto-generated TypeScript client. After Plan 3, the desktop UI (Plans 4-6) can call every operation the spec promises — including SSE-streamed macros — without further backend work.

**Architecture:**
- A `ProviderCompleter` adapter in `biorouter` wraps `Arc<dyn biorouter::providers::Provider>` and implements `biorouter_mcp::knowledge::subagent::loop_::Completer`. This is the dep-direction-correct bridge (biorouter already depends on biorouter-mcp) that lets HTTP handlers turn a user-picked model into something the Plan-2 macros accept.
- `KnowledgeService` is added to `AppState` as `Arc<KnowledgeService>` so every handler can reach it via `State(state): State<Arc<AppState>>`.
- One new file `crates/biorouter-server/src/routes/knowledge.rs` hosts all handlers. They follow the existing utoipa-annotated style from `routes/config_management.rs`.
- SSE for the three macro endpoints reuses the existing `SseResponse` pattern from `reply.rs`. The Plan-2 `SubAgent::run` is extended with an optional `mpsc::Sender<SubAgentEvent>` so events stream live instead of only landing in the final `SubAgentResult.events` vector.
- `.brkb` is a zip of the KB directory (including `.git/`) implemented with the existing `zip` crate dep. Export streams the bytes to the response; import accepts a multipart upload.

**Tech Stack:** axum, utoipa, zip 0.6, tokio_stream, the existing `biorouter` provider factory.

**Source spec:** [`docs/superpowers/specs/2026-05-30-knowledge-design.md`](../specs/2026-05-30-knowledge-design.md).

**This is Plan 3 of ~6.** Plans 4-6 (frontend route, graph view, change log drawer, chat KB chip) consume what Plan 3 ships.

**TDD note:** Same convention as Plans 1-2 — most tasks combine "write tests" + "write impl" into single steps. Verification commands gate each task.

---

## Before starting

- [ ] **Pre-step A:** confirm branch + baseline.

```bash
cd /Users/wgu/Desktop/BioRouter-knowledge && source bin/activate-hermit
git rev-parse --abbrev-ref HEAD       # expect feature/knowledge
cargo test -p biorouter-mcp --lib knowledge 2>&1 | tail -3  # expect 105 passed
```

- [ ] **Pre-step B:** skim the integration points from the recon.

  - Route composition at [`crates/biorouter-server/src/routes/mod.rs:24-40`](crates/biorouter-server/src/routes/mod.rs#L24).
  - Reference handler style at [`config_management.rs:150-169`](crates/biorouter-server/src/routes/config_management.rs#L150).
  - Custom SSE response at [`reply.rs:86-118`](crates/biorouter-server/src/routes/reply.rs#L86) — events are written as `data: {json}\n\n` to a `mpsc::Sender<String>` that the response stream drains.
  - `AppState` at [`state.rs:17-39`](crates/biorouter-server/src/state.rs#L17).
  - OpenAPI doc derive at [`openapi.rs`](crates/biorouter-server/src/openapi.rs).

---

## File structure (decomposition map)

```
crates/biorouter/src/knowledge/
└── provider_completer.rs           — NEW: ProviderCompleter adapter (Arc<Provider> → Completer)

crates/biorouter-mcp/src/knowledge/
├── brkb.rs                         — NEW: export() / import() zip handling
└── subagent/loop_.rs               — EXTEND: SubAgent::run accepts Option<mpsc::Sender<SubAgentEvent>>

crates/biorouter-server/src/
├── state.rs                        — EXTEND: AppState.knowledge_service: Arc<KnowledgeService>
├── routes/
│   ├── mod.rs                      — EXTEND: register knowledge router
│   └── knowledge.rs                — NEW: all knowledge handlers (17 routes)
└── openapi.rs                      — EXTEND: register handlers in ApiDoc

ui/desktop/src/api/                  — REGENERATED via `just generate-openapi` (auto)
```

---

## Task 1: `ProviderCompleter` adapter

**Files:**
- Modify: `crates/biorouter/src/knowledge.rs` (or `crates/biorouter/src/knowledge/mod.rs` — whichever currently re-exports `biorouter_mcp::knowledge::*`)
- Create: `crates/biorouter/src/knowledge/provider_completer.rs` (will require turning the re-export file into a `mod.rs` if it isn't already)

- [ ] **Step 1: Convert the re-export file into a directory module if needed**

Check whether `crates/biorouter/src/knowledge.rs` exists or `crates/biorouter/src/knowledge/mod.rs` exists. If the former, create `crates/biorouter/src/knowledge/` directory, move its single line into `mod.rs`, delete the old file. Result:

```
crates/biorouter/src/knowledge/
└── mod.rs   ← contains: pub use biorouter_mcp::knowledge::*; + pub mod provider_completer;
```

Verify the workspace still builds:
```bash
cargo build -p biorouter 2>&1 | tail -3
```

- [ ] **Step 2: Implement the adapter**

`crates/biorouter/src/knowledge/provider_completer.rs`:

```rust
//! Bridges biorouter::providers::Provider → biorouter_mcp::knowledge::subagent::loop_::Completer.
//!
//! The Completer trait was introduced in Plan 2 to avoid a circular dep on biorouter
//! from within biorouter-mcp. This adapter lives in biorouter (which already depends on
//! biorouter-mcp) and lets HTTP handlers in biorouter-server pass a user-selected
//! Provider into the Plan-2 macros (ingest / query / lint / agentic credibility).

use crate::providers::base::{Message, Provider};
use anyhow::Result;
use async_trait::async_trait;
use biorouter_mcp::knowledge::subagent::loop_::{
    Completer, LlmMessage, LlmReply, LlmToolCall, Tool,
};
use rmcp::model::CallToolRequestParams;
use std::sync::Arc;

pub struct ProviderCompleter {
    pub provider: Arc<dyn Provider>,
}

impl ProviderCompleter {
    pub fn new(provider: Arc<dyn Provider>) -> Self { Self { provider } }
}

#[async_trait]
impl Completer for ProviderCompleter {
    async fn complete(
        &self,
        system_prompt: &str,
        messages: &[LlmMessage],
        tools: &[Tool],
    ) -> Result<LlmReply> {
        // 1. Convert LlmMessage → biorouter::providers::base::Message
        let provider_messages = messages
            .iter()
            .map(|m| llm_to_provider_message(m))
            .collect::<Vec<_>>();

        // 2. Convert subagent tool specs → rmcp Tool (Provider trait uses rmcp::model::Tool)
        let provider_tools = tools
            .iter()
            .map(|t| subagent_tool_to_rmcp(t))
            .collect::<Vec<_>>();

        // 3. Call provider.complete().
        let (reply, _usage) = self.provider
            .complete(system_prompt, &provider_messages, &provider_tools)
            .await
            .map_err(|e| anyhow::anyhow!("provider.complete failed: {e}"))?;

        // 4. Extract assistant text + tool calls from the biorouter Message.
        let text = reply.text();
        let tool_calls = extract_tool_calls(&reply);

        Ok(LlmReply { text: text.to_string(), tool_calls })
    }
}

fn llm_to_provider_message(m: &LlmMessage) -> Message {
    // Inspect crates/biorouter/src/conversation/message.rs (or wherever Message lives)
    // and implement the conversion. The shape Batch B discovered:
    //   - LlmMessage::User { content }   → Message::user(content)
    //   - LlmMessage::Assistant { text, tool_calls } → Message with role=Assistant containing
    //     a Text content + one ToolRequest content per tool call
    //   - LlmMessage::ToolResult { name, result } → Message::tool_response(name, result)
    // If the API differs, adapt.
    todo!("implement based on actual biorouter Message constructors")
}

fn subagent_tool_to_rmcp(t: &Tool) -> rmcp::model::Tool {
    // The Tool type in biorouter_mcp::knowledge::subagent::loop_ is already
    // rmcp::model::Tool (Batch B verified). So this is a no-op clone.
    t.clone()
}

fn extract_tool_calls(msg: &Message) -> Vec<LlmToolCall> {
    // Walk msg.content() and pull out ToolRequest items. Each contains
    // a CallToolRequestParams { name, arguments, ... } per Batch B's findings.
    todo!("implement based on Batch B's MessageContent::ToolRequest unpacking")
}
```

**You must** read [`crates/biorouter/src/conversation/message.rs`](crates/biorouter/src/conversation/message.rs) and `crates/biorouter-mcp/src/knowledge/subagent/loop_.rs` (the `LlmMessage`/`LlmReply`/`LlmToolCall` defs Batch B introduced) before filling in the two `todo!()`s. They are the exact translation Batch B's mock did, but with real Provider/Message types.

- [ ] **Step 3: Re-export + tests**

In `crates/biorouter/src/knowledge/mod.rs`:

```rust
pub use biorouter_mcp::knowledge::*;
pub mod provider_completer;
pub use provider_completer::ProviderCompleter;
```

Test (in `provider_completer.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    // Build a minimal mock Provider that returns a canned assistant Message with one tool
    // call. Wrap it in ProviderCompleter. Call complete(). Assert the LlmReply has the
    // expected text + one LlmToolCall.

    #[tokio::test]
    async fn roundtrips_text_and_tool_calls() {
        // ... mock provider returns: Message::assistant("ok").with_tool_call("kb_search", {"query":"x"})
        // Wrap in ProviderCompleter, call complete()
        // Assert: reply.text == "ok"
        // Assert: reply.tool_calls.len() == 1
        // Assert: reply.tool_calls[0].name == "kb_search"
        // Assert: reply.tool_calls[0].args["query"] == "x"
    }
}
```

If the mock-Provider trait implementation is large, paste only the methods the SubAgent actually calls (`complete`) and `unimplemented!()` the rest.

- [ ] **Step 4: Verify**

```bash
cargo test -p biorouter --lib knowledge::provider_completer
# Expected: 1 passed
```

- [ ] **Step 5: Commit**

```bash
git add crates/biorouter/src/knowledge
git commit -m "feat(knowledge): ProviderCompleter adapter (Arc<Provider> → Completer)"
```

---

## Task 2: SubAgent emits live events (Plan 2 retrofit)

**Files:**
- Modify: `crates/biorouter-mcp/src/knowledge/subagent/loop_.rs`

The Plan-2 `SubAgent::run` returns events as part of `SubAgentResult.events` AFTER the run completes. For SSE streaming we need events to arrive in real time. Add an optional `mpsc::UnboundedSender<SubAgentEvent>` parameter.

- [ ] **Step 1: Extend the `run` signature**

```rust
pub async fn run(
    &self,
    user_message: &str,
    dispatch: &dyn ToolDispatch,
    cancel: Option<&tokio::sync::Notify>,
    event_sink: Option<&tokio::sync::mpsc::UnboundedSender<SubAgentEvent>>,
) -> Result<SubAgentResult>
```

Update every `events.push(ev)` inside the loop to also `if let Some(tx) = event_sink { let _ = tx.send(ev.clone()); }` immediately before the local push. Keep the local Vec — callers without an event sink still get events in the final result.

- [ ] **Step 2: Update macros to forward an optional event sink**

`crates/biorouter-mcp/src/knowledge/macros/ingest.rs`, `query.rs`, `lint.rs` — add an `event_sink: Option<UnboundedSender<SubAgentEvent>>` field to each macro's args struct (`IngestArgs.event_sink`, `QueryArgs.event_sink`, `LintArgs.event_sink`), thread it into the `agent.run(...)` call.

- [ ] **Step 3: Update existing call sites**

`add_raw_source`, `credibility::agentic::classify`, all macro tests — pass `None` for the new param. Compiler errors will point to every call site; update each minimally.

- [ ] **Step 4: Tests**

Add one test in `loop_.rs::tests` asserting that an event sink receives at least one `SubAgentEvent::Step` during a run:

```rust
#[tokio::test]
async fn run_emits_events_to_sink_live() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    // build a SubAgent with a two-step MockCompleter
    let _ = agent.run("test", &dispatch, None, Some(&tx)).await.unwrap();
    drop(tx); // close so rx.recv() returns None after draining
    let mut count = 0;
    while rx.recv().await.is_some() { count += 1; }
    assert!(count > 0, "sink received at least one event");
}
```

- [ ] **Step 5: Verify + commit**

```bash
cargo test -p biorouter-mcp --lib knowledge 2>&1 | tail -3
# Expected: 105 prior + 1 new = 106 passed (and all macro tests still pass after signature change)
git add crates/biorouter-mcp/src/knowledge
git commit -m "feat(knowledge): SubAgent.run + macros emit live events via optional mpsc sink"
```

---

## Task 3: `.brkb` export/import

**Files:**
- Create: `crates/biorouter-mcp/src/knowledge/brkb.rs`
- Modify: `crates/biorouter-mcp/src/knowledge/mod.rs` (`pub mod brkb;`)
- Modify: `crates/biorouter-mcp/src/knowledge/service.rs` (add `export_brkb` / `import_brkb` methods that wrap the brkb module)

- [ ] **Step 1: Implement export and import**

```rust
// brkb.rs
use anyhow::{Context, Result};
use std::io::{Cursor, Read, Seek, Write};
use std::path::{Path, PathBuf};
use zip::{write::FileOptions, ZipArchive, ZipWriter};

/// Pack a knowledge base directory (including .git, manifest.yaml, raw/, knowledge/, .biorouter-knowledge/)
/// into a .brkb zip and write the bytes to `out`. Walks the directory tree.
pub fn export<W: Write + Seek>(kb_root: &Path, out: &mut W) -> Result<()> {
    let mut zip = ZipWriter::new(out);
    let opts = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let kb_id = kb_root.file_name().ok_or_else(|| anyhow::anyhow!("kb root has no basename"))?
        .to_string_lossy().to_string();
    walk(kb_root, kb_root, &kb_id, &mut zip, opts)?;
    zip.finish().context("finish zip")?;
    Ok(())
}

fn walk<W: Write + Seek>(
    base: &Path, dir: &Path, prefix: &str,
    zip: &mut ZipWriter<W>, opts: FileOptions,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let rel = path.strip_prefix(base)?;
        let archive_path = format!("{prefix}/{}", rel.to_string_lossy());
        if path.is_dir() {
            zip.add_directory(&archive_path, opts)?;
            walk(base, &path, prefix, zip, opts)?;
        } else {
            zip.start_file(&archive_path, opts)?;
            let mut f = std::fs::File::open(&path)?;
            std::io::copy(&mut f, zip)?;
        }
    }
    Ok(())
}

/// Unpack a .brkb zip into a fresh directory under `knowledge_root` and return the new kb_id.
/// The .brkb is expected to contain exactly one top-level directory (the kb_id at export time).
/// If that id collides with an existing KB at the destination, suffix with `-N` to disambiguate.
pub fn import<R: Read + Seek>(zip_bytes: R, knowledge_root: &Path) -> Result<String> {
    let mut archive = ZipArchive::new(zip_bytes).context("open zip archive")?;
    // Detect the single top-level directory.
    let mut top_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    for i in 0..archive.len() {
        let name = archive.by_index(i)?.name().to_string();
        if let Some(first) = name.split('/').next() {
            if !first.is_empty() { top_names.insert(first.to_string()); }
        }
    }
    let original_id = if top_names.len() == 1 {
        top_names.into_iter().next().unwrap()
    } else {
        anyhow::bail!("brkb must contain exactly one top-level directory, found {}", top_names.len());
    };
    // Resolve a non-colliding id.
    let mut id = original_id.clone();
    let mut suffix = 1;
    while knowledge_root.join(&id).exists() {
        suffix += 1;
        id = format!("{original_id}-{suffix}");
    }
    // Extract.
    let target = knowledge_root.join(&id);
    std::fs::create_dir_all(&target)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let entry_name = entry.name().to_string();
        let rel: PathBuf = entry_name
            .strip_prefix(&format!("{original_id}/"))
            .unwrap_or(entry_name.as_str())
            .into();
        let dest = target.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&dest)?;
        } else {
            if let Some(parent) = dest.parent() { std::fs::create_dir_all(parent)?; }
            let mut f = std::fs::File::create(&dest)?;
            std::io::copy(&mut entry, &mut f)?;
        }
    }
    Ok(id)
}
```

- [ ] **Step 2: Service-level wrappers**

In `service.rs`, add:

```rust
impl KnowledgeService {
    pub fn export_brkb(&self, kb_id: &str) -> Result<Vec<u8>> {
        paths::validate_kb_id(kb_id)?;
        let kb_root = paths::kb_root(&self.root, kb_id);
        if !kb_root.exists() { anyhow::bail!("kb '{kb_id}' not found"); }
        let mut buf = std::io::Cursor::new(Vec::new());
        crate::knowledge::brkb::export(&kb_root, &mut buf)?;
        Ok(buf.into_inner())
    }

    pub fn import_brkb(&self, zip_bytes: &[u8]) -> Result<String> {
        std::fs::create_dir_all(&self.root)?;
        let cursor = std::io::Cursor::new(zip_bytes);
        let new_id = crate::knowledge::brkb::import(cursor, &self.root)?;
        // Register in the top-level manifest.
        let path = paths::kb_root(&self.root, &new_id);
        crate::knowledge::registry::register(
            &self.root,
            crate::knowledge::types::RegistryEntry { id: new_id.clone(), path },
        )?;
        Ok(new_id)
    }
}
```

- [ ] **Step 3: Round-trip tests**

In `brkb.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::service::KnowledgeService;

    #[test]
    fn export_then_import_preserves_files() {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("orig", "Orig", None).unwrap();
        // add some pages
        let kb_root = dir.path().join("orig");
        std::fs::write(kb_root.join("knowledge").join("entities").join("hrv.md"), "---\ntitle: HRV\n---\nbody").unwrap();

        let bytes = svc.export_brkb("orig").unwrap();
        assert!(bytes.len() > 100, "zip has some content");

        // Import into a new root, expect a non-colliding id.
        let dir2 = tempfile::tempdir().unwrap();
        let svc2 = KnowledgeService::new(dir2.path().to_path_buf());
        let new_id = svc2.import_brkb(&bytes).unwrap();
        assert_eq!(new_id, "orig");
        assert!(dir2.path().join("orig").join("manifest.yaml").exists());
        assert!(dir2.path().join("orig").join("knowledge").join("entities").join("hrv.md").exists());
        assert!(dir2.path().join("orig").join(".git").exists(), "git dir travels with the zip");
        // Registry has it.
        let bases = svc2.list_bases().unwrap();
        assert!(bases.iter().any(|b| b.id == "orig"));
    }

    #[test]
    fn import_assigns_suffix_on_collision() {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("dup", "Dup", None).unwrap();
        let bytes = svc.export_brkb("dup").unwrap();
        // Import into the SAME root — should collide.
        let new_id = svc.import_brkb(&bytes).unwrap();
        assert_eq!(new_id, "dup-2");
        assert!(dir.path().join("dup").exists());
        assert!(dir.path().join("dup-2").exists());
    }
}
```

- [ ] **Step 4: Verify**

```bash
cargo test -p biorouter-mcp --lib knowledge::brkb 2>&1 | tail -3
# Expected: 2 passed
cargo test -p biorouter-mcp --lib knowledge 2>&1 | tail -3
# Expected: cumulative test count + 2
```

- [ ] **Step 5: Commit**

```bash
git add crates/biorouter-mcp/src/knowledge
git commit -m "feat(knowledge): .brkb zip export/import + service wrappers + registry sync"
```

---

## Task 4: Add `KnowledgeService` to `AppState`

**Files:**
- Modify: `crates/biorouter-server/src/state.rs`
- Modify: `crates/biorouter-server/Cargo.toml` (add `biorouter-mcp = { workspace = true }` as a dep if it isn't already)

- [ ] **Step 1: Inspect existing AppState constructor**

```bash
grep -n 'fn new\|impl AppState' crates/biorouter-server/src/state.rs | head -10
```

Read the surrounding ~30 lines. There's likely an `async fn new(...)` that takes config and constructs each `Arc<>` field. Add `knowledge_service` initialization.

- [ ] **Step 2: Add the field + initializer**

```rust
// state.rs
use std::sync::Arc;

pub struct AppState {
    // ... existing fields ...
    pub knowledge_service: Arc<biorouter::knowledge::service::KnowledgeService>,
}

impl AppState {
    pub async fn new(...) -> Result<Arc<Self>> {
        // ... existing setup ...
        let knowledge_service = Arc::new(
            biorouter::knowledge::service::KnowledgeService::new_default()?
        );
        // ... return Arc::new(Self { ..., knowledge_service })
    }
}
```

`KnowledgeService::new_default()` resolves the standard config path. For tests, expose a `new_with_root(root: PathBuf)` constructor if you need to override.

- [ ] **Step 3: Verify build**

```bash
cargo build -p biorouter-server 2>&1 | tail -3
```

If `biorouter-mcp` isn't already a Cargo dep of `biorouter-server`, add it. Knowledge types and the service are re-exported via `biorouter::knowledge::*` so you should be able to import everything through `biorouter`.

- [ ] **Step 4: Commit**

```bash
git add crates/biorouter-server/Cargo.toml crates/biorouter-server/src/state.rs
git commit -m "feat(server): wire KnowledgeService into AppState"
```

---

## Task 5: `routes/knowledge.rs` — scaffold + read-only routes

**Files:**
- Create: `crates/biorouter-server/src/routes/knowledge.rs`
- Modify: `crates/biorouter-server/src/routes/mod.rs` (mount the new router)

- [ ] **Step 1: Scaffold + 4 read routes**

`crates/biorouter-server/src/routes/knowledge.rs`:

```rust
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use biorouter::knowledge::types::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/bases", get(list_bases).post(create_base))
        .route("/bases/:id", get(get_base).delete(delete_base))
        .route("/bases/:id/graph", get(get_graph))
        .route("/bases/:id/pages", get(list_pages))
        .route("/bases/:id/pages/*page_path", get(read_page).put(write_page))
        .route("/bases/:id/history", get(list_history))
        .route("/bases/:id/preview", post(preview_state))
        .route("/bases/:id/restore", post(restore_state))
        .route("/bases/:id/raw", post(add_raw_source))
        .route("/bases/:id/ingest", post(ingest))
        .route("/bases/:id/query", post(query))
        .route("/bases/:id/lint", post(lint))
        .route("/bases/:id/export", get(export_brkb))
        .route("/bases/import", post(import_brkb))
        .route("/bases/:id/sources/:sid/reclassify", post(reclassify))
        .route("/bases/:id/sources/:sid/credibility", put(override_credibility))
        .with_state(state)
}

// ---- read-only handlers ----

#[utoipa::path(
    get, path = "/knowledge/bases",
    responses((status = 200, description = "List of bases", body = Vec<Manifest>))
)]
pub async fn list_bases(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Manifest>>, (StatusCode, String)> {
    let bases = state.knowledge_service.list_bases()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(bases))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateBaseBody { pub id: String, pub name: String, #[serde(default)] pub color: Option<String> }

#[utoipa::path(
    post, path = "/knowledge/bases",
    request_body = CreateBaseBody,
    responses((status = 200, description = "Created", body = Manifest))
)]
pub async fn create_base(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateBaseBody>,
) -> Result<Json<Manifest>, (StatusCode, String)> {
    let m = state.knowledge_service.create_base(&body.id, &body.name, body.color.as_deref())
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(m))
}

#[utoipa::path(
    get, path = "/knowledge/bases/{id}",
    params(("id" = String, Path)),
    responses((status = 200, description = "Manifest", body = Manifest))
)]
pub async fn get_base(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Manifest>, (StatusCode, String)> {
    let bases = state.knowledge_service.list_bases()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    bases.into_iter().find(|b| b.id == id)
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, format!("kb '{id}' not found")))
}

#[utoipa::path(
    delete, path = "/knowledge/bases/{id}",
    params(("id" = String, Path)),
    responses((status = 204, description = "Deleted"))
)]
pub async fn delete_base(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let kb_root = biorouter::knowledge::paths::kb_root(state.knowledge_service.root(), &id);
    if !kb_root.exists() { return Err((StatusCode::NOT_FOUND, format!("kb '{id}' not found"))); }
    std::fs::remove_dir_all(&kb_root).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    biorouter::knowledge::registry::unregister(state.knowledge_service.root(), &id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get, path = "/knowledge/bases/{id}/graph",
    params(("id" = String, Path)),
    responses((status = 200, description = "Graph", body = Graph))
)]
pub async fn get_graph(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Graph>, (StatusCode, String)> {
    let g = state.knowledge_service.get_graph(&id)
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
    Ok(Json(g))
}

// stubs for routes implemented in later tasks
async fn list_pages() -> &'static str { "todo: Task 6" }
async fn read_page() -> &'static str { "todo: Task 6" }
async fn write_page() -> &'static str { "todo: Task 6" }
async fn list_history() -> &'static str { "todo: Task 7" }
async fn preview_state() -> &'static str { "todo: Task 7" }
async fn restore_state() -> &'static str { "todo: Task 7" }
async fn add_raw_source() -> &'static str { "todo: Task 8" }
async fn ingest() -> &'static str { "todo: Task 9" }
async fn query() -> &'static str { "todo: Task 9" }
async fn lint() -> &'static str { "todo: Task 9" }
async fn export_brkb() -> &'static str { "todo: Task 10" }
async fn import_brkb() -> &'static str { "todo: Task 10" }
async fn reclassify() -> &'static str { "todo: Task 11" }
async fn override_credibility() -> &'static str { "todo: Task 11" }
```

- [ ] **Step 2: Mount in `routes/mod.rs`**

Find the existing `configure(...)` function and add:

```rust
.nest("/knowledge", crate::routes::knowledge::router(state.clone()))
```

inside the `.merge(...)` chain.

- [ ] **Step 3: Smoke test the four real routes**

Create `crates/biorouter-server/tests/knowledge_routes.rs`:

```rust
use axum::body::Body;
use axum::http::Request;
use tower::ServiceExt;

async fn build_test_app() -> axum::Router {
    // Construct AppState with a tempdir-backed KnowledgeService
    // (you may need a new `AppState::new_for_tests` helper that injects KnowledgeService).
    // Then build the router via crate::routes::knowledge::router(state).
    unimplemented!("implement based on existing test helpers; see other route tests for examples")
}

#[tokio::test]
async fn list_bases_returns_empty_initially() {
    let app = build_test_app().await;
    let res = app.oneshot(Request::builder().uri("/bases").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn create_then_get_base() {
    let app = build_test_app().await;
    let body = serde_json::to_vec(&serde_json::json!({"id":"t","name":"T"})).unwrap();
    let res = app.clone().oneshot(Request::builder()
        .method("POST").uri("/bases")
        .header("content-type", "application/json")
        .body(Body::from(body)).unwrap()).await.unwrap();
    assert_eq!(res.status(), 200);
    let res = app.oneshot(Request::builder().uri("/bases/t").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), 200);
}
```

If `AppState` construction in tests is awkward, look at the existing `crates/biorouter-server/tests/` directory for a helper pattern.

- [ ] **Step 4: Verify**

```bash
cargo test -p biorouter-server --test knowledge_routes 2>&1 | tail -5
```

- [ ] **Step 5: Commit**

```bash
git add crates/biorouter-server/src/routes crates/biorouter-server/tests/knowledge_routes.rs
git commit -m "feat(server): /knowledge router scaffold + 5 read-only routes (list/create/get/delete/graph)"
```

---

## Task 6: Page CRUD routes

**Files:**
- Modify: `crates/biorouter-server/src/routes/knowledge.rs`

Replace the three page stubs with real handlers using `biorouter::knowledge::store::{list_pages, read_page, write_page}`. The `*page_path` axum param captures the rest of the URL after `/pages/`.

- [ ] **Step 1: Implement**

```rust
#[derive(Deserialize)]
pub struct ListPagesQuery {
    #[serde(default)]
    pub path_prefix: Option<String>,
}

#[utoipa::path(
    get, path = "/knowledge/bases/{id}/pages",
    params(("id" = String, Path), ("path_prefix" = Option<String>, Query)),
    responses((status = 200, description = "Page list", body = Vec<biorouter::knowledge::store::PageRef>))
)]
pub async fn list_pages(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(q): Query<ListPagesQuery>,
) -> Result<Json<Vec<biorouter::knowledge::store::PageRef>>, (StatusCode, String)> {
    let kb_root = biorouter::knowledge::paths::kb_root(state.knowledge_service.root(), &id);
    let pages = biorouter::knowledge::store::list_pages(&kb_root, q.path_prefix.as_deref())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(pages))
}

#[utoipa::path(
    get, path = "/knowledge/bases/{id}/pages/{page_path}",
    params(("id" = String, Path), ("page_path" = String, Path)),
    responses((status = 200, description = "Page", body = biorouter::knowledge::store::PageContent))
)]
pub async fn read_page(
    State(state): State<Arc<AppState>>,
    Path((id, page_path)): Path<(String, String)>,
) -> Result<Json<biorouter::knowledge::store::PageContent>, (StatusCode, String)> {
    let kb_root = biorouter::knowledge::paths::kb_root(state.knowledge_service.root(), &id);
    let page = biorouter::knowledge::store::read_page(&kb_root, &page_path)
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
    Ok(Json(page))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct WritePageBody { pub content: String, pub commit_message: String }

#[utoipa::path(
    put, path = "/knowledge/bases/{id}/pages/{page_path}",
    request_body = WritePageBody,
    params(("id" = String, Path), ("page_path" = String, Path)),
    responses((status = 200, description = "Written", body = String))
)]
pub async fn write_page(
    State(state): State<Arc<AppState>>,
    Path((id, page_path)): Path<(String, String)>,
    Json(body): Json<WritePageBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let kb_root = biorouter::knowledge::paths::kb_root(state.knowledge_service.root(), &id);
    let sha = biorouter::knowledge::store::write_page(&kb_root, &page_path, &body.content, &body.commit_message, None)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(serde_json::json!({"commit_sha": sha})))
}
```

- [ ] **Step 2: Tests + commit**

Add 2-3 tests to `knowledge_routes.rs`: list_pages on empty KB returns empty; write_page then read_page round-trips; read_page on missing path returns 404.

```bash
cargo test -p biorouter-server --test knowledge_routes 2>&1 | tail -5
git add crates/biorouter-server/src
git commit -m "feat(server): page CRUD routes (list/read/write)"
```

---

## Task 7: History + restore routes

**Files:**
- Modify: `crates/biorouter-server/src/routes/knowledge.rs`

Three routes: `GET /bases/:id/history?limit=N`, `POST /bases/:id/preview {commit_sha, path}`, `POST /bases/:id/restore {commit_sha}`. Use `KnowledgeService::list_history`, `preview_state`, `restore_state` from Plan 1.

- [ ] **Step 1: Implement (sketch)**

```rust
#[derive(Deserialize)]
pub struct HistoryQuery { #[serde(default = "default_limit")] pub limit: usize }
fn default_limit() -> usize { 50 }

pub async fn list_history(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<Vec<HistoryEntry>>, (StatusCode, String)> {
    state.knowledge_service.list_history(&id, q.limit)
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PreviewBody { pub commit_sha: String, pub path: String }

pub async fn preview_state(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<PreviewBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let content = state.knowledge_service.preview_state(&id, &body.commit_sha, &body.path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({"content": content})))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct RestoreBody { pub commit_sha: String }

pub async fn restore_state(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<RestoreBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let sha = state.knowledge_service.restore_state(&id, &body.commit_sha)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({"new_commit_sha": sha})))
}
```

Add `#[utoipa::path(...)]` annotations matching the existing route style.

- [ ] **Step 2: Test + commit**

One test: create base, write a page, list history, restore to first commit, assert the page is gone.

```bash
git add crates/biorouter-server/src
git commit -m "feat(server): history + preview + restore routes"
```

---

## Task 8: Raw source POST route (multipart / url / text)

**Files:**
- Modify: `crates/biorouter-server/src/routes/knowledge.rs`
- Modify: `crates/biorouter-server/Cargo.toml` (add `axum = { ..., features = ["multipart"] }` if not present)

One route accepting three modes via content-type:
- `multipart/form-data` with a `file` field (and optional `filename` field)
- `application/json` with `{ "url": "..." }`
- `application/json` with `{ "text": "...", "title": "..." }`

- [ ] **Step 1: Implement**

Use axum's `Multipart` extractor:

```rust
use axum::extract::Multipart;

pub async fn add_raw_source(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: axum::http::HeaderMap,
    body: axum::body::Body,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let content_type = headers.get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let input = if content_type.starts_with("multipart/form-data") {
        // Parse multipart, find the "file" part, read bytes + filename
        let mut mp = Multipart::from_request(
            axum::http::Request::from_parts(axum::http::request::Parts::default_with_uri(/*…*/), body),
            &(),
        ).await.map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
        let mut bytes: Option<Vec<u8>> = None;
        let mut filename: Option<String> = None;
        while let Some(field) = mp.next_field().await
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))? {
            match field.name() {
                Some("file") => {
                    filename = field.file_name().map(|s| s.to_string());
                    bytes = Some(field.bytes().await
                        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?.to_vec());
                }
                _ => {}
            }
        }
        let bytes = bytes.ok_or((StatusCode::BAD_REQUEST, "missing 'file' part".into()))?;
        let filename = filename.unwrap_or_else(|| "upload.bin".into());
        biorouter::knowledge::convert::SourceInput::File { bytes, filename, mime: None }
    } else {
        // JSON body
        let body_bytes = axum::body::to_bytes(body, usize::MAX).await
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
        let json: serde_json::Value = serde_json::from_slice(&body_bytes)
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
        if let Some(url) = json.get("url").and_then(|v| v.as_str()) {
            biorouter::knowledge::convert::SourceInput::Url(url.to_string())
        } else if let Some(text) = json.get("text").and_then(|v| v.as_str()) {
            let title = json.get("title").and_then(|v| v.as_str()).map(|s| s.to_string());
            biorouter::knowledge::convert::SourceInput::Text { text: text.to_string(), title }
        } else {
            return Err((StatusCode::BAD_REQUEST, "expected file (multipart), {url}, or {text}".into()));
        }
    };

    let res = state.knowledge_service.add_raw_source(&id, input, None).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({
        "source_id": res.source_id,
        "source_md_path": res.source_md_path,
    })))
}
```

The exact axum `Multipart::from_request` API may differ — inspect `axum`'s version pinned in the workspace and adapt. Goal is: parse multipart → extract one `file` part with bytes + filename. If awkward, use `axum::extract::Multipart` as a separate handler argument and let axum route based on content type via `axum_extra::extract::Multipart` or two separate routes.

- [ ] **Step 2: Test + commit**

Two tests: post `{"text":"hello"}` → 200 with source_id; post `{"url":"http://wiremock/..."}` against a wiremock returning a small HTML → 200.

```bash
git add crates/biorouter-server/Cargo.toml crates/biorouter-server/src
git commit -m "feat(server): POST /knowledge/bases/:id/raw (file/url/text)"
```

---

## Task 9: Macro SSE routes (ingest / query / lint)

**Files:**
- Modify: `crates/biorouter-server/src/routes/knowledge.rs`

This is the biggest task. For each macro: parse request body → resolve Provider via `biorouter::providers::factory::create` → wrap in `ProviderCompleter` → spawn the macro on a tokio task → forward `SubAgentEvent`s to an SSE response stream.

- [ ] **Step 1: SSE helper**

Reuse the existing `SseResponse` pattern from [`reply.rs`](crates/biorouter-server/src/routes/reply.rs). Build a small helper:

```rust
use tokio::sync::mpsc::UnboundedSender;
use biorouter_mcp::knowledge::subagent::events::SubAgentEvent;

fn sse_event(event: &SubAgentEvent) -> String {
    // Serialize as: data: {json}\n\n
    format!("data: {}\n\n", serde_json::to_string(event).unwrap_or_else(|_| "{}".into()))
}

fn sse_done(result: &serde_json::Value) -> String {
    format!("event: done\ndata: {}\n\n", result)
}

fn sse_error(message: &str) -> String {
    format!("event: error\ndata: {{\"message\":\"{message}\"}}\n\n")
}
```

Place this near the top of `knowledge.rs`.

- [ ] **Step 2: Implement the three macro handlers**

Sketch for `ingest`:

```rust
#[derive(Deserialize, utoipa::ToSchema)]
pub struct IngestBody {
    pub source: serde_json::Value,    // {url:...} or {text:..., title:...}
    pub model: biorouter::knowledge::types::ModelRef,
    #[serde(default)]
    pub focus: Option<String>,
}

pub async fn ingest(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<IngestBody>,
) -> Result<crate::routes::reply::SseResponse, (StatusCode, String)> {
    // Resolve source.
    let source = parse_source_input(&body.source)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    // Build the Completer.
    let provider = biorouter::providers::factory::create(&body.model.provider,
        biorouter::providers::base::ModelConfig::default_with(body.model.model.clone()))
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let completer = Box::new(biorouter::knowledge::ProviderCompleter::new(provider));

    // Build the SSE channel.
    let (tx, rx) = tokio::sync::mpsc::channel::<String>(64);   // string-typed for SseResponse
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<SubAgentEvent>();

    // Forwarder: SubAgentEvent → SSE string.
    let tx_for_forward = tx.clone();
    tokio::spawn(async move {
        while let Some(ev) = event_rx.recv().await {
            let _ = tx_for_forward.send(sse_event(&ev)).await;
        }
    });

    // Macro runner.
    let svc = state.knowledge_service.clone();
    let kb_id = id.clone();
    tokio::spawn(async move {
        let args = biorouter_mcp::knowledge::macros::ingest::IngestArgs {
            kb_id,
            source,
            completer,
            focus: body.focus,
            bounds: biorouter_mcp::knowledge::subagent::loop_::SubAgentBounds::default(),
            event_sink: Some(event_tx),
        };
        match biorouter_mcp::knowledge::macros::ingest::ingest(&svc, args).await {
            Ok(result) => {
                let json = serde_json::to_value(&result).unwrap_or(serde_json::json!({}));
                let _ = tx.send(sse_done(&json)).await;
            }
            Err(e) => { let _ = tx.send(sse_error(&e.to_string())).await; }
        }
    });

    // Build the SSE response from rx.
    Ok(crate::routes::reply::SseResponse::from_rx(rx))
}

fn parse_source_input(v: &serde_json::Value) -> anyhow::Result<biorouter::knowledge::convert::SourceInput> {
    if let Some(url) = v.get("url").and_then(|x| x.as_str()) {
        Ok(biorouter::knowledge::convert::SourceInput::Url(url.to_string()))
    } else if let Some(text) = v.get("text").and_then(|x| x.as_str()) {
        Ok(biorouter::knowledge::convert::SourceInput::Text {
            text: text.to_string(),
            title: v.get("title").and_then(|x| x.as_str()).map(|s| s.to_string()),
        })
    } else {
        anyhow::bail!("source must have 'url' or 'text'")
    }
}
```

You'll need to expose `SseResponse::from_rx` (or similar constructor) in `reply.rs` — read its existing constructor, may already be `pub` or trivially adaptable. If not, add a `pub fn from_rx(rx: mpsc::Receiver<String>) -> Self`.

`query` and `lint` follow the same pattern. `query` takes `QueryBody { question, model, file_as_page? }`. `lint` takes `LintBody { model, autofix }`.

- [ ] **Step 3: Tests**

For each route, a smoke test using a fake-Provider injected via a test-only `AppState::new_with_provider_factory` (you may need to introduce one). Acceptable simplified test: hit the endpoint with an invalid model name → expect 400. Real-LLM tests aren't appropriate.

If a full SSE-stream test is hard to write, just verify the route returns 200 + `text/event-stream` content type for a valid request. The Plan-2 macro logic is already unit-tested.

- [ ] **Step 4: Commit**

```bash
git add crates/biorouter-server/src
git commit -m "feat(server): SSE-streamed macro routes (ingest/query/lint)"
```

---

## Task 10: Export + import routes

**Files:**
- Modify: `crates/biorouter-server/src/routes/knowledge.rs`

- [ ] **Step 1: Implement**

```rust
pub async fn export_brkb(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<axum::response::Response, (StatusCode, String)> {
    let bytes = state.knowledge_service.export_brkb(&id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let filename = format!("{id}.brkb");
    Ok(axum::http::Response::builder()
        .header("Content-Type", "application/octet-stream")
        .header("Content-Disposition", format!("attachment; filename=\"{filename}\""))
        .body(axum::body::Body::from(bytes))
        .unwrap())
}

pub async fn import_brkb(
    State(state): State<Arc<AppState>>,
    mut mp: axum::extract::Multipart,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let mut bytes: Option<Vec<u8>> = None;
    while let Some(field) = mp.next_field().await.map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))? {
        if field.name() == Some("file") {
            bytes = Some(field.bytes().await
                .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?.to_vec());
        }
    }
    let bytes = bytes.ok_or((StatusCode::BAD_REQUEST, "missing 'file' part".into()))?;
    let new_id = state.knowledge_service.import_brkb(&bytes)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({"id": new_id})))
}
```

- [ ] **Step 2: Test + commit**

One test: create a KB, GET `/bases/:id/export`, POST the bytes back to `/bases/import`, assert a new KB exists with a suffixed id.

```bash
git add crates/biorouter-server/src
git commit -m "feat(server): .brkb export GET + import POST (multipart)"
```

---

## Task 11: Reclassify + override-credibility routes

**Files:**
- Modify: `crates/biorouter-server/src/routes/knowledge.rs`

Two routes:
- `POST /bases/:id/sources/:sid/reclassify` — re-run `credibility::classify(input, None)` for an existing raw source and update `meta.yaml`. Wraps a new small helper on `KnowledgeService`.
- `PUT /bases/:id/sources/:sid/credibility` — manual override; takes a JSON `Credibility` body and writes it directly to `meta.yaml`. Commits.

- [ ] **Step 1: Helpers on service**

Add to `KnowledgeService` in `service.rs`:

```rust
pub async fn reclassify_source(&self, kb_id: &str, source_id: &str) -> Result<Credibility> {
    let kb_root = paths::kb_root(&self.root, kb_id);
    let mut meta = raw::read_meta(&kb_root, source_id)?;
    // Rebuild a SourceInput from the meta. For URL-based sources we have the url; for File/Text
    // we don't store the raw bytes after ingest, so reclassification falls back to URL-only or
    // a text excerpt from source.md.
    let input = if let Some(url) = meta.url.clone() {
        crate::knowledge::convert::SourceInput::Url(url)
    } else {
        let body = std::fs::read_to_string(kb_root.join("raw").join(source_id).join("source.md"))?;
        crate::knowledge::convert::SourceInput::Text { text: body, title: Some(meta.title.clone()) }
    };
    let new_cred = crate::knowledge::credibility::classify(&input, None).await?;
    meta.credibility = new_cred.clone();
    let yaml = serde_yaml::to_string(&meta)?;
    std::fs::write(kb_root.join("raw").join(source_id).join("meta.yaml"), yaml)?;
    let repo = crate::knowledge::git::GitRepo::open(&kb_root)?;
    repo.commit_all(crate::knowledge::types::ChangeKind::Manual,
        &format!("reclassify {source_id}"), None)?;
    self.rebuild_graph_cache(kb_id)?;
    Ok(new_cred)
}

pub fn override_credibility(&self, kb_id: &str, source_id: &str, cred: Credibility) -> Result<()> {
    let kb_root = paths::kb_root(&self.root, kb_id);
    let mut meta = raw::read_meta(&kb_root, source_id)?;
    meta.credibility = cred;
    let yaml = serde_yaml::to_string(&meta)?;
    std::fs::write(kb_root.join("raw").join(source_id).join("meta.yaml"), yaml)?;
    let repo = crate::knowledge::git::GitRepo::open(&kb_root)?;
    repo.commit_all(crate::knowledge::types::ChangeKind::Manual,
        &format!("override credibility for {source_id}"), None)?;
    self.rebuild_graph_cache(kb_id)?;
    Ok(())
}
```

- [ ] **Step 2: Routes**

Straightforward axum handlers calling the above. Add `#[utoipa::path(...)]`.

- [ ] **Step 3: Tests + commit**

Two service-level tests on `reclassify_source` and `override_credibility`. Two route smoke tests.

```bash
git add crates/biorouter-server/src crates/biorouter-mcp/src/knowledge
git commit -m "feat(knowledge): reclassify_source + override_credibility (service + routes)"
```

---

## Task 12: OpenAPI registration + TypeScript client regen

**Files:**
- Modify: `crates/biorouter-server/src/openapi.rs`

- [ ] **Step 1: Register handlers**

In the `OpenApi` derive struct's `paths(...)` list, add every new knowledge handler. Roughly:

```rust
#[derive(OpenApi)]
#[openapi(
    paths(
        // ... existing paths ...
        crate::routes::knowledge::list_bases,
        crate::routes::knowledge::create_base,
        crate::routes::knowledge::get_base,
        crate::routes::knowledge::delete_base,
        crate::routes::knowledge::get_graph,
        crate::routes::knowledge::list_pages,
        crate::routes::knowledge::read_page,
        crate::routes::knowledge::write_page,
        crate::routes::knowledge::list_history,
        crate::routes::knowledge::preview_state,
        crate::routes::knowledge::restore_state,
        crate::routes::knowledge::add_raw_source,
        crate::routes::knowledge::ingest,
        crate::routes::knowledge::query,
        crate::routes::knowledge::lint,
        crate::routes::knowledge::export_brkb,
        crate::routes::knowledge::import_brkb,
        crate::routes::knowledge::reclassify,
        crate::routes::knowledge::override_credibility,
    ),
    components(schemas(
        biorouter::knowledge::types::Manifest,
        biorouter::knowledge::types::Graph,
        biorouter::knowledge::types::GraphNode,
        biorouter::knowledge::types::GraphEdge,
        biorouter::knowledge::types::HistoryEntry,
        biorouter::knowledge::types::ChangeKind,
        biorouter::knowledge::types::Credibility,
        biorouter::knowledge::types::CredibilityTier,
        crate::routes::knowledge::CreateBaseBody,
        crate::routes::knowledge::WritePageBody,
        crate::routes::knowledge::PreviewBody,
        crate::routes::knowledge::RestoreBody,
        crate::routes::knowledge::IngestBody,
        // ... etc ...
    ))
)]
pub struct ApiDoc;
```

Every body / response struct used in `#[utoipa::path]` blocks must derive `utoipa::ToSchema`. Add the derive where missing. For `biorouter`-owned types that don't yet derive `ToSchema`, add it to those types (they already derive `Serialize, Deserialize, JsonSchema` — `ToSchema` is another derive macro from utoipa, added similarly).

- [ ] **Step 2: Regenerate the TypeScript client**

```bash
just generate-openapi
```

This regenerates `ui/desktop/src/api/` (per CLAUDE.md). Confirm the new methods appear:

```bash
grep -E 'listBases|createBase|getGraph|ingestSource|queryKb' ui/desktop/src/api/sdk.gen.ts 2>/dev/null | head
```

- [ ] **Step 3: Commit both**

```bash
git add crates/biorouter-server/src/openapi.rs crates/biorouter/src/knowledge crates/biorouter-mcp/src/knowledge ui/desktop/src/api ui/desktop/openapi.json
git commit -m "feat(server): OpenAPI registration for /knowledge routes + regenerated TS client"
```

---

## Task 13: End-to-end integration test

**Files:**
- Create or extend: `crates/biorouter-server/tests/knowledge_routes_e2e.rs`

One test that drives the router through a realistic flow:
1. POST `/knowledge/bases` to create a KB.
2. POST `/knowledge/bases/:id/raw` with `{text}` to ingest a source.
3. GET `/knowledge/bases/:id/history` and assert 2 entries.
4. GET `/knowledge/bases/:id/graph` and assert it returns a Graph (possibly empty nodes).
5. GET `/knowledge/bases/:id/export` and assert ≥ 100 bytes returned.
6. POST `/knowledge/bases/import` with those bytes (multipart) and assert a new id is returned.

Skip the macro routes here — they need a Provider; covered by smoke tests in Task 9.

- [ ] **Verify + commit**

```bash
cargo test -p biorouter-server --test knowledge_routes_e2e 2>&1 | tail -5
git add crates/biorouter-server/tests
git commit -m "test(server): /knowledge e2e (create/raw/history/graph/export/import)"
```

---

## Task 14: CLAUDE.md update + final verification

- [ ] **Step 1: CLAUDE.md edit**

In the Core Agent Library section, append to the knowledge bullet: "…and HTTP routes under `/knowledge/*` via `biorouter-server` (with SSE-streamed macros and `.brkb` export/import)."

- [ ] **Step 2: Verification**

```bash
cd /Users/wgu/Desktop/BioRouter-knowledge
source bin/activate-hermit
cargo fmt -p biorouter -p biorouter-mcp -p biorouter-server -- --check 2>&1 | grep -i 'knowledge' | head -10
cargo clippy -p biorouter -p biorouter-mcp -p biorouter-server --no-deps -- -D warnings 2>&1 | tail -20
cargo test -p biorouter --lib knowledge 2>&1 | tail -3
cargo test -p biorouter-mcp --lib knowledge 2>&1 | tail -3
cargo test -p biorouter --test knowledge_e2e 2>&1 | tail -3
cargo test -p biorouter-mcp --test knowledge_macros_e2e 2>&1 | tail -3
cargo test -p biorouter-server --test knowledge_routes 2>&1 | tail -3
cargo test -p biorouter-server --test knowledge_routes_e2e 2>&1 | tail -3
```

All clean.

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "docs(claude): document Plan 3 HTTP routes + .brkb"
```

---

## Risks worth flagging up front

- **`provider_completer.rs` Message conversion** is the highest-risk wiring. Batch B already mapped the rmcp `CallToolRequestParams.name/arguments` ↔ `LlmToolCall` layout; Task 1 needs to inspect the actual biorouter `Message`/`MessageContent` types to fill in the two `todo!()`s. Allocate buffer time.
- **`axum::extract::Multipart` API** varies across axum minor versions. Inspect the workspace's axum version (`grep '^axum' crates/biorouter-server/Cargo.toml`) and adapt.
- **SSE stream lifecycle**: the macro task drops the event sender when it finishes; the forwarder task's `event_rx.recv()` then returns `None` and the forwarder exits, which drops `tx`, which closes the SSE stream. Make sure not to hold `tx` clones longer than needed.
- **TypeScript client regen** can produce a large `ui/desktop/src/api/*.gen.ts` diff. That's expected; CLAUDE.md says never hand-edit those files. Just commit the generated diff.
- **`utoipa::ToSchema` derives** propagate through every type used in route signatures. If you discover a type that doesn't have it (e.g., a deeply nested option), you may need to add `ToSchema` to types in `biorouter::knowledge::types`. This is a small chore but multiplies across types — budget for it.
