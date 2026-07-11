use std::{
    fs::File,
    io::Read,
    net::SocketAddr,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};

use anyhow::{anyhow, Context};
use axum::{
    body::{to_bytes, Body},
    extract::{OriginalUri, Query, State},
    http::{header, HeaderMap, HeaderName, Method, Response, StatusCode},
    response::IntoResponse,
    routing::{any, get, post},
    Json, Router,
};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use tokio::{net::TcpListener, process::Command, time::sleep};
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing::{info, warn};

const DEFAULT_PUBLIC_PORT: u16 = 8080;
const DEFAULT_API_HOST: &str = "127.0.0.1";
const DEFAULT_API_PORT: u16 = 3000;
const MAX_REGISTRY_DOWNLOAD_BYTES: u64 = 200 * 1024 * 1024;
const UV_SYNC_TIMEOUT: Duration = Duration::from_secs(600);
const TEXT_EXTENSIONS: &[&str] = &["md", "txt", "yaml", "yml", "json", "py", "sh"];
const REGISTRY_DOWNLOAD_HOSTS: &[&str] = &[
    "biorouter.ucsf.edu",
    "github.com",
    "objects.githubusercontent.com",
    "raw.githubusercontent.com",
    "codeload.github.com",
];

#[derive(Parser)]
#[command(
    author,
    version,
    about = "Serve BioRouter as a browser-only headless app"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Serve the headless UI, proxy API traffic, and optionally launch biorouterd.
    Serve(ServeArgs),
    /// Print the URL users should open for this headless instance.
    Url(UrlArgs),
}

#[derive(Parser, Clone)]
struct ServeArgs {
    #[arg(long, env = "BIOROUTER_HEADLESS_HOST", default_value = "0.0.0.0")]
    host: String,
    #[arg(long, env = "BIOROUTER_HEADLESS_PUBLIC_PORT", default_value_t = DEFAULT_PUBLIC_PORT)]
    port: u16,
    #[arg(long, env = "BIOROUTER_HEADLESS_WEB_DIR")]
    web_dir: Option<PathBuf>,
    #[arg(long, env = "BIOROUTER_HEADLESS_PUBLIC_URL")]
    public_url: Option<String>,
    #[arg(long, env = "BIOROUTER_HEADLESS_API_HOST", default_value = DEFAULT_API_HOST)]
    api_host: String,
    #[arg(long, env = "BIOROUTER_HEADLESS_API_PORT", default_value_t = DEFAULT_API_PORT)]
    api_port: u16,
    #[arg(long, env = "BIOROUTER_SERVER__SECRET_KEY")]
    secret_key: Option<String>,
    #[arg(long, env = "BIOROUTER_HEADLESS_BIOROUTERD")]
    biorouterd: Option<PathBuf>,
    #[arg(long, env = "BIOROUTER_HEADLESS_NO_SPAWN", default_value_t = false)]
    no_spawn: bool,
}

#[derive(Parser)]
struct UrlArgs {
    #[arg(long, env = "BIOROUTER_HEADLESS_PUBLIC_PORT", default_value_t = DEFAULT_PUBLIC_PORT)]
    port: u16,
    #[arg(long, env = "BIOROUTER_HEADLESS_PUBLIC_URL")]
    public_url: Option<String>,
}

