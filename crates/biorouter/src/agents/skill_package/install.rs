//! Atomic install and removal.
//!
//! # Why staging, and not "write the files"
//!
//! A package is many files across many directories. Writing them in place means
//! a failure halfway through — a full disk, a permission error, a process
//! killed — leaves a directory that *looks* like an installed package, is
//! missing components, and shadows whatever was there before. The catalog would
//! then serve it, the model would load half of it, and nothing would say so.
//!
//! So the whole package is written to a sibling staging directory, verified,
//! and swapped in with two renames. Both renames are within one directory on
//! one filesystem, which is where `rename` is atomic. A failure before the swap
//! leaves the previous install untouched; a failure between the renames puts it
//! back.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use super::{ImportKind, ImportPlan};
use crate::agents::skill_catalog::{self, PackageSummary, PACKAGE_RECORD_FILE};
use crate::agents::skills_extension::SkillsClient;
use crate::config::paths::Paths;

/// What an install did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPackage {
    pub id: String,
    pub display_name: String,
    pub kind: ImportKind,
    /// Component skill names, in the order they were installed.
    pub skills: Vec<String>,
    pub entry_point: Option<String>,
    #[schema(value_type = String)]
    pub directory: PathBuf,
    /// True when this replaced an existing install of the same id.
    pub replaced: bool,
    /// The catalog generation after the refresh, so a caller can tell whether
    /// it is looking at an inventory that already includes this.
    pub catalog_generation: u64,
}

/// The record written at a package's root, and read back by
/// [`skill_catalog::SkillCatalog::scan`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PackageRecord {
    id: String,
    display_name: String,
    version: Option<String>,
    source_url: Option<String>,
    source_ref: Option<String>,
    resolved_commit: Option<String>,
    installer: Option<String>,
    installed_at: Option<String>,
    entry_point: Option<String>,
    groups: std::collections::BTreeMap<String, Vec<String>>,
    /// Component names, so a re-import of an exported package round-trips.
    components: Vec<RecordComponent>,
}

#[derive(Debug, Clone, Serialize)]
struct RecordComponent {
    name: String,
    directory: String,
}

/// The skills root packages are installed into.
pub fn install_root() -> PathBuf {
    Paths::config_dir().join("skills")
}

/// Write `plan` into `root`, atomically, and refresh the catalog.
///
/// Refuses a plan that still carries an unanswered [`ImportPlan::ambiguity`] —
/// the whole point of that field is that somebody has to answer it, and an
/// installer that quietly picked one of the answers would be the flattening
/// this module exists to remove.
pub fn install(plan: &ImportPlan, root: &Path) -> Result<InstalledPackage> {
    if let Some(ambiguity) = &plan.ambiguity {
        bail!(
            "this import needs an answer before it can be installed: {}",
            ambiguity.reason
        );
    }
    if plan.components.is_empty() {
        bail!("nothing selected to install");
    }

    std::fs::create_dir_all(root)
        .with_context(|| format!("creating the skills directory {}", root.display()))?;

    let nonce = nonce();
    let staging = root.join(format!(".br-import-{}-{nonce}", plan.id));
    let replaced_dir = root.join(format!(".br-replaced-{}-{nonce}", plan.id));
    let destination = root.join(&plan.id);

    // Anything from here to the swap cleans up after itself.
    let staged = stage(plan, &staging);
    if let Err(error) = staged {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error);
    }

    let replaced = destination.exists();
    if replaced {
        std::fs::rename(&destination, &replaced_dir).map_err(|e| {
            let _ = std::fs::remove_dir_all(&staging);
            anyhow::anyhow!(
                "could not move the existing `{}` aside, so it was left untouched: {e}",
                plan.id
            )
        })?;
    }

    if let Err(error) = std::fs::rename(&staging, &destination) {
        // Put the previous install back before reporting. Leaving it under its
        // temporary name would read to every surface as an uninstall.
        if replaced {
            let _ = std::fs::rename(&replaced_dir, &destination);
        }
        let _ = std::fs::remove_dir_all(&staging);
        bail!("could not install `{}`: {error}", plan.id);
    }

    if replaced {
        let _ = std::fs::remove_dir_all(&replaced_dir);
    }

    let catalog = skill_catalog::refresh();
    Ok(InstalledPackage {
        id: plan.id.clone(),
        display_name: plan.display_name.clone(),
        kind: plan.kind,
        skills: plan.components.iter().map(|c| c.name.clone()).collect(),
        entry_point: plan.entry_point.clone(),
        directory: destination,
        replaced,
        catalog_generation: catalog.generation,
    })
}

