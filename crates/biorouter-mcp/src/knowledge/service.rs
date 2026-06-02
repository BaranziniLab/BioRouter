use crate::knowledge::{
    convert, credibility,
    git::GitRepo,
    manifest, paths, raw, registry,
    types::{Manifest, RegistryEntry, SourceMeta},
};
use anyhow::{Context, Result};
use chrono::Utc;
use dashmap::DashMap;
use fs2::FileExt as _;
use std::sync::Arc;
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};
use tokio::sync::{Mutex, OwnedMutexGuard};

const DEFAULT_SCHEMA: &str = include_str!("schema_default.md");
const DEFAULT_INDEX: &str = "# Index\n\n_no pages yet_\n";
const DEFAULT_LOG: &str = "# Log\n\n";
const GITIGNORE: &str = "raw/*/original.*\n.biorouter-knowledge/.crossref-cache/\n";

/// Cross-reference rules block appended to legacy `schema.md` files that
/// pre-date the Plan 5 Task 2 schema hardening. Kept in sync with the
/// equivalent block in `schema_default.md`. The unique substring
/// `"Cross-reference rules"` is used as the migration fingerprint.
const SCHEMA_CROSSREF_RULES: &str = r#"
### Cross-reference rules (the graph depends on these)

The knowledge graph is derived **purely** from `[[link]]` patterns in page
bodies. If you do not emit links, the graph will have nodes but no edges.

When you write or update any knowledge page:

1. Every mention of another entity or concept that has (or should have) its
   own page **must** be wrapped in `[[double brackets]]`. Match the target
   page's title exactly (case-insensitive); the deriver slugifies both sides.
   Good: `[[EPAS1]] interacts with [[HIF2A]] under [[hypoxia]].`
   Bad:  `EPAS1 interacts with HIF2A under hypoxia.`
2. Every source page **must** include a `## Related pages` section listing
   every entity/concept it touches, one `- [[Name]]` bullet per line.
3. Every entity/concept page **must** include a `## Sources` section with
   one `- [[source-id]]` bullet per supporting source.
4. Prefer linking over re-stating. If a fact lives on another page, write
   `See [[Page Name]]` instead of restating it.

The lint workflow (`kb_lint`) reports pages with no inbound links as orphans
— fix them by adding inbound `[[links]]` from related pages.
"#;

#[derive(Clone)]
pub struct KnowledgeService {
    root: PathBuf,
    locks: Arc<DashMap<String, Arc<Mutex<()>>>>,
}

struct FileLockGuard {
    file: File,
}

impl FileLockGuard {
    fn acquire(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        file.lock_exclusive()?;
        Ok(Self { file })
    }
}

impl Drop for FileLockGuard {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

pub struct KnowledgeWriteGuard {
    _process_guard: OwnedMutexGuard<()>,
    _file_guard: FileLockGuard,
}

impl KnowledgeService {
    fn slugify_kb_name(name: &str) -> String {
        let mut slug = String::with_capacity(name.len());
        let mut last_was_dash = false;

        for ch in name.chars().flat_map(|c| c.to_lowercase()) {
            if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
                slug.push(ch);
                last_was_dash = false;
            } else if !slug.is_empty() && !last_was_dash {
                slug.push('-');
                last_was_dash = true;
            }
        }

        while slug.ends_with('-') {
            slug.pop();
        }

