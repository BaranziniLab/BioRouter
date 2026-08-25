//! Async subagent handles (BR-40, second half).
//!
//! Wave 1 landed the structured result envelope ([`crate::agents::subagent_result`]);
//! this is the other half of the proposal: a **spawn → poll** model, so a long
//! subagent no longer stalls the parent's turn.
//!
//! A `subagent` call with `background: true` creates the child session, spawns
//! the run on a detached task, registers a [`BackgroundSubagent`] here, and
//! returns the child's *session id* to the parent immediately. The parent then
//! waits on it with `workspace_watch` (or reads it with
//! `workspace_read_conversation`) to collect the same [`SubagentResult`]
//! envelope it would have got synchronously.
//!
//! Invariants worth knowing:
//!
//! * **The parent's cancellation token is not inherited.** A background child
//!   outlives the turn that spawned it by design, so it gets a fresh token —
//!   otherwise the parent's turn ending would kill the very thing that was made
//!   detachable. The token is reachable through the handle (`workspace_close`,
//!   BR-71 decision 23's replacement for the old poll tool's `cancel: true`) and
//!   through the BR-42 active-work view, which
//!   [`crate::agents::subagent_handler::run_complete_subagent_task`] registers.
//! * **The fork-bomb guards still apply.** The in-flight counter is incremented
//!   before the spawn (so a storm of background spawns is refused just like a
//!   storm of blocking ones) and the concurrency semaphore is acquired *inside*
//!   the detached task, so background work queues rather than bypassing the cap.
//! * **Handles are process-local and in-memory.** The child *session* is
//!   persisted as always, but a handle does not survive a restart; a poll after
//!   a restart reports the handle as unknown rather than inventing a result.
//! * **Off by default.** `BIOROUTER_SUBAGENT_BACKGROUND` (config or env) gates
//!   the `subagent` tool's `background` parameter, so the default tool surface
//!   and the default blocking behaviour are unchanged. Since BR-71 decision 23
//!   it gates *only* that parameter: collecting a detached child is done with
//!   the workspace tools, which are advertised on their own terms.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::agents::subagent_result::SubagentResult;
use crate::config::Config;
use crate::conversation::message::Message;

/// Is the async-handle path available at all?
///
/// Default off: enabling it adds the `background` parameter to the `subagent`
/// tool's surface, which is a behaviour change for every existing session.
/// (Before BR-71 decision 23 it also added a second, dedicated poll tool; that
/// tool is gone and its jobs are workspace tools now.)
pub fn background_enabled() -> bool {
    Config::global()
        .get_param::<bool>("BIOROUTER_SUBAGENT_BACKGROUND")
        .unwrap_or(false)
}

/// How many *finished* handles are retained **per parent session**. Running
/// handles are never evicted. Beyond the cap the oldest finished, collected
/// handles of that session are dropped. Uncollected results stay authoritative
/// even when the watch lease that originally waited for them has expired.
///
/// Per session, not per process, on purpose: a global cap would let a busy chat
/// evict a quiet chat's uncollected result.
const MAX_RETAINED_FINISHED: usize = 32;

/// Belt-and-braces ceiling on collected finished handles across the registry.
/// It is well above the per-session cap, so it only bites the pathological case
/// without discarding a result the parent still has to collect.
const MAX_RETAINED_FINISHED_TOTAL: usize = 512;

static NEXT_HANDLE_ID: AtomicU64 = AtomicU64::new(1);
static HANDLES: LazyLock<Mutex<Vec<Arc<BackgroundSubagent>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

#[derive(Debug, Default)]
struct ContinuationPendingState {
    generation: u64,
    reservations: usize,
    committed: bool,
    result_was_current: bool,
    superseded_generation: u64,
    superseded_turn_id: Option<String>,
}

#[derive(Debug, Default)]
struct InitialRunState {
    runtime_ready: bool,
    finished: bool,
    pending_user_inputs: Vec<PendingInitialInput>,
}

#[derive(Debug)]
struct PendingInitialInput {
    turn_id: Option<String>,
    message: Message,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitialInputDisposition {
    NotInitializing,
    Queued,
    Duplicate,
}

#[derive(Debug, Clone)]
pub struct TerminalGeneration {
    pub generation: u64,
    pub result: SubagentResult,
}

/// A detached subagent run the parent can poll.
pub struct BackgroundSubagent {
    pub id: String,
    pub parent_session_id: String,
    pub child_session_id: String,
    pub title: String,
    parent_closing: Arc<AtomicBool>,
    started: Instant,
    cancel: CancellationToken,
    /// `None` until the run finishes; then the full result envelope. Using a
    /// watch channel means `wait` is a real await, not a poll loop.
    result: watch::Sender<Option<SubagentResult>>,
    terminal_generation: watch::Sender<Option<TerminalGeneration>>,
    state_version: watch::Sender<u64>,
    /// Whether this handle still represents the child's latest turn. A later
    /// turn invalidates the retained result explicitly; persisted message
    /// counts cannot safely identify generations because the spawn-context row
    /// can race with a very fast child completion.
    result_is_current: std::sync::atomic::AtomicBool,
    /// The first child turn belongs to this handle and must not invalidate it.
    /// Any subsequent turn does.
    initial_turn_started: std::sync::atomic::AtomicBool,
    /// A background child is registered before it receives a concurrency
    /// permit. Until its exact runtime profile is installed, an interactive
    /// reply must be retained as steering for the delegated initial turn, not
    /// allowed to claim that turn on a generic agent.
    initial_run: Mutex<InitialRunState>,
    /// Number of real provider turns that have begun in this child. Watchers
    /// snapshot it when they subscribe so they can distinguish a successor
    /// already in flight from lifecycle frames queued by the original turn.
    child_turn_generation: AtomicU64,
    /// The child-turn generation whose terminal result the parent actually
    /// collected through workspace_watch or workspace_read_conversation.
    /// Completion alone never advances this: a result that lands after a watch
    /// lease expires must still block the parent's final answer.
    collected_generation: Mutex<Option<u64>>,
    /// Stop-and-Send has admitted an exact-generation cancellation, but the
    /// replacement provider turn has not started yet. During this gap the
    /// original result is historical and the parent must keep supervising.
    continuation_pending: Mutex<ContinuationPendingState>,
}

impl BackgroundSubagent {
    /// Register a handle whose initial runtime already exists. This remains the
    /// constructor for retained/running-handle bookkeeping; detached spawning
    /// uses [`Self::register_initializing`] because it queues before runtime
    /// installation.
    pub fn register(
        parent_session_id: impl Into<String>,
        child_session_id: impl Into<String>,
        title: impl Into<String>,
        cancel: CancellationToken,
    ) -> Arc<Self> {
        Self::register_with_runtime_state(parent_session_id, child_session_id, title, cancel, true)
    }

