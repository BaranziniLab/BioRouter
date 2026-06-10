//! Hook results: the JSON a hook may print on stdout (camelCase, matching
//! Claude Code so existing hooks port over), per-hook outcomes, and the
//! merged aggregate across all hooks that ran for one event.

use serde::Deserialize;

use super::event::HookEvent;

fn default_true() -> bool {
    true
}

/// JSON document a command hook may print on stdout with exit code 0.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookOutput {
    /// "approve" | "block" (top-level decision, used by Stop/UserPromptSubmit).
    pub decision: Option<String>,
    pub reason: Option<String>,
    /// Message surfaced to the user (yellow inline notification).
    pub system_message: Option<String>,
    #[serde(default = "default_true", rename = "continue")]
    pub continue_: bool,
    pub stop_reason: Option<String>,
    pub hook_specific_output: Option<HookSpecificOutput>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookSpecificOutput {
    pub hook_event_name: Option<String>,
    /// "allow" | "deny" | "ask" (PreToolUse / PermissionRequest).
    pub permission_decision: Option<String>,
    pub permission_decision_reason: Option<String>,
    /// Context injected for the model (hidden from the user).
    pub additional_context: Option<String>,
}

/// Normalized decision from a single hook, ordered by restrictiveness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookDecision {
    Allow { reason: Option<String> },
    Ask { reason: Option<String> },
    Deny { reason: String },
}

impl HookDecision {
    fn rank(&self) -> u8 {
        match self {
            HookDecision::Allow { .. } => 0,
            HookDecision::Ask { .. } => 1,
            HookDecision::Deny { .. } => 2,
        }
    }
}

/// The outcome of running one hook definition.
#[derive(Debug, Default)]
pub struct HookOutcome {
    pub decision: Option<HookDecision>,
    pub additional_context: Option<String>,
    pub system_message: Option<String>,
    /// Non-blocking failure (timeout, spawn error, bad exit code) — recorded
    /// but never blocks (failure-open).
    pub error: Option<String>,
}

/// Merged result of all hooks that ran for one event.
#[derive(Debug, Default)]
pub struct HookAggregate {
    /// Most restrictive decision wins: Deny > Ask > Allow.
    pub decision: Option<HookDecision>,
    pub additional_context: Vec<String>,
    pub system_messages: Vec<String>,
    pub errors: Vec<String>,
}

impl HookAggregate {
    pub fn is_denied(&self) -> bool {
        matches!(self.decision, Some(HookDecision::Deny { .. }))
    }

    pub fn deny_reason(&self) -> Option<&str> {
        match &self.decision {
            Some(HookDecision::Deny { reason }) => Some(reason.as_str()),
            _ => None,
        }
    }

    pub fn joined_context(&self) -> Option<String> {
        if self.additional_context.is_empty() {
            None
        } else {
            Some(self.additional_context.join("\n\n"))
        }
    }
}

/// Merge per-hook outcomes; most restrictive decision wins, contexts and
/// messages concatenate in hook order.
pub fn merge_outcomes(outcomes: Vec<HookOutcome>) -> HookAggregate {
    let mut aggregate = HookAggregate::default();
    for outcome in outcomes {
        if let Some(decision) = outcome.decision {
            let replace = aggregate
                .decision
                .as_ref()
                .map(|current| decision.rank() > current.rank())
                .unwrap_or(true);
            if replace {
                aggregate.decision = Some(decision);
            }
        }
        if let Some(ctx) = outcome.additional_context {
            if !ctx.trim().is_empty() {
                aggregate.additional_context.push(ctx);
            }
        }
        if let Some(msg) = outcome.system_message {
            if !msg.trim().is_empty() {
                aggregate.system_messages.push(msg);
            }
        }
        if let Some(err) = outcome.error {
            aggregate.errors.push(err);
        }
    }
    aggregate
}

