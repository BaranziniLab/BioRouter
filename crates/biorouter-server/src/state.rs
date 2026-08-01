use axum::http::StatusCode;
use biorouter::execution::manager::AgentManager;
use biorouter::scheduler_trait::SchedulerTrait;
use biorouter::session::SessionManager;
use biorouter_mcp::knowledge::service::KnowledgeService;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::tunnel::TunnelManager;
use biorouter::agents::ExtensionLoadResult;

type ExtensionLoadingTasks =
    Arc<Mutex<HashMap<String, Arc<Mutex<Option<JoinHandle<Vec<ExtensionLoadResult>>>>>>>>;

/// Process-wide monotonic turn id source. Ids are only used to identify the
/// in-flight turn a rejected concurrent `/reply` collided with.
static TURN_SEQ: AtomicU64 = AtomicU64::new(1);

/// The turn currently in flight for a session.
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
}

/// RAII guard marking that a session has an interactive turn in flight. Held by
/// the `/reply` task and removed from the active-turns map when dropped (turn
/// completes, errors, or is cancelled), so the next `/reply` for that session
/// can proceed. See `AppState::try_begin_turn`.
#[derive(Debug)]
pub struct TurnGuard {
    session_id: String,
    /// The turn this guard owns. Checked on drop so a guard can only ever remove
    /// *its own* entry, never a successor's.
    turn_id: String,
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
}

impl Drop for TurnGuard {
    fn drop(&mut self) {
        let mut turns = self
            .active_turns
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Only clear the slot if it is still ours.
        if turns
            .get(&self.session_id)
            .is_some_and(|turn| turn.turn_id == self.turn_id)
        {
            turns.remove(&self.session_id);
        }
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
            tunnel_manager,
            extension_loading_tasks: Arc::new(Mutex::new(HashMap::new())),
            knowledge_service,
        }))
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
    /// flagged `duplicate` and the client knows its turn is simply still running.
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

        if let Some(running) = turns.get(session_id) {
            // A key only marks a duplicate when the client actually supplied one
            // *and* it names the running turn. Two keyless turns are two turns.
            let duplicate = idempotency_key.is_some() && idempotency_key == running.idempotency_key;
            return Err(TurnConflict {
                running_turn_id: running.turn_id.clone(),
                duplicate,
            });
        }

        let turn_id = format!("turn-{}", TURN_SEQ.fetch_add(1, Ordering::Relaxed));
        turns.insert(
            session_id.to_string(),
            ActiveTurn {
                turn_id: turn_id.clone(),
                idempotency_key,
                cancel,
            },
        );
        Ok(TurnGuard {
            session_id: session_id.to_string(),
            turn_id,
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
            .cloned()?;
        turn.cancel.cancel();
        Some(turn.turn_id)
    }

    /// True while an interactive turn is in flight for `session_id` (the BR-33
    /// turn lock is held). BR-61 uses it to reject a soft interrupt that has no
    /// running turn to steer — queueing it on the agent would otherwise strand
    /// the text until some later turn injected it out of nowhere.
    pub fn is_turn_active(&self, session_id: &str) -> bool {
        self.active_turns
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains_key(session_id)
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
            .keys()
            .cloned()
            .collect()
    }

    pub fn has_active_turns(&self) -> bool {
        !self
            .active_turns
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_empty()
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

    pub async fn take_extension_loading_task(
        &self,
        session_id: &str,
    ) -> Option<Vec<ExtensionLoadResult>> {
        let task_holder = {
            let tasks = self.extension_loading_tasks.lock().await;
            tasks.get(session_id).cloned()
        };

        if let Some(holder) = task_holder {
            let task = holder.lock().await.take();
            if let Some(handle) = task {
                match handle.await {
                    Ok(results) => return Some(results),
                    Err(e) => {
                        tracing::warn!("Background extension loading task failed: {}", e);
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

    pub async fn get_agent_for_route(
        &self,
        session_id: String,
    ) -> Result<Arc<biorouter::agents::Agent>, StatusCode> {
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
