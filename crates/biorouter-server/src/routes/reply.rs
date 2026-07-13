use crate::state::AppState;
use axum::{
    extract::{DefaultBodyLimit, State},
    http::{self, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use biorouter::agents::{AgentEvent, ReasoningEffort, SessionConfig};
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
    /// BR-63: how hard to think on this turn (`quick` / `normal` / `deep`), as
    /// picked in the composer. Omitted (the default) leaves the session's own
    /// `/effort` setting — and, failing that, the model's default depth — alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<ReasoningEffort>,
    /// Client-generated idempotency key naming *this* turn (BR-62). Optional, but
    /// a client that retries `/reply` — an SSE reconnect, a fetch retry on a
    /// flaky network — should send the same key it sent the first time. The retry
    /// then comes back as a 409 with `duplicate: true`, meaning "that turn is
    /// still running", instead of being mistaken for a genuine second turn.
    #[serde(default)]
    turn_id: Option<String>,
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

    // Created before the turn lock so the token can be registered *with* the
    // turn: that is what lets `/agent/cancel` (and `/agent/stop`) reach into a
    // running turn and trip it (BR-62).
    let cancel_token = CancellationToken::new();

    // Server-enforced single-turn-per-session lock (BR-33; also serializes the
    // per-session check-compact-persist path of BR-16). Two concurrent `/reply`
    // calls for one session would share one `Arc<Agent>`, confirmation channel,
    // and soft-interrupt queue, interleaving/duplicating output and doubling
    // token spend. Reject the duplicate with 409 instead of corrupting state;
    // the guard is released when the reply task ends (drops below).
    //
    // BR-62: a client that re-POSTs the same `turn_id` (an SSE reconnect) gets
    // `duplicate: true` back, so it can tell "my turn is still running" apart
    // from "someone else's turn is in the way" — and in neither case does a
    // second turn start.
    let turn_guard = match state.try_begin_turn_idempotent(
        &session_id,
        cancel_token.clone(),
        request.turn_id.clone(),
    ) {
        Ok(guard) => guard,
        Err(conflict) => {
            tracing::warn!(
                "Rejected concurrent /reply for session {}: turn {} already in flight (duplicate={})",
                session_id,
                conflict.running_turn_id,
                conflict.duplicate
            );
            let error = if conflict.duplicate {
                "This turn is already in progress for this session."
            } else {
                "A turn is already in progress for this session."
            };
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "type": "Error",
                    "error": error,
                    "running_turn_id": conflict.running_turn_id,
                    "duplicate": conflict.duplicate,
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

    let user_message = request.user_message;
    let conversation_so_far = request.conversation_so_far;
    let reasoning_effort = request.reasoning_effort;

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
            budget: None,
            retry_config: None,
            // BR-63: the composer's per-turn effort. `None` (nothing picked)
            // falls through to the session's `/effort`, then to the default.
            reasoning_effort,
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
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct InterruptRequest {
    pub session_id: String,
    pub text: String,
}

/// Soft interrupt: queue a user message to be injected into the session's
/// running turn at the next safe loop boundary, instead of cancelling the turn
/// and re-sending the whole context. Returns 202 Accepted; the message surfaces
/// as a normal user message in the active reply stream on the next loop step.
///
/// BR-61: rejected with 409 when the session has no turn in flight — with no
/// running loop to drain the queue the text would sit on the agent until some
/// unrelated later turn injected it. Clients treat 409 as "just send it as a
/// normal message".
#[utoipa::path(
    post,
    path = "/interrupt",
    request_body = InterruptRequest,
    responses(
        (status = 202, description = "Message queued for injection into the running turn"),
        (status = 400, description = "Empty message text"),
        (status = 409, description = "No turn is in flight for this session"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn interrupt(
    State(state): State<Arc<AppState>>,
    Json(req): Json<InterruptRequest>,
) -> Result<StatusCode, StatusCode> {
    if req.text.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if !state.is_turn_active(&req.session_id) {
        return Err(StatusCode::CONFLICT);
    }
    let agent = state.get_agent_for_route(req.session_id).await?;
    agent.queue_soft_interrupt(req.text);
    Ok(StatusCode::ACCEPTED)
}

/// Request body for the addressable cancel route.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct CancelTurnRequest {
    pub session_id: String,
}

/// Response body for the addressable cancel route.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct CancelTurnResponse {
    /// True when a running turn was found and its cancellation token tripped.
    /// False means there was nothing to cancel — which is a success, not an error.
    pub cancelled: bool,
    /// The id of the turn that was cancelled, when there was one.
    pub turn_id: Option<String>,
}

/// Hard cancel: trip the cancellation token of the turn in flight for this
/// session (BR-62).
///
/// Before this route, cancelling meant dropping the SSE socket — the only thing
/// that closed `tx` and tripped the token — so a turn could not be stopped by a
/// second client, the CLI, or a script, and `/agent/stop` merely evicted the
/// agent from the LRU while the in-flight reply task ran on happily against its
/// own `Arc<Agent>`. Tripping the token unwinds the agent loop at its next
/// boundary and unblocks a tool-permission prompt it may be parked on.
///
/// Deliberately **idempotent**: cancelling a session with no turn in flight (a
/// double-clicked Stop button, a cancel that raced the turn's own completion) is
/// a 200 with `cancelled: false`, never an error. A cancel that reports failure
/// because the thing was already stopped is exactly the unreliability this BR
/// exists to remove.
#[utoipa::path(
    post,
    path = "/agent/cancel",
    request_body = CancelTurnRequest,
    responses(
        (status = 200, description = "Cancel processed; `cancelled` reports whether a turn was running", body = CancelTurnResponse),
        (status = 401, description = "Unauthorized - invalid secret key"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn cancel_turn(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CancelTurnRequest>,
) -> Json<CancelTurnResponse> {
    match state.cancel_turn(&req.session_id) {
        Some(turn_id) => {
            tracing::info!("Cancelled turn {} for session {}", turn_id, req.session_id);
            Json(CancelTurnResponse {
                cancelled: true,
                turn_id: Some(turn_id),
            })
        }
        None => {
            tracing::debug!(
                "Cancel for session {} found no turn in flight",
                req.session_id
            );
            Json(CancelTurnResponse {
                cancelled: false,
                turn_id: None,
            })
        }
    }
}

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route(
            "/reply",
            post(reply).layer(DefaultBodyLimit::max(50 * 1024 * 1024)),
        )
        .route("/interrupt", post(interrupt))
        .route("/agent/cancel", post(cancel_turn))
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

        /// Begin a turn with a throwaway token and no idempotency key.
        fn begin_turn(
            state: &AppState,
            session_id: &str,
        ) -> Result<crate::state::TurnGuard, crate::state::TurnConflict> {
            state.try_begin_turn_idempotent(session_id, CancellationToken::new(), None)
        }

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
                        reasoning_effort: None,
                        turn_id: None,
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
            let _guard = begin_turn(&state, "busy-session").expect("first turn acquires the lock");

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
                        reasoning_effort: None,
                        turn_id: None,
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
                let _guard = begin_turn(&state, "recycled-session").unwrap();
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
                        reasoning_effort: None,
                        turn_id: None,
                    })
                    .unwrap(),
                ))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();

            assert_eq!(response.status(), StatusCode::OK);
        }

        fn interrupt_request(session_id: &str, text: &str) -> Request<Body> {
            Request::builder()
                .uri("/interrupt")
                .method("POST")
                .header("content-type", "application/json")
                .header("x-secret-key", "test-secret")
                .body(Body::from(
                    serde_json::to_string(&InterruptRequest {
                        session_id: session_id.to_string(),
                        text: text.to_string(),
                    })
                    .unwrap(),
                ))
                .unwrap()
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn test_interrupt_accepts_steer_while_turn_in_flight() {
            let state = AppState::new().await.unwrap();
            let _guard = begin_turn(&state, "steering-session").expect("turn lock acquired");

            let app = routes(Arc::clone(&state));
            let response = app
                .oneshot(interrupt_request("steering-session", "actually, use R"))
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::ACCEPTED);

            let agent = state
                .get_agent("steering-session".to_string())
                .await
                .unwrap();
            assert!(
                agent.has_soft_interrupts(),
                "the steer must be queued on the session's agent"
            );
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn test_interrupt_rejects_when_no_turn_in_flight() {
            let state = AppState::new().await.unwrap();

            let app = routes(state);
            let response = app
                .oneshot(interrupt_request("idle-session", "steer me"))
                .await
                .unwrap();

            // Nothing to steer: the client should send this as a normal message
            // rather than let it sit on the agent's queue.
            assert_eq!(response.status(), StatusCode::CONFLICT);
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn test_interrupt_rejects_empty_text() {
            let state = AppState::new().await.unwrap();
            let _guard = begin_turn(&state, "blank-session").unwrap();

            let app = routes(state);
            let response = app
                .oneshot(interrupt_request("blank-session", "   "))
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        }

        fn cancel_request(session_id: &str) -> Request<Body> {
            Request::builder()
                .uri("/agent/cancel")
                .method("POST")
                .header("content-type", "application/json")
                .header("x-secret-key", "test-secret")
                .body(Body::from(
                    serde_json::to_string(&CancelTurnRequest {
                        session_id: session_id.to_string(),
                    })
                    .unwrap(),
                ))
                .unwrap()
        }

        async fn json_body(response: axum::response::Response) -> serde_json::Value {
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            serde_json::from_slice(&bytes).unwrap()
        }

        fn reply_request(session_id: &str, turn_id: Option<&str>) -> Request<Body> {
            Request::builder()
                .uri("/reply")
                .method("POST")
                .header("content-type", "application/json")
                .header("x-secret-key", "test-secret")
                .body(Body::from(
                    serde_json::to_string(&ChatRequest {
                        user_message: Message::user().with_text("hello"),
                        conversation_so_far: None,
                        session_id: session_id.to_string(),
                        workflow_name: None,
                        workflow_version: None,
                        turn_id: turn_id.map(str::to_string),
                        reasoning_effort: None,
                    })
                    .unwrap(),
                ))
                .unwrap()
        }

        /// BR-62: a turn can now be stopped by session id, without the caller
        /// holding the SSE socket.
        #[tokio::test(flavor = "multi_thread")]
        async fn test_cancel_trips_the_running_turn() {
            let state = AppState::new().await.unwrap();
            let token = CancellationToken::new();
            let _guard = state
                .try_begin_turn_idempotent("busy-session", token.clone(), None)
                .expect("turn lock acquired");

            let app = routes(Arc::clone(&state));
            let response = app.oneshot(cancel_request("busy-session")).await.unwrap();

            assert_eq!(response.status(), StatusCode::OK);
            let body = json_body(response).await;
            assert_eq!(body["cancelled"], serde_json::json!(true));
            assert!(body["turn_id"].is_string());
            assert!(
                token.is_cancelled(),
                "the running turn's token must be tripped"
            );
        }

        /// Cancelling an idle session is a success no-op, not an error — a Stop
        /// button that reports failure because the turn already finished is
        /// exactly the unreliability BR-62 removes.
        #[tokio::test(flavor = "multi_thread")]
        async fn test_cancel_is_a_noop_when_no_turn_is_running() {
            let state = AppState::new().await.unwrap();

            let app = routes(state);
            let response = app.oneshot(cancel_request("idle-session")).await.unwrap();

            assert_eq!(response.status(), StatusCode::OK);
            let body = json_body(response).await;
            assert_eq!(body["cancelled"], serde_json::json!(false));
            assert_eq!(body["turn_id"], serde_json::Value::Null);
        }

        /// A re-POST of the same turn (SSE reconnect) is reported as a duplicate
        /// so the client can re-attach rather than surface a hard error — and no
        /// second turn starts either way.
        #[tokio::test(flavor = "multi_thread")]
        async fn test_reply_reports_a_reposted_turn_id_as_duplicate() {
            let state = AppState::new().await.unwrap();
            let _guard = state
                .try_begin_turn_idempotent(
                    "retry-session",
                    CancellationToken::new(),
                    Some("client-turn-1".to_string()),
                )
                .expect("turn lock acquired");

            let app = routes(state);
            let response = app
                .oneshot(reply_request("retry-session", Some("client-turn-1")))
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::CONFLICT);
            let body = json_body(response).await;
            assert_eq!(body["duplicate"], serde_json::json!(true));
        }

        /// A *different* turn arriving while one is in flight is a genuine
        /// conflict, not a retry.
        #[tokio::test(flavor = "multi_thread")]
        async fn test_reply_reports_a_different_turn_id_as_a_real_conflict() {
            let state = AppState::new().await.unwrap();
            let _guard = state
                .try_begin_turn_idempotent(
                    "contended-session",
                    CancellationToken::new(),
                    Some("client-turn-1".to_string()),
                )
                .expect("turn lock acquired");

            let app = routes(state);
            let response = app
                .oneshot(reply_request("contended-session", Some("client-turn-2")))
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::CONFLICT);
            let body = json_body(response).await;
            assert_eq!(body["duplicate"], serde_json::json!(false));
        }
    }
}
