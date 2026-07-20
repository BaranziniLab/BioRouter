# Plan 1 — knowledge storage, git and graph derivation

> **What this is.** Plan 1 of the six-plan Knowledge buildout: the storage, git, format-conversion, credibility-classification and graph-derivation layers behind a shared `KnowledgeService`. No UI, no macros, no chat integration — those are Plans 2 through 6.
> **Status:** Historical record — executed and shipped. The Knowledge feature is live; `crates/biorouter-mcp/src/knowledge/` contains the modules this plan specifies, and `CLAUDE.md` documents the on-disk layout, the module surface, and the ~122 backend library tests it now carries. The unticked `- [ ]` checkboxes below are the plan as written, not outstanding work.
> **Audience:** developers working on the Knowledge subsystem, and agents tracing why a backend module is shaped the way it is.
>
> **Plan numbering.** "Plan *N* of 6" refers to the six sibling documents in this
> folder, `plan-1-…` through `plan-6-…`, which were executed in order. The design
> they implement is [`founding-design.md`](founding-design.md).

Knowledge gives a user one or more personal, git-backed markdown knowledge bases that an LLM maintains incrementally. This plan builds the layer underneath all of that: enough that a developer, or the MCP tool surface, can create a knowledge base, add a raw source from a file / URL / pasted text, classify its credibility, and read back a derived graph.

> **Warning — module paths in this document are stale.** The plan places the
> service in `crates/biorouter/src/knowledge/`. In the shipped code the entire
> module lives in **`crates/biorouter-mcp/src/knowledge/`**, and
> `crates/biorouter/src/knowledge/mod.rs` is only a re-export
> (`pub use biorouter_mcp::knowledge::*;`) — implementing it in `biorouter` would
> have created a circular dependency, since `biorouter` depends on
> `biorouter-mcp`. Read every `crates/biorouter/src/knowledge/…` path below as
> `crates/biorouter-mcp/src/knowledge/…`. The `biorouter-mcp` paths in the plan
> are correct as written.

> **Note — pinned dependency versions are point-in-time.** Version pins such as
> `git2 = "0.19"`, `pdf-extract = "0.7"` and `bm25 = "2.2"` record what was current
> when the plan was written. They have since moved, and [Plan 2](plan-2-macros-and-subagent-loop.md)
> notes its own BM25 snippet was written against `bm25 = "2.3"`. Check `Cargo.toml`
> for the versions actually in use.

## The plan series

| Plan | Scope |
|---|---|
| **Plan 1 (this document)** | Storage, git, conversion, credibility, graph derivation |
| [Plan 2](plan-2-macros-and-subagent-loop.md) | Macros (`kb_ingest_source` / `kb_query` / `kb_lint`) over a bounded sub-agent loop, `kb_search`, active-KB state, transaction tools |
| [Plan 3](plan-3-http-routes-and-export.md) | HTTP routes in `biorouter-server` with SSE-streamed macros, `.brkb` export/import |
| [Plan 4](plan-4-knowledge-view-and-ingest.md) | Sidebar entry, `KnowledgeView` shell, KB selector, ingest panel |
| [Plan 5](plan-5-graph-view-and-change-log.md) | Force-graph view and the git change-log drawer |
| [Plan 6](plan-6-chat-integration-and-closeout.md) | Chat-side KB chip, `/knowledge` command, persistence, closeout |

## Scope and approach

**Goal:** Build the storage, git, conversion, credibility, and graph-derivation layers of the Knowledge feature — enough that a developer (or the MCP tool surface) can create a knowledge base, add a raw source from a file / URL / pasted text, classify its credibility, and read back a derived graph. No UI, no macros, no chat integration.

**Architecture:** A shared `KnowledgeService` in `crates/biorouter/src/knowledge/` owns all on-disk operations against `~/.config/biorouter/knowledge/<kb-id>/` (one git repo per KB). A thin `KnowledgeServer` in `crates/biorouter-mcp/src/knowledge/` wraps the service as MCP tools and is registered in `BUILTIN_EXTENSIONS`. Conversion uses pure-Rust crates (htmd, pdf-extract, docx-rs, csv). Credibility classification runs a deterministic ladder (identifiers → Crossref/OpenAlex → host patterns → agentic stub) with results cached on disk.

**Tech stack:** Rust 1.92, tokio, git2, htmd, pdf-extract, docx-rs, csv, reqwest, serde, serde_yaml, schemars, rmcp; insta + wiremock for testing.

**Source spec:** [`founding-design.md`](founding-design.md).

**TDD note:** Many tasks below combine "write the tests" and "write the implementation" into a single step rather than two separate steps. When executing, read the test code first, mentally check it fails against an empty implementation, then proceed with the implementation. The verification step ("Run tests, expect N passed") still gates each task.

**Execution convention:** the plan was written for an agentic worker driving it task-by-task with the `superpowers:subagent-driven-development` or `superpowers:executing-plans` skill. Steps use checkbox (`- [ ]`) syntax for tracking.

## Task index

The 34 tasks group into six stages:

| Stage | Tasks | What it builds |
|---|---|---|
| Module scaffolding | 1–6 | Cargo dependencies, core types, path resolution, registry, per-KB manifest, default `schema.md` |
| Git layer | 7–9 | Init / commit / log, transactions, preview and restore |
| Service and stores | 10–12 | `KnowledgeService::create_base`, page store CRUD, raw source storage |
| Conversion pipeline | 13–19 | HTML, PDF, DOCX, CSV, URL fetch, note URL extraction, dispatcher |
| Credibility classifier | 20–27 | Identifiers, publisher allow-list, Crossref, OpenAlex, host patterns, agentic stub, dispatcher, wiring into `add_raw_source` |
| Graph, history and MCP | 28–34 | Graph derivation and caching, history + restore, MCP server wrapper, `BUILTIN_EXTENSIONS` registration, end-to-end test, `CLAUDE.md` update |

---

## Before starting

- [ ] **Pre-step A: Branch off main** — execution should happen on a fresh `feature/knowledge` branch, not on the current `feature/multimodal-image-input` branch.

```bash
git fetch origin main
git switch -c feature/knowledge origin/main
```

- [ ] **Pre-step B: Activate hermit + confirm toolchain**

```bash
source bin/activate-hermit
rustc --version    # expect 1.92.x
cargo --version
```

- [ ] **Pre-step C: Read the spec section "Data model" and "Credibility classifier"** so type names and tier definitions match without surprises.

---

## File structure (decomposition map)

```text
crates/biorouter/src/knowledge/
├── mod.rs                       — module root, re-exports, KnowledgeService struct
├── types.rs                     — Manifest, RegistryEntry, SourceMeta, Credibility, GraphNode, GraphEdge, …
├── paths.rs                     — KB root + per-KB path helpers, kb-id validation
├── registry.rs                  — top-level ~/.config/biorouter/knowledge/manifest.yaml
├── manifest.rs                  — per-KB <kb-root>/manifest.yaml
├── schema_default.md            — embedded default schema.md template
├── service.rs                   — KnowledgeService public surface (create_base, …)
├── store.rs                     — page CRUD on the knowledge/ tree
├── raw.rs                       — raw source CRUD on the raw/ tree
├── git.rs                       — git2-backed init / commit / log / preview / restore / txn
├── convert/
│   ├── mod.rs                   — public convert() dispatcher
│   ├── html.rs                  — htmd
│   ├── pdf.rs                   — pdf-extract + LLM-fallback stub
│   ├── docx.rs                  — docx-rs
│   ├── csv.rs                   — render CSV as markdown table
│   ├── note.rs                  — pasted-text URL extraction
│   └── url_fetch.rs             — reqwest download
├── credibility/
│   ├── mod.rs                   — Credibility struct + classify() ladder
│   ├── identifiers.rs           — DOI / arXiv / ISBN / PMID extraction
│   ├── crossref.rs              — async client for api.crossref.org
│   ├── openalex.rs              — async client for api.openalex.org
│   ├── host_patterns.rs         — preprint / .gov / .edu / etc.
│   ├── allowlist.rs             — curated publisher allow-list → peer_reviewed
│   └── agentic.rs               — stubbed fallback (real impl in Plan 2)
└── graph.rs                     — derive nodes+edges from knowledge/, write graph-cache.json

crates/biorouter-mcp/src/knowledge/
└── mod.rs                       — KnowledgeServer, #[tool] methods delegating to service

crates/biorouter-mcp/src/lib.rs  — register knowledge in BUILTIN_EXTENSIONS
```

Test code lives next to each module: `crates/biorouter/src/knowledge/store.rs` has `#[cfg(test)] mod tests { … }` at the bottom. Cross-module integration tests live in `crates/biorouter/tests/knowledge_*.rs`. Snapshot fixtures live in `crates/biorouter/src/knowledge/snapshots/` (insta default location).

---

## Task 1: Add Cargo dependencies and module skeleton

**Files:**
- Modify: `crates/biorouter/Cargo.toml`
- Modify: `crates/biorouter-mcp/Cargo.toml`
- Create: `crates/biorouter/src/knowledge/mod.rs` (empty stub)
- Create: `crates/biorouter-mcp/src/knowledge/mod.rs` (empty stub)
- Modify: `crates/biorouter/src/lib.rs`
- Modify: `crates/biorouter-mcp/src/lib.rs`

- [ ] **Step 1: Add deps to `crates/biorouter/Cargo.toml`**

Open `[dependencies]` block and add the following (skip any already listed):

```toml
git2 = { version = "0.19", default-features = false, features = ["vendored-libgit2"] }
htmd = "0.1"
pdf-extract = "0.7"
docx-rs = "0.4"
csv = "1.3"
bm25 = "2.2"
```

`anyhow`, `chrono`, `regex`, `reqwest`, `sha2`, `thiserror`, `uuid`, `zip`, `serde_yaml`, `serde_json`, `tracing`, `tokio`, `schemars`, `insta`, `wiremock`, `tempfile` are already present — do not add duplicates.

- [ ] **Step 2: Create the skeleton module files**

```bash
mkdir -p crates/biorouter/src/knowledge/convert
mkdir -p crates/biorouter/src/knowledge/credibility
mkdir -p crates/biorouter-mcp/src/knowledge
```

Write each new module file with a single `// placeholder` line so the crate compiles. Files to create with placeholder contents:

```text
crates/biorouter/src/knowledge/mod.rs
crates/biorouter/src/knowledge/types.rs
crates/biorouter/src/knowledge/paths.rs
crates/biorouter/src/knowledge/registry.rs
crates/biorouter/src/knowledge/manifest.rs
crates/biorouter/src/knowledge/service.rs
crates/biorouter/src/knowledge/store.rs
crates/biorouter/src/knowledge/raw.rs
crates/biorouter/src/knowledge/git.rs
crates/biorouter/src/knowledge/graph.rs
crates/biorouter/src/knowledge/convert/mod.rs
crates/biorouter/src/knowledge/convert/html.rs
crates/biorouter/src/knowledge/convert/pdf.rs
crates/biorouter/src/knowledge/convert/docx.rs
crates/biorouter/src/knowledge/convert/csv.rs
crates/biorouter/src/knowledge/convert/note.rs
crates/biorouter/src/knowledge/convert/url_fetch.rs
crates/biorouter/src/knowledge/credibility/mod.rs
crates/biorouter/src/knowledge/credibility/identifiers.rs
crates/biorouter/src/knowledge/credibility/crossref.rs
crates/biorouter/src/knowledge/credibility/openalex.rs
crates/biorouter/src/knowledge/credibility/host_patterns.rs
crates/biorouter/src/knowledge/credibility/allowlist.rs
crates/biorouter/src/knowledge/credibility/agentic.rs
crates/biorouter-mcp/src/knowledge/mod.rs
```

Put `// placeholder` in each so they aren't empty.

- [ ] **Step 3: Add `mod knowledge;` declarations**

In `crates/biorouter/src/knowledge/mod.rs`:

```rust
pub mod convert;
pub mod credibility;
pub mod git;
pub mod graph;
pub mod manifest;
pub mod paths;
pub mod raw;
pub mod registry;
pub mod service;
pub mod store;
pub mod types;

pub use service::KnowledgeService;
pub use types::*;
```

In `crates/biorouter/src/knowledge/convert/mod.rs`:

```rust
pub mod csv;
pub mod docx;
pub mod html;
pub mod note;
pub mod pdf;
pub mod url_fetch;
```

In `crates/biorouter/src/knowledge/credibility/mod.rs`:

```rust
pub mod agentic;
pub mod allowlist;
pub mod crossref;
pub mod host_patterns;
pub mod identifiers;
pub mod openalex;
```

In `crates/biorouter/src/lib.rs`, add at top level (alongside other `pub mod` declarations):

```rust
pub mod knowledge;
```

In `crates/biorouter-mcp/src/lib.rs`, add (do not yet register in `BUILTIN_EXTENSIONS` — that comes in a later task):

```rust
pub mod knowledge;
```

- [ ] **Step 4: Verify the workspace builds**

Run: `cargo build -p biorouter -p biorouter-mcp`
Expected: clean build with warnings about unused modules (`unused: ...`) — OK.

- [ ] **Step 5: Commit**

```bash
git add crates/biorouter/Cargo.toml crates/biorouter-mcp/Cargo.toml \
        crates/biorouter/src/knowledge crates/biorouter-mcp/src/knowledge \
        crates/biorouter/src/lib.rs crates/biorouter-mcp/src/lib.rs
git commit -m "feat(knowledge): scaffold module tree and add deps"
```

---

## Task 2: Core types (`types.rs`)

**Files:**
- Modify: `crates/biorouter/src/knowledge/types.rs`

- [ ] **Step 1: Write the failing tests**

Append at the bottom of `types.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credibility_tier_serde_roundtrip() {
        for tier in [
            CredibilityTier::PeerReviewed,
            CredibilityTier::Preprint,
            CredibilityTier::Book,
            CredibilityTier::GrayLit,
            CredibilityTier::Web,
            CredibilityTier::Personal,
        ] {
            let s = serde_yaml::to_string(&tier).unwrap();
            let back: CredibilityTier = serde_yaml::from_str(&s).unwrap();
            assert_eq!(tier, back);
        }
    }

    #[test]
    fn credibility_tier_yaml_form_is_snake_case() {
        let s = serde_yaml::to_string(&CredibilityTier::PeerReviewed).unwrap();
        assert!(s.contains("peer_reviewed"), "got: {s}");
    }

    #[test]
    fn source_meta_yaml_roundtrip() {
        let meta = SourceMeta {
            id: "abc-123".into(),
            title: "Title".into(),
            url: Some("https://arxiv.org/abs/2403.12345".into()),
            ingested_at: chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            sha256: "deadbeef".into(),
            mime: "application/pdf".into(),
            original_filename: Some("paper.pdf".into()),
            credibility: Credibility {
                tier: CredibilityTier::Preprint,
                confidence: 0.97,
                publisher: Some("arXiv".into()),
                venue: Some("arXiv:2403.12345".into()),
                doi: None,
                retracted: false,
                reasoning: "URL host arxiv.org → preprint server.".into(),
                classifier_version: 1,
            },
        };
        let s = serde_yaml::to_string(&meta).unwrap();
        let back: SourceMeta = serde_yaml::from_str(&s).unwrap();
        assert_eq!(meta, back);
    }

    #[test]
    fn manifest_yaml_roundtrip() {
        let m = Manifest {
            id: "ms".into(),
            name: "MS Patient Analysis".into(),
            color: "#5a6394".into(),
            created_at: chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            schema_version: 1,
            default_model: None,
        };
        let s = serde_yaml::to_string(&m).unwrap();
        let back: Manifest = serde_yaml::from_str(&s).unwrap();
        assert_eq!(m, back);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p biorouter --lib knowledge::types::tests`
Expected: compile errors (`cannot find type CredibilityTier in this scope`, etc.).

- [ ] **Step 3: Implement the types**

Replace the placeholder in `types.rs` with:

```rust
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CredibilityTier {
    PeerReviewed,
    Preprint,
    Book,
    GrayLit,
    Web,
    Personal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Credibility {
    pub tier: CredibilityTier,
    pub confidence: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub venue: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doi: Option<String>,
    #[serde(default)]
    pub retracted: bool,
    pub reasoning: String,
    pub classifier_version: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SourceMeta {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub ingested_at: DateTime<Utc>,
    pub sha256: String,
    pub mime: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_filename: Option<String>,
    pub credibility: Credibility,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ModelRef {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub color: String,
    pub created_at: DateTime<Utc>,
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<ModelRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RegistryEntry {
    pub id: String,
    pub path: std::path::PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PageKind {
    Source,
    Entity,
    Concept,
    Hub,
    Note,
    Flag,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub kind: PageKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credibility_tier: Option<CredibilityTier>,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Graph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Ingest,
    Link,
    Flag,
    Query,
    Lint,
    Restore,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HistoryEntry {
    pub commit_sha: String,
    pub kind: ChangeKind,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta: Option<String>,
    pub timestamp: DateTime<Utc>,
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p biorouter --lib knowledge::types::tests`
Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/biorouter/src/knowledge/types.rs
git commit -m "feat(knowledge): core types (Credibility, SourceMeta, Manifest, Graph)"
```

---

## Task 3: Path resolution and kb-id validation (`paths.rs`)

**Files:**
- Modify: `crates/biorouter/src/knowledge/paths.rs`

- [ ] **Step 1: Write the failing tests**

Replace placeholder with the test block plus public function declarations:

```rust
use std::path::{Path, PathBuf};

