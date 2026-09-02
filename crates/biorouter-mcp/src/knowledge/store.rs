use crate::knowledge::{
    git::{GitRepo, KnowledgeWriteFailure},
    types::ChangeKind,
};
use anyhow::{Context, Result};
use bm25::{Language, SearchEngine, SearchEngineBuilder};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageRef {
    pub path: String,
    pub title: String,
    pub kind: String,
}

#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageContent {
    pub path: String,
    pub content: String,
    /// Raw YAML frontmatter from the page file.
    #[cfg_attr(feature = "utoipa", schema(value_type = Object))]
    pub frontmatter: serde_yaml::Value,
}

pub(crate) fn logical_path(prefix: &str, relative: &Path) -> String {
    let relative = relative
        .iter()
        .map(|component| component.to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    format!("{prefix}/{relative}")
}

pub fn list_pages(kb_root: &Path, prefix: Option<&str>) -> Result<Vec<PageRef>> {
    // #158: "this base has no pages yet" and "this base is not on disk" are
    // different answers, and returning `[]` for both makes them indistinguishable
    // to every caller. An agent asked to summarise a base whose directory has
    // gone would then report, truthfully as far as it can tell, that the base is
    // empty. A base with a registry row but no directory is exactly that case.
    //
    // The base's own root missing is an error; `knowledge/` missing under a real
    // root is a legitimately empty base, which is what a freshly created one is.
    if !kb_root.exists() {
        anyhow::bail!(
            "knowledge base directory is missing at {} — the base is registered but not on \
             disk, so it can be neither read nor re-created under that id",
            kb_root.display()
        );
    }
    let knowledge_dir = kb_root.join("knowledge");
    if !knowledge_dir.exists() {
        return Ok(Vec::new());
    }
    let knowledge_dir = ensure_existing_path_confined(kb_root, &knowledge_dir)?;
    let mut out = Vec::new();
    walk_md(&knowledge_dir, &knowledge_dir, prefix, &mut out)?;
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

fn walk_md(base: &Path, dir: &Path, prefix: Option<&str>, out: &mut Vec<PageRef>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        let p = entry.path();
        if file_type.is_dir() {
            let directory = ensure_existing_path_confined(base, &p)?;
            walk_md(base, &directory, prefix, out)?;
        } else if file_type.is_file() && p.extension().and_then(|e| e.to_str()) == Some("md") {
            let logical = logical_path("knowledge", p.strip_prefix(base).unwrap());
            if let Some(pre) = prefix {
                if !logical.starts_with(pre) {
                    continue;
                }
            }
            let readable = ensure_existing_path_confined(base, &p)?;
            let body = std::fs::read_to_string(readable)?;
            let (fm, _) = split_frontmatter(&body);
            let title = fm
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| p.file_stem().unwrap().to_str().unwrap())
                .to_string();
            let kind = fm
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("note")
                .to_string();
            out.push(PageRef {
                path: logical,
                title,
                kind,
            });
        }
    }
    Ok(())
}

pub fn read_page(kb_root: &Path, path: &str) -> Result<PageContent> {
    let abs = resolve_readable_path(kb_root, path)?;
    let raw =
        std::fs::read_to_string(&abs).with_context(|| format!("reading {}", abs.display()))?;
    let (fm, body) = split_frontmatter(&raw);
    Ok(PageContent {
        path: path.to_string(),
        content: body,
        frontmatter: fm,
    })
}

pub fn write_page(
    kb_root: &Path,
    path: &str,
    content: &str,
    commit_message: &str,
    txn_branch: Option<&str>,
) -> Result<Option<String>> {
    let abs = resolve_writable_path(kb_root, path)?;
    let repo = GitRepo::open(kb_root)?;
    if std::fs::read(&abs).is_ok_and(|existing| existing.as_slice() == content.as_bytes())
        && repo.head_file_matches(Path::new(path), content.as_bytes())?
    {
        return Ok(None);
    }
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent)?;
    }
    write_atomically(&abs, content.as_bytes())?;

    let committed = if let Some(branch) = txn_branch {
        // Caller has already switched HEAD to the txn branch via begin_txn.
        repo.commit_on_txn_in_progress(branch, commit_message)
    } else {
        repo.commit_all(ChangeKind::Manual, commit_message, None)
    };
    committed.map(Some).map_err(|error| {
        KnowledgeWriteFailure::outcome_uncertain(
            format!("page write to {path}"),
            error.context("committing the replaced page"),
        )
        .into()
    })
}

