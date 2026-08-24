use crate::agents::extension::PlatformExtensionContext;
use crate::agents::mcp_client::{Error, McpClientTrait, McpMeta};
use crate::agents::skill_catalog;
use crate::config::paths::Paths;
use anyhow::Result;
use async_trait::async_trait;
use indoc::indoc;
use rmcp::model::{
    CallToolResult, Content, Implementation, InitializeResult, JsonObject, ListToolsResult,
    ProtocolVersion, ServerCapabilities, Tool, ToolAnnotations, ToolsCapability,
};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub static EXTENSION_NAME: &str = "skills";

/// Skills that ship with Biorouter. They are re-seeded into the user's skills
/// directory on app and session startup, so removing the folder only lasts
/// until the next startup — users disable them via the normal toggle instead.
pub static BUILTIN_SKILLS: &[(&str, &str)] = &[
    (
        "about-biorouter",
        include_str!("builtin_skills/about-biorouter/SKILL.md"),
    ),
    (
        "develop-biorouter",
        include_str!("builtin_skills/develop-biorouter/SKILL.md"),
    ),
    (
        "develop-biorouter-extension",
        include_str!("builtin_skills/develop-biorouter-extension/SKILL.md"),
    ),
    (
        "develop-biorouter-skill",
        include_str!("builtin_skills/develop-biorouter-skill/SKILL.md"),
    ),
];

/// Skills that ship with Biorouter and teach the **knowledge-base formats** —
/// which of OKF and BioOKF to create, how to ingest into each, and how to read a
/// lint report. Seeded exactly like the array above.
///
/// ⚠ **A separate array, and the separation is not cosmetic.** The list above is
/// also the *Contexts* list: the shipped skills the desktop Settings pane offers
/// as toggles, hand-synced into two TypeScript copies, and pinned by
/// `ui/desktop/src/components/settings/contexts/contexts.test.ts`, which reads
/// **this file's source text** — it slices from the identifier above to its
/// closing `];` and matches the `("name", include_str!` pairs inside. Adding a
/// knowledge skill there would make that test fail on a UI this change does not
/// touch. These four are shipped skills that are not Contexts, which is a real
/// distinction (they trigger on a task, they are not always-on self-knowledge)
/// and is why they can live here honestly rather than as a workaround.
///
/// ⚠ **`BUILTIN_SKILL_NAMES` lists these; `CONTEXTS` must not.** That asymmetry
/// is the whole point of the split, and it mirrors the two functions below:
/// `context_skill_names()` is BUILTIN_SKILLS + soul, and `is_builtin_skill_name()`
/// is over `shipped_skills()`, which includes these four.
///
/// It was not always so. These shipped without being in either TypeScript list,
/// so the Skills pane offered a Delete control on a seeded skill: the delete
/// succeeded, the toast said so, and the next startup rewrote the folder. A
/// button that reports success and silently reverts is worse than no button.
/// `contexts.test.ts` could not catch it either — it sliced only the array
/// above, so the census was blind to exactly the names it needed to see.
pub static KNOWLEDGE_SKILLS: &[(&str, &str)] = &[
    (
        "knowledge-choose-a-format",
        include_str!("builtin_skills/knowledge-choose-a-format/SKILL.md"),
    ),
    (
        "knowledge-ingest-okf",
        include_str!("builtin_skills/knowledge-ingest-okf/SKILL.md"),
    ),
    (
        "knowledge-ingest-biookf",
        include_str!("builtin_skills/knowledge-ingest-biookf/SKILL.md"),
    ),
    (
        "knowledge-lint",
        include_str!("builtin_skills/knowledge-lint/SKILL.md"),
    ),
];

/// Every skill whose bytes ship inside the binary, Contexts and otherwise.
///
/// This is what the seeder and the reset path write, and what
/// [`is_builtin_skill_name`] answers over — "did Biorouter put this here?" is a
/// different question from "does Settings offer a toggle for it?", and conflating
/// them is what would make a knowledge skill count as one the *user* installed.
pub fn shipped_skills() -> impl Iterator<Item = &'static (&'static str, &'static str)> {
    BUILTIN_SKILLS.iter().chain(KNOWLEDGE_SKILLS.iter())
}

pub(crate) fn install_builtin_skills() {
    SkillsClient::ensure_builtin_skills(&Paths::config_dir().join("skills"));
}

/// Every shipped **Context**, by the `name:` in its `SKILL.md` frontmatter —
/// which is also its directory name and the key it occupies in the skill map.
///
/// The four [`BUILTIN_SKILLS`] plus the Soul skill, which is defined as a Rust
/// string in `soul.rs` rather than in that array. Hand-synced with
/// `ui/desktop/src/components/settings/contexts/contexts.ts`, whose
/// `contexts.test.ts` reads *this file* and asserts the two agree (#77).
///
/// ⚠ Deliberately **not** [`shipped_skills`]. The two answer different questions
/// — see [`KNOWLEDGE_SKILLS`] — and widening this one would change what the
/// Settings pane claims to offer.
pub fn context_skill_names() -> impl Iterator<Item = &'static str> {
    BUILTIN_SKILLS
        .iter()
        .map(|(name, _)| *name)
        .chain(std::iter::once(crate::knowledge::soul::SOUL_SKILL_DIR))
}

/// Did Biorouter put this skill on disk? Every shipped skill, not only the
/// Contexts — a knowledge skill the seeder wrote is not one the user installed,
/// so it must not be counted as one by [`count_user_skills`].
pub fn is_builtin_skill_name(name: &str) -> bool {
    shipped_skills()
        .map(|(name, _)| *name)
        .chain(std::iter::once(crate::knowledge::soul::SOUL_SKILL_DIR))
        .any(|builtin_name| builtin_name == name)
}

/// The config key holding one Context's enablement, as the desktop Settings
/// switch writes it.
///
/// ⚠ **Must match `contextConfigKey` in
/// `ui/desktop/src/components/settings/contexts/contexts.ts` exactly** —
/// `` `context_${id.replace(/-/g, '_')}` ``. That key is the *entire* connection
/// between the switch and this side: derive it differently by one character and
/// the toggle still moves, the value is still stored, and nothing changes —
/// which is precisely the defect this pair of functions exists to close. Both
/// sides pin the same literal in a test.
pub fn context_config_key(id: &str) -> String {
    format!("context_{}", id.replace('-', "_"))
}

/// The shipped Contexts the user has switched **off** in Settings → Contexts.
///
/// ⚠ **Absence means ON.** These have loaded since before the switch existed,
/// so a missing key — every install that has never opened that screen — must
/// read as enabled. Only an explicit `false` hides one, matching the
/// `?? true` the switch renders with.
///
/// ⚠ **Deliberately NOT `skills-config.json`'s `disabled[]`.** That array is
/// honoured by [`SkillsClient::handle_load_skill`], which refuses a disabled
/// skill outright, while `prompts/system.md` unconditionally instructs the
/// model to load `about-biorouter`. Routing a Context through it would turn "I
/// don't want this in my sidebar" into "the agent reports a failed skill load
/// on every turn". Off here means **not surfaced**, never **unloadable** — the
/// set returned here filters the catalog the model is told about
/// ([`SkillsClient::enabled_skill_entries`], and through it `listSkills`,
/// `searchSkills` and the `{skill_count}` sentence) and nothing else.
fn hidden_contexts_in(config: &crate::config::Config) -> std::collections::HashSet<String> {
    context_skill_names()
        .filter(|name| {
            // `Err` is "no opinion recorded" (or an unreadable config), which
            // must read as ON — see the absence rule above. Only a literal
            // `false` hides a Context.
            matches!(
                config.get_param::<bool>(&context_config_key(name)),
                Ok(false)
            )
        })
        .map(str::to_string)
        .collect()
}

/// [`hidden_contexts_in`] against the process's real configuration.
fn hidden_contexts() -> std::collections::HashSet<String> {
    hidden_contexts_in(crate::config::Config::global())
}

// ---------------------------------------------------------------------------
// The three inputs `skill_catalog` composes. They are exported from here — the
// module that owns each one's meaning — rather than reimplemented there, so
// the interface's switches and the model's catalog can never disagree about
// what "enabled" means.
// ---------------------------------------------------------------------------

/// The machine-wide `skills-config.json` `disabled[]` set. Contains **skill
/// names and bundle names**, which is why a caller must test both.
pub(crate) fn disabled_skill_names() -> std::collections::HashSet<String> {
    SkillsClient::get_disabled_skills()
}

/// The shipped Contexts the user switched off in Settings → Contexts.
pub(crate) fn hidden_context_names() -> std::collections::HashSet<String> {
    hidden_contexts()
}

/// See [`SkillsClient::add_missing_shipped_skills`].
pub(crate) fn add_missing_shipped_skills(skills: &mut HashMap<String, Skill>) {
    SkillsClient::add_missing_shipped_skills(skills)
}

