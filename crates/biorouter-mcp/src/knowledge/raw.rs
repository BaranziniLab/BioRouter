use crate::knowledge::types::SourceMeta;
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct RawWrite {
    pub source_id: String,
    pub source_md_path: String, // kb-relative path (raw/<id>/source.md)
    pub meta_path: String,
    /// The commit that made this raw source durable. `None` means the bytes
    /// already existed and deduplication did not create a new commit.
    pub commit_sha: Option<String>,
}

pub fn write_raw(
    kb_root: &Path,
    original_bytes: Option<&[u8]>,
    original_filename: Option<&str>,
    derived_md: &str,
    meta: SourceMeta,
) -> Result<RawWrite> {
    validate_source_id(&meta.id)?;
    let raw_dir = kb_root.join("raw").join(&meta.id);
    crate::knowledge::store::ensure_writable_path_confined(kb_root, &raw_dir)?;
    std::fs::create_dir_all(&raw_dir)?;
    if let (Some(bytes), Some(name)) = (original_bytes, original_filename) {
        let ext = Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("bin");
        crate::knowledge::store::write_atomically(&raw_dir.join(format!("original.{ext}")), bytes)?;
    }
    crate::knowledge::store::write_atomically(&raw_dir.join("source.md"), derived_md.as_bytes())?;
    let yaml = serde_yaml::to_string(&meta)?;
    crate::knowledge::store::write_atomically(&raw_dir.join("meta.yaml"), yaml.as_bytes())?;
    Ok(RawWrite {
        source_id: meta.id.clone(),
        source_md_path: format!("raw/{}/source.md", meta.id),
        meta_path: format!("raw/{}/meta.yaml", meta.id),
        commit_sha: None,
    })
}

pub fn read_meta(kb_root: &Path, source_id: &str) -> Result<SourceMeta> {
    validate_source_id(source_id)?;
    let logical = format!("raw/{source_id}/meta.yaml");
    let p = crate::knowledge::store::resolve_readable_path(kb_root, &logical)?;
    let s = std::fs::read_to_string(&p)?;
    Ok(serde_yaml::from_str(&s)?)
}

pub fn list_sources(kb_root: &Path) -> Result<Vec<SourceMeta>> {
    let dir = kb_root.join("raw");
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() && !file_type.is_symlink() {
            let id = entry.file_name().to_string_lossy().to_string();
            if validate_source_id(&id).is_err() {
                continue;
            }
            if let Ok(m) = read_meta(kb_root, &id) {
                if m.id != id {
                    continue;
                }
                out.push(m);
            }
        }
    }
    Ok(out)
}

pub(crate) fn validate_source_id(source_id: &str) -> Result<()> {
    let plain_component = !source_id.is_empty()
        && source_id != "."
        && source_id != ".."
        && !source_id.contains(['/', '\\', ':', '\0'])
        && !Path::new(source_id).is_absolute();
    anyhow::ensure!(
        plain_component,
        "invalid raw source id '{source_id}': expected one plain path component"
    );
    Ok(())
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

pub fn new_source_id(title: &str) -> String {
    let slug = title
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .chars()
        .take(40)
        .collect::<String>();
    let id = uuid::Uuid::new_v4();
    let id_str = id.to_string();
    if slug.is_empty() {
        format!("src-{}", id_str.get(..8).unwrap_or(&id_str))
    } else {
        format!("{slug}-{}", id_str.get(..6).unwrap_or(&id_str))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::{
        service::KnowledgeService,
        types::{Credibility, CredibilityTier},
    };
    use chrono::Utc;

    fn fresh() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("k", "K", None).unwrap();
        let kb = dir.path().join("k");
        (dir, kb)
    }

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
                publisher: None,
                venue: None,
                doi: None,
                retracted: false,
                reasoning: "test".into(),
                classifier_version: 1,
            },
        }
    }

    #[test]
    fn write_then_read_meta() {
        let (_d, kb) = fresh();
        let m = sample_meta("paper-x");
        let w = write_raw(
            &kb,
            Some(b"<html>x</html>"),
            Some("x.html"),
            "# X\n",
            m.clone(),
        )
        .unwrap();
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
        let mut ids: Vec<_> = list_sources(&kb)
            .unwrap()
            .into_iter()
            .map(|m| m.id)
            .collect();
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

    #[test]
    fn raw_source_ids_cannot_escape_the_knowledge_base() {
        let (_d, kb) = fresh();
        for id in [
            "",
            ".",
            "..",
            "../../outside",
            "/tmp/outside",
            r"..\outside",
        ] {
            let error = write_raw(&kb, None, None, "md", sample_meta(id)).unwrap_err();
            assert!(
                error.to_string().contains("invalid raw source id"),
                "{id}: {error}"
            );
        }
    }

    #[test]
    fn list_sources_rejects_metadata_that_renames_its_directory() {
        let (_d, kb) = fresh();
        let dir = kb.join("raw/safe");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("source.md"), "source").unwrap();
        std::fs::write(
            dir.join("meta.yaml"),
            serde_yaml::to_string(&sample_meta("../../outside")).unwrap(),
        )
        .unwrap();

        assert!(list_sources(&kb).unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn write_raw_refuses_a_symlinked_raw_directory() {
        let (_d, kb) = fresh();
        let outside = tempfile::tempdir().unwrap();
        std::fs::remove_dir(kb.join("raw")).unwrap();
        std::os::unix::fs::symlink(outside.path(), kb.join("raw")).unwrap();

        let error = write_raw(&kb, None, None, "md", sample_meta("safe")).unwrap_err();
        assert!(error.to_string().contains("symbolic links"));
        assert!(!outside.path().join("safe/source.md").exists());
    }
}
