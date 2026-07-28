use crate::state::AppState;
use axum::{
    extract::{DefaultBodyLimit, State},
    http::{self, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use biorouter::agents::{
    AgentEvent, PersistedMessage, ReasoningEffort, SessionConfig, TurnAbortCode,
};
use biorouter::conversation::message::{Message, MessageContent, TokenState};
use biorouter::conversation::Conversation;
use biorouter::session::session_manager::ReplaceOutcome;
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

pub(crate) fn track_tool_telemetry(content: &MessageContent, all_messages: &[Message]) {
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
    /// The client's own copy of the session's history, which REPLACES what the
    /// server has stored.
    ///
    /// DEPRECATED as an authoritative overwrite (#51 W5). It is now conditional:
    /// the copy must already contain every message the server holds, and the
    /// request is refused with 409 if it does not. See
    /// [`apply_client_writeback`]. The desktop app has never sent this field.
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

/// Why a client-supplied `conversation_so_far` was refused (#51 W5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WritebackConflict {
    /// Ids of messages the server holds that the client's copy does not
    /// contain. Storing that copy would delete them.
    pub missing: Vec<String>,
    /// How many messages the server holds.
    pub stored_message_count: usize,
}

/// Ids of messages `stored` holds that `client` does not, in stored order.
///
/// Message ids are durable and server-assigned (BR-45/#41), so a client's copy
/// naming a message is proof it has seen it. Anything stored that the copy does
/// not name would be DELETED by storing that copy — either an append the client
/// never saw or one it deliberately dropped, and the server cannot tell the two
/// apart. A stored row without an id cannot be reasoned about and is not
/// reported (the read path always synthesizes one, so this is defensive only).
fn unacknowledged_stored_ids(stored: &[Message], client: &[Message]) -> Vec<String> {
    let acknowledged: std::collections::HashSet<&str> =
        client.iter().filter_map(|m| m.id.as_deref()).collect();
    stored
        .iter()
        .filter_map(|m| m.id.as_deref())
        .filter(|id| !acknowledged.contains(id))
        .map(str::to_string)
        .collect()
}

/// The 409 a refused write-back answers with. Machine-readable: `code` names
/// the condition and `missing_message_ids` names exactly what the client's copy
/// would have deleted, so it can re-read the session and retry rather than
/// guess.
fn writeback_conflict_response(conflict: &WritebackConflict) -> axum::response::Response {
    (
        StatusCode::CONFLICT,
        Json(serde_json::json!({
            "type": "Error",
            "error": "conversation_so_far is missing messages this session already holds; \
                      nothing was written. Re-read the session and retry.",
            "code": "conversation_out_of_date",
            "missing_message_ids": conflict.missing,
            "stored_message_count": conflict.stored_message_count,
        })),
    )
        .into_response()
}

/// Store the client's copy of the history, IF it is safe to (#51 W5).
///
/// This used to be an unconditional [`SessionManager::replace_conversation`] —
/// the NAMED EXCEPTION, meant for a caller that owns the whole history — driven
/// by an HTTP client's copy of arbitrary staleness. `/reply`'s per-session turn
/// lock does not help: it is process-local and only `/reply` takes it, so the
/// CLI, `biorouter term log`, an apps agent socket and a second daemon on the
/// same `sessions.db` all append underneath it. Every one of those appends was
/// acknowledged before this write deleted it.
///
/// It is now conditional on the client's copy already containing everything the
/// server holds, and the write itself goes through the guarded rewrite so a
/// message landing between the check and the write is carried over rather than
/// destroyed. On [`WritebackConflict`] NOTHING is written and `/reply` answers
/// 409; the client re-reads the session and retries.
///
/// This is a deliberate DEPRECATION of the field's authoritative-overwrite
/// semantics. Nothing in this repository sends it — the desktop client posts
/// `session_id` + `user_message` only — so the compatibility cost falls
/// entirely on out-of-tree API clients, which now get a loud 409 instead of
/// silently deleting a user's messages.
async fn apply_client_writeback(
    session_manager: &SessionManager,
    session_id: &str,
    history: Vec<Message>,
) -> Result<Conversation, WritebackConflict> {
    let client = Conversation::new_unvalidated(history);

    // Read the revision BEFORE the conversation, so a message landing between
    // the two reads is inside the snapshot rather than looking foreign.
    let (session, basis) = match session_manager.snapshot_for_rewrite(session_id).await {
        Ok(snapshot) => snapshot,
        Err(e) => {
            // No session to overwrite, or the store is unreadable: there is no
            // precondition to evaluate, so nothing is written. The turn task's
            // own `get_session` surfaces the real failure to the client.
            tracing::warn!("Cannot check the client history for {session_id}: {e}");
            return Ok(client);
        }
    };
    let stored = session.conversation.unwrap_or_default();

    let missing = unacknowledged_stored_ids(stored.messages(), client.messages());
    if !missing.is_empty() {
        return Err(WritebackConflict {
            missing,
            stored_message_count: stored.messages().len(),
        });
    }

    // `known` is the CLIENT's copy rather than the snapshot's conversation. The
    // precondition that matters — `basis` must come from a real
    // `snapshot_for_rewrite` of this session — holds; and the check above
    // already proved the client's copy names every message the snapshot saw, so
    // it is a superset of the snapshot's uid set. What that buys: a row landing
    // above the watermark is foreign, and preserved, unless the client already
    // has it.
    match session_manager
        .replace_conversation_preserving_tail(session_id, &client, basis, &client)
        .await
    {
        // Stale: the store moved between the check above and the write, which
        // the guard caught inside its own transaction. Same answer as a stale
        // client copy — refuse, write nothing.
        Ok((ReplaceOutcome::Stale, _)) => Err(WritebackConflict {
            missing: Vec::new(),
            stored_message_count: stored.messages().len(),
        }),
        // The session vanished under us; let the turn task report it.
        Ok((ReplaceOutcome::SessionNotFound, _)) => Ok(client),
        Ok((_, stored_now)) => Ok(stored_now),
        Err(e) => {
            tracing::warn!("Failed to replace session conversation for {session_id}: {e}");
            Ok(client)
        }
    }
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
        code: String,
        scope: TurnErrorScope,
        retryable: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider_kind: Option<String>,
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
    /// A tool call the model has begun emitting, announced as soon as its name
    /// is known — before its arguments finish generating. Advisory only: the
    /// client draws a skeleton tool card keyed by `id` and later merges the
    /// authoritative `Message` tool request (same `id`) into it. This is NOT a
    /// tool request and must never be dispatched, gated, or persisted; it does
    /// not enter `all_messages` or the coalescer.
    ToolCallPending {
        id: String,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        partial_args: Option<String>,
    },
    UpdateConversation {
        conversation: Conversation,
        token_state: TokenState,
    },
    /// #59: the ids this turn's messages were actually persisted under.
    ///
    /// A client that watches a whole turn go by cannot otherwise learn them: a
    /// message is streamed *before* it is stored, one streamed assistant reply
    /// can become several stored rows, and the model-only rows (BR-47 post-edit
    /// diagnostics, loop-guard / stall / budget nudges, hook context) are never
    /// streamed at all. Without this frame `expectedMessageIds` on
    /// `POST /sessions/{session_id}/edit_message` — the ids of every message the
    /// client's view holds — is unsatisfiable, and the in-place edit it guards
    /// answers 409 on a session nobody else has touched.
    ///
    /// Ids only: this frame never re-sends message bodies. Each entry carries
    /// `userVisible`, which is what separates a row the client is deliberately
    /// not shown from one it was simply never told about — a client draws
    /// nothing for a `userVisible: false` row but must still name it.
    ///
    /// The converse does NOT hold: `userVisible: true` is not an instruction to
    /// draw, it is the absence of the hidden flag, and the content usually
    /// arrived already inside a `Message` frame (one streamed reply is stored as
    /// several rows). This frame is for accounting; the transcript comes from
    /// `Message` frames alone. See `PersistedMessage::user_visible`.
    MessagesPersisted {
        messages: Vec<PersistedMessage>,
    },
    Ping,
}

#[derive(Debug, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurnErrorScope {
    Provider,
    Session,
    Inference,
    Internal,
}

impl TurnErrorScope {
    /// The exact strings the `#[serde(rename_all = "snake_case")]` impl emits.
    /// Written out rather than derived through `serde_json`, so the wire values
    /// are greppable and a renamed variant is a compile error here first.
    pub(crate) fn wire_value(&self) -> &'static str {
        match self {
            TurnErrorScope::Provider => "provider",
            TurnErrorScope::Session => "session",
            TurnErrorScope::Inference => "inference",
            TurnErrorScope::Internal => "internal",
        }
    }

    /// The inverse. An unrecognized string degrades to `Internal` — a frame from
    /// a newer runner must never panic a consumer.
    ///
    /// Landed with its forward direction (Task 6) so the pair is written and
    /// reviewed together; its consumer is
    /// [`crate::routes::session_events::map_bus_event`], which turns a bus
    /// `TurnError.scope` string back into this enum.
    pub(crate) fn from_wire_value(value: &str) -> Self {
        match value {
            "provider" => TurnErrorScope::Provider,
            "session" => TurnErrorScope::Session,
            "inference" => TurnErrorScope::Inference,
            _ => TurnErrorScope::Internal,
        }
    }
}

