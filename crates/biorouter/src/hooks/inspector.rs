//! PreToolUse hooks plugged into the tool inspection pipeline.
//!
//! Hook decisions map onto inspection actions: `deny` -> `Deny` (the reason
//! is fed back to the model), `ask` -> `RequireApproval` (rides the existing
//! user-confirmation round trip), `allow`/no decision -> no result (other
//! inspectors still apply; to auto-approve past the permission prompt, use a
//! PermissionRequest hook instead).
//!
//! BR-19: a `ToolInspector` can only answer allow/deny/ask, so everything else a
//! PreToolUse hook returns — an `updatedInput` rewrite, `additionalContext`, a
//! `systemMessage` — is *staged* on the [`HooksManager`]
//! ([`HooksManager::stage_tool_hook`]) for the agent loop to apply: the rewrite
//! before dispatch, the context/messages at the turn's injection point. Those
//! fields used to be computed here and silently dropped.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use super::{HookDecision, HookEvent, HooksManager, StagedToolHook};
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

/// BR-19: the context injected for the model when a hook rewrote its tool call.
/// The rewritten arguments are what gets dispatched *and* what gets persisted as
/// the assistant's tool request, so without this the model would silently see a
/// call it never made — and results that do not match what it asked for.
fn rewrite_notice(
    tool_name: &str,
    before: &serde_json::Value,
    after: &serde_json::Value,
) -> String {
    format!(
        "A PreToolUse hook rewrote the arguments of `{tool_name}` before it ran. \
         Requested: {before}. Actually executed: {after}. \
         Work from the executed call; if the rewrite is unacceptable, say so rather than retrying the original."
    )
}

/// BR-19: apply staged input rewrites (`tool_request_id -> new arguments`) to the
/// pending tool requests, in place, before dispatch. Returns how many requests
/// were rewritten. A rewrite whose value is not a JSON object is ignored (already
/// rejected at parse time — belt and braces).
pub fn apply_tool_input_rewrites(
    requests: &mut [ToolRequest],
    rewrites: &HashMap<String, serde_json::Value>,
) -> usize {
    let mut applied = 0;
    for request in requests.iter_mut() {
        let Some(input) = rewrites.get(&request.id) else {
            continue;
        };
        let Some(object) = input.as_object() else {
            continue;
        };
        let Ok(tool_call) = &mut request.tool_call else {
            continue;
        };
        tool_call.arguments = Some(object.clone());
        applied += 1;
    }
    applied
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

            // BR-19: stage what the inspection channel cannot carry. A denied
            // call is never dispatched, so its rewrite is dropped — the deny
            // reason is what the model needs, not new arguments.
            let denied = matches!(aggregate.decision, Some(HookDecision::Deny { .. }));
            let updated_input = if denied {
                None
            } else {
                aggregate.updated_input.clone()
            };
            let mut additional_context = aggregate.additional_context.clone();
            if let Some(after) = &updated_input {
                additional_context.push(rewrite_notice(&tool_name, &tool_input, after));
            }
            if updated_input.is_some()
                || !additional_context.is_empty()
                || !aggregate.system_messages.is_empty()
            {
                self.hooks.stage_tool_hook(
                    &session.id,
                    StagedToolHook {
                        event: HookEvent::PreToolUse,
                        tool_request_id: request.id.clone(),
                        tool_name: tool_name.clone(),
                        updated_input,
                        additional_context,
                        system_messages: aggregate.system_messages.clone(),
                    },
                );
            }

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

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::CallToolRequestParams;
    use rmcp::object;

    fn request(
        id: &str,
        name: &str,
        args: serde_json::Map<String, serde_json::Value>,
    ) -> ToolRequest {
        ToolRequest {
            id: id.to_string(),
            tool_call: Ok(CallToolRequestParams {
                task: None,
                meta: None,
                name: name.to_string().into(),
                arguments: Some(args),
            }),
            metadata: None,
            tool_meta: None,
        }
    }

    #[test]
    fn rewrite_replaces_the_arguments_of_the_matching_request_only() {
        let mut requests = vec![
            request(
                "call_1",
                "developer__shell",
                object!({"command": "rm -rf /"}),
            ),
            request("call_2", "developer__shell", object!({"command": "ls"})),
        ];
        let rewrites = HashMap::from([(
            "call_1".to_string(),
            serde_json::json!({"command": "rm -rf ./build"}),
        )]);

        assert_eq!(apply_tool_input_rewrites(&mut requests, &rewrites), 1);
        let first = requests[0].tool_call.as_ref().unwrap();
        assert_eq!(
            first.arguments.as_ref().unwrap().get("command").unwrap(),
            "rm -rf ./build"
        );
        let second = requests[1].tool_call.as_ref().unwrap();
        assert_eq!(
            second.arguments.as_ref().unwrap().get("command").unwrap(),
            "ls",
            "an unrewritten request must be untouched"
        );
    }

    #[test]
    fn a_non_object_rewrite_is_ignored() {
        let mut requests = vec![request(
            "call_1",
            "developer__shell",
            object!({"command": "ls"}),
        )];
        let rewrites = HashMap::from([("call_1".to_string(), serde_json::json!("ls -la"))]);
        assert_eq!(apply_tool_input_rewrites(&mut requests, &rewrites), 0);
        let call = requests[0].tool_call.as_ref().unwrap();
        assert_eq!(
            call.arguments.as_ref().unwrap().get("command").unwrap(),
            "ls"
        );
    }

    #[test]
    fn rewrite_notice_names_both_calls() {
        let notice = rewrite_notice(
            "developer__shell",
            &serde_json::json!({"command": "rm -rf /"}),
            &serde_json::json!({"command": "rm -rf ./build"}),
        );
        assert!(notice.contains("developer__shell"));
        assert!(notice.contains("rm -rf /"));
        assert!(notice.contains("rm -rf ./build"));
    }
}
