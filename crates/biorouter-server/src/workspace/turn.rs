//! BR-71 §4.2: THE turn runner.
//!
//! Design §4.2 asks for a turn that can run "server-side with no attached HTTP
//! response", and for `/reply` to become "detached turn + a subscription".
//! This module is the first half; `routes/reply.rs` (Task 8) is the second.
//! Everything a turn owns lives here — the per-session turn lock, the
//! interactive-turn guard, `get_agent`, `agent.reply(...)`, consuming the
//! `AgentEvent` stream, tool telemetry, terminal-reason classification, the
//! best-effort session rename, session-completion metrics, and the
//! authoritative end-of-turn token read (BR-52). Everything a *request* owns
//! (SSE framing, delta coalescing, heartbeats, the JoinError envelope) stays in
//! the handler.
//!
//! Every event the turn produces is published to the session bus, so a client
//! that started the turn and an observer that did not see byte-identical
//! frames. That equality is the design's actual goal, and it is now structural
//! rather than maintained by hand across two loops.

use std::sync::Arc;

use biorouter::agents::{AgentEvent, SessionConfig};
use biorouter::conversation::message::Message;
use biorouter::conversation::Conversation;
use biorouter::session_events::{self, SessionBusEvent};
use futures::StreamExt;
use tokio_util::sync::CancellationToken;

use crate::routes::reply::{get_token_state, track_tool_telemetry, TurnErrorScope};
use crate::state::AppState;

#[derive(Debug, thiserror::Error)]
pub enum TurnStartError {
    #[error("a turn is already in flight for this session (running turn {running_turn_id})")]
    TurnInFlight {
        running_turn_id: String,
        /// BR-62: true when the caller re-sent the SAME idempotency key — "your
        /// turn is still running", not "someone else is in the way".
        duplicate: bool,
    },
}

/// Everything a turn needs that is not the session id or the user's message.
/// A struct rather than eight positional arguments because `/reply` supplies
/// four of these and an injected turn supplies none.
#[derive(Debug, Default)]
pub struct TurnExtras {
    /// BR-62 turn idempotency key (the client's `turn_id`). `None` for injected
    /// turns: two keyless turns are two turns, which is correct there.
    pub idempotency_key: Option<String>,
    /// The conversation this turn should START from — **already stored**.
    ///
    /// `Option<Conversation>`, NOT `Option<Vec<Message>>`. It is a *seed*, not a
    /// write to perform, and the type says so: it is the `Ok` value of
    /// `apply_client_writeback` (`reply.rs`), i.e. what the store holds
    /// after `/reply` has already validated and applied the client's copy.
    /// `None` means "read the session" — every caller except `/reply` passes
    /// `None`, because none of them has a client prefix.
    ///
    /// ⚠ **The whole-history rewrite does NOT move into `run_turn`.** An earlier
    /// revision of this plan said it did, quoting a `replace_conversation` call
    /// in the `/reply` handler. That call no longer exists: `306552fd
    /// fix(server): refuse a stale conversation_so_far instead of storing it`
    /// (#51 W5) replaced it with `apply_client_writeback`, a **pre-spawn
    /// precondition with an HTTP status code** — it reads a paired snapshot
    /// (`snapshot_for_rewrite`), computes `unacknowledged_stored_ids`, returns
    /// 409 `conversation_out_of_date` if the client's copy would delete a stored
    /// row, and otherwise writes through `replace_conversation_preserving_tail`,
    /// never through `replace_conversation`.
    ///
    /// A detached runner structurally cannot express that: it has no status code
    /// to refuse with, and `SessionManager::replace_conversation` is now
    /// documented as **the NAMED EXCEPTION**, correct only for a caller that
    /// owns the whole history — `/clear` and the import/copy/diverge paths.
    /// Calling it here, on a LIVE session, from a detached task, reintroduces
    /// exactly the tail-destroying bug #51 shipped a fix for. So the write stays
    /// in `/reply` (Task 8), above the range Task 8 replaces, and hands its
    /// result here as a seed.
    pub conversation_so_far: Option<Conversation>,
    /// `Option<ReasoningEffort>`, NOT `Option<String>`. It is copied verbatim
    /// from `ChatRequest.reasoning_effort` into `SessionConfig.reasoning_effort`,
    /// and both are `Option<biorouter::agents::ReasoningEffort>` — a fieldless
    /// enum (`Quick | Normal | Deep`) that derives `Clone, Copy`. Declaring it
    /// as a `String` is an E0308 in BOTH directions: at the `SessionConfig`
    /// literal below and at Task 8's `TurnExtras` construction.
    pub reasoning_effort: Option<biorouter::agents::ReasoningEffort>,
    /// Register in the `active_work` registry (the issue's binding table asks
    /// for this on workspace-spawned work). `/reply` passes `false` — an
    /// interactive turn is already visible as a turn.
    pub register_active_work: bool,
    /// Telemetry label: `"app"` for `/reply`, `"workspace"` for injected turns.
    pub session_type_label: &'static str,
}

