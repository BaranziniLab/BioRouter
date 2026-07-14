//! Shell command hook execution: payload JSON on stdin, cwd = session
//! working dir, hard timeout, exit-code semantics interpreted by the caller.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use anyhow::Result;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tracing::debug;

use crate::subprocess::configure_command_no_window;

pub struct CommandHookResult {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Run a hook command, piping `payload_json` to its stdin.
///
/// Errors (spawn failure, timeout) are returned as `Err` — callers treat
/// them as non-blocking hook failures (failure-open).
pub async fn run_command_hook(
    command: &str,
    payload_json: &str,
    cwd: &Path,
    envs: &[(String, String)],
    timeout: Duration,
) -> Result<CommandHookResult> {
    debug!(
        "hooks: running command hook (timeout {:?}): {}",
        timeout, command
    );

    let mut cmd = if cfg!(target_os = "windows") {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", command]);
        cmd
    } else {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", command]);
        cmd
    };
    configure_command_no_window(&mut cmd);
    cmd.current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for (key, value) in envs {
        cmd.env(key, value);
    }

    let run = async {
        let mut child = cmd.spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            // A hook that never reads stdin must not wedge us: write
            // concurrently with waiting and ignore broken pipes.
            let payload = payload_json.as_bytes().to_vec();
            tokio::spawn(async move {
                let _ = stdin.write_all(&payload).await;
                let _ = stdin.shutdown().await;
            });
        }
        let output = child.wait_with_output().await?;
        Ok::<_, anyhow::Error>(CommandHookResult {
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    };

    match tokio::time::timeout(timeout, run).await {
        Ok(result) => result,
        Err(_) => Err(anyhow::anyhow!(
            "hook command timed out after {:?}: {}",
            timeout,
            command
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envs() -> Vec<(String, String)> {
        vec![("BIOROUTER_HOOK_EVENT".to_string(), "PreToolUse".to_string())]
    }

    fn stdin_echo_command() -> &'static str {
        if cfg!(target_os = "windows") {
            "more"
        } else {
            "cat"
        }
    }

    fn env_stderr_exit_two_command() -> &'static str {
        if cfg!(target_os = "windows") {
            "echo event=%BIOROUTER_HOOK_EVENT% 1>&2 & exit /b 2"
        } else {
            "echo \"event=$BIOROUTER_HOOK_EVENT\" >&2; exit 2"
        }
    }

    fn success_command() -> &'static str {
        if cfg!(target_os = "windows") {
            "exit /b 0"
        } else {
            "exit 0"
        }
    }

    fn slow_command() -> &'static str {
        if cfg!(target_os = "windows") {
            "ping -n 30 127.0.0.1 >NUL"
        } else {
            "sleep 30"
        }
    }

    fn cwd_command() -> &'static str {
        if cfg!(target_os = "windows") {
            "cd"
        } else {
            "pwd"
        }
    }

    #[tokio::test]
    async fn hook_receives_payload_on_stdin() {
        let dir = tempfile::tempdir().unwrap();
        let result = run_command_hook(
            stdin_echo_command(),
            r#"{"hook_event_name":"PreToolUse"}"#,
            dir.path(),
            &envs(),
            Duration::from_secs(10),
        )
        .await
        .unwrap();
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.contains("PreToolUse"));
    }

    #[tokio::test]
    async fn hook_sees_env_and_exit_two_stderr() {
        let dir = tempfile::tempdir().unwrap();
        let result = run_command_hook(
            env_stderr_exit_two_command(),
            "{}",
            dir.path(),
            &envs(),
            Duration::from_secs(10),
        )
        .await
        .unwrap();
        assert_eq!(result.exit_code, Some(2));
        assert!(result.stderr.contains("event=PreToolUse"));
    }

    #[tokio::test]
    async fn hook_that_ignores_stdin_still_completes() {
        let dir = tempfile::tempdir().unwrap();
        let result = run_command_hook(
            success_command(),
            "{\"big\":\"payload\"}",
            dir.path(),
            &[],
            Duration::from_secs(10),
        )
        .await
        .unwrap();
        assert_eq!(result.exit_code, Some(0));
    }

    #[tokio::test]
    async fn timeout_kills_hook() {
        let started = std::time::Instant::now();
        let dir = tempfile::tempdir().unwrap();
        let result = run_command_hook(
            slow_command(),
            "{}",
            dir.path(),
            &[],
            Duration::from_millis(300),
        )
        .await;
        assert!(result.is_err());
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn runs_in_given_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let result = run_command_hook(
            cwd_command(),
            "{}",
            dir.path(),
            &[],
            Duration::from_secs(10),
        )
        .await
        .unwrap();
        let canonical = dir.path().canonicalize().unwrap();
        assert!(result
            .stdout
            .trim()
            .ends_with(canonical.file_name().unwrap().to_str().unwrap()));
    }
}
