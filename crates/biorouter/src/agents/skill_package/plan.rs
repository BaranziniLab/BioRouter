//! Turning a normalised archive into a reviewable [`ImportPlan`].
//!
//! Everything a manifest said is honoured; everything it did not say is
//! inferred from the archive's structure — and where structure alone could mean
//! two different things, the plan says so instead of choosing.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Result};

use super::archive::{self, Entry};
use super::manifest::{self, ManifestFacts};
use super::{
    sanitize_package_id, Ambiguity, Evidence, ImportKind, ImportPlan, PlannedSkill,
    SourceProvenance,
};
use crate::agents::skills_extension::SkillsClient;

/// Files a package carries but that are not part of any component — the
/// manifests themselves. Dropped on install: the record this importer writes is
/// the one the catalog reads, and shipping a second, differently-shaped
/// manifest beside it invites the two to disagree.
const MANIFEST_FILES: &[&str] = &[
    "skills-manifest.json",
    ".codex-plugin/plugin.json",
    ".claude-plugin/plugin.json",
];

/// Build a plan from an archive's entries.
///
/// `id_hints` are candidate package names in preference order — typically the
/// repository name, then the archive's filename stem. They are consulted only
/// after a manifest, and every one is filtered through
/// [`sanitize_package_id`], which is what stops an archive of a bare `skills/`
/// directory installing as a package called "skills".
pub fn plan_from_entries(
    entries: Vec<Entry>,
    id_hints: &[String],
    source: SourceProvenance,
) -> Result<ImportPlan> {
    let facts = manifest::detect(&entries)?;
    let skill_files = archive::skill_files(&entries);
    if skill_files.is_empty() {
        bail!("no SKILL.md anywhere in this package");
    }

    // ---------------------------------------------------------------- single
    // A SKILL.md at the root is a single skill, full stop. It is the one shape
    // that cannot also be a package, so it is decided before anything else.
    if let Some((_, root_skill)) = skill_files.iter().find(|(dir, _)| dir.is_empty()) {
        let (metadata, _) = SkillsClient::parse_frontmatter(&root_skill.text()).map_err(|_| {
            anyhow::anyhow!(
                "the root SKILL.md has no valid frontmatter with `name` and `description`"
            )
        })?;
        return single_plan(
            PlannedSkill {
                name: metadata.name,
                description: metadata.description,
                directory: String::new(),
                group: None,
                entry_point: false,
            },
            keep_files(&entries, ""),
            id_hints,
            facts.version.clone(),
            source,
        );
    }

    // ------------------------------------------------------- components root
    let (components_root, root_evidence) = resolve_components_root(&facts, &skill_files);
    let members = components_in(&skill_files, &components_root);
    if members.is_empty() {
        bail!(
            "the package declares its skills under `{}`, and there is no <name>/SKILL.md there",
            if components_root.is_empty() {
                "."
            } else {
                &components_root
            }
        );
    }

    let mut components = collect_components(&members)?;

    // ⚠ **Before identity, not after.** A single skill in a named folder with
    // no manifest needs no package name, and resolving one first refused the
    // import with "this package has no name" — a message about a package the
    // user never asked for. Found by running a real repository through the
    // real pipeline; every synthetic fixture reached this shape via
    // `strip_wrapper`, which unwraps it into the root-SKILL.md case and jumps
    // over the branch entirely.
    let single_component = components.len() == 1;
    // A single component and no manifest is one skill in one folder, not a
    // package of one — installing it as a bundle would put it a level deeper
    // than every other single-skill install.
    if single_component && facts.evidence.is_none() {
        let component = components.remove(0);
        let files = keep_files(&entries, &component.directory);
        return single_plan(
            PlannedSkill {
                directory: String::new(),
                ..component
            },
            files,
            id_hints,
            facts.version,
            source,
        );
    }

    let (id, display_name) = resolve_identity(&facts, id_hints, &components_root)?;

    let (entry_point, groups) = apply_manifest_metadata(&facts, &mut components, &id);

    let evidence = facts
        .evidence
        .clone()
        .unwrap_or_else(|| root_evidence.clone());

    let ambiguity = ambiguity_for(&facts, &components, &components_root);

    // A manifest-defined component root is meaningful package layout.  A
    // structurally inferred named parent is instead the package directory the
    // installer is about to create, so carrying it would duplicate the package
    // name (`pack/pack/member`).  `skills/` is the conventional meaningful
    // exception even when inferred from an archive without a manifest.
    let source_package_root = (facts.components_root.is_none()
        && components_root != "skills"
        && !components_root.ends_with("/skills"))
    .then_some(components_root.as_str())
    .filter(|root| !root.is_empty());
    let files = keep_bundle_files(&entries, &components, source_package_root);
    let components = components
        .into_iter()
        .map(|mut component| {
            component.directory = package_relative_path(&component.directory, source_package_root);
            component
        })
        .collect();

    Ok(ImportPlan {
        kind: ImportKind::Bundle,
        id,
        display_name,
        version: facts.version,
        entry_point,
        groups,
        evidence,
        ambiguity,
        shadows: Vec::new(),
        files,
        components,
        source,
    })
}

