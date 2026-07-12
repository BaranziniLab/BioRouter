pub mod classification_client;
pub mod patterns;
pub mod scanner;
pub mod security_inspector;

use crate::config::Config;
use crate::conversation::message::{Message, ToolRequest};
use crate::permission::permission_judge::PermissionCheckResult;
use anyhow::Result;
use rmcp::model::CallToolRequestParams;
use scanner::PromptInjectionScanner;
use std::sync::OnceLock;
use uuid::Uuid;

/// A catastrophic-command hard block. These fire regardless of permission mode
/// or `SECURITY_PROMPT_ENABLED` and cannot be bypassed by config.
#[derive(Debug, Clone)]
pub struct CatastrophicBlock {
    pub tool_request_id: String,
    pub rule_name: &'static str,
    /// A self-contained, user-facing message naming the rule that fired.
    pub message: String,
}

/// Argument keys that carry an executable command string (scanned on any tool).
const COMMAND_ARG_KEYS: &[&str] = &["command", "cmd", "commands", "shell_command"];

/// Substrings in a tool name that mark it as a shell/command executor (scan all
/// of its string arguments, not just the command-bearing keys above).
const SHELL_TOOL_HINTS: &[&str] = &["shell", "bash", "terminal"];

pub struct SecurityManager {
    scanner: OnceLock<PromptInjectionScanner>,
}

#[derive(Debug, Clone)]
pub struct SecurityResult {
    pub is_malicious: bool,
    pub confidence: f32,
    pub explanation: String,
    pub should_ask_user: bool,
    pub finding_id: String,
    pub tool_request_id: String,
}

impl SecurityManager {
    pub fn new() -> Self {
        Self {
            scanner: OnceLock::new(),
        }
    }

    pub fn is_prompt_injection_detection_enabled(&self) -> bool {
        let config = Config::global();

        config
            .get_param::<bool>("SECURITY_PROMPT_ENABLED")
            .unwrap_or(false)
    }

    fn is_ml_scanning_enabled(&self) -> bool {
        let config = Config::global();

        config
            .get_param::<bool>("SECURITY_PROMPT_CLASSIFIER_ENABLED")
            .unwrap_or(false)
    }

    /// Scan tool calls against the always-on catastrophic-command denylist.
    ///
    /// This runs unconditionally — independent of `SECURITY_PROMPT_ENABLED` and
    /// of the session's permission mode — so that a handful of unrecoverable
    /// commands (e.g. `rm -rf /`, disk wipes, fork bombs) are hard-blocked even
    /// in `Auto` mode. Blocks are returned as `Deny` decisions by the security
    /// inspector and cannot be overridden by the user.
    pub fn catastrophic_blocks(&self, tool_requests: &[ToolRequest]) -> Vec<CatastrophicBlock> {
        let mut blocks = Vec::new();
        for tool_request in tool_requests {
            let Ok(tool_call) = &tool_request.tool_call else {
                continue;
            };
            let Some(text) = command_text(tool_call) else {
                continue;
            };
            if let Some(rule) = patterns::match_catastrophic_command(&text) {
                let message = format!(
                    "Blocked by BioRouter's always-on catastrophic-command denylist \
                     (rule: {}): {}. This safety rule cannot be bypassed by permission mode.",
                    rule.name, rule.description
                );
                tracing::warn!(
                    counter.biorouter.catastrophic_command_blocked = 1,
                    tool_name = %tool_call.name,
                    tool_request_id = %tool_request.id,
                    rule = rule.name,
                    "Catastrophic command hard-blocked (non-bypassable)"
                );
                blocks.push(CatastrophicBlock {
                    tool_request_id: tool_request.id.clone(),
                    rule_name: rule.name,
                    message,
                });
            }
        }
        blocks
    }

