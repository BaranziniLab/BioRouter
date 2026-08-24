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
        let id = sanitize_package_id(&metadata.name)
            .or_else(|| id_hints.iter().find_map(|hint| sanitize_package_id(hint)))
            .ok_or_else(|| anyhow::anyhow!("could not derive a folder name for this skill"))?;
        return Ok(ImportPlan {
            kind: ImportKind::Single,
            display_name: metadata.name.clone(),
            id,
            version: facts.version.clone(),
            entry_point: None,
            groups: BTreeMap::new(),
            components: vec![PlannedSkill {
                name: metadata.name,
                description: metadata.description,
                directory: String::new(),
                group: None,
                entry_point: false,
            }],
            evidence: Evidence::SingleSkill,
            ambiguity: None,
            shadows: Vec::new(),
            files: keep_files(&entries, ""),
            source,
        });
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

    let mut components = Vec::new();
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for (directory, entry) in &members {
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

    // ------------------------------------------------------------- identity
    let declared_name = facts.name.clone().or_else(|| facts.display_name.clone());
    let id = declared_name
        .as_deref()
        .and_then(sanitize_package_id)
        .or_else(|| id_hints.iter().find_map(|hint| sanitize_package_id(hint)))
        .or_else(|| {
            // Last resort: the components' own parent, but never when that is
            // the generic `skills` — see `sanitize_package_id`.
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

    // ---------------------------------------------------------- entry point
    let entry_point = resolve_entry_point(&facts, &components, &id);
    if let Some(entry_point) = entry_point.as_deref() {
        for component in &mut components {
            component.entry_point = component.name == entry_point;
        }
    }

    // --------------------------------------------------------------- groups
    let groups = filter_groups(&facts.groups, &components);
    for component in &mut components {
        component.group = groups
            .iter()
            .find(|(_, names)| names.iter().any(|name| name == &component.name))
            .map(|(group, _)| group.clone());
    }

    let single_component = components.len() == 1;
    let evidence = facts
        .evidence
        .clone()
        .unwrap_or_else(|| root_evidence.clone());

    // A single component and no manifest is one skill in one folder, not a
    // package of one — installing it as a bundle would put it a level deeper
    // than every other single-skill install.
    if single_component && facts.evidence.is_none() {
        let component = components.remove(0);
        let directory = component.directory.clone();
        let id = sanitize_package_id(&component.name)
            .or_else(|| id_hints.iter().find_map(|hint| sanitize_package_id(hint)))
            .unwrap_or(id);
        return Ok(ImportPlan {
            kind: ImportKind::Single,
            display_name: component.name.clone(),
            id,
            version: facts.version,
            entry_point: None,
            groups: BTreeMap::new(),
            components: vec![PlannedSkill {
                directory: String::new(),
                ..component
            }],
            evidence: Evidence::SingleSkill,
            ambiguity: None,
            shadows: Vec::new(),
            files: keep_files(&entries, &directory),
            source,
        });
    }

    let ambiguity = ambiguity_for(&facts, &components, &components_root);

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
        files: components
            .iter()
            .flat_map(|component| {
                keep_files(&entries, &component.directory)
                    .into_iter()
                    .map(|(path, data)| (format!("{}/{path}", leaf(&component.directory)), data))
            })
            .collect(),
        components: components
            .into_iter()
            .map(|component| PlannedSkill {
                directory: leaf(&component.directory),
                ..component
            })
            .collect(),
        source,
    })
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

/// The last path component — the directory name a component keeps on disk.
///
/// ⚠ **This preserves the source's own folder name**, `media-use` and
/// `slideshow` included. No package prefix is added: several of HyperFrames'
/// members do not start with `hyperframes-`, so a prefix would be both a lie
/// about their identity and useless as a detector.
fn leaf(directory: &str) -> String {
    directory
        .rsplit('/')
        .next()
        .unwrap_or(directory)
        .to_string()
}
