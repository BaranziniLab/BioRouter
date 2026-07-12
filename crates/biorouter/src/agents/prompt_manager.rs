#[cfg(test)]
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

use crate::agents::extension::ExtensionInfo;
use crate::hints::load_hints::{load_hint_files, AGENTS_MD_FILENAME, BIOROUTER_HINTS_FILENAME};
use crate::{
    config::{BioRouterMode, Config},
    prompt_template,
    utils::sanitize_unicode_tags,
};
use std::path::Path;

/// Local time at hour granularity, e.g. `2026-07-12 14:00`. Hour (not
/// minute/second) granularity keeps the rendered system prompt byte-identical
/// within the hour so multi-session prompt caching still hits; Local (not UTC)
/// matches the MOIM `<info-msg>` clock so the model never sees two contradictory
/// timezones. Computed fresh at each `build()` (not frozen at construction) so a
/// long-lived agent's clock never goes stale.
fn current_hour_timestamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:00").to_string()
}

pub struct PromptManager {
    system_prompt_override: Option<String>,
    system_prompt_extras: Vec<String>,
    /// When `Some`, pins the rendered clock (deterministic tests). When `None`,
    /// the clock is computed live at `build()` time.
    fixed_timestamp: Option<String>,
}

impl Default for PromptManager {
    fn default() -> Self {
        PromptManager::new()
    }
}

#[derive(Serialize)]
struct SystemPromptContext {
    extensions: Vec<ExtensionInfo>,
    current_date_time: String,
    biorouter_mode: BioRouterMode,
    is_autonomous: bool,
    enable_subagents: bool,
    code_execution_mode: bool,
}

pub struct SystemPromptBuilder<'a, M> {
    manager: &'a M,

    extensions_info: Vec<ExtensionInfo>,
    frontend_instructions: Option<String>,
    subagents_enabled: bool,
    hints: Option<String>,
    code_execution_mode: bool,
}

impl<'a> SystemPromptBuilder<'a, PromptManager> {
    pub fn with_extension(mut self, extension: ExtensionInfo) -> Self {
        self.extensions_info.push(extension);
        self
    }

    pub fn with_extensions(mut self, extensions: impl Iterator<Item = ExtensionInfo>) -> Self {
        for extension in extensions {
            self.extensions_info.push(extension);
        }
        self
    }

    pub fn with_frontend_instructions(mut self, frontend_instructions: Option<String>) -> Self {
        self.frontend_instructions = frontend_instructions;
        self
    }

    pub fn with_code_execution_mode(mut self, enabled: bool) -> Self {
        self.code_execution_mode = enabled;
        self
    }

    pub fn with_hints(mut self, working_dir: &Path) -> Self {
        let config = Config::global();
        let hints_filenames = config
            .get_param::<Vec<String>>("CONTEXT_FILE_NAMES")
            .unwrap_or_else(|_| {
                vec![
                    BIOROUTER_HINTS_FILENAME.to_string(),
                    AGENTS_MD_FILENAME.to_string(),
                ]
            });
        let ignore_patterns = {
            let builder = ignore::gitignore::GitignoreBuilder::new(working_dir);
            builder.build().unwrap_or_else(|_| {
                ignore::gitignore::GitignoreBuilder::new(working_dir)
                    .build()
                    .expect("Failed to build default gitignore")
            })
        };

        let hints = load_hint_files(working_dir, &hints_filenames, &ignore_patterns);

        if !hints.is_empty() {
            self.hints = Some(hints);
        }
        self
    }

    pub fn with_enable_subagents(mut self, subagents_enabled: bool) -> Self {
        self.subagents_enabled = subagents_enabled;
        self
    }