impl MessageEvent {
    fn error(
        error: impl Into<String>,
        code: impl Into<String>,
        scope: TurnErrorScope,
        retryable: bool,
        provider_kind: Option<String>,
    ) -> Self {
        Self::Error {
            error: error.into(),
            code: code.into(),
            scope,
            retryable,
            provider_kind,
        }
    }
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
pub(crate) async fn get_token_state(
    session_manager: &SessionManager,
    session_id: &str,
) -> TokenState {
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
        serde_json::json!({
            "type": "Error",
            "error": format!("Failed to serialize stream event: {e}"),
            "code": "stream_serialization_failed",
            "scope": "internal",
            "retryable": false,
        })
        .to_string()
    });

    if tx.send(format!("data: {}\n\n", json)).await.is_err() {
        tracing::info!("client hung up");
        cancel_token.cancel();
    }
}

/// BR-53a: how long consecutive streamed text deltas are coalesced into a
/// single SSE frame, read once per `/reply` from `BIOROUTER_SSE_COALESCE_MS`.
///
/// Default (unset, empty, `0`, or unparseable) is `Duration::ZERO`, which
/// disables coalescing and keeps the byte-for-byte legacy behaviour of one SSE
/// frame per provider chunk. A non-zero value (e.g. `50`) batches same-id text
/// deltas on that millisecond window — the flush is bounded to the window and
/// happens immediately at any real boundary (see [`DeltaCoalescer`]).
fn sse_coalesce_window() -> Duration {
    std::env::var("BIOROUTER_SSE_COALESCE_MS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or(Duration::ZERO)
}

/// The delta text if `msg` is the shape the provider streams for a running
/// assistant answer — exactly one `Text` content plus a stable id. Tool
/// requests, thinking, redacted-thinking, multi-content, and id-less messages
/// are never coalesced (they always flush immediately), so nothing that carries
/// structure or ordering guarantees is ever merged.
fn coalescable_delta_text(msg: &Message) -> Option<&str> {
    if msg.id.is_none() || msg.content.len() != 1 {
        return None;
    }
    msg.content[0].as_text()
}

/// BR-53a: coalesces the provider's token-by-token text deltas so the stream
/// emits at most one SSE frame per configured window instead of one per token.
///
/// A run of same-id `Text` deltas is buffered in memory; the buffer is flushed
/// (as one `Message` carrying the concatenated text, with the run's stable id)
/// when the window elapses, when a delta with a different id arrives, when a
/// non-text message (tool request, thinking, …) or any non-`Message` event
/// arrives, or when the stream ends / is cancelled. The concatenation is exact
/// (`MessageContent::text` does not re-sanitize), so the client's append-based
/// accumulation reconstructs identical text to the un-coalesced path.
struct DeltaCoalescer {
    window: Duration,
    pending: Option<Message>,
    deadline: Option<tokio::time::Instant>,
}

impl DeltaCoalescer {
    fn new(window: Duration) -> Self {
        Self {
            window,
            pending: None,
            deadline: None,
        }
    }

    fn enabled(&self) -> bool {
        !self.window.is_zero()
    }

    /// When the buffered run must be flushed by, if anything is buffered.
    fn deadline(&self) -> Option<tokio::time::Instant> {
        self.deadline
    }

    /// Offer a streamed `Message`. Returns the messages to emit to the client
    /// *now*, in order (a flushed run and/or a pass-through); anything not
    /// returned is buffered for a later [`Self::drain`].
    fn push(&mut self, msg: Message) -> Vec<Message> {
        if !self.enabled() {
            return vec![msg];
        }
        if coalescable_delta_text(&msg).is_none() {
            // Not a coalescable text delta: flush the buffered run first so
            // ordering is preserved, then pass this message straight through.
            let mut out = Vec::new();
            out.extend(self.pending.take());
            self.deadline = None;
            out.push(msg);
            return out;
        }
        let continues_run = self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.id == msg.id);
        if continues_run {
            let delta = msg
                .content
                .into_iter()
                .next()
                .and_then(|c| c.as_text().map(str::to_owned))
                .unwrap_or_default();
            if let Some(pending) = self.pending.as_mut() {
                let current = pending
                    .content
                    .first()
                    .and_then(|c| c.as_text())
                    .unwrap_or("")
                    .to_owned();
                pending.content = vec![MessageContent::text(format!("{current}{delta}"))];
            }
            // Deadline stays anchored to the run's first delta, so latency is
            // bounded by the window regardless of how many deltas land in it.
            Vec::new()
        } else {
            // First delta of a run (or the id changed mid-stream): flush any
            // previous run, then start buffering this one.
            let flushed = self.pending.take();
            self.pending = Some(msg);
            self.deadline = Some(tokio::time::Instant::now() + self.window);
            flushed.into_iter().collect()
        }
    }

    /// Take the buffered run (if any), clearing the deadline. Called on the
    /// flush timer, at end-of-stream, and on cancellation.
    fn drain(&mut self) -> Option<Message> {
        self.deadline = None;
        self.pending.take()
    }
}