/// Replace one file without exposing a partially-written target or sharing a
/// predictable sibling name with another writer.
pub(crate) fn write_atomically(path: &Path, content: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("{} has no parent directory", path.display()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(content)?;
    temporary
        .persist(path)
        .map(|_| ())
        .map_err(|error| error.error.into())
}

/// Path is readable: `knowledge/`, `raw/`, or top-level `index.md` / `schema.md` / `log.md`.
pub(crate) fn resolve_readable_path(kb_root: &Path, logical: &str) -> Result<std::path::PathBuf> {
    // FIRST, ahead of the prefix test, because that test MISDIAGNOSES this
    // input: it would tell a developer whose path is `raw\evidence\source.md`
    // that it "must start with raw/", which it plainly does. A KB logical path
    // is slash-separated on every platform; a backslash means an OS `Path` was
    // stringified and handed over instead. That is not a near miss but a silent
    // wrong answer — the prefix test fails and a file that is really there is
    // reported absent, which is exactly how this survived as a Windows-only lint
    // bug, invisible on POSIX for the obvious reason.
    if logical.contains('\\') {
        anyhow::bail!(
            "page path must be slash-separated; `{logical}` looks like an OS path, not a \
             knowledge-base path"
        );
    }
    let ok = logical.starts_with("knowledge/")
        || logical.starts_with("raw/")
        || matches!(logical, "index.md" | "schema.md" | "log.md");
    if !ok {
        anyhow::bail!(
            "page path must start with knowledge/ or raw/ or be index.md/schema.md/log.md"
        );
    }
    if logical.contains("..") {
        anyhow::bail!("path traversal not allowed");
    }
    let path = kb_root.join(logical);
    match std::fs::symlink_metadata(&path) {
        Ok(_) => ensure_existing_path_confined(kb_root, &path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            ensure_writable_path_confined(kb_root, &path)?;
            Ok(path)
        }
        Err(error) => {
            Err(error).with_context(|| format!("reading metadata for {}", path.display()))
        }
    }
}

/// The write-path contract: `knowledge/` pages plus the top-level `index.md`,
/// `schema.md`, and `log.md`. `raw/` (and everything else) is read-only.
/// Shared by [`resolve_writable_path`] and the MCP server's `kb_write_page`
/// pre-validation (issue #26), so the two can never disagree.
pub(crate) fn is_writable_page_path(logical: &str) -> bool {
    logical.starts_with("knowledge/") || matches!(logical, "index.md" | "schema.md" | "log.md")
}

/// The recovery guidance appended to a write-path rejection (issue #26): the
/// old message said only why the write failed, never what to do instead, so
/// the agent had nothing to self-correct against.
pub(crate) const WRITE_PATH_RECOVERY: &str = "raw/ holds immutable ingested sources; write \
     curated content under knowledge/ (e.g. knowledge/<topic>.md); to add or update a source, \
     use kb_add_raw_source or re-ingest it";

/// Path is writable: `knowledge/` pages plus `index.md`, `schema.md`, and `log.md`.
/// `raw/` is read-only — the raw source tree is immutable by design.
fn resolve_writable_path(kb_root: &Path, logical: &str) -> Result<std::path::PathBuf> {
    // FIRST, ahead of the prefix test, because that test MISDIAGNOSES this
    // input: it would tell a developer whose path is `raw\evidence\source.md`
    // that it "must start with raw/", which it plainly does. A KB logical path
    // is slash-separated on every platform; a backslash means an OS `Path` was
    // stringified and handed over instead. That is not a near miss but a silent
    // wrong answer — the prefix test fails and a file that is really there is
    // reported absent, which is exactly how this survived as a Windows-only lint
    // bug, invisible on POSIX for the obvious reason.
    if logical.contains('\\') {
        anyhow::bail!(
            "page path must be slash-separated; `{logical}` looks like an OS path, not a \
             knowledge-base path"
        );
    }
    if !is_writable_page_path(logical) {
        anyhow::bail!(
            "write path must start with knowledge/ or be index.md/schema.md/log.md; \
             raw/ paths are read-only. {WRITE_PATH_RECOVERY}"
        );
    }
    if logical.contains("..") {
        anyhow::bail!("path traversal not allowed");
    }
    let path = kb_root.join(logical);
    ensure_writable_path_confined(kb_root, &path)?;
    Ok(path)
}

fn ensure_existing_path_confined(kb_root: &Path, path: &Path) -> Result<PathBuf> {
    let root = std::fs::canonicalize(kb_root)
        .with_context(|| format!("canonicalizing knowledge base {}", kb_root.display()))?;
    let metadata = std::fs::symlink_metadata(path)
        .with_context(|| format!("reading metadata for {}", path.display()))?;
    if metadata.file_type().is_symlink() {
        anyhow::bail!("knowledge paths may not be symbolic links");
    }
    let resolved = std::fs::canonicalize(path)
        .with_context(|| format!("canonicalizing knowledge path {}", path.display()))?;
    if !resolved.starts_with(&root) {
        anyhow::bail!("knowledge path resolves outside its knowledge base");
    }
    Ok(resolved)
}

pub(crate) fn ensure_writable_path_confined(kb_root: &Path, path: &Path) -> Result<()> {
    let root = std::fs::canonicalize(kb_root)
        .with_context(|| format!("canonicalizing knowledge base {}", kb_root.display()))?;
    let relative = path
        .strip_prefix(kb_root)
        .map_err(|_| anyhow::anyhow!("knowledge path is outside its knowledge base"))?;
    let mut current = kb_root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    anyhow::bail!("knowledge paths may not contain symbolic links");
                }
                let resolved = std::fs::canonicalize(&current).with_context(|| {
                    format!("canonicalizing knowledge path {}", current.display())
                })?;
                if !resolved.starts_with(&root) {
                    anyhow::bail!("knowledge path resolves outside its knowledge base");
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

pub fn split_frontmatter(s: &str) -> (serde_yaml::Value, String) {
    if let Some(rest) = s.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---\n") {
            if let (Some(fm), Some(body)) = (rest.get(..end), rest.get(end + 5..)) {
                if let Ok(v) = serde_yaml::from_str(fm) {
                    return (v, body.to_string());
                }
            }
        }
    }
    (serde_yaml::Value::Null, s.to_string())
}

