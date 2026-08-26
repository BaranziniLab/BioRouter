use anyhow::{Context, Result};
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use zip::{write::FileOptions, ZipArchive, ZipWriter};

/// The archive-borne provenance marker (issue #56, decision 2a).
///
/// It rides INSIDE the single top-level directory, because [`stage_import`] bails
/// unless there is exactly one and a sibling entry would break every archive.
/// It is written straight into the `ZipWriter` after the disk walk, so the KB's
/// git tree never gains a file.
///
/// It is read as a **floor**, never as a value: a hostile archive's only power
/// is to over-classify itself. Missing or unreadable provenance is refused,
/// because treating either as public and unowned would let corruption turn a
/// private export into a base every public or unrelated institutional model can
/// read.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
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
    owners: Vec<String>,
    /// Which profile the packed bundle is written in (DR-18), or `None` for an
    /// archive written before this field existed and for a **legacy** base,
    /// which genuinely declares no profile ([`crate::knowledge::types::Manifest::profile`]).
    ///
    /// ⚠ **An `Option<String>`, not an `Option<KbFormat>`, and the whole of
    /// DR-18 rests on it.** `KbFormat`'s own `Deserialize` is lenient by design
    /// — DR-12 traces what a `manifest.yaml` that fails to load costs the user
    /// — so a typed field here would read a profile this build has never heard
    /// of as plain `okf`, and [`stage_import`] could never refuse. The unknown word
    /// has to survive as itself all the way to the check.
    ///
    /// It is read as a **refusal**, where `tier` is read as a floor and
    /// `owners` as a union. The asymmetry is the point: over-classifying is a
    /// hostile archive's only power on those two axes, whereas a profile this
    /// build cannot read is a statement about whether the *content* is legible
    /// at all, and extracting it anyway lands a base whose pages nothing here
    /// can read and DR-22/DR-26 give no way to convert.
    format: Option<String>,
}

const PROVENANCE_SCHEMA: u32 = 3;

#[derive(Debug, thiserror::Error)]
#[error("invalid knowledge archive: {reason}")]
pub struct InvalidKnowledgeArchive {
    reason: String,
}

impl InvalidKnowledgeArchive {
    pub(crate) fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

fn invalid_archive(reason: impl Into<String>) -> anyhow::Error {
    InvalidKnowledgeArchive::new(reason).into()
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
    let root_metadata =
        std::fs::symlink_metadata(kb_root).context("inspect knowledge base root")?;
    if root_metadata.file_type().is_symlink() {
        anyhow::bail!("knowledge base export refuses a symbolic-link root");
    }
    let canonical_root =
        std::fs::canonicalize(kb_root).context("canonicalize knowledge base root")?;
    let mut zip = ZipWriter::new(out);
    let opts = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let kb_id = kb_root
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("kb root has no basename"))?
        .to_string_lossy()
        .to_string();
    walk(kb_root, kb_root, &canonical_root, &kb_id, &mut zip, opts)?;
    let provenance = serde_json::to_vec(&Provenance {
        // 1 -> 2 with `owners`, and a label rather than a gate for the reason
        // `tier::SCHEMA` is one: an older binary parses `tier` exactly as before
        // and never sees the new field, where a reader that refused an
        // unfamiliar number would read every archive this build writes as
        // having no marker at all — which is the fail-open direction.
        //
        // 2 -> 3 with `format`, on the same terms: an older binary ignores it
        // and imports exactly as it did before, which is correct — the archives
        // it can then mis-read are the ones this field was added to catch, and
        // no numbering here can teach a build that shipped first about a profile
        // invented after it.
        schema: PROVENANCE_SCHEMA,
        tier: if is_private { "private" } else { "public" }.to_string(),
        owners: owners.iter().cloned().collect(),
        // Read off the tree being packed rather than taken as an argument
        // (DR-18): the marker's job is to describe *this bundle*, and a
        // caller-supplied value would be a second answer that can disagree with
        // the `manifest.yaml` sitting inside the same archive.
        //
        // `profile()` and not `format`, because DR-6's trap is reached from
        // here too: the field reads `okf` on every `manifest.yaml` written
        // before Stage 3, so an archive of a legacy base would otherwise claim
        // to be an OKF bundle. `None` is the honest marker for one, and it is
        // also what a pre-Stage-6 archive carries — both mean "this says
        // nothing about its profile", which is the case [`stage_import`] passes.
        format: crate::knowledge::manifest::load(kb_root)
            .ok()
            .and_then(|m| m.profile())
            .map(|f| f.as_str().to_string()),
    })?;
    zip.start_file(format!("{kb_id}/{PROVENANCE_ENTRY}"), opts)?;
    zip.write_all(&provenance)?;
    zip.finish().context("finish zip")?;
    Ok(())
}

/// The archive-relative name of the marker. Spelled here once; `import` matches
/// it twice (read, then skip).
const PROVENANCE_ENTRY: &str = ".brkb-provenance";
pub const MAX_ARCHIVE_FILE_BYTES: u64 = 128 * 1024 * 1024;
pub const MAX_ARCHIVE_HTTP_BODY_BYTES: usize = MAX_ARCHIVE_FILE_BYTES as usize + 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 10_000;
const MAX_ARCHIVE_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ARCHIVE_EXPANDED_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ARCHIVE_COMPRESSION_RATIO: u64 = 500;
const MAX_PROVENANCE_BYTES: u64 = 64 * 1024;

fn read_bounded_archive(
    mut reader: impl Read,
    advertised_len: u64,
    max_bytes: u64,
) -> Result<Vec<u8>> {
    if advertised_len > max_bytes {
        return Err(invalid_archive(format!(
            "compressed archive exceeds the {} MiB limit",
            max_bytes / (1024 * 1024)
        )));
    }
    let mut bytes = Vec::with_capacity(advertised_len as usize);
    reader
        .by_ref()
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| invalid_archive(format!("cannot read archive bytes: {error}")))?;
    if bytes.len() as u64 > max_bytes {
        return Err(invalid_archive(format!(
            "compressed archive grew beyond the {} MiB limit while it was being read",
            max_bytes / (1024 * 1024)
        )));
    }
    Ok(bytes)
}

