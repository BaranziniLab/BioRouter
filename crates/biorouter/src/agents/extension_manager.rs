use anyhow::Result;
use axum::http::{HeaderMap, HeaderName};
use chrono::{DateTime, Utc};
use futures::stream::{FuturesUnordered, StreamExt};
use futures::{future, FutureExt};
use rand::{distributions::Alphanumeric, Rng};
use rmcp::service::{ClientInitializeError, ServiceError};
use rmcp::transport::streamable_http_client::{
    AuthRequiredError, StreamableHttpClientTransportConfig, StreamableHttpError,
};
use rmcp::transport::{
    ConfigureCommandExt, DynamicTransportError, StreamableHttpClientTransport, TokioChildProcess,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tempfile::{tempdir, TempDir};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tracing::{error, warn};

use super::extension::{
    ExtensionConfig, ExtensionError, ExtensionInfo, ExtensionResult, PlatformExtensionContext,
    ToolInfo, PLATFORM_EXTENSIONS,
};
use super::tool_execution::ToolCallResult;
use super::types::SharedProvider;
use crate::agents::extension::{Envs, ProcessExit};
use crate::agents::extension_malware_check;
use crate::agents::mcp_client::{McpClient, McpClientBox, McpClientTrait, McpMeta};
use crate::agents::mcp_pool::{PooledEntry, SharedMcpPool};
use crate::config::search_path::SearchPaths;
use crate::config::{get_all_extensions, Config};
use crate::oauth::oauth_flow;
use crate::prompt_template;
use crate::subprocess::configure_command_no_window;
use rmcp::model::{
    CallToolRequestParams, Content, ErrorCode, ErrorData, GetPromptResult, Prompt, Resource,
    ResourceContents, ServerInfo, Tool,
};
use rmcp::transport::auth::AuthClient;
use schemars::_private::NoSerialize;
use serde_json::Value;

/// Tags that wrap every MOIM (`collect_moim`) `<info-msg>` block. Shared so the
/// per-turn dedup in `moim.rs` recognises exactly what this function emits.
pub const MOIM_OPEN_TAG: &str = "<info-msg>";
pub const MOIM_CLOSE_TAG: &str = "</info-msg>";

struct Extension {
    pub config: ExtensionConfig,

    client: McpClientBox,
    server_info: Option<ServerInfo>,
    _temp_dir: Option<tempfile::TempDir>,
    /// True for per-app in-process servers injected via `add_inprocess_server`.
    /// Their `config` is a synthetic name-only marker that is NOT spawnable from
    /// any registry, so they are excluded from `get_extension_configs` (never
    /// persisted/replayed/propagated) and re-injected per connect instead.
    inprocess: bool,
    /// Keeps a shared (pooled) process alive while this extension references it
    /// (BR-54). When the last extension across all sessions drops this `Arc`, the
    /// pool's `Weak` dies and the child process is reaped. `None` for unpooled and
    /// in-process servers, which own their client directly via `_temp_dir`/`client`.
    _pooled: Option<Arc<PooledEntry>>,
}

impl Extension {
    fn new(
        config: ExtensionConfig,
        client: McpClientBox,
        server_info: Option<ServerInfo>,
        temp_dir: Option<tempfile::TempDir>,
    ) -> Self {
        Self {
            client,
            config,
            server_info,
            _temp_dir: temp_dir,
            inprocess: false,
            _pooled: None,
        }
    }

    fn supports_resources(&self) -> bool {
        self.server_info
            .as_ref()
            .and_then(|info| info.capabilities.resources.as_ref())
            .is_some()
    }

    fn get_instructions(&self) -> Option<String> {
        self.server_info
            .as_ref()
            .and_then(|info| info.instructions.clone())
    }

    fn get_client(&self) -> McpClientBox {
        self.client.clone()
    }
}

/// Manages biorouter extensions / MCP clients and their interactions
pub struct ExtensionManager {
    extensions: Mutex<HashMap<String, Extension>>,
    context: PlatformExtensionContext,
    provider: SharedProvider,
    tools_cache: Mutex<Option<Arc<Vec<Tool>>>>,
    tools_cache_version: AtomicU64,
    /// The session's working directory, when known. Extensions loaded from a
    /// session (`Agent::load_extensions_from_session`) set this so the shell
    /// tool and child-process extensions run in the directory the user selected
    /// (e.g. via the GUI folder picker) rather than the daemon's process cwd.
    /// `None` on non-session paths (CLI), which fall back to the process cwd.
    working_dir: Mutex<Option<PathBuf>>,
}

/// A flattened representation of a resource used by the agent to prepare inference
#[derive(Debug, Clone)]
pub struct ResourceItem {
    pub client_name: String,      // The name of the client that owns the resource
    pub uri: String,              // The URI of the resource
    pub name: String,             // The name of the resource
    pub content: String,          // The content of the resource
    pub timestamp: DateTime<Utc>, // The timestamp of the resource
    pub priority: f32,            // The priority of the resource
    pub token_count: Option<u32>, // The token count of the resource (filled in by the agent)
}

impl ResourceItem {
    pub fn new(
        client_name: String,
        uri: String,
        name: String,
        content: String,
        timestamp: DateTime<Utc>,
        priority: f32,
    ) -> Self {
        Self {
            client_name,
            uri,
            name,
            content,
            timestamp,
            priority,
            token_count: None,
        }
    }
}

/// Sanitizes a string by replacing invalid characters with underscores.
/// Valid characters match [a-zA-Z0-9_-]
pub fn normalize(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    for c in input.chars() {
        result.push(match c {
            c if c.is_ascii_alphanumeric() || c == '_' || c == '-' => c,
            c if c.is_whitespace() => continue, // effectively "strip" whitespace
            _ => '_',                           // Replace any other non-ASCII character with '_'
        });
    }
    result.to_lowercase()
}

/// Which registry owns a bundled extension, and therefore which spawn path a
/// `/ext:` selection has to be dispatched to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundledExtensionKind {
    /// A bundled MCP server in `biorouter_mcp::BUILTIN_EXTENSIONS`.
    Builtin,
    /// An in-process platform extension in `PLATFORM_EXTENSIONS`.
    Platform,
}

/// A bundled extension a `/ext:<name>` marker resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundledExtensionTarget {
    kind: BundledExtensionKind,
    /// The name the owning registry is keyed by — what the spawn path in
    /// `add_extension` looks up. For a platform extension this may be a display
    /// string (`"Extension Manager"`), which is exactly why it must never be
    /// handed to the *builtin* lookup.
    name: String,
}

impl BundledExtensionTarget {
    pub fn kind(&self) -> BundledExtensionKind {
        self.kind
    }

    /// The key this extension is stored under once enabled — also the `__`
    /// prefix its tools carry, so it is what the model should be told to use.
    pub fn key(&self) -> String {
        normalize(&crate::config::extensions::name_to_key(&self.name))
    }

    /// The config that enables this target, dispatched to the right variant.
    pub fn into_config(self, description: String) -> ExtensionConfig {
        match self.kind {
            BundledExtensionKind::Builtin => ExtensionConfig::Builtin {
                name: self.name,
                description,
                display_name: None,
                timeout: Some(300),
                bundled: Some(true),
                available_tools: Vec::new(),
            },
            BundledExtensionKind::Platform => ExtensionConfig::Platform {
                name: self.name,
                description,
                bundled: Some(true),
                available_tools: Vec::new(),
            },
        }
    }
}

/// Reduce an extension reference to its comparable id: letters and digits only,
/// lowercased. Collapses the spellings a user or a registry may use for the same
/// extension — `agent_drafter` / `agent-drafter` / `agentdrafter`, and
/// `Extension Manager` / `extensionmanager`.
fn extension_reference_key(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Resolve a `/ext:<name>` target to the bundled extension it names, **by id and
/// owning registry** rather than by display name.
///
/// Two of the five platform extensions are registered under a display string
/// (`"Extension Manager"`, `"Chat Recall"`). The `/ext:` resolver used to hand
/// that string to the *builtin* lookup, which failed with
/// `Unknown builtin extension: Extension Manager` — making a perfectly valid
/// request indistinguishable from a policy refusal (issue #48). Platform targets
/// now resolve to `ExtensionConfig::Platform` and take the platform spawn path.
///
/// Returns `None` for anything that is not bundled; a user-configured
/// stdio/http/inline_python/frontend extension is never auto-enabled from a chat
/// marker, so an operator-disabled entry stays disabled.
pub fn resolve_bundled_extension(requested: &str) -> Option<BundledExtensionTarget> {
    let key = extension_reference_key(requested);
    // The bundled figure server is spelled the British way; accept the American
    // spelling of it too.
    let key = if key == "autovisualizer" {
        "autovisualiser".to_string()
    } else {
        key
    };

    // Platform first: its registry is the smaller, hand-written one and shares
    // no ids with the builtin registry.
    if let Some(def) = PLATFORM_EXTENSIONS.iter().find(|(registry_key, def)| {
        extension_reference_key(registry_key) == key || extension_reference_key(def.name) == key
    }) {
        return Some(BundledExtensionTarget {
            kind: BundledExtensionKind::Platform,
            name: def.1.name.to_string(),
        });
    }

    if let Some(def) = biorouter_mcp::BUILTIN_EXTENSIONS.iter().find(|(k, def)| {
        extension_reference_key(k) == key || extension_reference_key(def.name) == key
    }) {
        return Some(BundledExtensionTarget {
            kind: BundledExtensionKind::Builtin,
            name: def.1.name.to_string(),
        });
    }

    None
}

/// Generates extension name from server info; adds random suffix on collision.
fn generate_extension_name(
    server_info: Option<&ServerInfo>,
    name_exists: impl Fn(&str) -> bool,
) -> String {
    let base = server_info
        .and_then(|info| {
            let name = info.server_info.name.as_str();
            (!name.is_empty()).then(|| normalize(name))
        })
        .unwrap_or_else(|| "unnamed".to_string());

    if !name_exists(&base) {
        return base;
    }

    let suffix: String = rand::thread_rng()
        .sample_iter(Alphanumeric)
        .take(6)
        .map(char::from)
        .collect();

    format!("{base}_{suffix}")
}

fn resolve_command(cmd: &str) -> PathBuf {
    SearchPaths::builder()
        .with_npm()
        .resolve(cmd)
        .unwrap_or_else(|_| {
            // let the OS raise the error
            PathBuf::from(cmd)
        })
}

fn require_str_parameter<'a>(v: &'a serde_json::Value, name: &str) -> Result<&'a str, ErrorData> {
    let v = v.get(name).ok_or_else(|| {
        ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            format!("The parameter {name} is required"),
            None,
        )
    })?;
    match v.as_str() {
        Some(r) => Ok(r),
        None => Err(ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            format!("The parameter {name} must be a string"),
            None,
        )),
    }
}

pub fn get_parameter_names(tool: &Tool) -> Vec<String> {
    let mut names: Vec<String> = tool
        .input_schema
        .get("properties")
        .and_then(|props| props.as_object())
        .map(|props| props.keys().cloned().collect())
        .unwrap_or_default();
    names.sort();
    names
}

/// The environment every stdio-extension child is spawned with.
///
/// Runs after the caller has applied the extension's own declared `envs` /
/// `env_keys`, and is the last thing to touch the child's environment before
/// [`TokioChildProcess`] spawns it — which is what lets it strip BioRouter's
/// daemon-private variables from a child no matter who put them there.
///
/// The working directory is resolved here too (explicit argument first, then
/// `BIOROUTER_WORKING_DIR`).
fn prepare_child_environment(command: &mut Command, working_dir: Option<&PathBuf>) {
    if let Ok(path) = SearchPaths::builder().path() {
        command.env("PATH", path);
    }

    // Use explicitly passed working_dir, falling back to BIOROUTER_WORKING_DIR env var
    let effective_working_dir = working_dir.map(|p| p.to_path_buf()).or_else(|| {
        std::env::var("BIOROUTER_WORKING_DIR")
            .ok()
            .map(PathBuf::from)
    });

    if let Some(ref dir) = effective_working_dir {
        if dir.exists() && dir.is_dir() {
            tracing::info!("Setting MCP process working directory: {:?}", dir);
            command.current_dir(dir);
            // Also set BIOROUTER_WORKING_DIR env var for the child process
            command.env("BIOROUTER_WORKING_DIR", dir);
        } else {
            tracing::warn!(
                "Working directory doesn't exist or isn't a directory: {:?}",
                dir
            );
        }
    } else {
        tracing::info!("No working directory specified, using default");
    }

    // Last, so neither the block above nor the extension's own declared `envs`
    // can leave a daemon credential in the child (issue #57). An extension that
    // needs its own secrets still gets them: they are not in BioRouter's
    // namespace, so the policy never looks at them.
    biorouter_mcp::developer::shell::strip_daemon_private_env(command);
}

