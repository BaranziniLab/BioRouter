use axum::http::StatusCode;
use biorouter::execution::manager::AgentManager;
use biorouter::scheduler_trait::SchedulerTrait;
use biorouter::session::SessionManager;
use biorouter_mcp::knowledge::service::KnowledgeService;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::tunnel::TunnelManager;
use crate::turn_stream::TurnStream;
use biorouter::agents::ExtensionLoadResult;

type ExtensionLoadingTasks =
    Arc<Mutex<HashMap<String, Arc<Mutex<Option<JoinHandle<Vec<ExtensionLoadResult>>>>>>>>;

/// Process-wide monotonic turn id source. Ids are only used to identify the
/// in-flight turn a rejected concurrent `/reply` collided with.
static TURN_SEQ: AtomicU64 = AtomicU64::new(1);

/// How long a FINISHED turn's entry is kept so a re-POST of its idempotency key
/// attaches to its replay instead of starting a second turn.
///
/// The window that matters is the same one the orphan reaper is sized for: a
/// renderer that reloads (~4.6 s) while the turn is completing must re-POST into
/// the finished turn's replay, not into a fresh turn that spends the tokens
/// again. Five minutes matches
/// [`crate::turn_stream::DEFAULT_ORPHAN_TIMEOUT`], so "the turn is still
/// addressable by its key" has ONE duration rather than two that can disagree.
/// The retained log is trimmed on close (see `CLOSED_REPLAY_BYTE_BUDGET`), so
/// the memory cost of the window is bounded per session.
const FINISHED_TURN_RETENTION: Duration = Duration::from_secs(300);

/// The turn currently in flight for a session — or, briefly, the one that just
/// ended (see [`FINISHED_TURN_RETENTION`]).
#[derive(Debug, Clone)]
struct ActiveTurn {
    /// Server-assigned id, surfaced to a client whose `/reply` was rejected.
    turn_id: String,
    /// The client's idempotency key for this turn, if it sent one (BR-62). Lets
    /// a re-POST of the *same* turn — an SSE reconnect, a retried fetch — be
    /// recognized as a duplicate rather than counted as a second turn.
    idempotency_key: Option<String>,
    /// Trips the running turn's agent loop and its SSE task. This is what makes
    /// cancel *addressable*: before BR-62 the token lived only inside the
    /// `/reply` task, so the only way to cancel was to tear down the SSE socket
    /// and `/agent/stop` merely evicted the agent from the LRU while the turn
    /// kept running on its own `Arc<Agent>`.
    cancel: CancellationToken,
    /// This turn's sequence-numbered, replayable frame log. Created WITH the
    /// turn rather than by the request that happens to start it, because the
    /// stream belongs to the turn: every observer, including the POST that
    /// began it, is just a reader of this.
    stream: Arc<TurnStream>,
    /// `Some` once the turn's guard has dropped — i.e. the turn is OVER. The
    /// entry survives its turn only so a re-POST of the same idempotency key
    /// can be answered from the replay above instead of starting a second turn;
    /// a finished entry blocks nothing and is swept after
    /// [`FINISHED_TURN_RETENTION`].
    finished_at: Option<Instant>,
}

/// Why `AppState::try_begin_turn_idempotent` refused to start a turn.
#[derive(Debug, Clone)]
pub struct TurnConflict {
    /// The id of the turn already in flight for this session.
    pub running_turn_id: String,
    /// True when the caller's idempotency key matches the in-flight turn's: this
    /// POST is a *re-delivery of that same turn*, not a second one. Clients treat
    /// it as "your turn is still running, re-attach" instead of an error.
    pub duplicate: bool,
    /// The colliding turn's frame log, so a `duplicate` caller can be ATTACHED
    /// to it — answered 200 with the whole turn replayed from seq 0 and then the
    /// live tail — rather than told 409 and left with no way back into a turn
    /// that is still spending its tokens.
    pub stream: Arc<TurnStream>,
    /// True when the colliding turn has already ENDED and is only being held for
    /// its replay. Attaching to it yields the complete turn and a terminal
    /// frame; nothing is still running.
    pub finished: bool,
}

/// RAII guard marking that a session has an interactive turn in flight. Held by
/// the `/reply` task and removed from the active-turns map when dropped (turn
/// completes, errors, or is cancelled), so the next `/reply` for that session
/// can proceed. See `AppState::try_begin_turn`.
#[derive(Debug)]
pub struct TurnGuard {
    session_id: String,
    /// The turn this guard owns. Checked on drop so a guard can only ever retire
    /// *its own* entry, never a successor's.
    turn_id: String,
    /// This turn's frame log, so the runner can publish into it without a second
    /// registry lookup — and so `Drop` can close it.
    stream: Arc<TurnStream>,
    active_turns: Arc<StdMutex<HashMap<String, ActiveTurn>>>,
}

impl TurnGuard {
    /// The server-assigned id of the turn this guard owns (BR-71: published as
    /// `SessionBusEvent::TurnStarted` so consumers can correlate lifecycles).
    pub fn turn_id(&self) -> &str {
        &self.turn_id
    }