        slug
    }

    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            locks: Arc::new(DashMap::new()),
        }
    }

    pub fn new_default() -> Result<Self> {
        Ok(Self::new(paths::knowledge_root()?))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn root_lock_path(&self) -> PathBuf {
        self.root.join(".knowledge-root.lock")
    }

    fn kb_lock_path(&self, kb_id: &str) -> PathBuf {
        paths::kb_internal_dir(&self.root, kb_id).join("write.lock")
    }

    fn lock_root(&self) -> Result<FileLockGuard> {
        FileLockGuard::acquire(&self.root_lock_path())
    }

    /// Acquire an exclusive lock for `kb_id`. Held until the returned guard is dropped.
    /// Used by macros to serialize concurrent writers against the same KB.
    pub async fn lock_kb(&self, kb_id: &str) -> Result<KnowledgeWriteGuard> {
        let m = self
            .locks
            .entry(kb_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let process_guard = m.lock_owned().await;
        let file_guard = FileLockGuard::acquire(&self.kb_lock_path(kb_id))?;
        Ok(KnowledgeWriteGuard {
            _process_guard: process_guard,
            _file_guard: file_guard,
        })
    }

    pub fn create_base(&self, id: &str, name: &str, color: Option<&str>) -> Result<Manifest> {
        let _lock = self.lock_root()?;
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
            RegistryEntry {
                id: id.to_string(),
                path: kb_root,
            },
        )?;
        self.rebuild_graph_cache(id)?;
        Ok(m)
    }

    pub fn export_brkb(&self, kb_id: &str) -> Result<Vec<u8>> {
        paths::validate_kb_id(kb_id)?;
        let kb_root = paths::kb_root(&self.root, kb_id);
        if !kb_root.exists() {
            anyhow::bail!("kb '{kb_id}' not found");
        }
        let mut buf = std::io::Cursor::new(Vec::new());
        crate::knowledge::brkb::export(&kb_root, &mut buf)?;
        Ok(buf.into_inner())
    }

    pub fn import_brkb(&self, zip_bytes: &[u8]) -> Result<String> {
        let _lock = self.lock_root()?;
        std::fs::create_dir_all(&self.root)?;
        let cursor = std::io::Cursor::new(zip_bytes);
        let new_id = crate::knowledge::brkb::import(cursor, &self.root)?;
        // Register in the top-level manifest.
        let path = paths::kb_root(&self.root, &new_id);
        crate::knowledge::registry::register(
            &self.root,
            crate::knowledge::types::RegistryEntry {
                id: new_id.clone(),
                path,
            },
        )?;
        Ok(new_id)
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

    pub fn update_base(
        &self,
        id: &str,
        name: Option<&str>,
        color: Option<&str>,
    ) -> Result<Manifest> {
        let _lock = self.lock_root()?;
        paths::validate_kb_id(id)?;
        let current_root = paths::kb_root(&self.root, id);
        if !current_root.exists() {
            anyhow::bail!("kb '{id}' not found");
        }

        let mut current = manifest::load(&current_root)?;
        let mut changed = false;
        let mut target_id = id.to_string();
        let mut target_root = current_root.clone();

        if let Some(name) = name {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                anyhow::bail!("knowledge base name cannot be empty");
            }
            if current.name != trimmed {
                let next_id = Self::slugify_kb_name(trimmed);
                if next_id.is_empty() {
                    anyhow::bail!("knowledge base name must contain letters or numbers");
                }
                paths::validate_kb_id(&next_id)?;
                let next_root = paths::kb_root(&self.root, &next_id);
                if next_id != id && next_root.exists() {
                    anyhow::bail!("kb '{next_id}' already exists");
                }
                current.name = trimmed.to_string();
                current.id = next_id.clone();
                target_id = next_id;
                target_root = next_root;
                changed = true;
            }
        }

        if let Some(color) = color {
            let trimmed = color.trim();
            if trimmed.is_empty() {
                anyhow::bail!("knowledge base color cannot be empty");
            }
            if current.color != trimmed {
                current.color = trimmed.to_string();
                changed = true;
            }
        }

        if changed {
            let commit_message = if target_id != id {
                format!("rename knowledge base {id} to {}", current.id)
            } else {
                format!("update knowledge base {id} metadata")
            };

            if target_id != id {
                std::fs::rename(&current_root, &target_root)?;
                registry::replace(
                    &self.root,
                    id,
                    RegistryEntry {
                        id: target_id.clone(),
                        path: target_root.clone(),
                    },
                )?;

                if self.get_active_persisted_unlocked()?.as_deref() == Some(id) {
                    self.set_active_persisted_unlocked(Some(&target_id))?;
                }
            }

            manifest::save(&target_root, &current)?;
            let repo = GitRepo::open(&target_root)?;
            repo.commit_all(
                crate::knowledge::types::ChangeKind::Manual,
                &commit_message,
                None,
            )?;
            self.rebuild_graph_cache(&current.id)?;
        }

        Ok(current)
    }

    pub fn delete_base(&self, id: &str) -> Result<()> {
        let _lock = self.lock_root()?;
        paths::validate_kb_id(id)?;
        let kb_root = paths::kb_root(&self.root, id);
        if !kb_root.exists() {
            anyhow::bail!("kb '{id}' not found");
        }

        registry::unregister(&self.root, id)?;
        if let Err(err) = std::fs::remove_dir_all(&kb_root) {
            let _ = registry::register(
                &self.root,
                RegistryEntry {
                    id: id.to_string(),
                    path: kb_root.clone(),
                },
            );
            return Err(err.into());
        }

        if self.get_active_persisted_unlocked()?.as_deref() == Some(id) {
            self.set_active_persisted_unlocked(None)?;
        }

        Ok(())
    }
}