    /// Register a detached child before it receives a concurrency permit. The
    /// caller must install the runtime profile and call
    /// [`mark_initial_runtime_ready`] before its delegated provider turn.
    pub fn register_initializing(
        parent_session_id: impl Into<String>,
        child_session_id: impl Into<String>,
        title: impl Into<String>,
        cancel: CancellationToken,
    ) -> Arc<Self> {
        Self::register_with_runtime_state(parent_session_id, child_session_id, title, cancel, false)
    }

    fn register_with_runtime_state(
        parent_session_id: impl Into<String>,
        child_session_id: impl Into<String>,
        title: impl Into<String>,
        cancel: CancellationToken,
        runtime_ready: bool,
    ) -> Arc<Self> {
        let parent_session_id = parent_session_id.into();
        let mut handles = HANDLES.lock().expect("subagent handle registry poisoned");
        let parent_closing = handles
            .iter()
            .find(|handle| handle.parent_session_id == parent_session_id)
            .map(|handle| Arc::clone(&handle.parent_closing))
            .unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
        let id = format!("sub_{}", NEXT_HANDLE_ID.fetch_add(1, Ordering::SeqCst));
        let (result, _) = watch::channel(None);
        let (terminal_generation, _) = watch::channel(None);
        let (state_version, _) = watch::channel(0);
        let handle = Arc::new(Self {
            id,
            parent_session_id,
            child_session_id: child_session_id.into(),
            title: title.into(),
            parent_closing,
            started: Instant::now(),
            cancel,
            result,
            terminal_generation,
            state_version,
            result_is_current: std::sync::atomic::AtomicBool::new(true),
            initial_turn_started: std::sync::atomic::AtomicBool::new(false),
            initial_run: Mutex::new(InitialRunState {
                runtime_ready,
                ..InitialRunState::default()
            }),
            child_turn_generation: AtomicU64::new(0),
            collected_generation: Mutex::new(None),
            continuation_pending: Mutex::new(ContinuationPendingState::default()),
        });

        handles.push(handle.clone());
        prune_locked(&mut handles, &handle.parent_session_id);
        handle
    }

    /// Publish the run's result. Idempotent — a second call overwrites, which is
    /// harmless (and never happens on the spawn path, which completes once).
    ///
    /// `send_replace`, not `send`: a `watch` sender with no live receivers treats
    /// `send` as a failure and **throws the value away**, which for a background
    /// subagent nobody happens to be waiting on would silently lose its result.
    pub fn complete(&self, result: SubagentResult) {
        let mut state = self
            .initial_run
            .lock()
            .expect("subagent initial-run state poisoned");
        state.finished = true;
        state.pending_user_inputs.clear();
        drop(state);
        let _continuation = self
            .continuation_pending
            .lock()
            .expect("subagent continuation state poisoned");
        self.result.send_replace(Some(result.clone()));
        self.terminal_generation
            .send_replace(Some(TerminalGeneration {
                generation: self.child_turn_generation(),
                result,
            }));
        self.signal_state_change();
    }

    fn signal_state_change(&self) {
        self.state_version
            .send_modify(|version| *version = version.wrapping_add(1));
    }

    pub fn state_version(&self) -> u64 {
        *self.state_version.borrow()
    }

    pub async fn wait_for_state_change(&self, observed: u64) -> u64 {
        let mut receiver = self.state_version.subscribe();
        let current = *receiver.borrow_and_update();
        if current != observed {
            return current;
        }
        loop {
            if receiver.changed().await.is_err() {
                return *receiver.borrow();
            }
            let current = *receiver.borrow_and_update();
            if current != observed {
                return current;
            }
        }
    }

    pub fn terminal_generation(&self) -> Option<TerminalGeneration> {
        self.terminal_generation.borrow().clone()
    }

    pub fn result_is_current(&self) -> bool {
        self.result_is_current.load(Ordering::Acquire)
    }

    pub fn continuation_pending(&self) -> bool {
        let state = self
            .continuation_pending
            .lock()
            .expect("subagent continuation state poisoned");
        state.committed || state.reservations > 0
    }

    pub fn child_turn_generation(&self) -> u64 {
        self.child_turn_generation.load(Ordering::Acquire)
    }

    pub fn latest_generation_collected(&self) -> bool {
        let collected = self
            .collected_generation
            .lock()
            .expect("subagent collection state poisoned");
        let generation = self.child_turn_generation();
        (*collected).is_some_and(|collected| collected == generation)
    }

    /// Record collection only if the result still belongs to `generation`.
    /// A successor beginning between a watch/read and this call makes the
    /// collection stale rather than accidentally collecting the successor.
    pub fn mark_collected_if_generation(&self, generation: u64) -> bool {
        let mut collected = self
            .collected_generation
            .lock()
            .expect("subagent collection state poisoned");
        if self.child_turn_generation() != generation {
            return false;
        }
        *collected = Some(generation);
        drop(collected);
        self.signal_state_change();
        true
    }

    /// Collect an exact retained result, never an idle lifecycle state or a
    /// generation already reserved for Stop-and-Send replacement.
    ///
    /// The continuation lock makes its admission atomic with
    /// `mark_continuation_pending_for_turn`. The generation is checked again
    /// after taking the collection lock because `begin_child_turn` advances it
    /// under that lock before invalidating the retained result.
    pub fn mark_current_result_collected_if_generation(&self, generation: u64) -> bool {
        let continuation = self
            .continuation_pending
            .lock()
            .expect("subagent continuation state poisoned");
        if continuation.committed
            || continuation.reservations > 0
            || self.is_running()
            || !self.result_is_current()
            || self.child_turn_generation() != generation
        {
            return false;
        }
        let mut collected = self
            .collected_generation
            .lock()
            .expect("subagent collection state poisoned");
        if self.child_turn_generation() != generation || !self.result_is_current() {
            return false;
        }
        *collected = Some(generation);
        drop(collected);
        drop(continuation);
        self.signal_state_change();
        true
    }

