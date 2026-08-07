//! BR-71 §4.2: the read-only observer stream. Lets a subagent tab, a second
//! window, or a parent-watching-child render a turn none of them started.
//! Frames reuse the `/reply` wire enum (`MessageEvent`) so the generated TS
//! client and `chatStreamStore.tsx` parse them unchanged.
//!
//! `map_bus_event` below is `pub(crate)` because after Task 8 it has TWO
//! callers: this route and `/reply` itself. That is what makes "an observer
//! sees exactly what the client sees" structural rather than a property two
//! hand-written loops have to keep agreeing on.
//!
//! **The inner `match ev` is EXHAUSTIVE and must stay that way — never add a
//! `_ => None` arm.** `AgentEvent` has eight variants today
//! (`agents/agent.rs`); an exhaustive match is what makes the NINTH fail the
//! build instead of silently vanishing from every wire. The repo makes the same
//! choice, for the same reason, at `MessageContent::is_pin_eligible`
//! (`conversation/message.rs`). If a future variant genuinely has no wire form,
//! give it a named arm returning `None` with the reason written next to it.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use biorouter::agents::AgentEvent;
use biorouter::session_events::{self, SessionBusEvent};
use tokio::sync::mpsc;

use crate::routes::reply::{get_token_state, MessageEvent, SseResponse, TurnErrorScope};
use crate::state::AppState;

/// Map one bus event to a wire frame. `None` means "nothing to send" (token
/// updates fold into the cached state that stamps subsequent frames, exactly as
/// the `/reply` loop does — BR-52).
pub(crate) fn map_bus_event(
    event: SessionBusEvent,
    token_state: &mut biorouter::conversation::message::TokenState,
) -> Option<MessageEvent> {
    match event {
        SessionBusEvent::TurnStarted { .. } => None,
        SessionBusEvent::TurnFinished {
            reason,
            token_state: final_state,
        } => {
            // BR-52: prefer the runner's authoritative end-of-turn read over the
            // running total this consumer accumulated from TokenUsage events.
            if let Some(final_state) = final_state {
                *token_state = final_state;
            }
            Some(MessageEvent::Finish {
                reason,
                token_state: token_state.clone(),
            })
        }
        // Reconciliation #9: a terminal error carries the four fields
        // `AgentEvent::TurnAborted` cannot express, so `/reply`'s envelope
        // survives the round trip through the bus byte-for-byte.
        SessionBusEvent::TurnError {
            message,
            code,
            scope,
            retryable,
            provider_kind,
        } => Some(MessageEvent::Error {
            error: message,
            code,
            scope: TurnErrorScope::from_wire_value(&scope),
            retryable,
            provider_kind,
        }),
        SessionBusEvent::Agent(ev) => match ev {
            AgentEvent::Message(message) => Some(MessageEvent::Message {
                message,
                token_state: token_state.clone(),
            }),
            AgentEvent::TokenUsage(new_state) => {
                *token_state = new_state;
                None
            }
            AgentEvent::HistoryReplaced(conversation) => Some(MessageEvent::UpdateConversation {
                conversation,
                token_state: token_state.clone(),
            }),
            AgentEvent::ModelChange { model, mode } => {
                Some(MessageEvent::ModelChange { model, mode })
            }
            AgentEvent::McpNotification((request_id, message)) => {
                Some(MessageEvent::Notification {
                    request_id,
                    message,
                })
            }
            AgentEvent::ToolCallPending(p) => Some(MessageEvent::ToolCallPending {
                id: p.id,
                name: p.name,
                partial_args: p.partial_args,
            }),
            // Reconciliation #22 / #59: the accounting frame naming the ids the
            // turn's rows were actually stored under.
            //
            // A pass-through — `MessageEvent` already has the
            // identically-shaped variant carrying the same
            // `Vec<PersistedMessage>` — but it is NOT optional, and it is NOT
            // `None`. Without it a client that watches a whole turn ends it
            // knowing none of the ids the store holds, so `expectedMessageIds`
            // on `POST /sessions/{id}/edit_message` is unsatisfiable and the
            // in-place edit it guards answers 409 on an untouched session.
            //
            // ORDERING (`agents/agent.rs`, and its `messages_then_persisted`
            // test): no `MessagesPersisted` may precede a `Message` frame
            // carrying one of the ids it publishes. This mapper preserves
            // whatever order it is fed — it is the CONSUMERS that must flush
            // before forwarding this frame (this route sends immediately, so it
            // has nothing buffered; Task 8's `/reply` loop holds a
            // `DeltaCoalescer` and flushes first — see its `!matches!(…
            // TokenUsage)` guard, which is load-bearing for exactly this).
            AgentEvent::MessagesPersisted(messages) => {
                Some(MessageEvent::MessagesPersisted { messages })
            }
            // FALLBACK ONLY. The turn runner never publishes a raw
            // `TurnAborted` — it classifies it and publishes `TurnError`
            // instead, precisely so no consumer renders two terminal Error
            // frames for one abort (Task 6). This arm exists for publishers
            // that only tee raw agent events onto the bus (Task 34's subagent
            // runs), and it reuses the SAME classifier so those frames carry the
            // provider envelope too.
            AgentEvent::TurnAborted { code, message } => {
                let (scope, retryable, provider_kind) =
                    crate::workspace::turn::classify_abort(&code);
                Some(MessageEvent::Error {
                    error: message,
                    code: code.wire_code().to_string(),
                    scope,
                    retryable,
                    provider_kind,
                })
            }
        },
    }
}