// ── BM25 search ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    pub path: String,
    pub score: f32,
    pub snippet: String,
}

/// Which corpus a search reads.
///
/// ⚠ **No doc comment on any variant.** schemars splits a documented variant
/// into `oneOf` + `const`, and `providers/formats/google.rs` strips both — a
/// Gemini-bound model would receive this field with no accepted values at all.
/// The per-value steer therefore lives in the TOOL's description, not here.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Deserialize, rmcp::schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SearchScope {
    #[default]
    Knowledge,
    RawSources,
    All,
}

/// A BM25 index cached per `(kb_root, scope)`. `paths[i]` is the logical path of
/// the document the engine assigned id `i`; the engine keeps each document's body
/// internally, so the search results carry it back for snippet extraction. The
/// `fingerprint` captures the corpus's (path, len, mtime) set so the cache
/// self-invalidates when any indexed file is added, removed, or edited — no
/// coupling to the write paths is required.
struct CachedIndex {
    fingerprint: u64,
    paths: Vec<String>,
    engine: SearchEngine<u32>,
}

/// Previously every `kb_search` rebuilt the whole BM25 index from scratch —
/// re-reading every doc, cloning bodies, and re-embedding the corpus — then
/// threw it away. For KB-heavy sessions (the ingest/query sub-agent loop searches
/// the same base repeatedly) that is O(corpus) per search. Cache the built engine
/// and reuse it while the corpus is unchanged, turning a repeat search into
/// O(query) (BR-59, perf lens P-42).
type SearchCache = HashMap<(PathBuf, SearchScope), CachedIndex>;
static SEARCH_CACHE: LazyLock<Mutex<SearchCache>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn lock_search_cache() -> std::sync::MutexGuard<'static, SearchCache> {
    SEARCH_CACHE.lock().unwrap_or_else(|poisoned| {
        let mut guard = poisoned.into_inner();
        guard.clear();
        guard
    })
}

pub fn search(kb_root: &Path, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
    search_with_scope(kb_root, query, limit, SearchScope::All)
}

