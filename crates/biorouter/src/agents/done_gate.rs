//! BR-48: a deterministic done-ness gate for interactive chat.
//!
//! Enforced verification used to exist **only for workflows**
//! (`execute_success_checks` + the retry loop, [`crate::agents::retry`]): a
//! workflow could declare `checks: [Shell { command }]` and, on failure, the
//! whole conversation was reset to the initial messages and re-run. Interactive
//! chat had no equivalent — "done" was whatever the model decided, so a coding
//! session could declare success with a broken build and nothing stopped it
//! (`internal/verification.md` gap #1, "the biggest gap").
//!
//! This module makes the same [`SuccessCheck`] machinery a done-ness gate for
//! ordinary chat, in the spirit of the `/goal` Stop-hook loop
//! ([`crate::agents::goal`]) but with **deterministic** checks instead of an LLM
//! judge. When an enabled gate's checks fail as the agent tries to finish, the
//! reply loop injects *what failed* as feedback and keeps working — iterating on
//! the diff, never resetting progress the way the workflow retry does — bounded
//! by a per-reply attempt cap so a genuinely-unsatisfiable check cannot spin
//! forever.
//!
//! **Config-gated and default OFF.** Running build/test checks on every turn is
//! not free, so the gate only runs when a user opts in:
//!
//! * `BIOROUTER_DONE_GATE` (bool) — master switch, default `false`.
//! * `BIOROUTER_DONE_GATE_CHECKS` — a JSON array of [`SuccessCheck`] (the same
//!   shape workflows use), e.g.
//!   `[{"type":"shell","command":"cargo build"},{"type":"file_exists","path":"out.csv"}]`.
//!   Empty/unset ⇒ inert even when the switch is on.
//! * `BIOROUTER_DONE_GATE_MAX_ITERATIONS` (u32) — per-reply corrective-attempt
//!   ceiling, default [`DEFAULT_MAX_ITERATIONS`]; once spent the agent is allowed
//!   to finish so a flaky/unsatisfiable check cannot wedge the turn.
//! * `BIOROUTER_DONE_GATE_TIMEOUT_SECS` (u64) — per-check timeout, default
//!   [`DEFAULT_CHECK_TIMEOUT_SECS`].

use std::time::Duration;

use crate::agents::types::SuccessCheck;
use crate::config::Config;

/// Default number of corrective attempts an enabled gate makes per reply before
/// it lets the agent finish anyway. Each attempt is a full model turn, and a
/// deterministic check that has not gone green in a few focused rounds is
/// unlikely to on the next, so a small cap keeps a stubborn check from spinning.
pub const DEFAULT_MAX_ITERATIONS: u32 = 3;

/// Default per-check timeout (seconds). A build/test check can be slow, so this
/// is generous; the per-reply budget ([`crate::agents::budget`]) is the real
/// wall-clock backstop.
pub const DEFAULT_CHECK_TIMEOUT_SECS: u64 = 300;

/// BR-48 policy, resolved once per reply (config reads touch the filesystem).
///
/// Not `PartialEq`/`Eq`: `SuccessCheck::JsonSchema` carries a `serde_json::Value`
/// schema, which is not `Eq`.
#[derive(Debug, Clone)]
pub struct DoneGateConfig {
    /// Master switch. **Default OFF** — the checks cost real work each turn.
    pub enabled: bool,
    /// The checks that must all pass before the turn may finish.
    pub checks: Vec<SuccessCheck>,
    /// Per-reply corrective-attempt ceiling. `0` also disables the gate.
    pub max_iterations: u32,
    /// Per-check timeout.
    pub timeout: Duration,
}

impl Default for DoneGateConfig {
    fn default() -> Self {
        Self {
            // BR-48 MUST default OFF: running checks on every turn is not free.
            enabled: false,
            checks: Vec::new(),
            max_iterations: DEFAULT_MAX_ITERATIONS,
            timeout: Duration::from_secs(DEFAULT_CHECK_TIMEOUT_SECS),
        }
    }
}

impl DoneGateConfig {
    /// Resolve from the global config.
    pub fn from_config() -> Self {
        Self::from(Config::global())
    }

    pub fn from(config: &Config) -> Self {
        let defaults = Self::default();
        let enabled = config
            .get_param::<bool>("BIOROUTER_DONE_GATE")
            .unwrap_or(defaults.enabled);
        // A JSON array is deserialized directly (an env value is JSON-parsed
        // first, a config.yaml value is a native sequence). Any error — unset,
        // malformed — leaves the check list empty, so the gate is simply inert
        // instead of crashing the turn.
        let checks = match config.get_param::<Vec<SuccessCheck>>("BIOROUTER_DONE_GATE_CHECKS") {
            Ok(checks) => checks,
            Err(e) => {
                // NotFound is the common (unset) case; only note real parse
                // errors so a typo in the config is discoverable.
                if !matches!(e, crate::config::ConfigError::NotFound(_)) {
                    tracing::warn!("BIOROUTER_DONE_GATE_CHECKS could not be parsed, ignoring: {e}");
                }
                Vec::new()
            }
        };
        let max_iterations = config
            .get_param::<u32>("BIOROUTER_DONE_GATE_MAX_ITERATIONS")
            .unwrap_or(defaults.max_iterations);
        let timeout_secs = config
            .get_param::<u64>("BIOROUTER_DONE_GATE_TIMEOUT_SECS")
            .unwrap_or(DEFAULT_CHECK_TIMEOUT_SECS);
        Self {
            enabled,
            checks,
            max_iterations,
            timeout: Duration::from_secs(timeout_secs.max(1)),
        }
    }

