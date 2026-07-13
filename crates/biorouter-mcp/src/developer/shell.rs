use std::{env, ffi::OsString, process::Stdio, sync::Once};

use biorouter_sandbox::shell_sandbox::{
    self, SandboxMode, SandboxPolicy, SandboxReport, SandboxTier,
};

#[cfg(unix)]
#[allow(unused_imports)] // False positive: trait is used for process_group method
use std::os::unix::process::CommandExt;

#[derive(Debug, Clone)]
pub struct ShellConfig {
    pub executable: String,
    pub args: Vec<String>,
    #[allow(dead_code)]
    pub envs: Vec<(OsString, OsString)>,
}

impl Default for ShellConfig {
    fn default() -> Self {
        #[cfg(windows)]
        {
            Self::detect_windows_shell()
        }
        #[cfg(not(windows))]
        {
            let shell = env::var("SHELL").unwrap_or_else(|_| "bash".to_string());
            Self {
                executable: shell,
                args: vec!["-c".to_string()], // -c is standard across bash/zsh/fish
                envs: vec![],
            }
        }
    }
}

impl ShellConfig {
    #[cfg(windows)]
    fn detect_windows_shell() -> Self {
        // Check for PowerShell first (more modern)
        if let Ok(ps_path) = which::which("pwsh") {
            // PowerShell 7+ (cross-platform PowerShell)
            Self {
                executable: ps_path.to_string_lossy().to_string(),
                args: vec![
                    "-NoProfile".to_string(),
                    "-NonInteractive".to_string(),
                    "-Command".to_string(),
                ],
                envs: vec![],
            }
        } else if let Ok(ps_path) = which::which("powershell") {
            // Windows PowerShell 5.1
            Self {
                executable: ps_path.to_string_lossy().to_string(),
                args: vec![
                    "-NoProfile".to_string(),
                    "-NonInteractive".to_string(),
                    "-Command".to_string(),
                ],
                envs: vec![],
            }
        } else {
            // Fall back to cmd.exe
            Self {
                executable: "cmd".to_string(),
                args: vec!["/c".to_string()],
                envs: vec![],
            }
        }
    }
}

pub fn expand_path(path_str: &str) -> String {
    if cfg!(windows) {
        // Expand Windows environment variables (%VAR%)
        let with_userprofile = path_str.replace(
            "%USERPROFILE%",
            &env::var("USERPROFILE").unwrap_or_default(),
        );
        // Add more Windows environment variables as needed
        with_userprofile.replace("%APPDATA%", &env::var("APPDATA").unwrap_or_default())
    } else {
        // Unix-style expansion
        shellexpand::tilde(path_str).into_owned()
    }
}

pub fn is_absolute_path(path_str: &str) -> bool {
    if cfg!(windows) {
        // Check for Windows absolute paths (drive letters and UNC)
        path_str.contains(":\\") || path_str.starts_with("\\\\")
    } else {
        // Unix absolute paths start with /
        path_str.starts_with('/')
    }
}

pub fn normalize_line_endings(text: &str) -> String {
    if cfg!(windows) {
        // Ensure CRLF line endings on Windows
        text.replace("\r\n", "\n").replace("\n", "\r\n")
    } else {
        // Ensure LF line endings on Unix
        text.replace("\r\n", "\n")
    }
}

/// Parse a plain truthy env value (`1`/`true`/`on`/`yes`, case-insensitive,
/// trimmed). Used for `BIOROUTER_SHELL_SANDBOX_NETWORK`; the main gate
/// `BIOROUTER_SHELL_SANDBOX` uses the three-valued [`SandboxMode`] instead.
pub(crate) fn env_flag_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "on" | "yes"
    )
}

fn env_truthy(name: &str) -> bool {
    env::var(name).map(|v| env_flag_truthy(&v)).unwrap_or(false)
}

/// Build the mechanism-neutral [`SandboxPolicy`] for the shell tool: writable
/// roots are the session working dir (the natural project root) plus the process
/// temp dir; everything else on the filesystem is read-only; outbound network is
/// denied unless `BIOROUTER_SHELL_SANDBOX_NETWORK` is truthy. Unchanged from
/// BR-64.
fn build_sandbox_policy(working_dir: Option<&std::path::Path>) -> SandboxPolicy {
    let mut roots = Vec::new();
    if let Some(dir) = working_dir {
        roots.push(dir.to_path_buf());
    }
    roots.push(env::temp_dir());
    SandboxPolicy::new(roots).with_network(env_truthy("BIOROUTER_SHELL_SANDBOX_NETWORK"))
}

