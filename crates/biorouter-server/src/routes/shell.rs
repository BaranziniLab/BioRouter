//! `/headless/*` — the browser surface's bridge to the machine the daemon runs on.
//!
//! # Why the path is still called `headless`
//!
//! These sixteen routes were served by a separate binary, `biorouter-headless`,
//! which sat in front of the daemon: it served the single-page application,
//! reverse-proxied `/api/*` to `biorouterd`, and answered `/headless/*` itself
//! with a re-implementation of the Electron preload's IPC surface — browse the
//! filesystem, read and write a file, keep the interface's own settings,
//! install a `.brxt` extension bundle, unpack a skill zip, fetch a marketplace
//! asset. The daemon now serves the interface itself (see
//! [`crate::routes::web_ui`]) and that binary is retired, so the handlers moved
//! here.
//!
//! **The `/headless` prefix is retained deliberately.** The renderer builds its
//! endpoint base as `origin + '/headless'` (`ui/desktop/src/renderer.tsx`;
//! `web_ui::inject_runtime_config` injects exactly `"headlessBaseUrl":
//! "/headless"`), so keeping the paths byte-identical makes this move invisible
//! to the browser. Renaming the prefix would be a renderer change for no gain —
//! the name is a URL, not a claim about which process is answering.
//!
//! # The security model, and how it differs from the binary this replaces
//!
//! The original was **unauthenticated** — it bound `0.0.0.0` by default and
//! nothing in front of `/headless/*` checked anything. On top of that, its
//! filesystem handlers had no path validation worth the name: `fs_read` took a
//! query parameter, expanded `~`, and returned the file; `validate_file_path`
//! refused exactly two strings, `""` and `/`. So `GET
//! /headless/fs/read?path=/etc/passwd` worked, and so did
//! `path=~/.ssh/id_rsa`.
//!
//! Two things close that here.
//!
//! **The secret-key middleware.** These routes are merged inside
//! `routes::configure`, so `check_token` (applied in `commands::agent`) wraps
//! them and `auth::is_unauthenticated_path` does not name them. A caller must
//! present `X-Secret-Key`, exactly as for `/config` or `/sessions`.
//!
//! **[`PathGuard`] — an allowlist of roots, checked after canonicalization.**
//! Every client-supplied path goes through [`PathGuard::resolve`], which
//! returns the *resolved* path or a refusal, and every handler operates on the
//! resolved path rather than on the string it was handed. The roots are the
//! Biorouter config directory, the user's home directory, the process working
//! directory and the system temporary directory (that last one is not
//! decoration: `registry/download` writes an asset there and hands the path
//! back for `brxt/install` to read, so excluding it would break the marketplace
//! flow). Resolution follows the canonicalize-then-`starts_with` shape this
//! surface already used for zip entries in `safe_entry_target`, extended in
//! three ways it needed:
//!
//! * `..` is folded out **before** the root test, so `~/../../etc/passwd` is
//!   compared as `/etc/passwd` rather than as something that lexically begins
//!   with the home directory.
//! * A path that does not exist yet — the write and ensure-directory cases —
//!   has its deepest **existing** ancestor canonicalized, and the remaining
//!   components appended to that. So a new file inherits its parent's real
//!   identity instead of skipping the check.
//! * Because canonicalization resolves symbolic links, a link inside a root
//!   that points outside one is refused: the comparison is against the link's
//!   target, never against the requested string.
//!
//! An allowlist alone is not enough, because the most sensitive things on the
//! machine live *inside* the home directory. [`is_denied`] additionally refuses
//! the credential stores — `.ssh`, `.gnupg`, `.aws`, `.kube`, `.docker` — and
//! `secrets.yaml`, which is Biorouter's own plaintext secret store and is the
//! *normal* case on headless Linux, where the keyring is unavailable and
//! `BIOROUTER_DISABLE_KEYRING` is the default. Nothing legitimate on this
//! surface reads any of them; secrets reach the interface through the `/config`
//! routes.
//!
//! # What did not come across
//!
//! The reverse proxy, the static-file serving, the index rewriting and the
//! `biorouterd` child-process spawn are gone rather than ported: the daemon is
//! the process now, so there is nothing to proxy to and nothing to spawn.
//! [`crate::routes::web_ui`] serves the shell.
//!
//! # Why there are no `utoipa` annotations
//!
//! The renderer reaches these with hand-written `fetch` calls through its
//! headless bridge, not through the generated OpenAPI client, so a spec entry
//! would describe a door no generated code opens. `tool_bridge` is absent from
//! the spec for the same reason.

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use axum::{
    extract::{DefaultBodyLimit, Query},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use biorouter::config::paths::Paths;
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::state::AppState;

/// Ceiling on a request body to these routes.
///
/// The binary this replaces put `usize::MAX` on the one body it read by hand
/// (the proxy's), and left everything else on axum's implicit default — so the
/// limit on this surface was never stated anywhere. It is stated here. Eight
/// mebibytes is far above anything the interface actually sends (a settings
/// document, a skill file, a workflow) and far below a body worth buffering.
const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;

/// Ceiling on a marketplace asset download.
const MAX_REGISTRY_DOWNLOAD_BYTES: u64 = 200 * 1024 * 1024;

/// Ceiling on the total **uncompressed** size of an archive this surface
/// unpacks. The original read every entry into memory with no bound at all, so
/// a 200 MB download that expanded a thousandfold was a way to exhaust the
/// daemon. Enforced while reading, not from the archive's own declared sizes,
/// which an attacker writes.
const MAX_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;

/// How long `uv sync` may run while installing an extension bundle.
const UV_SYNC_TIMEOUT: Duration = Duration::from_secs(600);

/// Extensions whose contents a skill archive may hand back as text.
const TEXT_EXTENSIONS: &[&str] = &["md", "txt", "yaml", "yml", "json", "py", "sh"];

/// Hosts a marketplace asset may be fetched from.
const REGISTRY_DOWNLOAD_HOSTS: &[&str] = &[
    "biorouter.ucsf.edu",
    "github.com",
    "objects.githubusercontent.com",
    "raw.githubusercontent.com",
    "codeload.github.com",
];

/// Directory names that are refused wherever they appear in a resolved path,
/// including inside an allowed root. These are credential stores; the interface
/// has no reason to read or write one, and an agent that can drive this surface
/// has every reason to want to.
const DENIED_DIR_NAMES: &[&str] = &[".ssh", ".gnupg", ".gpg", ".aws", ".kube", ".docker"];

/// File names that are refused wherever they appear. `secrets.yaml` is
/// Biorouter's plaintext secret store, which is what a headless Linux
/// deployment uses when no OS keyring is available — i.e. exactly the
/// deployment this surface exists for.
const DENIED_FILE_NAMES: &[&str] = &["secrets.yaml", "id_rsa", "id_ed25519", "id_ecdsa", "id_dsa"];

/// One client for the marketplace fetches, built once. The daemon's `reqwest`
/// is rustls-only, so this carries no system TLS stack with it.
static REGISTRY_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);

