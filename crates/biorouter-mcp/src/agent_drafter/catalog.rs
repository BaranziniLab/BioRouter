//! What this BioRouter install actually has.
//!
//! Agent Drafter's 12 tools were all create/configure/build/launch/read — **none
//! of them could ask what exists**. The authoring instructions nonetheless told
//! the model to give the app "the extensions/skills/knowledge it needs", and
//! `AgentConfig` had no field in which to say *"this app wants a ClinVar KB and
//! there isn't one"*. So the only way for the model to express the need was to
//! invent an id.
//!
//! It did, at scale. Across the 100-app test drive: 13 invented knowledge-base
//! ids, 7 nonexistent skill lists, and — memorably — the literal string `br.kb`,
//! which is the *client API namespace*, not an identifier. The runtime then made
//! it worse: the server armed the `skills` extension whenever the skill list was
//! non-empty (real or not) and wrote a system prompt commanding the model to use
//! skills that could not load, so turn 1 failed by construction.
//!
//! The model's error was epistemic — it could not see its environment — so no
//! amount of prompt text could fix it. This module is the missing eye. It backs:
//!
//!   * the `list_platform_catalog` tool (discovery),
//!   * write-boundary rejection in `create_app`/`configure_app` (an unknown id
//!     cannot be saved),
//!   * the `requires` manifest block (a typed slot for an unmet need), and
//!   * the runtime capability report (what was requested vs what is real).

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::BUILTIN_EXTENSION_NAMES;

/// An installed knowledge base.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KbEntry {
    pub id: String,
    pub name: String,
}

/// An installed skill.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillEntry {
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// An extension the app may name.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtEntry {
    pub name: String,
    /// Built-in extensions ship with the daemon and are always available.
    pub builtin: bool,
}

/// The environment an app is being authored against.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Catalog {
    pub knowledge_bases: Vec<KbEntry>,
    pub skills: Vec<SkillEntry>,
    pub extensions: Vec<ExtEntry>,
}

impl Catalog {
    /// Scan this install. Honours `BIOROUTER_PATH_ROOT` through
    /// [`crate::paths`], so a sandboxed run sees the sandbox's catalog — not the
    /// user's global one. (Before the path fix, an "isolated" test drive was
    /// reading the user's real knowledge bases.)
    pub fn discover() -> Self {
        Self {
            knowledge_bases: discover_kbs(),
            skills: discover_skills(),
            extensions: BUILTIN_EXTENSION_NAMES
                .iter()
                .map(|name| ExtEntry {
                    name: (*name).to_string(),
                    builtin: true,
                })
                .chain(discover_external_extensions())
                .collect(),
        }
    }

    /// An empty catalog — for tests, and for the "nothing is installed" case that
    /// the test drive ran against without knowing it.
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn has_kb(&self, id: &str) -> bool {
        self.knowledge_bases.iter().any(|k| k.id == id)
    }

    pub fn has_skill(&self, id: &str) -> bool {
        self.skills.iter().any(|s| s.id == id)
    }

    pub fn has_extension(&self, name: &str) -> bool {
        self.extensions.iter().any(|e| e.name == name)
    }

    pub fn kb_ids(&self) -> Vec<&str> {
        self.knowledge_bases.iter().map(|k| k.id.as_str()).collect()
    }

    pub fn skill_ids(&self) -> Vec<&str> {
        self.skills.iter().map(|s| s.id.as_str()).collect()
    }

    pub fn extension_names(&self) -> Vec<&str> {
        self.extensions.iter().map(|e| e.name.as_str()).collect()
    }

    /// Render a list for an error message. An empty catalog says so in words —
    /// "(none installed)" is actionable; an empty list looks like a bug.
    pub fn render_list(items: &[&str]) -> String {
        if items.is_empty() {
            "(none installed)".to_string()
        } else {
            items.join(", ")
        }
    }
}

