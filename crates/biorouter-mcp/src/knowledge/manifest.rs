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
