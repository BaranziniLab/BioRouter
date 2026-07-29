use crate::state::AppState;
use axum::{
    extract::{DefaultBodyLimit, State},
    http::{self, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use biorouter::agents::{InterruptRefused, PersistedMessage, ReasoningEffort};
use biorouter::conversation::message::{Message, MessageContent, TokenState};
use biorouter::conversation::Conversation;
use biorouter::session::session_manager::ReplaceOutcome;
use biorouter::session::SessionManager;
use bytes::Bytes;
use futures::Stream;
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

/// What a `/reply` SSE consumer does when it falls behind the session bus.
///
/// Before BR-71 this could not happen: the agent stream was throttled by the
/// `mpsc::channel(100)` into the SSE response, so a slow client slowed the
/// turn. With one runner publishing to a broadcast bus (design §4.2), the
/// publisher never blocks — which is the point, since an observer must never be
/// able to stall a turn — and a stalled renderer can miss frames instead.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum BusLagAction {
    /// Re-send the whole conversation from storage. Costs one session read;
    /// leaves the client correct rather than subtly short a tool result.
    ResyncFromStorage,
}

pub(crate) fn on_bus_lag_action() -> BusLagAction {
    BusLagAction::ResyncFromStorage
}

/// How long the supervisor lets the SSE task drain after the runner returns,
/// before releasing it. Only reached when the SSE task has NOT already ended on
/// a terminal frame, i.e. when the runner died without publishing one.
const RUNNER_EXIT_DRAIN_GRACE: Duration = Duration::from_millis(250);

/// Own the turn's end-of-life, for BOTH failure modes.
///
/// 1. **The runner panicked.** Send the one internal-error frame (unchanged
///    behaviour).
/// 2. **The SSE task is still waiting when the runner is gone.** Release it.
///    This is new and it is load-bearing: pre-refactor the turn task owned
///    `task_tx`, so its return — panic included — dropped the sender and closed
///    the body. Now the SSE task owns its own sender and only breaks on a
///    terminal bus event, `RecvError::Closed`, or `cancel_token`. A panicking
///    runner publishes no terminal event; `Closed` is unreachable because this
///    consumer's own `Receiver` keeps the session's `broadcast::Sender` alive;
///    and `TurnGuard::drop` does not trip the token (`state.rs`, `TurnGuard`'s
///    `Drop` impl). Without this the response stays open forever after the error
///    frame.
///
/// The grace period is what keeps case 2 from truncating a HEALTHY turn: the
/// runner publishes `TurnFinished` and returns, and the SSE task may not have
/// consumed it yet. Waiting on the SSE task's own handle first means a normal
/// turn is never cancelled behind its back.
async fn supervise_turn(
    runner: tokio::task::JoinHandle<()>,
    sse: tokio::task::JoinHandle<()>,
    tx: mpsc::Sender<String>,
    cancel: CancellationToken,
) {
    if let Err(join_error) = runner.await {
        tracing::error!("Reply task terminated unexpectedly: {join_error}");
        stream_event(
            MessageEvent::error(
                "The model turn ended unexpectedly. Please retry.",
                "internal_error",
                TurnErrorScope::Internal,
                true,
                None,
            ),
            &tx,
            &cancel,
        )
        .await;
    }
    if tokio::time::timeout(RUNNER_EXIT_DRAIN_GRACE, sse)
        .await
        .is_err()
    {
        tracing::warn!(
            counter.biorouter.reply_sse_released_by_supervisor = 1,
            "turn ended without a terminal frame; releasing the SSE stream"
        );
        cancel.cancel();
    }
}

/// The subscription half of `/reply`: one bus consumer, one HTTP response.
///
/// Named rather than inlined into the handler for one reason — every branch
/// below is a property the refactor must not lose, and an inline `tokio::spawn`
/// block inside a route handler is reachable by no test at all. The three that
/// matter are the ones a plausible wrong implementation would still ship green:
/// the flush that carries #59's ordering invariant across the coalescer, the
/// `Lagged` resync, and breaking on the terminal frame. Each is pinned by a test
/// that drives THIS function against the real bus (`mod tests`), so removing one
/// turns a suite red instead of only being described in a comment.
///
/// `coalesce_window` is a parameter rather than a call to
/// [`sse_coalesce_window`] so a test can run the loop with coalescing ON.
/// `BIOROUTER_SSE_COALESCE_MS` is unset in tests, and with a zero window the
/// coalescer passes every delta straight through — so the flush ordering this
/// function exists to guarantee would never execute under test.
///
/// Per-REQUEST concerns only: coalescing, heartbeat, resync-on-lag, and
/// terminating the SSE response on the turn's terminal frame. Everything about
/// the turn itself belongs to the runner.
async fn stream_bus_to_client(
    state: Arc<AppState>,
    session_id: String,
    mut bus: biorouter::session_events::Subscription,
    tx: mpsc::Sender<String>,
    cancel: CancellationToken,
    coalesce_window: Duration,
) {
    let mut token_state = get_token_state(state.session_manager(), &session_id).await;
    let mut heartbeat_interval = tokio::time::interval(Duration::from_millis(500));
    // BR-53a: batch the provider's token-by-token text deltas into one SSE
    // frame per window (`BIOROUTER_SSE_COALESCE_MS`; disabled by default).
    let mut coalescer = DeltaCoalescer::new(coalesce_window);
    loop {
        let flush_deadline = coalescer.deadline();
        tokio::select! {
            () = cancel.cancelled() => {
                flush_coalesced(&mut coalescer, &tx, &cancel, &token_state).await;
                break;
            }
            _ = heartbeat_interval.tick() => {
                stream_event(MessageEvent::Ping, &tx, &cancel).await;
            }
            () = tokio::time::sleep_until(
                    flush_deadline.unwrap_or_else(tokio::time::Instant::now)),
                if flush_deadline.is_some() =>
            {
                flush_coalesced(&mut coalescer, &tx, &cancel, &token_state).await;
            }
            received = bus.recv() => match received {
                Ok(biorouter::session_events::SessionBusEvent::Agent(
                    biorouter::agents::AgentEvent::Message(message),
                )) => {
                    // Coalescing is a client concern, so it stays here and
                    // is applied to the bus's Message events.
                    for message in coalescer.push(message) {
                        stream_event(
                            MessageEvent::Message { message, token_state: token_state.clone() },
                            &tx,
                            &cancel,
                        )
                        .await;
                    }
                }
                Ok(event) => {
                    let terminal = matches!(
                        event,
                        biorouter::session_events::SessionBusEvent::TurnFinished { .. }
                            | biorouter::session_events::SessionBusEvent::TurnError { .. }
                    );
                    if !matches!(
                        event,
                        biorouter::session_events::SessionBusEvent::Agent(
                            biorouter::agents::AgentEvent::TokenUsage(_)
                        )
                    ) {
                        // Anything that is not pure token bookkeeping ends a
                        // coalescing run, so cards appear after the prose
                        // that precedes them.
                        //
                        // ⚠ DO NOT narrow this to "flush only on terminal
                        // frames". It is what carries #59's ordering
                        // invariant across the coalescer: *no
                        // `MessagesPersisted` may precede a `Message` frame
                        // carrying one of the ids it publishes*
                        // (`agent.rs`). The coalescer can be holding the
                        // very delta whose stored row the next
                        // `MessagesPersisted` names, and the pre-refactor
                        // handler said exactly this in its own
                        // `MessagesPersisted` arm. Emitted backwards, a
                        // client that reads the id and then loses the stream
                        // claims every stored row while holding none of the
                        // bodies, passes the `expectedMessageIds` guard on a
                        // short transcript, and has the server truncate rows
                        // still on its screen.
                        //
                        // Pinned by `the_sse_loop_flushes_buffered_text_before_a_persisted_frame`
                        // and `token_usage_alone_does_not_break_a_coalescing_run`,
                        // which drive this loop with a non-zero window; narrowing
                        // the condition either way turns one of them red.
                        flush_coalesced(&mut coalescer, &tx, &cancel, &token_state).await;
                    }
                    if let Some(frame) = crate::routes::session_events::map_bus_event(
                        event,
                        &mut token_state,
                    ) {
                        stream_event(frame, &tx, &cancel).await;
                    }
                    if terminal {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    // BR-71 §8.4 + reconciliation #9: the publisher no longer
                    // blocks on this consumer, so a stalled renderer can fall
                    // behind. Resync from storage rather than silently
                    // dropping frames.
                    //
                    // #59 INTERACTION, stated because it is a real behaviour
                    // difference a user can hit. On `Lagged` this consumer can
                    // skip a `Message` frame and still receive the later
                    // `MessagesPersisted` naming its id — the exact
                    // "claims every stored row while holding none of the
                    // bodies" failure `agent.rs` describes. The resync below
                    // is what makes that safe, and it is safe for a specific
                    // reason: `bus_lag_resync_frame` reads
                    // `get_session(id, true)`, which INCLUDES hidden rows, so
                    // the client's view is restored complete rather than
                    // partial. The secondary effect is that the desktop's
                    // `viewNamesEveryStoredRow` gate clears on any wholesale
                    // `UpdateConversation` (`936f5a33`), so a client that lags
                    // once omits `expectedMessageIds` on its next in-place
                    // edit until it re-reads the session. Omission is the safe
                    // direction (the guard checks `stored ∖ client`), but it
                    // is a capability difference between the lag and non-lag
                    // paths and must not be discovered by a user.
                    //
                    // Pinned by `a_lagged_sse_loop_resyncs_the_client_from_storage`,
                    // which overruns the real ring before this loop's first
                    // `recv` — deleting the resync leaves the client with no
                    // frame at all and the test times out.
                    tracing::warn!(
                        counter.biorouter.reply_bus_lagged = 1,
                        skipped,
                        "reply SSE consumer lagged; resyncing from storage"
                    );
                    debug_assert!(matches!(on_bus_lag_action(), BusLagAction::ResyncFromStorage));
                    flush_coalesced(&mut coalescer, &tx, &cancel, &token_state).await;
                    if let Some(resync) = crate::routes::session_events::bus_lag_resync_frame(
                        &state,
                        &session_id,
                        &token_state,
                    )
                    .await
                    {
                        stream_event(resync, &tx, &cancel).await;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            },
        }
    }
}

/// The 409 body a refused `/reply` answers with, kept byte-identical to what the
/// handler has always produced.
///
/// `duplicate` is the whole point of the distinction: a client that re-POSTs the
/// same `turn_id` (an SSE reconnect) is told "this turn is already in progress",
/// which it treats as "re-attach", while a genuinely concurrent caller is told
/// "a turn is already in progress" and backs off. Same status, different copy —
/// so both strings live here together where they cannot drift apart.
fn turn_conflict_response(
    session_id: &str,
    conflict: &crate::state::TurnConflict,
) -> axum::response::Response {
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
    (
        StatusCode::CONFLICT,
        Json(serde_json::json!({
            "type": "Error",
            "error": error,
            "running_turn_id": conflict.running_turn_id,
            "duplicate": conflict.duplicate,
        })),
    )
        .into_response()
}

/// One `workflow_runs` counter per workflow *run*, not per turn: the
/// `mark_workflow_run_if_absent` gate is what keeps a ten-turn workflow from
/// reporting ten runs, and it is a session-scoped latch, so this must stay a
/// single call site.
async fn record_workflow_run(state: &Arc<AppState>, session_id: &str, request: &ChatRequest) {
    let Some(workflow_name) = request.workflow_name.clone() else {
        return;
    };
    if !state.mark_workflow_run_if_absent(session_id).await {
        return;
    }
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
    // Turn *duration* is the runner's (`emit_completion_metrics`); this handler
    // only reports that a session was started, at request entry.
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
        Err(conflict) => return turn_conflict_response(&session_id, &conflict),
    };

    record_workflow_run(&state, &session_id, &request).await;

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

    // BR-71: subscribe BEFORE the turn task is spawned, so no event can fall
    // into the gap between "turn started" and "we are listening".
    let bus = biorouter::session_events::subscribe(&session_id);

    // Re-declared from the deleted turn-task block: the supervisor outlives both
    // the turn task and the SSE task, so it needs its own sender and token
    // clones. (`task_cancel` / `task_tx` are gone with the turn task.)
    let supervisor_tx = tx.clone();
    let supervisor_cancel = cancel_token.clone();

    let turn_request = crate::workspace::turn::TurnRequest {
        session_id: session_id.clone(),
        user_message: request.user_message,
        extras: crate::workspace::turn::TurnExtras {
            // The lock is already held under this key; the runner receives the
            // guard rather than re-acquiring, so the key is informational here.
            idempotency_key: request.turn_id.clone(),
            // #51 W5: the ALREADY-STORED conversation, produced by
            // `apply_client_writeback` above this range, and deliberately so: it
            // is a precondition that can answer 409, which a detached task
            // cannot. `None` when the client sent no copy. The runner treats
            // this as a seed and performs no storage write of its own (see
            // `TurnExtras.conversation_so_far`).
            conversation_so_far: client_conversation,
            reasoning_effort: request.reasoning_effort,
            // An interactive turn is already visible as a turn; only injected
            // turns register in active_work.
            register_active_work: false,
            // Supplies the `session_type = "app"` telemetry label the three
            // completion counters have always reported.
            kind: crate::workspace::turn::TurnKind::Interactive,
        },
    };

    let runner_state = state.clone();
    let runner_cancel = cancel_token.clone();
    let handle = tokio::spawn(crate::workspace::turn::run_turn(
        runner_state,
        turn_request,
        turn_guard,
        runner_cancel,
    ));

    // The subscription half — see `stream_bus_to_client`, which is a named
    // function precisely so its branches are reachable by a test.
    let sse_handle = tokio::spawn(stream_bus_to_client(
        state.clone(),
        session_id.clone(),
        bus,
        tx.clone(),
        cancel_token.clone(),
        sse_coalesce_window(),
    ));

    tokio::spawn(supervise_turn(
        handle,
        sse_handle,
        supervisor_tx,
        supervisor_cancel,
    ));
    SseResponse::new(stream).into_response()
}

/// Request body for the soft-interrupt route.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct InterruptRequest {
    pub session_id: String,
    pub text: String,
}

/// Response body for an accepted soft interrupt (#69).
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct InterruptAccepted {
    /// The agent-loop turn that took the message and will inject it. Identifies
    /// the reply loop, and is deliberately *not* the `turn_id` `/agent/cancel`
    /// reports (that one names the server's turn lock); the two id spaces are
    /// shaped differently so they cannot be confused.
    pub turn_id: String,
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
///
/// #69: the 409 is now decided by the *agent's own queue*, not by the turn-lock
/// check above it. Checking the lock and then queueing are two steps against
/// state the reply loop changes underneath them: a steer could be accepted (202)
/// after the loop had already performed its final empty-queue check, and the text
/// then surfaced in an unrelated later turn. `try_queue_soft_interrupt` decides
/// acceptance and enqueues in one critical section, and reports back *which* turn
/// took the message — so a 202 names something real.
#[utoipa::path(
    post,
    path = "/interrupt",
    request_body = InterruptRequest,
    responses(
        (status = 202, description = "Message queued for injection into the running turn", body = InterruptAccepted),
        (status = 400, description = "Empty message text"),
        (status = 409, description = "No turn is accepting interrupts for this session"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn interrupt(
    State(state): State<Arc<AppState>>,
    Json(req): Json<InterruptRequest>,
) -> Result<(StatusCode, Json<InterruptAccepted>), StatusCode> {
    if req.text.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    // Cheap early-out only: it avoids constructing an agent for an idle session.
    // It is no longer the guard — see `try_queue_soft_interrupt` below.
    if !state.is_turn_active(&req.session_id) {
        return Err(StatusCode::CONFLICT);
    }
    let agent = state.get_agent_for_route(req.session_id).await?;
    match agent.try_queue_soft_interrupt(req.text, None) {
        Ok(turn_id) => Ok((
            StatusCode::ACCEPTED,
            Json(InterruptAccepted {
                turn_id: turn_id.into(),
            }),
        )),
        // #69: the turn the caller addressed has ended. Refusing is the honest
        // answer — queueing for whatever runs next is the bug this replaces.
        Err(InterruptRefused::TurnEnded) => Err(StatusCode::CONFLICT),
    }
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

    /// **[#59]** The accounting frame still reaches the WIRE, through the real
    /// handler, after the refactor.
    ///
    /// This is the only test in this plan that can catch the failure mode the
    /// 7 → 8 ordering exists to prevent: `map_bus_event` repaired with
    /// `_ => None`, or a bus loop that filters the frame out. Everything else
    /// stays green — `reply.rs`'s own
    /// `persisted_message_ids_reach_the_wire_with_their_visibility` tests the
    /// enum, not the handler, and the desktop store deliberately does not
    /// consume the frame yet. The consequence of missing it is a 409 on
    /// `POST /sessions/{id}/edit_message` for every session, in the shipped app.
    ///
    /// It publishes onto the bus directly rather than driving a real turn: a
    /// provider-less turn persists nothing, so the frame has to be injected to
    /// be observed. That is exactly the right scope here — the property under
    /// test is `bus → wire`, which is `/reply`'s half; the agent's half
    /// (`persist → bus`) is held by
    /// `conversation_writeback_freshness::a_client_that_watched_the_turn_knows_every_stored_message_id`.
    #[tokio::test]
    async fn a_persisted_batch_on_the_bus_reaches_the_reply_client() {
        use biorouter::agents::{AgentEvent, PersistedMessage};
        use biorouter::session_events::{self, SessionBusEvent};
        use tower::ServiceExt;

        let state = AppState::new().await.unwrap();
        let temp = tempfile::TempDir::new().unwrap();
        let session = state
            .session_manager()
            .create_session(
                temp.path().to_path_buf(),
                "reply-persisted-frame".to_string(),
                biorouter::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();

        // A PUMP, not a single publish, and that is not belt-and-braces.
        // `broadcast::Receiver` only sees sends made after it was created, and
        // the handler subscribes inside itself — so a one-shot publish before
        // the request is guaranteed to be missed, and one after it races the
        // provider-less turn's fast Error (which breaks the SSE loop and closes
        // the body). Publishing continuously for the request's whole life makes
        // the test deterministic in the only direction that matters: at least
        // one publish lands strictly between "subscribed" and "terminal".
        let sid = session.id.clone();
        let pump = tokio::spawn(async move {
            for _ in 0..400 {
                session_events::publish(
                    &sid,
                    SessionBusEvent::Agent(AgentEvent::MessagesPersisted(vec![PersistedMessage {
                        id: "probe-1".into(),
                        user_visible: true,
                    }])),
                );
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        });

        let body = serde_json::json!({
            "user_message": serde_json::to_value(
                biorouter::conversation::message::Message::user().with_text("hi")
            ).unwrap(),
            "session_id": session.id,
        });
        let response = routes(state.clone())
            .oneshot(
                axum::http::Request::post("/reply")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        pump.abort();

        let text = String::from_utf8_lossy(&bytes);
        assert!(
            text.contains("\"type\":\"MessagesPersisted\""),
            "the #59 accounting frame never reached the wire. If `map_bus_event` \
             was repaired with `_ => None`, this is that. Body: {text}"
        );
        assert!(text.contains("probe-1"), "…and it carries the ids: {text}");
    }

    /// The wire contract: a turn's frames reach the /reply client exactly as
    /// before, and a concurrent observer sees the same ones.
    #[tokio::test]
    async fn reply_streams_the_turn_and_an_observer_sees_the_same_frames() {
        use tower::ServiceExt;
        let state = AppState::new().await.unwrap();
        let temp = tempfile::TempDir::new().unwrap();
        let session = state
            .session_manager()
            .create_session(
                temp.path().to_path_buf(),
                "reply-refactor".to_string(),
                biorouter::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();

        let mut observer = biorouter::session_events::subscribe(&session.id);

        // No provider is configured on this fresh agent, so the turn starts and
        // fails fast — the lifecycle bracket and the error envelope are what we
        // assert, and both must survive the refactor.
        let body = serde_json::json!({
            "user_message": serde_json::to_value(
                biorouter::conversation::message::Message::user().with_text("hi")
            ).unwrap(),
            "session_id": session.id,
        });
        let app = routes(state.clone());
        let response = app
            .oneshot(
                axum::http::Request::post("/reply")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let text = String::from_utf8_lossy(&bytes);
        // The client still receives a terminal frame — Finish or Error, never
        // a stream that just stops.
        assert!(
            text.contains("\"type\":\"Finish\"") || text.contains("\"type\":\"Error\""),
            "no terminal frame in /reply body: {text}"
        );

        // What the /reply CLIENT actually received, heartbeats dropped.
        let client: Vec<serde_json::Value> = text.lines().filter_map(sse_frame).collect();

        // What an observer of the same turn would render: the raw bus events it
        // saw, through the one shared mapper. Both consumers subscribed before
        // the turn could publish, so the two sequences are comparable frame for
        // frame — which is the whole point of §4.2 and the reason the mapper is
        // `pub(crate)` rather than duplicated per route.
        let mut observer_token_state = TokenState::default();
        let mut saw_started = false;
        let mut observed: Vec<serde_json::Value> = Vec::new();
        while let Ok(ev) = observer.try_recv() {
            if matches!(
                ev,
                biorouter::session_events::SessionBusEvent::TurnStarted { .. }
            ) {
                saw_started = true;
            }
            if let Some(frame) =
                crate::routes::session_events::map_bus_event(ev, &mut observer_token_state)
            {
                observed.push(serde_json::to_value(&frame).unwrap());
            }
        }
        assert!(saw_started, "the observer saw the turn's opening bracket");
        assert_eq!(
            client, observed,
            "the /reply client and a concurrent observer must receive the SAME \
             frames, not merely both receive something"
        );
        assert_eq!(
            frame_types(&client),
            vec!["Error"],
            "a provider-less turn is one terminal frame and nothing else: {client:?}"
        );
    }

    /// BR-62 must survive: a re-POST of the same turn_id is a duplicate, not a
    /// second turn.
    #[tokio::test]
    async fn reply_still_rejects_a_duplicate_turn_id_with_409() {
        use tower::ServiceExt;
        let state = AppState::new().await.unwrap();
        let temp = tempfile::TempDir::new().unwrap();
        let session = state
            .session_manager()
            .create_session(
                temp.path().to_path_buf(),
                "idem".to_string(),
                biorouter::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();
        // Hold the lock under a known key, then POST the same key.
        let _guard = state
            .try_begin_turn_idempotent(
                &session.id,
                tokio_util::sync::CancellationToken::new(),
                Some("client-turn-1".to_string()),
            )
            .unwrap();
        let body = serde_json::json!({
            "user_message": serde_json::to_value(
                biorouter::conversation::message::Message::user().with_text("hi")
            ).unwrap(),
            "session_id": session.id,
            "turn_id": "client-turn-1",
        });
        let response = routes(state.clone())
            .oneshot(
                axum::http::Request::post("/reply")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::CONFLICT);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["duplicate"], serde_json::Value::Bool(true));
    }

    /// **[F]** The new backpressure semantics, tested against the REAL broadcast
    /// channel instead of a constant function: a consumer that falls behind
    /// genuinely receives `Lagged` (so the branch is not dead code), and the
    /// branch's action is a storage resync frame, not a silent skip.
    #[tokio::test]
    async fn a_lagged_consumer_gets_a_storage_resync_frame() {
        use biorouter::session_events::{self, SessionBusEvent, BUS_CAPACITY};
        let state = AppState::new().await.unwrap();
        let temp = tempfile::TempDir::new().unwrap();
        let session = state
            .session_manager()
            .create_session(
                temp.path().to_path_buf(),
                "lagged".to_string(),
                biorouter::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();

        let mut rx = session_events::subscribe(&session.id);
        // Overrun the ring without reading. BUS_CAPACITY + 1 is the smallest
        // overflow; if this ever stops producing Lagged the resync branch is
        // unreachable and the test is the thing that says so.
        for i in 0..(BUS_CAPACITY + 1) {
            session_events::publish(
                &session.id,
                SessionBusEvent::TurnStarted {
                    turn_id: format!("turn-{i}"),
                },
            );
        }
        assert!(
            matches!(
                rx.recv().await,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_))
            ),
            "the ring must actually overflow, or /reply's resync branch is dead code"
        );

        assert_eq!(on_bus_lag_action(), BusLagAction::ResyncFromStorage);
        let frame = crate::routes::session_events::bus_lag_resync_frame(
            &state,
            &session.id,
            &TokenState::default(),
        )
        .await
        .expect("a resync frame is produced from storage");
        assert!(matches!(frame, MessageEvent::UpdateConversation { .. }));
    }

    /// The error envelope's four fields survive the round trip through the bus.
    /// `provider` is the scope under test on purpose: it is the one the
    /// desktop's rate-limit / retry / compaction recovery keys off, and the one
    /// a three-variant `wire_value` would have silently mismapped.
    #[test]
    fn turn_error_bus_event_maps_back_to_the_exact_error_frame() {
        use crate::routes::session_events::map_bus_event;
        let mut token_state = Default::default();
        let mapped = map_bus_event(
            biorouter::session_events::SessionBusEvent::TurnError {
                message: "rate limited".into(),
                code: "provider_failure".into(),
                scope: "provider".into(),
                retryable: true,
                provider_kind: Some("rate_limit".into()),
            },
            &mut token_state,
        )
        .expect("maps");
        let json = serde_json::to_value(&mapped).unwrap();
        assert_eq!(json["type"], "Error");
        assert_eq!(json["code"], "provider_failure");
        assert_eq!(json["retryable"], serde_json::Value::Bool(true));
        assert_eq!(json["provider_kind"], "rate_limit");
        // The scope must round-trip to the ENUM, not to a string field.
        assert_eq!(
            serde_json::to_value(TurnErrorScope::Provider).unwrap(),
            json["scope"]
        );
    }

    /// **[F]** Exactly ONE terminal frame per turn, asserted on the bytes the
    /// client actually receives rather than on the bus. A runner that published
    /// both the raw `AgentEvent::TurnAborted` and the classified `TurnError`
    /// would emit two `Error` frames for one abort (`map_bus_event` maps both),
    /// and the desktop would render the turn as failing twice.
    #[tokio::test]
    async fn the_reply_body_carries_exactly_one_terminal_frame() {
        use tower::ServiceExt;
        let state = AppState::new().await.unwrap();
        let temp = tempfile::TempDir::new().unwrap();
        let session = state
            .session_manager()
            .create_session(
                temp.path().to_path_buf(),
                "one-terminal".to_string(),
                biorouter::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();
        let body = serde_json::json!({
            "user_message": serde_json::to_value(
                biorouter::conversation::message::Message::user().with_text("hi")
            ).unwrap(),
            "session_id": session.id,
        });
        let response = routes(state.clone())
            .oneshot(
                axum::http::Request::post("/reply")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let text = String::from_utf8_lossy(&bytes);
        let terminals = text
            .lines()
            .filter_map(|line| line.strip_prefix("data: "))
            .filter_map(|json| serde_json::from_str::<serde_json::Value>(json).ok())
            .filter(|frame| frame["type"] == "Error" || frame["type"] == "Finish")
            .count();
        assert_eq!(
            terminals, 1,
            "expected exactly one terminal frame in: {text}"
        );
    }

    /// **[F]** Frame ORDER under coalescing: the `/reply` consumer (which merges
    /// same-id text deltas) and an observer (which does not) must agree on the
    /// ORDER of frame types, and the coalescer's flush MUST land before the
    /// terminal frame. Step 4 used to say "pay particular attention to order";
    /// this asserts it.
    ///
    /// **Driving this through a real turn cannot test it, which is why this test
    /// does not.** A provider-less turn produces exactly ONE frame: `Agent::reply`
    /// reaches `check_if_compaction_needed(self.provider().await?…)` and
    /// `provider()` is `Err(anyhow!("Provider not set"))`, so the runner
    /// publishes one `TurnError` and returns. And `BIOROUTER_SSE_COALESCE_MS` is
    /// unset in tests, so `sse_coalesce_window()` returns `Duration::ZERO` and
    /// `DeltaCoalescer::enabled()` is false and the flush placement is never
    /// executed. An end-to-end version of this test compares two one-element
    /// vectors derived from the same bus event and certifies nothing.
    ///
    /// `DeltaCoalescer` is private but in-file, and these tests live in
    /// `reply.rs`'s test module, so it is reachable.
    #[tokio::test]
    async fn coalesced_deltas_flush_before_the_terminal_frame() {
        use biorouter::agents::AgentEvent;
        use biorouter::conversation::message::Message;
        use biorouter::session_events::SessionBusEvent;

        let delta = |id: &str, text: &str| Message::assistant().with_id(id).with_text(text);
        let events = vec![
            SessionBusEvent::TurnStarted {
                turn_id: "t".into(),
            },
            SessionBusEvent::Agent(AgentEvent::Message(delta("a", "he"))),
            SessionBusEvent::Agent(AgentEvent::Message(delta("a", "llo"))),
            SessionBusEvent::TurnFinished {
                reason: "stop".into(),
                token_state: None,
            },
        ];

        // Observer: every event through `map_bus_event`, no coalescing.
        let mut ts = TokenState::default();
        let observer: Vec<String> = events
            .iter()
            .cloned()
            .filter_map(|e| crate::routes::session_events::map_bus_event(e, &mut ts))
            .filter_map(|f| {
                serde_json::to_value(&f).unwrap()["type"]
                    .as_str()
                    .map(str::to_string)
            })
            .collect();
        assert_eq!(observer, vec!["Message", "Message", "Finish"]);

        // /reply: a 50 ms window merges the two same-id deltas into ONE frame,
        // which must still precede the terminal frame.
        let mut coalescer = DeltaCoalescer::new(Duration::from_millis(50));
        let mut ts = TokenState::default();
        let mut client: Vec<String> = Vec::new();
        for event in events {
            match event {
                SessionBusEvent::Agent(AgentEvent::Message(m)) => {
                    for _ in coalescer.push(m) {
                        client.push("Message".to_string());
                    }
                }
                other => {
                    if coalescer.drain().is_some() {
                        client.push("Message".to_string());
                    }
                    if let Some(f) = crate::routes::session_events::map_bus_event(other, &mut ts) {
                        if let Some(kind) = serde_json::to_value(&f).unwrap()["type"].as_str() {
                            client.push(kind.to_string());
                        }
                    }
                }
            }
        }
        assert_eq!(
            client,
            vec!["Message", "Finish"],
            "the coalescer must flush before the terminal frame"
        );
    }

    /// **[F]** The SSE response must END when the runner dies without publishing
    /// a terminal event, or the client hangs forever after one error frame.
    ///
    /// Before this refactor the turn task owned `task_tx`; when it returned —
    /// including through a panic unwind — the sender dropped and the body ended.
    /// In the subscription shape the SSE task owns its own sender and only
    /// breaks on a terminal bus event, `RecvError::Closed`, or its cancel token.
    /// A panicking runner publishes no terminal event; `Closed` cannot be relied
    /// on either, because this consumer's own `Receiver` holds the channel open
    /// and `session_events::release_if_idle` only reclaims a sender once
    /// `receiver_count() == 0` — which by construction it is not, here; and
    /// `TurnGuard::drop` removes the `ActiveTurn` entry **without** tripping the
    /// token (`state.rs`). The supervisor is therefore the only thing that
    /// can release the loop.
    #[tokio::test]
    async fn the_supervisor_ends_the_stream_even_when_the_runner_panics() {
        let (tx, mut rx) = mpsc::channel::<String>(8);

        // A runner that panics, with an SSE task that would never end on its own.
        let cancel = CancellationToken::new();
        let runner = tokio::spawn(async { panic!("runner exploded") });
        let sse = tokio::spawn({
            let cancel = cancel.clone();
            async move { cancel.cancelled().await }
        });
        supervise_turn(runner, sse, tx.clone(), cancel.clone()).await;
        let frame = rx.try_recv().expect("the supervisor sends one error frame");
        assert!(
            frame.contains("\"code\":\"internal_error\""),
            "got: {frame}"
        );
        assert!(
            cancel.is_cancelled(),
            "the SSE loop must be released, or the response never ends"
        );

        // A runner that returns cleanly while its SSE task has ALREADY ended on
        // the terminal frame: no error frame, and no premature cancellation
        // that could truncate the tail of a healthy turn.
        let cancel = CancellationToken::new();
        let runner = tokio::spawn(async {});
        let sse = tokio::spawn(async {});
        supervise_turn(runner, sse, tx.clone(), cancel.clone()).await;
        assert!(
            rx.try_recv().is_err(),
            "a clean runner exit sends no error frame"
        );
        assert!(
            !cancel.is_cancelled(),
            "a stream that ended on its own must not be cancelled behind its back"
        );
    }

    /// **[F]** The coalescer must not swallow the last text delta when the
    /// terminal frame lands in the same window. `BIOROUTER_SSE_COALESCE_MS` is
    /// off by default, so this is the configuration most likely to be
    /// under-tested — and the plan itself names the flush placement as the
    /// likely cause of any order failure, which makes it the thing to pin.
    #[tokio::test]
    async fn a_terminal_frame_flushes_pending_coalesced_text_first() {
        use biorouter::conversation::message::Message;
        let (tx, mut rx) = mpsc::channel::<String>(8);
        let cancel = CancellationToken::new();
        let mut coalescer = DeltaCoalescer::new(Duration::from_millis(50));
        assert!(coalescer
            .push(Message::assistant().with_id("a").with_text("hel"))
            .is_empty());
        assert!(coalescer
            .push(Message::assistant().with_id("a").with_text("lo"))
            .is_empty());

        // Exactly what the new terminal branch does, in the order it does it.
        flush_coalesced(&mut coalescer, &tx, &cancel, &TokenState::default()).await;
        stream_event(
            MessageEvent::Finish {
                reason: "stop".to_string(),
                token_state: TokenState::default(),
            },
            &tx,
            &cancel,
        )
        .await;

        let first = rx.try_recv().expect("the buffered run is flushed first");
        assert!(
            first.contains("\"type\":\"Message\"") && first.contains("hello"),
            "got: {first}"
        );
        let second = rx.try_recv().expect("then the terminal frame");
        assert!(second.contains("\"type\":\"Finish\""), "got: {second}");
        assert!(rx.try_recv().is_err(), "and nothing after it");
    }

    // The four `[M]` tests below drive the REAL SSE loop — `stream_bus_to_client`,
    // the function `/reply` spawns — over the REAL session bus, and each one
    // fails if a specific branch of that loop is deleted or moved.
    //
    // They exist because a review found the eight tests above collectively green
    // against a broken hot path: `a_lagged_consumer_gets_a_storage_resync_frame`
    // overflows its own receiver and calls `bus_lag_resync_frame` itself,
    // `coalesced_deltas_flush_before_the_terminal_frame` and
    // `a_terminal_frame_flushes_pending_coalesced_text_first` perform the flush
    // themselves, and `the_supervisor_ends_the_stream_even_when_the_runner_panics`
    // hands `supervise_turn` a stand-in SSE task. Every one of them describes the
    // loop rather than running it, so deleting the loop's resync branch, its
    // flush, or its `cancel` response leaves all three green.
    //
    // Verified by mutation: narrowing the flush guard to `if terminal`, widening
    // it to `if true`, deleting the `Lagged` resync, dropping `if terminal
    // { break }`, removing the supervisor's `cancel.cancel()`, giving
    // `map_bus_event` a `MessagesPersisted => None`, and forwarding only terminal
    // frames each turn at least one test below red.

    /// Parse one SSE line into its frame, dropping heartbeats — they are timing
    /// noise, not part of any ordering contract.
    fn sse_frame(raw: &str) -> Option<serde_json::Value> {
        let value: serde_json::Value = serde_json::from_str(raw.strip_prefix("data: ")?.trim_end())
            .expect("the loop must only ever write valid JSON frames");
        (value["type"] != "Ping").then_some(value)
    }

    /// Every frame the loop wrote, in order, until it ended.
    ///
    /// The deadline is what turns "a mutation left the loop running forever"
    /// into a failed assertion instead of a hung test binary, and it is an
    /// OVERALL deadline rather than a per-`recv` one on purpose: the loop
    /// heartbeats every 500 ms, so a per-message timeout is fed indefinitely by
    /// `Ping`s and never fires. (Learned by mutating `if terminal { break }` and
    /// watching the suite hang.)
    async fn drain_frames(rx: &mut mpsc::Receiver<String>) -> Vec<serde_json::Value> {
        let mut frames = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while let Ok(Some(raw)) = tokio::time::timeout_at(deadline, rx.recv()).await {
            frames.extend(sse_frame(&raw));
        }
        frames
    }

    fn frame_types(frames: &[serde_json::Value]) -> Vec<String> {
        frames
            .iter()
            .map(|f| f["type"].as_str().unwrap_or("?").to_string())
            .collect()
    }

    /// **[M]** #59's ordering invariant, through the real loop with coalescing
    /// ON: a buffered run of text deltas is flushed BEFORE the
    /// `MessagesPersisted` frame that names the id those deltas belong to.
    ///
    /// This is the test the `!matches!(… TokenUsage)` guard exists for. Narrow
    /// that guard to `if terminal` — the mutation the comment beside it warns
    /// against, and the one every other test in this task survives — and the
    /// buffered run is only flushed when the terminal frame arrives, so the
    /// client is told the id of a row whose body it has not been sent yet.
    ///
    /// Coalescing must be ON or nothing is buffered and the flush is a no-op,
    /// which is why the window is a parameter of `stream_bus_to_client` rather
    /// than a read of `BIOROUTER_SSE_COALESCE_MS` (unset under test).
    #[tokio::test]
    async fn the_sse_loop_flushes_buffered_text_before_a_persisted_frame() {
        use biorouter::agents::{AgentEvent, PersistedMessage};
        use biorouter::session_events::{self, SessionBusEvent};

        let state = AppState::new().await.unwrap();
        let session_id = "br71-reply-loop-flush-order".to_string();
        let bus = session_events::subscribe(&session_id);
        let (tx, mut rx) = mpsc::channel::<String>(64);
        let cancel = CancellationToken::new();
        // A window far longer than the test's runtime: the only thing that can
        // emit the buffered run is an explicit flush, never the deadline.
        let loop_task = tokio::spawn(stream_bus_to_client(
            state.clone(),
            session_id.clone(),
            bus,
            tx,
            cancel.clone(),
            Duration::from_secs(30),
        ));

        for chunk in ["he", "llo"] {
            session_events::publish(
                &session_id,
                SessionBusEvent::Agent(AgentEvent::Message(
                    Message::assistant().with_id("m-1").with_text(chunk),
                )),
            );
        }
        session_events::publish(
            &session_id,
            SessionBusEvent::Agent(AgentEvent::MessagesPersisted(vec![PersistedMessage {
                id: "m-1".into(),
                user_visible: true,
            }])),
        );
        session_events::publish(
            &session_id,
            SessionBusEvent::TurnFinished {
                reason: "stop".into(),
                token_state: None,
            },
        );

        let frames = drain_frames(&mut rx).await;
        // Bounded: the loop must END on the terminal frame. An unbounded await
        // here turns "the `if terminal { break }` was removed" into a hung test
        // binary instead of a failed assertion.
        tokio::time::timeout(Duration::from_secs(10), loop_task)
            .await
            .expect("the terminal frame must end the SSE loop")
            .unwrap();
        assert_eq!(
            frame_types(&frames),
            vec!["Message", "MessagesPersisted", "Finish"],
            "the buffered deltas must reach the client BEFORE the frame naming \
             their stored id: {frames:?}"
        );
        assert!(
            frames[0].to_string().contains("hello"),
            "one coalesced run, not one frame per delta: {frames:?}"
        );
    }

    /// **[M]** …and the same guard must not be widened either: a `TokenUsage`
    /// event is pure bookkeeping and must NOT end a coalescing run.
    ///
    /// The provider interleaves token accounting with text deltas continuously,
    /// so flushing on it would emit one frame per delta and silently undo BR-53a
    /// for every turn — with `coalesce_tests` still green, because they never
    /// feed the coalescer a bus event.
    #[tokio::test]
    async fn token_usage_alone_does_not_break_a_coalescing_run() {
        use biorouter::agents::AgentEvent;
        use biorouter::session_events::{self, SessionBusEvent};

        let state = AppState::new().await.unwrap();
        let session_id = "br71-reply-loop-token-usage".to_string();
        let bus = session_events::subscribe(&session_id);
        let (tx, mut rx) = mpsc::channel::<String>(64);
        let cancel = CancellationToken::new();
        let loop_task = tokio::spawn(stream_bus_to_client(
            state.clone(),
            session_id.clone(),
            bus,
            tx,
            cancel.clone(),
            Duration::from_secs(30),
        ));

        session_events::publish(
            &session_id,
            SessionBusEvent::Agent(AgentEvent::Message(
                Message::assistant().with_id("m-1").with_text("he"),
            )),
        );
        session_events::publish(
            &session_id,
            SessionBusEvent::Agent(AgentEvent::TokenUsage(TokenState::default())),
        );
        session_events::publish(
            &session_id,
            SessionBusEvent::Agent(AgentEvent::Message(
                Message::assistant().with_id("m-1").with_text("llo"),
            )),
        );
        session_events::publish(
            &session_id,
            SessionBusEvent::TurnFinished {
                reason: "stop".into(),
                token_state: None,
            },
        );

        let frames = drain_frames(&mut rx).await;
        // Bounded: the loop must END on the terminal frame. An unbounded await
        // here turns "the `if terminal { break }` was removed" into a hung test
        // binary instead of a failed assertion.
        tokio::time::timeout(Duration::from_secs(10), loop_task)
            .await
            .expect("the terminal frame must end the SSE loop")
            .unwrap();
        assert_eq!(
            frame_types(&frames),
            vec!["Message", "Finish"],
            "token bookkeeping must not split a coalescing run: {frames:?}"
        );
        assert!(
            frames[0].to_string().contains("hello"),
            "the run must still merge across the TokenUsage: {frames:?}"
        );
    }

    /// **[M]** The `Lagged` branch of the real loop, exercised by the real
    /// broadcast channel: a consumer that has fallen off the ring is sent the
    /// whole conversation from STORAGE, not left silently short of frames.
    ///
    /// The ring is overrun before the loop's first `recv`, so `Lagged` is
    /// deterministic rather than raced. Delete the resync (or replace it with a
    /// bare `continue`) and no frame is ever written — the drain below times out
    /// and says so.
    #[tokio::test]
    async fn a_lagged_sse_loop_resyncs_the_client_from_storage() {
        use biorouter::session_events::{self, SessionBusEvent, BUS_CAPACITY};

        let state = AppState::new().await.unwrap();
        let temp = tempfile::TempDir::new().unwrap();
        let session = state
            .session_manager()
            .create_session(
                temp.path().to_path_buf(),
                "lagged-loop".to_string(),
                biorouter::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();
        state
            .session_manager()
            .add_message(
                &session.id,
                &Message::user().with_text("stored before the lag"),
            )
            .await
            .unwrap();

        let bus = session_events::subscribe(&session.id);
        // Overrun the ring while nothing is reading it, so the loop's very first
        // `recv` returns `Lagged`.
        for i in 0..(BUS_CAPACITY + 1) {
            session_events::publish(
                &session.id,
                SessionBusEvent::TurnStarted {
                    turn_id: format!("turn-{i}"),
                },
            );
        }

        let (tx, mut rx) = mpsc::channel::<String>(64);
        let cancel = CancellationToken::new();
        let loop_task = tokio::spawn(stream_bus_to_client(
            state.clone(),
            session.id.clone(),
            bus,
            tx,
            cancel.clone(),
            Duration::ZERO,
        ));

        let resync = tokio::time::timeout(Duration::from_secs(10), async {
            while let Some(raw) = rx.recv().await {
                if let Some(frame) = sse_frame(&raw) {
                    return Some(frame);
                }
            }
            None
        })
        .await
        .expect("a lagged consumer must be resynced, not silently skipped")
        .expect("the stream closed without sending anything");

        assert_eq!(
            resync["type"], "UpdateConversation",
            "the resync is a whole-conversation frame: {resync}"
        );
        assert!(
            resync.to_string().contains("stored before the lag"),
            "…read from storage, not synthesised empty: {resync}"
        );

        cancel.cancel();
        // Bounded for the same reason as above: a loop that ignores its cancel
        // token must fail this test, not hang it.
        tokio::time::timeout(Duration::from_secs(10), loop_task)
            .await
            .expect("cancellation must end the SSE loop")
            .unwrap();
    }

    /// **[M]** The supervisor's release path against the REAL loop.
    ///
    /// `the_supervisor_ends_the_stream_even_when_the_runner_panics` hands
    /// `supervise_turn` a stand-in task that does nothing but await the token,
    /// so it proves the token is tripped and nothing about the loop. This one
    /// composes the two production functions: a runner that returns without ever
    /// publishing a terminal event leaves the real loop parked on `bus.recv()`
    /// forever, and only the supervisor's `cancel` can end the response.
    ///
    /// Remove that `cancel.cancel()` and the drain below never completes.
    #[tokio::test]
    async fn the_supervisor_releases_a_real_sse_loop_that_never_got_a_terminal() {
        let state = AppState::new().await.unwrap();
        let session_id = "br71-reply-supervisor-releases-loop".to_string();
        let bus = biorouter::session_events::subscribe(&session_id);
        let (tx, mut rx) = mpsc::channel::<String>(64);
        let cancel = CancellationToken::new();
        let sse = tokio::spawn(stream_bus_to_client(
            state.clone(),
            session_id,
            bus,
            tx.clone(),
            cancel.clone(),
            Duration::ZERO,
        ));

        // A runner that ends without publishing a terminal event — the shape a
        // turn leaves behind when it is aborted, or when its own supervisor
        // swallowed the panic after the bus entry was gone.
        let runner = tokio::spawn(async {});
        supervise_turn(runner, sse, tx.clone(), cancel.clone()).await;
        assert!(
            cancel.is_cancelled(),
            "the supervisor must release a loop that never saw a terminal frame"
        );

        drop(tx);
        let ended = tokio::time::timeout(Duration::from_secs(10), async {
            while rx.recv().await.is_some() {}
        })
        .await;
        assert!(
            ended.is_ok(),
            "the SSE response must end once the supervisor releases the loop"
        );
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
            // #69: acceptance is the *agent loop's* to give, so the session's
            // agent has to be in the accepting state a running loop puts it in.
            // The server's turn lock alone is no longer enough.
            let agent = state
                .get_agent("steering-session".to_string())
                .await
                .unwrap();
            agent.open_for_turn(biorouter::agents::TurnId::new("agent-turn-test"));

            let app = routes(Arc::clone(&state));
            let response = app
                .oneshot(interrupt_request("steering-session", "actually, use R"))
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::ACCEPTED);
            assert_eq!(
                json_body(response).await["turn_id"],
                serde_json::json!("agent-turn-test"),
                "a 202 must name the turn that actually took the message"
            );

            assert!(
                agent.has_soft_interrupts(),
                "the steer must be queued on the session's agent"
            );
        }

        /// #69: the turn lock is still held — the reply task has not unwound yet —
        /// but the loop has performed its final drain and committed to exiting.
        /// The old route read only the lock and returned 202, and the text then
        /// surfaced in an unrelated later turn.
        #[tokio::test(flavor = "multi_thread")]
        async fn test_interrupt_refuses_once_the_agent_loop_has_closed() {
            let state = AppState::new().await.unwrap();
            let _guard = begin_turn(&state, "closing-session").expect("turn lock acquired");
            let agent = state
                .get_agent("closing-session".to_string())
                .await
                .unwrap();
            agent.open_for_turn(biorouter::agents::TurnId::new("agent-turn-closing"));
            // What the loop does at its exit when nothing is queued.
            assert!(matches!(
                agent.close_and_drain(),
                biorouter::agents::Drained::Empty
            ));

            let app = routes(Arc::clone(&state));
            let response = app
                .oneshot(interrupt_request("closing-session", "too late"))
                .await
                .unwrap();

            assert_eq!(
                response.status(),
                StatusCode::CONFLICT,
                "a steer that misses its turn must be refused, not deferred"
            );
            assert!(
                !agent.has_soft_interrupts(),
                "the refused steer must not be sitting on the agent for a later turn"
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
