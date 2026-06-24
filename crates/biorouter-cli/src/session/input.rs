use super::completion::BioRouterCompleter;
use anyhow::Result;
use biorouter::config::Config;
use rustyline::Editor;
use shlex;
use std::collections::HashMap;

#[derive(Debug)]
pub enum InputResult {
    Message(String),
    Exit,
    AddExtension(String),
    AddBuiltin(String),
    ToggleTheme,
    SelectTheme(String),
    Retry,
    ListPrompts(Option<String>),
    PromptCommand(PromptCommandOptions),
    BioRouterMode(String),
    Plan(PlanCommandOptions),
    EndPlan,
    Clear,
    Workflow(Option<String>),
    Compact,
    ToggleFullToolOutput,
    /// Branch the current conversation into a brand-new session (full history
    /// preserved) and open it in a fresh BioRouter desktop window.
    Diverge,
}

#[derive(Debug)]
pub struct PromptCommandOptions {
    pub name: String,
    pub info: bool,
    pub arguments: HashMap<String, String>,
}

#[derive(Debug)]
pub struct PlanCommandOptions {
    pub message_text: String,
}

struct CtrlCHandler;

impl rustyline::ConditionalEventHandler for CtrlCHandler {
    /// Handle Ctrl+C to clear the line if text is entered, otherwise exit the session.
    fn handle(
        &self,
        _event: &rustyline::Event,
        _n: usize,
        _positive: bool,
        ctx: &rustyline::EventContext,
    ) -> Option<rustyline::Cmd> {
        if !ctx.line().is_empty() {
            Some(rustyline::Cmd::Kill(rustyline::Movement::WholeBuffer))
        } else {
            Some(rustyline::Cmd::Interrupt)
        }
    }
}

pub fn get_newline_key() -> char {
    Config::global()
        .get_param::<String>("BIOROUTER_CLI_NEWLINE_KEY")
        .ok()
        .and_then(|s| s.chars().next())
        .map(|c| c.to_ascii_lowercase())
        .unwrap_or('j')
}

pub fn get_input(
    editor: &mut Editor<BioRouterCompleter, rustyline::history::DefaultHistory>,
) -> Result<InputResult> {
    let newline_key = get_newline_key();
    editor.bind_sequence(
        rustyline::KeyEvent(
            rustyline::KeyCode::Char(newline_key),
            rustyline::Modifiers::CTRL,
        ),
        rustyline::EventHandler::Simple(rustyline::Cmd::Newline),
    );

    editor.bind_sequence(
        rustyline::KeyEvent(rustyline::KeyCode::Char('c'), rustyline::Modifiers::CTRL),
        rustyline::EventHandler::Conditional(Box::new(CtrlCHandler)),
    );

    let prompt = get_input_prompt_string();

    let input = match editor.readline(&prompt) {
        Ok(text) => text,
        Err(e) => match e {
            rustyline::error::ReadlineError::Interrupted => return Ok(InputResult::Exit),
            rustyline::error::ReadlineError::Eof => return Ok(InputResult::Exit),
            _ => return Err(e.into()),
        },
    };

    // Add valid input to history (history saving to file is handled in the Session::interactive method)
    if !input.trim().is_empty() {
        editor.add_history_entry(input.as_str())?;
    }

    // Handle non-slash commands first
    if !input.starts_with('/') {
        let trimmed = input.trim();
        if trimmed.is_empty()
            || trimmed.eq_ignore_ascii_case("exit")
            || trimmed.eq_ignore_ascii_case("quit")
        {
            return Ok(if trimmed.is_empty() {
                InputResult::Retry
            } else {
                InputResult::Exit
            });
        }
        return Ok(InputResult::Message(trimmed.to_string()));
    }

    // Handle slash commands
    match handle_slash_command(&input) {
        Some(result) => Ok(result),
        None => Ok(InputResult::Message(input.trim().to_string())),
    }
}