#[derive(Clone)]
struct AppState {
    api_base_url: String,
    secret_key: String,
    client: reqwest::Client,
    home_dir: PathBuf,
    settings_path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct PathQuery {
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListFilesQuery {
    path: Option<String>,
    extension: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WriteFileRequest {
    path: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct FsPathRequest {
    path: String,
}

#[derive(Debug, Deserialize)]
struct RegistryDownloadRequest {
    url: String,
}

#[derive(Debug, Serialize)]
struct RegistryDownloadResponse {
    path: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FilePathRequest {
    file_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrxtInstallRequest {
    file_path: String,
    extension_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrxtUninstallRequest {
    extension_name: String,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    home_dir: String,
    api_base_url: String,
}

#[derive(Debug, Serialize)]
struct RootsResponse {
    roots: Vec<FsRoot>,
}

#[derive(Debug, Serialize)]
struct FsRoot {
    label: String,
    path: String,
}

#[derive(Debug, Serialize)]
struct DirsResponse {
    dirs: Vec<String>,
}

#[derive(Debug, Serialize)]
struct FilesResponse {
    files: Vec<String>,
}

#[derive(Debug, Serialize)]
struct FsOkResponse {
    ok: bool,
}

#[derive(Debug, Serialize)]
struct SettingsResponse {
    settings: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct ReadFileResponse {
    #[serde(rename = "filePath")]
    file_path: String,
    file: String,
    found: bool,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct FsListResponse {
    path: String,
    entries: Vec<FsEntry>,
}

#[derive(Debug, Serialize)]
struct FsEntry {
    name: String,
    path: String,
    #[serde(rename = "isDir")]
    is_dir: bool,
    #[serde(rename = "isFile")]
    is_file: bool,
}

struct ArchiveEntry {
    name: String,
    is_dir: bool,
    data: Vec<u8>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "biorouter_headless=info,tower_http=info".into()),
        )
        .init();

    let cli = Cli::parse();
    match cli.command.unwrap_or_else(|| {
        Commands::Serve(ServeArgs::parse_from(std::iter::once(
            std::env::args().next().unwrap_or_default(),
        )))
    }) {
        Commands::Serve(args) => serve(args).await,
        Commands::Url(args) => {
            println!("{}", public_url(args.public_url, args.port).await);
            Ok(())
        }
    }
}

async fn serve(args: ServeArgs) -> anyhow::Result<()> {
    let web_dir = args.web_dir.clone().unwrap_or_else(default_web_dir);
    if !web_dir.join("index.html").is_file() {
        return Err(anyhow!("missing web bundle at {}", web_dir.display()));
    }

    let secret_key = args
        .secret_key
        .clone()
        .or_else(|| std::env::var("BIOROUTER_SERVER__SECRET_KEY").ok())
        .unwrap_or_else(|| "devkey".to_string());
    let api_base_url = format!("http://{}:{}", args.api_host, args.api_port);

    let mut child = None;
    if !args.no_spawn {
        let biorouterd = args
            .biorouterd
            .clone()
            .unwrap_or_else(default_biorouterd_path);
        child = Some(spawn_biorouterd(
            &biorouterd,
            &args.api_host,
            args.api_port,
            &secret_key,
        )?);
        wait_for_api(&api_base_url, &secret_key).await?;
    }

    let state = Arc::new(AppState {
        api_base_url,
        secret_key,
        client: reqwest::Client::new(),
        home_dir: dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")),
        settings_path: dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/"))
            .join(".config/biorouter/headless/settings.json"),
    });

    let base_path = base_path_from_public_url(&args.public_url);
    if !base_path.is_empty() {
        info!("serving under path prefix {base_path} (assets and API calls are prefixed)");
    }

    let app = router(state, web_dir, base_path);
    let bind_addr: SocketAddr = format!("{}:{}", args.host, args.port)
        .parse()
        .with_context(|| format!("invalid bind address {}:{}", args.host, args.port))?;
    let listener = TcpListener::bind(bind_addr).await?;
    let url = public_url(args.public_url.clone(), args.port).await;
    println!("{url}");
    info!("serving BioRouter headless at {url}");

    let server = axum::serve(listener, app).with_graceful_shutdown(shutdown_signal());
    let result = server.await.context("headless server failed");
    if let Some(mut child) = child.take() {
        if let Err(e) = child.kill().await {
            warn!("failed to stop biorouterd child: {e}");
        }
    }
    result
}

fn router(state: Arc<AppState>, web_dir: PathBuf, base_path: String) -> Router {
    let index_path = web_dir.join("index.html");
    // Precompute the app-shell HTML once. When served behind a path prefix, the
    // baked-in root-absolute asset URLs are rewritten and the headless runtime
    // config (API + headless base URLs) is injected, so the SPA resolves its
    // assets and API calls under the prefix rather than at the server root.
    let index_html = build_index_html(&index_path, &base_path);
    let index_service = get(move || {
        let html = index_html.clone();
        async move { axum::response::Html(html) }
    });
    Router::new()
        .route("/headless/health", get(health))
        .route(
            "/headless/settings",
            get(settings_read).post(settings_write),
        )
        .route("/headless/registry/download", post(registry_download))
        .route("/headless/skills/extract-zip", post(skills_extract_zip))
        .route("/headless/brxt/validate", post(brxt_validate))
        .route("/headless/brxt/install", post(brxt_install))
        .route("/headless/brxt/uninstall", post(brxt_uninstall))
        .route("/headless/fs/roots", get(fs_roots))
        .route("/headless/fs/list", get(fs_list))
        .route("/headless/fs/list-files", get(fs_list_files))
        .route("/headless/fs/list-dirs", get(fs_list_dirs))
        .route("/headless/fs/read", get(fs_read))
        .route("/headless/fs/write", post(fs_write))
        .route("/headless/fs/ensure-dir", post(fs_ensure_dir))
        .route("/headless/fs/delete-file", post(fs_delete_file))
        .route("/headless/fs/delete-dir", post(fs_delete_dir))
        .route("/api", any(proxy_api_root))
        .route("/api/", any(proxy_api_root))
        .route("/api/{*path}", any(proxy_api))
        // Serve real files (the hashed /assets/*, etc.) from disk. Disable
        // directory auto-indexing so a request for "/" falls through to the
        // rewritten app shell instead of the raw, un-rewritten index.html.
        .fallback_service(
            ServeDir::new(web_dir)
                .append_index_html_on_directories(false)
                .fallback(index_service),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// The URL path prefix a reverse proxy serves this app under, derived from the
/// configured public URL. `https://host/biorouter/` → `/biorouter`; a root URL
/// (`https://host/`) or no configured URL → `""` (served unchanged, as before).
///
/// This assumes a prefix-stripping proxy (e.g. jupyter-server-proxy): the proxy
/// removes `/biorouter` before forwarding, so the backend keeps serving routes
/// at the root while the emitted URLs carry the prefix for the browser.
fn base_path_from_public_url(public_url: &Option<String>) -> String {
    let Some(raw) = public_url else {
        return String::new();
    };
    let raw = raw.trim();
    // Drop the scheme, then take everything after the first '/' (the host's path).
    let after_scheme = raw.split_once("://").map_or(raw, |(_, rest)| rest);
    let path = after_scheme.split_once('/').map_or("", |(_, rest)| rest);
    // Ignore any query/fragment. `normalize_base_path` restores the leading '/'.
    let path = path.split(['?', '#']).next().unwrap_or("");
    normalize_base_path(path)
}

/// Normalize a path prefix to a leading slash and no trailing slash. `/` or an
/// empty path normalize to `""` (no prefix).
fn normalize_base_path(path: &str) -> String {
    let trimmed = path.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return String::new();
    }
    if let Some(stripped) = trimmed.strip_prefix('/') {
        format!("/{stripped}")
    } else {
        format!("/{trimmed}")
    }
}

/// Read the built `index.html` and, when a non-empty `base_path` is given,
/// rewrite the root-absolute asset URLs to include the prefix and inject the
/// headless runtime config (API + headless base URLs). With an empty prefix the
/// file is returned unchanged.
fn build_index_html(index_path: &Path, base_path: &str) -> String {
    let raw = std::fs::read_to_string(index_path).unwrap_or_default();
    rewrite_index_html(&raw, base_path)
}

/// Pure core of [`build_index_html`]: rewrite asset URLs and inject the runtime
/// config in `raw` for the given prefix. Returns `raw` unchanged when the prefix
/// is empty.
fn rewrite_index_html(raw: &str, base_path: &str) -> String {
    if base_path.is_empty() {
        return raw.to_string();
    }

    // Vite emits root-absolute asset URLs (`src="/assets/…"`, `href="/assets/…"`).
    // Prefix them so they resolve under the proxy path.
    let rewritten = raw
        .replace("\"/assets/", &format!("\"{base_path}/assets/"))
        .replace("'/assets/", &format!("'{base_path}/assets/"));

    // Populate the config the renderer reads before it derives API/headless base
    // URLs from window.location.origin (which drops the path prefix).
    let inject = format!(
        "<script>window.__BIOROUTER_HEADLESS_CONFIG__={{\"apiBaseUrl\":\"{base_path}/api\",\"headlessBaseUrl\":\"{base_path}/headless\"}};</script>"
    );
    match rewritten.split_once("</head>") {
        Some((head, rest)) => format!("{head}{inject}</head>{rest}"),
        None => format!("{inject}{rewritten}"),
    }
}

fn spawn_biorouterd(
    biorouterd: &Path,
    api_host: &str,
    api_port: u16,
    secret_key: &str,
) -> anyhow::Result<tokio::process::Child> {
    if !biorouterd.is_file() {
        return Err(anyhow!(
            "missing biorouterd binary at {}",
            biorouterd.display()
        ));
    }
    info!("starting biorouterd from {}", biorouterd.display());
    Command::new(biorouterd)
        .arg("agent")
        .env("BIOROUTER_HOST", api_host)
        .env("BIOROUTER_PORT", api_port.to_string())
        .env("BIOROUTER_SERVER__SECRET_KEY", secret_key)
        .env("BIOROUTER_DISABLE_KEYRING", "true")
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context("failed to start biorouterd")
}

async fn wait_for_api(api_base_url: &str, secret_key: &str) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/status", api_base_url.trim_end_matches('/'));
    for _ in 0..120 {
        if let Ok(response) = client
            .get(&url)
            .header("X-Secret-Key", secret_key)
            .send()
            .await
        {
            if response.status().is_success() {
                return Ok(());
            }
        }
        sleep(Duration::from_millis(500)).await;
    }
    Err(anyhow!("biorouterd did not become ready at {url}"))
}

async fn proxy_api_root(
    State(state): State<Arc<AppState>>,
    method: Method,
    headers: HeaderMap,
    uri: OriginalUri,
    body: Body,
) -> impl IntoResponse {
    proxy_to_api(state, method, headers, uri, body, "").await
}

async fn proxy_api(
    State(state): State<Arc<AppState>>,
    method: Method,
    headers: HeaderMap,
    uri: OriginalUri,
    body: Body,
) -> impl IntoResponse {
    let path = uri
        .0
        .path()
        .trim_start_matches("/api")
        .trim_start_matches('/');
    let path = path.to_string();
    proxy_to_api(state, method, headers, uri, body, &path).await
}

async fn proxy_to_api(
    state: Arc<AppState>,
    method: Method,
    headers: HeaderMap,
    uri: OriginalUri,
    body: Body,
    path: &str,
) -> Response<Body> {
    let mut upstream_url = if path.is_empty() {
        state.api_base_url.clone()
    } else {
        format!("{}/{}", state.api_base_url.trim_end_matches('/'), path)
    };
    if let Some(query) = uri.0.query() {
        upstream_url.push('?');
        upstream_url.push_str(query);
    }

    let Ok(bytes) = to_bytes(body, usize::MAX).await else {
        return plain_response(StatusCode::BAD_REQUEST, "failed to read request body");
    };

    let mut request = state.client.request(method, upstream_url);
    for (name, value) in headers.iter() {
        if should_forward_request_header(name) {
            request = request.header(name, value);
        }
    }
    request = request
        .header("X-Secret-Key", &state.secret_key)
        .body(bytes);

    match request.send().await {
        Ok(response) => {
            let status = response.status();
            let mut builder = Response::builder().status(status);
            for (name, value) in response.headers() {
                if should_forward_response_header(name) {
                    builder = builder.header(name, value);
                }
            }
            builder
                .body(Body::from_stream(response.bytes_stream()))
                .unwrap_or_else(|_| plain_response(StatusCode::BAD_GATEWAY, "proxy error"))
        }
        Err(e) => plain_response(
            StatusCode::BAD_GATEWAY,
            &format!("failed to proxy request to biorouterd: {e}"),
        ),
    }
}

fn should_forward_request_header(name: &HeaderName) -> bool {
    !matches!(
        name.as_str().to_ascii_lowercase().as_str(),
        "host" | "content-length" | "x-secret-key" | "connection"
    )
}

fn should_forward_response_header(name: &HeaderName) -> bool {
    !matches!(
        name.as_str().to_ascii_lowercase().as_str(),
        "content-length" | "transfer-encoding" | "connection"
    )
}

async fn health(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        home_dir: path_string(&state.home_dir),
        api_base_url: state.api_base_url.clone(),
    })
}

async fn fs_roots(State(state): State<Arc<AppState>>) -> Json<RootsResponse> {
    let mut roots = vec![
        FsRoot {
            label: "Home".to_string(),
            path: path_string(&state.home_dir),
        },
        FsRoot {
            label: "BioRouter config".to_string(),
            path: path_string(&state.home_dir.join(".config/biorouter")),
        },
        FsRoot {
            label: "Temporary files".to_string(),
            path: "/tmp".to_string(),
        },
    ];
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(FsRoot {
            label: "Current directory".to_string(),
            path: path_string(&cwd),
        });
    }
    Json(RootsResponse { roots })
}

async fn fs_list(Query(query): Query<PathQuery>) -> impl IntoResponse {
    let path = expand_path(query.path.as_deref().unwrap_or("~"));
    let mut read_dir = match tokio::fs::read_dir(&path).await {
        Ok(read_dir) => read_dir,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, format!("failed to list path: {e}")),
    };
    let mut entries = Vec::new();
    while let Ok(Some(entry)) = read_dir.next_entry().await {
        if let Ok(metadata) = entry.metadata().await {
            entries.push(FsEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                path: path_string(&entry.path()),
                is_dir: metadata.is_dir(),
                is_file: metadata.is_file(),
            });
        }
    }
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
    Json(FsListResponse {
        path: path_string(&path),
        entries,
    })
    .into_response()
}

async fn fs_list_dirs(Query(query): Query<PathQuery>) -> impl IntoResponse {
    let path = expand_path(query.path.as_deref().unwrap_or("~"));
    let mut read_dir = match tokio::fs::read_dir(&path).await {
        Ok(read_dir) => read_dir,
        Err(_) => return Json(DirsResponse { dirs: Vec::new() }).into_response(),
    };
    let mut dirs = Vec::new();
    while let Ok(Some(entry)) = read_dir.next_entry().await {
        if entry
            .metadata()
            .await
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false)
        {
            dirs.push(entry.file_name().to_string_lossy().to_string());
        }
    }
    dirs.sort();
    Json(DirsResponse { dirs }).into_response()
}

async fn fs_list_files(Query(query): Query<ListFilesQuery>) -> impl IntoResponse {
    let path = expand_path(query.path.as_deref().unwrap_or("~"));
    let mut read_dir = match tokio::fs::read_dir(&path).await {
        Ok(read_dir) => read_dir,
        Err(_) => return Json(FilesResponse { files: Vec::new() }).into_response(),
    };
    let mut files = Vec::new();
    while let Ok(Some(entry)) = read_dir.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if query
            .extension
            .as_ref()
            .is_none_or(|extension| name.ends_with(extension))
        {
            files.push(name);
        }
    }
    files.sort();
    Json(FilesResponse { files }).into_response()
}

