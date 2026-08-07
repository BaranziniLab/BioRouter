use anyhow::{Context, Result};
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use zip::{write::FileOptions, ZipArchive, ZipWriter};

/// The archive-borne provenance marker (issue #56, decision 2a).
///
/// It rides INSIDE the single top-level directory, because [`import`] bails
/// unless there is exactly one and a sibling entry would break every archive.
/// It is written straight into the `ZipWriter` after the disk walk, so the KB's
/// git tree never gains a file.
///
/// It is read as a **floor**, never as a value: a hostile archive's only power
/// is to over-classify itself. Absent or malformed means "unknown", which is
/// the importer's own tier and is the pre-#56 behaviour, so a foreign `.brkb`
/// is unaffected.
#[derive(serde::Serialize, serde::Deserialize)]
struct Provenance {
    schema: u32,
    tier: String,
    /// The institutions whose content this base holds (issue #56, DR-26 /
    /// Task 50). ⚠ **An archive is a transfer**, so DR-26's third axis has to
    /// ride with it: without this a UCSF chat could export a base it owns
    /// (permitted — it is the owner) and any other institution's chat could
    /// import the archive and read the content with no gate crossed, because an
    /// unclaimed base is reachable from every private model.
    ///
    /// `#[serde(default)]` — an archive written before this field existed
    /// carries no owners, which is the same **Missing** direction
    /// [`crate::knowledge::tier::affiliation`] takes for a store that predates
    /// the axis (AR-2's accepted fail-open), not the **Unreadable** one. The
    /// unknown-ownership case never reaches an archive at all: `export_brkb`
    /// refuses to package a base whose owners it cannot establish.
    #[serde(default)]
    owners: Vec<String>,
}

/// Pack a knowledge base directory (including .git, manifest.yaml, raw/, knowledge/, .biorouter-knowledge/)
/// into a .brkb zip and write the bytes to `out`. Walks the directory tree.
///
/// `is_private` and `owners` are stamped into the `<kb_id>/.brkb-provenance`
/// entry.
pub fn export<W: Write + Seek>(
    kb_root: &Path,
    out: &mut W,
    is_private: bool,
    owners: &std::collections::BTreeSet<String>,
) -> Result<()> {
    let mut zip = ZipWriter::new(out);
    let opts = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let kb_id = kb_root
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("kb root has no basename"))?
        .to_string_lossy()
        .to_string();
    walk(kb_root, kb_root, &kb_id, &mut zip, opts)?;
    let provenance = serde_json::to_vec(&Provenance {
        // 1 -> 2 with `owners`, and a label rather than a gate for the reason
        // `tier::SCHEMA` is one: an older binary parses `tier` exactly as before
        // and never sees the new field, where a reader that refused an
        // unfamiliar number would read every archive this build writes as
        // having no marker at all — which is the fail-open direction.
        schema: 2,
        tier: if is_private { "private" } else { "public" }.to_string(),
        owners: owners.iter().cloned().collect(),
    })?;
    zip.start_file(format!("{kb_id}/{PROVENANCE_ENTRY}"), opts)?;
    zip.write_all(&provenance)?;
    zip.finish().context("finish zip")?;
    Ok(())
}

/// The archive-relative name of the marker. Spelled here once; `import` matches
/// it twice (read, then skip).
const PROVENANCE_ENTRY: &str = ".brkb-provenance";

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

/// Validate that joining `rel` onto `target` cannot escape `target`.
///
/// Rejects any path component that is `..`, an absolute root, or a Windows drive
/// prefix. This prevents ZIP-slip attacks where a crafted archive entry such as
/// `legit-id/../../../etc/cron.d/evil` would write outside the extraction root.
fn safe_join(target: &Path, rel: &Path) -> Result<PathBuf> {
    for component in rel.components() {
        match component {
            std::path::Component::Normal(_) | std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                anyhow::bail!("path traversal: '..' component in archive entry");
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                anyhow::bail!("absolute path component in archive entry");
            }
        }
    }
    Ok(target.join(rel))
}

