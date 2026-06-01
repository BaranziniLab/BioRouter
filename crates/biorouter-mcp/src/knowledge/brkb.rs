use anyhow::{Context, Result};
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use zip::{write::FileOptions, ZipArchive, ZipWriter};

/// Pack a knowledge base directory (including .git, manifest.yaml, raw/, knowledge/, .biorouter-knowledge/)
/// into a .brkb zip and write the bytes to `out`. Walks the directory tree.
pub fn export<W: Write + Seek>(kb_root: &Path, out: &mut W) -> Result<()> {
    let mut zip = ZipWriter::new(out);
    let opts = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let kb_id = kb_root
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("kb root has no basename"))?
        .to_string_lossy()
        .to_string();
    walk(kb_root, kb_root, &kb_id, &mut zip, opts)?;
    zip.finish().context("finish zip")?;
    Ok(())
}

fn walk<W: Write + Seek>(
    base: &Path,
    dir: &Path,
    prefix: &str,
    zip: &mut ZipWriter<W>,
    opts: FileOptions,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let rel = path.strip_prefix(base)?;
        let archive_path = format!("{prefix}/{}", rel.to_string_lossy());
        if path.is_dir() {
            zip.add_directory(&archive_path, opts)?;
            walk(base, &path, prefix, zip, opts)?;
        } else {
            zip.start_file(&archive_path, opts)?;
            let mut f = std::fs::File::open(&path)?;
            std::io::copy(&mut f, zip)?;
        }
    }
    Ok(())
}

/// Unpack a .brkb zip into a fresh directory under `knowledge_root` and return the new kb_id.
/// The .brkb is expected to contain exactly one top-level directory (the kb_id at export time).
/// If that id collides with an existing KB at the destination, suffix with `-N` to disambiguate.
pub fn import<R: Read + Seek>(zip_bytes: R, knowledge_root: &Path) -> Result<String> {
    let mut archive = ZipArchive::new(zip_bytes).context("open zip archive")?;
    // Detect the single top-level directory.
    let mut top_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    for i in 0..archive.len() {
        let name = archive.by_index(i)?.name().to_string();
        if let Some(first) = name.split('/').next() {
            if !first.is_empty() {
                top_names.insert(first.to_string());
            }
        }
    }
    let original_id = if top_names.len() == 1 {
        top_names.into_iter().next().unwrap()
    } else {
        anyhow::bail!(
            "brkb must contain exactly one top-level directory, found {}",
            top_names.len()
        );
    };
    // Resolve a non-colliding id.
    let mut id = original_id.clone();
    let mut suffix = 1;
    while knowledge_root.join(&id).exists() {
        suffix += 1;
        id = format!("{original_id}-{suffix}");
    }
    // Extract.
    let target = knowledge_root.join(&id);
    std::fs::create_dir_all(&target)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let entry_name = entry.name().to_string();
        let rel: PathBuf = entry_name
            .strip_prefix(&format!("{original_id}/"))
            .unwrap_or(entry_name.as_str())
            .into();
        let dest = target.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&dest)?;
        } else {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut f = std::fs::File::create(&dest)?;
            std::io::copy(&mut entry, &mut f)?;
        }
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::service::KnowledgeService;

    #[test]
    fn export_then_import_preserves_files() {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("orig", "Orig", None).unwrap();
        // add some pages
        let kb_root = dir.path().join("orig");
        std::fs::write(
            kb_root.join("knowledge").join("entities").join("hrv.md"),
            "---\ntitle: HRV\n---\nbody",
        )
        .unwrap();

        let bytes = svc.export_brkb("orig").unwrap();
        assert!(bytes.len() > 100, "zip has some content");

        // Import into a new root, expect a non-colliding id.
        let dir2 = tempfile::tempdir().unwrap();
        let svc2 = KnowledgeService::new(dir2.path().to_path_buf());
        let new_id = svc2.import_brkb(&bytes).unwrap();
        assert_eq!(new_id, "orig");
        assert!(dir2.path().join("orig").join("manifest.yaml").exists());
        assert!(dir2
            .path()
            .join("orig")
            .join("knowledge")
            .join("entities")
            .join("hrv.md")
            .exists());
        assert!(
            dir2.path().join("orig").join(".git").exists(),
            "git dir travels with the zip"
        );
        // Registry has it.
        let bases = svc2.list_bases().unwrap();
        assert!(bases.iter().any(|b| b.id == "orig"));
    }

    #[test]
    fn import_assigns_suffix_on_collision() {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("dup", "Dup", None).unwrap();
        let bytes = svc.export_brkb("dup").unwrap();
        // Import into the SAME root — should collide.
        let new_id = svc.import_brkb(&bytes).unwrap();
        assert_eq!(new_id, "dup-2");
        assert!(dir.path().join("dup").exists());
        assert!(dir.path().join("dup-2").exists());
    }
}