/// The package's install-directory name and its display name.
///
/// A manifest's declared name wins, then the caller's hints (the repository
/// name, the archive's stem), then the components' own parent directory — every
/// one of them through [`sanitize_package_id`], which is what stops an archive
/// of a bare `skills/` folder installing as a package called "skills".
fn resolve_identity(
    facts: &ManifestFacts,
    id_hints: &[String],
    components_root: &str,
) -> Result<(String, String)> {
    let declared_name = facts.name.clone().or_else(|| facts.display_name.clone());
    let id = declared_name
        .as_deref()
        .and_then(sanitize_package_id)
        .or_else(|| id_hints.iter().find_map(|hint| sanitize_package_id(hint)))
        .or_else(|| {
            components_root
                .rsplit('/')
                .next()
                .and_then(sanitize_package_id)
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "this package has no name: no manifest declares one, and nothing in the source \
                 supplies one either"
            )
        })?;
    let display_name = facts
        .display_name
        .clone()
        .or(declared_name)
        .unwrap_or_else(|| id.clone());
    Ok((id, display_name))
}

/// Stamp the router and the group memberships onto the components.
///
/// Both are what flattening destroys: a package installed as N loose skills has
/// no way to say which one is read first, or which are core rather than
/// on-demand.
fn apply_manifest_metadata(
    facts: &ManifestFacts,
    components: &mut [PlannedSkill],
    id: &str,
) -> (Option<String>, BTreeMap<String, Vec<String>>) {
    let entry_point = resolve_entry_point(facts, components, id);
    if let Some(entry_point) = entry_point.as_deref() {
        for component in components.iter_mut() {
            component.entry_point = component.name == entry_point;
        }
    }

    let groups = filter_groups(&facts.groups, components);
    for component in components.iter_mut() {
        component.group = groups
            .iter()
            .find(|(_, names)| names.iter().any(|name| name == &component.name))
            .map(|(group, _)| group.clone());
    }
    (entry_point, groups)
}

/// One skill, at `<root>/<slug>/SKILL.md`.
///
/// Shared by the two ways a source turns out to be a single skill — a root
/// `SKILL.md`, and one folder holding one skill — so the two cannot come to
/// disagree about where a single skill is installed.
fn single_plan(
    component: PlannedSkill,
    files: Vec<(String, Vec<u8>)>,
    id_hints: &[String],
    version: Option<String>,
    source: SourceProvenance,
) -> Result<ImportPlan> {
    let id = sanitize_package_id(&component.name)
        .or_else(|| id_hints.iter().find_map(|hint| sanitize_package_id(hint)))
        .ok_or_else(|| anyhow::anyhow!("could not derive a folder name for this skill"))?;
    Ok(ImportPlan {
        kind: ImportKind::Single,
        display_name: component.name.clone(),
        id,
        version,
        entry_point: None,
        groups: BTreeMap::new(),
        components: vec![component],
        evidence: Evidence::SingleSkill,
        ambiguity: None,
        shadows: Vec::new(),
        files,
        source,
    })
}

