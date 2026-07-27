#[cfg(test)]
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

use crate::agents::extension::ExtensionInfo;
use crate::context_budget::{
    fit_context_blocks, injection_budget_tokens, BudgetReport, ContextBlock,
};
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

/// BR-3: which system-prompt variant to render for the active model.
///
/// One fixed `system.md` served 43+ providers of wildly varying capability:
/// strong models paid for scaffolding they don't need, and small/local models
/// got too little. Rather than the "one prompt file per model" sprawl the
/// review warned against, variants are kept intentionally minimal — a shared
/// base (`system.md`, the strong-model default) plus at most one small overlay.
/// The `Default` variant renders the base byte-identically, so strong models
/// (and their multi-session prompt cache key) are unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptVariant {
    /// Strong commercial / institution-hosted models. Base `system.md` only.
    Default,
    /// Small, weak local models (Llama Server, Ollama). Base + a compact
    /// scaffolding overlay (`system_small_local.md`).
    SmallLocal,
}

impl PromptVariant {
    /// Choose a variant for `(provider_name, model_name)`.
    ///
    /// `BIOROUTER_SYSTEM_PROMPT_VARIANT` (`default` | `small_local`) pins the
    /// choice for testing / power users; otherwise the provider/model-keyed
    /// table decides, defaulting to [`PromptVariant::Default`].
    pub fn select(provider_name: &str, model_name: &str) -> PromptVariant {
        if let Ok(pinned) = Config::global().get_param::<String>("BIOROUTER_SYSTEM_PROMPT_VARIANT")
        {
            match pinned.trim().to_ascii_lowercase().as_str() {
                "default" | "strong" => return PromptVariant::Default,
                "small_local" | "small" | "local" => return PromptVariant::SmallLocal,
                other => tracing::warn!(
                    variant = other,
                    "unknown BIOROUTER_SYSTEM_PROMPT_VARIANT; using the model-derived variant"
                ),
            }
        }
        select_variant_from_table(provider_name, model_name)
    }
}

/// Provider/model → variant rules, first match wins, `Default` as the fallback.
/// Kept deliberately tiny (BR-3: "keep variants minimal / avoid sprawl").
///
/// The local providers ship small, weak models by default (Llama Server's
/// Qwen3.5-4B / Gemma-4, Ollama's local tags), so they get the extra
/// scaffolding — except when the model name says a *large* model is loaded
/// locally, which needs no hand-holding.
fn select_variant_from_table(provider_name: &str, model_name: &str) -> PromptVariant {
    let provider = provider_name.to_ascii_lowercase();
    let model = model_name.to_ascii_lowercase();

    if matches!(provider.as_str(), "llamacpp" | "ollama") {
        const LARGE_LOCAL_MARKERS: &[&str] = &["70b", "72b", "65b", "34b", "large"];
        if LARGE_LOCAL_MARKERS.iter().any(|m| model.contains(m)) {
            return PromptVariant::Default;
        }
        return PromptVariant::SmallLocal;
    }

    PromptVariant::Default
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
    variant: PromptVariant,
}

impl<'a> SystemPromptBuilder<'a, PromptManager> {
    /// BR-3: select the per-model prompt variant (default: strong-model base).
    pub fn with_prompt_variant(mut self, variant: PromptVariant) -> Self {
        self.variant = variant;
        self
    }

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

        let mut sanitized_extensions_info: Vec<ExtensionInfo> = extensions_info
            .into_iter()
            .map(|mut ext_info| {
                ext_info.instructions = sanitize_unicode_tags(&ext_info.instructions);
                ext_info
            })
            .collect();

        // BR-2: bound the aggregate size of injected context (extension
        // instructions + hint files) so a chatty MCP server or a runaway
        // `AGENTS.md` can't silently blow the window. The base `system.md` and
        // the app-supplied extras (desktop/CLI prompt) are trusted and small, so
        // they stay pinned outside the budget.
        let mut hints = self.hints;
        let report = apply_injection_budget(
            &mut sanitized_extensions_info,
            &mut hints,
            injection_budget_tokens(),
        );
        if !report.is_empty() {
            tracing::warn!(
                dropped = ?report.dropped,
                truncated = ?report.truncated,
                "context budget: trimmed injected system-prompt blocks to fit CONTEXT_INJECTION_BUDGET_TOKENS"
            );
        }

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

        let mut base_prompt = if let Some(override_prompt) = &self.manager.system_prompt_override {
            let sanitized_override_prompt = sanitize_unicode_tags(override_prompt);
            prompt_template::render_inline_once(&sanitized_override_prompt, &context)
        } else {
            prompt_template::render_global_file("system.md", &context)
        }
        .unwrap_or_else(|_| {
            "You are Biorouter, a general-purpose AI agent and integrated research environment for biomedical discovery, created by Wanjun Gu and the Baranzini Lab at UCSF".to_string()
        });