/// Write the package into `staging` and verify what landed.
fn stage(plan: &ImportPlan, staging: &Path) -> Result<()> {
    std::fs::create_dir_all(staging).with_context(|| format!("creating {}", staging.display()))?;

    for (relative, data) in &plan.files {
        // Re-checked here rather than trusted from the plan: this is the last
        // point before bytes reach the filesystem, and a plan can be built by
        // any caller.
        let safe = super::archive::safe_entry_name(relative)?;
        let target = staging.join(&safe);
        if !target.starts_with(staging) {
            bail!("unsafe archive entry path: {relative}");
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::write(&target, data).with_context(|| format!("writing {}", target.display()))?;
    }

    // Verify from DISK, not from the plan. What matters is that the tree the
    // catalog will scan actually holds a loadable skill at every path the
    // package claims.
    for component in &plan.components {
        let skill_md = if plan.kind == ImportKind::Single {
            staging.join("SKILL.md")
        } else {
            staging.join(&component.directory).join("SKILL.md")
        };
        let content = std::fs::read_to_string(&skill_md).with_context(|| {
            format!(
                "`{}` was declared but no SKILL.md was written for it",
                component.name
            )
        })?;
        let (metadata, _) = SkillsClient::parse_frontmatter(&content).map_err(|_| {
            anyhow::anyhow!("`{}` has no valid SKILL.md frontmatter", component.name)
        })?;
        if metadata.name != component.name {
            bail!(
                "`{}` was installed but declares the name `{}`",
                component.name,
                metadata.name
            );
        }
    }

    if plan.kind == ImportKind::Bundle {
        let record = PackageRecord {
            id: plan.id.clone(),
            display_name: plan.display_name.clone(),
            version: plan.version.clone(),
            source_url: plan.source.url.clone(),
            source_ref: plan.source.reference.clone(),
            resolved_commit: plan.source.resolved_commit.clone(),
            installer: plan.source.installer.clone(),
            installed_at: Some(chrono::Utc::now().to_rfc3339()),
            entry_point: plan.entry_point.clone(),
            groups: plan.groups.clone(),
            components: plan
                .components
                .iter()
                .map(|c| RecordComponent {
                    name: c.name.clone(),
                    directory: c.directory.clone(),
                })
                .collect(),
        };
        std::fs::write(
            staging.join(PACKAGE_RECORD_FILE),
            serde_json::to_string_pretty(&record)?,
        )
        .context("writing the package record")?;
    }

    Ok(())
}

/// Remove an installed package (or single skill) by its directory name.
///
/// Renamed aside first and then deleted, so the directory disappears from the
/// catalog's view in one step rather than emptying out under a scan in flight.
pub fn remove(id: &str, root: &Path) -> Result<PackageSummary> {
    let sanitized = super::sanitize_package_id(id)
        .ok_or_else(|| anyhow::anyhow!("`{id}` is not a valid package name"))?;
    let directory = root.join(&sanitized);
    if !directory.is_dir() {
        bail!("no package named `{sanitized}` is installed");
    }

    let summary = std::fs::read_to_string(directory.join(PACKAGE_RECORD_FILE))
        .ok()
        .and_then(|raw| serde_json::from_str::<PackageSummary>(&raw).ok())
        .unwrap_or_else(|| PackageSummary {
            id: sanitized.clone(),
            display_name: sanitized.clone(),
            version: None,
            source_url: None,
            source_ref: None,
            resolved_commit: None,
            installer: None,
            installed_at: None,
            entry_point: None,
            groups: Default::default(),
        });

    let condemned = root.join(format!(".br-removing-{sanitized}-{}", nonce()));
    std::fs::rename(&directory, &condemned)
        .with_context(|| format!("removing {}", directory.display()))?;
    std::fs::remove_dir_all(&condemned)
        .with_context(|| format!("removing {}", condemned.display()))?;

    skill_catalog::refresh();
    Ok(summary)
}

fn nonce() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("{nanos:x}")
}