/// Parse each member's frontmatter, refusing a package that declares one name
/// twice.
///
/// ⚠ A duplicate is fatal rather than a warning. A skill's identity is its
/// frontmatter `name`, and discovery keys on it — so of two components
/// declaring `same-name`, one would permanently shadow the other, silently and
/// with no way for the user to tell which.
fn collect_components(members: &[(String, &Entry)]) -> Result<Vec<PlannedSkill>> {
    let mut components = Vec::new();
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for (directory, entry) in members {
        let Ok((metadata, _)) = SkillsClient::parse_frontmatter(&entry.text()) else {
            // A component whose frontmatter does not parse is one the extension
            // would never load, so it is not silently carried along as dead
            // weight inside a package that claims to contain it.
            continue;
        };
        if let Some(first) = seen.get(&metadata.name) {
            bail!(
                "this package declares the skill `{}` twice, in `{first}` and `{directory}`. \
                 A skill's identity is its frontmatter name, so one would permanently shadow \
                 the other",
                metadata.name
            );
        }
        seen.insert(metadata.name.clone(), directory.clone());
        components.push(PlannedSkill {
            name: metadata.name,
            description: metadata.description,
            directory: directory.clone(),
            group: None,
            entry_point: false,
        });
    }
    if components.is_empty() {
        bail!("no SKILL.md in this package has valid frontmatter with `name` and `description`");
    }
    Ok(components)
}

/// Where the components live, and what told us.
///
/// A manifest's answer wins. Otherwise the components root is the **directory
/// the `SKILL.md` folders share as their parent** — `pack` for a bundle
/// archive, `skills` for a plugin repository, `""` for skills sitting loose at
/// the archive root. Where they do not all share one parent, the parent with
/// the most children wins, ties going to the shallowest.
///
/// ⚠ An earlier version special-cased `skills/` and otherwise assumed the
/// archive root. That read a plain `<bundle>/<skill>/SKILL.md` archive — the
/// canonical BioRouter bundle, and the one shape the old parser *did* handle —
/// as having no components at all.
fn resolve_components_root(
    facts: &ManifestFacts,
    skill_files: &[(String, &Entry)],
) -> (String, Evidence) {
    if let Some(root) = facts.components_root.clone() {
        let evidence = facts
            .evidence
            .clone()
            .unwrap_or(Evidence::StructuralInference);
        return (root, evidence);
    }

    let mut children: BTreeMap<String, usize> = BTreeMap::new();
    for (dir, _) in skill_files {
        if dir.is_empty() {
            continue;
        }
        let parent = dir
            .rsplit_once('/')
            .map(|(parent, _)| parent.to_string())
            .unwrap_or_default();
        *children.entry(parent).or_default() += 1;
    }

    let root = children
        .into_iter()
        .max_by_key(|(parent, count)| {
            (
                *count,
                // Shallower wins a tie: a support file's own nested folder must
                // never out-rank the level the real components sit at.
                std::cmp::Reverse(parent.matches('/').count()),
            )
        })
        .map(|(parent, _)| parent)
        .unwrap_or_default();

    let evidence = if root == "skills" || root.ends_with("/skills") {
        Evidence::SkillsDirectory
    } else {
        Evidence::StructuralInference
    };
    (root, evidence)
}

/// The `<root>/<name>/SKILL.md` entries, i.e. exactly one level below `root`.
///
/// One level, not any depth: a skill's own `references/` or `scripts/` folder
/// may itself contain a `SKILL.md`-shaped file, and a deeper match would
/// promote a support file to a component.
fn components_in<'a>(skill_files: &[(String, &'a Entry)], root: &str) -> Vec<(String, &'a Entry)> {
    let depth = if root.is_empty() {
        0
    } else {
        root.matches('/').count() + 1
    };
    skill_files
        .iter()
        .filter(|(dir, _)| {
            if dir.is_empty() {
                return false;
            }
            if root.is_empty() {
                dir.matches('/').count() == 0
            } else {
                dir.starts_with(&format!("{root}/")) && dir.matches('/').count() == depth
            }
        })
        .map(|(dir, entry)| (dir.clone(), *entry))
        .collect()
}

