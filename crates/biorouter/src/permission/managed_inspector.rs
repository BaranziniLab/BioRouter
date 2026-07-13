//! Managed-policy tool inspector (BR-65).
//!
//! Emits `Deny` / `RequireApproval` for tools governed by a trusted admin
//! managed policy. Registered **first**, and — being a non-`permission`
//! inspector — its verdicts ride the escalation-only override merge
//! (`tool_inspection::apply_inspection_results_to_permissions`), so a managed
//! `Deny`/`Ask` wins over everything, including `Auto` mode's blanket `Allow`,
//! and cannot be lowered by any later inspector.
//!
//! Managed **allow** is intentionally *not* emitted here: escalation cannot
//! lower a user `Deny`, so a force-allow is applied as the permission baseline
//! inside [`super::permission_inspector::PermissionInspector`] instead.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use crate::config::BioRouterMode;
use crate::conversation::message::{Message, ToolRequest};
use crate::managed::{ManagedPolicy, ManagedVerdict};
use crate::tool_inspection::{InspectionAction, InspectionResult, ToolInspector};

/// User-facing reason for a managed denial, so the block reads as an org policy
/// rather than a generic decline (design open question #3).
pub const MANAGED_DENY_REASON: &str = "Blocked by your organization's managed policy.";
/// User-facing reason for a managed approval requirement.
pub const MANAGED_ASK_REASON: &str =
    "Your organization's managed policy requires approval for this tool.";

pub struct ManagedPolicyInspector {
    managed: Arc<ManagedPolicy>,
}

impl ManagedPolicyInspector {
    pub fn new(managed: Arc<ManagedPolicy>) -> Self {
        Self { managed }
    }
}

#[async_trait]
impl ToolInspector for ManagedPolicyInspector {
    fn name(&self) -> &'static str {
        "managed"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn is_enabled(&self) -> bool {
        // A machine with no trusted managed file pays nothing.
        self.managed.is_active()
    }

    async fn inspect(
        &self,
        tool_requests: &[ToolRequest],
        _messages: &[Message],
        _biorouter_mode: BioRouterMode,
        _session: &crate::session::Session,
    ) -> Result<Vec<InspectionResult>> {
        let mut results = Vec::new();
        for request in tool_requests {
            let Ok(tool_call) = &request.tool_call else {
                continue;
            };
            let (action, reason, finding) = match self.managed.permission_for(&tool_call.name) {
                Some(ManagedVerdict::Deny) => (
                    InspectionAction::Deny,
                    MANAGED_DENY_REASON.to_string(),
                    "MANAGED-DENY",
                ),
                Some(ManagedVerdict::Ask) => (
                    InspectionAction::RequireApproval(Some(MANAGED_ASK_REASON.to_string())),
                    MANAGED_ASK_REASON.to_string(),
                    "MANAGED-ASK",
                ),
                // Allow is applied as the permission baseline, not as an
                // escalation override; None means no managed rule applies.
                Some(ManagedVerdict::Allow) | None => continue,
            };
            results.push(InspectionResult {
                tool_request_id: request.id.clone(),
                action,
                reason,
                confidence: 1.0,
                inspector_name: self.name().to_string(),
                finding_id: Some(finding.to_string()),
            });
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed::{ManagedPolicy, ManagedPolicyFile};
    use rmcp::model::CallToolRequestParams;
    use rmcp::object;

    fn tool_request(id: &str, name: &str) -> ToolRequest {
        ToolRequest {
            id: id.to_string(),
            tool_call: Ok(CallToolRequestParams {
                task: None,
                name: name.to_string().into(),
                arguments: Some(object!({})),
                meta: None,
            }),
            metadata: None,
            tool_meta: None,
        }
    }

    fn inspector_from_yaml(yaml: &str) -> ManagedPolicyInspector {
        let file: ManagedPolicyFile = serde_yaml::from_str(yaml).unwrap();
        ManagedPolicyInspector::new(Arc::new(ManagedPolicy::from_file(file)))
    }

    #[test]
    fn inert_when_no_managed_file() {
        let inspector = ManagedPolicyInspector::new(Arc::new(ManagedPolicy::empty()));
        assert!(!inspector.is_enabled());
    }

    #[tokio::test]
    async fn deny_and_ask_emitted_allow_skipped() {
        let inspector = inspector_from_yaml(
            "permissions:\n  deny: [\"developer__shell\"]\n  ask: [\"memory__store\"]\n  allow: [\"developer__text_editor\"]\n",
        );
        assert!(inspector.is_enabled());
        let requests = vec![
            tool_request("a", "developer__shell"),
            tool_request("b", "memory__store"),
            tool_request("c", "developer__text_editor"),
            tool_request("d", "some__other_tool"),
        ];
        let results = inspector
            .inspect(
                &requests,
                &[],
                BioRouterMode::Auto,
                &crate::session::Session::default(),
            )
            .await
            .unwrap();

        // Deny + Ask produce results; Allow and unmatched produce none.
        assert_eq!(results.len(), 2);
        let deny = results.iter().find(|r| r.tool_request_id == "a").unwrap();
        assert_eq!(deny.action, InspectionAction::Deny);
        assert_eq!(deny.inspector_name, "managed");
        let ask = results.iter().find(|r| r.tool_request_id == "b").unwrap();
        assert!(matches!(ask.action, InspectionAction::RequireApproval(_)));
        assert!(!results.iter().any(|r| r.tool_request_id == "c"));
        assert!(!results.iter().any(|r| r.tool_request_id == "d"));
    }
}