async fn fs_read(Query(query): Query<PathQuery>) -> Json<ReadFileResponse> {
    let requested = query.path.unwrap_or_default();
    let path = expand_path(&requested);
    match tokio::fs::read_to_string(&path).await {
        Ok(file) => Json(ReadFileResponse {
            file_path: requested,
            file,
            found: true,
            error: None,
        }),
        Err(e) => Json(ReadFileResponse {
            file_path: requested,
            file: String::new(),
            found: false,
            error: Some(e.to_string()),
        }),
    }
}

async fn settings_read(State(state): State<Arc<AppState>>) -> Json<SettingsResponse> {
    let settings = match tokio::fs::read_to_string(&state.settings_path).await {
        Ok(file) => serde_json::from_str(&file).unwrap_or_else(|_| default_settings()),
        Err(_) => default_settings(),
    };
    Json(SettingsResponse { settings })
}

async fn settings_write(
    State(state): State<Arc<AppState>>,
    Json(settings): Json<serde_json::Value>,
) -> impl IntoResponse {
    if !settings.is_object() {
        return json_error(
            StatusCode::BAD_REQUEST,
            "settings payload must be a JSON object".to_string(),
        );
    }
    if let Some(parent) = state.settings_path.parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            return json_error(
                StatusCode::BAD_REQUEST,
                format!("failed to create settings directory: {e}"),
            );
        }
    }
    let Ok(serialized) = serde_json::to_string_pretty(&settings) else {
        return json_error(
            StatusCode::BAD_REQUEST,
            "failed to serialize settings".to_string(),
        );
    };
    match tokio::fs::write(&state.settings_path, serialized).await {
        Ok(()) => Json(FsOkResponse { ok: true }).into_response(),
        Err(e) => json_error(
            StatusCode::BAD_REQUEST,
            format!("failed to write settings: {e}"),
        ),
    }
}

