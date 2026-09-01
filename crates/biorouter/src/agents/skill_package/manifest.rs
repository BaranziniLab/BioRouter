//! The detection ladder: explicit package metadata, in order, before any
//! guess about shape.
//!
//! Each rung answers three questions — what is this package called, where do
//! its components live, and which of them is the way in — and the first rung
//! that answers is the one used. A rung that fires records itself as
//! [`Evidence`], so a preview can say *why*, and a test can assert the manifest
//! decided rather than the structure happening to agree with it.

use serde::Deserialize;
use std::collections::BTreeMap;

use super::archive::Entry;
use super::Evidence;

/// What a manifest said. Only `components_root` is required to be useful;
/// everything else is a refinement over what inference would have guessed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ManifestFacts {
    pub evidence: Option<Evidence>,
    /// The package's declared name, before slugging.
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub version: Option<String>,
    /// The directory holding one folder per component, relative to the archive
    /// root. `""` means the root itself.
    pub components_root: Option<String>,
    /// The router, by the component's declared `name` or its directory.
    pub entry_point: Option<String>,
    /// Named groups (`core`, `on-demand`, …) → component names.
    pub groups: BTreeMap<String, Vec<String>>,
    /// Components the manifest declares, by name. Used to report a manifest
    /// that promises more than the archive delivers.
    pub declared: Vec<String>,
}

// ---------------------------------------------------------------------------
// The manifest shapes.
// ---------------------------------------------------------------------------

/// `biorouter-package.json` — what [`super::install`] writes, so re-importing a
/// package this importer produced round-trips exactly.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BiorouterManifest {
    id: Option<String>,
    #[serde(alias = "name")]
    display_name: Option<String>,
    version: Option<String>,
    entry_point: Option<String>,
    #[serde(default)]
    groups: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    components: Vec<BiorouterComponent>,
    /// Where components live, when they are not at the record's own level.
    skills_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BiorouterComponent {
    name: String,
    directory: Option<String>,
}

/// `.codex-plugin/plugin.json` and `.claude-plugin/plugin.json`.
///
/// `skills` is a path in both, and both spell it with a leading `./` and a
/// trailing `/` in the wild — normalised in [`normalize_path`].
///
/// ⚠ **The two are not interchangeable, which is why the ladder merges.**
/// HyperFrames ships both, and only the Codex one carries `skills` at all —
/// its `.claude-plugin/plugin.json` is name, version and prose. Reading a
/// single rung would have found a package with no components root.
#[derive(Debug, Deserialize)]
struct PluginManifest {
    name: Option<String>,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    version: Option<String>,
    #[serde(default)]
    skills: Option<serde_json::Value>,
    /// The vendor's presentation block. Its `displayName` is where a
    /// human-readable name actually lives in the wild — HyperFrames' is
    /// "HyperFrames by HeyGen" — while the top-level `displayName` this
    /// originally read is absent from every real manifest inspected.
    #[serde(default)]
    interface: Option<PluginInterface>,
}

#[derive(Debug, Deserialize)]
struct PluginInterface {
    #[serde(rename = "displayName")]
    display_name: Option<String>,
}

/// `skills-manifest.json`, as HyperFrames ships it: a source and a component
/// list.
///
/// ⚠ **`skills` is a MAP in the real file**, keyed by component name with a
/// content hash and file count as the value — not the array this originally
/// assumed. A `Vec` here does not merely miss the names: serde fails the whole
/// document, `detect` reports "skills-manifest.json is not valid JSON", and the
/// import is refused outright. The fixture that said otherwise was invented,
/// and only running the real archive through the real pipeline found it.
/// [`SkillsList`] now accepts both, plus a bare string list.
#[derive(Debug, Deserialize)]
struct SkillsManifest {
    name: Option<String>,
    source: Option<String>,
    version: Option<String>,
    #[serde(rename = "entryPoint", alias = "entry_point", alias = "router")]
    entry_point: Option<String>,
    #[serde(default)]
    skills: SkillsList,
    /// `{"core": ["a"], "on-demand": ["b"]}`, when present.
    #[serde(default)]
    groups: BTreeMap<String, Vec<String>>,
}