    /// The session this guard locked.
    ///
    /// BR-71's `run_turn` takes the request and the guard as separate
    /// arguments, and both Task 8 and Task 14 acquire the guard themselves
    /// before calling it — so without this there is nothing stopping
    /// `run_turn(state, request_for_B, guard_for_A)` from compiling, running an
    /// unguarded turn on B while holding A's lock. The runner `debug_assert`s
    /// on it.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// This turn's frame log. The runner's SSE pump publishes into it and every
    /// HTTP response that watches the turn reads from it.
    pub fn stream(&self) -> Arc<TurnStream> {
        Arc::clone(&self.stream)
    }
}

impl Drop for TurnGuard {
    fn drop(&mut self) {
        // ⚠ This does NOT close the turn's stream, and must not.
        //
        // The guard drops the instant the RUNNER returns, but the runner's last
        // act was to *publish* `TurnFinished` onto the session bus — the pump
        // has not necessarily consumed it yet. Closing here would race that
        // read, `publish` would refuse the frame as post-terminal, and every
        // healthy turn would end in the synthesized "stream ended without a
        // result" error instead of a clean `Finish`. The pump owns the log's
        // lifetime and closes it on every one of its exit paths
        // (`pump_bus_into_stream`); a turn with no pump is only ever attached to
        // through the retired path, which closes on demand.
        let mut turns = self
            .active_turns
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Only touch the slot if it is still ours.
        if let Some(turn) = turns.get_mut(&self.session_id) {
            if turn.turn_id == self.turn_id {
                // NOT `remove`. The entry is retired, not deleted: a re-POST of
                // this turn's idempotency key inside FINISHED_TURN_RETENTION
                // must attach to the replay above, not start a second turn and
                // spend the tokens twice. `finished_at` is what every "is a turn
                // running?" reader below filters on, so a retired entry blocks
                // nothing.
                turn.finished_at = Some(Instant::now());
            }
        }
        prune_finished_turns(&mut turns);
    }
}

/// Drop retired entries once nothing can still be addressing them by key.
///
/// Called from the two places that already hold the registry lock — a guard
/// dropping and a turn beginning — so there is no sweeper task to leak, and the
/// map stays bounded by "sessions that ran a turn in the last five minutes"
/// rather than by every session id the process has ever seen.
fn prune_finished_turns(turns: &mut HashMap<String, ActiveTurn>) {
    turns.retain(|_, turn| {
        turn.finished_at
            .is_none_or(|at| at.elapsed() < FINISHED_TURN_RETENTION)
    });
}

/// How many session-observer streams (`GET /sessions/{id}/events`) this daemon
/// will follow the live tail for at the same time.
///
/// ⚠ **This is a limit on the CLIENT's connection budget, not on the server's
/// resources.** An observer stream costs the daemon almost nothing — a bus
/// subscription and a 500 ms heartbeat — so the obvious reading, "three is
/// plenty of capacity", is the wrong frame entirely. The cost is on the other
/// end of the socket. An observer never finishes on its own; it ends only when
/// the client hangs up. Every one of them therefore *parks a TCP connection for
/// as long as the tab exists*, and a browser will not open more than a handful
/// per host (six in Chromium, shared by every window of the app, because they
/// are one origin behind one network process). Once the daemon is holding that
/// many, the renderer cannot dispatch **any** further request to it: not
/// `/config/read`, not `/agent/tools`, and not `POST /reply`. The turn the user
/// just asked for is never sent, so the composer spins forever while the daemon
/// sits idle, healthy, and answering `curl` in under a millisecond — which is
/// exactly what makes the failure so hard to read from either side alone.
///
/// Measured on the wedge this constant was introduced for: six live observers,
/// 348 bytes/s of pure heartbeat out and 0 bytes in, a fresh `GET /status`
/// issued from inside the renderer still unanswered after 8 s while the same
/// request from a shell returned in 0.9 ms, and the whole app recovering the
/// instant enough tabs were closed to drop the observer count. The turns
/// themselves had all completed server-side minutes earlier.
///
/// Three leaves at least half of a six-connection budget free for `/reply` and
/// for ordinary request/response traffic. `/reply` is deliberately NOT counted
/// here: a turn stream is bounded by its turn rather than by a tab, and
/// refusing one would break the thing the app exists to do.
pub const MAX_LIVE_OBSERVER_STREAMS: usize = 3;

/// Permission to hold one session-observer stream open, released on drop.
///
/// RAII rather than a paired increment/decrement because the release has to
/// survive paths that do not run to the bottom of the observer task: the client
/// hanging up mid-send, the bus closing, a panic unwinding the task. A missed
/// decrement is not a leak that shows up as memory — it permanently shrinks the
/// client's usable connection budget, which is the same wedge one slot smaller.
#[derive(Debug)]
pub struct ObserverSlot(Arc<AtomicUsize>);