/// What an import landed on: the new id, and what the archive claimed about
/// itself on both of issue #56's axes.
///
/// A struct rather than a tuple because the two provenance fields are read the
/// same way and must stay together — `import_brkb` raises to
/// `max(marker, importer)` on the tier and to the UNION on the affiliation, and
/// a caller that used one and dropped the other is exactly the laundering path
/// [`Provenance::owners`] exists to close.
#[derive(Debug)]
pub struct Imported {
    pub id: String,
    /// The archive's privacy claim, `None` when the marker is absent or
    /// malformed. A FLOOR: it can only raise the new base, never lower it.
    pub provenance_private: Option<bool>,
    /// The institutions the archive says its content belongs to. Empty for an
    /// archive with no marker, or one written before DR-26.
    pub owners: std::collections::BTreeSet<String>,
}

/// Unpack a .brkb zip into a fresh directory under `knowledge_root` and return
/// the new kb_id together with the archive's provenance (issue #56).
///
/// The .brkb is expected to contain exactly one top-level directory (the kb_id at export time).
/// If that id collides with an existing KB at the destination, suffix with `-N` to disambiguate.
pub fn import<R: Read + Seek>(zip_bytes: R, knowledge_root: &Path) -> Result<Imported> {
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
    // Resolve a non-colliding id. Issue #56: a directory is not the only thing
    // that claims an id — the tier store can hold an entry for a base with no
    // directory (`tier::raise_unlocked` registers ids that have not been
    // created), and an import that landed on one would be classified by a base
    // that never existed rather than by its own provenance.
    let mut id = original_id.clone();
    let mut suffix = 1;
    while knowledge_root.join(&id).exists()
        || crate::knowledge::tier::has_entry_unlocked(knowledge_root, &id)
    {
        suffix += 1;
        id = format!("{original_id}-{suffix}");
    }
    // Extract.
    let target = knowledge_root.join(&id);
    std::fs::create_dir_all(&target)?;
    let mut provenance_private: Option<bool> = None;
    let mut owners: std::collections::BTreeSet<String> = Default::default();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let entry_name = entry.name().to_string();
        let rel: PathBuf = entry_name
            .strip_prefix(&format!("{original_id}/"))
            .unwrap_or(entry_name.as_str())
            .into();
        // Issue #56. Read the marker and SKIP it: it is provenance about the
        // archive, not a file of the knowledge base, so it must not be
        // extracted (a re-export would then carry a stale disk copy).
        if rel == Path::new(PROVENANCE_ENTRY) {
            let mut raw = String::new();
            if entry.read_to_string(&mut raw).is_ok() {
                if let Ok(p) = serde_json::from_str::<Provenance>(&raw) {
                    provenance_private = Some(p.tier == "private");
                    // Normalised on the way in, exactly as the wire id is
                    // (`affiliation::caller_affiliation`): an archive written by
                    // hand, or by a build whose normaliser differed, must not
                    // land a `UCSF` that mismatches the `ucsf` every model
                    // states. Empty ids are dropped rather than recorded — an
                    // owner nothing can ever match would make the base
                    // permanently unreachable with no declassification path.
                    owners.extend(
                        p.owners
                            .into_iter()
                            .map(|o| crate::knowledge::affiliation::normalize_institution(&o))
                            .filter(|o| !o.is_empty()),
                    );
                }
            }
            continue;
        }
        // Reject any path component that could escape the extraction root.
        let dest = safe_join(&target, &rel)?;
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
    // The archived manifest carries the *original* base id. When the import
    // landed under a deduplicated id (a collision with an existing base),
    // rewrite the manifest so its `id` matches the new folder / registry id.
    // Otherwise the registry points at `<id>` while the manifest still says
    // `<original_id>`, so the UI lists the imported base under the original id
    // and it visually collides with the source base.
    if id != original_id {
        if let Ok(mut m) = crate::knowledge::manifest::load(&target) {
            m.id = id.clone();
            crate::knowledge::manifest::save(&target, &m)?;
        }
    }
    Ok(Imported {
        id,
        provenance_private,
        owners,
    })
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
        let new_id = svc2
            .import_brkb(
                &bytes,
                false,
                &crate::knowledge::affiliation::CallerAffiliation::Unstated,
            )
            .unwrap();
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

    /// Issue #56 DR-26 / Task 50. The owner set survives the archive, and an
    /// archive written before the field existed still imports — carrying no
    /// owners, which is the **Missing** direction (AR-2's accepted fail-open),
    /// not a refusal.
    #[test]
    fn the_provenance_marker_round_trips_the_owner_set() {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("orig", "Orig", None).unwrap();
        let kb_root = dir.path().join("orig");

        let owners: std::collections::BTreeSet<String> =
            ["stanford".to_string(), "ucsf".to_string()].into();
        let mut buf = std::io::Cursor::new(Vec::new());
        export(&kb_root, &mut buf, true, &owners).unwrap();
        let bytes = buf.into_inner();

        let dest = tempfile::tempdir().unwrap();
        let imported = import(std::io::Cursor::new(bytes), dest.path()).unwrap();
        assert_eq!(imported.provenance_private, Some(true));
        assert_eq!(imported.owners, owners, "the owner set did not survive");

        // A pre-DR-26 archive: the marker exists but has no `owners` field.
        let dest2 = tempfile::tempdir().unwrap();
        let mut buf2 = std::io::Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut buf2);
            let opts = FileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zip.start_file("old/knowledge/x.md", opts).unwrap();
            zip.write_all(b"body").unwrap();
            zip.start_file(format!("old/{PROVENANCE_ENTRY}"), opts)
                .unwrap();
            zip.write_all(br#"{"schema":1,"tier":"private"}"#).unwrap();
            zip.finish().unwrap();
        }
        let old = import(std::io::Cursor::new(buf2.into_inner()), dest2.path()).unwrap();
        assert_eq!(old.provenance_private, Some(true));
        assert!(old.owners.is_empty(), "{:?}", old.owners);
    }

    #[test]
    fn import_assigns_suffix_on_collision() {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("dup", "Dup", None).unwrap();
        let bytes = svc.export_brkb("dup").unwrap();
        // Import into the SAME root — should collide.
        let new_id = svc
            .import_brkb(
                &bytes,
                false,
                &crate::knowledge::affiliation::CallerAffiliation::Unstated,
            )
            .unwrap();
        assert_eq!(new_id, "dup-2");
        assert!(dir.path().join("dup").exists());
        assert!(dir.path().join("dup-2").exists());
    }

    #[test]
    fn import_rejects_path_traversal() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            // Single top-level dir + one entry whose relative path escapes via "..".
            zip.add_directory("evil-kb", opts).unwrap();
            zip.start_file("evil-kb/../escaped.txt", opts).unwrap();
            zip.write_all(b"pwned").unwrap();
            zip.finish().unwrap();
        }
        let bytes = buf.into_inner();
        let cursor = std::io::Cursor::new(bytes);
        let err = import(cursor, dir.path()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("traversal") || msg.contains("..") || msg.contains("absolute"),
            "unexpected error: {msg}"
        );
        // Confirm no file was written outside the extraction root.
        assert!(
            !dir.path().parent().unwrap().join("escaped.txt").exists(),
            "escaped.txt must not exist outside the extraction dir"
        );
    }

    #[test]
    fn safe_join_rejects_parent_dir_component() {
        let target = std::path::Path::new("/tmp/safe-root");
        let rel = std::path::Path::new("../escape");
        let err = safe_join(target, rel).unwrap_err();
        assert!(err.to_string().contains("traversal") || err.to_string().contains(".."));
    }

    #[test]
    fn safe_join_allows_normal_paths() {
        let target = std::path::Path::new("/tmp/safe-root");
        let rel = std::path::Path::new("subdir/file.txt");
        let dest = safe_join(target, rel).unwrap();
        assert_eq!(dest, std::path::Path::new("/tmp/safe-root/subdir/file.txt"));
    }
}
