//! Plans waiting for an answer.
//!
//! An ambiguous import is a real pending-user-input state, not a guess made
//! quietly on the user's behalf (#115). That means the question and the answer
//! are two round trips, and the plan has to survive between them.
//!
//! ⚠ **Re-fetching the source on the answer would be a different archive.** A
//! branch moves; a preview that said "20 skills, entry point `hyperframes`" and
//! an install that wrote something else would make the preview a decoration.
//! So the resolved bytes are held here, keyed by an opaque id, and the answer
//! installs *that* plan.
//!
//! The store is bounded in both directions — a short time-to-live and a cap on
//! how many plans are held — because an archive is up to 256 MiB and a user who
//! previews and walks away must not pin that until the daemon exits.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::ImportPlan;

/// How long an unanswered plan is kept.
const TTL: Duration = Duration::from_secs(15 * 60);

/// How many unanswered plans are kept. The oldest is dropped past this.
const MAX_PENDING: usize = 8;

struct Pending {
    created: Instant,
    plan: ImportPlan,
}

fn store() -> &'static Mutex<HashMap<String, Pending>> {
    static STORE: OnceLock<Mutex<HashMap<String, Pending>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn now_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("plan-{nanos:x}")
}

/// Park a plan and return the id that answers it.
pub fn park(plan: ImportPlan) -> String {
    let id = now_id();
    let mut guard = store().lock().unwrap_or_else(|e| e.into_inner());
    guard.retain(|_, pending| pending.created.elapsed() < TTL);
    while guard.len() >= MAX_PENDING {
        let Some(oldest) = guard
            .iter()
            .min_by_key(|(_, pending)| pending.created)
            .map(|(id, _)| id.clone())
        else {
            break;
        };
        guard.remove(&oldest);
    }
    guard.insert(
        id.clone(),
        Pending {
            created: Instant::now(),
            plan,
        },
    );
    id
}

/// Take a parked plan. `None` if it never existed or has expired — which the
/// caller must report as "ask again", never as "install the default".
pub fn take(id: &str) -> Option<ImportPlan> {
    let mut guard = store().lock().unwrap_or_else(|e| e.into_inner());
    guard.retain(|_, pending| pending.created.elapsed() < TTL);
    guard.remove(id).map(|pending| pending.plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::skill_package::{Evidence, ImportKind, SourceProvenance};

    fn a_plan(id: &str) -> ImportPlan {
        ImportPlan {
            origin: None,
            kind: ImportKind::Bundle,
            id: id.to_string(),
            display_name: id.to_string(),
            version: None,
            entry_point: None,
            groups: Default::default(),
            components: Vec::new(),
            evidence: Evidence::StructuralInference,
            ambiguity: None,
            source: SourceProvenance::default(),
            shadows: Vec::new(),
            files: Vec::new(),
        }
    }

    #[test]
    fn a_parked_plan_comes_back_once_and_only_once() {
        let id = park(a_plan("first"));
        assert_eq!(take(&id).map(|plan| plan.id).as_deref(), Some("first"));
        assert!(take(&id).is_none(), "an answered plan is consumed");
    }

    #[test]
    fn an_unknown_id_is_absent_rather_than_a_default() {
        assert!(take("plan-does-not-exist").is_none());
    }

    #[test]
    fn the_store_is_bounded_so_a_forgotten_preview_cannot_pin_an_archive() {
        let ids: Vec<String> = (0..MAX_PENDING + 3)
            .map(|i| park(a_plan(&format!("p{i}"))))
            .collect();
        let held = store().lock().unwrap().len();
        assert!(held <= MAX_PENDING, "held {held}");
        // The most recent survive; the oldest were dropped.
        assert!(take(ids.last().unwrap()).is_some());
    }
}