async fn child_process_client(
    mut command: Command,
    timeout: &Option<u64>,
    provider: SharedProvider,
    working_dir: Option<&PathBuf>,
    routed_only: bool,
) -> ExtensionResult<McpClient> {
    #[cfg(unix)]
    command.process_group(0);
    configure_command_no_window(&mut command);

    prepare_child_environment(&mut command, working_dir);

    let (transport, mut stderr) = TokioChildProcess::builder(command)
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stderr = stderr.take().ok_or_else(|| {
        ExtensionError::SetupError("failed to attach child process stderr".to_owned())
    })?;

    let stderr_task = tokio::spawn(async move {
        let mut all_stderr = Vec::new();
        stderr.read_to_end(&mut all_stderr).await?;
        Ok::<String, std::io::Error>(String::from_utf8_lossy(&all_stderr).into())
    });

    let client_result = McpClient::connect_routed(
        transport,
        Duration::from_secs(timeout.unwrap_or(crate::config::DEFAULT_EXTENSION_TIMEOUT)),
        provider,
        routed_only,
    )
    .await;

    match client_result {
        Ok(client) => Ok(client),
        Err(error) => {
            let error_task_out = stderr_task.await?;
            Err::<McpClient, ExtensionError>(match error_task_out {
                Ok(stderr_content) => ProcessExit::new(stderr_content, error).into(),
                Err(e) => e.into(),
            })
        }
    }
}

fn extract_auth_error(
    res: &Result<McpClient, ClientInitializeError>,
) -> Option<&AuthRequiredError> {
    match res {
        Ok(_) => None,
        Err(err) => match err {
            ClientInitializeError::TransportError {
                error: DynamicTransportError { error, .. },
                ..
            } => error
                .downcast_ref::<StreamableHttpError<reqwest::Error>>()
                .and_then(|auth_error| match auth_error {
                    StreamableHttpError::AuthRequired(auth_required_error) => {
                        Some(auth_required_error)
                    }
                    _ => None,
                }),
            _ => None,
        },
    }
}

/// Merge environment variables from direct envs and keychain-stored env_keys
async fn merge_environments(
    envs: &Envs,
    env_keys: &[String],
    ext_name: &str,
) -> Result<HashMap<String, String>, ExtensionError> {
    let mut all_envs = envs.get_env();
    let config_instance = Config::global();

    for key in env_keys {
        if all_envs.contains_key(key) {
            continue;
        }

        match config_instance.get(key, true) {
            Ok(value) => {
                if value.is_null() {
                    warn!(
                        key = %key,
                        ext_name = %ext_name,
                        "Secret key not found in config (returned null)."
                    );
                    continue;
                }

                if let Some(str_val) = value.as_str() {
                    all_envs.insert(key.clone(), str_val.to_string());
                } else {
                    warn!(
                        key = %key,
                        ext_name = %ext_name,
                        value_type = %value.get("type").and_then(|t| t.as_str()).unwrap_or("unknown"),
                        "Secret value is not a string; skipping."
                    );
                }
            }
            Err(e) => {
                error!(
                    key = %key,
                    ext_name = %ext_name,
                    error = %e,
                    "Failed to fetch secret from config."
                );
                return Err(ExtensionError::ConfigError(format!(
                    "Failed to fetch secret '{}' from config: {}",
                    key, e
                )));
            }
        }
    }

    Ok(all_envs)
}

/// Substitute environment variables in a string. Supports both ${VAR} and $VAR syntax.
fn substitute_env_vars(value: &str, env_map: &HashMap<String, String>) -> String {
    // Compiled once process-wide rather than on every call (this runs once per
    // extension HTTP header during connection setup).
    static RE_BRACES: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(|| {
        regex::Regex::new(r"\$\{\s*([A-Za-z_][A-Za-z0-9_]*)\s*\}").expect("valid regex")
    });
    static RE_SIMPLE: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(|| {
        regex::Regex::new(r"\$([A-Za-z_][A-Za-z0-9_]*)").expect("valid regex")
    });

    let mut result = value.to_string();

    for cap in RE_BRACES.captures_iter(value) {
        if let Some(var_name) = cap.get(1) {
            if let Some(env_value) = env_map.get(var_name.as_str()) {
                result = result.replace(&cap[0], env_value);
            }
        }
    }

    let result_snapshot = result.clone();
    for cap in RE_SIMPLE.captures_iter(&result_snapshot) {
        if let Some(var_name) = cap.get(1) {
            if !value.contains(&format!("${{{}}}", var_name.as_str())) {
                if let Some(env_value) = env_map.get(var_name.as_str()) {
                    result = result.replace(&cap[0], env_value);
                }
            }
        }
    }

    result
}

async fn create_streamable_http_client(
    uri: &str,
    timeout: Option<u64>,
    headers: &HashMap<String, String>,
    name: &str,
    all_envs: &HashMap<String, String>,
    provider: SharedProvider,
    routed_only: bool,
) -> ExtensionResult<Box<dyn McpClientTrait>> {
    let mut default_headers = HeaderMap::new();
    for (key, value) in headers {
        let substituted_value = substitute_env_vars(value, all_envs);
        default_headers.insert(
            HeaderName::try_from(key)
                .map_err(|_| ExtensionError::ConfigError(format!("invalid header: {}", key)))?,
            substituted_value.parse().map_err(|_| {
                ExtensionError::ConfigError(format!("invalid header value: {}", key))
            })?,
        );
    }

    let http_client = reqwest::Client::builder()
        .default_headers(default_headers)
        .build()
        .map_err(|_| ExtensionError::ConfigError("could not construct http client".to_string()))?;

    let transport = StreamableHttpClientTransport::with_client(
        http_client,
        StreamableHttpClientTransportConfig {
            uri: uri.into(),
            ..Default::default()
        },
    );

    let timeout_duration =
        Duration::from_secs(timeout.unwrap_or(crate::config::DEFAULT_EXTENSION_TIMEOUT));

    let client_res =
        McpClient::connect_routed(transport, timeout_duration, provider.clone(), routed_only).await;

    if extract_auth_error(&client_res).is_some() {
        let am = oauth_flow(&uri.to_string(), &name.to_string())
            .await
            .map_err(|_| ExtensionError::SetupError("auth error".to_string()))?;
        let auth_client = AuthClient::new(reqwest::Client::default(), am);
        let transport = StreamableHttpClientTransport::with_client(
            auth_client,
            StreamableHttpClientTransportConfig {
                uri: uri.into(),
                ..Default::default()
            },
        );
        Ok(Box::new(
            McpClient::connect_routed(transport, timeout_duration, provider, routed_only).await?,
        ))
    } else {
        Ok(Box::new(client_res?))
    }
}

impl ExtensionManager {
    pub fn new(
        provider: SharedProvider,
        session_manager: Arc<crate::session::SessionManager>,
    ) -> Self {
        Self {
            extensions: Mutex::new(HashMap::new()),
            context: PlatformExtensionContext {
                extension_manager: None,
                session_manager,
            },
            provider,
            tools_cache: Mutex::new(None),
            tools_cache_version: AtomicU64::new(0),
            working_dir: Mutex::new(None),
        }
    }

    #[cfg(test)]
    pub fn new_without_provider(data_dir: std::path::PathBuf) -> Self {
        let session_manager = Arc::new(crate::session::SessionManager::new(data_dir));
        Self::new(Arc::new(Mutex::new(None)), session_manager)
    }

    pub fn get_context(&self) -> &PlatformExtensionContext {
        &self.context
    }

    /// Set the session working directory that newly-loaded extensions should run
    /// in. Called before (re)loading a session's extensions, so a change made via
    /// the GUI folder picker — which restarts the agent and reloads extensions —
    /// takes effect for the shell tool and child-process extensions.
    pub async fn set_working_dir(&self, dir: PathBuf) {
        *self.working_dir.lock().await = Some(dir);
    }

    /// Resolve the working directory for an extension.
    /// Prefers the session working directory (set via `set_working_dir`), and
    /// falls back to the process cwd when it is not available (e.g. the CLI).
    async fn resolve_working_dir(&self) -> PathBuf {
        if let Some(dir) = self.working_dir.lock().await.clone() {
            return dir;
        }
        std::env::current_dir().unwrap_or_default()
    }

    pub async fn supports_resources(&self) -> bool {
        self.extensions
            .lock()
            .await
            .values()
            .any(|ext| ext.supports_resources())
    }

    #[allow(clippy::too_many_lines)]
    pub async fn add_extension(self: &Arc<Self>, config: ExtensionConfig) -> ExtensionResult<()> {
        let config_name = config.key().to_string();
        let sanitized_name = normalize(&config_name);

        if self.extensions.lock().await.contains_key(&sanitized_name) {
            return Ok(());
        }

        // Resolve working_dir: session > current_dir
        let effective_working_dir = self.resolve_working_dir().await;

        // BR-54: when the SharedMcpPool is enabled and this variant is poolable,
        // reuse ONE process across sessions keyed by spawn identity; otherwise
        // spawn a private, unshared client (byte-identical to the old behavior).
        // A pooled (shared) client isolates notifications per dispatch
        // (`routed_only`); a private client keeps the legacy broadcast.
        let pool_key = if SharedMcpPool::is_enabled() {
            config.pool_key(&effective_working_dir)
        } else {
            None
        };
        let routed_only = pool_key.is_some();

        // The actual client construction, deferred into a closure so the pool can
        // skip it entirely when a live shared client already exists (single-flight).
        let spawn = {
            let this = Arc::clone(self);
            let config = config.clone();
            let working_dir = effective_working_dir.clone();
            let sanitized_name = sanitized_name.clone();
            move || async move {
                let mut temp_dir = None;
                let client: Box<dyn McpClientTrait> = match &config {
                    ExtensionConfig::Sse { .. } => {
                        return Err(ExtensionError::ConfigError(
                            "SSE is unsupported, migrate to streamable_http".to_string(),
                        ));
                    }
                    ExtensionConfig::StreamableHttp {
                        uri,
                        timeout,
                        headers,
                        name,
                        envs,
                        env_keys,
                        ..
                    } => {
                        let all_envs = merge_environments(envs, env_keys, &sanitized_name).await?;
                        create_streamable_http_client(
                            uri,
                            *timeout,
                            headers,
                            name,
                            &all_envs,
                            this.provider.clone(),
                            routed_only,
                        )
                        .await?
                    }
                    ExtensionConfig::Stdio {
                        cmd,
                        args,
                        envs,
                        env_keys,
                        timeout,
                        ..
                    } => {
                        let all_envs = merge_environments(envs, env_keys, &sanitized_name).await?;

                        // Check for malicious packages before launching the process
                        extension_malware_check::deny_if_malicious_cmd_args(cmd, args).await?;

                        let cmd = resolve_command(cmd);

                        let command = Command::new(cmd).configure(|command| {
                            command.args(args).envs(all_envs);
                        });

                        let client = child_process_client(
                            command,
                            timeout,
                            this.provider.clone(),
                            Some(&working_dir),
                            routed_only,
                        )
                        .await?;
                        Box::new(client)
                    }
                    ExtensionConfig::Builtin { name, timeout, .. } => {
                        let timeout_duration = Duration::from_secs(timeout.unwrap_or(300));
                        let def = biorouter_mcp::BUILTIN_EXTENSIONS
                            .get(name.as_str())
                            .ok_or_else(|| {
                                ExtensionError::ConfigError(format!(
                                    "Unknown builtin extension: {}",
                                    name
                                ))
                            })?;
                        let (server_read, client_write) = tokio::io::duplex(65536);
                        let (client_read, server_write) = tokio::io::duplex(65536);
                        // Pass the resolved working directory so in-process builtins
                        // that run shell commands (the developer extension) execute in
                        // the session's directory instead of the daemon's process cwd.
                        (def.spawn_server)(server_read, server_write, Some(working_dir.clone()));
                        Box::new(
                            McpClient::connect_routed(
                                (client_read, client_write),
                                timeout_duration,
                                this.provider.clone(),
                                routed_only,
                            )
                            .await?,
                        )
                    }
                    ExtensionConfig::Platform { name, .. } => {
                        let normalized_key = normalize(name);
                        let def = PLATFORM_EXTENSIONS
                            .get(normalized_key.as_str())
                            .ok_or_else(|| {
                                ExtensionError::ConfigError(format!(
                                    "Unknown platform extension: {}",
                                    name
                                ))
                            })?;
                        let mut context = this.context.clone();
                        context.extension_manager = Some(Arc::downgrade(&this));
                        (def.client_factory)(context)
                    }
                    ExtensionConfig::InlinePython {
                        name,
                        code,
                        timeout,
                        dependencies,
                        ..
                    } => {
                        let dir = tempdir()?;
                        let file_path = dir.path().join(format!("{}.py", name));
                        temp_dir = Some(dir);
                        tokio::fs::write(&file_path, code).await?;

                        let command = Command::new("uvx").configure(|command| {
                            command.arg("--with").arg("mcp");
                            dependencies.iter().flatten().for_each(|dep| {
                                command.arg("--with").arg(dep);
                            });
                            command.arg("python").arg(file_path.to_str().unwrap());
                        });

                        let client = child_process_client(
                            command,
                            timeout,
                            this.provider.clone(),
                            Some(&working_dir),
                            routed_only,
                        )
                        .await?;

                        Box::new(client)
                    }
                    ExtensionConfig::Frontend { .. } => {
                        return Err(ExtensionError::ConfigError(
                            "Invalid extension type: Frontend extensions cannot be added as server extensions".to_string()
                        ));
                    }
                };

                let server_info = client.get_info().cloned();
                // `Box<dyn McpClientTrait>` -> `Arc<dyn McpClientTrait>` (H6:
                // the handle is no longer mutex-wrapped).
                Ok(PooledEntry::new(Arc::from(client), server_info, temp_dir))
            }
        };

        let entry = if let Some(key) = pool_key {
            SharedMcpPool::global().get_or_spawn(key, spawn).await?
        } else {
            Arc::new(spawn().await?)
        };

        let server_info = entry.server_info();

        // Only generate name from server info when config has no name (e.g., CLI --with-*-extension args)
        let mut extensions = self.extensions.lock().await;
        let final_name = if sanitized_name.is_empty() {
            generate_extension_name(server_info.as_ref(), |n| extensions.contains_key(n))
        } else {
            sanitized_name
        };
        extensions.insert(
            final_name,
            Extension {
                config,
                client: entry.client(),
                server_info,
                _temp_dir: None,
                inprocess: false,
                _pooled: Some(entry),
            },
        );
        drop(extensions);
        self.invalidate_tools_cache_and_bump_version().await;

        Ok(())
    }

