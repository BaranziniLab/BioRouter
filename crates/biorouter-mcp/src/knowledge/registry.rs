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
    let s = std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
    let doc: RegistryDoc = serde_yaml::from_str(&s)?;
    Ok(doc.bases)
}

pub fn register(root: &Path, entry: RegistryEntry) -> Result<()> {
    let mut bases = load(root)?;
    if let Some(existing) = bases.iter().find(|b| b.id == entry.id) {
        // #158: a bare "already registered" is accurate and useless when the row
        // is an ORPHAN — the directory is gone, so `kb_list_bases` does not show
        // the base, and the user is refused an id they cannot see, cannot read
        // and cannot delete. Name the orphan and where it is recorded, so the
        // refusal points somewhere.
        if !existing.path.exists() {
            anyhow::bail!(
                "kb-id '{}' is registered but its directory is missing ({}). The row is stale, \
                 which is why this id is neither listed nor creatable. Remove it from {} to \
                 free the id.",
                entry.id,
                existing.path.display(),
                registry_path(root).display()
            );
        }
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

pub fn replace(root: &Path, old_id: &str, entry: RegistryEntry) -> Result<()> {
    let mut bases = load(root)?;
    let Some(index) = bases.iter().position(|b| b.id == old_id) else {
        anyhow::bail!("kb-id '{old_id}' not found in registry");
    };

    if entry.id != old_id && bases.iter().any(|b| b.id == entry.id) {
        anyhow::bail!("kb-id '{}' already registered", entry.id);
    }

    bases[index] = entry;
    save(root, &bases)
}

fn save(root: &Path, bases: &[RegistryEntry]) -> Result<()> {
    std::fs::create_dir_all(root)?;
    let doc = RegistryDoc {
        bases: bases.to_vec(),
    };
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
        let e = RegistryEntry {
            id: "ms".into(),
            path: dir.path().join("ms"),
        };
        register(dir.path(), e.clone()).unwrap();
        assert_eq!(load(dir.path()).unwrap(), vec![e]);
    }

    #[test]
    fn register_rejects_duplicate() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ms");
        // #158: the base directory is created here, where before this test used a
        // path that never existed. That made it pass through the ORPHAN branch
        // rather than the duplicate one — it was asserting the right message from
        // the wrong path, and would have gone on passing if the plain duplicate
        // refusal were deleted entirely. The orphan case has its own test below.
        std::fs::create_dir_all(&path).unwrap();
        let e = RegistryEntry {
            id: "ms".into(),
            path,
        };
        register(dir.path(), e.clone()).unwrap();
        let err = register(dir.path(), e).unwrap_err();
        assert!(err.to_string().contains("already registered"));
    }

    /// #158. An orphan row — registered, directory gone — made an id
    /// permanently unusable: `kb_list_bases` hid it, `kb_create_base` refused it
    /// as taken, and the refusal named nothing the user could act on.
    #[test]
    fn a_registered_id_whose_directory_is_gone_says_so_and_says_where() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let missing = root.join("ghost-base");
        super::register(
            root,
            super::RegistryEntry {
                id: "ghost".into(),
                path: missing.clone(),
            },
        )
        .expect("first registration");

        // The directory never existed, which is the shape a partial create or an
        // externally removed base leaves behind.
        let err = super::register(
            root,
            super::RegistryEntry {
                id: "ghost".into(),
                path: missing.clone(),
            },
        )
        .expect_err("a second registration must still be refused");
        let msg = err.to_string();
        assert!(
            msg.contains("directory is missing"),
            "the refusal must say WHY the id is unavailable: {msg}"
        );
        assert!(msg.contains("registry.yaml"), "and where to fix it: {msg}");
    }

    /// The ordinary duplicate — a real base — keeps its plain refusal.
    #[test]
    fn a_registered_id_that_really_exists_keeps_the_plain_refusal() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let real = root.join("real-base");
        std::fs::create_dir_all(&real).unwrap();
        super::register(
            root,
            super::RegistryEntry {
                id: "real".into(),
                path: real.clone(),
            },
        )
        .expect("first registration");
        let err = super::register(
            root,
            super::RegistryEntry {
                id: "real".into(),
                path: real,
            },
        )
        .expect_err("duplicate refused");
        assert!(err.to_string().contains("already registered"), "{err}");
        assert!(!err.to_string().contains("directory is missing"), "{err}");
    }

    #[test]
    fn unregister_removes_entry() {
        let dir = tempfile::tempdir().unwrap();
        let e = RegistryEntry {
            id: "ms".into(),
            path: dir.path().join("ms"),
        };
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

    #[test]
    fn replace_updates_entry() {
        let dir = tempfile::tempdir().unwrap();
        register(
            dir.path(),
            RegistryEntry {
                id: "old".into(),
                path: dir.path().join("old"),
            },
        )
        .unwrap();

        replace(
            dir.path(),
            "old",
            RegistryEntry {
                id: "new".into(),
                path: dir.path().join("new"),
            },
        )
        .unwrap();

        assert_eq!(
            load(dir.path()).unwrap(),
            vec![RegistryEntry {
                id: "new".into(),
                path: dir.path().join("new"),
            }]
        );
    }
}