/// Interpret a finished command hook (exit status + stdout/stderr) as a
/// `HookOutcome`, applying Claude Code exit-code semantics:
/// - exit 0: parse stdout as `HookOutput` JSON; non-JSON stdout becomes
///   additional context for context-accepting events, otherwise log-only.
/// - exit 2: blocking — deny with stderr as the reason.
/// - anything else: non-blocking error (failure-open).
pub fn interpret_command_result(
    event: HookEvent,
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> HookOutcome {
    match exit_code {
        Some(0) => interpret_stdout(event, stdout),
        Some(2) => {
            let reason = if stderr.trim().is_empty() {
                format!("{} blocked by hook", event)
            } else {
                stderr.trim().to_string()
            };
            HookOutcome {
                decision: Some(HookDecision::Deny { reason }),
                ..Default::default()
            }
        }
        code => HookOutcome {
            error: Some(format!(
                "hook exited with {} (stderr: {})",
                code.map(|c| c.to_string())
                    .unwrap_or_else(|| "signal".to_string()),
                stderr.trim()
            )),
            ..Default::default()
        },
    }
}

/// Events where bare (non-JSON) stdout from an exit-0 hook is treated as
/// additional context for the model.
fn accepts_raw_context(event: HookEvent) -> bool {
    matches!(event, HookEvent::UserPromptSubmit | HookEvent::SessionStart)
}

fn interpret_stdout(event: HookEvent, stdout: &str) -> HookOutcome {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return HookOutcome::default();
    }
    let Ok(output) = serde_json::from_str::<HookOutput>(trimmed) else {
        if accepts_raw_context(event) {
            return HookOutcome {
                additional_context: Some(trimmed.to_string()),
                ..Default::default()
            };
        }
        return HookOutcome::default();
    };

    let mut outcome = HookOutcome {
        system_message: output.system_message,
        ..Default::default()
    };

    // Per-event decision fields, most specific first.
    if let Some(specific) = output.hook_specific_output {
        outcome.additional_context = specific.additional_context;
        if let Some(permission) = specific.permission_decision {
            let reason = specific.permission_decision_reason;
            outcome.decision = match permission.as_str() {
                "deny" => Some(HookDecision::Deny {
                    reason: reason.unwrap_or_else(|| "denied by hook".to_string()),
                }),
                "ask" => Some(HookDecision::Ask { reason }),
                "allow" => Some(HookDecision::Allow { reason }),
                other => {
                    outcome.error = Some(format!("unknown permissionDecision '{}'", other));
                    None
                }
            };
        }
    }

    if outcome.decision.is_none() {
        if let Some(decision) = output.decision {
            outcome.decision = match decision.as_str() {
                "block" => Some(HookDecision::Deny {
                    reason: output
                        .reason
                        .clone()
                        .unwrap_or_else(|| format!("{} blocked by hook", event)),
                }),
                "approve" | "allow" => Some(HookDecision::Allow {
                    reason: output.reason.clone(),
                }),
                other => {
                    outcome.error = Some(format!("unknown decision '{}'", other));
                    None
                }
            };
        }
    }

    // {"continue": false} is an alternate way to block.
    if !output.continue_ && outcome.decision.is_none() {
        outcome.decision = Some(HookDecision::Deny {
            reason: output
                .stop_reason
                .or(output.reason)
                .unwrap_or_else(|| format!("{} stopped by hook", event)),
        });
    }

    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_zero_no_output_is_neutral() {
        let outcome = interpret_command_result(HookEvent::PreToolUse, Some(0), "", "");
        assert!(outcome.decision.is_none());
        assert!(outcome.error.is_none());
    }

    #[test]
    fn exit_two_denies_with_stderr() {
        let outcome =
            interpret_command_result(HookEvent::PreToolUse, Some(2), "", "rm is forbidden\n");
        assert_eq!(
            outcome.decision,
            Some(HookDecision::Deny {
                reason: "rm is forbidden".to_string()
            })
        );
    }

    #[test]
    fn other_exit_codes_fail_open() {
        let outcome = interpret_command_result(HookEvent::PreToolUse, Some(1), "", "oops");
        assert!(outcome.decision.is_none());
        assert!(outcome.error.is_some());
    }

    #[test]
    fn permission_decision_json_parses() {
        let stdout = r#"{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"nope"}}"#;
        let outcome = interpret_command_result(HookEvent::PreToolUse, Some(0), stdout, "");
        assert_eq!(
            outcome.decision,
            Some(HookDecision::Deny {
                reason: "nope".to_string()
            })
        );
    }

    #[test]
    fn ask_decision_and_context_parse() {
        let stdout = r#"{"hookSpecificOutput":{"permissionDecision":"ask","permissionDecisionReason":"verify","additionalContext":"be careful"},"systemMessage":"heads up"}"#;
        let outcome = interpret_command_result(HookEvent::PreToolUse, Some(0), stdout, "");
        assert_eq!(
            outcome.decision,
            Some(HookDecision::Ask {
                reason: Some("verify".to_string())
            })
        );
        assert_eq!(outcome.additional_context.as_deref(), Some("be careful"));
        assert_eq!(outcome.system_message.as_deref(), Some("heads up"));
    }

    #[test]
    fn top_level_block_decision_parses() {
        let stdout = r#"{"decision":"block","reason":"tests failed"}"#;
        let outcome = interpret_command_result(HookEvent::Stop, Some(0), stdout, "");
        assert_eq!(
            outcome.decision,
            Some(HookDecision::Deny {
                reason: "tests failed".to_string()
            })
        );
    }

    #[test]
    fn continue_false_blocks() {
        let stdout = r#"{"continue": false, "stopReason": "halt"}"#;
        let outcome = interpret_command_result(HookEvent::UserPromptSubmit, Some(0), stdout, "");
        assert_eq!(
            outcome.decision,
            Some(HookDecision::Deny {
                reason: "halt".to_string()
            })
        );
    }

    #[test]
    fn raw_stdout_is_context_only_for_context_events() {
        let outcome =
            interpret_command_result(HookEvent::UserPromptSubmit, Some(0), "remember X", "");
        assert_eq!(outcome.additional_context.as_deref(), Some("remember X"));
        let outcome = interpret_command_result(HookEvent::PreToolUse, Some(0), "remember X", "");
        assert!(outcome.additional_context.is_none());
    }

    #[test]
    fn merge_prefers_most_restrictive() {
        let aggregate = merge_outcomes(vec![
            HookOutcome {
                decision: Some(HookDecision::Allow { reason: None }),
                ..Default::default()
            },
            HookOutcome {
                decision: Some(HookDecision::Deny {
                    reason: "no".to_string(),
                }),
                ..Default::default()
            },
            HookOutcome {
                decision: Some(HookDecision::Ask { reason: None }),
                additional_context: Some("ctx".to_string()),
                ..Default::default()
            },
        ]);
        assert!(aggregate.is_denied());
        assert_eq!(aggregate.deny_reason(), Some("no"));
        assert_eq!(aggregate.joined_context().as_deref(), Some("ctx"));
    }
}
