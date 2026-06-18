//! Managed `llama-server` sidecar process for the Llama Server provider.
//!
//! Biorouter bundles a pinned llama.cpp `llama-server` binary (build
//! [`LLAMA_SERVER_BUILD`]) next to its own binaries. This module owns the
//! lifecycle of that process: locating the binary, spawning it with the
//! requested model (downloaded from Hugging Face via `-hf` on first use),
//! waiting for readiness via `GET /health`, exposing a status snapshot for
//! the GUI/CLI, and restarting when the requested model changes or the
//! process dies.
//!
//! The process is a singleton per Biorouter process ([`global`]): llama-server
//! loads one model at a time, mirroring how Ollama schedules a model per
//! request stream. Switching models restarts the sidecar.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use utoipa::ToSchema;

/// The llama.cpp release this Biorouter version is pinned to. Keep in sync
/// with `ui/desktop/scripts/fetch-llama-server.js`.
pub const LLAMA_SERVER_BUILD: &str = "b9611";
/// Default port the managed sidecar listens on (loopback only).
pub const LLAMACPP_DEFAULT_PORT: u16 = 11543;
const LOG_TAIL_LINES: usize = 60;
const HEALTH_POLL_INTERVAL: Duration = Duration::from_millis(500);
const STATUS_PROBE_TIMEOUT: Duration = Duration::from_millis(800);

/// Lifecycle state of the managed llama-server process.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SidecarState {
    /// No llama-server binary could be located.
    NoBinary,
    /// Binary present but no process running.
    Stopped,
    /// Process running but not serving yet (downloading or loading a model).
    Starting,
    /// `GET /health` returns 200 — ready for completions.
    Ready,
    /// Process exited unexpectedly.
    Error,
}

/// Snapshot of the sidecar consumed by the GUI onboarding card, settings and
/// `biorouter doctor`.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct SidecarStatus {
    pub state: SidecarState,
    /// Friendly model name (catalog name or raw HF spec) currently requested.
    pub model: Option<String>,
    /// Hugging Face `repo:quant` spec backing `model`.
    pub hf_spec: Option<String>,
    pub port: Option<u16>,
    pub binary_path: Option<String>,
    /// Pinned llama.cpp build.
    pub build: String,
    /// Most recent log line (download progress, load stage) or error tail.
    pub detail: Option<String>,
    /// Context window the running model actually exposes (read live from the
    /// server), or `None` until it is ready. Tracks the model, not a preset.
    #[serde(default)]
    pub context_size: Option<usize>,
}

struct Inner {
    child: Option<Child>,
    /// Port the current/last process was started on.
    port: Option<u16>,
    model: Option<String>,
    hf_spec: Option<String>,
    log_tail: Arc<StdMutex<VecDeque<String>>>,
    last_error: Option<String>,
    /// Live context window of the running model, filled once ready.
    context_size: Option<usize>,
}

/// Singleton manager for the llama-server sidecar.
pub struct LlamaSidecar {
    inner: Mutex<Inner>,
}

static SIDECAR: OnceLock<LlamaSidecar> = OnceLock::new();

/// The process-wide sidecar manager.
pub fn global() -> &'static LlamaSidecar {
    SIDECAR.get_or_init(|| LlamaSidecar {
        inner: Mutex::new(Inner {
            child: None,
            port: None,
            model: None,
            hf_spec: None,
            log_tail: Arc::new(StdMutex::new(VecDeque::new())),
            last_error: None,
            context_size: None,
        }),
    })
}

