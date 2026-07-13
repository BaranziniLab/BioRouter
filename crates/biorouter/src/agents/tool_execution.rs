use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use async_stream::try_stream;
use futures::stream::{self, BoxStream};
use futures::{Stream, StreamExt};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::config::permission::PermissionLevel;
use crate::mcp_utils::ToolResult;
use crate::permission::Permission;
use rmcp::model::{Content, ServerNotification};

// ToolCallResult combines the result of a tool call with an optional notification stream that
// can be used to receive notifications from the tool.
pub struct ToolCallResult {
    pub result: Box<dyn Future<Output = ToolResult<rmcp::model::CallToolResult>> + Send + Unpin>,
    pub notification_stream: Option<Box<dyn Stream<Item = ServerNotification> + Send + Unpin>>,
}

impl From<ToolResult<rmcp::model::CallToolResult>> for ToolCallResult {
    fn from(result: ToolResult<rmcp::model::CallToolResult>) -> Self {
        Self {
            result: Box::new(futures::future::ready(result)),
            notification_stream: None,
        }
    }
}

use super::agent::{tool_stream, ToolStream};
use crate::agents::Agent;
use crate::conversation::message::{Message, ToolRequest};
use crate::session::Session;
use crate::tool_inspection::get_security_finding_id_from_results;

pub const DECLINED_RESPONSE: &str = "The user has declined to run this tool. \
    DO NOT attempt to call this tool again. \
    If there are no alternative methods to proceed, clearly explain the situation and STOP.";

pub const CHAT_MODE_TOOL_SKIPPED_RESPONSE: &str = "Let the user know the tool call was skipped in biorouter chat mode. \
                                        DO NOT apologize for skipping the tool call. DO NOT say sorry. \
                                        Provide an explanation of what the tool call would do, structured as a \
                                        plan for the user. Again, DO NOT apologize. \
                                        **Example Plan:**\n \
                                        1. **Identify Task Scope** - Determine the purpose and expected outcome.\n \
                                        2. **Outline Steps** - Break down the steps.\n \
                                        If needed, adjust the explanation based on user preferences or questions.";