    pub fn mark_terminal_generation_collected_if_generation(&self, generation: u64) -> bool {
        let continuation = self
            .continuation_pending
            .lock()
            .expect("subagent continuation state poisoned");
        if continuation.committed
            || continuation.reservations > 0
            || self.child_turn_generation() != generation
            || self
                .terminal_generation()
                .is_none_or(|terminal| terminal.generation != generation)
        {
            return false;
        }
        let mut collected = self
            .collected_generation
            .lock()
            .expect("subagent collection state poisoned");
        if self.child_turn_generation() != generation
            || self
                .terminal_generation()
                .is_none_or(|terminal| terminal.generation != generation)
        {
            return false;
        }
        *collected = Some(generation);
        drop(collected);
        drop(continuation);
        self.signal_state_change();
        true
    }

    pub fn rollback_terminal_generation_collection(&self, generation: u64) {
        let mut collected = self
            .collected_generation
            .lock()
            .expect("subagent collection state poisoned");
        if self.child_turn_generation() == generation && *collected == Some(generation) {
            *collected = None;
            drop(collected);
            self.signal_state_change();
        }
    }

    pub fn superseded_turn_id(&self) -> Option<String> {
        self.continuation_pending
            .lock()
            .expect("subagent continuation state poisoned")
            .superseded_turn_id
            .clone()
    }

    pub fn superseded_child_turn_generation(&self) -> u64 {
        self.continuation_pending
            .lock()
            .expect("subagent continuation state poisoned")
            .superseded_generation
    }

    pub fn is_running(&self) -> bool {
        self.result.borrow().is_none()
    }

    /// The finished result, if the run has ended.
    pub fn result(&self) -> Option<SubagentResult> {
        self.result.borrow().clone()
    }

    /// Ask the run to stop. The child observes the token at its next checkpoint
    /// and finishes with whatever it produced so far.
    pub fn cancel(&self) {
        self.cancel.cancel();
        self.signal_state_change();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// Block for up to `timeout` for the run to finish. `None` means it is still
    /// running when the timeout expires — a poll, not a failure.
    pub async fn wait(&self, timeout: Duration) -> Option<SubagentResult> {
        // Subscribe *before* reading, so a result published between the read and
        // the subscribe cannot be missed (a fresh receiver marks the current
        // value seen, and `changed()` only reports later sends).
        let mut rx = self.result.subscribe();
        if let Some(result) = rx.borrow_and_update().clone() {
            return Some(result);
        }
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.checked_duration_since(Instant::now())?;
            match tokio::time::timeout(remaining, rx.changed()).await {
                // Sender dropped without a result: the task died. Report it
                // rather than parking the parent forever.
                Ok(Err(_)) => {
                    return Some(SubagentResult::from_error(
                        "Background subagent task ended without producing a result",
                    ))
                }
                Ok(Ok(())) => {
                    if let Some(result) = rx.borrow_and_update().clone() {
                        return Some(result);
                    }
                }
                Err(_) => return None,
            }
        }
    }

    /// Wait until this handle publishes its result, with no independent
    /// deadline. Callers must supply their own cancellation boundary. This is
    /// the supervision primitive used by `workspace_watch`: the watch tool has
    /// an explicit caller-visible lease, while the handle must never disappear
    /// at an unrelated hidden ten-minute deadline.
    pub async fn wait_until_complete(&self) -> SubagentResult {
        let mut rx = self.result.subscribe();
        if let Some(result) = rx.borrow_and_update().clone() {
            return result;
        }
        loop {
            if rx.changed().await.is_err() {
                return SubagentResult::from_error(
                    "Background subagent task ended without producing a result",
                );
            }
            if let Some(result) = rx.borrow_and_update().clone() {
                return result;
            }
        }
    }

