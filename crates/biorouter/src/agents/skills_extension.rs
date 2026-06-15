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
/// directory on every session start, so removing the folder only lasts until
/// the next session — users disable them via the normal toggle instead.
pub static BUILTIN_SKILLS: &[(&str, &str)] = &[(
    "about-biorouter",
    include_str!("builtin_skills/about-biorouter/SKILL.md"),
)];

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct LoadSkillParams {
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SkillMetadata {
    name: String,
    description: String,
}

#[derive(Debug, Clone)]
struct Skill {
    metadata: SkillMetadata,
    body: String,
    directory: PathBuf,
    supporting_files: Vec<PathBuf>,
    bundle_name: Option<String>,
}

pub struct SkillsClient {
    info: InitializeResult,
    skills: HashMap<String, Skill>,
}

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

        Self::ensure_builtin_skills(&Paths::config_dir().join("skills"));

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

    fn get_default_skill_directories() -> Vec<PathBuf> {
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

    fn parse_skill_file(path: &Path, bundle_name: Option<String>) -> Result<Skill> {
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
        })
    }

    fn parse_frontmatter(content: &str) -> Result<(SkillMetadata, String)> {
        let parts: Vec<&str> = content.split("---").collect();

        if parts.len() < 3 {
            return Err(anyhow::anyhow!("Invalid frontmatter format"));
        }

        let yaml_content = parts[1].trim();
        let metadata: SkillMetadata = serde_yaml::from_str(yaml_content)?;

        let body = parts[2..].join("---").trim().to_string();

        Ok((metadata, body))
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

    fn discover_skills_in_directories(directories: &[PathBuf]) -> HashMap<String, Skill> {
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
                        if let Ok(skill) = Self::parse_skill_file(&skill_file, None) {
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

        let disabled = Self::get_disabled_skills();
        let mut skill_list: Vec<_> = self
            .skills
            .iter()
            .filter(|(name, skill)| {
                !disabled.contains(*name)
                    && !skill
                        .bundle_name
                        .as_deref()
                        .is_some_and(|b| disabled.contains(b))
            })
            .collect();

        if skill_list.is_empty() {
            return String::new();
        }

        let mut instructions = String::from(
            "You have these skills at your disposal. When a skill's description matches the user's request, load it with the loadSkill tool before answering rather than guessing — for example, load about-biorouter for questions about Biorouter itself:\n\n"
        );
        skill_list.sort_by_key(|(name, _)| *name);
        for (name, skill) in skill_list {
            instructions.push_str(&format!("- {}: {}\n", name, skill.metadata.description));
        }
        instructions
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
                    "Skill '{}' is currently disabled. Enable it in BioRouter's Skills settings to use it.",
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
        let schema = schema_for!(LoadSkillParams);
        let schema_value =
            serde_json::to_value(schema).expect("Failed to serialize LoadSkillParams schema");

        let input_schema = schema_value
            .as_object()
            .expect("Schema should be an object")
            .clone();

        vec![Tool::new(
            "loadSkill".to_string(),
            indoc! {r#"
                Load a skill by name and return its content.

                This tool loads the specified skill and returns its body content along with
                information about any supporting files in the skill directory.
            "#}
            .to_string(),
            input_schema,
        )
        .annotate(ToolAnnotations {
            title: Some("Load skill".to_string()),
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            idempotent_hint: Some(true),
            open_world_hint: Some(false),
        })]
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

        let skill = SkillsClient::parse_skill_file(&skill_file, None).unwrap();
        assert_eq!(skill.metadata.name, "test-skill");
        assert_eq!(skill.metadata.description, "A test skill");
        assert!(skill.body.contains("# Test Skill Content"));
        assert_eq!(skill.supporting_files.len(), 2);
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
        assert_eq!(result.tools.len(), 1);
        assert_eq!(result.tools[0].name, "loadSkill");
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
        assert!(instructions.contains("You have these skills at your disposal"));
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
        assert!(instructions.contains("alpha-skill: First skill alphabetically"));
        assert!(instructions.contains("beta-skill: Second skill alphabetically"));

        let lines: Vec<&str> = instructions.lines().collect();
        let alpha_line = lines
            .iter()
            .position(|l| l.contains("alpha-skill"))
            .unwrap();
        let beta_line = lines.iter().position(|l| l.contains("beta-skill")).unwrap();
        assert!(alpha_line < beta_line);

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

        std::env::set_var("BIOROUTER_PATH_ROOT", temp_dir.path());
        let dirs = SkillsClient::get_default_skill_directories();
        std::env::remove_var("BIOROUTER_PATH_ROOT");

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

        // First call seeds from scratch.
        SkillsClient::ensure_builtin_skills(&skills_dir);
        let seeded = skills_dir.join("about-biorouter").join("SKILL.md");
        assert!(seeded.exists(), "builtin skill should be seeded");

        // Stale content is refreshed.
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
