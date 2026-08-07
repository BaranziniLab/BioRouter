//! The embedded, always-on baseline rule set.
//!
//! Compiled into the binary via `include_str!`, so command governance is on by
//! default with no user action. External tier files (user / project / admin)
//! are a later slice; this is the sole rule source today.

use serde::Deserialize;

use super::rule::Rule;

/// The shared, platform-agnostic baseline (POSIX-shaped `rm`/`dd`/`curl|bash`
/// rules that are correct on every platform — `rm -rf /etc` typed into `pwsh`
/// on Linux is still `rm -rf /etc`).
pub const BASELINE_YAML: &str = include_str!("baseline.policy.yaml");
/// BR-68 per-platform policy tiers. All are loaded on every host; each rule's
/// `platforms` field gates it at *match* time, which is what keeps the Windows
/// rules testable on a mac (`evaluate_for(Windows, …)` finds them) while making
/// them inert in production on macOS/Linux.
pub const WINDOWS_YAML: &str = include_str!("baseline.windows.policy.yaml");
pub const LINUX_YAML: &str = include_str!("baseline.linux.policy.yaml");
pub const MACOS_YAML: &str = include_str!("baseline.macos.policy.yaml");

/// On-disk policy file shape: a top-level `rules:` list.
#[derive(Debug, Deserialize)]
pub struct PolicyFile {
    #[serde(default)]
    pub rules: Vec<Rule>,
}

/// Parse one embedded policy file. A parse failure is fail-safe: it logs and
/// contributes no rules (the separate BR-20 catastrophic floor still protects
/// the worst commands), rather than panicking the whole agent.
fn parse_embedded(name: &str, yaml: &str) -> Vec<Rule> {
    match serde_yaml::from_str::<PolicyFile>(yaml) {
        Ok(file) => file.rules,
        Err(e) => {
            tracing::error!(
                file = name,
                error = %e,
                "Failed to parse embedded command policy file; its rules are disabled \
                 (BR-20 catastrophic floor still active)"
            );
            Vec::new()
        }
    }
}

/// Parse and concatenate the shared baseline + every per-platform tier. Order is
/// shared-first so a per-platform rule at equal priority wins the last-match tie.
pub fn baseline_rules() -> Vec<Rule> {
    let mut rules = parse_embedded("baseline.policy.yaml", BASELINE_YAML);
    rules.extend(parse_embedded("baseline.windows.policy.yaml", WINDOWS_YAML));
    rules.extend(parse_embedded("baseline.linux.policy.yaml", LINUX_YAML));
    rules.extend(parse_embedded("baseline.macos.policy.yaml", MACOS_YAML));
    rules
}
