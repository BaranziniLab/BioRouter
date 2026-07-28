//! End-to-end agent-loop coverage for the conversation-writeback freshness
//! discipline.
//!
//! `replace_conversation` DELETEs and re-INSERTs a session's ENTIRE message set,
//! so any caller that computed its new conversation from a snapshot destroys
//! whatever landed in between. BR-12 gave the *background* eager-compaction path
//! a freshness check; the in-turn compaction sites never got one.
//!
//! These tests drive the **real** reply loop with a mock provider (no network,
//! no keychain). The provider appends a foreign message from inside its own
//! completion call, which makes the race deterministic — no sleeps, no barriers.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::Result;
use async_trait::async_trait;
use biorouter::agents::{Agent, AgentConfig, AgentEvent, SessionConfig};
use biorouter::config::permission::PermissionManager;
use biorouter::config::BioRouterMode;
use biorouter::conversation::message::{Message, MessageContent};
use biorouter::model::ModelConfig;
use biorouter::providers::base::{Provider, ProviderMetadata, ProviderUsage, Usage};
use biorouter::providers::errors::ProviderError;
use biorouter::session::session_manager::{SessionType, DB_NAME, SESSIONS_FOLDER};
use biorouter::session::SessionManager;
use futures::StreamExt;
use rmcp::model::Tool;
use tempfile::TempDir;

/// A summary that clears `summary_is_usable` (>= 40 chars, >= 3 mandated
/// section headings), so compaction accepts it on the first attempt.
const GOOD_SUMMARY: &str = "## User Intent\nThe user asked for a plot.\n\
     ## Technical Concepts\nPlotting, data frames.\n\
     ## Files\nnone\n## Pending Tasks\nnone\n";

/// The one user message text every test seeds the turn with.
const USER_PROMPT: &str = "Plot the data";

/// A mock provider that can (a) append a foreign message to the session from
/// inside a completion — the deterministic stand-in for a concurrent appender
/// such as BR-71's note tool or `biorouter term log` — and (b) fail a given
/// number of main-loop completions with `ContextLengthExceeded` to drive the
/// overflow-recovery ladder.
struct RaceProvider {
    session_manager: Arc<SessionManager>,
    session_id: OnceLock<String>,
    /// Completions that are the agent loop's own (not the summarizer's).
    main_calls: AtomicUsize,
    /// Completions issued by the compaction summarizer.
    summarizer_calls: AtomicUsize,
    /// Zero-based main-call indices that return `ContextLengthExceeded`.
    overflow_on: Vec<usize>,
    /// Notes to append, keyed by the zero-based *summarizer* call index they
    /// should land during. Appending from inside the summarizer call is exactly
    /// the window the freshness discipline protects.
    notes_during_summarization: Vec<(usize, String)>,
    /// #51: the same, but the appended note carries the PRESERVATION MARKER.
    /// Together these two cover the whole promise — part (a) has to carry the
    /// note over the write-back, part (b) has to keep it out of the summary
    /// once it ages past the verbatim window.
    pinned_notes_during_summarization: Vec<(usize, String)>,
    /// Summarizer call indices during which the session's whole history is
    /// wholesale-rewritten by someone else, which moves the basis and makes the
    /// caller's write-back stale.
    rewrites_during_summarization: Vec<usize>,
    /// Notes to append, keyed by the zero-based MAIN-loop call index they land
    /// during. Models a writer that persists to the session mid-turn without
    /// the reply loop ever pushing the message into its in-memory conversation
    /// — `drain_elicitation_messages`, `biorouter term log`, a note tool.
    notes_during_main_call: Vec<(usize, String)>,
    /// Zero-based *summarizer* call indices that fail outright. Models the
    /// provider going away between a first summarization and its retry.
    fail_summarization_on: Vec<usize>,
    context_limit: usize,
    /// Message texts the provider saw on each main-loop call.
    seen: Mutex<Vec<Vec<String>>>,
    /// #51: the summarizer carries the history it is asked to condense in the
    /// SYSTEM prompt, so this is how a test asserts what did — and did not —
    /// reach it.
    summarizer_payloads: Mutex<Vec<String>>,
}

impl RaceProvider {
    fn new(session_manager: Arc<SessionManager>) -> Self {
        Self {
            session_manager,
            session_id: OnceLock::new(),
            main_calls: AtomicUsize::new(0),
            summarizer_calls: AtomicUsize::new(0),
            overflow_on: Vec::new(),
            notes_during_summarization: Vec::new(),
            pinned_notes_during_summarization: Vec::new(),
            rewrites_during_summarization: Vec::new(),
            notes_during_main_call: Vec::new(),
            fail_summarization_on: Vec::new(),
            context_limit: 200_000,
            seen: Mutex::new(Vec::new()),
            summarizer_payloads: Mutex::new(Vec::new()),
        }
    }

    fn overflow_on(mut self, calls: &[usize]) -> Self {
        self.overflow_on = calls.to_vec();
        self
    }

    fn fail_summarization_on(mut self, call: usize) -> Self {
        self.fail_summarization_on.push(call);
        self
    }

    fn note_during_summarization(mut self, call: usize, text: &str) -> Self {
        self.notes_during_summarization
            .push((call, text.to_string()));
        self
    }

    /// #51: append a note that carries the preservation marker.
    fn pinned_note_during_summarization(mut self, call: usize, text: &str) -> Self {
        self.pinned_notes_during_summarization
            .push((call, text.to_string()));
        self
    }

    fn note_during_main_call(mut self, call: usize, text: &str) -> Self {
        self.notes_during_main_call.push((call, text.to_string()));
        self
    }

    fn rewrite_during_summarization(mut self, call: usize) -> Self {
        self.rewrites_during_summarization.push(call);
        self
    }

    fn context_limit(mut self, limit: usize) -> Self {
        self.context_limit = limit;
        self
    }

    fn main_call_count(&self) -> usize {
        self.main_calls.load(Ordering::SeqCst)
    }

    fn summarizer_call_count(&self) -> usize {
        self.summarizer_calls.load(Ordering::SeqCst)
    }

    /// Everything every summarization round-trip was handed, concatenated.
    fn all_summarizer_payloads(&self) -> String {
        self.summarizer_payloads.lock().unwrap().join("\n")
    }

    fn texts_seen_on_main_call(&self, n: usize) -> Vec<String> {
        self.seen
            .lock()
            .unwrap()
            .get(n)
            .cloned()
            .unwrap_or_default()
    }

    /// The summarizer sends exactly one user message with a fixed prompt and
    /// carries the history in the system prompt; the agent loop never does.
    fn is_summarizer_call(messages: &[Message]) -> bool {
        messages.len() == 1
            && messages[0].content.iter().any(|c| {
                matches!(c, MessageContent::Text(t)
                    if t.text.contains("Please summarize the conversation history"))
            })
    }
}

