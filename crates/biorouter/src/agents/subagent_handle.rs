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

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::agents::subagent_result::SubagentResult;
use crate::config::Config;

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
/// handles are never evicted. Beyond the cap the oldest finished handles of that
/// session are dropped, so a long session that spawns hundreds of background
/// children cannot grow the registry without bound — the results it never
/// collected are the ones it stopped caring about.
///
/// Per session, not per process, on purpose: a global cap would let a busy chat
/// evict a quiet chat's uncollected result.
const MAX_RETAINED_FINISHED: usize = 32;

/// Belt-and-braces ceiling on the whole registry, so a daemon that has served
/// thousands of sessions still cannot accumulate finished handles forever. Well
/// above the per-session cap, so it only ever bites the pathological case.
const MAX_RETAINED_FINISHED_TOTAL: usize = 512;

/// Default and maximum block a caller of [`BackgroundSubagent::wait`] should
/// clamp to. BR-71 decision 23 removed the tool that used to apply them; the
/// caller-facing wait is `workspace_watch`, which carries its own clamp
/// (`workspace_extension.rs`, default 120s, same 600s ceiling). Kept as the
/// registry's own advertised bounds — unify the two literals here if a later
/// task needs one source of truth.
pub const DEFAULT_WAIT_SECS: u64 = 60;
pub const MAX_WAIT_SECS: u64 = 600;

static NEXT_HANDLE_ID: AtomicU64 = AtomicU64::new(1);
static HANDLES: LazyLock<Mutex<Vec<Arc<BackgroundSubagent>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// A detached subagent run the parent can poll.
pub struct BackgroundSubagent {
    pub id: String,
    pub parent_session_id: String,
    pub child_session_id: String,
    pub title: String,
    started: Instant,
    cancel: CancellationToken,
    /// `None` until the run finishes; then the full result envelope. Using a
    /// watch channel means `wait` is a real await, not a poll loop.
    result: watch::Sender<Option<SubagentResult>>,
}

impl BackgroundSubagent {
    /// Register a new running handle and return it. The caller spawns the work
    /// and must call [`BackgroundSubagent::complete`] exactly once when it ends.
    pub fn register(
        parent_session_id: impl Into<String>,
        child_session_id: impl Into<String>,
        title: impl Into<String>,
        cancel: CancellationToken,
    ) -> Arc<Self> {
        let id = format!("sub_{}", NEXT_HANDLE_ID.fetch_add(1, Ordering::SeqCst));
        let (result, _) = watch::channel(None);
        let handle = Arc::new(Self {
            id,
            parent_session_id: parent_session_id.into(),
            child_session_id: child_session_id.into(),
            title: title.into(),
            started: Instant::now(),
            cancel,
            result,
        });

        let mut handles = HANDLES.lock().expect("subagent handle registry poisoned");
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
        self.result.send_replace(Some(result));
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
/// Running handles are never evicted — their results have nowhere else to land.
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
        .filter(|h| in_scope(h) && !h.is_running())
        .count();
    if finished <= keep {
        return;
    }
    let mut to_drop = finished - keep;
    handles.retain(|h| {
        if to_drop > 0 && in_scope(h) && !h.is_running() {
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

    fn new_handle(parent: &str) -> Arc<BackgroundSubagent> {
        BackgroundSubagent::register(
            parent,
            "child-session",
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

    #[test]
    fn finished_handles_are_pruned_but_running_ones_are_kept() {
        let parent = "parent-prune";
        let running = new_handle(parent);
        let mut finished_ids = Vec::new();
        for i in 0..(MAX_RETAINED_FINISHED + 5) {
            let handle = new_handle(parent);
            handle.complete(done(&format!("run {i}")));
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
}