/// Flush any buffered coalesced run to the client as one `Message` frame.
async fn flush_coalesced(
    coalescer: &mut DeltaCoalescer,
    tx: &mpsc::Sender<String>,
    cancel_token: &CancellationToken,
    token_state: &TokenState,
) {
    if let Some(message) = coalescer.drain() {
        stream_event(
            MessageEvent::Message {
                message,
                token_state: token_state.clone(),
            },
            tx,
            cancel_token,
        )
        .await;
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
        (status = 409, description = "A turn is already in flight for this session, or the \
                                      supplied `conversation_so_far` is missing messages the \
                                      server holds (nothing was written; re-read the session \
                                      and retry)"),
        (status = 424, description = "Agent not initialized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn reply(
    State(state): State<Arc<AppState>>,
    Json(mut request): Json<ChatRequest>,
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

    // #51 W5: the client's copy of the history is stored HERE, before the turn
    // task is spawned, because it can now be REFUSED — once the SSE response has
    // been returned there is no status code left to say so with. Refusing drops
    // `turn_guard`, so the session is free again.
    let client_conversation = match request.conversation_so_far.take() {
        Some(history) => {
            match apply_client_writeback(state.session_manager(), &session_id, history).await {
                Ok(conversation) => Some(conversation),
                Err(conflict) => {
                    tracing::warn!(
                        "Rejected a stale conversation_so_far for session {}: {} stored \
                         message(s) it does not contain",
                        session_id,
                        conflict.missing.len()
                    );
                    return writeback_conflict_response(&conflict);
                }
            }
        }
        None => None,
    };

    let (tx, rx) = mpsc::channel(100);
    let stream = ReceiverStream::new(rx);

    let user_message = request.user_message;
    let reasoning_effort = request.reasoning_effort;

    let task_cancel = cancel_token.clone();
    let task_tx = tx.clone();
    let supervisor_tx = tx.clone();
    let supervisor_cancel = cancel_token.clone();

    let handle = tokio::spawn(async move {
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
                stream_event(
                    MessageEvent::error(
                        format!("Failed to get session agent: {e}"),
                        "agent_unavailable",
                        TurnErrorScope::Session,
                        true,
                        None,
                    ),
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
                stream_event(
                    MessageEvent::error(
                        format!("Failed to read session: {e}"),
                        "session_unavailable",
                        TurnErrorScope::Session,
                        true,
                        None,
                    ),
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

        // Either what the client's (accepted) copy actually became on disk, or
        // the stored history when it sent none.
        let mut all_messages =
            client_conversation.unwrap_or_else(|| session.conversation.unwrap_or_default());
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
                    MessageEvent::error(
                        e.to_string(),
                        "inference_start_failed",
                        TurnErrorScope::Inference,
                        false,
                        None,
                    ),
                    &task_tx,
                    &cancel_token,
                )
                .await;
                return;
            }
        };

        let mut heartbeat_interval = tokio::time::interval(Duration::from_millis(500));
        // BR-53a: batch the provider's token-by-token text deltas into one SSE
        // frame per window (`BIOROUTER_SSE_COALESCE_MS`; disabled by default).
        let mut coalescer = DeltaCoalescer::new(sse_coalesce_window());
        let mut terminal_error = false;
        loop {
            // When a run of text deltas is buffered, wake to flush it once the
            // window elapses. Disabled branch (no buffer) never fires.
            let flush_deadline = coalescer.deadline();
            tokio::select! {
                _ = task_cancel.cancelled() => {
                    tracing::info!("Agent task cancelled");
                    flush_coalesced(&mut coalescer, &tx, &cancel_token, &token_state).await;
                    break;
                }
                _ = heartbeat_interval.tick() => {
                    stream_event(MessageEvent::Ping, &tx, &cancel_token).await;
                }
                _ = tokio::time::sleep_until(flush_deadline.unwrap_or_else(tokio::time::Instant::now)),
                    if flush_deadline.is_some() =>
                {
                    flush_coalesced(&mut coalescer, &tx, &cancel_token, &token_state).await;
                }
                response = timeout(Duration::from_millis(500), stream.next()) => {
                    match response {
                        Ok(Some(Ok(AgentEvent::Message(message)))) => {
                            for content in &message.content {
                                track_tool_telemetry(content, all_messages.messages());
                            }

                            all_messages.push(message.clone());

                            for message in coalescer.push(message) {
                                stream_event(MessageEvent::Message { message, token_state: token_state.clone() }, &tx, &cancel_token).await;
                            }
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
                            flush_coalesced(&mut coalescer, &tx, &cancel_token, &token_state).await;
                            all_messages = new_messages.clone();
                            stream_event(MessageEvent::UpdateConversation {conversation: new_messages, token_state: token_state.clone()}, &tx, &cancel_token).await;

                        }
                        Ok(Some(Ok(AgentEvent::MessagesPersisted(messages)))) => {
                            // #59: the agent just made these rows durable. Flush
                            // any buffered text first so the client never learns
                            // an id before the message it belongs to — the
                            // coalescer can be holding the very delta whose row
                            // this names.
                            flush_coalesced(&mut coalescer, &tx, &cancel_token, &token_state).await;
                            stream_event(MessageEvent::MessagesPersisted { messages }, &tx, &cancel_token).await;
                        }
                        Ok(Some(Ok(AgentEvent::ModelChange { model, mode }))) => {
                            flush_coalesced(&mut coalescer, &tx, &cancel_token, &token_state).await;
                            stream_event(MessageEvent::ModelChange { model, mode }, &tx, &cancel_token).await;
                        }
                        Ok(Some(Ok(AgentEvent::McpNotification((request_id, n))))) => {
                            flush_coalesced(&mut coalescer, &tx, &cancel_token, &token_state).await;
                            stream_event(MessageEvent::Notification{
                                request_id: request_id.clone(),
                                message: n,
                            }, &tx, &cancel_token).await;
                        }
                        Ok(Some(Ok(AgentEvent::ToolCallPending(pending)))) => {
                            // Advisory skeleton for the UI. Deliberately does NOT
                            // touch `all_messages` or `track_tool_telemetry` — it
                            // is not a tool request and must never be counted or
                            // dispatched. Flush buffered text first so the card
                            // appears after the assistant prose that precedes it.
                            flush_coalesced(&mut coalescer, &tx, &cancel_token, &token_state).await;
                            stream_event(MessageEvent::ToolCallPending {
                                id: pending.id,
                                name: pending.name,
                                partial_args: pending.partial_args,
                            }, &tx, &cancel_token).await;
                        }

                        Ok(Some(Ok(AgentEvent::TurnAborted { code, message }))) => {
                            // The turn ended without doing its work. The agent
                            // already yielded the human-readable assistant message,
                            // but the stream used to then finish *normally* — so the
                            // desktop rendered a provider 403 as a completed turn.
                            // Surface it as a real error and stop.
                            tracing::error!(abort = code.wire_code(), "Turn aborted: {message}");
                            let (scope, retryable, provider_kind) = match &code {
                                TurnAbortCode::ProviderFailure { kind } => (
                                    TurnErrorScope::Provider,
                                    kind.is_transient(),
                                    Some(kind.wire_code().to_string()),
                                ),
                                // #31/#41: a session-store failure is a
                                // Session-scoped error — not the provider's
                                // fault and not retryable until the local
                                // db problem is fixed.
                                TurnAbortCode::SessionStore => {
                                    (TurnErrorScope::Session, false, None)
                                }
                                TurnAbortCode::ToolLoop { .. } => {
                                    (TurnErrorScope::Inference, false, None)
                                }
                                TurnAbortCode::WorkerTimeout { .. } => {
                                    (TurnErrorScope::Inference, true, None)
                                }
                            };
                            stream_event(
                                MessageEvent::error(
                                    message,
                                    code.wire_code(),
                                    scope,
                                    retryable,
                                    provider_kind,
                                ),
                                &tx,
                                &cancel_token,
                            ).await;
                            terminal_error = true;
                            break;
                        }
                        Ok(Some(Err(e))) => {
                            flush_coalesced(&mut coalescer, &tx, &cancel_token, &token_state).await;
                            tracing::error!("Error processing message: {}", e);
                            stream_event(
                                MessageEvent::error(
                                    e.to_string(),
                                    "inference_error",
                                    TurnErrorScope::Inference,
                                    false,
                                    None,
                                ),
                                &tx,
                                &cancel_token,
                            ).await;
                            terminal_error = true;
                            break;
                        }
                        Ok(None) => {
                            flush_coalesced(&mut coalescer, &tx, &cancel_token, &token_state).await;
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
        let exit_type = if terminal_error {
            "error"
        } else if task_cancel.is_cancelled() {
            "cancelled"
        } else {
            "normal"
        };

        if let Ok(session) = state.session_manager().get_session(&session_id, true).await {
            let total_tokens = session.total_tokens.unwrap_or(0);
            tracing::info!(
                counter.biorouter.session_completions = 1,
                session_type = "app",
                interface = "ui",
                exit_type = exit_type,
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
                exit_type = exit_type,
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

        if !terminal_error {
            stream_event(
                MessageEvent::Finish {
                    reason: if task_cancel.is_cancelled() {
                        "cancelled".to_string()
                    } else {
                        "stop".to_string()
                    },
                    token_state: final_token_state,
                },
                &task_tx,
                &cancel_token,
            )
            .await;
        }
    });

    tokio::spawn(async move {
        if let Err(join_error) = handle.await {
            tracing::error!("Reply task terminated unexpectedly: {join_error}");
            stream_event(
                MessageEvent::error(
                    "The model turn ended unexpectedly. Please retry.",
                    "internal_error",
                    TurnErrorScope::Internal,
                    true,
                    None,
                ),
                &supervisor_tx,
                &supervisor_cancel,
            )
            .await;
        }
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

    #[test]
    fn error_events_preserve_machine_readable_metadata() {
        let event = MessageEvent::error(
            "quota exhausted",
            "provider_failure",
            TurnErrorScope::Provider,
            false,
            Some("quota".to_string()),
        );

        let value = serde_json::to_value(event).unwrap();
        assert_eq!(value["type"], "Error");
        assert_eq!(value["error"], "quota exhausted");
        assert_eq!(value["code"], "provider_failure");
        assert_eq!(value["scope"], "provider");
        assert_eq!(value["retryable"], false);
        assert_eq!(value["provider_kind"], "quota");
    }

    /// #59: the frame that makes `expectedMessageIds` satisfiable has to reach
    /// the client as *data*, not as a re-send of the messages. A client reads
    /// `messages[].id` into the set it will hand back to
    /// `POST /sessions/{id}/edit_message`, and `userVisible` tells it which of
    /// those it must draw — the difference between a row it is deliberately not
    /// shown and one it was never told about.
    #[test]
    fn persisted_message_ids_reach_the_wire_with_their_visibility() {
        let event = MessageEvent::MessagesPersisted {
            messages: vec![
                PersistedMessage {
                    id: "019fa8-visible".to_string(),
                    user_visible: true,
                },
                PersistedMessage {
                    id: "019fa8-model-only".to_string(),
                    user_visible: false,
                },
            ],
        };

        let value = serde_json::to_value(event).unwrap();
        assert_eq!(value["type"], "MessagesPersisted");
        assert_eq!(value["messages"][0]["id"], "019fa8-visible");
        assert_eq!(value["messages"][0]["userVisible"], true);
        assert_eq!(value["messages"][1]["id"], "019fa8-model-only");
        assert_eq!(value["messages"][1]["userVisible"], false);
        // Ids only — a client must never have to diff message bodies to learn
        // what the store holds, and re-sending them per turn would be a second
        // copy of the whole transcript on the hot path.
        assert!(value["messages"][0].get("content").is_none());
    }

    /// #51 W5: `conversation_so_far` overwriting a live session.
    ///
    /// Driven against an ISOLATED `SessionManager` over a temp dir rather than
    /// the process-global store, so these can seed real sessions without
    /// touching the developer's own `sessions.db`.
    mod writeback_tests {
        use super::*;
        use biorouter::session::session_manager::SessionType;
        use std::path::PathBuf;
        use tempfile::TempDir;

        async fn seeded() -> (TempDir, SessionManager, String) {
            let temp_dir = TempDir::new().unwrap();
            let sm = SessionManager::new(temp_dir.path().to_path_buf());
            let id = sm
                .create_session(PathBuf::from("/tmp/wb"), "wb".into(), SessionType::User)
                .await
                .unwrap()
                .id;
            (temp_dir, sm, id)
        }

        async fn stored_texts(sm: &SessionManager, id: &str) -> Vec<String> {
            sm.get_session(id, true)
                .await
                .unwrap()
                .conversation
                .unwrap()
                .messages()
                .iter()
                .map(|m| m.as_concat_text())
                .collect()
        }

        /// THE defect. A client whose copy of the history predates a message
        /// another writer appended — `biorouter term log`, a second socket, a
        /// note tool, the CLI on the same `sessions.db` — used to have that
        /// copy stored verbatim, deleting a message whose writer had already
        /// been told the append succeeded. The `/reply` turn lock does not
        /// cover any of those writers.
        #[tokio::test]
        async fn a_stale_client_copy_cannot_delete_an_acknowledged_message() {
            let (_temp, sm, id) = seeded().await;
            sm.add_message(&id, &Message::user().with_text("one"))
                .await
                .unwrap();
            let client = sm
                .get_session(&id, true)
                .await
                .unwrap()
                .conversation
                .unwrap();

            // ...and only now does somebody else append.
            sm.add_message(&id, &Message::user().with_text("NOTE from elsewhere"))
                .await
                .unwrap();

            let conflict = apply_client_writeback(&sm, &id, client.messages().to_vec())
                .await
                .expect_err("a stale client copy must be refused");

            assert_eq!(conflict.stored_message_count, 2);
            assert_eq!(conflict.missing.len(), 1);
            assert_eq!(
                stored_texts(&sm, &id).await,
                vec!["one".to_string(), "NOTE from elsewhere".to_string()],
                "a refused write-back must leave the store untouched"
            );
        }

        /// The compatible path: a client that is up to date still gets its copy
        /// stored, so an API client using the field keeps working.
        #[tokio::test]
        async fn an_up_to_date_client_copy_is_still_stored() {
            let (_temp, sm, id) = seeded().await;
            sm.add_message(&id, &Message::user().with_text("one"))
                .await
                .unwrap();
            let mut client = sm
                .get_session(&id, true)
                .await
                .unwrap()
                .conversation
                .unwrap()
                .messages()
                .to_vec();
            client.push(Message::assistant().with_text("client-side answer"));

            let stored = apply_client_writeback(&sm, &id, client).await.unwrap();

            assert_eq!(
                stored
                    .messages()
                    .iter()
                    .map(|m| m.as_concat_text())
                    .collect::<Vec<_>>(),
                vec!["one".to_string(), "client-side answer".to_string()]
            );
            assert_eq!(
                stored_texts(&sm, &id).await,
                vec!["one".to_string(), "client-side answer".to_string()]
            );
        }

        /// A client that deliberately drops a stored message is refused too:
        /// from the server there is no way to tell "I edited this away" apart
        /// from "I never saw it". Message edits have their own endpoint
        /// (`POST /sessions/{id}/edit_message`), which is where that intent
        /// belongs.
        #[tokio::test]
        async fn a_client_copy_that_drops_a_stored_message_is_refused() {
            let (_temp, sm, id) = seeded().await;
            sm.add_message(&id, &Message::user().with_text("one"))
                .await
                .unwrap();
            sm.add_message(&id, &Message::user().with_text("two"))
                .await
                .unwrap();
            let full = sm
                .get_session(&id, true)
                .await
                .unwrap()
                .conversation
                .unwrap();
            let trimmed = vec![full.messages()[0].clone()];

            apply_client_writeback(&sm, &id, trimmed)
                .await
                .expect_err("dropping a stored message must be refused");
            assert_eq!(
                stored_texts(&sm, &id).await,
                vec!["one".to_string(), "two".to_string()]
            );
        }

        /// An empty client copy would wipe the session outright.
        #[tokio::test]
        async fn an_empty_client_copy_cannot_wipe_a_session() {
            let (_temp, sm, id) = seeded().await;
            sm.add_message(&id, &Message::user().with_text("one"))
                .await
                .unwrap();

            apply_client_writeback(&sm, &id, Vec::new())
                .await
                .expect_err("an empty copy must be refused");
            assert_eq!(stored_texts(&sm, &id).await, vec!["one".to_string()]);
        }

        /// A session that does not exist has no history to protect, and the
        /// turn task reports the missing session itself — so the write-back is
        /// a silent no-op rather than a 409 the client cannot act on.
        #[tokio::test]
        async fn an_unknown_session_is_not_reported_as_a_conflict() {
            let (_temp, sm, _id) = seeded().await;
            let conv = apply_client_writeback(
                &sm,
                "no-such-session",
                vec![Message::user().with_text("hi")],
            )
            .await
            .expect("a missing session is the turn task's error to report");
            assert_eq!(conv.messages().len(), 1);
        }

        /// The refusal has to be actionable: a status the client can branch on
        /// and the exact ids its copy would have destroyed.
        #[tokio::test]
        async fn the_conflict_response_names_what_would_have_been_destroyed() {
            let conflict = WritebackConflict {
                missing: vec!["msg-a".into(), "msg-b".into()],
                stored_message_count: 7,
            };
            let response = writeback_conflict_response(&conflict);
            assert_eq!(response.status(), StatusCode::CONFLICT);

            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(body["code"], "conversation_out_of_date");
            assert_eq!(
                body["missing_message_ids"],
                serde_json::json!(["msg-a", "msg-b"])
            );
            assert_eq!(body["stored_message_count"], serde_json::json!(7));
        }
    }

    /// BR-53a: the pure state machine that batches streamed text deltas.
    mod coalesce_tests {
        use super::*;
        use biorouter::conversation::message::Message;

        fn delta(id: &str, text: &str) -> Message {
            Message::assistant().with_id(id).with_text(text)
        }

        #[test]
        fn disabled_window_passes_each_delta_straight_through() {
            let mut c = DeltaCoalescer::new(Duration::ZERO);
            assert!(!c.enabled());
            let out = c.push(delta("a", "hello "));
            assert_eq!(out.len(), 1);
            assert_eq!(out[0].as_concat_text(), "hello ");
            // Nothing is ever buffered when disabled.
            assert!(c.deadline().is_none());
            assert!(c.drain().is_none());
        }

        #[test]
        fn same_id_text_deltas_merge_until_drained() {
            let mut c = DeltaCoalescer::new(Duration::from_millis(50));
            assert!(c.push(delta("a", "he")).is_empty());
            assert!(c.push(delta("a", "ll")).is_empty());
            assert!(c.push(delta("a", "o")).is_empty());
            assert!(c.deadline().is_some());

            let flushed = c.drain().expect("a buffered run");
            assert_eq!(flushed.as_concat_text(), "hello");
            assert_eq!(flushed.id.as_deref(), Some("a"));
            assert!(c.deadline().is_none());
        }

        #[test]
        fn a_new_message_id_flushes_the_previous_run() {
            let mut c = DeltaCoalescer::new(Duration::from_millis(50));
            assert!(c.push(delta("a", "aaa")).is_empty());

            let out = c.push(delta("b", "bbb"));
            assert_eq!(out.len(), 1);
            assert_eq!(out[0].as_concat_text(), "aaa");
            assert_eq!(out[0].id.as_deref(), Some("a"));

            // The new id is now buffered.
            let buffered = c.drain().expect("second run");
            assert_eq!(buffered.as_concat_text(), "bbb");
            assert_eq!(buffered.id.as_deref(), Some("b"));
        }

        #[test]
        fn non_text_message_flushes_run_then_passes_through_in_order() {
            let mut c = DeltaCoalescer::new(Duration::from_millis(50));
            assert!(c.push(delta("a", "partial ")).is_empty());

            // Two text contents => not a single-text delta => not coalescable.
            let multi = Message::assistant()
                .with_id("m")
                .with_text("x")
                .with_text("y");
            let out = c.push(multi);
            assert_eq!(out.len(), 2);
            assert_eq!(out[0].as_concat_text(), "partial "); // flushed run first
            assert_eq!(out[0].id.as_deref(), Some("a"));
            assert_eq!(out[1].id.as_deref(), Some("m")); // then the pass-through
            assert!(c.deadline().is_none());
            assert!(c.drain().is_none());
        }

        #[test]
        fn id_less_message_is_never_coalesced() {
            let mut c = DeltaCoalescer::new(Duration::from_millis(50));
            let out = c.push(Message::assistant().with_text("no id"));
            assert_eq!(out.len(), 1);
            assert_eq!(out[0].as_concat_text(), "no id");
            assert!(c.deadline().is_none());
        }

        #[test]
        fn coalesced_concatenation_is_byte_exact() {
            // No spacing or re-sanitising is introduced by merging.
            let mut c = DeltaCoalescer::new(Duration::from_millis(50));
            for chunk in ["The ", "quick", " brown\n", "fox"] {
                assert!(c.push(delta("x", chunk)).is_empty());
            }
            let merged = c.drain().unwrap();
            assert_eq!(merged.as_concat_text(), "The quick brown\nfox");
        }
    }

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