/// Locate a llama-server binary, in priority order:
/// 1. `BIOROUTER_LLAMACPP_BIN` (config param or environment variable)
/// 2. `<dir of current exe>/llamacpp/llama-server` (bundled by the desktop app)
/// 3. `<dir of current exe>/llama-server`
/// 4. `llama-server` on PATH
pub fn find_binary() -> Option<PathBuf> {
    let exe_name = if cfg!(target_os = "windows") {
        "llama-server.exe"
    } else {
        "llama-server"
    };

    let override_path = std::env::var("BIOROUTER_LLAMACPP_BIN").ok().or_else(|| {
        crate::config::Config::global()
            .get_param::<String>("BIOROUTER_LLAMACPP_BIN")
            .ok()
    });
    if let Some(p) = override_path {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for candidate in [
                dir.join("llamacpp").join(exe_name),
                dir.join(exe_name),
                // Dev builds run from target/{debug,release}/ inside the repo;
                // the fetched binary lives in the desktop app's bin dir.
                dir.join("../../ui/desktop/src/bin/llamacpp").join(exe_name),
            ] {
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    which_on_path(exe_name)
}

fn which_on_path(exe_name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths).find_map(|dir| {
        let candidate = dir.join(exe_name);
        candidate.is_file().then_some(candidate)
    })
}

/// Directory used as `LLAMA_CACHE` so downloaded GGUFs live under a
/// Biorouter-owned path rather than the shared `~/.cache/llama.cpp`.
pub fn model_cache_dir() -> PathBuf {
    crate::config::paths::Paths::in_data_dir("llamacpp/models")
}

fn configured_port() -> u16 {
    crate::config::Config::global()
        .get_param::<u16>("LLAMACPP_PORT")
        .unwrap_or(LLAMACPP_DEFAULT_PORT)
}

/// Total physical memory in GiB (best-effort; 0 when it can't be read). On
/// Apple Silicon this is unified memory, which doubles as the GPU/Metal budget,
/// so it's a good proxy for how much KV cache we can afford.
fn total_memory_gib() -> u64 {
    // `sys_info::mem_info().total` is in KiB.
    sys_info::mem_info()
        .map(|m| m.total / (1024 * 1024))
        .unwrap_or(0)
}

/// The default context window when the user hasn't pinned `LLAMACPP_CONTEXT_SIZE`.
///
/// A model's *trained* window (read later from `/props`) can be huge — Qwen3.5
/// is 262k — but the KV cache for it scales with both the window and the model
/// size, and allocating tens of GB at startup is slow-to-impossible on a
/// laptop. So instead of the model's full native window we pick a memory-tiered
/// cap: generous on a workstation, conservative on a 16 GB machine. 128k is the
/// ceiling. Users can still pin any explicit `LLAMACPP_CONTEXT_SIZE`, and the
/// real allocated window is always read back from `/props` for accounting.
pub fn default_context_size() -> usize {
    match total_memory_gib() {
        gib if gib >= 64 => 131_072, // 128k — workstations / Apple Silicon Max/Ultra
        gib if gib >= 32 => 65_536,  // 64k
        gib if gib >= 16 => 32_768,  // 32k — typical 16 GB laptop
        _ => 16_384,                 // 16k — small or unknown
    }
}

/// The `--ctx-size` passed to llama-server. An explicit positive
/// `LLAMACPP_CONTEXT_SIZE` pins the window (and caps memory); otherwise (unset
/// or `0` = "auto") we use a memory-tiered default ([`default_context_size`])
/// rather than the model's full native window, so large models don't hang
/// allocating a giant KV cache on startup. The size the server actually
/// allocates is read back from `/props` ([`live_context_size`]) and reported to
/// token accounting, so the gauge always matches reality. q8_0 KV-cache
/// quantization keeps the memory cost affordable.
fn configured_context_size() -> usize {
    crate::config::Config::global()
        .get_param::<usize>("LLAMACPP_CONTEXT_SIZE")
        .ok()
        .filter(|&n| n > 0)
        .unwrap_or_else(default_context_size)
}

/// Query a running llama-server's `/props` for the context window it actually
/// allocated for the current model. Returns `None` if the server isn't
/// reachable or doesn't report it.
/// Best-effort live context window of the managed sidecar, for callers (e.g.
/// token accounting) that want the running model's real window rather than a
/// fixed default. Returns the value recorded when the server became ready, or
/// probes `/props` directly if a server is up but not yet recorded. `None` when
/// no server is reachable.
pub async fn current_context_size() -> Option<usize> {
    let (recorded, port) = {
        let inner = global().inner.lock().await;
        (inner.context_size, inner.port)
    };
    if recorded.is_some() {
        return recorded;
    }
    // Not recorded by this process. Probe the recorded port if we have one,
    // otherwise the configured port — so an already-running server (started by
    // another Biorouter process, or before this one recorded it) still yields
    // the real window. Keeps the CLI gauge and GUI status consistent instead of
    // falling back to a fixed default.
    live_context_size(port.unwrap_or_else(configured_port)).await
}

async fn live_context_size(port: u16) -> Option<usize> {
    let url = format!("http://127.0.0.1:{port}/props");
    let client = reqwest::Client::builder()
        .timeout(STATUS_PROBE_TIMEOUT)
        .build()
        .ok()?;
    let body: serde_json::Value = client.get(&url).send().await.ok()?.json().await.ok()?;
    // llama-server reports the live window under
    // default_generation_settings.n_ctx; fall back to a top-level n_ctx.
    body.get("default_generation_settings")
        .and_then(|g| g.get("n_ctx"))
        .or_else(|| body.get("n_ctx"))
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
}

/// Build the llama-server argv (without the program itself). Factored out for
/// testing.
fn build_args(hf_spec: &str, alias: &str, port: u16, ctx_size: usize) -> Vec<String> {
    let config = crate::config::Config::global();
    let mut args = vec![
        "-hf".to_string(),
        hf_spec.to_string(),
        "--alias".to_string(),
        alias.to_string(),
        "--host".to_string(),
        "127.0.0.1".to_string(),
        "--port".to_string(),
        port.to_string(),
        "--ctx-size".to_string(),
        ctx_size.to_string(),
        "--jinja".to_string(),
        "--no-webui".to_string(),
        // Halve KV-cache memory so the (memory-tiered) default context stays
        // affordable. q8_0 is considered lossless in practice; it's q4 KV that
        // degrades tool calling. Overridable via LLAMACPP_EXTRA_ARGS.
        "--cache-type-k".to_string(),
        "q8_0".to_string(),
        "--cache-type-v".to_string(),
        "q8_0".to_string(),
    ];
    // Thinking is enabled by default so reasoning-capable models (Qwen3.5,
    // Gemma 4) can reason before answering. Set LLAMACPP_ENABLE_THINKING=false
    // to turn it off (faster, but weaker on multi-step and tool-use tasks).
    // Uses the current `--reasoning on|off` flag, not the deprecated
    // `--chat-template-kwargs {"enable_thinking":...}` form.
    let thinking = config
        .get_param::<bool>("LLAMACPP_ENABLE_THINKING")
        .unwrap_or(true);
    args.push("--reasoning".to_string());
    args.push(if thinking { "on" } else { "off" }.to_string());
    if let Ok(extra) = config.get_param::<String>("LLAMACPP_EXTRA_ARGS") {
        args.extend(sanitize_extra_args(&extra));
    }
    // Belt-and-suspenders: re-assert the loopback bind as the LAST `--host`/
    // `--port` on the line. llama-server's arg parser honors the last value, so
    // even if a dangerous flag slipped past the filter above it cannot move the
    // server off 127.0.0.1 or onto a different port.
    args.push("--host".to_string());
    args.push("127.0.0.1".to_string());
    args.push("--port".to_string());
    args.push(port.to_string());
    args
}

/// Flags a user must not be able to inject via `LLAMACPP_EXTRA_ARGS`: anything
/// that changes the network bind or the server's auth/file-serving posture. The
/// managed sidecar is a loopback-only, no-auth server *by design* (the only
/// access control is the 127.0.0.1 bind), so letting config re-bind it to
/// `0.0.0.0` would silently expose an unauthenticated inference server on the
/// LAN. `--port` is owned by `LLAMACPP_PORT`; the rest change file serving.
const FORBIDDEN_EXTRA_FLAGS: &[&str] = &[
    "--host",
    "--port",
    "--api-key",
    "--api-key-file",
    "--path",
    "--rpc",
];

/// Whitespace-split `LLAMACPP_EXTRA_ARGS` and drop any forbidden flag together
/// with the token that follows it (its value). Emits a warning so the dropped
/// flag is visible in logs rather than silently ignored.
fn sanitize_extra_args(extra: &str) -> Vec<String> {
    let tokens: Vec<&str> = extra.split_whitespace().collect();
    let mut out = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        let tok = tokens[i];
        // Match `--flag` and `--flag=value` forms.
        let flag_name = tok.split('=').next().unwrap_or(tok);
        if FORBIDDEN_EXTRA_FLAGS.contains(&flag_name) {
            tracing::warn!(
                "Ignoring forbidden flag '{tok}' in LLAMACPP_EXTRA_ARGS: the managed \
                 llama-server is loopback-only and cannot be re-bound or have its auth/path changed"
            );
            // Skip a following value token if this was the separate-arg form
            // (`--host 0.0.0.0`) rather than the `--host=0.0.0.0` form.
            if !tok.contains('=') && i + 1 < tokens.len() && !tokens[i + 1].starts_with('-') {
                i += 1;
            }
            i += 1;
            continue;
        }
        out.push(tok.to_string());
        i += 1;
    }
    out
}