    /// A serializable view for the tool's structured content.
    pub fn snapshot(&self) -> HandleSnapshot {
        let result = self.result();
        let state = match &result {
            None if self.is_cancelled() => HandleState::Cancelling,
            None => HandleState::Running,
            Some(_) => HandleState::Finished,
        };
        HandleSnapshot {
            handle: self.id.clone(),
            title: self.title.clone(),
            child_session_id: self.child_session_id.clone(),
            state,
            elapsed_seconds: self.elapsed().as_secs(),
            result,
        }
    }
}

/// Retain a proven user reply while a background child's delegated runtime is
/// still being assembled. The route returns 202 without taking the session turn
/// lock; the delegated runner drains this queue atomically when its runtime
/// profile is ready.
pub fn queue_initializing_child_input(
    child_session_id: &str,
    turn_id: Option<String>,
    message: Message,
) -> InitialInputDisposition {
    let handles = HANDLES.lock().expect("subagent handle registry poisoned");
    let Some(handle) = handles
        .iter()
        .find(|handle| handle.child_session_id == child_session_id && handle.is_running())
    else {
        return InitialInputDisposition::NotInitializing;
    };
    let mut state = handle
        .initial_run
        .lock()
        .expect("subagent initial-run state poisoned");
    if state.runtime_ready || state.finished {
        return InitialInputDisposition::NotInitializing;
    }
    if turn_id.is_some()
        && state
            .pending_user_inputs
            .iter()
            .any(|pending| pending.turn_id == turn_id)
    {
        return InitialInputDisposition::Duplicate;
    }
    state
        .pending_user_inputs
        .push(PendingInitialInput { turn_id, message });
    InitialInputDisposition::Queued
}

/// Atomically make the child eligible to claim its delegated initial turn and
/// return every user reply that arrived while it was waiting for a permit.
pub fn mark_initial_runtime_ready(child_session_id: &str) -> Vec<Message> {
    let handles = HANDLES.lock().expect("subagent handle registry poisoned");
    let Some(handle) = handles
        .iter()
        .find(|handle| handle.child_session_id == child_session_id && handle.is_running())
    else {
        return Vec::new();
    };
    let mut state = handle
        .initial_run
        .lock()
        .expect("subagent initial-run state poisoned");
    if state.finished {
        return Vec::new();
    }
    state.runtime_ready = true;
    std::mem::take(&mut state.pending_user_inputs)
        .into_iter()
        .map(|pending| pending.message)
        .collect()
}

pub fn is_child_initializing(child_session_id: &str) -> bool {
    let handles = HANDLES.lock().expect("subagent handle registry poisoned");
    handles.iter().any(|handle| {
        if handle.child_session_id != child_session_id {
            return false;
        }
        let state = handle
            .initial_run
            .lock()
            .expect("subagent initial-run state poisoned");
        !state.finished && !state.runtime_ready
    })
}

/// Cancel a child that is still waiting for its delegated initial runtime. Once
/// ready, the daemon's ordinary active-turn cancellation owns the lifecycle.
pub fn cancel_initializing_child(child_session_id: &str) -> bool {
    let handles = HANDLES.lock().expect("subagent handle registry poisoned");
    let Some(handle) = handles.iter().find(|handle| {
        if handle.child_session_id != child_session_id {
            return false;
        }
        let state = handle
            .initial_run
            .lock()
            .expect("subagent initial-run state poisoned");
        !state.finished && !state.runtime_ready
    }) else {
        return false;
    };
    handle.cancel();
    true
}

pub fn open_parent_continuation_admission(parent_session_id: &str) {
    let handles = HANDLES.lock().expect("subagent handle registry poisoned");
    for handle in handles
        .iter()
        .filter(|handle| handle.parent_session_id == parent_session_id)
    {
        handle.parent_closing.store(false, Ordering::Release);
        handle.signal_state_change();
    }
}

pub fn begin_parent_closing(parent_session_id: &str) {
    let handles = HANDLES.lock().expect("subagent handle registry poisoned");
    for handle in handles
        .iter()
        .filter(|handle| handle.parent_session_id == parent_session_id)
    {
        handle.parent_closing.store(true, Ordering::Release);
        handle.signal_state_change();
    }
}

pub fn record_child_turn_terminal(child_session_id: &str, result: SubagentResult) {
    let handles = HANDLES.lock().expect("subagent handle registry poisoned");
    for handle in handles
        .iter()
        .filter(|handle| handle.child_session_id == child_session_id)
    {
        let _continuation = handle
            .continuation_pending
            .lock()
            .expect("subagent continuation state poisoned");
        handle
            .terminal_generation
            .send_replace(Some(TerminalGeneration {
                generation: handle.child_turn_generation(),
                result: result.clone(),
            }));
        handle.signal_state_change();
    }
}

/// Mark the start of a real provider turn in a child session.
///
/// A detached registration is ineligible until [`mark_initial_runtime_ready`]
/// atomically installs its runtime boundary and drains pre-start steering. Its
/// first eligible call claims the handle instead of invalidating it. A later
/// call means the user has continued or redirected the child and any retained
/// result from the original delegated run is now historical.
pub fn begin_child_turn(child_session_id: &str) {
    let handles = HANDLES.lock().expect("subagent handle registry poisoned");
    for handle in handles
        .iter()
        .filter(|handle| handle.child_session_id == child_session_id)
    {
        if !handle
            .initial_run
            .lock()
            .expect("subagent initial-run state poisoned")
            .runtime_ready
        {
            continue;
        }
        let mut state = handle
            .continuation_pending
            .lock()
            .expect("subagent continuation state poisoned");
        let _collection = handle
            .collected_generation
            .lock()
            .expect("subagent collection state poisoned");
        let next_generation = handle.child_turn_generation.fetch_add(1, Ordering::AcqRel) + 1;
        let is_initial_running_turn =
            handle.is_running() && !handle.initial_turn_started.swap(true, Ordering::AcqRel);
        if is_initial_running_turn {
            // A fast Stop-and-Send can reserve the continuation after the
            // server publishes the initial TurnStarted but before the agent
            // reaches this hook. That initial turn is still the superseded
            // generation, not the promised replacement.
            if state.committed || state.reservations > 0 {
                state.superseded_generation = next_generation;
            }
            drop(state);
            handle.signal_state_change();
            continue;
        }
        let was_pending = state.committed || state.reservations > 0;
        if was_pending {
            state.generation = state.generation.wrapping_add(1);
            state.reservations = 0;
            state.committed = false;
        }
        if !was_pending {
            state.generation = state.generation.wrapping_add(1);
            state.superseded_generation = next_generation.saturating_sub(1);
            state.superseded_turn_id = None;
        }
        handle.result_is_current.store(false, Ordering::Release);
        drop(state);
        handle.signal_state_change();
    }
}

/// A reversible Stop-and-Send admission mark.
///
/// The route keeps this ticket until cancellation settles. Rolling it back only
/// restores state that this exact mark changed; if the replacement turn has
/// already begun, [`begin_child_turn`] has cleared the bit and rollback is a
/// no-op.
#[derive(Clone)]
pub struct ContinuationPendingMark {
    handles: Vec<ContinuationReservation>,
    refused_parent_closing: bool,
}

#[derive(Clone)]
struct ContinuationReservation {
    handle: Arc<BackgroundSubagent>,
    generation: u64,
    resolution: Arc<AtomicU8>,
}

impl std::fmt::Debug for ContinuationPendingMark {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContinuationPendingMark")
            .field("handles", &self.handles.len())
            .finish()
    }
}

impl ContinuationPendingMark {
    pub fn commit(&self) {
        for reservation in &self.handles {
            reservation.resolve(true);
        }
    }