fn handle_slash_command(input: &str) -> Option<InputResult> {
    let input = input.trim();

    // Command prefix constants
    const CMD_PROMPTS: &str = "/prompts ";
    const CMD_PROMPT: &str = "/prompt";
    const CMD_PROMPT_WITH_SPACE: &str = "/prompt ";
    const CMD_EXTENSION: &str = "/extension ";
    const CMD_BUILTIN: &str = "/builtin ";
    const CMD_MODE: &str = "/mode ";
    const CMD_PLAN: &str = "/plan";
    const CMD_ENDPLAN: &str = "/endplan";
    const CMD_CLEAR: &str = "/clear";
    const CMD_WORKFLOW: &str = "/workflow";
    const CMD_COMPACT: &str = "/compact";
    const CMD_SUMMARIZE_DEPRECATED: &str = "/summarize";

    match input {
        "/exit" | "/quit" => Some(InputResult::Exit),
        "/?" | "/help" => {
            print_help();
            Some(InputResult::Retry)
        }
        "/t" => Some(InputResult::ToggleTheme),
        s if s.starts_with("/t ") => {
            let t = s
                .strip_prefix("/t ")
                .unwrap_or_default()
                .trim()
                .to_lowercase();
            if ["light", "dark", "ansi"].contains(&t.as_str()) {
                Some(InputResult::SelectTheme(t))
            } else {
                println!(
                    "Theme Unavailable: {} Available themes are: light, dark, ansi",
                    t
                );
                Some(InputResult::Retry)
            }
        }
        "/prompts" => Some(InputResult::ListPrompts(None)),
        s if s.starts_with(CMD_PROMPTS) => {
            // Parse arguments for /prompts command
            let args = s.strip_prefix(CMD_PROMPTS).unwrap_or_default();
            parse_prompts_command(args)
        }
        s if s.starts_with(CMD_PROMPT) => {
            if s == CMD_PROMPT {
                // No arguments case
                Some(InputResult::PromptCommand(PromptCommandOptions {
                    name: String::new(), // Empty name will trigger the error message in the rendering
                    info: false,
                    arguments: HashMap::new(),
                }))
            } else if let Some(stripped) = s.strip_prefix(CMD_PROMPT_WITH_SPACE) {
                // Has arguments case
                parse_prompt_command(stripped)
            } else {
                // Handle invalid cases like "/promptxyz"
                None
            }
        }
        s if s.starts_with(CMD_EXTENSION) => Some(InputResult::AddExtension(
            s.get(CMD_EXTENSION.len()..).unwrap_or("").to_string(),
        )),
        s if s.starts_with(CMD_BUILTIN) => Some(InputResult::AddBuiltin(
            s.get(CMD_BUILTIN.len()..).unwrap_or("").to_string(),
        )),
        s if s.starts_with(CMD_MODE) => Some(InputResult::BioRouterMode(
            s.get(CMD_MODE.len()..).unwrap_or("").to_string(),
        )),
        s if s.starts_with(CMD_PLAN) => {
            parse_plan_command(s.get(CMD_PLAN.len()..).unwrap_or("").trim().to_string())
        }
        s if s == CMD_ENDPLAN => Some(InputResult::EndPlan),
        s if s == CMD_CLEAR => Some(InputResult::Clear),
        "/diverge" => Some(InputResult::Diverge),
        s if s.starts_with(CMD_WORKFLOW) => parse_workflow_command(s),
        s if s == CMD_COMPACT => Some(InputResult::Compact),
        s if s == CMD_SUMMARIZE_DEPRECATED => {
            println!("{}", console::style("Note: /summarize has been renamed to /compact and will be removed in a future release.").yellow());
            Some(InputResult::Compact)
        }
        "/r" => Some(InputResult::ToggleFullToolOutput),
        _ => None,
    }
}