pub fn count_user_skills() -> usize {
    let skills_dir = Paths::config_dir().join("skills");
    std::fs::read_dir(skills_dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .filter(|entry| !is_builtin_skill_name(&entry.file_name().to_string_lossy()))
        .count()
}

pub fn reset_to_builtin_skills() -> Result<usize> {
    let config_dir = Paths::config_dir();
    let skills_dir = config_dir.join("skills");
    let removed = count_user_skills();

    if skills_dir.exists() {
        std::fs::remove_dir_all(&skills_dir)?;
    }
    let skills_config = config_dir.join("skills-config.json");
    if skills_config.exists() {
        std::fs::remove_file(skills_config)?;
    }

    SkillsClient::ensure_builtin_skills(&skills_dir);
    let soul_skill_dir = skills_dir.join(crate::knowledge::soul::SOUL_SKILL_DIR);
    std::fs::create_dir_all(&soul_skill_dir)?;
    std::fs::write(
        soul_skill_dir.join("SKILL.md"),
        crate::knowledge::soul::SOUL_SKILL_MD,
    )?;

    for (name, _) in shipped_skills() {
        if !skills_dir.join(name).join("SKILL.md").is_file() {
            anyhow::bail!("failed to restore built-in skill '{name}'");
        }
    }
    Ok(removed)
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct LoadSkillParams {
    name: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct ListSkillsParams {
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct SearchSkillsParams {
    query: String,
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
}

/// A discovered skill. Pub (with pub fields) because the CLI's
/// `biorouter skill` commands reuse this exact discovery representation —
/// same roots, same frontmatter semantics — instead of a parallel scanner.
#[derive(Debug, Clone)]
pub struct Skill {
    pub metadata: SkillMetadata,
    pub body: String,
    pub directory: PathBuf,
    pub supporting_files: Vec<PathBuf>,
    pub bundle_name: Option<String>,
    /// The skills root directory this skill was discovered under (one of
    /// [`SkillsClient::get_default_skill_directories`]). Lets callers show
    /// where a skill comes from and derive its root-relative slug.
    pub source_root: PathBuf,
}

#[derive(Debug, Serialize)]
struct SkillCatalogItem {
    name: String,
    description: String,
    bundle: Option<String>,
}

/// Where a `SkillsClient` reads its skills from.
///
/// ⚠ **Production is always [`SkillIndex::Live`]**, and that is the whole point
/// of #113's root cause 3: the client used to hold a `HashMap` discovered in
/// its constructor, so a skill installed *afterwards* was not in it and no
/// amount of toggling could make the skill loadable in that conversation.
/// Reading the process-global catalog on every access means an install reaches
/// every live conversation at once.
///
/// [`SkillIndex::Pinned`] exists for tests, which need a fixed set that does
/// not depend on the developer's own `~/.config/biorouter/skills` — and cannot
/// get one from the global catalog without relocating `BIOROUTER_PATH_ROOT`,
/// a process-wide env var several of these tests would race each other on.
pub(crate) enum SkillIndex {
    Live,
    Pinned(Arc<HashMap<String, Skill>>),
}

impl SkillIndex {
    fn skills(&self) -> Arc<HashMap<String, Skill>> {
        match self {
            SkillIndex::Live => skill_catalog::current().skills(),
            SkillIndex::Pinned(skills) => Arc::clone(skills),
        }
    }

    /// Mutable access to a pinned set. Test-only, and it panics on `Live`
    /// rather than silently editing a copy nobody reads.
    #[cfg(test)]
    fn pinned_mut(&mut self) -> &mut HashMap<String, Skill> {
        match self {
            SkillIndex::Pinned(skills) => Arc::make_mut(skills),
            SkillIndex::Live => panic!("a live index is the catalog's; pin it first"),
        }
    }
}

impl From<HashMap<String, Skill>> for SkillIndex {
    fn from(skills: HashMap<String, Skill>) -> Self {
        SkillIndex::Pinned(Arc::new(skills))
    }
}

pub struct SkillsClient {
    info: InitializeResult,
    skills: SkillIndex,
    /// Retained so the session-scoped skill override can be read from the
    /// session row — the same reason `ChatRecallClient` keeps its context.
    ///
    /// BR-71: this client deliberately holds **no per-session state**. One
    /// `SkillsClient` can serve many sessions concurrently — the ACP server
    /// shares a single `Agent` (hence a single `ExtensionManager`, hence a
    /// single client) across every session and spawns prompts in parallel — so
    /// a "currently bound session" field would let one session's `loadSkill`
    /// resolve against another's grant across an await. The session id lives
    /// where it belongs: in the `McpMeta` of the dispatch that carries it.
    context: PlatformExtensionContext,
}

const DEFAULT_SKILL_PAGE_LIMIT: usize = 20;
const MAX_SKILL_PAGE_LIMIT: usize = 50;

impl SkillsClient {
    pub fn new(context: PlatformExtensionContext) -> Result<Self> {
        let info = InitializeResult {
            protocol_version: ProtocolVersion::V_2025_03_26,
            capabilities: ServerCapabilities {
                tasks: None,
                tools: Some(ToolsCapability {
                    list_changed: Some(false),
                }),
                resources: None,
                prompts: None,
                completions: None,
                experimental: None,
                logging: None,
            },
            server_info: Implementation {
                name: EXTENSION_NAME.to_string(),
                title: Some("Skills".to_string()),
                version: "1.0.0".to_string(),
                icons: None,
                website_url: None,
            },
            instructions: Some(String::new()),
        };

        install_builtin_skills();

        // The catalog is refreshed here rather than merely read, because a
        // client is constructed when a conversation starts and the seeding
        // above may just have written the shipped skills to disk.
        let catalog = skill_catalog::refresh();

        let mut client = Self {
            info,
            skills: SkillIndex::Live,
            context,
        };
        client.info.instructions = Some(Self::generate_instructions(&catalog.skills()));
        Ok(client)
    }

    /// Guarantee the shipped skills are present even if seeding to disk failed
    /// (a read-only config dir) or a user skill shadowed the slug.
    ///
    /// Lives here, beside the `include_str!` arrays it draws on, and is applied
    /// by [`skill_catalog::SkillCatalog::scan`] so that *every* view of the
    /// catalog has them — not only the one a client happened to build.
    pub(crate) fn add_missing_shipped_skills(skills: &mut HashMap<String, Skill>) {
        for (name, content) in shipped_skills() {
            if skills.contains_key(*name) {
                continue;
            }
            if let Ok((metadata, body)) = Self::parse_frontmatter(content) {
                skills.insert(
                    metadata.name.clone(),
                    Skill {
                        metadata,
                        body,
                        directory: Paths::config_dir().join("skills").join(name),
                        supporting_files: Vec::new(),
                        bundle_name: None,
                        source_root: Paths::config_dir().join("skills"),
                    },
                );
            }
        }
    }

    /// Seed (or refresh) the built-in skills under the user's skills directory
    /// so they show up in the Skills UI and survive deletion. Content is
    /// rewritten when it differs so app updates propagate. Failures are
    /// non-fatal: the in-memory fallback in `new()` still registers them.
    fn ensure_builtin_skills(skills_dir: &Path) {
        for (name, content) in shipped_skills() {
            let dir = skills_dir.join(name);
            let file = dir.join("SKILL.md");
            let up_to_date = std::fs::read_to_string(&file)
                .map(|existing| existing == *content)
                .unwrap_or(false);
            if up_to_date {
                continue;
            }
            if let Err(e) =
                std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&file, content))
            {
                tracing::warn!("failed to seed builtin skill '{}': {}", name, e);
            }
        }
    }

    /// Every directory skills are discovered under, in override order (later
    /// wins): `~/.claude/skills`, `~/.config/agents/skills`, the Biorouter
    /// config skills dir, installed extensions' `skills/` subdirs, and the
    /// working directory's `.claude/skills`, `.biorouter/skills`,
    /// `.agents/skills`. Pub so the CLI scans the exact same roots.
    ///
    /// ⚠ **The list itself lives in [`skill_catalog::roots`]**, which also
    /// records where each root came from. This function is the paths-only view
    /// of it, kept because the CLI and several tests name it. Adding a root
    /// here instead of there would give the interface a catalog that omits it —
    /// which is precisely root cause 2 of #113.
    pub fn get_default_skill_directories() -> Vec<PathBuf> {
        skill_catalog::roots()
            .into_iter()
            .map(|root| root.path)
            .collect()
    }

    pub(crate) fn get_disabled_skills() -> std::collections::HashSet<String> {
        let config_file = Paths::config_dir().join("skills-config.json");
        let Ok(content) = std::fs::read_to_string(&config_file) else {
            return std::collections::HashSet::new();
        };
        let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) else {
            return std::collections::HashSet::new();
        };
        config
            .get("disabled")
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn parse_skill_file(
        path: &Path,
        bundle_name: Option<String>,
        source_root: &Path,
    ) -> Result<Skill> {
        let content = std::fs::read_to_string(path)?;

        let (metadata, body) = Self::parse_frontmatter(&content)?;

        let directory = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Skill file has no parent directory"))?
            .to_path_buf();

        let supporting_files = Self::find_supporting_files(&directory, path)?;

        Ok(Skill {
            metadata,
            body,
            directory,
            supporting_files,
            bundle_name,
            source_root: source_root.to_path_buf(),
        })
    }

    /// Parse a SKILL.md: YAML frontmatter (with a line-based fallback for
    /// technically-invalid-but-common YAML like unquoted colons in the
    /// description) plus the markdown body. Pub so the CLI applies the exact
    /// same frontmatter semantics as the backend.
    pub fn parse_frontmatter(content: &str) -> Result<(SkillMetadata, String)> {
        let parts: Vec<&str> = content.split("---").collect();

        if parts.len() < 3 {
            return Err(anyhow::anyhow!("Invalid frontmatter format"));
        }

        let yaml_content = parts[1].trim();
        let metadata: SkillMetadata = serde_yaml::from_str(yaml_content)
            .or_else(|_| Self::parse_frontmatter_metadata_fallback(yaml_content))?;

        let body = parts[2..].join("---").trim().to_string();

        Ok((metadata, body))
    }

    fn parse_frontmatter_metadata_fallback(yaml_content: &str) -> Result<SkillMetadata> {
        let mut name = None;
        let mut description = None;

        for line in yaml_content.lines() {
            let trimmed = line.trim();
            if let Some(value) = trimmed.strip_prefix("name:") {
                name = Some(value.trim().trim_matches(['"', '\'']).to_string());
            } else if let Some(value) = trimmed.strip_prefix("description:") {
                description = Some(value.trim().trim_matches(['"', '\'']).to_string());
            }
        }

        match (name, description) {
            (Some(name), Some(description)) if !name.is_empty() => {
                Ok(SkillMetadata { name, description })
            }
            _ => Err(anyhow::anyhow!("Invalid frontmatter format")),
        }
    }

    fn find_supporting_files(directory: &Path, skill_file: &Path) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        if let Ok(entries) = std::fs::read_dir(directory) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path != skill_file {
                    files.push(path);
                } else if path.is_dir() {
                    if let Ok(sub_entries) = std::fs::read_dir(&path) {
                        for sub_entry in sub_entries.flatten() {
                            let sub_path = sub_entry.path();
                            if sub_path.is_file() {
                                files.push(sub_path);
                            }
                        }
                    }
                }
            }
        }

        Ok(files)
    }

    /// Two-level discovery over the given roots: `<slug>/SKILL.md` (single
    /// skill) or `<bundle>/<slug>/SKILL.md` (bundle sub-skill), keyed by
    /// frontmatter name — a later root's skill overrides an earlier one's.
    /// Files whose frontmatter fails to parse are skipped (never loaded).
    /// Pub so the CLI lists exactly what this extension will load.
    pub fn discover_skills_in_directories(directories: &[PathBuf]) -> HashMap<String, Skill> {
        let mut skills = HashMap::new();

        for dir in directories {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_dir() {
                        continue;
                    }

                    let skill_file = path.join("SKILL.md");
                    if skill_file.exists() {
                        // Single skill
                        if let Ok(skill) = Self::parse_skill_file(&skill_file, None, dir) {
                            skills.insert(skill.metadata.name.clone(), skill);
                        }
                    } else {
                        // Bundle: check if sub-directories contain SKILL.md
                        let bundle_name = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .map(str::to_string);

                        if let (Some(bundle_name), Ok(sub_entries)) =
                            (bundle_name, std::fs::read_dir(&path))
                        {
                            for sub_entry in sub_entries.flatten() {
                                let sub_path = sub_entry.path();
                                if !sub_path.is_dir() {
                                    continue;
                                }
                                let sub_skill_file = sub_path.join("SKILL.md");
                                if sub_skill_file.exists() {
                                    if let Ok(skill) = Self::parse_skill_file(
                                        &sub_skill_file,
                                        Some(bundle_name.clone()),
                                        dir,
                                    ) {
                                        skills.insert(skill.metadata.name.clone(), skill);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        skills
    }

    /// The extension's system-prompt sentence.
    ///
    /// **This is the machine-wide view, permanently** — it is generated once in
    /// [`Self::new`], `McpClientTrait::get_info` hands back that snapshot, and
    /// `ExtensionManager` clones it when the client is registered. There is no
    /// session id anywhere on that path (`get_info` and `list_tools` take
    /// none), and there is no per-turn refresh, so a session grant made by
    /// `workspace_set_tools` never moves this count — not on the next turn, not
    /// ever, for the life of the process.
    ///
    /// That is the whole residual, and it is confined to the *count in one
    /// sentence*. Every question actually answered about skills —
    /// `listSkills`, `searchSkills`, `loadSkill` — goes through `call_tool`,
    /// which carries `McpMeta`, so a granted skill is listable and loadable
    /// **immediately**, and a revoked one is refused immediately. Making the
    /// sentence session-aware would mean giving `get_info` a session id across
    /// every extension, and the shared-client hazard documented on
    /// [`SkillsClient`] rules out the cheap alternative of mutating this client
    /// per session.
    fn generate_instructions(skills: &HashMap<String, Skill>) -> String {
        if skills.is_empty() {
            return String::new();
        }

        let skill_count = Self::enabled_skill_entries(
            skills,
            &crate::agents::session_skills::SessionSkillOverride::default(),
        )
        .len();

        if skill_count == 0 {
            return String::new();
        }

        format!(
            "You have {skill_count} skills available through the skills extension. Use searchSkills to find relevant skills, listSkills to page through the catalog, and loadSkill to load an exact skill by name before relying on it. For Biorouter questions, load about-biorouter directly."
        )
    }

    /// The composed disabled test for the session this client serves: the
    /// machine-wide file (`skills-config.json`, which contains skill names AND
    /// bundle names) composed with the session override (`workspace_skills`).
    /// Never writes anything.
    ///
    /// ⚠ **Delegates to [`skill_catalog::compose_state`]** rather than
    /// restating the precedence. This function used to hold its own copy, and
    /// that copy knew only about skill names — so a per-chat toggle of a
    /// *bundle* wrote a name this test never matched, and changed nothing.
    /// One rule, two readers.
    ///
    /// `hidden_contexts` is passed empty here because
    /// [`Self::enabled_skill_entries`] applies that filter itself, ahead of the
    /// session test, for the reason recorded there.
    fn is_skill_enabled_for_session(
        name: &str,
        skill: &Skill,
        machine_disabled: &std::collections::HashSet<String>,
        over: &crate::agents::session_skills::SessionSkillOverride,
    ) -> bool {
        skill_catalog::compose_state(
            name,
            skill.bundle_name.as_deref(),
            machine_disabled,
            &std::collections::HashSet::new(),
            over,
        )
        .effective
    }

    /// The catalog as one session sees it. `over` is always the caller's — read
    /// from the `McpMeta` of the dispatch in flight, never from client state
    /// (see [`SkillsClient`]). The machine-wide view is
    /// `&SessionSkillOverride::default()`.
    ///
    /// This is **the one surface a Context toggle acts on**: everything the
    /// model is *told about* — `listSkills`, `searchSkills`, and the
    /// `{skill_count}` sentence in [`Self::generate_instructions`] — comes
    /// through here, while [`Self::handle_load_skill`] deliberately does not, so
    /// a switched-off Context stays loadable by exact name (see
    /// [`hidden_contexts_in`] for why that asymmetry is required rather than
    /// merely tolerated).
    fn enabled_skill_entries<'a>(
        skills: &'a HashMap<String, Skill>,
        over: &crate::agents::session_skills::SessionSkillOverride,
    ) -> Vec<(&'a String, &'a Skill)> {
        let disabled = Self::get_disabled_skills();
        let hidden_contexts = hidden_contexts();
        let mut skill_list: Vec<_> = skills
            .iter()
            .filter(|(name, skill)| {
                // ⚠ Checked BEFORE the session test, not folded into it. That
                // test's first rule is "an explicit session grant wins over
                // everything", and a Context the user switched off in Settings
                // is not something `workspace_set_tools` should be able to put
                // back into the catalog on the model's say-so.
                !hidden_contexts.contains(name.as_str())
                    && Self::is_skill_enabled_for_session(name, skill, &disabled, over)
            })
            .collect();
        skill_list.sort_by_key(|(name, _)| *name);
        skill_list
    }

    /// The enabled catalog's names, for tests that assert on membership.
    ///
    /// It reads through [`SkillIndex`] exactly as the tool handlers do, so a
    /// test cannot accidentally assert against a map the handlers do not use.
    #[cfg(test)]
    fn enabled_names(
        &self,
        over: &crate::agents::session_skills::SessionSkillOverride,
    ) -> Vec<String> {
        let skills = self.skills.skills();
        Self::enabled_skill_entries(&skills, over)
            .into_iter()
            .map(|(name, _)| name.clone())
            .collect()
    }

    fn parse_pagination(offset: Option<usize>, limit: Option<usize>) -> (usize, usize) {
        let offset = offset.unwrap_or(0);
        let limit = limit
            .unwrap_or(DEFAULT_SKILL_PAGE_LIMIT)
            .clamp(1, MAX_SKILL_PAGE_LIMIT);
        (offset, limit)
    }

    fn normalize_search_text(value: &str) -> String {
        value
            .chars()
            .map(|c| {
                if c.is_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    ' '
                }
            })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn search_score(skill: &Skill, query: &str, terms: &[&str]) -> usize {
        let name = Self::normalize_search_text(&skill.metadata.name);
        let description = Self::normalize_search_text(&skill.metadata.description);
        let bundle = Self::normalize_search_text(skill.bundle_name.as_deref().unwrap_or_default());

        if name == query {
            100
        } else if name.contains(query) {
            90
        } else if terms.iter().all(|term| name.contains(term)) {
            80
        } else if description.contains(query) {
            70
        } else if terms.iter().all(|term| description.contains(term)) {
            60
        } else if bundle.contains(query) {
            50
        } else if terms.iter().all(|term| bundle.contains(term)) {
            40
        } else {
            0
        }
    }

    fn catalog_item(skill: &Skill) -> SkillCatalogItem {
        SkillCatalogItem {
            name: skill.metadata.name.clone(),
            description: skill.metadata.description.clone(),
            bundle: skill.bundle_name.clone(),
        }
    }

    fn catalog_response(
        total: usize,
        offset: usize,
        limit: usize,
        skills: Vec<SkillCatalogItem>,
    ) -> Result<Vec<Content>, String> {
        let returned = skills.len();
        let next_offset = if offset + returned < total {
            Some(offset + returned)
        } else {
            None
        };

        let response = serde_json::json!({
            "total": total,
            "offset": offset,
            "limit": limit,
            "returned": returned,
            "next_offset": next_offset,
            "skills": skills,
        });
        serde_json::to_string_pretty(&response)
            .map(|text| vec![Content::text(text)])
            .map_err(|error| error.to_string())
    }

    fn parse_tool_args<T>(arguments: Option<JsonObject>) -> Result<T, String>
    where
        T: for<'de> Deserialize<'de>,
    {
        let value = serde_json::Value::Object(arguments.unwrap_or_default());
        serde_json::from_value(value).map_err(|error| error.to_string())
    }

    async fn handle_list_skills(
        &self,
        arguments: Option<JsonObject>,
        over: &crate::agents::session_skills::SessionSkillOverride,
    ) -> Result<Vec<Content>, String> {
        let params: ListSkillsParams = Self::parse_tool_args(arguments)?;
        let (offset, limit) = Self::parse_pagination(params.offset, params.limit);
        let skills = self.skills.skills();
        let skill_list = Self::enabled_skill_entries(&skills, over);
        let total = skill_list.len();
        let skills = skill_list
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|(_, skill)| Self::catalog_item(skill))
            .collect();

        Self::catalog_response(total, offset, limit, skills)
    }

    async fn handle_search_skills(
        &self,
        arguments: Option<JsonObject>,
        over: &crate::agents::session_skills::SessionSkillOverride,
    ) -> Result<Vec<Content>, String> {
        let params: SearchSkillsParams = Self::parse_tool_args(arguments)?;
        let query = Self::normalize_search_text(params.query.trim());
        if query.is_empty() {
            return Err("Missing required parameter: query".to_string());
        }

        let terms: Vec<&str> = query.split_whitespace().collect();
        let (offset, limit) = Self::parse_pagination(params.offset, params.limit);
        let skills = self.skills.skills();
        let mut matches: Vec<_> = Self::enabled_skill_entries(&skills, over)
            .into_iter()
            .filter(|(_, skill)| {
                let haystack = format!(
                    "{} {} {}",
                    Self::normalize_search_text(&skill.metadata.name),
                    Self::normalize_search_text(&skill.metadata.description),
                    Self::normalize_search_text(skill.bundle_name.as_deref().unwrap_or_default())
                );
                terms.iter().all(|term| haystack.contains(term))
            })
            .collect();

        matches.sort_by(|(left_name, left_skill), (right_name, right_skill)| {
            let left_score = Self::search_score(left_skill, &query, &terms);
            let right_score = Self::search_score(right_skill, &query, &terms);
            right_score
                .cmp(&left_score)
                .then_with(|| left_skill.metadata.name.cmp(&right_skill.metadata.name))
                .then_with(|| left_name.cmp(right_name))
        });

        let total = matches.len();
        let skills = matches
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|(_, skill)| Self::catalog_item(skill))
            .collect();

        Self::catalog_response(total, offset, limit, skills)
    }

    async fn handle_load_skill(
        &self,
        arguments: Option<JsonObject>,
        over: &crate::agents::session_skills::SessionSkillOverride,
    ) -> Result<Vec<Content>, String> {
        let skill_name = arguments
            .as_ref()
            .ok_or("Missing arguments")?
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: name")?;

        // Runtime check: reject disabled skills even mid-session. `skills` is
        // read from the live catalog here, so a package installed earlier in
        // THIS conversation is loadable in it (#113 root cause 3).
        let skills = self.skills.skills();
        let disabled = Self::get_disabled_skills();
        if let Some(skill) = skills.get(skill_name) {
            if !Self::is_skill_enabled_for_session(skill_name, skill, &disabled, over) {
                return Err(format!(
                    "Skill '{}' is currently disabled. Enable it in Biorouter's Skills settings to use it.",
                    skill_name
                ));
            }
        }

        let skill = skills
            .get(skill_name)
            .ok_or_else(|| format!("Skill '{}' not found", skill_name))?;

        let mut response = format!("# Skill: {}\n\n{}\n\n", skill.metadata.name, skill.body);

        if !skill.supporting_files.is_empty() {
            response.push_str(&format!(
                "## Supporting Files\n\nSkill directory: {}\n\n",
                skill.directory.display()
            ));
            response.push_str("The following supporting files are available:\n");
            for file in &skill.supporting_files {
                if let Ok(relative) = file.strip_prefix(&skill.directory) {
                    response.push_str(&format!("- {}\n", relative.display()));
                }
            }
            response.push_str("\nUse the view file tools to access these files as needed, or run scripts as directed with dev extension.\n");
        }

        Ok(vec![Content::text(response)])
    }

    fn get_tools() -> Vec<Tool> {
        fn input_schema<T: JsonSchema>() -> JsonObject {
            let schema = schema_for!(T);
            let schema_value =
                serde_json::to_value(schema).expect("Failed to serialize tool schema");

            schema_value
                .as_object()
                .expect("Schema should be an object")
                .clone()
        }

        vec![
            Tool::new(
                "searchSkills".to_string(),
                indoc! {r#"
                    Search installed skills by name, description, or bundle.

                    Use this before loadSkill when you need to find the exact skill name.
                    Results are paginated and include skill names and descriptions only.
                "#}
                .to_string(),
                input_schema::<SearchSkillsParams>(),
            )
            .annotate(ToolAnnotations {
                title: Some("Search skills".to_string()),
                read_only_hint: Some(true),
                destructive_hint: Some(false),
                idempotent_hint: Some(true),
                open_world_hint: Some(false),
            }),
            Tool::new(
                "listSkills".to_string(),
                indoc! {r#"
                    List installed skills in alphabetical pages.

                    Use this for browsing the skill catalog when a search query is not obvious.
                "#}
                .to_string(),
                input_schema::<ListSkillsParams>(),
            )
            .annotate(ToolAnnotations {
                title: Some("List skills".to_string()),
                read_only_hint: Some(true),
                destructive_hint: Some(false),
                idempotent_hint: Some(true),
                open_world_hint: Some(false),
            }),
            Tool::new(
                "loadSkill".to_string(),
                indoc! {r#"
                    Load a skill by exact name and return its content.

                    This tool loads the specified skill and returns its body content along with
                    information about any supporting files in the skill directory.
                "#}
                .to_string(),
                input_schema::<LoadSkillParams>(),
            )
            .annotate(ToolAnnotations {
                title: Some("Load skill".to_string()),
                read_only_hint: Some(true),
                destructive_hint: Some(false),
                idempotent_hint: Some(true),
                open_world_hint: Some(false),
            }),
        ]
    }
}

#[async_trait]
impl McpClientTrait for SkillsClient {
    async fn list_tools(
        &self,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        let disabled = Self::get_disabled_skills();
        let skills = self.skills.skills();
        // Machine-wide, because `list_tools` carries no session id — the same
        // documented residual as `generate_instructions`. Still routed through
        // the one composer, so "enabled" cannot mean something different here
        // than it does two functions away.
        let machine_wide = crate::agents::session_skills::SessionSkillOverride::default();
        let has_enabled_skills = skills.iter().any(|(name, skill)| {
            skill_catalog::compose_state(
                name,
                skill.bundle_name.as_deref(),
                &disabled,
                &std::collections::HashSet::new(),
                &machine_wide,
            )
            .effective
        });
        let tools = if has_enabled_skills {
            Self::get_tools()
        } else {
            Vec::new()
        };
        Ok(ListToolsResult {
            tools,
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        name: &str,
        arguments: Option<JsonObject>,
        meta: McpMeta,
        _cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        // BR-71: the session's override is read ONCE per dispatch, from the
        // session id this call carries, and then passed down. It is never
        // stashed on `self` — one client serves many sessions concurrently.
        //
        // Fail CLOSED. If the override cannot be read, answering from the
        // machine-wide view would hand back every skill this conversation had
        // revoked, so the call is refused instead.
        let over = match crate::agents::session_skills::for_session(
            &self.context.session_manager,
            &meta.session_id,
        )
        .await
        {
            Ok(over) => over,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Error: could not determine which skills are enabled for this conversation: {e:#}"
                ))]));
            }
        };

        let content = match name {
            "searchSkills" => self.handle_search_skills(arguments, &over).await,
            "listSkills" => self.handle_list_skills(arguments, &over).await,
            "loadSkill" => self.handle_load_skill(arguments, &over).await,
            _ => Err(format!("Unknown tool: {}", name)),
        };

        match content {
            Ok(content) => Ok(CallToolResult::success(content)),
            Err(error) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Error: {}",
                error
            ))])),
        }
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SessionManager;
    use std::fs;
    use std::sync::Arc;
    use tempfile::TempDir;

    /// A context for the literal `SkillsClient { … }` constructions below.
    ///
    /// ⚠ **One database per call, and that is load-bearing.** The comment here
    /// used to say these tests never dispatch a tool call, so the manager was
    /// never queried — and it was wrong: nine of them go through `call_tool`,
    /// whose first act is to read the session's skill override out of SQLite.
    /// Pointed at one fixed path, those nine concurrent tests opened one
    /// database through nine separate pools, each running its own
    /// initialization. When that collides `call_tool` fails CLOSED and returns
    /// a plain-text `Error: could not determine which skills are enabled…`,
    /// which the callers parse as JSON — so the race surfaces as
    /// `expected value at line 1 column 1`, naming neither SQLite nor the
    /// sharing. It is a race everywhere and was seen on macOS too; Windows'
    /// stricter file locking simply loses it far more often.
    fn test_context() -> PlatformExtensionContext {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let unique = format!(
            "biorouter-skills-test-sessions-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        PlatformExtensionContext {
            extension_manager: None,
            session_manager: Arc::new(SessionManager::new(std::env::temp_dir().join(unique))),
        }
    }

    #[test]
    fn test_parse_frontmatter() {
        let content = r#"---
name: test-skill
description: A test skill
---

# Test Skill

This is the body of the skill.
"#;

        let (metadata, body) = SkillsClient::parse_frontmatter(content).unwrap();
        assert_eq!(metadata.name, "test-skill");
        assert_eq!(metadata.description, "A test skill");
        assert!(body.contains("# Test Skill"));
        assert!(body.contains("This is the body of the skill."));
    }

    #[test]
    fn test_parse_frontmatter_missing() {
        let content = "# No frontmatter here";
        assert!(SkillsClient::parse_frontmatter(content).is_err());
    }

    #[test]
    fn test_parse_frontmatter_unclosed() {
        let content = r#"---
name: test
description: test
"#;
        assert!(SkillsClient::parse_frontmatter(content).is_err());
    }

    #[test]
    fn test_parse_frontmatter_with_extra_fields() {
        let content = r#"---
name: test-skill
description: A test skill
author: Test Author
version: 1.0.0
tags:
  - test
  - example
extra_field: some value
---

# Test Skill

This is the body of the skill.
"#;

        let (metadata, body) = SkillsClient::parse_frontmatter(content).unwrap();
        assert_eq!(metadata.name, "test-skill");
        assert_eq!(metadata.description, "A test skill");
        assert!(body.contains("# Test Skill"));
        assert!(body.contains("This is the body of the skill."));
    }

    #[test]
    fn test_parse_frontmatter_falls_back_for_unquoted_colon_description() {
        let content = r#"---
name: systematic-review-prisma
description: Run systematic-review workflows: protocol/PICO framing, search strings, screening logs, PRISMA flow, extraction tables, risk-of-bias checks, and evidence synthesis with citation verification.
user-invocable: true
---

# Systematic Review and PRISMA
"#;

        let (metadata, body) = SkillsClient::parse_frontmatter(content).unwrap();
        assert_eq!(metadata.name, "systematic-review-prisma");
        assert!(metadata
            .description
            .starts_with("Run systematic-review workflows: protocol/PICO framing"));
        assert!(body.contains("# Systematic Review and PRISMA"));
    }

    #[test]
    fn test_parse_skill_file() {
        let temp_dir = TempDir::new().unwrap();
        let skill_dir = temp_dir.path().join("test-skill");
        fs::create_dir(&skill_dir).unwrap();

        let skill_file = skill_dir.join("SKILL.md");
        fs::write(
            &skill_file,
            r#"---
name: test-skill
description: A test skill
---

# Test Skill Content
"#,
        )
        .unwrap();

        fs::write(skill_dir.join("helper.py"), "print('hello')").unwrap();
        fs::create_dir(skill_dir.join("templates")).unwrap();
        fs::write(skill_dir.join("templates/template.txt"), "template").unwrap();

        let skill = SkillsClient::parse_skill_file(&skill_file, None, temp_dir.path()).unwrap();
        assert_eq!(skill.metadata.name, "test-skill");
        assert_eq!(skill.metadata.description, "A test skill");
        assert!(skill.body.contains("# Test Skill Content"));
        assert_eq!(skill.supporting_files.len(), 2);
        assert_eq!(skill.source_root, temp_dir.path());
    }

    #[test]
    fn test_discover_skills() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        fs::create_dir(&skills_dir).unwrap();

        let skill1_dir = skills_dir.join("test-skill-one-a1b2c3");
        fs::create_dir(&skill1_dir).unwrap();
        fs::write(
            skill1_dir.join("SKILL.md"),
            r#"---
name: test-skill-one-a1b2c3
description: First test skill
---
Body 1
"#,
        )
        .unwrap();

        let skill2_dir = skills_dir.join("test-skill-two-d4e5f6");
        fs::create_dir(&skill2_dir).unwrap();
        fs::write(
            skill2_dir.join("SKILL.md"),
            r#"---
name: test-skill-two-d4e5f6
description: Second test skill
---
Body 2
"#,
        )
        .unwrap();

        let skill3_dir = skills_dir.join("test-skill-three-g7h8i9");
        fs::create_dir(&skill3_dir).unwrap();
        fs::write(
            skill3_dir.join("SKILL.md"),
            r#"---
name: test-skill-three-g7h8i9
description: Third test skill
---
Body 3
"#,
        )
        .unwrap();

        let skills = SkillsClient::discover_skills_in_directories(&[skills_dir]);

        assert_eq!(skills.len(), 3);
        assert!(skills.contains_key("test-skill-one-a1b2c3"));
        assert!(skills.contains_key("test-skill-two-d4e5f6"));
        assert!(skills.contains_key("test-skill-three-g7h8i9"));
    }

    #[test]
    fn test_discover_skills_from_multiple_directories() {
        let temp_dir = TempDir::new().unwrap();

        let dir1 = temp_dir.path().join("dir1");
        fs::create_dir(&dir1).unwrap();
        let skill1_dir = dir1.join("skill-from-dir1");
        fs::create_dir(&skill1_dir).unwrap();
        fs::write(
            skill1_dir.join("SKILL.md"),
            r#"---
name: skill-from-dir1
description: Skill from directory 1
---
Content from dir1
"#,
        )
        .unwrap();

        let dir2 = temp_dir.path().join("dir2");
        fs::create_dir(&dir2).unwrap();
        let skill2_dir = dir2.join("skill-from-dir2");
        fs::create_dir(&skill2_dir).unwrap();
        fs::write(
            skill2_dir.join("SKILL.md"),
            r#"---
name: skill-from-dir2
description: Skill from directory 2
---
Content from dir2
"#,
        )
        .unwrap();

        let dir3 = temp_dir.path().join("dir3");
        fs::create_dir(&dir3).unwrap();
        let skill3_dir = dir3.join("skill-from-dir3");
        fs::create_dir(&skill3_dir).unwrap();
        fs::write(
            skill3_dir.join("SKILL.md"),
            r#"---
name: skill-from-dir3
description: Skill from directory 3
---
Content from dir3
"#,
        )
        .unwrap();

        let skills = SkillsClient::discover_skills_in_directories(&[dir1, dir2, dir3]);

        assert_eq!(skills.len(), 3);
        assert!(skills.contains_key("skill-from-dir1"));
        assert!(skills.contains_key("skill-from-dir2"));
        assert!(skills.contains_key("skill-from-dir3"));

        assert_eq!(
            skills.get("skill-from-dir1").unwrap().metadata.description,
            "Skill from directory 1"
        );
        assert_eq!(
            skills.get("skill-from-dir2").unwrap().metadata.description,
            "Skill from directory 2"
        );
        assert_eq!(
            skills.get("skill-from-dir3").unwrap().metadata.description,
            "Skill from directory 3"
        );
    }

    // BR-71: `test_context()` builds a `SessionManager`, whose lazy sqlx pool
    // must be constructed inside a Tokio runtime, so this is now an async test.
    #[tokio::test]
    async fn test_empty_instructions_when_no_skills() {
        let temp_dir = TempDir::new().unwrap();
        let empty_dir = temp_dir.path().join("empty");
        fs::create_dir(&empty_dir).unwrap();

        let skills = SkillsClient::discover_skills_in_directories(&[empty_dir]);
        assert_eq!(skills.len(), 0);

        let mut client = SkillsClient {
            info: InitializeResult {
                protocol_version: ProtocolVersion::V_2025_03_26,
                capabilities: ServerCapabilities {
                    tasks: None,
                    tools: Some(ToolsCapability {
                        list_changed: Some(false),
                    }),
                    resources: None,
                    prompts: None,
                    completions: None,
                    experimental: None,
                    logging: None,
                },
                server_info: Implementation {
                    name: EXTENSION_NAME.to_string(),
                    title: Some("Skills".to_string()),
                    version: "1.0.0".to_string(),
                    icons: None,
                    website_url: None,
                },
                instructions: Some(String::new()),
            },
            skills: skills.into(),
            context: test_context(),
        };

        let instructions = SkillsClient::generate_instructions(&client.skills.skills());
        assert_eq!(instructions, "");
        assert!(instructions.is_empty());

        client.info.instructions = Some(instructions);
        assert_eq!(client.info.instructions.as_ref().unwrap(), "");
    }

    #[tokio::test]
    async fn test_no_tools_when_no_skills() {
        let temp_dir = TempDir::new().unwrap();
        let empty_dir = temp_dir.path().join("empty");
        fs::create_dir(&empty_dir).unwrap();

        let skills = SkillsClient::discover_skills_in_directories(&[empty_dir]);
        assert_eq!(skills.len(), 0);

        let client = SkillsClient {
            info: InitializeResult {
                protocol_version: ProtocolVersion::V_2025_03_26,
                capabilities: ServerCapabilities {
                    tasks: None,
                    tools: Some(ToolsCapability {
                        list_changed: Some(false),
                    }),
                    resources: None,
                    prompts: None,
                    completions: None,
                    experimental: None,
                    logging: None,
                },
                server_info: Implementation {
                    name: EXTENSION_NAME.to_string(),
                    title: Some("Skills".to_string()),
                    version: "1.0.0".to_string(),
                    icons: None,
                    website_url: None,
                },
                instructions: Some(String::new()),
            },
            skills: skills.into(),
            context: test_context(),
        };

        let result = client
            .list_tools(None, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(result.tools.len(), 0);
    }

    #[tokio::test]
    async fn test_tools_available_when_skills_exist() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        fs::create_dir(&skills_dir).unwrap();

        let skill_dir = skills_dir.join("test-skill");
        fs::create_dir(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: test-skill
description: A test skill
---
Content
"#,
        )
        .unwrap();

        let skills = SkillsClient::discover_skills_in_directories(&[skills_dir]);
        assert_eq!(skills.len(), 1);

        let client = SkillsClient {
            info: InitializeResult {
                protocol_version: ProtocolVersion::V_2025_03_26,
                capabilities: ServerCapabilities {
                    tasks: None,
                    tools: Some(ToolsCapability {
                        list_changed: Some(false),
                    }),
                    resources: None,
                    prompts: None,
                    completions: None,
                    experimental: None,
                    logging: None,
                },
                server_info: Implementation {
                    name: EXTENSION_NAME.to_string(),
                    title: Some("Skills".to_string()),
                    version: "1.0.0".to_string(),
                    icons: None,
                    website_url: None,
                },
                instructions: Some(String::new()),
            },
            skills: skills.into(),
            context: test_context(),
        };

        let result = client
            .list_tools(None, CancellationToken::new())
            .await
            .unwrap();
        let tool_names: Vec<_> = result.tools.iter().map(|tool| tool.name.as_ref()).collect();
        assert_eq!(tool_names, vec!["searchSkills", "listSkills", "loadSkill"]);
    }

    /// Issue #65, the consumer seam. `handle_load_skill` resolves a skill by
    /// an EXACT map key — the frontmatter `name`, which may contain anything a
    /// YAML scalar can hold — so the whole point of the reference tag is that
    /// the extractor hands that name back byte for byte.
    ///
    /// This is the assertion that makes the escaping worth anything. A decode
    /// that happened downstream of this lookup would be worthless, and the
    /// "normalise to a comparable id" fix that resolved `/ext:` in #60 would
    /// arrive here as `single-cellQC&prep` and match nothing — which is why
    /// that fix could not transfer to skills.
    #[tokio::test]
    async fn a_reference_tag_loads_a_skill_whose_name_needs_escaping() {
        use crate::agents::resource_refs::{extract_resource_refs, ref_tag, RefKind};

        let awkward = r#"single-cell "QC" & prep <v2>"#;

        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        let skill_dir = skills_dir.join("awkward");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\nname: '{awkward}'\ndescription: An awkwardly named skill\n---\nBody of the awkward skill\n"),
        )
        .unwrap();

        let skills = SkillsClient::discover_skills_in_directories(&[skills_dir]);
        assert!(
            skills.contains_key(awkward),
            "discovery keyed it as {:?}",
            skills.keys().collect::<Vec<_>>()
        );

        let client = SkillsClient {
            info: InitializeResult {
                protocol_version: ProtocolVersion::V_2025_03_26,
                capabilities: ServerCapabilities {
                    tasks: None,
                    tools: Some(ToolsCapability {
                        list_changed: Some(false),
                    }),
                    resources: None,
                    prompts: None,
                    completions: None,
                    experimental: None,
                    logging: None,
                },
                server_info: Implementation {
                    name: EXTENSION_NAME.to_string(),
                    title: Some("Skills".to_string()),
                    version: "1.0.0".to_string(),
                    icons: None,
                    website_url: None,
                },
                instructions: Some(String::new()),
            },
            skills: skills.into(),
            context: test_context(),
        };

        // Exactly what a composer sends, put through the real extractor.
        let message = format!("please use {} on this", ref_tag(RefKind::Skill, awkward));
        let extracted = extract_resource_refs(&message).skills;
        assert_eq!(extracted, vec![awkward.to_string()]);

        // ...and then through the argument `Agent::skill_resource_context`
        // builds from it, unchanged.
        let arguments = serde_json::json!({ "name": extracted[0].clone() })
            .as_object()
            .unwrap()
            .clone();
        // BR-71 Task 11 gave `handle_load_skill` a session-scoped override.
        // This test is about the composer's ref extraction, not scoping, so it
        // passes the default — every skill enabled, which is what the bare call
        // meant before the parameter existed.
        let content = client
            .handle_load_skill(
                Some(arguments),
                &crate::agents::session_skills::SessionSkillOverride::default(),
            )
            .await
            .expect("the selected skill must load");

        let text = content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect::<String>();
        assert!(text.contains(awkward), "loaded the wrong skill: {text}");
        assert!(text.contains("Body of the awkward skill"), "{text}");
    }

    #[tokio::test]
    async fn test_list_skills_is_paginated() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        fs::create_dir(&skills_dir).unwrap();

        for (name, description) in [
            ("alpha-skill", "Alpha skill"),
            ("beta-skill", "Beta skill"),
            ("gamma-skill", "Gamma skill"),
        ] {
            let skill_dir = skills_dir.join(name);
            fs::create_dir(&skill_dir).unwrap();
            fs::write(
                skill_dir.join("SKILL.md"),
                format!("---\nname: {name}\ndescription: {description}\n---\nBody"),
            )
            .unwrap();
        }

        let skills = SkillsClient::discover_skills_in_directories(&[skills_dir]);
        let client = SkillsClient {
            info: InitializeResult {
                protocol_version: ProtocolVersion::V_2025_03_26,
                capabilities: ServerCapabilities {
                    tasks: None,
                    tools: Some(ToolsCapability {
                        list_changed: Some(false),
                    }),
                    resources: None,
                    prompts: None,
                    completions: None,
                    experimental: None,
                    logging: None,
                },
                server_info: Implementation {
                    name: EXTENSION_NAME.to_string(),
                    title: Some("Skills".to_string()),
                    version: "1.0.0".to_string(),
                    icons: None,
                    website_url: None,
                },
                instructions: Some(String::new()),
            },
            skills: skills.into(),
            context: test_context(),
        };

        let args = serde_json::json!({ "offset": 1, "limit": 1 })
            .as_object()
            .unwrap()
            .clone();
        let result = client
            .call_tool(
                "listSkills",
                Some(args),
                McpMeta::new(
                    "test-session",
                    crate::privacy::CallCapability::for_test_restricted(),
                ),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let text = &result.content[0].as_text().unwrap().text;
        let payload: serde_json::Value = serde_json::from_str(text).unwrap();

        assert_eq!(payload["total"], 3);
        assert_eq!(payload["returned"], 1);
        assert_eq!(payload["next_offset"], 2);
        assert_eq!(payload["skills"][0]["name"], "beta-skill");
    }

    #[tokio::test]
    async fn test_search_skills_filters_by_name_description_and_bundle() {
        let temp_dir = TempDir::new().unwrap();
        let bundle_dir = temp_dir.path().join("bio-bundle");
        fs::create_dir(&bundle_dir).unwrap();

        for (name, description) in [
            ("variant-calling", "Call variants from sequencing reads"),
            ("rna-qc", "Quality control for transcriptomics"),
            (
                "open-science-review",
                "Review systematic evidence using PRISMA style checklists",
            ),
            (
                "systematic-review-prisma",
                "Run systematic-review workflows and PRISMA flow diagrams",
            ),
            ("other", "Unrelated helper"),
        ] {
            let skill_dir = bundle_dir.join(name);
            fs::create_dir(&skill_dir).unwrap();
            fs::write(
                skill_dir.join("SKILL.md"),
                format!("---\nname: {name}\ndescription: {description}\n---\nBody"),
            )
            .unwrap();
        }

        let skills = SkillsClient::discover_skills_in_directories(&[temp_dir.path().to_path_buf()]);
        let client = SkillsClient {
            info: InitializeResult {
                protocol_version: ProtocolVersion::V_2025_03_26,
                capabilities: ServerCapabilities {
                    tasks: None,
                    tools: Some(ToolsCapability {
                        list_changed: Some(false),
                    }),
                    resources: None,
                    prompts: None,
                    completions: None,
                    experimental: None,
                    logging: None,
                },
                server_info: Implementation {
                    name: EXTENSION_NAME.to_string(),
                    title: Some("Skills".to_string()),
                    version: "1.0.0".to_string(),
                    icons: None,
                    website_url: None,
                },
                instructions: Some(String::new()),
            },
            skills: skills.into(),
            context: test_context(),
        };

        let args = serde_json::json!({ "query": "sequencing reads", "limit": 10 })
            .as_object()
            .unwrap()
            .clone();
        let result = client
            .call_tool(
                "searchSkills",
                Some(args),
                McpMeta::new(
                    "test-session",
                    crate::privacy::CallCapability::for_test_restricted(),
                ),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let text = &result.content[0].as_text().unwrap().text;
        let payload: serde_json::Value = serde_json::from_str(text).unwrap();

        assert_eq!(payload["total"], 1);
        assert_eq!(payload["skills"][0]["name"], "variant-calling");
        assert_eq!(payload["skills"][0]["bundle"], "bio-bundle");

        let args = serde_json::json!({ "query": "bio-bundle rna", "limit": 10 })
            .as_object()
            .unwrap()
            .clone();
        let result = client
            .call_tool(
                "searchSkills",
                Some(args),
                McpMeta::new(
                    "test-session",
                    crate::privacy::CallCapability::for_test_restricted(),
                ),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let text = &result.content[0].as_text().unwrap().text;
        let payload: serde_json::Value = serde_json::from_str(text).unwrap();

        assert_eq!(payload["total"], 1);
        assert_eq!(payload["skills"][0]["name"], "rna-qc");

        let args = serde_json::json!({ "query": "systematic review PRISMA", "limit": 10 })
            .as_object()
            .unwrap()
            .clone();
        let result = client
            .call_tool(
                "searchSkills",
                Some(args),
                McpMeta::new(
                    "test-session",
                    crate::privacy::CallCapability::for_test_restricted(),
                ),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let text = &result.content[0].as_text().unwrap().text;
        let payload: serde_json::Value = serde_json::from_str(text).unwrap();

        assert_eq!(payload["total"], 2);
        assert_eq!(payload["skills"][0]["name"], "systematic-review-prisma");
    }

    // BR-71: `test_context()` builds a `SessionManager`, whose lazy sqlx pool
    // must be constructed inside a Tokio runtime, so this is now an async test.
    #[tokio::test]
    async fn test_instructions_with_skills() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        fs::create_dir(&skills_dir).unwrap();

        let skill1_dir = skills_dir.join("alpha-skill");
        fs::create_dir(&skill1_dir).unwrap();
        fs::write(
            skill1_dir.join("SKILL.md"),
            r#"---
name: alpha-skill
description: First skill alphabetically
---
Content
"#,
        )
        .unwrap();

        let skill2_dir = skills_dir.join("beta-skill");
        fs::create_dir(&skill2_dir).unwrap();
        fs::write(
            skill2_dir.join("SKILL.md"),
            r#"---
name: beta-skill
description: Second skill alphabetically
---
Content
"#,
        )
        .unwrap();

        let skills = SkillsClient::discover_skills_in_directories(&[skills_dir]);
        assert_eq!(skills.len(), 2);

        let mut client = SkillsClient {
            info: InitializeResult {
                protocol_version: ProtocolVersion::V_2025_03_26,
                capabilities: ServerCapabilities {
                    tasks: None,
                    tools: Some(ToolsCapability {
                        list_changed: Some(false),
                    }),
                    resources: None,
                    prompts: None,
                    completions: None,
                    experimental: None,
                    logging: None,
                },
                server_info: Implementation {
                    name: EXTENSION_NAME.to_string(),
                    title: Some("Skills".to_string()),
                    version: "1.0.0".to_string(),
                    icons: None,
                    website_url: None,
                },
                instructions: Some(String::new()),
            },
            skills: skills.into(),
            context: test_context(),
        };

        let instructions = SkillsClient::generate_instructions(&client.skills.skills());
        assert!(!instructions.is_empty());
        assert!(instructions.contains("You have 2 skills available"));
        assert!(instructions.contains("searchSkills"));
        assert!(instructions.contains("listSkills"));
        // The instruction must actively nudge proactive loading via loadSkill
        // and name about-biorouter as the example, so the agent loads
        // self-knowledge instead of guessing about Biorouter.
        assert!(
            instructions.contains("loadSkill"),
            "skills instructions must reference the loadSkill tool"
        );
        assert!(
            instructions.contains("about-biorouter"),
            "skills instructions must point at the about-biorouter skill"
        );
        assert!(!instructions.contains("First skill alphabetically"));
        assert!(!instructions.contains("Second skill alphabetically"));

        client.info.instructions = Some(instructions);
        assert!(!client.info.instructions.as_ref().unwrap().is_empty());
    }

    #[test]
    fn test_discover_skills_working_dir_overrides_global() {
        let temp_dir = TempDir::new().unwrap();

        // Simulate ~/.claude/skills (global, lowest priority)
        let global_claude = temp_dir.path().join("global-claude");
        fs::create_dir(&global_claude).unwrap();
        let skill_global_claude = global_claude.join("my-skill");
        fs::create_dir(&skill_global_claude).unwrap();
        fs::write(
            skill_global_claude.join("SKILL.md"),
            r#"---
name: my-skill
description: From global claude
---
Global claude content
"#,
        )
        .unwrap();

        // Simulate ~/.config/biorouter/skills (global, medium priority)
        let global_biorouter = temp_dir.path().join("global-biorouter");
        fs::create_dir(&global_biorouter).unwrap();
        let skill_global_biorouter = global_biorouter.join("my-skill");
        fs::create_dir(&skill_global_biorouter).unwrap();
        fs::write(
            skill_global_biorouter.join("SKILL.md"),
            r#"---
name: my-skill
description: From global biorouter config
---
Global biorouter config content
"#,
        )
        .unwrap();

        // Simulate $PWD/.claude/skills (working dir, higher priority)
        let working_claude = temp_dir.path().join("working-claude");
        fs::create_dir(&working_claude).unwrap();
        let skill_working_claude = working_claude.join("my-skill");
        fs::create_dir(&skill_working_claude).unwrap();
        fs::write(
            skill_working_claude.join("SKILL.md"),
            r#"---
name: my-skill
description: From working dir claude
---
Working dir claude content
"#,
        )
        .unwrap();

        // Simulate $PWD/.biorouter/skills (working dir, highest priority)
        let working_biorouter = temp_dir.path().join("working-biorouter");
        fs::create_dir(&working_biorouter).unwrap();
        let skill_working_biorouter = working_biorouter.join("my-skill");
        fs::create_dir(&skill_working_biorouter).unwrap();
        fs::write(
            skill_working_biorouter.join("SKILL.md"),
            r#"---
name: my-skill
description: From working dir biorouter
---
Working dir biorouter content
"#,
        )
        .unwrap();

        // Test priority order: global_claude < global_biorouter < working_claude < working_biorouter
        let skills = SkillsClient::discover_skills_in_directories(&[
            global_claude,
            global_biorouter,
            working_claude,
            working_biorouter,
        ]);

        assert_eq!(skills.len(), 1);
        assert!(skills.contains_key("my-skill"));
        // The last directory (working_biorouter) should win
        assert_eq!(
            skills.get("my-skill").unwrap().metadata.description,
            "From working dir biorouter"
        );
        assert!(skills
            .get("my-skill")
            .unwrap()
            .body
            .contains("Working dir biorouter content"));
    }

    #[test]
    fn test_discover_extension_skills() {
        let temp_dir = TempDir::new().unwrap();
        let skill_dir = temp_dir
            .path()
            .join("extensions")
            .join("myext")
            .join("skills")
            .join("my-ext-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: my-ext-skill\ndescription: An extension skill\n---\n\nBody here.",
        )
        .unwrap();

        let ext_skills_dir = temp_dir
            .path()
            .join("extensions")
            .join("myext")
            .join("skills");
        let skills = SkillsClient::discover_skills_in_directories(&[ext_skills_dir]);
        assert!(
            skills.contains_key("my-ext-skill"),
            "extension skill not found"
        );
        assert_eq!(
            skills["my-ext-skill"].metadata.description,
            "An extension skill"
        );
    }

    #[test]
    fn test_get_default_skill_directories_includes_extensions() {
        let temp_dir = TempDir::new().unwrap();
        let ext_skills = temp_dir
            .path()
            .join("config")
            .join("extensions")
            .join("myext")
            .join("skills");
        fs::create_dir_all(&ext_skills).unwrap();
        let skill_dir = ext_skills.join("my-ext-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: my-ext-skill\ndescription: test\n---\nbody",
        )
        .unwrap();

        // `BIOROUTER_PATH_ROOT` is process-global and is read on every
        // `Paths::*` call, so a bare set/remove pair leaks this scratch root
        // into whatever else is running concurrently (it made `logging::tests`
        // resolve into this `TempDir` and fail once it was dropped). The guard
        // both serialises against the other tests that pin this variable and
        // restores the previous value even if the assertions below panic.
        let dirs = {
            let _guard = env_lock::lock_env([(
                "BIOROUTER_PATH_ROOT",
                Some(temp_dir.path().to_str().unwrap()),
            )]);
            SkillsClient::get_default_skill_directories()
        };

        assert!(
            dirs.iter().any(|d| d == &ext_skills),
            "extension skills dir not in default dirs: {:?}",
            dirs
        );
    }

    #[test]
    fn test_discover_single_skill() {
        let temp_dir = TempDir::new().unwrap();
        let skill_dir = temp_dir.path().join("my-skill");
        fs::create_dir(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: my-skill\ndescription: A test skill\n---\nBody",
        )
        .unwrap();

        let skills = SkillsClient::discover_skills_in_directories(&[temp_dir.path().to_path_buf()]);
        assert_eq!(skills.len(), 1);
        let skill = skills.get("my-skill").unwrap();
        assert_eq!(skill.metadata.name, "my-skill");
        assert!(skill.bundle_name.is_none());
    }

    #[test]
    fn test_discover_bundle() {
        let temp_dir = TempDir::new().unwrap();
        let bundle_dir = temp_dir.path().join("superpowers");
        fs::create_dir(&bundle_dir).unwrap();

        let sub1 = bundle_dir.join("brainstorming");
        fs::create_dir(&sub1).unwrap();
        fs::write(
            sub1.join("SKILL.md"),
            "---\nname: brainstorming\ndescription: Brainstorm ideas\n---\nBody",
        )
        .unwrap();

        let sub2 = bundle_dir.join("debugging");
        fs::create_dir(&sub2).unwrap();
        fs::write(
            sub2.join("SKILL.md"),
            "---\nname: debugging\ndescription: Debug code\n---\nBody",
        )
        .unwrap();

        let skills = SkillsClient::discover_skills_in_directories(&[temp_dir.path().to_path_buf()]);
        assert_eq!(skills.len(), 2);

        let br = skills.get("brainstorming").unwrap();
        assert_eq!(br.bundle_name.as_deref(), Some("superpowers"));
        assert_eq!(
            br.source_root,
            temp_dir.path(),
            "discovery must record which root a skill came from"
        );

        let dbg = skills.get("debugging").unwrap();
        assert_eq!(dbg.bundle_name.as_deref(), Some("superpowers"));
    }

    #[test]
    fn test_bundle_disabled_by_bundle_name() {
        let temp_dir = TempDir::new().unwrap();
        let bundle_dir = temp_dir.path().join("superpowers");
        fs::create_dir(&bundle_dir).unwrap();

        let sub = bundle_dir.join("brainstorming");
        fs::create_dir(&sub).unwrap();
        fs::write(
            sub.join("SKILL.md"),
            "---\nname: brainstorming\ndescription: Brainstorm ideas\n---\nBody",
        )
        .unwrap();

        let skills = SkillsClient::discover_skills_in_directories(&[temp_dir.path().to_path_buf()]);
        assert_eq!(skills.len(), 1);

        let mut disabled = std::collections::HashSet::new();
        disabled.insert("superpowers".to_string());

        let filtered: Vec<_> = skills
            .into_iter()
            .filter(|(name, skill)| {
                !disabled.contains(name)
                    && !skill
                        .bundle_name
                        .as_deref()
                        .is_some_and(|b| disabled.contains(b))
            })
            .collect();

        assert!(
            filtered.is_empty(),
            "bundle skill should be filtered when bundle name is disabled"
        );
    }

    /// A shared fixture catalog, so the assertions below are about the session
    /// override and not about whatever the developer happens to have installed
    /// in `~/.config/biorouter/skills`.
    fn fixture_skill(name: &str, root: &std::path::Path) -> Skill {
        Skill {
            metadata: SkillMetadata {
                name: name.to_string(),
                description: format!("{name} fixture"),
            },
            body: String::new(),
            directory: root.join(name),
            supporting_files: Vec::new(),
            bundle_name: None,
            source_root: root.to_path_buf(),
        }
    }

    fn client_with(
        names: &[&str],
        root: &std::path::Path,
        sm: Arc<SessionManager>,
    ) -> SkillsClient {
        let mut client = SkillsClient::new(PlatformExtensionContext {
            extension_manager: None,
            session_manager: sm,
        })
        .unwrap();
        // (`mod tests` is a descendant of the module that defines
        // `SkillsClient`, so its private fields are in scope here — the same
        // access the existing frontmatter tests use.)
        client.skills = names
            .iter()
            .map(|n| (n.to_string(), fixture_skill(n, root)))
            .collect::<HashMap<_, _>>()
            .into();
        client
    }

    fn tool_text(result: &CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[tokio::test]
    async fn a_session_override_filters_the_catalog_without_touching_the_config_file() {
        let temp = TempDir::new().unwrap();
        let session_manager = Arc::new(SessionManager::new(temp.path().to_path_buf()));
        let session = session_manager
            .create_session(
                temp.path().to_path_buf(),
                "skills-override".to_string(),
                crate::session::SessionType::User,
            )
            .await
            .unwrap();

        let client = client_with(&["alpha", "beta"], temp.path(), session_manager.clone());

        let machine_config = Paths::config_dir().join("skills-config.json");
        let before = fs::read_to_string(&machine_config).ok();

        let over = crate::agents::session_skills::for_session(&session_manager, &session.id)
            .await
            .unwrap();
        assert_eq!(
            client.enabled_names(&over).len(),
            2,
            "both fixtures start enabled"
        );

        // Disable one FOR THIS SESSION ONLY.
        crate::agents::session_skills::apply(
            &session_manager,
            &session.id,
            &[],
            &["beta".to_string()],
        )
        .await
        .unwrap();

        let over = crate::agents::session_skills::for_session(&session_manager, &session.id)
            .await
            .unwrap();
        let names: Vec<String> = client.enabled_names(&over);
        assert_eq!(
            names,
            vec!["alpha".to_string()],
            "the session override must shrink this client's catalog"
        );

        // Decision (c): the machine-wide preference file is byte-identical —
        // including the case where it never existed and must still not exist.
        assert_eq!(
            fs::read_to_string(&machine_config).ok(),
            before,
            "workspace/session skill scoping must never write skills-config.json"
        );
    }

    /// The gate the reviewers asked for: drive the REAL runtime seam. Nothing
    /// here binds a session by hand — the override is persisted, the client is
    /// built cold, and the only thing that carries the session id is the
    /// `McpMeta` of a `call_tool` dispatch. It therefore pins, in one test,
    /// that `call_tool` reads the session's override at all, that the catalog
    /// (`listSkills`) is filtered by it, and that `loadSkill` enforces it —
    /// each of which a hand-bound unit test leaves free to regress.
    #[tokio::test]
    async fn a_persisted_override_reaches_call_tool_with_no_in_process_binding() {
        let temp = TempDir::new().unwrap();
        let session_manager = Arc::new(SessionManager::new(temp.path().to_path_buf()));
        let session = session_manager
            .create_session(
                temp.path().to_path_buf(),
                "skills-e2e".to_string(),
                crate::session::SessionType::User,
            )
            .await
            .unwrap();
        crate::agents::session_skills::apply(
            &session_manager,
            &session.id,
            &[],
            &["beta".to_string()],
        )
        .await
        .unwrap();

        // A COLD client: the override exists only in the session row.
        let client = client_with(&["alpha", "beta"], temp.path(), session_manager.clone());
        let meta = McpMeta::new(
            session.id.clone(),
            crate::privacy::CallCapability::for_test_restricted(),
        );

        let listed = client
            .call_tool("listSkills", None, meta.clone(), CancellationToken::new())
            .await
            .unwrap();
        let listed = tool_text(&listed);
        assert!(
            listed.contains("alpha"),
            "the surviving skill must still be listed: {listed}"
        );
        assert!(
            !listed.contains("beta"),
            "a session-revoked skill must not appear in listSkills: {listed}"
        );

        let refused = client
            .call_tool(
                "loadSkill",
                Some(
                    serde_json::json!({"name": "beta"})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
                meta.clone(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(refused.is_error, Some(true));
        assert!(
            tool_text(&refused).contains("disabled"),
            "loadSkill must refuse a session-revoked skill: {}",
            tool_text(&refused)
        );

        let allowed = client
            .call_tool(
                "loadSkill",
                Some(
                    serde_json::json!({"name": "alpha"})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
                meta,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_ne!(allowed.is_error, Some(true));
    }

    /// The ACP server shares ONE `Agent` — and therefore one
    /// `ExtensionManager` and one `SkillsClient` — across every session, and
    /// spawns prompts concurrently. So this client must hold no per-session
    /// state: an override belongs to the `call_tool` dispatch that carries the
    /// session id, never to the client. Storing the "currently bound session"
    /// on the client lets session A's `loadSkill` resolve against session B's
    /// grant after an await — a cross-session capability leak.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_dispatches_for_two_sessions_never_see_each_others_overrides() {
        let temp = TempDir::new().unwrap();
        let session_manager = Arc::new(SessionManager::new(temp.path().to_path_buf()));
        let a = session_manager
            .create_session(
                temp.path().to_path_buf(),
                "chat-a".to_string(),
                crate::session::SessionType::User,
            )
            .await
            .unwrap();
        let b = session_manager
            .create_session(
                temp.path().to_path_buf(),
                "chat-b".to_string(),
                crate::session::SessionType::User,
            )
            .await
            .unwrap();
        // Complementary revocations: whichever session's state a shared client
        // leaked, the other one's assertion fails.
        crate::agents::session_skills::apply(&session_manager, &a.id, &[], &["beta".to_string()])
            .await
            .unwrap();
        crate::agents::session_skills::apply(&session_manager, &b.id, &[], &["alpha".to_string()])
            .await
            .unwrap();

        let client = Arc::new(client_with(
            &["alpha", "beta"],
            temp.path(),
            session_manager.clone(),
        ));

        for _ in 0..20 {
            let load = |session_id: String, skill: &'static str| {
                let client = Arc::clone(&client);
                async move {
                    client
                        .call_tool(
                            "loadSkill",
                            Some(
                                serde_json::json!({"name": skill})
                                    .as_object()
                                    .unwrap()
                                    .clone(),
                            ),
                            McpMeta::new(
                                session_id,
                                crate::privacy::CallCapability::for_test_restricted(),
                            ),
                            CancellationToken::new(),
                        )
                        .await
                        .unwrap()
                }
            };
            // Each session asks for the skill IT revoked. Both must be refused.
            let (from_a, from_b) =
                tokio::join!(load(a.id.clone(), "beta"), load(b.id.clone(), "alpha"));
            assert_eq!(
                from_a.is_error,
                Some(true),
                "session A revoked beta; it must not load it: {}",
                tool_text(&from_a)
            );
            assert_eq!(
                from_b.is_error,
                Some(true),
                "session B revoked alpha; it must not load it: {}",
                tool_text(&from_b)
            );
        }
    }

    /// Fail CLOSED. A revocation that cannot be read is not "no revocation":
    /// answering an unreadable or corrupt override with an empty one hands the
    /// model back every skill the session had removed.
    #[tokio::test]
    async fn an_unreadable_override_refuses_the_call_instead_of_granting_everything() {
        let temp = TempDir::new().unwrap();
        let session_manager = Arc::new(SessionManager::new(temp.path().to_path_buf()));
        let session = session_manager
            .create_session(
                temp.path().to_path_buf(),
                "skills-corrupt".to_string(),
                crate::session::SessionType::User,
            )
            .await
            .unwrap();
        session_manager
            .update_extension_state(
                &session.id,
                crate::agents::session_skills::STATE_KEY,
                crate::agents::session_skills::STATE_VERSION,
                |_| Ok(serde_json::json!("not an override")),
            )
            .await
            .unwrap();

        let client = client_with(&["alpha", "beta"], temp.path(), session_manager.clone());
        let listed = client
            .call_tool(
                "listSkills",
                None,
                McpMeta::new(
                    session.id.clone(),
                    crate::privacy::CallCapability::for_test_restricted(),
                ),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(listed.is_error, Some(true), "{}", tool_text(&listed));
        assert!(
            !tool_text(&listed).contains("alpha"),
            "a call that cannot read the override must not answer from the machine-wide view"
        );
    }

    /// Decision (c), the half a file-untouched assertion cannot see: a session
    /// override must not change what the machine-wide preference MEANS.
    ///
    /// The machine-wide disabled array holds skill names AND bundle names
    /// (`is_skill_enabled` tests both; the existing
    /// `test_bundle_disabled_by_bundle_name` puts a bundle id in it). Any
    /// implementation that composes the override by rebuilding a name set from
    /// `self.skills.keys()` drops every bundle entry, so an unrelated
    /// `add_skills` silently re-enables a whole disabled bundle for that
    /// session — with `skills-config.json` still byte-identical, so the test
    /// above stays green.
    ///
    /// Asserted against the composed FILTER directly, with a hand-built
    /// disabled set, exactly as `test_bundle_disabled_by_bundle_name` does.
    /// Going through `enabled_skill_entries` would require writing
    /// `Paths::config_dir()/skills-config.json` — the developer's real machine
    /// preference file, which `get_disabled_skills` is the only reader of and
    /// which this feature must never touch.
    #[test]
    fn a_session_grant_does_not_resurrect_a_machine_disabled_bundle() {
        use crate::agents::session_skills::SessionSkillOverride;

        let temp = TempDir::new().unwrap();
        let bundled = Skill {
            metadata: SkillMetadata {
                name: "gamma".to_string(),
                description: "bundled fixture".to_string(),
            },
            body: String::new(),
            directory: temp.path().join("gamma"),
            supporting_files: Vec::new(),
            bundle_name: Some("bundle-x".to_string()),
            source_root: temp.path().to_path_buf(),
        };

        // The operator disabled the BUNDLE machine-wide.
        let mut machine = std::collections::HashSet::new();
        machine.insert("bundle-x".to_string());

        // Baseline: no override at all.
        let none = SessionSkillOverride::default();
        assert!(
            !SkillsClient::is_skill_enabled_for_session("gamma", &bundled, &machine, &none),
            "baseline: a machine-disabled bundle hides its skills"
        );

        // An UNRELATED session grant must not change that.
        let unrelated = SessionSkillOverride {
            add: vec!["something-else".to_string()],
            remove: Vec::new(),
        };
        assert!(
            !SkillsClient::is_skill_enabled_for_session("gamma", &bundled, &machine, &unrelated),
            "a session grant for another skill must not re-enable a machine-disabled BUNDLE"
        );

        // An EXPLICIT session grant of this skill still wins — that is the
        // feature, and it is scoped to one session and one skill.
        let explicit = SessionSkillOverride {
            add: vec!["gamma".to_string()],
            remove: Vec::new(),
        };
        assert!(
            SkillsClient::is_skill_enabled_for_session("gamma", &bundled, &machine, &explicit),
            "an explicit session grant of this skill is the documented escape hatch"
        );
    }

    #[test]
    fn test_builtin_skill_content_is_valid() {
        for (name, content) in BUILTIN_SKILLS {
            let (metadata, body) = SkillsClient::parse_frontmatter(content).unwrap_or_else(|e| {
                panic!("builtin skill '{}' has invalid frontmatter: {}", name, e)
            });
            assert_eq!(&metadata.name, name, "frontmatter name must match slug");
            assert!(!metadata.description.is_empty());
            assert!(!body.is_empty());
        }
    }

    /// The about-biorouter skill is the offload target for component self-
    /// knowledge: it must cover every pillar (so the agent can answer "what is
    /// Biorouter / how do I use X") and its description must trigger on
    /// questions about Biorouter itself.
    #[test]
    fn test_about_biorouter_skill_covers_all_pillars() {
        let content = BUILTIN_SKILLS
            .iter()
            .find(|(name, _)| *name == "about-biorouter")
            .map(|(_, c)| *c)
            .expect("about-biorouter skill must be built in");
        let (metadata, body) = SkillsClient::parse_frontmatter(content).unwrap();

        // Description is the trigger the model sees; it must mention Biorouter
        // self-knowledge so the skill is loaded on the right questions.
        let desc = metadata.description.to_lowercase();
        assert!(
            desc.contains("biorouter") && desc.contains("load this skill"),
            "description must instruct loading on Biorouter questions"
        );

        for pillar in [
            "Extensions",
            "Skills",
            "Workflows",
            "Scheduler",
            "Knowledge bases",
            "Soul",
        ] {
            assert!(
                body.contains(pillar),
                "about-biorouter skill is missing pillar coverage: {pillar}"
            );
        }
    }

    #[test]
    fn test_ensure_builtin_skills_seeds_and_restores() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");

        fs::create_dir_all(skills_dir.join("user-skill")).unwrap();
        let user_skill = skills_dir.join("user-skill").join("SKILL.md");
        fs::write(&user_skill, "user-authored content").unwrap();

        let config_file = temp_dir.path().join("skills-config.json");
        let disabled_preferences = r#"{"disabled":["about-biorouter","user-skill"]}"#;
        fs::write(&config_file, disabled_preferences).unwrap();

        // First call seeds from scratch.
        SkillsClient::ensure_builtin_skills(&skills_dir);
        for (name, content) in BUILTIN_SKILLS {
            let seeded = skills_dir.join(name).join("SKILL.md");
            assert!(seeded.exists(), "builtin skill {name} should be seeded");
            assert_eq!(fs::read_to_string(seeded).unwrap(), *content);
        }
        assert_eq!(
            fs::read_to_string(&user_skill).unwrap(),
            "user-authored content"
        );
        assert_eq!(
            fs::read_to_string(&config_file).unwrap(),
            disabled_preferences,
            "seeding must not change disabled skill preferences"
        );

        // Stale content is refreshed.
        let seeded = skills_dir.join("about-biorouter").join("SKILL.md");
        fs::write(&seeded, "outdated").unwrap();
        SkillsClient::ensure_builtin_skills(&skills_dir);
        let refreshed = fs::read_to_string(&seeded).unwrap();
        assert!(refreshed.contains("name: about-biorouter"));

        // Deletion is undone on the next call.
        fs::remove_dir_all(skills_dir.join("about-biorouter")).unwrap();
        SkillsClient::ensure_builtin_skills(&skills_dir);
        assert!(
            seeded.exists(),
            "builtin skill should be restored after deletion"
        );
    }

    /// The knowledge-format skills seed through the same path and are
    /// recognised as shipped, so `count_user_skills` does not report four
    /// skills the user never installed.
    #[test]
    fn the_knowledge_skills_ship_and_are_not_counted_as_the_users() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        SkillsClient::ensure_builtin_skills(&skills_dir);
        for (name, content) in KNOWLEDGE_SKILLS {
            let seeded = skills_dir.join(name).join("SKILL.md");
            assert!(seeded.exists(), "{name} should be seeded");
            assert_eq!(fs::read_to_string(seeded).unwrap(), *content);
            assert!(is_builtin_skill_name(name), "{name} reads as a user skill");
        }
    }

    /// ⚠ **They are shipped, and they are NOT Contexts.** `contexts.test.ts`
    /// reads this file's source, slices the `BUILTIN_SKILLS` literal, and
    /// asserts the desktop's two hand-synced copies name exactly those four plus
    /// `update-soul`. The knowledge skills live in their own array *because* of
    /// that: they are task-triggered, not always-on self-knowledge, so they do
    /// not belong in the Settings Contexts pane — and putting them there would
    /// fail a UI test from a Rust edit. This asserts the separation directly, so
    /// the reason cannot be lost by someone tidying the two arrays into one.
    #[test]
    fn a_knowledge_skill_is_shipped_but_is_not_a_context() {
        let contexts: Vec<&str> = context_skill_names().collect();
        assert_eq!(contexts.len(), 5, "the Contexts list moved: {contexts:?}");
        for (name, _) in KNOWLEDGE_SKILLS {
            assert!(
                !contexts.contains(name),
                "{name} became a Context; ui/desktop's CONTEXTS and \
                 skillUtils.BUILTIN_SKILL_NAMES must gain it in the same change"
            );
        }
    }

    /// Every shipped skill parses, and its frontmatter `name` is its directory
    /// name — which is the key it occupies in the skill map, so a mismatch makes
    /// the skill both un-loadable by name and un-filterable by directory.
    #[test]
    fn every_shipped_skill_parses_and_names_itself_after_its_directory() {
        let mut seen: Vec<&str> = Vec::new();
        for (dir, content) in shipped_skills() {
            let (metadata, body) = SkillsClient::parse_frontmatter(content)
                .unwrap_or_else(|e| panic!("{dir}/SKILL.md has unparseable frontmatter: {e}"));
            assert_eq!(&metadata.name, dir, "{dir} names itself something else");
            assert!(
                !metadata.description.trim().is_empty(),
                "{dir} has no description, which is its entire trigger"
            );
            assert!(!body.trim().is_empty(), "{dir} has no body");
            assert!(!seen.contains(dir), "{dir} is declared twice");
            seen.push(dir);
        }
    }

    /// ⚠ **The literal, both sides.** `contexts.test.ts` pins
    /// `contextConfigKey('about-biorouter') === 'context_about_biorouter'`; this
    /// pins the same string from Rust. The two files never meet at runtime — the
    /// config key is the whole handshake — so a hyphen/underscore or prefix
    /// change on one side is otherwise completely silent: the Settings switch
    /// keeps moving, the value keeps being written, and this side keeps reading
    /// a key nobody writes.
    #[test]
    fn the_context_config_key_matches_the_one_the_settings_switch_writes() {
        assert_eq!(
            context_config_key("about-biorouter"),
            "context_about_biorouter"
        );
        assert_eq!(context_config_key("update-soul"), "context_update_soul");
        assert_eq!(
            context_config_key("develop-biorouter-extension"),
            "context_develop_biorouter_extension"
        );
        // Every shipped Context must produce a plain identifier — the same
        // assertion `contexts.test.ts` makes with /^context_[a-z0-9_]+$/.
        for name in context_skill_names() {
            let key = context_config_key(name);
            let body = key
                .strip_prefix("context_")
                .unwrap_or_else(|| panic!("{name} derives a key without the prefix: {key}"));
            assert!(
                body.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "{name} derives a key that is not a plain identifier: {key}"
            );
        }
    }

    /// Absence means ON, `false` means off, and nothing else is consulted.
    #[test]
    fn only_an_explicit_false_hides_a_context() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.yaml");
        let config = crate::config::Config::new_with_file_secrets(
            &config_path,
            temp.path().join("secrets.yaml"),
        )
        .unwrap();

        // A config that has never seen the Contexts screen. The default MUST be
        // "everything on": reading a missing key as off would strip five
        // contexts from every existing install the moment this shipped.
        assert!(
            hidden_contexts_in(&config).is_empty(),
            "a user who never opened Settings must lose nothing"
        );

        config.set_param("context_about_biorouter", false).unwrap();
        config.set_param("context_develop_biorouter", true).unwrap();
        // A key that is not a Context's must not leak into the set, and a
        // *skill* disabled the ordinary way is not this function's business.
        config.set_param("context_single_cell", false).unwrap();
        assert_eq!(
            hidden_contexts_in(&config),
            std::collections::HashSet::from(["about-biorouter".to_string()]),
        );
    }

    /// The whole chain, through the real `Config::global()` seam: a Context
    /// switched off in Settings leaves the catalog the model is told about, and
    /// stays loadable by exact name.
    ///
    /// ⚠ **The second half is not a nicety.** `prompts/system.md` tells the
    /// model to load `about-biorouter` unconditionally, so a Context routed
    /// through `skills-config.json`'s `disabled[]` — the obvious "fix" — would
    /// make the agent report a failed skill load on every single turn. That is
    /// why enablement lives in a config key at all, and this is the assertion
    /// that stops someone simplifying it back.
    ///
    /// The overrides are task-local (`with_config_overrides`), so this neither
    /// reads the developer's own `config.yaml` for the keys it cares about nor
    /// mutates the process environment — the two ways a test of a
    /// process-global singleton usually becomes order-dependent.
    #[tokio::test]
    async fn a_context_switched_off_leaves_the_catalog_but_stays_loadable() {
        let temp = TempDir::new().unwrap();
        let session_manager = Arc::new(SessionManager::new(temp.path().to_path_buf()));
        let mut client = client_with(&["alpha"], temp.path(), session_manager);
        client.skills.pinned_mut().insert(
            "about-biorouter".to_string(),
            fixture_skill("about-biorouter", temp.path()),
        );
        let over = crate::agents::session_skills::SessionSkillOverride::default();

        // Pin every Context key so the developer's own config cannot decide the
        // outcome either way.
        let all_on: std::collections::HashMap<String, String> = context_skill_names()
            .map(|n| (context_config_key(n).to_uppercase(), "true".to_string()))
            .collect();
        let mut about_off = all_on.clone();
        about_off.insert("CONTEXT_ABOUT_BIOROUTER".to_string(), "false".to_string());

        let names = |client: &SkillsClient, over: &_| -> Vec<String> { client.enabled_names(over) };

        crate::config::with_config_overrides(all_on, async {
            assert_eq!(
                names(&client, &over),
                vec!["about-biorouter".to_string(), "alpha".to_string()],
                "switched on, a Context is in the catalog like any other skill"
            );
            assert!(
                SkillsClient::generate_instructions(&client.skills.skills()).contains("2 skills")
            );
        })
        .await;

        crate::config::with_config_overrides(about_off, async {
            assert_eq!(
                names(&client, &over),
                vec!["alpha".to_string()],
                "the switch must actually remove it from what the model is told about"
            );
            // The `{skill_count}` sentence is generated from the same list, so
            // the number the model reads has to move with it.
            assert!(
                SkillsClient::generate_instructions(&client.skills.skills()).contains("1 skills"),
                "instructions still count the hidden Context: {}",
                SkillsClient::generate_instructions(&client.skills.skills())
            );
            // listSkills is the catalog the model pages through.
            let listed = client.handle_list_skills(None, &over).await.unwrap();
            let listed: String = listed
                .iter()
                .filter_map(|c| c.as_text().map(|t| t.text.clone()))
                .collect();
            assert!(
                !listed.contains("about-biorouter"),
                "listSkills still advertises it: {listed}"
            );

            // …and it is still loadable, because the system prompt asks for it
            // by name on every turn.
            let loaded = client
                .handle_load_skill(
                    Some(
                        serde_json::json!({ "name": "about-biorouter" })
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                    &over,
                )
                .await
                .expect("a hidden Context must not become unloadable");
            let loaded: String = loaded
                .iter()
                .filter_map(|c| c.as_text().map(|t| t.text.clone()))
                .collect();
            assert!(loaded.contains("about-biorouter"), "{loaded}");
        })
        .await;
    }
}
