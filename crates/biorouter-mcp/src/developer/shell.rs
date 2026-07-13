use std::{env, ffi::OsString, process::Stdio};

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

/// Parse the `BIOROUTER_SHELL_SANDBOX` gate value (BR-64). Off unless the value
/// is one of `1`/`true`/`on`/`seatbelt` (case-insensitive, trimmed).
pub(crate) fn shell_sandbox_enabled(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "on" | "seatbelt"
    )
}

fn env_truthy(name: &str) -> bool {
    env::var(name)
        .map(|v| shell_sandbox_enabled(&v))
        .unwrap_or(false)
}

/// If the OS-level shell sandbox is enabled and usable, return the program +
/// leading arguments that wrap `program` in `sandbox-exec` (macOS Seatbelt,
/// BR-64 Slice 1). Off by default; opt in with `BIOROUTER_SHELL_SANDBOX`. On an
/// unsupported host, or when `sandbox-exec` is missing, returns `None` so the
/// command runs unsandboxed (fail-open — Slice 1 is opt-in hardening).
fn shell_sandbox_wrap(
    program: &str,
    working_dir: Option<&std::path::Path>,
) -> Option<(String, Vec<String>)> {
    if !env::var("BIOROUTER_SHELL_SANDBOX")
        .map(|v| shell_sandbox_enabled(&v))
        .unwrap_or(false)
    {
        return None;
    }
    if !biorouter_sandbox::seatbelt::available() {
        tracing::warn!(
            "BIOROUTER_SHELL_SANDBOX is set but no OS sandbox is available on this host; \
             running the shell tool unsandboxed"
        );
        return None;
    }

    // Writable roots: the session working dir (the natural project root) plus
    // the process temp dir. Everything else is read-only; network is denied
    // unless BIOROUTER_SHELL_SANDBOX_NETWORK is truthy.
    let mut roots = Vec::new();
    if let Some(dir) = working_dir {
        roots.push(dir.to_path_buf());
    }
    roots.push(env::temp_dir());

    let policy = biorouter_sandbox::seatbelt::SeatbeltPolicy::new(roots)
        .with_network(env_truthy("BIOROUTER_SHELL_SANDBOX_NETWORK"));
    Some(policy.wrap(program))
}

/// Configure a shell command with process group support for proper child process tracking.
///
/// On Unix systems, creates a new process group so child processes can be killed together.
/// On Windows, the default behavior already supports process tree termination.
pub fn configure_shell_command(
    shell_config: &ShellConfig,
    command: &str,
    working_dir: Option<&std::path::Path>,
) -> tokio::process::Command {
    // BR-64: optionally run the shell under an OS-level sandbox. When enabled,
    // the spawned program becomes `sandbox-exec … -- <shell>` and the shell's
    // own `-c <command>` args are appended after the `--` separator below.
    let (program, sandbox_prefix) = match shell_sandbox_wrap(&shell_config.executable, working_dir)
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

    command_builder
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
    fn sandbox_gate_parses_truthy_values() {
        for on in ["1", "true", "on", "seatbelt", "  ON ", "True", "SeatBelt"] {
            assert!(shell_sandbox_enabled(on), "expected {on:?} to enable");
        }
        for off in ["", "0", "off", "no", "false", "docker", " "] {
            assert!(!shell_sandbox_enabled(off), "expected {off:?} to stay off");
        }
    }

    #[test]
    fn unset_gate_runs_the_plain_shell() {
        // Default (gate unset) must be byte-for-byte the old behavior: the
        // spawned program is the shell itself, not `sandbox-exec`.
        if env::var("BIOROUTER_SHELL_SANDBOX").is_ok() {
            return; // don't fight an externally-set gate in this process
        }
        let cfg = ShellConfig::default();
        let cmd = configure_shell_command(&cfg, "echo hi", None);
        assert_eq!(cmd.as_std().get_program().to_string_lossy(), cfg.executable);
    }
}