/// The component list, in every shape seen in the wild.
#[derive(Debug, Default, Deserialize)]
#[serde(untagged)]
enum SkillsList {
    /// `["a", "b"]`, or `[{"name": "a"}, …]`.
    Array(Vec<serde_json::Value>),
    /// `{"a": {"hash": …}, "b": {…}}` — HyperFrames' actual shape.
    Map(BTreeMap<String, serde_json::Value>),
    #[default]
    Absent,
}

impl SkillsList {
    fn names(&self) -> Vec<String> {
        match self {
            SkillsList::Array(items) => items.iter().filter_map(component_name).collect(),
            SkillsList::Map(map) => map.keys().cloned().collect(),
            SkillsList::Absent => Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------

fn find<'a>(entries: &'a [Entry], name: &str) -> Option<&'a Entry> {
    entries.iter().find(|entry| entry.name == name)
}

/// `"./skills/"` → `"skills"`, `""`/`"."`/`"./"` → `""` (the archive root).
fn normalize_path(raw: &str) -> String {
    raw.replace('\\', "/")
        .trim_start_matches("./")
        .trim_matches('/')
        .trim_end_matches('.')
        .trim_matches('/')
        .to_string()
}

/// A plugin manifest's `skills` may be a path, or a list of paths, or a list of
/// component objects. Only a path tells us where to look; a list of components
/// is read for names.
fn plugin_skills_path(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(path) => Some(normalize_path(path)),
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(|item| item.as_str())
            .map(normalize_path)
            // A list of component directories shares one parent; that parent is
            // the components root.
            .map(|path| {
                path.rsplit_once('/')
                    .map(|(parent, _)| parent.to_string())
                    .unwrap_or_default()
            })
            .next(),
        _ => None,
    }
}

fn component_name(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(name) => Some(name.clone()),
        serde_json::Value::Object(map) => map
            .get("name")
            .or_else(|| map.get("id"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
        _ => None,
    }
}

/// The last component of a `owner/repo` source string.
fn repo_name(source: &str) -> Option<String> {
    source
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|part| !part.is_empty())
        .map(str::to_string)
}

impl ManifestFacts {
    /// Take from `lower` only what `self` does not already have.
    ///
    /// ⚠ **The ladder merges; it does not pick one winner and discard the
    /// rest.** A real repository carries complementary files rather than
    /// competing ones — HyperFrames' `.codex-plugin/plugin.json` supplies the
    /// package name, version and skills path, while its `skills-manifest.json`
    /// supplies the router and the core/on-demand groups. Reading only the
    /// higher rung dropped the groups on the floor, and the package installed
    /// with no core/on-demand distinction at all: present, but silently poorer
    /// than the source it came from.
    ///
    /// Priority is still absolute per field, and `evidence` reports the highest
    /// rung that contributed — so "the manifest decided the identity" stays
    /// assertable.
    fn fill_from(&mut self, lower: ManifestFacts) {
        self.evidence = self.evidence.take().or(lower.evidence);
        self.name = self.name.take().or(lower.name);
        self.display_name = self.display_name.take().or(lower.display_name);
        self.version = self.version.take().or(lower.version);
        self.components_root = self.components_root.take().or(lower.components_root);
        self.entry_point = self.entry_point.take().or(lower.entry_point);
        if self.groups.is_empty() {
            self.groups = lower.groups;
        }
        if self.declared.is_empty() {
            self.declared = lower.declared;
        }
    }
}