pub fn search_with_scope(
    kb_root: &Path,
    query: &str,
    limit: usize,
    scope: SearchScope,
) -> Result<Vec<SearchHit>> {
    let files = list_doc_files(kb_root, scope)?; // (logical_path, abs_path), sorted
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let fingerprint = corpus_fingerprint(&files);
    let key = (kb_root.to_path_buf(), scope);

    // Fast path: reuse the cached index if the corpus is unchanged.
    {
        let cache = lock_search_cache();
        if let Some(cached) = cache.get(&key) {
            if cached.fingerprint == fingerprint {
                return Ok(hits_from(&cached.engine, &cached.paths, query, limit));
            }
        }
    }

    // Miss: (re)build the index off-lock, run the query, then cache it. Two racing
    // misses may both build; the last insert wins and both return correct hits.
    let mut paths = Vec::with_capacity(files.len());
    let mut bodies = Vec::with_capacity(files.len());
    for (logical, abs) in &files {
        paths.push(logical.clone());
        bodies.push(std::fs::read_to_string(abs)?);
    }
    let engine = SearchEngineBuilder::<u32>::with_corpus(Language::English, bodies).build();
    let hits = hits_from(&engine, &paths, query, limit);

    let mut cache = lock_search_cache();
    cache.insert(
        key,
        CachedIndex {
            fingerprint,
            paths,
            engine,
        },
    );
    Ok(hits)
}

/// Run the query against a built engine and map results back to hits. `paths[id]`
/// gives the logical path; `sr.document.contents` is the indexed body (used for
/// the snippet), so no separate copy of the corpus text is kept in the cache.
fn hits_from(
    engine: &SearchEngine<u32>,
    paths: &[String],
    query: &str,
    limit: usize,
) -> Vec<SearchHit> {
    engine
        .search(query, limit)
        .into_iter()
        .map(|sr| {
            let idx = sr.document.id as usize;
            SearchHit {
                path: paths.get(idx).cloned().unwrap_or_default(),
                score: sr.score,
                snippet: snippet_of(&sr.document.contents, query, 200),
            }
        })
        .collect()
}

/// A cheap content-independent digest of the corpus: for each indexed file, its
/// logical path, byte length, and mtime. Any add / remove / edit changes it. This
/// walks + stats the tree but does not read file contents, so it is far cheaper
/// than the full index rebuild it guards.
fn corpus_fingerprint(files: &[(String, PathBuf)]) -> u64 {
    use std::hash::{Hash, Hasher};
    use std::time::{SystemTime, UNIX_EPOCH};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for (logical, abs) in files {
        logical.hash(&mut hasher);
        if let Ok(meta) = std::fs::metadata(abs) {
            meta.len().hash(&mut hasher);
            if let Ok(modified) = meta.modified() {
                if let Ok(since) = modified.duration_since(UNIX_EPOCH) {
                    since.as_nanos().hash(&mut hasher);
                } else if let Ok(before) = SystemTime::UNIX_EPOCH.duration_since(modified) {
                    // mtime before the epoch: fold it in with a distinct tag.
                    (u64::MAX - before.as_secs()).hash(&mut hasher);
                }
            }
        }
    }
    hasher.finish()
}

