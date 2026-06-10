//! Hook configuration: schema, loading from global config + project file,
//! and merge semantics (project hooks extend global hooks).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Deserializer};
use tracing::warn;

use super::event::HookEvent;
use crate::config::{Config, ConfigError};

/// File name of the project-level hooks file, relative to the session
/// working directory.
pub const PROJECT_HOOKS_FILE: &str = ".biorouter/hooks.yaml";

/// A single hook handler definition.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HookDefinition {
    /// Run a shell command. The event payload JSON is piped to stdin.
    Command {
        command: String,
        /// Timeout in seconds (default 60).
        #[serde(default)]
        timeout: Option<u64>,
    },
    /// Ask an LLM judge whether the event is OK. The judge receives the
    /// payload JSON and must answer {"ok": bool, "reason": string}.
    Prompt {
        prompt: String,
        /// Optional model override (e.g. a fast/cheap model).
        #[serde(default)]
        model: Option<String>,
        /// Optional provider override; requires `model` to be set too.
        #[serde(default)]
        provider: Option<String>,
        /// Timeout in seconds (default 30).
        #[serde(default)]
        timeout: Option<u64>,
    },
}

/// A matcher plus the hooks to run when it matches.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct HookMatcherGroup {
    /// `None`, `""` or `"*"` match everything. Otherwise exact match, with
    /// regex (incl. `a|b` alternation) as fallback.
    #[serde(default)]
    pub matcher: Option<String>,
    pub hooks: Vec<HookDefinition>,
}

/// Parsed hooks configuration: event name -> matcher groups.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HooksConfig {
    pub events: HashMap<HookEvent, Vec<HookMatcherGroup>>,
    /// Only meaningful in the global config: opt-in for running
    /// project-level `.biorouter/hooks.yaml` hooks.
    pub allow_project_hooks: bool,
}

impl HooksConfig {
    pub fn is_empty(&self) -> bool {
        self.events.values().all(|groups| groups.is_empty())
    }

    /// Append another config's hook groups after this one's (extend, never
    /// replace), per event.
    pub fn extend_with(&mut self, other: HooksConfig) {
        for (event, groups) in other.events {
            self.events.entry(event).or_default().extend(groups);
        }
    }
}

// Unknown event names and malformed groups are skipped with a warning rather
// than failing the whole config, so future event names stay forward-compatible.
impl<'de> Deserialize<'de> for HooksConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw: HashMap<String, serde_json::Value> = HashMap::deserialize(deserializer)?;
        let mut config = HooksConfig::default();
        for (key, value) in raw {
            if key == "allow_project_hooks" {
                config.allow_project_hooks = value.as_bool().unwrap_or(false);
                continue;
            }
            let Some(event) = HookEvent::from_str_opt(&key) else {
                warn!("hooks config: unknown event '{}' ignored", key);
                continue;
            };
            match serde_json::from_value::<Vec<HookMatcherGroup>>(value) {
                Ok(groups) => {
                    config.events.insert(event, groups);
                }
                Err(e) => {
                    warn!("hooks config: invalid groups for event '{}': {}", key, e);
                }
            }
        }
        Ok(config)
    }
}

/// Load the `hooks:` section from the global Biorouter config
/// (`~/.config/biorouter/config.yaml`, env override `BIOROUTER_HOOKS`).
pub fn load_global_config() -> HooksConfig {
    match Config::global().get_param::<HooksConfig>("hooks") {
        Ok(config) => config,
        Err(ConfigError::NotFound(_)) => HooksConfig::default(),
        Err(e) => {
            warn!("hooks: failed to load global hooks config: {}", e);
            HooksConfig::default()
        }
    }
}

/// Project hooks file wrapper: `.biorouter/hooks.yaml` uses the same schema
/// as the global config, under a top-level `hooks:` key.
#[derive(Deserialize)]
struct ProjectHooksFile {
    hooks: HooksConfig,
}

/// Parse a project hooks file's contents. Accepts either a top-level
/// `hooks:` mapping or the bare event mapping.
pub fn parse_project_hooks(contents: &str) -> Option<HooksConfig> {
    if let Ok(file) = serde_yaml::from_str::<ProjectHooksFile>(contents) {
        return Some(file.hooks);
    }
    match serde_yaml::from_str::<HooksConfig>(contents) {
        Ok(config) if !config.is_empty() => Some(config),
        Ok(_) => None,
        Err(e) => {
            warn!("hooks: failed to parse project hooks file: {}", e);
            None
        }
    }
}

/// A cached project hooks config keyed by file mtime.
#[derive(Debug, Clone)]
pub struct CachedProjectHooks {
    pub mtime: Option<SystemTime>,
    pub config: HooksConfig,
}

