use etcetera::{choose_app_strategy, AppStrategy};
use indoc::{formatdoc, indoc};
use reqwest::{Client, Url};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        AnnotateAble, CallToolResult, Content, ErrorCode, ErrorData, Implementation,
        ListResourcesResult, PaginatedRequestParams, RawResource, ReadResourceRequestParams,
        ReadResourceResult, Resource, ResourceContents, ServerCapabilities, ServerInfo,
    },
    schemars::JsonSchema,
    service::RequestContext,
    tool, tool_handler, tool_router, RoleServer, ServerHandler,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::PathBuf, sync::Arc, sync::Mutex};
use tokio::process::Command;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

mod docx_tool;
mod pdf_tool;
mod xlsx_tool;

mod platform;
use platform::{create_system_automation, SystemAutomation};

const MAX_INLINE_WEB_CONTENT_BYTES: usize = 128 * 1024;

/// `web_scrape` HTTP hardening (issue #25): a browser-compatible UA (the old
/// bare `biorouter/1.0` was bot-flagged into 403s), a request timeout (a hung
/// server used to hang the tool indefinitely), and one retry on transient
/// failures only — connect errors, 429, and 5xx. A timed-out request is NOT
/// retried: the client already waited the full timeout, so retrying a hung
/// server would double worst-case latency.
const WEB_SCRAPE_USER_AGENT: &str = "Mozilla/5.0 (compatible; biorouter/1.0)";
const WEB_SCRAPE_TIMEOUT_SECS: u64 = 30;
const WEB_SCRAPE_RETRY_BACKOFF_MS: u64 = 500;

/// Whether a non-success status is worth one retry: 429 and 5xx are transient;
/// 4xx client errors (403/404/…) are deterministic — an identical retry cannot
/// succeed.
fn web_scrape_status_is_retryable(status: reqwest::StatusCode) -> bool {
    status.as_u16() == 429 || status.is_server_error()
}

/// Per-status recovery hint appended to the error text. The status code itself
/// stays in the message — the tool-error classifier keys on it ('403' →
/// permission_denied, '404' → not_found, '429' → transient), so it is
/// load-bearing, and the hint tells the model what to do instead of retrying.
fn web_scrape_status_hint(status: reqwest::StatusCode) -> &'static str {
    match status.as_u16() {
        403 => {
            " The site blocks automated clients — try an alternative source, \
                 or browser automation if available."
        }
        404 => " The URL does not exist — verify the URL or pick another source.",
        429 => {
            " The site is rate-limiting requests — wait before retrying, or use \
                 another source."
        }
        code if (500..600).contains(&code) => {
            " The server failed — retry later or use another source."
        }
        _ => "",
    }
}

fn bounded_web_content(content: &str) -> (&str, bool) {
    if content.len() <= MAX_INLINE_WEB_CONTENT_BYTES {
        return (content, false);
    }

    let mut end = MAX_INLINE_WEB_CONTENT_BYTES;
    while !content.is_char_boundary(end) {
        end -= 1;
    }
    (
        content
            .get(..end)
            .expect("bounded web content must end on a character boundary"),
        true,
    )
}

/// Enum for save_as parameter in web_scrape tool
#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone, Default)]
#[serde(rename_all = "lowercase")]
pub enum SaveAsFormat {
    /// Save as text (for HTML pages)
    #[default]
    Text,
    /// Save as JSON (for API responses)
    Json,
    /// Save as binary (for images and other files)
    Binary,
}

/// Parameters for the web_scrape tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct WebScrapeParams {
    /// The URL to fetch content from
    pub url: String,
    /// Format of the response.
    #[serde(default)]
    pub save_as: SaveAsFormat,
}

/// Enum for language parameter in automation_script tool
#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "lowercase")]
pub enum ScriptLanguage {
    /// Shell/Bash script
    Shell,
    /// Batch script (Windows)
    Batch,
    /// Ruby script
    Ruby,
    /// PowerShell script
    Powershell,
}

/// Enum for command parameter in cache tool
#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "lowercase")]
pub enum CacheCommand {
    /// List all cached files
    List,
    /// View content of a cached file
    View,
    /// Delete a cached file
    Delete,
    /// Clear all cached files
    Clear,
}

/// Parameters for the automation_script tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct AutomationScriptParams {
    /// The scripting language to use
    #[serde(rename = "language")]
    pub language: ScriptLanguage,
    /// The script content
    pub script: String,
    /// Whether to save the script output to a file
    #[serde(default)]
    pub save_output: bool,
}

/// Parameters for the computer_control tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ComputerControlParams {
    /// The automation script content (PowerShell for Windows, AppleScript for macOS)
    pub script: String,
    /// Whether to save the script output to a file
    #[serde(default)]
    pub save_output: bool,
}

/// Parameters for the cache tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CacheParams {
    /// The command to perform
    pub command: CacheCommand,
    /// Path to the cached file for view/delete commands
    pub path: Option<String>,
}

/// Parameters for the pdf_tool
/// Enum for operation parameter in pdf_tool
#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "snake_case")]
pub enum PdfOperation {
    /// Extract all text content from the PDF
    ExtractText,
    /// Extract and save embedded images to PNG files
    ExtractImages,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PdfToolParams {
    /// Path to the PDF file
    pub path: String,
    /// Operation to perform on the PDF
    pub operation: PdfOperation,
}

/// Enum for operation parameter in docx_tool
#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "snake_case")]
pub enum DocxOperation {
    /// Extract all text content and structure from the DOCX
    ExtractText,
    /// Create a new DOCX or update existing one with provided content
    UpdateDoc,
}

/// Enum for update mode in docx_tool params
#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone, Default)]
#[serde(rename_all = "snake_case")]
pub enum DocxUpdateMode {
    /// Add content to end of document (default)
    #[default]
    Append,
    /// Replace specific text with new content
    Replace,
    /// Add content with specific heading level and styling
    Structured,
    /// Add an image to the document (with optional caption)
    AddImage,
}

/// Enum for text alignment in docx_tool params
#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "lowercase")]
pub enum TextAlignment {
    /// Left alignment
    Left,
    /// Center alignment
    Center,
    /// Right alignment
    Right,
    /// Justified alignment
    Justified,
}

/// Styling options for text in docx_tool
#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone, Default)]
pub struct DocxTextStyle {
    /// Make text bold
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    /// Make text italic
    #[serde(skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    /// Make text underlined
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    /// Font size in points
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u32>,
    /// Text color in hex format (e.g., 'FF0000' for red)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Text alignment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alignment: Option<TextAlignment>,
}

/// Additional parameters for update_doc operation
#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone, Default)]
pub struct DocxUpdateParams {
    /// Update mode (default: append)
    #[serde(default)]
    pub mode: DocxUpdateMode,
    /// Text to replace (required for replace mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_text: Option<String>,
    /// Heading level for structured mode (e.g., 'Heading1', 'Heading2')
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    /// Path to the image file (required for add_image mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_path: Option<String>,
    /// Image width in pixels (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    /// Image height in pixels (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// Styling options for the text
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<DocxTextStyle>,
}

/// Parameters for the docx_tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DocxToolParams {
    /// Path to the DOCX file
    pub path: String,
    /// Operation to perform on the DOCX
    pub operation: DocxOperation,
    /// Content to write (required for update_doc operation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Additional parameters for update_doc operation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<DocxUpdateParams>,
}