pub fn read_archive_path(path: &Path) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path).map_err(|error| {
        invalid_archive(format!("cannot open archive '{}': {error}", path.display()))
    })?;
    let metadata = file.metadata().map_err(|error| {
        invalid_archive(format!(
            "cannot inspect archive '{}': {error}",
            path.display()
        ))
    })?;
    if !metadata.is_file() {
        return Err(invalid_archive(format!(
            "archive path '{}' is not a regular file",
            path.display()
        )));
    }
    read_bounded_archive(file, metadata.len(), MAX_ARCHIVE_FILE_BYTES)
}

fn validate_archive_entry(entry: &zip::read::ZipFile<'_>) -> Result<()> {
    let expanded = entry.size();
    if expanded > MAX_ARCHIVE_ENTRY_BYTES {
        return Err(invalid_archive(
            "archive entry exceeds the 64 MiB expanded-size limit",
        ));
    }
    if expanded > 0 {
        let compressed = entry.compressed_size();
        if compressed == 0 || expanded / compressed.max(1) > MAX_ARCHIVE_COMPRESSION_RATIO {
            return Err(invalid_archive(
                "archive entry exceeds the safe compression-ratio limit",
            ));
        }
    }
    if entry
        .unix_mode()
        .is_some_and(|mode| mode & 0o170000 == 0o120000)
    {
        return Err(invalid_archive("archive contains a symbolic-link entry"));
    }
    Ok(())
}

/// The name a file at `rel` (relative to the KB root) gets inside the archive.
///
/// ⚠ Built by joining `rel`'s **components** with `/`, never from its display
/// form, and that is a portability rule rather than a preference. A zip entry
/// name is *defined* to use forward slashes (APPNOTE 4.4.17.1), and
/// `Path::to_string_lossy` hands back the platform's separator: on Windows the
/// walk produced `omop/knowledge\x.md`, so an archive written there carried
/// entries that every other platform reads as one file whose name contains a
/// backslash, not as a file inside `knowledge/`. It surfaced as
/// `a_models_export_of_a_private_base_lands_inside_the_knowledge_root` failing
/// on windows-latest the moment the os-error-33 fix let the export get far
/// enough to be inspected, and the archives it had been writing were already
/// wrong.
///
/// ⚠ Not observable from macOS or Linux against a real directory walk: there
/// `to_string_lossy` and this agree for every path `read_dir` can produce. The
/// unit test below reaches the difference from any platform by handing it a
/// path whose display form and component list differ (`a//b`, `./a`).
///
/// Only `Normal` components are emitted, because an entry name is a sequence of
/// plain names and nothing else. `Path::components` keeps a *leading* `.`
/// rather than normalising it away, and `RootDir` or a Windows `Prefix` would
/// be an absolute path. None of those can come out of `walk`, whose `rel` is a
/// `strip_prefix` of a directory entry, so this is the shape of the output
/// being stated rather than a case being handled: [`safe_join`] refuses the
/// same set on the way back in.
fn archive_name(prefix: &str, rel: &Path) -> String {
    let mut out = String::from(prefix);
    for component in rel.components() {
        if let std::path::Component::Normal(name) = component {
            out.push('/');
            out.push_str(&name.to_string_lossy());
        }
    }
    out
}

fn walk<W: Write + Seek>(
    base: &Path,
    dir: &Path,
    canonical_base: &Path,
    prefix: &str,
    zip: &mut ZipWriter<W>,
    opts: FileOptions,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        let rel = path.strip_prefix(base)?;
        // ⚠ The exporter is holding this file's lock **right now**, so reading
        // it is reading through its own lock. `kb_export` takes the KB write
        // lock and only then calls `export_brkb`, and the lock is
        // `fs2::lock_exclusive`, i.e. `LockFileEx` on Windows: a `ReadFile`
        // from the second handle this walk opens is refused with os error 33,
        // "another process has locked a portion of the file", and every model
        // export of every base failed there. Unix never noticed, because
        // `flock` is advisory and does not govern `read(2)` at all, so this
        // was a Windows-only *product* failure that three tests found and no
        // amount of macOS running could.
        //
        // Skipping is right regardless of the lock. A `write.lock` is transient
        // per-machine state whose contents are meaningless (`git::stage_all`
        // already refuses to commit it and `.gitignore` already lists it), so
        // an archive that carried it would be shipping one machine's lock file
        // to another. It is excluded, not merely tolerated.
        if crate::knowledge::paths::is_kb_write_lock(rel) {
            continue;
        }
        if file_type.is_symlink() {
            anyhow::bail!(
                "knowledge base export refuses symbolic link {}",
                rel.display()
            );
        }
        let canonical_path = std::fs::canonicalize(&path)
            .with_context(|| format!("canonicalize knowledge entry {}", rel.display()))?;
        if !canonical_path.starts_with(canonical_base) {
            anyhow::bail!(
                "knowledge base export entry resolves outside its base: {}",
                rel.display()
            );
        }
        let archive_path = archive_name(prefix, rel);
        if file_type.is_dir() {
            zip.add_directory(&archive_path, opts)?;
            walk(base, &path, canonical_base, prefix, zip, opts)?;
        } else if file_type.is_file() {
            zip.start_file(&archive_path, opts)?;
            let mut f = std::fs::File::open(&path)?;
            std::io::copy(&mut f, zip)?;
        } else {
            anyhow::bail!(
                "knowledge base export refuses non-file entry {}",
                rel.display()
            );
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
                return Err(invalid_archive(
                    "path traversal: '..' component in archive entry",
                ));
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return Err(invalid_archive("absolute path component in archive entry"));
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
    /// The archive's privacy claim. Missing or malformed provenance is refused.
    pub provenance_private: bool,
    /// The institutions the archive says its content belongs to.
    pub owners: std::collections::BTreeSet<String>,
}

pub(crate) struct StagedImport {
    pub imported: Imported,
    pub staged_path: PathBuf,
    pub final_path: PathBuf,
}

fn read_provenance<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    original_id: &str,
) -> Result<Provenance> {
    let mut entry = archive
        .by_name(&format!("{original_id}/{PROVENANCE_ENTRY}"))
        .map_err(|error| invalid_archive(format!("missing provenance marker: {error}")))?;
    validate_archive_entry(&entry)?;
    if entry.size() > MAX_PROVENANCE_BYTES {
        return Err(invalid_archive(
            "provenance marker exceeds the 64 KiB limit",
        ));
    }
    let mut raw = String::new();
    entry
        .read_to_string(&mut raw)
        .map_err(|error| invalid_archive(format!("cannot read provenance marker: {error}")))?;
    let provenance: Provenance = serde_json::from_str(&raw)
        .map_err(|error| invalid_archive(format!("malformed provenance marker: {error}")))?;
    if provenance.schema != PROVENANCE_SCHEMA {
        return Err(invalid_archive(format!(
            "unsupported provenance schema {}; this build requires schema {PROVENANCE_SCHEMA}",
            provenance.schema
        )));
    }
    if !matches!(provenance.tier.as_str(), "public" | "private") {
        return Err(invalid_archive(format!(
            "unknown provenance tier {:?}",
            provenance.tier
        )));
    }
    let Some(raw_format) = provenance.format.as_deref() else {
        return Err(crate::knowledge::service::LegacyKnowledgeArchiveUnsupported.into());
    };
    if crate::knowledge::types::KbFormat::parse(raw_format).is_none() {
        return Err(invalid_archive(format!(
            "unsupported provenance format {raw_format:?}; this build reads only {:?} and {:?}",
            crate::knowledge::types::KbFormat::Okf.as_str(),
            crate::knowledge::types::KbFormat::Biookf.as_str(),
        )));
    }
    if provenance
        .owners
        .iter()
        .any(|owner| crate::knowledge::affiliation::normalize_institution(owner).is_empty())
    {
        return Err(invalid_archive(
            "provenance marker contains an empty institution owner",
        ));
    }
    Ok(provenance)
}