/// The router, by declared name or by directory.
///
/// A manifest's `entryPoint` may name either, and when nothing declares one, a
/// component whose name equals the package's own is the router by construction
/// — which is exactly how HyperFrames' `skills/hyperframes/SKILL.md` announces
/// itself.
fn resolve_entry_point(
    facts: &ManifestFacts,
    components: &[PlannedSkill],
    id: &str,
) -> Option<String> {
    if let Some(declared) = facts.entry_point.as_deref() {
        if let Some(component) = components.iter().find(|c| {
            c.name == declared || leaf(&c.directory) == declared || c.directory == declared
        }) {
            return Some(component.name.clone());
        }
    }
    components
        .iter()
        .find(|c| sanitize_package_id(&c.name).as_deref() == Some(id))
        .map(|c| c.name.clone())
}

/// Drop group members the archive does not actually contain, so a manifest that
/// over-promises does not put a phantom name in the interface.
fn filter_groups(
    groups: &BTreeMap<String, Vec<String>>,
    components: &[PlannedSkill],
) -> BTreeMap<String, Vec<String>> {
    let present: BTreeSet<&str> = components.iter().map(|c| c.name.as_str()).collect();
    groups
        .iter()
        .map(|(group, names)| {
            (
                group.clone(),
                names
                    .iter()
                    .filter(|name| present.contains(name.as_str()))
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        })
        .filter(|(_, names)| !names.is_empty())
        .collect()
}

/// When must the caller decide?
///
/// Only when **no manifest spoke** and the components sit loose at the archive
/// root — which is equally the shape of one package and of a folder somebody
/// happened to zip.
///
/// A named parent directory is not ambiguous: whoever put `alpha` and `beta`
/// inside `superpowers/` already said they belong together, and that includes
/// `skills/`. Neither is anything a manifest defined — which is why an explicit
/// manifest installs as a bundle without making the user approve all twenty
/// children, as #115 asks.
fn ambiguity_for(
    facts: &ManifestFacts,
    components: &[PlannedSkill],
    components_root: &str,
) -> Option<Ambiguity> {
    if facts.evidence.is_some() || !components_root.is_empty() {
        return None;
    }
    if components.len() < 2 {
        return None;
    }
    Some(Ambiguity {
        reason: format!(
            "This source holds {} skills side by side and declares no package manifest, so \
             Biorouter cannot tell whether they belong together. Install them as one bundle, \
             or pick the ones you want as separate skills.",
            components.len()
        ),
        components: components.iter().map(|c| c.name.clone()).collect(),
    })
}

/// The files of one component (or of the whole archive when `directory` is
/// empty), minus the manifests.
fn keep_files(entries: &[Entry], directory: &str) -> Vec<(String, Vec<u8>)> {
    archive::entries_under(entries, directory)
        .into_iter()
        .filter(|entry| !MANIFEST_FILES.contains(&entry.name.as_str()))
        .filter(|entry| {
            // Never carry a nested package record into a component's directory:
            // the record install writes belongs at the package root, and a
            // second one a level down would make the catalog see a bundle
            // inside a bundle.
            entry.name != crate::agents::skill_catalog::PACKAGE_RECORD_FILE
        })
        .map(|entry| (entry.name, entry.data))
        .collect()
}

/// Preserve a bundle's declared component directories plus the small set of
/// shared package roots skill ecosystems conventionally reference.  Root
/// installers and arbitrary repository files remain outside the import plan.
fn keep_bundle_files(
    entries: &[Entry],
    components: &[PlannedSkill],
    source_package_root: Option<&str>,
) -> Vec<(String, Vec<u8>)> {
    const SHARED_ROOTS: &[&str] = &["assets", "references", "scripts"];

    entries
        .iter()
        .filter(|entry| !MANIFEST_FILES.contains(&entry.name.as_str()))
        // ⚠ Compare the BASENAME, not the full archive path. `entry.name` is the
        // path as it sits in the archive, so an equality test against
        // `PACKAGE_RECORD_FILE` matches only a record at the archive ROOT — and
        // this function's predecessor rebased names before comparing, which is
        // why its doc comment could say "Never carry a nested package record
        // into a component's directory" and mean it.
        //
        // A record at `<component>/biorouter-package.json` therefore survived
        // the filter, and `into_individual` then strips the component prefix —
        // depositing it at the installed skill's own root, where the catalog
        // reads it as a package record and the skill goes missing.
        .filter(|entry| {
            entry.name.rsplit('/').next() != Some(crate::agents::skill_catalog::PACKAGE_RECORD_FILE)
        })
        .filter(|entry| {
            components.iter().any(|component| {
                entry.name == format!("{}/SKILL.md", component.directory)
                    || entry.name.starts_with(&format!("{}/", component.directory))
            }) || {
                source_package_relative(&entry.name, source_package_root).is_some_and(|relative| {
                    SHARED_ROOTS
                        .iter()
                        .any(|root| relative.starts_with(&format!("{root}/")))
                })
            }
        })
        .map(|entry| {
            (
                package_relative_path(&entry.name, source_package_root),
                entry.data.clone(),
            )
        })
        .collect()
}

fn package_relative_path(path: &str, source_package_root: Option<&str>) -> String {
    source_package_relative(path, source_package_root)
        .unwrap_or(path)
        .to_string()
}

fn source_package_relative<'a>(
    path: &'a str,
    source_package_root: Option<&str>,
) -> Option<&'a str> {
    match source_package_root {
        Some(root) => path.strip_prefix(&format!("{root}/")),
        None => Some(path),
    }
}