/// Parameters for the xlsx_tool
/// Enum for operation parameter in xlsx_tool
#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "snake_case")]
pub enum XlsxOperation {
    /// List all worksheets in the workbook
    ListWorksheets,
    /// Get column names from a worksheet
    GetColumns,
    /// Get values and formulas from a cell range
    GetRange,
    /// Search for text in a worksheet
    FindText,
    /// Update a single cell's value
    UpdateCell,
    /// Get value and formula from a specific cell
    GetCell,
    /// Save changes back to the file
    Save,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct XlsxToolParams {
    /// Path to the XLSX file
    pub path: String,
    /// Operation to perform on the XLSX file
    pub operation: XlsxOperation,
    /// Worksheet name (if not provided, uses first worksheet)
    pub worksheet: Option<String>,
    /// Cell range in A1 notation (e.g., 'A1:C10') for get_range operation
    pub range: Option<String>,
    /// Text to search for in find_text operation
    pub search_text: Option<String>,
    /// Whether search should be case-sensitive
    #[serde(default)]
    pub case_sensitive: bool,
    /// Row number for update_cell and get_cell operations
    pub row: Option<u64>,
    /// Column number for update_cell and get_cell operations
    pub col: Option<u64>,
    /// New value for update_cell operation
    pub value: Option<String>,
}

/// ComputerController MCP Server using official RMCP SDK
#[derive(Clone)]
pub struct ComputerControllerServer {
    tool_router: ToolRouter<Self>,
    cache_dir: PathBuf,
    active_resources: Arc<Mutex<HashMap<String, ResourceContents>>>,
    http_client: Client,
    instructions: String,
    system_automation: Arc<Box<dyn SystemAutomation + Send + Sync>>,
}

impl Default for ComputerControllerServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router(router = tool_router)]
impl ComputerControllerServer {
    #[allow(clippy::too_many_lines)]
    pub fn new() -> Self {
        // choose_app_strategy().cache_dir()
        // - macOS/Linux: ~/.cache/biorouter/computer_controller/
        // - Windows:     ~\AppData\Local\BaranziniLab\Biorouter\cache\computer_controller\
        // keep previous behavior of defaulting to /tmp/
        let cache_dir = choose_app_strategy(crate::APP_STRATEGY.clone())
            .map(|strategy| strategy.in_cache_dir("computer_controller"))
            .unwrap_or_else(|_| create_system_automation().get_temp_path());

        fs::create_dir_all(&cache_dir).unwrap_or_else(|_| {
            println!(
                "Warning: Failed to create cache directory at {:?}",
                cache_dir
            )
        });

        let system_automation: Arc<Box<dyn SystemAutomation + Send + Sync>> =
            Arc::new(create_system_automation());

        let os_specific_instructions = match std::env::consts::OS {
            "windows" => indoc! {r#"
            Here are some extra tools:
            automation_script
              - Create and run PowerShell or Batch scripts
              - PowerShell is recommended for most tasks
              - Scripts can save their output to files
              - Windows-specific features:
                - PowerShell for system automation and UI control
                - Windows Management Instrumentation (WMI)
                - Registry access and system settings
              - Use the screenshot tool if needed to help with tasks

            computer_control
              - System automation using PowerShell
              - Consider the screenshot tool to work out what is on screen and what to do to help with the control task.
            "#},
            "macos" => indoc! {r#"
            Here are some extra tools:
            automation_script
              - Create and run Shell and Ruby scripts
              - Shell (bash) is recommended for most tasks
              - Scripts can save their output to files
              - macOS-specific features:
                - AppleScript for system and UI control
                - Integration with macOS apps and services
              - Use the screenshot tool if needed to help with tasks

            computer_control
              - System automation using AppleScript
              - Consider the screenshot tool to work out what is on screen and what to do to help with the control task.

            When you need to interact with websites or web applications, consider using the computer_control tool with AppleScript, which can automate Safari or other browsers to:
              - Open specific URLs
              - Fill in forms
              - Click buttons
              - Extract content
              - Handle web-based workflows
            This is often more reliable than web scraping for modern web applications.
            "#},
            _ => indoc! {r#"
            Here are some extra tools:
            automation_script
              - Create and run Shell scripts
              - Shell (bash) is recommended for most tasks
              - Scripts can save their output to files
              - Linux-specific features:
                - System automation through shell scripting
                - X11/Wayland window management
                - D-Bus system services integration
                - Desktop environment control
              - Use the screenshot tool if needed to help with tasks

            computer_control
              - System automation using shell commands and system tools
              - Desktop environment automation (GNOME, KDE, etc.)
              - Consider the screenshot tool to work out what is on screen and what to do to help with the control task.

            When you need to interact with websites or web applications, consider using tools like xdotool or wmctrl for:
              - Window management
              - Simulating keyboard/mouse input
              - Automating UI interactions
              - Desktop environment control
            "#},
        };

        let instructions = formatdoc! {r#"
            You are a helpful assistant to a power user who is not a professional developer, but you may use development tools to help assist them.
            The user may not know how to break down tasks, so you will need to ensure that you do, and run things in batches as needed.
            The ComputerControllerExtension helps you with common tasks like web scraping,
            data processing, and automation without requiring programming expertise.

            You can use scripting as needed to work with text files of data, such as csvs, json, or text files etc.
            Using the developer extension is allowed for more sophisticated tasks or instructed to (js or py can be helpful for more complex tasks if tools are available).

            Accessing web sites, even apis, may be common (you can use scripting to do this) without troubling them too much (they won't know what limits are).
            Try to do your best to find ways to complete a task without too many questions or offering options unless it is really unclear, find a way if you can.
            You can also guide them steps if they can help out as you go along.

            There is already a screenshot tool available you can use if needed to see what is on screen.

            ## How to operate the computer (read this before using computer_control)

            These principles apply on every operating system. Follow them to avoid
            wasting time and tokens going back and forth without making progress:

            1. Work PROGRESSIVELY in small, verifiable steps. Do ONE action
               (activate an app, click a control, type text), then confirm its
               effect — take a screenshot or query the UI/app state — BEFORE the
               next action. Do not chain many blind UI actions at once.
            2. NEVER repeat an action that did not visibly change anything. If the
               same step fails or has no effect twice, STOP repeating it. Re-read
               the latest screenshot/output, change your approach, or report what
               you observe and ask the user — looping wastes their tokens.
            3. PERMISSIONS are the most common real failure, not your script. If a
               control script reports a permission/accessibility/automation error
               (e.g. "not allowed", "assistive access", "not authorized"), the OS
               is blocking automation. Tell the user exactly which permission to
               grant (e.g. Accessibility / Screen Recording / Automation for the
               app) and stop — do NOT retry the same script until they confirm.
            4. Prefer the most RELIABLE method available, in this order: (a) the
               application's own automation/scripting interface or a CLI, (b)
               keyboard navigation and shortcuts, (c) clicking a named UI element,
               and only as a last resort (d) clicking raw screen coordinates.
               Coordinate clicks are brittle and are a frequent cause of getting
               stuck. Before clicking, identify the target element by name/role;
               if you cannot find it, list the available elements/windows rather
               than guessing where it is.
            5. SCREENSHOTS ARE PER-DISPLAY. The machine may have more than one
               monitor. The screen_capture tool reports the full list of connected
               displays and which one is primary; the window you need may be on a
               non-primary display, so target the correct display index instead of
               assuming everything is on display 0. To capture a specific app, you
               can also screen-capture by window title (a substring is enough).
            6. RECOVER FROM POPUPS AND HANGS. If a control action times out or
               hangs, or a search box / dialog / menu / autocomplete popup is open
               and not behaving as you expect, the UI is blocked. Press Escape to
               dismiss the popup (Escape again if needed), take a screenshot to see
               the real state, and only then choose your next action. Never keep
               typing into a popup that isn't responding, and never re-send a script
               that just timed out — fix the situation first.

            ## Driving messaging & chat apps (Slack, Teams, Discord, Mail, etc.)

            These apps are generally NOT scriptable through their automation
            interface, so you must drive their UI — and you must follow their actual
            UX rather than guessing:
            - To SEND a message: the composer is the text box at the BOTTOM of the
              open conversation. Click into it (it is usually already focused), type
              the message, then press Return/Enter to send. Afterwards take a
              screenshot and confirm the message actually appears in the conversation.
            - To SWITCH channel/DM in Slack: open the quick switcher with Cmd/Ctrl+K,
              type the channel or person name, and press Return to OPEN it. This is
              NOT the same as full-text Search (Cmd/Ctrl+G or the magnifier icon),
              which finds messages and will NOT navigate you to a channel. If you
              opened Search by mistake, press Escape and use the quick switcher.
            - Do NOT assume a channel such as #general exists. If the switcher reports
              "couldn't find anything", press Escape and pick a channel from the left
              sidebar, or just use the conversation that is already open. Do not get
              stuck retyping a channel name that does not exist.
            - Prefer keyboard flow (quick switcher → type → Return → type message →
              Return) over clicking screen coordinates, which is brittle in these apps.

            {os_instructions}

            web_scrape
              - Fetch content from html websites and APIs
              - Save as text, JSON, or binary files
              - Content is cached locally for later use
              - PREFER this as the FIRST tool for fetching any known URL — do not hand-roll
                HTTP fetches in shell scripts (curl/wget/python urllib). Reserve browser
                automation for JS-heavy or interactive sites.
            cache
              - Manage your cached files
              - List, view, delete files
              - Clear all cached data
            The extension automatically manages:
            - Cache directory: {cache_dir}
            - File organization and cleanup
            "#,
            os_instructions = os_specific_instructions,
            cache_dir = cache_dir.display()
        };

        Self {
            tool_router: Self::tool_router(),
            cache_dir,
            active_resources: Arc::new(Mutex::new(HashMap::new())),
            http_client: Client::builder()
                .user_agent(WEB_SCRAPE_USER_AGENT)
                .timeout(std::time::Duration::from_secs(WEB_SCRAPE_TIMEOUT_SECS))
                .build()
                .unwrap(),
            instructions,
            system_automation,
        }
    }

    // Helper function to generate a cache file path
    fn get_cache_path(&self, prefix: &str, extension: &str) -> PathBuf {
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        self.cache_dir
            .join(format!("{}_{}.{}", prefix, timestamp, extension))
    }

    // Helper function to save content to cache
    async fn save_to_cache(
        &self,
        content: &[u8],
        prefix: &str,
        extension: &str,
    ) -> Result<PathBuf, ErrorData> {
        let cache_path = self.get_cache_path(prefix, extension);
        tokio::fs::write(&cache_path, content).await.map_err(|e| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to write to cache: {}", e),
                None,
            )
        })?;
        Ok(cache_path)
    }

    // Helper function to register a file as a resource
    fn register_as_resource(&self, cache_path: &PathBuf, mime_type: &str) -> Result<(), ErrorData> {
        let uri = Url::from_file_path(cache_path)
            .map_err(|_| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    "Invalid cache path".to_string(),
                    None,
                )
            })?
            .to_string();

        let resource = ResourceContents::TextResourceContents {
            uri: uri.clone(),
            text: String::new(), // We'll read it when needed
            mime_type: Some(mime_type.to_string()),
            meta: None,
        };

        self.active_resources.lock().unwrap().insert(uri, resource);
        Ok(())
    }

