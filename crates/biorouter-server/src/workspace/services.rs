//! The daemon's `WorkspaceServices` implementation over `AppState` (BR-71).
//! GUI methods are wired in Slice 2 (Task 23); until then they report headless.

use std::path::PathBuf;
use std::sync::Arc;

use biorouter::config::{get_enabled_extensions, get_extension_by_name};
use biorouter::conversation::message::Message;
use biorouter::session::session_manager::SessionType;
// `ExtensionState` is imported for its PROVIDED METHOD `to_extension_data`,
// called on `extensions_state` in `start_session`. A trait method is only
// callable with the trait in scope, so importing `EnabledExtensionsState` alone
// is E0599. `routes/agent.rs` imports it exactly this way.
use biorouter::session::extension_data::ExtensionState;
use biorouter::session::EnabledExtensionsState;
// `WorkspaceTurnLease` is in this list because `begin_turn` names it in its
// return type AND constructs `ServerTurnLease` below — importing only
// `WorkspaceServices` is an E0412 the moment `begin_turn` is written.
//
// `KbPrimaryChoice` and `KbSelectionView` are here for exactly the same reason:
// `KbPrimaryChoice` in `start_session` and `set_knowledge_bases`,
// `KbSelectionView` in `set_knowledge_bases`'s return type, its constructed
// value, and `knowledge_selection`.
use biorouter::workspace_services::{
    KbPrimaryChoice, KbSelectionView, WorkspaceServices, WorkspaceTurnLease,
};

use crate::state::AppState;

pub struct ServerWorkspaceServices {
    state: Arc<AppState>,
}

impl ServerWorkspaceServices {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

#[async_trait::async_trait]
impl WorkspaceServices for ServerWorkspaceServices {
    fn gui_attached(&self) -> bool {
        false // Slice 2 (Task 23) wires the WorkspaceBridge registry here.
    }

    fn layout_snapshot(&self) -> Option<serde_json::Value> {
        None // Slice 2.
    }

    fn is_turn_active(&self, session_id: &str) -> bool {
        self.state.is_turn_active(session_id)
    }

    fn cancel_turn(&self, session_id: &str) -> Option<String> {
        self.state.cancel_turn(session_id)
    }

