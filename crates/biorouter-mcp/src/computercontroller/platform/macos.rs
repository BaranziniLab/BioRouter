use super::SystemAutomation;
use std::path::PathBuf;
use std::process::Command;

pub struct MacOSAutomation;

impl SystemAutomation for MacOSAutomation {
    fn execute_system_script(&self, script: &str) -> std::io::Result<String> {
        let output = Command::new("osascript").arg("-e").arg(script).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();

        // Honest reporting: osascript exits non-zero on failure and writes the
        // reason to stderr. The previous implementation discarded both, so a
        // failed UI script (very often a missing-permission error) looked like
        // an empty success — which makes the agent retry blindly and "circle".
        if output.status.success() {
            return Ok(stdout);
        }

        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let code = output
            .status
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "signal".to_string());
        let mut msg = format!("osascript exited with status {code}");
        let trimmed_err = stderr.trim();
        if !trimmed_err.is_empty() {
            msg.push_str(&format!(": {trimmed_err}"));
        }
        // The dominant, actionable cause on macOS: BioRouter has not been
        // granted Accessibility / Automation permission for the target app.
        // Surfacing this lets the agent stop and ask the user instead of looping.
        if trimmed_err.contains("not allowed")
            || trimmed_err.contains("assistive access")
            || trimmed_err.contains("-1743") // not authorized to send Apple events
            || trimmed_err.contains("-1719")
        // can't get ... (often UI scripting blocked)
        {
            msg.push_str(
                "\n\nThis is a macOS permission error, not a scripting mistake. BioRouter must be \
                 granted access under System Settings > Privacy & Security > Accessibility (and \
                 Automation for the target app). Ask the user to grant it; do NOT retry the same \
                 script until they have.",
            );
        }
        let trimmed_out = stdout.trim();
        if !trimmed_out.is_empty() {
            msg.push_str(&format!(
                "\n\nPartial output before failure:\n{trimmed_out}"
            ));
        }
        Err(std::io::Error::other(msg))
    }

    fn get_shell_command(&self) -> (&'static str, &'static str) {
        ("bash", "-c")
    }

    fn get_temp_path(&self) -> PathBuf {
        PathBuf::from("/tmp")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_script_returns_stdout() {
        let out = MacOSAutomation
            .execute_system_script("return 2 + 2")
            .expect("simple arithmetic script should succeed");
        assert_eq!(out.trim(), "4");
    }

    #[test]
    fn empty_output_script_still_succeeds() {
        // A no-output script (typical of UI actions) must NOT be reported as a
        // failure — it returns Ok with empty stdout.
        let out = MacOSAutomation
            .execute_system_script("set _x to 1\nreturn")
            .expect("no-output script should still succeed");
        assert!(out.trim().is_empty(), "expected empty output, got: {out:?}");
    }

    #[test]
    fn failing_script_returns_err_with_reason() {
        // A syntax error makes osascript exit non-zero and write to stderr.
        // The previous implementation swallowed this and returned Ok(""), which
        // is exactly what made the agent retry blindly. It must now be an Err.
        let result = MacOSAutomation.execute_system_script("this is not valid applescript @@@");
        assert!(
            result.is_err(),
            "a failing osascript must return Err, got Ok({:?})",
            result.ok()
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("osascript exited"),
            "error should explain the non-zero exit, got: {msg}"
        );
    }
}