/// Walk the ladder, merging every rung that answered into one set of facts —
/// see [`ManifestFacts::fill_from`] for why merging rather than picking.
/// Everything is `None` when nothing spoke, at which point the caller infers
/// from structure.
///
/// ⚠ A malformed manifest **does not fall through silently to the next rung**
/// with its identity lost. `detect` returns the parse error, so the caller can
/// refuse the import and say which file is broken. Treating a corrupt
/// `plugin.json` as "no manifest" is how a package with a declared identity
/// gets installed under an inferred one.
pub fn detect(entries: &[Entry]) -> anyhow::Result<ManifestFacts> {
    let mut merged = ManifestFacts::default();
    for rung in [
        biorouter_record(entries)?,
        plugin_manifest(entries, ".codex-plugin/plugin.json", Evidence::CodexPlugin)?,
        plugin_manifest(
            entries,
            ".claude-plugin/plugin.json",
            Evidence::ClaudePlugin,
        )?,
        skills_manifest(entries)?,
    ]
    .into_iter()
    .flatten()
    {
        merged.fill_from(rung);
    }
    Ok(merged)
}

fn biorouter_record(entries: &[Entry]) -> anyhow::Result<Option<ManifestFacts>> {
    if let Some(entry) = find(entries, crate::agents::skill_catalog::PACKAGE_RECORD_FILE) {
        let parsed: BiorouterManifest = serde_json::from_str(&entry.text()).map_err(|e| {
            anyhow::anyhow!(
                "{} is not valid JSON: {e}",
                crate::agents::skill_catalog::PACKAGE_RECORD_FILE
            )
        })?;
        let inferred_root = common_record_component_root(&parsed.components);
        return Ok(Some(ManifestFacts {
            evidence: Some(Evidence::BiorouterManifest),
            name: parsed.id.clone(),
            display_name: parsed.display_name.or(parsed.id),
            version: parsed.version,
            components_root: Some(
                parsed
                    .skills_path
                    .as_deref()
                    .map(normalize_path)
                    .or(inferred_root)
                    .unwrap_or_default(),
            ),
            entry_point: parsed.entry_point,
            groups: parsed.groups,
            declared: parsed.components.into_iter().map(|c| c.name).collect(),
        }));
    }
    Ok(None)
}

fn common_record_component_root(components: &[BiorouterComponent]) -> Option<String> {
    let mut parents = components.iter().map(|component| {
        let directory = normalize_path(component.directory.as_deref()?);
        let (parent, _) = directory.rsplit_once('/').unwrap_or(("", &directory));
        Some(parent.to_string())
    });
    let first = parents.next()??;
    parents
        .all(|parent| parent.as_deref() == Some(first.as_str()))
        .then_some(first)
}

fn plugin_manifest(
    entries: &[Entry],
    path: &str,
    evidence: Evidence,
) -> anyhow::Result<Option<ManifestFacts>> {
    let Some(entry) = find(entries, path) else {
        return Ok(None);
    };
    let parsed: PluginManifest = serde_json::from_str(&entry.text())
        .map_err(|e| anyhow::anyhow!("{path} is not valid JSON: {e}"))?;
    let components_root = parsed.skills.as_ref().and_then(plugin_skills_path);
    let display_name = parsed
        .display_name
        .or_else(|| parsed.interface.and_then(|i| i.display_name))
        .or_else(|| parsed.name.clone());
    Ok(Some(ManifestFacts {
        evidence: Some(evidence),
        name: parsed.name,
        display_name,
        version: parsed.version,
        components_root,
        entry_point: None,
        groups: BTreeMap::new(),
        declared: Vec::new(),
    }))
}