impl KnowledgeService {
    fn find_existing_source_match(
        &self,
        kb_root: &Path,
        url: Option<&str>,
        sha256: &str,
    ) -> Result<(Option<SourceMeta>, Option<SourceMeta>)> {
        let mut by_url = None;
        let mut by_hash = None;

        for meta in raw::list_sources(kb_root)? {
            if by_url.is_none() && meta.url.as_deref() == url && url.is_some() {
                by_url = Some(meta.clone());
            }
            if by_hash.is_none() && meta.sha256 == sha256 {
                by_hash = Some(meta);
            }
            if by_url.is_some() && by_hash.is_some() {
                break;
            }
        }

        Ok((by_url, by_hash))
    }

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
        let credibility = credibility::classify(&input, None).await?;

        let title = converted.title.clone().unwrap_or_else(|| match &input {
            convert::SourceInput::Text { title, .. } => {
                title.clone().unwrap_or_else(|| "Untitled note".into())
            }
            convert::SourceInput::Url(u) => u.clone(),
            convert::SourceInput::File { filename, .. } => filename.clone(),
        });

        let (original_bytes, original_filename, url) = match &input {
            convert::SourceInput::File {
                bytes, filename, ..
            } => (Some(bytes.clone()), Some(filename.clone()), None),
            convert::SourceInput::Url(u) => (None, None, Some(u.clone())),
            convert::SourceInput::Text { .. } => (None, None, None),
        };

        let hash = match &original_bytes {
            Some(b) => raw::hash_bytes(b),
            None => raw::hash_bytes(converted.markdown.as_bytes()),
        };

        let (existing_by_url, existing_by_hash) =
            self.find_existing_source_match(&kb_root, url.as_deref(), &hash)?;

        if let Some(existing) = existing_by_hash.as_ref() {
            if existing_by_url
                .as_ref()
                .map(|meta| meta.id.as_str())
                .unwrap_or(existing.id.as_str())
                == existing.id
            {
                return Ok(raw::RawWrite {
                    source_id: existing.id.clone(),
                    source_md_path: format!("raw/{}/source.md", existing.id),
                    meta_path: format!("raw/{}/meta.yaml", existing.id),
                });
            }
        }

        let source_id = existing_by_url
            .as_ref()
            .map(|meta| meta.id.clone())
            .unwrap_or_else(|| raw::new_source_id(&title));

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
            meta.original_filename.clone().as_deref(),
            &converted.markdown,
            meta,
        )?;

        let repo = GitRepo::open(&kb_root)?;
        let (summary, delta) = if existing_by_url.is_some() {
            (format!("refresh source {source_id}"), "~1 source")
        } else {
            (format!("ingested {source_id}"), "+1 source")
        };
        if let Some(_branch) = txn_branch {
            repo.commit_on_txn_in_progress(&summary)?;
        } else {
            repo.commit_all(
                crate::knowledge::types::ChangeKind::Ingest,
                &summary,
                Some(delta),
            )?;
        }
        self.rebuild_graph_cache(kb_id)?;
        Ok(written)
    }
}