    /// Fetch content from a web page, API, or feed and save a cached copy
    #[tool(
        name = "web_scrape",
        description = "
            Fetch an HTTP(S) URL for simple web research, APIs, RSS/Atom feeds, and web or news search-result URLs.
            Text and JSON content is returned inline so it can be used immediately, and a cached copy is also saved.
            Prefer this over an automation script when the URL is already known. The content can be saved as:
            - text (for HTML pages)
            - json (for API responses)
            - binary (for images and other files)
            Large responses are truncated inline but remain complete in the cached file.
        "
    )]
    pub async fn web_scrape(
        &self,
        params: Parameters<WebScrapeParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let url = &params.url;
        let save_as = params.save_as;

        // Fetch the content, with ONE retry on transient failures only —
        // connect errors, 429, and 5xx. Deterministic client errors (403/404/…)
        // fail immediately with a status-preserving message plus a recovery
        // hint (issue #25). Timeouts are deliberately NOT retried: the client
        // already waits up to WEB_SCRAPE_TIMEOUT_SECS, so a retry against a
        // server that is still hung would double worst-case latency to ~60 s
        // for no realistic gain.
        let mut response = None;
        let mut last_error = String::new();
        for attempt in 0..2 {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(
                    WEB_SCRAPE_RETRY_BACKOFF_MS,
                ))
                .await;
            }
            match self
                .http_client
                .get(url)
                .header("Accept", "text/markdown, */*")
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        response = Some(resp);
                        break;
                    }
                    last_error = format!(
                        "HTTP request failed with status: {status}.{}",
                        web_scrape_status_hint(status)
                    );
                    if !web_scrape_status_is_retryable(status) {
                        break;
                    }
                }
                Err(e) => {
                    last_error = format!("Failed to fetch URL: {e}");
                    if !e.is_connect() {
                        break;
                    }
                }
            }
        }
        let Some(response) = response else {
            return Err(ErrorData::new(ErrorCode::INTERNAL_ERROR, last_error, None));
        };

        // Process based on save_as parameter
        let (content, extension, mime_type, inline_content) = match save_as {
            SaveAsFormat::Text => {
                let text = response.text().await.map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to get text: {}", e),
                        None,
                    )
                })?;
                (text.as_bytes().to_vec(), "txt", "text/plain", Some(text))
            }
            SaveAsFormat::Json => {
                let text = response.text().await.map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to get text: {}", e),
                        None,
                    )
                })?;
                // Verify it's valid JSON
                serde_json::from_str::<serde_json::Value>(&text).map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Invalid JSON response: {}", e),
                        None,
                    )
                })?;
                (
                    text.as_bytes().to_vec(),
                    "json",
                    "application/json",
                    Some(text),
                )
            }
            SaveAsFormat::Binary => {
                let bytes = response.bytes().await.map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to get bytes: {}", e),
                        None,
                    )
                })?;
                (bytes.to_vec(), "bin", "application/octet-stream", None)
            }
        };

        // Save to cache
        let cache_path = self.save_to_cache(&content, "web", extension).await?;

        // Register as a resource
        self.register_as_resource(&cache_path, mime_type)?;

        let mut result = format!("Content saved to: {}", cache_path.display());
        if let Some(inline_content) = inline_content {
            let (inline_content, truncated) = bounded_web_content(&inline_content);
            result.push_str("\n\nFetched content:\n");
            result.push_str(inline_content);
            if truncated {
                result.push_str("\n\n[Inline content truncated; use the cached file for the complete response.]");
            }
        }

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Create and run small scripts for automation tasks
    #[cfg(target_os = "windows")]
    #[tool(
        name = "automation_script",
        description = "
            Create and run small PowerShell or Batch scripts for automation tasks.
            PowerShell is recommended for most tasks.

            This can run network-aware scripts for web, API, RSS, or news searches when no dedicated search tool exists.
            When embedding a multiline script inside execute_code, beware: String.raw`...` ONLY preserves backslashes.
            It does NOT make ${...} literal — every ${...} (PowerShell's ${env:Path} included) is still parsed as a
            JavaScript expression, and any backtick in the script (PowerShell's escape character) terminates the
            template literal early. Escape a literal dollar-brace as ${\"$\"}{ , or pass the script as a plain quoted
            JS string with \\n escapes, or write it to a file with developer/text_editor (write) and run that file.

            The script is saved to a temporary file and executed.
            Some examples:
            - Sort unique lines: Get-Content file.txt | Sort-Object -Unique
            - Extract CSV column: Import-Csv file.csv | Select-Object -ExpandProperty Column2
            - Find text: Select-String -Pattern 'pattern' -Path file.txt
        "
    )]
    pub async fn automation_script(
        &self,
        params: Parameters<AutomationScriptParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.automation_script_impl(params).await
    }

    /// Create and run small scripts for automation tasks
    #[cfg(not(target_os = "windows"))]
    #[tool(
        name = "automation_script",
        description = "
            Create and run small scripts for automation tasks.
            Supports Shell and Ruby (on macOS).

            This can run network-aware scripts for web, API, RSS, or news searches when no dedicated search tool exists.
            When embedding a multiline script inside execute_code, beware: String.raw`...` ONLY preserves backslashes.
            It does NOT make ${...} literal — every ${...} (bash's ${VAR:-default} or ${!v} included) is still parsed
            as a JavaScript expression, and any backtick in the script (command substitution, markdown fences)
            terminates the template literal early. Escape a literal dollar-brace as ${\"$\"}{ , or pass the script as a
            plain quoted JS string with \\n escapes, or write it to a file with developer/text_editor (write) and run
            that file.

            The script is saved to a temporary file and executed.
            Consider using shell script (bash) for most simple tasks first.
            Ruby is useful for text processing or when you need more sophisticated scripting capabilities.
            Some examples of shell:
                - create a sorted list of unique lines: sort file.txt | uniq
                - extract 2nd column in csv: awk -F ',' '{ print $2}'
                - pattern matching: grep pattern file.txt
        "
    )]
    pub async fn automation_script(
        &self,
        params: Parameters<AutomationScriptParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.automation_script_impl(params).await
    }

    #[allow(clippy::too_many_lines)]
    async fn automation_script_impl(
        &self,
        params: Parameters<AutomationScriptParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let language = params.language;
        let script = &params.script;
        let save_output = params.save_output;

        // Create a temporary directory for the script
        let script_dir = tempfile::tempdir().map_err(|e| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to create temporary directory: {}", e),
                None,
            )
        })?;

        let (shell, shell_arg) = self.system_automation.get_shell_command();

        let command = match language {
            ScriptLanguage::Shell | ScriptLanguage::Batch => {
                let script_path = script_dir.path().join(format!(
                    "script.{}",
                    if cfg!(windows) { "bat" } else { "sh" }
                ));
                fs::write(&script_path, script).map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to write script: {}", e),
                        None,
                    )
                })?;

                // Set execute permissions on Unix systems
                #[cfg(unix)]
                {
                    let mut perms = fs::metadata(&script_path)
                        .map_err(|e| {
                            ErrorData::new(
                                ErrorCode::INTERNAL_ERROR,
                                format!("Failed to get file metadata: {}", e),
                                None,
                            )
                        })?
                        .permissions();
                    perms.set_mode(0o755); // rwxr-xr-x
                    fs::set_permissions(&script_path, perms).map_err(|e| {
                        ErrorData::new(
                            ErrorCode::INTERNAL_ERROR,
                            format!("Failed to set execute permissions: {}", e),
                            None,
                        )
                    })?;
                }

                script_path.display().to_string()
            }
            ScriptLanguage::Ruby => {
                let script_path = script_dir.path().join("script.rb");
                fs::write(&script_path, script).map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to write script: {}", e),
                        None,
                    )
                })?;

                format!("ruby {}", script_path.display())
            }
            ScriptLanguage::Powershell => {
                let script_path = script_dir.path().join("script.ps1");
                fs::write(&script_path, script).map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to write script: {}", e),
                        None,
                    )
                })?;

                script_path.display().to_string()
            }
        };

        // Run the script
        let output = match language {
            ScriptLanguage::Powershell => {
                // For PowerShell, we need to use -File instead of -Command
                Command::new("powershell")
                    .arg("-NoProfile")
                    .arg("-NonInteractive")
                    .arg("-File")
                    .arg(&command)
                    .env("BIOROUTER_TERMINAL", "1")
                    .output()
                    .await
                    .map_err(|e| {
                        ErrorData::new(
                            ErrorCode::INTERNAL_ERROR,
                            format!("Failed to run script: {}", e),
                            None,
                        )
                    })?
            }
            _ => Command::new(shell)
                .arg(shell_arg)
                .arg(&command)
                .env("BIOROUTER_TERMINAL", "1")
                .output()
                .await
                .map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to run script: {}", e),
                        None,
                    )
                })?,
        };

        let output_str = String::from_utf8_lossy(&output.stdout).into_owned();
        let error_str = String::from_utf8_lossy(&output.stderr).into_owned();

        let succeeded = output.status.success();
        let mut result = if succeeded {
            format!("Script completed successfully.\n\nOutput:\n{}", output_str)
        } else {
            format!(
                "Script failed with error code {}.\n\nError:\n{}\nOutput:\n{}",
                output.status, error_str, output_str
            )
        };

        // Save output if requested
        if save_output && !output_str.is_empty() {
            let cache_path = self
                .save_to_cache(output_str.as_bytes(), "script_output", "txt")
                .await?;
            result.push_str(&format!("\n\nOutput saved to: {}", cache_path.display()));

            // Register as a resource
            self.register_as_resource(&cache_path, "text")?;
        }

        if succeeded {
            Ok(CallToolResult::success(vec![Content::text(result)]))
        } else {
            Err(ErrorData::new(ErrorCode::INTERNAL_ERROR, result, None))
        }
    }

    /// Control the computer using system automation
    #[cfg(target_os = "windows")]
    #[tool(
        name = "computer_control",
        description = "
            Control the computer using Windows system automation.

            Features available:
            - PowerShell automation for system control
            - UI automation through PowerShell
            - File and system management
            - Windows-specific features and settings

            Can be combined with screenshot tool for visual task assistance.
        "
    )]
    pub async fn computer_control(
        &self,
        params: Parameters<ComputerControlParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.computer_control_impl(params).await
    }

    /// Control the computer using system automation
    #[cfg(target_os = "macos")]
    #[tool(
        name = "computer_control",
        description = "
            Control the computer using AppleScript (macOS only). Automate applications and system features.

            Key capabilities:
            - Control Applications: Launch, quit, manage apps (Mail, Safari, iTunes, etc)
                - Interact with app-specific feature: (e.g, edit documents, process photos)
                - Perform tasks in third-party apps that support AppleScript
            - UI Automation: Simulate user interactions like, clicking buttons, select menus, type text, filling out forms
            - System Control: Manage settings (volume, brightness, wifi), shutdown/restart, monitor events
            - Web & Email: Open URLs, web automation, send/organize emails, handle attachments
            - Media: Manage music libraries, photo collections, playlists
            - File Operations: Organize files/folders
            - Integration: Calendar, reminders, messages
            - Data: Interact with spreadsheets and documents

            Can be combined with screenshot tool for visual task assistance.
        "
    )]
    pub async fn computer_control(
        &self,
        params: Parameters<ComputerControlParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.computer_control_impl(params).await
    }

    /// Control the computer using system automation
    #[cfg(target_os = "linux")]
    #[tool(
        name = "computer_control",
        description = "
            Control the computer using Linux system automation.

            Features available:
            - Shell scripting for system control
            - X11/Wayland window management
            - D-Bus for system services
            - File and system management
            - Desktop environment control (GNOME, KDE, etc.)
            - Process management and monitoring
            - System settings and configurations

            Can be combined with screenshot tool for visual task assistance.
        "
    )]
    pub async fn computer_control(
        &self,
        params: Parameters<ComputerControlParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.computer_control_impl(params).await
    }

    /// Control the computer using system automation (fallback for other OS)
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    #[tool(
        name = "computer_control",
        description = "Control the computer using system automation. Features available depend on your operating system. Can be combined with screenshot tool for visual task assistance."
    )]
    pub async fn computer_control(
        &self,
        params: Parameters<ComputerControlParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.computer_control_impl(params).await
    }

    async fn computer_control_impl(
        &self,
        params: Parameters<ComputerControlParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let script = &params.script;
        let save_output = params.save_output;

        // Use platform-specific automation. execute_system_script returns Err
        // (with stderr + exit status) when the underlying osascript/PowerShell
        // actually fails, so a failed UI action surfaces as a tool error the
        // model can react to — instead of a fake "success" that makes it retry
        // blindly and "circle".
        //
        // Guard against HANGS. A UI-automation script can block for a long time
        // when the target UI is busy or stuck behind a modal/overlay/search popup
        // (on macOS this shows up as AppleEvent error -1712 after the ~2-minute
        // default Apple-event timeout). Two minutes per stuck call is a huge time
        // and token sink and a major cause of the agent "circling". Bound it with
        // a watchdog so a hung action fails fast with actionable guidance instead.
        // OS-invariant: runs the blocking backend call under a tokio timeout.
        let timeout_secs = std::env::var("BIOROUTER_COMPUTER_CONTROL_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|s| *s > 0)
            .unwrap_or(45);
        let automation = Arc::clone(&self.system_automation);
        let script_owned = script.to_string();
        let run =
            tokio::task::spawn_blocking(move || automation.execute_system_script(&script_owned));
        let output =
            match tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), run).await {
                Ok(join) => join
                    .map_err(|e| {
                        ErrorData::new(
                            ErrorCode::INTERNAL_ERROR,
                            format!("Computer control task failed to run: {e}"),
                            None,
                        )
                    })?
                    .map_err(|e| {
                        ErrorData::new(
                            ErrorCode::INTERNAL_ERROR,
                            format!("Computer control script failed: {e}"),
                            None,
                        )
                    })?,
                Err(_elapsed) => {
                    return Err(ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!(
                            "Computer control script timed out after {timeout_secs}s and was \
                         abandoned. The UI is almost certainly blocked — by a modal dialog, an \
                         open search/autocomplete popup, a menu, or a pending permission prompt. \
                         Do NOT re-run this script. Instead: take a screenshot to see the current \
                         state, press Escape to dismiss any popup, and try a different approach."
                        ),
                        None,
                    ));
                }
            };

        // Many UI-automation scripts succeed without producing stdout (e.g.
        // "activate app", "click button"). Distinguish that from a result with
        // output so the model does not mistake silence for a failure (or vice
        // versa) and re-run the same step.
        let mut result = if output.trim().is_empty() {
            "Script ran with no errors and produced no output (this is normal for UI actions \
             like activating an app or clicking). Verify the effect (e.g. take a screenshot) \
             before assuming it did or did not work — do not blindly repeat the same step."
                .to_string()
        } else {
            format!("Script completed successfully.\n\nOutput:\n{}", output)
        };

        // Save output if requested
        if save_output && !output.is_empty() {
            let cache_path = self
                .save_to_cache(output.as_bytes(), "automation_output", "txt")
                .await?;
            result.push_str(&format!("\n\nOutput saved to: {}", cache_path.display()));

            // Register as a resource
            self.register_as_resource(&cache_path, "text")?;
        }

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Process Excel (XLSX) files to read and manipulate spreadsheet data
    #[tool(
        name = "xlsx_tool",
        description = "
            Process Excel (XLSX) files to read and manipulate spreadsheet data.
            Supports operations:
            - list_worksheets: List all worksheets in the workbook (returns name, index, column_count, row_count)
            - get_columns: Get column names from a worksheet (returns values from the first row)
            - get_range: Get values and formulas from a cell range (e.g., 'A1:C10') (returns a 2D array organized as [row][column])
            - find_text: Search for text in a worksheet (returns a list of (row, column) coordinates)
            - update_cell: Update a single cell's value (returns confirmation message)
            - get_cell: Get value and formula from a specific cell (returns both value and formula if present)
            - save: Save changes back to the file (returns confirmation message)

            Use this when working with Excel spreadsheets to analyze or modify data.
        "
    )]
    pub async fn xlsx_tool(
        &self,
        params: Parameters<XlsxToolParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let path = &params.path;
        let operation = params.operation;

        match operation {
            XlsxOperation::ListWorksheets => {
                let xlsx = xlsx_tool::XlsxTool::new(path)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                let worksheets = xlsx
                    .list_worksheets()
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "{:#?}",
                    worksheets
                ))]))
            }
            XlsxOperation::GetColumns => {
                let xlsx = xlsx_tool::XlsxTool::new(path)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                let worksheet = if let Some(name) = &params.worksheet {
                    xlsx.get_worksheet_by_name(name).map_err(|e| {
                        ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
                    })?
                } else {
                    xlsx.get_worksheet_by_index(0).map_err(|e| {
                        ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
                    })?
                };
                let columns = xlsx
                    .get_column_names(worksheet)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "{:#?}",
                    columns
                ))]))
            }
            XlsxOperation::GetRange => {
                let range = params.range.as_ref().ok_or_else(|| {
                    ErrorData::new(
                        ErrorCode::INVALID_PARAMS,
                        "Missing 'range' parameter".to_string(),
                        None,
                    )
                })?;

                let xlsx = xlsx_tool::XlsxTool::new(path)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                let worksheet = if let Some(name) = &params.worksheet {
                    xlsx.get_worksheet_by_name(name).map_err(|e| {
                        ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
                    })?
                } else {
                    xlsx.get_worksheet_by_index(0).map_err(|e| {
                        ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
                    })?
                };
                let range_data = xlsx
                    .get_range(worksheet, range)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "{:#?}",
                    range_data
                ))]))
            }
            XlsxOperation::FindText => {
                let search_text = params.search_text.as_ref().ok_or_else(|| {
                    ErrorData::new(
                        ErrorCode::INVALID_PARAMS,
                        "Missing 'search_text' parameter".to_string(),
                        None,
                    )
                })?;

                let case_sensitive = params.case_sensitive;

                let xlsx = xlsx_tool::XlsxTool::new(path)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                let worksheet = if let Some(name) = &params.worksheet {
                    xlsx.get_worksheet_by_name(name).map_err(|e| {
                        ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
                    })?
                } else {
                    xlsx.get_worksheet_by_index(0).map_err(|e| {
                        ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
                    })?
                };
                let matches = xlsx
                    .find_in_worksheet(worksheet, search_text, case_sensitive)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Found matches at: {:#?}",
                    matches
                ))]))
            }
            XlsxOperation::UpdateCell => {
                let row = params.row.ok_or_else(|| {
                    ErrorData::new(
                        ErrorCode::INVALID_PARAMS,
                        "Missing 'row' parameter".to_string(),
                        None,
                    )
                })?;
                let col = params.col.ok_or_else(|| {
                    ErrorData::new(
                        ErrorCode::INVALID_PARAMS,
                        "Missing 'col' parameter".to_string(),
                        None,
                    )
                })?;
                let value = params.value.as_ref().ok_or_else(|| {
                    ErrorData::new(
                        ErrorCode::INVALID_PARAMS,
                        "Missing 'value' parameter".to_string(),
                        None,
                    )
                })?;

                let worksheet_name = params.worksheet.as_deref().unwrap_or("Sheet1");

                let mut xlsx = xlsx_tool::XlsxTool::new(path)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                xlsx.update_cell(worksheet_name, row as u32, col as u32, value)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                xlsx.save(path)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Updated cell ({}, {}) to '{}' in worksheet '{}'",
                    row, col, value, worksheet_name
                ))]))
            }
            XlsxOperation::Save => {
                let xlsx = xlsx_tool::XlsxTool::new(path)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                xlsx.save(path)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                Ok(CallToolResult::success(vec![Content::text(
                    "File saved successfully.",
                )]))
            }
            XlsxOperation::GetCell => {
                let row = params.row.ok_or_else(|| {
                    ErrorData::new(
                        ErrorCode::INVALID_PARAMS,
                        "Missing 'row' parameter".to_string(),
                        None,
                    )
                })?;

                let col = params.col.ok_or_else(|| {
                    ErrorData::new(
                        ErrorCode::INVALID_PARAMS,
                        "Missing 'col' parameter".to_string(),
                        None,
                    )
                })?;

                let xlsx = xlsx_tool::XlsxTool::new(path)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                let worksheet = if let Some(name) = &params.worksheet {
                    xlsx.get_worksheet_by_name(name).map_err(|e| {
                        ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
                    })?
                } else {
                    xlsx.get_worksheet_by_index(0).map_err(|e| {
                        ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
                    })?
                };
                let cell_value = xlsx
                    .get_cell_value(worksheet, row as u32, col as u32)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "{:#?}",
                    cell_value
                ))]))
            }
        }
    }

    /// Process DOCX files to extract text and create/update documents
    #[tool(
        name = "docx_tool",
        description = "
            Process DOCX files to extract text and create/update documents.
            Supports operations:
            - extract_text: Extract all text content and structure (headings, TOC) from the DOCX
            - update_doc: Create a new DOCX or update existing one with provided content
              Modes:
              - append: Add content to end of document (default)
              - replace: Replace specific text with new content
              - structured: Add content with specific heading level and styling
              - add_image: Add an image to the document (with optional caption)

            Use this when there is a .docx file that needs to be processed or created.
        "
    )]
    pub async fn docx_tool(
        &self,
        params: Parameters<DocxToolParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let path = &params.path;
        let operation = params.operation;

        // Convert enum to string for the existing implementation
        let operation_str = match operation {
            DocxOperation::ExtractText => "extract_text",
            DocxOperation::UpdateDoc => "update_doc",
        };

        // Convert typed params back to JSON for the internal docx_tool impl
        let json_params = params
            .params
            .as_ref()
            .map(|p| serde_json::to_value(p).unwrap_or(serde_json::Value::Null));

        let result = crate::computercontroller::docx_tool::docx_tool(
            path,
            operation_str,
            params.content.as_deref(),
            json_params.as_ref(),
        )
        .await
        .map_err(|e| ErrorData::new(e.code, e.message, e.data))?;

        Ok(CallToolResult::success(result))
    }

    /// Process PDF files to extract text and images
    #[tool(
        name = "pdf_tool",
        description = "
            Process PDF files to extract text and images.
            Supports operations:
            - extract_text: Extract all text content from the PDF
            - extract_images: Extract and save embedded images to PNG files

            Use this when there is a .pdf file or files that need to be processed.
        "
    )]
    pub async fn pdf_tool(
        &self,
        params: Parameters<PdfToolParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let path = &params.path;
        let operation = params.operation;

        // Convert enum to string for the existing implementation
        let operation_str = match operation {
            PdfOperation::ExtractText => "extract_text",
            PdfOperation::ExtractImages => "extract_images",
        };

        let result =
            crate::computercontroller::pdf_tool::pdf_tool(path, operation_str, &self.cache_dir)
                .await
                .map_err(|e| ErrorData::new(e.code, e.message, e.data))?;

        Ok(CallToolResult::success(result))
    }

    /// Manage cached files and data
    #[tool(
        name = "cache",
        description = "
            Manage cached files and data:
            - list: List all cached files
            - view: View content of a cached file
            - delete: Delete a cached file
            - clear: Clear all cached files
        "
    )]
    pub async fn cache(
        &self,
        params: Parameters<CacheParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let command = params.0.command;
        let path = params.0.path.as_deref();

        match command {
            CacheCommand::List => {
                let mut files = Vec::new();
                for entry in fs::read_dir(&self.cache_dir).map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to read cache directory: {}", e),
                        None,
                    )
                })? {
                    let entry = entry.map_err(|e| {
                        ErrorData::new(
                            ErrorCode::INTERNAL_ERROR,
                            format!("Failed to read directory entry: {}", e),
                            None,
                        )
                    })?;
                    files.push(format!("{}", entry.path().display()));
                }
                files.sort();
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Cached files:\n{}",
                    files.join("\n")
                ))]))
            }
            CacheCommand::View => {
                let path = path.ok_or_else(|| {
                    ErrorData::new(
                        ErrorCode::INVALID_PARAMS,
                        "Missing 'path' parameter for view".to_string(),
                        None,
                    )
                })?;

                let content = tokio::fs::read_to_string(path).await.map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to read file: {}", e),
                        None,
                    )
                })?;

                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Content of {}:\n\n{}",
                    path, content
                ))]))
            }
            CacheCommand::Delete => {
                let path = path.ok_or_else(|| {
                    ErrorData::new(
                        ErrorCode::INVALID_PARAMS,
                        "Missing 'path' parameter for delete".to_string(),
                        None,
                    )
                })?;

                fs::remove_file(path).map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to delete file: {}", e),
                        None,
                    )
                })?;

                // Remove from active resources if present
                if let Ok(url) = Url::from_file_path(path) {
                    self.active_resources
                        .lock()
                        .unwrap()
                        .remove(&url.to_string());
                }

                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Deleted file: {}",
                    path
                ))]))
            }
            CacheCommand::Clear => {
                fs::remove_dir_all(&self.cache_dir).map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to clear cache directory: {}", e),
                        None,
                    )
                })?;
                fs::create_dir_all(&self.cache_dir).map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to recreate cache directory: {}", e),
                        None,
                    )
                })?;

                // Clear active resources
                self.active_resources.lock().unwrap().clear();

                Ok(CallToolResult::success(vec![Content::text(
                    "Cache cleared successfully.",
                )]))
            }
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for ComputerControllerServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: "biorouter-computercontroller".to_string(),
                // User-facing display name. The internal id/routing key stays
                // "computercontroller" (tools are prefixed `computercontroller__`),
                // but clients that honor the MCP `title` show "Computer Controller".
                title: Some("Computer Controller".to_string()),
                version: env!("CARGO_PKG_VERSION").to_owned(),
                icons: None,
                website_url: None,
            },
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
            instructions: Some(self.instructions.clone()),
            ..Default::default()
        }
    }

    async fn list_resources(
        &self,
        _pagination: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        let active_resources = self.active_resources.lock().unwrap();
        let resources: Vec<Resource> = active_resources
            .keys()
            .map(|uri| {
                RawResource::new(
                    uri.clone(),
                    uri.split('/').next_back().unwrap_or("").to_string(),
                )
                .no_annotation()
            })
            .collect();
        Ok(ListResourcesResult {
            resources,
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        params: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, ErrorData> {
        let active_resources = self.active_resources.lock().unwrap();
        let resource = active_resources.get(&params.uri).ok_or_else(|| {
            ErrorData::new(
                ErrorCode::INVALID_REQUEST,
                format!("Resource not found: {}", params.uri),
                None,
            )
        })?;

        // Clone the resource to return
        Ok(ReadResourceResult {
            contents: vec![resource.clone()],
        })
    }
}

#[cfg(test)]
mod web_and_script_tests {
    use super::*;
    use rmcp::model::RawContent;
    use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

    fn text_of(result: &CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|content| match &content.raw {
                RawContent::Text(text) => Some(text.text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn inline_web_content_respects_utf8_boundaries() {
        let content = format!("{}é", "a".repeat(MAX_INLINE_WEB_CONTENT_BYTES - 1));
        let (bounded, truncated) = bounded_web_content(&content);

        assert!(truncated);
        assert_eq!(bounded.len(), MAX_INLINE_WEB_CONTENT_BYTES - 1);
        assert!(bounded.is_char_boundary(bounded.len()));
    }

    #[tokio::test]
    async fn web_scrape_returns_text_inline_and_keeps_cached_copy() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "<rss><channel><item><title>Apple Watch update</title></item></channel></rss>",
            ))
            .mount(&mock_server)
            .await;

        let cache = tempfile::tempdir().unwrap();
        let mut server = ComputerControllerServer::new();
        server.cache_dir = cache.path().to_path_buf();
        let result = server
            .web_scrape(Parameters(WebScrapeParams {
                url: mock_server.uri(),
                save_as: SaveAsFormat::Text,
            }))
            .await
            .expect("web fetch should succeed");
        let text = text_of(&result);

        assert!(text.contains("Fetched content:"));
        assert!(text.contains("Apple Watch update"));
        let saved_path = text
            .lines()
            .next()
            .unwrap()
            .strip_prefix("Content saved to: ")
            .unwrap();
        assert!(std::path::Path::new(saved_path).is_file());
    }

    // ---- issue #25: web_scrape hardening -------------------------------------

    /// A 403 is deterministic: the error must keep the status (the tool-error
    /// classifier keys on '403' → permission_denied), carry the bot-block hint,
    /// and must NOT be retried.
    #[tokio::test]
    async fn web_scrape_403_keeps_status_adds_hint_and_does_not_retry() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(403))
            .expect(1) // exactly one request — no retry on 403
            .mount(&mock_server)
            .await;

        let cache = tempfile::tempdir().unwrap();
        let mut server = ComputerControllerServer::new();
        server.cache_dir = cache.path().to_path_buf();
        let error = server
            .web_scrape(Parameters(WebScrapeParams {
                url: mock_server.uri(),
                save_as: SaveAsFormat::Text,
            }))
            .await
            .expect_err("403 must be a tool error");

        let message = error.to_string();
        assert!(message.contains("403"), "status must survive: {message}");
        assert!(
            message.contains("blocks automated clients"),
            "403 must carry the bot-block hint: {message}"
        );
    }

    /// A 404 is deterministic too: no retry, and the hint says to verify the URL.
    #[tokio::test]
    async fn web_scrape_404_keeps_status_adds_hint_and_does_not_retry() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .expect(1)
            .mount(&mock_server)
            .await;

        let cache = tempfile::tempdir().unwrap();
        let mut server = ComputerControllerServer::new();
        server.cache_dir = cache.path().to_path_buf();
        let error = server
            .web_scrape(Parameters(WebScrapeParams {
                url: mock_server.uri(),
                save_as: SaveAsFormat::Text,
            }))
            .await
            .expect_err("404 must be a tool error");

        let message = error.to_string();
        assert!(message.contains("404"), "status must survive: {message}");
        assert!(
            message.contains("verify the URL"),
            "404 must carry the verify-URL hint: {message}"
        );
    }

    /// A transient 500 gets exactly one retry; the second attempt's 200 wins.
    #[tokio::test]
    async fn web_scrape_retries_once_on_5xx_then_succeeds() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .up_to_n_times(1)
            .expect(1)
            .with_priority(1)
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("recovered content"))
            .expect(1)
            .with_priority(2)
            .mount(&mock_server)
            .await;

        let cache = tempfile::tempdir().unwrap();
        let mut server = ComputerControllerServer::new();
        server.cache_dir = cache.path().to_path_buf();
        let result = server
            .web_scrape(Parameters(WebScrapeParams {
                url: mock_server.uri(),
                save_as: SaveAsFormat::Text,
            }))
            .await
            .expect("500-then-200 must succeed after one retry");

        assert!(text_of(&result).contains("recovered content"));
    }

    /// A request timeout is NOT retried (review follow-up on #25): the retry
    /// contract is connect errors, 429, and 5xx only. The client already
    /// waited the full request timeout, so a retry against a still-hung
    /// server would double worst-case latency for no realistic gain. The
    /// mock's `expect(1)` is verified on drop — a retry fails the test.
    #[tokio::test]
    async fn web_scrape_timeout_is_not_retried() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(std::time::Duration::from_secs(5))
                    .set_body_string("too late"),
            )
            .expect(1) // exactly one request — a timeout must not be retried
            .mount(&mock_server)
            .await;

        let cache = tempfile::tempdir().unwrap();
        let mut server = ComputerControllerServer::new();
        server.cache_dir = cache.path().to_path_buf();
        // Production client shape with the 30 s timeout shrunk, so the test
        // observes the timeout path in milliseconds instead of half a minute.
        server.http_client = Client::builder()
            .user_agent(WEB_SCRAPE_USER_AGENT)
            .timeout(std::time::Duration::from_millis(200))
            .build()
            .unwrap();

        let error = server
            .web_scrape(Parameters(WebScrapeParams {
                url: mock_server.uri(),
                save_as: SaveAsFormat::Text,
            }))
            .await
            .expect_err("a timed-out fetch must be a tool error");
        assert!(
            error.to_string().contains("Failed to fetch URL"),
            "timeout must surface as a fetch failure, got: {error}"
        );
    }

    /// The request must carry the browser-compatible UA — the old bare
    /// `biorouter/1.0` was bot-flagged into 403s.
    #[tokio::test]
    async fn web_scrape_sends_browser_compatible_user_agent() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(wiremock::matchers::header(
                "user-agent",
                WEB_SCRAPE_USER_AGENT,
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string("ua ok"))
            .expect(1)
            .mount(&mock_server)
            .await;

        let cache = tempfile::tempdir().unwrap();
        let mut server = ComputerControllerServer::new();
        server.cache_dir = cache.path().to_path_buf();
        let result = server
            .web_scrape(Parameters(WebScrapeParams {
                url: mock_server.uri(),
                save_as: SaveAsFormat::Text,
            }))
            .await
            .expect("UA-matched fetch should succeed");
        assert!(text_of(&result).contains("ua ok"));
    }

    /// The extension instructions must agree with the tool description: the
    /// old "don't use this as the first tool" line steered the model into
    /// hand-rolled shell+urllib fetches (the issue's failure mode).
    #[test]
    fn instructions_prefer_web_scrape_as_first_fetcher() {
        let server = ComputerControllerServer::new();
        assert!(
            !server
                .instructions
                .contains("don't use this as the first tool"),
            "the contradictory steer must be gone"
        );
        assert!(
            server
                .instructions
                .contains("FIRST tool for fetching any known URL"),
            "instructions must prefer web_scrape for known URLs, got: {}",
            &server.instructions
        );
    }

    /// Retryability contract: 429/5xx retryable, deterministic 4xx not.
    #[test]
    fn web_scrape_retryable_statuses() {
        use reqwest::StatusCode;
        assert!(web_scrape_status_is_retryable(
            StatusCode::TOO_MANY_REQUESTS
        ));
        assert!(web_scrape_status_is_retryable(
            StatusCode::INTERNAL_SERVER_ERROR
        ));
        assert!(web_scrape_status_is_retryable(StatusCode::BAD_GATEWAY));
        assert!(!web_scrape_status_is_retryable(StatusCode::FORBIDDEN));
        assert!(!web_scrape_status_is_retryable(StatusCode::NOT_FOUND));
        assert!(!web_scrape_status_is_retryable(StatusCode::BAD_REQUEST));
    }

    /// The automation_script description must carry the CORRECTED String.raw
    /// caveats from #23 (String.raw does not neutralise ${...} interpolation
    /// or backticks), not the old unconditional "use String.raw" steer that
    /// produced the very parse failures #23 fixed. Applies to both platform
    /// variants — the asserted phrases are shared.
    #[test]
    fn automation_script_description_carries_corrected_string_raw_caveats() {
        let server = ComputerControllerServer::new();
        let tool = server
            .tool_router
            .list_all()
            .into_iter()
            .find(|t| t.name == "automation_script")
            .expect("automation_script is registered");
        let description = tool.description.as_deref().unwrap_or_default();
        assert!(
            !description.contains("so backslashes remain intact"),
            "the old unconditional String.raw steer must be gone: {description}"
        );
        assert!(
            description.contains("ONLY preserves backslashes"),
            "must state String.raw's actual (narrow) effect: {description}"
        );
        assert!(
            description.contains("does NOT make ${...} literal"),
            "must correct the dollar-brace belief: {description}"
        );
        assert!(
            description.contains("terminates the template literal"),
            "must warn that payload backticks end the literal: {description}"
        );
        assert!(
            description.contains(r#"${"$"}{"#),
            "must teach the literal dollar-brace escape: {description}"
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[tokio::test]
    async fn automation_script_nonzero_exit_is_a_tool_error() {
        let server = ComputerControllerServer::new();
        let error = server
            .automation_script(Parameters(AutomationScriptParams {
                language: ScriptLanguage::Shell,
                script: "printf 'search failed' >&2\nexit 7".to_string(),
                save_output: false,
            }))
            .await
            .expect_err("a nonzero script must not be reported as a successful tool call");

        let message = error.to_string();
        assert!(message.contains("Script failed"));
        assert!(message.contains("search failed"));
    }
}

#[cfg(all(test, target_os = "macos"))]
mod computer_control_tests {
    use super::*;
    use rmcp::model::RawContent;

    fn text_of(result: &rmcp::model::CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|c| match &c.raw {
                RawContent::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[tokio::test]
    async fn computer_control_reports_failure_instead_of_fake_success() {
        let server = ComputerControllerServer::new();
        let result = server
            .computer_control(Parameters(ComputerControlParams {
                script: "this is not valid applescript @@@".to_string(),
                save_output: false,
            }))
            .await;
        // The script genuinely fails; the tool must return an error (Err here,
        // which the MCP layer turns into is_error=true) rather than the old
        // "Script completed successfully" with empty output.
        assert!(
            result.is_err(),
            "a failing control script must surface as an error, got: {:?}",
            result.ok().map(|r| text_of(&r))
        );
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("Computer control script failed"),
            "error should be explicit about the failure, got: {msg}"
        );
    }

    #[tokio::test]
    async fn computer_control_no_output_success_guides_to_verify() {
        let server = ComputerControllerServer::new();
        let result = server
            .computer_control(Parameters(ComputerControlParams {
                script: "set _x to 1\nreturn".to_string(),
                save_output: false,
            }))
            .await
            .expect("a valid no-output script should succeed");
        let text = text_of(&result);
        assert!(
            text.contains("no output") && text.contains("screenshot"),
            "no-output success should tell the model to verify rather than repeat, got: {text}"
        );
    }
}
