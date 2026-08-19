use crate::knowledge::types::Manifest;
use anyhow::{Context, Result};
use std::path::Path;

const MANIFEST_FILE: &str = "manifest.yaml";

pub fn manifest_path(kb_root: &Path) -> std::path::PathBuf {
    kb_root.join(MANIFEST_FILE)
}

pub fn load(kb_root: &Path) -> Result<Manifest> {
    let p = manifest_path(kb_root);
    let s = std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
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

    /// The exact bytes on every `manifest.yaml` written to date. DR-12's (c):
    /// this is the file the next added field must not break, and the only way
    /// to know it does not is to keep a copy of it here that no future writer
    /// can quietly regenerate.
    const V1_ON_DISK: &str = "id: ms\n\
         name: MS Patient Analysis\n\
         color: '#5a6394'\n\
         created_at: 2023-11-14T22:13:20Z\n\
         schema_version: 1\n";

    #[test]
    fn a_manifest_with_only_todays_keys_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(manifest_path(dir.path()), V1_ON_DISK).unwrap();
        assert_eq!(load(dir.path()).unwrap(), sample());
    }

    #[test]
    fn all_six_of_todays_keys_load_including_the_optional_one() {
        // `default_model` is the only field a manifest may legitimately omit,
        // so the five-key file above and this six-key one are both shapes on
        // disk right now. Stage 3 adds a seventh, and it must break neither.
        let dir = tempfile::tempdir().unwrap();
        let with_model =
            format!("{V1_ON_DISK}default_model:\n  provider: anthropic\n  model: claude-opus-5\n");
        std::fs::write(manifest_path(dir.path()), with_model).unwrap();
        let m = load(dir.path()).unwrap();
        assert_eq!(
            m.default_model.as_ref().map(|r| r.model.as_str()),
            Some("claude-opus-5")
        );
        assert_eq!(m.schema_version, 1);
    }

    #[test]
    fn a_manifest_missing_schema_version_loads_at_the_oldest_generation() {
        // Not 0. A missing key means "written before anyone was counting", and
        // the migration ladder has to run for such a base, not skip it.
        let dir = tempfile::tempdir().unwrap();
        let without = V1_ON_DISK.replace("schema_version: 1\n", "");
        std::fs::write(manifest_path(dir.path()), &without).unwrap();
        assert_eq!(load(dir.path()).unwrap().schema_version, 1);
    }

    #[test]
    fn a_manifest_missing_created_at_loads_at_the_epoch_not_at_now() {
        // `Utc::now()` as the default would invent a fact and re-invent it on
        // every read, so the base would sort as the newest one in the list
        // forever. The epoch is honest about not knowing.
        let dir = tempfile::tempdir().unwrap();
        let without = V1_ON_DISK.replace("created_at: 2023-11-14T22:13:20Z\n", "");
        std::fs::write(manifest_path(dir.path()), &without).unwrap();
        let m = load(dir.path()).unwrap();
        assert_eq!(m.created_at, chrono::DateTime::UNIX_EPOCH);
        assert_eq!(m.id, "ms", "the rest of the manifest still read");
    }

    #[test]
    fn an_unknown_future_key_is_not_a_load_failure() {
        // What a downgrade looks like: a manifest written by a build that knew
        // about `format` (Stage 3), read by one that does not.
        let dir = tempfile::tempdir().unwrap();
        let with_future = format!("{V1_ON_DISK}format: biookf\n");
        std::fs::write(manifest_path(dir.path()), with_future).unwrap();
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
