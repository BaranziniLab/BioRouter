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
    about = "Serve Biorouter as a browser-only headless app"
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
        validate_base_path(&base_path).map_err(|e| anyhow!(e))?;
        info!("serving under path prefix {base_path} (assets and API calls are prefixed)");
    }

    let app = router(state, web_dir, base_path);
    let bind_addr: SocketAddr = format!("{}:{}", args.host, args.port)
        .parse()
        .with_context(|| format!("invalid bind address {}:{}", args.host, args.port))?;
    let listener = TcpListener::bind(bind_addr).await?;
    let url = public_url(args.public_url.clone(), args.port).await;
    println!("{url}");
    info!("serving Biorouter headless at {url}");

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
    let index_html = build_index_html(&index_path, &web_dir, &base_path);
    let index_service = get(move || {
        let html = index_html.clone();
        async move { axum::response::Html(html) }
    });
    let mut router = Router::new()
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
        .route("/api/{*path}", any(proxy_api));

    // Vite bakes root-absolute asset URLs into the JS and CSS bundles too (the
    // minified `assetsURL` helper `return"/"+n`, literal `"/assets/…"` image
    // refs, and `url(/assets/…)` font refs in code-split stylesheets), which
    // the shell rewrite cannot reach. Behind a prefix, serve each rewritten
    // *.js / *.css from a dedicated route so those URLs resolve under the
    // prefix; every other asset (fonts, images) falls through to ServeDir
    // unchanged.
    if !base_path.is_empty() {
        for (route_path, body) in rewritten_asset_scripts(&web_dir, &base_path) {
            let body = Arc::new(body);
            router = router.route(
                &route_path,
                get(move || {
                    let body = body.clone();
                    async move { js_response(body.as_str()) }
                }),
            );
        }
        for (route_path, body) in rewritten_asset_stylesheets(&web_dir, &base_path) {
            let body = Arc::new(body);
            router = router.route(
                &route_path,
                get(move || {
                    let body = body.clone();
                    async move { css_response(body.as_str()) }
                }),
            );
        }
    }

    router
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

/// A JavaScript response with the correct module content-type.
fn js_response(body: &str) -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/javascript; charset=utf-8")
        .body(Body::from(body.to_string()))
        .expect("valid js response")
}

/// A stylesheet response with the correct content-type.
fn css_response(body: &str) -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/css; charset=utf-8")
        .body(Body::from(body.to_string()))
        .expect("valid css response")
}

/// Read every `*.js` under `<web_dir>/assets`, rewrite its baked root-absolute
/// asset URLs for the prefix, and return `(route_path, body)` pairs so each can
/// be served from a dedicated route.
fn rewritten_asset_scripts(web_dir: &Path, base_path: &str) -> Vec<(String, String)> {
    let mut scripts = Vec::new();
    let assets_dir = web_dir.join("assets");
    let Ok(entries) = std::fs::read_dir(&assets_dir) else {
        return scripts;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".js") {
            continue;
        }
        if let Ok(raw) = std::fs::read_to_string(entry.path()) {
            scripts.push((format!("/assets/{name}"), rewrite_asset_js(&raw, base_path)));
        }
    }
    scripts
}

/// Rewrite the root-absolute asset URLs Vite bakes into a JS chunk so they
/// resolve under `base_path`: literal `"/assets/…"` / `'/assets/…'` references,
/// and the minified `assetsURL` helper `function(n){return"/"+n}` that
/// `__vitePreload` uses to build modulepreload / code-split-CSS URLs. The helper
/// is matched as an exact literal; a future minifier that renames its parameter
/// won't match, in which case the shell's `vite:preloadError` guard + eager CSS
/// still keep the app rendering (only the modulepreload hint 404s).
fn rewrite_asset_js(raw: &str, base_path: &str) -> String {
    raw.replace("\"/assets/", &format!("\"{base_path}/assets/"))
        .replace("'/assets/", &format!("'{base_path}/assets/"))
        .replace(
            "function(n){return\"/\"+n}",
            &format!("function(n){{return\"{base_path}/\"+n}}"),
        )
}

