use crate::agents::extension::PlatformExtensionContext;
use crate::agents::mcp_client::{Error, McpClientTrait, McpMeta};
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
        "develop-biorouter-extension",
        include_str!("builtin_skills/develop-biorouter-extension/SKILL.md"),
    ),
    (
        "develop-biorouter-skill",
        include_str!("builtin_skills/develop-biorouter-skill/SKILL.md"),
    ),
];

pub(crate) fn install_builtin_skills() {
    SkillsClient::ensure_builtin_skills(&Paths::config_dir().join("skills"));
}

fn is_builtin_skill_name(name: &str) -> bool {
    BUILTIN_SKILLS
        .iter()
        .any(|(builtin_name, _)| *builtin_name == name)
        || name == crate::knowledge::soul::SOUL_SKILL_DIR
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

    for (name, _) in BUILTIN_SKILLS {
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

pub struct SkillsClient {
    info: InitializeResult,
    skills: HashMap<String, Skill>,
}

const DEFAULT_SKILL_PAGE_LIMIT: usize = 20;
const MAX_SKILL_PAGE_LIMIT: usize = 50;

impl SkillsClient {
    pub fn new(_context: PlatformExtensionContext) -> Result<Self> {
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

        let directories = Self::get_default_skill_directories()
            .into_iter()
            .filter(|d| d.exists())
            .collect::<Vec<_>>();
        let mut skills = Self::discover_skills_in_directories(&directories);

        // Guarantee builtin skills are present even if seeding to disk failed
        // (e.g. read-only config dir) or a user skill shadowed the slug.
        for (name, content) in BUILTIN_SKILLS {
            if !skills.contains_key(*name) {
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

        let mut client = Self { info, skills };
        client.info.instructions = Some(client.generate_instructions());
        Ok(client)
    }

    /// Seed (or refresh) the built-in skills under the user's skills directory
    /// so they show up in the Skills UI and survive deletion. Content is
    /// rewritten when it differs so app updates propagate. Failures are
    /// non-fatal: the in-memory fallback in `new()` still registers them.
    fn ensure_builtin_skills(skills_dir: &Path) {
        for (name, content) in BUILTIN_SKILLS {
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
    pub fn get_default_skill_directories() -> Vec<PathBuf> {
        let mut dirs = Vec::new();

        if let Some(home) = dirs::home_dir() {
            dirs.push(home.join(".claude/skills"));
            dirs.push(home.join(".config/agents/skills"));
        }

        dirs.push(Paths::config_dir().join("skills"));

        // Scan installed .brxt extension skills subdirectories
        let extensions_dir = Paths::config_dir().join("extensions");
        if extensions_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&extensions_dir) {
                for entry in entries.flatten() {
                    if !entry.path().is_dir() {
                        continue;
                    }
                    let skills_subdir = entry.path().join("skills");
                    if skills_subdir.is_dir() {
                        dirs.push(skills_subdir);
                    }
                }
            }
        }

        if let Ok(working_dir) = std::env::current_dir() {
            dirs.push(working_dir.join(".claude/skills"));
            dirs.push(working_dir.join(".biorouter/skills"));
            dirs.push(working_dir.join(".agents/skills"));
        }

        dirs
    }

    fn get_disabled_skills() -> std::collections::HashSet<String> {
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

    fn generate_instructions(&self) -> String {
        if self.skills.is_empty() {
            return String::new();
        }

        let skill_count = self.enabled_skill_entries().len();

        if skill_count == 0 {
            return String::new();
        }

        format!(
            "You have {skill_count} skills available through the skills extension. Use searchSkills to find relevant skills, listSkills to page through the catalog, and loadSkill to load an exact skill by name before relying on it. For Biorouter questions, load about-biorouter directly."
        )
    }

    fn is_skill_enabled(
        name: &str,
        skill: &Skill,
        disabled: &std::collections::HashSet<String>,
    ) -> bool {
        !disabled.contains(name)
            && !skill
                .bundle_name
                .as_deref()
                .is_some_and(|bundle| disabled.contains(bundle))
    }

    fn enabled_skill_entries(&self) -> Vec<(&String, &Skill)> {
        let disabled = Self::get_disabled_skills();
        let mut skill_list: Vec<_> = self
            .skills
            .iter()
            .filter(|(name, skill)| Self::is_skill_enabled(name, skill, &disabled))
            .collect();
        skill_list.sort_by_key(|(name, _)| *name);
        skill_list
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
    ) -> Result<Vec<Content>, String> {
        let params: ListSkillsParams = Self::parse_tool_args(arguments)?;
        let (offset, limit) = Self::parse_pagination(params.offset, params.limit);
        let skill_list = self.enabled_skill_entries();
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
    ) -> Result<Vec<Content>, String> {
        let params: SearchSkillsParams = Self::parse_tool_args(arguments)?;
        let query = Self::normalize_search_text(params.query.trim());
        if query.is_empty() {
            return Err("Missing required parameter: query".to_string());
        }

        let terms: Vec<&str> = query.split_whitespace().collect();
        let (offset, limit) = Self::parse_pagination(params.offset, params.limit);
        let mut matches: Vec<_> = self
            .enabled_skill_entries()
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
    ) -> Result<Vec<Content>, String> {
        let skill_name = arguments
            .as_ref()
            .ok_or("Missing arguments")?
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: name")?;

        // Runtime check: reject disabled skills even mid-session
        let disabled = Self::get_disabled_skills();
        if let Some(skill) = self.skills.get(skill_name) {
            let is_disabled = disabled.contains(skill_name)
                || skill
                    .bundle_name
                    .as_deref()
                    .is_some_and(|b| disabled.contains(b));
            if is_disabled {
                return Err(format!(
                    "Skill '{}' is currently disabled. Enable it in Biorouter's Skills settings to use it.",
                    skill_name
                ));
            }
        }

        let skill = self
            .skills
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
        let has_enabled_skills = self.skills.iter().any(|(name, skill)| {
            !disabled.contains(name)
                && !skill
                    .bundle_name
                    .as_deref()
                    .is_some_and(|b| disabled.contains(b))
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
        _meta: McpMeta,
        _cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        let content = match name {
            "searchSkills" => self.handle_search_skills(arguments).await,
            "listSkills" => self.handle_list_skills(arguments).await,
            "loadSkill" => self.handle_load_skill(arguments).await,
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
    use std::fs;
    use tempfile::TempDir;

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

    #[test]
    fn test_empty_instructions_when_no_skills() {
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
            skills,
        };

        let instructions = client.generate_instructions();
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
            skills,
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
            skills,
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
            skills,
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
        let content = client
            .handle_load_skill(Some(arguments))
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
            skills,
        };

        let args = serde_json::json!({ "offset": 1, "limit": 1 })
            .as_object()
            .unwrap()
            .clone();
        let result = client
            .call_tool(
                "listSkills",
                Some(args),
                McpMeta::new("test-session"),
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
            skills,
        };

        let args = serde_json::json!({ "query": "sequencing reads", "limit": 10 })
            .as_object()
            .unwrap()
            .clone();
        let result = client
            .call_tool(
                "searchSkills",
                Some(args),
                McpMeta::new("test-session"),
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
                McpMeta::new("test-session"),
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
                McpMeta::new("test-session"),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let text = &result.content[0].as_text().unwrap().text;
        let payload: serde_json::Value = serde_json::from_str(text).unwrap();

        assert_eq!(payload["total"], 2);
        assert_eq!(payload["skills"][0]["name"], "systematic-review-prisma");
    }

    #[test]
    fn test_instructions_with_skills() {
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
            skills,
        };

        let instructions = client.generate_instructions();
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
}