fn strict_manifest_profile(staged_path: &Path) -> Result<crate::knowledge::types::KbFormat> {
    let manifest_path = staged_path.join("manifest.yaml");
    let raw = std::fs::read_to_string(&manifest_path)
        .map_err(|error| invalid_archive(format!("missing manifest.yaml: {error}")))?;
    let raw_manifest: serde_yaml::Value = serde_yaml::from_str(&raw)
        .map_err(|error| invalid_archive(format!("malformed manifest.yaml: {error}")))?;
    let raw_format = raw_manifest
        .as_mapping()
        .and_then(|mapping| mapping.get(serde_yaml::Value::String("format".to_string())))
        .and_then(serde_yaml::Value::as_str)
        .ok_or_else(|| invalid_archive("manifest.yaml does not declare a string format"))?;
    let profile = crate::knowledge::types::KbFormat::parse(raw_format).ok_or_else(|| {
        invalid_archive(format!(
            "manifest.yaml declares unsupported format {raw_format:?}; use \"okf\" or \"biookf\""
        ))
    })?;
    let manifest: crate::knowledge::types::Manifest = serde_yaml::from_str(&raw)
        .map_err(|error| invalid_archive(format!("malformed manifest.yaml: {error}")))?;
    if manifest.schema_version < crate::knowledge::types::CURRENT_SCHEMA_VERSION {
        return Err(crate::knowledge::service::LegacyKnowledgeArchiveUnsupported.into());
    }
    if manifest.schema_version > crate::knowledge::types::CURRENT_SCHEMA_VERSION {
        return Err(invalid_archive(format!(
            "manifest.yaml uses future schema version {}; this build supports version {}",
            manifest.schema_version,
            crate::knowledge::types::CURRENT_SCHEMA_VERSION
        )));
    }
    Ok(profile)
}

fn collision_id(original_id: &str, suffix: usize) -> Result<String> {
    let suffix = format!("-{suffix}");
    let max_stem = 64_usize
        .checked_sub(suffix.len())
        .ok_or_else(|| invalid_archive("knowledge-base collision suffix is too long"))?;
    // `validate_kb_id` has already confirmed the id is ASCII by the time an
    // import reaches here, so taking chars is the same cut as taking bytes —
    // and unlike a byte slice it cannot panic on a boundary if that guarantee
    // ever moves.
    let truncated: String = original_id.chars().take(max_stem).collect();
    let stem = truncated.trim_end_matches('-');
    if stem.is_empty() {
        return Err(invalid_archive(
            "knowledge-base id cannot be collision-renamed within 64 bytes",
        ));
    }
    let id = format!("{stem}{suffix}");
    crate::knowledge::paths::validate_kb_id(&id)
        .map_err(|error| invalid_archive(format!("collision-renamed id is invalid: {error}")))?;
    Ok(id)
}

fn ensure_operational_repository(
    staged_path: &Path,
    original_id: &str,
    imported_id: &str,
) -> Result<()> {
    let git_dir = staged_path.join(".git");
    if git_dir.exists() && !git_dir.is_dir() {
        return Err(invalid_archive(".git must be a directory"));
    }
    if !git_dir.exists() {
        let repo = crate::knowledge::git::GitRepo::init(staged_path)?;
        repo.commit_all(
            crate::knowledge::types::ChangeKind::Manual,
            "initialize imported knowledge base",
            None,
        )?;
        return Ok(());
    }

    let repository = git2::Repository::open(staged_path)
        .map_err(|error| invalid_archive(format!("invalid Git repository: {error}")))?;
    if repository.is_bare() {
        return Err(invalid_archive("imported Git repository is bare"));
    }
    let workdir = repository
        .workdir()
        .ok_or_else(|| invalid_archive("imported Git repository has no working directory"))?;
    let canonical_workdir = workdir
        .canonicalize()
        .map_err(|error| invalid_archive(format!("cannot resolve Git worktree: {error}")))?;
    let canonical_stage = staged_path
        .canonicalize()
        .map_err(|error| invalid_archive(format!("cannot resolve staged import: {error}")))?;
    if canonical_workdir != canonical_stage {
        return Err(invalid_archive(
            "imported Git repository points outside the knowledge base",
        ));
    }
    let initialize_empty = match repository.head() {
        Ok(head) => match head.peel_to_commit() {
            Ok(_) => false,
            Err(error)
                if matches!(
                    error.code(),
                    git2::ErrorCode::UnbornBranch | git2::ErrorCode::NotFound
                ) =>
            {
                true
            }
            Err(error) => {
                return Err(invalid_archive(format!(
                    "imported Git repository has no readable HEAD commit: {error}"
                )));
            }
        },
        Err(error)
            if matches!(
                error.code(),
                git2::ErrorCode::UnbornBranch | git2::ErrorCode::NotFound
            ) =>
        {
            true
        }
        Err(error) => {
            return Err(invalid_archive(format!(
                "imported Git repository has no readable HEAD commit: {error}"
            )));
        }
    };
    if initialize_empty {
        drop(repository);
        let repo = crate::knowledge::git::GitRepo::open(staged_path)
            .map_err(|error| invalid_archive(format!("invalid Git repository: {error}")))?;
        repo.commit_all(
            crate::knowledge::types::ChangeKind::Manual,
            "initialize imported knowledge base",
            None,
        )?;
        return Ok(());
    }
    let has_changes = !repository
        .statuses(None)
        .map_err(|error| invalid_archive(format!("cannot inspect Git worktree: {error}")))?
        .is_empty();
    drop(repository);
    if has_changes || original_id != imported_id {
        crate::knowledge::git::GitRepo::open(staged_path)?.commit_all(
            crate::knowledge::types::ChangeKind::Manual,
            "assign imported knowledge-base id",
            None,
        )?;
    }
    Ok(())
}