async fn registry_download(
    State(state): State<Arc<AppState>>,
    Json(request): Json<RegistryDownloadRequest>,
) -> Json<RegistryDownloadResponse> {
    let Some(url) = allowed_registry_url(&request.url) else {
        return Json(RegistryDownloadResponse {
            path: None,
            error: Some("Refusing to download from an untrusted URL.".to_string()),
        });
    };
    let path_lower = url.path().to_ascii_lowercase();
    if !path_lower.ends_with(".zip") && !path_lower.ends_with(".brxt") {
        return Json(RegistryDownloadResponse {
            path: None,
            error: Some("Unsupported asset type.".to_string()),
        });
    }

    let response = match state
        .client
        .get(url.clone())
        .header(header::USER_AGENT, "BioRouter")
        .send()
        .await
    {
        Ok(response) => response,
        Err(e) => {
            return Json(RegistryDownloadResponse {
                path: None,
                error: Some(format!("Download failed: {e}")),
            });
        }
    };
    if !response.status().is_success() {
        return Json(RegistryDownloadResponse {
            path: None,
            error: Some(format!("Download failed: HTTP {}", response.status())),
        });
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_REGISTRY_DOWNLOAD_BYTES)
    {
        return Json(RegistryDownloadResponse {
            path: None,
            error: Some("Download too large.".to_string()),
        });
    }
    let bytes = match response.bytes().await {
        Ok(bytes) if bytes.len() as u64 <= MAX_REGISTRY_DOWNLOAD_BYTES => bytes,
        Ok(_) => {
            return Json(RegistryDownloadResponse {
                path: None,
                error: Some("Download too large.".to_string()),
            });
        }
        Err(e) => {
            return Json(RegistryDownloadResponse {
                path: None,
                error: Some(format!("Download failed: {e}")),
            });
        }
    };

    let dir = std::env::temp_dir().join("biorouter-registry");
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        return Json(RegistryDownloadResponse {
            path: None,
            error: Some(format!("Download failed: {e}")),
        });
    }
    let safe_name = url
        .path_segments()
        .and_then(Iterator::last)
        .filter(|name| !name.is_empty())
        .map(sanitize_asset_name)
        .unwrap_or_else(|| "asset.zip".to_string());
    let dest = dir.join(format!("{}-{safe_name}", random_suffix()));
    match tokio::fs::write(&dest, bytes).await {
        Ok(()) => Json(RegistryDownloadResponse {
            path: Some(path_string(&dest)),
            error: None,
        }),
        Err(e) => Json(RegistryDownloadResponse {
            path: None,
            error: Some(format!("Download failed: {e}")),
        }),
    }
}