    pub async fn add_client(
        &self,
        name: String,
        config: ExtensionConfig,
        client: McpClientBox,
        info: Option<ServerInfo>,
        temp_dir: Option<TempDir>,
    ) {
        self.extensions
            .lock()
            .await
            .insert(name, Extension::new(config, client, info, temp_dir));
        self.invalidate_tools_cache_and_bump_version().await;
    }

    /// Inject an already-constructed in-process rmcp server into this manager.
    ///
    /// This is the seam for **per-app** servers that carry app context (a
    /// workspace path, a data-source map, a jail) which the no-arg
    /// `BUILTIN_EXTENSIONS` registry cannot express. It mirrors the `Builtin`
    /// spawn path (duplex transport → serve task → `McpClient::connect`) but
    /// serves the *provided* instance instead of looking one up by name.
    pub async fn add_inprocess_server<S>(&self, name: &str, server: S) -> ExtensionResult<()>
    where
        S: rmcp::ServerHandler + Send + 'static,
    {
        use rmcp::ServiceExt;
        // Idempotent: configure_agent re-runs on every (re)connect against the
        // same cached agent, so skip if this server is already injected (avoids
        // a redundant connect/handshake + a transient double-server).
        if self.extensions.lock().await.contains_key(name) {
            return Ok(());
        }
        let (server_read, client_write) = tokio::io::duplex(65536);
        let (client_read, server_write) = tokio::io::duplex(65536);
        let label = name.to_string();
        tokio::spawn(async move {
            match server.serve((server_read, server_write)).await {
                Ok(running) => {
                    let _ = running.waiting().await;
                }
                Err(e) => {
                    tracing::error!(server = %label, error = %e, "in-process server error")
                }
            }
        });
        let client = McpClient::connect(
            (client_read, client_write),
            Duration::from_secs(300),
            self.provider.clone(),
        )
        .await?;
        let info = client.get_info().cloned();
        // A synthetic name-only marker config. It is NEVER spawned from a
        // registry (the `inprocess` flag excludes it from get_extension_configs),
        // so the absence of a "datasql" builtin entry is fine — it exists only so
        // extension listings have a name.
        let config = ExtensionConfig::Builtin {
            name: name.to_string(),
            description: name.to_string(),
            display_name: None,
            timeout: Some(300),
            bundled: None,
            available_tools: Vec::new(),
        };
        self.extensions.lock().await.insert(
            name.to_string(),
            Extension {
                config,
                client: Arc::new(client),
                server_info: info,
                _temp_dir: None,
                inprocess: true,
                _pooled: None,
            },
        );
        self.invalidate_tools_cache_and_bump_version().await;
        Ok(())
    }

    /// Get extensions info for building the system prompt
    pub async fn get_extensions_info(&self) -> Vec<ExtensionInfo> {
        self.extensions
            .lock()
            .await
            .iter()
            .map(|(name, ext)| {
                ExtensionInfo::new(
                    name,
                    ext.get_instructions().unwrap_or_default().as_str(),
                    ext.supports_resources(),
                )
            })
            .collect()
    }

    /// Get aggregated usage statistics
    pub async fn remove_extension(&self, name: &str) -> ExtensionResult<()> {
        let sanitized_name = normalize(name);
        self.extensions.lock().await.remove(&sanitized_name);
        self.invalidate_tools_cache_and_bump_version().await;
        Ok(())
    }

    pub async fn list_extensions(&self) -> ExtensionResult<Vec<String>> {
        Ok(self.extensions.lock().await.keys().cloned().collect())
    }

    pub async fn is_extension_enabled(&self, name: &str) -> bool {
        self.extensions.lock().await.contains_key(name)
    }

    pub async fn get_extension_configs(&self) -> Vec<ExtensionConfig> {
        self.extensions
            .lock()
            .await
            .values()
            // Exclude per-app in-process servers: their config is a name-only
            // marker that no registry can re-spawn, so it must never be persisted,
            // replayed on resume, or propagated to sub-agents (it would fail to
            // load). They are re-injected per connect via configure_agent.
            .filter(|ext| !ext.inprocess)
            .map(|ext| ext.config.clone())
            .collect()
    }

    /// Get all tools from all clients with proper prefixing
    pub async fn get_prefixed_tools(
        &self,
        extension_name: Option<String>,
    ) -> ExtensionResult<Vec<Tool>> {
        let all_tools = self.get_all_tools_cached().await?;
        Ok(self.filter_tools(&all_tools, extension_name.as_deref(), None))
    }

    pub async fn get_prefixed_tools_excluding(&self, exclude: &str) -> ExtensionResult<Vec<Tool>> {
        let all_tools = self.get_all_tools_cached().await?;
        Ok(self.filter_tools(&all_tools, None, Some(exclude)))
    }

    fn filter_tools(
        &self,
        tools: &[Tool],
        extension_name: Option<&str>,
        exclude: Option<&str>,
    ) -> Vec<Tool> {
        tools
            .iter()
            .filter(|tool| {
                let tool_prefix = tool.name.as_ref().split("__").next().unwrap_or("");

                if let Some(excluded) = exclude {
                    if tool_prefix == excluded {
                        return false;
                    }
                }

                if let Some(name_filter) = extension_name {
                    tool_prefix == name_filter
                } else {
                    true
                }
            })
            .cloned()
            .collect()
    }

    async fn get_all_tools_cached(&self) -> ExtensionResult<Arc<Vec<Tool>>> {
        {
            let cache = self.tools_cache.lock().await;
            if let Some(ref tools) = *cache {
                return Ok(Arc::clone(tools));
            }
        }

        let version_before = self.tools_cache_version.load(Ordering::SeqCst);
        // Deterministic tool order so the serialized tool-definitions block (which
        // Biorouter caches via Anthropic `cache_control`) is byte-stable across
        // process restarts. The source `extensions` HashMap iterates in a
        // per-process-randomized order, so a session resumed in a fresh process
        // would otherwise get a different tool order and miss the provider prompt
        // cache on its first turn. Tools are a named set — ordering is
        // semantically irrelevant to the model, so sorting is safe.
        let mut fetched = self.fetch_all_tools().await?;
        fetched.sort_by(|a, b| a.name.cmp(&b.name));
        let tools = Arc::new(fetched);

        {
            let mut cache = self.tools_cache.lock().await;
            let version_after = self.tools_cache_version.load(Ordering::SeqCst);
            if version_after == version_before && cache.is_none() {
                *cache = Some(Arc::clone(&tools));
            }
        }

        Ok(tools)
    }

    async fn invalidate_tools_cache_and_bump_version(&self) {
        self.tools_cache_version.fetch_add(1, Ordering::SeqCst);
        *self.tools_cache.lock().await = None;
    }

    async fn fetch_all_tools(&self) -> ExtensionResult<Vec<Tool>> {
        let clients: Vec<_> = self
            .extensions
            .lock()
            .await
            .iter()
            .map(|(name, ext)| (name.clone(), ext.config.clone(), ext.get_client()))
            .collect();

        let client_futures = clients.into_iter().map(|(name, config, client)| {
            let ext_name = name.clone();
            async move {
                let per_ext = async {
                    let cancel_token = CancellationToken::default();
                    let mut tools = Vec::new();
                    let client_guard = &*client;
                    let mut client_tools = match client_guard
                        .list_tools(None, cancel_token.clone())
                        .await
                    {
                        Ok(t) => t,
                        Err(e) => {
                            warn!(extension = %ext_name, error = %e, "Failed to list tools");
                            return vec![];
                        }
                    };

                    loop {
                        for tool in client_tools.tools {
                            if config.is_tool_available(&tool.name) {
                                tools.push(Tool {
                                    name: format!("{}__{}", name, tool.name).into(),
                                    description: tool.description,
                                    input_schema: tool.input_schema,
                                    annotations: tool.annotations,
                                    output_schema: tool.output_schema,
                                    icons: tool.icons,
                                    title: tool.title,
                                    meta: tool.meta,
                                });
                            }
                        }

                        if client_tools.next_cursor.is_none() {
                            break;
                        }

                        client_tools = match client_guard
                            .list_tools(client_tools.next_cursor, cancel_token.clone())
                            .await
                        {
                            Ok(t) => t,
                            Err(e) => {
                                warn!(extension = %ext_name, error = %e, "Failed to list tools (pagination)");
                                break;
                            }
                        };
                    }

                    tools
                };

                // 10s cap per extension — a hanging MCP server (e.g. one waiting on a DB
                // it can't reach) must not block tool listing for all other extensions.
                match tokio::time::timeout(std::time::Duration::from_secs(10), per_ext).await {
                    Ok(tools) => (ext_name, tools),
                    Err(_) => {
                        warn!(extension = %ext_name, "Timed out listing tools after 10s; extension will be skipped");
                        (ext_name, vec![])
                    }
                }
            }
        });

        let results = future::join_all(client_futures).await;

        let mut tools = Vec::new();
        for (_, client_tools) in results {
            tools.extend(client_tools);
        }

        Ok(tools)
    }

    /// Get the extension prompt including client instructions
    pub async fn get_planning_prompt(&self, tools_info: Vec<ToolInfo>) -> String {
        let mut context: HashMap<&str, Value> = HashMap::new();
        context.insert("tools", serde_json::to_value(tools_info).unwrap());

        prompt_template::render_global_file("plan.md", &context).expect("Prompt should render")
    }

    /// Find and return a reference to the appropriate client for a tool call
    async fn get_client_for_tool(&self, prefixed_name: &str) -> Option<(String, McpClientBox)> {
        self.extensions
            .lock()
            .await
            .iter()
            .find(|(key, _)| prefixed_name.starts_with(*key))
            .map(|(name, extension)| (name.clone(), extension.get_client()))
    }

