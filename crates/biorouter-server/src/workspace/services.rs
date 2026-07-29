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
    use biorouter::session::session_manager::SessionType;
    use biorouter::workspace_services::WorkspaceServices;

    /// NOTE — what these tests share with the rest of this crate's unit tests,
    /// and what they deliberately do not.
    ///
    /// * `AppState::new()` opens the **REAL user session database** (through
    ///   `AgentManager::instance()` → `SessionManager::instance()`, both
    ///   process-global `OnceCell`s with no path seam). `workspace/turn.rs`,
    ///   `routes/session.rs`, `routes/session_events.rs` and `state.rs` all
    ///   carry the same warning: these tests create rows in the developer's own
    ///   history. Keep session names unique and never assert on row counts.
    ///   Relocating that would mean setting `BIOROUTER_PATH_ROOT` before the
    ///   process starts, which no test can do for a binary it is already
    ///   running inside — it is a crate-wide change, not a Task 9 one.
    /// * **Extensions are always passed explicitly**, never `None`. `None`
    ///   means "the developer's own enabled extension set", and loading it
    ///   walks `merge_environments` → `Config::get_secret` → the macOS
    ///   Keychain, which on an unsigned test binary blocks on an interactive
    ///   authorization dialog no test run can answer. That is not a
    ///   hypothetical: `cargo test -p biorouter-server --lib workspace::services`
    ///   hung there indefinitely (`SecKeychainFindGenericPassword` under
    ///   `Agent::load_extensions_from_session`) until this rule was applied.
    ///   `Some(vec![])` exercises the same code path with an empty set.

    #[tokio::test]
    async fn start_session_creates_a_user_session_in_the_requested_dir_and_rejects_unknown_extensions(
    ) {
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
                Some(Vec::new()),
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
        assert_eq!(session.session_type, SessionType::User);
        // #44 conformance: the REQUESTED working directory is the session's,
        // set at creation. Without this assertion an implementation that
        // ignored `working_dir` entirely — and started every workspace session
        // in the daemon's cwd — passed the gate.
        assert_eq!(
            session.working_dir.canonicalize().unwrap(),
            temp.path().canonicalize().unwrap()
        );
        // The extension set the caller asked for is what was persisted.
        let persisted = biorouter::session::EnabledExtensionsState::from_extension_data(
            &session.extension_data,
        )
        .expect("start_session persists an extension state");
        assert!(
            persisted.extensions.is_empty(),
            "asked for no extensions, got {:?}",
            persisted
                .extensions
                .iter()
                .map(|e| e.name().to_string())
                .collect::<Vec<_>>()
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
    /// session's persisted provider, not against the bare agent `get_agent`
    /// mints for a session nobody has opened this run — the population
    /// `workspace_send_prompt mode:"turn"` exists to reach.
    ///
    /// # How this test can fail, and why the obvious version cannot
    ///
    /// `start_turn` **spawns** the turn and returns `Ok(turn_id)` immediately
    /// (`turn.rs`), so a turn that dies on its first step with "Provider not
    /// set" is not visible in `start_detached_turn`'s return value at all. The
    /// first version of this test asserted `!err.contains("Provider not set")`
    /// only `if let Some(err)` — with the hydration deleted there is no error,
    /// so the assertion was skipped and the test passed on a missing feature.
    ///
    /// This version pins a provider that CANNOT be constructed and requires the
    /// resulting error. That error can only come from
    /// `restore_provider_from_session`, so it is positive evidence that the
    /// session row was read and its provider restored *before* the turn was
    /// started. Delete the hydration and this test fails with "expected the
    /// cold session's persisted provider to be restored".
    #[tokio::test]
    async fn a_turn_injected_into_a_cold_session_hydrates_it_from_its_session_row() {
        let state = crate::state::AppState::new().await.unwrap();
        let services = ServerWorkspaceServices::new(state.clone());
        let temp = tempfile::TempDir::new().unwrap();

        // Cold BY CONSTRUCTION. An earlier version built the session through
        // `start_session` and then evicted the agent its eager load had created
        // — a race with a spawned task whose result had to be discarded to keep
        // the test green, which is not proof of eviction and not proof of a cold
        // session. Never entering the registry is the same state, reached
        // deterministically.
        let session = state
            .session_manager()
            .create_session(
                temp.path().to_path_buf(),
                "br71 workspace cold hydration".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();
        assert!(
            !state.agent_manager.has_session(&session.id).await,
            "the session must start with no cached agent for this test to mean anything"
        );

        state
            .session_manager()
            .update(&session.id)
            .provider_name("br71-no-such-provider")
            .model_config(biorouter::model::ModelConfig::new("br71-no-such-model").unwrap())
            .apply()
            .await
            .unwrap();

        let err = services
            .start_detached_turn(&session.id, Message::user().with_text("hello"))
            .await
            .unwrap_err();
        assert!(
            err.contains("br71-no-such-provider"),
            "expected the cold session's persisted provider to be restored before the turn \
             started, got: {err}"
        );
        // And the failure happened before any turn was started, so nothing
        // acquired the session's turn slot and leaked it.
        assert!(!state.is_turn_active(&session.id));
    }

    /// The KB pair is the only non-trivial *decision* in this file: resolving
    /// `KbPrimaryChoice::Auto` against the RESULTING set, in one place, so every
    /// surface gets the same answer. Both methods returning
    /// `KbSelectionView::default()` — the shape of a stub — passed the gate
    /// before this test existed.
    ///
    /// Runs against a temp knowledge root (`new_with_knowledge_root`), because
    /// it creates bases and moves a write target: against the real service it
    /// would invent knowledge bases in the developer's sidebar.
    #[tokio::test]
    async fn the_kb_pair_resolves_auto_against_the_resulting_set() {
        let temp = tempfile::TempDir::new().unwrap();
        let state = crate::state::AppState::new_with_knowledge_root(temp.path().to_path_buf())
            .await
            .unwrap();
        let services = ServerWorkspaceServices::new(state.clone());
        state
            .knowledge_service
            .create_base("alpha", "Alpha", None)
            .unwrap();
        state
            .knowledge_service
            .create_base("beta", "Beta", None)
            .unwrap();

        let sid = "br71-kb-scope";
        let set = |ids: &[&str]| ids.iter().map(|s| s.to_string()).collect::<Vec<_>>();

        // Auto on a scope with no primary pins the first id — the thing that
        // makes a fresh session's KB-less writes work at all.
        let view = services
            .set_knowledge_bases(sid, &set(&["alpha", "beta"]), KbPrimaryChoice::Auto)
            .unwrap();
        assert_eq!(view.kb_ids, set(&["alpha", "beta"]));
        assert_eq!(view.primary_kb.as_deref(), Some("alpha"));
        // `knowledge_selection` reports the same claim the mutator returned —
        // set and pointer together, not two unlocked reads.
        assert_eq!(services.knowledge_selection(sid), view);

        // Auto KEEPS a primary that is still a member of the new set.
        let view = services
            .set_knowledge_bases(sid, &set(&["beta", "alpha"]), KbPrimaryChoice::Auto)
            .unwrap();
        assert_eq!(view.primary_kb.as_deref(), Some("alpha"));

        // Auto MOVES it when the old target leaves the set.
        let view = services
            .set_knowledge_bases(sid, &set(&["beta"]), KbPrimaryChoice::Auto)
            .unwrap();
        assert_eq!(view.kb_ids, set(&["beta"]));
        assert_eq!(view.primary_kb.as_deref(), Some("beta"));

        // Set pins explicitly.
        let view = services
            .set_knowledge_bases(
                sid,
                &set(&["alpha", "beta"]),
                KbPrimaryChoice::Set("beta".to_string()),
            )
            .unwrap();
        assert_eq!(view.primary_kb.as_deref(), Some("beta"));

        // Clear removes the write target without narrowing the set. This is
        // "no primary for this session", not "inherit the machine's".
        let view = services
            .set_knowledge_bases(sid, &set(&["alpha", "beta"]), KbPrimaryChoice::Clear)
            .unwrap();
        assert_eq!(view.kb_ids, set(&["alpha", "beta"]));
        assert_eq!(view.primary_kb, None);
        assert_eq!(services.knowledge_selection(sid), view);

        // An empty set clears the pointer too: no legal member to point at.
        let view = services
            .set_knowledge_bases(sid, &[], KbPrimaryChoice::Auto)
            .unwrap();
        assert!(view.kb_ids.is_empty());
        assert_eq!(view.primary_kb, None);

        // A pin outside the resulting set is refused whole — the service
        // validates against the result, and this seam must not paper over it.
        let err = services
            .set_knowledge_bases(
                sid,
                &set(&["alpha"]),
                KbPrimaryChoice::Set("beta".to_string()),
            )
            .unwrap_err();
        assert!(err.contains("beta"), "{err}");

        // An unknown session has no selection, and reading one never fails.
        assert_eq!(
            services.knowledge_selection("br71-kb-never-seen").kb_ids,
            set(&["alpha", "beta"]),
            "a scope that has hidden nothing sees every installed base"
        );
    }
}