async fn skills_extract_zip(Json(request): Json<FilePathRequest>) -> Json<serde_json::Value> {
    Json(match extract_skill_zip(&expand_path(&request.file_path)) {
        Ok(value) => value,
        Err(e) => serde_json::json!({ "error": e }),
    })
}

async fn brxt_validate(Json(request): Json<FilePathRequest>) -> Json<serde_json::Value> {
    Json(
        match validate_brxt_bundle(&expand_path(&request.file_path)) {
            Ok(value) => value,
            Err(e) => serde_json::json!({ "error": e }),
        },
    )
}

async fn brxt_install(
    State(state): State<Arc<AppState>>,
    Json(request): Json<BrxtInstallRequest>,
) -> Json<serde_json::Value> {
    let result = install_brxt_bundle(
        state,
        &expand_path(&request.file_path),
        &request.extension_name,
    )
    .await;
    Json(match result {
        Ok(install_dir) => {
            serde_json::json!({ "success": true, "installDir": path_string(&install_dir) })
        }
        Err(e) => serde_json::json!({ "error": format!("Installation failed: {e}") }),
    })
}

async fn brxt_uninstall(
    State(state): State<Arc<AppState>>,
    Json(request): Json<BrxtUninstallRequest>,
) -> Json<serde_json::Value> {
    let result = uninstall_brxt_extension(&state.home_dir, &request.extension_name);
    Json(match result {
        Ok(()) => serde_json::json!({ "success": true }),
        Err(e) => serde_json::json!({ "error": format!("Uninstall failed: {e}") }),
    })
}

async fn fs_write(Json(request): Json<WriteFileRequest>) -> impl IntoResponse {
    let path = expand_path(&request.path);
    if let Err(message) = validate_file_path(&path) {
        return json_error(StatusCode::BAD_REQUEST, message);
    }
    if let Some(parent) = path.parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            return json_error(
                StatusCode::BAD_REQUEST,
                format!("failed to create parent directory: {e}"),
            );
        }
    }
    match tokio::fs::write(&path, request.content).await {
        Ok(()) => Json(FsOkResponse { ok: true }).into_response(),
        Err(e) => json_error(
            StatusCode::BAD_REQUEST,
            format!("failed to write file: {e}"),
        ),
    }
}

async fn fs_ensure_dir(Json(request): Json<FsPathRequest>) -> impl IntoResponse {
    let path = expand_path(&request.path);
    if let Err(message) = validate_file_path(&path) {
        return json_error(StatusCode::BAD_REQUEST, message);
    }
    match tokio::fs::create_dir_all(&path).await {
        Ok(()) => Json(FsOkResponse { ok: true }).into_response(),
        Err(e) => json_error(
            StatusCode::BAD_REQUEST,
            format!("failed to create directory: {e}"),
        ),
    }
}

async fn fs_delete_file(Json(request): Json<FsPathRequest>) -> impl IntoResponse {
    let path = expand_path(&request.path);
    if let Err(message) = validate_file_path(&path) {
        return json_error(StatusCode::BAD_REQUEST, message);
    }
    match tokio::fs::remove_file(&path).await {
        Ok(()) => Json(FsOkResponse { ok: true }).into_response(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Json(FsOkResponse { ok: true }).into_response()
        }
        Err(e) => json_error(
            StatusCode::BAD_REQUEST,
            format!("failed to delete file: {e}"),
        ),
    }
}

async fn fs_delete_dir(Json(request): Json<FsPathRequest>) -> impl IntoResponse {
    let path = expand_path(&request.path);
    if let Err(message) = validate_delete_dir_path(&path) {
        return json_error(StatusCode::BAD_REQUEST, message);
    }
    match tokio::fs::remove_dir_all(&path).await {
        Ok(()) => Json(FsOkResponse { ok: true }).into_response(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Json(FsOkResponse { ok: true }).into_response()
        }
        Err(e) => json_error(
            StatusCode::BAD_REQUEST,
            format!("failed to delete directory: {e}"),
        ),
    }
}

fn allowed_registry_url(raw_url: &str) -> Option<reqwest::Url> {
    let url = reqwest::Url::parse(raw_url).ok()?;
    if url.scheme() != "https" {
        return None;
    }
    let host = url.host_str()?;
    REGISTRY_DOWNLOAD_HOSTS.contains(&host).then_some(url)
}

fn sanitize_asset_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() {
        "asset.zip".to_string()
    } else {
        sanitized
    }
}

