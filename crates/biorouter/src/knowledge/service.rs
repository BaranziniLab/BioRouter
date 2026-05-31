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
        std::fs::create_dir_all(paths::kb_wiki_dir(&self.root, id).join("entities"))?;
        std::fs::create_dir_all(paths::kb_wiki_dir(&self.root, id).join("concepts"))?;
        std::fs::create_dir_all(paths::kb_wiki_dir(&self.root, id).join("sources"))?;
        std::fs::create_dir_all(paths::kb_wiki_dir(&self.root, id).join("notes"))?;
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
        assert!(kb.join("wiki/entities").exists());
        assert!(kb.join("wiki/concepts").exists());
        assert!(kb.join("wiki/sources").exists());
        assert!(kb.join("wiki/notes").exists());
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