    // Function that gets executed for read_resource tool
    pub async fn read_resource_tool(
        &self,
        params: Value,
        cancellation_token: CancellationToken,
    ) -> Result<Vec<Content>, ErrorData> {
        let uri = require_str_parameter(&params, "uri")?;

        let extension_name = params.get("extension_name").and_then(|v| v.as_str());

        // If extension name is provided, we can just look it up
        if let Some(ext_name) = extension_name {
            let read_result = self
                .read_resource(uri, ext_name, cancellation_token.clone())
                .await?;

            let mut result = Vec::new();
            for content in read_result.contents {
                if let ResourceContents::TextResourceContents { text, .. } = content {
                    let content_str = format!("{}\n\n{}", uri, text);
                    result.push(Content::text(content_str));
                }
            }
            return Ok(result);
        }

        // If extension name is not provided, we need to search for the resource across all extensions
        // Loop through each extension and try to read the resource, don't raise an error if the resource is not found
        // TODO: do we want to find if a provided uri is in multiple extensions?
        // currently it will return the first match and skip any others

        // Collect extension names first to avoid holding the lock during iteration
        let extension_names: Vec<String> = self.extensions.lock().await.keys().cloned().collect();

        for extension_name in extension_names {
            let read_result = self
                .read_resource(uri, &extension_name, cancellation_token.clone())
                .await;
            match read_result {
                Ok(read_result) => {
                    let mut result = Vec::new();
                    for content in read_result.contents {
                        if let ResourceContents::TextResourceContents { text, .. } = content {
                            let content_str = format!("{}\n\n{}", uri, text);
                            result.push(Content::text(content_str));
                        }
                    }
                    return Ok(result);
                }
                Err(_) => continue,
            }
        }

        // None of the extensions had the resource so we raise an error
        let available_extensions = self
            .extensions
            .lock()
            .await
            .keys()
            .map(|s| s.as_str())
            .collect::<Vec<&str>>()
            .join(", ");
        let error_msg = format!(
            "Resource with uri '{}' not found. Here are the available extensions: {}",
            uri, available_extensions
        );

        Err(ErrorData::new(
            ErrorCode::RESOURCE_NOT_FOUND,
            error_msg,
            None,
        ))
    }