fn random_suffix() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{nanos:x}")
}

fn read_archive(path: &Path) -> Result<Vec<ArchiveEntry>, String> {
    let file = File::open(path).map_err(|e| format!("Failed to read ZIP: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Failed to read ZIP: {e}"))?;
    let mut entries = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|e| format!("Failed to read ZIP: {e}"))?;
        let name = file.name().replace('\\', "/");
        safe_entry_name(&name).map_err(|e| format!("Failed to read ZIP: {e}"))?;
        let is_dir = file.is_dir();
        let mut data = Vec::new();
        if !is_dir {
            file.read_to_end(&mut data)
                .map_err(|e| format!("Failed to read ZIP: {e}"))?;
        }
        entries.push(ArchiveEntry { name, is_dir, data });
    }
    Ok(entries)
}

fn safe_entry_name(entry_name: &str) -> Result<PathBuf, String> {
    if entry_name.is_empty() || entry_name.contains('\0') {
        return Err(format!("Unsafe zip entry path: {entry_name}"));
    }
    let path = Path::new(entry_name);
    if path.is_absolute() || entry_name.starts_with('/') || entry_name.starts_with('\\') {
        return Err(format!("Unsafe zip entry path: {entry_name}"));
    }
    let mut safe = PathBuf::new();
    for part in entry_name
        .split(['/', '\\'])
        .filter(|part| !part.is_empty())
    {
        if part == ".." {
            return Err(format!("Unsafe zip entry path: {entry_name}"));
        }
        safe.push(part);
    }
    if safe.as_os_str().is_empty() {
        return Err(format!("Unsafe zip entry path: {entry_name}"));
    }
    Ok(safe)
}

fn safe_entry_target(base: &Path, entry_name: &str) -> Result<PathBuf, String> {
    let relative = safe_entry_name(entry_name)?;
    let base = base
        .canonicalize()
        .unwrap_or_else(|_| base.to_path_buf())
        .components()
        .collect::<PathBuf>();
    let target = base.join(relative);
    if target != base && target.starts_with(&base) {
        Ok(target)
    } else {
        Err(format!("Unsafe zip entry path: {entry_name}"))
    }
}

fn parse_skill_frontmatter(content: &str) -> Option<(String, String)> {
    let mut lines = content.lines();
    if lines.next()? != "---" {
        return None;
    }
    let mut name = None;
    let mut description = None;
    for line in lines {
        if line == "---" {
            break;
        }
        if let Some(value) = line.strip_prefix("name:") {
            name = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("description:") {
            description = Some(value.trim().to_string());
        }
    }
    match (name, description) {
        (Some(name), Some(description)) if !name.is_empty() && !description.is_empty() => {
            Some((name, description))
        }
        _ => None,
    }
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if ch == '-' {
            if !last_dash {
                slug.push('-');
                last_dash = true;
            }
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

fn is_text_file(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            TEXT_EXTENSIONS
                .iter()
                .any(|allowed| extension.eq_ignore_ascii_case(allowed))
        })
}

fn entry_text(entry: &ArchiveEntry) -> String {
    String::from_utf8_lossy(&entry.data).to_string()
}

fn extract_skill_zip(path: &Path) -> Result<serde_json::Value, String> {
    let entries = read_archive(path)?;
    if let Some(skill_entry) = entries.iter().find(|entry| entry.name == "SKILL.md") {
        return extract_single_skill(&entries, skill_entry, "");
    }
    if let Some(skill_entry) = entries.iter().find(|entry| {
        let parts: Vec<_> = entry.name.split('/').collect();
        parts.len() == 2 && parts[1] == "SKILL.md"
    }) {
        let prefix = skill_entry
            .name
            .strip_suffix("SKILL.md")
            .unwrap_or_default();
        return extract_single_skill(&entries, skill_entry, prefix);
    }
    extract_skill_bundle(&entries)
}

fn extract_single_skill(
    entries: &[ArchiveEntry],
    skill_entry: &ArchiveEntry,
    prefix: &str,
) -> Result<serde_json::Value, String> {
    let Some((name, description)) = parse_skill_frontmatter(&entry_text(skill_entry)) else {
        return Err(
            "SKILL.md must have valid frontmatter with \"name\" and \"description\".".to_string(),
        );
    };
    let mut files = Vec::new();
    for entry in entries {
        if entry.is_dir || !entry.name.starts_with(prefix) {
            continue;
        }
        let rel_name = entry.name.strip_prefix(prefix).unwrap_or(&entry.name);
        if rel_name.is_empty() || safe_entry_name(rel_name).is_err() || !is_text_file(rel_name) {
            continue;
        }
        files.push(serde_json::json!([rel_name, entry_text(entry)]));
    }
    Ok(serde_json::json!({
        "isBundle": false,
        "files": files,
        "name": name,
        "description": description,
        "slug": slugify(&name),
    }))
}

fn extract_skill_bundle(entries: &[ArchiveEntry]) -> Result<serde_json::Value, String> {
    let skill_entries: Vec<_> = entries
        .iter()
        .filter(|entry| {
            let parts: Vec<_> = entry.name.split('/').collect();
            parts.len() == 3 && parts[2] == "SKILL.md"
        })
        .collect();
    if skill_entries.is_empty() {
        return Err("No SKILL.md found in the ZIP file.".to_string());
    }

    let bundle_folder = skill_entries[0]
        .name
        .split('/')
        .next()
        .unwrap_or_default()
        .to_string();
    let bundle_prefix = format!("{bundle_folder}/");
    let mut bundle_skills = Vec::new();
    for entry in skill_entries {
        if !entry.name.starts_with(&bundle_prefix) {
            continue;
        }
        if let Some((name, description)) = parse_skill_frontmatter(&entry_text(entry)) {
            bundle_skills.push(serde_json::json!({
                "name": name,
                "description": description,
            }));
        }
    }
    if bundle_skills.is_empty() {
        return Err("No valid SKILL.md files found in bundle.".to_string());
    }

    let mut files = Vec::new();
    for entry in entries {
        if entry.is_dir || !entry.name.starts_with(&bundle_prefix) {
            continue;
        }
        let rel_name = entry
            .name
            .strip_prefix(&bundle_prefix)
            .unwrap_or(&entry.name);
        if rel_name.is_empty() || safe_entry_name(rel_name).is_err() || !is_text_file(rel_name) {
            continue;
        }
        files.push(serde_json::json!([rel_name, entry_text(entry)]));
    }

    Ok(serde_json::json!({
        "isBundle": true,
        "bundleName": bundle_folder,
        "bundleSkills": bundle_skills,
        "files": files,
        "slug": slugify(&bundle_folder),
        "name": bundle_folder,
        "description": format!("Bundle of {} skills", bundle_skills.len()),
    }))
}

fn validate_brxt_bundle(path: &Path) -> Result<serde_json::Value, String> {
    let entries = read_archive(path)?;
    if !entries.iter().any(|entry| entry.name == "manifest.json") {
        return Err("Missing manifest.json — not a valid .brxt bundle".to_string());
    }
    if !entries
        .iter()
        .any(|entry| entry.name.eq_ignore_ascii_case("README.md"))
    {
        return Err("Missing README.md — not a valid .brxt bundle".to_string());
    }
    if !entries.iter().any(|entry| entry.name == "pyproject.toml") {
        return Err("Missing pyproject.toml — not a valid .brxt bundle".to_string());
    }
    if !entries.iter().any(|entry| entry.name.starts_with("src/")) {
        return Err("Missing src/ directory — not a valid .brxt bundle".to_string());
    }

    let manifest_entry = entries
        .iter()
        .find(|entry| entry.name == "manifest.json")
        .ok_or_else(|| "Could not read manifest.json".to_string())?;
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_entry.data)
        .map_err(|e| format!("Failed to read bundle: {e}"))?;
    for field in [
        "name",
        "display_name",
        "description",
        "version",
        "entry_point",
        "repository",
    ] {
        if manifest
            .get(field)
            .and_then(|value| value.as_str())
            .is_none_or(|value| value.trim().is_empty())
        {
            return Err(format!("manifest.json missing required field: \"{field}\""));
        }
    }
    if !manifest
        .get("env_vars")
        .is_some_and(serde_json::Value::is_array)
    {
        return Err("manifest.json \"env_vars\" must be an array".to_string());
    }

    let mut skills_preview = Vec::new();
    for entry in &entries {
        let parts: Vec<_> = entry.name.split('/').collect();
        if parts.len() == 3 && parts[0] == "skills" && parts[2] == "SKILL.md" {
            if let Some((name, description)) = parse_skill_frontmatter(&entry_text(entry)) {
                skills_preview.push(serde_json::json!({
                    "slug": parts[1],
                    "name": name,
                    "description": description,
                }));
            }
        }
    }

    Ok(serde_json::json!({
        "manifest": manifest,
        "skillsPreview": skills_preview,
    }))
}

