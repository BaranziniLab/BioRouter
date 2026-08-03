//! Task 30's behavioural gate: the master toggle `BIOROUTER_PRIVACY_TIERS`,
//! asserted in BOTH positions at every enforcement point this crate can reach
//! (issue #56, DR-15).
//!
//! ⚠ **THE ROW LIST IS THE PLAN'S GATE INVENTORY.** It must agree, row for row,
//! with the structural inventory Step 5 diffs against the tree, and Step 5 fails
//! if the two disagree or if the tree contains an enforcement point that appears
//! in neither.
//!
//! ⚠ **Do not collapse this into a loop over closures returning `bool`.** Each
//! row needs a different fixture, and the value of the test is that a compile
//! error appears when a gate this plan adds later is not represented here.
//!
//! ⚠ **Why this is an integration binary and not `--lib`.** `set_privacy_tiers`
//! mutates a PROCESS-GLOBAL atomic, and `cargo test` runs a crate's unit tests
//! in parallel threads of ONE process. A mutex serializes the tests that *take*
//! it, but `crates/biorouter/src/**`'s ~60 existing privacy tests do not take
//! it and would read `false` while this matrix held it there — they would fail
//! spuriously, or worse, pass while asserting nothing. Each `tests/*.rs` file is
//! its own process, so the only tests that can observe this file's writes are
//! this file's own, and they all take the guard. That is a deviation from the
//! plan's `cargo test -p biorouter --lib privacy`; the plan's own ⚠ names the
//! hazard but its remedy only covers callers of the helper.
//!
//! ⚠ **Two rows live elsewhere, and the reason is Rust's crate graph, not
//! omission.** `kb_export`'s forced export location is inside `biorouter-mcp`
//! and its only cheap fixture is a `KnowledgeServer` built by that crate's own
//! `#[cfg(test)]` helpers; `/config/upsert`'s gated arm is inside
//! `biorouter-server`, which this crate cannot depend on. Both are asserted in
//! both toggle positions beside the code they govern.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use biorouter::agents::{Agent, AgentConfig, AgentEvent, SessionConfig};
use biorouter::config::permission::PermissionManager;
use biorouter::config::BioRouterMode;
use biorouter::conversation::message::Message;
use biorouter::model::ModelConfig;
use biorouter::privacy::{ProviderTier, SessionClassification};
use biorouter::providers::base::{Provider, ProviderMetadata, ProviderUsage, Usage};
use biorouter::providers::errors::ProviderError;
use biorouter::session::session_manager::{Session, SessionType};
use biorouter::session::SessionManager;
use futures::StreamExt;
use rmcp::model::{CallToolRequestParams, Tool};
use tempfile::TempDir;

// ─────────────────────────────────────────────────────────────────────────────
// The one helper every test in this file shares, DEFINED rather than assumed.
// ─────────────────────────────────────────────────────────────────────────────

/// ⚠ Its own mutex, NOT `env_lock`'s. `env_lock`'s surface here is
/// `lock_env(iter) -> EnvGuard`; it exposes no bare process lock, and `EnvGuard`
/// is a sync guard, so reaching for it would mean either locking a variable this
/// has nothing to do with or holding a `std::sync` guard across an `.await` —
/// not `Send` in a `#[tokio::test]`. What the serialization actually requires is
/// only that **every** mutation of the toggle in this test binary goes through
/// one mutex, and this is it. A test that mutates BOTH the toggle and the
/// environment takes both guards, toggle first.
static PRIVACY_TIER_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// ⚠ Drop restores the PREVIOUS value, not `true`. Nesting has to unwind, or the
/// last test to run decides what every test scheduled after it asserts.
pub struct PrivacyTierGuard {
    prev: bool,
    _lock: tokio::sync::MutexGuard<'static, ()>,
}

impl Drop for PrivacyTierGuard {
    fn drop(&mut self) {
        biorouter_mcp::privacy_toggle::set_privacy_tiers_enabled(self.prev);
    }
}

/// ⚠ NOT reentrant. A test holding one guard must `drop` it before taking
/// another, and must never re-arm the flag by calling this again while one is
/// live — that deadlocks rather than failing, which is the worst way for a test
/// helper to be wrong. Re-arming mid-test is a bare
/// `set_privacy_tiers_enabled(..)` under the guard already held.
pub async fn set_privacy_tiers(on: bool) -> PrivacyTierGuard {
    let _lock = PRIVACY_TIER_LOCK.lock().await;
    let prev = biorouter_mcp::privacy_toggle::privacy_tiers_enabled();
    biorouter_mcp::privacy_toggle::set_privacy_tiers_enabled(on);
    PrivacyTierGuard { prev, _lock }
}