    pub fn build(self) -> String {
        let mut extensions_info = self.extensions_info;

        // Add frontend instructions to extensions_info to simplify json rendering
        if let Some(frontend_instructions) = self.frontend_instructions {
            extensions_info.push(ExtensionInfo::new(
                "frontend",
                &frontend_instructions,
                false,
            ));
        }
        // Stable tool ordering is important for multi session prompt caching.
        extensions_info.sort_by(|a, b| a.name.cmp(&b.name));

        let sanitized_extensions_info: Vec<ExtensionInfo> = extensions_info
            .into_iter()
            .map(|mut ext_info| {
                ext_info.instructions = sanitize_unicode_tags(&ext_info.instructions);
                ext_info
            })
            .collect();

        let config = Config::global();
        let biorouter_mode = config.get_biorouter_mode().unwrap_or(BioRouterMode::Auto);

        let context = SystemPromptContext {
            extensions: sanitized_extensions_info,
            current_date_time: self
                .manager
                .fixed_timestamp
                .clone()
                .unwrap_or_else(current_hour_timestamp),
            biorouter_mode,
            is_autonomous: biorouter_mode == BioRouterMode::Auto,
            enable_subagents: self.subagents_enabled,
            code_execution_mode: self.code_execution_mode,
        };

        let base_prompt = if let Some(override_prompt) = &self.manager.system_prompt_override {
            let sanitized_override_prompt = sanitize_unicode_tags(override_prompt);
            prompt_template::render_inline_once(&sanitized_override_prompt, &context)
        } else {
            prompt_template::render_global_file("system.md", &context)
        }
        .unwrap_or_else(|_| {
            "You are Biorouter, a general-purpose AI agent and integrated research environment for biomedical discovery, created by Wanjun Gu and the Baranzini Lab at UCSF".to_string()
        });

        let mut system_prompt_extras = self.manager.system_prompt_extras.clone();

        // Add hints if provided
        if let Some(hints) = self.hints {
            system_prompt_extras.push(hints);
        }

        if biorouter_mode == BioRouterMode::Chat {
            system_prompt_extras.push(
                "Right now you are in the chat only mode, no access to any tool use and system."
                    .to_string(),
            );
        }

        let sanitized_system_prompt_extras: Vec<String> = system_prompt_extras
            .into_iter()
            .map(|extra| sanitize_unicode_tags(&extra))
            .collect();

        if sanitized_system_prompt_extras.is_empty() {
            base_prompt
        } else {
            format!(
                "{}\n\n# Additional Instructions:\n\n{}",
                base_prompt,
                sanitized_system_prompt_extras.join("\n\n")
            )
        }
    }
}

impl PromptManager {
    pub fn new() -> Self {
        PromptManager {
            system_prompt_override: None,
            system_prompt_extras: Vec::new(),
            // Left unset: the clock is computed live per `build()` (hour
            // granularity) so it stays cache-stable within the hour yet never
            // freezes at agent-construction time.
            fixed_timestamp: None,
        }
    }

    #[cfg(test)]
    pub fn with_timestamp(dt: DateTime<Utc>) -> Self {
        PromptManager {
            system_prompt_override: None,
            system_prompt_extras: Vec::new(),
            fixed_timestamp: Some(dt.format("%Y-%m-%d %H:%M:%S").to_string()),
        }
    }

    /// Add an additional instruction to the system prompt
    pub fn add_system_prompt_extra(&mut self, instruction: String) {
        self.system_prompt_extras.push(instruction);
    }

    /// Override the system prompt with custom text
    pub fn set_system_prompt_override(&mut self, template: String) {
        self.system_prompt_override = Some(template);
    }