async fn install_brxt_bundle(
    state: Arc<AppState>,
    file_path: &Path,
    extension_name: &str,
) -> anyhow::Result<PathBuf> {
    validate_extension_name(extension_name)?;
    let install_dir = state
        .home_dir
        .join(".config/biorouter/extensions")
        .join(extension_name);
    tokio::fs::create_dir_all(&install_dir).await?;
    let entries = read_archive(file_path).map_err(|e| anyhow!(e))?;
    for entry in entries {
        let target = safe_entry_target(&install_dir, &entry.name).map_err(|e| anyhow!(e))?;
        if entry.is_dir {
            tokio::fs::create_dir_all(&target).await?;
        } else {
            if let Some(parent) = target.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::write(&target, entry.data).await?;
        }
    }

    let output = match tokio::time::timeout(
        UV_SYNC_TIMEOUT,
        Command::new("uv")
            .arg("sync")
            .current_dir(&install_dir)
            .env("HOME", &state.home_dir)
            .env("PATH", headless_command_path(&state.home_dir))
            .output(),
    )
    .await
    {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => return Err(anyhow!("uv sync failed: {e}")),
        Err(_) => {
            return Err(anyhow!(
                "uv sync timed out after {} minutes",
                UV_SYNC_TIMEOUT.as_secs() / 60
            ));
        }
    };
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        let detail = if detail.trim().is_empty() {
            String::from_utf8_lossy(&output.stdout).to_string()
        } else {
            detail.to_string()
        };
        return Err(anyhow!("uv sync failed: {}", detail.trim()));
    }
    Ok(install_dir)
}

fn uninstall_brxt_extension(home_dir: &Path, extension_name: &str) -> anyhow::Result<()> {
    validate_extension_name(extension_name)?;
    let extensions_base = home_dir.join(".config/biorouter/extensions");
    let install_dir = extensions_base.join(extension_name);
    if !install_dir.starts_with(&extensions_base) {
        return Err(anyhow!("Invalid extension name."));
    }
    if install_dir.exists() {
        std::fs::remove_dir_all(install_dir)?;
    }
    Ok(())
}

fn validate_extension_name(extension_name: &str) -> anyhow::Result<()> {
    if extension_name.is_empty()
        || extension_name == "."
        || extension_name == ".."
        || extension_name.contains('/')
        || extension_name.contains('\\')
    {
        return Err(anyhow!("Invalid extension name."));
    }
    Ok(())
}