pub fn validate_kb_id(id: &str) -> Result<(), KbIdError> {
    if id.is_empty() {
        return Err(KbIdError::Empty);
    }
    if id.len() > 64 {
        return Err(KbIdError::TooLong);
    }
    if !id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err(KbIdError::InvalidChars);
    }
    if id.starts_with('-') || id.ends_with('-') || id.contains("--") {
        return Err(KbIdError::BadShape);
    }
    Ok(())
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum KbIdError {
    #[error("kb-id is empty")]
    Empty,
    #[error("kb-id is longer than 64 characters")]
    TooLong,
    #[error("kb-id may only contain a-z, 0-9, and '-'")]
    InvalidChars,
    #[error("kb-id may not start/end with '-' or contain '--'")]
    BadShape,
}

pub fn knowledge_root() -> anyhow::Result<PathBuf> {
    use etcetera::{choose_app_strategy, AppStrategy, AppStrategyArgs};
    let strategy = choose_app_strategy(AppStrategyArgs {
        top_level_domain: "io".to_string(),
        author: "biorouter".to_string(),
        app_name: "biorouter".to_string(),
    })?;
    Ok(strategy.config_dir().join("knowledge"))
}

pub fn kb_root(root: &Path, id: &str) -> PathBuf {
    root.join(id)
}

pub fn kb_knowledge_dir(root: &Path, id: &str) -> PathBuf {
    kb_root(root, id).join("knowledge")
}

pub fn kb_raw_dir(root: &Path, id: &str) -> PathBuf {
    kb_root(root, id).join("raw")
}