fn discover_kbs() -> Vec<KbEntry> {
    let Ok(service) = crate::knowledge::service::KnowledgeService::new_default() else {
        return Vec::new();
    };
    service
        .list_bases()
        .map(|bases| {
            bases
                .into_iter()
                .map(|m| KbEntry {
                    id: m.id,
                    name: m.name,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The directories `biorouter`'s `SkillsExtension` scans. Duplicated here because
/// `biorouter-mcp` cannot depend on `biorouter` (circular) — pinned against drift
/// by a cross-crate agreement test in `biorouter`
/// (`tests/skill_catalog_agreement.rs`).
fn skill_directories() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    if let Ok(home) = etcetera::home_dir() {
        dirs.push(home.join(".claude/skills"));
        dirs.push(home.join(".config/agents/skills"));
    }

    dirs.push(crate::paths::in_config_dir("skills"));

    let extensions_dir = crate::paths::in_config_dir("extensions");
    if let Ok(entries) = std::fs::read_dir(&extensions_dir) {
        for entry in entries.flatten() {
            let skills_subdir = entry.path().join("skills");
            if skills_subdir.is_dir() {
                dirs.push(skills_subdir);
            }
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.join(".claude/skills"));
        dirs.push(cwd.join(".biorouter/skills"));
        dirs.push(cwd.join(".agents/skills"));
    }

    dirs
}

/// A skill is a directory holding a `SKILL.md`, whose id is the frontmatter
/// `name`. A directory of such directories is a *bundle*, scanned one level deep.
fn discover_skills() -> Vec<SkillEntry> {
    let mut found: Vec<SkillEntry> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();

    for dir in skill_directories() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if path.join("SKILL.md").exists() {
                if let Some(skill) = read_skill(&path.join("SKILL.md")) {
                    if seen.insert(skill.id.clone()) {
                        found.push(skill);
                    }
                }
            } else if let Ok(subs) = std::fs::read_dir(&path) {
                // A bundle: <dir>/<bundle>/<skill>/SKILL.md
                for sub in subs.flatten() {
                    let sub_skill = sub.path().join("SKILL.md");
                    if sub_skill.exists() {
                        if let Some(skill) = read_skill(&sub_skill) {
                            if seen.insert(skill.id.clone()) {
                                found.push(skill);
                            }
                        }
                    }
                }
            }
        }
    }

    found.sort_by(|a, b| a.id.cmp(&b.id));
    found
}

/// Pull `name` and `description` out of a `SKILL.md`'s YAML frontmatter. Kept
/// deliberately small — we need the *id*, not the skill.
fn read_skill(skill_md: &std::path::Path) -> Option<SkillEntry> {
    let text = std::fs::read_to_string(skill_md).ok()?;
    let body = text.strip_prefix("---")?;
    let end = body.find("\n---")?;
    let frontmatter = &body[..end];

    let mut id = None;
    let mut description = String::new();
    for line in frontmatter.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim_matches('\'').trim();
        match key.trim() {
            "name" if !value.is_empty() => id = Some(value.to_string()),
            "description" if !value.is_empty() => description = value.to_string(),
            _ => {}
        }
    }

    Some(SkillEntry {
        id: id?,
        description,
    })
}

/// Installed `.brxt` extensions — each is a directory under `<config>/extensions`.
fn discover_external_extensions() -> Vec<ExtEntry> {
    let dir = crate::paths::in_config_dir("extensions");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<ExtEntry> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| !BUILTIN_EXTENSION_NAMES.contains(&name.as_str()))
        .map(|name| ExtEntry {
            name,
            builtin: false,
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_always_present() {
        let c = Catalog::discover();
        for name in BUILTIN_EXTENSION_NAMES {
            assert!(c.has_extension(name), "builtin '{name}' missing");
        }
    }

    #[test]
    fn an_empty_catalog_says_so_in_words() {
        let c = Catalog::empty();
        assert_eq!(Catalog::render_list(&c.kb_ids()), "(none installed)");
        assert_eq!(Catalog::render_list(&c.skill_ids()), "(none installed)");
        assert!(!c.has_kb("clinvar"));
        assert!(!c.has_skill("pathway-analysis"));
    }

    #[test]
    fn render_list_joins_real_ids() {
        assert_eq!(Catalog::render_list(&["a", "b"]), "a, b");
    }

    #[test]
    fn a_skill_is_read_from_its_frontmatter_name_not_its_directory_name() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("some-folder-name");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: pathway-analysis\ndescription: Reason over pathways\n---\n\nBody.\n",
        )
        .unwrap();

        let skill = read_skill(&skill_dir.join("SKILL.md")).expect("parsed");
        assert_eq!(skill.id, "pathway-analysis");
        assert_eq!(skill.description, "Reason over pathways");
    }

    #[test]
    fn a_skill_md_without_a_name_is_not_a_skill() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("SKILL.md"),
            "---\ndescription: no name here\n---\n",
        )
        .unwrap();
        assert!(read_skill(&dir.path().join("SKILL.md")).is_none());
    }
}
