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

use crate::tunnel::TunnelManager;
use biorouter::agents::ExtensionLoadResult;

type ExtensionLoadingTasks =
    Arc<Mutex<HashMap<String, Arc<Mutex<Option<JoinHandle<Vec<ExtensionLoadResult>>>>>>>>;

/// Process-wide monotonic turn id source. Ids are only used to identify the
/// in-flight turn a rejected concurrent `/reply` collided with.
static TURN_SEQ: AtomicU64 = AtomicU64::new(1);

/// RAII guard marking that a session has an interactive turn in flight. Held by
/// the `/reply` task and removed from the active-turns map when dropped (turn
/// completes, errors, or is cancelled), so the next `/reply` for that session
/// can proceed. See `AppState::try_begin_turn`.
#[derive(Debug)]
pub struct TurnGuard {
    session_id: String,
    active_turns: Arc<StdMutex<HashMap<String, String>>>,
}

impl Drop for TurnGuard {
    fn drop(&mut self) {
        if let Ok(mut turns) = self.active_turns.lock() {
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
    /// Sessions with an interactive turn in flight, mapped to that turn's id.
    /// Enforces one turn per session at the server so a second `/reply` can't
    /// race a shared `Arc<Agent>` (confirmation channel, soft-interrupt queue,
    /// check-compact-persist) with the running turn.
    active_turns: Arc<StdMutex<HashMap<String, String>>>,
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

    /// Try to begin an interactive turn for `session_id`. Returns a `TurnGuard`
    /// that keeps the session marked busy until dropped, or `Err(running_turn_id)`
    /// if a turn is already in flight for that session — the caller should reject
    /// the duplicate `/reply` rather than start a second turn on the shared agent.
    pub fn try_begin_turn(&self, session_id: &str) -> Result<TurnGuard, String> {
        let mut turns = self
            .active_turns
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(running_turn_id) = turns.get(session_id) {
            return Err(running_turn_id.clone());
        }
        let turn_id = format!("turn-{}", TURN_SEQ.fetch_add(1, Ordering::Relaxed));
        turns.insert(session_id.to_string(), turn_id);
        Ok(TurnGuard {
            session_id: session_id.to_string(),
            active_turns: Arc::clone(&self.active_turns),
        })
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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_try_begin_turn_rejects_second_and_recovers_on_drop() {
        let state = AppState::new().await.unwrap();

        let guard = state
            .try_begin_turn("s1")
            .expect("first turn acquires the lock");

        // A second turn for the same session is rejected with the running id.
        let running = state.try_begin_turn("s1").unwrap_err();
        assert!(running.starts_with("turn-"), "got id {running}");

        // A different session is independent.
        let _other = state
            .try_begin_turn("s2")
            .expect("distinct session is unaffected");

        // Dropping the guard releases the session for the next turn.
        drop(guard);
        let _next = state
            .try_begin_turn("s1")
            .expect("session is free after the guard drops");
    }
}
