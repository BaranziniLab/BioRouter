//! The daemon's `WorkspaceServices` implementation over `AppState` (BR-71).
//! The GUI methods are backed by the `WorkspaceBridge` registry (Slice 2), which
//! `routes::workspace`'s `/ui/workspace` socket populates; with no window
//! attached they degrade to the headless answers.

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

    /// Hydrate a session's extensions for a turn this crate is about to run,
    /// **reusing the eager load if one is already in flight**.
    ///
    /// `start_session` spawns a full `load_extensions_from_session` and
    /// registers the handle, exactly as `POST /agent/start` does. Loading again
    /// unconditionally means two concurrent loads on one agent: each spawns the
    /// session's stdio MCP subprocesses, and `add_extension_with_origin`'s
    /// double-check under the map lock drops the losing racer's — spawned,
    /// unreferenced, never reaped. `POST /agent/reply` has always avoided this
    /// by taking the pending results instead (`routes/agent.rs`); this is the
    /// same move, and `workspace_open { new: { prompt } }` — which chains
    /// `start_session` straight into a detached turn — is the caller that made
    /// it necessary here.
    async fn hydrate_extensions(
        &self,
        agent: &Arc<biorouter::agents::Agent>,
        session: &biorouter::session::Session,
    ) -> Vec<biorouter::agents::ExtensionLoadResult> {
        if let Some(results) = self.state.take_extension_loading_task(&session.id).await {
            self.state.remove_extension_loading_task(&session.id).await;
            return results;
        }
        agent.load_extensions_from_session(session).await
    }
}

/// Publish this daemon's platform services to the `biorouter` crate, so the
/// workspace extension's tools can reach the turn lock, the detached turn
/// runner and (Slice 2) the GUI bridge. Without it those tools degrade to their
/// headless behaviour *inside the daemon*, which is silent: every one of them
/// still answers, just about a session-level world.
///
/// A named function rather than three lines inlined in `commands/agent.rs`,
/// because a bootstrap step that only exists at a call site is a bootstrap step
/// nothing can test. `biorouter::workspace_services::install` writes a
/// `OnceLock`, so this is first-call-wins and later calls are no-ops.
pub fn install_workspace_services(state: Arc<AppState>) {
    biorouter::workspace_services::install(Arc::new(ServerWorkspaceServices::new(state)));
}

#[async_trait::async_trait]
impl WorkspaceServices for ServerWorkspaceServices {
    fn gui_attached(&self) -> bool {
        crate::workspace::bridge::any_attached()
    }