pub struct TurnRequest {
    pub session_id: String,
    pub user_message: Message,
    pub extras: TurnExtras,
}

impl TurnRequest {
    pub fn new(session_id: String, user_message: Message) -> Self {
        Self {
            session_id,
            user_message,
            extras: TurnExtras {
                register_active_work: true,
                session_type_label: "workspace",
                ..TurnExtras::default()
            },
        }
    }

    pub fn with_idempotency_key(mut self, key: Option<String>) -> Self {
        self.extras.idempotency_key = key;
        self
    }

    pub fn with_extras(mut self, extras: TurnExtras) -> Self {
        self.extras = extras;
        self
    }
}

/// Acquire the session's turn lock and spawn the turn. Returns the
/// server-assigned turn id immediately; the turn runs on a spawned task holding
/// the lock, and every event it produces is published to the session's
/// broadcast. The user message is stamped/persisted by the agent's own reply
/// path — this function persists nothing itself.
pub async fn start_turn(
    state: Arc<AppState>,
    request: TurnRequest,
) -> Result<String, TurnStartError> {
    let cancel_token = CancellationToken::new();
    let turn_guard = state
        .try_begin_turn_idempotent(
            &request.session_id,
            cancel_token.clone(),
            request.extras.idempotency_key.clone(),
        )
        .map_err(|conflict| TurnStartError::TurnInFlight {
            running_turn_id: conflict.running_turn_id,
            duplicate: conflict.duplicate,
        })?;
    let turn_id = turn_guard.turn_id().to_string();

    tokio::spawn(run_turn(state, request, turn_guard, cancel_token));
    Ok(turn_id)
}