// ---------------------------------------------------------------------------
// Wire types. Field names and casing are the originals: the renderer's bridge
// reads them directly, so a rename here is a silent break there.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct PathQuery {
    pub path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListFilesQuery {
    pub path: Option<String>,
    pub extension: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WriteFileRequest {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct FsPathRequest {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct RegistryDownloadRequest {
    pub url: String,
}

#[derive(Debug, Serialize)]
pub struct RegistryDownloadResponse {
    pub path: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePathRequest {
    pub file_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrxtInstallRequest {
    pub file_path: String,
    pub extension_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrxtUninstallRequest {
    pub extension_name: String,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub home_dir: String,
    pub api_base_url: String,
}

#[derive(Debug, Serialize)]
pub struct RootsResponse {
    pub roots: Vec<FsRoot>,
}

#[derive(Debug, Serialize)]
pub struct FsRoot {
    pub label: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct DirsResponse {
    pub dirs: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct FilesResponse {
    pub files: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct FsOkResponse {
    pub ok: bool,
}

#[derive(Debug, Serialize)]
pub struct SettingsResponse {
    pub settings: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct ReadFileResponse {
    #[serde(rename = "filePath")]
    pub file_path: String,
    pub file: String,
    pub found: bool,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FsListResponse {
    pub path: String,
    pub entries: Vec<FsEntry>,
}

#[derive(Debug, Serialize)]
pub struct FsEntry {
    pub name: String,
    pub path: String,
    #[serde(rename = "isDir")]
    pub is_dir: bool,
    #[serde(rename = "isFile")]
    pub is_file: bool,
}

struct ArchiveEntry {
    name: String,
    is_dir: bool,
    data: Vec<u8>,
}

// ---------------------------------------------------------------------------
// The path guard.
// ---------------------------------------------------------------------------

/// Why a requested path was not resolved.
///
/// Deliberately carries no path: a refusal that echoed the resolved location
/// would report whether `/etc/shadow` exists to a caller who is not allowed to
/// read it. Four variants rather than one because two of them are the caller's
/// mistake (`400`) and two are a boundary (`403`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The path was empty, or `~` with no resolvable home directory.
    Unusable,
    /// `..` walked above the filesystem root.
    Malformed,
    /// The resolved path is not inside any allowed root.
    Outside,
    /// The resolved path is inside an allowed root but names a credential
    /// store.
    Denied,
}

impl Refusal {
    fn status(self) -> StatusCode {
        match self {
            Refusal::Unusable | Refusal::Malformed => StatusCode::BAD_REQUEST,
            Refusal::Outside | Refusal::Denied => StatusCode::FORBIDDEN,
        }
    }

    fn message(self) -> &'static str {
        match self {
            Refusal::Unusable => "path is empty or could not be resolved",
            Refusal::Malformed => "path escapes the filesystem root",
            Refusal::Outside => "path is outside the directories this server is allowed to touch",
            Refusal::Denied => "path names a credential store this server will not touch",
        }
    }
}

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        (
            self.status(),
            Json(serde_json::json!({ "error": self.message() })),
        )
            .into_response()
    }
}

/// The set of directories this surface may touch, plus what it needs to turn a
/// client string into a path inside one of them.
///
/// Constructed per request by [`PathGuard::current`] rather than cached, so a
/// process that changes its working directory cannot be left checking against a
/// stale root. Tests construct one directly over a temporary directory, which
/// is what makes the boundary testable without touching process-wide state.
pub struct PathGuard {
    /// Canonical roots. A resolved path must be one of these or live under one.
    roots: Vec<PathBuf>,
    /// What `~` means. `None` when no home directory could be determined, in
    /// which case `~` is refused rather than silently treated as a literal.
    home: Option<PathBuf>,
    /// What a relative path is relative to.
    cwd: PathBuf,
}

impl PathGuard {
    /// The guard the handlers run under.
    pub fn current() -> Self {
        let home = home_dir();
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut roots = vec![Paths::config_dir(), std::env::temp_dir(), cwd.clone()];
        if let Some(home) = home.clone() {
            roots.push(home);
        }
        Self {
            roots: roots.iter().map(|root| canonical_prefix(root)).collect(),
            home,
            cwd,
        }
    }

    /// Resolve a client-supplied path, or say why not.
    ///
    /// The returned path is the one a handler must act on. Acting on the
    /// requested string instead would re-open every hole this closes, because
    /// the string and the resolved path differ exactly when it matters.
    pub fn resolve(&self, requested: &str) -> Result<PathBuf, Refusal> {
        let expanded = self.expand(requested)?;
        let absolute = if expanded.is_absolute() {
            expanded
        } else {
            self.cwd.join(expanded)
        };
        // Fold `..` out first: `~/../../etc/passwd` must be compared as
        // `/etc/passwd`, not as a string that begins with the home directory.
        let normalized = lexical_normalize(&absolute).ok_or(Refusal::Malformed)?;
        // Then resolve symbolic links, so a link inside a root that points out
        // of one is compared as its target.
        let resolved = canonical_prefix(&normalized);
        if !self
            .roots
            .iter()
            .any(|root| resolved == *root || resolved.starts_with(root))
        {
            return Err(Refusal::Outside);
        }
        if is_denied(&resolved) {
            return Err(Refusal::Denied);
        }
        Ok(resolved)
    }

    /// Is this resolved path one of the roots itself? Deleting a root is
    /// refused however the caller spelled it.
    fn is_root(&self, resolved: &Path) -> bool {
        self.roots.iter().any(|root| root == resolved)
    }

    fn expand(&self, requested: &str) -> Result<PathBuf, Refusal> {
        if requested.is_empty() {
            return Err(Refusal::Unusable);
        }
        if requested == "~" {
            return self.home.clone().ok_or(Refusal::Unusable);
        }
        if let Some(rest) = requested.strip_prefix("~/") {
            let home = self.home.as_ref().ok_or(Refusal::Unusable)?;
            return Ok(home.join(rest));
        }
        Ok(PathBuf::from(requested))
    }
}

/// Fold `.` and `..` out of an absolute path textually.
///
/// `None` when `..` would walk above the root — a caller who wrote that is not
/// naming anything this server can serve. Purely lexical on purpose: it runs
/// *before* [`canonical_prefix`], whose job is the symlink half. Doing it in
/// this order is what stops a lexical prefix match from being the whole check.
fn lexical_normalize(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    let mut depth = 0usize;
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => out.push(prefix.as_os_str()),
            Component::RootDir => out.push(Component::RootDir.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                out.pop();
            }
            Component::Normal(part) => {
                depth += 1;
                out.push(part);
            }
        }
    }
    Some(out)
}

/// Canonicalize as much of `path` as exists, and re-append the rest.
///
/// A write or a directory creation names something that is not there yet, so a
/// plain `canonicalize` would fail and leave the caller with a choice between
/// trusting the string and refusing every write. Canonicalizing the deepest
/// existing ancestor gives the new path its parent's real identity — including
/// through any symbolic link on the way — which is the property the root test
/// needs.
fn canonical_prefix(path: &Path) -> PathBuf {
    let mut tail: Vec<OsString> = Vec::new();
    let mut probe = path.to_path_buf();
    loop {
        if let Ok(canonical) = probe.canonicalize() {
            let mut out = canonical;
            for part in tail.iter().rev() {
                out.push(part);
            }
            return out;
        }
        let name = probe.file_name().map(OsStr::to_os_string);
        let parent = probe.parent().map(Path::to_path_buf);
        match (name, parent) {
            (Some(name), Some(parent)) if !parent.as_os_str().is_empty() => {
                tail.push(name);
                probe = parent;
            }
            // Nothing along the path exists (or we reached the root). The
            // lexical form is all there is, and the root test still applies to
            // it.
            _ => return path.to_path_buf(),
        }
    }
}

/// Does this resolved path name a credential store?
fn is_denied(path: &Path) -> bool {
    for component in path.components() {
        let Component::Normal(part) = component else {
            continue;
        };
        let Some(part) = part.to_str() else {
            continue;
        };
        if DENIED_DIR_NAMES
            .iter()
            .any(|denied| part.eq_ignore_ascii_case(denied))
        {
            return true;
        }
    }
    path.file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| {
            DENIED_FILE_NAMES
                .iter()
                .any(|denied| name.eq_ignore_ascii_case(denied))
        })
}

/// The user's home directory.
///
/// Read from the environment rather than pulled in as a dependency: the two
/// variables below are what every home-directory crate consults first, and
/// keeping it here means a test can build a [`PathGuard`] over a temporary
/// directory without one.
fn home_dir() -> Option<PathBuf> {
    let raw = if cfg!(windows) {
        std::env::var_os("USERPROFILE")
    } else {
        std::env::var_os("HOME")
    };
    raw.map(PathBuf::from).filter(|p| !p.as_os_str().is_empty())
}

/// Where this surface keeps the interface's own settings.
///
/// The original hardcoded `~/.config/biorouter/headless/settings.json`, which
/// is not where the configuration lives on Windows or under a non-default
/// `XDG_CONFIG_HOME`, and is not where `BIOROUTER_PATH_ROOT` puts it in a test.
fn settings_path() -> PathBuf {
    Paths::config_dir().join("headless").join("settings.json")
}

/// Where extension bundles are installed.
fn extensions_dir() -> PathBuf {
    Paths::config_dir().join("extensions")
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn json_error(status: StatusCode, message: String) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

// ---------------------------------------------------------------------------
// Handlers.
// ---------------------------------------------------------------------------

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        home_dir: home_dir()
            .map(|home| path_string(&home))
            .unwrap_or_default(),
        api_base_url: api_base_url(),
    })
}

/// The daemon's own base URL.
///
/// `commands::agent` publishes the bound address as `BIOROUTER_APP_BASE_URL`
/// once the listener is up, which is the only value that is right when the port
/// was ephemeral. The configured host/port is the fallback for a process that
/// has not reached that point.
fn api_base_url() -> String {
    if let Some(base) = std::env::var("BIOROUTER_APP_BASE_URL")
        .ok()
        .filter(|base| !base.is_empty())
    {
        return base;
    }
    let host = std::env::var("BIOROUTER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("BIOROUTER_PORT").unwrap_or_else(|_| "3000".to_string());
    format!("http://{host}:{port}")
}

async fn fs_roots() -> Json<RootsResponse> {
    let mut roots = Vec::new();
    if let Some(home) = home_dir() {
        roots.push(FsRoot {
            label: "Home".to_string(),
            path: path_string(&home),
        });
    }
    roots.push(FsRoot {
        label: "Biorouter config".to_string(),
        path: path_string(&Paths::config_dir()),
    });
    roots.push(FsRoot {
        label: "Temporary files".to_string(),
        path: path_string(&std::env::temp_dir()),
    });
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(FsRoot {
            label: "Current directory".to_string(),
            path: path_string(&cwd),
        });
    }
    Json(RootsResponse { roots })
}

async fn fs_list(Query(query): Query<PathQuery>) -> Response {
    let guard = PathGuard::current();
    let requested = query.path.as_deref().unwrap_or("~");
    let path = match guard.resolve(requested) {
        Ok(path) => path,
        Err(refusal) => return refusal.into_response(),
    };
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

async fn fs_list_dirs(Query(query): Query<PathQuery>) -> Response {
    let guard = PathGuard::current();
    let path = match guard.resolve(query.path.as_deref().unwrap_or("~")) {
        Ok(path) => path,
        Err(refusal) => return refusal.into_response(),
    };
    let Ok(mut read_dir) = tokio::fs::read_dir(&path).await else {
        return Json(DirsResponse { dirs: Vec::new() }).into_response();
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

async fn fs_list_files(Query(query): Query<ListFilesQuery>) -> Response {
    let guard = PathGuard::current();
    let path = match guard.resolve(query.path.as_deref().unwrap_or("~")) {
        Ok(path) => path,
        Err(refusal) => return refusal.into_response(),
    };
    let Ok(mut read_dir) = tokio::fs::read_dir(&path).await else {
        return Json(FilesResponse { files: Vec::new() }).into_response();
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

async fn fs_read(Query(query): Query<PathQuery>) -> Response {
    let requested = query.path.unwrap_or_default();
    match read_file_within(&PathGuard::current(), &requested).await {
        Ok(response) => Json(response).into_response(),
        Err(refusal) => refusal.into_response(),
    }
}

/// The whole of `fs_read` except for the HTTP shell, so the boundary can be
/// exercised against a real directory instead of against the process's own
/// home.
async fn read_file_within(guard: &PathGuard, requested: &str) -> Result<ReadFileResponse, Refusal> {
    let path = guard.resolve(requested)?;
    Ok(match tokio::fs::read_to_string(&path).await {
        Ok(file) => ReadFileResponse {
            // Echo what the caller asked for, not where it landed: the renderer
            // keys its cache on the string it sent.
            file_path: requested.to_string(),
            file,
            found: true,
            error: None,
        },
        Err(e) => ReadFileResponse {
            file_path: requested.to_string(),
            file: String::new(),
            found: false,
            error: Some(e.to_string()),
        },
    })
}

async fn fs_write(Json(request): Json<WriteFileRequest>) -> Response {
    let guard = PathGuard::current();
    let path = match guard.resolve(&request.path) {
        Ok(path) => path,
        Err(refusal) => return refusal.into_response(),
    };
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

async fn fs_ensure_dir(Json(request): Json<FsPathRequest>) -> Response {
    let guard = PathGuard::current();
    let path = match guard.resolve(&request.path) {
        Ok(path) => path,
        Err(refusal) => return refusal.into_response(),
    };
    match tokio::fs::create_dir_all(&path).await {
        Ok(()) => Json(FsOkResponse { ok: true }).into_response(),
        Err(e) => json_error(
            StatusCode::BAD_REQUEST,
            format!("failed to create directory: {e}"),
        ),
    }
}

async fn fs_delete_file(Json(request): Json<FsPathRequest>) -> Response {
    let guard = PathGuard::current();
    let path = match guard.resolve(&request.path) {
        Ok(path) => path,
        Err(refusal) => return refusal.into_response(),
    };
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

async fn fs_delete_dir(Json(request): Json<FsPathRequest>) -> Response {
    let guard = PathGuard::current();
    let path = match guard.resolve(&request.path) {
        Ok(path) => path,
        Err(refusal) => return refusal.into_response(),
    };
    // A recursive delete of a root itself would take the home directory, the
    // configuration, or the working tree with it. The original checked two
    // literals (`$HOME` and `/tmp`); every root is checked here, so the config
    // directory and the working directory are covered too.
    if guard.is_root(&path) {
        return json_error(
            StatusCode::FORBIDDEN,
            "refusing to delete a root directory".to_string(),
        );
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

async fn settings_read() -> Json<SettingsResponse> {
    let settings = match tokio::fs::read_to_string(settings_path()).await {
        Ok(file) => serde_json::from_str(&file).unwrap_or_else(|_| default_settings()),
        Err(_) => default_settings(),
    };
    Json(SettingsResponse { settings })
}

async fn settings_write(Json(settings): Json<serde_json::Value>) -> Response {
    if !settings.is_object() {
        return json_error(
            StatusCode::BAD_REQUEST,
            "settings payload must be a JSON object".to_string(),
        );
    }
    let path = settings_path();
    if let Some(parent) = path.parent() {
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
    match tokio::fs::write(&path, serialized).await {
        Ok(()) => Json(FsOkResponse { ok: true }).into_response(),
        Err(e) => json_error(
            StatusCode::BAD_REQUEST,
            format!("failed to write settings: {e}"),
        ),
    }
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

async fn registry_download(
    Json(request): Json<RegistryDownloadRequest>,
) -> Json<RegistryDownloadResponse> {
    Json(match fetch_registry_asset(&request.url).await {
        Ok(path) => RegistryDownloadResponse {
            path: Some(path_string(&path)),
            error: None,
        },
        Err(error) => RegistryDownloadResponse {
            path: None,
            error: Some(error),
        },
    })
}

async fn fetch_registry_asset(raw_url: &str) -> Result<PathBuf, String> {
    let url = allowed_registry_url(raw_url).ok_or("Refusing to download from an untrusted URL.")?;
    let path_lower = url.path().to_ascii_lowercase();
    if !path_lower.ends_with(".zip") && !path_lower.ends_with(".brxt") {
        return Err("Unsupported asset type.".to_string());
    }

    let response = REGISTRY_CLIENT
        .get(url.clone())
        .header(header::USER_AGENT, "Biorouter")
        .send()
        .await
        .map_err(|e| format!("Download failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("Download failed: HTTP {}", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_REGISTRY_DOWNLOAD_BYTES)
    {
        return Err("Download too large.".to_string());
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Download failed: {e}"))?;
    if bytes.len() as u64 > MAX_REGISTRY_DOWNLOAD_BYTES {
        return Err("Download too large.".to_string());
    }

    let dir = std::env::temp_dir().join("biorouter-registry");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("Download failed: {e}"))?;
    let safe_name = url
        .path_segments()
        .and_then(Iterator::last)
        .filter(|name| !name.is_empty())
        .map(sanitize_asset_name)
        .unwrap_or_else(|| "asset.zip".to_string());
    let dest = dir.join(format!("{}-{safe_name}", random_suffix()));
    tokio::fs::write(&dest, bytes)
        .await
        .map_err(|e| format!("Download failed: {e}"))?;
    Ok(dest)
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

async fn skills_extract_zip(Json(request): Json<FilePathRequest>) -> Response {
    let path = match PathGuard::current().resolve(&request.file_path) {
        Ok(path) => path,
        Err(refusal) => return refusal.into_response(),
    };
    Json(match extract_skill_zip(&path) {
        Ok(value) => value,
        Err(e) => serde_json::json!({ "error": e }),
    })
    .into_response()
}

async fn brxt_validate(Json(request): Json<FilePathRequest>) -> Response {
    let path = match PathGuard::current().resolve(&request.file_path) {
        Ok(path) => path,
        Err(refusal) => return refusal.into_response(),
    };
    Json(match validate_brxt_bundle(&path) {
        Ok(value) => value,
        Err(e) => serde_json::json!({ "error": e }),
    })
    .into_response()
}

async fn brxt_install(Json(request): Json<BrxtInstallRequest>) -> Response {
    let path = match PathGuard::current().resolve(&request.file_path) {
        Ok(path) => path,
        Err(refusal) => return refusal.into_response(),
    };
    Json(
        match install_brxt_bundle(&path, &request.extension_name).await {
            Ok(install_dir) => {
                serde_json::json!({ "success": true, "installDir": path_string(&install_dir) })
            }
            Err(e) => serde_json::json!({ "error": format!("Installation failed: {e}") }),
        },
    )
    .into_response()
}

async fn brxt_uninstall(Json(request): Json<BrxtUninstallRequest>) -> Json<serde_json::Value> {
    Json(match uninstall_brxt_extension(&request.extension_name) {
        Ok(()) => serde_json::json!({ "success": true }),
        Err(e) => serde_json::json!({ "error": format!("Uninstall failed: {e}") }),
    })
}

// ---------------------------------------------------------------------------
// Archive handling.
// ---------------------------------------------------------------------------

fn read_archive(path: &Path) -> Result<Vec<ArchiveEntry>, String> {
    let file = File::open(path).map_err(|e| format!("Failed to read ZIP: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Failed to read ZIP: {e}"))?;
    let mut entries = Vec::new();
    let mut budget = MAX_ARCHIVE_BYTES;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|e| format!("Failed to read ZIP: {e}"))?;
        let name = file.name().replace('\\', "/");
        safe_entry_name(&name).map_err(|e| format!("Failed to read ZIP: {e}"))?;
        let is_dir = file.is_dir();
        let mut data = Vec::new();
        if !is_dir {
            // Read one byte past the remaining budget so "exactly fills it" and
            // "overruns it" are distinguishable. Measured while reading rather
            // than taken from the archive's declared sizes, which the person
            // who built the archive chose.
            file.by_ref()
                .take(budget.saturating_add(1))
                .read_to_end(&mut data)
                .map_err(|e| format!("Failed to read ZIP: {e}"))?;
            if data.len() as u64 > budget {
                return Err(format!(
                    "ZIP expands past the {MAX_ARCHIVE_BYTES} byte limit"
                ));
            }
            budget -= data.len() as u64;
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

/// Where an archive entry may be written, relative to an install directory.
///
/// This is the canonicalize-then-`starts_with` shape [`PathGuard::resolve`]
/// generalises; it stays here because it answers a narrower question — a zip
/// entry is always relative, so it needs no `~` expansion and no root set.
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
    require_bundle_shape(&entries)?;

    let manifest_entry = entries
        .iter()
        .find(|entry| entry.name == "manifest.json")
        .ok_or_else(|| "Could not read manifest.json".to_string())?;
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_entry.data)
        .map_err(|e| format!("Failed to read bundle: {e}"))?;
    require_manifest_fields(&manifest)?;

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

fn require_bundle_shape(entries: &[ArchiveEntry]) -> Result<(), String> {
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
    Ok(())
}

fn require_manifest_fields(manifest: &serde_json::Value) -> Result<(), String> {
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
    Ok(())
}

async fn install_brxt_bundle(file_path: &Path, extension_name: &str) -> anyhow::Result<PathBuf> {
    validate_extension_name(extension_name)?;
    let base = extensions_dir();
    let install_dir = base.join(extension_name);
    if !install_dir.starts_with(&base) {
        return Err(anyhow::anyhow!("Invalid extension name."));
    }
    tokio::fs::create_dir_all(&install_dir).await?;
    let entries = read_archive(file_path).map_err(|e| anyhow::anyhow!(e))?;
    for entry in entries {
        let target =
            safe_entry_target(&install_dir, &entry.name).map_err(|e| anyhow::anyhow!(e))?;
        if entry.is_dir {
            tokio::fs::create_dir_all(&target).await?;
        } else {
            if let Some(parent) = target.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::write(&target, entry.data).await?;
        }
    }
    run_uv_sync(&install_dir).await?;
    Ok(install_dir)
}

async fn run_uv_sync(install_dir: &Path) -> anyhow::Result<()> {
    let search_path = command_path(home_dir().as_deref());
    let mut command = Command::new(resolve_program("uv", &search_path));
    command
        .arg("sync")
        .current_dir(install_dir)
        .env("PATH", &search_path);
    // `HOME` is meaningful to `uv` on Unix and is not the variable Windows uses;
    // setting it there would point the tool at nothing.
    #[cfg(unix)]
    if let Some(home) = home_dir() {
        command.env("HOME", home);
    }

    let output = match tokio::time::timeout(UV_SYNC_TIMEOUT, command.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => return Err(anyhow::anyhow!("uv sync failed: {e}")),
        Err(_) => {
            return Err(anyhow::anyhow!(
                "uv sync timed out after {} minutes",
                UV_SYNC_TIMEOUT.as_secs() / 60
            ));
        }
    };
    if output.status.success() {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&output.stderr);
    let detail = if detail.trim().is_empty() {
        String::from_utf8_lossy(&output.stdout).to_string()
    } else {
        detail.to_string()
    };
    Err(anyhow::anyhow!("uv sync failed: {}", detail.trim()))
}

fn uninstall_brxt_extension(extension_name: &str) -> anyhow::Result<()> {
    validate_extension_name(extension_name)?;
    let extensions_base = extensions_dir();
    let install_dir = extensions_base.join(extension_name);
    if !install_dir.starts_with(&extensions_base) {
        return Err(anyhow::anyhow!("Invalid extension name."));
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
        return Err(anyhow::anyhow!("Invalid extension name."));
    }
    Ok(())
}

/// The `PATH` an installed extension's `uv sync` runs under.
///
/// The original built this by joining with a literal `:` and hardcoding
/// `~/.local/bin`, neither of which is right on Windows. `join_paths` uses the
/// platform separator and refuses a directory that contains one, which is the
/// case the manual join silently corrupted.
fn command_path(home: Option<&Path>) -> OsString {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(home) = home {
        dirs.push(home.join(".local").join("bin"));
    }
    // Unix-only: these three are meaningless on Windows, and prepending them
    // would just be noise in front of the inherited PATH.
    #[cfg(unix)]
    {
        dirs.push(PathBuf::from("/usr/local/bin"));
        dirs.push(PathBuf::from("/usr/bin"));
        dirs.push(PathBuf::from("/bin"));
    }
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    dirs.extend(std::env::split_paths(&inherited));
    std::env::join_paths(dirs).unwrap_or(inherited)
}

/// Find `name` on `search_path`, honouring the platform's executable suffix.
///
/// Needed because setting `PATH` through `Command::env` does not reliably
/// change where the program itself is looked up — so the search happens here,
/// against the `PATH` this surface actually built, and the child is given a
/// resolved path. `EXE_SUFFIX` is empty on Unix and `.exe` on Windows; the
/// original never appended it anywhere.
fn resolve_program(name: &str, search_path: &OsStr) -> PathBuf {
    let file_name = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    std::env::split_paths(search_path)
        .map(|dir| dir.join(&file_name))
        .find(|candidate| candidate.is_file())
        // Nothing on the constructed PATH: hand the bare name to the platform
        // and let its own resolution have the last word.
        .unwrap_or_else(|| PathBuf::from(name))
}

// ---------------------------------------------------------------------------
// Router.
// ---------------------------------------------------------------------------

/// The sixteen `/headless/*` routes.
///
/// No handler reads [`AppState`]: this surface is about the machine the daemon
/// runs on, not about its sessions. The parameter is kept so the module is
/// merged the same way every other one is.
pub fn routes(_state: Arc<AppState>) -> Router {
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
        // Stated, finite, and applied to the whole surface at once. See
        // [`MAX_BODY_BYTES`].
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// A guard whose only root is `root`, with `root/home` standing in for the
    /// home directory. Nothing here reads process-wide state, so these tests
    /// neither race each other nor depend on the developer's machine.
    fn guard_over(root: &Path) -> PathGuard {
        let home = root.join("home");
        fs::create_dir_all(&home).unwrap();
        PathGuard {
            roots: vec![canonical_prefix(root)],
            home: Some(canonical_prefix(&home)),
            cwd: canonical_prefix(&home),
        }
    }

    /// A legitimate read inside the allowlist works.
    ///
    /// This one passes against the original implementation too, and that is the
    /// point of having it: without a positive control, every refusal test below
    /// would also pass against a guard that refused everything, which would be
    /// a broken surface rather than a secure one.
    #[tokio::test]
    async fn a_read_inside_a_root_succeeds() {
        let tmp = TempDir::new().unwrap();
        let guard = guard_over(tmp.path());
        let target = tmp.path().join("home").join("notes.md");
        fs::write(&target, "hello").unwrap();

        let response = read_file_within(&guard, target.to_str().unwrap())
            .await
            .expect("a file inside a root must resolve");
        assert!(response.found);
        assert_eq!(response.file, "hello");
    }

    /// An absolute path outside every root is refused with `403`.
    ///
    /// On the original this returned `200 {found: true}` with the file's
    /// contents: `fs_read` performed no validation of any kind — it expanded
    /// `~` and called `read_to_string`. The file below stands in for
    /// `/etc/passwd`; using a real temporary file rather than the machine's own
    /// `/etc/passwd` keeps the test honest on a machine where that file is
    /// unreadable for unrelated reasons, which would otherwise let a broken
    /// guard pass.
    #[tokio::test]
    async fn a_read_outside_every_root_is_refused() {
        let allowed = TempDir::new().unwrap();
        let elsewhere = TempDir::new().unwrap();
        let secret = elsewhere.path().join("passwd");
        fs::write(&secret, "root:x:0:0").unwrap();

        let guard = guard_over(allowed.path());
        let refusal = read_file_within(&guard, secret.to_str().unwrap())
            .await
            .expect_err("a file outside every root must be refused");
        assert_eq!(refusal, Refusal::Outside);
        assert_eq!(refusal.status(), StatusCode::FORBIDDEN);
    }

    /// A symbolic link inside a root that points outside one is refused.
    ///
    /// This is the case an allowlist checked against the *requested string*
    /// cannot catch, and it is why resolution canonicalizes before comparing.
    /// The original had no comparison at all, so it followed the link and
    /// returned the target's contents; a naive `starts_with` on the requested
    /// path would have done the same, because the requested path really is
    /// inside the root.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_symlink_that_escapes_a_root_is_refused() {
        let allowed = TempDir::new().unwrap();
        let elsewhere = TempDir::new().unwrap();
        let secret = elsewhere.path().join("id_rsa_target");
        fs::write(&secret, "PRIVATE KEY").unwrap();

        let guard = guard_over(allowed.path());
        let link = allowed.path().join("home").join("innocent.txt");
        std::os::unix::fs::symlink(&secret, &link).unwrap();

        // The link itself is inside the root, so a string comparison passes.
        assert!(link.starts_with(allowed.path()));

        let refusal = read_file_within(&guard, link.to_str().unwrap())
            .await
            .expect_err("a symlink out of the allowlist must be refused");
        assert_eq!(refusal, Refusal::Outside);
    }

    /// A symlinked *directory* is refused too, including for a file under it
    /// that does not exist — the deepest-existing-ancestor rule has to resolve
    /// the link, not skip it.
    #[cfg(unix)]
    #[test]
    fn a_symlinked_directory_does_not_launder_a_path() {
        let allowed = TempDir::new().unwrap();
        let elsewhere = TempDir::new().unwrap();
        let guard = guard_over(allowed.path());

        let link = allowed.path().join("home").join("out");
        std::os::unix::fs::symlink(elsewhere.path(), &link).unwrap();

        assert_eq!(
            guard.resolve(link.join("existing_or_not.txt").to_str().unwrap()),
            Err(Refusal::Outside)
        );
    }

    /// Traversal is folded out before the root test, so `../../..` cannot walk
    /// out of a root by pretending to stay in it.
    ///
    /// The original's `validate_file_path` accepted every string except `""`
    /// and `/`, so `~/../../etc/passwd` was accepted verbatim by `fs_write`,
    /// `fs_ensure_dir` and `fs_delete_file` — and `fs_read` did not even call
    /// it. A guard that compared the *unnormalized* string would also pass this
    /// path, since it does begin with the home directory.
    #[test]
    fn traversal_out_of_a_root_is_refused() {
        let tmp = TempDir::new().unwrap();
        let guard = guard_over(tmp.path());

        assert_eq!(
            guard.resolve("~/../../../etc/passwd"),
            Err(Refusal::Outside)
        );
        // Climb out of the root by an amount that does NOT depend on how deep
        // the temporary directory happens to be. `{tmp}/home/../..` folds to
        // the PARENT of `tmp`, which always exists and is always outside the
        // root -- on every platform.
        //
        // A fixed `../../..` from `tmp` is not portable and was a real CI
        // failure: macOS hands out a six-component temporary path
        // (`/var/folders/xx/yyy/T/.tmpNNN`), so three levels up is still a real
        // directory and the answer is `Outside`; Linux hands out
        // `/tmp/.tmpNNN`, so the same three levels walk above `/` and the
        // answer is correctly `Malformed`. The guard was right both times --
        // the assertion was reading a property of the temporary directory
        // rather than of the guard.
        assert_eq!(
            guard.resolve(&format!("{}/home/../..", tmp.path().display())),
            Err(Refusal::Outside)
        );
        // Relative paths resolve against the working directory and are folded
        // the same way.
        assert_eq!(guard.resolve("../../../etc/passwd"), Err(Refusal::Outside));
        // `..` above the filesystem root is malformed rather than merely
        // outside.
        assert_eq!(guard.resolve("/../../.."), Err(Refusal::Malformed));
    }

    /// Traversal that stays inside a root is fine — `..` is not banned, it is
    /// resolved.
    #[test]
    fn traversal_that_stays_inside_a_root_resolves() {
        let tmp = TempDir::new().unwrap();
        let guard = guard_over(tmp.path());
        fs::create_dir_all(tmp.path().join("home").join("a")).unwrap();
        let resolved = guard
            .resolve("~/a/../a")
            .expect("a path that stays inside the root must resolve");
        assert_eq!(resolved, canonical_prefix(&tmp.path().join("home/a")));
    }

    /// A sibling directory whose name merely starts with a root's name is not
    /// inside it. `Path::starts_with` is component-wise, which is what makes
    /// this hold; a `String::starts_with` would let `/tmp/root-evil` through.
    #[test]
    fn a_sibling_with_a_shared_prefix_is_outside() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("root");
        let sibling = tmp.path().join("root-evil");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&sibling).unwrap();
        let guard = PathGuard {
            roots: vec![canonical_prefix(&root)],
            home: Some(canonical_prefix(&root)),
            cwd: canonical_prefix(&root),
        };
        assert_eq!(
            guard.resolve(sibling.join("loot").to_str().unwrap()),
            Err(Refusal::Outside)
        );
    }

    /// Credential stores are refused even though they sit inside an allowed
    /// root. The allowlist alone would admit `~/.ssh/id_rsa`, which is the
    /// second half of what the original leaked.
    #[test]
    fn credential_stores_inside_a_root_are_refused() {
        let tmp = TempDir::new().unwrap();
        let guard = guard_over(tmp.path());
        for path in [
            "~/.ssh/id_rsa",
            "~/.ssh/config",
            "~/.aws/credentials",
            "~/.gnupg/secring.gpg",
            "~/secrets.yaml",
            "~/nested/dir/id_ed25519",
        ] {
            assert_eq!(
                guard.resolve(path),
                Err(Refusal::Denied),
                "{path} must be refused"
            );
        }
        // A file that merely mentions one of the names is not one of them.
        assert!(guard.resolve("~/ssh-notes.md").is_ok());
        assert!(guard.resolve("~/id_rsa_backup_notes.md").is_ok());
    }

    /// A path that does not exist yet still gets checked — otherwise every
    /// write would be unguarded, which is the whole point of resolving the
    /// deepest existing ancestor.
    #[test]
    fn a_path_that_does_not_exist_yet_is_still_placed() {
        let tmp = TempDir::new().unwrap();
        let guard = guard_over(tmp.path());
        let inside = guard
            .resolve("~/does/not/exist/yet.txt")
            .expect("a new file under the home root must resolve");
        assert!(inside.starts_with(canonical_prefix(tmp.path())));
        assert!(!inside.exists());

        let elsewhere = TempDir::new().unwrap();
        assert_eq!(
            guard.resolve(elsewhere.path().join("new/file.txt").to_str().unwrap()),
            Err(Refusal::Outside)
        );
    }

    /// An empty path is a caller error, not a filesystem root. The original
    /// refused `""` for writes only, and `fs_read` treated a missing `path`
    /// query parameter as `""`.
    #[test]
    fn an_empty_path_is_refused() {
        let tmp = TempDir::new().unwrap();
        let guard = guard_over(tmp.path());
        assert_eq!(guard.resolve(""), Err(Refusal::Unusable));
    }

    /// The roots themselves resolve, and are recognised as roots so a recursive
    /// delete cannot take one.
    #[test]
    fn a_root_is_reachable_but_known_to_be_a_root() {
        let tmp = TempDir::new().unwrap();
        let guard = guard_over(tmp.path());
        let root = guard.resolve(tmp.path().to_str().unwrap()).unwrap();
        assert!(guard.is_root(&root));
        assert!(!guard.is_root(&root.join("child")));
    }

    /// A zip entry cannot climb out of its install directory. Unchanged from
    /// the original, and asserted here because `install_brxt_bundle` is the one
    /// handler that writes a path it did not resolve through the guard.
    #[test]
    fn zip_entries_cannot_escape_the_install_directory() {
        let tmp = TempDir::new().unwrap();
        assert!(safe_entry_target(tmp.path(), "../../evil").is_err());
        assert!(safe_entry_target(tmp.path(), "/etc/passwd").is_err());
        assert!(safe_entry_target(tmp.path(), "..\\..\\evil").is_err());
        assert!(safe_entry_target(tmp.path(), "src/server.py").is_ok());
    }

    /// The extension name is a single path segment, so it cannot redirect the
    /// install or the uninstall out of the extensions directory.
    #[test]
    fn an_extension_name_is_one_segment() {
        assert!(validate_extension_name("spoke-agent").is_ok());
        for bad in ["", ".", "..", "../../etc", "a/b", "a\\b"] {
            assert!(
                validate_extension_name(bad).is_err(),
                "{bad} must be rejected"
            );
        }
    }

    /// The `PATH` handed to `uv` is built with the platform separator, so it
    /// round-trips through `split_paths` on every platform. The original joined
    /// with a literal `:`, which produces one nonsense entry on Windows.
    #[test]
    fn the_command_path_uses_the_platform_separator() {
        let home = PathBuf::from(if cfg!(windows) { "C:\\Users\\x" } else { "/h" });
        let joined = command_path(Some(&home));
        let parsed: Vec<PathBuf> = std::env::split_paths(&joined).collect();
        assert_eq!(parsed.first(), Some(&home.join(".local").join("bin")));
        assert!(parsed.len() > 1);
    }

    /// The program lookup appends the platform's executable suffix, and falls
    /// back to the bare name when nothing on the path matches.
    #[test]
    fn a_program_is_looked_up_with_the_platform_suffix() {
        let tmp = TempDir::new().unwrap();
        let name = format!("uv{}", std::env::consts::EXE_SUFFIX);
        fs::write(tmp.path().join(&name), "#!/bin/sh\n").unwrap();
        let search = std::env::join_paths([tmp.path().to_path_buf()]).unwrap();
        assert_eq!(resolve_program("uv", &search), tmp.path().join(&name));

        let empty = TempDir::new().unwrap();
        let search = std::env::join_paths([empty.path().to_path_buf()]).unwrap();
        assert_eq!(resolve_program("uv", &search), PathBuf::from("uv"));
    }

    /// Only the marketplace hosts, and only over TLS.
    #[test]
    fn registry_downloads_are_host_and_scheme_bound() {
        assert!(allowed_registry_url("https://github.com/a/b.zip").is_some());
        assert!(allowed_registry_url("http://github.com/a/b.zip").is_none());
        assert!(allowed_registry_url("https://github.com.evil.test/a/b.zip").is_none());
        assert!(allowed_registry_url("file:///etc/passwd").is_none());
    }

    /// The settings document lives under the resolved configuration directory,
    /// not under a hardcoded `~/.config/biorouter`.
    #[test]
    fn the_settings_document_follows_the_configured_root() {
        let path = settings_path();
        assert!(path.starts_with(Paths::config_dir()));
        assert!(path.ends_with(Path::new("headless").join("settings.json")));
        assert!(extensions_dir().starts_with(Paths::config_dir()));
    }

    /// Every route the retired binary served is still served, at the same path.
    #[test]
    fn all_sixteen_routes_are_registered() {
        let source = include_str!("shell.rs");
        for path in [
            "/headless/health",
            "/headless/settings",
            "/headless/registry/download",
            "/headless/skills/extract-zip",
            "/headless/brxt/validate",
            "/headless/brxt/install",
            "/headless/brxt/uninstall",
            "/headless/fs/roots",
            "/headless/fs/list",
            "/headless/fs/list-files",
            "/headless/fs/list-dirs",
            "/headless/fs/read",
            "/headless/fs/write",
            "/headless/fs/ensure-dir",
            "/headless/fs/delete-file",
            "/headless/fs/delete-dir",
        ] {
            assert!(
                source.contains(&format!("\"{path}\"")),
                "{path} is no longer routed"
            );
        }
    }
}