fn extract_archive<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    original_id: &str,
    imported_id: &str,
    provenance_profile: crate::knowledge::types::KbFormat,
    staged_path: &Path,
) -> Result<()> {
    let mut expanded_total = 0_u64;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|error| invalid_archive(format!("cannot read archive entry: {error}")))?;
        validate_archive_entry(&entry)?;
        expanded_total = expanded_total
            .checked_add(entry.size())
            .ok_or_else(|| invalid_archive("archive expanded-size total overflowed"))?;
        if expanded_total > MAX_ARCHIVE_EXPANDED_BYTES {
            return Err(invalid_archive(
                "archive exceeds the 1 GiB expanded-size limit",
            ));
        }
        let entry_name = entry.name().to_string();
        let rel: PathBuf = entry_name
            .strip_prefix(&format!("{original_id}/"))
            .unwrap_or(entry_name.as_str())
            .into();
        if rel == Path::new(PROVENANCE_ENTRY) {
            continue;
        }
        let dest = safe_join(staged_path, &rel)?;
        if entry.is_dir() {
            std::fs::create_dir_all(&dest)?;
        } else {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut file = std::fs::File::create(&dest)?;
            let copied = std::io::copy(&mut entry, &mut file).map_err(|error| {
                invalid_archive(format!("cannot unpack archive entry: {error}"))
            })?;
            if copied > MAX_ARCHIVE_ENTRY_BYTES {
                return Err(invalid_archive(
                    "archive entry exceeded the expanded-size limit while extracting",
                ));
            }
        }
    }
    let actual_profile = strict_manifest_profile(staged_path)?;
    let mut manifest = crate::knowledge::manifest::load(staged_path)
        .map_err(|error| invalid_archive(format!("malformed manifest.yaml: {error}")))?;
    if manifest.id != original_id {
        return Err(invalid_archive(format!(
            "bundle identity mismatch: top-level directory is {original_id:?}, but manifest id is {:?}",
            manifest.id
        )));
    }
    if actual_profile != provenance_profile {
        return Err(invalid_archive(format!(
            "provenance declares format {:?}, but manifest.yaml declares {:?}",
            provenance_profile.as_str(),
            actual_profile.as_str()
        )));
    }
    for required_file in ["schema.md", "index.md", "log.md"] {
        if !staged_path.join(required_file).is_file() {
            return Err(invalid_archive(format!(
                "imported bundle is missing required file {required_file}"
            )));
        }
    }
    if !staged_path.join("knowledge").is_dir() {
        return Err(invalid_archive(
            "imported bundle is missing required directory knowledge/",
        ));
    }
    manifest.id = imported_id.to_string();
    crate::knowledge::manifest::save(staged_path, &manifest)?;
    ensure_operational_repository(staged_path, original_id, imported_id)?;
    Ok(())
}

/// Unpack a .brkb zip into a fresh directory under `knowledge_root` and return
/// the new kb_id together with the archive's provenance (issue #56).
///
/// The .brkb is expected to contain exactly one top-level directory (the kb_id at export time).
/// If that id collides with an existing KB at the destination, suffix with `-N` to disambiguate.
/// What one pass over the archive established about its shape.
struct ArchiveShape {
    /// The distinct first path segments. A bundle must have exactly one.
    top_names: std::collections::HashSet<String>,
    /// Every entry name, so the provenance marker can be required without a
    /// second pass.
    entry_names: std::collections::HashSet<String>,
}

/// Validate every entry and read the archive's shape, before anything is
/// written.
///
/// The size budget is accumulated here rather than during extraction so an
/// over-budget bundle is refused with nothing on disk to roll back — the same
/// ordering argument the provenance marker is read under (DR-18).
fn read_archive_shape<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Result<ArchiveShape> {
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(invalid_archive("archive contains too many entries"));
    }
    let mut top_names = std::collections::HashSet::new();
    let mut entry_names = std::collections::HashSet::new();
    let mut expanded_total = 0_u64;
    for i in 0..archive.len() {
        let entry = archive
            .by_index(i)
            .map_err(|error| invalid_archive(format!("cannot read archive entry: {error}")))?;
        validate_archive_entry(&entry)?;
        expanded_total = expanded_total
            .checked_add(entry.size())
            .ok_or_else(|| invalid_archive("archive expanded-size total overflowed"))?;
        if expanded_total > MAX_ARCHIVE_EXPANDED_BYTES {
            return Err(invalid_archive(
                "archive exceeds the 1 GiB expanded-size limit",
            ));
        }
        let name = entry.name().to_string();
        if !entry_names.insert(name.clone()) {
            return Err(invalid_archive(format!(
                "archive contains duplicate entry {name:?}"
            )));
        }
        if let Some(first) = name.split('/').next() {
            if !first.is_empty() {
                top_names.insert(first.to_string());
            }
        }
    }
    Ok(ArchiveShape {
        top_names,
        entry_names,
    })
}