impl Drop for ObserverSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Clone)]
pub struct AppState {
    pub(crate) agent_manager: Arc<AgentManager>,
    pub workflow_file_hash_map: Arc<Mutex<HashMap<String, PathBuf>>>,
    /// Tracks sessions that have already emitted workflow telemetry to prevent double counting.
    workflow_session_tracker: Arc<Mutex<HashSet<String>>>,
    /// Sessions with an interactive turn in flight, mapped to that turn's state.
    /// Enforces one turn per session at the server so a second `/reply` can't
    /// race a shared `Arc<Agent>` (confirmation channel, soft-interrupt queue,
    /// check-compact-persist) with the running turn (BR-33), and holds each
    /// running turn's cancellation token so cancel is addressable by session id
    /// rather than only by dropping the SSE socket (BR-62).
    active_turns: Arc<StdMutex<HashMap<String, ActiveTurn>>>,
    /// Session-observer SSE responses this daemon is holding open right now.
    /// See [`MAX_LIVE_OBSERVER_STREAMS`] for why the count is bounded at all.
    observer_streams: Arc<AtomicUsize>,
    pub tunnel_manager: Arc<TunnelManager>,
    pub extension_loading_tasks: ExtensionLoadingTasks,
    // Used by knowledge route handlers (Task 5+).
    pub knowledge_service: Arc<KnowledgeService>,
}

impl AppState {
    pub async fn new() -> anyhow::Result<Arc<AppState>> {
        let agent_manager = AgentManager::instance().await?;
        let tunnel_manager = Arc::new(TunnelManager::new());
        let knowledge_service = Arc::new(KnowledgeService::new_default()?);

        Ok(Arc::new(Self {
            agent_manager,
            workflow_file_hash_map: Arc::new(Mutex::new(HashMap::new())),
            workflow_session_tracker: Arc::new(Mutex::new(HashSet::new())),
            active_turns: Arc::new(StdMutex::new(HashMap::new())),
            observer_streams: Arc::new(AtomicUsize::new(0)),
            tunnel_manager,
            extension_loading_tasks: Arc::new(Mutex::new(HashMap::new())),
            knowledge_service,
        }))
    }

    /// [`AppState::new`] with its `KnowledgeService` rooted at `knowledge_root`
    /// instead of the developer's real knowledge tree.
    ///
    /// Test-only, and it exists because a test of the knowledge-selection seam
    /// has to CREATE knowledge bases and move a session's write target around.
    /// Against `new_default()` that would write into
    /// `~/.config/biorouter/knowledge` — inventing bases in the developer's own
    /// sidebar and repointing their primary. The session database behind
    /// `AgentManager::instance()` still cannot be relocated from inside a
    /// running test binary (`BIOROUTER_PATH_ROOT` is read before the process
    /// starts), so this isolates the part that can be isolated; see the
    /// preamble in `workspace/services.rs`'s tests.
    #[cfg(test)]
    pub(crate) async fn new_with_knowledge_root(
        knowledge_root: PathBuf,
    ) -> anyhow::Result<Arc<AppState>> {
        let agent_manager = AgentManager::instance().await?;
        let tunnel_manager = Arc::new(TunnelManager::new());
        let knowledge_service = Arc::new(KnowledgeService::new(knowledge_root));

        Ok(Arc::new(Self {
            agent_manager,
            workflow_file_hash_map: Arc::new(Mutex::new(HashMap::new())),
            workflow_session_tracker: Arc::new(Mutex::new(HashSet::new())),
            active_turns: Arc::new(StdMutex::new(HashMap::new())),
            observer_streams: Arc::new(AtomicUsize::new(0)),
            tunnel_manager,
            extension_loading_tasks: Arc::new(Mutex::new(HashMap::new())),
            knowledge_service,
        }))
    }