/// Warn (once per process) that the sandbox was requested but is unavailable, so
/// a fleet admin sees one loud line instead of a silent no-op buried in the log.
fn warn_sandbox_unavailable_once(report: &SandboxReport) {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        tracing::warn!(
            "BIOROUTER_SHELL_SANDBOX=auto but no full OS sandbox is available on this host \
             ({}); running the shell tool unsandboxed. Set BIOROUTER_SHELL_SANDBOX=strict to \
             refuse instead.",
            report.summary()
        );
    });
}

/// Decide how to wrap the shell `program` under the requested [`SandboxMode`]
/// (BR-69). Returns:
/// - `Ok(None)` — run `program` directly (mode off, or `auto` on a host with no
///   full sandbox: warn once and fall open, exactly BR-64's opt-in behaviour).
/// - `Ok(Some((program, prefix_args)))` — run the wrapped program (Seatbelt on
///   macOS, Landlock/seccomp helper or bubblewrap on Linux).
/// - `Err(msg)` — `strict` mode on a host that cannot reach [`SandboxTier::Full`]:
///   the shell tool surfaces this as a tool error and refuses to run, rather
///   than the false assurance of silently degrading to unsandboxed.
fn shell_sandbox_wrap(
    program: &str,
    working_dir: Option<&std::path::Path>,
) -> Result<Option<(String, Vec<String>)>, String> {
    let mode = SandboxMode::from_env();
    if mode == SandboxMode::Off {
        return Ok(None);
    }

    let backend = shell_sandbox::detect();
    let report = backend.probe();

    if report.tier == SandboxTier::None {
        return match mode {
            SandboxMode::Strict => Err(format!(
                "BIOROUTER_SHELL_SANDBOX=strict but no OS sandbox is available: {}. Refusing to \
                 run. Set BIOROUTER_SHELL_SANDBOX=auto to run unsandboxed, or install/enable a \
                 sandbox.",
                report.summary()
            )),
            _ => {
                warn_sandbox_unavailable_once(&report);
                Ok(None)
            }
        };
    }

    // `strict` requires the top tier (writes confined AND network denied). A
    // partial tier (WriteOnly / ContainmentOnly) does not satisfy it — network
    // egress is the exfiltration channel that matters for prompt injection.
    if mode == SandboxMode::Strict && report.tier != SandboxTier::Full {
        return Err(format!(
            "BIOROUTER_SHELL_SANDBOX=strict requires a full sandbox but this host only offers \
             {}. Refusing to run.",
            report.summary()
        ));
    }

    let policy = build_sandbox_policy(working_dir);
    match backend.wrap(&policy, program) {
        Ok(w) => Ok(Some((w.program, w.prefix_args))),
        Err(e) => match mode {
            SandboxMode::Strict => Err(format!(
                "BIOROUTER_SHELL_SANDBOX=strict but the sandbox wrapper could not be built: {e}. \
                 Refusing to run."
            )),
            _ => {
                warn_sandbox_unavailable_once(&report);
                Ok(None)
            }
        },
    }
}

/// The assistant-visible one-line sandbox status prepended to shell output when
/// the gate is on, e.g. `[sandbox: landlock+seccomp — writes confined + network
/// denied]`. `None` when the gate is off (byte-for-byte no change to output).
/// The model sees it, so it can reason about a denial instead of retrying
/// blindly, and it is what a user screenshots in a bug report.
pub fn shell_sandbox_status_line(working_dir: Option<&std::path::Path>) -> Option<String> {
    let mode = SandboxMode::from_env();
    if mode == SandboxMode::Off {
        return None;
    }
    let report = shell_sandbox::detect().probe();
    if report.tier == SandboxTier::None {
        // `auto` fell open (strict would have errored before reaching output).
        return Some("[sandbox: none — command ran with full user authority]".to_string());
    }

    let policy = build_sandbox_policy(working_dir);
    let roots: Vec<String> = policy
        .writable_roots
        .iter()
        .map(|p| p.display().to_string())
        .collect();
    let net = if policy.allow_network {
        "network allowed"
    } else if report.tier == SandboxTier::Full {
        "network denied"
    } else {
        "network NOT denied"
    };
    let mut line = format!(
        "[sandbox: {} — writes confined to {}; {}",
        report.mechanism,
        roots.join(", "),
        net
    );
    if !report.degradations.is_empty() {
        line.push_str("; ");
        line.push_str(&report.degradations.join("; "));
    }
    line.push(']');
    Some(line)
}