// ─────────────────────────────────────────────────────────────────────────────
// Fixtures. Each row needs a different one.
// ─────────────────────────────────────────────────────────────────────────────

/// One of the two extensions the compiled-in BAAM baseline calls private.
const PRIVATE_EXTENSION: &str = "ucsfomopagent";
const PRIVATE_TOOL: &str = "ucsfomopagent__data_sources";

/// The one sentence every turn refusal contains. Spelled as a literal rather
/// than imported, so a change to `turn_refusal`'s wording that silently stopped
/// refusing would still have to get past this test.
const TURN_REFUSAL_MARKER: &str = "this turn was not sent";

struct TieredProvider {
    name: &'static str,
    tier: ProviderTier,
}

#[async_trait]
impl Provider for TieredProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new("tiered", "Tiered", "", "tiered-model", vec![], "", vec![])
    }

    fn get_name(&self) -> &str {
        self.name
    }

    fn tier(&self) -> ProviderTier {
        self.tier
    }

    async fn complete_with_model(
        &self,
        _model_config: &ModelConfig,
        _system: &str,
        _messages: &[Message],
        _tools: &[Tool],
    ) -> Result<(Message, ProviderUsage), ProviderError> {
        Ok((
            Message::assistant().with_text("ok"),
            ProviderUsage::new("tiered-model".to_string(), Usage::default()),
        ))
    }

    fn get_model_config(&self) -> ModelConfig {
        ModelConfig::new_or_fail("tiered-model")
    }
}

fn public_provider() -> Arc<dyn Provider> {
    Arc::new(TieredProvider {
        name: "anthropic",
        tier: ProviderTier::Public,
    })
}

fn private_provider() -> Arc<dyn Provider> {
    Arc::new(TieredProvider {
        name: "versa_azure",
        tier: ProviderTier::Private,
    })
}

/// An agent over an isolated session store, already bound to `provider`. The
/// `TempDir` is returned because dropping it deletes the SQLite file the agent
/// still holds.
async fn agent_on(provider: Arc<dyn Provider>) -> (TempDir, Arc<Agent>, Session) {
    let dir = TempDir::new().unwrap();
    let session_manager = Arc::new(SessionManager::new(dir.path().to_path_buf()));
    let permission_manager = Arc::new(PermissionManager::new(dir.path().to_path_buf()));
    let agent = Arc::new(Agent::with_config(AgentConfig::new(
        session_manager,
        permission_manager,
        None,
        BioRouterMode::Auto,
    )));
    let session = agent
        .config
        .session_manager
        .create_session(
            PathBuf::from("."),
            "toggle-matrix".to_string(),
            SessionType::User,
        )
        .await
        .unwrap();
    agent.update_provider(provider, &session.id).await.unwrap();
    (dir, agent, session)
}

/// The same, with the private extension loaded under a private NAME, so the tier
/// the gates read is stamped by the production admission path rather than poked
/// into the record by the fixture.
async fn agent_with_the_private_extension(
    provider: Arc<dyn Provider>,
) -> (TempDir, Arc<Agent>, Session) {
    let (dir, agent, session) = agent_on(provider).await;
    agent
        .extension_manager
        .add_inprocess_server(
            PRIVATE_EXTENSION,
            biorouter_mcp::datasql::server::DataSqlServer::new(std::collections::HashMap::new()),
        )
        .await
        .expect("inject the private extension");
    (dir, agent, session)
}

async fn ratchet_to_private(sm: &SessionManager, id: &str) {
    sm.update(id)
        .raise_privacy(SessionClassification::Private, "turn:versa_azure")
        .apply()
        .await
        .unwrap();
}

fn cfg(session: &Session) -> SessionConfig {
    SessionConfig {
        id: session.id.clone(),
        schedule_id: None,
        max_turns: Some(2),
        max_tool_calls: None,
        budget: None,
        retry_config: None,
        reasoning_effort: None,
    }
}

/// Row 2's subject: the turn barrier, driven through the real `Agent::reply`.
/// Returns the concatenated text of every message event, refusal included.
async fn turn_text(agent: &Agent, session: &Session) -> String {
    let mut stream = agent
        .reply(Message::user().with_text("hi"), cfg(session), None)
        .await
        .unwrap();
    let mut out = String::new();
    while let Some(event) = stream.next().await {
        if let Ok(AgentEvent::Message(m)) = event {
            out.push_str(&m.as_concat_text());
            out.push('\n');
        }
    }
    out
}