fn skills_manifest(entries: &[Entry]) -> anyhow::Result<Option<ManifestFacts>> {
    if let Some(entry) = find(entries, "skills-manifest.json") {
        let parsed: SkillsManifest = serde_json::from_str(&entry.text())
            .map_err(|e| anyhow::anyhow!("skills-manifest.json is not valid JSON: {e}"))?;
        let name = parsed
            .name
            .clone()
            .or_else(|| parsed.source.as_deref().and_then(repo_name));
        return Ok(Some(ManifestFacts {
            evidence: Some(Evidence::SkillsManifest),
            display_name: name.clone(),
            name,
            version: parsed.version,
            // Deliberately unset: this manifest names components, not a path,
            // and the components may sit at the root or under `skills/`. The
            // caller resolves that from the archive rather than guessing here.
            components_root: None,
            entry_point: parsed.entry_point,
            groups: parsed.groups,
            declared: parsed.skills.names(),
        }));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, body: &str) -> Entry {
        Entry {
            name: name.to_string(),
            data: body.as_bytes().to_vec(),
        }
    }

    #[test]
    fn a_codex_plugin_manifest_supplies_the_name_and_the_skills_path() {
        let facts = detect(&[entry(
            ".codex-plugin/plugin.json",
            r#"{"name":"hyperframes","version":"0.8.12","skills":"./skills/"}"#,
        )])
        .unwrap();
        assert_eq!(facts.evidence, Some(Evidence::CodexPlugin));
        assert_eq!(facts.name.as_deref(), Some("hyperframes"));
        assert_eq!(facts.version.as_deref(), Some("0.8.12"));
        assert_eq!(facts.components_root.as_deref(), Some("skills"));
    }

    /// The real HyperFrames `skills-manifest.json`, in miniature: `skills` is a
    /// map keyed by name, and there is no `entryPoint` or `groups` at all.
    /// Reading it as an array failed the whole document and refused the import.
    #[test]
    fn a_skills_manifest_reads_the_map_shape_the_real_file_uses() {
        let facts = detect(&[entry(
            "skills-manifest.json",
            r#"{"source":"heygen-com/hyperframes","skills":{
                 "hyperframes":{"hash":"5be130da8d7ff59e","files":17},
                 "media-use":{"hash":"1b0ce647f5c7df95","files":152}}}"#,
        )])
        .unwrap();
        assert_eq!(facts.evidence, Some(Evidence::SkillsManifest));
        assert_eq!(facts.declared, vec!["hyperframes", "media-use"]);
        assert_eq!(facts.entry_point, None, "the real file declares none");
        assert!(facts.groups.is_empty());
    }

    /// A human-readable name lives in the vendor's `interface` block in every
    /// real manifest inspected; the top-level `displayName` this first read is
    /// absent from all of them.
    #[test]
    fn a_plugin_manifests_interface_block_supplies_the_display_name() {
        let facts = detect(&[entry(
            ".codex-plugin/plugin.json",
            r#"{"name":"hyperframes","version":"0.8.12","skills":"./skills/",
                "interface":{"displayName":"HyperFrames by HeyGen"}}"#,
        )])
        .unwrap();
        assert_eq!(facts.name.as_deref(), Some("hyperframes"));
        assert_eq!(facts.display_name.as_deref(), Some("HyperFrames by HeyGen"));
    }

    #[test]
    fn a_skills_manifest_names_the_package_after_its_source_repository() {
        let facts = detect(&[entry(
            "skills-manifest.json",
            r#"{"source":"heygen-com/hyperframes","skills":["hyperframes","media-use"]}"#,
        )])
        .unwrap();
        assert_eq!(facts.evidence, Some(Evidence::SkillsManifest));
        assert_eq!(facts.name.as_deref(), Some("hyperframes"));
        assert_eq!(facts.declared, vec!["hyperframes", "media-use"]);
    }

    #[test]
    fn a_skills_manifest_reads_object_components_and_groups() {
        let facts = detect(&[entry(
            "skills-manifest.json",
            r#"{"name":"HyperFrames","router":"hyperframes",
                "skills":[{"name":"hyperframes"},{"name":"slideshow"}],
                "groups":{"core":["hyperframes"],"on-demand":["slideshow"]}}"#,
        )])
        .unwrap();
        assert_eq!(facts.entry_point.as_deref(), Some("hyperframes"));
        assert_eq!(facts.declared, vec!["hyperframes", "slideshow"]);
        assert_eq!(facts.groups["core"], vec!["hyperframes"]);
        assert_eq!(facts.groups["on-demand"], vec!["slideshow"]);
    }

    /// The ladder's order is the contract. A repository carrying both must be
    /// read as its own package record, not as whatever the plugin file says.
    #[test]
    fn the_biorouter_record_outranks_a_plugin_manifest() {
        let facts = detect(&[
            entry(
                crate::agents::skill_catalog::PACKAGE_RECORD_FILE,
                r#"{"id":"ours","displayName":"Ours","version":"2"}"#,
            ),
            entry(
                ".codex-plugin/plugin.json",
                r#"{"name":"theirs","version":"1","skills":"./skills/"}"#,
            ),
        ])
        .unwrap();
        assert_eq!(facts.evidence, Some(Evidence::BiorouterManifest));
        assert_eq!(facts.name.as_deref(), Some("ours"));
    }

    #[test]
    fn an_older_biorouter_record_recovers_its_component_root_from_directories() {
        let facts = detect(&[entry(
            crate::agents::skill_catalog::PACKAGE_RECORD_FILE,
            r#"{"id":"destiny-skill","components":[
                {"name":"destiny","directory":"skills/destiny"},
                {"name":"destiny-mbti","directory":"skills/destiny-mbti"}
            ]}"#,
        )])
        .unwrap();

        assert_eq!(facts.components_root.as_deref(), Some("skills"));
        assert_eq!(facts.declared, vec!["destiny", "destiny-mbti"]);
    }

    /// A broken manifest must not be read as "no manifest". Falling through
    /// would install a package that declares an identity under an inferred one.
    #[test]
    fn a_malformed_manifest_is_an_error_rather_than_a_silent_fall_through() {
        let err = detect(&[entry(".codex-plugin/plugin.json", "{not json")]).unwrap_err();
        assert!(err.to_string().contains("plugin.json is not valid JSON"));
    }

    #[test]
    fn no_manifest_yields_no_evidence_and_the_caller_infers() {
        assert_eq!(
            detect(&[entry("a/SKILL.md", "---")]).unwrap(),
            ManifestFacts::default()
        );
    }

    /// The rungs are complementary in the wild, not competing: HyperFrames'
    /// plugin file carries the identity and skills path, its skills manifest
    /// carries the router and the groups. Picking one rung dropped the groups.
    #[test]
    fn a_lower_rung_fills_in_what_a_higher_one_did_not_say() {
        let facts = detect(&[
            entry(
                ".codex-plugin/plugin.json",
                r#"{"name":"hyperframes","version":"0.8.12","skills":"./skills/"}"#,
            ),
            entry(
                "skills-manifest.json",
                r#"{"source":"heygen-com/other","router":"hyperframes",
                    "skills":["hyperframes"],"groups":{"core":["hyperframes"]}}"#,
            ),
        ])
        .unwrap();
        // Identity from the higher rung...
        assert_eq!(facts.evidence, Some(Evidence::CodexPlugin));
        assert_eq!(facts.name.as_deref(), Some("hyperframes"));
        assert_eq!(facts.components_root.as_deref(), Some("skills"));
        // ...and what it never mentioned, from the lower one.
        assert_eq!(facts.entry_point.as_deref(), Some("hyperframes"));
        assert_eq!(facts.groups["core"], vec!["hyperframes"]);
    }

    #[test]
    fn a_skills_path_is_normalised_whatever_the_manifest_spelled() {
        for spelling in ["./skills/", "skills", "/skills", "./skills"] {
            let facts = detect(&[entry(
                ".codex-plugin/plugin.json",
                &format!(r#"{{"name":"p","skills":"{spelling}"}}"#),
            )])
            .unwrap();
            assert_eq!(
                facts.components_root.as_deref(),
                Some("skills"),
                "{spelling}"
            );
        }
        let root = detect(&[entry(
            ".codex-plugin/plugin.json",
            r#"{"name":"p","skills":"./"}"#,
        )])
        .unwrap();
        assert_eq!(root.components_root.as_deref(), Some(""));
    }
}
