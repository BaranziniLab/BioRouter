use anyhow::anyhow;
use base64::Engine;
use include_dir::{include_dir, Dir};
use indoc::{formatdoc, indoc};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, CancelledNotificationParam, Content, ErrorCode, ErrorData,
        GetPromptRequestParams, GetPromptResult, Implementation, ListPromptsResult, LoggingLevel,
        LoggingMessageNotificationParam, PaginatedRequestParams, Prompt, PromptArgument,
        PromptMessage, PromptMessageRole, Role, ServerCapabilities, ServerInfo,
    },
    schemars::JsonSchema,
    service::{NotificationContext, RequestContext},
    tool, tool_handler, tool_router, RoleServer, ServerHandler,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env::join_paths,
    ffi::OsString,
    future::Future,
    io::Cursor,
    path::{Path, PathBuf},
    sync::Arc,
};
use xcap::{Monitor, Window};

use tokio::{
    io::{AsyncBufReadExt, BufReader},
    sync::RwLock,
};
use tokio_stream::{wrappers::SplitStream, StreamExt as _};
use tokio_util::sync::CancellationToken;

use crate::developer::{paths::get_shell_path_dirs, shell::ShellConfig};
use crate::secret_guard::SecretGuard;

use super::analyze::{types::AnalyzeParams, CodeAnalyzer};
use super::editor_models::{create_editor_model, EditorModel};
use super::shell::{configure_shell_command, expand_path, is_absolute_path, kill_process_group};
use super::text_editor::{
    save_file_history, text_editor_insert, text_editor_replace, text_editor_undo, text_editor_view,
    text_editor_write,
};
use super::undo_history::{self, FileHistory};

/// Process-global switch for the `text_editor` working-directory containment
/// jail (see [`DeveloperServer::resolve_path`]).
///
/// The `biorouter` agent sets this from the **live** `BioRouterMode` before it
/// dispatches each tool batch: `true` in `Auto` ("Fully Automatic") mode,
/// `false` in every other mode. In Auto mode the agent's `SensitiveOpsInspector`
/// already routes writes to sensitive system paths through the approval flow, so
/// the additional path jail would only reject *legitimate* writes outside the
/// session working directory (e.g. to `/tmp`) — the exact false-rejection the
/// 2026-07-19 tool-errors audit found. The **policy** (which mode relaxes the
/// jail) lives in `biorouter`; this crate merely reads the flag it toggles, so
/// the mode read is never duplicated here. A process global (not a per-instance
/// field) is correct because `BioRouterMode` is itself global config, and the
/// developer server is a long-lived pooled MCP server the agent cannot re-plumb
/// per turn.
static PATH_JAIL_RELAXED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Relax (or re-engage) the `text_editor` working-directory jail process-wide.
/// Called by the `biorouter` agent with `biorouter_mode == BioRouterMode::Auto`.
pub fn set_path_jail_relaxed(relaxed: bool) {
    PATH_JAIL_RELAXED.store(relaxed, std::sync::atomic::Ordering::Relaxed);
}

fn path_jail_relaxed() -> bool {
    PATH_JAIL_RELAXED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Build a git context + version-control policy block for the extension
/// instructions. If `cwd` is inside a git work tree, the agent is told the
/// current branch and how many files are uncommitted, plus a concise policy
/// encouraging disciplined commits and forbidding destructive history ops
/// without an explicit request. Outside a repo this returns an empty string so
/// it adds no noise to non-versioned tasks.
fn git_context_block(cwd: &std::path::Path) -> String {
    let git = |args: &[&str]| -> Option<String> {
        let mut command = std::process::Command::new("git");
        command.args(args).current_dir(cwd);
        crate::developer::shell::strip_daemon_private_env_std(&mut command);
        let out = command.output().ok()?;
        if !out.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    };

    // Only emit anything when we're actually inside a work tree.
    match git(&["rev-parse", "--is-inside-work-tree"]).as_deref() {
        Some("true") => {}
        _ => return String::new(),
    }

    let branch =
        git(&["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|| "unknown".to_string());
    let dirty = git(&["status", "--porcelain"])
        .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count())
        .unwrap_or(0);
    let dirty_str = if dirty == 0 {
        "clean".to_string()
    } else {
        format!("{dirty} uncommitted change(s)")
    };

    formatdoc! {r#"

        Version control (this directory is a git repository):
        - git: branch {branch}, {dirty_str}
        - Treat git as part of doing the work: as you complete a logical unit (a module,
          a fix, a passing test suite), stage and commit it with a clear, specific message.
          Prefer several small, meaningful commits over one giant one; don't end with a
          large pile of uncommitted changes.
        - Before finishing, run `git status` and commit outstanding work so the result is
          reproducible from a clean checkout. Add a `.gitignore` for build artifacts and
          dependencies (e.g. target/, __pycache__/, node_modules/, build/) rather than
          committing them.
        - Never run history-rewriting or destructive git commands (`git reset --hard`,
          `git push --force`, `git clean -fd`, `git rebase`, branch deletion) unless the
          user explicitly asks for them.
    "#}
}

/// Parameters for the screen_capture tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ScreenCaptureParams {
    /// The 0-based display index to capture. If omitted, the primary display is
    /// captured. Every display capture also reports the full list of connected
    /// displays (with indices), so on a multi-monitor setup you can re-capture
    /// another screen by its index.
    #[serde(default)]
    pub display: Option<u64>,

    /// Optional: the title (or any substring of it, case-insensitive) of the
    /// window to capture. Use the list_windows tool to find available windows.
    pub window_title: Option<String>,
}

/// Produce a compact, agent-readable description of every connected display:
/// index, name, resolution, position, scale factor, and which one is primary.
/// `screen_capture` includes this on each display capture so the agent always
/// knows how many screens exist and which index is which — directly avoiding
/// the multi-monitor failure where it only ever sees display 0 and reports the
/// rest as "not found". Cross-platform via xcap (macOS/Windows/Linux).
fn describe_monitors(monitors: &[Monitor]) -> String {
    let mut out = String::from("Connected displays:");
    for (i, m) in monitors.iter().enumerate() {
        let name = m.name().unwrap_or_else(|_| "unknown".to_string());
        let w = m.width().unwrap_or(0);
        let h = m.height().unwrap_or(0);
        let x = m.x().unwrap_or(0);
        let y = m.y().unwrap_or(0);
        let primary = if m.is_primary().unwrap_or(false) {
            " [primary]"
        } else {
            ""
        };
        let scale = m.scale_factor().unwrap_or(1.0);
        out.push_str(&format!(
            "\n  {i}: \"{name}\" {w}x{h} at ({x},{y}) scale {scale:.1}{primary}"
        ));
    }
    out.push_str(
        "\nPass the 0-based `display` index above to screen_capture to grab a specific screen.",
    );
    out
}

/// Parameters for the text_editor tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TextEditorParams {
    /// Absolute path to file or directory, e.g. `/repo/file.py` or `/repo`.
    /// Accepts `file_path` as an alias: some models (e.g. Xiaomi MiMo) intermittently
    /// emit the key as `file_path`, which previously caused an opaque
    /// `-32602: missing field 'path'` deserialization failure and a wasted turn.
    #[serde(alias = "file_path")]
    pub path: String,

    /// The operation to perform. Allowed options are: `view`, `write`, `str_replace`, `insert`, `undo_edit`.
    pub command: String,

    /// Unified diff to apply. Supports editing multiple files simultaneously. Cannot create or delete files
    /// Example: "--- a/file\n+++ b/file\n@@ -1,3 +1,3 @@\n context\n-old\n+new\n context"
    /// Preferred edit method.
    pub diff: Option<String>,

    /// Optional array of two integers specifying the start and end line numbers to view.
    /// Line numbers are 1-indexed, and -1 for the end line means read to the end of the file.
    /// This parameter only applies when viewing files, not directories.
    pub view_range: Option<Vec<i64>>,

    /// The content to write to the file. Required for `write` command.
    pub file_text: Option<String>,

    /// The old string to replace.
    pub old_str: Option<String>,

    /// The new string to replace with. Required for `insert` command.
    pub new_str: Option<String>,

    /// The line number after which to insert text (0 for beginning). Required for `insert` command.
    pub insert_line: Option<i64>,
}

/// Parameters for the shell tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ShellParams {
    /// The command string to execute in the shell
    pub command: String,
    /// Optional directory to run this command in, overriding the session's
    /// working directory for this call only. An absolute path is recommended; a
    /// relative path resolves against the session working directory. Note that a
    /// `cd` inside `command` does not persist to later calls (each call runs in
    /// its own process) — use this field, or chain with `&&`, instead.
    #[serde(default)]
    pub working_directory: Option<String>,
    /// Run the command in the background instead of waiting for it to finish.
    /// Use this for long-lived commands (dev servers, builds, test suites,
    /// training runs) you need to keep running and observe. Returns a job_id
    /// immediately; then use shell_wait / shell_output / shell_kill. Do NOT
    /// append `&` yourself for this — set background=true.
    #[serde(default)]
    pub background: Option<bool>,
    /// Optional short label for a background job, shown by shell_output/wait.
    #[serde(default)]
    pub label: Option<String>,
}

/// Parameters for tools that act on a background job by id.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct JobIdParams {
    /// The job_id returned by `shell` when run with background=true.
    pub job_id: String,
}

/// Parameters for the shell_wait tool.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ShellWaitParams {
    /// The job_id returned by `shell` when run with background=true.
    pub job_id: String,
    /// Maximum seconds to watch before returning. Returns earlier the moment
    /// the job exits. Default 120, capped at 600. If the job is still running
    /// when the timeout elapses, the result says so; call shell_wait again to
    /// keep watching. The job is never killed by waiting.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

/// Parameters for the image_processor tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ImageProcessorParams {
    /// Absolute path to the image file to process
    pub path: String,
}

/// Template structure for prompt definitions
#[derive(Debug, Serialize, Deserialize)]
pub struct PromptTemplate {
    pub id: String,
    pub template: String,
    pub arguments: Vec<PromptArgumentTemplate>,
}

/// Template structure for prompt arguments
#[derive(Debug, Serialize, Deserialize)]
pub struct PromptArgumentTemplate {
    pub name: String,
    pub description: Option<String>,
    pub required: Option<bool>,
}

// Embeds the prompts directory to the build
static PROMPTS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/src/developer/prompts");

/// Loads prompt files from the embedded PROMPTS_DIR and returns a HashMap of prompts.
/// Ensures that each prompt name is unique.
fn load_prompt_files() -> HashMap<String, Prompt> {
    let mut prompts = HashMap::new();

    for entry in PROMPTS_DIR.files() {
        // Only process JSON files
        if entry.path().extension().is_none_or(|ext| ext != "json") {
            continue;
        }

        let prompt_str = String::from_utf8_lossy(entry.contents()).into_owned();

        let template: PromptTemplate = match serde_json::from_str(&prompt_str) {
            Ok(t) => t,
            Err(e) => {
                eprintln!(
                    "Failed to parse prompt template in {}: {}",
                    entry.path().display(),
                    e
                );
                continue; // Skip invalid prompt file
            }
        };

        let arguments = template
            .arguments
            .into_iter()
            .map(|arg| PromptArgument {
                name: arg.name,
                description: arg.description,
                required: arg.required,
                title: None,
            })
            .collect::<Vec<PromptArgument>>();

        let prompt = Prompt::new(&template.id, Some(&template.template), Some(arguments));

        if prompts.contains_key(&prompt.name) {
            eprintln!("Duplicate prompt name '{}' found. Skipping.", prompt.name);
            continue; // Skip duplicate prompt name
        }

        prompts.insert(prompt.name.clone(), prompt);
    }

    prompts
}