    pub fn rollback(&self) {
        for reservation in &self.handles {
            reservation.resolve(false);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.handles.is_empty()
    }

    pub fn refused_parent_closing(&self) -> bool {
        self.refused_parent_closing
    }
}

impl ContinuationReservation {
    fn resolve(&self, commit: bool) {
        let resolution = if commit { 1 } else { 2 };
        if self
            .resolution
            .compare_exchange(0, resolution, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }

        let mut state = self
            .handle
            .continuation_pending
            .lock()
            .expect("subagent continuation state poisoned");
        if state.generation != self.generation {
            return;
        }
        state.reservations = state.reservations.saturating_sub(1);
        if commit {
            state.committed = true;
        } else if state.reservations == 0 && !state.committed {
            self.handle
                .result_is_current
                .store(state.result_was_current, Ordering::Release);
            state.superseded_turn_id = None;
        }
        drop(state);
        self.handle.signal_state_change();
    }
}

/// Mark every retained delegated-run handle for `child_session_id` as waiting
/// for a replacement turn. The caller must have generation-matched the child's
/// active or safely retained turn before calling this.
pub fn mark_continuation_pending(child_session_id: &str) -> ContinuationPendingMark {
    mark_continuation_pending_for_turn(child_session_id, None)
}

/// Reserve supervision across a Stop-and-Send gap and identify the exact
/// canonical turn whose queued lifecycle frames must remain historical.
pub fn mark_continuation_pending_for_turn(
    child_session_id: &str,
    superseded_turn_id: Option<String>,
) -> ContinuationPendingMark {
    let handles = HANDLES.lock().expect("subagent handle registry poisoned");
    if handles.iter().any(|handle| {
        handle.child_session_id == child_session_id && handle.parent_closing.load(Ordering::Acquire)
    }) {
        return ContinuationPendingMark {
            handles: Vec::new(),
            refused_parent_closing: true,
        };
    }
    let mut marked = Vec::new();
    for handle in handles
        .iter()
        .filter(|handle| handle.child_session_id == child_session_id)
    {
        let mut state = handle
            .continuation_pending
            .lock()
            .expect("subagent continuation state poisoned");
        if state.reservations == 0 && !state.committed {
            state.generation = state.generation.wrapping_add(1);
            state.result_was_current = handle.result_is_current.swap(false, Ordering::AcqRel);
            state.superseded_generation = handle.child_turn_generation();
            state.superseded_turn_id.clone_from(&superseded_turn_id);
        } else if superseded_turn_id.is_some() && state.superseded_turn_id != superseded_turn_id {
            state.superseded_turn_id.clone_from(&superseded_turn_id);
        }
        state.reservations += 1;
        marked.push(ContinuationReservation {
            handle: Arc::clone(handle),
            generation: state.generation,
            resolution: Arc::new(AtomicU8::new(0)),
        });
        drop(state);
        handle.signal_state_change();
    }
    ContinuationPendingMark {
        handles: marked,
        refused_parent_closing: false,
    }
}

/// Explicitly abandon an admitted continuation, for example when
/// `workspace_close` cancels or evicts the child instead of starting the
/// promised replacement. There is deliberately no elapsed-time fallback:
/// supervision changes only because a successor starts or an explicit close
/// says it will not.
pub fn abandon_continuation(child_session_id: &str) -> bool {
    abandon_continuation_matching(child_session_id, None)
}

pub fn abandon_continuation_for_turn(child_session_id: &str, superseded_turn_id: &str) -> bool {
    abandon_continuation_matching(child_session_id, Some(superseded_turn_id))
}

fn abandon_continuation_matching(child_session_id: &str, superseded_turn_id: Option<&str>) -> bool {
    let handles = HANDLES.lock().expect("subagent handle registry poisoned");
    let mut abandoned = false;
    for handle in handles
        .iter()
        .filter(|handle| handle.child_session_id == child_session_id)
    {
        let mut state = handle
            .continuation_pending
            .lock()
            .expect("subagent continuation state poisoned");
        if superseded_turn_id
            .is_some_and(|expected| state.superseded_turn_id.as_deref() != Some(expected))
        {
            continue;
        }
        if state.committed || state.reservations > 0 {
            state.generation = state.generation.wrapping_add(1);
            state.reservations = 0;
            state.committed = false;
            state.superseded_turn_id = None;
            handle
                .result_is_current
                .store(state.result_was_current, Ordering::Release);
            abandoned = true;
            drop(state);
            handle.signal_state_change();
        }
    }
    abandoned
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HandleState {
    Running,
    /// Cancellation was requested but the child has not yet unwound.
    Cancelling,
    /// The run ended; `result` carries the envelope (which has its own
    /// completed / incomplete / error status).
    Finished,
}

#[derive(Debug, Clone, Serialize)]
pub struct HandleSnapshot {
    pub handle: String,
    pub title: String,
    pub child_session_id: String,
    pub state: HandleState,
    pub elapsed_seconds: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<SubagentResult>,
}

impl HandleSnapshot {
    /// One line for the parent LLM's list view.
    pub fn to_line(&self) -> String {
        match (&self.state, &self.result) {
            (HandleState::Finished, Some(result)) => format!(
                "{}: {} ({}, {}s): {}",
                self.handle,
                self.title,
                result.status.as_str(),
                self.elapsed_seconds,
                first_line(&result.summary),
            ),
            (state, _) => format!(
                "{}: {} ({}, {}s elapsed)",
                self.handle,
                self.title,
                match state {
                    HandleState::Cancelling => "cancelling",
                    _ => "running",
                },
                self.elapsed_seconds,
            ),
        }
    }
}

fn first_line(text: &str) -> String {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let trimmed: String = line.chars().take(160).collect();
    if line.chars().count() > 160 {
        format!("{trimmed}…")
    } else {
        trimmed
    }
}

/// Every handle owned by a parent session, oldest first. Scoped by session so
/// one chat can never poll (or cancel) another chat's children.
pub fn list_for_session(parent_session_id: &str) -> Vec<Arc<BackgroundSubagent>> {
    HANDLES
        .lock()
        .expect("subagent handle registry poisoned")
        .iter()
        .filter(|h| h.parent_session_id == parent_session_id)
        .cloned()
        .collect()
}

/// Look a handle up within its owning session.
pub fn get_for_session(parent_session_id: &str, id: &str) -> Option<Arc<BackgroundSubagent>> {
    HANDLES
        .lock()
        .expect("subagent handle registry poisoned")
        .iter()
        .find(|h| h.parent_session_id == parent_session_id && h.id == id)
        .cloned()
}

/// How many background subagents are still running (test/introspection helper).
pub fn running_count() -> usize {
    HANDLES
        .lock()
        .expect("subagent handle registry poisoned")
        .iter()
        .filter(|h| h.is_running())
        .count()
}

/// Drop the oldest finished handles of `parent_session_id` once it holds more
/// than [`MAX_RETAINED_FINISHED`], then enforce the process-wide ceiling.
/// Running and uncollected handles are never evicted — their results have
/// nowhere else to land, and dropping an uncollected handle would bypass the
/// parent's final-output supervision gate.
fn prune_locked(handles: &mut Vec<Arc<BackgroundSubagent>>, parent_session_id: &str) {
    drop_oldest_finished(handles, MAX_RETAINED_FINISHED, |h| {
        h.parent_session_id == parent_session_id
    });
    drop_oldest_finished(handles, MAX_RETAINED_FINISHED_TOTAL, |_| true);
}

fn drop_oldest_finished(
    handles: &mut Vec<Arc<BackgroundSubagent>>,
    keep: usize,
    in_scope: impl Fn(&BackgroundSubagent) -> bool,
) {
    let finished = handles
        .iter()
        .filter(|h| {
            in_scope(h)
                && !h.is_running()
                && !h.continuation_pending()
                && h.latest_generation_collected()
        })
        .count();
    if finished <= keep {
        return;
    }
    let mut to_drop = finished - keep;
    handles.retain(|h| {
        if to_drop > 0
            && in_scope(h)
            && !h.is_running()
            && !h.continuation_pending()
            && h.latest_generation_collected()
        {
            to_drop -= 1;
            false
        } else {
            true
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::subagent_result::SubagentStatus;
    use crate::conversation::message::Message;
    use crate::conversation::Conversation;

    fn done(text: &str) -> SubagentResult {
        SubagentResult::from_conversation(
            &Conversation::new_unvalidated(vec![Message::assistant().with_text(text)]),
            None,
            true,
        )
    }

    fn new_initializing_handle(parent: &str) -> Arc<BackgroundSubagent> {
        BackgroundSubagent::register_initializing(
            parent,
            format!("child-session-{parent}"),
            "do the thing",
            CancellationToken::new(),
        )
    }

    fn new_handle(parent: &str) -> Arc<BackgroundSubagent> {
        BackgroundSubagent::register(
            parent,
            format!("child-session-{parent}"),
            "do the thing",
            CancellationToken::new(),
        )
    }

    #[test]
    fn handle_starts_running_and_finishes_with_result() {
        let handle = new_handle("parent-running");
        assert!(handle.is_running());
        assert_eq!(handle.snapshot().state, HandleState::Running);
        assert!(handle.result().is_none());

        handle.complete(done("all done"));

        assert!(!handle.is_running());
        let snapshot = handle.snapshot();
        assert_eq!(snapshot.state, HandleState::Finished);
        let result = snapshot.result.expect("finished handle carries a result");
        assert_eq!(result.status, SubagentStatus::Completed);
        assert_eq!(result.summary, "all done");
    }

    #[test]
    fn handles_are_scoped_to_their_parent_session() {
        let mine = new_handle("parent-a");
        let _theirs = new_handle("parent-b");

        let listed = list_for_session("parent-a");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, mine.id);

        assert!(get_for_session("parent-a", &mine.id).is_some());
        // Another session cannot reach into this one's handles.
        assert!(get_for_session("parent-b", &mine.id).is_none());
        assert!(get_for_session("parent-a", "sub_does_not_exist").is_none());
    }

    #[test]
    fn cancel_marks_the_handle_cancelling() {
        let handle = new_handle("parent-cancel");
        assert!(!handle.is_cancelled());
        handle.cancel();
        assert!(handle.is_cancelled());
        assert_eq!(handle.snapshot().state, HandleState::Cancelling);
        // Still resolves to a result once the child unwinds.
        handle.complete(done("stopped early"));
        assert_eq!(handle.snapshot().state, HandleState::Finished);
    }

    #[test]
    fn queued_user_input_cannot_claim_the_delegated_initial_turn() {
        let handle = new_initializing_handle("parent-initializing");
        let child = handle.child_session_id.clone();
        let steer = Message::user().with_text("change the requested output");

        assert_eq!(
            queue_initializing_child_input(&child, Some("client-turn-1".into()), steer.clone()),
            InitialInputDisposition::Queued
        );
        assert_eq!(
            queue_initializing_child_input(&child, Some("client-turn-1".into()), steer),
            InitialInputDisposition::Duplicate
        );
        begin_child_turn(&child);
        assert_eq!(handle.child_turn_generation(), 0);
        assert!(handle.result_is_current());

        let queued = mark_initial_runtime_ready(&child);
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].as_concat_text(), "change the requested output");
        begin_child_turn(&child);
        assert_eq!(handle.child_turn_generation(), 1);
        assert!(handle.result_is_current());
    }

    #[test]
    fn queued_child_remains_cancellable_before_it_has_a_daemon_turn() {
        let handle = new_initializing_handle("parent-queued-cancel");
        assert!(is_child_initializing(&handle.child_session_id));
        assert!(cancel_initializing_child(&handle.child_session_id));
        assert!(handle.is_cancelled());
    }

    #[tokio::test]
    async fn wait_returns_none_while_running_and_the_result_once_done() {
        let handle = new_handle("parent-wait");

        // Still running -> a short wait times out rather than failing.
        assert!(handle.wait(Duration::from_millis(20)).await.is_none());

        let waiter = handle.clone();
        let task = tokio::spawn(async move { waiter.wait(Duration::from_secs(5)).await });
        tokio::time::sleep(Duration::from_millis(20)).await;
        handle.complete(done("finished in the background"));

        let result = task.await.unwrap().expect("wait resolves once complete");
        assert_eq!(result.summary, "finished in the background");
    }

    #[tokio::test]
    async fn wait_on_an_already_finished_handle_returns_immediately() {
        let handle = new_handle("parent-wait-done");
        handle.complete(done("already done"));
        let result = handle
            .wait(Duration::from_millis(0))
            .await
            .expect("finished handle resolves without waiting");
        assert_eq!(result.summary, "already done");
    }

    #[tokio::test]
    async fn unbounded_supervision_wait_has_only_the_callers_cancellation_boundary() {
        let handle = new_handle("parent-unbounded-wait");
        let waiter = handle.clone();
        let task = tokio::spawn(async move { waiter.wait_until_complete().await });
        tokio::task::yield_now().await;
        assert!(!task.is_finished());

        handle.complete(done("finished without an internal deadline"));
        let result = tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .expect("test safety deadline")
            .unwrap();
        assert_eq!(result.summary, "finished without an internal deadline");
    }

    #[tokio::test]
    async fn lifecycle_wait_wakes_for_continuation_and_terminal_transitions() {
        let handle = new_handle("parent-lifecycle-events");
        begin_child_turn(&handle.child_session_id);
        let initial_version = handle.state_version();

        let mark = mark_continuation_pending(&handle.child_session_id);
        let after_reservation = handle.wait_for_state_change(initial_version).await;
        assert_ne!(after_reservation, initial_version);
        mark.rollback();
        let after_rollback = handle.wait_for_state_change(after_reservation).await;
        assert_ne!(after_rollback, after_reservation);

        record_child_turn_terminal(&handle.child_session_id, done("terminal result"));
        let after_terminal = handle.wait_for_state_change(after_rollback).await;
        assert_ne!(after_terminal, after_rollback);
        assert_eq!(
            handle.terminal_generation().unwrap().result.summary,
            "terminal result"
        );
    }

    #[test]
    fn finished_handles_are_pruned_but_running_ones_are_kept() {
        let parent = "parent-prune";
        let running = new_handle(parent);
        let mut finished_ids = Vec::new();
        for i in 0..(MAX_RETAINED_FINISHED + 5) {
            let handle = new_handle(parent);
            handle.complete(done(&format!("run {i}")));
            assert!(handle.mark_collected_if_generation(handle.child_turn_generation()));
            finished_ids.push(handle.id.clone());
        }
        // Registering one more triggers the prune of the excess finished ones.
        let newest = new_handle(parent);

        let listed = list_for_session(parent);
        let finished = listed.iter().filter(|h| !h.is_running()).count();
        assert_eq!(finished, MAX_RETAINED_FINISHED);
        // The running handles survived regardless of the cap.
        assert!(listed.iter().any(|h| h.id == running.id));
        assert!(listed.iter().any(|h| h.id == newest.id));
        // The *oldest* finished results are the ones that were dropped, and the
        // newest finished result is still collectable.
        assert!(get_for_session(parent, &finished_ids[0]).is_none());
        assert!(get_for_session(parent, finished_ids.last().unwrap()).is_some());
    }

    #[test]
    fn pruning_one_session_never_evicts_another_sessions_results() {
        let quiet = "parent-quiet";
        let busy = "parent-busy";
        let precious = new_handle(quiet);
        precious.complete(done("the result nobody collected yet"));

        for i in 0..(MAX_RETAINED_FINISHED + 10) {
            let handle = new_handle(busy);
            handle.complete(done(&format!("busy run {i}")));
            assert!(handle.mark_collected_if_generation(handle.child_turn_generation()));
        }

        let kept = get_for_session(quiet, &precious.id).expect("quiet session's result survives");
        assert_eq!(
            kept.result().unwrap().summary,
            "the result nobody collected yet"
        );
        // Pruning happens on registration, so the busy session settles at the cap
        // (plus at most the one handle that finished after the last register).
        assert!(list_for_session(busy).len() <= MAX_RETAINED_FINISHED + 1);
    }

    #[test]
    fn starting_a_successor_invalidates_collection_of_the_previous_generation() {
        let handle = new_handle("parent-collection-generation");
        begin_child_turn(&handle.child_session_id);
        handle.complete(done("initial result"));
        let initial_generation = handle.child_turn_generation();
        assert!(handle.mark_collected_if_generation(initial_generation));
        assert!(handle.latest_generation_collected());

        begin_child_turn(&handle.child_session_id);
        assert!(!handle.latest_generation_collected());
        assert!(
            !handle.mark_collected_if_generation(initial_generation),
            "a late collector for the initial turn cannot collect its successor"
        );
    }

    #[test]
    fn snapshot_lines_read_well_in_both_states() {
        let handle = new_handle("parent-lines");
        let running = handle.snapshot().to_line();
        assert!(running.contains("do the thing"));
        assert!(running.contains("running"));

        handle.complete(done("wrote the report\nand more detail"));
        let finished = handle.snapshot().to_line();
        assert!(finished.contains("completed"));
        assert!(finished.contains("wrote the report"));
        // Only the first line of the summary makes the list view.
        assert!(!finished.contains("and more detail"));
    }

    #[test]
    fn retained_result_is_invalidated_when_a_later_child_turn_begins() {
        let handle = new_handle("parent-generation");
        begin_child_turn(&handle.child_session_id);
        handle.complete(done("original run"));
        assert!(handle.result_is_current());

        begin_child_turn(&handle.child_session_id);
        assert!(!handle.result_is_current());
    }

    #[test]
    fn concurrent_second_child_turn_invalidates_the_running_handle() {
        let handle = new_handle("parent-concurrent-generation");
        begin_child_turn(&handle.child_session_id);
        begin_child_turn(&handle.child_session_id);
        assert!(!handle.result_is_current());
    }

    #[test]
    fn continuation_mark_invalidates_the_original_until_the_replacement_begins() {
        let handle = new_handle("parent-continuation");
        begin_child_turn(&handle.child_session_id);
        handle.complete(done("original run"));

        let mark = mark_continuation_pending(&handle.child_session_id);
        assert!(!mark.is_empty());
        assert!(handle.continuation_pending());
        assert!(!handle.result_is_current());

        begin_child_turn(&handle.child_session_id);
        assert!(!handle.continuation_pending());
        assert!(!handle.result_is_current());
    }

    #[test]
    fn initial_turn_start_after_admission_does_not_consume_the_replacement_lease() {
        let handle = new_handle("parent-fast-continuation");
        let mark = mark_continuation_pending_for_turn(
            &handle.child_session_id,
            Some("turn-original".to_string()),
        );
        mark.commit();

        begin_child_turn(&handle.child_session_id);
        assert!(handle.continuation_pending());

        handle.complete(done("original run"));
        begin_child_turn(&handle.child_session_id);
        assert!(!handle.continuation_pending());
        assert!(!handle.result_is_current());
    }

    #[test]
    fn failed_continuation_admission_restores_the_original_result() {
        let handle = new_handle("parent-continuation-rollback");
        begin_child_turn(&handle.child_session_id);
        handle.complete(done("original run"));

        let mark = mark_continuation_pending(&handle.child_session_id);
        mark.rollback();

        assert!(!handle.continuation_pending());
        assert!(handle.result_is_current());
    }

    #[test]
    fn explicit_close_abandons_a_continuation_without_a_successor_turn() {
        let handle = new_handle("parent-continuation-abandoned");
        begin_child_turn(&handle.child_session_id);
        handle.complete(done("original run"));

        let mark = mark_continuation_pending(&handle.child_session_id);
        assert!(!mark.is_empty());
        mark.commit();
        assert!(abandon_continuation(&handle.child_session_id));

        assert!(!handle.continuation_pending());
        assert!(handle.result_is_current());
    }

    #[test]
    fn one_dropped_concurrent_admission_cannot_rollback_another_committed_lease() {
        let handle = new_handle("parent-concurrent-continuations");
        begin_child_turn(&handle.child_session_id);
        handle.complete(done("original run"));

        let child_for_dropped = handle.child_session_id.clone();
        let child_for_success = handle.child_session_id.clone();
        let barrier = Arc::new(std::sync::Barrier::new(3));
        let dropped_barrier = barrier.clone();
        let dropped = std::thread::spawn(move || {
            dropped_barrier.wait();
            mark_continuation_pending_for_turn(
                &child_for_dropped,
                Some("turn-original".to_string()),
            )
        });
        let successful_barrier = barrier.clone();
        let successful = std::thread::spawn(move || {
            successful_barrier.wait();
            mark_continuation_pending_for_turn(
                &child_for_success,
                Some("turn-original".to_string()),
            )
        });
        barrier.wait();
        let dropped = dropped.join().unwrap();
        let successful = successful.join().unwrap();
        successful.commit();
        dropped.rollback();

        assert!(handle.continuation_pending());
        assert!(!handle.result_is_current());

        begin_child_turn(&handle.child_session_id);
        assert!(!handle.continuation_pending());
        assert!(!handle.result_is_current());
    }

    #[test]
    fn cloned_ticket_resolves_its_reservation_only_once() {
        let handle = new_handle("parent-cloned-continuation-ticket");
        begin_child_turn(&handle.child_session_id);
        handle.complete(done("original run"));

        let ticket = mark_continuation_pending(&handle.child_session_id);
        let clone = ticket.clone();
        ticket.rollback();
        clone.commit();

        assert!(!handle.continuation_pending());
        assert!(handle.result_is_current());
    }

    #[test]
    fn continuation_remembers_the_exact_superseded_turn_for_watchers() {
        let handle = new_handle("parent-canonical-continuation");
        begin_child_turn(&handle.child_session_id);
        handle.complete(done("original run"));

        let mark = mark_continuation_pending_for_turn(
            &handle.child_session_id,
            Some("turn-original".to_string()),
        );
        mark.commit();

        assert_eq!(
            handle.superseded_turn_id().as_deref(),
            Some("turn-original")
        );
    }

    #[test]
    fn abandoning_an_older_generation_cannot_clear_a_newer_continuation() {
        let handle = new_handle("parent-exact-continuation-abandonment");
        begin_child_turn(&handle.child_session_id);
        handle.complete(done("original run"));

        let first = mark_continuation_pending_for_turn(
            &handle.child_session_id,
            Some("turn-original".to_string()),
        );
        first.commit();
        let second = mark_continuation_pending_for_turn(
            &handle.child_session_id,
            Some("turn-successor".to_string()),
        );
        second.commit();

        assert!(!abandon_continuation_for_turn(
            &handle.child_session_id,
            "turn-original"
        ));
        assert!(handle.continuation_pending());
        assert!(abandon_continuation_for_turn(
            &handle.child_session_id,
            "turn-successor"
        ));
        assert!(!handle.continuation_pending());
    }

    #[test]
    fn stale_rollback_cannot_clear_a_later_continuation_generation() {
        let handle = new_handle("parent-continuation-generation");
        begin_child_turn(&handle.child_session_id);
        handle.complete(done("original run"));

        let first = mark_continuation_pending(&handle.child_session_id);
        begin_child_turn(&handle.child_session_id);
        let second = mark_continuation_pending(&handle.child_session_id);
        assert!(!second.is_empty());

        first.rollback();
        assert!(handle.continuation_pending());
        assert!(!handle.result_is_current());
    }

    #[test]
    fn parent_closing_refuses_continuation_without_mutating_the_handle() {
        let parent = "parent-closing-barrier";
        let handle = new_handle(parent);
        begin_child_turn(&handle.child_session_id);
        handle.complete(done("original result"));

        begin_parent_closing(parent);
        let refused = mark_continuation_pending(&handle.child_session_id);
        assert!(refused.is_empty());
        assert!(refused.refused_parent_closing());
        assert!(!handle.continuation_pending());
        assert!(handle.result_is_current());

        let late_child = format!("late-{}", handle.child_session_id);
        let late_handle = BackgroundSubagent::register(
            parent,
            &late_child,
            "registered after close",
            CancellationToken::new(),
        );
        begin_child_turn(&late_child);
        let late_refused = mark_continuation_pending(&late_child);
        assert!(late_refused.refused_parent_closing());
        assert!(!late_handle.continuation_pending());

        open_parent_continuation_admission(parent);
        let admitted = mark_continuation_pending(&handle.child_session_id);
        assert!(!admitted.is_empty());
        assert!(!admitted.refused_parent_closing());
        admitted.rollback();
    }

    #[test]
    fn terminal_collection_is_exact_to_the_generation_that_finished() {
        let handle = new_handle("parent-terminal-generation");
        begin_child_turn(&handle.child_session_id);
        let first_generation = handle.child_turn_generation();
        record_child_turn_terminal(&handle.child_session_id, done("first result"));

        begin_child_turn(&handle.child_session_id);
        let second_generation = handle.child_turn_generation();
        assert_ne!(first_generation, second_generation);
        assert!(
            !handle.mark_terminal_generation_collected_if_generation(first_generation),
            "a stale terminal cannot collect its successor"
        );

        record_child_turn_terminal(&handle.child_session_id, done("second result"));
        assert!(handle.mark_terminal_generation_collected_if_generation(second_generation));
        assert!(handle.latest_generation_collected());
    }
}
