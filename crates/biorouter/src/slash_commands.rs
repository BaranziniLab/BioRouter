use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::config::Config;
use crate::workflow::Workflow;

const SLASH_COMMANDS_CONFIG_KEY: &str = "slash_commands";
const REMOVED_SLASH_COMMANDS: &[&str] = &["prompt", "prompts"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlashCommandMapping {
    pub command: String,
    pub workflow_path: String,
}

pub fn list_commands() -> Vec<SlashCommandMapping> {
    Config::global()
        .get_param(SLASH_COMMANDS_CONFIG_KEY)
        .unwrap_or_else(|err| {
            warn!(
                "Failed to load {}: {}. Falling back to empty list.",
                SLASH_COMMANDS_CONFIG_KEY, err
            );
            Vec::new()
        })
        .into_iter()
        .filter(|mapping: &SlashCommandMapping| !is_removed_slash_command(&mapping.command))
        .collect()
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
