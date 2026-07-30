//! Process-global registry of active "long-running work" (BR-42).
//!
//! Background shell jobs and running subagents are otherwise two disjoint,
//! per-`DeveloperServer` / in-flight in-memory systems with no unified view of
//! "what is this agent running right now" (`long-running.md` gap #11). Each of
//! those two subsystems registers its live items here as it starts them and
//! deregisters on completion, so a single HTTP surface can list them (plus the
//! scheduler's in-flight runs, which the scheduler already tracks and can kill)
//! and offer a per-item cancel affordance.
//!
//! The registry is a process-wide singleton (`active_work()`), the lowest crate
//! both `biorouter` (subagents) and `biorouter-server` (the route) already
//! depend on — mirroring the pid-file/singleton pattern the background-job
//! reaper and llama.cpp sidecar use. Only two systems feed it; the scheduler is
//! aggregated at the HTTP layer because it has its own registry and kill path.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// What kind of work an entry represents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveWorkKind {
    /// A background shell job (`shell` with `background=true`).
    BackgroundJob,
    /// A running subagent task.
    Subagent,
    /// A **foreground** shell command, blocking the turn it was called from
    /// (issue #72). Registered for the same reason as the other two: a command
    /// that turned out to be far more expensive than it looked was invisible
    /// while it ran, and there was no way to stop just it.
    ForegroundCommand,
}

impl ActiveWorkKind {
    /// Stable machine string used in the HTTP payload and to prefix entry ids.
    pub fn as_str(self) -> &'static str {
        match self {
            ActiveWorkKind::BackgroundJob => "background_job",
            ActiveWorkKind::Subagent => "subagent",
            ActiveWorkKind::ForegroundCommand => "foreground_command",
        }
    }

    fn id_prefix(self) -> &'static str {
        match self {
            ActiveWorkKind::BackgroundJob => "bg",
            ActiveWorkKind::Subagent => "sub",
            ActiveWorkKind::ForegroundCommand => "fg",
        }
    }
}

/// A best-effort, idempotent cancel action for one entry. Each subsystem
/// supplies its own mechanism (process-group signal, cancellation token, …);
/// held as an `Arc` closure so it can be cloned out and invoked without holding
/// the registry lock.
type CancelFn = Arc<dyn Fn() + Send + Sync>;

struct Entry {
    kind: ActiveWorkKind,
    title: String,
    detail: Option<String>,
    session_id: Option<String>,
    started_at_epoch_ms: u128,
    cancel: Option<CancelFn>,
}

/// A closure-free snapshot of one active-work entry, safe to hand to the HTTP
/// layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveWorkItem {
    /// Registry-unique id (e.g. `bg-7`, `sub-3`); also the cancel handle.
    pub id: String,
    pub kind: ActiveWorkKind,
    pub title: String,
    pub detail: Option<String>,
    pub session_id: Option<String>,
    pub started_at_epoch_ms: u128,
    /// Whether this entry carries a cancel action.
    pub cancellable: bool,
}

/// Process-wide registry of live background jobs and subagents.
#[derive(Default)]
pub struct ActiveWorkRegistry {
    entries: Mutex<BTreeMap<String, Entry>>,
    next_id: AtomicU64,
}

impl ActiveWorkRegistry {
    fn new() -> Self {
        Self {
            entries: Mutex::new(BTreeMap::new()),
            next_id: AtomicU64::new(1),
        }
    }

    /// Register a live unit of work, returning its registry-unique id. The
    /// caller must call [`ActiveWorkRegistry::deregister`] with that id once the
    /// work reaches a terminal state (see also [`ActiveWorkGuard`]).
    pub fn register(
        &self,
        kind: ActiveWorkKind,
        title: impl Into<String>,
        detail: Option<String>,
        session_id: Option<String>,
        cancel: Option<CancelFn>,
    ) -> String {
        let id = format!(
            "{}-{}",
            kind.id_prefix(),
            self.next_id.fetch_add(1, Ordering::SeqCst)
        );
        let entry = Entry {
            kind,
            title: title.into(),
            detail,
            session_id,
            started_at_epoch_ms: now_epoch_ms(),
            cancel,
        };
        self.lock().insert(id.clone(), entry);
        id
    }

    /// Drop an entry once its work has finished. Idempotent.
    pub fn deregister(&self, id: &str) {
        self.lock().remove(id);
    }

    /// Snapshot every live entry, sorted by id (creation order within a kind).
    pub fn list(&self) -> Vec<ActiveWorkItem> {
        self.lock()
            .iter()
            .map(|(id, e)| ActiveWorkItem {
                id: id.clone(),
                kind: e.kind,
                title: e.title.clone(),
                detail: e.detail.clone(),
                session_id: e.session_id.clone(),
                started_at_epoch_ms: e.started_at_epoch_ms,
                cancellable: e.cancel.is_some(),
            })
            .collect()
    }