fn leaf(directory: &str) -> &str {
    directory.rsplit('/').next().unwrap_or(directory)
}

#[cfg(test)]
mod nested_record_tests {
    use super::*;
    use crate::agents::skill_package::archive::Entry;

    fn entry(name: &str, body: &str) -> Entry {
        Entry {
            name: name.to_string(),
            data: body.as_bytes().to_vec(),
        }
    }

    fn skill(name: &str) -> String {
        format!("---\nname: {name}\ndescription: a fixture skill for {name}\n---\n\nbody\n")
    }

    /// A package record one level down used to survive the bundle filter, and
    /// `into_individual` then strips the component prefix — depositing it at the
    /// installed skill's own root, where the catalog reads it as a package
    /// record and the skill itself goes missing.
    ///
    /// The predecessor of `keep_bundle_files` rebased names before comparing,
    /// which is why its doc comment could promise "Never carry a nested package
    /// record into a component's directory". The promise outlived the code.
    #[test]
    fn a_nested_package_record_never_reaches_a_component() {
        let entries = vec![
            entry("alpha/SKILL.md", &skill("alpha")),
            entry("alpha/biorouter-package.json", r#"{"id":"alpha"}"#),
            entry("alpha/references/notes.md", "keep me"),
            entry("beta/SKILL.md", &skill("beta")),
            entry(
                crate::agents::skill_catalog::PACKAGE_RECORD_FILE,
                r#"{"id":"pkg"}"#,
            ),
        ];
        let plan = plan_from_entries(entries, &["pkg".to_string()], Default::default())
            .expect("a two-component bundle plans");

        for (path, _) in &plan.files {
            assert!(
                path.rsplit('/').next() != Some(crate::agents::skill_catalog::PACKAGE_RECORD_FILE),
                "a package record survived at {path}"
            );
        }

        // And the component's own supporting files are still there — the filter
        // must drop the record, not the directory.
        assert!(
            plan.files
                .iter()
                .any(|(path, _)| path.ends_with("references/notes.md")),
            "the filter took a supporting file with it: {:?}",
            plan.files.iter().map(|(p, _)| p).collect::<Vec<_>>()
        );

        // The shape the bug actually produced: after the component prefix is
        // stripped, the record would sit at the installed skill's root.
        let individual = plan.into_individual(&["alpha".to_string()]);
        let alpha = individual.first().expect("alpha plans on its own");
        assert!(
            !alpha
                .files
                .iter()
                .any(|(path, _)| path == crate::agents::skill_catalog::PACKAGE_RECORD_FILE),
            "an individually-installed skill carries a package record at its root: {:?}",
            alpha.files.iter().map(|(p, _)| p).collect::<Vec<_>>()
        );
    }
}