#[async_trait]
impl Provider for RaceProvider {
    async fn complete_with_model(
        &self,
        _model_config: &ModelConfig,
        system_prompt: &str,
        messages: &[Message],
        _tools: &[Tool],
    ) -> Result<(Message, ProviderUsage), ProviderError> {
        let usage = ProviderUsage::new(
            "mock-model".to_string(),
            Usage::new(Some(10), Some(5), Some(15)),
        );

        if Self::is_summarizer_call(messages) {
            let n = self.summarizer_calls.fetch_add(1, Ordering::SeqCst);
            self.summarizer_payloads
                .lock()
                .unwrap()
                .push(system_prompt.to_string());
            if self.fail_summarization_on.contains(&n) {
                return Err(ProviderError::RequestFailed(
                    "mock summarizer failure".to_string(),
                ));
            }
            // Append the foreign message *during* the summarization round-trip:
            // after the caller took its snapshot, before it writes back.
            for (call, text) in &self.notes_during_summarization {
                if *call == n {
                    let id = self.session_id.get().expect("session id set");
                    self.session_manager
                        .add_message(id, &Message::user().with_text(text.clone()))
                        .await
                        .expect("note append");
                }
            }
            for (call, text) in &self.pinned_notes_during_summarization {
                if *call == n {
                    let id = self.session_id.get().expect("session id set");
                    self.session_manager
                        .add_message(id, &Message::user().with_text(text.clone()).pinned())
                        .await
                        .expect("pinned note append");
                }
            }
            if self.rewrites_during_summarization.contains(&n) {
                let id = self.session_id.get().expect("session id set");
                let current = self
                    .session_manager
                    .get_session(id, true)
                    .await
                    .expect("read")
                    .conversation
                    .expect("conversation");
                // A wholesale rewrite renumbers every row, so the caller's
                // basis prefix vanishes: the definition of a moved basis.
                self.session_manager
                    .replace_conversation(id, &current)
                    .await
                    .expect("rewrite");
            }
            return Ok((Message::assistant().with_text(GOOD_SUMMARY), usage));
        }

        let n = self.main_calls.fetch_add(1, Ordering::SeqCst);
        self.seen.lock().unwrap().push(
            messages
                .iter()
                .flat_map(|m| m.content.iter())
                .filter_map(|c| match c {
                    MessageContent::Text(t) => Some(t.text.clone()),
                    _ => None,
                })
                .collect(),
        );

        for (call, text) in &self.notes_during_main_call {
            if *call == n {
                let id = self.session_id.get().expect("session id set");
                self.session_manager
                    .add_message(id, &Message::user().with_text(text.clone()))
                    .await
                    .expect("mid-turn append");
            }
        }

        if self.overflow_on.contains(&n) {
            return Err(ProviderError::ContextLengthExceeded(
                "mock overflow".to_string(),
            ));
        }

        Ok((Message::assistant().with_text("All done."), usage))
    }

    fn get_model_config(&self) -> ModelConfig {
        let mut config = ModelConfig::new("mock-model").unwrap();
        config.context_limit = Some(self.context_limit);
        config
    }

    fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            name: "mock".to_string(),
            display_name: "Mock Provider".to_string(),
            description: "Mock provider for testing".to_string(),
            default_model: "mock-model".to_string(),
            known_models: vec![],
            model_doc_link: String::new(),
            config_keys: vec![],
            allows_unlisted_models: false,
        }
    }

    fn get_name(&self) -> &str {
        "mock-test"
    }
}

struct Harness {
    agent: Arc<Agent>,
    session_id: String,
    session_manager: Arc<SessionManager>,
    /// Where the session store's sqlite file lives, so a test can reach the
    /// database directly (see `fail_to_store_the_summary`).
    data_dir: PathBuf,
    _work_dir: TempDir,
}

async fn harness(build: impl FnOnce(RaceProvider) -> RaceProvider) -> (Harness, Arc<RaceProvider>) {
    let work_dir = TempDir::new().unwrap();
    let data_dir = TempDir::new().unwrap();
    let data_path = data_dir.path().to_path_buf();
    let session_manager = Arc::new(SessionManager::new(data_dir.path().to_path_buf()));

    let provider = Arc::new(build(RaceProvider::new(session_manager.clone())));

    let config = AgentConfig::new(
        session_manager.clone(),
        PermissionManager::instance(),
        None,
        BioRouterMode::Auto,
    );
    let agent = Agent::with_config(config);

    let session = session_manager
        .create_session(
            work_dir.path().to_path_buf(),
            "writeback-freshness-test".to_string(),
            SessionType::Hidden,
        )
        .await
        .unwrap();
    provider.session_id.set(session.id.clone()).unwrap();

    agent
        .update_provider(provider.clone() as Arc<dyn Provider>, &session.id)
        .await
        .unwrap();

    // The session store lives under data_dir for the life of the agent.
    std::mem::forget(data_dir);

    (
        Harness {
            agent: Arc::new(agent),
            session_id: session.id,
            session_manager,
            data_dir: data_path,
            _work_dir: work_dir,
        },
        provider,
    )
}

impl Harness {
    async fn run_turn(&self, prompt: &str) -> Result<Vec<AgentEvent>> {
        let session_config = SessionConfig {
            id: self.session_id.clone(),
            schedule_id: None,
            max_turns: Some(8),
            max_tool_calls: None,
            retry_config: None,
            budget: None,
            reasoning_effort: None,
        };
        let stream = self
            .agent
            .reply(Message::user().with_text(prompt), session_config, None)
            .await?;
        tokio::pin!(stream);
        let mut out = Vec::new();
        while let Some(ev) = stream.next().await {
            out.push(ev?);
        }
        Ok(out)
    }