/// Read every `*.css` under `<web_dir>/assets`, rewrite its baked root-absolute
/// `url(/assets/…)` references (KaTeX fonts and the like) for the prefix, and
/// return `(route_path, body)` pairs so each can be served from a dedicated
/// route.
fn rewritten_asset_stylesheets(web_dir: &Path, base_path: &str) -> Vec<(String, String)> {
    let mut sheets = Vec::new();
    let assets_dir = web_dir.join("assets");
    let Ok(entries) = std::fs::read_dir(&assets_dir) else {
        return sheets;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".css") {
            continue;
        }
        if let Ok(raw) = std::fs::read_to_string(entry.path()) {
            sheets.push((
                format!("/assets/{name}"),
                rewrite_asset_css(&raw, base_path),
            ));
        }
    }
    sheets
}

/// Rewrite the root-absolute `url(/assets/…)` references Vite bakes into a CSS
/// chunk (unquoted, double- and single-quoted forms) so fonts and images
/// resolve under `base_path`. Relative and `data:` URLs are left untouched.
fn rewrite_asset_css(raw: &str, base_path: &str) -> String {
    raw.replace("url(/assets/", &format!("url({base_path}/assets/"))
        .replace("url(\"/assets/", &format!("url(\"{base_path}/assets/"))
        .replace("url('/assets/", &format!("url('{base_path}/assets/"))
}

/// Restrict a derived path prefix to the splice-safe subset of RFC 3986
/// `pchar` (plus `/`) so it can be spliced verbatim into the shell's
/// double-quoted HTML attributes, JS string literals, and CSS `url()` tokens
/// without escaping: ASCII alphanumerics, the unreserved marks `. _ ~ -`,
/// `/`, the inert pchar members `@ : ! $ * + , ; =`, and percent-encoded
/// octets (`%` followed by exactly two ASCII hex digits — a three-character
/// literal browsers never decode while parsing HTML/CSS/JS). The prefix is
/// never percent-decoded: it must round-trip byte-exact so a proxy matching
/// the encoded spelling (e.g. a JupyterHub `%40` username) keeps working.
///
/// Deliberately still rejected because they can break a splice context —
/// `'` (single-quoted JS/CSS splices), `(` `)` (unquoted CSS `url()` token),
/// `&` (HTML character-reference hazard in attribute values), plus quotes,
/// angle brackets, spaces, backslash, and non-ASCII. Spell those
/// percent-encoded instead (`'` → `%27`, `&` → `%26`, …).
///
/// A nonempty prefix must also begin with exactly one `/`. A leading `//`
/// splices into `src="//evil.example/…"` — a scheme-relative (network-path)
/// URL the browser resolves against the *foreign* host, so
/// `--public-url=https://host///evil.example` would point every rewritten
/// script/asset/API URL at an attacker-chosen origin. Embedded `//`
/// sequences are left as-is: past the first byte the spliced URL is already
/// path-absolute (only the first two characters decide network-path form),
/// empty path segments are legal per RFC 3986, and collapsing them would
/// break the byte-exact round-trip a prefix-matching proxy relies on. The
/// `/\` network-path spelling browsers also honour is already rejected by
/// the charset rule (backslash is never allowed).
fn validate_base_path(base_path: &str) -> Result<(), String> {
    if !base_path.is_empty() && !base_path.starts_with('/') {
        return Err(format!(
            "invalid path prefix {base_path:?} derived from --public-url \
             (a nonempty prefix must begin with '/')"
        ));
    }
    if base_path.starts_with("//") {
        return Err(format!(
            "invalid path prefix {base_path:?} derived from --public-url \
             (a prefix starting with \"//\" is a scheme-relative URL that \
             would send asset and API requests to another host; use exactly \
             one leading '/')"
        ));
    }
    let invalid = || {
        format!(
            "invalid path prefix {base_path:?} derived from --public-url \
             (allowed: letters, digits, . _ ~ / - @ : ! $ * + , ; = and %XX \
             percent-escapes with two hex digits; percent-encode anything else, \
             e.g. ' as %27 and & as %26)"
        )
    };
    let bytes = base_path.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let valid_escape = i + 2 < bytes.len()
                    && bytes[i + 1].is_ascii_hexdigit()
                    && bytes[i + 2].is_ascii_hexdigit();
                if !valid_escape {
                    return Err(invalid());
                }
                i += 3;
            }
            b if b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b'.' | b'_'
                        | b'~'
                        | b'/'
                        | b'-'
                        | b'@'
                        | b':'
                        | b'!'
                        | b'$'
                        | b'*'
                        | b'+'
                        | b','
                        | b';'
                        | b'='
                ) =>
            {
                i += 1;
            }
            _ => return Err(invalid()),
        }
    }
    Ok(())
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
/// headless runtime config (API + headless base URLs), a `vite:preloadError`
/// guard, and eager `<link>`s for code-split CSS the shell does not reference.
/// With an empty prefix the file is returned unchanged.
fn build_index_html(index_path: &Path, web_dir: &Path, base_path: &str) -> String {
    let raw = std::fs::read_to_string(index_path).unwrap_or_default();
    let extra_css = if base_path.is_empty() {
        Vec::new()
    } else {
        unreferenced_css(&raw, &web_dir.join("assets"))
    };
    rewrite_index_html(&raw, base_path, &extra_css)
}

