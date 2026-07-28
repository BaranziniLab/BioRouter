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
use biorouter::session::session_manager::SessionType;
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
    /// Summarizer call indices during which the session's whole history is
    /// wholesale-rewritten by someone else, which moves the basis and makes the
    /// caller's write-back stale.
    rewrites_during_summarization: Vec<usize>,
    /// Notes to append, keyed by the zero-based MAIN-loop call index they land
    /// during. Models a writer that persists to the session mid-turn without
    /// the reply loop ever pushing the message into its in-memory conversation
    /// — `drain_elicitation_messages`, `biorouter term log`, a note tool.
    notes_during_main_call: Vec<(usize, String)>,
    context_limit: usize,
    /// Message texts the provider saw on each main-loop call.
    seen: Mutex<Vec<Vec<String>>>,
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
            rewrites_during_summarization: Vec::new(),
            notes_during_main_call: Vec::new(),
            context_limit: 200_000,
            seen: Mutex::new(Vec::new()),
        }
    }

    fn overflow_on(mut self, calls: &[usize]) -> Self {
        self.overflow_on = calls.to_vec();
        self
    }

    fn note_during_summarization(mut self, call: usize, text: &str) -> Self {
        self.notes_during_summarization
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
        _system_prompt: &str,
        messages: &[Message],
        _tools: &[Tool],
    ) -> Result<(Message, ProviderUsage), ProviderError> {
        let usage = ProviderUsage::new(
            "mock-model".to_string(),
            Usage::new(Some(10), Some(5), Some(15)),
        );

        if Self::is_summarizer_call(messages) {
            let n = self.summarizer_calls.fetch_add(1, Ordering::SeqCst);
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
    _work_dir: TempDir,
}

async fn harness(build: impl FnOnce(RaceProvider) -> RaceProvider) -> (Harness, Arc<RaceProvider>) {
    let work_dir = TempDir::new().unwrap();
    let data_dir = TempDir::new().unwrap();
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
