use anyhow::Result;
use async_trait::async_trait;

use crate::config::BioRouterMode;
use crate::conversation::message::{Message, ToolRequest};
use crate::security::{SecurityManager, SecurityResult};
use crate::tool_inspection::{InspectionAction, InspectionResult, ToolInspector};

/// Security inspector that uses pattern matching to detect malicious tool calls
pub struct SecurityInspector {
    security_manager: SecurityManager,
}

impl SecurityInspector {
    pub fn new() -> Self {
        Self {
            security_manager: SecurityManager::new(),
        }
    }

    /// Convert SecurityResult to InspectionResult
    fn convert_security_result(
        &self,
        security_result: &SecurityResult,
        tool_request_id: String,
    ) -> InspectionResult {
        let action = if security_result.is_malicious && security_result.should_ask_user {
            // High confidence threat - require user approval with warning
            InspectionAction::RequireApproval(Some(format!(
                "🔒 Security Alert: This tool call has been flagged as potentially dangerous.\n\
                Confidence: {:.1}%\n\
                Explanation: {}\n\
                Finding ID: {}",
                security_result.confidence * 100.0,
                security_result.explanation,
                security_result.finding_id
            )))
        } else {
            // Either not malicious, or below threshold (already logged) - allow
            InspectionAction::Allow
        };

        InspectionResult {
            tool_request_id,
            action,
            reason: security_result.explanation.clone(),
            confidence: security_result.confidence,
            inspector_name: self.name().to_string(),
            finding_id: Some(security_result.finding_id.clone()),
        }
    }
}

#[async_trait]
impl ToolInspector for SecurityInspector {
    fn name(&self) -> &'static str {
        "security"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    async fn inspect(
        &self,
        tool_requests: &[ToolRequest],
        messages: &[Message],
        _biorouter_mode: BioRouterMode,
        _session: &crate::session::Session,
    ) -> Result<Vec<InspectionResult>> {
        let mut inspection_results = Vec::new();

        // 1. Always-on catastrophic-command denylist (non-bypassable). This runs
        //    regardless of `SECURITY_PROMPT_ENABLED` or permission mode, so a
        //    handful of unrecoverable commands are hard-blocked even in Auto mode.
        let mut hard_blocked: std::collections::HashSet<String> = std::collections::HashSet::new();
        for block in self.security_manager.catastrophic_blocks(tool_requests) {
            hard_blocked.insert(block.tool_request_id.clone());
            inspection_results.push(InspectionResult {
                tool_request_id: block.tool_request_id,
                action: InspectionAction::Deny,
                reason: block.message,
                confidence: 1.0,
                inspector_name: self.name().to_string(),
                finding_id: Some(format!("SEC-CATASTROPHIC-{}", block.rule_name)),
            });
        }

        // 2. Optional prompt-injection / dangerous-command scan (asks, does not
        //    hard-block). Gated on config and skipped for already hard-blocked
        //    requests so a call is never both denied and escalated for approval.
        if self
            .security_manager
            .is_prompt_injection_detection_enabled()
        {
            let security_results = self
                .security_manager
                .analyze_tool_requests(tool_requests, messages)
                .await?;

            // The SecurityManager already correlates tool requests and results.
            inspection_results.extend(
                security_results
                    .into_iter()
                    .filter(|r| !hard_blocked.contains(&r.tool_request_id))
                    .map(|security_result| {
                        let tool_request_id = security_result.tool_request_id.clone();
                        self.convert_security_result(&security_result, tool_request_id)
                    }),
            );
        }

        Ok(inspection_results)
    }

    fn is_enabled(&self) -> bool {
        // Always enabled: the catastrophic-command denylist must run in every
        // mode. The optional prompt-injection scan is gated separately inside
        // `inspect` on `SECURITY_PROMPT_ENABLED`.
        true
    }
}

impl Default for SecurityInspector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::ToolRequest;
    use rmcp::model::CallToolRequestParams;
    use rmcp::object;

    fn shell_request(id: &str, command: &str) -> ToolRequest {
        ToolRequest {
            id: id.to_string(),
            tool_call: Ok(CallToolRequestParams {
                task: None,
                name: "developer__shell".into(),
                arguments: Some(object!({ "command": command })),
                meta: None,
            }),
            metadata: None,
            tool_meta: None,
        }
    }

    /// A catastrophic command is hard-blocked (Deny) even in Auto mode, and the
    /// error names the rule — regardless of `SECURITY_PROMPT_ENABLED`.
    #[tokio::test]
    async fn test_catastrophic_command_hard_blocked_in_auto_mode() {
        let inspector = SecurityInspector::new();
        let tool_requests = vec![shell_request("req_rm", "rm -rf /")];

        let results = inspector
            .inspect(
                &tool_requests,
                &[],
                BioRouterMode::Auto,
                &crate::session::Session::default(),
            )
            .await
            .unwrap();

        let deny = results
            .iter()
            .find(|r| r.tool_request_id == "req_rm")
            .expect("catastrophic command should produce an inspection result");
        assert_eq!(deny.action, InspectionAction::Deny);
        assert_eq!(deny.inspector_name, "security");
        assert!(
            deny.reason.contains("rm_rf_root"),
            "block reason should name the rule, got: {}",
            deny.reason
        );
        assert!(
            deny.reason.contains("cannot be bypassed"),
            "block reason should state it is non-bypassable, got: {}",
            deny.reason
        );
    }

    /// A legitimate near-miss is not blocked (and, with the scanner off by
    /// default, produces no results at all).
    #[tokio::test]
    async fn test_benign_command_not_blocked() {
        let inspector = SecurityInspector::new();
        let tool_requests = vec![shell_request("req_ok", "rm -rf ./node_modules")];

        let results = inspector
            .inspect(
                &tool_requests,
                &[],
                BioRouterMode::Auto,
                &crate::session::Session::default(),
            )
            .await
            .unwrap();

        assert!(
            !results
                .iter()
                .any(|r| r.tool_request_id == "req_ok" && r.action == InspectionAction::Deny),
            "a benign rm of a subdirectory must not be hard-blocked"
        );
    }

    #[test]
    fn test_security_inspector_always_enabled() {
        // The inspector must always run so the catastrophic denylist is active
        // in every mode, independent of config.
        let inspector = SecurityInspector::new();
        assert!(inspector.is_enabled());
    }

    #[test]
    fn test_security_inspector_name() {
        let inspector = SecurityInspector::new();
        assert_eq!(inspector.name(), "security");
    }
}
