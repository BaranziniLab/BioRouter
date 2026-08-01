pub mod action_required_manager;
pub mod agents;
pub mod biorouter_apps;
pub mod checkpoint;
pub mod config;
pub mod context_budget;
pub mod context_mgmt;
pub mod conversation;
pub mod execution;
pub mod guardrails;
pub mod hints;
pub mod hooks;
pub mod knowledge;
pub mod logging;
pub mod managed;
pub mod mcp_utils;
pub mod model;
pub mod oauth;
pub mod observability;
pub mod permission;
pub mod privacy;
pub mod prompt_template;
pub mod providers;
pub mod scheduler;
pub mod scheduler_trait;
pub mod security;
pub mod session;
pub mod session_context;
pub mod session_events;
pub mod slash_commands;
pub mod subprocess;
pub mod system;
/// Test-binary-only: pin `BIOROUTER_PATH_ROOT` under a temp dir before any test
/// runs, so a default `cargo test -p biorouter --lib` cannot write into the
/// developer's real `~/.config/biorouter`.
#[cfg(test)]
mod test_sandbox;
pub mod token_counter;
pub mod tool_inspection;
pub mod tool_monitor;
pub mod tracing;
pub mod utils;
pub mod workflow;
pub mod workflow_deeplink;
pub mod workspace_services;
