//! Reading an archive, and stripping the directory a source archive wraps
//! itself in.
//!
//! GitHub's source archives are wrapped: `hyperframes-main/skills/…`. So are
//! most hand-made ones. Stripping that wrapper before anything looks at the
//! structure is what lets the rest of this module reason about a repository's
//! real layout — the previous parser counted slashes on the *wrapped* names and
//! therefore recognised nothing.

use anyhow::{bail, Result};
use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;

/// Ceiling on the total **uncompressed** size of an archive. Enforced while
/// reading rather than from the archive's own declared sizes, which the person
/// who built the archive chose. Same ceiling, and the same reasoning, as
/// `biorouter-server`'s `MAX_ARCHIVE_BYTES`.
pub const MAX_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;

/// One file from an archive. Directories are dropped on read — every shape
/// question here is answered by where the *files* are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// `/`-separated, relative, with `..` already refused.
    pub name: String,
    pub data: Vec<u8>,
}

impl Entry {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.data).to_string()
    }

    fn first_component(&self) -> Option<&str> {
        self.name.split('/').next().filter(|c| !c.is_empty())
    }
}

/// Refuse a zip entry that would escape the directory it is unpacked into.
///
/// Absolute paths, `..` anywhere, NUL bytes, and Windows separators (folded to
/// `/` first, so `..\..\etc` is caught too).
pub fn safe_entry_name(raw: &str) -> Result<String> {
    if raw.is_empty() || raw.contains('\0') {
        bail!("unsafe archive entry path: {raw}");
    }
    let folded = raw.replace('\\', "/");
    if folded.starts_with('/') || Path::new(&folded).is_absolute() {
        bail!("unsafe archive entry path: {raw}");
    }
    let parts: Vec<&str> = folded
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    if parts.iter().any(|part| *part == "..") {
        bail!("unsafe archive entry path: {raw}");
    }
    if parts.is_empty() {
        bail!("unsafe archive entry path: {raw}");
    }
    Ok(parts.join("/"))
}

/// Read a `.zip` into entries, bounded by [`MAX_ARCHIVE_BYTES`].
pub fn read_zip(bytes: &[u8]) -> Result<Vec<Entry>> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| anyhow::anyhow!("this does not look like a .zip archive: {e}"))?;
    let mut entries = Vec::new();
    let mut budget = MAX_ARCHIVE_BYTES;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|e| anyhow::anyhow!("could not read the archive: {e}"))?;
        if file.is_dir() {
            continue;
        }
        let name = safe_entry_name(file.name())?;
        let mut data = Vec::new();
        // One byte past the remaining budget, so "exactly fills it" and
        // "overruns it" are distinguishable.
        file.by_ref()
            .take(budget.saturating_add(1))
            .read_to_end(&mut data)
            .map_err(|e| anyhow::anyhow!("could not read the archive: {e}"))?;
        if data.len() as u64 > budget {
            bail!("the archive expands past the {MAX_ARCHIVE_BYTES} byte limit");
        }
        budget -= data.len() as u64;
        entries.push(Entry { name, data });
    }
    if entries.is_empty() {
        bail!("the archive is empty");
    }
    Ok(entries)
}

/// The single directory every entry sits inside, if there is one.
pub fn common_root(entries: &[Entry]) -> Option<String> {
    let roots: BTreeSet<&str> = entries.iter().filter_map(Entry::first_component).collect();
    if roots.len() != 1 {
        return None;
    }
    let root = *roots.iter().next()?;
    // A root that is itself a file (`SKILL.md` at the top) is not a wrapper.
    entries
        .iter()
        .all(|entry| entry.name.len() > root.len())
        .then(|| root.to_string())
}

/// What the caller knows about the archive's wrapper directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrapperHint<'a> {
    /// A source archive downloaded from a code host. GitHub's always wraps its
    /// contents in exactly one directory, so the single common root **is** the
    /// wrapper whatever it is named.
    ///
    /// ⚠ Knowing this rather than predicting the name matters: codeload's
    /// directory is `<repo>-<branch>` with `/` folded to `-`, and for a tag it
    /// also drops a leading `v` — so `refs/tags/v1.0` unpacks into `repo-1.0`.
    /// A predicted name would silently fail to match on exactly the refs a
    /// user is most likely to pin to.
    SourceArchive,
    /// This exact directory, if the archive has it as its single root.
    Named(&'a str),
    /// Nothing known.
    Infer,
}

/// Strip the archive's wrapper directory, if it has one.
///
/// ⚠ **The `Infer` rule is the load-bearing one.** With no hint, the wrapper is
/// stripped only when stripping **reveals** package structure: a manifest, a
/// `skills/` directory, or a root `SKILL.md`. That is what keeps a genuine
/// bundle archive — `pack/alpha/SKILL.md`, `pack/beta/SKILL.md` — from being
/// unwrapped into two unrelated skills, which is exactly the flattening #115 is
/// about. A repository whose root holds one folder per skill has the same shape
/// as that bundle, so nothing but the caller's own knowledge can separate them.
pub fn strip_wrapper(entries: Vec<Entry>, hint: WrapperHint<'_>) -> (Vec<Entry>, Option<String>) {
    let Some(root) = common_root(&entries) else {
        return (entries, None);
    };
    let prefix = format!("{root}/");

    let known = match hint {
        WrapperHint::SourceArchive => true,
        WrapperHint::Named(expected) => expected == root,
        WrapperHint::Infer => false,
    };
    if !known && !reveals_structure(&entries, &prefix) {
        return (entries, None);
    }

    let stripped = entries
        .into_iter()
        .filter_map(|entry| {
            entry.name.strip_prefix(&prefix).map(|name| Entry {
                name: name.to_string(),
                data: entry.data,
            })
        })
        .collect();
    (stripped, Some(root))
}

fn reveals_structure(entries: &[Entry], prefix: &str) -> bool {
    entries.iter().any(|entry| {
        let Some(rest) = entry.name.strip_prefix(prefix) else {
            return false;
        };
        rest == "SKILL.md"
            || rest == "skills-manifest.json"
            || rest == crate::agents::skill_catalog::PACKAGE_RECORD_FILE
            || rest.starts_with("skills/")
            || rest.starts_with(".codex-plugin/")
            || rest.starts_with(".claude-plugin/")
    })
}

/// Every `SKILL.md` in the entry set, as `(directory, entry)` where the
/// directory is `""` for one at the root.
pub fn skill_files(entries: &[Entry]) -> Vec<(String, &Entry)> {
    entries
        .iter()
        .filter(|entry| entry.name == "SKILL.md" || entry.name.ends_with("/SKILL.md"))
        .map(|entry| {
            let directory = entry
                .name
                .strip_suffix("SKILL.md")
                .unwrap_or_default()
                .trim_end_matches('/')
                .to_string();
            (directory, entry)
        })
        .collect()
}

/// Every entry under `directory` (or all of them when it is empty), renamed
/// relative to it.
pub fn entries_under(entries: &[Entry], directory: &str) -> Vec<Entry> {
    if directory.is_empty() {
        return entries.to_vec();
    }
    let prefix = format!("{directory}/");
    entries
        .iter()
        .filter_map(|entry| {
            entry.name.strip_prefix(&prefix).map(|name| Entry {
                name: name.to_string(),
                data: entry.data.clone(),
            })
        })
        .collect()
}
