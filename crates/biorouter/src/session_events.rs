//! Per-session event broadcast (BR-71 §4.2).
//!
//! Before BR-71, agent events flowed only inside the `POST /reply` response
//! that started the turn — nothing could *observe* a session it didn't start.
//! This bus is the missing publisher, and after Task 8 it is the ONLY path
//! turn events take: the single turn runner
//! (`biorouter-server/src/workspace/turn.rs`) publishes here, and both
//! consumers — the `/reply` SSE response and the read-only observer route
//! `GET /sessions/{id}/events` — subscribe. Lives in the `biorouter` crate,
//! not the server, because subagent turns publish from `subagent_handler.rs`,
//! which cannot depend on `biorouter-server`. The server maps these to its
//! `MessageEvent` wire enum in exactly one place
//! (`routes::session_events::map_bus_event`), so every consumer sees
//! byte-identical frames.
//!
//! **The `Agent` variant's payload is a closed set of EIGHT, and downstream
//! depends on that.** `AgentEvent` (`crate::agents::AgentEvent`,
//! `agents/agent.rs:612-679`) is `Message`, `McpNotification`, `ModelChange`,
//! `HistoryReplaced`, `ToolCallPending`, `TokenUsage`, `TurnAborted` and —
//! since #59 — `MessagesPersisted(Vec<PersistedMessage>)`. The server's
//! `map_bus_event` matches all eight with **no wildcard arm**, so adding a
//! ninth is a deliberate, compiler-enforced conversation about how it reaches
//! the wire, rather than a variant that silently vanishes at the adapter. Do
//! not "simplify" that match, and do not add a `_` here either.
//!
//! **Senders are reclaimed, not retained for the life of the process.** A
//! `tokio::sync::broadcast::Sender` is NOT cheap to hold: `broadcast::channel`
//! allocates the entire ring up front, before any receiver exists —
//! `Sender::new_with_receiver_count` does
//! `let mut buffer = Vec::with_capacity(capacity); for i in 0..capacity {
//! buffer.push(Mutex::new(Slot { … val: None })) }`. With `BUS_CAPACITY = 1024`
//! and a `SessionBusEvent` slot (its `Agent` variant wraps `Message` /
//! `Conversation` / `McpNotification`), that is on the order of 10^5 bytes per
//! session id, allocated the moment a session first publishes or is watched. A
//! desktop daemon runs for days, and after Task 8 EVERY turn of EVERY session
//! publishes here, so "insert and never remove" is an unbounded leak measured
//! in hundreds of MB for a few thousand distinct sessions. Hence
//! [`release_if_idle`], which the turn runner calls at the end of every turn.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use tokio::sync::broadcast;

use crate::agents::AgentEvent;
use crate::conversation::message::TokenState;

/// Ring capacity per session. Observers that fall further behind see
/// `RecvError::Lagged` and must resync from storage (both consumers re-send an
/// `UpdateConversation` snapshot).
///
/// 1024, not 256: after Task 8 the interactive `/reply` client is a bus
/// consumer on the hot path, and a long tool-heavy turn streaming token deltas
/// through a briefly-stalled renderer must not trip a resync for a hiccup.
///
/// It IS real memory, not "bounded work": the ring is allocated in full at
/// channel creation, with no receivers. That cost is acceptable only because
/// [`release_if_idle`] frees it when the session goes quiet — a per-session
/// ring that lives for the process lifetime would not be.
pub const BUS_CAPACITY: usize = 1024;

