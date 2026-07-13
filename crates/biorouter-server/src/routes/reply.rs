use crate::state::AppState;
use axum::{
    extract::{DefaultBodyLimit, State},
    http::{self, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use biorouter::agents::{AgentEvent, SessionConfig};
use biorouter::conversation::message::{Message, MessageContent, TokenState};
use biorouter::conversation::Conversation;
use biorouter::session::SessionManager;
use bytes::Bytes;
use futures::{stream::StreamExt, Stream};
use rmcp::model::ServerNotification;
use serde::{Deserialize, Serialize};
use std::{
    convert::Infallible,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

fn track_tool_telemetry(content: &MessageContent, all_messages: &[Message]) {
    match content {
        MessageContent::ToolRequest(tool_request) => {
            if let Ok(tool_call) = &tool_request.tool_call {
                tracing::info!(monotonic_counter.biorouter.tool_calls = 1,
                    tool_name = %tool_call.name,
                    "Tool call started"
                );
            }
        }
        MessageContent::ToolResponse(tool_response) => {
            let tool_name = all_messages
                .iter()
                .rev()
                .find_map(|msg| {
                    msg.content.iter().find_map(|c| {
                        if let MessageContent::ToolRequest(req) = c {
                            if req.id == tool_response.id {
                                if let Ok(tool_call) = &req.tool_call {
                                    Some(tool_call.name.clone())
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                })
                .unwrap_or_else(|| "unknown".to_string().into());

            let success = tool_response.tool_result.is_ok();
            let result_status = if success { "success" } else { "error" };

            tracing::info!(
                counter.biorouter.tool_completions = 1,
                tool_name = %tool_name,
                result = %result_status,
                "Tool call completed"
            );
        }
        _ => {}
    }
}

#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct ChatRequest {
    user_message: Message,
    #[serde(default)]
    conversation_so_far: Option<Vec<Message>>,
    session_id: String,
    workflow_name: Option<String>,
    workflow_version: Option<String>,
}

pub struct SseResponse {
    rx: ReceiverStream<String>,
}

impl SseResponse {
    fn new(rx: ReceiverStream<String>) -> Self {
        Self { rx }
    }

    /// Construct an `SseResponse` from a raw `mpsc::Receiver<String>`.
    /// Each string pushed to the sender is forwarded verbatim as SSE bytes.
    pub fn from_rx(rx: mpsc::Receiver<String>) -> Self {
        Self {
            rx: ReceiverStream::new(rx),
        }
    }
}

impl Stream for SseResponse {
    type Item = Result<Bytes, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.rx)
            .poll_next(cx)
            .map(|opt| opt.map(|s| Ok(Bytes::from(s))))
    }
}

impl IntoResponse for SseResponse {
    fn into_response(self) -> axum::response::Response {
        let stream = self;
        let body = axum::body::Body::from_stream(stream);

        http::Response::builder()
            .header("Content-Type", "text/event-stream")
            .header("Cache-Control", "no-cache")
            .header("Connection", "keep-alive")
            .body(body)
            .unwrap()
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(tag = "type")]
pub enum MessageEvent {
    Message {
        message: Message,
        token_state: TokenState,
    },
    Error {
        error: String,
    },
    Finish {
        reason: String,
        token_state: TokenState,
    },
    ModelChange {
        model: String,
        mode: String,
    },
    Notification {
        request_id: String,
        #[schema(value_type = Object)]
        message: ServerNotification,
    },
    UpdateConversation {
        conversation: Conversation,
        token_state: TokenState,
    },
    Ping,
}

/// Read the session's token counters straight from the store.
///
/// BR-52: this used to run on *every* streamed event — one SQLite query per
/// token — even though the counters only ever move at a turn/compaction
/// boundary, so every mid-stream read returned the value the previous one had
/// already returned. The stream now carries the agent's own
/// [`AgentEvent::TokenUsage`] snapshot and this remains only for the two places
/// a DB read is genuinely needed: seeding the state when the stream opens (a
/// resumed session already has counters) and the authoritative reconciliation on
/// `Finish`.
async fn get_token_state(session_manager: &SessionManager, session_id: &str) -> TokenState {
    // Fetch only the token counters — not a full session row plus a
    // `COUNT(*)` over the messages table — since only the token fields are used.
    session_manager
        .get_token_counts(session_id)
        .await
        .map(TokenState::from)
        .inspect_err(|e| {
            tracing::warn!(
                "Failed to fetch session token state for {}: {}",
                session_id,
                e
            );
        })
        .unwrap_or_default()
}

async fn stream_event(
    event: MessageEvent,
    tx: &mpsc::Sender<String>,
    cancel_token: &CancellationToken,
) {
    let json = serde_json::to_string(&event).unwrap_or_else(|e| {
        format!(
            r#"{{"type":"Error","error":"Failed to serialize event: {}"}}"#,
            e
        )
    });

    if tx.send(format!("data: {}\n\n", json)).await.is_err() {
        tracing::info!("client hung up");
        cancel_token.cancel();
    }
}

#[allow(clippy::too_many_lines)]
#[utoipa::path(
    post,
    path = "/reply",
    request_body = ChatRequest,
    responses(
        (status = 200, description = "Streaming response initiated",
         body = MessageEvent,
         content_type = "text/event-stream"),
        (status = 424, description = "Agent not initialized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn reply(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ChatRequest>,
) -> axum::response::Response {
    let session_start = std::time::Instant::now();

    tracing::info!(
        counter.biorouter.session_starts = 1,
        session_type = "app",
        interface = "ui",
        "Session started"
    );

    let session_id = request.session_id.clone();

    // Server-enforced single-turn-per-session lock (BR-33; also serializes the
    // per-session check-compact-persist path of BR-16). Two concurrent `/reply`
    // calls for one session would share one `Arc<Agent>`, confirmation channel,
    // and soft-interrupt queue, interleaving/duplicating output and doubling
    // token spend. Reject the duplicate with 409 instead of corrupting state;
    // the guard is released when the reply task ends (drops below).
    let turn_guard = match state.try_begin_turn(&session_id) {
        Ok(guard) => guard,
        Err(running_turn_id) => {
            tracing::warn!(
                "Rejected concurrent /reply for session {}: turn {} already in flight",
                session_id,
                running_turn_id
            );
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "type": "Error",
                    "error": "A turn is already in progress for this session.",
                    "running_turn_id": running_turn_id,
                })),
            )
                .into_response();
        }
    };

    if let Some(workflow_name) = request.workflow_name.clone() {
        if state.mark_workflow_run_if_absent(&session_id).await {
            let workflow_version = request
                .workflow_version
                .clone()
                .unwrap_or_else(|| "unknown".to_string());

            tracing::info!(
                counter.biorouter.workflow_runs = 1,
                workflow_name = %workflow_name,
                workflow_version = %workflow_version,
                session_type = "app",
                interface = "ui",
                "Workflow execution started"
            );
        }
    }

    let (tx, rx) = mpsc::channel(100);
    let stream = ReceiverStream::new(rx);
    let cancel_token = CancellationToken::new();

    let user_message = request.user_message;
    let conversation_so_far = request.conversation_so_far;

    let task_cancel = cancel_token.clone();
    let task_tx = tx.clone();

    let _handle = tokio::spawn(async move {
        // Holds the per-session turn lock for the lifetime of this reply stream;
        // dropped (releasing the session) when the task ends.
        let _turn_guard = turn_guard;
        // Mark an interactive turn in progress for the lifetime of this reply
        // stream so the scheduler defers background jobs while the user is
        // mid-conversation (dropped when the stream task ends).
        let _interactive_turn = biorouter::scheduler::interactive_turn_guard();
        let agent = match state.get_agent(session_id.clone()).await {
            Ok(agent) => agent,
            Err(e) => {
                tracing::error!("Failed to get session agent: {}", e);
                let _ = stream_event(
                    MessageEvent::Error {
                        error: format!("Failed to get session agent: {}", e),
                    },
                    &task_tx,
                    &task_cancel,
                )
                .await;
                return;
            }
        };

        let session = match state.session_manager().get_session(&session_id, true).await {
            Ok(metadata) => metadata,
            Err(e) => {
                tracing::error!("Failed to read session for {}: {}", session_id, e);
                let _ = stream_event(
                    MessageEvent::Error {
                        error: format!("Failed to read session: {}", e),
                    },
                    &task_tx,
                    &cancel_token,
                )
                .await;
                return;
            }
        };

        let session_config = SessionConfig {
            id: session_id.clone(),
            schedule_id: session.schedule_id.clone(),
            max_turns: None,
            max_tool_calls: None,
            retry_config: None,
        };

        // BR-52: the token state we attach to every streamed event. Seeded from
        // the session row we just read (free — no extra query) and refreshed only
        // when the agent tells us the counters moved, which is the only time they
        // can. Previously every single streamed chunk re-read this from SQLite.
        let mut token_state = TokenState::from(&session);

        let mut all_messages = match conversation_so_far {
            Some(history) => {
                let conv = Conversation::new_unvalidated(history);
                if let Err(e) = state
                    .session_manager()
                    .replace_conversation(&session_id, &conv)
                    .await
                {
                    tracing::warn!(
                        "Failed to replace session conversation for {}: {}",
                        session_id,
                        e
                    );
                }
                conv
            }
            None => session.conversation.unwrap_or_default(),
        };
        all_messages.push(user_message.clone());

        let mut stream = match agent
            .reply(
                user_message.clone(),
                session_config,
                Some(task_cancel.clone()),
            )
            .await
        {
            Ok(stream) => stream,
            Err(e) => {
                tracing::error!("Failed to start reply stream: {:?}", e);
                stream_event(
                    MessageEvent::Error {
                        error: e.to_string(),
                    },
                    &task_tx,
                    &cancel_token,
                )
                .await;
                return;
            }
        };

        let mut heartbeat_interval = tokio::time::interval(Duration::from_millis(500));
        loop {
            tokio::select! {
                _ = task_cancel.cancelled() => {
                    tracing::info!("Agent task cancelled");
                    break;
                }
                _ = heartbeat_interval.tick() => {
                    stream_event(MessageEvent::Ping, &tx, &cancel_token).await;
                }
                response = timeout(Duration::from_millis(500), stream.next()) => {
                    match response {
                        Ok(Some(Ok(AgentEvent::Message(message)))) => {
                            for content in &message.content {
                                track_tool_telemetry(content, all_messages.messages());
                            }

                            all_messages.push(message.clone());

                            stream_event(MessageEvent::Message { message, token_state: token_state.clone() }, &tx, &cancel_token).await;
                        }
                        Ok(Some(Ok(AgentEvent::TokenUsage(new_token_state)))) => {
                            // BR-52: the agent wrote the session's counters at a
                            // turn/compaction boundary and handed us the result.
                            // Every event we emit from here on carries it — no
                            // per-event DB read, and no separate SSE frame (the
                            // wire schema is unchanged, so older clients that only
                            // read `token_state` off Message/Finish still work).
                            token_state = new_token_state;
                        }
                        Ok(Some(Ok(AgentEvent::HistoryReplaced(new_messages)))) => {
                            all_messages = new_messages.clone();
                            stream_event(MessageEvent::UpdateConversation {conversation: new_messages, token_state: token_state.clone()}, &tx, &cancel_token).await;

                        }
                        Ok(Some(Ok(AgentEvent::ModelChange { model, mode }))) => {
                            stream_event(MessageEvent::ModelChange { model, mode }, &tx, &cancel_token).await;
                        }
                        Ok(Some(Ok(AgentEvent::McpNotification((request_id, n))))) => {
                            stream_event(MessageEvent::Notification{
                                request_id: request_id.clone(),
                                message: n,
                            }, &tx, &cancel_token).await;
                        }

                        Ok(Some(Err(e))) => {
                            tracing::error!("Error processing message: {}", e);
                            stream_event(
                                MessageEvent::Error {
                                    error: e.to_string(),
                                },
                                &tx,
                                &cancel_token,
                            ).await;
                            break;
                        }
                        Ok(None) => {
                            break;
                        }
                        Err(_) => {
                            if tx.is_closed() {
                                break;
                            }
                            continue;
                        }
                    }
                }
            }
        }

        // The reply loop has ended (normal completion, error, or cancellation).
        // Trigger the best-effort LLM session rename here — this always runs,
        // unlike a tail on the lazy reply stream which the early `break` above
        // can skip, leaving the session stuck on "New Session".
        {
            let agent_for_rename = agent.clone();
            let session_id_for_rename = session_id.clone();
            tokio::spawn(async move {
                agent_for_rename
                    .maybe_rename_session(&session_id_for_rename)
                    .await;
            });
        }

        let session_duration = session_start.elapsed();

        if let Ok(session) = state.session_manager().get_session(&session_id, true).await {
            let total_tokens = session.total_tokens.unwrap_or(0);
            tracing::info!(
                counter.biorouter.session_completions = 1,
                session_type = "app",
                interface = "ui",
                exit_type = "normal",
                duration_ms = session_duration.as_millis() as u64,
                total_tokens = total_tokens,
                message_count = session.message_count,
                "Session completed"
            );

            tracing::info!(
                counter.biorouter.session_duration_ms = session_duration.as_millis() as u64,
                session_type = "app",
                interface = "ui",
                "Session duration"
            );

            if total_tokens > 0 {
                tracing::info!(
                    counter.biorouter.session_tokens = total_tokens,
                    session_type = "app",
                    interface = "ui",
                    "Session tokens"
                );
            }
        } else {
            tracing::info!(
                counter.biorouter.session_completions = 1,
                session_type = "app",
                interface = "ui",
                exit_type = "normal",
                duration_ms = session_duration.as_millis() as u64,
                total_tokens = 0u64,
                message_count = all_messages.len(),
                "Session completed"
            );

            tracing::info!(
                counter.biorouter.session_duration_ms = session_duration.as_millis() as u64,
                session_type = "app",
                interface = "ui",
                "Session duration"
            );
        }

        // BR-52: one authoritative read at the end of the turn — the single point
        // where a client's token readout is reconciled with the store, so nothing
        // written outside this stream (a background eager compaction, a concurrent
        // scheduled run) can leave the UI on a stale count.
        let final_token_state = get_token_state(state.session_manager(), &session_id).await;

        let _ = stream_event(
            MessageEvent::Finish {
                reason: "stop".to_string(),
                token_state: final_token_state,
            },
            &task_tx,
            &cancel_token,
        )
        .await;
    });
    SseResponse::new(stream).into_response()
}

/// Request body for the soft-interrupt route.
#[derive(Deserialize)]
pub struct InterruptRequest {
    pub session_id: String,
    pub text: String,
}

/// Soft interrupt: queue a user message to be injected into the session's
/// running turn at the next safe loop boundary, instead of cancelling the turn
/// and re-sending the whole context. Returns 202 Accepted; the message surfaces
/// as a normal user message in the active reply stream on the next loop step.
async fn interrupt(
    State(state): State<Arc<AppState>>,
    Json(req): Json<InterruptRequest>,
) -> Result<StatusCode, StatusCode> {
    let agent = state.get_agent_for_route(req.session_id).await?;
    agent.queue_soft_interrupt(req.text);
    Ok(StatusCode::ACCEPTED)
}

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route(
            "/reply",
            post(reply).layer(DefaultBodyLimit::max(50 * 1024 * 1024)),
        )
        .route("/interrupt", post(interrupt))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    mod integration_tests {
        use super::*;
        use axum::{body::Body, http::Request};
        use biorouter::conversation::message::Message;
        use tower::ServiceExt;

        #[tokio::test(flavor = "multi_thread")]
        async fn test_reply_endpoint() {
            let state = AppState::new().await.unwrap();

            let app = routes(state);

            let request = Request::builder()
                .uri("/reply")
                .method("POST")
                .header("content-type", "application/json")
                .header("x-secret-key", "test-secret")
                .body(Body::from(
                    serde_json::to_string(&ChatRequest {
                        user_message: Message::user().with_text("test message"),
                        conversation_so_far: None,
                        session_id: "test-session".to_string(),
                        workflow_name: None,
                        workflow_version: None,
                    })
                    .unwrap(),
                ))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();

            assert_eq!(response.status(), StatusCode::OK);
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn test_reply_rejects_concurrent_turn() {
            let state = AppState::new().await.unwrap();

            // Simulate a turn already in flight for this session. The guard owns
            // its own Arc into the shared active-turns map, so it stays valid
            // after `state` is moved into `routes`.
            let _guard = state
                .try_begin_turn("busy-session")
                .expect("first turn acquires the lock");

            let app = routes(state);

            let request = Request::builder()
                .uri("/reply")
                .method("POST")
                .header("content-type", "application/json")
                .header("x-secret-key", "test-secret")
                .body(Body::from(
                    serde_json::to_string(&ChatRequest {
                        user_message: Message::user().with_text("second message"),
                        conversation_so_far: None,
                        session_id: "busy-session".to_string(),
                        workflow_name: None,
                        workflow_version: None,
                    })
                    .unwrap(),
                ))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();

            assert_eq!(response.status(), StatusCode::CONFLICT);
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn test_reply_allows_new_turn_after_guard_dropped() {
            let state = AppState::new().await.unwrap();

            // A turn that has ended (guard dropped) must not block the next one.
            {
                let _guard = state.try_begin_turn("recycled-session").unwrap();
            }

            let app = routes(state);

            let request = Request::builder()
                .uri("/reply")
                .method("POST")
                .header("content-type", "application/json")
                .header("x-secret-key", "test-secret")
                .body(Body::from(
                    serde_json::to_string(&ChatRequest {
                        user_message: Message::user().with_text("fresh message"),
                        conversation_so_far: None,
                        session_id: "recycled-session".to_string(),
                        workflow_name: None,
                        workflow_version: None,
                    })
                    .unwrap(),
                ))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();

            assert_eq!(response.status(), StatusCode::OK);
        }
    }
}
