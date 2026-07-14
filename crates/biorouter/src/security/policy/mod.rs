//! Auditable command policy engine (BR-21, Slice 1).
//!
//! Replaces the command-governance half of the regex scanner with a
//! declarative, self-testing engine that (a) parses argv and canonicalizes
//! paths before matching (`command.rs`), (b) emits a first-class
//! `Allow | Ask | Deny` verdict with a per-rule justification (`rule.rs`), and
//! (c) ships an always-on embedded baseline (`baseline.rs` /
//! `baseline.policy.yaml`). The ML prompt-injection classifier is untouched.
//!
//! Slice 1 loads the embedded baseline only (all `deny` rules). Tiered external
//! files, the remaining `ask` rules, and the `SECURITY_COMMAND_POLICY` off/enforce
//! knob's finer states are follow-up slices; the tier/priority data model is
//! already in place so they slot in without a rewrite.

mod baseline;
mod cmd_shell;
pub(crate) mod command;
mod pwsh;
mod rule;
pub(crate) mod target;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_platform;

pub use command::ParsedCommand;
pub use rule::{Decision, PolicyVerdict, Rule, Tier};
pub use target::{Blast, Dialect, EnvFacts, Platform};

use rule::CompiledRule;
use serde_json::Value;
use std::path::Path;
use uuid::Uuid;

/// Per-platform self-test fixtures. A rule's embedded `matches` examples run
/// under the platform the rule declares (a Windows rule's example uses a Windows
/// cwd/home), so `Remove-Item -Recurse -Force C:\` classifies correctly on a
/// mac CI box.
fn self_test_fixture(platform: Platform) -> (&'static str, &'static str) {
    match platform {
        Platform::Windows => (r"C:\Users\me\project", r"C:\Users\me"),
        _ => (SELF_TEST_CWD, "/home/user"),
    }
}

/// A rule that failed its embedded self-test at load / in `cargo test`.
#[derive(Debug, Clone)]
pub struct SelfTestFailure {
    pub rule_id: String,
    pub command: String,
    /// The command was expected to match this rule (`true`) or not (`false`).
    pub expected_match: bool,
}

/// Fixed inputs for running a rule's embedded `tests` — the tool name is
/// irrelevant because baseline rules leave `tools` unset (all tools), and a
/// stable cwd keeps relative-path canonicalization deterministic.
const SELF_TEST_TOOL: &str = "shell";
const SELF_TEST_CWD: &str = "/home/user/project";

/// The resolved rule set + evaluation entry point.
pub struct PolicyEngine {
    rules: Vec<CompiledRule>,
}

impl PolicyEngine {
    /// Load the engine from all tiers. Slice 1 = the embedded baseline only.
    /// Rules that fail to compile are logged and skipped (fail-safe).
    pub fn load() -> Self {
        let engine = Self::with_rules(baseline::baseline_rules());
        if let Err(failures) = engine.run_self_tests() {
            // A shipped baseline should always pass; log loudly if it does not,
            // but do not panic the agent — the BR-20 floor still guards the worst.
            for f in &failures {
                tracing::error!(
                    rule_id = %f.rule_id,
                    command = %f.command,
                    expected_match = f.expected_match,
                    "Baseline command policy self-test failed"
                );
            }
        }
        engine
    }

    /// Build an engine from an explicit rule list (tests / future loaders).
    pub fn with_rules(rules: Vec<Rule>) -> Self {
        let mut compiled = Vec::with_capacity(rules.len());
        for rule in rules {
            let id = rule.id.clone();
            match CompiledRule::compile(rule) {
                Ok(cr) => compiled.push(cr),
                Err(e) => {
                    tracing::error!(rule_id = %id, error = %e, "Skipping invalid policy rule")
                }
            }
        }
        Self { rules: compiled }
    }

    /// Evaluate a tool call for the running host. Winner = highest effective
    /// priority; ties broken last-match-wins. No match = Allow.
    pub fn evaluate(&self, tool_name: &str, args: &Value, cwd: &Path) -> PolicyVerdict {
        let platform = Platform::host();
        let env = EnvFacts::host(&cwd.to_string_lossy());
        self.evaluate_with(tool_name, args, platform, platform.default_dialect(), &env)
    }