fn headless_command_path(home_dir: &Path) -> String {
    let existing = std::env::var("PATH").unwrap_or_default();
    let home_local_bin = home_dir.join(".local/bin");
    format!(
        "{}:/usr/local/bin:/usr/bin:/bin:{}",
        path_string(&home_local_bin),
        existing
    )
}

fn json_error(status: StatusCode, message: String) -> axum::response::Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

fn default_settings() -> serde_json::Value {
    serde_json::json!({
        "envToggles": {
            "BIOROUTER_SERVER__MEMORY": false,
            "BIOROUTER_SERVER__COMPUTER_CONTROLLER": false
        },
        "showMenuBarIcon": false,
        "showDockIcon": false,
        "enableWakelock": false,
        "spellcheckEnabled": true
    })
}

fn plain_response(status: StatusCode, message: &str) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(message.to_string()))
        .expect("valid plain response")
}

fn expand_path(path: &str) -> PathBuf {
    if path == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(path));
    }
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    PathBuf::from(path)
}

fn validate_file_path(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() || path == Path::new("/") {
        return Err("refusing to modify an empty path or filesystem root".to_string());
    }
    Ok(())
}

fn validate_delete_dir_path(path: &Path) -> Result<(), String> {
    validate_file_path(path)?;
    if dirs::home_dir().is_some_and(|home| home == path) {
        return Err("refusing to delete the home directory".to_string());
    }
    if path == Path::new("/tmp") {
        return Err("refusing to delete /tmp".to_string());
    }
    Ok(())
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn default_web_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .and_then(|bin| bin.parent().map(|install| install.join("web")))
        .unwrap_or_else(|| PathBuf::from("web"))
}

fn default_biorouterd_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|bin| bin.join("biorouterd")))
        .unwrap_or_else(|| PathBuf::from("biorouterd"))
}

async fn public_url(configured: Option<String>, port: u16) -> String {
    if let Some(url) = configured {
        return url;
    }
    let host = metadata_public_ip()
        .await
        .or_else(|| local_lan_ip().ok())
        .unwrap_or_else(|| "127.0.0.1".to_string());
    format!("http://{host}:{port}/")
}

async fn metadata_public_ip() -> Option<String> {
    let client = reqwest::Client::new();
    let token = client
        .put("http://169.254.169.254/latest/api/token")
        .header("X-aws-ec2-metadata-token-ttl-seconds", "60")
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    let ip = client
        .get("http://169.254.169.254/latest/meta-data/public-ipv4")
        .header("X-aws-ec2-metadata-token", token)
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    (!ip.trim().is_empty()).then(|| ip.trim().to_string())
}

fn local_lan_ip() -> anyhow::Result<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0")?;
    socket.connect("8.8.8.8:80")?;
    Ok(socket.local_addr()?.ip().to_string())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            warn!("failed to install Ctrl-C handler: {e}");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(e) => warn!("failed to install terminate handler: {e}"),
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

#[cfg(test)]
mod tests {
    use super::{base_path_from_public_url, normalize_base_path, rewrite_index_html};

    #[test]
    fn base_path_derives_prefix_from_public_url() {
        let cases = [
            (Some("https://host/biorouter/"), "/biorouter"),
            (Some("https://host/biorouter"), "/biorouter"),
            (Some("http://host:8080/a/b/"), "/a/b"),
            (Some("https://host/"), ""),
            (Some("http://host:8080"), ""),
            (Some("host:8080/prefix"), "/prefix"),
            (Some("https://host/p/?x=1#frag"), "/p"),
            (None, ""),
        ];
        for (input, expected) in cases {
            let got = base_path_from_public_url(&input.map(str::to_string));
            assert_eq!(got, expected, "input: {input:?}");
        }
    }

    #[test]
    fn normalize_base_path_forces_leading_slash_and_no_trailing() {
        assert_eq!(normalize_base_path("/biorouter/"), "/biorouter");
        assert_eq!(normalize_base_path("biorouter"), "/biorouter");
        assert_eq!(normalize_base_path("/"), "");
        assert_eq!(normalize_base_path(""), "");
        assert_eq!(normalize_base_path("/a/b/"), "/a/b");
    }

    const SAMPLE_INDEX: &str = r#"<!doctype html><html><head>
<script type="module" crossorigin src="/assets/index-abc.js"></script>
<link rel="stylesheet" href="/assets/index-def.css">
</head><body><div id="root"></div></body></html>"#;

    #[test]
    fn rewrite_prefixes_assets_and_injects_config() {
        let out = rewrite_index_html(SAMPLE_INDEX, "/biorouter");
        assert!(
            out.contains("src=\"/biorouter/assets/index-abc.js\""),
            "js asset not prefixed: {out}"
        );
        assert!(
            out.contains("href=\"/biorouter/assets/index-def.css\""),
            "css asset not prefixed: {out}"
        );
        assert!(
            out.contains(r#"window.__BIOROUTER_HEADLESS_CONFIG__={"apiBaseUrl":"/biorouter/api","headlessBaseUrl":"/biorouter/headless"}"#),
            "runtime config not injected: {out}"
        );
        // Config must be injected inside <head> so it runs before the module.
        let head_end = out.find("</head>").unwrap();
        let config_at = out.find("__BIOROUTER_HEADLESS_CONFIG__").unwrap();
        assert!(config_at < head_end, "config injected after </head>");
        // No un-prefixed asset URLs remain.
        assert!(
            !out.contains("\"/assets/"),
            "stray root-absolute asset: {out}"
        );
    }

    #[test]
    fn rewrite_is_identity_without_prefix() {
        assert_eq!(rewrite_index_html(SAMPLE_INDEX, ""), SAMPLE_INDEX);
    }
}