/// Read (or report absent) the project hooks file for a working directory.
pub fn read_project_hooks(working_dir: &Path) -> CachedProjectHooks {
    let path: PathBuf = working_dir.join(PROJECT_HOOKS_FILE);
    let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    let config = match std::fs::read_to_string(&path) {
        Ok(contents) => parse_project_hooks(&contents).unwrap_or_default(),
        Err(_) => HooksConfig::default(),
    };
    CachedProjectHooks { mtime, config }
}

/// Current mtime of the project hooks file (None if absent).
pub fn project_hooks_mtime(working_dir: &Path) -> Option<SystemTime> {
    std::fs::metadata(working_dir.join(PROJECT_HOOKS_FILE))
        .and_then(|m| m.modified())
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
PreToolUse:
  - matcher: "developer__shell|developer__text_editor"
    hooks:
      - type: command
        command: "./guard.sh"
        timeout: 30
      - type: prompt
        prompt: "Block writes outside the project."
PreCompact:
  - matcher: "auto"
    hooks:
      - type: command
        command: "backup.sh"
"#;

    #[test]
    fn parses_events_matchers_and_hook_types() {
        let config: HooksConfig = serde_yaml::from_str(SAMPLE).unwrap();
        let pre_tool = &config.events[&HookEvent::PreToolUse];
        assert_eq!(pre_tool.len(), 1);
        assert_eq!(
            pre_tool[0].matcher.as_deref(),
            Some("developer__shell|developer__text_editor")
        );
        assert_eq!(pre_tool[0].hooks.len(), 2);
        assert!(matches!(
            &pre_tool[0].hooks[0],
            HookDefinition::Command { command, timeout: Some(30) } if command == "./guard.sh"
        ));
        assert!(matches!(
            &pre_tool[0].hooks[1],
            HookDefinition::Prompt {
                model: None,
                provider: None,
                ..
            }
        ));
        assert_eq!(config.events[&HookEvent::PreCompact].len(), 1);
        assert!(!config.allow_project_hooks);
    }

    #[test]
    fn unknown_event_is_skipped_not_fatal() {
        let yaml = r#"
FileChanged:
  - hooks: [{ type: command, command: "x" }]
Stop:
  - hooks: [{ type: command, command: "y" }]
"#;
        let config: HooksConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.events.len(), 1);
        assert!(config.events.contains_key(&HookEvent::Stop));
    }

    #[test]
    fn malformed_group_is_skipped_not_fatal() {
        let yaml = r#"
Stop: "not a list"
PreToolUse:
  - hooks: [{ type: command, command: "ok" }]
"#;
        let config: HooksConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.events.len(), 1);
        assert!(config.events.contains_key(&HookEvent::PreToolUse));
    }

    #[test]
    fn allow_project_hooks_flag_parses() {
        let yaml = "allow_project_hooks: true\n";
        let config: HooksConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.allow_project_hooks);
        assert!(config.is_empty());
    }

    #[test]
    fn merge_appends_project_groups_after_global() {
        let mut global: HooksConfig =
            serde_yaml::from_str("Stop:\n  - hooks: [{ type: command, command: \"global.sh\" }]\n")
                .unwrap();
        let project: HooksConfig = serde_yaml::from_str(
            "Stop:\n  - hooks: [{ type: command, command: \"project.sh\" }]\nPreCompact:\n  - hooks: [{ type: command, command: \"p.sh\" }]\n",
        )
        .unwrap();
        global.extend_with(project);
        let stop = &global.events[&HookEvent::Stop];
        assert_eq!(stop.len(), 2);
        assert!(
            matches!(&stop[0].hooks[0], HookDefinition::Command { command, .. } if command == "global.sh")
        );
        assert!(
            matches!(&stop[1].hooks[0], HookDefinition::Command { command, .. } if command == "project.sh")
        );
        assert!(global.events.contains_key(&HookEvent::PreCompact));
    }

    #[test]
    fn project_file_accepts_hooks_root_and_bare_mapping() {
        let with_root = "hooks:\n  Stop:\n    - hooks: [{ type: command, command: \"a\" }]\n";
        let bare = "Stop:\n  - hooks: [{ type: command, command: \"a\" }]\n";
        assert!(parse_project_hooks(with_root)
            .unwrap()
            .events
            .contains_key(&HookEvent::Stop));
        assert!(parse_project_hooks(bare)
            .unwrap()
            .events
            .contains_key(&HookEvent::Stop));
    }

    #[test]
    fn hooks_config_parses_from_json_env_style() {
        // get_param's env path goes through serde_json
        let json =
            r#"{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"x"}]}]}"#;
        let config: HooksConfig = serde_json::from_str(json).unwrap();
        assert!(config.events.contains_key(&HookEvent::PreToolUse));
    }
}