/// Configure a shell command with process group support for proper child process tracking.
///
/// On Unix systems, creates a new process group so child processes can be killed together.
/// On Windows, the default behavior already supports process tree termination.
///
/// Returns `Err(message)` only when `BIOROUTER_SHELL_SANDBOX=strict` and the
/// host cannot provide a full sandbox — the caller surfaces that as a tool error
/// (BR-69). In every other mode this is infallible.
pub fn configure_shell_command(
    shell_config: &ShellConfig,
    command: &str,
    working_dir: Option<&std::path::Path>,
) -> Result<tokio::process::Command, String> {
    // BR-69: optionally run the shell under an OS-level sandbox. When enabled,
    // the spawned program becomes `<wrapper> … -- <shell>` and the shell's own
    // `-c <command>` args are appended after the `--` separator below.
    let (program, sandbox_prefix) = match shell_sandbox_wrap(&shell_config.executable, working_dir)?
    {
        Some((prog, prefix)) => (prog, prefix),
        None => (shell_config.executable.clone(), Vec::new()),
    };

    let mut command_builder = tokio::process::Command::new(&program);

    if let Some(dir) = working_dir {
        command_builder.current_dir(dir);
    }

    command_builder
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .env("BIOROUTER_TERMINAL", "1")
        .env("GIT_EDITOR", "sh -c 'echo \"Interactive Git commands are not supported in this environment.\" >&2; exit 1'")
        .env("GIT_SEQUENCE_EDITOR", "sh -c 'echo \"Interactive Git commands are not supported in this environment.\" >&2; exit 1'")
        .env("VISUAL", "sh -c 'echo \"Interactive editor not available in this environment.\" >&2; exit 1'")
        .env("EDITOR", "sh -c 'echo \"Interactive editor not available in this environment.\" >&2; exit 1'")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_PAGER", "cat")
        .args(&sandbox_prefix)
        .args(&shell_config.args)
        .arg(command);

    // On Unix systems, create a new process group so we can kill child processes
    #[cfg(unix)]
    {
        command_builder.process_group(0);
    }

    Ok(command_builder)
}

/// Kill a process and all its child processes using platform-specific approaches.
///
/// On Unix systems, kills the entire process group.
/// On Windows, kills the process tree.
pub async fn kill_process_group(
    child: &mut tokio::process::Child,
    pid: Option<u32>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    #[cfg(unix)]
    {
        if let Some(pid) = pid {
            // Try SIGTERM first
            let sigterm_result = unsafe { libc::kill(-(pid as i32), libc::SIGTERM) };
            if sigterm_result != 0 {
                tracing::warn!(
                    "SIGTERM to process group {} failed with errno {}",
                    pid,
                    sigterm_result
                );
            }

            // Wait a brief moment for graceful shutdown
            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

            // Force kill with SIGKILL
            let sigkill_result = unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
            if sigkill_result != 0 {
                tracing::warn!(
                    "SIGKILL to process group {} failed with errno {}",
                    pid,
                    sigkill_result
                );
            }
        }

        // Last fallback, return the result of tokio's kill
        child.kill().await.map_err(|e| e.into())
    }

    #[cfg(windows)]
    {
        if let Some(pid) = pid {
            // Use taskkill to kill the process tree on Windows
            let _kill_result = tokio::process::Command::new("taskkill")
                .args(&["/F", "/T", "/PID", &pid.to_string()])
                .output()
                .await;
        }

        // Return the result of tokio's kill
        child.kill().await.map_err(|e| e.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_flag_parses_truthy_values() {
        for on in ["1", "true", "on", "yes", "  ON ", "True", "Yes"] {
            assert!(env_flag_truthy(on), "expected {on:?} to enable");
        }
        for off in ["", "0", "off", "no", "false", "docker", " ", "seatbelt"] {
            assert!(!env_flag_truthy(off), "expected {off:?} to stay off");
        }
    }

    #[test]
    fn unset_gate_runs_the_plain_shell() {
        // Default (gate unset) must be byte-for-byte the old behavior: the
        // spawned program is the shell itself, not a sandbox wrapper, and no
        // status line is prepended.
        if env::var("BIOROUTER_SHELL_SANDBOX").is_ok() {
            return; // don't fight an externally-set gate in this process
        }
        let cfg = ShellConfig::default();
        let cmd = configure_shell_command(&cfg, "echo hi", None).expect("infallible when off");
        assert_eq!(cmd.as_std().get_program().to_string_lossy(), cfg.executable);
        assert!(
            shell_sandbox_status_line(None).is_none(),
            "no status line when the gate is off"
        );
    }

    #[test]
    fn shell_sandbox_wrap_is_none_when_gate_off() {
        if env::var("BIOROUTER_SHELL_SANDBOX").is_ok() {
            return;
        }
        assert!(matches!(shell_sandbox_wrap("/bin/sh", None), Ok(None)));
    }
}
