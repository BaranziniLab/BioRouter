//! The one env-aware path resolver for `biorouter-mcp`.
//!
//! This crate cannot depend on `biorouter` (that would be circular), which is
//! why `biorouter::config::Paths` — the authoritative resolver — is not
//! reachable from here. Before this module existed, three separate places
//! hand-rolled a `choose_app_strategy(...)` call and each of them silently
//! ignored `BIOROUTER_PATH_ROOT`:
//!
//!   * `agent_drafter::default_root()`      — drafted apps
//!   * `agent_drafter::skills_root_for_export()` — installed skills
//!   * `knowledge::paths::knowledge_root()` — knowledge bases (and with a
//!     *different* app-strategy tuple, `io/biorouter/biorouter`)
//!
//! The consequence was that a sandboxed run — a test drive, a worktree, a
//! per-app jail — wrote drafted apps into, and read knowledge bases out of, the
//! user's **global** store. Every resolver in this crate now goes through here,
//! and `biorouter` carries a cross-crate test asserting this module agrees with
//! `biorouter::config::Paths` byte for byte, so a new hand-rolled
//! `choose_app_strategy` call fails CI.

use etcetera::{choose_app_strategy, AppStrategy};
use std::path::{Path, PathBuf};

/// Resolve the config dir the way `biorouter::config::Paths::get_dir(Config)`
/// does: `$BIOROUTER_PATH_ROOT/config` when the override is set and non-empty,
/// otherwise the platform config dir.
pub fn config_dir() -> PathBuf {
    let root = std::env::var("BIOROUTER_PATH_ROOT").ok();
    resolve_config_dir(root.as_deref(), &platform_config_dir())
}

/// `<config>/<sub>`.
pub fn in_config_dir(sub: &str) -> PathBuf {
    config_dir().join(sub)
}

/// The pure core of [`config_dir`], split out so it is testable without
/// mutating process env (which races every other test in the binary).
pub fn resolve_config_dir(root_env: Option<&str>, platform_fallback: &Path) -> PathBuf {
    match root_env {
        Some(root) if !root.trim().is_empty() => PathBuf::from(root).join("config"),
        _ => platform_fallback.to_path_buf(),
    }
}

fn platform_config_dir() -> PathBuf {
    choose_app_strategy(crate::APP_STRATEGY.clone())
        .map(|s| s.config_dir())
        .unwrap_or_else(|_| PathBuf::from(".config/biorouter"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_root_override_wins() {
        let fallback = PathBuf::from("/home/u/.config/biorouter");
        assert_eq!(
            resolve_config_dir(Some("/tmp/sandbox"), &fallback),
            PathBuf::from("/tmp/sandbox/config")
        );
    }

    #[test]
    fn empty_or_absent_override_falls_back_to_the_platform_dir() {
        let fallback = PathBuf::from("/home/u/.config/biorouter");
        assert_eq!(resolve_config_dir(None, &fallback), fallback);
        assert_eq!(resolve_config_dir(Some(""), &fallback), fallback);
        assert_eq!(resolve_config_dir(Some("   "), &fallback), fallback);
    }

    #[test]
    fn in_config_dir_appends_the_subdir() {
        // Exercises the real resolver; only asserts the tail so it holds
        // whether or not BIOROUTER_PATH_ROOT is set in the ambient env.
        assert!(in_config_dir("agent_drafter").ends_with("agent_drafter"));
    }
}