/// Developer MCP Server using official RMCP SDK
#[derive(Clone)]
pub struct DeveloperServer {
    tool_router: ToolRouter<Self>,
    file_history: Arc<FileHistory>,
    secret_guard: SecretGuard,
    editor_model: Option<EditorModel>,
    prompts: HashMap<String, Prompt>,
    code_analyzer: CodeAnalyzer,
    #[cfg(test)]
    pub running_processes: Arc<RwLock<HashMap<String, CancellationToken>>>,
    #[cfg(not(test))]
    running_processes: Arc<RwLock<HashMap<String, CancellationToken>>>,
    /// Long-lived background jobs started by `shell` with background=true.
    background_jobs: Arc<super::background::BackgroundJobs>,
    bash_env_file: Option<PathBuf>,
    extend_path_with_shell: bool,
    /// The session's working directory, when known. Shell commands run here
    /// (unless a per-call `working_directory` overrides it). When `None`, the
    /// shell falls back to `BIOROUTER_WORKING_DIR` / the process cwd. Set at
    /// construction from the session so the tool follows the GUI folder picker.
    working_dir: Option<PathBuf>,
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for DeveloperServer {
    #[allow(clippy::too_many_lines)]
    fn get_info(&self) -> ServerInfo {
        // Get base instructions and working directory. Report the base the tools
        // actually resolve against, so the instructions can never name a
        // directory the jail disagrees with. It is legitimately unavailable when
        // the working directory has been deleted, and `get_info` runs during
        // `initialize` — saying so beats taking the process down with an
        // `expect` (#64).
        let cwd = self.effective_cwd().ok();
        let cwd_display = cwd.as_ref().map_or_else(
            || "unavailable (the working directory no longer exists)".to_string(),
            |dir| dir.to_string_lossy().into_owned(),
        );
        let os = std::env::consts::OS;
        let in_container = Self::is_definitely_container();

        let base_instructions = match os {
            "windows" => formatdoc! {r#"
                The developer extension gives you the capabilities to edit code files and run shell commands,
                and can be used to solve a wide range of problems.

                You can use the shell tool to run Windows commands (PowerShell or CMD).
                When using paths, you can use either backslashes or forward slashes.

                Use the shell tool as needed to locate files or interact with the project.

                This extension is the default, first-choice tool for everyday file and system work: use `shell`
                to list, copy, move, delete, or find files and to run commands, and use `text_editor` to read
                (`view`), create/overwrite (`write`), and edit (`str_replace`, `insert`) files. Prefer these
                direct tools over routing a simple file or shell operation through a code-execution script or
                another extension — reach for those only when a task needs real computation, control flow, or a
                specialized capability.

                Leverage `analyze` through `return_last_only=true` subagents for deep codebase understanding with lean context
                - delegate analysis, retain summaries

                Your windows/screen tools can be used for visual debugging. You should not use these tools unless
                prompted to, but you can mention they are available if they are relevant.

                operating system: {os}
                current directory: {cwd}
                {container_info}
                "#,
                os=os,
                cwd=cwd_display,
                container_info=if in_container { "container: true" } else { "" },
            },
            _ => {
                let shell_info = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

                formatdoc! {r#"
                The developer extension gives you the capabilities to edit code files and run shell commands,
                and can be used to solve a wide range of problems.

            You can use the shell tool to run any command that would work on the relevant operating system.
            Use the shell tool as needed to locate files or interact with the project.

            This extension is the default, first-choice tool for everyday file and system work: use `shell`
            to list, copy, move, delete, or find files (`ls`, `cp`, `mv`, `rm`, `mkdir`, `rg`) and to run
            commands, and use `text_editor` to read (`view`), create/overwrite (`write`), and edit
            (`str_replace`, `insert`) files. Prefer these direct tools over routing a simple file or shell
            operation through a code-execution script or another extension — reach for those only when a task
            needs real computation, control flow, or a specialized capability. When you just need a file's
            contents, use `text_editor` view rather than `cat`/`head` in shell.

            Leverage `analyze` through `return_last_only=true` subagents for deep codebase understanding with lean context
            - delegate analysis, retain summaries

            Your windows/screen tools can be used for visual debugging. You should not use these tools unless
            prompted to, but you can mention they are available if they are relevant.

            Always prefer ripgrep (rg -C 3) to grep.

            operating system: {os}
            current directory: {cwd}
            shell: {shell}
            {container_info}
                "#,
                os=os,
                cwd=cwd_display,
                shell=shell_info,
                container_info=if in_container { "container: true" } else { "" },
                }
            }
        };

        // Check if editor model exists and augment with custom llm editor tool description
        let editor_description = if let Some(ref editor) = self.editor_model {
            formatdoc! {r#"

                Additional Text Editor Tool Instructions:

                Perform text editing operations on files.
                The `command` parameter specifies the operation to perform. Allowed options are:
                - `view`: View the content of a file.
                - `write`: Create or overwrite a file with the given content
                - `str_replace`: Replace text in one or more files.
                - `insert`: Insert text at a specific line location in the file.
                - `undo_edit`: Undo the last edit made to a file.

                To use the write command, you must specify `file_text` which will become the new content of the file. Be careful with
                existing files! This is a full overwrite, so you must include everything - not just sections you are modifying.

                To use the insert command, you must specify both `insert_line` (the line number after which to insert, 0 for beginning, -1 for end)
                and `new_str` (the text to insert).

                To use the str_replace command to edit multiple files, use the `diff` parameter with a unified diff.
                To use the str_replace command to edit one file, you must specify both `old_str` and `new_str` - the `old_str` needs to exactly match one
                unique section of the original file, including any whitespace. Make sure to include enough context that the match is not
                ambiguous. The entire original string will be replaced with `new_str`

                When possible, batch file edits together by using a multi-file unified `diff` within a single str_replace tool call.

                {}

            "#, editor.get_str_replace_description()}
        } else {
            formatdoc! {r#"

                Additional Text Editor Tool Instructions:

                Perform text editing operations on files.

                The `command` parameter specifies the operation to perform. Allowed options are:
                - `view`: View the content of a file.
                - `write`: Create or overwrite a file with the given content
                - `str_replace`: Replace text in one or more files.
                - `insert`: Insert text at a specific line location in the file.
                - `undo_edit`: Undo the last edit made to a file.

                To use the write command, you must specify `file_text` which will become the new content of the file. Be careful with
                existing files! This is a full overwrite, so you must include everything - not just sections you are modifying.

                To use the str_replace command to edit multiple files, use the `diff` parameter with a unified diff.
                To use the str_replace command to edit one file, you must specify both `old_str` and `new_str` - the `old_str` needs to exactly match one
                unique section of the original file, including any whitespace. Make sure to include enough context that the match is not
                ambiguous. The entire original string will be replaced with `new_str`

                When possible, batch file edits together by using a multi-file unified `diff` within a single str_replace tool call.

                To use the insert command, you must specify both `insert_line` (the line number after which to insert, 0 for beginning, -1 for end)
                and `new_str` (the text to insert).


            "#}
        };

        // Create comprehensive shell tool instructions
        let common_shell_instructions = indoc! {r#"
            Additional Shell Tool Instructions:
            Execute a command in the shell.

            This will return the output and error concatenated into a single string, as
            you would see from running on the command line. There will also be an indication
            of if the command succeeded or failed.

            Avoid commands that produce a large amount of output, and consider piping those outputs to files.

            Use this shell tool directly for filesystem operations (ls, cp, mv, rm, mkdir, rg) rather than
            wrapping them in a code-execution script. To read or write a file's contents, prefer the
            `text_editor` tool (view/write/str_replace) over `cat`/`head`/`sed`/`echo >` in the shell.

            **Important**: Each shell command runs in its own process. Things like directory changes or
            sourcing files do not persist between tool calls. So you may need to repeat them each time by
            stringing together commands.

            If fetching web content, consider adding Accept: text/markdown header
        "#};

        let windows_specific = indoc! {r#"
            **Important**: For searching files and code:

            Preferred: Use ripgrep (`rg`) when available - it respects .gitignore and is fast:
              - To locate a file by name: `rg --files | rg example.py`
              - To locate content inside files: `rg 'class Example'`

            Alternative Windows commands (if ripgrep is not installed):
              - To locate a file by name: `dir /s /b example.py`
              - To locate content inside files: `findstr /s /i "class Example" *.py`

            Note: Alternative commands may show ignored/hidden files that should be excluded.

              - Multiple commands: Use && to chain commands, avoid newlines
              - Example: `cd example && dir` or `activate.bat && pip install numpy`

             **Important**: Use forward slashes in paths (e.g., `C:/Users/name`) to avoid
                 escape character issues with backslashes, i.e. \n in a path could be
                 mistaken for a newline.
        "#};

        let unix_specific = indoc! {r#"
            For a long-lived command (dev server, build, test suite, training run) that you need to
            keep running and observe, run shell with `background=true` rather than appending `&`. That
            returns a job_id and keeps the process alive across tool calls; then call `shell_wait` to
            wait for it to finish (it returns the moment it exits, or after a timeout still running, so
            it never blocks your turn or kills the job), `shell_output` to peek at new output, and
            `shell_kill` to stop it. Completion is decided by the command's exit status.

            **Important**: Use ripgrep - `rg` - exclusively when you need to locate a file or a code reference,
            other solutions may produce too large output because of hidden files! For example *do not* use `find` or `ls -r`
              - List files by name: `rg --files | rg <filename>`
              - List files that contain a regex: `rg '<regex>' -l`

              - Multiple commands: Use && to chain commands, avoid newlines
              - Example: `cd example && ls` or `source env/bin/activate && pip install numpy`
        "#};

        let shell_tool_desc = match os {
            "windows" => format!("{}{}", common_shell_instructions, windows_specific),
            _ => format!("{}{}", common_shell_instructions, unix_specific),
        };

        let git_desc = cwd.as_deref().map(git_context_block).unwrap_or_default();

        let instructions =
            format!("{base_instructions}{git_desc}{editor_description}\n{shell_tool_desc}");

        ServerInfo {
            server_info: Implementation {
                name: "biorouter-developer".to_string(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
                title: None,
                icons: None,
                website_url: None,
            },
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .build(),
            instructions: Some(instructions),
            ..Default::default()
        }
    }

    // TODO: use the rmcp prompt macros instead when SDK is updated
    // Current rmcp version 0.6.0 doesn't support prompt macros yet.
    // When upgrading to a newer version that supports it, replace this manual
    // implementation with the macro-based approach for better maintainability.
    fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListPromptsResult, ErrorData>> + Send + '_ {
        let prompts: Vec<Prompt> = self.prompts.values().cloned().collect();
        std::future::ready(Ok(ListPromptsResult {
            prompts,
            next_cursor: None,
            meta: None,
        }))
    }

    fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<GetPromptResult, ErrorData>> + Send + '_ {
        let prompt_name = request.name;
        let arguments = request.arguments.unwrap_or_default();

        match self.prompts.get(&prompt_name) {
            Some(prompt) => {
                // Get the template from the prompt description
                let template = prompt.description.clone().unwrap_or_default();

                // Validate template length
                if template.len() > 10000 {
                    return std::future::ready(Err(ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        "Prompt template exceeds maximum allowed length".to_string(),
                        None,
                    )));
                }

                // Validate arguments for security (same checks as router)
                for (key, value) in &arguments {
                    // Check for empty or overly long keys/values
                    if key.is_empty() || key.len() > 1000 {
                        return std::future::ready(Err(ErrorData::new(
                            ErrorCode::INVALID_PARAMS,
                            "Argument keys must be between 1-1000 characters".to_string(),
                            None,
                        )));
                    }

                    // Validate key with allowlist: only alphanumeric, underscore, dash, dot
                    if !key
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
                    {
                        return std::future::ready(Err(ErrorData::new(
                            ErrorCode::INVALID_PARAMS,
                            format!(
                                "Invalid parameter key '{}': only alphanumeric, underscore, dash, and dot characters are allowed",
                                key
                            ),
                            None,
                        )));
                    }

                    let value_str = value.as_str().unwrap_or_default();
                    // Reject values longer than 1MB
                    if value_str.len() > 1_048_576 {
                        return std::future::ready(Err(ErrorData::new(
                            ErrorCode::INVALID_PARAMS,
                            "Argument values must not exceed 1MB".to_string(),
                            None,
                        )));
                    }
                }

                // Validate required arguments
                if let Some(args) = &prompt.arguments {
                    for arg in args {
                        if arg.required.unwrap_or(false)
                            && (!arguments.contains_key(&arg.name)
                                || arguments
                                    .get(&arg.name)
                                    .and_then(|v| v.as_str())
                                    .is_none_or(str::is_empty))
                        {
                            return std::future::ready(Err(ErrorData::new(
                                ErrorCode::INVALID_PARAMS,
                                format!("Missing required argument: '{}'", arg.name),
                                None,
                            )));
                        }
                    }
                }

                // Create a mutable copy of the template to fill in arguments
                let mut template_filled = template.clone();

                // Replace each argument placeholder with its value from the arguments object
                for (key, value) in &arguments {
                    let placeholder = format!("{{{}}}", key);
                    template_filled =
                        template_filled.replace(&placeholder, value.as_str().unwrap_or_default());
                }

                // Create prompt messages with the filled template
                let messages = vec![PromptMessage::new_text(
                    PromptMessageRole::User,
                    template_filled.clone(),
                )];

                let result = GetPromptResult {
                    description: Some(template_filled),
                    messages,
                };
                std::future::ready(Ok(result))
            }
            None => std::future::ready(Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Prompt '{}' not found", prompt_name),
                None,
            ))),
        }
    }

    /// Called when the client cancels a specific request.
    /// This method cancels the running process associated with the given request_id.
    #[allow(clippy::manual_async_fn)]
    fn on_cancelled(
        &self,
        notification: CancelledNotificationParam,
        _context: NotificationContext<RoleServer>,
    ) -> impl Future<Output = ()> + Send + '_ {
        async move {
            let request_id = notification.request_id.to_string();
            let processes = self.running_processes.read().await;

            if let Some(token) = processes.get(&request_id) {
                token.cancel();
                tracing::debug!("Found process for request {}, cancelling token", request_id);
            } else {
                tracing::warn!("No process found for request ID: {}", request_id);
            }
        }
    }
}

impl Default for DeveloperServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router(router = tool_router)]
impl DeveloperServer {
    pub fn new() -> Self {
        // Build the shared secret/ignore guard (BR-23) rooted at the cwd.
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let secret_guard = SecretGuard::for_dir(&cwd);

        // Initialize editor model for AI-powered code editing
        let editor_model = create_editor_model();

        Self {
            tool_router: Self::tool_router(),
            file_history: Arc::new(FileHistory::in_memory()),
            secret_guard,
            editor_model,
            prompts: load_prompt_files(),
            code_analyzer: CodeAnalyzer::new(),
            running_processes: Arc::new(RwLock::new(HashMap::new())),
            background_jobs: Arc::new(super::background::BackgroundJobs::new()),
            extend_path_with_shell: false,
            bash_env_file: None,
            working_dir: None,
        }
    }

    pub fn extend_path_with_shell(mut self, value: bool) -> Self {
        self.extend_path_with_shell = value;
        self
    }

    /// Set the session working directory that shell commands run in. When unset,
    /// the shell falls back to `BIOROUTER_WORKING_DIR` / the process cwd.
    ///
    /// Once the working directory is known, the `undo_edit` history is persisted
    /// to disk keyed by that directory (BR-44), so undo survives a developer
    /// server restart. Any prior history for this directory is reloaded here.
    ///
    /// The `SecretGuard` is re-rooted here too (#68). `new()` has to root it at
    /// the process cwd because that is all it knows, but every real caller
    /// constructs and *then* binds a working directory — so leaving the guard
    /// where `new()` put it meant the deny set was read relative to a directory
    /// this server was never bound to, and the project's own `.biorouterignore`
    /// was never read at all (measured, not inferred). The guard is a security
    /// root, so it tracks the same base [`Self::effective_cwd`] jails to rather
    /// than whichever directory the process happened to start in.
    ///
    /// This is defence in depth, not the only enforcement: the extension
    /// manager's dispatch boundary already builds a guard rooted at the resolved
    /// session directory for every tool call. It should not be the only one that
    /// gets the root right.
    pub fn with_working_dir(mut self, dir: PathBuf) -> Self {
        self.file_history = Arc::new(FileHistory::persistent(&dir));
        self.secret_guard = SecretGuard::for_dir(&dir);
        self.working_dir = Some(dir);
        self
    }

    /// Resolve the directory a shell command should run in: an explicit per-call
    /// `working_directory` wins (resolved against the session dir if relative),
    /// then the session working directory, then `BIOROUTER_WORKING_DIR`. `None`
    /// means inherit the process cwd (the pre-existing behavior).
    fn resolve_shell_cwd(&self, override_dir: Option<&str>) -> Option<PathBuf> {
        if let Some(s) = override_dir.map(str::trim).filter(|s| !s.is_empty()) {
            let p = PathBuf::from(s);
            return Some(if p.is_absolute() {
                p
            } else if let Some(base) = &self.working_dir {
                base.join(p)
            } else {
                p
            });
        }
        self.working_dir.clone().or_else(|| {
            std::env::var("BIOROUTER_WORKING_DIR")
                .ok()
                .map(PathBuf::from)
        })
    }

    /// The session working directory to run in, honoring existence: the session
    /// dir if it still exists, else `BIOROUTER_WORKING_DIR` if it exists, else
    /// `None` (inherit the process cwd). A candidate that has vanished (deleted
    /// after selection, or a stale session row) is logged and skipped instead of
    /// jamming every shell command with an opaque spawn error.
    ///
    /// **This is the shell's helper alone** (#68). It once backed the
    /// `text_editor` jail too, on the reasoning that both should agree on where
    /// "here" is — but the two questions have different risk profiles. This one
    /// answers *where a command runs*, which grants no file access, so walking
    /// down to the next candidate costs nothing. The jail base answers *what the
    /// file tools may touch*; substituting a candidate there moves a security
    /// boundary, and when the session dir sat inside `BIOROUTER_WORKING_DIR` it
    /// moved it outward. The jail resolves its own base in
    /// [`Self::effective_cwd`] and refuses rather than falling through. Do not
    /// re-point it here.
    fn session_cwd_or_fallback(&self) -> Option<PathBuf> {
        if let Some(dir) = &self.working_dir {
            if dir.is_dir() {
                return Some(dir.clone());
            }
            tracing::warn!(
                dir = %dir.display(),
                "session working directory no longer exists; falling back to BIOROUTER_WORKING_DIR / process cwd"
            );
        }
        if let Some(dir) = std::env::var("BIOROUTER_WORKING_DIR")
            .ok()
            .map(PathBuf::from)
        {
            if dir.is_dir() {
                return Some(dir);
            }
            tracing::warn!(
                dir = %dir.display(),
                "BIOROUTER_WORKING_DIR does not exist; falling back to process cwd"
            );
        }
        None
    }