/// What a turn publishes. `TurnStarted` / (`TurnError` |`TurnFinished`) bracket
/// every turn so consumers can render lifecycle without parsing message
/// content, and so `workspace_watch` and `wait:"final_message"` have an
/// unambiguous completion signal.
#[derive(Clone, Debug)]
pub enum SessionBusEvent {
    TurnStarted {
        turn_id: String,
    },
    Agent(AgentEvent),
    /// A terminal error, carried with enough fidelity to reproduce `/reply`'s
    /// `MessageEvent::Error` envelope exactly (BR-71 reconciliation #9).
    /// Strings, not the server's `TurnErrorScope` enum, because this crate
    /// cannot depend on `biorouter-server`; the server maps them back.
    TurnError {
        message: String,
        code: String,
        /// `"provider" | "session" | "inference" | "internal"` — the wire values
        /// of `biorouter_server::routes::reply::TurnErrorScope`, which has
        /// exactly those FOUR variants. `provider` is the one that matters
        /// most: it is what the desktop keys its rate-limit / retry /
        /// compaction recovery off, together with `retryable` and
        /// `provider_kind`.
        scope: String,
        retryable: bool,
        provider_kind: Option<String>,
    },
    /// Normal closure. `token_state` is the authoritative end-of-turn read
    /// (BR-52) when the runner performed one; `None` for brackets published
    /// without a store read (subagent runs headless of the daemon).
    TurnFinished {
        reason: String,
        token_state: Option<TokenState>,
    },
}

static BUS: LazyLock<Mutex<HashMap<String, broadcast::Sender<SessionBusEvent>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn sender_for(session_id: &str) -> broadcast::Sender<SessionBusEvent> {
    let mut map = BUS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    map.entry(session_id.to_string())
        .or_insert_with(|| broadcast::channel(BUS_CAPACITY).0)
        .clone()
}

/// Subscribe to a session's live events. Safe for any session id, including one
/// with no turn running — the receiver simply waits.
pub fn subscribe(session_id: &str) -> broadcast::Receiver<SessionBusEvent> {
    sender_for(session_id).subscribe()
}

/// Publish, best-effort. A send with no receivers is a no-op, never an error —
/// publishing must cost nothing when nobody is watching.
pub fn publish(session_id: &str, event: SessionBusEvent) {
    let _ = sender_for(session_id).send(event);
}

/// Drop a session's sender — and its 1024-slot ring — once nothing is listening.
///
/// Called by the turn runner AFTER the terminal event has been published and
/// the consumers have had it (see `run_turn`'s exit path, Task 6). A live
/// observer keeps `receiver_count() > 0` and the entry survives; when the last
/// one goes, the next idle session's turn reclaims it. Re-creating the entry
/// later is one allocation, which is exactly what happens for a session's first
/// turn anyway.
///
/// NOT idempotency-sensitive: `subscribe` re-inserts on demand, and a receiver
/// created from a sender that has since been removed from the map keeps working
/// (it holds its own `Arc` to the shared state) — it simply stops seeing events
/// published through a *new* sender. That is why this only fires at `0`.
pub fn release_if_idle(session_id: &str) {
    let mut map = BUS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(sender) = map.get(session_id) {
        if sender.receiver_count() == 0 {
            map.remove(session_id);
        }
    }
}

/// How many live observers a session currently has (introspection/tests).
pub fn observer_count(session_id: &str) -> usize {
    sender_for(session_id).receiver_count()
}