    pub async fn read_resource(
        &self,
        uri: &str,
        extension_name: &str,
        cancellation_token: CancellationToken,
    ) -> Result<rmcp::model::ReadResourceResult, ErrorData> {
        let available_extensions = self
            .extensions
            .lock()
            .await
            .keys()
            .map(|s| s.as_str())
            .collect::<Vec<&str>>()
            .join(", ");
        let error_msg = format!(
            "Extension '{}' not found. Here are the available extensions: {}",
            extension_name, available_extensions
        );

        let client = self
            .get_server_client(extension_name)
            .await
            .ok_or(ErrorData::new(ErrorCode::INVALID_PARAMS, error_msg, None))?;

        let client_guard = &*client;
        client_guard
            .read_resource(uri, cancellation_token)
            .await
            .map_err(|_| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Could not read resource with uri: {}", uri),
                    None,
                )
            })
    }

    pub async fn get_ui_resources(&self) -> Result<Vec<(String, Resource)>, ErrorData> {
        let mut ui_resources = Vec::new();

        let extensions_to_check: Vec<(String, McpClientBox)> = {
            let extensions = self.extensions.lock().await;
            extensions
                .iter()
                .map(|(name, ext)| (name.clone(), ext.get_client()))
                .collect()
        };

        for (extension_name, client) in extensions_to_check {
            let client_guard = &*client;

            match client_guard
                .list_resources(None, CancellationToken::default())
                .await
            {
                Ok(list_response) => {
                    for resource in list_response.resources {
                        if resource.uri.starts_with("ui://") {
                            ui_resources.push((extension_name.clone(), resource));
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to list resources for {}: {:?}", extension_name, e);
                }
            }
        }

        Ok(ui_resources)
    }

    async fn list_resources_from_extension(
        &self,
        extension_name: &str,
        cancellation_token: CancellationToken,
    ) -> Result<Vec<Content>, ErrorData> {
        let client = self
            .get_server_client(extension_name)
            .await
            .ok_or_else(|| {
                ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    format!("Extension {} is not valid", extension_name),
                    None,
                )
            })?;

        let client_guard = &*client;
        client_guard
            .list_resources(None, cancellation_token)
            .await
            .map_err(|e| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Unable to list resources for {}, {:?}", extension_name, e),
                    None,
                )
            })
            .map(|lr| {
                let resource_list = lr
                    .resources
                    .into_iter()
                    .map(|r| format!("{} - {}, uri: ({})", extension_name, r.name, r.uri))
                    .collect::<Vec<String>>()
                    .join("\n");

                vec![Content::text(resource_list)]
            })
    }

    pub async fn list_resources(
        &self,
        params: Value,
        cancellation_token: CancellationToken,
    ) -> Result<Vec<Content>, ErrorData> {
        let extension = params.get("extension").and_then(|v| v.as_str());

        match extension {
            Some(extension_name) => {
                // Handle single extension case
                self.list_resources_from_extension(extension_name, cancellation_token)
                    .await
            }
            None => {
                // Handle all extensions case using FuturesUnordered
                let mut futures = FuturesUnordered::new();

                // Create futures for each resource_capable_extension
                self.extensions
                    .lock()
                    .await
                    .iter()
                    .filter(|(_name, ext)| ext.supports_resources())
                    .map(|(name, _ext)| name.clone())
                    .for_each(|name| {
                        let token = cancellation_token.clone();
                        futures.push(async move {
                            self.list_resources_from_extension(&name.clone(), token)
                                .await
                        });
                    });

                let mut all_resources = Vec::new();
                let mut errors = Vec::new();

                // Process results as they complete
                while let Some(result) = futures.next().await {
                    match result {
                        Ok(content) => {
                            all_resources.extend(content);
                        }
                        Err(tool_error) => {
                            errors.push(tool_error);
                        }
                    }
                }

                if !errors.is_empty() {
                    tracing::error!(
                        errors = ?errors
                            .into_iter()
                            .map(|e| format!("{:?}", e))
                            .collect::<Vec<_>>(),
                        "errors from listing resources"
                    );
                }

                Ok(all_resources)
            }
        }
    }

    pub async fn dispatch_tool_call(
        &self,
        session_id: &str,
        tool_call: CallToolRequestParams,
        cancellation_token: CancellationToken,
    ) -> Result<ToolCallResult> {
        // Some models strip the tool prefix, so auto-add it for known code_execution tools
        let tool_name_str = tool_call.name.to_string();
        let prefixed_name = if !tool_name_str.contains("__") {
            let code_exec_tools = ["execute_code", "read_module", "search_modules"];
            if code_exec_tools.contains(&tool_name_str.as_str())
                && self.extensions.lock().await.contains_key("code_execution")
            {
                format!("code_execution__{}", tool_name_str)
            } else {
                tool_name_str
            }
        } else {
            tool_name_str
        };

        // Dispatch tool call based on the prefix naming convention
        let (client_name, client) =
            self.get_client_for_tool(&prefixed_name)
                .await
                .ok_or_else(|| {
                    ErrorData::new(
                        ErrorCode::RESOURCE_NOT_FOUND,
                        format!("Tool '{}' not found", tool_call.name),
                        None,
                    )
                })?;

        let tool_name = prefixed_name
            .strip_prefix(client_name.as_str())
            .and_then(|s| s.strip_prefix("__"))
            .ok_or_else(|| {
                ErrorData::new(
                    ErrorCode::RESOURCE_NOT_FOUND,
                    format!("Invalid tool name format: '{}'", tool_call.name),
                    None,
                )
            })?
            .to_string();

        if let Some(extension) = self.extensions.lock().await.get(&client_name) {
            if !extension.config.is_tool_available(&tool_name) {
                return Err(ErrorData::new(
                    ErrorCode::RESOURCE_NOT_FOUND,
                    format!(
                        "Tool '{}' is not available for extension '{}'",
                        tool_name, client_name
                    ),
                    None,
                )
                .into());
            }
        }

        // BR-23: central secret-redaction boundary. The `.biorouterignore`/secret
        // deny set used to live only inside the Developer MCP server, so any other
        // extension (compute, files, a third-party MCP, a different shell wrapper)
        // could read a `.env`/private-key/cloud-credential file that the deny set
        // forbids. Enforce it here — the single choke point every tool call flows
        // through — so no extension can bypass it. The scan is conservative: it
        // only blocks when an argument names a secret file that actually exists on
        // disk (see `SecretGuard::find_denied_path`).
        if let Some(args) = tool_call.arguments.as_ref() {
            let cwd = self.resolve_working_dir().await;
            let secret_guard_phase =
                crate::agents::phase_timing::Phase::start("mcp.secret_guard_for_dir");
            // 6.2d: memoised per resolved cwd. Invalidated on the exact bytes
            // of every `.biorouterignore` that backs the guard, so an edit is
            // honoured on the very next dispatch (see `cached_for_dir`).
            let guard = biorouter_mcp::secret_guard::SecretGuard::cached_for_dir(&cwd);
            drop(secret_guard_phase);
            if let Some(denied) = guard.find_denied_path(args) {
                return Err(ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    format!(
                        "Access to '{}' is blocked: it matches a secret/credential deny pattern \
                         (.env, private key, or cloud credentials). Add a negation to \
                         .biorouterignore to allow it.",
                        denied
                    ),
                    None,
                )
                .into());
            }
        }

        let arguments = tool_call.arguments.clone();
        let client = client.clone();
        // BR-54: on a shared (pooled) client this mints a per-dispatch progress
        // token and returns a receiver that gets ONLY this dispatch's
        // notifications; on an unpooled client it is the legacy broadcast
        // subscription with no token.
        // H6: there is no client-wide lock to wait on any more, so the former
        // `mcp.register_dispatch_wait` span is gone. `register_dispatch()` is
        // still real work (it mints a progress token and installs a routing
        // entry) and keeps its own span.
        let register_call = crate::agents::phase_timing::Phase::start("mcp.register_dispatch");
        let (progress_token, notifications_receiver) = client.register_dispatch().await;
        drop(register_call);
        let session_id = session_id.to_string();

        let fut = async move {
            tracing::debug!(
                "dispatch_tool_call fut: calling client.call_tool tool={} session_id={}",
                tool_name,
                session_id
            );
            // H6 (fixed): this used to take a client-wide mutex and hold it
            // across the whole `call_tool` await, so two tool calls on the SAME
            // extension ran back-to-back — `sum(durations)` instead of
            // `max(durations)`. `call_tool` takes `&self` and implementations
            // are internally synchronized, so the guard (and its
            // `mcp.client_lock_wait` span) is gone and calls now overlap.
            let _call_phase = crate::agents::phase_timing::Phase::start("mcp.call_tool");
            let mut meta = McpMeta::new(&session_id);
            if let Some(token) = progress_token {
                meta = meta.with_progress_token(token);
            }
            client
                .call_tool(&tool_name, arguments, meta, cancellation_token)
                .await
                .map_err(|e| match e {
                    ServiceError::McpError(error_data) => error_data,
                    _ => {
                        ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), e.maybe_to_value())
                    }
                })
        };

        Ok(ToolCallResult {
            result: Box::new(fut.boxed()),
            notification_stream: Some(Box::new(ReceiverStream::new(notifications_receiver))),
        })
    }

    pub async fn list_prompts_from_extension(
        &self,
        extension_name: &str,
        cancellation_token: CancellationToken,
    ) -> Result<Vec<Prompt>, ErrorData> {
        let client = self
            .get_server_client(extension_name)
            .await
            .ok_or_else(|| {
                ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    format!("Extension {} is not valid", extension_name),
                    None,
                )
            })?;

        let client_guard = &*client;
        client_guard
            .list_prompts(None, cancellation_token)
            .await
            .map_err(|e| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Unable to list prompts for {}, {:?}", extension_name, e),
                    None,
                )
            })
            .map(|lp| lp.prompts)
    }

    pub async fn list_prompts(
        &self,
        cancellation_token: CancellationToken,
    ) -> Result<HashMap<String, Vec<Prompt>>, ErrorData> {
        let mut futures = FuturesUnordered::new();

        let names: Vec<_> = self.extensions.lock().await.keys().cloned().collect();
        for extension_name in names {
            let token = cancellation_token.clone();
            futures.push(async move {
                (
                    extension_name.clone(),
                    self.list_prompts_from_extension(extension_name.as_str(), token)
                        .await,
                )
            });
        }

        let mut all_prompts = HashMap::new();
        let mut errors = Vec::new();

        // Process results as they complete
        while let Some(result) = futures.next().await {
            let (name, prompts) = result;
            match prompts {
                Ok(content) => {
                    all_prompts.insert(name.to_string(), content);
                }
                Err(tool_error) => {
                    errors.push(tool_error);
                }
            }
        }

        if !errors.is_empty() {
            tracing::debug!(
                errors = ?errors
                    .into_iter()
                    .map(|e| format!("{:?}", e))
                    .collect::<Vec<_>>(),
                "errors from listing prompts"
            );
        }

        Ok(all_prompts)
    }

    pub async fn get_prompt(
        &self,
        extension_name: &str,
        name: &str,
        arguments: Value,
        cancellation_token: CancellationToken,
    ) -> Result<GetPromptResult> {
        let client = self
            .get_server_client(extension_name)
            .await
            .ok_or_else(|| anyhow::anyhow!("Extension {} not found", extension_name))?;

        let client_guard = &*client;
        client_guard
            .get_prompt(name, arguments, cancellation_token)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get prompt: {}", e))
    }

    pub async fn search_available_extensions(&self) -> Result<Vec<Content>, ErrorData> {
        let mut output_parts = vec![];

        // First get disabled extensions from current config; only entries the
        // operator actually persisted with `enabled: false` get the
        // do-not-enable label (#42) — injected default-off platform entries
        // stay listed as plainly enableable.
        let disabled_extensions = config_disabled_extension_lines(
            &get_all_extensions(),
            &crate::config::persisted_extension_names(),
        );

        // Get currently enabled extensions that can be disabled
        let enabled_extensions: Vec<String> =
            self.extensions.lock().await.keys().cloned().collect();

        // Build output string
        if !disabled_extensions.is_empty() {
            output_parts.push(format!(
                "Extensions disabled in the config:\n{}\n",
                disabled_extensions.join("\n")
            ));
        } else {
            output_parts.push("No extensions available to enable.\n".to_string());
        }

        if !enabled_extensions.is_empty() {
            output_parts.push(format!(
                "\n\nExtensions available to disable:\n{}\n",
                enabled_extensions
                    .iter()
                    .map(|name| format!("- {}", name))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        } else {
            output_parts.push("No extensions that can be disabled.\n".to_string());
        }

        Ok(vec![Content::text(output_parts.join("\n"))])
    }

    async fn get_server_client(&self, name: impl Into<String>) -> Option<McpClientBox> {
        self.extensions
            .lock()
            .await
            .get(&name.into())
            .map(|ext| ext.get_client())
    }

    pub async fn collect_moim(
        &self,
        session_id: &str,
        working_dir: &std::path::Path,
    ) -> Option<String> {
        // Use minute-level granularity to prevent conversation changes every second
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:00").to_string();
        let mut content = format!(
            "{MOIM_OPEN_TAG}\nIt is currently {}\nWorking directory: {}\n",
            timestamp,
            working_dir.display()
        );

        // BR-1: give the model a bounded, gitignore-aware map of the workspace so
        // it doesn't rediscover project structure from scratch every session. The
        // map is cached and token-capped inside `workspace_summary`.
        if let Some(map) = crate::agents::workspace_summary::workspace_summary(working_dir) {
            content.push('\n');
            content.push_str(&map);
            content.push('\n');
        }

        let platform_clients: Vec<(String, McpClientBox)> = {
            let extensions = self.extensions.lock().await;
            extensions
                .iter()
                .filter_map(|(name, extension)| {
                    if let ExtensionConfig::Platform { .. } = &extension.config {
                        Some((name.clone(), extension.get_client()))
                    } else {
                        None
                    }
                })
                .collect()
        };

        for (name, client) in platform_clients {
            let client_guard = &*client;
            if let Some(moim_content) = client_guard.get_moim(session_id).await {
                tracing::debug!("MOIM content from {}: {} chars", name, moim_content.len());
                content.push('\n');
                content.push_str(&moim_content);
            }
        }

        content.push('\n');
        content.push_str(MOIM_CLOSE_TAG);

        Some(content)
    }
}

/// Label appended to every **operator**-disabled entry in
/// `search_available_extensions` output (#42): these extensions were turned
/// off by the operator, so the model must not treat the listing as an
/// invitation to enable them on its own (`manage_extensions` refuses anyway —
/// see `extension_manager_extension::check_enable_allowed`).
pub(crate) const CONFIG_DISABLED_LABEL: &str = "(disabled by user — do not enable without asking)";

/// One listing line per config-disabled extension. Only entries the operator
/// actually wrote into the config file (`persisted`, keyed by
/// `config.name()`) carry [`CONFIG_DISABLED_LABEL`] — an absent platform
/// extension injected with a default-off entry (e.g. `chatrecall`) is still
/// listed as available to enable, but unlabeled, because no operator ever
/// disabled it. Pure so the labeling is unit-testable without a global
/// config.
fn config_disabled_extension_lines(
    entries: &[crate::config::ExtensionEntry],
    persisted: &std::collections::HashSet<String>,
) -> Vec<String> {
    entries
        .iter()
        .filter(|extension| !extension.enabled)
        .map(|extension| {
            let config = &extension.config;
            let description = match config {
                ExtensionConfig::Builtin {
                    description,
                    display_name,
                    ..
                } => {
                    if description.is_empty() {
                        display_name.as_deref().unwrap_or("Built-in extension")
                    } else {
                        description
                    }
                }
                ExtensionConfig::Sse { .. } => "SSE extension (unsupported)",
                ExtensionConfig::Platform { description, .. }
                | ExtensionConfig::StreamableHttp { description, .. }
                | ExtensionConfig::Stdio { description, .. }
                | ExtensionConfig::Frontend { description, .. }
                | ExtensionConfig::InlinePython { description, .. } => description,
            };
            if persisted.contains(&config.name()) {
                format!(
                    "- {} - {} {}",
                    config.name(),
                    description,
                    CONFIG_DISABLED_LABEL
                )
            } else {
                format!("- {} - {}", config.name(), description)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::CallToolResult;
    use rmcp::model::{InitializeResult, JsonObject};
    use rmcp::{object, ServiceError as Error};

    use rmcp::model::ListPromptsResult;
    use rmcp::model::ListResourcesResult;
    use rmcp::model::ListToolsResult;
    use rmcp::model::ReadResourceResult;
    use rmcp::model::ServerNotification;

    use tokio::sync::mpsc;

    impl ExtensionManager {
        async fn add_mock_extension(&self, name: String, client: McpClientBox) {
            self.add_mock_extension_with_tools(name, client, vec![])
                .await;
        }

        async fn add_mock_extension_with_tools(
            &self,
            name: String,
            client: McpClientBox,
            available_tools: Vec<String>,
        ) {
            let sanitized_name = normalize(&name);
            let config = ExtensionConfig::Builtin {
                name: name.clone(),
                display_name: Some(name.clone()),
                description: "built-in".to_string(),
                timeout: None,
                bundled: None,
                available_tools,
            };
            let extension = Extension::new(config, client, None, None);
            self.extensions
                .lock()
                .await
                .insert(sanitized_name, extension);
            self.invalidate_tools_cache_and_bump_version().await;
        }
    }

    struct MockClient {}

    #[async_trait::async_trait]
    impl McpClientTrait for MockClient {
        fn get_info(&self) -> Option<&InitializeResult> {
            None
        }

        async fn list_resources(
            &self,
            _next_cursor: Option<String>,
            _cancellation_token: CancellationToken,
        ) -> Result<ListResourcesResult, Error> {
            Err(Error::TransportClosed)
        }

        async fn read_resource(
            &self,
            _uri: &str,
            _cancellation_token: CancellationToken,
        ) -> Result<ReadResourceResult, Error> {
            Err(Error::TransportClosed)
        }

        async fn list_tools(
            &self,
            _next_cursor: Option<String>,
            _cancellation_token: CancellationToken,
        ) -> Result<ListToolsResult, Error> {
            use serde_json::json;
            use std::sync::Arc;
            Ok(ListToolsResult {
                tools: vec![
                    Tool::new(
                        "tool".to_string(),
                        "A basic tool".to_string(),
                        Arc::new(json!({}).as_object().unwrap().clone()),
                    ),
                    Tool::new(
                        "available_tool".to_string(),
                        "An available tool".to_string(),
                        Arc::new(json!({}).as_object().unwrap().clone()),
                    ),
                    Tool::new(
                        "hidden_tool".to_string(),
                        "hidden tool".to_string(),
                        Arc::new(json!({}).as_object().unwrap().clone()),
                    ),
                ],
                next_cursor: None,
                meta: None,
            })
        }

        async fn call_tool(
            &self,
            name: &str,
            _arguments: Option<JsonObject>,
            _meta: McpMeta,
            _cancellation_token: CancellationToken,
        ) -> Result<CallToolResult, Error> {
            match name {
                "tool" | "test__tool" | "available_tool" | "hidden_tool" => Ok(CallToolResult {
                    content: vec![],
                    is_error: None,
                    structured_content: None,
                    meta: None,
                }),
                _ => Err(Error::TransportClosed),
            }
        }

        async fn list_prompts(
            &self,
            _next_cursor: Option<String>,
            _cancellation_token: CancellationToken,
        ) -> Result<ListPromptsResult, Error> {
            Err(Error::TransportClosed)
        }

        async fn get_prompt(
            &self,
            _name: &str,
            _arguments: Value,
            _cancellation_token: CancellationToken,
        ) -> Result<GetPromptResult, Error> {
            Err(Error::TransportClosed)
        }

        async fn subscribe(&self) -> mpsc::Receiver<ServerNotification> {
            mpsc::channel(1).1
        }
    }

    /// H6 regression guard. A client whose `call_tool` sleeps, so that N
    /// concurrent dispatches against the SAME extension take `max(durations)`
    /// when they truly run in parallel and `sum(durations)` when something
    /// serializes them.
    struct SlowMockClient {
        delay: std::time::Duration,
    }

    #[async_trait::async_trait]
    impl McpClientTrait for SlowMockClient {
        fn get_info(&self) -> Option<&InitializeResult> {
            None
        }

        async fn list_resources(
            &self,
            _next_cursor: Option<String>,
            _cancellation_token: CancellationToken,
        ) -> Result<ListResourcesResult, Error> {
            Err(Error::TransportClosed)
        }

        async fn read_resource(
            &self,
            _uri: &str,
            _cancellation_token: CancellationToken,
        ) -> Result<ReadResourceResult, Error> {
            Err(Error::TransportClosed)
        }

        async fn list_tools(
            &self,
            _next_cursor: Option<String>,
            _cancellation_token: CancellationToken,
        ) -> Result<ListToolsResult, Error> {
            Ok(ListToolsResult {
                tools: vec![],
                next_cursor: None,
                meta: None,
            })
        }

        async fn call_tool(
            &self,
            _name: &str,
            _arguments: Option<JsonObject>,
            _meta: McpMeta,
            _cancellation_token: CancellationToken,
        ) -> Result<CallToolResult, Error> {
            tokio::time::sleep(self.delay).await;
            Ok(CallToolResult {
                content: vec![],
                is_error: None,
                structured_content: None,
                meta: None,
            })
        }

        async fn list_prompts(
            &self,
            _next_cursor: Option<String>,
            _cancellation_token: CancellationToken,
        ) -> Result<ListPromptsResult, Error> {
            Err(Error::TransportClosed)
        }

        async fn get_prompt(
            &self,
            _name: &str,
            _arguments: Value,
            _cancellation_token: CancellationToken,
        ) -> Result<GetPromptResult, Error> {
            Err(Error::TransportClosed)
        }

        async fn subscribe(&self) -> mpsc::Receiver<ServerNotification> {
            mpsc::channel(1).1
        }
    }

    /// H6: three tool calls on ONE extension client must overlap.
    ///
    /// Before the redundant `McpClientBox` mutex was removed, `dispatch_tool_call`
    /// held the client guard across the whole `call_tool` await, turning
    /// `max(400ms, 400ms, 400ms)` into `sum(...)` = ~1200ms. This test fails
    /// (~1.2s elapsed) with the mutex in place and passes (~0.4s) without it.
    #[tokio::test]
    async fn h6_parallel_same_extension() {
        let temp_dir = tempfile::tempdir().unwrap();
        let extension_manager =
            ExtensionManager::new_without_provider(temp_dir.path().to_path_buf());

        extension_manager
            .add_mock_extension(
                "slow".to_string(),
                Arc::new(SlowMockClient {
                    delay: std::time::Duration::from_millis(400),
                }),
            )
            .await;

        // Dispatch all three first: `register_dispatch` runs on the dispatch
        // loop, the actual `call_tool` runs inside the returned future.
        let mut futures = Vec::new();
        for _ in 0..3 {
            let tool_call = CallToolRequestParams {
                task: None,
                name: "slow__tool".to_string().into(),
                arguments: Some(object!({})),
                meta: None,
            };
            let dispatched = extension_manager
                .dispatch_tool_call("test-session-id", tool_call, CancellationToken::default())
                .await
                .expect("dispatch should succeed");
            futures.push(dispatched.result);
        }

        let start = std::time::Instant::now();
        let results = futures::future::join_all(futures).await;
        let elapsed = start.elapsed();

        for result in results {
            assert!(result.is_ok(), "each concurrent call should succeed");
        }

        assert!(
            elapsed < std::time::Duration::from_millis(700),
            "3 concurrent calls on one extension took {elapsed:?}; expected ~400ms. \
             They are being serialized on a client-wide mutex (H6)."
        );
    }

    #[tokio::test]
    async fn test_get_client_for_tool() {
        let temp_dir = tempfile::tempdir().unwrap();
        let extension_manager =
            ExtensionManager::new_without_provider(temp_dir.path().to_path_buf());

        // Add some mock clients using the helper method
        extension_manager
            .add_mock_extension("test_client".to_string(), Arc::new(MockClient {}))
            .await;

        extension_manager
            .add_mock_extension("__client".to_string(), Arc::new(MockClient {}))
            .await;

        extension_manager
            .add_mock_extension("__cli__ent__".to_string(), Arc::new(MockClient {}))
            .await;

        extension_manager
            .add_mock_extension("client 🚀".to_string(), Arc::new(MockClient {}))
            .await;

        // Test basic case
        assert!(extension_manager
            .get_client_for_tool("test_client__tool")
            .await
            .is_some());

        // Test leading underscores
        assert!(extension_manager
            .get_client_for_tool("__client__tool")
            .await
            .is_some());

        // Test multiple underscores in client name, and ending with __
        assert!(extension_manager
            .get_client_for_tool("__cli__ent____tool")
            .await
            .is_some());

        // Test unicode in tool name, "client 🚀" should become "client_"
        assert!(extension_manager
            .get_client_for_tool("client___tool")
            .await
            .is_some());
    }

    /// BRSDK INTEGRATED end-to-end: a per-app `DataSqlServer` injected via the
    /// `add_inprocess_server` seam is discoverable as an MCP tool and dispatches
    /// real read-only SQL through the actual MCP transport — exercising the full
    /// path (manifest source → per-app server → in-process MCP → tool discovery
    /// → dispatch → real rows), plus end-to-end mutation rejection.
    #[tokio::test]
    async fn brsdk_inprocess_datasql_end_to_end() {
        use biorouter_mcp::datasql::server::DataSqlServer;
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("cohort.db");
        {
            let opts = SqliteConnectOptions::new()
                .filename(&db_path)
                .create_if_missing(true);
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(opts)
                .await
                .unwrap();
            sqlx::query("CREATE TABLE genes (symbol TEXT, chrom TEXT)")
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query("INSERT INTO genes VALUES ('CFTR','7'), ('TP53','17')")
                .execute(&pool)
                .await
                .unwrap();
            pool.close().await;
        }

        let em = ExtensionManager::new_without_provider(dir.path().to_path_buf());
        let mut sources = std::collections::HashMap::new();
        sources.insert("cohort".to_string(), db_path);
        em.add_inprocess_server("datasql", DataSqlServer::new(sources))
            .await
            .expect("inject per-app server");

        // (1) The tool is discoverable over the in-process MCP transport.
        let tools = em.get_prefixed_tools(None).await.unwrap();
        assert!(
            tools.iter().any(|t| t.name.contains("data_query")),
            "data_query not discovered; tools = {:?}",
            tools.iter().map(|t| t.name.to_string()).collect::<Vec<_>>()
        );

        // (2) Dispatch a real read-only query and read the rows back.
        let call = CallToolRequestParams {
            task: None,
            name: "datasql__data_query".to_string().into(),
            arguments: Some(
                serde_json::json!({"source":"cohort","sql":"SELECT symbol FROM genes WHERE chrom='7'"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            meta: None,
        };
        let dispatched = em
            .dispatch_tool_call("test-session", call, CancellationToken::default())
            .await
            .expect("dispatch ok");
        let output = dispatched.result.await.expect("tool result ok");
        let text = serde_json::to_string(&output).unwrap();
        assert!(
            text.contains("CFTR"),
            "expected query rows in output: {text}"
        );
        assert!(!text.contains("TP53"), "WHERE filter must apply: {text}");

        // (2b) The in-process server's synthetic config is EXCLUDED from
        // get_extension_configs — so it's never persisted/replayed/propagated as
        // an un-spawnable "datasql" builtin.
        let configs = em.get_extension_configs().await;
        assert!(
            !configs.iter().any(|c| c.name() == "datasql"),
            "in-process datasql config must be excluded from exported configs"
        );

        // (2c) Re-injection is idempotent (configure_agent re-runs per connect).
        let before = em.get_prefixed_tools(None).await.unwrap().len();
        em.add_inprocess_server(
            "datasql",
            DataSqlServer::new(std::collections::HashMap::new()),
        )
        .await
        .expect("idempotent re-inject");
        let after = em.get_prefixed_tools(None).await.unwrap().len();
        assert_eq!(before, after, "re-injecting an existing server is a no-op");

        // (3) A mutation is rejected end-to-end (whichever way the error surfaces).
        let bad = CallToolRequestParams {
            task: None,
            name: "datasql__data_query".to_string().into(),
            arguments: Some(
                serde_json::json!({"source":"cohort","sql":"DROP TABLE genes"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            meta: None,
        };
        let rejected = match em
            .dispatch_tool_call("test-session", bad, CancellationToken::default())
            .await
        {
            Err(_) => true,
            Ok(tcr) => match tcr.result.await {
                Err(_) => true,
                Ok(r) => {
                    r.is_error == Some(true)
                        || serde_json::to_string(&r)
                            .unwrap()
                            .to_lowercase()
                            .contains("read-only")
                }
            },
        };
        assert!(rejected, "mutation must be rejected end-to-end");
    }

    #[tokio::test]
    async fn test_dispatch_tool_call() {
        // test that dispatch_tool_call parses out the sanitized name correctly, and extracts
        // tool_names
        let temp_dir = tempfile::tempdir().unwrap();
        let extension_manager =
            ExtensionManager::new_without_provider(temp_dir.path().to_path_buf());

        // Add some mock clients using the helper method
        extension_manager
            .add_mock_extension("test_client".to_string(), Arc::new(MockClient {}))
            .await;

        extension_manager
            .add_mock_extension("__cli__ent__".to_string(), Arc::new(MockClient {}))
            .await;

        extension_manager
            .add_mock_extension("client 🚀".to_string(), Arc::new(MockClient {}))
            .await;

        // verify a normal tool call
        let tool_call = CallToolRequestParams {
            task: None,
            name: "test_client__tool".to_string().into(),
            arguments: Some(object!({})),
            meta: None,
        };

        let result = extension_manager
            .dispatch_tool_call("test-session-id", tool_call, CancellationToken::default())
            .await;
        assert!(result.is_ok());

        let tool_call = CallToolRequestParams {
            task: None,
            name: "test_client__test__tool".to_string().into(),
            arguments: Some(object!({})),
            meta: None,
        };

        let result = extension_manager
            .dispatch_tool_call("test-session-id", tool_call, CancellationToken::default())
            .await;
        assert!(result.is_ok());

        // verify a multiple underscores dispatch
        let tool_call = CallToolRequestParams {
            task: None,
            name: "__cli__ent____tool".to_string().into(),
            arguments: Some(object!({})),
            meta: None,
        };

        let result = extension_manager
            .dispatch_tool_call("test-session-id", tool_call, CancellationToken::default())
            .await;
        assert!(result.is_ok());

        // Test unicode in tool name, "client 🚀" should become "client_"
        let tool_call = CallToolRequestParams {
            task: None,
            name: "client___tool".to_string().into(),
            arguments: Some(object!({})),
            meta: None,
        };

        let result = extension_manager
            .dispatch_tool_call("test-session-id", tool_call, CancellationToken::default())
            .await;
        assert!(result.is_ok());

        let tool_call = CallToolRequestParams {
            task: None,
            name: "client___test__tool".to_string().into(),
            arguments: Some(object!({})),
            meta: None,
        };

        let result = extension_manager
            .dispatch_tool_call("test-session-id", tool_call, CancellationToken::default())
            .await;
        assert!(result.is_ok());

        // this should error out, specifically for an ToolError::ExecutionError
        let invalid_tool_call = CallToolRequestParams {
            task: None,
            name: "client___tools".to_string().into(),
            arguments: Some(object!({})),
            meta: None,
        };

        let result = extension_manager
            .dispatch_tool_call(
                "test-session-id",
                invalid_tool_call,
                CancellationToken::default(),
            )
            .await
            .unwrap()
            .result
            .await;
        assert!(matches!(
            result,
            Err(ErrorData {
                code: ErrorCode::INTERNAL_ERROR,
                ..
            })
        ));

        // this should error out, specifically with an ToolError::NotFound
        // this client doesn't exist
        let invalid_tool_call = CallToolRequestParams {
            task: None,
            name: "_client__tools".to_string().into(),
            arguments: Some(object!({})),
            meta: None,
        };

        let result = extension_manager
            .dispatch_tool_call(
                "test-session-id",
                invalid_tool_call,
                CancellationToken::default(),
            )
            .await;
        if let Err(err) = result {
            let tool_err = err.downcast_ref::<ErrorData>().expect("Expected ErrorData");
            assert_eq!(tool_err.code, ErrorCode::RESOURCE_NOT_FOUND);
        } else {
            panic!("Expected ErrorData with ErrorCode::RESOURCE_NOT_FOUND");
        }
    }

    // BR-23: the central secret-redaction boundary must block a tool call that
    // references an existing secret file, no matter which extension owns the tool.
    #[tokio::test]
    async fn test_dispatch_blocks_secret_file_access() {
        let temp_dir = tempfile::tempdir().unwrap();
        std::fs::write(temp_dir.path().join(".env"), "SECRET=1").unwrap();

        let extension_manager =
            ExtensionManager::new_without_provider(temp_dir.path().to_path_buf());
        extension_manager
            .set_working_dir(temp_dir.path().to_path_buf())
            .await;
        extension_manager
            .add_mock_extension("test_client".to_string(), Arc::new(MockClient {}))
            .await;

        // A tool from an arbitrary extension that reads the existing .env is denied
        // at dispatch, before the tool ever runs.
        let secret_call = CallToolRequestParams {
            task: None,
            name: "test_client__tool".to_string().into(),
            arguments: Some(object!({"path": ".env"})),
            meta: None,
        };
        let result = extension_manager
            .dispatch_tool_call("test-session-id", secret_call, CancellationToken::default())
            .await;
        match result {
            Err(err) => {
                let tool_err = err.downcast_ref::<ErrorData>().expect("Expected ErrorData");
                assert_eq!(tool_err.code, ErrorCode::INVALID_PARAMS);
            }
            Ok(_) => panic!("expected the secret-file access to be blocked at dispatch"),
        }

        // A benign, non-existent path is not blocked.
        let benign_call = CallToolRequestParams {
            task: None,
            name: "test_client__tool".to_string().into(),
            arguments: Some(object!({"path": "notes.txt"})),
            meta: None,
        };
        let result = extension_manager
            .dispatch_tool_call("test-session-id", benign_call, CancellationToken::default())
            .await;
        assert!(result.is_ok(), "benign path must not be blocked");
    }

    #[tokio::test]
    async fn test_tool_availability_filtering() {
        let temp_dir = tempfile::tempdir().unwrap();
        let extension_manager =
            ExtensionManager::new_without_provider(temp_dir.path().to_path_buf());

        // Only "available_tool" should be available to the LLM
        let available_tools = vec!["available_tool".to_string()];

        extension_manager
            .add_mock_extension_with_tools(
                "test_extension".to_string(),
                Arc::new(MockClient {}),
                available_tools,
            )
            .await;

        let tools = extension_manager.get_prefixed_tools(None).await.unwrap();

        let tool_names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
        assert!(!tool_names.iter().any(|name| name == "test_extension__tool")); // Default unavailable
        assert!(tool_names
            .iter()
            .any(|name| name == "test_extension__available_tool"));
        assert!(!tool_names
            .iter()
            .any(|name| name == "test_extension__hidden_tool"));
        assert!(tool_names.len() == 1);
    }

    #[tokio::test]
    async fn test_tool_availability_defaults_to_available() {
        let temp_dir = tempfile::tempdir().unwrap();
        let extension_manager =
            ExtensionManager::new_without_provider(temp_dir.path().to_path_buf());

        extension_manager
            .add_mock_extension_with_tools(
                "test_extension".to_string(),
                Arc::new(MockClient {}),
                vec![], // Empty available_tools means all tools are available by default
            )
            .await;

        let tools = extension_manager.get_prefixed_tools(None).await.unwrap();

        let tool_names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
        assert!(tool_names.iter().any(|name| name == "test_extension__tool"));
        assert!(tool_names
            .iter()
            .any(|name| name == "test_extension__available_tool"));
        assert!(tool_names
            .iter()
            .any(|name| name == "test_extension__hidden_tool"));
        assert!(tool_names.len() == 3);
    }

    #[tokio::test]
    async fn test_dispatch_unavailable_tool_returns_error() {
        let temp_dir = tempfile::tempdir().unwrap();
        let extension_manager =
            ExtensionManager::new_without_provider(temp_dir.path().to_path_buf());

        let available_tools = vec!["available_tool".to_string()];

        extension_manager
            .add_mock_extension_with_tools(
                "test_extension".to_string(),
                Arc::new(MockClient {}),
                available_tools,
            )
            .await;

        // Try to call an unavailable tool
        let unavailable_tool_call = CallToolRequestParams {
            task: None,
            name: "test_extension__tool".to_string().into(),
            arguments: Some(object!({})),
            meta: None,
        };

        let result = extension_manager
            .dispatch_tool_call(
                "test-session-id",
                unavailable_tool_call,
                CancellationToken::default(),
            )
            .await;

        // Should return RESOURCE_NOT_FOUND error
        if let Err(err) = result {
            let tool_err = err.downcast_ref::<ErrorData>().expect("Expected ErrorData");
            assert_eq!(tool_err.code, ErrorCode::RESOURCE_NOT_FOUND);
            assert!(tool_err.message.contains("is not available"));
        } else {
            panic!("Expected ErrorData with ErrorCode::RESOURCE_NOT_FOUND");
        }

        // Try to call an available tool - should succeed
        let available_tool_call = CallToolRequestParams {
            task: None,
            name: "test_extension__available_tool".to_string().into(),
            arguments: Some(object!({})),
            meta: None,
        };

        let result = extension_manager
            .dispatch_tool_call(
                "test-session-id",
                available_tool_call,
                CancellationToken::default(),
            )
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_streamable_http_header_env_substitution() {
        let mut env_map = HashMap::new();
        env_map.insert("AUTH_TOKEN".to_string(), "secret123".to_string());
        env_map.insert("API_KEY".to_string(), "key456".to_string());

        // Test ${VAR} syntax
        let result = substitute_env_vars("Bearer ${ AUTH_TOKEN }", &env_map);
        assert_eq!(result, "Bearer secret123");

        // Test ${VAR} syntax without spaces
        let result = substitute_env_vars("Bearer ${AUTH_TOKEN}", &env_map);
        assert_eq!(result, "Bearer secret123");

        // Test $VAR syntax
        let result = substitute_env_vars("Bearer $AUTH_TOKEN", &env_map);
        assert_eq!(result, "Bearer secret123");

        // Test multiple substitutions
        let result = substitute_env_vars("Key: $API_KEY, Token: ${AUTH_TOKEN}", &env_map);
        assert_eq!(result, "Key: key456, Token: secret123");

        // Test no substitution when variable doesn't exist
        let result = substitute_env_vars("Bearer ${UNKNOWN_VAR}", &env_map);
        assert_eq!(result, "Bearer ${UNKNOWN_VAR}");

        // Test mixed content
        let result = substitute_env_vars(
            "Authorization: Bearer ${AUTH_TOKEN} and API ${API_KEY}",
            &env_map,
        );
        assert_eq!(result, "Authorization: Bearer secret123 and API key456");
    }

    mod generate_extension_name_tests {
        use super::*;
        use rmcp::model::Implementation;
        use test_case::test_case;

        fn make_info(name: &str) -> ServerInfo {
            ServerInfo {
                server_info: Implementation {
                    name: name.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }

        #[test_case(Some("kiwi-mcp-server"), None, "^kiwi-mcp-server$" ; "already normalized server name")]
        #[test_case(Some("Context7"), None, "^context7$" ; "mixed case normalized")]
        #[test_case(Some("@huggingface/mcp-services"), None, "^_huggingface_mcp-services$" ; "special chars normalized")]
        #[test_case(None, None, "^unnamed$" ; "no server info falls back")]
        #[test_case(Some(""), None, "^unnamed$" ; "empty server name falls back")]
        #[test_case(Some("github-mcp-server"), Some("github-mcp-server"), r"^github-mcp-server_[A-Za-z0-9]{6}$" ; "duplicate adds suffix")]
        fn test_generate_name(server_name: Option<&str>, collision: Option<&str>, expected: &str) {
            let info = server_name.map(make_info);
            let result = generate_extension_name(info.as_ref(), |n| collision == Some(n));
            let re = regex::Regex::new(expected).unwrap();
            assert!(re.is_match(&result));
        }
    }

    #[tokio::test]
    async fn test_collect_moim_uses_minute_granularity() {
        let temp_dir = tempfile::tempdir().unwrap();
        let em = ExtensionManager::new_without_provider(temp_dir.path().to_path_buf());
        let working_dir = std::path::Path::new("/tmp");

        if let Some(moim) = em.collect_moim("test-session-id", working_dir).await {
            // Timestamp should end with :00 (seconds fixed to 00)
            assert!(
                moim.contains(":00\n"),
                "Timestamp should use minute granularity"
            );
        }
    }

    #[tokio::test]
    async fn test_tools_cache_invalidated_on_add_extension() {
        let temp_dir = tempfile::tempdir().unwrap();
        let extension_manager =
            ExtensionManager::new_without_provider(temp_dir.path().to_path_buf());

        extension_manager
            .add_mock_extension("ext_a".to_string(), Arc::new(MockClient {}))
            .await;

        let tools_after_first = extension_manager.get_prefixed_tools(None).await.unwrap();
        let tool_names: Vec<String> = tools_after_first
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        assert!(tool_names.iter().any(|n| n.starts_with("ext_a__")));
        assert!(!tool_names.iter().any(|n| n.starts_with("ext_b__")));

        extension_manager
            .add_mock_extension("ext_b".to_string(), Arc::new(MockClient {}))
            .await;

        let tools_after_second = extension_manager.get_prefixed_tools(None).await.unwrap();
        let tool_names: Vec<String> = tools_after_second
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        assert!(tool_names.iter().any(|n| n.starts_with("ext_a__")));
        assert!(tool_names.iter().any(|n| n.starts_with("ext_b__")));
    }

    #[tokio::test]
    async fn test_tools_cache_invalidated_on_remove_extension() {
        let temp_dir = tempfile::tempdir().unwrap();
        let extension_manager =
            ExtensionManager::new_without_provider(temp_dir.path().to_path_buf());

        extension_manager
            .add_mock_extension("ext_a".to_string(), Arc::new(MockClient {}))
            .await;
        extension_manager
            .add_mock_extension("ext_b".to_string(), Arc::new(MockClient {}))
            .await;

        let tools_before = extension_manager.get_prefixed_tools(None).await.unwrap();
        let tool_names: Vec<String> = tools_before.iter().map(|t| t.name.to_string()).collect();
        assert!(tool_names.iter().any(|n| n.starts_with("ext_a__")));
        assert!(tool_names.iter().any(|n| n.starts_with("ext_b__")));

        extension_manager.remove_extension("ext_b").await.unwrap();

        let tools_after = extension_manager.get_prefixed_tools(None).await.unwrap();
        let tool_names: Vec<String> = tools_after.iter().map(|t| t.name.to_string()).collect();
        assert!(tool_names.iter().any(|n| n.starts_with("ext_a__")));
        assert!(!tool_names.iter().any(|n| n.starts_with("ext_b__")));
    }

    #[tokio::test]
    async fn test_get_prefixed_tools_excluding() {
        let temp_dir = tempfile::tempdir().unwrap();
        let extension_manager =
            ExtensionManager::new_without_provider(temp_dir.path().to_path_buf());

        extension_manager
            .add_mock_extension("ext_a".to_string(), Arc::new(MockClient {}))
            .await;
        extension_manager
            .add_mock_extension("ext_b".to_string(), Arc::new(MockClient {}))
            .await;

        let tools = extension_manager
            .get_prefixed_tools_excluding("ext_a")
            .await
            .unwrap();
        let tool_names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();

        assert!(!tool_names.iter().any(|n| n.starts_with("ext_a__")));
        assert!(tool_names.iter().any(|n| n.starts_with("ext_b__")));
    }

    #[tokio::test]
    async fn test_get_prefixed_tools_by_extension_name() {
        let temp_dir = tempfile::tempdir().unwrap();
        let extension_manager =
            ExtensionManager::new_without_provider(temp_dir.path().to_path_buf());

        extension_manager
            .add_mock_extension("ext_a".to_string(), Arc::new(MockClient {}))
            .await;
        extension_manager
            .add_mock_extension("ext_b".to_string(), Arc::new(MockClient {}))
            .await;

        let tools = extension_manager
            .get_prefixed_tools(Some("ext_a".to_string()))
            .await
            .unwrap();
        let tool_names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();

        assert!(tool_names.iter().any(|n| n.starts_with("ext_a__")));
        assert!(!tool_names.iter().any(|n| n.starts_with("ext_b__")));
    }

    // #42 hardening: operator-disabled entries in search_available_extensions
    // output must be labeled so the model doesn't treat the listing as an
    // invitation to silently re-enable what the operator turned off.
    #[test]
    fn config_disabled_lines_label_every_disabled_entry_and_skip_enabled_ones() {
        use crate::config::ExtensionEntry;

        let entries = vec![
            ExtensionEntry {
                enabled: false,
                config: ExtensionConfig::Builtin {
                    name: "developer".to_string(),
                    display_name: Some("Developer".to_string()),
                    description: String::new(),
                    timeout: None,
                    bundled: Some(true),
                    available_tools: vec![],
                },
            },
            ExtensionEntry {
                enabled: true,
                config: ExtensionConfig::stdio("running", "cmd", "An enabled one", 30_u64),
            },
            ExtensionEntry {
                enabled: false,
                config: ExtensionConfig::stdio("custom", "cmd", "A custom server", 30_u64),
            },
        ];
        let persisted = std::collections::HashSet::from([
            "developer".to_string(),
            "running".to_string(),
            "custom".to_string(),
        ]);

        let lines = config_disabled_extension_lines(&entries, &persisted);
        assert_eq!(lines.len(), 2, "enabled entries must not be listed");
        assert!(
            lines.iter().all(|l| l.contains(CONFIG_DISABLED_LABEL)),
            "every operator-disabled entry must carry the label: {lines:?}"
        );
        // Empty Builtin description falls back to the display name.
        assert!(
            lines[0].starts_with("- developer - Developer "),
            "{}",
            lines[0]
        );
        assert!(
            lines[1].starts_with("- custom - A custom server "),
            "{}",
            lines[1]
        );
        assert!(
            !lines.iter().any(|l| l.contains("running")),
            "enabled extension leaked into the disabled list: {lines:?}"
        );
    }

    // #42 provenance: an absent platform extension is injected with its
    // default — a default-off one (chatrecall) reads `enabled: false` without
    // any operator action. It must still be listed as available to enable,
    // but must NOT carry the do-not-enable label.
    #[test]
    fn config_disabled_lines_leave_injected_default_off_entries_unlabeled() {
        use crate::config::ExtensionEntry;

        let entries = vec![
            ExtensionEntry {
                enabled: false,
                config: ExtensionConfig::Platform {
                    name: "chatrecall".to_string(),
                    description: "Recall previous chats".to_string(),
                    bundled: Some(true),
                    available_tools: vec![],
                },
            },
            ExtensionEntry {
                enabled: false,
                config: ExtensionConfig::stdio("custom", "cmd", "A custom server", 30_u64),
            },
        ];
        // Only `custom` was actually written to the config file.
        let persisted = std::collections::HashSet::from(["custom".to_string()]);

        let lines = config_disabled_extension_lines(&entries, &persisted);
        assert_eq!(lines.len(), 2, "both entries stay listed as enableable");
        let chatrecall = lines
            .iter()
            .find(|l| l.contains("chatrecall"))
            .expect("injected default-off entry must stay listed");
        assert!(
            !chatrecall.contains(CONFIG_DISABLED_LABEL),
            "no operator disabled chatrecall — it must not be labeled: {chatrecall}"
        );
        let custom = lines
            .iter()
            .find(|l| l.contains("custom"))
            .expect("persisted entry listed");
        assert!(
            custom.contains(CONFIG_DISABLED_LABEL),
            "operator-persisted enabled:false must be labeled: {custom}"
        );
    }

    #[test]
    fn config_disabled_lines_empty_when_everything_is_enabled() {
        use crate::config::ExtensionEntry;

        let entries = vec![ExtensionEntry {
            enabled: true,
            config: ExtensionConfig::default(),
        }];
        let persisted = std::collections::HashSet::new();
        assert!(config_disabled_extension_lines(&entries, &persisted).is_empty());
        assert!(config_disabled_extension_lines(&[], &persisted).is_empty());
    }

    // ---- issue #48: `/ext:` resolution by id + owning registry ----

    /// `/ext:extensionmanager` used to build an `ExtensionConfig::Builtin` named
    /// `"Extension Manager"` — the entry's *display* string — and the builtin
    /// lookup rejected it with `Unknown builtin extension: Extension Manager`,
    /// which the turn reported as if the extension had been refused.
    #[tokio::test]
    async fn ext_marker_enables_a_platform_extension() {
        let temp_dir = tempfile::tempdir().unwrap();
        let em = Arc::new(ExtensionManager::new_without_provider(
            temp_dir.path().to_path_buf(),
        ));

        let target = resolve_bundled_extension("extensionmanager")
            .expect("`extensionmanager` is a bundled extension");
        assert_eq!(target.kind(), BundledExtensionKind::Platform);
        assert_eq!(target.key(), "extensionmanager");

        let config = target.into_config("Selected via explicit resource marker".to_string());
        assert!(
            matches!(config, ExtensionConfig::Platform { .. }),
            "a platform target must take the platform spawn path, not the builtin one: {config:?}"
        );

        em.add_extension(config)
            .await
            .expect("/ext:extensionmanager must enable the Extension Manager");
        assert!(em.is_extension_enabled("extensionmanager").await);
    }

    /// The `builtin` case that already worked must keep working, end to end.
    #[tokio::test]
    async fn ext_marker_enables_a_builtin_extension() {
        let temp_dir = tempfile::tempdir().unwrap();
        let em = Arc::new(ExtensionManager::new_without_provider(
            temp_dir.path().to_path_buf(),
        ));

        let target =
            resolve_bundled_extension("developer").expect("`developer` is a bundled extension");
        assert_eq!(target.kind(), BundledExtensionKind::Builtin);
        assert_eq!(target.key(), "developer");

        let config = target.into_config("Selected via explicit resource marker".to_string());
        assert!(matches!(config, ExtensionConfig::Builtin { .. }));

        em.add_extension(config)
            .await
            .expect("/ext:developer must enable the developer extension");
        assert!(em.is_extension_enabled("developer").await);
    }

    /// Every bundled extension must resolve to the registry that actually owns
    /// it, whether it is keyed by a lowercase id or by a display string.
    #[test]
    fn resolves_every_bundled_extension_to_its_owning_registry() {
        for (registry_key, def) in PLATFORM_EXTENSIONS.iter() {
            for reference in [*registry_key, def.name] {
                let target = resolve_bundled_extension(reference)
                    .unwrap_or_else(|| panic!("platform extension `{reference}` must resolve"));
                assert_eq!(
                    target.kind(),
                    BundledExtensionKind::Platform,
                    "`{reference}` must resolve to the platform registry"
                );
                assert!(
                    matches!(
                        target.clone().into_config(String::new()),
                        ExtensionConfig::Platform { .. }
                    ),
                    "`{reference}` must produce a Platform config"
                );
                // The name handed to the config is the one the spawn path looks
                // up, so it has to be present in PLATFORM_EXTENSIONS.
                let config = target.into_config(String::new());
                let ExtensionConfig::Platform { name, .. } = &config else {
                    unreachable!()
                };
                assert!(
                    PLATFORM_EXTENSIONS.contains_key(normalize(name).as_str()),
                    "`{name}` must be spawnable by the platform path"
                );
            }
        }

        for (registry_key, def) in biorouter_mcp::BUILTIN_EXTENSIONS.iter() {
            for reference in [*registry_key, def.name] {
                let target = resolve_bundled_extension(reference)
                    .unwrap_or_else(|| panic!("builtin extension `{reference}` must resolve"));
                assert_eq!(
                    target.kind(),
                    BundledExtensionKind::Builtin,
                    "`{reference}` must resolve to the builtin registry"
                );
                let config = target.into_config(String::new());
                let ExtensionConfig::Builtin { name, .. } = &config else {
                    panic!("`{reference}` must produce a Builtin config")
                };
                assert!(
                    biorouter_mcp::BUILTIN_EXTENSIONS.contains_key(name.as_str()),
                    "`{name}` must be spawnable by the builtin path"
                );
            }
        }
    }

    /// Moved here from `resource_refs`, which used to own this table.
    #[test]
    fn resolves_bundled_extension_spelling_aliases() {
        for reference in ["agentdrafter", "agent-drafter", "agent_drafter"] {
            let target = resolve_bundled_extension(reference).expect(reference);
            assert_eq!(target.kind(), BundledExtensionKind::Builtin);
            assert_eq!(target.key(), "agent_drafter");
        }
        for reference in [
            "autovisualizer",
            "auto-visualizer",
            "auto_visualiser",
            "autovisualiser",
        ] {
            let target = resolve_bundled_extension(reference).expect(reference);
            assert_eq!(target.kind(), BundledExtensionKind::Builtin);
            assert_eq!(target.key(), "autovisualiser");
        }
        for reference in [
            "extensionmanager",
            "extension-manager",
            "extension_manager",
            "Extension Manager",
        ] {
            let target = resolve_bundled_extension(reference).expect(reference);
            assert_eq!(target.kind(), BundledExtensionKind::Platform);
            assert_eq!(target.key(), "extensionmanager");
        }
        for reference in ["chatrecall", "chat-recall", "Chat Recall"] {
            let target = resolve_bundled_extension(reference).expect(reference);
            assert_eq!(target.kind(), BundledExtensionKind::Platform);
            assert_eq!(target.key(), "chatrecall");
        }
    }

    /// A non-bundled reference resolves to nothing, so a `/ext:` marker can
    /// never auto-enable a user-configured (stdio / streamable_http /
    /// inline_python / frontend / sse) extension the operator turned off.
    #[test]
    fn does_not_resolve_non_bundled_extensions() {
        for reference in ["", "pubmed", "spokeagent", "some-stdio-server"] {
            assert!(
                resolve_bundled_extension(reference).is_none(),
                "`{reference}` must not resolve to a bundled extension"
            );
        }
    }

    // ---- issue #57: the daemon's auth secret must not reach an extension ------

    /// Child half of the stdio-extension leak probe.
    ///
    /// The leak is in the *inherited* environment, so exercising it means
    /// controlling this process's environment — and `set_var` is unsound in a
    /// threaded test binary. So the parent re-invokes this test binary with the
    /// canary exported, and this half spawns a real child through the real
    /// [`prepare_child_environment`] — exactly what every stdio / inline-python
    /// extension is spawned with — and prints the environment it received.
    #[cfg(unix)]
    #[tokio::test]
    #[ignore]
    async fn leak_probe_prints_extension_child_env() {
        // What `merge_environments` hands the spawn path for an extension that
        // declares its own credentials.
        let declared = HashMap::from([
            (
                "CLINICAL_RECORDS_TOKEN".to_string(),
                "declared-credential-ok".to_string(),
            ),
            (
                "EXTENSION_MODE".to_string(),
                "declared-plain-ok".to_string(),
            ),
        ]);
        let mut command = Command::new("printenv");
        command.envs(declared);
        prepare_child_environment(&mut command, None);
        let out = command.output().await.expect("extension child must spawn");
        println!("BEGIN_CHILD_ENV");
        println!("{}", probe_report(&String::from_utf8_lossy(&out.stdout)));
        println!("END_CHILD_ENV");
    }

    /// What the probe reports back: BioRouter's own namespace, the variables the
    /// parent injected, and — under *any* name — anything whose value carries
    /// the canary, so a copy of the secret under a different key is still
    /// caught. Everything else is dropped: printing the whole environment of
    /// whoever runs the suite would be its own small leak.
    #[cfg(unix)]
    fn probe_report(raw: &str) -> String {
        let canary = std::env::var("BR_TEST_CANARY").unwrap_or_default();
        raw.lines()
            .filter(|line| {
                let key = line.split('=').next().unwrap_or("");
                // The channel that carries the canary *to* the probe is itself
                // inherited; it is not the leak under test.
                if key == "BR_TEST_CANARY" {
                    return false;
                }
                key.starts_with("BIOROUTER_")
                    || key.starts_with("GOOSE_")
                    || matches!(
                        key,
                        "PATH"
                            | "HOME"
                            | "BR_TEST_USER_VAR"
                            | "CLINICAL_RECORDS_TOKEN"
                            | "EXTENSION_MODE"
                    )
                    || (!canary.is_empty() && line.contains(&canary))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[cfg(unix)]
    fn run_extension_leak_probe(canary: &str) -> String {
        let m = module_path!();
        let without_crate = m.split_once("::").map(|(_, rest)| rest).unwrap_or(m);
        let exe = std::env::current_exe().expect("test binary path");
        let out = std::process::Command::new(exe)
            .args([
                "--exact",
                &format!("{without_crate}::leak_probe_prints_extension_child_env"),
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("BIOROUTER_SERVER__SECRET_KEY", canary)
            .env("BR_TEST_CANARY", canary)
            .env("BIOROUTER_PORT", "54321")
            .env("BR_TEST_USER_VAR", "user-env-ok")
            .output()
            .expect("re-invoking the test binary must work");
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        stdout
            .split_once("BEGIN_CHILD_ENV\n")
            .and_then(|(_, rest)| rest.split_once("END_CHILD_ENV"))
            .map(|(body, _)| body.to_string())
            .unwrap_or_else(|| {
                panic!(
                    "probe produced no child environment.\nstdout:\n{stdout}\nstderr:\n{}",
                    String::from_utf8_lossy(&out.stderr)
                )
            })
    }

    #[cfg(unix)]
    #[test]
    fn daemon_secret_never_reaches_an_extension_child() {
        const CANARY: &str = "canary-daemon-secret-4b71";
        let child_env = run_extension_leak_probe(CANARY);
        assert!(
            !child_env.contains(CANARY),
            "issue #57: the daemon's auth secret reached a stdio extension, which \
             can then call biorouterd as an authenticated client.\nchild env:\n{child_env}"
        );
        assert!(
            !child_env.contains("BIOROUTER_SERVER__SECRET_KEY"),
            "the key name itself must be gone, not just the value:\n{child_env}"
        );
    }

    /// The other direction: an extension's own declared credentials, and the
    /// user's environment, must still arrive. Note `CLINICAL_RECORDS_TOKEN` is
    /// secret-shaped by name — the policy deliberately does not touch names
    /// outside BioRouter's own namespace.
    #[cfg(unix)]
    #[test]
    fn extension_child_still_receives_declared_and_user_environment() {
        let child_env = run_extension_leak_probe("canary-unused");
        for expected in [
            "PATH=",
            "HOME=",
            "BIOROUTER_PORT=54321",
            "BR_TEST_USER_VAR=user-env-ok",
            "CLINICAL_RECORDS_TOKEN=declared-credential-ok",
            "EXTENSION_MODE=declared-plain-ok",
        ] {
            assert!(
                child_env.lines().any(|l| l.starts_with(expected)),
                "extension child lost {expected:?} — removing too much is its own \
                 regression.\nchild env:\n{child_env}"
            );
        }
    }
}