    /// Existence-checked variant of [`resolve_shell_cwd`], keeping the same
    /// resolution order but validating the target:
    /// - a per-call `working_directory` override that does not exist or is not a
    ///   directory is a hard error naming the path (the caller explicitly asked
    ///   for it, so never run somewhere else silently);
    /// - a missing session dir / `BIOROUTER_WORKING_DIR` is not fatal — it warns
    ///   and falls back to the next candidate so the shell keeps working.
    fn resolve_shell_cwd_checked(
        &self,
        override_dir: Option<&str>,
    ) -> Result<Option<PathBuf>, ErrorData> {
        if override_dir.map(str::trim).is_some_and(|s| !s.is_empty()) {
            // Same absolute/relative-to-session resolution as the pure helper.
            let resolved = self
                .resolve_shell_cwd(override_dir)
                .expect("a non-empty override always resolves to Some");
            if !resolved.is_dir() {
                return Err(ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    format!(
                        "working_directory `{}` does not exist or is not a directory",
                        resolved.display()
                    ),
                    None,
                ));
            }
            return Ok(Some(resolved));
        }
        Ok(self.session_cwd_or_fallback())
    }

    pub fn bash_env_file(mut self, value: Option<PathBuf>) -> Self {
        self.bash_env_file = value;
        self
    }

    /// Snapshot the pre-command content of files a shell command redirects to
    /// (`>`/`>>`), so `undo_edit <path>` restores the state before the write
    /// (BR-44). Best-effort and side-effect free on the command: an unresolved,
    /// out-of-tree, ignored, or `..`-containing target is simply skipped.
    fn snapshot_shell_redirect_targets(&self, command: &str, working_dir: Option<&Path>) {
        let targets = undo_history::redirect_targets(command);
        if targets.is_empty() {
            return;
        }
        // Resolve the base directory without the panicking `effective_cwd`: if
        // the process has no valid cwd there is nothing safe to snapshot.
        let base = match working_dir {
            Some(w) => w.to_path_buf(),
            None => match std::env::current_dir() {
                Ok(d) => d,
                Err(_) => return,
            },
        };
        for raw in targets {
            if raw.contains("..") {
                continue;
            }
            let expanded = expand_path(&raw);
            let resolved = if is_absolute_path(&expanded) {
                let p = PathBuf::from(&expanded);
                // Only snapshot absolute targets inside the working directory.
                if !p.starts_with(&base) {
                    continue;
                }
                p
            } else {
                base.join(&expanded)
            };
            // Don't copy ignored files (.env, secrets, ...) into the history.
            if self.is_ignored(&resolved) {
                continue;
            }
            let _ = self.file_history.snapshot(&resolved);
        }
    }

    /// List all available windows that can be used with screen_capture.
    /// Returns a list of window titles that can be used with the window_title parameter
    /// of the screen_capture tool.
    #[tool(
        name = "list_windows",
        description = "List all available window titles that can be used with screen_capture. Returns a list of window titles that can be used with the window_title parameter of the screen_capture tool."
    )]
    pub async fn list_windows(&self) -> Result<CallToolResult, ErrorData> {
        let windows = Window::all().map_err(|_| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                "Failed to list windows".to_string(),
                None,
            )
        })?;

        let window_titles: Vec<String> =
            windows.into_iter().filter_map(|w| w.title().ok()).collect();

        let content_text = format!("Available windows:\n{}", window_titles.join("\n"));

        Ok(CallToolResult::success(vec![
            Content::text(content_text.clone()).with_audience(vec![Role::Assistant]),
            Content::text(content_text)
                .with_audience(vec![Role::User])
                .with_priority(0.0),
        ]))
    }

    /// Capture a screenshot of a specified display or window.
    /// You can capture either:
    /// 1. A full display (monitor) using the display parameter
    /// 2. A specific window by its title using the window_title parameter
    ///
    /// Only one of display or window_title should be specified.
    #[tool(
        name = "screen_capture",
        description = "Capture a screenshot of a display or a window. Capture either: 1. a full display via the 0-based `display` index (omit it to capture the primary display; the result lists every connected display with its index so you can target other monitors), or 2. a specific window via `window_title` (case-insensitive substring match; on no match the result lists the open window titles). Specify only one of `display` or `window_title`. Works across macOS, Windows, and Linux."
    )]
    pub async fn screen_capture(
        &self,
        params: Parameters<ScreenCaptureParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;

        // Human/agent-readable note describing what was captured and, for
        // display captures, the full multi-monitor topology. Reporting the
        // topology on every capture is what lets the agent realise it has more
        // than one screen (and which index is which) instead of repeatedly
        // capturing display 0 and reporting things "not found".
        // Assigned in each branch below (deferred init).
        let capture_note: String;

        let mut image = if let Some(window_title) = &params.window_title {
            // Try to find and capture the specified window. Match case-insensitively
            // and by substring: real window titles are noisy (e.g. "Slack | general
            // | Acme") so requiring an exact match is the main reason window capture
            // "fails to find" an app that is clearly open.
            let windows = Window::all().map_err(|_| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    "Failed to list windows".to_string(),
                    None,
                )
            })?;

            let needle = window_title.to_lowercase();
            let titles: Vec<String> = windows
                .iter()
                .filter_map(|w| w.title().ok())
                .filter(|t| !t.is_empty())
                .collect();

            // Prefer an exact (case-insensitive) match, then fall back to substring.
            let window = windows
                .iter()
                .find(|w| {
                    w.title()
                        .is_ok_and(|t| t.eq_ignore_ascii_case(window_title))
                })
                .or_else(|| {
                    windows
                        .iter()
                        .find(|w| w.title().is_ok_and(|t| t.to_lowercase().contains(&needle)))
                })
                .ok_or_else(|| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!(
                            "No open window matches '{}'. Available window titles:\n{}\n\nPick one \
                             of the titles above (a substring is enough), or capture a whole \
                             display instead.",
                            window_title,
                            if titles.is_empty() {
                                "  (none — no titled windows are currently open)".to_string()
                            } else {
                                titles
                                    .iter()
                                    .map(|t| format!("  - {t}"))
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            }
                        ),
                        None,
                    )
                })?;

            let matched_title = window.title().unwrap_or_default();
            capture_note = format!("Captured window '{matched_title}'.");

            window.capture_image().map_err(|e| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Failed to capture window '{}': {}", matched_title, e),
                    None,
                )
            })?
        } else {
            let monitors = Monitor::all().map_err(|_| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    "Failed to access monitors".to_string(),
                    None,
                )
            })?;
            if monitors.is_empty() {
                return Err(ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    "No displays were detected.".to_string(),
                    None,
                ));
            }

            let topology = describe_monitors(&monitors);

            // Default to the *primary* display rather than index 0: on
            // multi-monitor setups Monitor::all() ordering is not guaranteed, so
            // index 0 is frequently the wrong (secondary) screen.
            let display = match params.display {
                Some(d) => d as usize,
                None => monitors
                    .iter()
                    .position(|m| m.is_primary().unwrap_or(false))
                    .unwrap_or(0),
            };

            let monitor = monitors.get(display).ok_or_else(|| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!(
                        "Display {} does not exist. {} display(s) are connected (valid indices \
                         0..={}).\n{}",
                        display,
                        monitors.len(),
                        monitors.len() - 1,
                        topology
                    ),
                    None,
                )
            })?;

            capture_note = format!("Captured display {}.\n{}", display, topology);

            monitor.capture_image().map_err(|e| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Failed to capture display {}: {}", display, e),
                    None,
                )
            })?
        };

        // Resize the image to a reasonable width while maintaining aspect ratio
        let max_width = 768;
        if image.width() > max_width {
            let scale = max_width as f32 / image.width() as f32;
            let new_height = (image.height() as f32 * scale) as u32;
            image = xcap::image::imageops::resize(
                &image,
                max_width,
                new_height,
                xcap::image::imageops::FilterType::Lanczos3,
            );
        }

        let mut bytes: Vec<u8> = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut bytes), xcap::image::ImageFormat::Png)
            .map_err(|e| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Failed to write image buffer {}", e),
                    None,
                )
            })?;

        // Convert to base64
        let data = base64::prelude::BASE64_STANDARD.encode(bytes);

        // Return two Content objects like the old implementation:
        // one text for Assistant, one image with priority 0.0
        let note = if capture_note.is_empty() {
            "Screenshot captured".to_string()
        } else {
            format!("Screenshot captured. {capture_note}")
        };
        Ok(CallToolResult::success(vec![
            Content::text(note).with_audience(vec![Role::Assistant]),
            Content::image(data, "image/png").with_priority(0.0),
        ]))
    }

    /// Perform text editing operations on files.
    ///
    /// The `command` parameter specifies the operation to perform. Allowed options are:
    /// - `view`: View the content of a file.
    /// - `write`: Create or overwrite a file with the given content
    /// - `str_replace`: Replace old_str with new_str in the file.
    /// - `insert`: Insert text at a specific line location in the file.
    /// - `undo_edit`: Undo the last edit made to a file.
    #[tool(
        name = "text_editor",
        description = "Perform text editing operations on files. Commands: view (show file content), write (create/overwrite file), str_replace (edit file), insert (insert at line), undo_edit (undo last change)."
    )]
    pub async fn text_editor(
        &self,
        params: Parameters<TextEditorParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let path = self.resolve_path(&params.path)?;

        // Check if file is ignored before proceeding with any text editor operation
        if self.is_ignored(&path) {
            return Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!(
                    "Access to '{}' is restricted by .biorouterignore",
                    path.display()
                ),
                None,
            ));
        }

        match params.command.as_str() {
            "view" => {
                let view_range = params.view_range.as_ref().and_then(|vr| {
                    if vr.len() == 2 {
                        Some((vr[0] as usize, vr[1]))
                    } else {
                        None
                    }
                });
                let content = text_editor_view(&path, view_range).await?;
                Ok(CallToolResult::success(content))
            }
            "write" => {
                let file_text = params.file_text.ok_or_else(|| {
                    ErrorData::new(
                        ErrorCode::INVALID_PARAMS,
                        "Missing 'file_text' parameter for write command".to_string(),
                        None,
                    )
                })?;
                // Snapshot the pre-write content so a whole-file overwrite is
                // undoable too, not just str_replace/insert/diff edits (BR-44).
                save_file_history(&path, &self.file_history)?;
                let content = text_editor_write(&path, &file_text).await?;
                Ok(CallToolResult::success(content))
            }
            "str_replace" => {
                // Check if diff parameter is provided
                if let Some(ref diff) = params.diff {
                    // When diff is provided, old_str and new_str are not required
                    let content = text_editor_replace(
                        &path,
                        "", // old_str not used with diff
                        "", // new_str not used with diff
                        Some(diff),
                        &self.editor_model,
                        &self.file_history,
                    )
                    .await?;
                    Ok(CallToolResult::success(content))
                } else {
                    // Traditional str_replace with old_str and new_str
                    let old_str = params.old_str.ok_or_else(|| {
                        ErrorData::new(
                            ErrorCode::INVALID_PARAMS,
                            "Missing 'old_str' parameter for str_replace command".to_string(),
                            None,
                        )
                    })?;
                    let new_str = params.new_str.ok_or_else(|| {
                        ErrorData::new(
                            ErrorCode::INVALID_PARAMS,
                            "Missing 'new_str' parameter for str_replace command".to_string(),
                            None,
                        )
                    })?;
                    let content = text_editor_replace(
                        &path,
                        &old_str,
                        &new_str,
                        None,
                        &self.editor_model,
                        &self.file_history,
                    )
                    .await?;
                    Ok(CallToolResult::success(content))
                }
            }
            "insert" => {
                let insert_line = params.insert_line.ok_or_else(|| {
                    ErrorData::new(
                        ErrorCode::INVALID_PARAMS,
                        "Missing 'insert_line' parameter for insert command".to_string(),
                        None,
                    )
                })? as usize;
                let new_str = params.new_str.ok_or_else(|| {
                    ErrorData::new(
                        ErrorCode::INVALID_PARAMS,
                        "Missing 'new_str' parameter for insert command".to_string(),
                        None,
                    )
                })?;
                let content =
                    text_editor_insert(&path, insert_line as i64, &new_str, &self.file_history)
                        .await?;
                Ok(CallToolResult::success(content))
            }
            "undo_edit" => {
                let content = text_editor_undo(&path, &self.file_history).await?;
                Ok(CallToolResult::success(content))
            }
            _ => Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                format!("Unknown command '{}'", params.command),
                None,
            )),
        }
    }

    /// Execute a command in the shell.
    ///
    /// This will return the output and error concatenated into a single string, as
    /// you would see from running on the command line. There will also be an indication
    /// of if the command succeeded or failed.
    ///
    /// Avoid commands that produce a large amount of output, and consider piping those outputs to files.
    /// For long-lived commands (dev servers, builds, test suites), set `background=true` instead of
    /// appending `&`; this returns a job_id you can watch with shell_wait / shell_output / shell_kill.
    #[tool(
        name = "shell",
        description = "Execute a command in the shell.This will return the output and error concatenated into a single string, as you would see from running on the command line. There will also be an indication of if the command succeeded or failed. Avoid commands that produce a large amount of output, and consider piping those outputs to files. For a long-lived command (dev server, build, test suite, training run) that you need to keep running and observe, set background=true instead of appending `&`: it returns a job_id immediately, and you then use shell_wait to wait for it, shell_output to peek, or shell_kill to stop it."
    )]
    pub async fn shell(
        &self,
        params: Parameters<ShellParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let command = &params.command;
        let peer = context.peer;
        let request_id = context.id;

        // Validate the shell command
        self.validate_shell_command(command)?;

        // Resolve the directory this command runs in: a per-call override, else
        // the session working directory, else the process cwd. A missing
        // override errors here; a vanished session/env dir warns and falls back
        // so the shell keeps working. Applies to both foreground and background.
        let working_dir = self.resolve_shell_cwd_checked(params.working_directory.as_deref())?;

        // Snapshot the pre-command content of any file this command redirects
        // to (`>`/`>>`), so `undo_edit` can revert shell-driven writes, not just
        // text_editor edits (BR-44).
        self.snapshot_shell_redirect_targets(command, working_dir.as_deref());

        // Background mode: start the command in its own process group, register
        // it, and return a job_id immediately instead of waiting for it.
        if params.background.unwrap_or(false) {
            let id = self
                .background_jobs
                .spawn(command, params.label.clone(), working_dir)
                .await
                .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e, None))?;
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "Started background job {id}. It keeps running across tool calls. Use shell_wait with this job_id to watch for completion, shell_output to peek, shell_kill to stop it, or shell_list to see every background job."
            ))]));
        }

        let cancellation_token = CancellationToken::new();
        // Track the process using the request ID
        {
            let mut processes = self.running_processes.write().await;
            let request_id_str = request_id.to_string();
            processes.insert(request_id_str.clone(), cancellation_token.clone());
        }

        // Execute the command and capture output
        let output_result = self
            .execute_shell_command(command, working_dir, &peer, cancellation_token.clone())
            .await;

        // Clean up the process from tracking
        {
            let mut processes = self.running_processes.write().await;
            let request_id_str = request_id.to_string();
            let was_present = processes.remove(&request_id_str).is_some();
            if !was_present {
                tracing::warn!(
                    "Process for request_id {} was not in tracking map when trying to remove",
                    request_id
                );
            }
        }

        let (output_str, exit_code) = output_result?;

        // Validate output size
        self.validate_shell_output_size(command, &output_str)?;

        // Process and format the output
        let (final_output, user_output) = self.process_shell_output(&output_str)?;

        // PAR-02: honour the documented contract — say whether the command
        // succeeded. A non-zero exit is appended as an explicit status line (so
        // the model can see it even when the command wrote nothing at all) and
        // flips `is_error`, which is what the chat UI reads to render a failed
        // call as an error card instead of a green success (dfa6dc32).
        //
        // Only a genuinely non-zero exit is an error. Exit 0 stays clean, and a
        // signal-terminated process (`code() == None`) is reported as a failure
        // too, since it certainly did not succeed.
        let failed = exit_code != Some(0);
        let status_line = match exit_code {
            Some(0) => None,
            Some(code) => Some(format!("[shell: command exited with status {code}]")),
            None => Some("[shell: command terminated by a signal]".to_string()),
        };
        let with_status = |body: String| match &status_line {
            None => body,
            Some(line) if body.trim().is_empty() => line.clone(),
            Some(line) => format!("{body}\n{line}"),
        };

        let mut result = CallToolResult::success(vec![
            Content::text(with_status(final_output)).with_audience(vec![Role::Assistant]),
            Content::text(with_status(user_output))
                .with_audience(vec![Role::User])
                .with_priority(0.0),
        ]);
        result.is_error = Some(failed);
        // Name the failure ourselves. `is_error` is what admits a result into
        // the BR-51 taxonomy, and with no envelope of its own that taxonomy
        // falls back to substring-matching the raw command output against
        // curated patterns ("401", "403", "not found", "timeout", …). Ordinary
        // build and test output contains those words for reasons that have
        // nothing to do with why the command exited non-zero, so the model
        // could be told a failed build was `permission_denied` and
        // `retryable=false` — advice to stop rather than to fix. A non-zero
        // exit is definitionally a plain tool failure: the command ran, nothing
        // was missing and the environment refused nothing. Saying so takes the
        // guesswork out, because a tool-supplied envelope wins over the text
        // heuristics.
        if failed {
            result.structured_content = Some(serde_json::json!({
                "error": {
                    "kind": "tool_failure",
                    "message": status_line
                        .clone()
                        .unwrap_or_else(|| "command failed".to_string()),
                }
            }));
        }
        Ok(result)
    }

    #[tool(
        name = "shell_wait",
        description = "Wait for a background shell job (started by shell with background=true) for up to timeout_secs (default 120, max 600). Returns the moment the job exits, or at the timeout with status: running (the job is NOT killed — call again to keep watching). Use this to wait for background work without ending your turn or busy-looping."
    )]
    pub async fn shell_wait(
        &self,
        params: Parameters<ShellWaitParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        let dur = p
            .timeout_secs
            .unwrap_or(super::background::DEFAULT_WAIT_SECS)
            .min(super::background::MAX_WAIT_SECS);
        let out = self
            .background_jobs
            .wait(&p.job_id, dur)
            .await
            .map_err(|e| ErrorData::new(ErrorCode::INVALID_PARAMS, e, None))?;
        Ok(CallToolResult::success(vec![Content::text(out)]))
    }

    #[tool(
        name = "shell_output",
        description = "Non-blocking peek at a background shell job: returns its status and only the output produced since the last check. Omit to wait; use shell_wait instead when you want to block until it finishes."
    )]
    pub async fn shell_output(
        &self,
        params: Parameters<JobIdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let out = self
            .background_jobs
            .snapshot(&params.0.job_id)
            .await
            .map_err(|e| ErrorData::new(ErrorCode::INVALID_PARAMS, e, None))?;
        Ok(CallToolResult::success(vec![Content::text(out)]))
    }

    #[tool(
        name = "shell_kill",
        description = "Stop a background shell job by killing its whole process group (SIGTERM then SIGKILL). Its status becomes killed."
    )]
    pub async fn shell_kill(
        &self,
        params: Parameters<JobIdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let out = self
            .background_jobs
            .kill(&params.0.job_id)
            .await
            .map_err(|e| ErrorData::new(ErrorCode::INVALID_PARAMS, e, None))?;
        Ok(CallToolResult::success(vec![Content::text(out)]))
    }

    #[tool(
        name = "shell_list",
        description = "List every background shell job (started by shell with background=true) with its job_id, label, status (running or the exit result), runtime, whether it has unread output, and the command. Use this to rediscover a job_id you've lost or to see everything you have running before you shell_wait/shell_output/shell_kill."
    )]
    pub async fn shell_list(&self) -> Result<CallToolResult, ErrorData> {
        let out = self.background_jobs.list().await;
        Ok(CallToolResult::success(vec![Content::text(out)]))
    }

    /// Validate a shell command before execution.
    ///
    /// Checks for empty commands and ensures the command doesn't attempt to access
    /// files that are restricted by ignore patterns.
    fn validate_shell_command(&self, command: &str) -> Result<(), ErrorData> {
        // Check for empty commands
        if command.trim().is_empty() {
            return Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                "Shell command cannot be empty".to_string(),
                None,
            ));
        }

        let cmd_parts: Vec<&str> = command.split_whitespace().collect();

        // Check if command arguments reference ignored files
        for arg in &cmd_parts[1..] {
            // Skip command flags
            if arg.starts_with('-') {
                continue;
            }

            // Skip invalid paths
            let path = Path::new(arg);
            if !path.exists() {
                continue;
            }

            if self.is_ignored(path) {
                return Err(ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!(
                        "The command attempts to access '{}' which is restricted by .biorouterignore",
                        arg
                    ),
                    None,
                ));
            }
        }

        Ok(())
    }

    /// Execute a shell command and return the combined output plus the exit
    /// code the command finished with.
    ///
    /// PAR-02: the exit code is part of the return value because the tool's own
    /// contract ("There will also be an indication of if the command succeeded
    /// or failed") depends on it. It used to be discarded, which made a command
    /// that failed silently (`exit 7`, a build that dies with no stderr)
    /// indistinguishable from one that succeeded quietly — both surfaced as an
    /// `Ok` result with empty text and a green card.
    ///
    /// `None` means the process was terminated by a signal and reported no code.
    ///
    /// Streams output in real-time to the client using logging notifications.
    async fn execute_shell_command(
        &self,
        command: &str,
        working_dir: Option<PathBuf>,
        peer: &rmcp::service::Peer<RoleServer>,
        cancellation_token: CancellationToken,
    ) -> Result<(String, Option<i32>), ErrorData> {
        let mut shell_config = ShellConfig::default();
        let shell_name = std::path::Path::new(&shell_config.executable)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("bash");

        if let Some(ref env_file) = self.bash_env_file {
            if shell_name == "bash" {
                shell_config.envs.push((
                    OsString::from("BASH_ENV"),
                    env_file.clone().into_os_string(),
                ))
            }
        }

        // BR-69: under `BIOROUTER_SHELL_SANDBOX=strict` on a host that cannot
        // provide a full sandbox, refuse to run rather than silently degrade.
        let mut command = configure_shell_command(&shell_config, command, working_dir.as_deref())
            .map_err(|e| ErrorData::new(ErrorCode::INVALID_REQUEST, e, None))?;
        // The assistant-visible sandbox tier line, prepended to the output so the
        // model (and a bug reporter) can see which enforcement actually applied.
        let sandbox_status_line = super::shell::shell_sandbox_status_line(working_dir.as_deref());

        if self.extend_path_with_shell {
            if let Err(e) = get_shell_path_dirs()
                .await
                .and_then(|dirs| join_paths(dirs).map_err(|e| anyhow!(e)))
                .map(|path| command.env("PATH", path))
            {
                tracing::error!("Failed to extend PATH with shell directories: {}", e)
            }
        }

        let mut child = command
            .spawn()
            .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;

        let pid = child.id();
        if let Some(pid) = pid {
            tracing::debug!("Shell process spawned with PID: {}", pid);
        } else {
            tracing::warn!("Shell process spawned but PID not available");
        }

        // Stream the output and wait for completion with cancellation support
        let output_task = self.stream_shell_output(
            child.stdout.take().unwrap(),
            child.stderr.take().unwrap(),
            peer.clone(),
        );

        tokio::select! {
            output_result = output_task => {
                // Wait for the process to complete. PAR-02: the status is the
                // only signal that a silent command failed — carry it out.
                let exit_status = child.wait().await.map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                // BR-69: prepend the sandbox tier line when the gate is on.
                output_result.map(|out| {
                    let out = match sandbox_status_line {
                        Some(line) => format!("{line}\n{out}"),
                        None => out,
                    };
                    (out, exit_status.code())
                })
            }
            _ = cancellation_token.cancelled() => {
                tracing::info!("Cancellation token triggered! Attempting to kill process and all child processes");

                // Kill the process and its children using platform-specific approach
                match kill_process_group(&mut child, pid).await {
                    Ok(_) => {
                        tracing::debug!("Successfully killed shell process and child processes");
                    }
                    Err(e) => {
                        tracing::error!("Failed to kill shell process and child processes: {}", e);
                    }
                }

                Err(ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    "Shell command was cancelled by user".to_string(),
                    None,
                ))
            }
        }
    }

    /// Stream shell output in real-time and return the combined output.
    ///
    /// Merges stdout and stderr streams and sends each line as a logging notification.
    async fn stream_shell_output(
        &self,
        stdout: tokio::process::ChildStdout,
        stderr: tokio::process::ChildStderr,
        peer: rmcp::service::Peer<RoleServer>,
    ) -> Result<String, ErrorData> {
        let stdout = BufReader::new(stdout);
        let stderr = BufReader::new(stderr);

        let output_task = tokio::spawn(async move {
            let mut combined_output = String::new();

            // Merge stdout and stderr streams
            // ref https://blog.yoshuawuyts.com/futures-concurrency-3
            let stdout = SplitStream::new(stdout.split(b'\n')).map(|v| ("stdout", v));
            let stderr = SplitStream::new(stderr.split(b'\n')).map(|v| ("stderr", v));
            let mut merged = stdout.merge(stderr);

            while let Some((stream_type, line)) = merged.next().await {
                let mut line = line?;
                // Re-add newline as clients expect it
                line.push(b'\n');
                // Convert to UTF-8 to avoid corrupted output
                let line_str = String::from_utf8_lossy(&line);

                combined_output.push_str(&line_str);

                // Stream each line back to the client in real-time
                let trimmed_line = line_str.trim();
                if !trimmed_line.is_empty() {
                    // Send the output line as a structured logging message
                    if let Err(e) = peer
                        .notify_logging_message(LoggingMessageNotificationParam {
                            level: LoggingLevel::Info,
                            data: serde_json::json!({
                                "type": "shell_output",
                                "stream": stream_type,
                                "output": trimmed_line
                            }),
                            logger: Some("shell_tool".to_string()),
                        })
                        .await
                    {
                        // Don't break execution if streaming fails, just log it
                        eprintln!("Failed to stream output line: {}", e);
                    }
                }
            }
            Ok::<_, std::io::Error>(combined_output)
        });

        match output_task.await {
            Ok(result) => {
                result.map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))
            }
            Err(e) => Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                e.to_string(),
                None,
            )),
        }
    }

    /// Validate that shell output doesn't exceed size limits.
    fn validate_shell_output_size(&self, command: &str, output: &str) -> Result<(), ErrorData> {
        const MAX_CHAR_COUNT: usize = 400_000; // 400KB
        let char_count = output.chars().count();

        if char_count > MAX_CHAR_COUNT {
            return Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!(
                    "Shell output from command '{}' has too many characters ({}). Maximum character count is {}.",
                    command,
                    char_count,
                    MAX_CHAR_COUNT
                ),
                None,
            ));
        }

        Ok(())
    }

    /// Analyze code structure and relationships.
    ///
    /// Automatically selects the appropriate analysis:
    /// - Files: Semantic analysis with call graphs
    /// - Directories: Structure overview with metrics
    /// - With focus parameter: Track symbol across files
    ///
    /// Examples:
    /// analyze(path="file.py") -> semantic analysis
    /// analyze(path="src/") -> structure overview down to max_depth subdirs
    /// analyze(path="src/", focus="main") -> track main() across files in src/ down to max_depth subdirs
    #[tool(
        name = "analyze",
        description = "Analyze code structure in 3 modes: 1) Directory overview - file tree with LOC/function/class counts to max_depth. 2) File details - functions, classes, imports. 3) Symbol focus - call graphs across directory to max_depth (requires directory path, case-sensitive). Typical flow: directory → files → symbols. Functions called >3x show •N."
    )]
    pub async fn analyze(
        &self,
        params: Parameters<AnalyzeParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let path = self.resolve_path(&params.path)?;
        self.code_analyzer
            .analyze(params, path, self.secret_guard.gitignore())
    }

    /// Process an image file from disk.
    ///
    /// The image will be:
    /// 1. Resized if larger than max width while maintaining aspect ratio
    /// 2. Converted to PNG format
    /// 3. Returned as base64 encoded data
    ///
    /// This allows processing image files for use in the conversation.
    #[tool(
        name = "image_processor",
        description = "Process an image file from disk. Resizes if needed, converts to PNG, and returns as base64 data."
    )]
    pub async fn image_processor(
        &self,
        params: Parameters<ImageProcessorParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let path_str = &params.path;

        let path = {
            let p = self.resolve_path(path_str)?;
            if cfg!(target_os = "macos") {
                self.normalize_mac_screenshot_path(&p)
            } else {
                p
            }
        };

        // Check if file is ignored before proceeding
        if self.is_ignored(&path) {
            return Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!(
                    "Access to '{}' is restricted by .biorouterignore",
                    path.display()
                ),
                None,
            ));
        }

        // Check if file exists
        if !path.exists() {
            return Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("File '{}' does not exist", path.display()),
                None,
            ));
        }

        // Check file size (10MB limit for image files)
        const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024; // 10MB in bytes
        let file_size = std::fs::metadata(&path)
            .map_err(|e| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Failed to get file metadata: {}", e),
                    None,
                )
            })?
            .len();

        if file_size > MAX_FILE_SIZE {
            return Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!(
                    "File '{}' is too large ({:.2}MB). Maximum size is 10MB.",
                    path.display(),
                    file_size as f64 / (1024.0 * 1024.0)
                ),
                None,
            ));
        }

        // Open and decode the image
        let image = xcap::image::open(&path).map_err(|e| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to open image file: {}", e),
                None,
            )
        })?;

        // Resize if necessary (same logic as screen_capture)
        let mut processed_image = image;
        let max_width = 768;
        if processed_image.width() > max_width {
            let scale = max_width as f32 / processed_image.width() as f32;
            let new_height = (processed_image.height() as f32 * scale) as u32;
            processed_image = xcap::image::DynamicImage::ImageRgba8(xcap::image::imageops::resize(
                &processed_image,
                max_width,
                new_height,
                xcap::image::imageops::FilterType::Lanczos3,
            ));
        }

        // Convert to PNG and encode as base64
        let mut bytes: Vec<u8> = Vec::new();
        processed_image
            .write_to(&mut Cursor::new(&mut bytes), xcap::image::ImageFormat::Png)
            .map_err(|e| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Failed to write image buffer: {}", e),
                    None,
                )
            })?;

        let data = base64::prelude::BASE64_STANDARD.encode(bytes);

        Ok(CallToolResult::success(vec![
            Content::text(format!(
                "Successfully processed image from {}",
                path.display()
            ))
            .with_audience(vec![Role::Assistant]),
            Content::image(data, "image/png").with_priority(0.0),
        ]))
    }

    /// The base a caller explicitly *sanctioned* for this server: the session
    /// working directory it was constructed with, else `BIOROUTER_WORKING_DIR`
    /// (which the app sets to the same value for child extension processes).
    ///
    /// Unlike [`Self::session_cwd_or_fallback`] this does not check existence —
    /// it answers "was a base ever chosen for us?", which is what separates
    /// "the process cwd *is* the intended base" (plain `biorouter session`, no
    /// folder picked) from "the intended base has disappeared". An empty env var
    /// counts as unset, since the desktop app writes `''` when no folder is
    /// selected.
    fn sanctioned_base(&self) -> Option<PathBuf> {
        self.working_dir.clone().or_else(|| {
            std::env::var("BIOROUTER_WORKING_DIR")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .map(PathBuf::from)
        })
    }

    // Helper method to resolve and validate file paths
    /// The base directory file operations resolve against and are jailed to:
    /// the session working directory (so the text_editor jail tracks the same
    /// directory the shell runs in), or the process cwd when no base was ever
    /// chosen.
    ///
    /// **This is a jail base, so it is never guessed** (#64). When a base *was*
    /// sanctioned and has since vanished, this fails instead of quietly
    /// substituting the process cwd or `"."`: those are different directories —
    /// usually much wider ones (`/` under the desktop app) — and re-rooting the
    /// jail there would hand the tools every path the sanctioned base excluded,
    /// under a justification ("don't panic") that has nothing to do with the
    /// boundary it moves. Failing the call keeps the boundary intact and says
    /// exactly what went wrong; only the caller's directory can restore it.
    ///
    /// Note the deliberate asymmetry with [`Self::session_cwd_or_fallback`],
    /// which the shell uses: *where a command runs* may fall back, because the
    /// shell is not jailed by this base at all and a fallback grants nothing.
    /// *What the file tools may touch* may not. That asymmetry is why this
    /// resolves the base itself rather than calling the shell's helper — see
    /// #68 below.
    ///
    /// **The base is the one the caller was actually given** (#68). This reads
    /// [`Self::sanctioned_base`] and requires *that* directory to exist; it does
    /// not walk the shell's candidate list. Sharing that list left one live
    /// substitution behind #64's: when `working_dir` vanished but
    /// `BIOROUTER_WORKING_DIR` survived, the jail moved to the env directory.
    /// Both are values the application sanctioned, so it was never an escape to
    /// an arbitrary path — but a session working in a *subdirectory* of the env
    /// base had its jail widened to the parent when that subdirectory was
    /// deleted, and a sibling file refused a moment earlier became writable
    /// (measured, not inferred). "Sanctioned somewhere" is not the property that
    /// matters; "the base this jail was built from" is. A wider directory the
    /// app also blessed is still a different directory.
    fn effective_cwd(&self) -> Result<PathBuf, ErrorData> {
        if let Some(base) = self.sanctioned_base() {
            if base.is_dir() {
                return Ok(base);
            }
            return Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!(
                    "The working directory `{}` no longer exists, so file paths cannot be \
                     resolved. File access stays confined to that directory and is not \
                     re-rooted elsewhere: recreate it, or start a session in a directory \
                     that exists, and retry.",
                    base.display()
                ),
                None,
            ));
        }
        // No base was ever chosen, so the process cwd is the intended one. If
        // even that is gone (the directory a `biorouter session` started in was
        // deleted) there is nothing to resolve against — fail this call rather
        // than the whole process.
        std::env::current_dir().map_err(|e| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!(
                    "No working directory is available to resolve file paths against: the \
                     process working directory could not be read ({e}). Run from a directory \
                     that exists, or set BIOROUTER_WORKING_DIR."
                ),
                None,
            )
        })
    }

    fn resolve_path(&self, path_str: &str) -> Result<PathBuf, ErrorData> {
        self.resolve_path_jailed(path_str, path_jail_relaxed())
    }

    /// Resolve `path_str` against the effective working directory, applying the
    /// containment jail unless `jail_relaxed` is set.
    ///
    /// `jail_relaxed` mirrors [`BioRouterMode::Auto`] (via [`path_jail_relaxed`]):
    /// in Auto mode the agent's `SensitiveOpsInspector` gates sensitive-path
    /// writes through approval, so this convenience jail is skipped and any
    /// non-sensitive target (e.g. `/tmp/out.md`) resolves instead of being
    /// rejected as "outside the working directory". In every other mode the jail
    /// is enforced exactly as before.
    fn resolve_path_jailed(
        &self,
        path_str: &str,
        jail_relaxed: bool,
    ) -> Result<PathBuf, ErrorData> {
        let expanded = expand_path(path_str);
        let path = Path::new(&expanded);

        // Ask for the base only where it is actually used: an absolute path
        // needs none, so Auto mode keeps resolving one even when the working
        // directory is gone. A relative path is meaningless without a base.
        let resolved = if is_absolute_path(&expanded) {
            path.to_path_buf()
        } else {
            self.effective_cwd()?.join(path)
        };

        // Auto mode: the containment jail is relaxed (sensitive writes are still
        // gated upstream by the agent's SensitiveOpsInspector), so skip the
        // outside-working-directory check entirely.
        if jail_relaxed {
            return Ok(resolved);
        }

        // Enforcing the jail always needs the base.
        let cwd = self.effective_cwd()?;

        // Canonicalize the cwd for comparison
        let canonical_cwd = std::fs::canonicalize(&cwd).map_err(|e| {
            ErrorData::new(
                rmcp::model::ErrorCode::INTERNAL_ERROR,
                format!("Failed to canonicalize working directory: {}", e),
                None,
            )
        })?;

        // Check that the resolved path stays within cwd
        if resolved.exists() {
            let canonical_resolved = std::fs::canonicalize(&resolved).map_err(|e| {
                ErrorData::new(
                    rmcp::model::ErrorCode::INTERNAL_ERROR,
                    format!("Failed to canonicalize path: {}", e),
                    None,
                )
            })?;
            if !canonical_resolved.starts_with(&canonical_cwd) {
                return Err(ErrorData::new(
                    rmcp::model::ErrorCode::INVALID_PARAMS,
                    format!("Path '{}' is outside the working directory", path_str),
                    None,
                ));
            }
        } else {
            // Path doesn't exist yet — check its nearest existing ancestor
            let mut ancestor = resolved.as_path();
            loop {
                if let Some(parent) = ancestor.parent() {
                    if parent.exists() {
                        let canonical_parent = std::fs::canonicalize(parent).map_err(|e| {
                            ErrorData::new(
                                rmcp::model::ErrorCode::INTERNAL_ERROR,
                                format!("Failed to canonicalize path: {}", e),
                                None,
                            )
                        })?;
                        if !canonical_parent.starts_with(&canonical_cwd) {
                            return Err(ErrorData::new(
                                rmcp::model::ErrorCode::INVALID_PARAMS,
                                format!("Path '{}' is outside the working directory", path_str),
                                None,
                            ));
                        }
                        break;
                    }
                    ancestor = parent;
                } else {
                    // Reached root without finding an existing ancestor within cwd
                    return Err(ErrorData::new(
                        rmcp::model::ErrorCode::INVALID_PARAMS,
                        format!("Path '{}' is outside the working directory", path_str),
                        None,
                    ));
                }
            }
        }

        Ok(resolved)
    }

    // Helper method to check if a path should be ignored. Delegates to the
    // shared `SecretGuard` (BR-23) so the Developer server and the central
    // extension-manager dispatch boundary enforce the same deny set.
    fn is_ignored(&self, path: &Path) -> bool {
        self.secret_guard.is_denied(path)
    }

    // Only returns true when 100% certain (checks /proc/1/cgroup for container markers)
    fn is_definitely_container() -> bool {
        let Ok(content) = std::fs::read_to_string("/proc/1/cgroup") else {
            // If the file doesn't exist, we're definitely not in a Linux container
            return false;
        };

        // Check for definitive container markers in cgroup paths
        for line in content.lines() {
            if line.contains("/docker/")
                || line.contains("/docker-")
                || line.contains("/kubepods/")
                || line.contains("/libpod-")
                || line.contains("/lxc/")
                || line.contains("/containerd/")
            {
                return true;
            }
        }

        // Check for cgroups v2 unified hierarchy in containers
        // In Docker with cgroups v2, we typically see just "0::/"
        // This is a strong signal when it's the only line
        if content.trim() == "0::/" {
            return true;
        }

        false
    }

    // Helper function to handle Mac screenshot filenames that contain U+202F (narrow no-break space)
    fn normalize_mac_screenshot_path(&self, path: &Path) -> PathBuf {
        // Compiled once rather than on every call.
        static SCREENSHOT_RE: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(
            || {
                regex::Regex::new(
                    r"^Screenshot \d{4}-\d{2}-\d{2} at \d{1,2}\.\d{2}\.\d{2} (AM|PM|am|pm)(?: \(\d+\))?\.png$",
                )
                .expect("valid regex")
            },
        );
        // Only process if the path has a filename
        if let Some(filename) = path.file_name().and_then(|f| f.to_str()) {
            // Check if this matches Mac screenshot pattern:
            // "Screenshot YYYY-MM-DD at H.MM.SS AM/PM.png"
            if let Some(captures) = SCREENSHOT_RE.captures(filename) {
                // Get the AM/PM part
                let meridian = captures.get(1).unwrap().as_str();

                // Find the last space before AM/PM and replace it with U+202F
                let space_pos = filename
                    .rfind(meridian)
                    .and_then(|pos| filename.get(..pos).map(|s| s.trim_end().len()))
                    .unwrap_or(0);

                if space_pos > 0 {
                    let parent = path.parent().unwrap_or(Path::new(""));
                    if let (Some(before), Some(after)) =
                        (filename.get(..space_pos), filename.get(space_pos + 1..))
                    {
                        let new_filename = format!("{}{}{}", before, '\u{202F}', after);
                        let new_path = parent.join(new_filename);

                        return new_path;
                    }
                }
            }
        }

        // Return the original path if it doesn't match or couldn't be processed
        path.to_path_buf()
    }

    // shell output can be large, this will help manage that
    fn process_shell_output(&self, output_str: &str) -> Result<(String, String), ErrorData> {
        let lines: Vec<&str> = output_str.lines().collect();
        let line_count = lines.len();

        let start = lines.len().saturating_sub(100);
        let last_100_lines_str = lines[start..].join("\n");

        let final_output = if line_count > 100 {
            let tmp_file = tempfile::NamedTempFile::new().map_err(|e| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Failed to create temporary file: {}", e),
                    None,
                )
            })?;

            std::fs::write(tmp_file.path(), output_str).map_err(|e| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Failed to write to temporary file: {}", e),
                    None,
                )
            })?;

            let (_, path) = tmp_file.keep().map_err(|e| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Failed to persist temporary file: {}", e),
                    None,
                )
            })?;

            format!(
                "private note: output was {} lines and we are only showing the most recent lines, remainder of lines in {} do not show tmp file to user, that file can be searched if extra context needed to fulfill request. truncated output: \n{}",
                line_count,
                path.display(),
                last_100_lines_str
            )
        } else {
            output_str.to_string()
        };

        let user_output = if line_count > 100 {
            format!(
                "NOTE: Output was {} lines, showing only the last 100 lines.\n\n{}",
                line_count, last_100_lines_str
            )
        } else {
            output_str.to_string()
        };

        Ok((final_output, user_output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_monitors_empty_is_safe_and_explains_indexing() {
        // Must not panic on zero monitors and must always tell the agent how
        // the `display` index works.
        let s = describe_monitors(&[]);
        assert!(s.contains("Connected displays:"));
        assert!(s.contains("display"));
    }

    #[test]
    fn describe_monitors_lists_each_connected_display_with_index() {
        // Environment-dependent: only assert structure when displays exist
        // (headless CI may have none).
        if let Ok(monitors) = Monitor::all() {
            if !monitors.is_empty() {
                let s = describe_monitors(&monitors);
                assert!(s.contains("0:"), "should index the first display: {s}");
                // One line per monitor plus the header and the trailing hint.
                let display_lines = s
                    .lines()
                    .filter(|l| l.trim_start().starts_with(|c: char| c.is_ascii_digit()))
                    .count();
                assert_eq!(
                    display_lines,
                    monitors.len(),
                    "every connected display should be listed: {s}"
                );
            }
        }
    }

    #[test]
    fn test_text_editor_params_accepts_file_path_alias() {
        // Some models (e.g. Xiaomi MiMo) intermittently emit `file_path` instead
        // of `path`; the alias prevents an opaque -32602 deserialization failure.
        let with_alias: TextEditorParams = serde_json::from_value(serde_json::json!({
            "file_path": "/repo/src/lib.rs",
            "command": "view"
        }))
        .expect("file_path alias should deserialize");
        assert_eq!(with_alias.path, "/repo/src/lib.rs");
        assert_eq!(with_alias.command, "view");

        // Canonical `path` still works.
        let canonical: TextEditorParams = serde_json::from_value(serde_json::json!({
            "path": "/repo/src/lib.rs",
            "command": "view"
        }))
        .expect("path should deserialize");
        assert_eq!(canonical.path, "/repo/src/lib.rs");
    }

    use rmcp::handler::server::wrapper::Parameters;
    use rmcp::model::{CancelledNotificationParam, NumberOrString};
    use rmcp::service::{serve_directly, NotificationContext};
    use rmcp::ServerHandler;
    use serial_test::serial;
    use std::{
        fs,
        time::{Duration, Instant},
    };
    use tempfile::TempDir;
    use tokio::io::AsyncReadExt;
    use tokio::time::timeout;

    use crate::developer::shell::normalize_line_endings;

    fn create_test_server() -> DeveloperServer {
        DeveloperServer::new()
    }

    /// #2: the shell runs in the session working directory, an absolute
    /// `working_directory` overrides it, and a relative one resolves against it.
    #[test]
    fn shell_cwd_prefers_session_dir_and_override() {
        let server = DeveloperServer::new().with_working_dir(PathBuf::from("/session/dir"));
        assert_eq!(
            server.resolve_shell_cwd(None),
            Some(PathBuf::from("/session/dir"))
        );
        assert_eq!(
            server.resolve_shell_cwd(Some("/other")),
            Some(PathBuf::from("/other"))
        );
        assert_eq!(
            server.resolve_shell_cwd(Some("sub/dir")),
            Some(PathBuf::from("/session/dir/sub/dir"))
        );
        // Blank override is ignored.
        assert_eq!(
            server.resolve_shell_cwd(Some("   ")),
            Some(PathBuf::from("/session/dir"))
        );
        // An absolute override still works with no session dir.
        assert_eq!(
            DeveloperServer::new().resolve_shell_cwd(Some("/abs")),
            Some(PathBuf::from("/abs"))
        );
    }

    /// #2 regression: a per-call `working_directory` that does not exist is an
    /// explicit request that must fail loudly, naming the path — never silently
    /// run somewhere else.
    #[test]
    fn shell_cwd_missing_override_is_error() {
        let server = DeveloperServer::new();
        let missing = "/no/such/dir/for/biorouter-test";
        let err = server
            .resolve_shell_cwd_checked(Some(missing))
            .expect_err("a missing override must error");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(
            err.message.contains(missing),
            "error should name the offending path: {}",
            err.message
        );
    }

    /// #2 regression: a per-call override pointing at a file (not a directory)
    /// is likewise rejected.
    #[test]
    fn shell_cwd_override_that_is_a_file_is_error() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("not-a-dir.txt");
        std::fs::write(&file, "x").unwrap();
        let server = DeveloperServer::new();
        let err = server
            .resolve_shell_cwd_checked(Some(file.to_str().unwrap()))
            .expect_err("a file override must error");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    }

    /// #2 regression: a session working dir that no longer exists must NOT jam
    /// the shell — resolution falls back to the next candidate.
    #[test]
    #[serial]
    fn shell_cwd_missing_session_dir_falls_back() {
        // Isolate from a stray BIOROUTER_WORKING_DIR so the fallback lands on
        // the process cwd (None).
        let saved = std::env::var("BIOROUTER_WORKING_DIR").ok();
        std::env::remove_var("BIOROUTER_WORKING_DIR");

        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_path_buf();
        let server = DeveloperServer::new().with_working_dir(dir.clone());
        // While it exists, it is honored.
        assert_eq!(
            server.resolve_shell_cwd_checked(None).unwrap(),
            Some(dir.clone())
        );
        // Once deleted, resolution falls back rather than returning the dead dir.
        drop(tmp);
        let resolved = server.resolve_shell_cwd_checked(None).unwrap();
        assert_ne!(
            resolved,
            Some(dir),
            "must not resolve to the deleted session dir"
        );
        assert_eq!(resolved, None, "with no env dir, falls back to process cwd");

        if let Some(v) = saved {
            std::env::set_var("BIOROUTER_WORKING_DIR", v);
        }
    }

    /// #2 regression: a missing session dir falls THROUGH to a still-present
    /// BIOROUTER_WORKING_DIR before the process cwd.
    #[test]
    #[serial]
    fn shell_cwd_missing_session_dir_falls_back_to_env() {
        let saved = std::env::var("BIOROUTER_WORKING_DIR").ok();
        let env_dir = tempfile::tempdir().unwrap();
        std::env::set_var("BIOROUTER_WORKING_DIR", env_dir.path());

        let gone = tempfile::tempdir().unwrap();
        let gone_path = gone.path().to_path_buf();
        drop(gone);
        let server = DeveloperServer::new().with_working_dir(gone_path);
        assert_eq!(
            server.resolve_shell_cwd_checked(None).unwrap(),
            Some(env_dir.path().to_path_buf())
        );

        match saved {
            Some(v) => std::env::set_var("BIOROUTER_WORKING_DIR", v),
            None => std::env::remove_var("BIOROUTER_WORKING_DIR"),
        }
    }

    /// #2 regression: BIOROUTER_WORKING_DIR pointing at a missing dir warns and
    /// falls back to the process cwd instead of erroring.
    #[test]
    #[serial]
    fn shell_cwd_missing_env_dir_falls_back() {
        let saved = std::env::var("BIOROUTER_WORKING_DIR").ok();
        std::env::set_var("BIOROUTER_WORKING_DIR", "/no/such/env/dir/biorouter");
        let server = DeveloperServer::new();
        assert_eq!(server.resolve_shell_cwd_checked(None).unwrap(), None);

        match saved {
            Some(v) => std::env::set_var("BIOROUTER_WORKING_DIR", v),
            None => std::env::remove_var("BIOROUTER_WORKING_DIR"),
        }
    }

    /// #2 wiring: an actually-executed shell command runs in the session dir.
    #[test]
    #[serial]
    fn shell_executes_in_session_dir() {
        run_shell_test(|| async {
            let tmp = tempfile::tempdir().unwrap();
            // Resolve symlinks (macOS /var → /private/var) so `pwd` matches.
            let dir = std::fs::canonicalize(tmp.path()).unwrap();
            let server = DeveloperServer::new().with_working_dir(dir.clone());
            let running_service = serve_directly(server.clone(), create_test_transport(), None);
            let peer = running_service.peer().clone();

            let command = if cfg!(windows) {
                "(Get-Location).Path"
            } else {
                "pwd"
            };
            let result = server
                .shell(
                    Parameters(ShellParams {
                        working_directory: None,
                        command: command.to_string(),
                        background: None,
                        label: None,
                    }),
                    RequestContext {
                        ct: Default::default(),
                        id: NumberOrString::Number(4242),
                        meta: Default::default(),
                        extensions: Default::default(),
                        peer: peer.clone(),
                    },
                )
                .await
                .expect("pwd should run");
            let text = result
                .content
                .iter()
                .find_map(|c| c.as_text())
                .expect("shell output has text")
                .text
                .clone();
            let expected_dir = dir
                .to_string_lossy()
                .trim_start_matches(r"\\?\")
                .replace('\\', "/")
                .to_ascii_lowercase();
            let comparable_text = text.replace('\\', "/").to_ascii_lowercase();
            assert!(
                comparable_text.contains(&expected_dir),
                "pwd should report the session dir {}, got: {text}",
                dir.display()
            );

            cleanup_test_service(running_service, peer);
        });
    }

    /// #2 wiring: when the session dir has been deleted, the shell still runs
    /// (falling back to the process cwd) instead of failing to spawn.
    #[test]
    #[serial]
    fn shell_survives_deleted_session_dir() {
        run_shell_test(|| async {
            let saved = std::env::var("BIOROUTER_WORKING_DIR").ok();
            std::env::remove_var("BIOROUTER_WORKING_DIR");

            let tmp = tempfile::tempdir().unwrap();
            let dir = tmp.path().to_path_buf();
            drop(tmp);
            let server = DeveloperServer::new().with_working_dir(dir);
            let running_service = serve_directly(server.clone(), create_test_transport(), None);
            let peer = running_service.peer().clone();

            let result = server
                .shell(
                    Parameters(ShellParams {
                        working_directory: None,
                        command: "pwd".to_string(),
                        background: None,
                        label: None,
                    }),
                    RequestContext {
                        ct: Default::default(),
                        id: NumberOrString::Number(4343),
                        meta: Default::default(),
                        extensions: Default::default(),
                        peer: peer.clone(),
                    },
                )
                .await;
            assert!(
                result.is_ok(),
                "shell must keep working after the session dir disappears: {result:?}"
            );

            cleanup_test_service(running_service, peer);
            if let Some(v) = saved {
                std::env::set_var("BIOROUTER_WORKING_DIR", v);
            }
        });
    }

    /// #2: the text_editor path jail follows the session working directory too,
    /// so file edits and shell commands agree on where "here" is.
    #[test]
    fn editor_path_jail_uses_session_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let server = DeveloperServer::new().with_working_dir(tmp.path().to_path_buf());
        // A (not-yet-existing) path inside the session dir resolves.
        assert!(server
            .resolve_path(tmp.path().join("new.txt").to_str().unwrap())
            .is_ok());
        // A path outside the session dir is rejected by the jail.
        assert!(server.resolve_path("/etc/hosts").is_err());
    }

    /// GATE (2026-07-19 tool-errors audit, ITER-1): in Auto mode the containment
    /// jail is relaxed, so a write target outside the session working directory
    /// resolves instead of being rejected as "outside the working directory".
    /// Every other mode keeps the jail. Exercises the pure `jail_relaxed`
    /// parameter directly (no global mutation) so it is race-free under the test
    /// harness; the live global is driven by `biorouter`'s agent from the mode.
    #[test]
    fn auto_mode_relaxes_path_jail_outside_working_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let server = DeveloperServer::new().with_working_dir(tmp.path().to_path_buf());
        // A target outside the session working dir (a sibling temp dir).
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("br-jail-relax-probe.md");
        let target_str = target.to_str().unwrap();

        // Jail enforced (non-Auto modes): rejected.
        assert!(
            server.resolve_path_jailed(target_str, false).is_err(),
            "with the jail enforced, an outside-working-dir path must be rejected"
        );
        // Jail relaxed (Auto mode): the path resolves.
        let resolved = server
            .resolve_path_jailed(target_str, true)
            .expect("Auto mode must relax the jail and resolve an outside path");
        assert_eq!(resolved, target);
    }

    /// #2 regression, narrowed by #64: if the session dir is deleted, the editor
    /// jail must not blow up on a missing root — the #2 failure was an opaque
    /// "Failed to canonicalize working directory" INTERNAL_ERROR. That intent is
    /// kept: the caller gets a clean error that names the directory and says how
    /// to fix it.
    ///
    /// What #64 overturns is the *other* half of the original #2 fix, which made
    /// this resolve against the process cwd instead. That silently moved the
    /// jail base — the one value bounding every path these tools may touch —
    /// onto a directory nobody sanctioned, and the widening is asserted against
    /// directly in `editor_jail_is_not_widened_when_session_dir_disappears`. The
    /// shell keeps its fallback (see `shell_cwd_missing_session_dir_falls_back`):
    /// it is not jailed by this base, so running elsewhere grants nothing.
    #[test]
    #[serial]
    fn editor_path_jail_refuses_when_session_dir_missing() {
        let saved = std::env::var("BIOROUTER_WORKING_DIR").ok();
        std::env::remove_var("BIOROUTER_WORKING_DIR");

        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_path_buf();
        drop(tmp);
        let server = DeveloperServer::new().with_working_dir(dir.clone());
        let err = server.resolve_path_jailed("scratch.txt", false).expect_err(
            "a relative path has no base to resolve against once the session dir is gone",
        );

        if let Some(v) = saved {
            std::env::set_var("BIOROUTER_WORKING_DIR", v);
        }

        assert_eq!(err.code, ErrorCode::INTERNAL_ERROR);
        assert!(
            err.message.contains(&dir.display().to_string()),
            "the error must name the directory that vanished, got: {}",
            err.message
        );
        assert!(
            !err.message.contains("canonicalize"),
            "and must not surface as the opaque canonicalize failure #2 fixed, got: {}",
            err.message
        );
    }

    /// #64: the jail base is not re-rooted when the session working directory
    /// disappears. Every path the developer tools may touch is resolved relative
    /// to this base, so quietly substituting the process cwd — a *different*,
    /// usually much wider directory (`/` under the desktop app) — hands the tools
    /// everything the sanctioned base excluded, and does it under a justification
    /// ("don't panic") that has nothing to do with the boundary it moves.
    #[test]
    #[serial]
    fn editor_jail_is_not_widened_when_session_dir_disappears() {
        let saved_env = std::env::var("BIOROUTER_WORKING_DIR").ok();
        std::env::remove_var("BIOROUTER_WORKING_DIR");
        let saved_cwd = std::env::current_dir().ok();

        // The process sits in `outside`; the session is jailed to `session`.
        let outside = tempfile::tempdir().unwrap();
        let outside_path = std::fs::canonicalize(outside.path()).unwrap();
        std::env::set_current_dir(&outside_path).unwrap();

        let session = tempfile::tempdir().unwrap();
        let server = DeveloperServer::new().with_working_dir(session.path().to_path_buf());

        // A real file that lives in the process cwd but outside the session jail.
        let probe = outside_path.join("outside-the-jail.txt");
        std::fs::write(&probe, "not for the tools").unwrap();
        let probe_str = probe.to_str().unwrap().to_string();
        // Sanity: while the session dir exists the jail refuses it.
        assert!(
            server.resolve_path_jailed(&probe_str, false).is_err(),
            "sanity: a path outside the session dir must be refused"
        );

        // The session directory disappears mid-session.
        drop(session);

        let outcome = server.resolve_path_jailed(&probe_str, false);

        if let Some(dir) = saved_cwd {
            let _ = std::env::set_current_dir(dir);
        }
        if let Some(v) = saved_env {
            std::env::set_var("BIOROUTER_WORKING_DIR", v);
        }

        let err = outcome.expect_err(
            "a path outside the session dir must stay refused after the session dir \
             disappears — the jail must not be re-rooted onto the process cwd",
        );
        assert!(
            err.message.contains("no longer exists"),
            "the refusal must name the real reason, got: {}",
            err.message
        );
    }

    /// #68, the related shape: the `SecretGuard` backing `.biorouterignore` is
    /// rooted at construction, before `with_working_dir` is called — so it used
    /// to keep the *process* cwd as its root for the whole session, and a
    /// project's own `.biorouterignore` was never read. Measured with this test
    /// against the pre-fix code: the file was readable through `text_editor`.
    ///
    /// Deliberately touches neither the process cwd nor the environment: the
    /// rule names a file no ambient ignore source mentions, so the assertion can
    /// only pass if the guard is rooted at the session directory.
    #[test]
    fn secret_guard_is_rerooted_onto_the_session_working_directory() {
        let session = tempfile::tempdir().unwrap();
        // A name matching none of DEFAULT_SECRET_PATTERNS, so only the project's
        // own ignore file can deny it.
        std::fs::write(
            session.path().join(".biorouterignore"),
            "proprietary-notes.md\n",
        )
        .unwrap();
        let denied = session.path().join("proprietary-notes.md");
        std::fs::write(&denied, "internal").unwrap();

        let unbound = DeveloperServer::new();
        assert!(
            !unbound.is_ignored(&denied),
            "sanity: with no session dir bound, the project's ignore file is not this \
             server's to read — otherwise the test proves nothing"
        );

        let server = DeveloperServer::new().with_working_dir(session.path().to_path_buf());
        assert!(
            server.is_ignored(&denied),
            "the session directory's .biorouterignore must be honoured once the server \
             is bound to it"
        );
    }

    /// #68: the second base substitution, the one #64 left standing. When the
    /// session directory is a *subdirectory* of `BIOROUTER_WORKING_DIR`, deleting
    /// it used to hand the jail to the env base — widening it to the parent, so a
    /// sibling file the jail refused a moment earlier became reachable.
    ///
    /// Both values are app-sanctioned, so this is not an escape to an arbitrary
    /// path; it is still a base *substitution*, and the base is the one value
    /// bounding every path these tools may touch. The jail now requires the
    /// directory it was actually given, and refuses instead of moving.
    #[test]
    #[serial]
    fn editor_jail_is_not_widened_to_the_env_base_when_session_dir_disappears() {
        let saved_env = std::env::var("BIOROUTER_WORKING_DIR").ok();
        let saved_cwd = std::env::current_dir().ok();

        // The env base is the *parent*; the session works in a subdirectory of
        // it. This is the shape that widens: the two do not vanish together.
        let env_base = tempfile::tempdir().unwrap();
        let env_path = std::fs::canonicalize(env_base.path()).unwrap();
        std::env::set_var("BIOROUTER_WORKING_DIR", &env_path);

        let session_path = env_path.join("session");
        std::fs::create_dir(&session_path).unwrap();
        let server = DeveloperServer::new().with_working_dir(session_path.clone());

        // A real file inside the env base but outside the session jail.
        let probe = env_path.join("sibling.txt");
        std::fs::write(&probe, "not for the tools").unwrap();
        let probe_str = probe.to_str().unwrap().to_string();
        assert!(
            server.resolve_path_jailed(&probe_str, false).is_err(),
            "sanity: a sibling of the session dir must be refused while it exists"
        );

        // The session subdirectory disappears; the env base survives.
        std::fs::remove_dir_all(&session_path).unwrap();
        assert!(env_path.is_dir(), "the env base must still exist");

        let outcome = server.resolve_path_jailed(&probe_str, false);

        if let Some(dir) = saved_cwd {
            let _ = std::env::set_current_dir(dir);
        }
        match saved_env {
            Some(v) => std::env::set_var("BIOROUTER_WORKING_DIR", v),
            None => std::env::remove_var("BIOROUTER_WORKING_DIR"),
        }

        let err = outcome.expect_err(
            "a path outside the session dir must stay refused after the session dir \
             disappears — the jail must not widen to BIOROUTER_WORKING_DIR",
        );
        assert!(
            err.message.contains("no longer exists"),
            "the refusal must name the real reason, got: {}",
            err.message
        );
        assert!(
            err.message.contains(&session_path.display().to_string()),
            "and must name the directory that vanished, got: {}",
            err.message
        );
    }

    /// #64: `effective_cwd` used to `expect()` the process working directory, so
    /// in `biorouter session` — where the process cwd *is* the session cwd —
    /// deleting the directory you started in panicked the whole process on the
    /// next tool call. The tool call must fail; the process must not.
    #[test]
    #[serial]
    fn resolve_path_errors_instead_of_panicking_when_process_cwd_is_gone() {
        let saved_env = std::env::var("BIOROUTER_WORKING_DIR").ok();
        std::env::remove_var("BIOROUTER_WORKING_DIR");
        let saved_cwd = std::env::current_dir().ok();

        // No session working directory: the process cwd is the intended base.
        let server = DeveloperServer::new();
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        drop(tmp);

        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            server.resolve_path_jailed("scratch.txt", false)
        }));

        // Put the process back on a real directory *before* asserting, so a
        // failure here cannot strand the rest of the suite without a cwd.
        if let Some(dir) = saved_cwd {
            let _ = std::env::set_current_dir(dir);
        }
        if let Some(v) = saved_env {
            std::env::set_var("BIOROUTER_WORKING_DIR", v);
        }

        let resolved =
            outcome.expect("a vanished working directory must not panic the whole process");
        let err = resolved.expect_err("with no base to resolve against, the call must fail");
        assert_eq!(err.code, ErrorCode::INTERNAL_ERROR);
        assert!(
            err.message.contains("working directory"),
            "the error must say what is missing, got: {}",
            err.message
        );
    }

    /// #64, the sibling site: `get_info` runs during `initialize` and carried
    /// the same `expect()`. It now reads the one base helper, so it reports the
    /// missing directory rather than panicking — and, because that helper is the
    /// jail base, the instructions can no longer advertise a directory the tools
    /// would refuse to work in.
    #[test]
    #[serial]
    fn get_info_reports_a_missing_working_directory() {
        let saved = std::env::var("BIOROUTER_WORKING_DIR").ok();
        std::env::remove_var("BIOROUTER_WORKING_DIR");

        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_path_buf();
        drop(tmp);
        let server = DeveloperServer::new().with_working_dir(dir.clone());

        let instructions = server.get_info().instructions.unwrap_or_default();

        if let Some(v) = saved {
            std::env::set_var("BIOROUTER_WORKING_DIR", v);
        }

        assert!(
            instructions.contains("current directory: unavailable"),
            "the instructions must say the working directory is gone, got: {instructions}"
        );
        assert!(
            !instructions.contains(&dir.display().to_string()),
            "and must not advertise the directory that no longer exists"
        );
    }

    /// Creates a test transport using in-memory streams instead of stdio
    /// This avoids the hanging issues caused by multiple tests competing for stdio
    fn create_test_transport() -> impl rmcp::transport::IntoTransport<
        RoleServer,
        std::io::Error,
        rmcp::transport::async_rw::TransportAdapterAsyncCombinedRW,
    > {
        let (mut client, server) = tokio::io::duplex(64 * 1024);
        tokio::spawn(async move {
            let mut buffer = [0_u8; 8192];
            while client.read(&mut buffer).await.unwrap_or(0) != 0 {}
        });
        server
    }

    /// Helper function to run shell tests with proper runtime management
    /// This ensures clean shutdown and prevents hanging tests
    fn run_shell_test<F, Fut, T>(test_fn: F) -> T
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        // Create a separate runtime for this test to ensure clean shutdown
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(test_fn());

        // Force shutdown the runtime to kill ALL spawned tasks
        // This terminates the fire-and-forget tasks that rmcp doesn't track
        rt.shutdown_timeout(std::time::Duration::from_millis(100));

        // Return the test result
        result
    }

    /// Helper function to clean up test services and prevent hanging tests
    /// This should be called at the end of tests that create running services
    fn cleanup_test_service(
        running_service: rmcp::service::RunningService<RoleServer, DeveloperServer>,
        peer: rmcp::service::Peer<RoleServer>,
    ) {
        let cancellation_token = running_service.cancellation_token();
        cancellation_token.cancel();
        drop(peer);
        drop(running_service);
    }

    #[test]
    #[serial]
    fn test_shell_missing_parameters() {
        run_shell_test(|| async {
            let server = create_test_server();
            let running_service = serve_directly(server.clone(), create_test_transport(), None);
            let peer = running_service.peer().clone();

            // Test directly on the server instead of using peer.call_tool
            let result = server
                .shell(
                    Parameters(ShellParams {
                        working_directory: None,
                        command: "".to_string(),
                        background: None,
                        label: None,
                    }),
                    RequestContext {
                        ct: Default::default(),
                        id: NumberOrString::Number(1),
                        meta: Default::default(),
                        extensions: Default::default(),
                        peer: peer.clone(),
                    },
                )
                .await;

            assert!(result.is_err());
            let err = result.err().unwrap();
            assert_eq!(err.code, ErrorCode::INVALID_PARAMS);

            // Force cleanup before runtime shutdown
            cleanup_test_service(running_service, peer);
        });
    }

    #[test]
    #[serial]
    #[cfg(windows)]
    fn test_windows_specific_commands() {
        run_shell_test(|| async {
            let temp_dir = tempfile::tempdir().unwrap();

            let server = create_test_server().with_working_dir(temp_dir.path().to_path_buf());
            let running_service = serve_directly(server.clone(), create_test_transport(), None);
            let peer = running_service.peer().clone();

            let shell_params = Parameters(ShellParams {
                working_directory: None,
                command: "Get-ChildItem | Out-Null; Write-Output biorouter-windows-shell-ok"
                    .to_string(),
                background: None,
                label: None,
            });

            let result = server
                .shell(
                    shell_params,
                    RequestContext {
                        ct: Default::default(),
                        id: NumberOrString::Number(1),
                        meta: Default::default(),
                        extensions: Default::default(),
                        peer: peer.clone(),
                    },
                )
                .await
                .expect("PowerShell command should run");
            assert!(result.content.iter().any(|content| {
                content
                    .as_text()
                    .is_some_and(|text| text.text.contains("biorouter-windows-shell-ok"))
            }));

            let allowed_dir = temp_dir.path().join("windows-path-test");
            std::fs::create_dir(&allowed_dir).unwrap();
            assert_eq!(
                server
                    .resolve_path(allowed_dir.to_str().unwrap())
                    .expect("an absolute Windows path inside the working directory should resolve"),
                allowed_dir
            );

            let system_dir = r"C:\Windows\System32";
            if Path::new(system_dir).exists() {
                let error = server
                    .resolve_path(system_dir)
                    .expect_err("a path outside the working directory must remain jailed");
                assert_eq!(error.code, ErrorCode::INVALID_PARAMS);
            }

            // Force cleanup before runtime shutdown
            cleanup_test_service(running_service, peer);
        });
    }

    #[tokio::test]
    #[serial]
    async fn test_text_editor_size_limits() {
        let temp_dir = tempfile::tempdir().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();
        let server = create_test_server();

        // Test file size limit
        {
            let large_file_path = temp_dir.path().join("large.txt");

            // Create a file larger than 2MB
            let content = "x".repeat(3 * 1024 * 1024); // 3MB
            fs::write(&large_file_path, content).unwrap();

            let view_params = Parameters(TextEditorParams {
                path: large_file_path.to_str().unwrap().to_string(),
                command: "view".to_string(),
                view_range: None,
                file_text: None,
                old_str: None,
                new_str: None,
                insert_line: None,
                diff: None,
            });

            let result = server.text_editor(view_params).await;

            assert!(result.is_err());
            let err = result.err().unwrap();
            assert_eq!(err.code, ErrorCode::INTERNAL_ERROR);
            assert!(err.to_string().contains("too large"));
        }

        // Test character count limit
        {
            let many_chars_path = temp_dir.path().join("many_chars.txt");

            // This is above MAX_FILE_SIZE
            let content = "x".repeat(500_000);
            fs::write(&many_chars_path, content).unwrap();

            let view_params = Parameters(TextEditorParams {
                path: many_chars_path.to_str().unwrap().to_string(),
                command: "view".to_string(),
                view_range: None,
                file_text: None,
                old_str: None,
                new_str: None,
                insert_line: None,
                diff: None,
            });

            let result = server.text_editor(view_params).await;

            assert!(result.is_err());
            let err = result.err().unwrap();
            assert_eq!(err.code, ErrorCode::INTERNAL_ERROR);
            assert!(err.to_string().contains("is too large"));
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_text_editor_write_and_view_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let file_path_str = file_path.to_str().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let server = create_test_server();

        // Create a new file
        let write_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "write".to_string(),
            view_range: None,
            file_text: Some("Hello, world!".to_string()),
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        server.text_editor(write_params).await.unwrap();

        // View the file
        let view_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "view".to_string(),
            view_range: None,
            file_text: None,
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        let view_result = server.text_editor(view_params).await.unwrap();

        assert!(!view_result.content.is_empty());
        let user_content = view_result
            .content
            .iter()
            .find(|c| {
                c.audience()
                    .is_some_and(|roles| roles.contains(&Role::User))
            })
            .unwrap()
            .as_text()
            .unwrap();
        assert!(user_content.text.contains("Hello, world!"));
    }

    /// BR-44: a whole-file `write` snapshots the previous content, so an
    /// overwrite is undoable — not just `str_replace`/`insert`/diff edits.
    #[tokio::test]
    #[serial]
    async fn test_write_then_undo_restores_previous_content() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("doc.txt");
        let file_path_str = file_path.to_str().unwrap().to_string();
        std::env::set_current_dir(&temp_dir).unwrap();
        let server = create_test_server();

        let write = |text: &str| {
            Parameters(TextEditorParams {
                path: file_path_str.clone(),
                command: "write".to_string(),
                view_range: None,
                file_text: Some(text.to_string()),
                old_str: None,
                new_str: None,
                insert_line: None,
                diff: None,
            })
        };

        server.text_editor(write("v1\n")).await.unwrap();
        server.text_editor(write("v2\n")).await.unwrap();
        assert_eq!(
            std::fs::read_to_string(&file_path).unwrap(),
            normalize_line_endings("v2\n")
        );

        let undo = Parameters(TextEditorParams {
            path: file_path_str.clone(),
            command: "undo_edit".to_string(),
            view_range: None,
            file_text: None,
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });
        server.text_editor(undo).await.unwrap();
        assert_eq!(
            std::fs::read_to_string(&file_path).unwrap(),
            normalize_line_endings("v1\n")
        );
    }

    /// BR-44: a `shell` redirect target is pre-snapshotted, so `undo_edit`
    /// reverts shell-driven writes back to their pre-command content.
    #[tokio::test]
    #[serial]
    async fn test_shell_redirect_snapshot_enables_undo() {
        let temp_dir = tempfile::tempdir().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();
        let out = temp_dir.path().join("out.txt");
        std::fs::write(&out, "before\n").unwrap();
        let server = create_test_server();

        // The shell tool snapshots redirect targets before running the command.
        server.snapshot_shell_redirect_targets("echo more >> out.txt", Some(temp_dir.path()));
        // Simulate the append the shell command would perform.
        std::fs::write(&out, "before\nmore\n").unwrap();

        text_editor_undo(&out, &server.file_history).await.unwrap();
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "before\n");
    }

    #[tokio::test]
    #[serial]
    async fn test_text_editor_str_replace() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let file_path_str = file_path.to_str().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let server = create_test_server();

        // Create a new file
        let write_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "write".to_string(),
            view_range: None,
            file_text: Some("Hello, world!".to_string()),
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        server.text_editor(write_params).await.unwrap();

        // Replace string
        let replace_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "str_replace".to_string(),
            view_range: None,
            file_text: None,
            old_str: Some("world".to_string()),
            new_str: Some("Rust".to_string()),
            insert_line: None,
            diff: None,
        });

        let replace_result = server.text_editor(replace_params).await.unwrap();

        let assistant_content = replace_result
            .content
            .iter()
            .find(|c| {
                c.audience()
                    .is_some_and(|roles| roles.contains(&Role::Assistant))
            })
            .unwrap()
            .as_text()
            .unwrap();

        assert!(
            assistant_content.text.contains("The file")
                && assistant_content.text.contains("has been edited")
        );

        // Verify the file contents changed
        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("Hello, Rust!"));
    }

    #[tokio::test]
    #[serial]
    async fn test_text_editor_undo_edit() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let file_path_str = file_path.to_str().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let server = create_test_server();

        // Create a file
        let write_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "write".to_string(),
            view_range: None,
            file_text: Some("Original content".to_string()),
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        server.text_editor(write_params).await.unwrap();

        // Make an edit
        let replace_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "str_replace".to_string(),
            view_range: None,
            file_text: None,
            old_str: Some("Original".to_string()),
            new_str: Some("Modified".to_string()),
            insert_line: None,
            diff: None,
        });

        server.text_editor(replace_params).await.unwrap();

        // Verify the edit was made
        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("Modified content"));

        // Undo the edit
        let undo_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "undo_edit".to_string(),
            view_range: None,
            file_text: None,
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        let undo_result = server.text_editor(undo_params).await.unwrap();

        // Verify undo worked
        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("Original content"));

        let undo_content = undo_result
            .content
            .iter()
            .find(|c| c.as_text().is_some())
            .unwrap()
            .as_text()
            .unwrap();
        assert!(undo_content.text.contains("Undid the last edit"));
    }

    #[tokio::test]
    #[serial]
    async fn test_biorouter_ignore_basic_patterns() {
        let temp_dir = tempfile::tempdir().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        // Create .biorouterignore file with patterns
        fs::write(".biorouterignore", "secret.txt\n*.env").unwrap();

        let server = create_test_server();

        // Test basic file matching
        assert!(
            server.is_ignored(Path::new("secret.txt")),
            "secret.txt should be ignored"
        );
        assert!(
            server.is_ignored(Path::new("./secret.txt")),
            "./secret.txt should be ignored"
        );
        assert!(
            !server.is_ignored(Path::new("not_secret.txt")),
            "not_secret.txt should not be ignored"
        );

        // Test pattern matching
        assert!(
            server.is_ignored(Path::new("test.env")),
            "*.env pattern should match test.env"
        );
        assert!(
            server.is_ignored(Path::new("./test.env")),
            "*.env pattern should match ./test.env"
        );
        assert!(
            !server.is_ignored(Path::new("test.txt")),
            "*.env pattern should not match test.txt"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_text_editor_respects_ignore_patterns() {
        let temp_dir = tempfile::tempdir().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        // Create .biorouterignore file
        fs::write(".biorouterignore", "secret.txt").unwrap();

        let server = create_test_server();

        // Try to write to an ignored file
        let secret_path = temp_dir.path().join("secret.txt");
        let write_params = Parameters(TextEditorParams {
            path: secret_path.to_str().unwrap().to_string(),
            command: "write".to_string(),
            view_range: None,
            file_text: Some("test content".to_string()),
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        let result = server.text_editor(write_params).await;
        assert!(
            result.is_err(),
            "Should not be able to write to ignored file"
        );
        assert_eq!(result.unwrap_err().code, ErrorCode::INTERNAL_ERROR);

        // Try to write to a non-ignored file
        let allowed_path = temp_dir.path().join("allowed.txt");
        let write_params = Parameters(TextEditorParams {
            path: allowed_path.to_str().unwrap().to_string(),
            command: "write".to_string(),
            view_range: None,
            file_text: Some("test content".to_string()),
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        let result = server.text_editor(write_params).await;
        assert!(
            result.is_ok(),
            "Should be able to write to non-ignored file"
        );
    }

    #[test]
    #[serial]
    fn test_shell_respects_ignore_patterns() {
        run_shell_test(|| async {
            let temp_dir = tempfile::tempdir().unwrap();
            std::env::set_current_dir(&temp_dir).unwrap();

            let server = create_test_server();
            let running_service = serve_directly(server.clone(), create_test_transport(), None);
            let peer = running_service.peer().clone();

            // Create an ignored file
            let secret_file_path = temp_dir.path().join("secrets.txt");
            fs::write(&secret_file_path, "secret content").unwrap();

            // try to cat the ignored file
            let result = server
                .shell(
                    Parameters(ShellParams {
                        working_directory: None,
                        command: format!("cat {}", secret_file_path.to_str().unwrap()),
                        background: None,
                        label: None,
                    }),
                    RequestContext {
                        ct: Default::default(),
                        id: NumberOrString::Number(1),
                        meta: Default::default(),
                        extensions: Default::default(),
                        peer: peer.clone(),
                    },
                )
                .await;

            assert!(result.is_err(), "Should not be able to cat ignored file");
            assert_eq!(result.unwrap_err().code, ErrorCode::INTERNAL_ERROR);

            // Try to cat a non-ignored file
            let allowed_file_path = temp_dir.path().join("allowed.txt");
            fs::write(&allowed_file_path, "allowed content").unwrap();

            let result = server
                .shell(
                    Parameters(ShellParams {
                        working_directory: None,
                        command: format!("cat {}", allowed_file_path.to_str().unwrap()),
                        background: None,
                        label: None,
                    }),
                    RequestContext {
                        ct: Default::default(),
                        id: NumberOrString::Number(1),
                        meta: Default::default(),
                        extensions: Default::default(),
                        peer: peer.clone(),
                    },
                )
                .await;

            assert!(result.is_ok(), "Should be able to cat non-ignored file");

            // Clean up
            let cancellation_token = running_service.cancellation_token();
            cancellation_token.cancel();
            drop(peer);
            drop(running_service);
        });
    }

    #[tokio::test]
    #[serial]
    async fn test_text_editor_descriptions() {
        let temp_dir = tempfile::tempdir().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        // Test without editor API configured (should be the case in tests due to cfg!(test))
        let server = create_test_server();

        // Get server info which contains tool descriptions
        let server_info = server.get_info();
        let instructions = server_info.instructions.unwrap_or_default();

        // Should use traditional description with str_replace command
        assert!(instructions.contains("Replace text in one or more files"));
        assert!(instructions.contains("str_replace"));

        // Should not contain editor API description or edit_file command
        assert!(!instructions.contains("Edit the file with the new content"));
        assert!(!instructions.contains("edit_file"));
        assert!(!instructions.contains("work out how to place old_str with it intelligently"));
    }

    #[tokio::test]
    #[serial]
    async fn test_text_editor_view_range() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let file_path_str = file_path.to_str().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let server = create_test_server();

        // Create a multi-line file
        let content =
            "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7\nLine 8\nLine 9\nLine 10";
        let write_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "write".to_string(),
            view_range: None,
            file_text: Some(content.to_string()),
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        server.text_editor(write_params).await.unwrap();

        // Test viewing specific range
        let view_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "view".to_string(),
            view_range: Some(vec![3, 6]),
            file_text: None,
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        let view_result = server.text_editor(view_params).await.unwrap();

        let text = view_result
            .content
            .iter()
            .find(|c| {
                c.audience()
                    .is_some_and(|roles| roles.contains(&Role::User))
            })
            .unwrap()
            .as_text()
            .unwrap();

        // Should contain lines 3-6 with line numbers
        assert!(text.text.contains("3: Line 3"));
        assert!(text.text.contains("4: Line 4"));
        assert!(text.text.contains("5: Line 5"));
        assert!(text.text.contains("6: Line 6"));
        assert!(text.text.contains("(lines 3-6)"));
        // Should not contain other lines
        assert!(!text.text.contains("1: Line 1"));
        assert!(!text.text.contains("7: Line 7"));
    }

    #[tokio::test]
    #[serial]
    async fn test_text_editor_view_range_to_end() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let file_path_str = file_path.to_str().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let server = create_test_server();

        // Create a multi-line file
        let content = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5";
        let write_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "write".to_string(),
            view_range: None,
            file_text: Some(content.to_string()),
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        server.text_editor(write_params).await.unwrap();

        // Test viewing from line 3 to end using -1
        let view_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "view".to_string(),
            view_range: Some(vec![3, -1]),
            file_text: None,
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        let view_result = server.text_editor(view_params).await.unwrap();

        let text = view_result
            .content
            .iter()
            .find(|c| {
                c.audience()
                    .is_some_and(|roles| roles.contains(&Role::User))
            })
            .unwrap()
            .as_text()
            .unwrap();

        // Should contain lines 3-5
        assert!(text.text.contains("3: Line 3"));
        assert!(text.text.contains("4: Line 4"));
        assert!(text.text.contains("5: Line 5"));
        assert!(text.text.contains("(lines 3-end)"));
        // Should not contain lines 1-2
        assert!(!text.text.contains("1: Line 1"));
        assert!(!text.text.contains("2: Line 2"));
    }

    #[tokio::test]
    #[serial]
    async fn test_text_editor_view_range_invalid() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let file_path_str = file_path.to_str().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let server = create_test_server();

        // Create a small file
        let content = "Line 1\nLine 2\nLine 3";
        let write_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "write".to_string(),
            view_range: None,
            file_text: Some(content.to_string()),
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        server.text_editor(write_params).await.unwrap();

        // Test invalid range - start line beyond file
        let view_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "view".to_string(),
            view_range: Some(vec![10, 15]),
            file_text: None,
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        let result = server.text_editor(view_params).await;
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, ErrorCode::INVALID_PARAMS);
        assert!(error.message.contains("beyond the end of the file"));
    }

    #[tokio::test]
    #[serial]
    async fn test_text_editor_insert_at_beginning() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let file_path_str = file_path.to_str().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let server = create_test_server();

        // Create a file with some content
        let content = "Line 2\nLine 3\nLine 4";
        let write_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "write".to_string(),
            view_range: None,
            file_text: Some(content.to_string()),
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        server.text_editor(write_params).await.unwrap();

        // Insert at the beginning (line 0)
        let insert_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "insert".to_string(),
            view_range: None,
            file_text: None,
            old_str: None,
            new_str: Some("Line 1".to_string()),
            insert_line: Some(0),
            diff: None,
        });

        let insert_result = server.text_editor(insert_params).await.unwrap();

        let text = insert_result
            .content
            .iter()
            .find(|c| {
                c.audience()
                    .is_some_and(|roles| roles.contains(&Role::Assistant))
            })
            .unwrap()
            .as_text()
            .unwrap();

        assert!(text.text.contains("Text has been inserted at line 1"));

        // Verify the file content by reading it directly
        let file_content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(
            file_content,
            normalize_line_endings("Line 1\nLine 2\nLine 3\nLine 4\n")
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_text_editor_insert_in_middle() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let file_path_str = file_path.to_str().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let server = create_test_server();

        // Create a file with some content
        let content = "Line 1\nLine 2\nLine 4\nLine 5";
        let write_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "write".to_string(),
            view_range: None,
            file_text: Some(content.to_string()),
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        server.text_editor(write_params).await.unwrap();

        // Insert after line 2
        let insert_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "insert".to_string(),
            view_range: None,
            file_text: None,
            old_str: None,
            new_str: Some("Line 3".to_string()),
            insert_line: Some(2),
            diff: None,
        });

        let insert_result = server.text_editor(insert_params).await.unwrap();

        let text = insert_result
            .content
            .iter()
            .find(|c| {
                c.audience()
                    .is_some_and(|roles| roles.contains(&Role::Assistant))
            })
            .unwrap()
            .as_text()
            .unwrap();

        assert!(text.text.contains("Text has been inserted at line 3"));

        // Verify the file content by reading it directly
        let file_content = fs::read_to_string(&file_path).unwrap();
        let lines: Vec<&str> = file_content.lines().collect();
        assert_eq!(lines[0], "Line 1");
        assert_eq!(lines[1], "Line 2");
        assert_eq!(lines[2], "Line 3");
        assert_eq!(lines[3], "Line 4");
        assert_eq!(lines[4], "Line 5");
    }

    #[tokio::test]
    #[serial]
    async fn test_text_editor_insert_at_end() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let file_path_str = file_path.to_str().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let server = create_test_server();

        // Create a file with some content
        let content = "Line 1\nLine 2\nLine 3";
        let write_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "write".to_string(),
            view_range: None,
            file_text: Some(content.to_string()),
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        server.text_editor(write_params).await.unwrap();

        // Insert at the end (after line 3)
        let insert_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "insert".to_string(),
            view_range: None,
            file_text: None,
            old_str: None,
            new_str: Some("Line 4".to_string()),
            insert_line: Some(3),
            diff: None,
        });

        let insert_result = server.text_editor(insert_params).await.unwrap();

        let text = insert_result
            .content
            .iter()
            .find(|c| {
                c.audience()
                    .is_some_and(|roles| roles.contains(&Role::Assistant))
            })
            .unwrap()
            .as_text()
            .unwrap();

        assert!(text.text.contains("Text has been inserted at line 4"));

        // Verify the file content by reading it directly
        let file_content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(
            file_content,
            normalize_line_endings("Line 1\nLine 2\nLine 3\nLine 4\n")
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_text_editor_insert_at_end_negative() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let file_path_str = file_path.to_str().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let server = create_test_server();

        // Create a file with some content
        let content = "Line 1\nLine 2\nLine 3";
        let write_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "write".to_string(),
            view_range: None,
            file_text: Some(content.to_string()),
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        server.text_editor(write_params).await.unwrap();

        // Insert at the end using -1
        let insert_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "insert".to_string(),
            view_range: None,
            file_text: None,
            old_str: None,
            new_str: Some("Line 4".to_string()),
            insert_line: Some(-1),
            diff: None,
        });

        let insert_result = server.text_editor(insert_params).await.unwrap();

        let text = insert_result
            .content
            .iter()
            .find(|c| {
                c.audience()
                    .is_some_and(|roles| roles.contains(&Role::Assistant))
            })
            .unwrap()
            .as_text()
            .unwrap();

        assert!(text.text.contains("Text has been inserted at line 4"));

        // Verify the file content by reading it directly
        let file_content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(
            file_content,
            normalize_line_endings("Line 1\nLine 2\nLine 3\nLine 4\n")
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_text_editor_insert_invalid_line() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let file_path_str = file_path.to_str().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let server = create_test_server();

        // Create a file with some content
        let content = "Line 1\nLine 2\nLine 3";
        let write_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "write".to_string(),
            view_range: None,
            file_text: Some(content.to_string()),
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        server.text_editor(write_params).await.unwrap();

        // Try to insert beyond the end of the file
        let insert_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "insert".to_string(),
            view_range: None,
            file_text: None,
            old_str: None,
            new_str: Some("Line 11".to_string()),
            insert_line: Some(10),
            diff: None,
        });

        let result = server.text_editor(insert_params).await;

        assert!(result.is_err());
        let err = result.err().unwrap();
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(err.message.contains("beyond the end of the file"));
    }

    #[tokio::test]
    #[serial]
    async fn test_text_editor_insert_missing_parameters() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let file_path_str = file_path.to_str().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let server = create_test_server();

        // Create a file first
        let write_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "write".to_string(),
            view_range: None,
            file_text: Some("Initial content".to_string()),
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        server.text_editor(write_params).await.unwrap();

        // Test insert without new_str parameter
        let insert_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "insert".to_string(),
            view_range: None,
            file_text: None,
            old_str: None,
            new_str: None, // Missing required parameter
            insert_line: Some(1),
            diff: None,
        });

        let result = server.text_editor(insert_params).await;
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, ErrorCode::INVALID_PARAMS);
        assert!(error.message.contains("Missing 'new_str' parameter"));

        // Test insert without insert_line parameter
        let insert_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "insert".to_string(),
            view_range: None,
            file_text: None,
            old_str: None,
            new_str: Some("New text".to_string()),
            insert_line: None, // Missing required parameter
            diff: None,
        });

        let result = server.text_editor(insert_params).await;
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, ErrorCode::INVALID_PARAMS);
        assert!(error.message.contains("Missing 'insert_line' parameter"));
    }

    #[tokio::test]
    #[serial]
    async fn test_text_editor_insert_with_undo() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let file_path_str = file_path.to_str().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let server = create_test_server();

        // Create a file with some content
        let content = "Line 1\nLine 2";
        let write_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "write".to_string(),
            view_range: None,
            file_text: Some(content.to_string()),
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        server.text_editor(write_params).await.unwrap();

        // Insert a line
        let insert_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "insert".to_string(),
            view_range: None,
            file_text: None,
            old_str: None,
            new_str: Some("Inserted Line".to_string()),
            insert_line: Some(1),
            diff: None,
        });

        server.text_editor(insert_params).await.unwrap();

        // Undo the insert
        let undo_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "undo_edit".to_string(),
            view_range: None,
            file_text: None,
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        let undo_result = server.text_editor(undo_params).await.unwrap();

        let text = undo_result
            .content
            .iter()
            .find(|c| c.as_text().is_some())
            .unwrap()
            .as_text()
            .unwrap();
        assert!(text.text.contains("Undid the last edit"));

        // Verify the file is back to original content
        let file_content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(file_content, normalize_line_endings("Line 1\nLine 2\n"));
        assert!(!file_content.contains("Inserted Line"));
    }

    #[tokio::test]
    #[serial]
    async fn test_text_editor_insert_nonexistent_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("nonexistent.txt");
        let file_path_str = file_path.to_str().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let server = create_test_server();

        // Try to insert into a nonexistent file
        let insert_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "insert".to_string(),
            view_range: None,
            file_text: None,
            old_str: None,
            new_str: Some("New line".to_string()),
            insert_line: Some(0),
            diff: None,
        });

        let result = server.text_editor(insert_params).await;

        assert!(result.is_err());
        let err = result.err().unwrap();
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(err.message.contains("does not exist"));
    }

    #[tokio::test]
    #[serial]
    async fn test_text_editor_view_large_file_without_range() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("large_file.txt");
        let file_path_str = file_path.to_str().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let server = create_test_server();

        // Create a file with more than 2000 lines (LINE_READ_LIMIT)
        let mut content = String::new();
        for i in 1..=2001 {
            content.push_str(&format!("Line {}\n", i));
        }

        let write_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "write".to_string(),
            view_range: None,
            file_text: Some(content),
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        server.text_editor(write_params).await.unwrap();

        // Test viewing without view_range - should trigger the error
        let view_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "view".to_string(),
            view_range: None,
            file_text: None,
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        let result = server.text_editor(view_params).await;

        assert!(result.is_err());
        let err = result.err().unwrap();
        assert_eq!(err.code, ErrorCode::INTERNAL_ERROR);
        assert!(err.message.contains("2001 lines long"));
        assert!(err
            .message
            .contains("recommended to read in with view_range"));
        assert!(err
            .message
            .contains("please pass in view_range with [1, 2001]"));

        // Test viewing with view_range - should work
        let view_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "view".to_string(),
            view_range: Some(vec![1, 100]),
            file_text: None,
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        let result = server.text_editor(view_params).await;
        assert!(result.is_ok());

        let view_result = result.unwrap();
        let text = view_result
            .content
            .iter()
            .find(|c| {
                c.audience()
                    .is_some_and(|roles| roles.contains(&Role::User))
            })
            .unwrap()
            .as_text()
            .unwrap();

        // Should contain lines 1-100
        assert!(text.text.contains("1: Line 1"));
        assert!(text.text.contains("100: Line 100"));
        assert!(!text.text.contains("101: Line 101"));

        // Test viewing with explicit full range - should work
        let view_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "view".to_string(),
            view_range: Some(vec![1, 2001]),
            file_text: None,
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        let result = server.text_editor(view_params).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    #[serial]
    async fn test_text_editor_view_file_with_exactly_2000_lines() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("file_2000.txt");
        let file_path_str = file_path.to_str().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let server = create_test_server();

        // Create a file with exactly 2000 lines (should not trigger the check)
        let mut content = String::new();
        for i in 1..=2000 {
            content.push_str(&format!("Line {}\n", i));
        }

        let write_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "write".to_string(),
            view_range: None,
            file_text: Some(content),
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        server.text_editor(write_params).await.unwrap();

        // Test viewing without view_range - should work since it's exactly 2000 lines
        let view_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "view".to_string(),
            view_range: None,
            file_text: None,
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        let result = server.text_editor(view_params).await;

        assert!(result.is_ok());
        let view_result = result.unwrap();
        let text = view_result
            .content
            .iter()
            .find(|c| {
                c.audience()
                    .is_some_and(|roles| roles.contains(&Role::User))
            })
            .unwrap()
            .as_text()
            .unwrap();

        // Should contain all lines
        assert!(text.text.contains("1: Line 1"));
        assert!(text.text.contains("2000: Line 2000"));
    }

    #[tokio::test]
    #[serial]
    async fn test_text_editor_view_small_file_without_range() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("small_file.txt");
        let file_path_str = file_path.to_str().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let server = create_test_server();

        // Create a file with less than 2000 lines
        let mut content = String::new();
        for i in 1..=100 {
            content.push_str(&format!("Line {}\n", i));
        }

        let write_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "write".to_string(),
            view_range: None,
            file_text: Some(content),
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        server.text_editor(write_params).await.unwrap();

        // Test viewing without view_range - should work fine
        let view_params = Parameters(TextEditorParams {
            path: file_path_str.to_string(),
            command: "view".to_string(),
            view_range: None,
            file_text: None,
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        let result = server.text_editor(view_params).await;

        assert!(result.is_ok());
        let view_result = result.unwrap();
        let text = view_result
            .content
            .iter()
            .find(|c| {
                c.audience()
                    .is_some_and(|roles| roles.contains(&Role::User))
            })
            .unwrap()
            .as_text()
            .unwrap();

        // Should contain all lines
        assert!(text.text.contains("1: Line 1"));
        assert!(text.text.contains("100: Line 100"));
    }

    #[tokio::test]
    #[serial]
    async fn test_text_editor_view_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path();

        // Set the current directory before creating the server
        std::env::set_current_dir(temp_path).unwrap();

        // Create some test files and directories
        fs::create_dir(temp_path.join("subdir1")).unwrap();
        fs::create_dir(temp_path.join("subdir2")).unwrap();
        fs::create_dir(temp_path.join("another_dir")).unwrap();

        fs::write(temp_path.join("file1.txt"), "content1").unwrap();
        fs::write(temp_path.join("file2.rs"), "content2").unwrap();
        fs::write(temp_path.join("README.md"), "content3").unwrap();

        let server = create_test_server();

        // Test viewing a directory
        let result = server
            .text_editor(Parameters(TextEditorParams {
                command: "view".to_string(),
                path: temp_path.to_str().unwrap().to_string(),
                view_range: None,
                file_text: None,
                old_str: None,
                new_str: None,
                insert_line: None,
                diff: None,
            }))
            .await;

        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert_eq!(content.len(), 1);

        // Check the content is a text message with directory listing
        let text_content = content[0].as_text().expect("Expected text content");
        let output = &text_content.text;

        // Check that it identifies as a directory
        assert!(output.contains("is a directory"));
        assert!(output.contains("Contents:"));

        // Check directories are listed with trailing slash
        assert!(output.contains("Directories:"));
        assert!(output.contains("another_dir/"));
        assert!(output.contains("subdir1/"));
        assert!(output.contains("subdir2/"));

        // Check files are listed
        assert!(output.contains("Files:"));
        assert!(output.contains("file1.txt"));
        assert!(output.contains("file2.rs"));
        assert!(output.contains("README.md"));
    }

    #[tokio::test]
    #[serial]
    async fn test_text_editor_view_directory_with_many_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path();

        // Set the current directory before creating the server
        std::env::set_current_dir(temp_path).unwrap();

        // Create more than 50 files to test the limit
        for i in 0..60 {
            fs::write(
                temp_path.join(format!("file{:03}.txt", i)),
                format!("content{}", i),
            )
            .unwrap();
        }

        // Create some directories too
        for i in 0..10 {
            fs::create_dir(temp_path.join(format!("dir{:02}", i))).unwrap();
        }

        let server = create_test_server();

        let result = server
            .text_editor(Parameters(TextEditorParams {
                command: "view".to_string(),
                path: temp_path.to_str().unwrap().to_string(),
                view_range: None,
                file_text: None,
                old_str: None,
                new_str: None,
                insert_line: None,
                diff: None,
            }))
            .await;

        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert_eq!(content.len(), 1);

        let text_content = content[0].as_text().expect("Expected text content");
        let output = &text_content.text;

        // Check that it shows the limit message
        assert!(output.contains("... and"));
        assert!(output.contains("more items"));
        assert!(output.contains("(showing first 50 items)"));

        // Count the actual number of items shown (should be 50)
        let dir_count = output.matches("/\n").count(); // directories end with /
        let file_count = output.matches(".txt\n").count(); // only counting .txt files for simplicity
        assert!(dir_count + file_count <= 50);
    }

    #[tokio::test]
    #[serial]
    async fn test_text_editor_view_empty_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path();

        // Set the current directory before creating the server
        std::env::set_current_dir(temp_path).unwrap();

        let server = create_test_server();

        let result = server
            .text_editor(Parameters(TextEditorParams {
                command: "view".to_string(),
                path: temp_path.to_str().unwrap().to_string(),
                view_range: None,
                file_text: None,
                old_str: None,
                new_str: None,
                insert_line: None,
                diff: None,
            }))
            .await;

        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert_eq!(content.len(), 1);

        let text_content = content[0].as_text().expect("Expected text content");
        let output = &text_content.text;

        // Check that it shows empty directory message
        assert!(output.contains("is a directory"));
        assert!(output.contains("(empty directory)"));
    }

    /// Setting `is_error` admits the result into the BR-51 taxonomy, which
    /// otherwise guesses a kind by substring-matching the raw command output
    /// against curated patterns. Ordinary output contains "401", "not found"
    /// and "timeout" for reasons unrelated to the exit status, so the shell
    /// names its own failure and takes the guesswork out — a tool-supplied
    /// envelope wins over the heuristics.
    #[test]
    #[serial]
    fn a_failing_shell_names_its_own_error_kind() {
        run_shell_test(|| async {
            let temp_dir = tempfile::tempdir().unwrap();
            let server = create_test_server().with_working_dir(temp_dir.path().to_path_buf());
            let running_service = serve_directly(server.clone(), create_test_transport(), None);
            let peer = running_service.peer().clone();

            // Output deliberately seeded with words the text heuristics match.
            let command = if cfg!(windows) {
                "Write-Output '401 not found timeout'; exit 1"
            } else {
                "echo '401 not found timeout'; exit 1"
            };

            let result = server
                .shell(
                    Parameters(ShellParams {
                        working_directory: None,
                        command: command.to_string(),
                        background: None,
                        label: None,
                    }),
                    RequestContext {
                        ct: Default::default(),
                        id: NumberOrString::Number(1),
                        meta: Default::default(),
                        extensions: Default::default(),
                        peer: peer.clone(),
                    },
                )
                .await
                .expect("a non-zero exit is a result, not a transport error");

            assert_eq!(result.is_error, Some(true), "a non-zero exit is an error");
            let kind = result
                .structured_content
                .as_ref()
                .and_then(|v| v.get("error"))
                .and_then(|e| e.get("kind"))
                .and_then(|k| k.as_str());
            assert_eq!(
                kind,
                Some("tool_failure"),
                "the command ran and nothing was missing or refused, so it is a plain \
                 tool failure — not whatever its output happens to spell, got: {:?}",
                result.structured_content
            );
        });
    }

    /// Exit 0 must stay clean: no error flag, and no envelope to classify.
    #[test]
    #[serial]
    fn a_succeeding_shell_carries_no_error_envelope() {
        run_shell_test(|| async {
            let temp_dir = tempfile::tempdir().unwrap();
            let server = create_test_server().with_working_dir(temp_dir.path().to_path_buf());
            let running_service = serve_directly(server.clone(), create_test_transport(), None);
            let peer = running_service.peer().clone();

            let result = server
                .shell(
                    Parameters(ShellParams {
                        working_directory: None,
                        command: "echo 'error: not found'".to_string(),
                        background: None,
                        label: None,
                    }),
                    RequestContext {
                        ct: Default::default(),
                        id: NumberOrString::Number(1),
                        meta: Default::default(),
                        extensions: Default::default(),
                        peer: peer.clone(),
                    },
                )
                .await
                .expect("a successful command returns a result");

            assert_eq!(result.is_error, Some(false));
            assert!(
                result.structured_content.is_none(),
                "a command that succeeded must not carry an error envelope just \
                 because its output mentions an error, got: {:?}",
                result.structured_content
            );
        });
    }

    #[test]
    #[serial]
    fn test_shell_output_truncation() {
        run_shell_test(|| async {
            let temp_dir = tempfile::tempdir().unwrap();

            let server = create_test_server().with_working_dir(temp_dir.path().to_path_buf());
            let running_service = serve_directly(server.clone(), create_test_transport(), None);
            let peer = running_service.peer().clone();

            // Create a command that generates > 100 lines of output
            let command = if cfg!(windows) {
                "1..150 | ForEach-Object { 'Line ' + $_ }"
            } else {
                "for i in {1..150}; do echo \"Line $i\"; done"
            };

            let result = server
                .shell(
                    Parameters(ShellParams {
                        working_directory: None,
                        command: command.to_string(),
                        background: None,
                        label: None,
                    }),
                    RequestContext {
                        ct: Default::default(),
                        id: NumberOrString::Number(1),
                        meta: Default::default(),
                        extensions: Default::default(),
                        peer: peer.clone(),
                    },
                )
                .await;

            // Should have two Content items
            assert_eq!(result.clone().unwrap().content.len(), 2);

            let content = result.clone().unwrap().content;

            // Find the Assistant and User content
            let assistant_content = content
                .iter()
                .find(|c| {
                    c.audience()
                        .is_some_and(|roles| roles.contains(&Role::Assistant))
                })
                .unwrap()
                .as_text()
                .unwrap();

            let user_content = content
                .iter()
                .find(|c| {
                    c.audience()
                        .is_some_and(|roles| roles.contains(&Role::User))
                })
                .unwrap()
                .as_text()
                .unwrap();

            // Assistant should get the full message with temp file info
            assert!(assistant_content
                .text
                .contains("private note: output was 150 lines"));

            // User should only get the truncated output with prefix
            assert!(user_content
                .text
                .starts_with("NOTE: Output was 150 lines, showing only the last 100 lines"));
            assert!(!user_content.text.contains("private note: output was"));

            // User output should contain lines 51-150 (last 100 lines)
            assert!(user_content.text.contains("Line 51"));
            assert!(user_content.text.contains("Line 150"));
            assert!(!user_content.text.contains("Line 50"));

            let start_tag = "remainder of lines in";
            let end_tag = "do not show tmp file to user";

            if let (Some(start), Some(end)) = (
                assistant_content.text.find(start_tag),
                assistant_content.text.find(end_tag),
            ) {
                let start_idx = start + start_tag.len();
                if start_idx < end {
                    let Some(path) = assistant_content.text.get(start_idx..end).map(|s| s.trim())
                    else {
                        panic!("Failed to extract path from assistant content");
                    };
                    println!("Extracted path: {}", path);

                    let file_contents =
                        std::fs::read_to_string(path).expect("Failed to read extracted temp file");

                    let lines: Vec<&str> = file_contents.lines().collect();

                    // Ensure we have exactly 150 lines
                    assert_eq!(lines.len(), 150, "Expected 150 lines in temp file");

                    // Ensure the first and last lines are correct
                    assert_eq!(lines.first(), Some(&"Line 1"), "First line mismatch");
                    assert_eq!(lines.last(), Some(&"Line 150"), "Last line mismatch");
                } else {
                    panic!("No path found in bash output truncation output");
                }
            } else {
                panic!("Failed to find start or end tag in bash output truncation output");
            }

            // Force cleanup before runtime shutdown
            cleanup_test_service(running_service, peer);

            temp_dir.close().unwrap();
        });
    }

    #[tokio::test]
    #[serial]
    async fn test_process_shell_output_short() {
        let dir = TempDir::new().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let server = create_test_server();

        // Test with short output (< 100 lines)
        let short_output = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5";
        let result = server.process_shell_output(short_output).unwrap();

        // Both outputs should be the same for short outputs
        assert_eq!(result.0, short_output);
        assert_eq!(result.1, short_output);
    }

    #[tokio::test]
    #[serial]
    async fn test_process_shell_output_empty() {
        let dir = TempDir::new().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let server = create_test_server();

        // Test with empty output
        let empty_output = "";
        let result = server.process_shell_output(empty_output).unwrap();

        // Both outputs should be empty
        assert_eq!(result.0, "");
        assert_eq!(result.1, "");
    }

    #[test]
    #[serial]
    fn test_shell_output_without_trailing_newline() {
        run_shell_test(|| async {
            let temp_dir = tempfile::tempdir().unwrap();
            std::env::set_current_dir(&temp_dir).unwrap();

            let server = create_test_server();
            let running_service = serve_directly(server.clone(), create_test_transport(), None);
            let peer = running_service.peer().clone();

            // Test command that outputs content without a trailing newline
            let command = if cfg!(windows) {
                "echo|set /p=\"Content without newline\""
            } else {
                "printf 'Content without newline'"
            };

            let result = server
                .shell(
                    Parameters(ShellParams {
                        working_directory: None,
                        command: command.to_string(),
                        background: None,
                        label: None,
                    }),
                    RequestContext {
                        ct: Default::default(),
                        id: NumberOrString::Number(1),
                        meta: Default::default(),
                        extensions: Default::default(),
                        peer: peer.clone(),
                    },
                )
                .await;

            assert!(result.is_ok());

            // Test the output processing logic that would be used by shell method
            let output_without_newline = "Content without newline";
            let result = server.process_shell_output(output_without_newline).unwrap();

            // The output should contain the content even without a trailing newline
            assert!(
                result.0.contains("Content without newline"),
                "Output should contain content even without trailing newline, but got: {}",
                result.0
            );
            assert!(
                result.1.contains("Content without newline"),
                "User output should contain content even without trailing newline, but got: {}",
                result.1
            );

            // Both should be the same for short output
            assert_eq!(result.0, output_without_newline);
            assert_eq!(result.1, output_without_newline);

            // Force cleanup before runtime shutdown
            cleanup_test_service(running_service, peer);
        });
    }

    #[tokio::test]
    #[serial]
    async fn test_shell_output_handling_logic() {
        let temp_dir = tempfile::tempdir().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let server = create_test_server();

        // Test output truncation logic with content without trailing newlines
        let content_without_newline = "Content without newline";
        let result = server
            .process_shell_output(content_without_newline)
            .unwrap();

        assert_eq!(result.0, content_without_newline);
        assert_eq!(result.1, content_without_newline);
        assert!(
            result.0.contains("Content without newline"),
            "Output processing should preserve content without trailing newlines"
        );

        // Test with content that has trailing newlines
        let content_with_newline = "Content with newline\n";
        let result = server.process_shell_output(content_with_newline).unwrap();
        assert_eq!(result.0, content_with_newline);
        assert_eq!(result.1, content_with_newline);

        // Test empty output handling
        let empty_output = "";
        let result = server.process_shell_output(empty_output).unwrap();
        assert_eq!(result.0, "");
        assert_eq!(result.1, "");
    }

    #[tokio::test]
    #[serial]
    async fn test_default_patterns_when_no_ignore_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        // Don't create any ignore files
        let server = create_test_server();

        // Default patterns should be used
        assert!(
            server.is_ignored(Path::new(".env")),
            ".env should be ignored by default patterns"
        );
        assert!(
            server.is_ignored(Path::new(".env.local")),
            ".env.local should be ignored by default patterns"
        );
        assert!(
            server.is_ignored(Path::new("secrets.txt")),
            "secrets.txt should be ignored by default patterns"
        );
        assert!(
            !server.is_ignored(Path::new("normal.txt")),
            "normal.txt should not be ignored"
        );
    }

    #[test]
    #[serial]
    fn test_resolve_path_absolute() {
        let temp_dir = tempfile::tempdir().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let server = create_test_server();
        let absolute_path = temp_dir.path().join("test.txt");
        let absolute_path_str = absolute_path.to_str().unwrap();

        let resolved = server.resolve_path(absolute_path_str).unwrap();
        assert_eq!(resolved, absolute_path);
    }

    #[tokio::test]
    #[serial]
    async fn test_resolve_path_relative() {
        let temp_dir = tempfile::tempdir().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let server = create_test_server();
        let relative_path = "subdir/test.txt";

        let resolved = server.resolve_path(relative_path).unwrap();
        let expected = std::env::current_dir().unwrap().join("subdir/test.txt");
        assert_eq!(resolved, expected);
    }

    #[tokio::test]
    #[serial]
    async fn test_text_editor_with_absolute_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let server = create_test_server();
        let absolute_path = temp_dir.path().join("absolute_test.txt");
        let absolute_path_str = absolute_path.to_str().unwrap();

        let write_params = Parameters(TextEditorParams {
            path: absolute_path_str.to_string(),
            command: "write".to_string(),
            view_range: None,
            file_text: Some("Absolute path test".to_string()),
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        let result = server.text_editor(write_params).await;
        assert!(result.is_ok());

        let content = fs::read_to_string(&absolute_path).unwrap();
        assert_eq!(content.trim(), "Absolute path test");
    }

    #[tokio::test]
    #[serial]
    async fn test_text_editor_with_relative_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let server = create_test_server();
        let relative_path = "relative_test.txt";

        let write_params = Parameters(TextEditorParams {
            path: relative_path.to_string(),
            command: "write".to_string(),
            view_range: None,
            file_text: Some("Relative path test".to_string()),
            old_str: None,
            new_str: None,
            insert_line: None,
            diff: None,
        });

        let result = server.text_editor(write_params).await;
        assert!(result.is_ok());

        let absolute_path = temp_dir.path().join(relative_path);
        let content = fs::read_to_string(&absolute_path).unwrap();
        assert_eq!(content.trim(), "Relative path test");
    }

    #[test]
    #[serial]
    #[cfg(unix)] // Unix-specific test using sleep command
    fn test_shell_command_cancellation() {
        run_shell_test(|| async {
            let server = create_test_server();
            let running_service = serve_directly(server.clone(), create_test_transport(), None);
            let peer = running_service.peer().clone();

            let request_id = NumberOrString::Number(123);

            let context = RequestContext {
                ct: Default::default(),
                id: request_id.clone(),
                meta: Default::default(),
                extensions: Default::default(),
                peer: peer.clone(),
            };

            // Start a long-running shell command in the background
            let server_clone = server.clone();
            let shell_task = tokio::spawn(async move {
                server_clone
                    .shell(
                        Parameters(ShellParams {
                            working_directory: None,
                            command: "sleep 30".to_string(),
                            background: None,
                            label: None,
                        }),
                        context,
                    )
                    .await
            });

            // Give the command a moment to start
            tokio::time::sleep(Duration::from_millis(200)).await;

            // Verify the process is tracked
            {
                let processes = server.running_processes.read().await;
                assert!(processes.contains_key("123"), "Process should be tracked");
            }

            let start_time = Instant::now();

            // Cancel the command
            let cancel_params = CancelledNotificationParam {
                request_id,
                reason: Some("test cancellation".to_string()),
            };

            let notification_context = NotificationContext {
                peer: peer.clone(),
                meta: Default::default(),
                extensions: Default::default(),
            };

            server
                .on_cancelled(cancel_params, notification_context)
                .await;

            // Wait for the shell task to complete
            let result = timeout(Duration::from_secs(5), shell_task).await;
            let elapsed = start_time.elapsed();

            // Verify the task completed due to cancellation (not timeout)
            assert!(result.is_ok(), "Shell task should complete within timeout");
            let task_result = result.unwrap();
            assert!(task_result.is_ok(), "Shell task should not panic");

            // Verify the command was cancelled quickly (much less than 30 seconds)
            assert!(
                elapsed < Duration::from_secs(5),
                "Command should be cancelled quickly, took {:?}",
                elapsed
            );

            // Verify the process is no longer tracked
            {
                let processes = server.running_processes.read().await;
                assert!(
                    !processes.contains_key("123"),
                    "Process should be removed from tracking"
                );
            }

            cleanup_test_service(running_service, peer);
        });
    }

    #[test]
    #[serial]
    #[cfg(unix)] // Unix-specific test using shell commands
    fn test_child_process_cancellation() {
        run_shell_test(|| async {
            let server = create_test_server();
            let running_service = serve_directly(server.clone(), create_test_transport(), None);
            let peer = running_service.peer().clone();

            let request_id = NumberOrString::Number(456);

            let context = RequestContext {
                ct: Default::default(),
                id: request_id.clone(),
                meta: Default::default(),
                extensions: Default::default(),
                peer: peer.clone(),
            };

            // Start a command that spawns child processes
            let server_clone = server.clone();
            let shell_task = tokio::spawn(async move {
                server_clone
                    .shell(
                        Parameters(ShellParams {
                            working_directory: None,
                            command: "bash -c 'sleep 60 & wait'".to_string(),
                            background: None,
                            label: None,
                        }),
                        context,
                    )
                    .await
            });

            // Give the command time to start and spawn child processes
            tokio::time::sleep(Duration::from_millis(300)).await;

            let start_time = Instant::now();

            // Cancel the command
            let cancel_params = CancelledNotificationParam {
                request_id,
                reason: Some("test cancellation".to_string()),
            };

            let notification_context = NotificationContext {
                peer: peer.clone(),
                meta: Default::default(),
                extensions: Default::default(),
            };

            server
                .on_cancelled(cancel_params, notification_context)
                .await;

            // Wait for completion
            let result = timeout(Duration::from_secs(5), shell_task).await;
            let elapsed = start_time.elapsed();

            assert!(result.is_ok(), "Shell task should complete within timeout");
            assert!(
                elapsed < Duration::from_secs(5),
                "Command with child processes should be cancelled quickly, took {:?}",
                elapsed
            );

            cleanup_test_service(running_service, peer);
        });
    }

    #[test]
    #[serial]
    fn test_cancel_nonexistent_process() {
        run_shell_test(|| async {
            let server = create_test_server();
            let running_service = serve_directly(server.clone(), create_test_transport(), None);
            let peer = running_service.peer().clone();

            // Try to cancel a process that doesn't exist
            let cancel_params = CancelledNotificationParam {
                request_id: NumberOrString::Number(999),
                reason: Some("test cancellation".to_string()),
            };

            let notification_context = NotificationContext {
                peer: peer.clone(),
                meta: Default::default(),
                extensions: Default::default(),
            };

            // This should not panic or cause issues
            server
                .on_cancelled(cancel_params, notification_context)
                .await;

            // Verify no processes are tracked
            let processes = server.running_processes.read().await;
            assert!(processes.is_empty(), "No processes should be tracked");

            cleanup_test_service(running_service, peer);
        });
    }

    #[test]
    #[serial]
    #[cfg(unix)]
    fn test_successful_shell_command_completion() {
        run_shell_test(|| async {
            let server = create_test_server();
            let running_service = serve_directly(server.clone(), create_test_transport(), None);
            let peer = running_service.peer().clone();

            let context = RequestContext {
                ct: Default::default(),
                id: NumberOrString::Number(789),
                meta: Default::default(),
                extensions: Default::default(),
                peer: peer.clone(),
            };

            // Run a quick command that should complete successfully
            let result = server
                .shell(
                    Parameters(ShellParams {
                        working_directory: None,
                        command: "echo 'Hello, World!'".to_string(),
                        background: None,
                        label: None,
                    }),
                    context,
                )
                .await;

            assert!(
                result.is_ok(),
                "Simple shell command should succeed: {:?}",
                result
            );

            // Verify no processes are left tracked after completion
            let processes = server.running_processes.read().await;
            assert!(
                !processes.contains_key("789"),
                "Process should be cleaned up after completion"
            );

            cleanup_test_service(running_service, peer);
        });
    }
}