/// Row 3's subject: the agent loop's own dispatch, which samples the capability
/// from the bound provider and hands it down. Returns the tool's output text
/// **whatever the outcome**, refusal message included.
async fn call_private_tool_via_agent_loop(agent: &Agent, session: &Session) -> String {
    let (_id, result) = agent
        .dispatch_tool_call(
            CallToolRequestParams {
                task: None,
                name: PRIVATE_TOOL.to_string().into(),
                arguments: Some(rmcp::object!({})),
                meta: None,
            },
            "req-1".to_string(),
            None,
            session,
        )
        .await;
    match result {
        Ok(call) => match call.result.await {
            Ok(ok) => format!("{ok:?}"),
            Err(e) => e.message.to_string(),
        },
        Err(e) => e.to_string(),
    }
}

/// Rows 4/5's subject: what discovery — and therefore the SYSTEM PROMPT — is
/// allowed to name. `get_extensions_info` carries a private server's own
/// instructions, so this covers Gate F2 as well as Gate E.
async fn extension_names_and_instructions(agent: &Agent) -> String {
    agent
        .extension_manager
        .get_extensions_info()
        .await
        .iter()
        .map(|info| format!("{}::{}", info.name, info.instructions))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Row 6's subject: Gate D, over a store this test owns.
async fn search_as(tier: ProviderTier, sm: &SessionManager, query: &str) -> usize {
    sm.search_chat_history(query, None, None, None, None, tier)
        .await
        .unwrap()
        .results
        .len()
}

// ─────────────────────────────────────────────────────────────────────────────

/// DR-15. Every enforcement point this crate can reach, asserted in BOTH toggle
/// positions, plus the copy INVARIANT asserted identically in both — carrying a
/// parent's stamp is column propagation, not a classification decision, and a
/// copy that laundered a private row to public while the feature was off would
/// do exactly that, durably, because re-enabling does not revisit it (AR-7).
///
/// The `on` column is what every other task already tests, restated here so this
/// test fails when a gate is wired to the toggle but broken, not only when it is
/// unwired.
#[tokio::test]
async fn the_master_toggle_governs_every_gate_in_both_directions() {
    // ---- ON: the shipped default. Every gate refuses. -----------------------
    // ⚠ `set_privacy_tiers` RETURNS AN RAII GUARD and is never called for
    //   effect: a bare setter here would leave the flag true-by-luck rather than
    //   true-by-construction.
    let on = set_privacy_tiers(true).await;

    // 1  A     (Task 12) — the bind lattice, through its production caller.
    let (_d1, agent1, s1) = agent_on(private_provider()).await;
    let sm1 = agent1.config.session_manager.clone();
    ratchet_to_private(&sm1, &s1.id).await;
    assert!(agent1
        .update_provider(public_provider(), &s1.id)
        .await
        .is_err());

    // 2  B     (Task 13) — the turn barrier.
    let (_d2, agent2, s2) = agent_on(public_provider()).await;
    let sm2 = agent2.config.session_manager.clone();
    ratchet_to_private(&sm2, &s2.id).await;
    assert!(turn_text(&agent2, &s2).await.contains(TURN_REFUSAL_MARKER));

    // 3  C     (Task 14) — tool dispatch into a private extension.
    let (_d3, agent3, s3) = agent_with_the_private_extension(public_provider()).await;
    assert!(call_private_tool_via_agent_loop(&agent3, &s3)
        .await
        .contains(PRIVATE_EXTENSION));

    // 4/5 C'+E+F2 (Tasks 15, 16, 18) — discovery, and a private server's
    //          instructions in a public system prompt.
    assert!(!extension_names_and_instructions(&agent3)
        .await
        .contains(PRIVATE_EXTENSION));

    // 6  D     (Task 17) — chat history search.
    let (_d6, agent6, s6) = agent_on(private_provider()).await;
    let sm6 = agent6.config.session_manager.clone();
    sm6.add_message(&s6.id, &Message::user().with_text("cohort n=412"))
        .await
        .unwrap();
    ratchet_to_private(&sm6, &s6.id).await;
    assert_eq!(search_as(ProviderTier::Public, &sm6, "cohort").await, 0);

    // 8  KB    (Task 10C) — the knowledge-base read barrier.
    let kb_dir = TempDir::new().unwrap();
    let path_root = kb_dir.path();
    let kb_root = kb_root_under(path_root);
    let kb_root = kb_root.as_path();
    // A REAL base, created through the production path that stamps its tier in
    // the same transaction — not a bare directory plus a hand-written tier
    // entry. `Catalog::discover` reads the registry and the manifest, so a
    // hand-made directory would leave row 14 passing vacuously in BOTH columns:
    // an empty catalog contains no private base either.
    biorouter_mcp::knowledge::service::KnowledgeService::new(kb_root.to_path_buf())
        .create_base_as("omop", "OMOP", None, /* caller_is_private */ true)
        .expect("create the private base");
    assert!(biorouter_mcp::knowledge::tier::is_private(kb_root, "omop"));
    assert!(biorouter_mcp::knowledge::tier::assert_reachable(kb_root, "omop", false).is_err());

    // 9  G     (Task 11) — conversation ingest.
    let private_row = sm6.get_session(&s6.id, false).await.unwrap();
    assert!(
        biorouter::knowledge::conversation_ingest::refuses_every_session(
            ProviderTier::Public,
            std::slice::from_ref(&private_row)
        )
    );

    // 10 H     (Task 19) — an alternate provider built outside the session bind.
    assert!(biorouter::privacy::assert_alt_provider_allowed(
        "plan mode",
        public_provider().as_ref(),
        SessionClassification::Private,
        "BIOROUTER_PLANNER_PROVIDER",
    )
    .is_err());

    // 14 CP5   (Task 10D) — the Agent Drafter catalog's knowledge bases. The
    //          private base is omitted for a public caller, and the assertion is
    //          NOT vacuous: a private caller sees it in the same tree.
    assert!(catalog_kb_ids(path_root, /* caller_is_private */ true).contains(&"omop".to_string()));
    assert!(!catalog_kb_ids(path_root, /* caller_is_private */ false).contains(&"omop".to_string()));

    // 16 ratchet (Task 10B) — a private caller's write marks the base private.
    biorouter_mcp::knowledge::tier::raise_unlocked(kb_root, "notes", true).unwrap();
    assert!(biorouter_mcp::knowledge::tier::is_private(kb_root, "notes"));

    // 17 copy  (Task 22) — an INVARIANT, not a gate. Identical in both columns.
    //
    // Its own store: a copy lands in the same database as its original, and Gate
    // D's row above counts rows matching "cohort". Sharing `sm6` would make the
    // OFF column's search find two, which reads as a Gate D regression and is
    // really this row's side effect.
    let (_d17, agent17, s17) = agent_on(private_provider()).await;
    let sm17 = agent17.config.session_manager.clone();
    ratchet_to_private(&sm17, &s17.id).await;
    let copy_on = sm17
        .copy_session(&s17.id, "copy-on".to_string())
        .await
        .unwrap();
    assert_eq!(copy_on.privacy_tier, SessionClassification::Private);

    // 18 VIS   (Task 21) — the visibility predicate is PURE; the toggle reaches
    //          it through the caller, which is why the row above (Gate D, a
    //          caller) changes below while this line does not.
    assert!(!biorouter::privacy::visible_to(
        ProviderTier::Public,
        SessionClassification::Private
    ));

    // ---- OFF: nothing is refused. ------------------------------------------
    drop(on); // ← restores the previous value; the guard below then owns it
    let _off = set_privacy_tiers(false).await;

    assert!(agent1
        .update_provider(public_provider(), &s1.id)
        .await
        .is_ok());
    assert!(!turn_text(&agent2, &s2).await.contains(TURN_REFUSAL_MARKER));
    assert!(!call_private_tool_via_agent_loop(&agent3, &s3)
        .await
        .contains("private extension"));
    assert!(extension_names_and_instructions(&agent3)
        .await
        .contains(PRIVATE_EXTENSION));
    assert_eq!(search_as(ProviderTier::Public, &sm6, "cohort").await, 1);
    assert!(biorouter_mcp::knowledge::tier::assert_reachable(kb_root, "omop", false).is_ok());
    assert!(
        !biorouter::knowledge::conversation_ingest::refuses_every_session(
            ProviderTier::Public,
            std::slice::from_ref(&private_row)
        )
    );
    assert!(biorouter::privacy::assert_alt_provider_allowed(
        "plan mode",
        public_provider().as_ref(),
        SessionClassification::Private,
        "BIOROUTER_PLANNER_PROVIDER",
    )
    .is_ok());
    assert!(catalog_kb_ids(path_root, /* caller_is_private */ false).contains(&"omop".to_string()));
    // AR-7: the ratchet stops too.
    biorouter_mcp::knowledge::tier::raise_unlocked(kb_root, "notes2", true).unwrap();
    assert!(!biorouter_mcp::knowledge::tier::is_private(
        kb_root, "notes2"
    ));
    // ⚠ Row 17 is IDENTICAL in both columns, on purpose. The toggle stops
    //   ENFORCEMENT; it does not delete the columns or rewrite the stamps
    //   already written, and a copy that laundered a private row to public while
    //   the feature was off would do exactly that — durably, because re-enabling
    //   does not revisit it (AR-7). Row 16 differs because a ratchet *writes a
    //   new* classification, which is the thing AR-7 says stops happening.
    let copy_off = sm17
        .copy_session(&s17.id, "copy-off".to_string())
        .await
        .unwrap();
    assert_eq!(copy_off.privacy_tier, SessionClassification::Private);
    // …and the pure predicate's own answer is unchanged: the toggle reaches it
    //   through its callers, never inside it.
    assert!(!biorouter::privacy::visible_to(
        ProviderTier::Public,
        SessionClassification::Private
    ));
}

/// AR-7, as an assertion rather than a paragraph: with the toggle off the ratchet
/// does not fire, and turning it back on does not go back and fix it. This is the
/// one behaviour a reader is most likely to assume works the other way, so it is
/// pinned rather than described.
#[tokio::test]
async fn nothing_ratchets_while_the_toggle_is_off_and_re_enabling_does_not_backfill() {
    let off = set_privacy_tiers(false).await;
    let (_dir, agent, s) = agent_with_the_private_extension(private_provider()).await;
    let sm = agent.config.session_manager.clone();
    // DR-4's two triggers: a permitted private-extension dispatch, and a turn.
    let _ = call_private_tool_via_agent_loop(&agent, &s).await;
    let _ = turn_text(&agent, &s).await;
    assert_eq!(
        sm.get_session(&s.id, false).await.unwrap().privacy_tier,
        SessionClassification::Public,
        "DR-4's two triggers must not fire while the feature is off"
    );

    drop(off);
    let _on = set_privacy_tiers(true).await;
    assert_eq!(
        sm.get_session(&s.id, false).await.unwrap().privacy_tier,
        SessionClassification::Public,
        "re-enabling must not retro-classify; there is no content scan (AR-7)"
    );
}

/// The failure mode is an agent disabling its own protection, and
/// `Config::get_param`'s env branch is the easiest lever in the tree. The
/// authoritative value is read from the loaded values map instead.
///
/// ⚠ This reads — never writes — whatever `config.yaml` `Config::global()`
/// resolved. It cannot be redirected: `Config::global()` is a `OnceCell` that
/// the matrix above has already initialised through the agent's own provider
/// rebind. The read is harmless, and the one way it can fail for an innocent
/// reason is named in the message rather than left as a mystery.
#[tokio::test]
async fn no_environment_variable_can_turn_protection_off() {
    let _g = set_privacy_tiers(true).await;
    let _env = env_lock::lock_env([("BIOROUTER_PRIVACY_TIERS", Some("off"))]);
    biorouter::privacy::load_privacy_tiers_from_config();
    assert!(
        biorouter::privacy::privacy_tiers_enabled(),
        "an env var disabled the whole feature — unless this machine's own \
         config.yaml really does set BIOROUTER_PRIVACY_TIERS to off, in which \
         case the loader is right and the fixture is wrong"
    );
}

/// The knowledge bases the Agent Drafter catalog offers a caller.
///
/// `Catalog::discover` resolves its own root through `BIOROUTER_PATH_ROOT`, so
/// the env guard is taken here — inside a SYNCHRONOUS helper — rather than
/// around the `await`s in the matrix: `env_lock`'s guard is a `std::sync` guard
/// and holding one across an `.await` is not `Send` in a `#[tokio::test]`.
fn catalog_kb_ids(path_root: &std::path::Path, caller_is_private: bool) -> Vec<String> {
    let _env = env_lock::lock_env([(
        "BIOROUTER_PATH_ROOT",
        Some(path_root.to_str().expect("utf-8 temp path")),
    )]);
    biorouter_mcp::agent_drafter::catalog::Catalog::discover(caller_is_private)
        .knowledge_bases
        .into_iter()
        .map(|kb| kb.id)
        .collect()
}

/// The knowledge root `Catalog::discover` will resolve under `path_root`, so the
/// matrix stamps tiers on the same tree the catalog reads.
fn kb_root_under(path_root: &std::path::Path) -> std::path::PathBuf {
    let _env = env_lock::lock_env([(
        "BIOROUTER_PATH_ROOT",
        Some(path_root.to_str().expect("utf-8 temp path")),
    )]);
    let root = biorouter_mcp::knowledge::paths::knowledge_root().expect("knowledge root");
    std::fs::create_dir_all(&root).expect("create knowledge root");
    root
}