        // BR-3: small/weak local models get a compact scaffolding overlay on top
        // of the shared base prompt. The `Default` variant appends nothing, so a
        // strong model's rendered prompt (and its cache key) stays byte-identical.
        // Skipped under a full custom prompt override, which is already complete.
        if self.variant == PromptVariant::SmallLocal
            && self.manager.system_prompt_override.is_none()
        {
            match prompt_template::render_global_file("system_small_local.md", &context) {
                Ok(overlay) if !overlay.trim().is_empty() => {
                    base_prompt.push_str("\n\n");
                    base_prompt.push_str(&overlay);
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!("failed to render small-local prompt overlay: {e}")
                }
            }
        }

        let mut system_prompt_extras = self.manager.system_prompt_extras.clone();

        // Add hints if provided (post-budget)
        if let Some(hints) = hints {
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
            variant: PromptVariant::Default,
        }
    }

    pub async fn get_workflow_prompt(&self) -> String {
        let context: HashMap<&str, Value> = HashMap::new();
        prompt_template::render_global_file("workflow.md", &context)
            .unwrap_or_else(|_| "The workflow prompt is busted. Tell the user.".to_string())
    }
}

/// Apply the BR-2 injection budget across the injected system-prompt blocks —
/// the MCP extension instructions and the hint files — mutating each in place
/// (truncating or emptying). Extension instructions rank above hints: teaching
/// the model how to call a tool matters more than project hints, so hints are
/// trimmed first and instructions only if the servers alone exceed the budget.
/// `budget_tokens == 0` disables the cap. Returns what was trimmed for logging.
fn apply_injection_budget(
    extensions: &mut [ExtensionInfo],
    hints: &mut Option<String>,
    budget_tokens: usize,
) -> BudgetReport {
    if budget_tokens == 0 {
        return BudgetReport::default();
    }

    let has_hints = hints.is_some();
    let mut blocks: Vec<ContextBlock> = extensions
        .iter()
        .map(|ext| ContextBlock {
            label: format!("extension:{}", ext.name),
            content: ext.instructions.clone(),
            priority: 100,
        })
        .collect();
    if let Some(h) = hints.as_ref() {
        blocks.push(ContextBlock {
            label: "hints".to_string(),
            content: h.clone(),
            priority: 50,
        });
    }

    let (fitted, report) = fit_context_blocks(blocks, budget_tokens);
    if report.is_empty() {
        // Fast path: nothing changed, avoid rewriting every field.
        return report;
    }

    // `fitted` preserves input order: the first `extensions.len()` entries are
    // the extension blocks, followed by the optional hints block.
    for (ext, fitted_block) in extensions.iter_mut().zip(fitted.iter()) {
        ext.instructions = fitted_block.content.clone();
    }
    if has_hints {
        let fitted_hints = &fitted[extensions.len()].content;
        *hints = if fitted_hints.is_empty() {
            None
        } else {
            Some(fitted_hints.clone())
        };
    }

    report
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
        // #44: with the session working dir immutable, printed file references
        // must be absolute (or ~-relative) so the artifact previewer can
        // always resolve them — a bare relative path only resolves against
        // whatever directory happens to be current when it is read.
        assert!(
            p.contains("never a bare relative path"),
            "missing absolute-path rule for printed file references (#44)"
        );
        // Tool-routing discipline (prefer primitives; keep basic file/shell ops
        // off code-execution). Renders in every mode, including code-execution
        // mode where per-extension instructions are hidden.
        assert!(p.contains("# Tool Routing"), "missing Tool Routing section");
        assert!(
            p.contains("Prefer the simplest tool that does the job"),
            "missing prefer-the-simplest-tool rule"
        );
        assert!(
            p.contains("Use a code-execution tool ONLY when"),
            "missing code-execution-only-for-computation rule"
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

    /// BR-2: with a generous budget, injected blocks are left byte-identical and
    /// the report is empty — ordinary sessions are unaffected.
    #[test]
    fn test_injection_budget_noop_under_budget() {
        let mut extensions = vec![
            ExtensionInfo::new("a", "short instructions", false),
            ExtensionInfo::new("b", "more instructions", false),
        ];
        let mut hints = Some("some project hints".to_string());
        let report = apply_injection_budget(&mut extensions, &mut hints, 10_000);
        assert!(report.is_empty());
        assert_eq!(extensions[0].instructions, "short instructions");
        assert_eq!(extensions[1].instructions, "more instructions");
        assert_eq!(hints.as_deref(), Some("some project hints"));
    }

    /// BR-2: hints (lower priority) are dropped before extension instructions
    /// (higher priority) when the budget is tight.
    #[test]
    fn test_injection_budget_drops_hints_before_instructions() {
        let big_instructions = "i".repeat(3_000); // ~750 tokens
        let big_hints = "h".repeat(3_000);
        let mut extensions = vec![ExtensionInfo::new("dev", &big_instructions, false)];
        let mut hints = Some(big_hints.clone());

        let report = apply_injection_budget(&mut extensions, &mut hints, 800);

        assert_eq!(
            extensions[0].instructions, big_instructions,
            "extension instructions must be kept in full"
        );
        assert_eq!(hints, None, "hints must be dropped to fit the budget");
        assert!(report.dropped.iter().any(|l| l == "hints"));
    }

    /// BR-2: a budget of 0 disables the cap entirely.
    #[test]
    fn test_injection_budget_disabled_with_zero() {
        let big = "x".repeat(100_000);
        let mut extensions = vec![ExtensionInfo::new("dev", &big, false)];
        let mut hints = Some(big.clone());
        let report = apply_injection_budget(&mut extensions, &mut hints, 0);
        assert!(report.is_empty());
        assert_eq!(extensions[0].instructions, big);
        assert_eq!(hints.as_deref(), Some(big.as_str()));
    }

    /// BR-3: the provider/model table routes the local providers to the
    /// small-local variant and everything else to the strong-model default.
    #[test]
    fn test_prompt_variant_table() {
        // Local providers → small-local scaffolding.
        assert_eq!(
            select_variant_from_table("llamacpp", "qwen3.5-4b"),
            PromptVariant::SmallLocal
        );
        assert_eq!(
            select_variant_from_table("ollama", "gemma4:latest"),
            PromptVariant::SmallLocal
        );
        // Strong commercial / institution-hosted → base prompt only.
        assert_eq!(
            select_variant_from_table("anthropic", "claude-sonnet-4"),
            PromptVariant::Default
        );
        assert_eq!(
            select_variant_from_table("openai", "gpt-5.2"),
            PromptVariant::Default
        );
        // A *large* model run locally does not need the extra hand-holding.
        assert_eq!(
            select_variant_from_table("ollama", "llama3.3:70b"),
            PromptVariant::Default
        );
    }

    /// BR-3: the small-local variant appends the scaffolding overlay, while the
    /// default variant renders the base prompt byte-identically (no overlay).
    #[test]
    fn test_small_local_variant_appends_overlay() {
        let manager = PromptManager::with_timestamp(DateTime::<Utc>::from_timestamp(0, 0).unwrap());

        let default_prompt = manager
            .builder()
            .with_prompt_variant(PromptVariant::Default)
            .build();
        assert!(
            !default_prompt.contains("Running on a Smaller Model"),
            "the default (strong-model) variant must not append the overlay"
        );

        let small_prompt = manager
            .builder()
            .with_prompt_variant(PromptVariant::SmallLocal)
            .build();
        assert!(
            small_prompt.starts_with(&default_prompt),
            "the small-local variant is the shared base plus an appended overlay"
        );
        assert!(
            small_prompt.len() > default_prompt.len(),
            "the small-local variant must add scaffolding on top of the base"
        );
    }

    /// BR-3 contract test: guard the small-local overlay's intentional clauses
    /// against silent removal (mirrors `test_system_prompt_has_behavior_clauses`
    /// for the base prompt). If any of these disappears, the weak-model
    /// scaffolding is quietly lost.
    #[test]
    fn test_small_local_overlay_has_scaffolding_clauses() {
        let manager = PromptManager::with_timestamp(DateTime::<Utc>::from_timestamp(0, 0).unwrap());
        let p = manager
            .builder()
            .with_prompt_variant(PromptVariant::SmallLocal)
            .build();

        assert!(
            p.contains("You are running on a smaller local model"),
            "missing small-model self-identification"
        );
        assert!(
            p.contains("single tool call per turn"),
            "missing one-step-at-a-time discipline"
        );
        assert!(
            p.contains("emit only valid JSON for tool arguments"),
            "missing valid-tool-JSON discipline"
        );
        assert!(
            p.contains("ask a short clarifying question"),
            "missing ask-vs-act discipline"
        );
        assert!(
            p.contains("Before saying a task is done"),
            "missing verify-before-done discipline"
        );
        assert!(
            p.contains("Prefer the simplest tool for the job"),
            "missing small-local tool-routing discipline"
        );
    }

    /// BR-3: a full custom prompt override is already complete, so the overlay
    /// is not appended even for the small-local variant.
    #[test]
    fn test_small_local_variant_skips_overlay_under_override() {
        let mut manager = PromptManager::new();
        manager.set_system_prompt_override("Custom prompt for a workflow.".to_string());

        let p = manager
            .builder()
            .with_prompt_variant(PromptVariant::SmallLocal)
            .build();

        assert!(p.contains("Custom prompt for a workflow."));
        assert!(
            !p.contains("Running on a Smaller Model"),
            "the overlay must not be appended to a custom prompt override"
        );
    }
}