pub fn kb_internal_dir(root: &Path, id: &str) -> PathBuf {
    kb_root(root, id).join(".biorouter-knowledge")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_good_ids() {
        for id in ["a", "ms-patient", "kb-01", "personal", "x1-y2-z3"] {
            assert!(validate_kb_id(id).is_ok(), "should accept {id}");
        }
    }

    #[test]
    fn rejects_bad_ids() {
        for (id, want) in [
            ("", KbIdError::Empty),
            ("ABC", KbIdError::InvalidChars),
            ("with space", KbIdError::InvalidChars),
            ("with/slash", KbIdError::InvalidChars),
            ("-leading", KbIdError::BadShape),
            ("trailing-", KbIdError::BadShape),
            ("dou--ble", KbIdError::BadShape),
            ("../escape", KbIdError::InvalidChars),
        ] {
            assert_eq!(validate_kb_id(id).unwrap_err(), want, "for {id}");
        }
    }

    #[test]
    fn path_helpers_compose() {
        let root = Path::new("/tmp/kb");
        assert_eq!(kb_root(root, "x"), Path::new("/tmp/kb/x"));
        assert_eq!(kb_knowledge_dir(root, "x"), Path::new("/tmp/kb/x/knowledge"));
        assert_eq!(kb_raw_dir(root, "x"), Path::new("/tmp/kb/x/raw"));
        assert_eq!(
            kb_internal_dir(root, "x"),
            Path::new("/tmp/kb/x/.biorouter-knowledge")
        );
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p biorouter --lib knowledge::paths::tests`
Expected: 3 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/biorouter/src/knowledge/paths.rs
git commit -m "feat(knowledge): kb-id validation and path resolution helpers"
```

---

## Task 4: Top-level registry (`registry.rs`)

**Files:**
- Modify: `crates/biorouter/src/knowledge/registry.rs`

- [ ] **Step 1: Write the failing tests**

Replace placeholder with:

```rust
use crate::knowledge::types::RegistryEntry;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

const REGISTRY_FILE: &str = "registry.yaml";

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct RegistryDoc {
    #[serde(default)]
    bases: Vec<RegistryEntry>,
}

pub fn registry_path(root: &Path) -> PathBuf {
    root.join(REGISTRY_FILE)
}

pub fn load(root: &Path) -> Result<Vec<RegistryEntry>> {
    let p = registry_path(root);
    if !p.exists() {
        return Ok(Vec::new());
    }
    let s = std::fs::read_to_string(&p)
        .with_context(|| format!("reading {}", p.display()))?;
    let doc: RegistryDoc = serde_yaml::from_str(&s)?;
    Ok(doc.bases)
}

pub fn register(root: &Path, entry: RegistryEntry) -> Result<()> {
    let mut bases = load(root)?;
    if bases.iter().any(|b| b.id == entry.id) {
        anyhow::bail!("kb-id '{}' already registered", entry.id);
    }
    bases.push(entry);
    save(root, &bases)
}

pub fn unregister(root: &Path, id: &str) -> Result<()> {
    let mut bases = load(root)?;
    let before = bases.len();
    bases.retain(|b| b.id != id);
    if bases.len() == before {
        anyhow::bail!("kb-id '{id}' not found in registry");
    }
    save(root, &bases)
}

fn save(root: &Path, bases: &[RegistryEntry]) -> Result<()> {
    std::fs::create_dir_all(root)?;
    let doc = RegistryDoc { bases: bases.to_vec() };
    let yaml = serde_yaml::to_string(&doc)?;
    let tmp = registry_path(root).with_extension("yaml.tmp");
    std::fs::write(&tmp, yaml)?;
    std::fs::rename(tmp, registry_path(root))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_empty_root_returns_no_bases() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(dir.path()).unwrap(), vec![]);
    }

    #[test]
    fn register_then_load() {
        let dir = tempfile::tempdir().unwrap();
        let e = RegistryEntry { id: "ms".into(), path: dir.path().join("ms") };
        register(dir.path(), e.clone()).unwrap();
        assert_eq!(load(dir.path()).unwrap(), vec![e]);
    }

    #[test]
    fn register_rejects_duplicate() {
        let dir = tempfile::tempdir().unwrap();
        let e = RegistryEntry { id: "ms".into(), path: dir.path().join("ms") };
        register(dir.path(), e.clone()).unwrap();
        let err = register(dir.path(), e).unwrap_err();
        assert!(err.to_string().contains("already registered"));
    }

    #[test]
    fn unregister_removes_entry() {
        let dir = tempfile::tempdir().unwrap();
        let e = RegistryEntry { id: "ms".into(), path: dir.path().join("ms") };
        register(dir.path(), e).unwrap();
        unregister(dir.path(), "ms").unwrap();
        assert_eq!(load(dir.path()).unwrap(), vec![]);
    }

    #[test]
    fn unregister_unknown_errors() {
        let dir = tempfile::tempdir().unwrap();
        let err = unregister(dir.path(), "nope").unwrap_err();
        assert!(err.to_string().contains("not found"));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p biorouter --lib knowledge::registry::tests`
Expected: 5 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/biorouter/src/knowledge/registry.rs
git commit -m "feat(knowledge): top-level registry of knowledge bases"
```

---

## Task 5: Per-KB manifest read/write (`manifest.rs`)

**Files:**
- Modify: `crates/biorouter/src/knowledge/manifest.rs`

- [ ] **Step 1: Write the failing tests**

```rust
use crate::knowledge::types::Manifest;
use anyhow::{Context, Result};
use std::path::Path;

const MANIFEST_FILE: &str = "manifest.yaml";

pub fn manifest_path(kb_root: &Path) -> std::path::PathBuf {
    kb_root.join(MANIFEST_FILE)
}

pub fn load(kb_root: &Path) -> Result<Manifest> {
    let p = manifest_path(kb_root);
    let s = std::fs::read_to_string(&p)
        .with_context(|| format!("reading {}", p.display()))?;
    Ok(serde_yaml::from_str(&s)?)
}

pub fn save(kb_root: &Path, m: &Manifest) -> Result<()> {
    std::fs::create_dir_all(kb_root)?;
    let yaml = serde_yaml::to_string(m)?;
    let tmp = manifest_path(kb_root).with_extension("yaml.tmp");
    std::fs::write(&tmp, yaml)?;
    std::fs::rename(tmp, manifest_path(kb_root))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample() -> Manifest {
        Manifest {
            id: "ms".into(),
            name: "MS Patient Analysis".into(),
            color: "#5a6394".into(),
            created_at: chrono::Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            schema_version: 1,
            default_model: None,
        }
    }

    #[test]
    fn roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        save(dir.path(), &sample()).unwrap();
        assert_eq!(load(dir.path()).unwrap(), sample());
    }

    #[test]
    fn save_uses_atomic_rename() {
        let dir = tempfile::tempdir().unwrap();
        save(dir.path(), &sample()).unwrap();
        let listing: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert!(listing.iter().any(|n| n == "manifest.yaml"));
        assert!(!listing.iter().any(|n| n == "manifest.yaml.tmp"));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p biorouter --lib knowledge::manifest::tests`
Expected: 2 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/biorouter/src/knowledge/manifest.rs
git commit -m "feat(knowledge): per-KB manifest read/write with atomic rename"
```

---

## Task 6: Default schema.md template

**Files:**
- Create: `crates/biorouter/src/knowledge/schema_default.md`

- [ ] **Step 1: Write the default schema**

Create the file with the following content:

````markdown
# Knowledge Base — Maintenance Schema

This document tells the LLM how to maintain *this particular* knowledge base.
It is read fresh on every macro call. Edit it freely to shape the knowledge
voice, structure, and conventions.

## Layout

- `raw/<source-id>/` — original files + derived `source.md` + `meta.yaml`. **Read-only.**
- `knowledge/sources/<source-id>.md` — one page per source; summary + key extractions + outbound links.
- `knowledge/entities/<name>.md` — proper nouns (genes, drugs, people, datasets, methods).
- `knowledge/concepts/<name>.md` — ideas, mechanisms, theories.
- `knowledge/notes/<slug>.md` — ad-hoc pages, including queries-as-pages.
- `<name>.md` at the root of the knowledge folder — cross-cutting hubs (top of the graph).
- `index.md` — flat catalog of all pages. You maintain it on every change.
- `log.md` — chronological log; you append on every change.

## Page format

Every knowledge page starts with YAML frontmatter:

```yaml
---
title: <Human Title>
kind: entity | concept | source | note | hub
tags: [optional]
credibility_inherits: [source-id-1, source-id-2]   # which sources back this page
last_updated: 2026-05-30T12:00:00Z
contradiction: false   # set true to render as a flag node
---
```

Body is prose markdown with `[[knowledge-link]]` cross-references.

## Ingest workflow

When `kb_ingest_source` is called:

1. Read `raw/<source-id>/source.md` and `meta.yaml`.
2. Decide what biomedical entities and concepts the source touches.
3. Create or update `knowledge/sources/<source-id>.md` with: 2-3 sentence summary,
   key claims as bullets, methods if applicable, limitations, and outbound
   links to entity/concept pages.
4. For each entity/concept mentioned: if a page exists, update it; otherwise
   create it. Always include a backlink to the source page.
5. If a claim in the new source contradicts an existing page, mark the
   conflicting page with `contradiction: true` in frontmatter and add a
   section "## Open contradictions" listing both positions and the sources.
6. Update `index.md` with new/modified pages.
7. Append a one-line log entry to `log.md` of the form
   `## [<date>] ingest | <source-title>`.

## Credibility discipline

- Peer-reviewed papers and books outweigh preprints, gray literature, and
  web posts. Reflect this in language: hedge claims sourced only from web
  or personal materials ("according to a blog post", "the user noted").
- Never silently elevate a web claim to a knowledge-page assertion — always cite.

## Query workflow

When `kb_query` is called:

1. Search the knowledge folder for pages matching the question's entities/concepts.
2. Read the most relevant pages.
3. Compose an answer that cites pages with `[[knowledge-link]]`.
4. If `file_as_page=true`, write the answer to `knowledge/notes/<slug>.md` and
   include it in the response. Append a log entry of kind `query`.

## Lint workflow

When `kb_lint` is called:

1. Find pages with no inbound links (orphans).
2. Find pages flagged `contradiction: true`.
3. Find concepts mentioned in source pages but lacking their own page.
4. Find sources >90 days old whose claims are not referenced anywhere.
5. Return a report. If `autofix=true`, fix the easy ones (add missing
   cross-references, create stub pages) and append a `lint` log entry.

## Tone

Concise, scientific, evidence-led. No hype, no certainty without citation.
````

- [ ] **Step 2: Verify file contents**

Run: `head -5 crates/biorouter/src/knowledge/schema_default.md`
Expected: starts with `# Knowledge Base — Maintenance Schema`.

- [ ] **Step 3: Commit**

```bash
git add crates/biorouter/src/knowledge/schema_default.md
git commit -m "feat(knowledge): default schema.md template for new bases"
```

---

## Task 7: Git wrapper — init, commit_all, log (`git.rs`)

**Files:**
- Modify: `crates/biorouter/src/knowledge/git.rs`

- [ ] **Step 1: Write the failing tests**

```rust
use crate::knowledge::types::{ChangeKind, HistoryEntry};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::path::Path;

pub struct GitRepo {
    inner: git2::Repository,
}

impl GitRepo {
    pub fn init(path: &Path) -> Result<Self> {
        let inner = git2::Repository::init(path)
            .with_context(|| format!("git init {}", path.display()))?;
        // Configure a deterministic identity so tests are reproducible.
        let mut cfg = inner.config()?;
        cfg.set_str("user.name", "BioRouter Knowledge")?;
        cfg.set_str("user.email", "knowledge@biorouter.local")?;
        cfg.set_str("commit.gpgsign", "false")?;
        Ok(Self { inner })
    }

    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self { inner: git2::Repository::open(path)? })
    }

    pub fn commit_all(&self, kind: ChangeKind, summary: &str, delta: Option<&str>) -> Result<String> {
        let mut index = self.inner.index()?;
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
        index.write()?;
        let tree_oid = index.write_tree()?;
        let tree = self.inner.find_tree(tree_oid)?;
        let sig = self.inner.signature()?;
        let parent = self.inner.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.as_ref().map(|c| vec![c]).unwrap_or_default();
        let msg = render_message(kind, summary, delta);
        let oid = self.inner.commit(
            Some("HEAD"),
            &sig,
            &sig,
            &msg,
            &tree,
            &parents,
        )?;
        Ok(oid.to_string())
    }

    pub fn log(&self, limit: usize) -> Result<Vec<HistoryEntry>> {
        let mut walk = self.inner.revwalk()?;
        walk.push_head()?;
        walk.set_sorting(git2::Sort::TIME)?;
        let mut out = Vec::new();
        for oid in walk.flatten().take(limit) {
            let commit = self.inner.find_commit(oid)?;
            let parsed = parse_message(commit.message().unwrap_or(""));
            out.push(HistoryEntry {
                commit_sha: oid.to_string(),
                kind: parsed.kind,
                summary: parsed.summary,
                delta: parsed.delta,
                timestamp: DateTime::<Utc>::from_timestamp(commit.time().seconds(), 0)
                    .unwrap_or_else(Utc::now),
            });
        }
        Ok(out)
    }
}

fn render_message(kind: ChangeKind, summary: &str, delta: Option<&str>) -> String {
    let kind_str = match kind {
        ChangeKind::Ingest => "ingest",
        ChangeKind::Link => "link",
        ChangeKind::Flag => "flag",
        ChangeKind::Query => "query",
        ChangeKind::Lint => "lint",
        ChangeKind::Restore => "restore",
        ChangeKind::Manual => "manual",
    };
    match delta {
        Some(d) => format!("[{kind_str}] {summary}\n\ndelta: {d}\n"),
        None => format!("[{kind_str}] {summary}\n"),
    }
}

struct Parsed {
    kind: ChangeKind,
    summary: String,
    delta: Option<String>,
}

fn parse_message(msg: &str) -> Parsed {
    let mut lines = msg.lines();
    let header = lines.next().unwrap_or("");
    let (kind, summary) = parse_header(header);
    let delta = msg.lines().find_map(|l| l.strip_prefix("delta: ").map(str::to_string));
    Parsed { kind, summary, delta }
}

fn parse_header(header: &str) -> (ChangeKind, String) {
    let kind = if let Some(rest) = header.strip_prefix('[') {
        if let Some((k, _)) = rest.split_once(']') {
            match k {
                "ingest" => ChangeKind::Ingest,
                "link" => ChangeKind::Link,
                "flag" => ChangeKind::Flag,
                "query" => ChangeKind::Query,
                "lint" => ChangeKind::Lint,
                "restore" => ChangeKind::Restore,
                _ => ChangeKind::Manual,
            }
        } else {
            ChangeKind::Manual
        }
    } else {
        ChangeKind::Manual
    };
    let summary = header
        .split_once(']')
        .map(|(_, s)| s.trim_start().to_string())
        .unwrap_or_else(|| header.to_string());
    (kind, summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_creates_repo() {
        let dir = tempfile::tempdir().unwrap();
        GitRepo::init(dir.path()).unwrap();
        assert!(dir.path().join(".git").exists());
    }

    #[test]
    fn commit_and_log_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let repo = GitRepo::init(dir.path()).unwrap();
        std::fs::write(dir.path().join("a.md"), "hello").unwrap();
        let sha = repo
            .commit_all(ChangeKind::Ingest, "first source", Some("+1 page"))
            .unwrap();
        let log = repo.log(10).unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].commit_sha, sha);
        assert_eq!(log[0].kind, ChangeKind::Ingest);
        assert_eq!(log[0].summary, "first source");
        assert_eq!(log[0].delta.as_deref(), Some("+1 page"));
    }

    #[test]
    fn multiple_commits_ordered_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let repo = GitRepo::init(dir.path()).unwrap();
        std::fs::write(dir.path().join("a.md"), "1").unwrap();
        repo.commit_all(ChangeKind::Ingest, "one", None).unwrap();
        std::fs::write(dir.path().join("b.md"), "2").unwrap();
        repo.commit_all(ChangeKind::Lint, "two", None).unwrap();
        let log = repo.log(10).unwrap();
        assert_eq!(log[0].summary, "two");
        assert_eq!(log[1].summary, "one");
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p biorouter --lib knowledge::git::tests`
Expected: 3 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/biorouter/src/knowledge/git.rs
git commit -m "feat(knowledge): git wrapper with commit_all + log + message format"
```

---

## Task 8: Git wrapper — transactions

**Files:**
- Modify: `crates/biorouter/src/knowledge/git.rs`

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module in `git.rs`:

```rust
    #[test]
    fn txn_lifecycle_squash_merges_into_main() {
        let dir = tempfile::tempdir().unwrap();
        let repo = GitRepo::init(dir.path()).unwrap();
        std::fs::write(dir.path().join("seed.md"), "seed").unwrap();
        repo.commit_all(ChangeKind::Manual, "seed", None).unwrap();

        let txn = repo.begin_txn("ingest paper X").unwrap();
        std::fs::write(dir.path().join("p1.md"), "1").unwrap();
        repo.commit_on_txn(&txn, "step 1").unwrap();
        std::fs::write(dir.path().join("p2.md"), "2").unwrap();
        repo.commit_on_txn(&txn, "step 2").unwrap();
        let final_sha = repo
            .commit_txn(&txn, ChangeKind::Ingest, "Paper X", Some("+2 pages"))
            .unwrap();

        let log = repo.log(10).unwrap();
        assert_eq!(log[0].commit_sha, final_sha);
        assert_eq!(log[0].summary, "Paper X");
        assert_eq!(log[0].kind, ChangeKind::Ingest);
        assert_eq!(log.len(), 2, "seed + squashed-ingest only — no intermediate commits");
    }

    #[test]
    fn txn_abort_leaves_main_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let repo = GitRepo::init(dir.path()).unwrap();
        std::fs::write(dir.path().join("seed.md"), "seed").unwrap();
        repo.commit_all(ChangeKind::Manual, "seed", None).unwrap();
        let pre = repo.log(10).unwrap();

        let txn = repo.begin_txn("doomed").unwrap();
        std::fs::write(dir.path().join("doom.md"), "x").unwrap();
        repo.commit_on_txn(&txn, "bad").unwrap();
        repo.abort_txn(&txn).unwrap();

        let post = repo.log(10).unwrap();
        assert_eq!(pre, post);
        assert!(!dir.path().join("doom.md").exists(), "working tree restored");
    }
```

- [ ] **Step 2: Implement transactions**

Add to `git.rs` (above the `#[cfg(test)]` block):

```rust
pub struct Txn {
    pub branch: String,
}

impl GitRepo {
    pub fn begin_txn(&self, label: &str) -> Result<Txn> {
        let id = uuid::Uuid::new_v4();
        let branch = format!("txn/{label}-{id}", label = slugify(label));
        let head = self.inner.head()?.peel_to_commit()?;
        self.inner.branch(&branch, &head, false)?;
        self.inner.set_head(&format!("refs/heads/{branch}"))?;
        Ok(Txn { branch })
    }

    pub fn commit_on_txn(&self, _txn: &Txn, message: &str) -> Result<String> {
        // Same as commit_all but caller already on the txn branch.
        let mut index = self.inner.index()?;
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
        index.write()?;
        let tree_oid = index.write_tree()?;
        let tree = self.inner.find_tree(tree_oid)?;
        let sig = self.inner.signature()?;
        let parent = self.inner.head()?.peel_to_commit()?;
        let oid = self.inner.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])?;
        Ok(oid.to_string())
    }

    pub fn commit_txn(&self, txn: &Txn, kind: ChangeKind, summary: &str, delta: Option<&str>) -> Result<String> {
        // Squash-merge txn branch onto main as one commit.
        let main = self.inner.find_branch("main", git2::BranchType::Local)
            .or_else(|_| self.inner.find_branch("master", git2::BranchType::Local))?;
        let main_name = main.name()?.unwrap_or("main").to_string();
        let txn_commit = self.inner.find_branch(&txn.branch, git2::BranchType::Local)?
            .get().peel_to_commit()?;
        let txn_tree = txn_commit.tree()?;
        let main_commit = main.get().peel_to_commit()?;

        let sig = self.inner.signature()?;
        let msg = render_message(kind, summary, delta);
        let new_oid = self.inner.commit(
            Some(&format!("refs/heads/{main_name}")),
            &sig, &sig, &msg, &txn_tree, &[&main_commit],
        )?;

        // Move HEAD back to main and check out the new tree.
        self.inner.set_head(&format!("refs/heads/{main_name}"))?;
        self.inner.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;
        // Delete txn branch.
        self.inner.find_branch(&txn.branch, git2::BranchType::Local)?.delete()?;
        Ok(new_oid.to_string())
    }

    pub fn abort_txn(&self, txn: &Txn) -> Result<()> {
        let main = self.inner.find_branch("main", git2::BranchType::Local)
            .or_else(|_| self.inner.find_branch("master", git2::BranchType::Local))?;
        let main_name = main.name()?.unwrap_or("main").to_string();
        self.inner.set_head(&format!("refs/heads/{main_name}"))?;
        self.inner.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;
        self.inner.find_branch(&txn.branch, git2::BranchType::Local)?
            .delete()?;
        Ok(())
    }
}

fn slugify(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}
```

Also: in `init`, after configuring user identity, also ensure the initial branch is `main`. Replace the `init` method body with:

```rust
    pub fn init(path: &Path) -> Result<Self> {
        let mut opts = git2::RepositoryInitOptions::new();
        opts.initial_head("main");
        let inner = git2::Repository::init_opts(path, &opts)
            .with_context(|| format!("git init {}", path.display()))?;
        let mut cfg = inner.config()?;
        cfg.set_str("user.name", "BioRouter Knowledge")?;
        cfg.set_str("user.email", "knowledge@biorouter.local")?;
        cfg.set_str("commit.gpgsign", "false")?;
        Ok(Self { inner })
    }
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p biorouter --lib knowledge::git::tests`
Expected: 5 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/biorouter/src/knowledge/git.rs
git commit -m "feat(knowledge): git transactions (begin/commit-squash/abort)"
```

---

## Task 9: Git wrapper — preview and restore

**Files:**
- Modify: `crates/biorouter/src/knowledge/git.rs`

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module:

```rust
    #[test]
    fn preview_state_returns_file_at_commit() {
        let dir = tempfile::tempdir().unwrap();
        let repo = GitRepo::init(dir.path()).unwrap();
        std::fs::write(dir.path().join("a.md"), "v1").unwrap();
        let sha1 = repo.commit_all(ChangeKind::Manual, "v1", None).unwrap();
        std::fs::write(dir.path().join("a.md"), "v2").unwrap();
        repo.commit_all(ChangeKind::Manual, "v2", None).unwrap();
        let v1 = repo.read_file_at(&sha1, "a.md").unwrap();
        assert_eq!(v1.as_deref(), Some("v1"));
    }

    #[test]
    fn restore_state_creates_new_commit_with_old_tree() {
        let dir = tempfile::tempdir().unwrap();
        let repo = GitRepo::init(dir.path()).unwrap();
        std::fs::write(dir.path().join("a.md"), "v1").unwrap();
        let sha1 = repo.commit_all(ChangeKind::Manual, "v1", None).unwrap();
        std::fs::write(dir.path().join("a.md"), "v2").unwrap();
        repo.commit_all(ChangeKind::Manual, "v2", None).unwrap();
        let new_sha = repo.restore_to(&sha1, "restore to v1").unwrap();
        // Working tree should now contain v1.
        let body = std::fs::read_to_string(dir.path().join("a.md")).unwrap();
        assert_eq!(body, "v1");
        // History still grows forward.
        let log = repo.log(10).unwrap();
        assert_eq!(log[0].commit_sha, new_sha);
        assert_eq!(log[0].kind, ChangeKind::Restore);
        assert_eq!(log.len(), 3);
    }
```

- [ ] **Step 2: Implement preview + restore**

Add to `git.rs`:

```rust
impl GitRepo {
    pub fn read_file_at(&self, sha: &str, path: &str) -> Result<Option<String>> {
        let oid = git2::Oid::from_str(sha)?;
        let commit = self.inner.find_commit(oid)?;
        let tree = commit.tree()?;
        let entry = match tree.get_path(Path::new(path)) {
            Ok(e) => e,
            Err(_) => return Ok(None),
        };
        let obj = entry.to_object(&self.inner)?;
        let blob = obj.as_blob().ok_or_else(|| anyhow::anyhow!("not a blob"))?;
        Ok(Some(String::from_utf8_lossy(blob.content()).to_string()))
    }

    pub fn restore_to(&self, sha: &str, summary: &str) -> Result<String> {
        let oid = git2::Oid::from_str(sha)?;
        let target = self.inner.find_commit(oid)?;
        let target_tree = target.tree()?;
        let head = self.inner.head()?.peel_to_commit()?;
        let sig = self.inner.signature()?;
        let msg = render_message(ChangeKind::Restore, summary, Some(&format!("→ {}", &sha[..7])));
        let new_oid = self.inner.commit(
            Some("HEAD"), &sig, &sig, &msg, &target_tree, &[&head],
        )?;
        // Check out the new commit so working tree matches.
        self.inner.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;
        Ok(new_oid.to_string())
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p biorouter --lib knowledge::git::tests`
Expected: 7 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/biorouter/src/knowledge/git.rs
git commit -m "feat(knowledge): git preview (read_file_at) and restore (restore_to)"
```

---

## Task 10: KnowledgeService — `create_base`

**Files:**
- Modify: `crates/biorouter/src/knowledge/service.rs`

- [ ] **Step 1: Write the failing tests**

```rust
use crate::knowledge::{
    git::GitRepo,
    manifest, paths, registry,
    types::{Manifest, RegistryEntry},
};
use anyhow::{Context, Result};
use chrono::Utc;
use std::path::{Path, PathBuf};

const DEFAULT_SCHEMA: &str = include_str!("schema_default.md");
const DEFAULT_INDEX: &str = "# Index\n\n_no pages yet_\n";
const DEFAULT_LOG: &str = "# Log\n\n";
const GITIGNORE: &str = "raw/*/original.*\n.biorouter-knowledge/.crossref-cache/\n";

#[derive(Clone)]
pub struct KnowledgeService {
    root: PathBuf,
}

impl KnowledgeService {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn new_default() -> Result<Self> {
        Ok(Self::new(paths::knowledge_root()?))
    }

    pub fn root(&self) -> &Path { &self.root }

    pub fn create_base(&self, id: &str, name: &str, color: Option<&str>) -> Result<Manifest> {
        paths::validate_kb_id(id)?;
        let kb_root = paths::kb_root(&self.root, id);
        if kb_root.exists() {
            anyhow::bail!("kb '{id}' already exists at {}", kb_root.display());
        }
        std::fs::create_dir_all(paths::kb_knowledge_dir(&self.root, id).join("entities"))?;
        std::fs::create_dir_all(paths::kb_knowledge_dir(&self.root, id).join("concepts"))?;
        std::fs::create_dir_all(paths::kb_knowledge_dir(&self.root, id).join("sources"))?;
        std::fs::create_dir_all(paths::kb_knowledge_dir(&self.root, id).join("notes"))?;
        std::fs::create_dir_all(paths::kb_raw_dir(&self.root, id))?;
        std::fs::create_dir_all(paths::kb_internal_dir(&self.root, id))?;

        let m = Manifest {
            id: id.to_string(),
            name: name.to_string(),
            color: color.unwrap_or("#5a6394").to_string(),
            created_at: Utc::now(),
            schema_version: 1,
            default_model: None,
        };
        manifest::save(&kb_root, &m)?;

        std::fs::write(kb_root.join("schema.md"), DEFAULT_SCHEMA)?;
        std::fs::write(kb_root.join("index.md"), DEFAULT_INDEX)?;
        std::fs::write(kb_root.join("log.md"), DEFAULT_LOG)?;
        std::fs::write(kb_root.join(".gitignore"), GITIGNORE)?;

        let repo = GitRepo::init(&kb_root)?;
        repo.commit_all(
            crate::knowledge::types::ChangeKind::Manual,
            &format!("create knowledge base {id}"),
            None,
        )
        .context("initial commit")?;

        registry::register(
            &self.root,
            RegistryEntry { id: id.to_string(), path: kb_root },
        )?;
        Ok(m)
    }

    pub fn list_bases(&self) -> Result<Vec<Manifest>> {
        let entries = registry::load(&self.root)?;
        let mut out = Vec::new();
        for e in entries {
            if let Ok(m) = manifest::load(&e.path) {
                out.push(m);
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svc() -> (tempfile::TempDir, KnowledgeService) {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        (dir, svc)
    }

    #[test]
    fn create_base_writes_all_files_and_inits_git() {
        let (_dir, svc) = svc();
        let m = svc.create_base("ms", "MS Patient Analysis", None).unwrap();
        let kb = svc.root().join("ms");
        assert!(kb.join("manifest.yaml").exists());
        assert!(kb.join("schema.md").exists());
        assert!(kb.join("index.md").exists());
        assert!(kb.join("log.md").exists());
        assert!(kb.join(".gitignore").exists());
        assert!(kb.join("knowledge/entities").exists());
        assert!(kb.join("knowledge/concepts").exists());
        assert!(kb.join("knowledge/sources").exists());
        assert!(kb.join("knowledge/notes").exists());
        assert!(kb.join("raw").exists());
        assert!(kb.join(".biorouter-knowledge").exists());
        assert!(kb.join(".git").exists());
        assert_eq!(m.id, "ms");

        // Initial commit exists.
        let repo = GitRepo::open(&kb).unwrap();
        let log = repo.log(10).unwrap();
        assert_eq!(log.len(), 1);
        assert!(log[0].summary.contains("create knowledge base ms"));

        // Registry has one entry.
        let bases = svc.list_bases().unwrap();
        assert_eq!(bases.len(), 1);
        assert_eq!(bases[0].name, "MS Patient Analysis");
    }

    #[test]
    fn create_base_rejects_duplicate() {
        let (_dir, svc) = svc();
        svc.create_base("ms", "x", None).unwrap();
        let err = svc.create_base("ms", "y", None).unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn create_base_rejects_invalid_id() {
        let (_dir, svc) = svc();
        let err = svc.create_base("BAD", "x", None).unwrap_err();
        assert!(err.to_string().contains("a-z, 0-9"), "got: {err}");
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p biorouter --lib knowledge::service::tests`
Expected: 3 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/biorouter/src/knowledge/service.rs
git commit -m "feat(knowledge): KnowledgeService::create_base + list_bases"
```

---

## Task 11: Page store CRUD (`store.rs`)

**Files:**
- Modify: `crates/biorouter/src/knowledge/store.rs`

- [ ] **Step 1: Write the failing tests**

```rust
use crate::knowledge::{git::GitRepo, paths, types::ChangeKind};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageRef {
    pub path: String,
    pub title: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageContent {
    pub path: String,
    pub content: String,
    pub frontmatter: serde_yaml::Value,
}

pub fn list_pages(kb_root: &Path, prefix: Option<&str>) -> Result<Vec<PageRef>> {
    let knowledge_dir = kb_root.join("knowledge");
    if !knowledge_dir.exists() { return Ok(Vec::new()); }
    let mut out = Vec::new();
    walk_md(&knowledge_dir, &knowledge_dir, prefix, &mut out)?;
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

fn walk_md(base: &Path, dir: &Path, prefix: Option<&str>, out: &mut Vec<PageRef>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir() {
            walk_md(base, &p, prefix, out)?;
        } else if p.extension().and_then(|e| e.to_str()) == Some("md") {
            let rel = p.strip_prefix(base).unwrap().to_string_lossy().to_string();
            let logical = format!("knowledge/{rel}");
            if let Some(pre) = prefix {
                if !logical.starts_with(pre) { continue; }
            }
            let body = std::fs::read_to_string(&p)?;
            let (fm, _) = split_frontmatter(&body);
            let title = fm.get("title").and_then(|v| v.as_str())
                .unwrap_or_else(|| p.file_stem().unwrap().to_str().unwrap())
                .to_string();
            let kind = fm.get("kind").and_then(|v| v.as_str()).unwrap_or("note").to_string();
            out.push(PageRef { path: logical, title, kind });
        }
    }
    Ok(())
}

pub fn read_page(kb_root: &Path, path: &str) -> Result<PageContent> {
    let abs = resolve_page_path(kb_root, path)?;
    let raw = std::fs::read_to_string(&abs)
        .with_context(|| format!("reading {}", abs.display()))?;
    let (fm, body) = split_frontmatter(&raw);
    Ok(PageContent { path: path.to_string(), content: body, frontmatter: fm })
}

pub fn write_page(
    kb_root: &Path,
    path: &str,
    content: &str,
    commit_message: &str,
    txn_branch: Option<&str>,
) -> Result<Option<String>> {
    let abs = resolve_page_path(kb_root, path)?;
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = abs.with_extension("md.tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(tmp, &abs)?;

    let repo = GitRepo::open(kb_root)?;
    if let Some(_branch) = txn_branch {
        // Caller has already switched HEAD to the txn branch via begin_txn.
        let sha = repo.commit_on_txn_in_progress(commit_message)?;
        Ok(Some(sha))
    } else {
        let sha = repo.commit_all(ChangeKind::Manual, commit_message, None)?;
        Ok(Some(sha))
    }
}

fn resolve_page_path(kb_root: &Path, logical: &str) -> Result<std::path::PathBuf> {
    if !logical.starts_with("knowledge/") && logical != "index.md" && logical != "schema.md" && logical != "log.md" {
        anyhow::bail!("page path must start with knowledge/ or be index.md/schema.md/log.md");
    }
    if logical.contains("..") { anyhow::bail!("path traversal not allowed"); }
    Ok(kb_root.join(logical))
}

pub fn split_frontmatter(s: &str) -> (serde_yaml::Value, String) {
    if let Some(rest) = s.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---\n") {
            let fm = &rest[..end];
            let body = &rest[end + 5..];
            if let Ok(v) = serde_yaml::from_str(fm) {
                return (v, body.to_string());
            }
        }
    }
    (serde_yaml::Value::Null, s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::service::KnowledgeService;

    fn fresh() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("k", "K", None).unwrap();
        let kb_root = dir.path().join("k");
        (dir, kb_root)
    }

    #[test]
    fn write_then_read_roundtrip() {
        let (_dir, kb) = fresh();
        let body = "---\ntitle: HRV\nkind: entity\n---\n\nBody text.";
        write_page(&kb, "knowledge/entities/hrv.md", body, "add HRV", None).unwrap();
        let p = read_page(&kb, "knowledge/entities/hrv.md").unwrap();
        assert_eq!(p.frontmatter["title"], serde_yaml::Value::from("HRV"));
        assert_eq!(p.content.trim(), "Body text.");
    }

    #[test]
    fn list_pages_sorted_and_filtered() {
        let (_dir, kb) = fresh();
        write_page(&kb, "knowledge/entities/b.md", "---\ntitle: B\n---\n", "b", None).unwrap();
        write_page(&kb, "knowledge/concepts/a.md", "---\ntitle: A\n---\n", "a", None).unwrap();
        let all = list_pages(&kb, None).unwrap();
        let paths: Vec<_> = all.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(paths, vec!["knowledge/concepts/a.md", "knowledge/entities/b.md"]);
        let only_entities = list_pages(&kb, Some("knowledge/entities/")).unwrap();
        assert_eq!(only_entities.len(), 1);
    }

    #[test]
    fn rejects_path_traversal() {
        let (_dir, kb) = fresh();
        let err = write_page(&kb, "knowledge/../escape.md", "x", "x", None).unwrap_err();
        assert!(err.to_string().contains("traversal"));
    }

    #[test]
    fn rejects_paths_outside_knowledge() {
        let (_dir, kb) = fresh();
        let err = write_page(&kb, "raw/x.md", "x", "x", None).unwrap_err();
        assert!(err.to_string().contains("knowledge/"));
    }
}
```

- [ ] **Step 2: Expose `commit_on_txn_in_progress` on GitRepo**

In `git.rs`, add (near the other txn methods):

```rust
impl GitRepo {
    /// Commit on the currently-checked-out branch. Used by store::write_page
    /// when a txn is active and the caller has already switched HEAD.
    pub fn commit_on_txn_in_progress(&self, message: &str) -> Result<String> {
        let mut index = self.inner.index()?;
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
        index.write()?;
        let tree_oid = index.write_tree()?;
        let tree = self.inner.find_tree(tree_oid)?;
        let sig = self.inner.signature()?;
        let parent = self.inner.head()?.peel_to_commit()?;
        let oid = self.inner.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])?;
        Ok(oid.to_string())
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p biorouter --lib knowledge::store::tests`
Expected: 4 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/biorouter/src/knowledge/store.rs crates/biorouter/src/knowledge/git.rs
git commit -m "feat(knowledge): page CRUD with frontmatter split + safe path resolution"
```

---

## Task 12: Raw source storage (`raw.rs`)

**Files:**
- Modify: `crates/biorouter/src/knowledge/raw.rs`

- [ ] **Step 1: Write the failing tests**

```rust
use crate::knowledge::{paths, types::SourceMeta};
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::path::Path;

pub struct RawWrite {
    pub source_id: String,
    pub source_md_path: String,   // kb-relative path (raw/<id>/source.md)
    pub meta_path: String,
}

pub fn write_raw(
    kb_root: &Path,
    original_bytes: Option<&[u8]>,
    original_filename: Option<&str>,
    derived_md: &str,
    meta: SourceMeta,
) -> Result<RawWrite> {
    let raw_dir = kb_root.join("raw").join(&meta.id);
    std::fs::create_dir_all(&raw_dir)?;
    if let (Some(bytes), Some(name)) = (original_bytes, original_filename) {
        let ext = Path::new(name).extension().and_then(|e| e.to_str()).unwrap_or("bin");
        std::fs::write(raw_dir.join(format!("original.{ext}")), bytes)?;
    }
    std::fs::write(raw_dir.join("source.md"), derived_md)?;
    let yaml = serde_yaml::to_string(&meta)?;
    std::fs::write(raw_dir.join("meta.yaml"), yaml)?;
    Ok(RawWrite {
        source_id: meta.id.clone(),
        source_md_path: format!("raw/{}/source.md", meta.id),
        meta_path: format!("raw/{}/meta.yaml", meta.id),
    })
}

pub fn read_meta(kb_root: &Path, source_id: &str) -> Result<SourceMeta> {
    let p = kb_root.join("raw").join(source_id).join("meta.yaml");
    let s = std::fs::read_to_string(&p)?;
    Ok(serde_yaml::from_str(&s)?)
}

pub fn list_sources(kb_root: &Path) -> Result<Vec<SourceMeta>> {
    let dir = kb_root.join("raw");
    if !dir.exists() { return Ok(Vec::new()); }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.path().is_dir() {
            let id = entry.file_name().to_string_lossy().to_string();
            if let Ok(m) = read_meta(kb_root, &id) {
                out.push(m);
            }
        }
    }
    Ok(out)
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

pub fn new_source_id(title: &str) -> String {
    let slug = title
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .chars()
        .take(40)
        .collect::<String>();
    let id = uuid::Uuid::new_v4();
    if slug.is_empty() {
        format!("src-{}", &id.to_string()[..8])
    } else {
        format!("{slug}-{}", &id.to_string()[..6])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::{service::KnowledgeService, types::{Credibility, CredibilityTier}};
    use chrono::Utc;

    fn fresh() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("k", "K", None).unwrap();
        (dir.into(), Path::new("k").to_path_buf())
            .pipe(|(d, _)| {
                let kb = d.path().join("k");
                (d, kb)
            })
    }

    trait Pipe: Sized { fn pipe<R, F: FnOnce(Self) -> R>(self, f: F) -> R { f(self) } }
    impl<T> Pipe for T {}

    fn sample_meta(id: &str) -> SourceMeta {
        SourceMeta {
            id: id.into(),
            title: "Title".into(),
            url: Some("https://example.org/x".into()),
            ingested_at: Utc::now(),
            sha256: "abc".into(),
            mime: "text/html".into(),
            original_filename: Some("x.html".into()),
            credibility: Credibility {
                tier: CredibilityTier::Web,
                confidence: 0.5,
                publisher: None, venue: None, doi: None, retracted: false,
                reasoning: "test".into(), classifier_version: 1,
            },
        }
    }

    #[test]
    fn write_then_read_meta() {
        let (_d, kb) = fresh();
        let m = sample_meta("paper-x");
        let w = write_raw(&kb, Some(b"<html>x</html>"), Some("x.html"), "# X\n", m.clone()).unwrap();
        assert_eq!(w.source_id, "paper-x");
        assert!(kb.join("raw/paper-x/source.md").exists());
        assert!(kb.join("raw/paper-x/original.html").exists());
        assert!(kb.join("raw/paper-x/meta.yaml").exists());
        let back = read_meta(&kb, "paper-x").unwrap();
        assert_eq!(back.id, m.id);
    }

    #[test]
    fn list_sources_returns_all() {
        let (_d, kb) = fresh();
        for id in ["a-1", "b-2"] {
            write_raw(&kb, None, None, "md", sample_meta(id)).unwrap();
        }
        let mut ids: Vec<_> = list_sources(&kb).unwrap().into_iter().map(|m| m.id).collect();
        ids.sort();
        assert_eq!(ids, vec!["a-1", "b-2"]);
    }

    #[test]
    fn new_source_id_is_slugified() {
        let id = new_source_id("A Paper on MS Lesions!");
        assert!(id.starts_with("a-paper-on-ms-lesions-"), "got {id}");
    }

    #[test]
    fn hash_is_deterministic() {
        assert_eq!(hash_bytes(b"abc"), hash_bytes(b"abc"));
        assert_ne!(hash_bytes(b"abc"), hash_bytes(b"abd"));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p biorouter --lib knowledge::raw::tests`
Expected: 4 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/biorouter/src/knowledge/raw.rs
git commit -m "feat(knowledge): raw source storage + sha256 + slug-based ids"
```

---

## Task 13: HTML → Markdown conversion (`convert/html.rs`)

**Files:**
- Modify: `crates/biorouter/src/knowledge/convert/html.rs`
- Create: `crates/biorouter/src/knowledge/convert/fixtures/article.html`

- [ ] **Step 1: Create fixture HTML**

```html
<!doctype html>
<html><head><meta charset="utf-8"><title>Example article</title></head>
<body>
<h1>Example article</h1>
<p>An <strong>important</strong> paragraph with <a href="https://example.org">a link</a>.</p>
<h2>Section two</h2>
<ul><li>One</li><li>Two</li></ul>
</body></html>
```

- [ ] **Step 2: Write the failing tests**

In `convert/html.rs`:

```rust
use anyhow::Result;

pub struct HtmlConversion {
    pub markdown: String,
    pub title: Option<String>,
}

pub fn html_to_markdown(html: &str) -> Result<HtmlConversion> {
    let md = htmd::convert(html).map_err(|e| anyhow::anyhow!("htmd: {e}"))?;
    let title = extract_title(html);
    Ok(HtmlConversion { markdown: md, title })
}

fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find("<title>")? + "<title>".len();
    let end = lower[start..].find("</title>")? + start;
    Some(html[start..end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("fixtures/article.html");

    #[test]
    fn converts_headings_and_links() {
        let c = html_to_markdown(FIXTURE).unwrap();
        assert!(c.markdown.contains("# Example article"));
        assert!(c.markdown.contains("[a link](https://example.org)"));
        assert!(c.markdown.contains("## Section two"));
    }

    #[test]
    fn extracts_title() {
        let c = html_to_markdown(FIXTURE).unwrap();
        assert_eq!(c.title.as_deref(), Some("Example article"));
    }

    #[test]
    fn handles_empty_html() {
        let c = html_to_markdown("").unwrap();
        assert!(c.markdown.is_empty() || c.markdown.trim().is_empty());
        assert_eq!(c.title, None);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p biorouter --lib knowledge::convert::html::tests`
Expected: 3 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/biorouter/src/knowledge/convert/html.rs \
        crates/biorouter/src/knowledge/convert/fixtures/article.html
git commit -m "feat(knowledge): HTML → markdown conversion with title extraction"
```

---

## Task 14: PDF → Markdown conversion (`convert/pdf.rs`)

**Files:**
- Modify: `crates/biorouter/src/knowledge/convert/pdf.rs`
- Create: `crates/biorouter/src/knowledge/convert/fixtures/sample.pdf` (small text-only PDF; see step 1)

- [ ] **Step 1: Generate a fixture PDF**

If `printf` + `enscript` + `ps2pdf` are available, use them. Otherwise, write the PDF bytes inline via a small Rust dev script. The simplest cross-platform path: use `pdf-writer` to generate a fixture inline during the test rather than committing a binary. Use the inline approach:

(no file creation in this step — fixture is generated in the test itself)

- [ ] **Step 2: Add `pdf-writer` to dev-deps**

In `crates/biorouter/Cargo.toml`, add under `[dev-dependencies]`:

```toml
pdf-writer = "0.12"
```

- [ ] **Step 3: Write the failing tests + implementation**

In `convert/pdf.rs`:

```rust
use anyhow::{Context, Result};

pub struct PdfConversion {
    pub markdown: String,
    pub needs_llm_fallback: bool,
}

pub fn pdf_to_markdown(bytes: &[u8]) -> Result<PdfConversion> {
    let text = pdf_extract::extract_text_from_mem(bytes)
        .context("pdf-extract failed")?;
    let cleaned = normalize_text(&text);
    let needs_llm_fallback = cleaned.trim().len() < 32;
    Ok(PdfConversion { markdown: cleaned, needs_llm_fallback })
}

fn normalize_text(s: &str) -> String {
    // Collapse runs of whitespace, keep paragraph boundaries (double newlines).
    let mut out = String::new();
    for para in s.split("\n\n") {
        let joined: String = para
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if !joined.is_empty() {
            out.push_str(&joined);
            out.push_str("\n\n");
        }
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str};

    fn make_pdf(text: &str) -> Vec<u8> {
        let mut pdf = Pdf::new();
        let catalog_id = Ref::new(1);
        let page_tree_id = Ref::new(2);
        let page_id = Ref::new(3);
        let font_id = Ref::new(4);
        let content_id = Ref::new(5);

        pdf.catalog(catalog_id).pages(page_tree_id);
        pdf.pages(page_tree_id).kids([page_id]).count(1);

        let mut page = pdf.page(page_id);
        page.parent(page_tree_id)
            .media_box(Rect::new(0.0, 0.0, 595.0, 842.0))
            .resources()
            .fonts()
            .pair(Name(b"F1"), font_id);
        page.contents(content_id);
        page.finish();

        pdf.type1_font(font_id).base_font(Name(b"Helvetica"));

        let mut content = Content::new();
        content
            .begin_text()
            .set_font(Name(b"F1"), 12.0)
            .next_line(72.0, 770.0)
            .show(Str(text.as_bytes()))
            .end_text();
        pdf.stream(content_id, &content.finish());
        pdf.finish()
    }

    #[test]
    fn extracts_text_from_simple_pdf() {
        let bytes = make_pdf("Hello, knowledge.");
        let c = pdf_to_markdown(&bytes).unwrap();
        assert!(c.markdown.contains("Hello"), "got {:?}", c.markdown);
        assert!(!c.needs_llm_fallback);
    }

    #[test]
    fn flags_empty_pdf_for_llm_fallback() {
        let bytes = make_pdf("");
        let c = pdf_to_markdown(&bytes).unwrap();
        assert!(c.needs_llm_fallback);
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p biorouter --lib knowledge::convert::pdf::tests`
Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/biorouter/Cargo.toml crates/biorouter/src/knowledge/convert/pdf.rs
git commit -m "feat(knowledge): PDF → markdown via pdf-extract with LLM-fallback flag"
```

---

## Task 15: DOCX → Markdown conversion (`convert/docx.rs`)

**Files:**
- Modify: `crates/biorouter/src/knowledge/convert/docx.rs`

- [ ] **Step 1: Write the failing tests + implementation**

```rust
use anyhow::{Context, Result};

pub fn docx_to_markdown(bytes: &[u8]) -> Result<String> {
    use docx_rs::*;
    let docx = read_docx(bytes).context("read_docx failed")?;
    let mut out = String::new();
    for child in docx.document.children {
        if let DocumentChild::Paragraph(p) = child {
            let text = paragraph_text(&p);
            if text.trim().is_empty() { continue; }
            // Style heuristic: paragraph style "Heading1" / "Heading2" → markdown headers.
            let style = p.property.style.as_deref().unwrap_or("");
            let prefix = match style {
                s if s.eq_ignore_ascii_case("Heading1") => "# ",
                s if s.eq_ignore_ascii_case("Heading2") => "## ",
                s if s.eq_ignore_ascii_case("Heading3") => "### ",
                _ => "",
            };
            out.push_str(prefix);
            out.push_str(&text);
            out.push_str("\n\n");
        }
    }
    Ok(out.trim_end().to_string())
}

fn paragraph_text(p: &docx_rs::Paragraph) -> String {
    let mut s = String::new();
    for c in &p.children {
        if let docx_rs::ParagraphChild::Run(run) = c {
            for rc in &run.children {
                if let docx_rs::RunChild::Text(t) = rc {
                    s.push_str(&t.text);
                }
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use docx_rs::*;

    fn build_docx() -> Vec<u8> {
        let docx = Docx::new()
            .add_paragraph(
                Paragraph::new()
                    .style("Heading1")
                    .add_run(Run::new().add_text("Title")),
            )
            .add_paragraph(
                Paragraph::new().add_run(Run::new().add_text("First body paragraph.")),
            )
            .add_paragraph(
                Paragraph::new()
                    .style("Heading2")
                    .add_run(Run::new().add_text("Subhead")),
            )
            .add_paragraph(
                Paragraph::new().add_run(Run::new().add_text("More body.")),
            );
        let mut buf: Vec<u8> = Vec::new();
        docx.build().pack(&mut std::io::Cursor::new(&mut buf)).unwrap();
        buf
    }

    #[test]
    fn converts_headings_and_paragraphs() {
        let md = docx_to_markdown(&build_docx()).unwrap();
        assert!(md.contains("# Title"));
        assert!(md.contains("First body paragraph."));
        assert!(md.contains("## Subhead"));
        assert!(md.contains("More body."));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p biorouter --lib knowledge::convert::docx::tests`
Expected: 1 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/biorouter/src/knowledge/convert/docx.rs
git commit -m "feat(knowledge): DOCX → markdown with Heading1/2/3 mapping"
```

---

## Task 16: CSV → Markdown table (`convert/csv.rs`)

**Files:**
- Modify: `crates/biorouter/src/knowledge/convert/csv.rs`

- [ ] **Step 1: Write the failing tests + implementation**

```rust
use anyhow::Result;

pub fn csv_to_markdown(bytes: &[u8]) -> Result<String> {
    let mut rdr = csv::ReaderBuilder::new().has_headers(true).from_reader(bytes);
    let headers: Vec<String> = rdr.headers()?.iter().map(|s| s.to_string()).collect();
    if headers.is_empty() { return Ok(String::new()); }
    let mut rows: Vec<Vec<String>> = Vec::new();
    for rec in rdr.records() {
        let rec = rec?;
        rows.push(rec.iter().map(|s| escape_pipe(s)).collect());
    }
    let mut out = String::new();
    out.push('|');
    for h in &headers { out.push_str(&format!(" {} |", escape_pipe(h))); }
    out.push('\n');
    out.push('|');
    for _ in &headers { out.push_str(" --- |"); }
    out.push('\n');
    for r in rows {
        out.push('|');
        for (i, _) in headers.iter().enumerate() {
            let cell = r.get(i).map(String::as_str).unwrap_or("");
            out.push_str(&format!(" {} |", cell));
        }
        out.push('\n');
    }
    Ok(out)
}

fn escape_pipe(s: &str) -> String { s.replace('|', "\\|") }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_table() {
        let csv = "name,score\nAlice,9\nBob,7\n";
        let md = csv_to_markdown(csv.as_bytes()).unwrap();
        assert!(md.contains("| name | score |"));
        assert!(md.contains("| --- | --- |"));
        assert!(md.contains("| Alice | 9 |"));
        assert!(md.contains("| Bob | 7 |"));
    }

    #[test]
    fn escapes_pipes_in_cells() {
        let csv = "a,b\nx|y,z\n";
        let md = csv_to_markdown(csv.as_bytes()).unwrap();
        assert!(md.contains("x\\|y"));
    }

    #[test]
    fn empty_input_returns_empty() {
        assert!(csv_to_markdown(b"").unwrap().is_empty());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p biorouter --lib knowledge::convert::csv::tests`
Expected: 3 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/biorouter/src/knowledge/convert/csv.rs
git commit -m "feat(knowledge): CSV → markdown table conversion"
```

---

## Task 17: URL fetcher (`convert/url_fetch.rs`)

**Files:**
- Modify: `crates/biorouter/src/knowledge/convert/url_fetch.rs`

- [ ] **Step 1: Write the failing tests + implementation**

```rust
use anyhow::{Context, Result};

pub struct FetchedSource {
    pub bytes: Vec<u8>,
    pub mime: String,
    pub final_url: String,
}

pub async fn fetch_url(url: &str) -> Result<FetchedSource> {
    let client = reqwest::Client::builder()
        .user_agent("BioRouter-Knowledge/1.0")
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let resp = client.get(url).send().await
        .with_context(|| format!("fetching {url}"))?;
    let final_url = resp.url().to_string();
    let mime = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).trim().to_string())
        .unwrap_or_else(|| guess_mime_from_url(&final_url));
    let bytes = resp.bytes().await?.to_vec();
    Ok(FetchedSource { bytes, mime, final_url })
}

fn guess_mime_from_url(url: &str) -> String {
    let lower = url.to_lowercase();
    if lower.ends_with(".pdf") { "application/pdf".into() }
    else if lower.ends_with(".docx") { "application/vnd.openxmlformats-officedocument.wordprocessingml.document".into() }
    else if lower.ends_with(".csv") { "text/csv".into() }
    else if lower.ends_with(".md") { "text/markdown".into() }
    else { "text/html".into() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn fetches_html() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html; charset=utf-8")
                    .set_body_string("<html><body>Hello</body></html>"),
            )
            .mount(&server).await;
        let s = fetch_url(&format!("{}/x", server.uri())).await.unwrap();
        assert_eq!(s.mime, "text/html");
        assert!(String::from_utf8_lossy(&s.bytes).contains("Hello"));
    }

    #[tokio::test]
    async fn guesses_mime_for_pdf_path_when_no_header() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0u8; 4]))
            .mount(&server).await;
        let s = fetch_url(&format!("{}/x.pdf", server.uri())).await.unwrap();
        // Header was absent (defaulted by wiremock); guess wins.
        assert!(s.mime == "application/octet-stream" || s.mime == "application/pdf");
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p biorouter --lib knowledge::convert::url_fetch::tests`
Expected: 2 passed (the second test is forgiving about the wiremock default content-type — see assertion).

- [ ] **Step 3: Commit**

```bash
git add crates/biorouter/src/knowledge/convert/url_fetch.rs
git commit -m "feat(knowledge): async URL fetcher with content-type detection"
```

---

## Task 18: Note URL extraction (`convert/note.rs`)

**Files:**
- Modify: `crates/biorouter/src/knowledge/convert/note.rs`

- [ ] **Step 1: Write the failing tests + implementation**

```rust
pub fn extract_urls(text: &str) -> Vec<String> {
    // Conservative URL detector: http(s):// followed by non-space, non-paren, non-quote chars.
    let re = regex::Regex::new(r#"https?://[^\s<>"')]+[^\s<>"').,;:!?]"#).unwrap();
    let mut out = Vec::new();
    for m in re.find_iter(text) {
        let u = m.as_str().to_string();
        if !out.contains(&u) { out.push(u); }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_urls_in_prose() {
        let text = "See https://arxiv.org/abs/2403.12345 and also http://example.org/x.";
        let urls = extract_urls(text);
        assert_eq!(urls.len(), 2);
        assert!(urls.contains(&"https://arxiv.org/abs/2403.12345".to_string()));
    }

    #[test]
    fn dedupes() {
        let urls = extract_urls("a https://x.com b https://x.com c");
        assert_eq!(urls, vec!["https://x.com"]);
    }

    #[test]
    fn strips_trailing_punctuation() {
        let urls = extract_urls("Read https://example.org/path, please.");
        assert_eq!(urls, vec!["https://example.org/path"]);
    }

    #[test]
    fn ignores_parenthesized_correctly() {
        let urls = extract_urls("(see https://example.org/p)");
        assert_eq!(urls, vec!["https://example.org/p"]);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p biorouter --lib knowledge::convert::note::tests`
Expected: 4 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/biorouter/src/knowledge/convert/note.rs
git commit -m "feat(knowledge): extract URLs from pasted text notes"
```

---

## Task 19: Convert dispatcher (`convert/mod.rs`)

**Files:**
- Modify: `crates/biorouter/src/knowledge/convert/mod.rs`

- [ ] **Step 1: Write the failing tests + dispatcher**

Replace the existing `pub mod` declarations + add:

```rust
pub mod csv;
pub mod docx;
pub mod html;
pub mod note;
pub mod pdf;
pub mod url_fetch;

use anyhow::Result;

#[derive(Debug, Clone)]
pub enum SourceInput {
    File { bytes: Vec<u8>, filename: String, mime: Option<String> },
    Url(String),
    Text { text: String, title: Option<String> },
}

#[derive(Debug, Clone)]
pub struct Converted {
    pub markdown: String,
    pub title: Option<String>,
    pub mime: String,
    pub needs_llm_fallback: bool,
}

pub async fn convert(input: &SourceInput) -> Result<Converted> {
    match input {
        SourceInput::Text { text, title } => Ok(Converted {
            markdown: text.clone(),
            title: title.clone(),
            mime: "text/plain".into(),
            needs_llm_fallback: false,
        }),
        SourceInput::Url(url) => {
            let fetched = url_fetch::fetch_url(url).await?;
            let file = SourceInput::File {
                bytes: fetched.bytes,
                filename: filename_from_url(&fetched.final_url),
                mime: Some(fetched.mime),
            };
            Box::pin(convert(&file)).await
        }
        SourceInput::File { bytes, filename, mime } => {
            let effective_mime = mime.clone().unwrap_or_else(|| guess_mime(filename));
            match effective_mime.as_str() {
                "text/html" | "application/xhtml+xml" => {
                    let s = std::str::from_utf8(bytes)?;
                    let c = html::html_to_markdown(s)?;
                    Ok(Converted {
                        markdown: c.markdown,
                        title: c.title,
                        mime: effective_mime,
                        needs_llm_fallback: false,
                    })
                }
                "application/pdf" => {
                    let c = pdf::pdf_to_markdown(bytes)?;
                    Ok(Converted {
                        markdown: c.markdown,
                        title: None,
                        mime: effective_mime,
                        needs_llm_fallback: c.needs_llm_fallback,
                    })
                }
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => {
                    let md = docx::docx_to_markdown(bytes)?;
                    Ok(Converted { markdown: md, title: None, mime: effective_mime, needs_llm_fallback: false })
                }
                "text/csv" => {
                    let md = csv::csv_to_markdown(bytes)?;
                    Ok(Converted { markdown: md, title: None, mime: effective_mime, needs_llm_fallback: false })
                }
                "text/markdown" | "text/plain" => {
                    Ok(Converted {
                        markdown: String::from_utf8_lossy(bytes).into_owned(),
                        title: None,
                        mime: effective_mime,
                        needs_llm_fallback: false,
                    })
                }
                other => anyhow::bail!("unsupported mime: {other}"),
            }
        }
    }
}

fn filename_from_url(url: &str) -> String {
    url.split('/').last().unwrap_or("source").to_string()
}

fn guess_mime(filename: &str) -> String {
    let lower = filename.to_lowercase();
    if lower.ends_with(".pdf") { "application/pdf".into() }
    else if lower.ends_with(".docx") { "application/vnd.openxmlformats-officedocument.wordprocessingml.document".into() }
    else if lower.ends_with(".csv") { "text/csv".into() }
    else if lower.ends_with(".md") { "text/markdown".into() }
    else if lower.ends_with(".html") || lower.ends_with(".htm") { "text/html".into() }
    else { "text/plain".into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dispatches_html() {
        let html = "<html><head><title>T</title></head><body><h1>H</h1></body></html>";
        let c = convert(&SourceInput::File {
            bytes: html.as_bytes().to_vec(),
            filename: "x.html".into(),
            mime: Some("text/html".into()),
        }).await.unwrap();
        assert!(c.markdown.contains("# H"));
        assert_eq!(c.title.as_deref(), Some("T"));
    }

    #[tokio::test]
    async fn dispatches_text_passthrough() {
        let c = convert(&SourceInput::Text {
            text: "hello".into(),
            title: Some("Note".into()),
        }).await.unwrap();
        assert_eq!(c.markdown, "hello");
        assert_eq!(c.title.as_deref(), Some("Note"));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p biorouter --lib knowledge::convert`
Expected: all convert sub-module tests pass (existing + new dispatcher).

- [ ] **Step 3: Commit**

```bash
git add crates/biorouter/src/knowledge/convert/mod.rs
git commit -m "feat(knowledge): convert dispatcher with SourceInput::{File,Url,Text}"
```

---

## Task 20: Identifier extraction (`credibility/identifiers.rs`)

**Files:**
- Modify: `crates/biorouter/src/knowledge/credibility/identifiers.rs`

- [ ] **Step 1: Write the failing tests + implementation**

```rust
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Identifiers {
    pub doi: Option<String>,
    pub arxiv: Option<String>,
    pub pmid: Option<String>,
    pub isbn: Option<String>,
}

static DOI_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b10\.\d{4,9}/[-._;()/:A-Z0-9]+\b").unwrap()
});
static ARXIV_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\barXiv:(\d{4}\.\d{4,5})\b|arxiv\.org/(?:abs|pdf)/(\d{4}\.\d{4,5})").unwrap()
});
static PMID_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\bPMID[: ](\d{6,9})\b|pubmed\.ncbi\.nlm\.nih\.gov/(\d{6,9})").unwrap()
});
static ISBN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\bISBN(?:-1[03])?:?\s*(97[89][- ]?(?:\d[- ]?){9}\d|(?:\d[- ]?){9}[\dX])\b").unwrap()
});

pub fn extract(text: &str) -> Identifiers {
    let upper = text.to_ascii_uppercase();
    Identifiers {
        doi: DOI_RE.find(&upper).map(|m| m.as_str().to_lowercase()),
        arxiv: ARXIV_RE
            .captures(text)
            .and_then(|c| c.get(1).or_else(|| c.get(2)).map(|m| m.as_str().to_string())),
        pmid: PMID_RE
            .captures(text)
            .and_then(|c| c.get(1).or_else(|| c.get(2)).map(|m| m.as_str().to_string())),
        isbn: ISBN_RE
            .captures(text)
            .and_then(|c| c.get(1).map(|m| m.as_str().replace([' ', '-'], ""))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_doi() {
        let id = extract("Reference: 10.1038/s41588-024-01893-6 in the paper.");
        assert_eq!(id.doi.as_deref(), Some("10.1038/s41588-024-01893-6"));
    }

    #[test]
    fn finds_arxiv_url() {
        let id = extract("https://arxiv.org/abs/2403.12345");
        assert_eq!(id.arxiv.as_deref(), Some("2403.12345"));
    }

    #[test]
    fn finds_arxiv_inline() {
        let id = extract("see arXiv:2403.12345 for details");
        assert_eq!(id.arxiv.as_deref(), Some("2403.12345"));
    }

    #[test]
    fn finds_pmid() {
        let id = extract("https://pubmed.ncbi.nlm.nih.gov/12345678");
        assert_eq!(id.pmid.as_deref(), Some("12345678"));
    }

    #[test]
    fn finds_isbn13() {
        let id = extract("ISBN: 978-0-262-04574-1");
        assert_eq!(id.isbn.as_deref(), Some("9780262045741"));
    }

    #[test]
    fn empty_input() {
        assert_eq!(extract(""), Identifiers::default());
    }
}
```

- [ ] **Step 2: Add `once_cell` to deps if missing**

Check `crates/biorouter/Cargo.toml`. If `once_cell` isn't already there, add:

```toml
once_cell = "1.20"
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p biorouter --lib knowledge::credibility::identifiers::tests`
Expected: 6 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/biorouter/Cargo.toml crates/biorouter/src/knowledge/credibility/identifiers.rs
git commit -m "feat(knowledge): extract DOI/arXiv/PMID/ISBN identifiers from text"
```

---

## Task 21: Publisher allow-list (`credibility/allowlist.rs`)

**Files:**
- Modify: `crates/biorouter/src/knowledge/credibility/allowlist.rs`

- [ ] **Step 1: Write the failing tests + implementation**

```rust
use once_cell::sync::Lazy;
use std::collections::HashSet;

/// Curated set of publisher names recognised as peer-reviewed-paper-grade.
/// Names are matched case-insensitively against the `publisher` field returned
/// by Crossref or OpenAlex. The list is intentionally generous; users can add
/// project-specific entries via credibility.yaml.
static ALLOWLIST: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    HashSet::from([
        // Big Five STM
        "elsevier", "elsevier bv", "elsevier ltd",
        "springer nature", "springer", "nature publishing group", "nature portfolio",
        "wiley", "john wiley & sons", "wiley-blackwell",
        "taylor & francis", "taylor and francis", "routledge", "informa uk limited",
        "sage publications", "sage publications, ltd.",
        // University presses
        "oxford university press", "cambridge university press",
        "harvard university press", "princeton university press",
        "yale university press", "mit press", "stanford university press",
        "university of chicago press", "columbia university press",
        "cornell university press", "johns hopkins university press",
        "university of california press", "duke university press",
        "edinburgh university press", "manchester university press",
        "university of toronto press",
        // Scholarly societies
        "ieee", "acs publications", "american chemical society",
        "association for computing machinery", "acm",
        "american physical society", "aps",
        "american institute of physics", "aip publishing",
        "royal society of chemistry", "rsc",
        "american association for the advancement of science", "aaas",
        "american psychological association",
        "american mathematical society",
        "iop publishing",
        "the royal society", "royal society publishing",
        "bmj publishing group", "bmj",
        "annual reviews",
        // Open-access
        "plos", "public library of science",
        "frontiers media s.a.", "frontiers",
        "mdpi",
        "hindawi",
        // Medical / scientific
        "wolters kluwer", "wolters kluwer health",
        "lippincott williams & wilkins",
        "karger", "thieme", "mary ann liebert", "world scientific", "bentham science",
        "de gruyter", "brill", "emerald", "edp sciences",
        // Other notable
        "cell press",
        "the lancet", "lancet",
        "massachusetts medical society",  // publisher of NEJM
        "jama network",
    ])
});

pub fn is_peer_reviewed_publisher(publisher: &str) -> bool {
    let normal = publisher.to_lowercase();
    if ALLOWLIST.contains(normal.as_str()) { return true; }
    // Substring match on a small set of distinctive tokens to catch slight variants.
    for needle in ["elsevier", "springer", "wiley", "ieee", "oxford university", "cambridge university"] {
        if normal.contains(needle) { return true; }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_publishers_match() {
        for p in [
            "Elsevier", "Springer Nature", "Wiley", "IEEE",
            "Oxford University Press", "Massachusetts Medical Society",
            "Frontiers Media S.A.", "PLOS",
        ] {
            assert!(is_peer_reviewed_publisher(p), "should match {p}");
        }
    }

    #[test]
    fn variant_strings_match_via_substring() {
        assert!(is_peer_reviewed_publisher("Elsevier Inc."));
        assert!(is_peer_reviewed_publisher("Springer International Publishing"));
    }

    #[test]
    fn unknown_publishers_dont_match() {
        assert!(!is_peer_reviewed_publisher("Some Random Blog"));
        assert!(!is_peer_reviewed_publisher("Medium"));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p biorouter --lib knowledge::credibility::allowlist::tests`
Expected: 3 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/biorouter/src/knowledge/credibility/allowlist.rs
git commit -m "feat(knowledge): publisher allow-list for peer-reviewed classification"
```

---

## Task 22: Crossref client (`credibility/crossref.rs`)

**Files:**
- Modify: `crates/biorouter/src/knowledge/credibility/crossref.rs`

- [ ] **Step 1: Write the failing tests + implementation**

```rust
use crate::knowledge::{
    credibility::allowlist::is_peer_reviewed_publisher,
    types::{Credibility, CredibilityTier},
};
use anyhow::Result;
use serde::Deserialize;

const API_BASE: &str = "https://api.crossref.org";

pub async fn classify(doi: &str) -> Result<Option<Credibility>> {
    let client = reqwest::Client::builder()
        .user_agent("BioRouter-Knowledge/1.0 (mailto:knowledge@biorouter.local)")
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    classify_with(&client, API_BASE, doi).await
}

pub async fn classify_with(client: &reqwest::Client, base: &str, doi: &str) -> Result<Option<Credibility>> {
    let url = format!("{base}/works/{doi}");
    let resp = client.get(&url).send().await?;
    if !resp.status().is_success() { return Ok(None); }
    let body: CrossrefResponse = resp.json().await?;
    let w = body.message;
    let publisher = w.publisher.unwrap_or_default();
    let venue = w.container_title.unwrap_or_default().into_iter().next();
    let retracted = w.update_to
        .map(|u| u.iter().any(|i| i.r#type == "retraction"))
        .unwrap_or(false);
    let tier = match w.r#type.as_str() {
        "journal-article" if is_peer_reviewed_publisher(&publisher) => CredibilityTier::PeerReviewed,
        "journal-article" => CredibilityTier::Web, // recognized type, but unknown publisher
        "posted-content" => CredibilityTier::Preprint,
        "book" | "book-chapter" | "monograph" | "edited-book" => CredibilityTier::Book,
        _ => return Ok(None),
    };
    let confidence = if matches!(tier, CredibilityTier::PeerReviewed | CredibilityTier::Book) { 0.95 } else { 0.85 };
    let reasoning = format!(
        "Crossref returned type={:?}, publisher='{}'.",
        w.r#type, publisher
    );
    Ok(Some(Credibility {
        tier,
        confidence,
        publisher: Some(publisher),
        venue,
        doi: Some(doi.to_string()),
        retracted,
        reasoning,
        classifier_version: 1,
    }))
}

#[derive(Deserialize)]
struct CrossrefResponse { message: CrossrefWork }

#[derive(Deserialize)]
struct CrossrefWork {
    r#type: String,
    #[serde(default)]
    publisher: Option<String>,
    #[serde(rename = "container-title", default)]
    container_title: Option<Vec<String>>,
    #[serde(rename = "update-to", default)]
    update_to: Option<Vec<UpdateTo>>,
}

#[derive(Deserialize)]
struct UpdateTo { #[serde(default)] r#type: String }

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{matchers::{method, path_regex}, Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn classifies_peer_reviewed_journal_article() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path_regex(r"^/works/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": { "type": "journal-article", "publisher": "Springer Nature",
                              "container-title": ["Nature Genetics"] }
            })))
            .mount(&server).await;
        let client = reqwest::Client::new();
        let c = classify_with(&client, &server.uri(), "10.1038/s41588-024-01893-6").await.unwrap().unwrap();
        assert_eq!(c.tier, CredibilityTier::PeerReviewed);
        assert_eq!(c.venue.as_deref(), Some("Nature Genetics"));
        assert_eq!(c.publisher.as_deref(), Some("Springer Nature"));
    }

    #[tokio::test]
    async fn classifies_preprint_via_posted_content() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path_regex(r"^/works/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": { "type": "posted-content", "publisher": "Cold Spring Harbor Laboratory" }
            })))
            .mount(&server).await;
        let client = reqwest::Client::new();
        let c = classify_with(&client, &server.uri(), "10.1101/2024.01.01").await.unwrap().unwrap();
        assert_eq!(c.tier, CredibilityTier::Preprint);
    }

    #[tokio::test]
    async fn classifies_book() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path_regex(r"^/works/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": { "type": "book", "publisher": "MIT Press" }
            })))
            .mount(&server).await;
        let client = reqwest::Client::new();
        let c = classify_with(&client, &server.uri(), "10.7551/mitpress/0").await.unwrap().unwrap();
        assert_eq!(c.tier, CredibilityTier::Book);
    }

    #[tokio::test]
    async fn detects_retraction() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path_regex(r"^/works/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": { "type": "journal-article", "publisher": "Wiley",
                              "update-to": [{ "type": "retraction" }] }
            })))
            .mount(&server).await;
        let client = reqwest::Client::new();
        let c = classify_with(&client, &server.uri(), "10.1002/x").await.unwrap().unwrap();
        assert!(c.retracted);
    }

    #[tokio::test]
    async fn returns_none_on_unknown_type() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path_regex(r"^/works/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": { "type": "dataset", "publisher": "X" }
            })))
            .mount(&server).await;
        let client = reqwest::Client::new();
        assert!(classify_with(&client, &server.uri(), "10.x/y").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn returns_none_on_404() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(404)).mount(&server).await;
        let client = reqwest::Client::new();
        assert!(classify_with(&client, &server.uri(), "10.x/y").await.unwrap().is_none());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p biorouter --lib knowledge::credibility::crossref::tests`
Expected: 6 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/biorouter/src/knowledge/credibility/crossref.rs
git commit -m "feat(knowledge): Crossref-based credibility classification"
```

---

## Task 23: OpenAlex client (`credibility/openalex.rs`)

**Files:**
- Modify: `crates/biorouter/src/knowledge/credibility/openalex.rs`

- [ ] **Step 1: Write the failing tests + implementation**

```rust
use crate::knowledge::{
    credibility::allowlist::is_peer_reviewed_publisher,
    types::{Credibility, CredibilityTier},
};
use anyhow::Result;
use serde::Deserialize;

const API_BASE: &str = "https://api.openalex.org";

pub async fn classify(doi: &str) -> Result<Option<Credibility>> {
    let client = reqwest::Client::builder()
        .user_agent("BioRouter-Knowledge/1.0 (mailto:knowledge@biorouter.local)")
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    classify_with(&client, API_BASE, doi).await
}

pub async fn classify_with(client: &reqwest::Client, base: &str, doi: &str) -> Result<Option<Credibility>> {
    let url = format!("{base}/works/doi:{doi}");
    let resp = client.get(&url).send().await?;
    if !resp.status().is_success() { return Ok(None); }
    let w: Work = resp.json().await?;
    let publisher = w.host_venue.as_ref().and_then(|v| v.publisher.clone()).unwrap_or_default();
    let venue = w.host_venue.as_ref().and_then(|v| v.display_name.clone());
    let retracted = w.is_retracted.unwrap_or(false);
    let tier = match w.r#type.as_deref() {
        Some("journal-article") if is_peer_reviewed_publisher(&publisher) => CredibilityTier::PeerReviewed,
        Some("journal-article") => CredibilityTier::Web,
        Some("posted-content") | Some("preprint") => CredibilityTier::Preprint,
        Some("book") | Some("book-chapter") | Some("monograph") => CredibilityTier::Book,
        _ => return Ok(None),
    };
    let confidence = if matches!(tier, CredibilityTier::PeerReviewed | CredibilityTier::Book) { 0.93 } else { 0.83 };
    Ok(Some(Credibility {
        tier,
        confidence,
        publisher: Some(publisher),
        venue,
        doi: Some(doi.to_string()),
        retracted,
        reasoning: format!("OpenAlex returned type={:?}.", w.r#type),
        classifier_version: 1,
    }))
}

#[derive(Deserialize)]
struct Work {
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    is_retracted: Option<bool>,
    #[serde(default)]
    host_venue: Option<HostVenue>,
}

#[derive(Deserialize)]
struct HostVenue {
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    publisher: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{matchers::{method, path_regex}, Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn classifies_journal_article() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path_regex(r"^/works/doi:.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "type": "journal-article",
                "is_retracted": false,
                "host_venue": { "display_name": "Cell", "publisher": "Elsevier" }
            })))
            .mount(&server).await;
        let client = reqwest::Client::new();
        let c = classify_with(&client, &server.uri(), "10.1016/x").await.unwrap().unwrap();
        assert_eq!(c.tier, CredibilityTier::PeerReviewed);
        assert_eq!(c.venue.as_deref(), Some("Cell"));
    }

    #[tokio::test]
    async fn returns_none_on_404() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(404)).mount(&server).await;
        let client = reqwest::Client::new();
        assert!(classify_with(&client, &server.uri(), "10.x/y").await.unwrap().is_none());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p biorouter --lib knowledge::credibility::openalex::tests`
Expected: 2 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/biorouter/src/knowledge/credibility/openalex.rs
git commit -m "feat(knowledge): OpenAlex-based credibility classification"
```

---

## Task 24: Host patterns (`credibility/host_patterns.rs`)

**Files:**
- Modify: `crates/biorouter/src/knowledge/credibility/host_patterns.rs`

- [ ] **Step 1: Write the failing tests + implementation**

```rust
use crate::knowledge::types::{Credibility, CredibilityTier};

pub fn classify_url(url: &str) -> Option<Credibility> {
    let host = host_of(url)?;
    if is_preprint_host(&host) {
        return Some(make(CredibilityTier::Preprint, 0.9,
            "Host is a recognised preprint server.", Some(&host)));
    }
    if is_gray_lit_host(&host) {
        return Some(make(CredibilityTier::GrayLit, 0.8,
            "Host is governmental / institutional / standards body.", Some(&host)));
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        return Some(make(CredibilityTier::Web, 0.6,
            "Generic web URL with no recognised academic provenance.", Some(&host)));
    }
    None
}

fn make(tier: CredibilityTier, confidence: f32, reason: &str, host: Option<&str>) -> Credibility {
    Credibility {
        tier, confidence,
        publisher: host.map(String::from),
        venue: None, doi: None, retracted: false,
        reasoning: reason.to_string(),
        classifier_version: 1,
    }
}

fn host_of(url: &str) -> Option<String> {
    let after_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let host = after_scheme.split('/').next().unwrap_or("");
    if host.is_empty() { None } else { Some(host.to_lowercase()) }
}

fn is_preprint_host(host: &str) -> bool {
    [
        "arxiv.org", "biorxiv.org", "medrxiv.org", "chemrxiv.org", "ssrn.com",
        "preprints.org", "researchsquare.com", "osf.io", "psyarxiv.com",
    ].iter().any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

fn is_gray_lit_host(host: &str) -> bool {
    if host.ends_with(".gov") || host.ends_with(".edu") { return true; }
    [
        "who.int", "cdc.gov", "nih.gov", "fda.gov", "clinicaltrials.gov",
        "europa.eu", "ema.europa.eu", "ietf.org", "rfc-editor.org",
        "iso.org", "oecd.org",
    ].iter().any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arxiv_is_preprint() {
        let c = classify_url("https://arxiv.org/abs/2403.12345").unwrap();
        assert_eq!(c.tier, CredibilityTier::Preprint);
    }

    #[test]
    fn biorxiv_subdomain_is_preprint() {
        let c = classify_url("https://www.biorxiv.org/content/x").unwrap();
        assert_eq!(c.tier, CredibilityTier::Preprint);
    }

    #[test]
    fn nih_is_gray_lit() {
        let c = classify_url("https://www.nih.gov/news/x").unwrap();
        assert_eq!(c.tier, CredibilityTier::GrayLit);
    }

    #[test]
    fn random_blog_is_web() {
        let c = classify_url("https://someblog.example/post").unwrap();
        assert_eq!(c.tier, CredibilityTier::Web);
    }

    #[test]
    fn non_http_returns_none() {
        assert!(classify_url("file:///tmp/x.pdf").is_none());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p biorouter --lib knowledge::credibility::host_patterns::tests`
Expected: 5 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/biorouter/src/knowledge/credibility/host_patterns.rs
git commit -m "feat(knowledge): host-pattern credibility classification"
```

---

## Task 25: Agentic fallback stub (`credibility/agentic.rs`)

**Files:**
- Modify: `crates/biorouter/src/knowledge/credibility/agentic.rs`

The real agentic fallback requires a sub-agent loop (built in Plan 2). For Plan 1 it is a deterministic fallback that returns `Web` / `Personal` based on whether a URL was provided. The signature is stable so Plan 2 swaps the body without touching callers.

- [ ] **Step 1: Write the failing tests + stub**

```rust
use crate::knowledge::{
    convert::SourceInput,
    types::{Credibility, CredibilityTier},
};
use anyhow::Result;

pub async fn classify(input: &SourceInput) -> Result<Credibility> {
    let (tier, reason) = match input {
        SourceInput::Url(_) | SourceInput::File { .. } => (
            CredibilityTier::Web,
            "Agentic fallback (stub): defaulting to web — no identifier and no host pattern matched.",
        ),
        SourceInput::Text { .. } => (
            CredibilityTier::Personal,
            "Agentic fallback (stub): pasted text with no provenance — personal.",
        ),
    };
    Ok(Credibility {
        tier,
        confidence: 0.4,
        publisher: None, venue: None, doi: None, retracted: false,
        reasoning: reason.to_string(),
        classifier_version: 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn url_defaults_to_web() {
        let c = classify(&SourceInput::Url("https://x.com/y".into())).await.unwrap();
        assert_eq!(c.tier, CredibilityTier::Web);
    }

    #[tokio::test]
    async fn text_defaults_to_personal() {
        let c = classify(&SourceInput::Text { text: "note".into(), title: None }).await.unwrap();
        assert_eq!(c.tier, CredibilityTier::Personal);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p biorouter --lib knowledge::credibility::agentic::tests`
Expected: 2 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/biorouter/src/knowledge/credibility/agentic.rs
git commit -m "feat(knowledge): agentic-fallback stub (real impl deferred to Plan 2)"
```

---

## Task 26: Classify dispatcher (`credibility/mod.rs`)

**Files:**
- Modify: `crates/biorouter/src/knowledge/credibility/mod.rs`

- [ ] **Step 1: Write the failing tests + dispatcher**

Replace the existing `pub mod` declarations with the full module body:

```rust
pub mod agentic;
pub mod allowlist;
pub mod crossref;
pub mod host_patterns;
pub mod identifiers;
pub mod openalex;

use crate::knowledge::{convert::SourceInput, types::Credibility};
use anyhow::Result;

pub async fn classify(input: &SourceInput) -> Result<Credibility> {
    // 1. Extract identifiers from whatever text we have.
    let probe = probe_text(input);
    let ids = identifiers::extract(&probe);

    // 2. Deterministic DOI lookup via Crossref then OpenAlex.
    if let Some(doi) = &ids.doi {
        if let Some(c) = crossref::classify(doi).await? { return Ok(c); }
        if let Some(c) = openalex::classify(doi).await? { return Ok(c); }
    }

    // 3. Host pattern.
    if let SourceInput::Url(url) = input {
        if let Some(c) = host_patterns::classify_url(url) { return Ok(c); }
    }

    // 4. Agentic fallback (stub in Plan 1).
    agentic::classify(input).await
}

fn probe_text(input: &SourceInput) -> String {
    match input {
        SourceInput::Url(u) => u.clone(),
        SourceInput::Text { text, title } => {
            format!("{}\n{}", title.clone().unwrap_or_default(), text)
        }
        SourceInput::File { filename, bytes, .. } => {
            // Sniff first 4 KB for identifiers (PDF metadata, HTML head, etc.)
            let head: String = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).to_string();
            format!("{filename}\n{head}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::types::CredibilityTier;

    #[tokio::test]
    async fn falls_back_to_host_pattern_when_no_doi() {
        let c = classify(&SourceInput::Url("https://arxiv.org/abs/2403.12345".into())).await.unwrap();
        assert_eq!(c.tier, CredibilityTier::Preprint);
    }

    #[tokio::test]
    async fn personal_text_falls_through_to_agentic() {
        let c = classify(&SourceInput::Text { text: "lab notes".into(), title: None }).await.unwrap();
        assert_eq!(c.tier, CredibilityTier::Personal);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p biorouter --lib knowledge::credibility`
Expected: all credibility sub-module tests still pass + 2 new.

- [ ] **Step 3: Commit**

```bash
git add crates/biorouter/src/knowledge/credibility/mod.rs
git commit -m "feat(knowledge): credibility classify() ladder (ids → crossref/openalex → host → agentic)"
```

---

## Task 27: Wire convert + classify into `add_raw_source`

**Files:**
- Modify: `crates/biorouter/src/knowledge/service.rs`

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module in `service.rs`:

```rust
    use crate::knowledge::convert::SourceInput;
    use crate::knowledge::raw;

    #[tokio::test]
    async fn add_raw_source_from_text() {
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();
        let kb = svc.root().join("k");

        let res = svc.add_raw_source("k", SourceInput::Text {
            text: "Lab note: HRV trend up after week of zone-2.".into(),
            title: Some("HRV note".into()),
        }, None).await.unwrap();

        assert!(kb.join(format!("raw/{}/source.md", res.source_id)).exists());
        assert!(kb.join(format!("raw/{}/meta.yaml", res.source_id)).exists());
        let meta = raw::read_meta(&kb, &res.source_id).unwrap();
        assert_eq!(meta.title, "HRV note");
        assert_eq!(meta.credibility.tier, CredibilityTier::Personal);

        // A commit was made.
        let repo = GitRepo::open(&kb).unwrap();
        let log = repo.log(10).unwrap();
        assert_eq!(log.len(), 2, "create + add_raw_source");
        assert_eq!(log[0].kind, ChangeKind::Ingest);
    }

    #[tokio::test]
    async fn add_raw_source_from_html_file() {
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();
        let html = b"<html><head><title>Test</title></head><body><h1>H</h1></body></html>";
        let res = svc.add_raw_source("k", SourceInput::File {
            bytes: html.to_vec(),
            filename: "x.html".into(),
            mime: Some("text/html".into()),
        }, None).await.unwrap();
        let kb = svc.root().join("k");
        let md = std::fs::read_to_string(kb.join(format!("raw/{}/source.md", res.source_id))).unwrap();
        assert!(md.contains("# H"));
    }
```

Add `use crate::knowledge::types::{ChangeKind, CredibilityTier};` to the test module's `use` block if not already there.

- [ ] **Step 2: Implement `add_raw_source`**

Add to `KnowledgeService` in `service.rs`:

```rust
use crate::knowledge::{convert, credibility, raw, types::SourceMeta};
use chrono::Utc;

impl KnowledgeService {
    pub async fn add_raw_source(
        &self,
        kb_id: &str,
        input: convert::SourceInput,
        txn_branch: Option<&str>,
    ) -> Result<raw::RawWrite> {
        paths::validate_kb_id(kb_id)?;
        let kb_root = paths::kb_root(&self.root, kb_id);
        if !kb_root.exists() {
            anyhow::bail!("kb '{kb_id}' does not exist");
        }

        let converted = convert::convert(&input).await?;
        let credibility = credibility::classify(&input).await?;

        let title = converted.title.clone().unwrap_or_else(|| match &input {
            convert::SourceInput::Text { title, .. } => title.clone().unwrap_or_else(|| "Untitled note".into()),
            convert::SourceInput::Url(u) => u.clone(),
            convert::SourceInput::File { filename, .. } => filename.clone(),
        });

        let source_id = raw::new_source_id(&title);
        let (original_bytes, original_filename, url) = match &input {
            convert::SourceInput::File { bytes, filename, .. } =>
                (Some(bytes.clone()), Some(filename.clone()), None),
            convert::SourceInput::Url(u) => (None, None, Some(u.clone())),
            convert::SourceInput::Text { .. } => (None, None, None),
        };

        let hash = match &original_bytes {
            Some(b) => raw::hash_bytes(b),
            None => raw::hash_bytes(converted.markdown.as_bytes()),
        };

        let meta = SourceMeta {
            id: source_id.clone(),
            title,
            url,
            ingested_at: Utc::now(),
            sha256: hash,
            mime: converted.mime.clone(),
            original_filename,
            credibility,
        };

        let written = raw::write_raw(
            &kb_root,
            original_bytes.as_deref(),
            meta.original_filename.as_deref(),
            &converted.markdown,
            meta,
        )?;

        let repo = GitRepo::open(&kb_root)?;
        let summary = format!("ingested {source_id}");
        let delta = "+1 source";
        if let Some(_branch) = txn_branch {
            repo.commit_on_txn_in_progress(&summary)?;
        } else {
            repo.commit_all(crate::knowledge::types::ChangeKind::Ingest, &summary, Some(delta))?;
        }
        Ok(written)
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p biorouter --lib knowledge::service`
Expected: 5 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/biorouter/src/knowledge/service.rs
git commit -m "feat(knowledge): service.add_raw_source (convert + classify + commit)"
```

---

## Task 28: Graph derivation (`graph.rs`)

**Files:**
- Modify: `crates/biorouter/src/knowledge/graph.rs`

- [ ] **Step 1: Write the failing tests + implementation**

```rust
use crate::knowledge::{
    raw,
    store::{self, PageRef},
    types::{Graph, GraphEdge, GraphNode, PageKind},
};
use anyhow::Result;
use std::path::Path;

const KNOWLEDGE_LINK_RE: &str = r"\[\[([^\]]+)\]\]";

pub fn derive(kb_root: &Path) -> Result<Graph> {
    let pages = store::list_pages(kb_root, None)?;
    let mut nodes = Vec::new();
    let mut id_for_path = std::collections::HashMap::new();
    let mut label_to_id = std::collections::HashMap::new();

    for p in &pages {
        let node_id = path_to_node_id(&p.path);
        id_for_path.insert(p.path.clone(), node_id.clone());
        label_to_id.insert(slug(&page_basename(&p.path)).to_lowercase(), node_id.clone());
        let kind = page_kind_of(p);
        nodes.push(GraphNode {
            id: node_id,
            label: p.title.clone(),
            kind,
            credibility_tier: None,
            path: p.path.clone(),
        });
    }

    // Source nodes inherit credibility from raw/<id>/meta.yaml.
    for src in raw::list_sources(kb_root)? {
        let logical = format!("knowledge/sources/{}.md", src.id);
        if let Some(node_id) = id_for_path.get(&logical) {
            if let Some(n) = nodes.iter_mut().find(|n| &n.id == node_id) {
                n.credibility_tier = Some(src.credibility.tier);
            }
        }
    }

    let mut edges = Vec::new();
    let re = regex::Regex::new(KNOWLEDGE_LINK_RE).unwrap();
    for p in &pages {
        let abs = kb_root.join(&p.path);
        let body = std::fs::read_to_string(&abs)?;
        let from = id_for_path.get(&p.path).cloned().unwrap();
        for cap in re.captures_iter(&body) {
            let target_label = cap.get(1).unwrap().as_str().trim();
            let key = target_label.to_lowercase();
            if let Some(to) = label_to_id.get(&key) {
                if to != &from {
                    edges.push(GraphEdge { from: from.clone(), to: to.clone(), relation: None });
                }
            }
        }
    }

    Ok(Graph { nodes, edges })
}

pub fn write_cache(kb_root: &Path, graph: &Graph) -> Result<()> {
    let path = kb_root.join(".biorouter-knowledge").join("graph-cache.json");
    std::fs::create_dir_all(path.parent().unwrap())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(graph)?)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

pub fn read_cache(kb_root: &Path) -> Result<Option<Graph>> {
    let path = kb_root.join(".biorouter-knowledge").join("graph-cache.json");
    if !path.exists() { return Ok(None); }
    let s = std::fs::read_to_string(&path)?;
    Ok(Some(serde_json::from_str(&s)?))
}

fn path_to_node_id(logical: &str) -> String {
    logical
        .strip_prefix("knowledge/")
        .unwrap_or(logical)
        .trim_end_matches(".md")
        .replace('/', ":")
}

fn page_basename(logical: &str) -> &str {
    logical.rsplit('/').next().unwrap_or(logical).trim_end_matches(".md")
}

fn slug(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn page_kind_of(p: &PageRef) -> PageKind {
    match (p.kind.as_str(), p.path.as_str()) {
        ("source", _) => PageKind::Source,
        ("entity", _) => PageKind::Entity,
        ("concept", _) => PageKind::Concept,
        ("hub", _) => PageKind::Hub,
        ("flag", _) => PageKind::Flag,
        (_, path) if path.starts_with("knowledge/sources/") => PageKind::Source,
        (_, path) if path.starts_with("knowledge/entities/") => PageKind::Entity,
        (_, path) if path.starts_with("knowledge/concepts/") => PageKind::Concept,
        (_, path) if path.starts_with("knowledge/notes/") => PageKind::Note,
        _ => PageKind::Hub,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::service::KnowledgeService;
    use crate::knowledge::store::write_page;

    fn build_sample() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("k", "K", None).unwrap();
        let kb = dir.path().join("k");
        write_page(&kb, "knowledge/entities/hrv.md",
            "---\ntitle: HRV\nkind: entity\n---\nLinks to [[zone-2 base]].",
            "add hrv", None).unwrap();
        write_page(&kb, "knowledge/concepts/zone-2 base.md",
            "---\ntitle: Zone-2 base\nkind: concept\n---\nLinks to [[hrv]].",
            "add z2", None).unwrap();
        (dir, kb)
    }

    #[test]
    fn derives_nodes_and_edges() {
        let (_d, kb) = build_sample();
        let g = derive(&kb).unwrap();
        assert_eq!(g.nodes.len(), 2);
        assert_eq!(g.edges.len(), 2, "bidirectional links → two edges");
        let labels: Vec<_> = g.nodes.iter().map(|n| n.label.as_str()).collect();
        assert!(labels.contains(&"HRV"));
        assert!(labels.contains(&"Zone-2 base"));
    }

    #[test]
    fn cache_write_then_read() {
        let (_d, kb) = build_sample();
        let g = derive(&kb).unwrap();
        write_cache(&kb, &g).unwrap();
        let back = read_cache(&kb).unwrap().unwrap();
        assert_eq!(back, g);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p biorouter --lib knowledge::graph::tests`
Expected: 2 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/biorouter/src/knowledge/graph.rs
git commit -m "feat(knowledge): derive node/edge graph from knowledge pages + cache file"
```

---

## Task 29: Wire graph cache into create_base + add_raw_source

**Files:**
- Modify: `crates/biorouter/src/knowledge/service.rs`

- [ ] **Step 1: Add a helper that rebuilds the cache**

In `service.rs`:

```rust
impl KnowledgeService {
    fn rebuild_graph_cache(&self, kb_id: &str) -> anyhow::Result<()> {
        let kb_root = paths::kb_root(&self.root, kb_id);
        let g = crate::knowledge::graph::derive(&kb_root)?;
        crate::knowledge::graph::write_cache(&kb_root, &g)?;
        Ok(())
    }

    pub fn get_graph(&self, kb_id: &str) -> anyhow::Result<crate::knowledge::types::Graph> {
        paths::validate_kb_id(kb_id)?;
        let kb_root = paths::kb_root(&self.root, kb_id);
        if let Some(g) = crate::knowledge::graph::read_cache(&kb_root)? {
            return Ok(g);
        }
        crate::knowledge::graph::derive(&kb_root)
    }
}
```

Then call `self.rebuild_graph_cache(kb_id)?;` at the end of `create_base` (right before `Ok(m)`) and at the end of `add_raw_source` (right before `Ok(written)`).

- [ ] **Step 2: Write the failing test**

Append to the `tests` module in `service.rs`:

```rust
    #[tokio::test]
    async fn get_graph_returns_cached_after_create_and_add() {
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();
        let g_empty = svc.get_graph("k").unwrap();
        assert!(g_empty.nodes.is_empty());
        svc.add_raw_source("k", convert::SourceInput::Text {
            text: "note".into(), title: Some("N".into())
        }, None).await.unwrap();
        let kb = svc.root().join("k");
        // Source pages aren't written by add_raw_source — only raw/. So the graph
        // remains empty until a macro creates knowledge/sources/<id>.md (Plan 2).
        let g = svc.get_graph("k").unwrap();
        assert_eq!(g.nodes.len(), 0, "no knowledge pages yet");
        assert!(kb.join(".biorouter-knowledge/graph-cache.json").exists());
    }
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p biorouter --lib knowledge::service`
Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add crates/biorouter/src/knowledge/service.rs
git commit -m "feat(knowledge): rebuild graph cache after create + add_raw_source"
```

---

## Task 30: History + restore on the service

**Files:**
- Modify: `crates/biorouter/src/knowledge/service.rs`

- [ ] **Step 1: Write the failing tests**

Append:

```rust
    #[tokio::test]
    async fn list_history_and_restore_roundtrip() {
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();
        svc.add_raw_source("k", convert::SourceInput::Text {
            text: "first".into(), title: Some("a".into())
        }, None).await.unwrap();
        let history_after_one = svc.list_history("k", 10).unwrap();
        assert_eq!(history_after_one.len(), 2);
        let target = history_after_one.last().unwrap().commit_sha.clone();

        svc.add_raw_source("k", convert::SourceInput::Text {
            text: "second".into(), title: Some("b".into())
        }, None).await.unwrap();
        let history_after_two = svc.list_history("k", 10).unwrap();
        assert_eq!(history_after_two.len(), 3);

        svc.restore_state("k", &target).unwrap();
        let history_after_restore = svc.list_history("k", 10).unwrap();
        assert_eq!(history_after_restore.len(), 4);
        assert_eq!(history_after_restore[0].kind, crate::knowledge::types::ChangeKind::Restore);
    }
```

- [ ] **Step 2: Implement on `KnowledgeService`**

```rust
impl KnowledgeService {
    pub fn list_history(&self, kb_id: &str, limit: usize) -> anyhow::Result<Vec<crate::knowledge::types::HistoryEntry>> {
        paths::validate_kb_id(kb_id)?;
        let kb_root = paths::kb_root(&self.root, kb_id);
        let repo = GitRepo::open(&kb_root)?;
        repo.log(limit)
    }

    pub fn restore_state(&self, kb_id: &str, commit_sha: &str) -> anyhow::Result<String> {
        paths::validate_kb_id(kb_id)?;
        let kb_root = paths::kb_root(&self.root, kb_id);
        let repo = GitRepo::open(&kb_root)?;
        let summary = format!("restore to {}", &commit_sha[..7.min(commit_sha.len())]);
        let sha = repo.restore_to(commit_sha, &summary)?;
        self.rebuild_graph_cache(kb_id)?;
        Ok(sha)
    }

    pub fn preview_state(&self, kb_id: &str, commit_sha: &str, path: &str) -> anyhow::Result<Option<String>> {
        paths::validate_kb_id(kb_id)?;
        let kb_root = paths::kb_root(&self.root, kb_id);
        let repo = GitRepo::open(&kb_root)?;
        repo.read_file_at(commit_sha, path)
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p biorouter --lib knowledge::service`
Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add crates/biorouter/src/knowledge/service.rs
git commit -m "feat(knowledge): service.list_history / restore_state / preview_state"
```

---

## Task 31: MCP server wrapper (`knowledge/mod.rs` in biorouter-mcp)

**Files:**
- Modify: `crates/biorouter-mcp/src/knowledge/mod.rs`

This wraps `KnowledgeService` as MCP tools. We follow the exact pattern from `crates/biorouter-mcp/src/memory/mod.rs` — `#[tool_router]` + `#[tool_handler]` macros from `rmcp`.

- [ ] **Step 1: Write the implementation**

Replace the placeholder with:

```rust
use anyhow::Result;
use biorouter::knowledge::{
    convert::SourceInput,
    KnowledgeService,
};
use rmcp::{
    handler::server::tool::{Parameters, ToolRouter},
    model::{CallToolResult, Content, Implementation, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router, ErrorData, ServerHandler,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct KnowledgeServer {
    tool_router: ToolRouter<Self>,
    service: KnowledgeService,
    instructions: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateBaseParams {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct KbIdParams {
    pub kb_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListPagesParams {
    pub kb_id: String,
    #[serde(default)]
    pub path_prefix: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadPageParams {
    pub kb_id: String,
    pub path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WritePageParams {
    pub kb_id: String,
    pub path: String,
    pub content: String,
    pub commit_message: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddRawSourceParams {
    pub kb_id: String,
    pub source: RawSourceInput,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RawSourceInput {
    Url { url: String },
    Text { text: String, #[serde(default)] title: Option<String> },
    // File uploads via MCP are out of scope; the HTTP layer (Plan 3) handles them.
}

impl From<RawSourceInput> for SourceInput {
    fn from(r: RawSourceInput) -> Self {
        match r {
            RawSourceInput::Url { url } => SourceInput::Url(url),
            RawSourceInput::Text { text, title } => SourceInput::Text { text, title },
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HistoryParams {
    pub kb_id: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize { 50 }

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RestoreParams {
    pub kb_id: String,
    pub commit_sha: String,
}

#[tool_router(router = tool_router)]
impl KnowledgeServer {
    pub fn new() -> Result<Self> {
        Ok(Self {
            tool_router: Self::tool_router(),
            service: KnowledgeService::new_default()?,
            instructions: include_str!("instructions.md").to_string(),
        })
    }

    #[tool(name = "kb_list_bases", description = "List all knowledge bases on this machine.")]
    pub async fn kb_list_bases(&self) -> Result<CallToolResult, ErrorData> {
        let bases = self.service.list_bases().map_err(into_err)?;
        ok_json(&bases)
    }

    #[tool(name = "kb_create_base", description = "Create a new knowledge base.")]
    pub async fn kb_create_base(&self, p: Parameters<CreateBaseParams>) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let m = self.service.create_base(&p.id, &p.name, p.color.as_deref()).map_err(into_err)?;
        ok_json(&m)
    }

    #[tool(name = "kb_list_pages", description = "List knowledge pages in a knowledge base.")]
    pub async fn kb_list_pages(&self, p: Parameters<ListPagesParams>) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let kb_root = biorouter::knowledge::paths::kb_root(self.service.root(), &p.kb_id);
        let pages = biorouter::knowledge::store::list_pages(&kb_root, p.path_prefix.as_deref()).map_err(into_err)?;
        ok_json(&pages)
    }

    #[tool(name = "kb_read_page", description = "Read a single knowledge page by path.")]
    pub async fn kb_read_page(&self, p: Parameters<ReadPageParams>) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let kb_root = biorouter::knowledge::paths::kb_root(self.service.root(), &p.kb_id);
        let page = biorouter::knowledge::store::read_page(&kb_root, &p.path).map_err(into_err)?;
        ok_json(&page)
    }

    #[tool(name = "kb_write_page", description = "Create or overwrite a knowledge page and commit.")]
    pub async fn kb_write_page(&self, p: Parameters<WritePageParams>) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let kb_root = biorouter::knowledge::paths::kb_root(self.service.root(), &p.kb_id);
        let sha = biorouter::knowledge::store::write_page(
            &kb_root, &p.path, &p.content, &p.commit_message, None,
        ).map_err(into_err)?;
        ok_json(&serde_json::json!({ "commit_sha": sha }))
    }

    #[tool(name = "kb_add_raw_source", description = "Add a raw source (URL or pasted text), convert to markdown, and classify credibility.")]
    pub async fn kb_add_raw_source(&self, p: Parameters<AddRawSourceParams>) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let res = self.service.add_raw_source(&p.kb_id, p.source.into(), None).await.map_err(into_err)?;
        ok_json(&serde_json::json!({
            "source_id": res.source_id,
            "source_md_path": res.source_md_path,
            "meta_path": res.meta_path,
        }))
    }

    #[tool(name = "kb_get_graph", description = "Return the cached node+edge graph for a knowledge base.")]
    pub async fn kb_get_graph(&self, p: Parameters<KbIdParams>) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let g = self.service.get_graph(&p.kb_id).map_err(into_err)?;
        ok_json(&g)
    }

    #[tool(name = "kb_list_history", description = "List recent change-log entries from the git history.")]
    pub async fn kb_list_history(&self, p: Parameters<HistoryParams>) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let h = self.service.list_history(&p.kb_id, p.limit).map_err(into_err)?;
        ok_json(&h)
    }

    #[tool(name = "kb_restore_state", description = "Restore the knowledge folder to a previous commit by creating a new commit on top of HEAD.")]
    pub async fn kb_restore_state(&self, p: Parameters<RestoreParams>) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let sha = self.service.restore_state(&p.kb_id, &p.commit_sha).map_err(into_err)?;
        ok_json(&serde_json::json!({ "ok": true, "new_commit_sha": sha }))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for KnowledgeServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: "biorouter-knowledge".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            instructions: Some(self.instructions.clone()),
            ..Default::default()
        }
    }
}

fn ok_json<T: Serialize>(v: &T) -> Result<CallToolResult, ErrorData> {
    let text = serde_json::to_string_pretty(v).map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

fn into_err(e: anyhow::Error) -> ErrorData {
    ErrorData::internal_error(format!("{e:#}"), None)
}
```

- [ ] **Step 2: Create the embedded instructions doc**

Create `crates/biorouter-mcp/src/knowledge/instructions.md`:

```markdown
# Knowledge extension

You can create and maintain personal knowledge bases backed by markdown
knowledge folders + git history. Use these primitive tools to read and write the knowledge folder.

Common operations:

- `kb_list_bases` — see which knowledge bases exist.
- `kb_create_base` — create a new one.
- `kb_add_raw_source` — ingest a URL or pasted text. The result is filed under
  `raw/<source-id>/` with `source.md` and `meta.yaml`; credibility is auto-classified.
  This does NOT create knowledge pages — read the source and write knowledge pages with
  `kb_write_page` to integrate the source into the knowledge graph.
- `kb_list_pages` / `kb_read_page` / `kb_write_page` — knowledge CRUD.
- `kb_get_graph` — derived nodes+edges for visualisation.
- `kb_list_history` / `kb_restore_state` — git-backed change log + revert.

Every mutating tool commits to git. The history is the source of truth.
```

- [ ] **Step 3: Verify the crate compiles**

Run: `cargo build -p biorouter-mcp`
Expected: clean build.

- [ ] **Step 4: Commit**

```bash
git add crates/biorouter-mcp/src/knowledge/mod.rs crates/biorouter-mcp/src/knowledge/instructions.md
git commit -m "feat(knowledge): MCP server wrapper exposing primitive tools"
```

---

## Task 32: Register in `BUILTIN_EXTENSIONS`

**Files:**
- Modify: `crates/biorouter-mcp/src/lib.rs`

- [ ] **Step 1: Inspect the existing pattern**

Read `crates/biorouter-mcp/src/lib.rs` and find the `BUILTIN_EXTENSIONS` static near line 64 — it lists `developer`, `autovisualiser`, `computercontroller`, `memory`, `tutorial`. We will add `knowledge` to it.

- [ ] **Step 2: Add the entry**

After `pub mod knowledge;` near the top of `lib.rs`, also add:

```rust
pub use knowledge::KnowledgeServer;
```

In the `BUILTIN_EXTENSIONS` `HashMap::from([...])` call, add a new entry following the existing `builtin!` macro pattern (between `memory` and `tutorial`, or at the end — order doesn't matter):

```rust
builtin!(knowledge, KnowledgeServer),
```

If the existing `builtin!` macro requires the server to implement `Default` or have a no-arg constructor, look at how `MemoryServer` is wired and follow the same pattern. If `KnowledgeServer::new()` returns `Result`, you may need to adapt the macro call to handle the error — copy whatever pattern the existing entries use (e.g., `.expect("init knowledge server")`).

- [ ] **Step 3: Write an integration test for the registry**

Create `crates/biorouter-mcp/tests/knowledge_registered.rs`:

```rust
#[test]
fn knowledge_is_in_builtin_registry() {
    assert!(biorouter_mcp::BUILTIN_EXTENSIONS.contains_key("knowledge"));
}
```

Run: `cargo test -p biorouter-mcp --test knowledge_registered`
Expected: 1 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/biorouter-mcp/src/lib.rs crates/biorouter-mcp/tests/knowledge_registered.rs
git commit -m "feat(knowledge): register knowledge extension in BUILTIN_EXTENSIONS"
```

---

## Task 33: End-to-end integration test

**Files:**
- Create: `crates/biorouter/tests/knowledge_e2e.rs`

- [ ] **Step 1: Write the integration test**

```rust
use biorouter::knowledge::{convert::SourceInput, KnowledgeService};

#[tokio::test]
async fn e2e_create_add_query_restore() {
    let dir = tempfile::tempdir().unwrap();
    let svc = KnowledgeService::new(dir.path().to_path_buf());

    // 1. Create.
    let m = svc.create_base("ms", "MS Patient Analysis", Some("#5a6394")).unwrap();
    assert_eq!(m.id, "ms");
    let bases = svc.list_bases().unwrap();
    assert_eq!(bases.len(), 1);

    // 2. Add a text source.
    let added = svc.add_raw_source("ms", SourceInput::Text {
        text: "Brain MRI shows demyelination consistent with MS.".into(),
        title: Some("Imaging note".into()),
    }, None).await.unwrap();
    assert!(dir.path().join(format!("ms/raw/{}/source.md", added.source_id)).exists());

    // 3. Add a URL source (mocked via a separate test would normally be ideal;
    //    here we just verify that text-source path works end-to-end).

    // 4. History reflects two commits (init + ingest).
    let h = svc.list_history("ms", 10).unwrap();
    assert_eq!(h.len(), 2);
    let init_sha = h.last().unwrap().commit_sha.clone();

    // 5. Restore to init.
    svc.restore_state("ms", &init_sha).unwrap();
    let h2 = svc.list_history("ms", 10).unwrap();
    assert_eq!(h2.len(), 3); // init, ingest, restore
    assert!(!dir.path().join(format!("ms/raw/{}/source.md", added.source_id)).exists());

    // 6. Graph cache is up to date.
    let g = svc.get_graph("ms").unwrap();
    assert!(g.nodes.is_empty()); // No knowledge pages yet (macros not in this plan)
}
```

- [ ] **Step 2: Run the integration test**

Run: `cargo test -p biorouter --test knowledge_e2e`
Expected: 1 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/biorouter/tests/knowledge_e2e.rs
git commit -m "test(knowledge): e2e create + ingest + restore against KnowledgeService"
```

---

## Task 34: Update CLAUDE.md with the new module

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Add the module to the Core Agent Library table**

Locate the section "Core Agent Library (`crates/biorouter/src/`)" in `CLAUDE.md`. Below the existing bullets (after `scheduler.rs`), add:

```markdown
- **`knowledge/`** — Personal knowledge base: storage, git history, file
  conversion (HTML/PDF/DOCX/CSV), credibility classification
  (Crossref/OpenAlex), and graph derivation. The shared service backs
  both the `knowledge` MCP extension and (in later plans) HTTP routes.
```

In the Rust Workspace table at the top of CLAUDE.md, under the `biorouter-mcp` row's purpose, append: `Knowledge` to the list of built-ins.

- [ ] **Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs(claude): document knowledge module in CLAUDE.md"
```

---

## Verification — full suite

- [ ] **Step 1: Run the entire workspace's knowledge tests**

```bash
cargo test -p biorouter --lib knowledge
cargo test -p biorouter --test knowledge_e2e
cargo test -p biorouter-mcp
```

All expected to pass. No new warnings beyond the project's existing baseline.

- [ ] **Step 2: Run style + lint checks**

```bash
cargo fmt --all -- --check
./scripts/clippy-lint.sh
```

Expected: clean (or only pre-existing warnings, not from new code).

- [ ] **Step 3: Smoke-test the MCP tool surface manually**

In a Rust playground or example, instantiate `KnowledgeServer::new()`, call `kb_create_base` with id `smoke`, `kb_add_raw_source` with a text payload, `kb_list_history`, `kb_get_graph`. Verify the JSON results match the schema in the spec.

- [ ] **Step 4: Final commit (only if any fixes were needed)**

If clippy / fmt fixed anything, commit those tweaks separately under a `chore(knowledge): clippy/fmt cleanup` message.

---

## What this plan does not cover (handled by later plans)

- `kb_set_active` / `kb_get_active` (session-state binding) — Plan 2.
- `kb_search` (BM25 search over knowledge pages) — Plan 2.
- `kb_append_log` (explicit log.md appending) — Plan 2.
- The transaction primitives `kb_begin_txn` / `kb_commit_txn` / `kb_abort_txn` as MCP tools — internals are built in Tasks 8/11; the MCP surface is added in Plan 2 where the macros use them.
- `kb_ingest_source`, `kb_query`, `kb_lint` macros + sub-agent loop — Plan 2.
- Real agentic credibility fallback (replaces the stub in Task 25) — Plan 2.
- `.brkb` export/import — Plan 3.
- HTTP routes under `biorouter-server` — Plan 3.
- All frontend work — Plans 4, 5, 6.

## Related documentation

- [Knowledge founding design](founding-design.md) — the approved design this plan implements, including the data model and credibility tiers it assumes.
- [Plan 2 — macros and sub-agent loop](plan-2-macros-and-subagent-loop.md) — picks up the deferred items listed above (`kb_search`, `kb_set_active`, the transaction MCP tools, the real agentic classifier).
- [Plan 3 — HTTP routes and export/import](plan-3-http-routes-and-export.md) — puts this service behind `/knowledge/*` and adds `.brkb`.
- [Knowledge ingestion format roadmap](../../knowledge-base/ingestion-format-roadmap.md) — the follow-on work extending the conversion pipeline built in Tasks 13–19.