async fn health_ok(port: u16, timeout: Duration) -> bool {
    let url = format!("http://127.0.0.1:{port}/health");
    let client = match reqwest::Client::builder().timeout(timeout).build() {
        Ok(c) => c,
        Err(_) => return false,
    };
    matches!(client.get(&url).send().await, Ok(r) if r.status().is_success())
}

fn port_in_use(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_err()
}

// ── orphan reaping ──────────────────────────────────────────────────────────
// `kill_on_drop` only covers drops inside a live tokio runtime; the manager
// lives in a static, so a normal process exit (or SIGTERM/SIGKILL) leaks the
// llama-server child — with a multi-GB model held in memory. Each spawn is
// therefore recorded as `<parent-pid>.pid -> <child-pid>` under a run dir,
// and the next `ensure()` in any Biorouter process kills children whose
// parent is gone. Pidfiles of still-living parents (e.g. the CLI and the
// desktop running side by side) are left alone.

fn run_dir() -> PathBuf {
    crate::config::paths::Paths::in_data_dir("llamacpp/run")
}

fn pid_alive(pid: u32) -> bool {
    let pid = pid.to_string();
    if cfg!(target_os = "windows") {
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&format!("\"{pid}\"")))
            .unwrap_or(false)
    } else {
        std::process::Command::new("kill")
            .args(["-0", &pid])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

/// Guards against PID reuse: only kill a recorded pid if it still looks like
/// a llama-server process.
fn pid_is_llama_server(pid: u32) -> bool {
    let pid = pid.to_string();
    if cfg!(target_os = "windows") {
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("llama-server"))
            .unwrap_or(false)
    } else {
        std::process::Command::new("ps")
            .args(["-p", &pid, "-o", "comm="])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("llama-server"))
            .unwrap_or(false)
    }
}