/// The turn body. Split out of `start_turn` so Task 8 can also call it with a
/// guard it acquired itself (`/reply` needs the guard's conflict detail before
/// it decides whether to open an SSE response at all).
///
/// # Stated non-goals — both are load-bearing, and both are one-line mistakes
///
/// 1. **This function performs NO whole-history rewrite.** It never calls
///    `SessionManager::replace_conversation` (the NAMED EXCEPTION) and never
///    calls `replace_conversation_preserving_tail` either. The only rewrite that
///    happens during a turn is the agent's own guarded compaction, inside
///    `Agent::reply`, which pairs its revision and its view through
///    `RewriteBasis` — a private struct with private fields and no field-wise
///    constructor, precisely so a conversation cannot be carried across an async
///    boundary, detached from its revision, and then used as an authoritative
///    rewrite. `extras.conversation_so_far` is a *seed* and must stay one;
///    making it a write here is that forbidden split, one layer up.
/// 2. **Events are published to the bus in stream order, never reordered,
///    filtered or coalesced.** #59's ordering invariant — *no
///    `MessagesPersisted` may precede a `Message` frame carrying one of the ids
///    it publishes* (`agent.rs`;
///    `docs/agent-loop/conversation-writeback-freshness.md`) — is a
///    producer-side property that the agent already satisfies
///    (`messages_then_persisted`). Re-ordering here would make it false **at the
///    bus**, where no consumer-side flush can repair it, and the failure is
///    silent: a client that reads an id and then loses the stream claims every
///    stored row while holding none of the bodies, and the `expectedMessageIds`
///    guard truncates rows the user can still see. Coalescing is a per-client
///    concern and lives in `/reply`'s subscription (Task 8), not here.
pub async fn run_turn(
    state: Arc<AppState>,
    request: TurnRequest,
    turn_guard: crate::state::TurnGuard,
    cancel_token: CancellationToken,
) {
    let TurnRequest {
        session_id,
        user_message,
        extras,
    } = request;
    let turn_started = std::time::Instant::now();

    // Holds the per-session turn lock for the turn's lifetime; dropped
    // (releasing the session) when this future ends — the same RAII discipline
    // the pre-BR-71 /reply task used.
    let _turn_guard = turn_guard;
    // Defer scheduled background jobs while a turn is in flight.
    let _interactive_turn = biorouter::scheduler::interactive_turn_guard();

    // Reclaim this session's 1024-slot broadcast ring once its consumers are
    // gone. `broadcast::channel` allocates the whole ring at creation
    // (`Sender::new_with_receiver_count`), so an insert-and-never-remove map is
    // a real leak on a daemon that publishes for every turn of every session and
    // stays up for days. RAII, so every `return` below is covered — there are
    // several of them and enumerating them by hand is how one gets missed.
    struct BusRelease(String);
    impl Drop for BusRelease {
        fn drop(&mut self) {
            let session_id = std::mem::take(&mut self.0);
            // Only when a runtime is still around: the guard can also unwind
            // during runtime teardown, where there is nothing to spawn onto.
            let Ok(handle) = tokio::runtime::Handle::try_current() else {
                biorouter::session_events::release_if_idle(&session_id);
                return;
            };
            handle.spawn(async move {
                // Grace period, deliberately: at the instant `run_turn` returns,
                // the `/reply` SSE consumer is still holding its `Receiver` to
                // read the terminal frame, so an immediate call would always
                // find `receiver_count() > 0` and free nothing — and the entry
                // would then live forever for a session that never runs another
                // turn. 30 s also keeps a rapid back-and-forth from churning
                // one allocation per turn.
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                biorouter::session_events::release_if_idle(&session_id);
            });
        }
    }
    let _bus_release = BusRelease(session_id.clone());

    let _active_work = extras.register_active_work.then(|| {
        use biorouter_mcp::active_work::{ActiveWorkGuard, ActiveWorkKind};
        let token = cancel_token.clone();
        let cancel: std::sync::Arc<dyn Fn() + Send + Sync> =
            std::sync::Arc::new(move || token.cancel());
        ActiveWorkGuard::register(
            ActiveWorkKind::DetachedTurn,
            "detached workspace turn",
            Some(format!("session {session_id}")),
            Some(session_id.clone()),
            Some(cancel),
        )
    });

    session_events::publish(
        &session_id,
        SessionBusEvent::TurnStarted {
            turn_id: _turn_guard.turn_id().to_string(),
        },
    );

    // One terminal event per turn, always. Every exit path below publishes
    // exactly one `TurnError` or one `TurnFinished`, never both and never two.
    //
    // `provider_kind` is a parameter, not a hardcoded `None`: it is one of the
    // three fields the desktop's rate-limit/retry/compaction recovery reads, and
    // the classifier below is the only thing in the process that produces it.
    let publish_error = |message: String,
                         code: &str,
                         scope: TurnErrorScope,
                         retryable: bool,
                         provider_kind: Option<String>| {
        session_events::publish(
            &session_id,
            SessionBusEvent::TurnError {
                message,
                code: code.to_string(),
                scope: scope.wire_value().to_string(),
                retryable,
                provider_kind,
            },
        );
    };

    let agent = match state.get_agent(session_id.clone()).await {
        Ok(agent) => agent,
        Err(e) => {
            tracing::error!("turn: failed to get session agent: {e}");
            publish_error(
                format!("Failed to get session agent: {e}"),
                "agent_unavailable",
                TurnErrorScope::Session,
                true,
                None,
            );
            return;
        }
    };
    let session = match state.session_manager().get_session(&session_id, true).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("turn: failed to read session: {e}");
            publish_error(
                format!("Failed to read session: {e}"),
                "session_unavailable",
                TurnErrorScope::Session,
                true,
                None,
            );
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
        // `ReasoningEffort` is `Copy`, so this is a plain move-out-of-a-copy;
        // the field types on both sides are `Option<ReasoningEffort>`.
        reasoning_effort: extras.reasoning_effort,
    };

    // Seed the accumulator. NO STORAGE WRITE HAPPENS HERE — see the stated
    // non-goal above, and `TurnExtras.conversation_so_far`'s doc.
    //
    // `Some(_)` is the conversation `/reply` already validated and stored via
    // `apply_client_writeback` before spawning this task; `None` is every other
    // caller, which starts from the session's own history. The trailing push is
    // load-bearing and must not be dropped: without it
    // `emit_completion_metrics`' fallback `message_count` is off by one and
    // `track_tool_telemetry`'s lookup base differs by a message.
    let mut all_messages = extras
        .conversation_so_far
        .unwrap_or_else(|| session.conversation.clone().unwrap_or_default());
    all_messages.push(user_message.clone());

    let mut stream = match agent
        .reply(user_message, session_config, Some(cancel_token.clone()))
        .await
    {
        Ok(stream) => stream,
        Err(e) => {
            tracing::error!("turn: failed to start reply stream: {e:?}");
            publish_error(
                e.to_string(),
                "inference_start_failed",
                TurnErrorScope::Inference,
                false,
                None,
            );
            return;
        }
    };

    let mut terminal_error = false;
    while let Some(item) = stream.next().await {
        match item {
            Ok(event) => {
                // Bookkeeping that belongs to the TURN, not to any consumer:
                // telemetry and the accumulated conversation used by the
                // completion metrics below.
                match &event {
                    AgentEvent::Message(message) => {
                        for content in &message.content {
                            track_tool_telemetry(content, all_messages.messages());
                        }
                        all_messages.push(message.clone());
                    }
                    AgentEvent::HistoryReplaced(new_messages) => {
                        all_messages = new_messages.clone();
                    }
                    _ => {}
                }

                // An abort is CLASSIFIED and republished as this turn's single
                // terminal event — it is deliberately NOT also forwarded raw.
                // `map_bus_event` maps both `SessionBusEvent::TurnError` and
                // `AgentEvent::TurnAborted` to `MessageEvent::Error`, so
                // publishing both would give every consumer two terminal Error
                // frames for one abort. The agent has already yielded the
                // human-readable assistant message in an earlier iteration; this
                // event is what stops the desktop from rendering a provider 403
                // as a completed turn — and, with `classify_abort`, what keeps
                // it recoverable.
                if let AgentEvent::TurnAborted { code, message } = &event {
                    tracing::error!(abort = code.wire_code(), "Turn aborted: {message}");
                    let (scope, retryable, provider_kind) = classify_abort(code);
                    publish_error(
                        message.clone(),
                        code.wire_code(),
                        scope,
                        retryable,
                        provider_kind,
                    );
                    terminal_error = true;
                    break;
                }

                // Publish the raw AgentEvent. Consumers map it; the runner does
                // not pre-render any wire frame.
                session_events::publish(&session_id, SessionBusEvent::Agent(event));
            }
            Err(e) => {
                tracing::error!("turn: stream error: {e}");
                publish_error(
                    e.to_string(),
                    "stream_error",
                    TurnErrorScope::Inference,
                    false,
                    None,
                );
                terminal_error = true;
                break;
            }
        }
    }

    // Best-effort LLM session rename — always runs, unlike a tail on the lazy
    // reply stream which an early `break` above would skip, leaving the session
    // stuck on "New Session".
    {
        let agent_for_rename = agent.clone();
        let session_id_for_rename = session_id.clone();
        tokio::spawn(async move {
            agent_for_rename
                .maybe_rename_session(&session_id_for_rename)
                .await;
        });
    }

    let exit_type = if terminal_error {
        "error"
    } else if cancel_token.is_cancelled() {
        "cancelled"
    } else {
        "normal"
    };
    emit_completion_metrics(
        &state,
        &session_id,
        extras.session_type_label,
        exit_type,
        turn_started.elapsed(),
        all_messages.len(),
    )
    .await;

    // BR-52: one authoritative read at the end of the turn — the single point
    // where a client's token readout is reconciled with the store, so nothing
    // written outside this turn (a background eager compaction, a concurrent
    // scheduled run) can leave the UI on a stale count.
    let final_token_state = get_token_state(state.session_manager(), &session_id).await;

    if !terminal_error {
        session_events::publish(
            &session_id,
            SessionBusEvent::TurnFinished {
                reason: if cancel_token.is_cancelled() {
                    "cancelled".into()
                } else {
                    "stop".into()
                },
                token_state: Some(final_token_state),
            },
        );
    }
}