    pub async fn analyze_tool_requests(
        &self,
        tool_requests: &[ToolRequest],
        messages: &[Message],
    ) -> Result<Vec<SecurityResult>> {
        if !self.is_prompt_injection_detection_enabled() {
            tracing::debug!(
                counter.biorouter.prompt_injection_scanner_disabled = 1,
                "Security scanning disabled"
            );
            return Ok(vec![]);
        }

        let scanner = self.scanner.get_or_init(|| {
            let ml_enabled = self.is_ml_scanning_enabled();

            let scanner = if ml_enabled {
                match PromptInjectionScanner::with_ml_detection() {
                    Ok(s) => {
                        tracing::info!(
                            counter.biorouter.prompt_injection_scanner_enabled = 1,
                            "🔓 Security scanner initialized with ML-based detection"
                        );
                        s
                    }
                    Err(e) => {
                        let error_chain = format!("{:#}", e);
                        tracing::warn!(
                            "⚠️ ML scanning requested but failed to initialize. Falling back to pattern-only scanning.\n\nError details:\n{}",
                            error_chain
                        );
                        PromptInjectionScanner::new()
                    }
                }
            } else {
                tracing::info!(
                    counter.biorouter.prompt_injection_scanner_enabled = 1,
                    "🔓 Security scanner initialized with pattern-based detection only"
                );
                PromptInjectionScanner::new()
            };

            scanner
        });

        let mut results = Vec::new();

        tracing::info!(
            "🔍 Starting security analysis - {} tool requests, {} messages",
            tool_requests.len(),
            messages.len()
        );

        for tool_request in tool_requests.iter() {
            if let Ok(tool_call) = &tool_request.tool_call {
                let analysis_result = scanner
                    .analyze_tool_call_with_context(tool_call, messages)
                    .await?;

                let config_threshold = scanner.get_threshold_from_config();
                let sanitized_explanation = analysis_result.explanation.replace('\n', " | ");

                if analysis_result.is_malicious {
                    let above_threshold = analysis_result.confidence > config_threshold;
                    let finding_id = format!("SEC-{}", Uuid::new_v4().simple());

                    tracing::warn!(
                        counter.biorouter.prompt_injection_finding = 1,
                        above_threshold = above_threshold,
                        tool_name = %tool_call.name,
                        tool_request_id = %tool_request.id,
                        confidence = analysis_result.confidence,
                        explanation = %sanitized_explanation,
                        finding_id = %finding_id,
                        threshold = config_threshold,
                        "{}",
                        if above_threshold {
                            "Current tool call flagged as malicious after security analysis (above threshold)"
                        } else {
                            "Security finding below threshold - logged but not blocking execution"
                        }
                    );
                    if above_threshold {
                        results.push(SecurityResult {
                            is_malicious: analysis_result.is_malicious,
                            confidence: analysis_result.confidence,
                            explanation: analysis_result.explanation,
                            should_ask_user: true, // Always ask user for threats above threshold
                            finding_id,
                            tool_request_id: tool_request.id.clone(),
                        });
                    }
                } else {
                    tracing::info!(
                        tool_name = %tool_call.name,
                        tool_request_id = %tool_request.id,
                        confidence = analysis_result.confidence,
                        explanation = %sanitized_explanation,
                        "✅ Current tool call passed security analysis"
                    );
                }
            }
        }

        tracing::info!(
            counter.biorouter.prompt_injection_analysis_performed = 1,
            security_issues_found = results.len(),
            "Security analysis complete"
        );
        Ok(results)
    }

    pub async fn filter_malicious_tool_calls(
        &self,
        messages: &[Message],
        permission_check_result: &PermissionCheckResult,
        _system_prompt: Option<&str>,
    ) -> Result<Vec<SecurityResult>> {
        let tool_requests: Vec<_> = permission_check_result
            .approved
            .iter()
            .chain(permission_check_result.needs_approval.iter())
            .cloned()
            .collect();

        self.analyze_tool_requests(&tool_requests, messages).await
    }
}

impl Default for SecurityManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract the command text to screen from a tool call: the string values of
/// any command-bearing argument, plus every string argument when the tool name
/// looks like a shell/command executor. Returns `None` when there is nothing
/// command-like to scan (so file contents written by an editor are not screened).
fn command_text(tool_call: &CallToolRequestParams) -> Option<String> {
    let args = tool_call.arguments.as_ref()?;
    let name = tool_call.name.to_ascii_lowercase();
    let shell_like = SHELL_TOOL_HINTS.iter().any(|hint| name.contains(hint));

    let mut parts: Vec<String> = Vec::new();
    for (key, value) in args.iter() {
        let key_lc = key.to_ascii_lowercase();
        let is_command_key = COMMAND_ARG_KEYS.iter().any(|k| key_lc == *k);
        if shell_like || is_command_key {
            collect_strings(value, &mut parts);
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

/// Recursively collect every string scalar in a JSON value (handles arrays of
/// commands and nested objects).
fn collect_strings(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(s) => out.push(s.clone()),
        serde_json::Value::Array(items) => {
            for item in items {
                collect_strings(item, out);
            }
        }
        serde_json::Value::Object(map) => {
            for v in map.values() {
                collect_strings(v, out);
            }
        }
        _ => {}
    }
}