    async fn stored_texts(&self) -> Vec<String> {
        self.session_manager
            .get_session(&self.session_id, true)
            .await
            .unwrap()
            .conversation
            .unwrap()
            .messages()
            .iter()
            .flat_map(|m| m.content.iter())
            .filter_map(|c| match c {
                MessageContent::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect()
    }

    /// How many stored messages carry `needle`.
    ///
    /// A recovered row is re-inserted, so "did it survive" and "was it survived
    /// exactly once" are different questions: a basis split can equally well
    /// produce a DUPLICATE (the row is recovered onto the tail of a replacement
    /// that already contains it) instead of a deletion.
    async fn stored_occurrences(&self, needle: &str) -> usize {
        self.stored_texts()
            .await
            .iter()
            .filter(|t| t.contains(needle))
            .count()
    }
}

/// The user-facing system notifications the turn emitted.
fn notification_texts(events: &[AgentEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|ev| match ev {
            AgentEvent::Message(m) => Some(m),
            _ => None,
        })
        .flat_map(|m| m.content.iter())
        .filter_map(|c| c.as_system_notification().map(|n| n.msg.clone()))
        .collect()
}

fn history_replaced_texts(events: &[AgentEvent]) -> Option<Vec<String>> {
    events.iter().rev().find_map(|ev| match ev {
        AgentEvent::HistoryReplaced(conv) => Some(
            conv.messages()
                .iter()
                .flat_map(|m| m.content.iter())
                .filter_map(|c| match c {
                    MessageContent::Text(t) => Some(t.text.clone()),
                    _ => None,
                })
                .collect(),
        ),
        _ => None,
    })
}

// ── F1: the headline case ────────────────────────────────────────────────────

/// A message appended to a live session while the overflow-recovery summarizer
/// runs must survive the writeback. This is the BR-71 `mode: "note"` case, and
/// the shipped `biorouter term log` case: the tool already returned success, so
/// losing the row here is a silent lie.
#[tokio::test(flavor = "multi_thread")]
async fn overflow_recovery_preserves_a_concurrent_note() {
    let (h, provider) = harness(|p| {
        p.overflow_on(&[0])
            .note_during_summarization(0, "NOTE: reviewer asked for log scale")
    })
    .await;

    let events = h.run_turn(USER_PROMPT).await.unwrap();

    // Liveness: the turn still completed after recovering from the overflow.
    assert_eq!(
        provider.main_call_count(),
        2,
        "the loop must retry after the recovery compaction"
    );
    assert_eq!(provider.summarizer_call_count(), 1);

    let stored = h.stored_texts().await;
    assert!(
        stored
            .iter()
            .any(|t| t.contains("NOTE: reviewer asked for log scale")),
        "the concurrently appended note must survive the writeback; stored: {stored:#?}"
    );

    // ...and the client's view agrees with the store.
    let replaced = history_replaced_texts(&events).expect("HistoryReplaced emitted");
    assert!(
        replaced
            .iter()
            .any(|t| t.contains("NOTE: reviewer asked for log scale")),
        "HistoryReplaced must carry the preserved note; got: {replaced:#?}"
    );
}

// ── F2: the anti-false-conflict twin ─────────────────────────────────────────

/// With no concurrent writer the recovery compaction must still be PERSISTED.
/// A freshness guard that misfires here silently disables durable compaction —
/// nothing user-visible breaks, the session just grows forever.
#[tokio::test(flavor = "multi_thread")]
async fn overflow_recovery_persists_when_nothing_else_wrote() {
    let (h, provider) = harness(|p| p.overflow_on(&[0])).await;

    let events = h.run_turn(USER_PROMPT).await.unwrap();

    assert_eq!(provider.main_call_count(), 2);
    let stored = h.stored_texts().await;
    assert!(
        stored.iter().any(|t| t.contains("User Intent")),
        "the compaction summary must be persisted; stored: {stored:#?}"
    );
    assert!(
        history_replaced_texts(&events).is_some(),
        "HistoryReplaced must be emitted when the store really was replaced"
    );
}

// ── F4: two overflows in one turn ────────────────────────────────────────────

/// A second overflow later in the same turn must ALSO persist. The first
/// recovery rewrote every row (new rowids, a minted uid for the summary), so a
/// basis captured once at turn start is stale by construction — it has to be
/// re-seeded after each successful swap.
#[tokio::test(flavor = "multi_thread")]
async fn a_second_overflow_in_one_turn_still_persists() {
    let (h, provider) = harness(|p| p.overflow_on(&[0, 1])).await;

    h.run_turn(USER_PROMPT).await.unwrap();

    assert_eq!(
        provider.main_call_count(),
        3,
        "two overflows, then a success"
    );
    assert_eq!(
        provider.summarizer_call_count(),
        2,
        "each overflow runs its own recovery compaction"
    );

    let stored = h.stored_texts().await;
    assert!(
        stored.iter().any(|t| t.contains("User Intent")),
        "the second recovery compaction must be persisted too; stored: {stored:#?}"
    );
}

// ── F3: overflow recovery on a turn that also auto-compacted ─────────────────

/// The in-turn auto-compaction at the top of `reply()` rewrites every row
/// before the loop starts. Overflow recovery later in the same turn must still
/// persist — i.e. its freshness basis is captured AFTER that rewrite.
#[tokio::test(flavor = "multi_thread")]
async fn overflow_recovery_persists_on_a_turn_that_also_auto_compacted() {
    // A tiny context limit plus a live gauge over it forces the synchronous
    // auto-compaction at the top of reply().
    let (h, provider) = harness(|p| p.overflow_on(&[0]).context_limit(100)).await;

    for i in 0..6 {
        h.session_manager
            .add_message(&h.session_id, &Message::user().with_text(format!("q{i}")))
            .await
            .unwrap();
        h.session_manager
            .add_message(
                &h.session_id,
                &Message::assistant().with_text(format!("a{i}")),
            )
            .await
            .unwrap();
    }
    h.session_manager
        .update(&h.session_id)
        .total_tokens(Some(95))
        .apply()
        .await
        .unwrap();

    h.run_turn(USER_PROMPT).await.unwrap();

    assert!(
        provider.summarizer_call_count() >= 2,
        "an auto-compaction and a recovery compaction should both have run, saw {}",
        provider.summarizer_call_count()
    );

    // Both summaries are on disk: the auto-compaction's and the recovery's. A
    // basis captured before the auto-compaction would make the recovery swap
    // look stale, and only the first summary would survive.
    let stored = h.stored_texts().await;
    let summaries = stored.iter().filter(|t| t.contains("User Intent")).count();
    assert!(
        summaries >= 2,
        "the recovery compaction must persist even after an auto-compaction \
         renumbered every row (expected both summaries, saw {summaries}); stored: {stored:#?}"
    );
}

// ── auto-compaction (agent.rs:3061) ──────────────────────────────────────────

/// The same race one level up: a note that lands while the *auto*-compaction
/// summarizer runs must survive, and must reach the model this turn.
#[tokio::test(flavor = "multi_thread")]
async fn auto_compaction_preserves_a_concurrent_note() {
    let (h, provider) = harness(|p| {
        p.context_limit(100)
            .note_during_summarization(0, "NOTE: use the 2024 cohort")
    })
    .await;

    for i in 0..6 {
        h.session_manager
            .add_message(&h.session_id, &Message::user().with_text(format!("q{i}")))
            .await
            .unwrap();
        h.session_manager
            .add_message(
                &h.session_id,
                &Message::assistant().with_text(format!("a{i}")),
            )
            .await
            .unwrap();
    }
    h.session_manager
        .update(&h.session_id)
        .total_tokens(Some(95))
        .apply()
        .await
        .unwrap();

    h.run_turn(USER_PROMPT).await.unwrap();

    assert_eq!(provider.summarizer_call_count(), 1);
    let stored = h.stored_texts().await;
    assert!(
        stored
            .iter()
            .any(|t| t.contains("NOTE: use the 2024 cohort")),
        "the note appended during auto-compaction must survive; stored: {stored:#?}"
    );
    assert!(
        provider
            .texts_seen_on_main_call(0)
            .iter()
            .any(|t| t.contains("NOTE: use the 2024 cohort")),
        "the preserved note must also be in the model's context for this turn, saw: {:?}",
        provider.texts_seen_on_main_call(0)
    );
}

/// Anti-false-conflict twin for the auto-compaction site.
#[tokio::test(flavor = "multi_thread")]
async fn auto_compaction_still_persists_with_no_concurrent_writer() {
    let (h, provider) = harness(|p| p.context_limit(100)).await;

    for i in 0..6 {
        h.session_manager
            .add_message(&h.session_id, &Message::user().with_text(format!("q{i}")))
            .await
            .unwrap();
        h.session_manager
            .add_message(
                &h.session_id,
                &Message::assistant().with_text(format!("a{i}")),
            )
            .await
            .unwrap();
    }
    h.session_manager
        .update(&h.session_id)
        .total_tokens(Some(95))
        .apply()
        .await
        .unwrap();

    h.run_turn(USER_PROMPT).await.unwrap();

    assert_eq!(provider.summarizer_call_count(), 1);
    let stored = h.stored_texts().await;
    assert!(
        stored.iter().any(|t| t.contains("User Intent")),
        "the auto-compaction summary must be persisted; stored: {stored:#?}"
    );
}

/// When the basis moves out from under the auto-compaction (a checkpoint
/// restore, a message edit, another turn's rewrite) the swap is declined — and
/// that must NOT fail the turn. Trading a data-loss bug for a liveness bug is
/// not a fix. The turn proceeds on the FRESH history, so whatever landed is in
/// the model's context this turn.
#[tokio::test(flavor = "multi_thread")]
async fn auto_compaction_stale_continues_the_turn_uncompacted() {
    let (h, provider) = harness(|p| p.context_limit(100).rewrite_during_summarization(0)).await;

    for i in 0..6 {
        h.session_manager
            .add_message(&h.session_id, &Message::user().with_text(format!("q{i}")))
            .await
            .unwrap();
        h.session_manager
            .add_message(
                &h.session_id,
                &Message::assistant().with_text(format!("a{i}")),
            )
            .await
            .unwrap();
    }
    h.session_manager
        .update(&h.session_id)
        .total_tokens(Some(95))
        .apply()
        .await
        .unwrap();

    let events = h.run_turn(USER_PROMPT).await.unwrap();

    // Liveness: the turn ran to completion.
    assert_eq!(provider.main_call_count(), 1, "the turn must still run");
    assert!(
        notification_texts(&events)
            .iter()
            .any(|t| t.contains("Compaction skipped")),
        "the skipped compaction must be surfaced, not swallowed; saw: {:?}",
        notification_texts(&events)
    );

    // The store was NOT clobbered: every seeded message is still there.
    let stored = h.stored_texts().await;
    for i in 0..6 {
        assert!(
            stored.iter().any(|t| t == &format!("q{i}")),
            "q{i} must survive a declined compaction; stored: {stored:#?}"
        );
    }
    // ...and the turn ran on the full history, not the discarded summary.
    assert!(
        provider
            .texts_seen_on_main_call(0)
            .iter()
            .any(|t| t == "q0"),
        "the turn must continue from the fresh history, saw: {:?}",
        provider.texts_seen_on_main_call(0)
    );
}

/// A message persisted mid-turn that the reply loop never pushed into its
/// in-memory conversation — the shape of `drain_elicitation_messages`, of
/// `biorouter term log` writing from a shell hook in another process, and of
/// BR-71's note tool — must survive the overflow-recovery writeback.
///
/// This is the case where a freshness discipline built on "every id the loop
/// knows about" would report a FALSE conflict and quietly stop persisting
/// compactions forever. Carrying the message over instead is both correct and
/// self-correcting.
#[tokio::test(flavor = "multi_thread")]
async fn overflow_recovery_preserves_a_message_persisted_mid_turn() {
    let (h, provider) = harness(|p| {
        p.overflow_on(&[0])
            .note_during_main_call(0, "NOTE: persisted but never pushed")
    })
    .await;

    h.run_turn(USER_PROMPT).await.unwrap();

    assert_eq!(provider.main_call_count(), 2, "the turn must complete");
    let stored = h.stored_texts().await;
    assert!(
        stored
            .iter()
            .any(|t| t.contains("NOTE: persisted but never pushed")),
        "a message the loop persisted but never held in memory must survive; \
         stored: {stored:#?}"
    );
    assert!(
        stored.iter().any(|t| t.contains("User Intent")),
        "and the compaction must still be persisted; stored: {stored:#?}"
    );
}

/// Twice declined: retry exactly once (each attempt re-spends a billed
/// summarization call), then keep going in memory rather than clobbering. The
/// stored history must be untouched and no HistoryReplaced may be emitted —
/// claiming the store was replaced when it was not would make a reload look
/// like it resurrected messages.
#[tokio::test(flavor = "multi_thread")]
async fn overflow_recovery_retries_once_then_keeps_going_without_clobbering() {
    let (h, provider) = harness(|p| {
        p.overflow_on(&[0])
            .rewrite_during_summarization(0)
            .rewrite_during_summarization(1)
    })
    .await;

    h.session_manager
        .add_message(&h.session_id, &Message::user().with_text("earlier turn"))
        .await
        .unwrap();
    let before = h.stored_texts().await;

    let events = h.run_turn(USER_PROMPT).await.unwrap();

    assert_eq!(
        provider.summarizer_call_count(),
        2,
        "exactly one retry: every attempt re-spends a billed summarization call"
    );
    assert_eq!(
        provider.main_call_count(),
        2,
        "the turn must still complete"
    );

    let after = h.stored_texts().await;
    for text in &before {
        assert!(
            after.contains(text),
            "a twice-declined swap must leave the stored history intact; \
             {text:?} vanished. before: {before:#?} after: {after:#?}"
        );
    }
    assert!(
        !after.iter().any(|t| t.contains("User Intent")),
        "nothing was persisted, so no summary may appear on disk; after: {after:#?}"
    );
    assert!(
        history_replaced_texts(&events).is_none(),
        "HistoryReplaced must not claim a replacement that never happened"
    );
}

/// A retry that FAILS must not throw away the first summarization's spend.
///
/// The first swap is declined, so the ladder recomputes and retries — and the
/// retry's summarization errors. By then the provider has already charged for
/// the first round-trip, and there is a perfectly usable compaction in memory.
/// Bailing with `?` there dropped both: the spend never reached the budget or
/// the session gauge (contradicting `OverflowCompactionSwap::usages`' own
/// contract, "all of it is spend whether or not the result was kept"), and the
/// caller's `Err` arm ended the turn instead of continuing on the compaction it
/// already had.
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_retry_still_bills_the_first_summarization_and_finishes_the_turn() {
    let (h, provider) = harness(|p| {
        p.overflow_on(&[0])
            // Declines the first swap: the basis moves under the caller.
            .rewrite_during_summarization(0)
            // ...and the retry's summarizer is gone.
            .fail_summarization_on(1)
    })
    .await;

    h.session_manager
        .add_message(&h.session_id, &Message::user().with_text("earlier turn"))
        .await
        .unwrap();
    let before = h.stored_texts().await;

    let events = h.run_turn(USER_PROMPT).await.unwrap();

    assert_eq!(
        provider.summarizer_call_count(),
        2,
        "the ladder must still attempt exactly one retry"
    );
    assert_eq!(
        provider.main_call_count(),
        2,
        "a failed RETRY must not end the turn — the first compaction is still \
         usable in memory"
    );

    // The billed spend: the first summarization (15) plus the main call that
    // finished the turn (15). The failed retry charged nothing and is absent.
    let session = h
        .session_manager
        .get_session(&h.session_id, false)
        .await
        .unwrap();
    assert_eq!(
        session.accumulated_total_tokens,
        Some(30),
        "the first summarization was charged by the provider and must still be \
         reported, even though its result was never persisted"
    );

    // Nothing was persisted, so the stored history must be intact and no
    // HistoryReplaced may claim otherwise.
    let after = h.stored_texts().await;
    for text in &before {
        assert!(
            after.contains(text),
            "a failed retry must leave the stored history intact; {text:?} vanished"
        );
    }
    assert!(
        history_replaced_texts(&events).is_none(),
        "HistoryReplaced must not claim a replacement that never happened"
    );
}

// ── /compact (execute_commands.rs) ───────────────────────────────────────────

async fn seed_history(h: &Harness, turns: usize) {
    for i in 0..turns {
        h.session_manager
            .add_message(&h.session_id, &Message::user().with_text(format!("q{i}")))
            .await
            .unwrap();
        h.session_manager
            .add_message(
                &h.session_id,
                &Message::assistant().with_text(format!("a{i}")),
            )
            .await
            .unwrap();
    }
}

/// `/compact` is the third snapshot -> summarize -> write-back site, and the
/// most user-visible one. A note appended while it summarizes must survive.
#[tokio::test(flavor = "multi_thread")]
async fn manual_compact_preserves_a_concurrent_note() {
    let (h, provider) =
        harness(|p| p.note_during_summarization(0, "NOTE: landed during /compact")).await;
    seed_history(&h, 6).await;

    h.run_turn("/compact").await.unwrap();

    assert_eq!(provider.summarizer_call_count(), 1);
    let stored = h.stored_texts().await;
    assert!(
        stored
            .iter()
            .any(|t| t.contains("NOTE: landed during /compact")),
        "the note must survive /compact; stored: {stored:#?}"
    );
    assert!(
        stored.iter().any(|t| t.contains("User Intent")),
        "and the compaction must land; stored: {stored:#?}"
    );
}

/// The anti-false-conflict twin: with no concurrent writer `/compact` must
/// still compact and still report success.
#[tokio::test(flavor = "multi_thread")]
async fn manual_compact_still_compacts_with_no_concurrent_writer() {
    let (h, provider) = harness(|p| p).await;
    seed_history(&h, 6).await;

    let events = h.run_turn("/compact").await.unwrap();

    assert_eq!(provider.summarizer_call_count(), 1);
    assert!(
        notification_texts(&events)
            .iter()
            .any(|t| t.contains("Compaction complete")),
        "saw: {:?}",
        notification_texts(&events)
    );
    assert!(h
        .stored_texts()
        .await
        .iter()
        .any(|t| t.contains("User Intent")));
}

/// When the basis moved, `/compact` tells the user rather than silently doing
/// nothing — it is user-initiated and trivially re-runnable.
#[tokio::test(flavor = "multi_thread")]
async fn manual_compact_reports_a_skipped_compaction_to_the_user() {
    let (h, provider) = harness(|p| p.rewrite_during_summarization(0)).await;
    seed_history(&h, 6).await;
    let before = h.stored_texts().await;

    let events = h.run_turn("/compact").await.unwrap();

    assert_eq!(provider.summarizer_call_count(), 1);
    assert!(
        notification_texts(&events)
            .iter()
            .any(|t| t.contains("Run /compact again")),
        "the skipped compaction must be reported; saw: {:?}",
        notification_texts(&events)
    );
    let after = h.stored_texts().await;
    for text in &before {
        assert!(
            after.contains(text),
            "a skipped /compact must leave the history intact; {text:?} vanished"
        );
    }
    assert!(
        !after.iter().any(|t| t.contains("User Intent")),
        "nothing may be persisted when the swap was declined"
    );
}

// ── #51 part (b): the preservation marker, through the real reply loop ───────

/// The pinned-message texts stored for the session, with their agent visibility.
async fn stored_pins(h: &Harness) -> Vec<(String, bool)> {
    h.session_manager
        .get_session(&h.session_id, true)
        .await
        .unwrap()
        .conversation
        .unwrap()
        .messages()
        .iter()
        .filter(|m| m.is_pinned())
        .map(|m| {
            (
                m.content
                    .iter()
                    .filter_map(|c| match c {
                        MessageContent::Text(t) => Some(t.text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(""),
                m.is_agent_visible(),
            )
        })
        .collect()
}

/// Is `text` still in the agent's context, per the store?
async fn agent_sees(h: &Harness, text: &str) -> bool {
    h.session_manager
        .get_session(&h.session_id, true)
        .await
        .unwrap()
        .conversation
        .unwrap()
        .agent_visible_messages()
        .iter()
        .flat_map(|m| m.content.iter())
        .any(|c| matches!(c, MessageContent::Text(t) if t.text.contains(text)))
}

/// **The whole promise, in one test.** A message appended concurrently AND
/// marked must survive both halves of #51: the write-back that would have
/// deleted it (part a), and the compaction that would have summarized it away
/// several turns later (part b).
///
/// Fixing only part (a) produces exactly the failure this test would catch — a
/// note that lands, is confirmed, and quietly evaporates once it falls out of
/// the four-turn verbatim window.
#[tokio::test(flavor = "multi_thread")]
async fn a_pinned_note_survives_both_the_writeback_and_a_later_compaction() {
    const NOTE: &str = "NOTE: always report the log-scale version";

    let (h, provider) = harness(|p| {
        p.overflow_on(&[0])
            .pinned_note_during_summarization(0, NOTE)
    })
    .await;

    // Turn 1: the overflow-recovery summarizer runs and the note lands inside
    // its round-trip. Part (a) has to carry it over the write-back.
    h.run_turn(USER_PROMPT).await.unwrap();
    assert_eq!(provider.summarizer_call_count(), 1);
    assert_eq!(
        stored_pins(&h).await,
        vec![(NOTE.to_string(), true)],
        "part (a): the pinned note must survive the write-back, still marked"
    );

    // Age it well out of the verbatim window: six more user-prompt turns plus
    // the next turn's own prompt, against a default keep-last of four.
    for i in 0..6 {
        h.session_manager
            .add_message(&h.session_id, &Message::user().with_text(format!("q{i}")))
            .await
            .unwrap();
        h.session_manager
            .add_message(
                &h.session_id,
                &Message::assistant().with_text(format!("a{i}")),
            )
            .await
            .unwrap();
    }
    // Push reported usage over the auto-compaction threshold so turn 2 compacts.
    h.session_manager
        .update(&h.session_id)
        .total_tokens(Some(190_000))
        .apply()
        .await
        .unwrap();

    h.run_turn("and now the second question").await.unwrap();

    assert_eq!(
        provider.summarizer_call_count(),
        2,
        "turn 2 must actually have compacted, or this proves nothing"
    );
    assert_eq!(
        stored_pins(&h).await,
        vec![(NOTE.to_string(), true)],
        "part (b): the note must still be in the agent's context after the \
         compaction that summarized everything around it"
    );
    // The control: an unmarked message from the same summarized prefix is gone
    // from the agent's context, so the compaction really did run over it.
    assert!(
        !agent_sees(&h, "q0").await,
        "an unmarked older message must have been summarized away"
    );
}

/// The in-turn auto-compaction site (`agent.rs`), reached when a turn starts
/// over the threshold.
#[tokio::test(flavor = "multi_thread")]
async fn auto_compaction_keeps_a_pinned_message() {
    const NOTE: &str = "NOTE: the cohort is 2019, not 2024";

    let (h, provider) = harness(|p| p.context_limit(100)).await;

    h.session_manager
        .add_message(&h.session_id, &Message::user().with_text(NOTE).pinned())
        .await
        .unwrap();
    for i in 0..6 {
        h.session_manager
            .add_message(&h.session_id, &Message::user().with_text(format!("q{i}")))
            .await
            .unwrap();
        h.session_manager
            .add_message(
                &h.session_id,
                &Message::assistant().with_text(format!("a{i}")),
            )
            .await
            .unwrap();
    }
    h.session_manager
        .update(&h.session_id)
        .total_tokens(Some(95))
        .apply()
        .await
        .unwrap();

    h.run_turn(USER_PROMPT).await.unwrap();

    assert_eq!(provider.summarizer_call_count(), 1);
    assert_eq!(stored_pins(&h).await, vec![(NOTE.to_string(), true)]);
    assert!(
        !agent_sees(&h, "q0").await,
        "control: q0 was summarized away"
    );
    // ...and it reached the model on the very turn that compacted.
    assert!(
        provider
            .texts_seen_on_main_call(0)
            .iter()
            .any(|t| t.contains(NOTE)),
        "the pinned note must be in this turn's request, saw: {:?}",
        provider.texts_seen_on_main_call(0)
    );
}

/// The `/compact` and `/summarize` site (`execute_commands.rs`). A user who
/// explicitly compacts must not thereby destroy the note they pinned.
#[tokio::test(flavor = "multi_thread")]
async fn manual_compact_keeps_a_pinned_message() {
    const NOTE: &str = "NOTE: keep the units in mg/dL";

    let (h, provider) = harness(|p| p).await;
    h.session_manager
        .add_message(&h.session_id, &Message::user().with_text(NOTE).pinned())
        .await
        .unwrap();
    seed_history(&h, 6).await;

    h.run_turn("/compact").await.unwrap();

    assert_eq!(provider.summarizer_call_count(), 1);
    assert_eq!(stored_pins(&h).await, vec![(NOTE.to_string(), true)]);
    assert!(
        h.stored_texts()
            .await
            .iter()
            .any(|t| t.contains("User Intent")),
        "the compaction itself must still have landed"
    );
    assert!(
        !agent_sees(&h, "q0").await,
        "control: q0 was summarized away"
    );
}

/// The marker must not be sent to the summarizer as well as kept. Its text
/// survives verbatim, so summarizing it too spends the window on the same
/// content twice — and on a long-lived session that cost recurs every
/// compaction.
#[tokio::test(flavor = "multi_thread")]
async fn a_pinned_message_is_not_also_summarized() {
    const NOTE: &str = "NOTE: unique-marker-string-for-the-summarizer-payload";

    let (h, provider) = harness(|p| p).await;
    h.session_manager
        .add_message(&h.session_id, &Message::user().with_text(NOTE).pinned())
        .await
        .unwrap();
    seed_history(&h, 6).await;

    h.run_turn("/compact").await.unwrap();

    assert_eq!(provider.summarizer_call_count(), 1);
    let payload = provider.all_summarizer_payloads();
    assert!(
        !payload.contains(NOTE),
        "the pinned note must not be summarized as well as kept; payload: {payload}"
    );
    assert!(
        payload.contains("q0"),
        "control: the unpinned history WAS handed to the summarizer"
    );
    // And exactly one copy is stored — no duplicate re-appended alongside it.
    let occurrences = h
        .stored_texts()
        .await
        .iter()
        .filter(|t| t.contains(NOTE))
        .count();
    assert_eq!(occurrences, 1, "the pinned note must appear exactly once");
}

// ── W1: `basis` and `known` must come from ONE snapshot ──────────────────────
//
// The store's guard splits the history at `basis.max_rowid`: rows ABOVE the
// watermark that `known` does not name are another writer's and are carried
// over; everything at or below it is deleted and replaced. That is only sound
// when `known` was read WITH `basis` (revision first, then the conversation —
// `SessionManager::snapshot_for_rewrite`). Source the two independently and an
// append that lands in between is counted by `basis.max_rowid` — so
// `scan_foreign_tail` never looks at it — while `known` does not contain it
// either. The DELETE destroys it, after `add_message` already returned success.
//
// Both tests below hit that window with no sleeps and no barriers, by choosing
// where in the event stream the concurrent append happens.

/// The turn-start window: `reply()` takes its snapshot inside the async fn, and
/// the reply loop reads its overflow-recovery basis in the stream body, which
/// does not run until the stream is polled. An append between the two is
/// invisible to the loop's view and already inside its basis.
///
/// **The handshake.** `reply()` is an `async fn` returning a stream, and the
/// stream body does not run until it is polled. So awaiting `reply()` parks the
/// writer — the whole turn — at exactly that boundary, the test's `add_message`
/// runs to COMMIT while it is parked, and only then is the turn allowed to
/// proceed. No sleep, no barrier, no scheduler overlap to hope for: the ordering
/// is a data dependency, not a race that usually goes the right way.
///
/// **The vacuity guard** is `texts_seen_on_main_call(0)`. The window's whole
/// premise is that the note landed AFTER the turn's view of the history was
/// fixed. If a future refactor moved the basis read later, the note would simply
/// be part of `known`, the store would preserve it for free, and this test would
/// pass while testing nothing. Asserting the model never saw it pins the
/// precondition instead of hoping for it.
#[tokio::test(flavor = "multi_thread")]
async fn overflow_recovery_preserves_a_note_appended_before_the_basis_was_read() {
    const NOTE: &str = "NOTE: landed between the snapshot and the basis read";

    let (h, provider) = harness(|p| p.overflow_on(&[0])).await;

    let stream = h
        .agent
        .reply(
            Message::user().with_text(USER_PROMPT),
            turn_config(&h),
            None,
        )
        .await
        .unwrap();

    // The window. `reply()` has snapshotted the conversation; nothing of the
    // loop has run yet, and nothing of it CAN run until this await returns and
    // the stream below is polled.
    h.session_manager
        .add_message(&h.session_id, &Message::user().with_text(NOTE))
        .await
        .unwrap();

    tokio::pin!(stream);
    while let Some(event) = stream.next().await {
        event.unwrap();
    }

    assert_eq!(
        provider.summarizer_call_count(),
        1,
        "the overflow recovery must actually have run, or this proves nothing"
    );
    assert_eq!(
        provider.main_call_count(),
        2,
        "and the turn must still complete"
    );
    assert!(
        !provider
            .texts_seen_on_main_call(0)
            .iter()
            .any(|t| t.contains(NOTE)),
        "the append must land OUTSIDE the turn's view, or the window under test \
         was never entered and the store preserves the note for free; saw: {:?}",
        provider.texts_seen_on_main_call(0)
    );

    let stored = h.stored_texts().await;
    assert!(
        stored.iter().any(|t| t.contains(NOTE)),
        "a message appended before the turn read its freshness basis must \
         survive the overflow-recovery writeback; stored: {stored:#?}"
    );
    assert_eq!(
        h.stored_occurrences(NOTE).await,
        1,
        "and exactly once — a split basis can duplicate a row as easily as it \
         can delete one; stored: {stored:#?}"
    );
}

/// The post-swap window: a landed swap renumbers every row, so the basis has to
/// be re-seeded — and re-seeding the REVISION alone, after the conversation the
/// turn adopts was already decided, reopens the same split one iteration later.
/// The turn reports its new token state between the two, which is where this
/// test appends.
///
/// **The handshake.** The stream is polled by this task and by nothing else, so
/// the turn is parked at the yield for as long as the append takes; it resumes
/// only when the append has COMMITTED and the loop below asks for the next
/// event.
///
/// **Why this yield and no other.** The window is bounded below by the swap's
/// commit and above by the basis refresh. In the shape this test exists to pin,
/// the refresh sat between the `TokenUsage` yield and the `HistoryReplaced`
/// yield — so `TokenUsage` is the only stream position inside it, and the test
/// asserts it appended before ever seeing a `HistoryReplaced`. Move the refresh
/// and that assertion fires rather than the test going quietly vacuous.
#[tokio::test(flavor = "multi_thread")]
async fn a_second_overflow_preserves_a_note_appended_right_after_the_first_swap() {
    const NOTE: &str = "NOTE: landed between the swap and the basis refresh";

    let (h, provider) = harness(|p| p.overflow_on(&[0, 1])).await;

    let stream = h
        .agent
        .reply(
            Message::user().with_text(USER_PROMPT),
            turn_config(&h),
            None,
        )
        .await
        .unwrap();
    tokio::pin!(stream);

    let mut appended = false;
    let mut history_replaced_before_append = false;
    while let Some(event) = stream.next().await {
        let event = event.unwrap();
        if !appended && matches!(event, AgentEvent::HistoryReplaced(_)) {
            history_replaced_before_append = true;
        }
        // The first recovery swap has committed by the time the turn reports the
        // token state it moved; the basis is refreshed after this yield.
        if !appended && matches!(event, AgentEvent::TokenUsage(_)) {
            h.session_manager
                .add_message(&h.session_id, &Message::user().with_text(NOTE))
                .await
                .unwrap();
            appended = true;
        }
    }

    assert!(
        appended,
        "the turn must have reported a token state after the first swap"
    );
    assert!(
        !history_replaced_before_append,
        "the append must land BEFORE the loop adopts the swapped history, or it \
         is past the window this test exists to cover"
    );
    assert_eq!(
        provider.summarizer_call_count(),
        2,
        "two overflows must each run their own recovery compaction"
    );
    assert_eq!(
        provider.main_call_count(),
        3,
        "two overflows, then a success"
    );
    assert!(
        !provider
            .texts_seen_on_main_call(1)
            .iter()
            .any(|t| t.contains(NOTE)),
        "the append must land outside the view the SECOND swap writes against, \
         or the window under test was never entered; saw: {:?}",
        provider.texts_seen_on_main_call(1)
    );

    let stored = h.stored_texts().await;
    assert!(
        stored.iter().any(|t| t.contains(NOTE)),
        "a message appended after the first swap committed must survive the \
         second one; stored: {stored:#?}"
    );
    assert_eq!(
        h.stored_occurrences(NOTE).await,
        1,
        "and exactly once; stored: {stored:#?}"
    );
}

/// The same window on the OTHER handoff into the reply loop: the one an
/// auto-compaction goes through.
///
/// `reply()` compacts before the loop starts, and the conversation it hands down
/// is fixed at that rewrite's commit. Everything the loop then does — reading
/// its own freshness basis included — happens later. So an append that lands
/// after the auto-compaction committed but before the loop has a basis is inside
/// the turn's watermark and outside its view: the exact split, reached without
/// touching `reply()`'s own snapshot at all.
///
/// This matters separately from the turn-start test above because the two enter
/// the loop by different routes. A fix that paired the basis only on the
/// no-compaction route would leave this one open, and the largest sessions —
/// the ones that auto-compact every turn — take precisely this route.
#[tokio::test(flavor = "multi_thread")]
async fn overflow_recovery_preserves_a_note_appended_between_the_auto_compaction_and_the_loop() {
    const NOTE: &str = "NOTE: landed between the auto-compaction and the loop";

    // Small context limit + a live gauge over it => `reply()` auto-compacts
    // before the loop; `overflow_on(&[0])` then forces a second, in-loop swap.
    let (h, provider) = harness(|p| p.overflow_on(&[0]).context_limit(100)).await;
    seed_history(&h, 6).await;
    h.session_manager
        .update(&h.session_id)
        .total_tokens(Some(95))
        .apply()
        .await
        .unwrap();

    let stream = h
        .agent
        .reply(
            Message::user().with_text(USER_PROMPT),
            turn_config(&h),
            None,
        )
        .await
        .unwrap();
    tokio::pin!(stream);

    let mut appended = false;
    while let Some(event) = stream.next().await {
        let event = event.unwrap();
        // The auto-compaction announces its replacement only after the rewrite
        // committed. The turn is parked here until the append lands.
        if !appended && matches!(event, AgentEvent::HistoryReplaced(_)) {
            h.session_manager
                .add_message(&h.session_id, &Message::user().with_text(NOTE))
                .await
                .unwrap();
            appended = true;
        }
    }

    assert!(
        appended,
        "the auto-compaction must have replaced the history, or there was no \
         window to append into"
    );
    assert!(
        provider.summarizer_call_count() >= 2,
        "an auto-compaction AND an overflow recovery must both have run, saw {}",
        provider.summarizer_call_count()
    );
    assert_eq!(
        provider.main_call_count(),
        2,
        "one overflow, then a success"
    );
    assert!(
        !provider
            .texts_seen_on_main_call(0)
            .iter()
            .any(|t| t.contains(NOTE)),
        "the append must land outside the view the loop inherited, or the window \
         under test was never entered; saw: {:?}",
        provider.texts_seen_on_main_call(0)
    );

    let stored = h.stored_texts().await;
    assert!(
        stored.iter().any(|t| t.contains(NOTE)),
        "a message appended after the auto-compaction committed, but before the \
         loop read its basis, must survive the loop's own writeback; \
         stored: {stored:#?}"
    );
    assert_eq!(
        h.stored_occurrences(NOTE).await,
        1,
        "and exactly once; stored: {stored:#?}"
    );
}

// ── W2: the FIRST persistence attempt is billed spend too ────────────────────

/// Reject exactly the compaction summary's own INSERT, in the store itself.
/// Every other write on the turn still succeeds, so this is a real database
/// error on the swap — not a declined swap, and not a broken session.
async fn fail_to_store_the_summary(h: &Harness) {
    let db = h.data_dir.join(SESSIONS_FOLDER).join(DB_NAME);
    let pool =
        sqlx::SqlitePool::connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(&db))
            .await
            .unwrap();
    sqlx::query(
        "CREATE TRIGGER reject_the_summary BEFORE INSERT ON messages \
         WHEN NEW.content_json LIKE '%User Intent%' \
         BEGIN SELECT RAISE(ABORT, 'injected store failure'); END",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;
}

/// A database error on the FIRST persistence attempt must not throw away the
/// summarization the provider has already charged for, and must not end the
/// turn.
///
/// This is the same defect class as `a_failed_retry_still_bills_...` one rung
/// earlier on the ladder: the caller bills only the `Ok` branch and `break`s on
/// `Err`, so an `await?` here loses the spend from both the budget and the
/// session gauge, leaves the `PreCompact` it fired without a `PostCompact`, and
/// ends a turn that had a perfectly usable compaction in memory — at the last
/// rung before "context limit still exceeded".
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_first_persist_still_bills_the_summarization_and_finishes_the_turn() {
    let (h, provider) = harness(|p| p.overflow_on(&[0])).await;

    h.session_manager
        .add_message(&h.session_id, &Message::user().with_text("earlier turn"))
        .await
        .unwrap();
    let before = h.stored_texts().await;
    fail_to_store_the_summary(&h).await;

    let events = h.run_turn(USER_PROMPT).await.unwrap();

    assert_eq!(
        provider.summarizer_call_count(),
        1,
        "a store error is not a moved basis: there is nothing to recompute \
         against, so it must not re-spend a summarization"
    );
    assert_eq!(
        provider.main_call_count(),
        2,
        "a failed persist must not end the turn — the compaction is still \
         usable in memory"
    );

    // The billed spend: the summarization (15) plus the main call that finished
    // the turn (15). The overflowing call charged nothing.
    let session = h
        .session_manager
        .get_session(&h.session_id, false)
        .await
        .unwrap();
    assert_eq!(
        session.accumulated_total_tokens,
        Some(30),
        "the summarization was charged by the provider and must still be \
         reported even though the store refused it"
    );

    // Nothing landed, so the stored history must be intact and no
    // HistoryReplaced may claim otherwise.
    let after = h.stored_texts().await;
    for text in &before {
        assert!(
            after.contains(text),
            "a failed persist must leave the stored history intact; \
             {text:?} vanished. before: {before:#?} after: {after:#?}"
        );
    }
    assert!(
        !after.iter().any(|t| t.contains("User Intent")),
        "nothing was persisted, so no summary may appear on disk; after: {after:#?}"
    );
    assert!(
        history_replaced_texts(&events).is_none(),
        "HistoryReplaced must not claim a replacement that never happened"
    );
}

/// The `SessionConfig` `Harness::run_turn` uses, for the tests that have to
/// drive the stream themselves to land an append in a specific window.
fn turn_config(h: &Harness) -> SessionConfig {
    SessionConfig {
        id: h.session_id.clone(),
        schedule_id: None,
        max_turns: Some(8),
        max_tool_calls: None,
        retry_config: None,
        budget: None,
        reasoning_effort: None,
    }
}

// ── #51 W6/W7/W8: the pin rule, through the loop that actually persists ──────
//
// The three unit-level fixes for these all live under `crates/biorouter/src`,
// where the input is a `Vec<Message>` somebody wrote by hand. The tests below
// exercise them from the outside instead: a real session, the real reply loop,
// and — crucially — the OVERFLOW-recovery site, which is the one compaction
// whose input is the NORMALIZED transcript and whose output is written to disk.
// A pin that the normalizer destroys is destroyed durably there, and no unit
// test of `merge_consecutive_messages` can say whether anything reaches it.

/// W6: a pin next to another message of the same role.
///
/// `merge_consecutive_messages` extends the FIRST message and keeps only its
/// metadata and its durable id, so an unpinned neighbour that absorbs a pinned
/// note deletes the marker — and the note is then summarized away like anything
/// else. Every pre-existing pin test in this file goes through a compaction site
/// that sees the RAW stored history (`reply()`'s auto-compaction, `/compact`),
/// so none of them could reach the merge pass at all.
#[tokio::test(flavor = "multi_thread")]
async fn a_pinned_note_next_to_an_unpinned_one_survives_the_normalized_writeback() {
    const NOTE: &str = "NOTE: units are mg/dL, never mmol/L";
    const NEIGHBOUR: &str = "for context, the cohort is the 2019 one";

    let (h, provider) = harness(|p| p.overflow_on(&[0])).await;

    // Adjacent, same role, pin SECOND: the merge makes the unpinned one the
    // carrier, so the marker is what gets dropped.
    h.session_manager
        .add_message(&h.session_id, &Message::user().with_text(NEIGHBOUR))
        .await
        .unwrap();
    h.session_manager
        .add_message(&h.session_id, &Message::user().with_text(NOTE).pinned())
        .await
        .unwrap();
    seed_history(&h, 6).await;

    h.run_turn(USER_PROMPT).await.unwrap();

    assert_eq!(
        provider.summarizer_call_count(),
        1,
        "the overflow recovery must have compacted, or this proves nothing"
    );
    assert_eq!(
        stored_pins(&h).await,
        vec![(NOTE.to_string(), true)],
        "the pinned note must survive a compaction whose input went through the \
         merge pass, still marked and still in the agent's context"
    );
    assert!(
        !agent_sees(&h, "q0").await,
        "control: the unmarked history WAS summarized away"
    );
    // ...and the pin did not broaden the other way either: the neighbour is not
    // exempt just because it sat next to a marked message.
    assert!(
        !agent_sees(&h, NEIGHBOUR).await,
        "an unpinned neighbour must not inherit the exemption; it is ordinary \
         history and belongs in the summary"
    );
}

/// W7: flipping the marker on a message the incremental normalizer has already
/// frozen into its cached prefix.
///
/// `ConversationNormalizer` reuses a frozen prefix whenever a per-message
/// fingerprint still matches, and serves that prefix's OUTPUT messages rather
/// than re-deriving them. A fingerprint blind to `pinned` therefore hands back
/// the marker's OLD value for as long as the session lives — and the overflow
/// path writes that transcript to the store, so a pin set on an older message is
/// not merely ignored, it is erased.
///
/// The normalizer lives on the `Agent`, so both turns below have to run on ONE
/// harness for the cache to be warm at all.
#[tokio::test(flavor = "multi_thread")]
async fn a_pin_set_on_a_message_the_normalizer_already_froze_is_still_honoured() {
    // `MIN_MESSAGES_TO_CACHE` is 16 and `TAIL_SLACK` is 8, so 20 seeded
    // messages put "q2" comfortably inside the frozen prefix.
    const MARKED: &str = "q2";

    let (h, provider) = harness(|p| p.overflow_on(&[1])).await;
    seed_history(&h, 10).await;

    // Turn 1: no compaction, so nothing invalidates the cache — it just warms.
    h.run_turn("first question").await.unwrap();
    assert_eq!(
        provider.summarizer_call_count(),
        0,
        "turn 1 must not compact, or the cache is reset and the seam is never \
         exercised"
    );

    // The marker goes on afterwards — the shape of BR-71's note tool marking an
    // earlier message, or a user pinning something from the transcript.
    let current = h
        .session_manager
        .get_session(&h.session_id, true)
        .await
        .unwrap()
        .conversation
        .unwrap();
    let marked: Vec<Message> = current
        .messages()
        .iter()
        .cloned()
        .map(|m| {
            let is_target = m
                .content
                .iter()
                .any(|c| matches!(c, MessageContent::Text(t) if t.text == MARKED));
            if is_target {
                m.pinned()
            } else {
                m
            }
        })
        .collect();
    assert_eq!(
        marked.iter().filter(|m| m.is_pinned()).count(),
        1,
        "exactly one message must have been marked"
    );
    h.session_manager
        .replace_conversation(
            &h.session_id,
            &biorouter::conversation::Conversation::new_unvalidated(marked),
        )
        .await
        .unwrap();

    // Turn 2 overflows, so the compaction runs over the NORMALIZED transcript
    // and persists it.
    h.run_turn("second question").await.unwrap();

    assert_eq!(
        provider.summarizer_call_count(),
        1,
        "turn 2 must actually have compacted, or this proves nothing"
    );
    assert_eq!(
        stored_pins(&h).await,
        vec![(MARKED.to_string(), true)],
        "a marker set after the normalizer froze that message must still be \
         honoured — a stale cached prefix erases it durably"
    );
    assert!(
        !agent_sees(&h, "q0").await,
        "control: an unmarked message from the same frozen prefix WAS summarized"
    );
}

/// W8: a marker on content that can never be honoured must not be honoured.
///
/// `FrontendToolRequest` is a real provider `tool_use` block — every formatter
/// emits one — and the normalizer strips it only from ASSISTANT messages, so a
/// user-role instance arriving through the API survives to the selector.
/// Preserving one past a compaction that summarized its response away leaves a
/// dangling tool call in the durable history, which the next request carries to
/// the provider.
#[tokio::test(flavor = "multi_thread")]
async fn a_pinned_frontend_tool_request_is_not_preserved_by_a_real_compaction() {
    const FTR_ID: &str = "ftr-must-not-be-preserved";
    const TEXT_NOTE: &str = "NOTE: this one is preservable and must survive";

    // `/compact` sees the RAW stored history, which is what makes a user-role
    // frontend tool request reachable at all. It is used here in preference to
    // the auto-compaction site for a second reason: forcing that one needs a
    // tiny `context_limit`, and `PinLimits::max_tokens` is a SHARE of the same
    // number — at 100 the text note below alone exhausts the pinned-set budget
    // and the frontend tool request is evicted for that reason instead, so the
    // test passes whatever the eligibility rule says. (Observed, on the way to
    // writing this: "the marked set exceeded the preserved-set token budget".)
    let (h, provider) = harness(|p| p).await;

    h.session_manager
        .add_message(
            &h.session_id,
            &Message::user()
                .with_content(MessageContent::frontend_tool_request(
                    FTR_ID,
                    Ok(rmcp::model::CallToolRequestParams {
                        task: None,
                        name: "read_file".into(),
                        arguments: None,
                        meta: None,
                    }),
                ))
                .pinned(),
        )
        .await
        .unwrap();
    // The control, marked the same way: a text note IS preservable, so the
    // compaction below is demonstrably honouring markers at all.
    h.session_manager
        .add_message(
            &h.session_id,
            &Message::user().with_text(TEXT_NOTE).pinned(),
        )
        .await
        .unwrap();
    seed_history(&h, 6).await;

    h.run_turn("/compact").await.unwrap();

    assert_eq!(provider.summarizer_call_count(), 1);
    assert!(
        agent_sees(&h, TEXT_NOTE).await,
        "control: a marked TEXT note must be preserved, or this compaction is \
         not honouring markers and the assertion below means nothing"
    );
    // ...and nothing was evicted, so whatever happens to the frontend tool
    // request below is the ELIGIBILITY rule talking and not the budget.
    assert!(
        !h.stored_texts()
            .await
            .iter()
            .any(|t| t.contains("could not all be kept")),
        "the pinned-set budget must not have evicted anything, or this test \
         cannot tell an ineligible pin from a crowded-out one; stored: {:#?}",
        h.stored_texts().await
    );
    assert!(
        !agent_visible_frontend_tool_request(&h, FTR_ID).await,
        "a marked frontend tool request must NOT be exempt from summarization: \
         it is half a provider tool pair, and preserving it past the compaction \
         that hid its response leaves a dangling tool call on disk"
    );
    assert!(
        !agent_sees(&h, "q0").await,
        "control: the unmarked history WAS summarized away"
    );
}

/// Is a `FrontendToolRequest` with this id still in the agent's context?
async fn agent_visible_frontend_tool_request(h: &Harness, id: &str) -> bool {
    h.session_manager
        .get_session(&h.session_id, true)
        .await
        .unwrap()
        .conversation
        .unwrap()
        .agent_visible_messages()
        .iter()
        .flat_map(|m| m.content.iter())
        .any(|c| matches!(c, MessageContent::FrontendToolRequest(r) if r.id == id))
}
