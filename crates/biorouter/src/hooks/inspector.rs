//! PreToolUse hooks plugged into the tool inspection pipeline.
//!
//! Hook decisions map onto inspection actions: `deny` -> `Deny` (the reason
//! is fed back to the model), `ask` -> `RequireApproval` (rides the existing
//! user-confirmation round trip), `allow`/no decision -> no result (other
//! inspectors still apply; to auto-approve past the permission prompt, use a
//! PermissionRequest hook instead).

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use super::{HookDecision, HooksManager};
use crate::config::BioRouterMode;
use crate::conversation::message::{Message, ToolRequest};
use crate::observability::loop_safety::{self, LoopSafetyEvent, LoopSafetyKind};
use crate::session::Session;
use crate::tool_inspection::{InspectionAction, InspectionResult, ToolInspector};

pub const HOOK_INSPECTOR_NAME: &str = "hooks";

pub struct HookInspector {
    hooks: Arc<HooksManager>,
}

impl HookInspector {
    pub fn new(hooks: Arc<HooksManager>) -> Self {
        Self { hooks }
    }
}

#[async_trait]
impl ToolInspector for HookInspector {
    fn name(&self) -> &'static str {
        HOOK_INSPECTOR_NAME
    }

    async fn inspect(
        &self,
        tool_requests: &[ToolRequest],
        _messages: &[Message],
        _biorouter_mode: BioRouterMode,
        session: &Session,
    ) -> Result<Vec<InspectionResult>> {
        let mut results = Vec::new();
        for request in tool_requests {
            let Ok(tool_call) = &request.tool_call else {
                continue;
            };
            let tool_name = tool_call.name.to_string();
            let tool_input = tool_call
                .arguments
                .clone()
                .map(serde_json::Value::Object)
                .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));

            let aggregate = self
                .hooks
                .pre_tool_use(&session.id, &session.working_dir, &tool_name, &tool_input)
                .await;

            match aggregate.decision {
                Some(HookDecision::Deny { reason }) => {
                    // BR-67: a hook veto is a loop-safety stop like any other —
                    // record which tool was blocked (never the hook's reason
                    // text, which is free-form and can quote the call).
                    loop_safety::emit(
                        LoopSafetyEvent::new(LoopSafetyKind::HookBlock)
                            .session(&session.id)
                            .tool(&tool_name),
                    );
                    results.push(InspectionResult {
                        tool_request_id: request.id.clone(),
                        action: InspectionAction::Deny,
                        reason,
                        confidence: 1.0,
                        inspector_name: HOOK_INSPECTOR_NAME.to_string(),
                        finding_id: None,
                    });
                }
                Some(HookDecision::Ask { reason }) => {
                    loop_safety::emit(
                        LoopSafetyEvent::new(LoopSafetyKind::HookAsk)
                            .session(&session.id)
                            .tool(&tool_name),
                    );
                    let warning = reason
                        .unwrap_or_else(|| format!("A hook requires approval for {tool_name}"));
                    results.push(InspectionResult {
                        tool_request_id: request.id.clone(),
                        action: InspectionAction::RequireApproval(Some(warning.clone())),
                        reason: warning,
                        confidence: 1.0,
                        inspector_name: HOOK_INSPECTOR_NAME.to_string(),
                        finding_id: None,
                    });
                }
                Some(HookDecision::Allow { .. }) | None => {}
            }
        }
        Ok(results)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
