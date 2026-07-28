use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::config::{Config, ConfigError};
use crate::workflow::Workflow;

const SLASH_COMMANDS_CONFIG_KEY: &str = "slash_commands";

/// Guards the once-per-process debug note about an unset `slash_commands` key.
static MISSING_KEY_LOGGED: AtomicBool = AtomicBool::new(false);
const REMOVED_SLASH_COMMANDS: &[&str] = &["prompt", "prompts"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlashCommandMapping {
    pub command: String,
    pub workflow_path: String,
}

pub fn list_commands() -> Vec<SlashCommandMapping> {
    commands_or_default(Config::global().get_param(SLASH_COMMANDS_CONFIG_KEY))
        .into_iter()
        .filter(|mapping: &SlashCommandMapping| !is_removed_slash_command(&mapping.command))
        .collect()
}

/// Resolve the configured mappings, tolerating a config that has never defined
/// the key.
///
/// An absent optional key is the default state, not a warning condition — and
/// `list_commands` is called many times per session, so warning on it filled
/// the log with the same line 23 times (issue #49). It is recorded once per
/// process at `debug`; a *real* failure (unreadable or undeserializable config)
/// still warns every time.
fn commands_or_default(
    loaded: Result<Vec<SlashCommandMapping>, ConfigError>,
) -> Vec<SlashCommandMapping> {
    match loaded {
        Ok(commands) => commands,
        Err(ConfigError::NotFound(_)) => {
            if !MISSING_KEY_LOGGED.swap(true, Ordering::Relaxed) {
                debug!(
                    "No {} configured; using an empty list.",
                    SLASH_COMMANDS_CONFIG_KEY
                );
            }
            Vec::new()
        }
        Err(err) => {
            warn!(
                "Failed to load {}: {}. Falling back to empty list.",
                SLASH_COMMANDS_CONFIG_KEY, err
            );
            Vec::new()
        }
    }
}

pub fn is_removed_slash_command(command: &str) -> bool {
    let normalized = command.trim_start_matches('/').to_lowercase();
    REMOVED_SLASH_COMMANDS.contains(&normalized.as_str())
}

fn save_slash_commands(commands: Vec<SlashCommandMapping>) -> Result<()> {
    Config::global()
        .set_param(SLASH_COMMANDS_CONFIG_KEY, &commands)
        .map_err(|e| anyhow::anyhow!("Failed to save slash commands: {}", e))
}

pub fn remove_commands_for_directory(directory: &std::path::Path) -> Result<usize> {
    let mut commands = list_commands();
    let before = commands.len();
    commands.retain(|mapping| !PathBuf::from(&mapping.workflow_path).starts_with(directory));
    let removed = before - commands.len();
    save_slash_commands(commands)?;
    Ok(removed)
}

pub fn set_workflow_slash_command(workflow_path: PathBuf, command: Option<String>) -> Result<()> {
    let workflow_path_str = workflow_path.to_string_lossy().to_string();

    let mut commands = list_commands();
    commands.retain(|mapping| mapping.workflow_path != workflow_path_str);

    if let Some(cmd) = command {
        let normalized_cmd = cmd.trim_start_matches('/').to_lowercase();
        if !normalized_cmd.is_empty() && !is_removed_slash_command(&normalized_cmd) {
            commands.push(SlashCommandMapping {
                command: normalized_cmd,
                workflow_path: workflow_path_str,
            });
        }
    }

    save_slash_commands(commands)
}

pub fn get_workflow_for_command(command: &str) -> Option<PathBuf> {
    let normalized = command.trim_start_matches('/').to_lowercase();
    if is_removed_slash_command(&normalized) {
        return None;
    }
    let commands = list_commands();
    commands
        .into_iter()
        .find(|mapping| mapping.command == normalized)
        .map(|mapping| PathBuf::from(mapping.workflow_path))
}

pub fn resolve_slash_command(command: &str) -> Option<Workflow> {
    let workflow_path = get_workflow_for_command(command)?;

    if !workflow_path.exists() {
        return None;
    }
    let workflow_content = std::fs::read_to_string(&workflow_path).ok()?;
    let workflow = Workflow::from_content(&workflow_content).ok()?;

    Some(workflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::fmt::MakeWriter;

    /// Collects formatted tracing output so a test can assert the *level* a
    /// message was emitted at, not merely that it was emitted.
    #[derive(Clone, Default)]
    struct CapturedLogs(Arc<Mutex<Vec<u8>>>);

    impl CapturedLogs {
        fn text(&self) -> String {
            String::from_utf8_lossy(&self.0.lock().unwrap()).to_string()
        }
    }

    impl std::io::Write for CapturedLogs {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for CapturedLogs {
        type Writer = CapturedLogs;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    fn capture<T>(f: impl FnOnce() -> T) -> (T, String) {
        let logs = CapturedLogs::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(logs.clone())
            .with_max_level(tracing::Level::DEBUG)
            .with_ansi(false)
            .finish();
        let value = tracing::subscriber::with_default(subscriber, f);
        (value, logs.text())
    }

    #[test]
    fn an_unset_key_is_the_empty_default_not_a_warning() {
        MISSING_KEY_LOGGED.store(false, Ordering::Relaxed);

        let (commands, logs) = capture(|| {
            let mut all = Vec::new();
            // list_commands runs many times per session; the note must not be
            // repeated once per call.
            for _ in 0..3 {
                all.extend(commands_or_default(Err(ConfigError::NotFound(
                    SLASH_COMMANDS_CONFIG_KEY.to_string(),
                ))));
            }
            all
        });

        assert!(commands.is_empty());
        assert!(
            !logs.contains("WARN"),
            "an absent optional key is the default state; logs were:\n{logs}"
        );
        assert_eq!(
            logs.matches("DEBUG").count(),
            1,
            "the note is worth recording once per process, not once per call; logs were:\n{logs}"
        );
    }

    #[test]
    fn a_real_config_failure_still_warns() {
        let (commands, logs) = capture(|| {
            commands_or_default(Err(ConfigError::DeserializeError(
                "invalid type: string".to_string(),
            )))
        });

        assert!(commands.is_empty());
        assert!(
            logs.contains("WARN") && logs.contains("invalid type: string"),
            "a config that cannot be read is a real problem; logs were:\n{logs}"
        );
    }

    #[test]
    fn configured_commands_are_returned_unchanged() {
        let (commands, logs) = capture(|| {
            commands_or_default(Ok(vec![SlashCommandMapping {
                command: "review".to_string(),
                workflow_path: "/tmp/review.yaml".to_string(),
            }]))
        });

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command, "review");
        assert!(logs.is_empty(), "logs were:\n{logs}");
    }
}
