use crate::knowledge::{
    convert, credibility,
    git::GitRepo,
    manifest, paths, raw, registry,
    types::{Manifest, RegistryEntry, SourceMeta},
};
use anyhow::{Context, Result};
use chrono::Utc;
use dashmap::DashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{Mutex, OwnedMutexGuard};

const DEFAULT_SCHEMA: &str = include_str!("schema_default.md");
const DEFAULT_INDEX: &str = "# Index\n\n_no pages yet_\n";
const DEFAULT_LOG: &str = "# Log\n\n";
const GITIGNORE: &str = "raw/*/original.*\n.biorouter-knowledge/.crossref-cache/\n";

#[derive(Clone)]
pub struct KnowledgeService {
    root: PathBuf,
    locks: Arc<DashMap<String, Arc<Mutex<()>>>>,
}

impl KnowledgeService {
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

    /// Acquire an exclusive lock for `kb_id`. Held until the returned guard is dropped.
    /// Used by macros to serialize concurrent writers against the same KB.
    pub async fn lock_kb(&self, kb_id: &str) -> OwnedMutexGuard<()> {
        let m = self
            .locks
            .entry(kb_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        m.lock_owned().await
    }

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
}

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
        let credibility = credibility::classify(&input, None).await?;

        let title = converted.title.clone().unwrap_or_else(|| match &input {
            convert::SourceInput::Text { title, .. } => {
                title.clone().unwrap_or_else(|| "Untitled note".into())
            }
            convert::SourceInput::Url(u) => u.clone(),
            convert::SourceInput::File { filename, .. } => filename.clone(),
        });

        let source_id = raw::new_source_id(&title);
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
        let summary = format!("ingested {source_id}");
        let delta = "+1 source";
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
}