/// The terminal-error classifier, moved out of `/reply`'s event loop so the ONE
/// runner still produces the full envelope. Pure, so it is unit-testable
/// without a provider (see this module's tests).
pub(crate) fn classify_abort(
    code: &biorouter::agents::TurnAbortCode,
) -> (TurnErrorScope, bool, Option<String>) {
    use biorouter::agents::TurnAbortCode;
    match code {
        TurnAbortCode::ProviderFailure { kind } => (
            TurnErrorScope::Provider,
            kind.is_transient(),
            Some(kind.wire_code().to_string()),
        ),
        // #31/#41: a session-store failure is a Session-scoped error — not the
        // provider's fault and not retryable until the local db problem is fixed.
        TurnAbortCode::SessionStore => (TurnErrorScope::Session, false, None),
        TurnAbortCode::ToolLoop { .. } => (TurnErrorScope::Inference, false, None),
        TurnAbortCode::WorkerTimeout { .. } => (TurnErrorScope::Inference, true, None),
    }
}

/// The session-completion telemetry, byte-for-byte the block in `/reply`'s task
/// at `275d735d` with one substitution: the three literal `session_type = "app"`
/// fields become `session_type = session_type_label`, so an injected turn is not
/// counted as an app session.
async fn emit_completion_metrics(
    state: &Arc<AppState>,
    session_id: &str,
    session_type_label: &'static str,
    exit_type: &'static str,
    duration: std::time::Duration,
    fallback_message_count: usize,
) {
    if let Ok(session) = state.session_manager().get_session(session_id, true).await {
        let total_tokens = session.total_tokens.unwrap_or(0);
        tracing::info!(
            counter.biorouter.session_completions = 1,
            session_type = session_type_label,
            interface = "ui",
            exit_type = exit_type,
            duration_ms = duration.as_millis() as u64,
            total_tokens = total_tokens,
            message_count = session.message_count,
            "Session completed"
        );

        tracing::info!(
            counter.biorouter.session_duration_ms = duration.as_millis() as u64,
            session_type = session_type_label,
            interface = "ui",
            "Session duration"
        );

        if total_tokens > 0 {
            tracing::info!(
                counter.biorouter.session_tokens = total_tokens,
                session_type = session_type_label,
                interface = "ui",
                "Session tokens"
            );
        }
    } else {
        tracing::info!(
            counter.biorouter.session_completions = 1,
            session_type = session_type_label,
            interface = "ui",
            exit_type = exit_type,
            duration_ms = duration.as_millis() as u64,
            total_tokens = 0u64,
            message_count = fallback_message_count,
            "Session completed"
        );

        tracing::info!(
            counter.biorouter.session_duration_ms = duration.as_millis() as u64,
            session_type = session_type_label,
            interface = "ui",
            "Session duration"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use biorouter::conversation::message::Message;
    use biorouter::session::session_manager::SessionType;
    use biorouter::session_events::{self, SessionBusEvent};
    use tokio_util::sync::CancellationToken;

    /// NOTE — two things about every test in this module:
    ///
    /// 1. `AppState::new()` opens the **REAL user session database** (it goes
    ///    through `AgentManager::instance()` → `SessionManager::instance()`;
    ///    `routes/session.rs:1122` and `:1414` carry the same warning). These
    ///    tests create rows in the developer's own history. Keep session names
    ///    unique and never assert on total row counts.
    /// 2. The `TempDir` is the session's **working dir**, not a database.
    ///    `create_session`'s first parameter is `working_dir`
    ///    (`session_manager.rs:1191-1196`). An earlier draft did
    ///    `std::mem::forget(temp)` with the comment "keep the DB alive" — a false
    ///    invariant that would send the next reader looking for a database that
    ///    is not there. The guard is still returned, because deleting the
    ///    working directory while the turn runner is using it is its own bug;
    ///    the caller just has to hold it.
    ///
    /// (Contrast Task 18's helper, which builds `SessionManager::new(temp.path())`
    /// — there the TempDir really does hold the sqlite file.)
    async fn session(
        state: &std::sync::Arc<crate::state::AppState>,
        name: &str,
    ) -> (tempfile::TempDir, String) {
        let temp = tempfile::TempDir::new().unwrap();
        let s = state
            .session_manager()
            .create_session(temp.path().to_path_buf(), name.into(), SessionType::User)
            .await
            .unwrap();
        (temp, s.id)
    }

    #[tokio::test]
    async fn start_turn_refuses_when_a_turn_is_in_flight() {
        let state = crate::state::AppState::new().await.unwrap();
        let (_workdir, sid) = session(&state, "busy").await;

        let _guard = state
            .try_begin_turn_idempotent(&sid, CancellationToken::new(), None)
            .unwrap();

        let err = start_turn(
            state.clone(),
            TurnRequest::new(sid.clone(), Message::user().with_text("x")),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, TurnStartError::TurnInFlight { .. }));
    }

    #[tokio::test]
    async fn turn_publishes_lifecycle_and_releases_the_lock_even_when_it_fails() {
        let state = crate::state::AppState::new().await.unwrap();
        let (_workdir, sid) = session(&state, "detached").await;

        let mut rx = session_events::subscribe(&sid);
        // No provider on the fresh agent → the turn starts, fails fast, and
        // must still bracket itself on the bus.
        let turn_id = start_turn(
            state.clone(),
            TurnRequest::new(sid.clone(), Message::user().with_text("go")),
        )
        .await
        .unwrap();
        assert!(turn_id.starts_with("turn-"));

        let first = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
            .await
            .expect("event in time")
            .unwrap();
        assert!(matches!(first, SessionBusEvent::TurnStarted { .. }));

        // Drain the WHOLE turn, not just up to the first terminal. Breaking on
        // the first one asserts "at least one" while the message claims
        // "exactly one" — and the regression this guards against is a runner
        // that publishes BOTH the raw `AgentEvent::TurnAborted` and the
        // classified `TurnError`, which `map_bus_event` renders as two `Error`
        // frames. Step 3's implementation comment forbids exactly that.
        //
        // A short timeout, not `try_recv`: the double-publish puts the two
        // terminals on the bus adjacently but asynchronously, so an immediate
        // `try_recv() == Empty` right after the first proves nothing.
        //
        // The loop exits on anything that is not a delivered event — a timeout
        // or a closed/lagged channel both mean the turn has stopped publishing.
        let mut terminals: Vec<SessionBusEvent> = Vec::new();
        while let Ok(Ok(ev)) =
            tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv()).await
        {
            if matches!(
                ev,
                SessionBusEvent::TurnFinished { .. } | SessionBusEvent::TurnError { .. }
            ) {
                terminals.push(ev);
            }
        }
        assert_eq!(
            terminals.len(),
            1,
            "every turn must publish exactly one terminal event, got {terminals:?}"
        );

        // The turn lock must be released once the task unwinds.
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while state.is_turn_active(&sid) {
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("turn lock released");
    }

    #[tokio::test]
    async fn idempotency_key_is_forwarded_so_reply_reconnects_still_dedupe() {
        // Task 8 depends on this: /reply forwards the client's turn_id, and a
        // re-POST of the same id must be reported as a duplicate, not a second
        // turn (BR-62).
        let state = crate::state::AppState::new().await.unwrap();
        let (_workdir, sid) = session(&state, "idem").await;

        let request = TurnRequest::new(sid.clone(), Message::user().with_text("a"))
            .with_idempotency_key(Some("client-turn-1".to_string()));
        let _first = start_turn(state.clone(), request).await.unwrap();

        let again = TurnRequest::new(sid.clone(), Message::user().with_text("a"))
            .with_idempotency_key(Some("client-turn-1".to_string()));
        // Deliberately no catch-all arm: `TurnStartError` has exactly one
        // variant today, so an `other => panic!(…)` arm is an
        // `unreachable_patterns` warning and this repo lints with `-D warnings`.
        // An exhaustive match is also the stronger guard — a new variant breaks
        // the build here instead of silently taking a panic arm at runtime.
        match start_turn(state.clone(), again).await {
            Err(TurnStartError::TurnInFlight { duplicate, .. }) => assert!(duplicate),
            // A fast machine may have finished the (provider-less) turn already;
            // then the second call legitimately starts a new turn.
            Ok(_) => {}
        }
    }

    /// ⚠ Reconciliation #21: `run_turn` performs NO whole-history rewrite.
    ///
    /// The property, not a grep: seed a session with a stored message, then run
    /// a turn whose `conversation_so_far` **omits** it. The stored row must
    /// still be there afterwards. Under the pre-amendment plan — which had
    /// `run_turn` call `SessionManager::replace_conversation` on the client's
    /// copy — that row is DELETEd and re-INSERTed away, silently, from a
    /// detached task with no status code to refuse with. That is the #51 W5 bug
    /// `306552fd` shipped a fix for, and `grep -c replace_conversation turn.rs`
    /// cannot tell the two implementations apart.
    ///
    /// This test does not need a provider: the turn fails fast without one, and
    /// the destruction (if any) happens before `agent.reply` is ever reached.
    #[tokio::test]
    async fn a_seed_conversation_is_never_written_back_over_the_store() {
        use biorouter::conversation::Conversation;

        let state = crate::state::AppState::new().await.unwrap();
        let (_workdir, sid) = session(&state, "seed-is-not-a-write").await;

        // One durable row the client's copy will not name.
        let mut stored = Message::user().with_text("br71-seed-probe");
        state
            .session_manager()
            .add_message_adopting_uid(&sid, &mut stored)
            .await
            .unwrap();
        let probe_id = stored.id.clone().expect("the store stamps an id");

        // A seed that pretends the session is empty.
        let request = TurnRequest {
            session_id: sid.clone(),
            user_message: Message::user().with_text("go"),
            extras: TurnExtras {
                conversation_so_far: Some(Conversation::new_unvalidated(Vec::new())),
                ..TurnExtras::default()
            },
        };
        let cancel = CancellationToken::new();
        let guard = state
            .try_begin_turn_idempotent(&sid, cancel.clone(), None)
            .expect("session is idle");
        run_turn(state.clone(), request, guard, cancel).await;

        let reread = state
            .session_manager()
            .get_session(&sid, true)
            .await
            .unwrap();
        let ids: Vec<String> = reread
            .conversation
            .unwrap_or_default()
            .messages()
            .iter()
            .filter_map(|m| m.id.clone())
            .collect();
        assert!(
            ids.contains(&probe_id),
            "run_turn destroyed a stored message it was merely seeded around; \
             the seed is not a write-back (ids: {ids:?})"
        );
    }

    /// Reconciliation #9: the terminal-error CLASSIFIER moved out of `/reply`
    /// with its fidelity intact. This is the test that stops the refactor from
    /// silently collapsing every abort to `(Inference, false, None)` — which
    /// would delete the desktop's rate-limit/retry/compaction recovery, because
    /// `scope:"provider"`, `retryable:true` and `provider_kind` are exactly what
    /// it keys off and nothing else in the process emits them.
    #[test]
    fn abort_codes_classify_exactly_as_the_pre_refactor_handler_did() {
        use biorouter::agents::TurnAbortCode;
        use biorouter::providers::errors::ProviderErrorKind;

        // A transient provider failure: scope provider, retryable, kind named.
        let (scope, retryable, kind) = classify_abort(&TurnAbortCode::ProviderFailure {
            kind: ProviderErrorKind::RateLimit,
        });
        assert_eq!(scope, TurnErrorScope::Provider);
        assert!(
            retryable,
            "a rate limit is transient — the client retries it"
        );
        assert_eq!(
            kind.as_deref(),
            Some(ProviderErrorKind::RateLimit.wire_code())
        );

        // A non-transient one: still provider-scoped, still named, not retryable.
        let (scope, retryable, kind) = classify_abort(&TurnAbortCode::ProviderFailure {
            kind: ProviderErrorKind::Auth,
        });
        assert_eq!(scope, TurnErrorScope::Provider);
        assert!(!retryable, "a bad credential never succeeds on retry");
        assert_eq!(kind.as_deref(), Some("auth"));

        // #31/#41: a local store failure is NOT the provider's fault.
        assert_eq!(
            classify_abort(&TurnAbortCode::SessionStore),
            (TurnErrorScope::Session, false, None)
        );
        assert_eq!(
            classify_abort(&TurnAbortCode::ToolLoop {
                tool: "shell".into()
            }),
            (TurnErrorScope::Inference, false, None)
        );
        assert_eq!(
            classify_abort(&TurnAbortCode::WorkerTimeout {
                agent: "reviewer".into(),
                elapsed_s: 90,
            }),
            (TurnErrorScope::Inference, true, None)
        );
    }
}