/// Whether a session currently holds a ring (tests only).
///
/// Deliberately a per-key predicate and **not** a `tracked_session_count()`.
/// `BUS` is process-global and libtest runs this module's tests as parallel
/// threads on one process: the three tests above insert `bus-t1`..`bus-t4` and
/// never release them, so any `count == before + 1` assertion can observe
/// `before + 2` depending on interleaving and fail for reasons that have
/// nothing to do with the ring under test. A key the leak test owns outright
/// cannot race, and it asserts the actual property (this entry was reclaimed)
/// instead of a proxy for it.
#[cfg(test)]
pub(crate) fn is_tracked(session_id: &str) -> bool {
    BUS.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .contains_key(session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn two_observers_both_receive_and_publish_without_observers_is_ok() {
        publish(
            "bus-t1",
            SessionBusEvent::TurnFinished {
                reason: "stop".into(),
                token_state: None,
            },
        ); // no panic

        let mut a = subscribe("bus-t2");
        let mut b = subscribe("bus-t2");
        publish(
            "bus-t2",
            SessionBusEvent::TurnStarted {
                turn_id: "turn-9".into(),
            },
        );
        assert!(matches!(
            a.recv().await.unwrap(),
            SessionBusEvent::TurnStarted { .. }
        ));
        assert!(matches!(
            b.recv().await.unwrap(),
            SessionBusEvent::TurnStarted { .. }
        ));
    }

    #[tokio::test]
    async fn slow_observer_lags_rather_than_blocking() {
        let mut rx = subscribe("bus-t3");
        for i in 0..(BUS_CAPACITY + 8) {
            publish(
                "bus-t3",
                SessionBusEvent::TurnFinished {
                    reason: format!("r{i}"),
                    token_state: None,
                },
            );
        }
        // The first recv reports the overflow instead of stalling the publisher.
        assert!(matches!(
            rx.recv().await,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_))
        ));
    }

    /// Task 8 depends on this: `/reply`'s Error frame carries four fields beyond
    /// the message, and `AgentEvent::TurnAborted` has none of them.
    ///
    /// Published and read back **through the real channel**, so the assertion
    /// covers the bus rather than a struct literal. A literal round trip
    /// (construct, destructure, compare) cannot fail at runtime — it is a
    /// type-level pin dressed as a behavioural test, and this is the area where
    /// false assurance is most expensive.
    ///
    /// The string→`TurnErrorScope` half of the contract is asserted in Task 7's
    /// `turn_error_scopes_round_trip_through_their_wire_values`;
    /// `TurnErrorScope` lives in `biorouter-server`, which this crate cannot
    /// depend on.
    #[tokio::test]
    async fn turn_error_carries_the_full_wire_envelope_across_the_bus() {
        let mut rx = subscribe("bus-t4");
        publish(
            "bus-t4",
            SessionBusEvent::TurnError {
                message: "provider refused".into(),
                code: "provider_forbidden".into(),
                scope: "inference".into(),
                retryable: false,
                provider_kind: Some("anthropic".into()),
            },
        );
        let SessionBusEvent::TurnError {
            message,
            code,
            scope,
            retryable,
            provider_kind,
        } = rx.recv().await.unwrap()
        else {
            panic!("variant");
        };
        assert_eq!(message, "provider refused");
        assert_eq!(code, "provider_forbidden");
        assert_eq!(scope, "inference");
        assert!(!retryable);
        assert_eq!(provider_kind.as_deref(), Some("anthropic"));
    }

    /// A finished turn with no observers must not leave a 1024-slot ring
    /// behind. `broadcast::channel` allocates the whole ring at creation, so an
    /// insert-and-never-remove map is a real leak on a daemon that runs for days
    /// and publishes for every turn of every session.
    ///
    /// Asserts on ITS OWN KEY, never on a map size. `BUS` is process-global and
    /// the three tests above leave `bus-t1`..`bus-t4` in it forever; libtest
    /// runs them as parallel threads, so a `count == before + 1` assertion is a
    /// race against its own module. `leak-check` is touched by this test alone.
    #[tokio::test]
    async fn an_idle_session_releases_its_ring() {
        assert!(
            !is_tracked("leak-check"),
            "precondition: nothing else uses this key"
        );
        publish(
            "leak-check",
            SessionBusEvent::TurnStarted {
                turn_id: "t".into(),
            },
        );
        assert!(is_tracked("leak-check"), "publishing creates the ring");

        // A live observer pins it …
        let rx = subscribe("leak-check");
        release_if_idle("leak-check");
        assert!(is_tracked("leak-check"), "an observer keeps the ring");

        // … and losing the last one releases it.
        drop(rx);
        release_if_idle("leak-check");
        assert!(
            !is_tracked("leak-check"),
            "the last observer leaving reclaims the ring"
        );
    }
}