    /// Claim one of the [`MAX_LIVE_OBSERVER_STREAMS`] slots for following a
    /// session's live tail, or `None` when the daemon is already holding that
    /// many open.
    ///
    /// The check and the increment are one `fetch_update`, not a load followed
    /// by a store: several tabs re-attach in the same instant after a reload,
    /// and a read-then-write would let all of them see the same under-budget
    /// count and admit every one of them — which is the state being bounded.
    ///
    /// Refusal is not an error, and callers must not report it as one. The
    /// observer's first frame is the whole stored conversation, so a caller
    /// that is turned away has already been told everything the session
    /// currently says; all it loses is the live tail, and its own reconnect
    /// backoff will ask again.
    pub fn try_admit_observer_stream(&self) -> Option<ObserverSlot> {
        let counter = Arc::clone(&self.observer_streams);
        counter
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |live| {
                (live < MAX_LIVE_OBSERVER_STREAMS).then_some(live + 1)
            })
            .ok()
            .map(|_| ObserverSlot(counter))
    }

    /// Session-observer streams currently held open. Test-facing.
    #[cfg(test)]
    pub(crate) fn live_observer_streams(&self) -> usize {
        self.observer_streams.load(Ordering::Acquire)
    }

    /// Begin an interactive turn for `session_id`. Returns a `TurnGuard` that
    /// keeps the session marked busy until dropped, or a [`TurnConflict`] if a
    /// turn is already in flight — the caller must reject the duplicate `/reply`
    /// rather than start a second turn on the shared agent (BR-33).
    ///
    /// `cancel` is registered as the token that [`AppState::cancel_turn`] trips,
    /// and `idempotency_key` as the client's name for this turn (BR-62).
    ///
    /// The key is what makes `/reply` safe to retry. With `sseMaxRetryAttempts`,
    /// a dropped stream makes the client re-POST the *same* turn; without a key
    /// the server cannot distinguish that from a genuine second turn, so the
    /// retry either starts a duplicate turn (double token spend, interleaved
    /// output) or is rejected as a hard error. With one, the conflict comes back
    /// flagged `duplicate` — carrying the turn's [`TurnStream`], so `/reply` can
    /// ATTACH the caller to the running turn (200 + its replay) instead of
    /// answering 409 and leaving it with no way back into a turn that is still
    /// spending its tokens.
    pub fn try_begin_turn_idempotent(
        &self,
        session_id: &str,
        cancel: CancellationToken,
        idempotency_key: Option<String>,
    ) -> Result<TurnGuard, TurnConflict> {
        let mut turns = self
            .active_turns
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        prune_finished_turns(&mut turns);

        if let Some(running) = turns.get(session_id) {
            // A key only marks a duplicate when the client actually supplied one
            // *and* it names the running turn. Two keyless turns are two turns.
            //
            // EITHER NAME identifies the turn: the key the client chose, or the
            // server-assigned `turn-N`. A client that reloaded did not keep its
            // own key — what it has is the `turn_id` stamped on the last frame it
            // rendered, or the one `POST /agent/resume` handed it, and both of
            // those are the server's. Matching only the client's key would 409
            // every reload-then-reattach, which is the case this exists for.
            let duplicate = idempotency_key.is_some()
                && (idempotency_key == running.idempotency_key
                    || idempotency_key.as_deref() == Some(running.turn_id.as_str()));
            let finished = running.finished_at.is_some();
            // A RUNNING turn always conflicts. A FINISHED one conflicts only for
            // the client re-POSTing its own key — that caller is re-delivering a
            // turn it already paid for and must be given the replay, while
            // anyone else is simply sending the next message and gets a fresh
            // turn (the insert below replaces the retired entry).
            if !finished || duplicate {
                return Err(TurnConflict {
                    running_turn_id: running.turn_id.clone(),
                    duplicate,
                    stream: Arc::clone(&running.stream),
                    finished,
                });
            }
        }

        let turn_id = format!("turn-{}", TURN_SEQ.fetch_add(1, Ordering::Relaxed));
        let stream = TurnStream::new(session_id, &turn_id);
        turns.insert(
            session_id.to_string(),
            ActiveTurn {
                turn_id: turn_id.clone(),
                idempotency_key,
                cancel,
                stream: Arc::clone(&stream),
                finished_at: None,
            },
        );
        Ok(TurnGuard {
            session_id: session_id.to_string(),
            turn_id,
            stream,
            active_turns: Arc::clone(&self.active_turns),
        })
    }

    /// Cancel the turn in flight for `session_id`, returning its id. `None` when
    /// no turn is running.
    ///
    /// This is the addressable cancel BR-62 adds: tripping the token unwinds the
    /// agent loop at its next boundary, unblocks any tool-permission prompt it is
    /// parked on, and ends the SSE task — all without the client having to drop
    /// its socket, so a second client, the CLI, or a script can stop a runaway
    /// turn. The `TurnGuard` clears the slot as the turn task unwinds; we
    /// deliberately do **not** remove the entry here, so `cancel_turn` stays
    /// idempotent (a second call finds the same token, already tripped) rather
    /// than racing the guard.
    pub fn cancel_turn(&self, session_id: &str) -> Option<String> {
        let turn = self
            .active_turns
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(session_id)
            // A retired entry is not a turn: cancelling a turn that already
            // ended must stay the 200 `cancelled: false` no-op it has always
            // been, not report that it stopped something.
            .filter(|turn| turn.finished_at.is_none())
            .cloned()?;
        turn.cancel.cancel();
        Some(turn.turn_id)
    }

    /// True while an interactive turn is in flight for `session_id` (the BR-33
    /// turn lock is held). BR-61 uses it to reject a soft interrupt that has no
    /// running turn to steer — queueing it on the agent would otherwise strand
    /// the text until some later turn injected it out of nowhere.
    ///
    /// A retired entry (a turn that has ended, kept only so its key can be
    /// re-POSTed into its replay) is NOT active. Every reader below filters the
    /// same way, so "the map has an entry" and "a turn is running" never drift.
    pub fn is_turn_active(&self, session_id: &str) -> bool {
        self.active_turns
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(session_id)
            .is_some_and(|turn| turn.finished_at.is_none())
    }

    /// Every session with a turn in flight right now (BR-71 CLI parity).
    ///
    /// `is_turn_active` answers for one session; a listing needs the set, and
    /// N round-trips for an N-row page is both slower and racier — the rows
    /// would be read at N different instants.
    pub fn active_turn_session_ids(&self) -> Vec<String> {
        self.active_turns
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|(_, turn)| turn.finished_at.is_none())
            .map(|(session_id, _)| session_id.clone())
            .collect()
    }

    /// The id of the turn in flight for `session_id`, if there is one.
    ///
    /// Exists so `POST /agent/resume` can hand a reloading window the turn it
    /// should re-attach to. Without it the client has to publish its own turn
    /// pointer through `localStorage` and survive a reload with it — and a
    /// pointer the client keeps is a pointer that can go stale, which makes
    /// "attached to a turn that no longer exists" a category of bug rather than
    /// an impossible state. The server always knows; asking it removes the
    /// class. Returns `None` for a retired (already finished) turn: there is
    /// nothing live to attach to.
    ///
    /// It also returns `None` for a turn whose stream has no WRITER, and that
    /// filter is not an optimisation. Several callers take this same turn lock
    /// for reasons that have nothing to do with streaming — an in-place edit and
    /// a working-directory change hold it as a plain mutex, and the workspace
    /// and app turn runners hold it without a `/reply` pump. Advertising one of
    /// those as attachable told a reloading window to follow a log that nothing
    /// will ever write to and nothing will ever close: the window parked on it,
    /// set `chatState: Streaming`, and its composer was dead until the user
    /// reloaded. What a client may attach to and what a pump is writing are the
    /// same set, by construction, here.
    pub fn active_turn_id(&self, session_id: &str) -> Option<String> {
        self.active_turns
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(session_id)
            .filter(|turn| turn.finished_at.is_none() && turn.stream.has_writer())
            .map(|turn| turn.turn_id.clone())
    }

    /// Does this session hold a turn — running or retained for replay — that
    /// `turn_id` names?
    ///
    /// Read-only, and that is the point: `/reply` uses it to tell an ATTACH
    /// whose turn is gone from a first POST, and it must be able to ask without
    /// minting a turn entry to find out. Matches EITHER name, exactly as
    /// [`Self::try_begin_turn_idempotent`] does — the client's own idempotency
    /// key, or the server-assigned `turn-N` a reloaded window read off a frame.
    pub fn knows_turn(&self, session_id: &str, turn_id: &str) -> bool {
        self.active_turns
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(session_id)
            .is_some_and(|turn| {
                turn.turn_id == turn_id || turn.idempotency_key.as_deref() == Some(turn_id)
            })
    }

    pub fn has_active_turns(&self) -> bool {
        self.active_turns
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .any(|turn| turn.finished_at.is_none())
    }

    pub async fn clear_cached_agents(&self) -> usize {
        let tasks = {
            let mut tasks = self.extension_loading_tasks.lock().await;
            tasks.drain().map(|(_, task)| task).collect::<Vec<_>>()
        };
        for task in tasks {
            if let Some(handle) = task.lock().await.take() {
                handle.abort();
            }
        }
        self.workflow_session_tracker.lock().await.clear();
        self.agent_manager.clear_sessions().await
    }

    pub async fn set_extension_loading_task(
        &self,
        session_id: String,
        task: JoinHandle<Vec<ExtensionLoadResult>>,
    ) {
        let mut tasks = self.extension_loading_tasks.lock().await;
        tasks.insert(session_id, Arc::new(Mutex::new(Some(task))));
    }

    /// How long a turn will wait for a session's extensions before going ahead
    /// without them.
    ///
    /// Generous, because spawning stdio MCP servers is genuinely slow and the
    /// normal wait is ~300 ms. It is a deadlock bound, not a performance tuning
    /// knob: what it rules out is one wedged extension parking every turn in the
    /// session forever. `cdwagent` and `medcp` fail to load on a machine without
    /// UCSF credentials, so this is the ordinary case, not the exotic one.
    const EXTENSION_LOAD_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

    pub async fn take_extension_loading_task(
        &self,
        session_id: &str,
    ) -> Option<Vec<ExtensionLoadResult>> {
        self.take_extension_loading_task_bounded(session_id, Self::EXTENSION_LOAD_WAIT)
            .await
    }

    /// The same wait with an explicit bound, so a test can prove the bound
    /// EXISTS without sitting through the production one. A 30 second unit test
    /// is a test somebody eventually deletes.
    async fn take_extension_loading_task_bounded(
        &self,
        session_id: &str,
        wait: std::time::Duration,
    ) -> Option<Vec<ExtensionLoadResult>> {
        let task_holder = {
            let tasks = self.extension_loading_tasks.lock().await;
            tasks.get(session_id).cloned()
        };

        if let Some(holder) = task_holder {
            let task = holder.lock().await.take();
            if let Some(handle) = task {
                // ⚠ BOUNDED. An unbounded `handle.await` here turns a slow
                // extension into a HUNG PRODUCT, and that is not hypothetical:
                // it wedged a live app for hours. `/reply` runs this while
                // holding the session's turn guard, so one extension that never
                // finishes loading parks the turn task forever and every later
                // turn then blocks on the lock. Symptom set, all of which point
                // away from the cause: no outbound network, ~0% CPU, every
                // tokio worker in `kevent`, `/active_work` empty, HTTP reads
                // still 200 in 2 ms, and NO turn frame in `sample` (a parked
                // task has no thread stack). The GUI shows "Thinking" forever
                // over a daemon that holds no turn at all.
                //
                // Timing out is the pre-D2 behaviour and it is safe: the point
                // of the wait was only to stop a turn racing ahead of a load
                // that is about to finish in ~300 ms. Waiting an unbounded time
                // for one that never will buys nothing and costs everything.
                //
                // Dropping the handle DETACHES the task, it does not abort it,
                // so a genuinely slow extension still finishes loading in the
                // background and the next turn sees it.
                match tokio::time::timeout(wait, handle).await {
                    Ok(Ok(results)) => return Some(results),
                    Ok(Err(e)) => {
                        tracing::warn!("Background extension loading task failed: {}", e);
                    }
                    Err(_) => {
                        tracing::warn!(
                            session_id,
                            timeout_secs = wait.as_secs(),
                            "extensions still loading after the wait; continuing \
                             with whatever has loaded so far. A turn may see a \
                             partial toolset."
                        );
                    }
                }
            }
        }
        None
    }

    pub async fn remove_extension_loading_task(&self, session_id: &str) {
        let mut tasks = self.extension_loading_tasks.lock().await;
        tasks.remove(session_id);
    }

    pub fn scheduler(&self) -> Arc<dyn SchedulerTrait> {
        self.agent_manager.scheduler()
    }

    pub fn session_manager(&self) -> &SessionManager {
        self.agent_manager.session_manager()
    }

    pub async fn set_workflow_file_hash_map(&self, hash_map: HashMap<String, PathBuf>) {
        let mut map = self.workflow_file_hash_map.lock().await;
        *map = hash_map;
    }

    pub async fn mark_workflow_run_if_absent(&self, session_id: &str) -> bool {
        let mut sessions = self.workflow_session_tracker.lock().await;
        if sessions.contains(session_id) {
            false
        } else {
            sessions.insert(session_id.to_string());
            true
        }
    }

    pub async fn get_agent(
        &self,
        session_id: String,
    ) -> anyhow::Result<Arc<biorouter::agents::Agent>> {
        self.agent_manager.get_or_create_agent(session_id).await
    }

    /// Look up a live agent for `session_id` **without creating one**.
    ///
    /// ⚠ **The route that INSPECTS a chat wants this, never [`Self::get_agent`].**
    /// That one is `AgentManager::get_or_create_agent`, and its miss path mints a
    /// bare agent bound to the process default provider with no extensions, then
    /// caches it under the session id — so an inspection reads today's global
    /// config instead of the chat's, gets an empty answer, and leaves a
    /// placeholder behind for whoever asks next. `AgentManager::peek_agent`'s own
    /// doc records that hazard; this is the accessor that makes it avoidable from
    /// a route.
    ///
    /// `None` is a real answer — a daemon restart or an LRU eviction — and a
    /// caller must say so rather than reporting it as an empty chat.
    pub async fn peek_agent(&self, session_id: &str) -> Option<Arc<biorouter::agents::Agent>> {
        self.agent_manager.peek_agent(session_id).await
    }

    /// The agent for a route that is about to DO something with it.
    ///
    /// ⚠ **Waits for the session's extensions first, and that wait is the
    /// point.** `/agent/start` kicks extension loading off in the background
    /// and returns 200 immediately, so for roughly 300 ms a session reports a
    /// couple of tools before settling to its full set. Measured 4/4, and under
    /// concurrent starts a session became "ready" holding 10 of 116 tools.
    ///
    /// A turn running inside that window silently gets a degraded toolset with
    /// no `subagent`, so the model answers "I cannot delegate" — which is
    /// **indistinguishable from the legitimate condition-5 refusal** (no
    /// non-injected extensions). A live stress pass corrupted one of its own
    /// batches on this before it added a client-side readiness wait, and no
    /// client should have to.
    ///
    /// `/agent/resume` already awaited the same handle; this puts the wait at
    /// the choke point every turn-running route passes through instead, so a
    /// new route cannot forget it.
    pub async fn get_agent_for_route(
        &self,
        session_id: String,
    ) -> Result<Arc<biorouter::agents::Agent>, StatusCode> {
        // Best-effort: a load that failed is reported by the route that started
        // it, and a turn on a partially-loaded session is still better than a
        // 500 here. What matters is that we do not run BEFORE it settles.
        self.take_extension_loading_task(&session_id).await;
        self.remove_extension_loading_task(&session_id).await;

        self.get_agent(session_id).await.map_err(|e| {
            tracing::error!("Failed to get agent: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Begin a turn with a throwaway token and no idempotency key.
    fn begin(state: &AppState, session_id: &str) -> Result<TurnGuard, TurnConflict> {
        state.try_begin_turn_idempotent(session_id, CancellationToken::new(), None)
    }

    /// D2. A route that is about to USE an agent waits for that session's
    /// extensions to finish loading.
    ///
    /// ⚠ This asserts a *wait*, which is why it uses a flag flipped by the task
    /// rather than a duration: a timing assertion would pass on a slow machine
    /// with the wait deleted. Delete either line in `get_agent_for_route` and
    /// the flag is still false when the assertion runs.
    ///
    /// The scenario it stands for: `/agent/start` returns 200 while extensions
    /// are still loading, the client replies immediately, and the turn runs
    /// with a couple of tools instead of its full set — so the model answers
    /// "I cannot delegate", which reads exactly like a real refusal.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_route_waits_for_the_sessions_extensions_before_using_the_agent() {
        let state = AppState::new().await.unwrap();
        let loaded = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let flag = loaded.clone();
        state
            .set_extension_loading_task(
                "s-wait".to_string(),
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    flag.store(true, std::sync::atomic::Ordering::SeqCst);
                    Vec::new()
                }),
            )
            .await;

        // The agent itself may or may not build here; what is under test is the
        // ordering, so the result is deliberately ignored.
        let _ = state.get_agent_for_route("s-wait".to_string()).await;

        assert!(
            loaded.load(std::sync::atomic::Ordering::SeqCst),
            "returned before the session's extensions had finished loading"
        );
    }

    /// A load that never finishes must NOT park the turn forever.
    ///
    /// ⚠ This is the guard for a real outage: an unbounded `handle.await`
    /// wedged a live daemon for hours. `/reply` waits while holding the
    /// session's turn guard, so one extension that never settles parks the turn
    /// task and every later turn blocks behind the lock. It presents as a
    /// daemon with no network, no CPU and no turn at all, under a GUI that says
    /// "Thinking" indefinitely.
    ///
    /// The test drives a handle that never completes, so it hangs forever if
    /// the timeout is removed rather than failing an assertion. That is the
    /// point: nothing weaker distinguishes "bounded" from "fast enough today".
    #[tokio::test(flavor = "multi_thread")]
    async fn a_wedged_extension_load_does_not_park_the_turn_forever() {
        let state = AppState::new().await.unwrap();
        state
            .set_extension_loading_task(
                "s-wedged".to_string(),
                // Never completes, and is never aborted: exactly a stdio MCP
                // server that spawned and then went silent.
                tokio::spawn(std::future::pending()),
            )
            .await;

        // A short bound, then an outer limit well above it: the inner one is
        // what must fire. Remove the `timeout` in the production path and the
        // outer one trips instead, failing this.
        let waited = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            state.take_extension_loading_task_bounded(
                "s-wedged",
                std::time::Duration::from_millis(150),
            ),
        )
        .await;

        assert!(
            waited.is_ok(),
            "a wedged extension load parked the turn: the wait is unbounded"
        );

        // And the production path really does pass a bound, not `Duration::MAX`.
        // Without this, the test above passes against a wait that is
        // technically finite and effectively forever.
        assert!(
            AppState::EXTENSION_LOAD_WAIT <= std::time::Duration::from_secs(60),
            "the production wait is too long to be a deadlock bound"
        );
    }

    /// The wait is not a leak: the handle is consumed once and the entry drops,
    /// so the second turn of a chat does not re-await anything.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_extension_wait_is_consumed_once() {
        let state = AppState::new().await.unwrap();
        state
            .set_extension_loading_task("s-once".to_string(), tokio::spawn(async { Vec::new() }))
            .await;

        let _ = state.get_agent_for_route("s-once".to_string()).await;
        assert!(
            state.take_extension_loading_task("s-once").await.is_none(),
            "the loading handle outlived the turn that awaited it"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_try_begin_turn_rejects_second_and_recovers_on_drop() {
        let state = AppState::new().await.unwrap();

        let guard = begin(&state, "s1").expect("first turn acquires the lock");

        // A second turn for the same session is rejected with the running id.
        let running = begin(&state, "s1").unwrap_err().running_turn_id;
        assert!(running.starts_with("turn-"), "got id {running}");

        // A different session is independent.
        let _other = begin(&state, "s2").expect("distinct session is unaffected");

        // Dropping the guard releases the session for the next turn.
        drop(guard);
        let _next = begin(&state, "s1").expect("session is free after the guard drops");
    }

    /// BR-62: cancel is addressable by session id. Before, the running turn's
    /// token lived only inside the `/reply` task, so nothing outside that socket
    /// could stop the turn.
    #[tokio::test(flavor = "multi_thread")]
    async fn cancel_turn_trips_the_running_turns_token() {
        let state = AppState::new().await.unwrap();
        let token = CancellationToken::new();

        let _guard = state
            .try_begin_turn_idempotent("s1", token.clone(), None)
            .expect("turn starts");
        assert!(!token.is_cancelled());

        let cancelled = state.cancel_turn("s1").expect("a turn was running");
        assert!(cancelled.starts_with("turn-"), "got id {cancelled}");
        assert!(token.is_cancelled(), "the running turn's token was tripped");
    }

    /// Cancelling a session with no turn in flight — a double-clicked Stop, a
    /// cancel that raced the turn's own completion — is a no-op, never an error.
    #[tokio::test(flavor = "multi_thread")]
    async fn cancel_turn_is_idempotent_and_safe_when_idle() {
        let state = AppState::new().await.unwrap();
        let token = CancellationToken::new();

        assert!(state.cancel_turn("s1").is_none(), "nothing to cancel yet");

        let guard = state
            .try_begin_turn_idempotent("s1", token.clone(), None)
            .expect("turn starts");

        // Cancelling twice is fine: the second call finds the same, already
        // tripped token rather than racing the guard's cleanup.
        assert!(state.cancel_turn("s1").is_some());
        assert!(state.cancel_turn("s1").is_some());
        assert!(token.is_cancelled());

        // Once the turn task unwinds and drops its guard, the slot is clear.
        drop(guard);
        assert!(state.cancel_turn("s1").is_none());
    }

    /// A re-POST of the same turn (an SSE reconnect) is recognizable as a
    /// duplicate, so the client can re-attach instead of treating it as a hard
    /// conflict — and, either way, no second turn starts.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_repost_of_the_same_turn_id_is_flagged_duplicate() {
        let state = AppState::new().await.unwrap();

        let _guard = state
            .try_begin_turn_idempotent("s1", CancellationToken::new(), Some("turn-abc".into()))
            .expect("turn starts");

        let conflict = state
            .try_begin_turn_idempotent("s1", CancellationToken::new(), Some("turn-abc".into()))
            .expect_err("no second turn starts");
        assert!(conflict.duplicate, "same key => same turn, re-delivered");

        // A genuinely different turn is a real conflict, not a duplicate.
        let conflict = state
            .try_begin_turn_idempotent("s1", CancellationToken::new(), Some("turn-xyz".into()))
            .expect_err("no second turn starts");
        assert!(!conflict.duplicate);
    }

    /// Two keyless turns are two turns — absence of a key must not be mistaken
    /// for a matching key, or every concurrent `/reply` would look like a retry.
    #[tokio::test(flavor = "multi_thread")]
    async fn keyless_turns_are_never_duplicates_of_each_other() {
        let state = AppState::new().await.unwrap();

        let _guard = state
            .try_begin_turn_idempotent("s1", CancellationToken::new(), None)
            .expect("turn starts");

        let conflict = state
            .try_begin_turn_idempotent("s1", CancellationToken::new(), None)
            .expect_err("no second turn starts");
        assert!(!conflict.duplicate);
    }

    #[tokio::test]
    async fn turn_guard_exposes_its_turn_id() {
        let state = AppState::new().await.unwrap();
        let guard = state
            .try_begin_turn_idempotent("tg-id-test", CancellationToken::new(), None)
            .unwrap();
        assert!(guard.turn_id().starts_with("turn-"));
    }

    /// A guard must also say WHICH session it locked.
    ///
    /// BR-71's runner takes a `TurnRequest` and a `TurnGuard` as separate
    /// arguments, and Tasks 8 and 14 both acquire the guard themselves before
    /// calling `run_turn`. Without an accessor there is nothing — not a type,
    /// not an assertion — stopping `run_turn(state, request_for_B, guard_for_A)`
    /// from compiling and running an unguarded turn on B while holding A's lock.
    #[tokio::test]
    async fn turn_guard_exposes_the_session_it_locked() {
        let state = AppState::new().await.unwrap();
        let guard = state
            .try_begin_turn_idempotent("tg-session-test", CancellationToken::new(), None)
            .unwrap();
        assert_eq!(guard.session_id(), "tg-session-test");
    }

    /// A turn that has ENDED is retired, not deleted — its entry survives so a
    /// re-POST of its idempotency key attaches to the replay instead of starting
    /// a second turn. But a retired entry must be invisible to every "is a turn
    /// running?" reader, or the session would look permanently busy: no new
    /// turn, no steer, and a Stop button reporting it stopped something.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_retired_turn_blocks_nothing_and_is_not_reported_as_running() {
        let state = AppState::new().await.unwrap();
        let token = CancellationToken::new();

        let guard = state
            .try_begin_turn_idempotent("retired", token.clone(), Some("k-1".into()))
            .expect("turn starts");
        assert!(state.is_turn_active("retired"));
        drop(guard);

        assert!(
            !state.is_turn_active("retired"),
            "a finished turn is not active"
        );
        assert!(state.active_turn_id("retired").is_none());
        assert!(!state
            .active_turn_session_ids()
            .contains(&"retired".to_string()));
        assert!(
            state.cancel_turn("retired").is_none(),
            "nothing left to stop"
        );

        // The same key still names the finished turn, so a reconnect is answered
        // from its replay rather than re-running it.
        let conflict = state
            .try_begin_turn_idempotent("retired", CancellationToken::new(), Some("k-1".into()))
            .expect_err("the retired turn is still addressable by its key");
        assert!(conflict.duplicate && conflict.finished);
        // The guard deliberately did NOT close the log — closing there would
        // race the pump's read of the runner's own terminal frame. Closing is
        // the pump's job, or (for a turn with no pump) the retired-attach path's.
        assert!(!conflict.stream.is_closed());

        // A DIFFERENT key is simply the next message, and starts a fresh turn.
        let next = state
            .try_begin_turn_idempotent("retired", CancellationToken::new(), Some("k-2".into()))
            .expect("a retired entry must never block the next turn");
        assert_ne!(next.turn_id(), conflict.running_turn_id);
        assert!(state.is_turn_active("retired"));
    }

    /// The server-assigned id is an attach pointer too — it is what a reloaded
    /// window holds, since it never kept the key it originally chose.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_turn_is_addressable_by_its_server_assigned_id() {
        let state = AppState::new().await.unwrap();
        let guard = state
            .try_begin_turn_idempotent("by-server-id", CancellationToken::new(), Some("k".into()))
            .expect("turn starts");
        let server_id = guard.turn_id().to_string();

        let conflict = state
            .try_begin_turn_idempotent(
                "by-server-id",
                CancellationToken::new(),
                Some(server_id.clone()),
            )
            .expect_err("no second turn starts");
        assert!(
            conflict.duplicate,
            "the id a reloaded window actually holds must identify the turn"
        );
        assert_eq!(conflict.stream.turn_id(), server_id);
    }

    /// A guard may only ever clear its own turn. If a guard outlived its slot and
    /// removed a successor's entry, the session would look idle while a turn ran.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_stale_guard_cannot_clear_a_successors_turn() {
        let state = AppState::new().await.unwrap();

        let first = state
            .try_begin_turn_idempotent("s1", CancellationToken::new(), None)
            .expect("first turn starts");
        drop(first);

        let second_token = CancellationToken::new();
        let _second = state
            .try_begin_turn_idempotent("s1", second_token.clone(), None)
            .expect("second turn starts");

        // The second turn still owns the slot and is still cancellable.
        assert!(state.is_turn_active("s1"));
        assert!(state.cancel_turn("s1").is_some());
        assert!(second_token.is_cancelled());
    }
}