impl Agent {
    #[allow(clippy::too_many_lines)]
    pub(crate) fn handle_approval_tool_requests<'a>(
        &'a self,
        tool_requests: &'a [ToolRequest],
        tool_futures: Arc<Mutex<Vec<(String, ToolStream)>>>,
        request_to_response_map: &'a HashMap<String, Arc<Mutex<Message>>>,
        cancellation_token: Option<CancellationToken>,
        session: &'a Session,
        inspection_results: &'a [crate::tool_inspection::InspectionResult],
    ) -> BoxStream<'a, anyhow::Result<Message>> {
        try_stream! {
        for request in tool_requests.iter() {
            if let Ok(tool_call) = request.tool_call.clone() {
                // PermissionRequest hooks may resolve the approval without
                // prompting the user (allow -> dispatch, deny -> declined).
                let hook_decision = {
                    let tool_input = tool_call
                        .arguments
                        .clone()
                        .map(serde_json::Value::Object)
                        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
                    let aggregate = self
                        .hooks_manager
                        .permission_request(&session.id, &session.working_dir, &tool_call.name, &tool_input)
                        .await;
                    // BR-19: this gate used to read only `aggregate.decision`, so a
                    // PermissionRequest hook's `additionalContext` / `systemMessage`
                    // was computed and thrown away. Stage them for the turn's
                    // injection point (a message yielded from here never enters the
                    // conversation, only the event stream).
                    if !aggregate.additional_context.is_empty() || !aggregate.system_messages.is_empty() {
                        self.hooks_manager.stage_tool_hook(
                            &session.id,
                            crate::hooks::StagedToolHook {
                                event: crate::hooks::HookEvent::PermissionRequest,
                                tool_request_id: request.id.clone(),
                                tool_name: tool_call.name.to_string(),
                                // Rewrites are PreToolUse-only: by here the call is
                                // already recorded in the transcript.
                                updated_input: None,
                                additional_context: aggregate.additional_context.clone(),
                                system_messages: aggregate.system_messages.clone(),
                            },
                        );
                    }
                    aggregate.decision
                };

                match hook_decision {
                    Some(crate::hooks::HookDecision::Allow { reason }) => {
                        tracing::info!(
                            tool_name = %tool_call.name,
                            reason = reason.as_deref().unwrap_or(""),
                            "PermissionRequest hook auto-approved tool call"
                        );
                        let (req_id, tool_result) = self.dispatch_tool_call(tool_call.clone(), request.id.clone(), cancellation_token.clone(), session).await;
                        let mut futures = tool_futures.lock().await;
                        futures.push((req_id, match tool_result {
                            Ok(result) => tool_stream(
                                result.notification_stream.unwrap_or_else(|| Box::new(stream::empty())),
                                result.result,
                            ),
                            Err(e) => tool_stream(
                                Box::new(stream::empty()),
                                futures::future::ready(Err(e)),
                            ),
                        }));
                        continue;
                    }
                    Some(crate::hooks::HookDecision::Deny { reason }) => {
                        tracing::info!(
                            tool_name = %tool_call.name,
                            reason = %reason,
                            "PermissionRequest hook denied tool call"
                        );
                        if let Some(response_msg) = request_to_response_map.get(&request.id) {
                            let mut response = response_msg.lock().await;
                            *response = response.clone().with_tool_response_with_metadata(
                                request.id.clone(),
                                Ok(rmcp::model::CallToolResult {
                                    content: vec![Content::text(format!(
                                        "{DECLINED_RESPONSE}\n\nHook feedback: {reason}"
                                    ))],
                                    structured_content: None,
                                    is_error: Some(true),
                                    meta: None,
                                }),
                                request.metadata.as_ref(),
                            );
                        }
                        yield Message::assistant()
                            .with_system_notification(
                                crate::conversation::message::SystemNotificationType::InlineMessage,
                                format!("Hook denied permission for {}: {}", tool_call.name, reason),
                            )
                            .user_only();
                        continue;
                    }
                    _ => {}
                }

                // No hook decision: notify hooks that a permission prompt is
                // being shown, then fall through to the normal confirmation.
                {
                    let mut payload = crate::hooks::HookPayload::new(
                        crate::hooks::HookEvent::Notification,
                        &session.id,
                        session.working_dir.to_string_lossy(),
                    );
                    payload.message = Some(format!("Permission required for {}", tool_call.name));
                    self.hooks_manager.fire(
                        crate::hooks::HookEvent::Notification,
                        Some("permission_prompt".to_string()),
                        payload,
                        session.working_dir.clone(),
                    );
                }

                // Find the corresponding inspection result for this tool request
                let security_message = inspection_results.iter()
                    .find(|result| result.tool_request_id == request.id)
                    .and_then(|result| {
                        if let crate::tool_inspection::InspectionAction::RequireApproval(Some(message)) = &result.action {
                            Some(message.clone())
                        } else {
                            None
                        }
                    });

                // BR-63: give the card enough to decide with. The BR-18 registry
                // already grades every tool it handed the model this turn, and
                // the preview turns the raw arguments into the thing the user
                // actually needs to see — the command, or the diff.
                let arguments = tool_call.arguments.clone().unwrap_or_default();
                let risk = self.tool_risks.risk_for(&tool_call.name);
                let preview = crate::conversation::tool_preview::ToolPreview::for_tool_call(
                    &tool_call.name,
                    &arguments,
                );

                let confirmation = Message::assistant()
                    .with_action_required_with_context(
                        request.id.clone(),
                        tool_call.name.to_string().clone(),
                        arguments,
                        security_message,
                        Some(risk),
                        preview,
                    )
                    .user_only();
                yield confirmation;

                let mut rx = self.confirmation_rx.lock().await;
                while let Some((req_id, confirmation)) = rx.recv().await {
                    if req_id == request.id {
                        // Log user decision if this was a security alert
                        if let Some(finding_id) = get_security_finding_id_from_results(&request.id, inspection_results) {
                            tracing::info!(
                                counter.biorouter.prompt_injection_user_decisions = 1,
                                decision = ?confirmation.permission,
                                finding_id = %finding_id,
                                "User security decision"
                            );
                        }

                        if confirmation.permission == Permission::AllowOnce || confirmation.permission == Permission::AlwaysAllow {
                            let (req_id, tool_result) = self.dispatch_tool_call(tool_call.clone(), request.id.clone(), cancellation_token.clone(), session).await;
                            let mut futures = tool_futures.lock().await;

                            futures.push((req_id, match tool_result {
                                Ok(result) => tool_stream(
                                    result.notification_stream.unwrap_or_else(|| Box::new(stream::empty())),
                                    result.result,
                                ),
                                Err(e) => tool_stream(
                                    Box::new(stream::empty()),
                                    futures::future::ready(Err(e)),
                                ),
                            }));

                            // Update the shared permission manager when user selects "Always Allow"
                            if confirmation.permission == Permission::AlwaysAllow {
                                self.tool_inspection_manager
                                    .update_permission_manager(&tool_call.name, PermissionLevel::AlwaysAllow)
                                    .await;
                            }
                        } else {
                            // User declined - update the specific response message for this request
                            if let Some(response_msg) = request_to_response_map.get(&request.id) {
                                let mut response = response_msg.lock().await;
                                *response = response.clone().with_tool_response_with_metadata(
                                    request.id.clone(),
                                    Ok(rmcp::model::CallToolResult {
                                        content: vec![Content::text(DECLINED_RESPONSE)],
                                        structured_content: None,
                                        is_error: Some(true),
                                        meta: None,
                                    }),
                                    request.metadata.as_ref(),
                                );
                            }

                            if confirmation.permission == Permission::AlwaysDeny {
                                self.tool_inspection_manager
                                    .update_permission_manager(&tool_call.name, PermissionLevel::NeverAllow)
                                    .await;
                            }
                        }
                        break; // Exit the loop once the matching `req_id` is found
                    }
                }
            }
        }
    }.boxed()
    }

    pub(crate) fn handle_frontend_tool_request<'a>(
        &'a self,
        tool_request: &'a ToolRequest,
        message_tool_response: Arc<Mutex<Message>>,
    ) -> BoxStream<'a, anyhow::Result<Message>> {
        try_stream! {
                if let Ok(tool_call) = tool_request.tool_call.clone() {
                    if self.is_frontend_tool(&tool_call.name).await {
                        // Send frontend tool request and wait for response
                        yield Message::assistant().with_frontend_tool_request(
                            tool_request.id.clone(),
                            Ok(tool_call.clone())
                        );

                        if let Some((id, result)) = self.tool_result_rx.lock().await.recv().await {
                            let mut response = message_tool_response.lock().await;
                            *response = response.clone().with_tool_response_with_metadata(
                                id,
                                result,
                                tool_request.metadata.as_ref(),
                            );
                        }
                    }
            }
        }
        .boxed()
    }
}