    /// Fire an entry's cancel action. Returns `false` if no such entry exists.
    /// The entry is left in place; the owning subsystem deregisters it when the
    /// work actually reaches a terminal state, so the view keeps showing it as
    /// "stopping" until then.
    pub fn cancel(&self, id: &str) -> bool {
        // Clone the closure out and release the lock before invoking it, so a
        // cancel action never runs (or blocks) while holding the registry.
        let cancel = {
            let entries = self.lock();
            match entries.get(id) {
                Some(entry) => entry.cancel.clone(),
                None => return false,
            }
        };
        if let Some(cancel) = cancel {
            cancel();
        }
        true
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Entry>> {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// RAII handle that deregisters its entry on drop — for callers whose work runs
/// inside a single scope (e.g. an awaited subagent task), so an early return or
/// panic can never leak a phantom "still running" entry.
pub struct ActiveWorkGuard {
    id: String,
}

impl ActiveWorkGuard {
    /// Register an entry and return a guard that deregisters it on drop.
    pub fn register(
        kind: ActiveWorkKind,
        title: impl Into<String>,
        detail: Option<String>,
        session_id: Option<String>,
        cancel: Option<CancelFn>,
    ) -> Self {
        let id = active_work().register(kind, title, detail, session_id, cancel);
        Self { id }
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

impl Drop for ActiveWorkGuard {
    fn drop(&mut self) {
        active_work().deregister(&self.id);
    }
}

/// The process-wide registry.
pub fn active_work() -> &'static ActiveWorkRegistry {
    static REGISTRY: OnceLock<ActiveWorkRegistry> = OnceLock::new();
    REGISTRY.get_or_init(ActiveWorkRegistry::new)
}

fn now_epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    fn fresh() -> ActiveWorkRegistry {
        ActiveWorkRegistry::new()
    }

    #[test]
    fn register_list_deregister_roundtrip() {
        let reg = fresh();
        let id = reg.register(
            ActiveWorkKind::BackgroundJob,
            "job-1: build",
            Some("cargo build".to_string()),
            None,
            None,
        );
        let items = reg.list();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, id);
        assert_eq!(items[0].kind, ActiveWorkKind::BackgroundJob);
        assert_eq!(items[0].title, "job-1: build");
        assert_eq!(items[0].detail.as_deref(), Some("cargo build"));
        assert!(!items[0].cancellable, "no cancel fn was supplied");

        reg.deregister(&id);
        assert!(reg.list().is_empty());
        // Deregistering again is harmless.
        reg.deregister(&id);
    }

    #[test]
    fn ids_are_unique_and_kind_prefixed() {
        let reg = fresh();
        let a = reg.register(ActiveWorkKind::BackgroundJob, "a", None, None, None);
        let b = reg.register(ActiveWorkKind::Subagent, "b", None, None, None);
        let c = reg.register(ActiveWorkKind::BackgroundJob, "c", None, None, None);
        assert!(a.starts_with("bg-"));
        assert!(b.starts_with("sub-"));
        assert!(c.starts_with("bg-"));
        assert_ne!(a, c);
    }

    #[test]
    fn cancel_invokes_closure_and_reports_presence() {
        let reg = fresh();
        let fired = Arc::new(AtomicBool::new(false));
        let fired_c = fired.clone();
        let id = reg.register(
            ActiveWorkKind::Subagent,
            "task",
            None,
            Some("s-parent".to_string()),
            Some(Arc::new(move || fired_c.store(true, Ordering::SeqCst))),
        );
        assert!(reg.list()[0].cancellable);
        assert!(reg.cancel(&id), "known id should report success");
        assert!(fired.load(Ordering::SeqCst), "cancel closure must run");

        // Cancelling keeps the entry listed until the owner deregisters it.
        assert_eq!(reg.list().len(), 1);
        assert!(!reg.cancel("sub-999"), "unknown id should report failure");
    }

    #[test]
    fn cancel_without_closure_is_a_noop_success() {
        let reg = fresh();
        let id = reg.register(ActiveWorkKind::BackgroundJob, "j", None, None, None);
        assert!(reg.cancel(&id), "entry exists even with no cancel action");
    }

    #[test]
    fn list_is_sorted_by_id() {
        let reg = fresh();
        for i in 0..12 {
            reg.register(
                ActiveWorkKind::BackgroundJob,
                format!("j{i}"),
                None,
                None,
                None,
            );
        }
        let items = reg.list();
        let mut sorted = items.clone();
        sorted.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(items, sorted);
    }

    #[test]
    fn guard_deregisters_on_drop() {
        // Uses the process-global registry; assert only on our own entry so the
        // test is robust to other work registered concurrently.
        let before = active_work().list().len();
        {
            let guard =
                ActiveWorkGuard::register(ActiveWorkKind::Subagent, "guarded", None, None, None);
            let id = guard.id().to_string();
            assert!(active_work().list().iter().any(|i| i.id == id));
        }
        assert_eq!(active_work().list().len(), before, "guard must clean up");
    }
}
