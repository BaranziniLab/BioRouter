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
    sync::{Arc, Mutex},
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
    text_editor_insert, text_editor_replace, text_editor_undo, text_editor_view, text_editor_write,
};

/// Build a git context + version-control policy block for the extension
/// instructions. If `cwd` is inside a git work tree, the agent is told the
/// current branch and how many files are uncommitted, plus a concise policy
/// encouraging disciplined commits and forbidding destructive history ops
/// without an explicit request. Outside a repo this returns an empty string so
/// it adds no noise to non-versioned tasks.
fn git_context_block(cwd: &std::path::Path) -> String {
    let git = |args: &[&str]| -> Option<String> {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .ok()?;
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
    file_history: Arc<Mutex<HashMap<PathBuf, Vec<String>>>>,
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
        // Get base instructions and working directory
        let cwd = std::env::current_dir().expect("should have a current working dir");
        let os = std::env::consts::OS;
        let in_container = Self::is_definitely_container();

        let base_instructions = match os {
            "windows" => formatdoc! {r#"
                The developer extension gives you the capabilities to edit code files and run shell commands,
                and can be used to solve a wide range of problems.

                You can use the shell tool to run Windows commands (PowerShell or CMD).
                When using paths, you can use either backslashes or forward slashes.

                Use the shell tool as needed to locate files or interact with the project.

                Leverage `analyze` through `return_last_only=true` subagents for deep codebase understanding with lean context
                - delegate analysis, retain summaries

                Your windows/screen tools can be used for visual debugging. You should not use these tools unless
                prompted to, but you can mention they are available if they are relevant.

                operating system: {os}
                current directory: {cwd}
                {container_info}
                "#,
                os=os,
                cwd=cwd.to_string_lossy(),
                container_info=if in_container { "container: true" } else { "" },
            },
            _ => {
                let shell_info = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

                formatdoc! {r#"
                The developer extension gives you the capabilities to edit code files and run shell commands,
                and can be used to solve a wide range of problems.

            You can use the shell tool to run any command that would work on the relevant operating system.
            Use the shell tool as needed to locate files or interact with the project.

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
                cwd=cwd.to_string_lossy(),
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

        let git_desc = git_context_block(&cwd);

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
            file_history: Arc::new(Mutex::new(HashMap::new())),
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
    pub fn with_working_dir(mut self, dir: PathBuf) -> Self {
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

    pub fn bash_env_file(mut self, value: Option<PathBuf>) -> Self {
        self.bash_env_file = value;
        self
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
        // the session working directory, else the process cwd.
        let working_dir = self.resolve_shell_cwd(params.working_directory.as_deref());

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

        let output_str = output_result?;

        // Validate output size
        self.validate_shell_output_size(command, &output_str)?;

        // Process and format the output
        let (final_output, user_output) = self.process_shell_output(&output_str)?;

        Ok(CallToolResult::success(vec![
            Content::text(final_output).with_audience(vec![Role::Assistant]),
            Content::text(user_output)
                .with_audience(vec![Role::User])
                .with_priority(0.0),
        ]))
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

    /// Execute a shell command and return the combined output.
    ///
    /// Streams output in real-time to the client using logging notifications.
    async fn execute_shell_command(
        &self,
        command: &str,
        working_dir: Option<PathBuf>,
        peer: &rmcp::service::Peer<RoleServer>,
        cancellation_token: CancellationToken,
    ) -> Result<String, ErrorData> {
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

        let mut command = configure_shell_command(&shell_config, command, working_dir.as_deref());

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
                // Wait for the process to complete
                let _exit_status = child.wait().await.map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                output_result
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

    // Helper method to resolve and validate file paths
    /// The base directory file operations resolve against and are jailed to.
    /// Prefers the session working directory (so the text_editor jail tracks the
    /// same directory the shell runs in), falling back to the process cwd.
    fn effective_cwd(&self) -> PathBuf {
        self.working_dir
            .clone()
            .unwrap_or_else(|| std::env::current_dir().expect("should have a current working dir"))
    }

    fn resolve_path(&self, path_str: &str) -> Result<PathBuf, ErrorData> {
        let cwd = self.effective_cwd();
        let expanded = expand_path(path_str);
        let path = Path::new(&expanded);

        let resolved = if is_absolute_path(&expanded) {
            path.to_path_buf()
        } else {
            cwd.join(path)
        };

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
    use tokio::time::timeout;

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

    /// Creates a test transport using in-memory streams instead of stdio
    /// This avoids the hanging issues caused by multiple tests competing for stdio
    fn create_test_transport() -> impl rmcp::transport::IntoTransport<
        RoleServer,
        std::io::Error,
        rmcp::transport::async_rw::TransportAdapterAsyncCombinedRW,
    > {
        let (_client, server) = tokio::io::duplex(1024);
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
            std::env::set_current_dir(&temp_dir).unwrap();

            let server = create_test_server();
            let running_service = serve_directly(server.clone(), create_test_transport(), None);
            let peer = running_service.peer().clone();

            // Test PowerShell command
            let shell_params = Parameters(ShellParams {
                working_directory: None,
                command: "Get-ChildItem".to_string(),
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
                .await;

            assert!(result.is_err());

            // Test that resolve_path works with Windows paths
            let windows_path = r"C:\Windows\System32";
            if Path::new(windows_path).exists() {
                let resolved = server.resolve_path(windows_path);
                assert!(resolved.is_ok());
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
        assert!(file_content.contains("Line 1\nLine 2\nLine 3\nLine 4"));
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
        assert!(file_content.contains("Line 1\nLine 2\nLine 3\nLine 4"));
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
        assert!(file_content.contains("Line 1\nLine 2\nLine 3\nLine 4"));
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
        assert!(file_content.contains("Line 1\nLine 2"));
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

    #[test]
    #[serial]
    fn test_shell_output_truncation() {
        run_shell_test(|| async {
            let temp_dir = tempfile::tempdir().unwrap();
            std::env::set_current_dir(&temp_dir).unwrap();

            let server = create_test_server();
            let running_service = serve_directly(server.clone(), create_test_transport(), None);
            let peer = running_service.peer().clone();

            // Create a command that generates > 100 lines of output
            let command = if cfg!(windows) {
                "for /L %i in (1,1,150) do @echo Line %i"
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