/// The `*.css` files under `assets_dir` that the shell HTML does not already
/// reference, sorted for deterministic output. These are Vite's code-split
/// stylesheets: the shell only loads them via a root-absolute preload that 404s
/// behind a path prefix, so they must be eager-loaded to keep styling correct.
fn unreferenced_css(shell_html: &str, assets_dir: &Path) -> Vec<String> {
    let mut css = Vec::new();
    if let Ok(entries) = std::fs::read_dir(assets_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".css") && !shell_html.contains(&name) {
                css.push(name);
            }
        }
    }
    css.sort();
    css
}

/// Pure core of [`build_index_html`]: rewrite asset URLs and inject the runtime
/// config, the `vite:preloadError` guard, and eager CSS `<link>`s in `raw` for
/// the given prefix. Returns `raw` unchanged when the prefix is empty.
fn rewrite_index_html(raw: &str, base_path: &str, extra_css: &[String]) -> String {
    if base_path.is_empty() {
        return raw.to_string();
    }

    // Vite emits root-absolute asset URLs (`src="/assets/…"`, `href="/assets/…"`).
    // Prefix them so they resolve under the proxy path.
    let rewritten = raw
        .replace("\"/assets/", &format!("\"{base_path}/assets/"))
        .replace("'/assets/", &format!("'{base_path}/assets/"));

    // Build the runtime config with serde_json rather than string splicing so the
    // values are always valid, escaped JSON. The renderer reads this before it
    // derives API/headless base URLs from window.location.origin (which drops the
    // path prefix).
    let config = serde_json::json!({
        "apiBaseUrl": format!("{base_path}/api"),
        "headlessBaseUrl": format!("{base_path}/headless"),
    });
    let config_json = serde_json::to_string(&config).unwrap_or_else(|_| "{}".to_string());

    let mut inject = String::new();
    // The JS bundle preloads code-split chunks via baked root-absolute URLs
    // (`/assets/App-*.{js,css}`), which 404 behind a prefix-stripping proxy. A
    // failed preload dispatches `vite:preloadError`; unhandled, __vitePreload
    // rethrows and the lazily-imported App is lost to the ErrorBoundary. This
    // guard (a classic script, so it runs before the deferred module) swallows
    // the error so the app still renders.
    inject.push_str(
        "<script>window.addEventListener('vite:preloadError',function(e){e.preventDefault();});</script>",
    );
    inject.push_str(&format!(
        "<script>window.__BIOROUTER_HEADLESS_CONFIG__={config_json};</script>"
    ));
    // Eager-load code-split CSS the shell does not reference so styling is correct
    // even though its root-absolute preload 404s behind the prefix.
    for css in extra_css {
        inject.push_str(&format!(
            "<link rel=\"stylesheet\" href=\"{base_path}/assets/{css}\">"
        ));
    }

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
            label: "Biorouter config".to_string(),
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
        .header(header::USER_AGENT, "Biorouter")
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
        return Err("Missing manifest.json: not a valid .brxt bundle".to_string());
    }
    if !entries
        .iter()
        .any(|entry| entry.name.eq_ignore_ascii_case("README.md"))
    {
        return Err("Missing README.md: not a valid .brxt bundle".to_string());
    }
    if !entries.iter().any(|entry| entry.name == "pyproject.toml") {
        return Err("Missing pyproject.toml: not a valid .brxt bundle".to_string());
    }
    if !entries.iter().any(|entry| entry.name.starts_with("src/")) {
        return Err("Missing src/ directory: not a valid .brxt bundle".to_string());
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
    use super::{
        base_path_from_public_url, normalize_base_path, rewrite_asset_css, rewrite_asset_js,
        rewrite_index_html, unreferenced_css, validate_base_path,
    };

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
            // Issue #18: both spellings survive derivation byte-exact.
            (Some("http://example.org/url%40prefix/"), "/url%40prefix"),
            (Some("http://example.org/url@prefix/"), "/url@prefix"),
            // A tripled slash derives a scheme-relative prefix byte-exact;
            // `validate_base_path` is the gate that then refuses to start.
            (Some("https://host///evil.example"), "//evil.example"),
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
        let out = rewrite_index_html(SAMPLE_INDEX, "/biorouter", &[]);
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
        assert_eq!(rewrite_index_html(SAMPLE_INDEX, "", &[]), SAMPLE_INDEX);
    }

    #[test]
    fn rewrite_is_identity_without_prefix_ignores_extra_css() {
        // An empty prefix must yield byte-identical output regardless of any
        // discovered code-split CSS.
        let extra = vec!["App-test.css".to_string()];
        assert_eq!(rewrite_index_html(SAMPLE_INDEX, "", &extra), SAMPLE_INDEX);
    }

    #[test]
    fn rewrite_injects_preload_error_guard() {
        let out = rewrite_index_html(SAMPLE_INDEX, "/biorouter", &[]);
        assert!(
            out.contains("vite:preloadError") && out.contains("preventDefault"),
            "preloadError guard not injected: {out}"
        );
        // The guard must precede the deferred module so its listener is registered
        // before __vitePreload runs.
        let guard_at = out.find("vite:preloadError").unwrap();
        let head_end = out.find("</head>").unwrap();
        assert!(guard_at < head_end, "guard injected after </head>");
    }

    #[test]
    fn rewrite_injects_eager_css_links() {
        let extra = vec!["App-test.css".to_string()];
        let out = rewrite_index_html(SAMPLE_INDEX, "/biorouter", &extra);
        assert!(
            out.contains(r#"<link rel="stylesheet" href="/biorouter/assets/App-test.css">"#),
            "eager css link not injected: {out}"
        );
        // Injected inside <head>.
        let css_at = out.find("App-test.css").unwrap();
        let head_end = out.find("</head>").unwrap();
        assert!(css_at < head_end, "eager css injected after </head>");
    }

    #[test]
    fn rewrite_config_is_valid_json() {
        let out = rewrite_index_html(SAMPLE_INDEX, "/biorouter", &[]);
        let json = out
            .split_once("window.__BIOROUTER_HEADLESS_CONFIG__=")
            .and_then(|(_, rest)| rest.split_once(";</script>"))
            .map(|(json, _)| json)
            .expect("config marker present");
        let parsed: serde_json::Value = serde_json::from_str(json).expect("valid JSON");
        assert_eq!(parsed["apiBaseUrl"], "/biorouter/api");
        assert_eq!(parsed["headlessBaseUrl"], "/biorouter/headless");
    }

    #[test]
    fn rewrite_round_trips_percent_encoded_prefix_byte_exact() {
        // Issue #18: a JupyterHub per-user prefix containing %40 must be
        // spliced verbatim (never percent-decoded) into asset URLs, eager CSS
        // links, and the runtime config, and every splice must stay
        // well-formed.
        let prefix = "/user/foo%40bar.org/biorouter";
        let extra = vec!["App-test.css".to_string()];
        let out = rewrite_index_html(SAMPLE_INDEX, prefix, &extra);
        assert!(
            out.contains(r#"src="/user/foo%40bar.org/biorouter/assets/index-abc.js""#),
            "js asset not prefixed byte-exact: {out}"
        );
        assert!(
            out.contains(r#"href="/user/foo%40bar.org/biorouter/assets/index-def.css""#),
            "css asset not prefixed byte-exact: {out}"
        );
        assert!(
            out.contains(
                r#"<link rel="stylesheet" href="/user/foo%40bar.org/biorouter/assets/App-test.css">"#
            ),
            "eager css link not prefixed byte-exact: {out}"
        );
        let json = out
            .split_once("window.__BIOROUTER_HEADLESS_CONFIG__=")
            .and_then(|(_, rest)| rest.split_once(";</script>"))
            .map(|(json, _)| json)
            .expect("config marker present");
        let parsed: serde_json::Value = serde_json::from_str(json).expect("valid JSON");
        assert_eq!(parsed["apiBaseUrl"], "/user/foo%40bar.org/biorouter/api");
        assert_eq!(
            parsed["headlessBaseUrl"],
            "/user/foo%40bar.org/biorouter/headless"
        );
    }

    #[test]
    fn unreferenced_css_skips_shell_referenced_files() {
        let dir = std::env::temp_dir().join(format!("br-headless-css-{}", std::process::id()));
        let assets = dir.join("assets");
        std::fs::create_dir_all(&assets).unwrap();
        std::fs::write(assets.join("index-def.css"), "/* referenced */").unwrap();
        std::fs::write(assets.join("App-test.css"), "/* code-split */").unwrap();
        std::fs::write(assets.join("App-test.js"), "// not css").unwrap();

        let shell = r#"<link rel="stylesheet" href="/assets/index-def.css">"#;
        let found = unreferenced_css(shell, &assets);
        std::fs::remove_dir_all(&dir).ok();

        assert_eq!(found, vec!["App-test.css".to_string()]);
    }

    #[test]
    fn validate_base_path_accepts_safe_charset() {
        assert!(validate_base_path("/biorouter").is_ok());
        assert!(validate_base_path("/a/b-c_d.e~f").is_ok());
        // Empty prefix (no --public-url path) is trivially valid.
        assert!(validate_base_path("").is_ok());
    }

    #[test]
    fn validate_base_path_accepts_pchar_subset() {
        // Issue #18: both reported spellings of a JupyterHub per-user prefix
        // containing an email address must be accepted.
        assert!(validate_base_path("/url@prefix").is_ok());
        assert!(validate_base_path("/url%40prefix").is_ok());
        assert!(validate_base_path("/user/foo@example.org/biorouter").is_ok());
        // Other splice-inert RFC 3986 pchar members.
        assert!(validate_base_path("/a:b").is_ok());
        assert!(validate_base_path("/a!$*+,;=b").is_ok());
        // Percent-escapes stay verbatim three-character literals — including
        // ones that would decode to rejected characters.
        assert!(validate_base_path("/a%20b").is_ok());
        assert!(validate_base_path("/a%2Fb").is_ok());
        assert!(validate_base_path("/a%27b%26c").is_ok());
    }

    #[test]
    fn validate_base_path_accepts_lowercase_hex_escapes() {
        // The escape check uses is_ascii_hexdigit(), so lowercase hex must
        // stay as valid as uppercase and round-trip byte-exact.
        assert!(validate_base_path("/url%2fprefix").is_ok());
        assert!(validate_base_path("/a%c3%a9b").is_ok()); // é, pct-encoded lowercase
        assert!(validate_base_path("/a%ffb").is_ok());
    }

    #[test]
    fn validate_base_path_rejects_unsafe_charset() {
        // A quote or angle bracket would break out of the injected attribute /
        // script context.
        assert!(validate_base_path("/a\"b").is_err());
        assert!(validate_base_path("/a<script>").is_err());
        assert!(validate_base_path("/a b").is_err());
    }

    #[test]
    fn validate_base_path_rejects_splice_breakers() {
        // These pchar/sub-delim members can break a splice context, so they
        // must stay rejected (percent-encode them instead).
        assert!(validate_base_path("/a'b").is_err()); // single-quoted JS/CSS splice
        assert!(validate_base_path("/a(b)").is_err()); // unquoted CSS url() token
                                                       // Isolated close tokens, so a regression in one character alone is
                                                       // caught (above, ')' rides along with '(' and '>' with '<').
        assert!(validate_base_path("/a)b").is_err()); // ')' closes unquoted url(
        assert!(validate_base_path("/a>b").is_err()); // '>' closes an HTML tag
        assert!(validate_base_path("/a&b").is_err()); // HTML char-reference hazard
        assert!(validate_base_path("/a\\b").is_err());
        assert!(validate_base_path("/caf\u{e9}").is_err()); // non-ASCII must be pct-encoded
    }

    #[test]
    fn validate_base_path_rejects_scheme_relative_prefixes() {
        // A leading "//" splices into src="//evil.example/…" — a
        // network-path (scheme-relative) URL the browser resolves against
        // the attacker's host, not this server.
        assert!(validate_base_path("//evil.example").is_err());
        assert!(validate_base_path("///evil.example").is_err());
        assert!(validate_base_path("//evil.example/biorouter").is_err());
        // Defense in depth: a nonempty prefix must begin with '/' at all
        // (normalize_base_path always provides one in production).
        assert!(validate_base_path("biorouter").is_err());
        // An *embedded* "//" is not a network-path reference — the spliced
        // URL is already path-absolute — and stays accepted byte-exact.
        assert!(validate_base_path("/a//b").is_ok());
    }

    #[test]
    fn validate_base_path_rejects_malformed_percent_escapes() {
        assert!(validate_base_path("/a%b").is_err()); // only one trailing char
        assert!(validate_base_path("/a%zzb").is_err()); // not hex digits
        assert!(validate_base_path("/a%").is_err()); // bare trailing percent
        assert!(validate_base_path("/a%4").is_err()); // one hex digit then end
        assert!(validate_base_path("/a%4Gb").is_err()); // second digit not hex
    }

    #[test]
    fn rewrite_asset_js_prefixes_assets_and_helper() {
        // Mirrors the real dist: the minified assetsURL helper plus a literal
        // image reference baked into a code-split chunk.
        let js = r#"const assetsURL=function(n){return"/"+n};img.src="/assets/logo.png";"#;
        let out = rewrite_asset_js(js, "/biorouter");
        assert!(
            out.contains(r#"function(n){return"/biorouter/"+n}"#),
            "assetsURL helper not prefixed: {out}"
        );
        assert!(
            out.contains(r#""/biorouter/assets/logo.png""#),
            "literal asset ref not prefixed: {out}"
        );
        assert!(
            !out.contains(r#"return"/"+n"#),
            "stray root-absolute helper: {out}"
        );
    }

    #[test]
    fn rewrite_asset_js_leaves_relative_imports() {
        // The dynamic App import is relative and already resolves under the
        // prefix; it must not be touched.
        let js = r#"import("./App-CCeH5OUw.js")"#;
        assert_eq!(rewrite_asset_js(js, "/biorouter"), js);
    }

    #[test]
    fn rewrite_asset_css_prefixes_font_urls() {
        // Mirrors the real dist: App-*.css carries dozens of root-absolute
        // KaTeX font references that must resolve under the prefix.
        let css = concat!(
            "@font-face{src:url(/assets/KaTeX_AMS-Regular-BQhdFMY1.woff2) format(\"woff2\"),",
            "url(\"/assets/KaTeX_AMS-Regular-DMm9YOAa.woff\"),",
            "url('/assets/KaTeX_AMS-Regular-DRggAlZN.ttf')}"
        );
        let out = rewrite_asset_css(css, "/biorouter");
        assert!(
            out.contains("url(/biorouter/assets/KaTeX_AMS-Regular-BQhdFMY1.woff2)"),
            "unquoted url() not prefixed: {out}"
        );
        assert!(
            out.contains("url(\"/biorouter/assets/KaTeX_AMS-Regular-DMm9YOAa.woff\")"),
            "double-quoted url() not prefixed: {out}"
        );
        assert!(
            out.contains("url('/biorouter/assets/KaTeX_AMS-Regular-DRggAlZN.ttf')"),
            "single-quoted url() not prefixed: {out}"
        );
        assert!(
            !out.contains("url(/assets/"),
            "stray root-absolute url(): {out}"
        );
    }

    #[test]
    fn rewrite_asset_css_leaves_relative_and_data_urls() {
        let css =
            "a{background:url(data:image/png;base64,AA==) url(./local.png) url(fonts/x.woff)}";
        assert_eq!(rewrite_asset_css(css, "/biorouter"), css);
    }
}