pub(crate) fn stage_import<R: Read + Seek>(
    zip_bytes: R,
    knowledge_root: &Path,
) -> Result<StagedImport> {
    let mut archive = ZipArchive::new(zip_bytes)
        .map_err(|error| invalid_archive(format!("not a readable zip archive: {error}")))?;
    let ArchiveShape {
        top_names,
        entry_names,
    } = read_archive_shape(&mut archive)?;
    let original_id = if top_names.len() == 1 {
        top_names.into_iter().next().unwrap()
    } else {
        return Err(invalid_archive(format!(
            "brkb must contain exactly one top-level directory, found {}",
            top_names.len()
        )));
    };
    crate::knowledge::paths::validate_kb_id(&original_id).map_err(|error| {
        invalid_archive(format!(
            "the archive's top-level directory is not a valid knowledge-base id: {error}"
        ))
    })?;
    let marker_name = format!("{original_id}/{PROVENANCE_ENTRY}");
    if !entry_names.contains(&marker_name) {
        return Err(invalid_archive("archive is missing its provenance marker"));
    }

    // ⚠ **The marker is read here, before a single byte is written** (DR-18).
    //
    // It used to be read inside the extraction loop, which was fine while
    // everything it carried was advisory: `tier` and `owners` are applied
    // *after* extraction either way. `format` is not advisory — it can refuse —
    // and the marker is the last entry the exporter writes, so a refusal
    // decided in the loop would fire with the whole base already unpacked on
    // disk. "Partial-extracting a bundle whose format this build cannot read"
    // is precisely what DR-18 forbids, and the fix is an ordering, not a
    // cleanup path: there is nothing to roll back if nothing was created.
    let provenance = read_provenance(&mut archive, &original_id)?;
    let provenance_profile = crate::knowledge::types::KbFormat::parse(
        provenance
            .format
            .as_deref()
            .expect("read_provenance requires a known format"),
    )
    .expect("read_provenance validates the format word");

    // Resolve a non-colliding id. Issue #56: a directory is not the only thing
    // that claims an id — the tier store can hold an entry for a base with no
    // directory (`tier::raise_unlocked` registers ids that have not been
    // created), and an import that landed on one would be classified by a base
    // that never existed rather than by its own provenance.
    let registered = crate::knowledge::registry::load(knowledge_root)?
        .into_iter()
        .map(|entry| entry.id)
        .collect::<std::collections::HashSet<_>>();
    let mut id = original_id.clone();
    let mut suffix = 1_usize;
    while knowledge_root.join(&id).exists()
        || crate::knowledge::tier::has_entry_unlocked(knowledge_root, &id)
        || registered.contains(&id)
    {
        suffix = suffix
            .checked_add(1)
            .ok_or_else(|| invalid_archive("knowledge-base collision counter overflowed"))?;
        id = collision_id(&original_id, suffix)?;
    }
    let provenance_private = provenance.tier == "private";
    // Normalised on the way in, exactly as the wire id is
    // (`affiliation::caller_affiliation`): an archive written by hand, or by a
    // build whose normaliser differed, must not land a `UCSF` that mismatches
    // the `ucsf` every model states. Empty ids were refused while the marker was
    // parsed; dropping one here would erase an ownership constraint.
    let owners: std::collections::BTreeSet<String> = provenance
        .owners
        .into_iter()
        .map(|o| crate::knowledge::affiliation::normalize_institution(&o))
        .collect();
    let final_path = knowledge_root.join(&id);
    let staged_path = knowledge_root.join(format!(".importing-{id}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&staged_path)?;
    let extraction = extract_archive(
        &mut archive,
        &original_id,
        &id,
        provenance_profile,
        &staged_path,
    );
    if let Err(error) = extraction {
        return match std::fs::remove_dir_all(&staged_path) {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(anyhow::anyhow!(
                "archive extraction failed ({error:#}); staged files at {} could not be removed: {rollback_error}",
                staged_path.display()
            )),
        };
    }
    Ok(StagedImport {
        imported: Imported {
            id,
            provenance_private,
            owners,
        },
        staged_path,
        final_path,
    })
}

#[cfg(test)]
fn import<R: Read + Seek>(zip_bytes: R, knowledge_root: &Path) -> Result<Imported> {
    let staged = stage_import(zip_bytes, knowledge_root)?;
    if let Err(error) = std::fs::rename(&staged.staged_path, &staged.final_path) {
        return match std::fs::remove_dir_all(&staged.staged_path) {
            Ok(()) => Err(anyhow::anyhow!(
                "publish imported knowledge base failed ({error:#}); staged files at {} were removed",
                staged.staged_path.display()
            )),
            Err(rollback_error) => Err(anyhow::anyhow!(
                "publish imported knowledge base failed ({error}); staged files at {} could not be removed: {rollback_error}",
                staged.staged_path.display()
            )),
        };
    }
    Ok(staged.imported)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::service::KnowledgeService;

    fn zip_names(bytes: &[u8]) -> Vec<String> {
        let mut a = ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
        (0..a.len())
            .map(|i| a.by_index(i).unwrap().name().to_string())
            .collect()
    }

    fn current_archive_with_marker(id: &str, marker: &[u8]) -> Vec<u8> {
        let source_root = tempfile::tempdir().unwrap();
        let service = KnowledgeService::new(source_root.path().to_path_buf());
        service.create_base(id, id, None).unwrap();
        let source_bytes = service.export_brkb(id).unwrap();
        let mut source = ZipArchive::new(std::io::Cursor::new(source_bytes)).unwrap();
        let mut output = std::io::Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut output);
            let opts = FileOptions::default().compression_method(zip::CompressionMethod::Stored);
            for index in 0..source.len() {
                let mut entry = source.by_index(index).unwrap();
                let name = entry.name().to_string();
                if name == format!("{id}/{PROVENANCE_ENTRY}") {
                    continue;
                }
                if entry.is_dir() {
                    zip.add_directory(name, opts).unwrap();
                } else {
                    zip.start_file(name, opts).unwrap();
                    std::io::copy(&mut entry, &mut zip).unwrap();
                }
            }
            zip.start_file(format!("{id}/{PROVENANCE_ENTRY}"), opts)
                .unwrap();
            zip.write_all(marker).unwrap();
            zip.finish().unwrap();
        }
        output.into_inner()
    }

    /// An archive is content; a lock is not. This is the platform-independent
    /// half of the os-error-33 fix and the one with teeth off Windows: delete
    /// the `is_kb_write_lock` skip in [`walk`] and the entry reappears here on
    /// macOS and Linux, where reading a `flock`ed file is perfectly legal and
    /// the export therefore still *succeeds*.
    ///
    /// The `.crossref-cache` neighbour is the mutation guard on the guard: a
    /// skip written as "anything under `.biorouter-knowledge/`" would pass the
    /// first assertion and quietly stop archiving the internal directory.
    #[test]
    fn the_transient_write_lock_is_never_packed_into_an_archive() {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("orig", "Orig", None).unwrap();
        let internal = dir.path().join("orig").join(".biorouter-knowledge");
        std::fs::create_dir_all(&internal).unwrap();
        std::fs::write(internal.join("write.lock"), b"transient").unwrap();
        std::fs::write(internal.join("keep.json"), b"{}").unwrap();

        let names = zip_names(&svc.export_brkb("orig").unwrap());

        assert!(
            !names
                .iter()
                .any(|n| n.ends_with(".biorouter-knowledge/write.lock")),
            "the machine's transient write lock was shipped inside the archive: {names:?}"
        );
        assert!(
            names
                .iter()
                .any(|n| n.ends_with(".biorouter-knowledge/keep.json")),
            "the skip swallowed the whole internal directory: {names:?}"
        );
        // Entry names are the archive's own grammar, not the host's. Asserted
        // here rather than only in `archive_name`'s unit test because this is
        // the end-to-end path, and on windows-latest it is where a separator
        // regression would actually bite.
        assert!(
            !names.iter().any(|n| n.contains('\\')),
            "a zip entry name must use forward slashes on every platform: {names:?}"
        );
    }

    /// The separator rule, reachable from any platform.
    ///
    /// A real directory walk cannot distinguish the correct implementation from
    /// `to_string_lossy` on macOS or Linux, because there the two agree for
    /// every path `read_dir` produces. These inputs separate them anywhere: a
    /// doubled separator and a `.` prefix both survive the display form and are
    /// both dropped by `components()`. So a regression to `to_string_lossy`, or
    /// to a `replace('\\', "/")` patch over it, fails here on the developer's
    /// own machine instead of waiting for a Windows runner.
    #[test]
    fn an_archive_name_is_joined_from_components_with_forward_slashes() {
        assert_eq!(
            archive_name("omop", Path::new("knowledge").join("x.md").as_path()),
            "omop/knowledge/x.md"
        );
        assert_eq!(archive_name("omop", Path::new("a//b")), "omop/a/b");
        assert_eq!(archive_name("omop", Path::new("./a")), "omop/a");
        assert_eq!(
            archive_name("omop", Path::new("single.md")),
            "omop/single.md"
        );
    }

    /// The Windows half, stated as behaviour rather than as a platform.
    ///
    /// `kb_export` holds the KB write lock across `export_brkb`, so the walk
    /// reads a file this process has locked exclusively. On Windows that is
    /// `LockFileEx`, and the read is refused with os error 33, which is what
    /// took three `knowledge::server` tests red on windows-latest. On Unix
    /// `flock` does not govern reads, so this test cannot fail here; it is
    /// carried because windows-latest is where it is a real assertion, and
    /// because it pins the *scenario* (lock held, then export) that the comment
    /// in [`walk`] describes.
    #[test]
    fn exporting_while_holding_the_write_lock_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("orig", "Orig", None).unwrap();
        let lock_path = dir
            .path()
            .join("orig")
            .join(crate::knowledge::paths::KB_WRITE_LOCK_REL);
        std::fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
        let held = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .unwrap();
        fs2::FileExt::lock_exclusive(&held).unwrap();

        let bytes = svc
            .export_brkb("orig")
            .expect("an export must not be blocked by the lock the exporter itself holds");

        assert!(bytes.len() > 100, "the archive has content");
        let _ = fs2::FileExt::unlock(&held);
    }

    #[cfg(unix)]
    #[test]
    fn export_refuses_file_symlinks_instead_of_archiving_their_targets() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(outside.path(), b"EXTERNAL-FILE-SENTINEL").unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("orig", "Orig", None).unwrap();
        symlink(outside.path(), dir.path().join("orig/knowledge/escaped.md")).unwrap();

        let error = svc.export_brkb("orig").unwrap_err().to_string();
        assert!(error.contains("symbolic link"), "{error}");
        assert!(!error.contains("EXTERNAL-FILE-SENTINEL"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn export_refuses_directory_symlinks_instead_of_walking_their_targets() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("escaped.md"), b"EXTERNAL-DIR-SENTINEL").unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("orig", "Orig", None).unwrap();
        symlink(outside.path(), dir.path().join("orig/raw/escaped-dir")).unwrap();

        let error = svc.export_brkb("orig").unwrap_err().to_string();
        assert!(error.contains("symbolic link"), "{error}");
        assert!(!error.contains("EXTERNAL-DIR-SENTINEL"), "{error}");
    }

    #[test]
    fn export_then_import_preserves_files() {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("orig", "Orig", None).unwrap();
        // add some pages
        let kb_root = dir.path().join("orig");
        // `concept/`, because that is one of the directories `create_base`
        // scaffolds under OKF; `create_dir_all` all the same, so the test is
        // about the archive and not about the scaffold's directory names.
        let pages = kb_root.join("knowledge").join("concept");
        std::fs::create_dir_all(&pages).unwrap();
        std::fs::write(
            pages.join("hrv.md"),
            "---\ntype: Method\nidentifier: HRV\n---\nbody",
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
            .join("concept")
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

    /// Issue #56 DR-26 / Task 50. The owner set survives the archive. Older
    /// markers that cannot prove an owner set are refused rather than imported
    /// as unowned.
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
        assert!(imported.provenance_private);
        assert_eq!(imported.owners, owners, "the owner set did not survive");

        // A pre-DR-26 archive: the marker exists but has no `owners` field.
        let dest2 = tempfile::tempdir().unwrap();
        let bytes = current_archive_with_marker("old", br#"{"schema":1,"tier":"private"}"#);
        let error = import(std::io::Cursor::new(bytes), dest2.path()).unwrap_err();
        assert!(
            error.downcast_ref::<InvalidKnowledgeArchive>().is_some(),
            "{error:#}"
        );
        assert!(!dest2.path().join("old").exists());
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
    fn collision_suffix_keeps_a_maximum_length_id_valid_and_operational() {
        let id = "a".repeat(64);
        let source = tempfile::tempdir().unwrap();
        let source_service = KnowledgeService::new(source.path().to_path_buf());
        source_service.create_base(&id, "Long", None).unwrap();
        let bytes = source_service.export_brkb(&id).unwrap();

        let destination = tempfile::tempdir().unwrap();
        let service = KnowledgeService::new(destination.path().to_path_buf());
        service.create_base(&id, "Collision", None).unwrap();
        let imported = service
            .import_brkb(
                &bytes,
                false,
                &crate::knowledge::affiliation::CallerAffiliation::Unstated,
            )
            .unwrap();

        assert_eq!(imported.len(), 64, "{imported}");
        assert!(imported.ends_with("-2"), "{imported}");
        crate::knowledge::paths::validate_kb_id(&imported).unwrap();
        assert!(!service.list_history(&imported, 10).unwrap().is_empty());
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
            zip.start_file(format!("evil-kb/{PROVENANCE_ENTRY}"), opts)
                .unwrap();
            zip.write_all(br#"{"schema":3,"tier":"public","owners":[],"format":"okf"}"#)
                .unwrap();
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
    // ── DR-18: the marker declares a profile, and import refuses one it cannot
    // read ─────────────────────────────────────────────────────────────────

    fn marker_of(bytes: &[u8], kb_id: &str) -> serde_json::Value {
        let mut a = ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
        let mut raw = String::new();
        a.by_name(&format!("{kb_id}/{PROVENANCE_ENTRY}"))
            .expect("every archive this build writes carries a marker")
            .read_to_string(&mut raw)
            .unwrap();
        serde_json::from_str(&raw).unwrap()
    }

    /// The marker states the bundle's profile, read off the tree being packed.
    ///
    /// `Manifest::profile()` and not `Manifest::format`, which is DR-6's trap
    /// reached from the exporter: the field reads `okf` on every `manifest.yaml`
    /// written before Stage 3, so a legacy base would otherwise ship an archive
    /// claiming to be an OKF bundle when its pages are `title`/`kind`
    /// frontmatter. `None` is the honest marker for one — and it is the same
    /// thing a pre-Stage-6 archive carries, which is the case `import` waves
    /// through.
    #[test]
    fn the_marker_declares_the_bundles_profile_and_a_legacy_base_declares_none() {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base_in(
            "lit",
            "Lit",
            None,
            crate::knowledge::types::KbFormat::Biookf,
        )
        .unwrap();
        svc.create_base("gen", "Gen", None).unwrap();

        assert_eq!(
            marker_of(&svc.export_brkb("lit").unwrap(), "lit")["format"],
            "biookf"
        );
        assert_eq!(
            marker_of(&svc.export_brkb("gen").unwrap(), "gen")["format"],
            "okf"
        );

        // Now walk `gen` back to the generation every base on disk carries. Its
        // `format` field still reads `okf` — that is the trap — and the marker
        // must not repeat it.
        let kb_root = dir.path().join("gen");
        let mut m = crate::knowledge::manifest::load(&kb_root).unwrap();
        m.schema_version = crate::knowledge::types::AUTOMATIC_SCHEMA_CEILING;
        crate::knowledge::manifest::save(&kb_root, &m).unwrap();
        assert_eq!(m.format, crate::knowledge::types::KbFormat::Okf);

        let marker = marker_of(&svc.export_brkb("gen").unwrap(), "gen");
        assert!(
            marker["format"].is_null(),
            "a legacy base declares no profile: {marker}"
        );
        // The other two axes are untouched by any of this.
        assert_eq!(marker["tier"], "public", "{marker}");
        assert!(marker["owners"].as_array().is_some(), "{marker}");
    }

    /// DR-18's refusal, and the half of it that is an ORDERING.
    ///
    /// The marker is the last entry the exporter writes, so a check made while
    /// the extraction loop is running would fire with the whole base already
    /// unpacked — "partial-extracting a bundle whose format this build cannot
    /// read" is exactly what DR-18 forbids. The second assertion is therefore
    /// the load-bearing one: move the check back into the loop and the message
    /// is unchanged and this line fails.
    #[test]
    fn an_unreadable_profile_is_refused_before_anything_is_extracted() {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut buf);
            let opts = FileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zip.start_file("future/manifest.yaml", opts).unwrap();
            zip.write_all(b"id: future\nschema_version: 9\n").unwrap();
            zip.start_file("future/knowledge/x.md", opts).unwrap();
            zip.write_all(b"---\ntype: Whatever\n---\nbody").unwrap();
            // Last, exactly where a real export puts it.
            zip.start_file(format!("future/{PROVENANCE_ENTRY}"), opts)
                .unwrap();
            zip.write_all(br#"{"schema":3,"tier":"public","owners":[],"format":"okf-2030"}"#)
                .unwrap();
            zip.finish().unwrap();
        }
        let bytes = buf.into_inner();

        let dest = tempfile::tempdir().unwrap();
        let err = import(std::io::Cursor::new(bytes), dest.path()).unwrap_err();
        let msg = err.to_string();
        for word in ["okf-2030", "okf", "biookf"] {
            assert!(
                msg.contains(word),
                "the refusal must name the profile asked for and the ones that exist: {msg}"
            );
        }
        assert!(
            !dest.path().join("future").exists(),
            "the archive was partially extracted before it was refused"
        );
    }

    #[test]
    fn missing_malformed_old_and_unknown_provenance_fail_closed() {
        for marker in [
            &br#"{"schema":1,"tier":"public"}"#[..],
            &br#"{"schema":3,"tier":"public","owners":[],"format":""}"#[..],
            &br#"{"schema":4,"tier":"public","owners":[],"format":"okf"}"#[..],
            &br#"{"schema":3,"tier":"publik","owners":[],"format":"okf"}"#[..],
            &b"not json at all"[..],
        ] {
            let bytes = current_archive_with_marker("old", marker);
            let dest = tempfile::tempdir().unwrap();
            let error = import(std::io::Cursor::new(bytes), dest.path()).unwrap_err();
            assert!(
                error.downcast_ref::<InvalidKnowledgeArchive>().is_some(),
                "marker {:?}: {error:#}",
                String::from_utf8_lossy(marker)
            );
            assert!(!dest.path().join("old").exists());
        }

        let source = tempfile::tempdir().unwrap();
        let service = KnowledgeService::new(source.path().to_path_buf());
        service.create_base("missing", "Missing", None).unwrap();
        let bytes = service.export_brkb("missing").unwrap();
        let mut source_zip = ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut output = std::io::Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut output);
            let opts = FileOptions::default().compression_method(zip::CompressionMethod::Stored);
            for index in 0..source_zip.len() {
                let mut entry = source_zip.by_index(index).unwrap();
                let name = entry.name().to_string();
                if name.ends_with(PROVENANCE_ENTRY) {
                    continue;
                }
                if entry.is_dir() {
                    zip.add_directory(name, opts).unwrap();
                } else {
                    zip.start_file(name, opts).unwrap();
                    std::io::copy(&mut entry, &mut zip).unwrap();
                }
            }
            zip.finish().unwrap();
        }
        let dest = tempfile::tempdir().unwrap();
        let error = import(std::io::Cursor::new(output.into_inner()), dest.path()).unwrap_err();
        assert!(error.downcast_ref::<InvalidKnowledgeArchive>().is_some());
        assert!(!dest.path().join("missing").exists());
    }

    #[test]
    fn the_actual_manifest_format_is_strict_even_when_the_marker_is_valid() {
        let bytes = current_archive_with_marker(
            "strict",
            br#"{"schema":3,"tier":"public","owners":[],"format":"okf"}"#,
        );
        let mut source = ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut output = std::io::Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut output);
            let opts = FileOptions::default().compression_method(zip::CompressionMethod::Stored);
            for index in 0..source.len() {
                let mut entry = source.by_index(index).unwrap();
                let name = entry.name().to_string();
                zip.start_file(name.clone(), opts).unwrap();
                if name == "strict/manifest.yaml" {
                    let mut manifest = String::new();
                    entry.read_to_string(&mut manifest).unwrap();
                    zip.write_all(manifest.replace("format: okf", "format: okff").as_bytes())
                        .unwrap();
                } else {
                    std::io::copy(&mut entry, &mut zip).unwrap();
                }
            }
            zip.finish().unwrap();
        }
        let dest = tempfile::tempdir().unwrap();
        let error = import(std::io::Cursor::new(output.into_inner()), dest.path()).unwrap_err();
        assert!(error.to_string().contains("okff"), "{error:#}");
        assert!(error.downcast_ref::<InvalidKnowledgeArchive>().is_some());
        assert!(!dest.path().join("strict").exists());
    }

    #[test]
    fn an_archive_without_git_is_initialized_for_history_restore_and_writes() {
        let source = tempfile::tempdir().unwrap();
        let service = KnowledgeService::new(source.path().to_path_buf());
        service.create_base("nogit", "No Git", None).unwrap();
        let bytes = service.export_brkb("nogit").unwrap();
        let mut source_zip = ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut output = std::io::Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut output);
            let opts = FileOptions::default().compression_method(zip::CompressionMethod::Stored);
            for index in 0..source_zip.len() {
                let mut entry = source_zip.by_index(index).unwrap();
                let name = entry.name().to_string();
                if name.starts_with("nogit/.git/") || name == "nogit/.git" {
                    continue;
                }
                if entry.is_dir() {
                    zip.add_directory(name, opts).unwrap();
                } else {
                    zip.start_file(name, opts).unwrap();
                    std::io::copy(&mut entry, &mut zip).unwrap();
                }
            }
            zip.finish().unwrap();
        }
        let destination = tempfile::tempdir().unwrap();
        let imported_service = KnowledgeService::new(destination.path().to_path_buf());
        imported_service
            .import_brkb(
                &output.into_inner(),
                false,
                &crate::knowledge::affiliation::CallerAffiliation::Unstated,
            )
            .unwrap();
        assert!(!imported_service
            .list_history("nogit", 10)
            .unwrap()
            .is_empty());
        crate::knowledge::store::write_page(
            &destination.path().join("nogit"),
            "knowledge/note/after-import.md",
            "---\ntype: Note\nidentifier: After import\n---\n\nbody\n",
            "post import write",
            None,
        )
        .unwrap();
        assert!(imported_service.list_history("nogit", 10).unwrap().len() >= 2);
    }

    #[test]
    fn a_malformed_git_directory_is_refused_instead_of_publishing_a_broken_base() {
        let source = tempfile::tempdir().unwrap();
        let service = KnowledgeService::new(source.path().to_path_buf());
        service.create_base("badgit", "Bad Git", None).unwrap();
        let bytes = service.export_brkb("badgit").unwrap();
        let mut source_zip = ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut output = std::io::Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut output);
            let opts = FileOptions::default().compression_method(zip::CompressionMethod::Stored);
            for index in 0..source_zip.len() {
                let mut entry = source_zip.by_index(index).unwrap();
                let name = entry.name().to_string();
                if name.starts_with("badgit/.git/") || name == "badgit/.git" {
                    continue;
                }
                if entry.is_dir() {
                    zip.add_directory(name, opts).unwrap();
                } else {
                    zip.start_file(name, opts).unwrap();
                    std::io::copy(&mut entry, &mut zip).unwrap();
                }
            }
            zip.start_file("badgit/.git/config", opts).unwrap();
            zip.write_all(b"this is not a repository").unwrap();
            zip.finish().unwrap();
        }
        let destination = tempfile::tempdir().unwrap();
        let error = KnowledgeService::new(destination.path().to_path_buf())
            .import_brkb(
                &output.into_inner(),
                false,
                &crate::knowledge::affiliation::CallerAffiliation::Unstated,
            )
            .unwrap_err();
        assert!(error.downcast_ref::<InvalidKnowledgeArchive>().is_some());
        assert!(error.to_string().contains("Git repository"), "{error:#}");
        assert!(!destination.path().join("badgit").exists());
    }

    #[test]
    fn bounded_reader_rejects_advertised_and_trailing_bytes() {
        let advertised = read_bounded_archive(std::io::Cursor::new([0_u8; 1]), 9, 8).unwrap_err();
        assert!(advertised
            .downcast_ref::<InvalidKnowledgeArchive>()
            .is_some());

        let trailing = read_bounded_archive(std::io::Cursor::new([0_u8; 9]), 8, 8).unwrap_err();
        assert!(trailing.downcast_ref::<InvalidKnowledgeArchive>().is_some());
    }

    /// A profile this build DOES know is not refused, and the archive round
    /// trips through the new pre-pass with its other two axes intact.
    #[test]
    fn a_biookf_archive_imports_with_its_tier_and_owners_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base_in(
            "lit",
            "Lit",
            None,
            crate::knowledge::types::KbFormat::Biookf,
        )
        .unwrap();
        let kb_root = dir.path().join("lit");
        let owners: std::collections::BTreeSet<String> = ["ucsf".to_string()].into();
        let mut buf = std::io::Cursor::new(Vec::new());
        export(&kb_root, &mut buf, true, &owners).unwrap();

        let dest = tempfile::tempdir().unwrap();
        let imported = import(std::io::Cursor::new(buf.into_inner()), dest.path()).unwrap();
        assert!(imported.provenance_private);
        assert_eq!(imported.owners, owners);
        let m = crate::knowledge::manifest::load(&dest.path().join("lit")).unwrap();
        assert_eq!(m.profile(), Some(crate::knowledge::types::KbFormat::Biookf));
    }
}