    fn begin_turn(
        &self,
        session_id: &str,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<Box<dyn WorkspaceTurnLease>, String> {
        let guard = self
            .state
            .try_begin_turn_idempotent(session_id, cancel, None)
            .map_err(|conflict| {
                format!(
                    "a turn is already in flight for this session (running turn {})",
                    conflict.running_turn_id
                )
            })?;
        Ok(Box::new(ServerTurnLease { guard }))
    }

    async fn stop_agent(&self, session_id: &str) -> Result<(), String> {
        // Mirror POST /agent/stop: cancel the turn, then evict — the session
        // record remains.
        let _ = self.state.cancel_turn(session_id);
        self.state
            .agent_manager
            .remove_session(session_id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn start_detached_turn(
        &self,
        session_id: &str,
        message: Message,
    ) -> Result<String, String> {
        // HYDRATE FIRST. A target the user has not opened this run has no live
        // agent, and `get_agent` (inside `start_turn`) creates a BARE one: no
        // extensions, and NO PROVIDER — `AgentManager::default_provider` has no
        // production setter, so `Agent::provider()` returns
        // `Err("Provider not set")` and the injected turn dies on its first
        // step. This mirrors what `/agent/resume` and `restart_agent_internal`
        // do, and it is what makes `workspace_send_prompt mode:"turn"` work on
        // exactly the sessions the tool exists to reach. Without it the turn
        // would also run with none of the tools `workspace_list` reports the
        // target as having.
        let session = self
            .state
            .session_manager()
            .get_session(session_id, false)
            .await
            .map_err(|e| e.to_string())?;
        let agent = self
            .state
            .get_agent(session_id.to_string())
            .await
            .map_err(|e| e.to_string())?;
        let (provider_result, _extension_results) = tokio::join!(
            agent.restore_provider_from_session(&session),
            agent.load_extensions_from_session(&session),
        );
        provider_result.map_err(|e| e.to_string())?;

        // The ONE turn runner (Task 6). An injected turn and a `/reply` turn
        // differ only in their `TurnExtras`.
        super::turn::start_turn(
            self.state.clone(),
            super::turn::TurnRequest::new(session_id.to_string(), message),
        )
        .await
        .map(|started| started.turn_id)
        .map_err(|e| e.to_string())
    }

    async fn start_session(
        &self,
        working_dir: PathBuf,
        extensions: Option<Vec<String>>,
        knowledge_bases: Vec<String>,
        primary: KbPrimaryChoice,
    ) -> Result<String, String> {
        // The minimal core of POST /agent/start (`start_agent`):
        // create → apply extension set → persist → eager-load in background.
        let configs = match extensions {
            None => get_enabled_extensions(),
            Some(names) => {
                let mut configs = Vec::with_capacity(names.len());
                for name in &names {
                    match get_extension_by_name(name) {
                        Some(c) => configs.push(c),
                        None => return Err(format!("unknown extension '{name}'")),
                    }
                }
                configs
            }
        };

        let manager = self.state.session_manager();
        let session = manager
            .create_session(working_dir, "New Session".to_string(), SessionType::User)
            .await
            .map_err(|e| format!("failed to create session: {e}"))?;

        let mut extension_data = session.extension_data.clone();
        let extensions_state = EnabledExtensionsState::new(configs);
        extensions_state
            .to_extension_data(&mut extension_data)
            .map_err(|e| format!("failed to initialize extensions: {e}"))?;
        manager
            .update(&session.id)
            .extension_data(extension_data)
            .apply()
            .await
            .map_err(|e| format!("failed to save extension state: {e}"))?;

        if !knowledge_bases.is_empty() {
            // A fresh session has no primary of its own, so `Auto` here always
            // resolves to "pin the first id" — which is what makes the new
            // session's KB-less writes work at all. Passing an explicit
            // `Set(id)` from `workspace_open` is honoured unchanged.
            self.set_knowledge_bases(&session.id, &knowledge_bases, primary)?;
        }

        // Eager extension load, exactly as start_agent does.
        let state = self.state.clone();
        let session_for_spawn = manager
            .get_session(&session.id, false)
            .await
            .map_err(|e| e.to_string())?;
        let sid = session.id.clone();
        let task = tokio::spawn(async move {
            match state.get_agent(session_for_spawn.id.clone()).await {
                Ok(agent) => agent.load_extensions_from_session(&session_for_spawn).await,
                Err(e) => {
                    tracing::warn!("workspace start_session: agent create failed: {e}");
                    vec![]
                }
            }
        });
        self.state.set_extension_loading_task(sid, task).await;

        Ok(session.id)
    }

    fn set_knowledge_bases(
        &self,
        session_id: &str,
        kbs: &[String],
        primary: KbPrimaryChoice,
    ) -> Result<KbSelectionView, String> {
        // Same path `state.rs` already uses for `KnowledgeService` —
        // `biorouter-server` depends on `biorouter-mcp` directly.
        use biorouter_mcp::knowledge::service::PrimaryUpdate;

        // THE ONE SEAM where the subtractive model and the borrowed
        // `PrimaryUpdate` are visible. `set_visible_kbs` takes the ids that
        // should be VISIBLE and inverts them into the stored hidden list itself,
        // under the root lock — which is exactly why we call it rather than
        // reading the hidden list, editing it and writing it back: that
        // read-modify-write across two unlocked calls is a documented race where
        // two surfaces each write a list computed before the other's edit and
        // one silently disappears.
        //
        // Resolve `Auto` HERE, not in the tools, so every surface gets the same
        // answer. `Unchanged` is safe for the keep-it case because the service
        // re-validates membership against the resulting set and repairs the
        // pointer itself when the old target has just been hidden.
        let current = self
            .state
            .knowledge_service
            .primary_for_session(Some(session_id))
            .unwrap_or_default();
        let auto_pick: Option<String> = match &current {
            Some(id) if kbs.iter().any(|k| k == id) => None, // keep it
            _ => kbs.first().cloned(),
        };
        let update = match &primary {
            KbPrimaryChoice::Set(id) => PrimaryUpdate::Set(id.as_str()),
            KbPrimaryChoice::Clear => PrimaryUpdate::Clear,
            KbPrimaryChoice::Auto => match auto_pick.as_deref() {
                Some(id) => PrimaryUpdate::Set(id),
                // Either the current primary is still a member (keep it), or the
                // new set is empty and there is nothing legal to point at.
                None if current.is_some() && !kbs.is_empty() => PrimaryUpdate::Unchanged,
                None => PrimaryUpdate::Clear,
            },
        };

        let selection = self
            .state
            .knowledge_service
            .set_visible_kbs(Some(session_id), kbs, update)
            .map_err(|e| e.to_string())?;
        Ok(KbSelectionView {
            kb_ids: selection.kb_ids,
            primary_kb: selection.primary_kb,
        })
    }

    fn knowledge_selection(&self, session_id: &str) -> KbSelectionView {
        // `KnowledgeService::selection` takes the root lock and returns set +
        // pointer TOGETHER. Do not rebuild this from `session_kb_ids` +
        // `primary_for_session`: composing two unlocked reads is what made the
        // "primary is a member of the set" claim false whenever a writer landed
        // between them.
        //
        // Best-effort — a read error reports "no KBs", never fails a list.
        self.state
            .knowledge_service
            .selection(Some(session_id))
            .map(|s| KbSelectionView {
                kb_ids: s.kb_ids,
                primary_kb: s.primary_kb,
            })
            .unwrap_or_default()
    }

    async fn gui_command(
        &self,
        _frame: serde_json::Value,
        _wait_result: bool,
    ) -> Result<serde_json::Value, String> {
        Err("no GUI attached".to_string()) // Slice 2.
    }
}

/// The daemon's lease: a wrapped `TurnGuard`. Dropping it releases the session's
/// turn slot exactly as the /reply task's guard does.
struct ServerTurnLease {
    guard: crate::state::TurnGuard,
}

impl biorouter::workspace_services::WorkspaceTurnLease for ServerTurnLease {
    fn turn_id(&self) -> &str {
        self.guard.turn_id()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use biorouter::workspace_services::WorkspaceServices;

    #[tokio::test]
    async fn start_session_creates_a_user_session_and_rejects_unknown_extensions() {
        let state = crate::state::AppState::new().await.unwrap();
        let services = ServerWorkspaceServices::new(state.clone());
        let temp = tempfile::TempDir::new().unwrap();

        let err = services
            .start_session(
                temp.path().to_path_buf(),
                Some(vec!["no-such-ext".into()]),
                Vec::new(),
                KbPrimaryChoice::Auto,
            )
            .await
            .unwrap_err();
        assert!(err.contains("no-such-ext"));

        let sid = services
            .start_session(
                temp.path().to_path_buf(),
                None,
                Vec::new(),
                KbPrimaryChoice::Auto,
            )
            .await
            .unwrap();
        let session = state
            .session_manager()
            .get_session(&sid, false)
            .await
            .unwrap();
        assert_eq!(
            session.session_type,
            biorouter::session::session_manager::SessionType::User
        );
    }

    #[tokio::test]
    async fn begin_turn_lease_holds_the_lock_and_cancel_turn_trips_its_token() {
        use tokio_util::sync::CancellationToken;
        let state = crate::state::AppState::new().await.unwrap();
        let services = ServerWorkspaceServices::new(state.clone());

        let token = CancellationToken::new();
        let lease = services
            .begin_turn("lease-s1", token.clone())
            .expect("lock acquired");
        assert!(lease.turn_id().starts_with("turn-"));
        assert!(services.is_turn_active("lease-s1"));

        // A second begin_turn conflicts — the one-turn-per-session invariant.
        // Matched rather than `unwrap_err()`: the Ok arm is a
        // `Box<dyn WorkspaceTurnLease>`, which is not `Debug`, and making the
        // trait `Debug` purely to satisfy a test assertion would put a bound on
        // every future implementor for no production reason.
        let conflict = match services.begin_turn("lease-s1", CancellationToken::new()) {
            Ok(_) => panic!("a second begin_turn for a busy session must be refused"),
            Err(e) => e,
        };
        assert!(conflict.contains("already in flight"));

        // cancel_turn reaches the lease's token — this is what makes the tab's
        // Stop / workspace_close scope:"turn" work on a subagent run (Task 33).
        assert!(services.cancel_turn("lease-s1").is_some());
        assert!(token.is_cancelled());

        // Dropping the lease frees the session.
        drop(lease);
        assert!(!services.is_turn_active("lease-s1"));
    }

    /// A turn injected into a session with NO live agent must run against that
    /// session's persisted provider and extensions, not against a bare agent.
    /// Without the hydration in `start_detached_turn` this fails with
    /// "Provider not set" — `AgentManager::default_provider` is never set in the
    /// daemon, so the agent `get_agent` mints for a cold session has none. That
    /// is every session the user has not opened this run, i.e. exactly the
    /// population `workspace_send_prompt mode:"turn"` exists to reach.
    #[tokio::test]
    async fn a_turn_injected_into_a_cold_session_hydrates_it_first() {
        let state = crate::state::AppState::new().await.unwrap();
        let services = ServerWorkspaceServices::new(state.clone());
        let temp = tempfile::TempDir::new().unwrap();

        let sid = services
            .start_session(
                temp.path().to_path_buf(),
                None,
                Vec::new(),
                KbPrimaryChoice::Auto,
            )
            .await
            .unwrap();
        // Evict any agent `start_session`'s eager load created, so the next
        // resolution is a genuine cold start. Best-effort on purpose:
        // `AgentManager::remove_session` errors with "Session … not found" when
        // nothing is cached, and the eager load is a spawned task that may not
        // have reached `get_agent` yet — in which case the session is already
        // cold, which is exactly the state this line is trying to reach.
        // `restart_agent_internal` (routes/agent.rs) discards it the same way.
        let _ = state.agent_manager.remove_session(&sid).await;

        let err = services
            .start_detached_turn(&sid, Message::user().with_text("hello"))
            .await
            .err();
        // The turn may still fail for want of a configured provider on this
        // machine — but it must NOT fail with the bare-agent symptom, which is
        // what a missing hydration produces.
        if let Some(err) = err {
            assert!(
                !err.contains("Provider not set"),
                "start_detached_turn must hydrate the target from its session row: {err}"
            );
        }
    }
}