    /// Whether the gate can do anything at all this reply.
    pub fn is_active(&self) -> bool {
        self.enabled && self.max_iterations > 0 && !self.checks.is_empty()
    }
}

/// The user-role instruction injected when the gate's checks fail. Lists exactly
/// what failed and asks the agent to fix it and finish — deliberately framed to
/// iterate on the current work, never to restart, and never to fabricate a pass.
pub(crate) fn gate_instruction(failures: &[String]) -> String {
    let list = failures
        .iter()
        .map(|f| format!("• {}", crate::agents::goal::ellipsize(f, 500)))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "A done-ness check ran before your turn was allowed to finish and some \
         conditions are not yet met:\n\n{list}\n\n\
         Fix the underlying problem in the current work, then finish your turn. \
         the same checks re-run automatically and will let you stop once they all \
         pass. Do NOT restart from scratch, and do NOT claim the work is done \
         while a check is still failing; if a check cannot be satisfied, say so \
         explicitly and explain why."
    )
}

/// The user-facing notice when the gate spends its attempt budget without going
/// green: the agent is allowed to finish, but the user is told it is on unmet
/// conditions so a flaky/unsatisfiable check cannot silently wedge the turn.
pub(crate) fn giveup_notice(attempts: u32, failures: &[String]) -> String {
    let summary = failures
        .first()
        .map(|f| crate::agents::goal::ellipsize(f, 160))
        .unwrap_or_default();
    format!(
        "Done-ness checks still failing after {attempts} attempt(s); finishing \
         anyway. Unmet: {summary}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_off_and_inert() {
        let cfg = DoneGateConfig::default();
        assert!(!cfg.enabled, "BR-48 must default OFF: checks cost work");
        assert!(!cfg.is_active(), "no checks configured ⇒ inert");
    }

    #[test]
    fn enabled_but_no_checks_is_inert() {
        let cfg = DoneGateConfig {
            enabled: true,
            checks: Vec::new(),
            ..DoneGateConfig::default()
        };
        assert!(!cfg.is_active(), "an empty check list disables the gate");
    }

    #[test]
    fn enabled_with_zero_iterations_is_inert() {
        let cfg = DoneGateConfig {
            enabled: true,
            checks: vec![SuccessCheck::Shell {
                command: "true".into(),
            }],
            max_iterations: 0,
            ..DoneGateConfig::default()
        };
        assert!(!cfg.is_active(), "zero attempts disables even when enabled");
    }

    #[test]
    fn enabled_with_checks_and_budget_is_active() {
        let cfg = DoneGateConfig {
            enabled: true,
            checks: vec![SuccessCheck::Shell {
                command: "true".into(),
            }],
            max_iterations: 3,
            ..DoneGateConfig::default()
        };
        assert!(cfg.is_active());
    }

    #[test]
    fn gate_instruction_lists_failures_and_forbids_restart() {
        let text = gate_instruction(&[
            "command `cargo build` exited with status 101".to_string(),
            "expected file `out.csv` does not exist".to_string(),
        ]);
        assert!(text.contains("cargo build"));
        assert!(text.contains("out.csv"));
        // It must steer toward iterating, not resetting or faking a pass.
        assert!(text.contains("Do NOT restart from scratch"));
        assert!(text.to_lowercase().contains("not yet met"));
    }

    #[test]
    fn gate_instruction_ellipsizes_a_long_reason() {
        let long = "x".repeat(2_000);
        let text = gate_instruction(std::slice::from_ref(&long));
        assert!(text.contains('…'), "a long failure reason is truncated");
        assert!(text.len() < long.len() + 600);
    }

    #[test]
    fn giveup_notice_mentions_attempts_and_first_failure() {
        let notice = giveup_notice(3, &["tests still failing".to_string()]);
        assert!(notice.contains("3 attempt"));
        assert!(notice.contains("tests still failing"));
    }

    #[test]
    fn json_checks_parse_from_config_array_shape() {
        // The exact JSON a user would put in BIOROUTER_DONE_GATE_CHECKS must
        // deserialize into the shared SuccessCheck enum, including the new
        // non-shell variants and their snake_case aliases.
        let raw = r#"[
            {"type":"shell","command":"cargo build"},
            {"type":"file_exists","path":"out.csv"},
            {"type":"output_contains","command":"pytest","substring":"passed"},
            {"type":"json_schema","path":"r.json","schema":{"type":"object"}}
        ]"#;
        let checks: Vec<SuccessCheck> = serde_json::from_str(raw).expect("checks parse");
        assert_eq!(checks.len(), 4);
        assert!(matches!(checks[0], SuccessCheck::Shell { .. }));
        assert!(matches!(checks[1], SuccessCheck::FileExists { .. }));
        assert!(matches!(checks[2], SuccessCheck::OutputContains { .. }));
        assert!(matches!(checks[3], SuccessCheck::JsonSchema { .. }));
    }
}