fn kill_pid(pid: u32) {
    let pid = pid.to_string();
    if cfg!(target_os = "windows") {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid, "/F"])
            .output();
    } else {
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid])
            .output();
    }
}

fn pidfile_path() -> PathBuf {
    run_dir().join(format!("{}.pid", std::process::id()))
}

fn write_pidfile(child_pid: u32) {
    let dir = run_dir();
    if std::fs::create_dir_all(&dir).is_ok() {
        let _ = std::fs::write(pidfile_path(), child_pid.to_string());
    }
}

fn clear_pidfile() {
    let _ = std::fs::remove_file(pidfile_path());
}

/// Kill llama-server children recorded by Biorouter processes that no longer
/// exist. Returns true when anything was killed (callers may want to give
/// the OS a moment to release the port).
fn reap_orphans() -> bool {
    let mut killed = false;
    let Ok(entries) = std::fs::read_dir(run_dir()) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let parent: Option<u32> = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse().ok());
        let child: Option<u32> = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| s.trim().parse().ok());
        let (Some(parent), Some(child)) = (parent, child) else {
            let _ = std::fs::remove_file(&path);
            continue;
        };
        if parent == std::process::id() || pid_alive(parent) {
            continue;
        }
        if pid_alive(child) && pid_is_llama_server(child) {
            tracing::info!(
                "Reaping orphaned llama-server (pid {child}) left by exited Biorouter process {parent}"
            );
            kill_pid(child);
            killed = true;
        }
        let _ = std::fs::remove_file(&path);
    }
    killed
}

fn free_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}

fn push_log(tail: &StdMutex<VecDeque<String>>, line: String) {
    if let Ok(mut t) = tail.lock() {
        if t.len() >= LOG_TAIL_LINES {
            t.pop_front();
        }
        t.push_back(line);
    }
}