/// Enumerate the files an index of `scope` covers, as `(logical_path, abs_path)`
/// pairs sorted by logical path for a stable fingerprint. Does not read contents.
fn list_doc_files(kb_root: &Path, scope: SearchScope) -> Result<Vec<(String, PathBuf)>> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    if matches!(scope, SearchScope::Knowledge | SearchScope::All) {
        let knowledge_dir = kb_root.join("knowledge");
        if knowledge_dir.exists() {
            let knowledge_dir = ensure_existing_path_confined(kb_root, &knowledge_dir)?;
            list_md_files_under(&knowledge_dir, &knowledge_dir, "knowledge", &mut out)?;
        }
    }
    if matches!(scope, SearchScope::RawSources | SearchScope::All) {
        let raw_dir = kb_root.join("raw");
        if raw_dir.exists() {
            let raw_dir = ensure_existing_path_confined(kb_root, &raw_dir)?;
            for entry in std::fs::read_dir(&raw_dir)? {
                let entry = entry?;
                let file_type = entry.file_type()?;
                if file_type.is_symlink() || !file_type.is_dir() {
                    continue;
                }
                let id = entry.file_name().to_string_lossy().to_string();
                let source_md = entry.path().join("source.md");
                if source_md
                    .symlink_metadata()
                    .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
                {
                    let source_md = ensure_existing_path_confined(&raw_dir, &source_md)?;
                    out.push((format!("raw/{id}/source.md"), source_md));
                }
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

fn list_md_files_under(
    base: &Path,
    dir: &Path,
    prefix: &str,
    out: &mut Vec<(String, PathBuf)>,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        let p = entry.path();
        if file_type.is_dir() {
            let directory = ensure_existing_path_confined(base, &p)?;
            list_md_files_under(base, &directory, prefix, out)?;
        } else if file_type.is_file() && p.extension().and_then(|e| e.to_str()) == Some("md") {
            let logical = logical_path(prefix, p.strip_prefix(base).unwrap());
            let readable = ensure_existing_path_confined(base, &p)?;
            out.push((logical, readable));
        }
    }
    Ok(())
}

/// Test-only observer of the cached index fingerprint for a `(kb_root, scope)`.
#[cfg(test)]
pub(crate) fn cached_fingerprint(kb_root: &Path, scope: SearchScope) -> Option<u64> {
    let cache = lock_search_cache();
    cache
        .get(&(kb_root.to_path_buf(), scope))
        .map(|c| c.fingerprint)
}

fn snippet_of(body: &str, query: &str, max_len: usize) -> String {
    // Collect char-boundary offsets so we can slice safely.
    let hay = body.to_ascii_lowercase();
    let needle = query.to_ascii_lowercase();

    let mut snippet = if let Some(match_byte) = hay.find(needle.as_str()) {
        // snap start/end to char boundaries by using `get` with clamped offsets.
        let desired_start = match_byte.saturating_sub(60);
        let desired_end = (match_byte + needle.len() + 140).min(body.len());

        // Walk forward from desired_start until we hit a char boundary.
        let start = (desired_start..=match_byte)
            .find(|&i| body.is_char_boundary(i))
            .unwrap_or(match_byte);
        // Walk backward from desired_end until we hit a char boundary.
        let end = (match_byte..=desired_end)
            .rev()
            .find(|&i| body.is_char_boundary(i))
            .unwrap_or(match_byte);
        let end = end.max(start);

        body.get(start..end).unwrap_or("").replace('\n', " ")
    } else {
        body.chars()
            .take(max_len)
            .collect::<String>()
            .replace('\n', " ")
    };

    if snippet.len() > max_len {
        // Truncate at a char boundary.
        let trunc = (0..=max_len)
            .rev()
            .find(|&i| snippet.is_char_boundary(i))
            .unwrap_or(0);
        snippet.truncate(trunc);
    }
    snippet
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::service::KnowledgeService;

    #[test]
    fn logical_paths_always_use_forward_slashes() {
        let relative = Path::new("concepts").join("a.md");
        assert_eq!(
            logical_path("knowledge", &relative),
            "knowledge/concepts/a.md"
        );
    }

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
    fn a_predictable_legacy_tmp_collision_cannot_block_page_replacement() {
        let (_dir, kb) = fresh();
        let page = kb.join("knowledge/entities/hrv.md");
        std::fs::create_dir_all(page.with_extension("md.tmp")).unwrap();

        let body = "---\ntitle: HRV\nkind: entity\n---\n\nBody text.";
        write_page(&kb, "knowledge/entities/hrv.md", body, "add HRV", None).unwrap();

        assert_eq!(std::fs::read_to_string(page).unwrap(), body);
    }

    #[test]
    fn retrying_identical_durable_page_content_does_not_commit_again() {
        let (_dir, kb) = fresh();
        let body = "---\ntitle: HRV\nkind: entity\n---\n\nBody text.";
        assert!(
            write_page(&kb, "knowledge/entities/hrv.md", body, "add HRV", None)
                .unwrap()
                .is_some()
        );
        let history_len = GitRepo::open(&kb).unwrap().log(10).unwrap().len();

        assert!(
            write_page(&kb, "knowledge/entities/hrv.md", body, "retry HRV", None)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            GitRepo::open(&kb).unwrap().log(10).unwrap().len(),
            history_len
        );
    }

    #[test]
    fn identical_uncommitted_page_content_is_not_mistaken_for_a_completed_retry() {
        let (_dir, kb) = fresh();
        let body = "---\ntitle: HRV\nkind: entity\n---\n\nBody text.";
        let path = kb.join("knowledge/entities/hrv.md");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, body).unwrap();

        assert!(
            write_page(&kb, "knowledge/entities/hrv.md", body, "finish HRV", None)
                .unwrap()
                .is_some(),
            "matching working-tree bytes are not durable until HEAD contains them"
        );
        assert!(GitRepo::open(&kb)
            .unwrap()
            .head_file_matches(Path::new("knowledge/entities/hrv.md"), body.as_bytes())
            .unwrap());
    }

    /// #158. `[]` must mean "this base has no pages", never "this base is not
    /// on disk". A registry row whose directory has gone produced the second
    /// while looking like the first, so an agent asked to summarise the base
    /// would report it empty — truthfully, as far as it could tell.
    ///
    /// The mutation that exposed this: reverting the guard left all 19 store
    /// tests green, because nothing covered a missing base root.
    #[test]
    fn a_missing_base_root_is_an_error_not_an_empty_list() {
        let dir = tempfile::tempdir().unwrap();

        // A real base with no pages yet — legitimately empty.
        let fresh = dir.path().join("fresh");
        std::fs::create_dir_all(&fresh).unwrap();
        assert!(
            super::list_pages(&fresh, None)
                .expect("a real base must read")
                .is_empty(),
            "a base with no knowledge/ directory is empty, not an error"
        );

        // A base root that is not there at all.
        let gone = dir.path().join("gone");
        let err = super::list_pages(&gone, None)
            .expect_err("a base whose directory is missing must not read as empty");
        let msg = err.to_string();
        assert!(
            msg.contains("missing"),
            "the error must say the base is missing: {msg}"
        );
        assert!(
            msg.contains("gone"),
            "and name the path so the caller can act: {msg}"
        );
    }

    #[test]
    fn list_pages_sorted_and_filtered() {
        let (_dir, kb) = fresh();
        write_page(
            &kb,
            "knowledge/entities/b.md",
            "---\ntitle: B\n---\n",
            "b",
            None,
        )
        .unwrap();
        write_page(
            &kb,
            "knowledge/concepts/a.md",
            "---\ntitle: A\n---\n",
            "a",
            None,
        )
        .unwrap();
        let all = list_pages(&kb, None).unwrap();
        let paths: Vec<_> = all.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(
            paths,
            vec!["knowledge/concepts/a.md", "knowledge/entities/b.md"]
        );
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
        // write_page must still reject raw/ paths (raw/ tree is read-only)
        let err = write_page(&kb, "raw/x.md", "x", "x", None).unwrap_err();
        assert!(
            err.to_string().contains("knowledge/") || err.to_string().contains("write path"),
            "unexpected error: {err}"
        );
        // Issue #26: the rejection must carry the recovery path, not just the rule.
        assert!(
            err.to_string().contains("kb_add_raw_source")
                && err.to_string().contains("knowledge/<topic>.md"),
            "rejection must name the recovery, got: {err}"
        );
    }

    #[test]
    fn read_page_allows_raw_source_md() {
        let (_d, kb) = fresh();
        std::fs::create_dir_all(kb.join("raw/x")).unwrap();
        std::fs::write(kb.join("raw/x/source.md"), "---\ntitle: X\n---\nbody").unwrap();
        let p = read_page(&kb, "raw/x/source.md").unwrap();
        assert!(p.content.contains("body"));
    }

    #[test]
    fn read_page_allows_raw_meta_yaml() {
        let (_d, kb) = fresh();
        std::fs::create_dir_all(kb.join("raw/x")).unwrap();
        std::fs::write(kb.join("raw/x/meta.yaml"), "id: x\ntitle: X\n").unwrap();
        let p = read_page(&kb, "raw/x/meta.yaml").unwrap();
        // The full file content is available (either as frontmatter or as raw body)
        assert!(
            p.content.contains("title: X") || p.frontmatter.is_mapping(),
            "expected YAML content in page; got: {:?} / {:?}",
            p.content,
            p.frontmatter
        );
    }

    #[test]
    fn write_page_still_rejects_raw_paths() {
        let (_d, kb) = fresh();
        let err = write_page(&kb, "raw/x/source.md", "x", "x", None).unwrap_err();
        assert!(
            err.to_string().contains("knowledge/") || err.to_string().contains("write path"),
            "unexpected error: {err}"
        );
        // Issue #26: pin the recovery guidance so it cannot silently regress.
        assert!(
            err.to_string().contains(WRITE_PATH_RECOVERY),
            "rejection must carry the recovery guidance, got: {err}"
        );
    }

    #[test]
    fn writable_page_path_contract() {
        // Shared predicate behind resolve_writable_path and the MCP server's
        // kb_write_page pre-validation (issue #26).
        assert!(is_writable_page_path("knowledge/topic.md"));
        assert!(is_writable_page_path("knowledge/concepts/a.md"));
        assert!(is_writable_page_path("index.md"));
        assert!(is_writable_page_path("schema.md"));
        assert!(is_writable_page_path("log.md"));
        assert!(!is_writable_page_path("raw/x/source.md"));
        assert!(!is_writable_page_path("notes.md"));
        assert!(!is_writable_page_path(""));
    }

    #[cfg(unix)]
    #[test]
    fn page_reads_indexes_and_writes_cannot_escape_through_symlinks() {
        use std::os::unix::fs::symlink;

        let (_dir, kb) = fresh();
        let outside = tempfile::tempdir().unwrap();
        let secret = outside.path().join("auth.json");
        std::fs::write(&secret, "subscription-secret-marker").unwrap();
        symlink(&secret, kb.join("knowledge/leak.md")).unwrap();

        let read = read_page(&kb, "knowledge/leak.md").unwrap_err();
        assert!(
            read.to_string().contains("symbolic link")
                || read.to_string().contains("outside its knowledge base"),
            "unexpected refusal: {read:#}"
        );
        assert!(
            list_pages(&kb, None)
                .unwrap()
                .iter()
                .all(|page| page.path != "knowledge/leak.md"),
            "symlinked pages must not enter the catalog"
        );
        assert!(
            search(&kb, "subscription-secret-marker", 5)
                .unwrap()
                .is_empty(),
            "symlinked content must not enter the search index"
        );

        let outside_dir = outside.path().join("redirected");
        std::fs::create_dir_all(&outside_dir).unwrap();
        symlink(&outside_dir, kb.join("knowledge/redirected")).unwrap();
        let write = write_page(
            &kb,
            "knowledge/redirected/new.md",
            "must stay inside",
            "symlink escape",
            None,
        )
        .unwrap_err();
        assert!(write.to_string().contains("symbolic link"), "{write:#}");
        assert!(!outside_dir.join("new.md").exists());
    }

    #[test]
    fn a_missing_read_path_is_confined_and_left_for_the_caller_to_classify() {
        let (_dir, kb) = fresh();
        let path = resolve_readable_path(&kb, "knowledge/notes/missing.md").unwrap();
        assert_eq!(path, kb.join("knowledge/notes/missing.md"));
        assert!(!path.exists());
    }

    /// An OS path is refused where a knowledge-base path is required.
    ///
    /// This is the POSIX-testable half of a bug that could only FAIL on Windows.
    /// `is_existing_raw_evidence` in the lint built an OS `PathBuf` and handed
    /// over `to_str()`, so on Windows the resolver received
    /// `raw\evidence\source.md`. The prefix test wants a literal `raw/`, so the
    /// call bailed, `is_file()` was never reached, and a source page's links to
    /// evidence that was really on disk came back as `missing_concept_pages` —
    /// on Windows only, with a symptom that reads as a linting bug rather than a
    /// path bug.
    ///
    /// A test of the CALLER cannot catch that from here: on POSIX the buggy and
    /// the fixed spellings produce byte-identical strings. This can, because it
    /// feeds the resolver the exact value Windows produced. It is asserted on
    /// both resolvers because both carry the same prefix contract, and the write
    /// side would have failed the same way the moment a caller made the same
    /// mistake.
    #[test]
    fn a_backslash_separated_path_is_not_a_knowledge_base_path() {
        let (_dir, kb) = fresh();
        for logical in ["raw\\evidence\\source.md", "knowledge\\notes\\a.md"] {
            let read = resolve_readable_path(&kb, logical).unwrap_err();
            assert!(
                read.to_string().contains("slash-separated"),
                "read path `{logical}`: {read:#}"
            );
            let write = resolve_writable_path(&kb, logical).unwrap_err();
            assert!(
                write.to_string().contains("slash-separated"),
                "write path `{logical}`: {write:#}"
            );
        }
        // …and the slash spelling of the same page is still accepted, so the
        // refusal is about the separator and not about the path.
        resolve_readable_path(&kb, "raw/evidence/source.md").unwrap();
    }

    #[test]
    fn search_returns_relevant_hits() {
        let (_dir, kb) = fresh();
        write_page(
            &kb,
            "knowledge/entities/hrv.md",
            "---\ntitle: HRV\nkind: entity\n---\n\nHeart rate variability is a key marker.",
            "a",
            None,
        )
        .unwrap();
        write_page(
            &kb,
            "knowledge/concepts/sleep.md",
            "---\ntitle: Sleep\nkind: concept\n---\n\nSleep quality affects HRV directly.",
            "b",
            None,
        )
        .unwrap();
        let hits = search(&kb, "heart rate variability", 5).unwrap();
        assert!(!hits.is_empty());
        assert!(hits.iter().any(|h| h.path.ends_with("hrv.md")));
    }

    #[test]
    fn search_returns_empty_when_no_match() {
        let (_dir, kb) = fresh();
        write_page(
            &kb,
            "knowledge/entities/x.md",
            "---\ntitle: X\n---\nbody",
            "a",
            None,
        )
        .unwrap();
        let hits = search(&kb, "zzznonexistent", 5).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn search_scope_controls_raw_sources() {
        let (_dir, kb) = fresh();
        write_page(
            &kb,
            "knowledge/entities/curated.md",
            "---\ntitle: Curated\n---\ncurated-only signal",
            "curated",
            None,
        )
        .unwrap();
        std::fs::create_dir_all(kb.join("raw/raw-a")).unwrap();
        std::fs::write(kb.join("raw/raw-a/source.md"), "raw-only signal").unwrap();

        let curated = search_with_scope(&kb, "raw-only", 5, SearchScope::Knowledge).unwrap();
        assert!(curated.is_empty());

        let raw = search_with_scope(&kb, "raw-only", 5, SearchScope::RawSources).unwrap();
        assert_eq!(raw.len(), 1);
        assert_eq!(raw[0].path, "raw/raw-a/source.md");

        let all = search(&kb, "raw-only", 5).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].path, "raw/raw-a/source.md");
    }

    #[test]
    fn search_index_is_cached_and_reused_while_unchanged() {
        let (_dir, kb) = fresh();
        write_page(
            &kb,
            "knowledge/entities/hrv.md",
            "---\ntitle: HRV\n---\n\nHeart rate variability is a key marker.",
            "a",
            None,
        )
        .unwrap();

        // No index is cached until the first search builds one.
        assert_eq!(cached_fingerprint(&kb, SearchScope::All), None);

        let hits1 = search(&kb, "heart rate variability", 5).unwrap();
        assert!(hits1.iter().any(|h| h.path.ends_with("hrv.md")));
        let fp1 =
            cached_fingerprint(&kb, SearchScope::All).expect("index cached after first search");

        // A second identical search reuses the cached index: same fingerprint,
        // byte-identical results.
        let hits2 = search(&kb, "heart rate variability", 5).unwrap();
        assert_eq!(hits1, hits2);
        assert_eq!(Some(fp1), cached_fingerprint(&kb, SearchScope::All));
    }

    #[test]
    fn search_cache_invalidates_when_corpus_changes() {
        let (_dir, kb) = fresh();
        write_page(
            &kb,
            "knowledge/entities/hrv.md",
            "---\ntitle: HRV\n---\n\nHeart rate variability is a key marker.",
            "a",
            None,
        )
        .unwrap();
        let _ = search(&kb, "heart rate variability", 5).unwrap();
        let fp1 = cached_fingerprint(&kb, SearchScope::All).unwrap();

        // Adding a page changes the corpus, so the fingerprint (and thus the
        // cached index) must change on the next search, and the new page must be
        // findable — proving the stale index was not reused.
        write_page(
            &kb,
            "knowledge/concepts/sleep.md",
            "---\ntitle: Sleep\n---\n\nSleep quality affects recovery.",
            "b",
            None,
        )
        .unwrap();
        let hits = search(&kb, "sleep quality recovery", 5).unwrap();
        assert!(hits.iter().any(|h| h.path.ends_with("sleep.md")));

        let fp2 = cached_fingerprint(&kb, SearchScope::All).unwrap();
        assert_ne!(fp1, fp2, "adding a page must change the cached fingerprint");
    }
}