    pub fn builder<'a>(&'a self) -> SystemPromptBuilder<'a, Self> {
        SystemPromptBuilder {
            manager: self,

            extensions_info: vec![],
            frontend_instructions: None,
            subagents_enabled: false,
            hints: None,
            code_execution_mode: false,
        }
    }

    pub async fn get_workflow_prompt(&self) -> String {
        let context: HashMap<&str, Value> = HashMap::new();
        prompt_template::render_global_file("workflow.md", &context)
            .unwrap_or_else(|_| "The workflow prompt is busted. Tell the user.".to_string())
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use super::*;

    #[test]
    fn test_build_system_prompt_sanitizes_override() {
        let mut manager = PromptManager::new();
        let malicious_override = "System prompt\u{E0041}\u{E0042}\u{E0043}with hidden text";
        manager.set_system_prompt_override(malicious_override.to_string());

        let result = manager.builder().build();

        assert!(!result.contains('\u{E0041}'));
        assert!(!result.contains('\u{E0042}'));
        assert!(!result.contains('\u{E0043}'));
        assert!(result.contains("System prompt"));
        assert!(result.contains("with hidden text"));
    }

    #[test]
    fn test_build_system_prompt_sanitizes_extras() {
        let mut manager = PromptManager::new();
        let malicious_extra = "Extra instruction\u{E0041}\u{E0042}\u{E0043}hidden";
        manager.add_system_prompt_extra(malicious_extra.to_string());

        let result = manager.builder().build();

        assert!(!result.contains('\u{E0041}'));
        assert!(!result.contains('\u{E0042}'));
        assert!(!result.contains('\u{E0043}'));
        assert!(result.contains("Extra instruction"));
        assert!(result.contains("hidden"));
    }

    #[test]
    fn test_build_system_prompt_sanitizes_multiple_extras() {
        let mut manager = PromptManager::new();
        manager.add_system_prompt_extra("First\u{E0041}instruction".to_string());
        manager.add_system_prompt_extra("Second\u{E0042}instruction".to_string());
        manager.add_system_prompt_extra("Third\u{E0043}instruction".to_string());

        let result = manager.builder().build();

        assert!(!result.contains('\u{E0041}'));
        assert!(!result.contains('\u{E0042}'));
        assert!(!result.contains('\u{E0043}'));
        assert!(result.contains("Firstinstruction"));
        assert!(result.contains("Secondinstruction"));
        assert!(result.contains("Thirdinstruction"));
    }

    #[test]
    fn test_build_system_prompt_preserves_legitimate_unicode_in_extras() {
        let mut manager = PromptManager::new();
        let legitimate_unicode = "Instruction with 世界 and 🌍 emojis";
        manager.add_system_prompt_extra(legitimate_unicode.to_string());

        let result = manager.builder().build();

        assert!(result.contains("世界"));
        assert!(result.contains("🌍"));
        assert!(result.contains("Instruction with"));
        assert!(result.contains("emojis"));
    }

    #[test]
    fn test_build_system_prompt_sanitizes_extension_instructions() {
        let manager = PromptManager::new();
        let malicious_extension_info = ExtensionInfo::new(
            "test_extension",
            "Extension help\u{E0041}\u{E0042}\u{E0043}hidden instructions",
            false,
        );

        let result = manager
            .builder()
            .with_extension(malicious_extension_info)
            .build();

        assert!(!result.contains('\u{E0041}'));
        assert!(!result.contains('\u{E0042}'));
        assert!(!result.contains('\u{E0043}'));
        assert!(result.contains("Extension help"));
        assert!(result.contains("hidden instructions"));
    }

    #[test]
    fn test_basic() {
        let manager = PromptManager::with_timestamp(DateTime::<Utc>::from_timestamp(0, 0).unwrap());

        let system_prompt = manager.builder().build();

        assert_snapshot!(system_prompt)
    }

    /// The live (non-test) clock is computed at `build()` time, at Local hour
    /// granularity — not frozen at construction, not UTC. Robust across an hour
    /// tick by accepting either boundary.
    #[test]
    fn test_live_clock_is_fresh_local_hour() {
        let before = chrono::Local::now().format("%Y-%m-%d %H:00").to_string();
        let prompt = PromptManager::new().builder().build();
        let after = chrono::Local::now().format("%Y-%m-%d %H:00").to_string();

        let expected_before = format!("The current date and time is {before}.");
        let expected_after = format!("The current date and time is {after}.");
        assert!(
            prompt.contains(&expected_before) || prompt.contains(&expected_after),
            "system prompt must render the live Local hour clock (minutes pinned to :00)"
        );
    }

    #[test]
    fn test_one_extension() {
        let manager = PromptManager::with_timestamp(DateTime::<Utc>::from_timestamp(0, 0).unwrap());

        let system_prompt = manager
            .builder()
            .with_extension(ExtensionInfo::new(
                "test",
                "how to use this extension",
                true,
            ))
            .build();

        assert_snapshot!(system_prompt)
    }

    #[test]
    fn test_typical_setup() {
        let manager = PromptManager::with_timestamp(DateTime::<Utc>::from_timestamp(0, 0).unwrap());

        let system_prompt = manager
            .builder()
            .with_extension(ExtensionInfo::new(
                "extension_A",
                "<instructions on how to use extension A>",
                true,
            ))
            .with_extension(ExtensionInfo::new(
                "extension_B",
                "<instructions on how to use extension B (no resources)>",
                false,
            ))
            .build();

        assert_snapshot!(system_prompt)
    }

    /// Contract test for the agentic-behavior clauses added to `system.md`.
    /// Each assertion guards one intentional instruction against silent
    /// removal/regression. These are the table-stakes behaviors the prompt
    /// review found missing; if any of these strings disappears, the agent
    /// quietly loses the behavior.
    #[test]
    fn test_system_prompt_has_behavior_clauses() {
        let manager = PromptManager::with_timestamp(DateTime::<Utc>::from_timestamp(0, 0).unwrap());
        let p = manager.builder().build();

        // Date is actually rendered (was computed-but-unused before).
        assert!(
            p.contains("The current date and time is 1970-01-01 00:00:00"),
            "current_date_time must render so the agent can judge what is 'recent'"
        );
        // Conciseness budget.
        assert!(p.contains("Be concise."), "missing conciseness clause");
        assert!(
            p.to_lowercase().contains("preamble"),
            "missing preamble/postamble ban"
        );
        // Tool-use discipline.
        assert!(
            p.contains("run in parallel"),
            "missing parallel-tool-call guidance"
        );
        assert!(
            p.contains("don't expose internal tool names"),
            "missing never-name-tools rule"
        );
        // Working-on-tasks discipline.
        assert!(
            p.contains("Before editing a file, read the relevant parts"),
            "missing read-before-edit rule"
        );
        assert!(
            p.contains("not surprising the user"),
            "missing proactiveness/don't-surprise balance"
        );
        // Planning/todo discipline (moved here from the todo extension MOIM).
        assert!(
            p.contains("plan before acting"),
            "missing planning/todo discipline"
        );
        // Verification-before-completion discipline.
        assert!(
            p.contains("Before reporting a task complete, verify it"),
            "missing verify-before-done discipline"
        );
        // Safety posture (including the biomedical-accuracy clause).
        assert!(
            p.contains("Never expose, log, or commit secrets"),
            "missing secrets rule"
        );
        assert!(
            p.contains("biomedical and scientific claims"),
            "missing biomedical-accuracy/anti-fabrication clause"
        );
        // Output conventions.
        assert!(
            p.contains("file_path:line_number"),
            "missing code-reference citation convention"
        );
    }

    /// The pillar-awareness paragraph (about-biorouter + Soul) must render only
    /// when extensions are present, since it points at the skills/knowledge
    /// tools. With no extensions it must NOT appear (those tools aren't there).
    #[test]
    fn test_pillar_awareness_is_conditional_on_extensions() {
        let manager = PromptManager::with_timestamp(DateTime::<Utc>::from_timestamp(0, 0).unwrap());

        let with_ext = manager
            .builder()
            .with_extension(ExtensionInfo::new("developer", "dev instructions", false))
            .build();
        assert!(
            with_ext.contains("about-biorouter") && with_ext.contains("Soul"),
            "pillar/Soul awareness must appear when extensions are loaded"
        );

        let without_ext = manager.builder().build();
        assert!(
            !without_ext.contains("about-biorouter"),
            "pillar awareness must not appear when no extensions are loaded"
        );
    }
}