fn last_log(tail: &StdMutex<VecDeque<String>>) -> Option<String> {
    tail.lock().ok().and_then(|t| t.back().cloned())
}

fn log_tail_joined(tail: &StdMutex<VecDeque<String>>, n: usize) -> String {
    tail.lock()
        .map(|t| {
            t.iter()
                .rev()
                .take(n)
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

impl LlamaSidecar {
    /// Ensure a llama-server is running and serving `model` (friendly alias)
    /// backed by `hf_spec`. Returns the port it is (or will be) serving on.
    /// Does NOT wait for readiness — pair with [`Self::wait_ready`].
    pub async fn ensure(&self, model: &str, hf_spec: &str) -> Result<u16> {
        let mut inner = self.inner.lock().await;

        // Already running the requested model?
        if inner.model.as_deref() == Some(model) {
            if let Some(child) = inner.child.as_mut() {
                if child.try_wait()?.is_none() {
                    return Ok(inner.port.expect("running child always has a port"));
                }
                // Process died — fall through to respawn.
                inner.last_error = Some(log_tail_joined(&inner.log_tail, 8));
                inner.child = None;
            }
        }

        // A different model (or nothing) is running: stop what we own.
        if let Some(mut child) = inner.child.take() {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }

        // Clean up llama-servers leaked by Biorouter processes that died
        // without dropping their child (statics are never dropped on exit).
        if reap_orphans() {
            tokio::time::sleep(Duration::from_millis(300)).await;
        }

        // Pick a port. If something else (an unrelated service, or another
        // live Biorouter process's sidecar) already listens on the configured
        // port, leave it alone and use an ephemeral one — never adopt a
        // process we did not spawn, since its launch flags may not match
        // this version's.
        let preferred = configured_port();
        let port = if port_in_use(preferred) {
            tracing::warn!(
                "Port {preferred} is already in use; starting llama-server on an ephemeral port"
            );
            free_port()?
        } else {
            preferred
        };

        let binary = find_binary().ok_or_else(|| {
            anyhow!(
                "llama-server binary not found. It ships with the Biorouter desktop app; for \
                 CLI-only installs, install llama.cpp (e.g. `brew install llama.cpp`) or set \
                 BIOROUTER_LLAMACPP_BIN to a llama-server path."
            )
        })?;

        let cache_dir = model_cache_dir();
        std::fs::create_dir_all(&cache_dir)?;

        let ctx_size = configured_context_size();
        let args = build_args(hf_spec, model, port, ctx_size);
        tracing::info!(
            "Starting llama-server ({}) on port {port}: {} {}",
            LLAMA_SERVER_BUILD,
            binary.display(),
            args.join(" ")
        );

        if let Ok(mut t) = inner.log_tail.lock() {
            t.clear();
        }
        inner.last_error = None;

        let mut child = Command::new(&binary)
            .args(&args)
            .env("LLAMA_CACHE", &cache_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| anyhow!("Failed to start llama-server at {}: {e}", binary.display()))?;

        // Stream child output into the rolling log tail (download progress,
        // model load stages) so status polling can surface it.
        if let Some(stdout) = child.stdout.take() {
            let tail = inner.log_tail.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if !line.trim().is_empty() {
                        push_log(&tail, line);
                    }
                }
            });
        }
        if let Some(stderr) = child.stderr.take() {
            let tail = inner.log_tail.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if !line.trim().is_empty() {
                        push_log(&tail, line);
                    }
                }
            });
        }

        if let Some(child_pid) = child.id() {
            write_pidfile(child_pid);
        }
        inner.child = Some(child);
        inner.port = Some(port);
        inner.model = Some(model.to_string());
        inner.hf_spec = Some(hf_spec.to_string());
        // New model: the previous model's window no longer applies; it is
        // re-read from /props once this one is ready.
        inner.context_size = None;
        Ok(port)
    }

    /// Wait until `GET /health` succeeds or `timeout` elapses. The first run
    /// of a model includes the Hugging Face download, so generous timeouts
    /// are expected here.
    pub async fn wait_ready(&self, timeout: Duration) -> Result<u16> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let (port, dead, tail) = {
                let mut inner = self.inner.lock().await;
                let port = inner
                    .port
                    .ok_or_else(|| anyhow!("llama-server is not running"))?;
                let dead = match inner.child.as_mut() {
                    Some(child) => child.try_wait()?.is_some(),
                    None => true,
                };
                (port, dead, inner.log_tail.clone())
            };

            if dead {
                let tail = log_tail_joined(&tail, 10);
                return Err(anyhow!("llama-server exited during startup:\n{tail}"));
            }
            if health_ok(port, STATUS_PROBE_TIMEOUT).await {
                // Record the live context window now that the model is loaded,
                // so callers report the model's real window, not a preset.
                if let Some(ctx) = live_context_size(port).await {
                    self.inner.lock().await.context_size = Some(ctx);
                }
                return Ok(port);
            }
            if tokio::time::Instant::now() >= deadline {
                let last = last_log(&tail).unwrap_or_default();
                return Err(anyhow!(
                    "llama-server did not become ready within {}s (last output: {last})",
                    timeout.as_secs()
                ));
            }
            tokio::time::sleep(HEALTH_POLL_INTERVAL).await;
        }
    }

    /// Non-blocking-ish status snapshot for UIs.
    pub async fn status(&self) -> SidecarStatus {
        let binary = find_binary();
        let mut inner = self.inner.lock().await;

        let mut status = SidecarStatus {
            state: SidecarState::Stopped,
            model: inner.model.clone(),
            hf_spec: inner.hf_spec.clone(),
            port: inner.port,
            binary_path: binary.as_ref().map(|p| p.display().to_string()),
            build: LLAMA_SERVER_BUILD.to_string(),
            detail: None,
            context_size: inner.context_size,
        };

        if binary.is_none() {
            status.state = SidecarState::NoBinary;
            return status;
        }

        let running = match inner.child.as_mut() {
            Some(child) => match child.try_wait() {
                Ok(None) => true,
                _ => {
                    status.state = SidecarState::Error;
                    status.detail = Some(
                        inner
                            .last_error
                            .clone()
                            .unwrap_or_else(|| log_tail_joined(&inner.log_tail, 8)),
                    );
                    return status;
                }
            },
            None => false,
        };

        if !running {
            // No process managed by this instance. A server may still be
            // running on the configured port — started by another Biorouter
            // process and not yet adopted here. Probe it so callers (the GUI's
            // context gauge, token accounting) see a real, ready window instead
            // of a "stopped"/default fallback. We don't take ownership of the
            // child; we just report what's reachable.
            let probe_port = inner.port.unwrap_or_else(configured_port);
            if health_ok(probe_port, STATUS_PROBE_TIMEOUT).await {
                status.state = SidecarState::Ready;
                status.port = Some(probe_port);
                if let Some(ctx) = live_context_size(probe_port).await {
                    inner.context_size = Some(ctx);
                    status.context_size = Some(ctx);
                }
            }
            return status;
        }

        let port = inner.port.expect("running sidecar always has a port");
        if health_ok(port, STATUS_PROBE_TIMEOUT).await {
            status.state = SidecarState::Ready;
            // Fill the live window if we haven't yet (e.g. an adopted server).
            if status.context_size.is_none() {
                if let Some(ctx) = live_context_size(port).await {
                    inner.context_size = Some(ctx);
                    status.context_size = Some(ctx);
                }
            }
        } else {
            status.state = SidecarState::Starting;
            status.detail = last_log(&inner.log_tail);
        }
        status
    }

    /// Stop the managed process (no-op when nothing is running or adopted).
    pub async fn stop(&self) {
        let mut inner = self.inner.lock().await;
        if let Some(mut child) = inner.child.take() {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
        clear_pidfile();
        inner.model = None;
        inner.hf_spec = None;
        inner.port = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_context_size_is_memory_tiered_and_capped() {
        // Whatever this machine reports, the default must be one of the tiers,
        // never exceed the 128k cap, and never be absurdly small.
        let d = default_context_size();
        assert!(
            [16_384usize, 32_768, 65_536, 131_072].contains(&d),
            "unexpected tier: {d}"
        );
        assert!(d <= 131_072, "must not exceed the 128k cap: {d}");
        assert!(d >= 16_384, "must stay usable: {d}");
        // The tier must agree with the measured memory (guards the thresholds).
        let gib = total_memory_gib();
        let expected = if gib >= 64 {
            131_072
        } else if gib >= 32 {
            65_536
        } else if gib >= 16 {
            32_768
        } else {
            16_384
        };
        assert_eq!(d, expected, "tier disagrees with {gib} GiB");
    }

    #[test]
    fn build_args_includes_model_port_and_jinja() {
        // ctx_size 0 means "use the model's own trained context".
        let args = build_args("unsloth/Qwen3.5-4B-GGUF:Q4_K_M", "qwen3.5-4b", 12345, 0);
        let joined = args.join(" ");
        assert!(joined.contains("-hf unsloth/Qwen3.5-4B-GGUF:Q4_K_M"));
        assert!(joined.contains("--alias qwen3.5-4b"));
        assert!(joined.contains("--port 12345"));
        // Model-native context window (tracks the model, not a fixed preset).
        assert!(joined.contains("--ctx-size 0"));
        assert!(joined.contains("--cache-type-k q8_0"));
        assert!(joined.contains("--cache-type-v q8_0"));
        assert!(joined.contains("--jinja"));
        assert!(joined.contains("--host 127.0.0.1"));
        assert!(joined.contains("--no-webui"));
        // Thinking is enabled by default via the current --reasoning flag.
        assert!(joined.contains("--reasoning on"));
        assert!(!joined.contains("enable_thinking"));
    }

    #[test]
    fn build_args_explicit_ctx_size_is_passed_through() {
        let args = build_args("owner/repo:Q4_K_M", "m", 11543, 16384);
        assert!(args.join(" ").contains("--ctx-size 16384"));
    }

    #[test]
    fn build_args_reasoning_off_when_disabled() {
        let _guard = env_lock::lock_env([("LLAMACPP_ENABLE_THINKING", Some("false"))]);
        let args = build_args("owner/repo:Q4_K_M", "m", 11543, 0);
        assert!(args.join(" ").contains("--reasoning off"));
    }

    #[test]
    fn sanitize_extra_args_drops_forbidden_flags() {
        // Separate-arg form: flag and its value are both dropped.
        assert_eq!(
            sanitize_extra_args("--host 0.0.0.0 --threads 8"),
            vec!["--threads", "8"]
        );
        // `--flag=value` form is dropped (no following value to skip).
        assert_eq!(
            sanitize_extra_args("--host=0.0.0.0 --threads 8"),
            vec!["--threads", "8"]
        );
        // api-key / path / rpc are also forbidden.
        assert_eq!(
            sanitize_extra_args("--api-key secret"),
            Vec::<String>::new()
        );
        assert_eq!(
            sanitize_extra_args("--rpc 1.2.3.4:50052"),
            Vec::<String>::new()
        );
        // Benign flags pass through untouched.
        assert_eq!(
            sanitize_extra_args("--flash-attn --parallel 4"),
            vec!["--flash-attn", "--parallel", "4"]
        );
    }

    #[test]
    fn build_args_reasserts_loopback_last_even_with_injected_host() {
        let _guard =
            env_lock::lock_env([("LLAMACPP_EXTRA_ARGS", Some("--host 0.0.0.0 --port 9999"))]);
        let args = build_args("owner/repo:Q4_K_M", "m", 11543, 32768);
        // The injected 0.0.0.0 must not survive, and the final --host/--port
        // (last-wins for llama-server) must be the loopback bind on our port.
        assert!(!args.iter().any(|a| a == "0.0.0.0"));
        let last_host = args.iter().rposition(|a| a == "--host").unwrap();
        assert_eq!(args[last_host + 1], "127.0.0.1");
        let last_port = args.iter().rposition(|a| a == "--port").unwrap();
        assert_eq!(args[last_port + 1], "11543");
    }

    #[test]
    fn find_binary_env_override() {
        let _guard =
            env_lock::lock_env([("BIOROUTER_LLAMACPP_BIN", Some("/nonexistent/llama-server"))]);
        // Nonexistent override is skipped, not an error.
        let _ = find_binary();
    }

    #[test]
    fn free_port_is_nonzero() {
        assert!(free_port().unwrap() > 0);
    }
}
