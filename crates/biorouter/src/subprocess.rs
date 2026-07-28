use tokio::process::Command;

#[cfg(windows)]
const CREATE_NO_WINDOW_FLAG: u32 = 0x08000000;

#[allow(unused_variables)]
pub fn configure_command_no_window(command: &mut Command) {
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW_FLAG);
}

/// Prepare a child process that runs on the agent's behalf.
///
/// Two things every such child needs, in one place so a new spawn site cannot
/// get only one of them:
///
/// 1. no console window on Windows ([`configure_command_no_window`]);
/// 2. none of the daemon's own credentials
///    ([`biorouter_mcp::developer::shell::strip_daemon_private_env`], issue #57).
///
/// The second is the security-relevant half. `biorouterd` holds
/// `BIOROUTER_SERVER__SECRET_KEY` in its environment, and a child that inherits
/// it becomes a fully authenticated client of the daemon's own REST API — it
/// can read or import *any* session, which defeats every control that lives in
/// the agent loop.
///
/// This matters most for the CLI-agent providers (`claude_code`, `codex`,
/// `gemini_cli`, `cursor_agent`): the process being spawned is itself a coding
/// agent with shell access, so it is the highest-privilege child the daemon
/// creates. It also covers hook commands, which run on agent activity.
///
/// Call this **last**, after every other `env`/`envs` call on the command: the
/// strip and `env` write to the same map, so a later `env` would re-admit a
/// credential the strip had already removed.
pub fn prepare_agent_child_command(command: &mut Command) {
    configure_command_no_window(command);
    biorouter_mcp::developer::shell::strip_daemon_private_env(command);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The daemon's own credentials are removed, and nothing else is — the
    /// user's environment and an extension's declared credential are not ours
    /// to censor (issue #24 was a truncated `PATH` breaking every Homebrew
    /// binary; over-stripping is its own regression).
    #[test]
    fn agent_child_loses_the_daemon_credentials_and_keeps_everything_else() {
        let mut cmd = Command::new("printenv");
        cmd.env("BIOROUTER_SERVER__SECRET_KEY", "daemon-private")
            .env("BIOROUTER_ACP_WS_TOKEN", "daemon-private")
            .env("BIOROUTER_PORT", "4931")
            .env("SPOKEAGENT_PASSCODE", "extension-credential")
            .env("AWS_SECRET_ACCESS_KEY", "the-user's-own")
            .env("PATH", "/usr/bin");
        prepare_agent_child_command(&mut cmd);

        let envs: Vec<(String, Option<String>)> = cmd
            .as_std()
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect();
        let removed = |key: &str| envs.contains(&(key.to_string(), None));
        let kept =
            |key: &str, value: &str| envs.contains(&(key.to_string(), Some(value.to_string())));

        assert!(
            removed("BIOROUTER_SERVER__SECRET_KEY"),
            "the daemon's auth secret must not reach a child it spawns: {envs:?}"
        );
        assert!(
            removed("BIOROUTER_ACP_WS_TOKEN"),
            "the ACP token is a BioRouter credential too: {envs:?}"
        );
        assert!(kept("BIOROUTER_PORT", "4931"), "a port is not a credential");
        assert!(
            kept("SPOKEAGENT_PASSCODE", "extension-credential"),
            "an extension's declared credential is not the daemon's to strip"
        );
        assert!(
            kept("AWS_SECRET_ACCESS_KEY", "the-user's-own"),
            "the user's own credentials are not ours to censor"
        );
        assert!(kept("PATH", "/usr/bin"), "PATH must survive");
    }
}