fn parse_workflow_command(s: &str) -> Option<InputResult> {
    const CMD_WORKFLOW: &str = "/workflow";

    if s == CMD_WORKFLOW {
        // No filepath provided, use default
        return Some(InputResult::Workflow(None));
    }

    // Extract the filepath from the command
    let filepath = s.get(CMD_WORKFLOW.len()..).unwrap_or("").trim();

    if filepath.is_empty() {
        return Some(InputResult::Workflow(None));
    }

    // Validate that the filepath ends with .yaml
    if !filepath.to_lowercase().ends_with(".yaml") {
        println!("{}", console::style("Filepath must end with .yaml").red());
        return Some(InputResult::Retry);
    }

    // Return the filepath for validation in the handler
    Some(InputResult::Workflow(Some(filepath.to_string())))
}

fn parse_prompts_command(args: &str) -> Option<InputResult> {
    let parts: Vec<String> = shlex::split(args).unwrap_or_default();

    // Look for --extension flag
    for i in 0..parts.len() {
        if parts[i] == "--extension" && i + 1 < parts.len() {
            // Return the extension name that follows the flag
            return Some(InputResult::ListPrompts(Some(parts[i + 1].clone())));
        }
    }

    // If we got here, there was no valid --extension flag
    Some(InputResult::ListPrompts(None))
}

fn parse_prompt_command(args: &str) -> Option<InputResult> {
    let parts: Vec<String> = shlex::split(args).unwrap_or_default();

    // set name to empty and error out in the rendering
    let mut options = PromptCommandOptions {
        name: parts.first().cloned().unwrap_or_default(),
        info: false,
        arguments: HashMap::new(),
    };

    // handle info at any point in the command
    if parts.iter().any(|part| part == "--info") {
        options.info = true;
    }

    // Parse remaining arguments
    let mut i = 1;

    while i < parts.len() {
        let part = &parts[i];

        // Skip flag arguments
        if part == "--info" {
            i += 1;
            continue;
        }

        // Process key=value pairs - removed redundant contains check
        if let Some((key, value)) = part.split_once('=') {
            options.arguments.insert(key.to_string(), value.to_string());
        }

        i += 1;
    }

    Some(InputResult::PromptCommand(options))
}

fn parse_plan_command(input: String) -> Option<InputResult> {
    let options = PlanCommandOptions {
        message_text: input.trim().to_string(),
    };

    Some(InputResult::Plan(options))
}

/// Generates the input prompt string for the CLI interface.
fn get_input_prompt_string() -> String {
    // The brand warm tan-brown accent (xterm-256 137 ≈ #af875f), Biorouter's light cream palette
    const ACCENT: console::Color = console::Color::Color256(137);
    if cfg!(target_os = "windows") {
        "Biorouter> ".to_string()
    } else {
        format!(
            "{} {} ",
            console::style("Biorouter").fg(ACCENT).bold(),
            console::style("❯").fg(ACCENT)
        )
    }
}