/// Typed error returned by [`KnowledgeService::read_page`] so HTTP handlers can
/// map each variant onto the right status code (400 / 404 / 500) without
/// substring-matching the `Display` text.
#[derive(thiserror::Error, Debug)]
pub enum ReadPageError {
    #[error("invalid kb id: {0}")]
    InvalidKbId(String),
    #[error("invalid path: {0}")]
    InvalidPath(String),
    #[error("page not found: {0}")]
    NotFound(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl KnowledgeService {
    /// Re-derive the knowledge graph from the on-disk pages and overwrite the
    /// cached `graph-cache.json`. Public so macros (and bug-fix migrations
    /// like the wiki-link deriver fix) can refresh stale caches without
    /// hand-crafting a commit.
    pub fn rebuild_graph_cache(&self, kb_id: &str) -> anyhow::Result<()> {
        let kb_root = paths::kb_root(&self.root, kb_id);
        let g = crate::knowledge::graph::derive(&kb_root)?;
        crate::knowledge::graph::write_cache(&kb_root, &g)?;
        Ok(())
    }

    /// One-shot, idempotent upgrade for KBs created before the schema gained
    /// explicit cross-reference rules (Plan 5 Task 2). If `schema.md` does
    /// not already mention `"Cross-reference rules"`, the rules block is
    /// appended in place and committed. User customisations elsewhere in
    /// the file are preserved.
    ///
    /// Returns `Ok(true)` if the schema was rewritten, `Ok(false)` if it was
    /// already up-to-date or the KB has no `schema.md`.
    pub fn migrate_schema_if_needed(&self, kb_id: &str) -> Result<bool> {
        paths::validate_kb_id(kb_id)?;
        let kb_root = paths::kb_root(&self.root, kb_id);
        let schema_path = kb_root.join("schema.md");
        if !schema_path.exists() {
            return Ok(false);
        }
        let current = std::fs::read_to_string(&schema_path).context("read schema.md")?;
        if current.contains("Cross-reference rules") {
            return Ok(false);
        }
        // Ensure a blank line separates whatever the user had from the new
        // section, even if their file did not end with a newline.
        let mut next = current;
        if !next.ends_with('\n') {
            next.push('\n');
        }
        next.push_str(SCHEMA_CROSSREF_RULES);
        std::fs::write(&schema_path, next).context("write schema.md")?;

        let repo = GitRepo::open(&kb_root)?;
        repo.commit_all(
            crate::knowledge::types::ChangeKind::Manual,
            "migrate schema: add cross-reference rules",
            None,
        )
        .context("commit schema migration")?;
        Ok(true)
    }

    pub fn get_graph(&self, kb_id: &str) -> anyhow::Result<crate::knowledge::types::Graph> {
        paths::validate_kb_id(kb_id)?;
        let kb_root = paths::kb_root(&self.root, kb_id);
        if let Some(g) = crate::knowledge::graph::read_cache(&kb_root)? {
            return Ok(g);
        }
        crate::knowledge::graph::derive(&kb_root)
    }

    /// Read the raw markdown body of a page (knowledge/*.md, raw/*/source.md,
    /// or top-level index.md/schema.md/log.md).
    ///
    /// Returns a typed [`ReadPageError`] so HTTP handlers can map each variant
    /// onto the right status code (400 / 404 / 500) without inspecting the
    /// `Display` text. Path traversal and out-of-scope paths are rejected.
    pub fn read_page(&self, kb_id: &str, rel_path: &str) -> Result<String, ReadPageError> {
        paths::validate_kb_id(kb_id).map_err(|e| ReadPageError::InvalidKbId(e.to_string()))?;
        let kb_root = paths::kb_root(&self.root, kb_id);
        let abs = crate::knowledge::store::resolve_readable_path(&kb_root, rel_path)
            .map_err(|e| ReadPageError::InvalidPath(e.to_string()))?;
        if !abs.exists() {
            return Err(ReadPageError::NotFound(rel_path.to_string()));
        }
        Ok(std::fs::read_to_string(&abs)?)
    }

    /// Read the persisted active-KB id (set via the UI or `kb_set_active`).
    /// Returns `Ok(None)` if no file exists or the file is empty.
    pub fn get_active_persisted(&self) -> anyhow::Result<Option<String>> {
        self.get_active_persisted_unlocked()
    }

    fn get_active_persisted_unlocked(&self) -> anyhow::Result<Option<String>> {
        let path = crate::knowledge::paths::active_kb_path(self.root());
        if !path.exists() {
            return Ok(None);
        }
        let s = std::fs::read_to_string(&path)?;
        let trimmed = s.trim();
        if trimmed.is_empty() {
            Ok(None)
        } else {
            Ok(Some(trimmed.to_string()))
        }
    }

    /// Persist the active-KB id. Pass `None` to clear.
    pub fn set_active_persisted(&self, id: Option<&str>) -> anyhow::Result<()> {
        let _lock = self.lock_root()?;
        self.set_active_persisted_unlocked(id)
    }

    fn set_active_persisted_unlocked(&self, id: Option<&str>) -> anyhow::Result<()> {
        let path = crate::knowledge::paths::active_kb_path(self.root());
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        match id {
            Some(id) => {
                crate::knowledge::paths::validate_kb_id(id)?;
                let tmp = path.with_extension("tmp");
                std::fs::write(&tmp, id.as_bytes())?;
                std::fs::rename(tmp, &path)?;
            }
            None => {
                if path.exists() {
                    std::fs::remove_file(&path)?;
                }
            }
        }
        Ok(())
    }
}

impl KnowledgeService {
    pub fn list_history(
        &self,
        kb_id: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<crate::knowledge::types::HistoryEntry>> {
        paths::validate_kb_id(kb_id)?;
        let kb_root = paths::kb_root(&self.root, kb_id);
        let repo = GitRepo::open(&kb_root)?;
        repo.log(limit)
    }

    pub fn restore_state(&self, kb_id: &str, commit_sha: &str) -> anyhow::Result<String> {
        paths::validate_kb_id(kb_id)?;
        let kb_root = paths::kb_root(&self.root, kb_id);
        let repo = GitRepo::open(&kb_root)?;
        let summary = format!("restore to {}", commit_sha.get(..7).unwrap_or(commit_sha));
        let sha = repo.restore_to(commit_sha, &summary)?;
        self.rebuild_graph_cache(kb_id)?;
        Ok(sha)
    }

    pub fn preview_state(
        &self,
        kb_id: &str,
        commit_sha: &str,
        path: &str,
    ) -> anyhow::Result<Option<String>> {
        paths::validate_kb_id(kb_id)?;
        let kb_root = paths::kb_root(&self.root, kb_id);
        let repo = GitRepo::open(&kb_root)?;
        repo.read_file_at(commit_sha, path)
    }
}

impl KnowledgeService {
    /// Re-run credibility classification for an existing raw source using the stored URL or
    /// the derived markdown text (for File/Text sources) and persist the result to `meta.yaml`.
    pub async fn reclassify_source(
        &self,
        kb_id: &str,
        source_id: &str,
    ) -> anyhow::Result<crate::knowledge::types::Credibility> {
        paths::validate_kb_id(kb_id)?;
        let kb_root = paths::kb_root(&self.root, kb_id);
        let mut meta = raw::read_meta(&kb_root, source_id)?;

        // Reconstruct a SourceInput from what was stored.  URL-based sources keep the url;
        // everything else falls back to the derived markdown (source.md).
        let input = if let Some(url) = meta.url.clone() {
            convert::SourceInput::Url(url)
        } else {
            let body =
                std::fs::read_to_string(kb_root.join("raw").join(source_id).join("source.md"))?;
            convert::SourceInput::Text {
                text: body,
                title: Some(meta.title.clone()),
            }
        };

        let new_cred = credibility::classify(&input, None).await?;
        meta.credibility = new_cred.clone();
        let yaml = serde_yaml::to_string(&meta)?;
        std::fs::write(kb_root.join("raw").join(source_id).join("meta.yaml"), yaml)?;

        let repo = GitRepo::open(&kb_root)?;
        repo.commit_all(
            crate::knowledge::types::ChangeKind::Manual,
            &format!("reclassify {source_id}"),
            None,
        )?;
        self.rebuild_graph_cache(kb_id)?;
        Ok(new_cred)
    }

    /// Write a manually-specified `Credibility` override to `meta.yaml` and commit.
    /// Returns the credibility that was stored (same as input).
    pub fn override_credibility(
        &self,
        kb_id: &str,
        source_id: &str,
        cred: crate::knowledge::types::Credibility,
    ) -> anyhow::Result<crate::knowledge::types::Credibility> {
        paths::validate_kb_id(kb_id)?;
        let kb_root = paths::kb_root(&self.root, kb_id);
        let mut meta = raw::read_meta(&kb_root, source_id)?;
        meta.credibility = cred.clone();
        let yaml = serde_yaml::to_string(&meta)?;
        std::fs::write(kb_root.join("raw").join(source_id).join("meta.yaml"), yaml)?;
        let repo = GitRepo::open(&kb_root)?;
        repo.commit_all(
            crate::knowledge::types::ChangeKind::Manual,
            &format!("override credibility for {source_id}"),
            None,
        )?;
        self.rebuild_graph_cache(kb_id)?;
        Ok(cred)
    }
}

impl KnowledgeService {
    /// Perform a trivial LLM completion to verify the provider is reachable.
    ///
    /// Sends a single "Reply with OK" message to the model.  Any network error,
    /// authentication failure, or invalid model name surfaces as `Err`.
    pub async fn check_model(
        &self,
        completer: Box<dyn crate::knowledge::subagent::loop_::Completer>,
    ) -> anyhow::Result<()> {
        let messages = vec![crate::knowledge::subagent::loop_::LlmMessage::User(
            "Reply with exactly OK and nothing else.".to_string(),
        )];
        completer
            .complete(
                "You are a connectivity test. Respond with OK.",
                &messages,
                &[], // no tools
            )
            .await
            .map_err(|e| anyhow::anyhow!("LLM check failed: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::convert::SourceInput;
    use crate::knowledge::types::{ChangeKind, Credibility, CredibilityTier};

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

    #[tokio::test]
    async fn add_raw_source_from_text() {
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();
        let kb = svc.root().join("k");

        let res = svc
            .add_raw_source(
                "k",
                SourceInput::Text {
                    text: "Lab note: HRV trend up after week of zone-2.".into(),
                    title: Some("HRV note".into()),
                },
                None,
            )
            .await
            .unwrap();

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
        let res = svc
            .add_raw_source(
                "k",
                SourceInput::File {
                    bytes: html.to_vec(),
                    filename: "x.html".into(),
                    mime: Some("text/html".into()),
                },
                None,
            )
            .await
            .unwrap();
        let kb = svc.root().join("k");
        let md =
            std::fs::read_to_string(kb.join(format!("raw/{}/source.md", res.source_id))).unwrap();
        assert!(md.contains("# H"));
    }

    #[tokio::test]
    async fn add_raw_source_reuses_existing_source_for_same_text_hash() {
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();
        let first = svc
            .add_raw_source(
                "k",
                SourceInput::Text {
                    text: "Same content".into(),
                    title: Some("First note".into()),
                },
                None,
            )
            .await
            .unwrap();

        let second = svc
            .add_raw_source(
                "k",
                SourceInput::Text {
                    text: "Same content".into(),
                    title: Some("Second note".into()),
                },
                None,
            )
            .await
            .unwrap();

        assert_eq!(first.source_id, second.source_id);

        let repo = GitRepo::open(&svc.root().join("k")).unwrap();
        let log = repo.log(10).unwrap();
        assert_eq!(log.len(), 2, "create + first add_raw_source only");
    }

    #[tokio::test]
    async fn get_graph_returns_cached_after_create_and_add() {
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();
        let g_empty = svc.get_graph("k").unwrap();
        assert!(g_empty.nodes.is_empty());
        svc.add_raw_source(
            "k",
            convert::SourceInput::Text {
                text: "note".into(),
                title: Some("N".into()),
            },
            None,
        )
        .await
        .unwrap();
        let kb = svc.root().join("k");
        // Source pages aren't written by add_raw_source — only raw/. So the graph
        // remains empty until a macro creates knowledge/sources/<id>.md (Plan 2).
        let g = svc.get_graph("k").unwrap();
        assert_eq!(g.nodes.len(), 0, "no knowledge pages yet");
        assert!(kb.join(".biorouter-knowledge/graph-cache.json").exists());
    }

    #[tokio::test]
    async fn lock_kb_serializes_writers() {
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();
        let svc1 = svc.clone();
        let svc2 = svc.clone();
        let h1 = tokio::spawn(async move {
            let _g = svc1.lock_kb("k").await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            std::time::Instant::now()
        });
        // Brief delay so h1 acquires the lock first.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let h2 = tokio::spawn(async move {
            let _g = svc2.lock_kb("k").await.unwrap();
            std::time::Instant::now()
        });
        let t1 = h1.await.unwrap();
        let t2 = h2.await.unwrap();
        assert!(
            t2 >= t1,
            "h2 must observe lock acquisition after h1 released"
        );
    }

    #[tokio::test]
    async fn reclassify_source_updates_meta() {
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();

        // Add a text source (no URL → falls back to Personal tier).
        let written = svc
            .add_raw_source(
                "k",
                SourceInput::Text {
                    text: "lab note".into(),
                    title: Some("note".into()),
                },
                None,
            )
            .await
            .unwrap();

        // Reclassify — same text, should still come back Personal.
        let cred = svc
            .reclassify_source("k", &written.source_id)
            .await
            .unwrap();
        assert_eq!(cred.tier, CredibilityTier::Personal);

        // Verify meta.yaml was updated.
        let kb = svc.root().join("k");
        let meta = raw::read_meta(&kb, &written.source_id).unwrap();
        assert_eq!(meta.credibility.tier, CredibilityTier::Personal);

        // A new commit was made.
        let repo = crate::knowledge::git::GitRepo::open(&kb).unwrap();
        let log = repo.log(10).unwrap();
        // create + add_raw + reclassify = 3 commits
        assert!(log.len() >= 3);
    }

    #[tokio::test]
    async fn override_credibility_writes_and_commits() {
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();

        let written = svc
            .add_raw_source(
                "k",
                SourceInput::Text {
                    text: "draft".into(),
                    title: Some("draft".into()),
                },
                None,
            )
            .await
            .unwrap();

        let override_cred = Credibility {
            tier: CredibilityTier::PeerReviewed,
            confidence: 0.99,
            publisher: Some("Nature".into()),
            venue: Some("Nature 2024".into()),
            doi: Some("10.1000/xyz".into()),
            retracted: false,
            reasoning: "Manual override: confirmed peer-reviewed publication.".into(),
            classifier_version: 1,
        };

        let returned = svc
            .override_credibility("k", &written.source_id, override_cred.clone())
            .unwrap();
        assert_eq!(returned.tier, CredibilityTier::PeerReviewed);
        assert_eq!(returned.doi.as_deref(), Some("10.1000/xyz"));

        // Verify meta.yaml persisted the override.
        let kb = svc.root().join("k");
        let meta = raw::read_meta(&kb, &written.source_id).unwrap();
        assert_eq!(meta.credibility.tier, CredibilityTier::PeerReviewed);
        assert_eq!(meta.credibility.doi.as_deref(), Some("10.1000/xyz"));

        // A commit was made with the override.
        let repo = crate::knowledge::git::GitRepo::open(&kb).unwrap();
        let log = repo.log(10).unwrap();
        assert!(log[0].summary.contains("override credibility"));
        assert_eq!(log[0].kind, ChangeKind::Manual);
    }

    // -----------------------------------------------------------------------
    // check_model tests
    // -----------------------------------------------------------------------

    use crate::knowledge::subagent::loop_::{Completer, LlmMessage, LlmReply};
    use async_trait::async_trait;
    use rmcp::model::Tool;

    struct OkCompleter;

    #[async_trait]
    impl Completer for OkCompleter {
        async fn complete(
            &self,
            _system: &str,
            _messages: &[LlmMessage],
            _tools: &[Tool],
        ) -> anyhow::Result<LlmReply> {
            Ok(LlmReply {
                text: "OK".to_string(),
                tool_calls: vec![],
            })
        }
    }

    struct ErrCompleter;

    #[async_trait]
    impl Completer for ErrCompleter {
        async fn complete(
            &self,
            _system: &str,
            _messages: &[LlmMessage],
            _tools: &[Tool],
        ) -> anyhow::Result<LlmReply> {
            anyhow::bail!("provider unreachable")
        }
    }

    #[tokio::test]
    async fn check_model_ok_with_mock_completer() {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.check_model(Box::new(OkCompleter)).await.unwrap();
    }

    #[tokio::test]
    async fn check_model_propagates_completer_error() {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        let err = svc.check_model(Box::new(ErrCompleter)).await.unwrap_err();
        assert!(
            err.to_string().contains("provider unreachable"),
            "error should mention underlying cause, got: {err}"
        );
    }

    #[tokio::test]
    async fn list_history_and_restore_roundtrip() {
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();
        svc.add_raw_source(
            "k",
            convert::SourceInput::Text {
                text: "first".into(),
                title: Some("a".into()),
            },
            None,
        )
        .await
        .unwrap();
        let history_after_one = svc.list_history("k", 10).unwrap();
        assert_eq!(history_after_one.len(), 2);
        let target = history_after_one.last().unwrap().commit_sha.clone();

        svc.add_raw_source(
            "k",
            convert::SourceInput::Text {
                text: "second".into(),
                title: Some("b".into()),
            },
            None,
        )
        .await
        .unwrap();
        let history_after_two = svc.list_history("k", 10).unwrap();
        assert_eq!(history_after_two.len(), 3);

        svc.restore_state("k", &target).unwrap();
        let history_after_restore = svc.list_history("k", 10).unwrap();
        assert_eq!(history_after_restore.len(), 4);
        assert_eq!(
            history_after_restore[0].kind,
            crate::knowledge::types::ChangeKind::Restore
        );
    }

    #[test]
    fn active_kb_persists_to_disk() -> anyhow::Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let svc = KnowledgeService::new(tmp.path().to_path_buf());
        assert!(svc.get_active_persisted()?.is_none());

        svc.set_active_persisted(Some("my-kb"))?;
        assert_eq!(svc.get_active_persisted()?.as_deref(), Some("my-kb"));

        // Setting again overwrites.
        svc.set_active_persisted(Some("other-kb"))?;
        assert_eq!(svc.get_active_persisted()?.as_deref(), Some("other-kb"));

        // Clearing removes the file.
        svc.set_active_persisted(None)?;
        assert!(svc.get_active_persisted()?.is_none());

        // Invalid IDs are rejected.
        let err = svc.set_active_persisted(Some("INVALID--KB"));
        assert!(err.is_err());
        Ok(())
    }

    // -----------------------------------------------------------------------
    // migrate_schema_if_needed tests
    // -----------------------------------------------------------------------

    /// Legacy schema fingerprint: minimal pre-Plan-5-Task-2 schema (no
    /// "Cross-reference rules" section).
    const LEGACY_SCHEMA: &str =
        "# Knowledge Base — Maintenance Schema\n\n## Layout\n\n- wiki/sources/...\n";

    #[test]
    fn migrate_schema_appends_cross_reference_rules_when_missing() {
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();
        let kb = svc.root().join("k");

        // Overwrite with the legacy schema and commit so we have a clean
        // baseline to migrate from.
        std::fs::write(kb.join("schema.md"), LEGACY_SCHEMA).unwrap();
        let repo = GitRepo::open(&kb).unwrap();
        repo.commit_all(ChangeKind::Manual, "seed legacy schema", None)
            .unwrap();
        let before = repo.log(10).unwrap().len();

        let migrated = svc.migrate_schema_if_needed("k").unwrap();
        assert!(migrated, "first call should migrate");

        let new_schema = std::fs::read_to_string(kb.join("schema.md")).unwrap();
        assert!(new_schema.contains("Cross-reference rules"));
        // Original content is preserved.
        assert!(new_schema.contains("wiki/sources/..."));

        let after = repo.log(10).unwrap().len();
        assert_eq!(after, before + 1, "exactly one migration commit added");
        assert!(repo.log(1).unwrap()[0].summary.contains("migrate schema"));
    }

    #[test]
    fn migrate_schema_is_noop_when_already_present() {
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();
        let kb = svc.root().join("k");

        // create_base already writes the current DEFAULT_SCHEMA, which
        // contains the cross-reference rules section.
        let original = std::fs::read_to_string(kb.join("schema.md")).unwrap();
        assert!(original.contains("Cross-reference rules"));
        let repo = GitRepo::open(&kb).unwrap();
        let before = repo.log(10).unwrap().len();

        let migrated = svc.migrate_schema_if_needed("k").unwrap();
        assert!(!migrated, "already-migrated KB should be a no-op");

        let after_schema = std::fs::read_to_string(kb.join("schema.md")).unwrap();
        assert_eq!(after_schema, original, "schema bytes unchanged");
        let after = repo.log(10).unwrap().len();
        assert_eq!(after, before, "no new commit");
    }

    #[test]
    fn migrate_schema_is_idempotent_across_calls() {
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();
        let kb = svc.root().join("k");
        std::fs::write(kb.join("schema.md"), LEGACY_SCHEMA).unwrap();
        let repo = GitRepo::open(&kb).unwrap();
        repo.commit_all(ChangeKind::Manual, "seed legacy schema", None)
            .unwrap();

        // First call migrates.
        assert!(svc.migrate_schema_if_needed("k").unwrap());
        // Second call is a no-op.
        assert!(!svc.migrate_schema_if_needed("k").unwrap());
        // Third call too.
        assert!(!svc.migrate_schema_if_needed("k").unwrap());
    }
}
