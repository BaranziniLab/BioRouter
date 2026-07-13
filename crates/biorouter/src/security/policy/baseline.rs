//! The embedded, always-on baseline rule set.
//!
//! Compiled into the binary via `include_str!`, so command governance is on by
//! default with no user action — a strict improvement over the old
//! `SECURITY_PROMPT_ENABLED=false` no-op scanner. External tier files (user /
//! project / admin) are a later slice; this is the sole rule source today.

use serde::Deserialize;

use super::rule::Rule;

pub const BASELINE_YAML: &str = include_str!("baseline.policy.yaml");

/// On-disk policy file shape: a top-level `rules:` list.
#[derive(Debug, Deserialize)]
pub struct PolicyFile {
    #[serde(default)]
    pub rules: Vec<Rule>,
}

/// Parse the embedded baseline. A parse failure is fail-safe: it logs and
/// returns an empty set (the separate BR-20 catastrophic floor still protects
/// the worst commands), rather than panicking the whole agent.
pub fn baseline_rules() -> Vec<Rule> {
    match serde_yaml::from_str::<PolicyFile>(BASELINE_YAML) {
        Ok(file) => file.rules,
        Err(e) => {
            tracing::error!(
                error = %e,
                "Failed to parse embedded baseline command policy; command policy disabled \
                 (BR-20 catastrophic floor still active)"
            );
            Vec::new()
        }
    }
}