/// What a lagged consumer sends instead of silently skipping frames (§8.4).
/// `pub(crate)` because BOTH consumers use it — this route and, after Task 8,
/// `/reply` — for the same reason `map_bus_event` is shared: one resync
/// behaviour, not two that have to keep agreeing.
pub(crate) async fn bus_lag_resync_frame(
    state: &AppState,
    session_id: &str,
    token_state: &biorouter::conversation::message::TokenState,
) -> Option<MessageEvent> {
    let fresh = state
        .session_manager()
        .get_session(session_id, true)
        .await
        .ok()?;
    Some(MessageEvent::UpdateConversation {
        conversation: fresh.conversation.unwrap_or_default(),
        token_state: token_state.clone(),
    })
}

#[utoipa::path(
    get,
    path = "/sessions/{session_id}/events",
    // EXPLICIT tag, not utoipa's default module path: Task 42b's CLI-parity gate
    // selects the workspace-control route surface by this tag.
    tag = "workspace",
    params(("session_id" = String, Path, description = "Session to observe")),
    responses(
        (status = 200, description = "Read-only observer stream of the session's live events",
         body = MessageEvent, content_type = "text/event-stream"),
        (status = 403, description = "Out of reach - a private or unreadable session named without the user-action proof"),
        (status = 404, description = "No such session"),
        (status = 401, description = "Unauthorized - invalid secret key")
    )
)]
pub async fn observe_session_events(
    Path(session_id): Path<String>,
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> axum::response::Response {
    // Issue #56. This route is `GET /sessions/{session_id}` plus a live tail: the
    // first frame it sends is the whole stored conversation. That sibling has
    // been gated by `session_reach` since Task 58 and this one was not, which is
    // the "unguarded sibling" defect exactly. The gate goes FIRST — ahead of the
    // bus subscription as well as the store read, because a subscription that
    // outlives a refusal is a side channel of its own.
    if let Err(refusal) =
        crate::routes::session_reach::session_reach(state.session_manager(), &session_id, &headers)
            .await
    {
        return refusal.into_response();
    }

    // Subscribe BEFORE the snapshot so no event falls in the gap between them.
    let mut rx = session_events::subscribe(&session_id);

    let session = match state.session_manager().get_session(&session_id, true).await {
        Ok(s) => s,
        Err(_) => return axum::http::StatusCode::NOT_FOUND.into_response(),
    };

    let mut token_state = get_token_state(state.session_manager(), &session_id).await;
    let (tx, rx_out) = mpsc::channel::<String>(64);

    let manager_session_id = session_id.clone();
    let state_for_task = state.clone();
    tokio::spawn(async move {
        let send = |tx: &mpsc::Sender<String>, ev: &MessageEvent| {
            let frame = format!(
                "data: {}\n\n",
                serde_json::to_string(ev).unwrap_or_default()
            );
            let tx = tx.clone();
            async move { tx.send(frame).await.is_ok() }
        };

        // Join-mid-turn snapshot: the observer starts from the full stored
        // conversation, then applies live events (BR-71 §4.2).
        let snapshot = MessageEvent::UpdateConversation {
            conversation: session.conversation.unwrap_or_default(),
            token_state: token_state.clone(),
        };
        if !send(&tx, &snapshot).await {
            return;
        }

        let mut heartbeat = tokio::time::interval(Duration::from_millis(500));
        loop {
            tokio::select! {
                _ = heartbeat.tick() => {
                    if !send(&tx, &MessageEvent::Ping).await { return; }
                }
                received = rx.recv() => match received {
                    Ok(event) => {
                        if let Some(mapped) = map_bus_event(event, &mut token_state) {
                            if !send(&tx, &mapped).await { return; }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        // §8.4: resync from storage instead of dropping frames
                        // silently. Shared with /reply (Task 8).
                        if let Some(resync) = bus_lag_resync_frame(
                            &state_for_task,
                            &manager_session_id,
                            &token_state,
                        )
                        .await
                        {
                            if !send(&tx, &resync).await { return; }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                },
            }
        }
    });

    SseResponse::from_rx(rx_out).into_response()
}

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/sessions/{session_id}/events", get(observe_session_events))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use biorouter::agents::AgentEvent;
    use biorouter::conversation::message::Message;
    use biorouter::session_events::SessionBusEvent;

    #[test]
    fn maps_lifecycle_and_messages_and_swallows_token_updates() {
        let mut token_state = Default::default();

        assert!(map_bus_event(
            SessionBusEvent::TurnStarted {
                turn_id: "turn-1".into()
            },
            &mut token_state
        )
        .is_none());

        let mapped = map_bus_event(
            SessionBusEvent::Agent(AgentEvent::Message(Message::user().with_text("hello"))),
            &mut token_state,
        )
        .expect("message maps");
        assert!(serde_json::to_string(&mapped)
            .unwrap()
            .contains("\"type\":\"Message\""));

        let fin = map_bus_event(
            SessionBusEvent::TurnFinished {
                reason: "stop".into(),
                token_state: None,
            },
            &mut token_state,
        )
        .expect("finish maps");
        assert!(serde_json::to_string(&fin)
            .unwrap()
            .contains("\"type\":\"Finish\""));
    }

    /// Reconciliation #22 / #59: `MessagesPersisted` reaches the wire through
    /// the mapper, and it maps to a frame — NOT to `None`.
    ///
    /// This is the one arm whose *plausible wrong answer compiles*. Dropping it
    /// (or reaching for a `_ => None` wildcard to silence `E0004`) is invisible
    /// to every other test in this plan: the desktop store deliberately does not
    /// consume the frame yet, and `reply.rs`'s own
    /// `persisted_message_ids_reach_the_wire_with_their_visibility` tests the
    /// *enum's serialization*, not the handler — so it stays green while no
    /// handler emits the frame at all. After Task 8 that silence means no
    /// `/reply` client ever learns a stored id again, `expectedMessageIds`
    /// becomes unsatisfiable, and `POST /sessions/{id}/edit_message` answers 409
    /// on sessions nobody touched: exactly the regression `0312dff4` +
    /// `936f5a33` were written to close.
    #[test]
    fn a_persisted_batch_maps_to_a_wire_frame_and_never_to_none() {
        use biorouter::agents::PersistedMessage;
        let mut token_state = Default::default();

        let mapped = map_bus_event(
            SessionBusEvent::Agent(AgentEvent::MessagesPersisted(vec![
                PersistedMessage {
                    id: "m-1".into(),
                    user_visible: true,
                },
                PersistedMessage {
                    id: "m-2".into(),
                    user_visible: false,
                },
            ])),
            &mut token_state,
        )
        .expect("MessagesPersisted maps to a frame, not to None");

        let json = serde_json::to_value(&mapped).unwrap();
        assert_eq!(json["type"], "MessagesPersisted");
        assert_eq!(json["messages"][0]["id"], "m-1");
        // `userVisible` is camelCase on the wire and BOTH values survive: a
        // client must be able to NAME a hidden row without drawing it.
        assert_eq!(json["messages"][0]["userVisible"], true);
        assert_eq!(json["messages"][1]["userVisible"], false);
    }

    /// Reconciliation #9: every `TurnErrorScope` variant survives the string
    /// round trip through the bus, and an unknown one degrades instead of
    /// panicking. All FOUR variants — `Provider` is the one the desktop's
    /// retry/rate-limit recovery keys off.
    #[test]
    fn turn_error_scopes_round_trip_through_their_wire_values() {
        use crate::routes::reply::TurnErrorScope;
        for scope in [
            TurnErrorScope::Provider,
            TurnErrorScope::Session,
            TurnErrorScope::Inference,
            TurnErrorScope::Internal,
        ] {
            assert_eq!(TurnErrorScope::from_wire_value(scope.wire_value()), scope);
            // …and the wire value is what serde emits, so the enum and the bus
            // can never drift apart.
            assert_eq!(
                serde_json::to_value(&scope).unwrap(),
                serde_json::Value::String(scope.wire_value().to_string())
            );
        }
        assert_eq!(
            TurnErrorScope::from_wire_value("a_scope_from_a_newer_runner"),
            TurnErrorScope::Internal
        );
    }

    #[tokio::test]
    async fn observer_gets_snapshot_then_live_events() {
        use tower::ServiceExt;
        let state = crate::state::AppState::new().await.unwrap();
        let temp = tempfile::TempDir::new().unwrap();
        let session = state
            .session_manager()
            .create_session(
                temp.path().to_path_buf(),
                "obs".to_string(),
                biorouter::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();

        let app = routes(state.clone());
        let response = app
            .oneshot(
                axum::http::Request::get(format!("/sessions/{}/events", session.id))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);

        // Publish one live event, then read the body: it must contain the
        // snapshot (UpdateConversation) and then the Finish frame.
        session_events::publish(
            &session.id,
            SessionBusEvent::TurnFinished {
                reason: "stop".into(),
                token_state: None,
            },
        );
        let bytes =
            tokio::time::timeout(Duration::from_secs(5), collect_prefix(response.into_body()))
                .await
                .expect("body bytes in time");
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("\"type\":\"UpdateConversation\""));
        assert!(text.contains("\"type\":\"Finish\""));
    }

    /// Read chunks until both expected markers have arrived, then stop.
    ///
    /// `axum::body::to_bytes` — which every other body-reading test in this
    /// crate uses (`routes/session.rs`, `routes/reply.rs`) — CANNOT be used
    /// here: this body is an observer stream that never ends, so `to_bytes`
    /// would hang until the test's 5 s timeout every time.
    ///
    /// Nor can it use `http_body_util::BodyExt::frame()`: `http-body-util` is
    /// not a dependency of `biorouter-server` at any level (the only manifest in
    /// the workspace that has it is `crates/biorouter-headless/Cargo.toml`),
    /// and Rust does not let a crate import a transitive dependency — that is
    /// E0432, which fails the WHOLE crate's test build, so every test in this
    /// module would stop running. `Body::into_data_stream()` (axum-core 0.5) is
    /// the same thing over `futures::StreamExt`, which IS a direct dependency
    /// (`crates/biorouter-server/Cargo.toml`).
    async fn collect_prefix(body: axum::body::Body) -> Vec<u8> {
        use futures::StreamExt;
        let mut stream = body.into_data_stream();
        let mut collected = Vec::new();
        while let Some(Ok(chunk)) = stream.next().await {
            collected.extend_from_slice(&chunk);
            let text = String::from_utf8_lossy(&collected);
            if text.contains("UpdateConversation") && text.contains("Finish") {
                break;
            }
        }
        collected
    }

    /// A watch on a session that does not exist is refused, not an empty stream
    /// — and since this route joined the `session_reach` list it is refused as
    /// **403, not 404**, for an unproven caller.
    ///
    /// ⚠ **That change is the control working, not a regression.** `Unreadable`
    /// is refused identically to `Private` by design (`routes::session_reach`,
    /// "the blast radius is wider than private chats"): a 404 here would answer
    /// "no such chat" for ids that do not exist and 403 for ids that do, which is
    /// precisely the per-id existence oracle the refusal is worded to close. A
    /// caller holding the user-action proof still gets the honest 404, and
    /// `session_reach`'s own tests hold that half.
    ///
    /// Note the ordering this is NOT allowed to change: the gate runs before
    /// `observe_session_events` subscribes to the bus, and the subscription still
    /// precedes `get_session` so no event falls between snapshot and stream.
    #[tokio::test]
    async fn observing_an_unknown_session_is_refused() {
        use tower::ServiceExt;
        let state = crate::state::AppState::new().await.unwrap();
        let response = routes(state.clone())
            .oneshot(
                axum::http::Request::get("/sessions/does-not-exist/events")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::FORBIDDEN);
    }
}