fn print_help() {
    use console::{style, Color};
    const ACCENT: Color = Color::Color256(137);

    // (command, description) pairs rendered as an aligned two-column table.
    let commands: &[(&str, &str)] = &[
        ("/exit, /quit", "Exit the session"),
        ("/t", "Toggle Light / Dark / Ansi theme"),
        ("/t <name>", "Set theme directly (light, dark, ansi)"),
        ("/r", "Toggle full (untruncated) tool output"),
        (
            "/extension <cmd>",
            "Add a stdio extension (ENV1=val1 command args…)",
        ),
        (
            "/builtin <names>",
            "Add builtin extensions by name (comma-separated)",
        ),
        (
            "/prompts [--extension <name>]",
            "List available prompts, optionally filtered",
        ),
        (
            "/prompt <name> [--info] [k=v…]",
            "Show prompt info or execute a prompt",
        ),
        (
            "/mode <name>",
            "Set mode: auto, approve, chat, smart_approve",
        ),
        (
            "/plan [message]",
            "Enter plan mode, then optionally act on the plan",
        ),
        ("/endplan", "Exit plan mode, return to normal mode"),
        (
            "/workflow [file.yaml]",
            "Save the conversation as a workflow",
        ),
        ("/compact", "Compact the conversation to reclaim context"),
        ("/clear", "Clear the current chat history"),
        (
            "/diverge",
            "Branch this conversation into a new BioRouter window (keeps full history)",
        ),
        (
            "/goal <condition>",
            "Keep working until the condition is met (/goal clear to stop)",
        ),
        (
            "/loop <interval> <prompt>",
            "Run a prompt on an interval, e.g. /loop 5m … (/loop stop <id>)",
        ),
        (
            "/schedule <spec> <prompt>",
            "Schedule a recurring prompt: 5m, @daily, or a quoted cron",
        ),
        ("/help, /?", "Show this help message"),
    ];

    let width = commands.iter().map(|(c, _)| c.len()).max().unwrap_or(0);

    println!();
    println!("  {} {}", style("▌").fg(ACCENT), style("Commands").bold());
    for (cmd, desc) in commands {
        println!(
            "    {:<width$}   {}",
            style(cmd).fg(ACCENT),
            style(desc).dim(),
            width = width
        );
    }

    let newline_key = get_newline_key().to_ascii_uppercase();
    let nav: &[(&str, &str)] = &[
        ("Ctrl+C", "Clear the current line, or exit when empty"),
        (
            "Ctrl+_KEY_",
            "Insert a newline (set via BIOROUTER_CLI_NEWLINE_KEY)",
        ),
        ("↑ / ↓", "Navigate command history"),
    ];

    println!();
    println!("  {} {}", style("▌").fg(ACCENT), style("Navigation").bold());
    for (key, desc) in nav {
        let key = key.replace("_KEY_", &newline_key.to_string());
        println!(
            "    {:<width$}   {}",
            style(key).fg(ACCENT),
            style(desc).dim(),
            width = width
        );
    }

    // Pointers to the shell-level management subcommands (run outside a session).
    let shell: &[(&str, &str)] = &[
        (
            "biorouter knowledge",
            "Manage knowledge bases — ingest, lint, query",
        ),
        (
            "biorouter extension",
            "Install extensions from .brxt bundles",
        ),
        ("biorouter skill", "Install skills from .zip files"),
        (
            "biorouter workflow",
            "Install and run workflows (.json / .yaml)",
        ),
        ("biorouter models", "Inspect and set the provider / model"),
        ("biorouter schedule", "Manage scheduled jobs"),
    ];
    println!();
    println!(
        "  {} {} {}",
        style("▌").fg(ACCENT),
        style("Shell commands").bold(),
        style("(run from your terminal)").dim()
    );
    let shell_width = shell.iter().map(|(c, _)| c.len()).max().unwrap_or(0);
    for (cmd, desc) in shell {
        println!(
            "    {:<shell_width$}   {}",
            style(cmd).fg(ACCENT),
            style(desc).dim(),
            shell_width = shell_width
        );
    }
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_slash_command() {
        // Test exit commands
        assert!(matches!(
            handle_slash_command("/exit"),
            Some(InputResult::Exit)
        ));
        assert!(matches!(
            handle_slash_command("/quit"),
            Some(InputResult::Exit)
        ));

        // Test help commands
        assert!(matches!(
            handle_slash_command("/help"),
            Some(InputResult::Retry)
        ));
        assert!(matches!(
            handle_slash_command("/?"),
            Some(InputResult::Retry)
        ));

        // Test theme toggle
        assert!(matches!(
            handle_slash_command("/t"),
            Some(InputResult::ToggleTheme)
        ));

        // Test full tool output toggle
        assert!(matches!(
            handle_slash_command("/r"),
            Some(InputResult::ToggleFullToolOutput)
        ));

        // Test extension command
        if let Some(InputResult::AddExtension(cmd)) = handle_slash_command("/extension foo bar") {
            assert_eq!(cmd, "foo bar");
        } else {
            panic!("Expected AddExtension");
        }

        // Test builtin command
        if let Some(InputResult::AddBuiltin(names)) = handle_slash_command("/builtin dev,git") {
            assert_eq!(names, "dev,git");
        } else {
            panic!("Expected AddBuiltin");
        }

        // Test diverge command
        assert!(matches!(
            handle_slash_command("/diverge"),
            Some(InputResult::Diverge)
        ));
        assert!(matches!(
            handle_slash_command("  /diverge  "),
            Some(InputResult::Diverge)
        ));
        assert!(handle_slash_command("/divergent").is_none());
        assert!(handle_slash_command("/diverge now").is_none());

        // Test unknown commands
        assert!(handle_slash_command("/unknown").is_none());
    }

    #[test]
    fn test_diverge_is_in_completion_registry() {
        // Keep the parser and the autocomplete list in sync.
        assert!(super::super::completion::SLASH_COMMANDS.contains(&"/diverge"));
    }

    #[test]
    fn test_prompts_command() {
        // Test basic prompts command
        if let Some(InputResult::ListPrompts(extension)) = handle_slash_command("/prompts") {
            assert!(extension.is_none());
        } else {
            panic!("Expected ListPrompts");
        }

        // Test prompts with extension filter
        if let Some(InputResult::ListPrompts(extension)) =
            handle_slash_command("/prompts --extension test")
        {
            assert_eq!(extension, Some("test".to_string()));
        } else {
            panic!("Expected ListPrompts with extension");
        }
    }

    #[test]
    fn test_prompt_command() {
        // Test basic prompt info command
        if let Some(InputResult::PromptCommand(opts)) =
            handle_slash_command("/prompt test-prompt --info")
        {
            assert_eq!(opts.name, "test-prompt");
            assert!(opts.info);
            assert!(opts.arguments.is_empty());
        } else {
            panic!("Expected PromptCommand");
        }

        // Test prompt with arguments
        if let Some(InputResult::PromptCommand(opts)) =
            handle_slash_command("/prompt test-prompt arg1=val1 arg2=val2")
        {
            assert_eq!(opts.name, "test-prompt");
            assert!(!opts.info);
            assert_eq!(opts.arguments.len(), 2);
            assert_eq!(opts.arguments.get("arg1"), Some(&"val1".to_string()));
            assert_eq!(opts.arguments.get("arg2"), Some(&"val2".to_string()));
        } else {
            panic!("Expected PromptCommand");
        }
    }

    // Test whitespace handling
    #[test]
    fn test_whitespace_handling() {
        // Leading/trailing whitespace in extension command
        if let Some(InputResult::AddExtension(cmd)) = handle_slash_command("  /extension foo bar  ")
        {
            assert_eq!(cmd, "foo bar");
        } else {
            panic!("Expected AddExtension");
        }

        // Leading/trailing whitespace in builtin command
        if let Some(InputResult::AddBuiltin(names)) = handle_slash_command("  /builtin dev,git  ") {
            assert_eq!(names, "dev,git");
        } else {
            panic!("Expected AddBuiltin");
        }
    }

    // Test prompt with no arguments
    #[test]
    fn test_prompt_no_args() {
        // Test just "/prompt" with no arguments
        if let Some(InputResult::PromptCommand(opts)) = handle_slash_command("/prompt") {
            assert_eq!(opts.name, "");
            assert!(!opts.info);
            assert!(opts.arguments.is_empty());
        } else {
            panic!("Expected PromptCommand");
        }

        // Test invalid prompt command
        assert!(handle_slash_command("/promptxyz").is_none());
    }

    // Test quoted arguments
    #[test]
    fn test_quoted_arguments() {
        // Test prompt with quoted arguments
        if let Some(InputResult::PromptCommand(opts)) = handle_slash_command(
            r#"/prompt test-prompt arg1="value with spaces" arg2="another value""#,
        ) {
            assert_eq!(opts.name, "test-prompt");
            assert_eq!(opts.arguments.len(), 2);
            assert_eq!(
                opts.arguments.get("arg1"),
                Some(&"value with spaces".to_string())
            );
            assert_eq!(
                opts.arguments.get("arg2"),
                Some(&"another value".to_string())
            );
        } else {
            panic!("Expected PromptCommand");
        }

        // Test prompt with mixed quoted and unquoted arguments
        if let Some(InputResult::PromptCommand(opts)) = handle_slash_command(
            r#"/prompt test-prompt simple=value quoted="value with \"nested\" quotes""#,
        ) {
            assert_eq!(opts.name, "test-prompt");
            assert_eq!(opts.arguments.len(), 2);
            assert_eq!(opts.arguments.get("simple"), Some(&"value".to_string()));
            assert_eq!(
                opts.arguments.get("quoted"),
                Some(&r#"value with "nested" quotes"#.to_string())
            );
        } else {
            panic!("Expected PromptCommand");
        }
    }

    // Test invalid arguments
    #[test]
    fn test_invalid_arguments() {
        // Test prompt with invalid arguments
        if let Some(InputResult::PromptCommand(opts)) =
            handle_slash_command(r#"/prompt test-prompt valid=value invalid_arg another_invalid"#)
        {
            assert_eq!(opts.name, "test-prompt");
            assert_eq!(opts.arguments.len(), 1);
            assert_eq!(opts.arguments.get("valid"), Some(&"value".to_string()));
            // Invalid arguments are ignored but logged
        } else {
            panic!("Expected PromptCommand");
        }
    }

    #[test]
    fn test_plan_mode() {
        // Test plan mode with no text
        let result = handle_slash_command("/plan");
        assert!(result.is_some());

        // Test plan mode with text
        let result = handle_slash_command("/plan hello world");
        assert!(result.is_some());
        let options = result.unwrap();
        match options {
            InputResult::Plan(options) => {
                assert_eq!(options.message_text, "hello world");
            }
            _ => panic!("Expected Plan"),
        }
    }

    #[test]
    fn test_workflow_command() {
        // Test workflow with no filepath
        if let Some(InputResult::Workflow(filepath)) = handle_slash_command("/workflow") {
            assert!(filepath.is_none());
        } else {
            panic!("Expected Workflow");
        }

        // Test workflow with filepath
        if let Some(InputResult::Workflow(filepath)) =
            handle_slash_command("/workflow /path/to/file.yaml")
        {
            assert_eq!(filepath, Some("/path/to/file.yaml".to_string()));
        } else {
            panic!("Expected workflow with filepath");
        }

        // Test workflow with invalid extension
        let result = handle_slash_command("/workflow /path/to/file.txt");
        assert!(matches!(result, Some(InputResult::Retry)));
    }

    #[test]
    fn test_get_input_prompt_string() {
        let prompt = get_input_prompt_string();

        // Prompt should always end with a space
        assert!(prompt.ends_with(" "));

        // Prompt should contain the brand label
        assert!(prompt.contains("Biorouter"));

        // On Windows, prompt should be plain text without ANSI codes
        #[cfg(target_os = "windows")]
        {
            assert_eq!(prompt, "Biorouter> ");
            // Ensure no ANSI escape sequences
            assert!(!prompt.contains("\x1b["));
        }

        // On non-Windows, prompt behavior depends on terminal capabilities
        #[cfg(not(target_os = "windows"))]
        {
            // In CI environments, console crate may strip ANSI codes
            let is_ci = std::env::var("CI").is_ok();

            if is_ci {
                // In CI, just verify basic structure - console crate handles ANSI detection
                assert!(prompt.len() >= "Biorouter> ".len());
            } else {
                // In interactive terminals, expect styling to be applied
                // Note: This may still vary based on terminal capabilities
                assert!(prompt.len() >= "Biorouter> ".len());

                // If ANSI codes are present, they should be valid 256-color/bold sequences
                if prompt.contains("\x1b[") {
                    assert!(prompt.contains("38") || prompt.contains("1"));
                }
            }
        }
    }
}