    /// Evaluate for an explicit target platform + environment, using that
    /// platform's default shell dialect. Used by the BR-68 test matrix to run
    /// the Windows rule set on a mac.
    pub fn evaluate_for(
        &self,
        platform: Platform,
        tool_name: &str,
        args: &Value,
        cwd: &str,
        home: &str,
    ) -> PolicyVerdict {
        self.evaluate_for_dialect(
            platform,
            platform.default_dialect(),
            tool_name,
            args,
            cwd,
            home,
        )
    }

    /// Evaluate for an explicit (platform × dialect). The dialect is stated by
    /// the test matrix because a bare command carries no wrapper to infer it.
    #[allow(clippy::too_many_arguments)]
    pub fn evaluate_for_dialect(
        &self,
        platform: Platform,
        dialect: Dialect,
        tool_name: &str,
        args: &Value,
        cwd: &str,
        home: &str,
    ) -> PolicyVerdict {
        let env = EnvFacts::for_platform(platform, cwd, home);
        self.evaluate_with(tool_name, args, platform, dialect, &env)
    }

    fn evaluate_with(
        &self,
        tool_name: &str,
        args: &Value,
        platform: Platform,
        dialect: Dialect,
        env: &EnvFacts,
    ) -> PolicyVerdict {
        let command = args
            .as_object()
            .and_then(|m| super::command_text_from(tool_name, m))
            .unwrap_or_default();
        let parsed = ParsedCommand::parse_for_dialect(&command, platform, dialect, env);

        let mut winner: Option<&CompiledRule> = None;
        let mut winner_priority: i64 = -1;
        for cr in &self.rules {
            if !cr.rule.enabled {
                continue;
            }
            if cr.matches(tool_name, &parsed) {
                let eff = cr.rule.effective_priority() as i64;
                // `>=` so, on equal priority, the later rule wins (last-match-wins).
                if eff >= winner_priority {
                    winner_priority = eff;
                    winner = Some(cr);
                }
            }
        }

        let finding_id = format!("POL-{}", Uuid::new_v4().simple());
        match winner {
            Some(cr) => PolicyVerdict {
                decision: cr.rule.decision,
                rule_id: Some(cr.rule.id.clone()),
                justification: cr.rule.justification.clone(),
                finding_id,
            },
            None => PolicyVerdict {
                decision: Decision::Allow,
                rule_id: None,
                justification: String::new(),
                finding_id,
            },
        }
    }

    /// Run every rule's embedded `matches` / `not_matches` examples. Returns the
    /// list of failures so a `#[test]` can assert the baseline is self-consistent
    /// (the "auditable" guarantee: a rule edit that breaks its examples fails CI).
    pub fn run_self_tests(&self) -> Result<(), Vec<SelfTestFailure>> {
        let mut failures = Vec::new();
        for cr in &self.rules {
            // Run each rule under a platform it declares (Windows rules need a
            // Windows cwd/home to classify their targets). The shared baseline's
            // embedded examples are POSIX fixtures, so keep their oracle
            // deterministic instead of interpreting them in the host dialect.
            let platform = cr
                .rule
                .platforms
                .first()
                .copied()
                .unwrap_or(Platform::Linux);
            let (cwd, home) = self_test_fixture(platform);
            let env = EnvFacts::for_platform(platform, cwd, home);
            for cmd in &cr.rule.tests.matches {
                let parsed = ParsedCommand::parse_for(cmd, platform, &env);
                if !cr.matches(SELF_TEST_TOOL, &parsed) {
                    failures.push(SelfTestFailure {
                        rule_id: cr.rule.id.clone(),
                        command: cmd.clone(),
                        expected_match: true,
                    });
                }
            }
            for cmd in &cr.rule.tests.not_matches {
                let parsed = ParsedCommand::parse_for(cmd, platform, &env);
                if cr.matches(SELF_TEST_TOOL, &parsed) {
                    failures.push(SelfTestFailure {
                        rule_id: cr.rule.id.clone(),
                        command: cmd.clone(),
                        expected_match: false,
                    });
                }
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures)
        }
    }
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::load()
    }
}