    fn layout_snapshot(&self) -> Option<serde_json::Value> {
        crate::workspace::bridge::merged_layout()
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
            self.hydrate_extensions(&agent, &session),
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

    /// Route to the window holding `near_session`, or fall back to the guess.
    ///
    /// ⚠ The fallback is not laziness (issue #78). A parent legitimately has no
    /// tab anywhere — a headless spawn, a tab closed mid-turn, or an echo not
    /// yet delivered through its 300 ms debounce — and the fire-and-forget
    /// contract requires that a spawn never break because a window could not be
    /// located. Falling back to today's behaviour is strictly better than
    /// dropping the frame.
    async fn gui_command_near(
        &self,
        frame: serde_json::Value,
        wait_result: bool,
        near_session: &str,
    ) -> Result<serde_json::Value, String> {
        let bridge = crate::workspace::bridge::bridge_for_session(near_session)
            .or_else(crate::workspace::bridge::focused_or_recent)
            .ok_or("no GUI attached")?;
        if wait_result {
            bridge
                .emit_and_wait(frame, std::time::Duration::from_secs(10))
                .await
        } else {
            bridge
                .emit(frame)
                .map(|()| serde_json::json!({ "sent": true }))
                .map_err(|e| e.to_string())
        }
    }

    async fn gui_command(
        &self,
        frame: serde_json::Value,
        wait_result: bool,
    ) -> Result<serde_json::Value, String> {
        let bridge = crate::workspace::bridge::focused_or_recent().ok_or("no GUI attached")?;
        if wait_result {
            bridge
                .emit_and_wait(frame, std::time::Duration::from_secs(10))
                .await
        } else {
            bridge
                .emit(frame)
                .map(|()| serde_json::json!({ "sent": true }))
        }
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
    ///   Relocating it would mean `BIOROUTER_PATH_ROOT` being set before the
    ///   `LazyLock<SESSION_STORAGE>` resolves `Paths::data_dir()`
    ///   (`session_manager.rs`), i.e. before whichever test touches the manager
    ///   first — which libtest's parallel scheduling does not let a test decide.
    ///   Fixing that is a crate-wide change (a path seam on `SessionManager`),
    ///   not a Task 9 one.
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

    /// `start_session` kicks off an EAGER extension load in a background task
    /// and registers it, exactly as `POST /agent/start` does. A detached turn on
    /// that same fresh session — which is what `workspace_open { new: { prompt
    /// } }` does microseconds later, the first caller to chain the two — must
    /// consume that pending load, not start a second concurrent one.
    ///
    /// `POST /agent/reply` already does precisely this (`take_extension_loading_task`
    /// then *reuse*, `routes/agent.rs`), and for the reason that applies here
    /// too: two concurrent `load_extensions_from_session` calls on one agent
    /// each spawn the session's stdio MCP subprocesses, and
    /// `add_extension_with_origin`'s double-check under the map lock means the
    /// losing racer's children are simply dropped — spawned, unreferenced, and
    /// never reaped.
    #[tokio::test]
    async fn a_detached_turn_reuses_the_eager_extension_load_instead_of_racing_it() {
        let state = crate::state::AppState::new().await.unwrap();
        let services = ServerWorkspaceServices::new(state.clone());
        let temp = tempfile::TempDir::new().unwrap();

        // There IS one to reuse — without this the assertion below would pass
        // against a `start_session` that never registered anything.
        let untouched = services
            .start_session(
                temp.path().to_path_buf(),
                Some(Vec::new()),
                Vec::new(),
                KbPrimaryChoice::Auto,
            )
            .await
            .unwrap();
        assert!(
            state
                .take_extension_loading_task(&untouched)
                .await
                .is_some(),
            "start_session registers an eager extension load"
        );

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
        let agent = state.get_agent(sid.clone()).await.unwrap();

        // The hydration a detached turn performs, without running the turn:
        // `start_detached_turn` would go on to restore a provider and call the
        // real turn runner, and in a developer's own environment that provider
        // resolves — a test must not fire a live turn to check a load.
        services.hydrate_extensions(&agent, &session).await;

        assert!(
            state.take_extension_loading_task(&sid).await.is_none(),
            "the detached turn must consume the eager load, not leave it running \
             beside a second one of its own"
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

    /// The daemon's bootstrap step (`commands/agent.rs`) must actually publish a
    /// working implementation — `install` being a no-op, or the daemon never
    /// calling it, is the difference between the workspace tools controlling the
    /// workspace and silently answering about a session-level world.
    ///
    /// **This test writes the process-global `OnceLock`** in
    /// `biorouter::workspace_services`, deliberately and once: it is the only
    /// way to exercise `install`, which every other test in the workspace suite
    /// avoids by using `set_for_tests` instead (see that module's doc comment on
    /// why a `OnceLock` written from a test is unrecoverable). Nothing else in
    /// `biorouter-server` reads `workspace_services::get()`, so pinning it for
    /// the rest of this binary's run is inert. If a future task adds a
    /// `biorouter-server` test that needs "no daemon installed", it will have to
    /// live in its own integration-test binary — not weaken this one.
    #[tokio::test]
    async fn the_bootstrap_publishes_a_working_implementation() {
        let state = crate::state::AppState::new().await.unwrap();
        install_workspace_services(state.clone());

        let installed =
            biorouter::workspace_services::get().expect("the daemon's services are installed");

        // Not just "something is there": it answers, and it answers about THIS
        // daemon's state. A stand-in that returned false for everything would
        // pass the first assertion, so the turn lock is the witness — the
        // installed services see a turn this test starts through `state`.
        let token = tokio_util::sync::CancellationToken::new();
        let _guard = state
            .try_begin_turn_idempotent("br71-bootstrap-witness", token, None)
            .expect("lock acquired");
        assert!(installed.is_turn_active("br71-bootstrap-witness"));
        assert!(!installed.is_turn_active("br71-bootstrap-never-started"));

        // The GUI half, now that Task 23 has wired it to the WorkspaceBridge
        // registry. This replaces Slice 1's `assert!(!gui_attached())` /
        // `assert!(layout_snapshot().is_none())` pair, which stated the STUB's
        // behaviour and, against the live registry, is a cross-test flake:
        // `BRIDGES` is a process-wide static shared with
        // `workspace::bridge::tests::registry_tracks_focus_and_merges_layouts`,
        // which holds two attached windows for part of its run. (Unaided the
        // collision was 0/15; widening that test's window with an 800 ms sleep
        // reproduced it 1/1.) So the assertions below are CONTAINMENT
        // assertions about a window this test owns — never global counts, and
        // never "nothing is attached".
        //
        // They are also strictly stronger than the pair they replace: a stub
        // returning a constant cannot make this daemon's services report an
        // echo that this test stored a moment ago, nor drop it again on detach.
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let win = format!("br71-bootstrap-win-{nonce}");
        let bridge = crate::workspace::bridge::bridge_for(&win);
        let (_rx, conn) = bridge.attach();
        bridge.store_echo(serde_json::json!({"window_id": win, "focused_session": null}));

        assert!(
            installed.gui_attached(),
            "with a window attached the daemon's services must report a GUI"
        );
        let carries_our_window = |snapshot: Option<serde_json::Value>| -> bool {
            snapshot
                .and_then(|v| v.as_array().cloned())
                .is_some_and(|entries| {
                    entries
                        .iter()
                        .any(|e| e.get("window_id").and_then(|w| w.as_str()) == Some(win.as_str()))
                })
        };
        assert!(
            carries_our_window(installed.layout_snapshot()),
            "layout_snapshot must carry this window's echo — it IS workspace_list's `gui`"
        );

        // …and it is live, not a captured constant: closing the window drops it.
        bridge.detach(conn);
        assert!(
            !carries_our_window(installed.layout_snapshot()),
            "a closed window's stale echo must not still be reported as GUI state"
        );
    }
}
