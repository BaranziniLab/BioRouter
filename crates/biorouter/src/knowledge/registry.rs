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
